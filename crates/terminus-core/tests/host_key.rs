use russh::keys::parse_public_key_base64;
use std::io::Write;
use terminus_core::error::Error;
use terminus_core::ssh::{
    host_key_fingerprint, trust_host_key, verify_host_key,
};

const KEY_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
const KEY_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X";

fn write_known_hosts(contents: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("known_hosts");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
    dir
}

#[test]
fn accepts_matching_known_host_key() {
    let dir = write_known_hosts(&format!("[127.0.0.1]:2222 ssh-ed25519 {KEY_A}\n"));
    let key = parse_public_key_base64(KEY_A).unwrap();
    let path = dir.path().join("known_hosts");
    assert!(verify_host_key("127.0.0.1", 2222, &key, Some(&path)).unwrap());
}

#[test]
fn rejects_unknown_host_key() {
    let dir = write_known_hosts("");
    let key = parse_public_key_base64(KEY_A).unwrap();
    let path = dir.path().join("known_hosts");
    let err = verify_host_key("127.0.0.1", 2222, &key, Some(&path)).unwrap_err();
    match err {
        Error::HostKeyUnknown {
            host,
            port,
            public_key,
            algo,
            fingerprint,
        } => {
            assert_eq!(host, "127.0.0.1");
            assert_eq!(port, 2222);
            assert!(public_key.contains(KEY_A), "public_key={public_key}");
            assert_eq!(algo, "ssh-ed25519");
            assert!(fingerprint.starts_with("SHA256:"), "fingerprint={fingerprint}");
            let fp = host_key_fingerprint(&public_key).unwrap();
            assert_eq!(fp.algo, algo);
            assert_eq!(fp.sha256, fingerprint);
        }
        other => panic!("expected HostKeyUnknown, got {other:?}"),
    }
}

#[test]
fn trust_appends_atomically_then_verify_ok() {
    let dir = write_known_hosts("");
    let path = dir.path().join("known_hosts");
    let key = parse_public_key_base64(KEY_A).unwrap();
    let public_key = key.to_openssh().unwrap();

    // Cancel path: never call trust → file stays empty, verify still unknown.
    let before = std::fs::read_to_string(&path).unwrap();
    assert!(before.trim().is_empty());
    let err = verify_host_key("127.0.0.1", 2222, &key, Some(&path)).unwrap_err();
    assert!(matches!(err, Error::HostKeyUnknown { .. }));
    let after_cancel = std::fs::read_to_string(&path).unwrap();
    assert_eq!(before, after_cancel, "cancel must not write known_hosts");

    trust_host_key("127.0.0.1", 2222, &public_key, None, Some(&path)).unwrap();
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains(KEY_A));
    assert!(written.contains("[127.0.0.1]:2222"));
    assert!(verify_host_key("127.0.0.1", 2222, &key, Some(&path)).unwrap());
}

#[test]
fn rejects_mismatched_host_key_fail_closed() {
    let dir = write_known_hosts(&format!("[127.0.0.1]:2222 ssh-ed25519 {KEY_A}\n"));
    let key = parse_public_key_base64(KEY_B).unwrap();
    let path = dir.path().join("known_hosts");
    let err = verify_host_key("127.0.0.1", 2222, &key, Some(&path)).unwrap_err();
    match err {
        Error::HostKeyMismatch {
            host,
            port,
            line,
            public_key,
            algo,
            fingerprint,
        } => {
            assert_eq!(host, "127.0.0.1");
            assert_eq!(port, 2222);
            assert_eq!(line, 1);
            assert!(public_key.contains(KEY_B), "public_key={public_key}");
            assert_eq!(algo, "ssh-ed25519");
            assert!(fingerprint.starts_with("SHA256:"));
        }
        other => panic!("expected HostKeyMismatch, got {other:?}"),
    }

    // Without trust, file unchanged and still fail-closed.
    let unchanged = std::fs::read_to_string(&path).unwrap();
    assert!(unchanged.contains(KEY_A));
    assert!(!unchanged.contains(KEY_B));
    let again = verify_host_key("127.0.0.1", 2222, &key, Some(&path)).unwrap_err();
    assert!(matches!(again, Error::HostKeyMismatch { .. }));
}

