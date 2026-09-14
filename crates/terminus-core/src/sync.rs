//! Sync engine: keeps the local store convergent with a remote backend.
//!
//! The remote backend is a sqlx [`SqlitePool`] today (a PostgreSQL pool can be
//! dropped in behind the same handle once the transport lands). The engine
//! itself owns the *state machine* and the bookkeeping around a sync run:
//!
//! ```text
//!                    ┌──────────────┐
//!  configured  ┌────▶│ Unconfigured │
//!              │     └──────┬───────┘
//!              │            │ remote attached
//!              │            ▼
//!  ┌───────────┴──┐   ┌─────────┐   network lost   ┌─────────┐
//!  │     Idle     │◀─▶│ Syncing │─────────────────▶│ Offline │
//!  └──────┬───────┘   └────┬────┘                  └────┬────┘
//!         │                │ transport/build failure   │
//!         │                ▼                           │
//!         │           ┌─────────┐                      │
//!         └──────────▶│  Error  │◀─────────────────────┘
//!                     └─────────┘
//! ```
//!
//! `push_all()` / `pull_all()` are phase stubs at the moment: they validate the
//! engine is configured, keep the state machine + last-error/last-sync
//! bookkeeping honest, and log what the transport will have to do. The API
//! shape (`Result<SyncReport>`) is final so callers do not have to change when
//! the transport is wired in.

use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::vault::UnlockedVault;

/// Sync endpoint configuration. Persisted in the local store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Master switch; `false` disables every sync run.
    pub enabled: bool,
    /// Remote backend URL. `None`/empty means "not configured yet".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    /// Stable identifier for this device, used for conflict attribution.
    pub device_id: String,
    /// Desired interval between automatic runs, in seconds.
    pub interval_secs: u64,
    /// Whether encrypted secrets (`SecretEnvelope`s) are synced too.
    pub sync_secrets: bool,
    /// Last successful run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<DateTime<Utc>>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            remote_url: None,
            device_id: Uuid::new_v4().to_string(),
            interval_secs: 60,
            sync_secrets: false,
            last_sync: None,
        }
    }
}

impl SyncConfig {
    /// Configuration pointing at `remote_url`, enabled.
    pub fn remote(remote_url: impl Into<String>) -> Self {
        Self {
            enabled: true,
            remote_url: Some(remote_url.into()),
            ..Self::default()
        }
    }

    /// True when a remote endpoint has been set.
    pub fn has_remote(&self) -> bool {
        self.remote_url
            .as_deref()
            .map(str::trim)
            .is_some_and(|u| !u.is_empty())
    }
}

/// Sync engine state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    /// No remote endpoint configured.
    Unconfigured,
    /// Configured and idle.
    Idle,
    /// A `sync_now()` run is in flight.
    Syncing,
    /// Remote unreachable; will retry.
    Offline,
    /// Last run failed (bad credentials, schema mismatch, ...).
    Error,
}

impl SyncStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            SyncStatus::Unconfigured => "unconfigured",
            SyncStatus::Idle => "idle",
            SyncStatus::Syncing => "syncing",
            SyncStatus::Offline => "offline",
            SyncStatus::Error => "error",
        }
    }

    /// Whether the engine is currently running a sync.
    pub const fn is_busy(self) -> bool {
        matches!(self, SyncStatus::Syncing)
    }

    /// Whether the state machine allows `self -> next`.
    pub const fn can_transition_to(self, next: SyncStatus) -> bool {
        use SyncStatus::*;
        if matches!(
            (self, next),
            (Unconfigured, Unconfigured)
                | (Idle, Idle)
                | (Syncing, Syncing)
                | (Offline, Offline)
                | (Error, Error)
        ) {
            // Idempotent transitions are always allowed.
            return true;
        }

        matches!(
            (self, next),
            // A remote gets attached (or the endpoint is cleared).
            (Unconfigured, Idle)
                | (Unconfigured, Error)
                | (Idle, Unconfigured)
                | (Offline, Unconfigured)
                | (Error, Unconfigured)
                // Normal runs.
                | (Idle, Syncing)
                | (Syncing, Idle)
                | (Offline, Syncing)
                | (Syncing, Offline)
                | (Idle, Offline)
                // Failures and recoveries.
                | (Syncing, Error)
                | (Idle, Error)
                | (Error, Idle)
                | (Error, Syncing)
                | (Error, Offline)
                | (Offline, Error)
        )
    }
}

