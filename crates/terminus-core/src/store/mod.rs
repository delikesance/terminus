//! Local persistence layer (SQLx/SQLite).
mod credentials;
mod forwards;
mod groups;
mod history;
mod hosts;
mod identities;
mod migrate;
mod settings;
mod snippets;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};
use std::path::PathBuf;
use uuid::Uuid;

use crate::error::{Error, Result};
use chrono::{DateTime, Utc};
use sqlx::Row;

/// Owner-only permissions for the data directory and database file.
#[cfg(unix)]
fn restrict_permissions(dir: &std::path::Path, db: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(db)?;
    std::fs::set_permissions(db, std::fs::Permissions::from_mode(0o600))?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let sibling = db.with_file_name(format!(
            "{}{suffix}",
            db.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("terminus.db")
        ));
        if sibling.exists() {
            std::fs::set_permissions(&sibling, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(())
}

/// Windows ACLs on the per-user data directory already scope access.
#[cfg(not(unix))]
fn restrict_permissions(_dir: &std::path::Path, _db: &std::path::Path) -> Result<()> {
    Ok(())
}

/// Decode a UUID column; `None` (row skipped) when it is missing or malformed.
fn row_uuid(r: &sqlx::sqlite::SqliteRow, col: &str) -> Option<Uuid> {
    let raw: String = r.try_get(col).ok()?;
    match Uuid::parse_str(raw.trim()) {
        Ok(id) => Some(id),
        Err(err) => {
            tracing::warn!(column = col, value = %raw, %err, "skipping row with malformed uuid");
            None
        }
    }
}

/// Decode a timestamp column. Accepts RFC 3339 (what this store writes) and
/// SQLite's `datetime()` form (`YYYY-MM-DD HH:MM:SS[.fff]`, UTC), so a row
/// written by another tool or a sync peer cannot take the whole list down.
fn row_ts(r: &sqlx::sqlite::SqliteRow, col: &str) -> Option<DateTime<Utc>> {
    let raw: String = r.try_get(col).ok()?;
    let parsed = parse_db_timestamp(&raw);
    if parsed.is_none() {
        tracing::warn!(column = col, value = %raw, "skipping row with malformed timestamp");
    }
    parsed
}

/// Parse a stored timestamp (RFC 3339 or SQLite `datetime()` format).
pub fn parse_db_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(raw) {
        return Some(d.with_timezone(&Utc));
    }
    for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(raw, fmt) {
            return Some(naive.and_utc());
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub async fn open(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("terminus.db");
        // The database holds managed SSH keys and sealed secrets: keep it (and
        // the directory SQLite creates its WAL/SHM siblings in) owner-only.
        // SQLite gives -wal / -shm files the database file's mode, so creating
        // the file 0600 up front covers them too.
        restrict_permissions(&data_dir, &db_path)?;

        // Options are built directly instead of using a
        // `sqlite://<path>?foreign_keys=on` URL: sqlx 0.7's SQLite URL parser
        // only accepts the `mode` and `cache` query parameters and rejects
        // every other one, so the URL form failed to open at all. This also
        // creates the database file when it does not exist yet.
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;

        Self::migrate(&pool).await?;
        let store = Self { pool };
        store.ensure_default_root_order().await?;
        Ok(store)
    }

    /// The underlying pool (the sync engine reads and merges through it).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Create / upgrade the Terminus schema on `pool` (local store or a sync
    /// remote: both carry the same tables).
    pub async fn ensure_schema(pool: &SqlitePool) -> Result<()> {
        Self::migrate(pool).await
    }
}
