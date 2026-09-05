use russh::keys::parse_public_key_base64;
use std::io::Write;
use terminus_core::error::Error;
use terminus_core::ssh::verify_host_key;

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
        Error::HostKeyUnknown { host, port } => {
            assert_eq!(host, "127.0.0.1");
            assert_eq!(port, 2222);
        }
        other => panic!("expected HostKeyUnknown, got {other:?}"),
    }
}

#[test]
fn rejects_mismatched_host_key() {
    let dir = write_known_hosts(&format!("[127.0.0.1]:2222 ssh-ed25519 {KEY_A}\n"));
    let key = parse_public_key_base64(KEY_B).unwrap();
    let path = dir.path().join("known_hosts");
    let err = verify_host_key("127.0.0.1", 2222, &key, Some(&path)).unwrap_err();
    match err {
        Error::HostKeyMismatch { host, port, line } => {
            assert_eq!(host, "127.0.0.1");
            assert_eq!(port, 2222);
            assert_eq!(line, 1);
        }
        other => panic!("expected HostKeyMismatch, got {other:?}"),
    }
}