impl Default for SyncStatus {
    fn default() -> Self {
        SyncStatus::Unconfigured
    }
}

impl fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Summary of one `push_all()` / `pull_all()` / `sync_now()` run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncReport {
    /// Local changes accepted by the remote.
    pub pushed: u64,
    /// Remote changes applied locally.
    pub pulled: u64,
    /// Rows skipped (soft-deleted locally, filtered, unchanged, ...).
    pub skipped: u64,
    /// Conflicts detected during the run.
    pub conflicts: u64,
    /// Non-fatal errors collected while the run continued.
    pub errors: Vec<String>,
    /// When the run started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the run finished.
    pub finished_at: Option<DateTime<Utc>>,
    /// Wall-clock duration of the run, milliseconds.
    pub duration_ms: u64,
}

impl SyncReport {
    /// An empty report stamped with `started_at`.
    pub fn started_now() -> Self {
        Self {
            started_at: Some(Utc::now()),
            ..Self::default()
        }
    }

    /// Stamps `finished_at`/`duration_ms` from the given monotonic clock.
    pub fn finish(mut self, elapsed_ms: u64) -> Self {
        self.finished_at = Some(Utc::now());
        self.duration_ms = elapsed_ms;
        self
    }

    /// Merges another phase report into this one.
    pub fn absorb(&mut self, other: SyncReport) {
        self.pushed += other.pushed;
        self.pulled += other.pulled;
        self.skipped += other.skipped;
        self.conflicts += other.conflicts;
        self.errors.extend(other.errors);
    }

    /// Total number of rows that crossed the wire in either direction.
    pub fn transferred(&self) -> u64 {
        self.pushed + self.pulled
    }
}

/// Owns the remote backend handle, the vault reference and the sync state.
pub struct SyncEngine {
    /// Sync endpoint configuration.
    pub config: SyncConfig,
    /// Remote backend pool. `None` while unconfigured or while offline.
    remote: Arc<Mutex<Option<SqlitePool>>>,
    /// Current state machine value.
    status: Arc<Mutex<SyncStatus>>,
    /// Unlocked vault, when the user has unlocked it. Required to sync secrets.
    vault: Arc<Mutex<Option<Arc<UnlockedVault>>>>,
    /// Last error message, for the sync-aware UI badge.
    last_error: Arc<Mutex<Option<String>>>,
    /// Last successful run.
    last_sync: Arc<Mutex<Option<DateTime<Utc>>>>,
    /// Serializes runs: only one `sync_now()` at a time.
    run_lock: Arc<Mutex<()>>,
}

impl Default for SyncEngine {
    fn default() -> Self {
        Self::new(SyncConfig::default())
    }
}

