//! Sync state shown outside Settings: the "last synced / N conflicts" line
//! on the sidebar's Settings button. Pure state and text, no paint; times
//! are Unix seconds so this crate needs no clock or date dependency.

/// How the badge is tinted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTone {
    /// Healthy.
    Ok,
    /// Conflicts need a look.
    Warn,
}

/// Snapshot of the sync engine for the chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncBadge {
    /// Sync is configured and attached (hidden otherwise).
    pub connected: bool,
    /// Finish time of the last successful run, Unix seconds.
    pub last_sync: Option<i64>,
    /// Conflicts reported by that run (`SyncReport::conflicts`).
    pub conflicts: u64,
}

impl SyncBadge {
    pub fn new(connected: bool, last_sync: Option<i64>, conflicts: u64) -> Self {
        Self {
            connected,
            last_sync,
            conflicts,
        }
    }

    /// Text for the badge at `now` (Unix seconds); `None` when sync is not
    /// connected.
    pub fn label(&self, now: i64) -> Option<String> {
        if !self.connected {
            return None;
        }
        if self.conflicts > 0 {
            let s = if self.conflicts == 1 { "" } else { "s" };
            return Some(format!("{} conflict{s}", self.conflicts));
        }
        let Some(at) = self.last_sync else {
            return Some("Synced".into());
        };
        let age = (now - at).max(0);
        Some(match age {
            0..=59 => "Synced just now".into(),
            60..=3599 => format!("Synced {}m ago", age / 60),
            3600..=86_399 => format!("Synced {}h ago", age / 3600),
            _ => format!("Synced {}d ago", age / 86_400),
        })
    }

    pub fn tone(&self) -> SyncTone {
        if self.conflicts > 0 {
            SyncTone::Warn
        } else {
            SyncTone::Ok
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000;

    #[test]
    fn hidden_when_not_connected() {
        assert_eq!(SyncBadge::new(false, Some(NOW), 3).label(NOW), None);
    }

    #[test]
    fn connected_without_run_is_plain_synced() {
        assert_eq!(
            SyncBadge::new(true, None, 0).label(NOW).as_deref(),
            Some("Synced")
        );
    }

    #[test]
    fn relative_age() {
        let l = |ago: i64| SyncBadge::new(true, Some(NOW - ago), 0).label(NOW).unwrap();
        assert_eq!(l(5), "Synced just now");
        assert_eq!(l(59), "Synced just now");
        assert_eq!(l(120), "Synced 2m ago");
        assert_eq!(l(3 * 3600), "Synced 3h ago");
        assert_eq!(l(2 * 86_400), "Synced 2d ago");
    }

    #[test]
    fn future_timestamp_does_not_underflow() {
        let b = SyncBadge::new(true, Some(NOW + 500), 0);
        assert_eq!(b.label(NOW).as_deref(), Some("Synced just now"));
    }

    #[test]
    fn conflicts_take_over_the_label() {
        let b = SyncBadge::new(true, Some(NOW - 10), 1);
        assert_eq!(b.label(NOW).as_deref(), Some("1 conflict"));
        assert_eq!(b.tone(), SyncTone::Warn);
        let b = SyncBadge::new(true, Some(NOW - 10), 4);
        assert_eq!(b.label(NOW).as_deref(), Some("4 conflicts"));
    }

    #[test]
    fn no_conflicts_is_ok_tone() {
        assert_eq!(SyncBadge::new(true, None, 0).tone(), SyncTone::Ok);
    }
}
