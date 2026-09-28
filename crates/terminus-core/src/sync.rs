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
//! `push_all()` / `pull_all()` merge the local store with the remote table by
//! table, last-writer-wins on `updated_at`. Soft-delete tombstones are rows
//! too, so deletions propagate. Vault envelopes (`credentials`, sealed
//! `identities`) only move with `sync_secrets` and a matching vault header.

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

    /// Serialize for the local settings table.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| Error::SyncError(e.to_string()))
    }

    /// Parse from the local settings table.
    pub fn from_json(raw: &str) -> Result<Self> {
        serde_json::from_str(raw).map_err(|e| Error::SyncError(e.to_string()))
    }
}

/// Open a sqlite pool for the sync remote URI.
pub async fn open_remote_sqlite_pool(uri: &str) -> Result<SqlitePool> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    let uri = uri.trim();
    if uri.is_empty() {
        return Err(Error::SyncError("Connection URI is empty".into()));
    }
    let lower = uri.to_ascii_lowercase();
    if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
        return Err(Error::SyncError(
            "PostgreSQL remote sync is not available yet — use a sqlite: URI".into(),
        ));
    }

    let options = if lower.starts_with("sqlite:") {
        SqliteConnectOptions::from_str(uri)
            .map_err(|e| Error::DatabaseError(e.to_string()))?
            .create_if_missing(true)
            .foreign_keys(true)
    } else {
        SqliteConnectOptions::new()
            .filename(uri)
            .create_if_missing(true)
            .foreign_keys(true)
    };

    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .map_err(|e| Error::DatabaseError(format!("Could not open remote database: {e}")))
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

#[derive(Debug, Clone, Copy)]
enum Direction {
    Push,
    Pull,
}

/// Tables synced for everyone. Soft-delete tombstones travel as rows.
const PLAIN_TABLES: &[&str] = &["groups", "hosts", "snippets", "port_forwards"];
/// Tables holding vault envelopes; synced only with `sync_secrets` and a
/// matching vault header on both sides.
const SECRET_TABLES: &[&str] = &["identities", "credentials"];
/// Columns never copied to a remote (legacy plaintext secrets).
const NEVER_PUSHED: &[(&str, &str)] = &[("hosts", "password")];

async fn ensure_meta_table(pool: &SqlitePool) -> Result<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS sync_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        .execute(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok(())
}

/// Secrets are only exchanged between devices sharing one vault (same
/// salt + wrapped key); the first device to sync secrets publishes its header.
async fn check_vault_match(
    local: &SqlitePool,
    remote: &SqlitePool,
) -> std::result::Result<(), String> {
    use sqlx::Row;
    let db = |e: sqlx::Error| e.to_string();
    let local_header: Option<String> =
        sqlx::query("SELECT value FROM settings WHERE key = ?")
            .bind(crate::vault::VAULT_HEADER_SETTING)
            .fetch_optional(local)
            .await
            .map_err(db)?
            .map(|r| r.get("value"));
    let Some(local_header) = local_header else {
        return Err("secrets not synced: this device has no vault".into());
    };
    let remote_header: Option<String> =
        sqlx::query("SELECT value FROM sync_meta WHERE key = ?")
            .bind(crate::vault::VAULT_HEADER_SETTING)
            .fetch_optional(remote)
            .await
            .map_err(db)?
            .map(|r| r.get("value"));
    match remote_header {
        None => {
            sqlx::query("INSERT INTO sync_meta (key, value) VALUES (?, ?)")
                .bind(crate::vault::VAULT_HEADER_SETTING)
                .bind(&local_header)
                .execute(remote)
                .await
                .map_err(db)?;
            Ok(())
        }
        Some(remote_header) => {
            let same = match (
                crate::vault::parse_vault_header(&local_header),
                crate::vault::parse_vault_header(&remote_header),
            ) {
                (Ok(a), Ok(b)) => a.salt == b.salt && a.wrapped_dek == b.wrapped_dek,
                _ => false,
            };
            if same {
                Ok(())
            } else {
                Err("secrets not synced: this device's vault differs from the synced vault".into())
            }
        }
    }
}

/// One SQLite value, copied verbatim between databases.
#[derive(Debug, Clone)]
enum SqlValue {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

fn read_value(row: &sqlx::sqlite::SqliteRow, idx: usize) -> SqlValue {
    use sqlx::{Row, TypeInfo, ValueRef};
    let Ok(raw) = row.try_get_raw(idx) else {
        return SqlValue::Null;
    };
    if raw.is_null() {
        return SqlValue::Null;
    }
    let kind = raw.type_info().name().to_ascii_uppercase();
    match kind.as_str() {
        "INTEGER" | "INT" | "BIGINT" | "BOOLEAN" => row
            .try_get::<i64, _>(idx)
            .map(SqlValue::Int)
            .unwrap_or(SqlValue::Null),
        "REAL" | "FLOAT" | "DOUBLE" => row
            .try_get::<f64, _>(idx)
            .map(SqlValue::Real)
            .unwrap_or(SqlValue::Null),
        "BLOB" => row
            .try_get::<Vec<u8>, _>(idx)
            .map(SqlValue::Blob)
            .unwrap_or(SqlValue::Null),
        _ => row
            .try_get::<String, _>(idx)
            .map(SqlValue::Text)
            .or_else(|_| row.try_get::<i64, _>(idx).map(SqlValue::Int))
            .or_else(|_| row.try_get::<f64, _>(idx).map(SqlValue::Real))
            .unwrap_or(SqlValue::Null),
    }
}

async fn table_columns(pool: &SqlitePool, table: &str) -> Result<Vec<String>> {
    use sqlx::Row;
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok(rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("name").ok())
        .collect())
}

