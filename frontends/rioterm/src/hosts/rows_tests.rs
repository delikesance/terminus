use super::test_support::*;
use super::*;

fn machine(hostname: &str, user: &str, distro: Option<&str>) -> LocalMachine {
    LocalMachine {
        hostname: hostname.to_string(),
        username: user.to_string(),
        os_id: if distro.is_some() { "nixos" } else { "ubuntu" }.to_string(),
        wsl_distro: distro.map(str::to_string),
        wsl_kernel: distro.is_some(),
    }
}

fn distro(
    name: &str,
    display: &str,
    running: Option<bool>,
    is_default: bool,
) -> WslDistro {
    WslDistro {
        name: name.to_string(),
        display: display.to_string(),
        source: terminus_core::wsl::Source::WindowsTerminal,
        running,
        is_default,
    }
}

fn hosts_of(rows: &[Row]) -> Vec<&HostItem> {
    rows.iter().filter_map(Row::host).collect()
}

fn labels(rows: &[Row]) -> Vec<&str> {
    rows.iter().filter_map(Row::label).collect()
}

#[test]
fn the_local_machine_comes_first_and_always() {
    let platform = PlatformFacts {
        machine: machine("NixOS", "nixos", Some("NixOS")),
        distros: Vec::new(),
        unnamed_distros: 0,
    };

    let empty = HashSet::new();
    let rows = sidebar_rows(&platform, &[], &[], &empty, &empty, &[], &[]);
    assert_eq!(labels(&rows), vec![LOCAL_SECTION, HOSTS_SECTION]);

    let local = hosts_of(&rows)[0];
    assert_eq!(local.id, LOCAL_ID);
    assert_eq!(local.name, "Local");
    assert_eq!(local.badge, Badge::Local);
    assert_eq!(local.endpoint, "nixos@NixOS · WSL");
}

#[test]
fn a_collapsed_group_is_not_an_empty_host_list() {
    let host = |id: &str| HostRow {
        id: id.to_string(),
        name: id.to_string(),
        hostname: format!("{id}.internal"),
        port: 22,
        username: "root".to_string(),
        auth_method: "key".to_string(),
        identity_id: None,
        group_id: Some("g1".to_string()),
        tags: Vec::new(),
        notes: String::new(),
        os_id: None,
        sort_order: 0,
        updated_at: Utc::now(),
    };
    let groups = vec![("g1".to_string(), "prod".to_string(), 0)];
    let collapsed: HashSet<String> = ["g1".to_string()].into();
    let rows = sidebar_rows(
        &PlatformFacts::default(),
        &[host("a"), host("b")],
        &groups,
        &collapsed,
        &HashSet::new(),
        &[],
        &[],
    );
    let mut panel = terminus_ui::sidebar::HostPanel::default();
    panel.set_rows(rows);
    assert_eq!(panel.host_count(), 2);
    assert_eq!(panel.empty_hint(), None);
}

#[test]
fn the_distro_name_is_not_repeated_when_it_is_the_hostname() {
    let platform = PlatformFacts {
        machine: machine("NixOS", "nixos", Some("NixOS")),
        ..PlatformFacts::default()
    };
    assert_eq!(
        sidebar_rows(
            &platform,
            &[],
            &[],
            &HashSet::new(),
            &HashSet::new(),
            &[],
            &[],
        )[1]
        .host()
        .unwrap()
        .endpoint,
        "nixos@NixOS · WSL"
    );

    // A differently-named distro is worth spelling out.
    let platform = PlatformFacts {
        machine: machine("box", "nixos", Some("Ubuntu-24.04")),
        ..PlatformFacts::default()
    };
    assert_eq!(
        sidebar_rows(
            &platform,
            &[],
            &[],
            &HashSet::new(),
            &HashSet::new(),
            &[],
            &[],
        )[1]
        .host()
        .unwrap()
        .endpoint,
        "nixos@box · WSL · Ubuntu-24.04"
    );
}

#[test]
fn a_plain_linux_machine_is_labelled_by_its_os() {
    let platform = PlatformFacts {
        machine: machine("web-01", "deploy", None),
        ..PlatformFacts::default()
    };
    assert_eq!(
        sidebar_rows(
            &platform,
            &[],
            &[],
            &HashSet::new(),
            &HashSet::new(),
            &[],
            &[],
        )[1]
        .host()
        .unwrap()
        .endpoint,
        "deploy@web-01 · Ubuntu"
    );
}

