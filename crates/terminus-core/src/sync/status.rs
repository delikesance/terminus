use std::fmt;

use serde::{Deserialize, Serialize};

/// Sync engine state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SyncStatus {
    /// No remote endpoint configured.
    #[default]
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

impl fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
