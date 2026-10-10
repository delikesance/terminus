use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{Error, Result};

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
