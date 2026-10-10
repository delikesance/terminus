use super::*;
use std::time::{Duration, Instant};

pub(super) fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("terminus-hosts-{tag}-{}", Uuid::new_v4()))
}

/// Poll `drain` until `done` holds or the deadline passes.
///
/// Draining is not one-event-per-call — `drain` applies everything that
/// has arrived — so tests wait on observable state rather than counting
/// events.
pub(super) fn drain_until(
    repo: &mut HostRepository,
    timeout: Duration,
    done: impl Fn(&HostRepository) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        repo.drain();
        if done(repo) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn host_row(id: &str, name: &str) -> HostRow {
    HostRow {
        id: id.to_string(),
        name: name.to_string(),
        hostname: format!("{name}.internal"),
        port: 22,
        username: "root".to_string(),
        auth_method: "key".to_string(),
        identity_id: None,
        group_id: None,
        tags: Vec::new(),
        notes: String::new(),
        os_id: None,
        sort_order: 0,
        updated_at: Utc::now(),
    }
}
