use super::*;
use crate::sidebar::Badge;

pub(super) fn centre(r: &crate::geom::Rect) -> (f32, f32) {
    (r.x + r.width / 2.0, r.y + r.height / 2.0)
}

pub(super) fn chrome_with_pills() -> Chrome {
    let mut chrome = chrome_with_hosts(1);
    chrome.shell.machine = Some(crate::shell::MachineInfo {
        id: "id-0".into(),
        name: "host-0".into(),
        address: "root@host-0".into(),
    });
    chrome.shell.pills = vec![
        crate::shell::SessionPill {
            tab_index: 0,
            label: "home".into(),
            active: false,
            new_output: false,
            closable: false,
        },
        crate::shell::SessionPill {
            tab_index: 7,
            label: "~".into(),
            active: true,
            new_output: false,
            closable: true,
        },
    ];
    chrome
}

pub(super) fn chrome_with_hosts(n: usize) -> Chrome {
    let mut chrome = Chrome::default();
    let mut rows = vec![Row::Section("Hosts".to_string())];
    rows.extend((0..n).map(|i| {
        Row::Host(HostItem {
            id: format!("id-{i}"),
            name: format!("host-{i}"),
            endpoint: format!("root@host-{i}"),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: crate::os_icons::HostStatus::Idle,
            nested: false,
            session_count: 0,
        })
    }));
    chrome.set_rows(rows);
    chrome
}