impl SyncEngine {
    /// Builds an engine for `config`.
    pub fn new(config: SyncConfig) -> Self {
        let status = if config.enabled && config.has_remote() {
            SyncStatus::Idle
        } else {
            SyncStatus::Unconfigured
        };

        Self {
            config,
            remote: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(status)),
            vault: Arc::new(Mutex::new(None)),
            last_error: Arc::new(Mutex::new(None)),
            last_sync: Arc::new(Mutex::new(None)),
            run_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Attaches an unlocked vault (enables secret sync).
    pub fn with_vault(mut self, vault: Arc<UnlockedVault>) -> Self {
        self.vault = Arc::new(Mutex::new(Some(vault)));
        self
    }

    /// Current state machine value.
    pub async fn status(&self) -> SyncStatus {
        *self.status.lock().await
    }

    /// Last error message, if any.
    pub async fn last_error(&self) -> Option<String> {
        self.last_error.lock().await.clone()
    }

    /// Timestamp of the last successful run.
    pub async fn last_sync(&self) -> Option<DateTime<Utc>> {
        *self.last_sync.lock().await
    }

    /// Whether a remote backend pool is attached.
    pub async fn is_configured(&self) -> bool {
        self.remote.lock().await.is_some()
    }

    /// Attaches the remote backend pool and moves the machine to `Idle`.
    pub async fn set_remote(&self, pool: SqlitePool) -> Result<()> {
        *self.remote.lock().await = Some(pool);
        self.transition_to(SyncStatus::Idle).await
    }

    /// Detaches the remote backend pool and moves back to `Unconfigured`.
    pub async fn clear_remote(&self) {
        *self.remote.lock().await = None;
        *self.status.lock().await = SyncStatus::Unconfigured;
    }

    /// Attaches an unlocked vault.
    pub async fn attach_vault(&self, vault: Arc<UnlockedVault>) {
        *self.vault.lock().await = Some(vault);
    }

    /// Drops the unlocked vault (lock button / app shutdown).
    pub async fn detach_vault(&self) {
        *self.vault.lock().await = None;
    }

    /// The vault currently held by the engine, if any.
    pub async fn vault(&self) -> Option<Arc<UnlockedVault>> {
        self.vault.lock().await.clone()
    }

    /// Whether secrets can be synced right now.
    pub async fn can_sync_secrets(&self) -> bool {
        self.config.sync_secrets && self.vault.lock().await.is_some()
    }

    /// Applies a state transition, validating it against the state machine.
    pub async fn transition_to(&self, next: SyncStatus) -> Result<()> {
        let mut status = self.status.lock().await;
        if !status.can_transition_to(next) {
            return Err(Error::SyncError(format!(
                "invalid sync transition {status} -> {next}"
            )));
        }
        *status = next;
        Ok(())
    }

    /// Marks the remote as unreachable.
    pub async fn mark_offline(&self) -> Result<()> {
        self.transition_to(SyncStatus::Offline).await
    }

    /// Marks the engine as failed and records the reason.
    pub async fn mark_error(&self, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        *self.last_error.lock().await = Some(message.clone());
        warn!(error = %message, "sync: engine error");
        self.transition_to(SyncStatus::Error).await
    }

    /// Clears the recorded error.
    pub async fn clear_error(&self) {
        *self.last_error.lock().await = None;
    }

    /// Pushes every pending local change to the remote backend.
    ///
    /// The transport is not wired yet: the method validates the engine state and
    /// returns an empty report, logging what the real implementation will push.
    pub async fn push_all(&self) -> Result<SyncReport> {
        let report = SyncReport::started_now();
        let started = Instant::now();

        if !self.config.enabled {
            return Err(Error::SyncError("sync is disabled".to_string()));
        }

        let remote = self.remote.lock().await;
        if remote.is_none() {
            return Err(Error::SyncError("no remote backend attached".to_string()));
        }

        let secrets = self.can_sync_secrets().await;
        info!(
            device = %self.config.device_id,
            secrets,
            "sync: push_all — remote transport not wired yet, nothing was sent"
        );

        Ok(report.finish(started.elapsed().as_millis() as u64))
    }

    /// Applies every remote change that is missing locally.
    ///
    /// See [`SyncEngine::push_all`] for the state of the transport.
    pub async fn pull_all(&self) -> Result<SyncReport> {
        let report = SyncReport::started_now();
        let started = Instant::now();

        if !self.config.enabled {
            return Err(Error::SyncError("sync is disabled".to_string()));
        }

        let remote = self.remote.lock().await;
        if remote.is_none() {
            return Err(Error::SyncError("no remote backend attached".to_string()));
        }

        let secrets = self.can_sync_secrets().await;
        info!(
            device = %self.config.device_id,
            secrets,
            "sync: pull_all — remote transport not wired yet, nothing was fetched"
        );

        Ok(report.finish(started.elapsed().as_millis() as u64))
    }

    /// Runs `push_all()` then `pull_all()` under a single exclusive lock,
    /// maintaining the state machine and the last-sync bookkeeping.
    pub async fn sync_now(&self) -> Result<SyncReport> {
        // Only one run at a time; the second caller waits for the first.
        let _run = self.run_lock.lock().await;

        if !self.config.enabled {
            return Err(Error::SyncError("sync is disabled".to_string()));
        }
        if !self.is_configured().await {
            // A configured endpoint that has no attached transport is a
            // failure the UI must surface; an engine with no endpoint at all
            // simply stays `Unconfigured`.
            if self.config.has_remote() {
                self.mark_error("no remote backend attached").await.ok();
            }
            return Err(Error::SyncError("no remote backend attached".to_string()));
        }

        if let Err(err) = self.transition_to(SyncStatus::Syncing).await {
            return Err(err);
        }

        let started = Instant::now();
        let mut report = SyncReport::started_now();

        let push = self.push_all().await;
        let pull = self.pull_all().await;

        let outcome: Result<()> = match (push, pull) {
            (Ok(push), Ok(pull)) => {
                report.absorb(push);
                report.absorb(pull);
                Ok(())
            }
            (Err(err), _) | (_, Err(err)) => Err(err),
        };

        match outcome {
            Ok(()) => {
                let finished = report.finish(started.elapsed().as_millis() as u64);
                let now = finished.finished_at.unwrap_or_else(Utc::now);
                *self.last_sync.lock().await = Some(now);
                *self.last_error.lock().await = None;
                self.transition_to(SyncStatus::Idle).await?;
                info!(
                    pushed = finished.pushed,
                    pulled = finished.pulled,
                    duration_ms = finished.duration_ms,
                    "sync: run finished"
                );
                Ok(finished)
            }
            Err(err) => {
                let message = err.to_string();
                *self.last_error.lock().await = Some(message.clone());
                let _ = self.transition_to(SyncStatus::Error).await;
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_accepts_valid_transitions() {
        use SyncStatus::*;
        assert!(Unconfigured.can_transition_to(Idle));
        assert!(Idle.can_transition_to(Syncing));
        assert!(Syncing.can_transition_to(Idle));
        assert!(Syncing.can_transition_to(Error));
        assert!(Idle.can_transition_to(Offline));
        assert!(Offline.can_transition_to(Syncing));
        assert!(Error.can_transition_to(Idle));
        // Idempotent transitions are allowed.
        assert!(Idle.can_transition_to(Idle));
    }

    #[test]
    fn state_machine_rejects_invalid_transitions() {
        use SyncStatus::*;
        assert!(!Unconfigured.can_transition_to(Syncing));
        assert!(!Unconfigured.can_transition_to(Offline));
        assert!(!Syncing.can_transition_to(Unconfigured));
    }

    #[tokio::test]
    async fn unconfigured_engine_starts_in_unconfigured() {
        let engine = SyncEngine::default();
        assert_eq!(engine.status().await, SyncStatus::Unconfigured);
        assert!(!engine.is_configured().await);
        assert!(engine.sync_now().await.is_err());
        assert_eq!(engine.status().await, SyncStatus::Unconfigured);
    }

    #[tokio::test]
    async fn config_with_remote_starts_idle() {
        let engine = SyncEngine::new(SyncConfig::remote("sqlite://remote.db"));
        assert_eq!(engine.status().await, SyncStatus::Idle);

        // Without an attached pool a run must fail and move the machine to Error.
        let err = engine.sync_now().await.expect_err("no pool attached");
        assert!(matches!(err, Error::SyncError(_)));
        assert_eq!(engine.status().await, SyncStatus::Error);
        assert!(engine.last_error().await.is_some());

        // Recovery is a valid transition.
        engine.clear_error().await;
        engine
            .transition_to(SyncStatus::Idle)
            .await
            .expect("recover");
        assert_eq!(engine.status().await, SyncStatus::Idle);
    }

    #[tokio::test]
    async fn vault_attachment_gates_secret_sync() {
        let engine = SyncEngine::default();
        assert!(!engine.can_sync_secrets().await);

        let (_header, vault) =
            crate::vault::create_with_key("passphrase").expect("vault");
        engine.attach_vault(Arc::new(vault)).await;
        assert!(engine.vault().await.is_some());
        // Config has `sync_secrets = false` by default, so still gated.
        assert!(!engine.can_sync_secrets().await);

        engine.detach_vault().await;
        assert!(engine.vault().await.is_none());
    }

    #[test]
    fn report_absorb_and_telemetry() {
        let mut report = SyncReport::started_now();
        report.absorb(SyncReport {
            pushed: 3,
            pulled: 1,
            skipped: 2,
            errors: vec!["one warning".to_string()],
            ..SyncReport::default()
        });
        let report = report.finish(42);
        assert_eq!(report.transferred(), 4);
        assert_eq!(report.skipped, 2);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.duration_ms, 42);
        assert!(report.finished_at.is_some());
    }
}
