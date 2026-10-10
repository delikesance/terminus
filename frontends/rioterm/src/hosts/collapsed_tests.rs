use super::test_support::*;
use super::*;
use std::time::{Duration, Instant};

/// Poll a `HostPersistHandle` for its outcome until `timeout` elapses.
fn wait_for_outcome(
    handle: &HostPersistHandle,
    timeout: Duration,
) -> Option<GroupCollapseOutcome> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(outcome) = handle.poll_outcome() {
            return Some(outcome);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn set_collapsed_true_persists_group_id() {
    let dir = temp_dir("set-true");
    let repo = HostRepository::spawn(dir.clone(), None);
    let handle = repo.persist_handle();

    handle.set_group_collapsed("g1", true);
    let outcome =
        wait_for_outcome(&handle, Duration::from_secs(10)).expect("outcome must arrive");
    assert_eq!(outcome.group_id, "g1");
    assert!(outcome.collapsed);
    assert!(
        outcome.error.is_none(),
        "unexpected error: {:?}",
        outcome.error
    );

    // Reopen: the group must be in the restored set.
    drop(repo);
    let mut reopened = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut reopened, Duration::from_secs(10), |r| r
        .collapsed_groups_seeded));
    let restored = reopened.take_collapsed_groups_seed().unwrap_or_default();
    assert!(
        restored.contains("g1"),
        "g1 should be in the restored set; got {restored:?}"
    );

    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn set_collapsed_false_removes_group_id() {
    let dir = temp_dir("set-false");
    let repo = HostRepository::spawn(dir.clone(), None);
    let handle = repo.persist_handle();

    // Collapse first.
    handle.set_group_collapsed("g2", true);
    wait_for_outcome(&handle, Duration::from_secs(10)).expect("first outcome");

    // Expand.
    handle.set_group_collapsed("g2", false);
    let outcome = wait_for_outcome(&handle, Duration::from_secs(10))
        .expect("second outcome must arrive");
    assert_eq!(outcome.group_id, "g2");
    assert!(!outcome.collapsed);
    assert!(
        outcome.error.is_none(),
        "unexpected error: {:?}",
        outcome.error
    );

    // Reopen: the group must be absent.
    drop(repo);
    let mut reopened = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut reopened, Duration::from_secs(10), |r| r
        .collapsed_groups_seeded));
    let restored = reopened.take_collapsed_groups_seed().unwrap_or_default();
    assert!(
        !restored.contains("g2"),
        "g2 should be absent (expanded); got {restored:?}"
    );

    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn set_collapsed_same_value_twice_is_idempotent() {
    let dir = temp_dir("idempotent");
    let repo = HostRepository::spawn(dir.clone(), None);
    let handle = repo.persist_handle();

    // Collapse twice — must not double-insert or produce an error.
    handle.set_group_collapsed("g3", true);
    wait_for_outcome(&handle, Duration::from_secs(10)).expect("first outcome");
    handle.set_group_collapsed("g3", true);
    wait_for_outcome(&handle, Duration::from_secs(10)).expect("second outcome");

    // Reopen: exactly one entry for g3.
    drop(repo);
    let mut reopened = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut reopened, Duration::from_secs(10), |r| r
        .collapsed_groups_seeded));
    let restored = reopened.take_collapsed_groups_seed().unwrap_or_default();
    assert!(
        restored.contains("g3"),
        "g3 should be collapsed after idempotent set; got {restored:?}"
    );
    // A `HashSet` cannot contain duplicates, so this also verifies no double-entry.
    assert_eq!(
        restored.iter().filter(|id| *id == "g3").count(),
        1,
        "g3 must appear exactly once in the set"
    );

    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn startup_restores_persisted_collapsed_groups() {
    let dir = temp_dir("startup-restore");
    let repo = HostRepository::spawn(dir.clone(), None);
    let handle = repo.persist_handle();

    handle.set_group_collapsed("persistent-group", true);
    wait_for_outcome(&handle, Duration::from_secs(10)).expect("outcome");

    drop(repo);

    let mut reopened = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut reopened, Duration::from_secs(10), |r| r
        .collapsed_groups_seeded));
    let restored = reopened.take_collapsed_groups_seed().unwrap_or_default();
    assert!(
        restored.contains("persistent-group"),
        "persistent-group must survive a restart; got {restored:?}"
    );
    // Second take is always None — seed is one-shot.
    assert!(
        reopened.take_collapsed_groups_seed().is_none(),
        "seed must be consumed after first take"
    );

    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refresh_after_local_change_does_not_overwrite_seed() {
    // After the first seed is delivered, subsequent Refresh commands must
    // not clobber it — collapsed_groups_seeded acts as a one-shot gate.
    let dir = temp_dir("no-overwrite");
    let mut repo = HostRepository::spawn(dir.clone(), None);

    // Wait for the first load (and CollapsedGroupsLoaded emission).
    assert!(drain_until(&mut repo, Duration::from_secs(10), |r| {
        r.collapsed_groups_seeded
    }));
    let first_seed = repo.take_collapsed_groups_seed();
    // Seed is consumed; future drains must not re-populate it.
    assert!(first_seed.is_some(), "seed must arrive on initial Refresh");

    // Trigger a second Refresh (simulates host-list reload after a create).
    let _ = repo.commands.send(Command::Refresh);
    // Allow time for the second CollapsedGroupsLoaded to arrive.
    std::thread::sleep(Duration::from_millis(300));
    repo.drain();

    assert!(
        repo.take_collapsed_groups_seed().is_none(),
        "second Refresh must not deliver a new seed — local state is authoritative"
    );

    drop(repo);
    let _ = std::fs::remove_dir_all(&dir);
}
