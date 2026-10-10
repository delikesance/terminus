use super::registry::*;
use super::test_support::*;
use std::process::Command;
use std::time::Duration;

/// A killed app (SIGTERM/SIGKILL skip Drop) must not leave `ssh -N`
/// holding ports: the child dies with the thread that started it.
#[cfg(target_os = "linux")]
#[test]
fn a_tunnel_dies_with_the_thread_that_started_it() {
    let mut r = TunnelRegistry::new(Duration::from_millis(10));
    let mut r = std::thread::spawn(move || {
        r.start("orphan", sh("sleep 30")).unwrap();
        r
    })
    .join()
    .unwrap();
    let mut events = Vec::new();
    assert!(
        wait_for(|| {
            events.extend(r.poll());
            events
                .iter()
                .any(|e| matches!(e, TunnelEvent::Exited { id, .. } if id == "orphan"))
        }),
        "child outlived its parent thread: {events:?}"
    );
}

#[test]
fn a_process_that_stays_up_becomes_running_then_stops_cleanly() {
    let mut r = TunnelRegistry::new(Duration::from_millis(50));
    r.start("a", sh("sleep 30")).unwrap();
    assert!(r.is_active("a"));
    let mut events = Vec::new();
    assert!(wait_for(|| {
        events.extend(r.poll());
        events.contains(&TunnelEvent::Running("a".into()))
    }));
    assert!(r.stop("a"));
    assert!(!r.is_active("a"));
    assert!(r.poll().is_empty(), "a requested stop is not an error");
    assert!(!r.stop("a"));
}

#[test]
fn an_early_exit_reports_the_error_once() {
    let mut r = TunnelRegistry::new(Duration::from_millis(500));
    r.start("b", sh("echo 'bind: Address already in use' >&2; exit 255"))
        .unwrap();
    let mut got = None;
    assert!(wait_for(|| {
        for e in r.poll() {
            if let TunnelEvent::Exited { id, message } = e {
                got = Some((id, message));
            }
        }
        got.is_some()
    }));
    let (id, message) = got.unwrap();
    assert_eq!(id, "b");
    assert!(message.contains("already in use"), "{message}");
    assert!(!r.is_active("b"));
    assert!(r.poll().is_empty());
}

/// A login that fails never reaches ssh's `LocalCommand`: reaping the
/// process is what removes its temp key and askpass secret.
#[test]
fn a_failed_tunnel_removes_its_temp_secrets() {
    use crate::ssh_secrets::{private_temp_dir, write_private_file};
    let key = private_temp_dir(crate::ssh_secrets::IDENTITY_DIR)
        .unwrap()
        .join(format!("{}.pem", uuid::Uuid::new_v4()));
    write_private_file(&key, b"key").unwrap();
    let secret = private_temp_dir(crate::ssh_secrets::ASKPASS_DIR)
        .unwrap()
        .join(format!("{}.secret", uuid::Uuid::new_v4()));
    write_private_file(&secret, b"pass").unwrap();
    let mut cmd = sh("echo 'Permission denied (publickey).' >&2; exit 255");
    cmd.arg("-i").arg(&key);
    cmd.env(crate::ssh_secrets::ASKPASS_FILE_ENV, &secret);
    let mut r = TunnelRegistry::new(Duration::from_millis(500));
    r.start("k", cmd).unwrap();
    assert!(key.exists(), "removed before ssh could read it");
    assert!(wait_for(|| r
        .poll()
        .iter()
        .any(|e| matches!(e, TunnelEvent::Exited { .. }))));
    assert!(!key.exists(), "temp key left on disk after a failed login");
    assert!(!secret.exists(), "askpass secret left on disk");
}

#[test]
fn starting_twice_replaces_the_old_process() {
    let mut r = TunnelRegistry::new(Duration::from_millis(10));
    r.start("a", sh("sleep 30")).unwrap();
    r.start("a", sh("sleep 30")).unwrap();
    assert_eq!(r.active_ids(), vec!["a".to_string()]);
    r.stop_all();
    assert!(r.active_ids().is_empty());
}

#[test]
fn a_missing_program_is_a_start_error() {
    let mut r = TunnelRegistry::new(Duration::from_millis(10));
    let err = r
        .start("a", Command::new("definitely-not-a-real-binary-xyz"))
        .unwrap_err();
    assert!(err.contains("ssh") || err.contains("start"), "{err}");
    assert!(!r.is_active("a"));
}

#[test]
fn dropping_the_registry_kills_the_children() {
    let pid;
    {
        let mut r = TunnelRegistry::new(Duration::from_millis(10));
        r.start("a", sh("sleep 30")).unwrap();
        pid = r.pid("a").unwrap();
    }
    assert!(wait_for(|| unsafe { libc::kill(pid as i32, 0) } != 0));
}

#[test]
fn the_registry_reports_uptime_until_the_stop() {
    let mut r = TunnelRegistry::new(Duration::from_millis(10));
    assert_eq!(r.uptime("a"), None);
    r.start("a", sh("sleep 30")).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let up = r.uptime("a").unwrap();
    assert!(up >= Duration::from_millis(30) && up < Duration::from_secs(5));
    r.stop("a");
    assert_eq!(r.uptime("a"), None);
}
