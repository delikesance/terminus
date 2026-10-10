use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
