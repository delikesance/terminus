use super::*;

#[test]
fn apply_os_detected_updates_host_row() {
    let mut hosts = vec![HostRow {
        id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into(),
        name: "box".into(),
        hostname: "box.example".into(),
        port: 22,
        username: "u".into(),
        auth_method: "key".into(),
        identity_id: None,
        group_id: None,
        tags: Vec::new(),
        notes: String::new(),
        sort_order: 0,
        os_id: None,
        updated_at: Utc::now(),
    }];
    assert!(apply_os_detected(
        &mut hosts,
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        "nixos"
    ));
    assert_eq!(hosts[0].os_id.as_deref(), Some("nixos"));
    assert!(!apply_os_detected(
        &mut hosts,
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        "nixos"
    ));
    assert!(!apply_os_detected(&mut hosts, "missing", "ubuntu"));
    assert!(!apply_os_detected(
        &mut hosts,
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        ""
    ));
}
