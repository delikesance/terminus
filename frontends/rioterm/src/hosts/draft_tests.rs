use super::test_support::*;
use super::*;

#[test]
fn parses_ports_with_default_for_empty() {
    assert_eq!(parse_port(""), Ok(22));
    assert_eq!(parse_port("   "), Ok(22));
    assert_eq!(parse_port("2222"), Ok(2222));
    assert!(parse_port("ssh").is_err());
    assert!(parse_port("70000").is_err());
}

#[test]
fn normalize_rejects_hostnames_and_usernames_ssh_would_parse_as_options() {
    for (hostname, username) in [
        ("-oProxyCommand=touch /tmp/x", "root"),
        ("-oProxyCommand=x", "root"),
        ("box host", "root"),
        ("bo\nx", "root"),
        ("box", "-oProxyCommand=x"),
        ("box", "ro ot"),
        ("box", "a\tb"),
    ] {
        let draft = HostDraft {
            hostname: hostname.to_string(),
            username: username.to_string(),
            auth_method: "gssapi".to_string(),
            ..HostDraft::default()
        };
        assert!(draft.normalize().is_err(), "{hostname:?} {username:?}");
    }
}

#[test]
fn normalize_fills_defaults_and_rejects_missing_hostname() {
    let draft = HostDraft {
        name: "  ".to_string(),
        hostname: " box.internal ".to_string(),
        username: String::new(),
        port: String::new(),
        auth_method: "gssapi".to_string(),
        ..HostDraft::default()
    }
    .normalize()
    .expect("hostname present");
    assert_eq!(draft.name, "box.internal");
    assert_eq!(draft.username, "root");
    assert_eq!(draft.hostname, "box.internal");
    assert_eq!(draft.resolved_port(), Ok(22));
    assert_eq!(draft.auth_method, "gssapi");

    assert!(HostDraft::default().normalize().is_err());
    let bad_port = HostDraft {
        hostname: "box".to_string(),
        port: "not-a-port".to_string(),
        auth_method: "gssapi".to_string(),
        ..HostDraft::default()
    };
    assert!(bad_port.normalize().is_err());

    let key_missing = HostDraft {
        hostname: "box".to_string(),
        auth_method: "key".to_string(),
        ..HostDraft::default()
    };
    assert!(key_missing.normalize().unwrap_err().contains("SSH key"));

    let pw_missing = HostDraft {
        hostname: "box".to_string(),
        auth_method: "password".to_string(),
        ..HostDraft::default()
    };
    assert!(pw_missing.normalize().unwrap_err().contains("Password"));

    let pw_keep = HostDraft {
        id: Some("existing".into()),
        hostname: "box".to_string(),
        auth_method: "password".to_string(),
        ..HostDraft::default()
    };
    assert!(pw_keep.normalize().is_ok());
}

#[test]
fn endpoint_hides_the_default_port() {
    let mut row = HostRow {
        id: "id".to_string(),
        name: "Box".to_string(),
        hostname: "box.internal".to_string(),
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
    };
    assert_eq!(row.endpoint(), "root@box.internal");
    assert_eq!(row.ssh_command(), "ssh root@box.internal");
    row.port = 2222;
    assert_eq!(row.endpoint(), "root@box.internal:2222");
    assert_eq!(row.ssh_command(), "ssh -p 2222 root@box.internal");
    row.username = String::new();
    assert_eq!(row.endpoint(), "box.internal:2222");
}

#[test]
fn a_private_key_can_be_given_by_path() {
    let dir = temp_dir("keypath");
    std::fs::create_dir_all(&dir).unwrap();
    let generated = terminus_core::generate_ed25519_identity("t").expect("pem");
    let pem = generated.private_key.expect("private");
    let key = dir.join("id_ed25519");
    std::fs::write(&key, &pem).unwrap();
    std::fs::write(dir.join("id_ed25519.pub"), generated.public_key.unwrap()).unwrap();

    let home = Some(dir.as_path());
    // Pasted PEM is used as is.
    let read = |input: &str| resolve_private_key_input(input, home).unwrap();
    assert_eq!(read(&pem).trim(), pem.trim());
    // A path, absolute or under ~, is read.
    assert_eq!(read(key.to_str().unwrap()).trim(), pem.trim());
    assert_eq!(read(" ~/id_ed25519 ").trim(), pem.trim());
    // The public half is refused with a hint.
    let err = resolve_private_key_input("~/id_ed25519.pub", home).unwrap_err();
    assert!(err.contains("public"), "{err}");
    // A missing file says so.
    let err = resolve_private_key_input("~/nope", home).unwrap_err();
    assert!(err.contains("No file"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_draft_names_the_same_endpoint_as_its_stored_row() {
    let draft = HostDraft {
        hostname: " 127.0.0.1 ".to_string(),
        username: "tuser".to_string(),
        port: "2223".to_string(),
        auth_method: "gssapi".to_string(),
        ..HostDraft::default()
    }
    .normalize()
    .expect("valid");
    assert_eq!(draft.endpoint(), "tuser@127.0.0.1:2223");
    let default_port = HostDraft {
        hostname: "box".to_string(),
        auth_method: "gssapi".to_string(),
        ..HostDraft::default()
    }
    .normalize()
    .expect("valid");
    assert_eq!(default_port.endpoint(), "root@box");
}
