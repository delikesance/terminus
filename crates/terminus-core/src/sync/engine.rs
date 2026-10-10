use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::error::{Error, Result};
use crate::vault::UnlockedVault;

use super::*;

/// Owns the remote backend handle, the vault reference and the sync state.
pub struct SyncEngine {
    /// Sync endpoint configuration.
    pub config: SyncConfig,
    /// Remote backend pool. `None` while unconfigured or while offline.
    remote: Arc<Mutex<Option<SqlitePool>>>,
    /// Local store pool the engine merges into the remote.
    local: Arc<Mutex<Option<SqlitePool>>>,
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
            local: Arc::new(Mutex::new(None)),
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

    /// Attaches the remote backend pool (creating the Terminus schema on it)
    /// and moves the machine to `Idle`.
    pub async fn set_remote(&self, pool: SqlitePool) -> Result<()> {
        crate::store::Store::ensure_schema(&pool).await?;
        ensure_meta_table(&pool).await?;
        *self.remote.lock().await = Some(pool);
        self.transition_to(SyncStatus::Idle).await
    }

    /// Attaches the local store pool the engine syncs.
    pub async fn attach_local(&self, pool: SqlitePool) {
        *self.local.lock().await = Some(pool);
    }

    /// Opens a sqlite remote from a connection URI and attaches it.
    ///
    /// `postgres://` / `postgresql://` URIs are rejected until the Postgres
    /// transport lands. Bare filesystem paths are accepted as create-if-missing
    /// sqlite files.
    pub async fn attach_remote_uri(&self, uri: &str) -> Result<()> {
        let pool = open_remote_sqlite_pool(uri).await?;
        self.set_remote(pool).await
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

    /// Pushes every local row that is newer than (or missing on) the remote.
    pub async fn push_all(&self) -> Result<SyncReport> {
        self.run_phase(Direction::Push).await
    }

    /// Applies every remote row that is newer than (or missing) locally.
    pub async fn pull_all(&self) -> Result<SyncReport> {
        self.run_phase(Direction::Pull).await
    }

    async fn run_phase(&self, direction: Direction) -> Result<SyncReport> {
        let report = SyncReport::started_now();
        let started = Instant::now();

        if !self.config.enabled {
            return Err(Error::SyncError("sync is disabled".to_string()));
        }
        let Some(remote) = self.remote.lock().await.clone() else {
            return Err(Error::SyncError("no remote backend attached".to_string()));
        };
        let Some(local) = self.local.lock().await.clone() else {
            return Err(Error::SyncError("no local store attached".to_string()));
        };

        let mut report = report;
        let mut tables: Vec<&str> = PLAIN_TABLES.to_vec();
        if self.can_sync_secrets().await {
            match check_vault_match(&local, &remote).await {
                Ok(()) => tables.extend_from_slice(SECRET_TABLES),
                Err(message) => report.errors.push(message),
            }
        }
        for table in tables {
            let (from, to) = match direction {
                Direction::Push => (&local, &remote),
                Direction::Pull => (&remote, &local),
            };
            let (copied, skipped) = merge_table(from, to, table).await?;
            match direction {
                Direction::Push => report.pushed += copied,
                Direction::Pull => report.pulled += copied,
            }
            report.skipped += skipped;
        }
        info!(
            device = %self.config.device_id,
            ?direction,
            pushed = report.pushed,
            pulled = report.pulled,
            "sync: phase finished"
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

        self.transition_to(SyncStatus::Syncing).await?;

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