type TableRows =
    std::collections::HashMap<String, (Option<DateTime<Utc>>, Vec<SqlValue>)>;

async fn read_table(
    pool: &SqlitePool,
    table: &str,
    cols: &[String],
) -> Result<TableRows> {
    use sqlx::Row;
    let list = cols
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let rows = sqlx::query(&format!("SELECT {list} FROM {table}"))
        .fetch_all(pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    let id_idx = cols.iter().position(|c| c == "id");
    let ts_idx = cols.iter().position(|c| c == "updated_at");
    let mut out = TableRows::new();
    for row in rows {
        let Some(id) = id_idx.and_then(|i| row.try_get::<String, _>(i).ok()) else {
            continue;
        };
        let ts = ts_idx
            .and_then(|i| row.try_get::<String, _>(i).ok())
            .and_then(|raw| crate::store::parse_db_timestamp(&raw));
        let values = (0..cols.len()).map(|i| read_value(&row, i)).collect();
        out.insert(id, (ts, values));
    }
    Ok(out)
}

/// Copy every row of `table` that is newer in `from` (or missing in `to`).
/// Returns `(copied, skipped)`. Ties and older rows are left alone, so the
/// newest `updated_at` wins on both sides after a push + pull.
async fn merge_table(
    from: &SqlitePool,
    to: &SqlitePool,
    table: &str,
) -> Result<(u64, u64)> {
    let from_cols = table_columns(from, table).await?;
    let to_cols = table_columns(to, table).await?;
    let cols: Vec<String> = from_cols
        .into_iter()
        .filter(|c| to_cols.contains(c))
        .collect();
    if !cols.iter().any(|c| c == "id") {
        return Ok((0, 0));
    }
    let source = read_table(from, table, &cols).await?;
    let target = read_table(to, table, &cols).await?;

    let blanked: Vec<usize> = cols
        .iter()
        .enumerate()
        .filter(|(_, c)| NEVER_PUSHED.contains(&(table, c.as_str())))
        .map(|(i, _)| i)
        .collect();

    let quoted: Vec<String> = cols.iter().map(|c| format!("\"{c}\"")).collect();
    let placeholders = vec!["?"; cols.len()].join(", ");
    let updates = quoted
        .iter()
        .filter(|c| c.as_str() != "\"id\"")
        .map(|c| format!("{c} = excluded.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({placeholders}) ON CONFLICT(id) DO UPDATE SET {updates}",
        quoted.join(", ")
    );

    let mut copied = 0;
    let mut skipped = 0;
    let mut tx = to
        .begin()
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    for (id, (ts, values)) in &source {
        let newer = match target.get(id) {
            None => true,
            Some((theirs, _)) => match (ts, theirs) {
                (Some(ours), Some(theirs)) => ours > theirs,
                (Some(_), None) => true,
                _ => false,
            },
        };
        if !newer {
            skipped += 1;
            continue;
        }
        let mut query = sqlx::query(&sql);
        for (i, value) in values.iter().enumerate() {
            let value = if blanked.contains(&i) {
                &SqlValue::Null
            } else {
                value
            };
            query = match value {
                SqlValue::Null => query.bind(None::<String>),
                SqlValue::Int(v) => query.bind(*v),
                SqlValue::Real(v) => query.bind(*v),
                SqlValue::Text(v) => query.bind(v.clone()),
                SqlValue::Blob(v) => query.bind(v.clone()),
            };
        }
        query
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::DatabaseError(format!("sync {table} row {id}: {e}")))?;
        copied += 1;
    }
    tx.commit()
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?;
    Ok((copied, skipped))
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

    #[tokio::test]
    async fn open_remote_rejects_postgres_and_opens_sqlite() {
        let err = open_remote_sqlite_pool("postgres://user:pass@localhost/db")
            .await
            .expect_err("postgres not wired");
        assert!(err.to_string().contains("PostgreSQL"));

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("remote.db");
        let uri = format!("sqlite:{}", path.display());
        let pool = open_remote_sqlite_pool(&uri).await.expect("sqlite open");
        let engine = SyncEngine::new(SyncConfig::remote(&uri));
        engine.set_remote(pool).await.expect("attach");
        let local = crate::store::Store::open(dir.path().join("local"))
            .await
            .expect("local store");
        engine.attach_local(local.pool().clone()).await;
        let report = engine.sync_now().await.expect("sync");
        assert!(report.finished_at.is_some());
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
