use super::test_support::*;
use super::*;

#[test]
fn sidebar_errors_fit_the_toast_and_name_the_j5_settings() {
    for m in [
        msg::NO_PASSWORD,
        msg::NO_SSH_KEY,
        msg::PICK_SSH_KEY,
        msg::VAULT_FOR_PASSWORD,
        msg::VAULT_FOR_CONNECT,
        msg::VAULT_FOR_SSH_KEY,
    ] {
        // Sora, the UI face, has no arrow: it would draw a box.
        assert!(!m.contains('\u{2192}'), "{m}");
        assert!(
            !m.contains("Managed SSH Keys") && !m.contains("Remote SQL Sync"),
            "pre-J5 Settings names: {m}"
        );
        // The sidebar error band shows two lines, ~64 characters.
        assert!(m.chars().count() <= 64, "{m}");
    }
}

#[test]
fn group_membership_excludes_other_groups_and_ungrouped_hosts() {
    let in_group = |id: &str, group: Option<&str>| HostRow {
        group_id: group.map(str::to_string),
        ..host_row(id, id)
    };
    let hosts = [
        in_group("a", Some("g1")),
        in_group("b", Some("g2")),
        in_group("c", None),
        in_group("d", Some("g1")),
    ];
    assert_eq!(host_ids_in_group(&hosts, "g1"), ["a", "d"]);
    assert!(host_ids_in_group(&hosts, "missing").is_empty());
}

#[test]
fn a_browser_is_stale_when_its_host_is_gone_or_its_connection_changed() {
    let host = host_row("a", "alpha");
    let key = host.connection_key();
    let hosts = vec![host.clone(), host_row("b", "beta")];
    assert!(!sftp_connection_stale(&hosts, "a", &key));
    // Renamed, regrouped, re-noted: the same connection.
    let mut renamed = host.clone();
    renamed.name = "Alpha prod".into();
    renamed.group_id = Some("g".into());
    renamed.notes = "x".into();
    assert!(!sftp_connection_stale(&[renamed], "a", &key));
    // Deleted.
    assert!(sftp_connection_stale(&hosts[1..], "a", &key));
    // Address, port, user, key or auth method changed.
    type Edit = fn(&mut HostRow);
    let edits: [Edit; 5] = [
        |h| h.hostname = "other.internal".into(),
        |h| h.port = 2222,
        |h| h.username = "deploy".into(),
        |h| h.identity_id = Some("k2".into()),
        |h| h.auth_method = "password".into(),
    ];
    for edit in edits {
        let mut edited = host.clone();
        edit(&mut edited);
        assert!(sftp_connection_stale(&[edited], "a", &key));
    }
}