#[test]
fn groups_come_before_the_servers_section() {
    let platform = PlatformFacts {
        machine: machine("NixOS", "nixos", Some("NixOS")),
        distros: Vec::new(),
        unnamed_distros: 0,
    };
    // Explicit sort_order mimics the one-shot hosts-then-groups backfill.
    let mut host = host_row("h1", "mainserver");
    host.sort_order = 0;
    let hosts = vec![host];
    let groups = vec![
        ("g1".into(), "jeremy".into(), 1),
        ("g2".into(), "test".into(), 2),
    ];
    let empty = HashSet::new();
    let rows = sidebar_rows(&platform, &hosts, &groups, &empty, &empty, &[], &[]);

    let mut root: Vec<&str> = Vec::new();
    for row in &rows {
        match row {
            Row::Host(h) if h.stored && !h.nested => root.push(h.name.as_str()),
            Row::Group { name, .. } => root.push(name.as_str()),
            _ => {}
        }
    }
    assert_eq!(root, vec!["jeremy", "test", "mainserver"]);
    assert_eq!(labels(&rows), vec![LOCAL_SECTION, HOSTS_SECTION]);
}

#[test]
fn groups_keep_their_own_sort_order() {
    let platform = PlatformFacts {
        machine: machine("NixOS", "nixos", Some("NixOS")),
        distros: Vec::new(),
        unnamed_distros: 0,
    };
    let mut host = host_row("h1", "mainserver");
    host.sort_order = 1;
    let hosts = vec![host];
    let groups = vec![
        ("g2".into(), "zeta".into(), 0),
        ("g1".into(), "alpha".into(), 1),
    ];
    let empty = HashSet::new();
    let rows = sidebar_rows(&platform, &hosts, &groups, &empty, &empty, &[], &[]);

    let mut root: Vec<&str> = Vec::new();
    for row in &rows {
        match row {
            Row::Host(h) if h.stored && !h.nested => root.push(h.name.as_str()),
            Row::Group { name, .. } => root.push(name.as_str()),
            _ => {}
        }
    }
    assert_eq!(root, vec!["zeta", "alpha", "mainserver"]);
}

#[test]
fn distros_and_hosts_get_their_own_groups() {
    let platform = PlatformFacts {
        machine: machine("NixOS", "nixos", Some("NixOS")),
        distros: vec![
            distro("Ubuntu-24.04", "Ubuntu 24.04 LTS", Some(true), false),
            distro("Alpine", "Alpine", None, true),
        ],
        unnamed_distros: 0,
    };
    let hosts = vec![host_row("9c1e", "web-01")];

    let rows = sidebar_rows(
        &platform,
        &hosts,
        &[],
        &HashSet::new(),
        &HashSet::new(),
        &[],
        &[],
    );

    assert_eq!(labels(&rows), vec![LOCAL_SECTION, HOSTS_SECTION]);

    let items = hosts_of(&rows);
    assert_eq!(
        items
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Local", "Ubuntu 24.04 LTS", "Alpine", "web-01"]
    );

    // A row id is what the screen resolves a session from, so it has to
    // name the distro exactly as `wsl.exe -d` wants it.
    assert_eq!(items[1].id, "wsl:Ubuntu-24.04");
    assert_eq!(items[1].badge, Badge::Wsl);
    assert_eq!(items[1].endpoint, "WSL · running");
    assert_eq!(items[2].endpoint, "WSL · default");
    assert_eq!(items[3].badge, Badge::Ssh);
    assert_eq!(items[3].endpoint, "root@web-01.internal");
    assert_eq!(
        platform.distro_named("wsl:Ubuntu-24.04").unwrap().name,
        "Ubuntu-24.04"
    );
    assert!(platform.distro_named("local").is_none());
}

#[test]
fn open_sessions_are_counted_on_their_machine_with_local_fallback() {
    let platform = PlatformFacts {
        machine: machine("NixOS", "nixos", Some("NixOS")),
        distros: Vec::new(),
        unnamed_distros: 0,
    };
    let hosts = vec![host_row("9c1e", "web-01")];
    let sessions = vec![
        OpenSession {
            tab_index: 0,
            host_id: None,
            title: "This computer".into(),
            active: true,
            closable: false,
        },
        OpenSession {
            tab_index: 1,
            host_id: Some("9c1e".into()),
            title: "web-01".into(),
            active: false,
            closable: true,
        },
    ];
    let empty = HashSet::new();
    let rows = sidebar_rows(
        &platform,
        &hosts,
        &[],
        &empty,
        &empty,
        &["local".into(), "9c1e".into()],
        &sessions,
    );
    assert!(
        !rows.iter().any(|r| matches!(r, Row::Session(_))),
        "sessions are pills now, not rows"
    );
    let counts: Vec<_> = rows
        .iter()
        .filter_map(Row::host)
        .map(|h| (h.id.as_str(), h.session_count))
        .collect();
    assert_eq!(counts, vec![("local", 1), ("9c1e", 1)]);
}

#[test]
fn an_unnamed_distro_still_shows_up_as_a_note() {
    let platform = PlatformFacts {
        machine: machine("NixOS", "nixos", Some("NixOS")),
        distros: Vec::new(),
        unnamed_distros: 1,
    };

    let empty = HashSet::new();
    let rows = sidebar_rows(&platform, &[], &[], &empty, &empty, &[], &[]);
    assert_eq!(labels(&rows), vec![LOCAL_SECTION, HOSTS_SECTION]);
    assert_eq!(hosts_of(&rows).len(), 1);
}
