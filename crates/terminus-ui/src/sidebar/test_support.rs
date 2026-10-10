use super::*;
use crate::os_icons::HostStatus;

pub(super) fn items(n: usize) -> Vec<HostItem> {
    (0..n)
        .map(|i| HostItem {
            id: format!("id-{i}"),
            name: format!("host-{i}"),
            endpoint: format!("deploy@host-{i}:2222"),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Idle,
            nested: false,
            session_count: 0,
        })
        .collect()
}

pub(super) fn panel(n: usize) -> HostPanel {
    let mut panel = HostPanel::default();
    panel.set_items(items(n));
    panel
}

/// A viewport tall enough that N rows fit without scrolling.
pub(super) fn tall() -> (f32, f32) {
    (0.0, 1000.0)
}

/// Local section, platform rows, then a Hosts section.
pub(super) fn grouped() -> Vec<Row> {
    vec![
        Row::Section("Local".to_string()),
        Row::Host(HostItem {
            id: "local".to_string(),
            name: "This computer".to_string(),
            endpoint: "nixos@NixOS".to_string(),
            badge: Badge::Local,
            stored: false,
            os_id: Some("nixos".to_string()),
            status: HostStatus::Idle,
            nested: false,
            session_count: 0,
        }),
        Row::Host(HostItem {
            id: "wsl:Ubuntu-24.04".to_string(),
            name: "Ubuntu 24.04 LTS".to_string(),
            endpoint: "windows".to_string(),
            badge: Badge::Wsl,
            stored: false,
            os_id: None,
            status: HostStatus::Running,
            nested: false,
            session_count: 0,
        }),
        Row::Section("Hosts".to_string()),
        Row::Host(HostItem {
            id: "9c1e".to_string(),
            name: "web-01".to_string(),
            endpoint: "deploy@web-01:2222".to_string(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Idle,
            nested: false,
            session_count: 0,
        }),
        Row::Host(HostItem {
            id: "77ab".to_string(),
            name: "db-01".to_string(),
            endpoint: "deploy@db-01:22".to_string(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Active,
            nested: false,
            session_count: 0,
        }),
    ]
}

// ---- intent slice --------------------------------------------------

pub(super) fn panel_with_sessions() -> HostPanel {
    let mut p = HostPanel::default();
    p.set_rows(vec![
        Row::Host(HostItem {
            id: "host-a".into(),
            name: "Host A".into(),
            endpoint: "a@host-a".into(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Active,
            nested: false,
            session_count: 2,
        }),
        Row::Session(SessionItem {
            tab_index: 0,
            host_id: "host-a".into(),
            title: "shell".into(),
            active: true,
            closable: false,
        }),
        Row::Session(SessionItem {
            tab_index: 1,
            host_id: "host-a".into(),
            title: "vim".into(),
            active: false,
            closable: true,
        }),
    ]);
    p.row_controls = true;
    p
}

pub(super) fn panel_with_group() -> HostPanel {
    let mut p = HostPanel::default();
    p.set_rows(vec![
        Row::Group {
            id: "grp-1".into(),
            name: "Production".into(),
            host_count: 1,
            session_count: 0,
            collapsed: false,
        },
        Row::Host(HostItem {
            id: "nested-host".into(),
            name: "web-01".into(),
            endpoint: "deploy@web-01".into(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Idle,
            nested: true,
            session_count: 0,
        }),
    ]);
    p
}

pub(super) fn host(id: &str, nested: bool, stored: bool) -> Row {
    Row::Host(HostItem {
        id: id.into(),
        name: id.into(),
        endpoint: format!("root@{id}"),
        badge: if stored { Badge::Ssh } else { Badge::Local },
        stored,
        os_id: None,
        status: HostStatus::Idle,
        nested,
        session_count: 0,
    })
}

pub(super) fn shell_rows() -> Vec<Row> {
    vec![
        Row::Section(LOCAL_SECTION.into()),
        host("local", false, false),
        Row::Group {
            id: "g1".into(),
            name: "jeremy".into(),
            host_count: 1,
            session_count: 0,
            collapsed: false,
        },
        host("prod", true, true),
        Row::Section(SERVERS_SECTION.into()),
        host("h2", false, true),
    ]
}

pub(super) fn panel_fit() -> HostPanel {
    panel(2)
}

/// 922x630: the default window clamped to a 1024x700 screen.
pub(super) const SMALL_H: f32 = 630.0;