#[test]
fn mismatch_remove_and_trust_then_connect_ok() {
    let dir = write_known_hosts(&format!("[127.0.0.1]:2222 ssh-ed25519 {KEY_A}\n"));
    let path = dir.path().join("known_hosts");
    let key_b = parse_public_key_base64(KEY_B).unwrap();
    let public_key = key_b.to_openssh().unwrap();

    let err = verify_host_key("127.0.0.1", 2222, &key_b, Some(&path)).unwrap_err();
    let line = match err {
        Error::HostKeyMismatch { line, .. } => line,
        other => panic!("expected HostKeyMismatch, got {other:?}"),
    };

    trust_host_key(
        "127.0.0.1",
        2222,
        &public_key,
        Some(line),
        Some(&path),
    )
    .unwrap();

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(!written.contains(KEY_A), "old key must be removed");
    assert!(written.contains(KEY_B), "new key must be trusted");
    assert!(verify_host_key("127.0.0.1", 2222, &key_b, Some(&path)).unwrap());
}

#[test]
fn cancel_leaves_known_hosts_untouched() {
    let dir = write_known_hosts(&format!("example.com ssh-ed25519 {KEY_A}\n"));
    let path = dir.path().join("known_hosts");
    let before = std::fs::read_to_string(&path).unwrap();
    let key = parse_public_key_base64(KEY_B).unwrap();

    // Simulate UI cancel: verify fails, caller does not invoke trust.
    let _ = verify_host_key("127.0.0.1", 2222, &key, Some(&path)).unwrap_err();
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(before, after);
}

#[test]
fn trust_overwrites_existing_known_hosts_file() {
    // Exercises atomic_write replace when dest already exists (Windows-critical path).
    let dir = write_known_hosts(&format!("other.example ssh-ed25519 {KEY_B}\n"));
    let path = dir.path().join("known_hosts");
    let key = parse_public_key_base64(KEY_A).unwrap();
    let public_key = key.to_openssh().unwrap();

    trust_host_key("127.0.0.1", 2222, &public_key, None, Some(&path)).unwrap();
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("other.example"), "prior entries preserved");
    assert!(written.contains(KEY_A));
    assert!(verify_host_key("127.0.0.1", 2222, &key, Some(&path)).unwrap());

    // Second trust (append again) must also succeed when dest exists.
    trust_host_key("10.0.0.1", 22, &public_key, None, Some(&path)).unwrap();
    let again = std::fs::read_to_string(&path).unwrap();
    assert!(again.contains("10.0.0.1"));
    assert!(again.matches(KEY_A).count() >= 2);
}

#[test]
fn mismatch_replace_is_single_rewrite() {
    // replace_line + append must not leave a half-updated file: old gone, new present.
    let dir = write_known_hosts(&format!(
        "keep.example ssh-ed25519 {KEY_A}\n[127.0.0.1]:2222 ssh-ed25519 {KEY_A}\n"
    ));
    let path = dir.path().join("known_hosts");
    let key_b = parse_public_key_base64(KEY_B).unwrap();
    let public_key = key_b.to_openssh().unwrap();

    trust_host_key(
        "127.0.0.1",
        2222,
        &public_key,
        Some(2),
        Some(&path),
    )
    .unwrap();

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("keep.example"));
    assert!(written.contains(KEY_A)); // keep.example still uses KEY_A
    assert!(written.contains(KEY_B));
    assert_eq!(
        written
            .lines()
            .filter(|l| l.contains("[127.0.0.1]:2222"))
            .count(),
        1,
        "exactly one entry for the replaced host"
    );
    assert!(verify_host_key("127.0.0.1", 2222, &key_b, Some(&path)).unwrap());
}
