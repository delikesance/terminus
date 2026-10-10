use super::known_hosts::is_rsa;
use super::*;

pub(super) fn temp_known_hosts(tag: &str) -> KnownHosts {
    let path = std::env::temp_dir()
        .join(format!("terminus-known-hosts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    KnownHosts::at(path)
}

#[test]
fn entry_name_matches_openssh_convention() {
    assert_eq!(KnownHosts::entry_name("example.com", 22), "example.com");
    assert_eq!(
        KnownHosts::entry_name("example.com", 2222),
        "[example.com]:2222"
    );
}

#[test]
fn lookup_parses_entries_and_ignores_comments() {
    let kh = temp_known_hosts("lookup");
    std::fs::write(
            kh.path(),
            "# comment\n\nexample.com ssh-ed25519 AAAAKEY\nexample.com,10.0.0.1 rsa-sha2-512 BBBBKEY\n",
        )
        .unwrap();
    let entries = kh.lookup("example.com", 22);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].algorithm, "ssh-ed25519");
    assert_eq!(entries[1].key, "BBBBKEY");
    assert!(
        kh.lookup("10.0.0.1", 22).is_empty()
            || kh.lookup("10.0.0.1", 22)[0].key == "BBBBKEY"
    );
    let _ = std::fs::remove_file(kh.path());
}

#[test]
fn lookup_uses_bracketed_form_for_non_standard_ports() {
    let kh = temp_known_hosts("ports");
    std::fs::write(kh.path(), "[example.com]:2222 ssh-ed25519 KEY2222\n").unwrap();
    assert!(kh.lookup("example.com", 22).is_empty());
    assert_eq!(kh.lookup("example.com", 2222)[0].key, "KEY2222");
    let _ = std::fs::remove_file(kh.path());
}

#[test]
fn lookup_matches_openssh_hashed_entries() {
    // Written by `ssh-keygen -H` (HashKnownHosts yes, Debian/Ubuntu default).
    let kh = temp_known_hosts("hashed");
    std::fs::write(
            kh.path(),
            "|1|Bo57nr5sulZwOhqnzRIAeEOVSHc=|XzhJQGRrF4yLCnrFamfaJgAyXlE= ssh-ed25519 AAAAKEY\n\
             |1|g3yTRG7U8sQLMOZEPY+hx5kEomo=|IOTmpGQKZFzSy7adpBoSIYvg4Fg= ssh-ed25519 KEY2222\n",
        )
        .unwrap();
    assert_eq!(kh.lookup("example.com", 22)[0].key, "AAAAKEY");
    assert_eq!(kh.lookup("example.com", 2222)[0].key, "KEY2222");
    assert!(kh.lookup("other.com", 22).is_empty());
    let _ = std::fs::remove_file(kh.path());
}

#[test]
fn lookup_skips_marker_lines() {
    let kh = temp_known_hosts("markers");
    std::fs::write(
            kh.path(),
            "@revoked example.com ssh-ed25519 REVOKED\n@cert-authority *.example.com ssh-ed25519 CA\n",
        )
        .unwrap();
    assert!(kh.lookup("example.com", 22).is_empty());
    let _ = std::fs::remove_file(kh.path());
}

#[test]
fn preferred_host_key_algorithms_put_known_types_first() {
    use russh::keys::Algorithm;
    let known = vec![KnownHostEntry {
        algorithm: "ecdsa-sha2-nistp256".into(),
        key: "X".into(),
    }];
    let order = preferred_host_key_algorithms(&known);
    assert_eq!(
        order[0],
        Algorithm::Ecdsa {
            curve: russh::keys::EcdsaCurve::NistP256
        }
    );
    assert_eq!(order.len(), russh::Preferred::DEFAULT.key.len());

    let rsa = vec![KnownHostEntry {
        algorithm: "ssh-rsa".into(),
        key: "X".into(),
    }];
    assert!(is_rsa(&preferred_host_key_algorithms(&rsa)[0]));
    assert_eq!(
        preferred_host_key_algorithms(&[]),
        russh::Preferred::DEFAULT.key.to_vec()
    );
}
