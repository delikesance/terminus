use super::*;

/// What the sidebar shows besides the stored hosts.
///
/// Gathered together because they are all "where else can a session start" —
/// and because gathering them is filesystem work that belongs on the worker
/// thread, next to the database it is shown with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformFacts {
    /// Facts about the machine terminus runs on.
    pub machine: LocalMachine,
    /// WSL distros of the Windows machine this one is nested in, minus the
    /// one we are already in: that one *is* `machine`.
    pub distros: Vec<WslDistro>,
    /// Distro disks found with no way to name them. Kept as a count so the
    /// panel can admit they exist instead of silently dropping them.
    pub unnamed_distros: usize,
}

impl PlatformFacts {
    /// The name `wsl.exe -d` would take for a row id, if the row is a distro.
    pub fn distro_named(&self, id: &str) -> Option<&WslDistro> {
        let name = id.strip_prefix(WSL_PREFIX)?;
        self.distros.iter().find(|distro| distro.name == name)
    }
}

/// Gather the platform facts.
///
/// Called on the host worker: it reads `/proc`, the Windows drive and, when
/// interop is up, runs a `wsl.exe` that takes a moment to answer.
pub fn discover_platform() -> PlatformFacts {
    let machine = machine::detect();

    let Some(roots) = wsl::WindowsRoots::detect() else {
        // Not a WSL machine: there is no Windows side to enumerate.
        return PlatformFacts {
            machine,
            distros: Vec::new(),
            unnamed_distros: 0,
        };
    };

    let discovery = wsl::discover(&roots);
    let current = machine.wsl_distro.as_deref();
    let distros: Vec<WslDistro> = discovery
        .distros
        .into_iter()
        .filter(|distro| {
            // The distro we are inside is the *current computer* row, and
            // offering it twice would make the list lie about where a
            // session lands.
            !current.is_some_and(|current| current.eq_ignore_ascii_case(&distro.name))
        })
        .collect();

    PlatformFacts {
        machine,
        distros,
        unnamed_distros: discovery.unnamed,
    }
}

pub(super) fn session_status(id: &str, open_host_ids: &[String]) -> HostStatus {
    if open_host_ids.iter().any(|open| open == id) {
        HostStatus::Active
    } else {
        HostStatus::Idle
    }
}

pub(super) fn wsl_host_status(
    id: &str,
    open_host_ids: &[String],
    running: Option<bool>,
) -> HostStatus {
    if open_host_ids.iter().any(|open| open == id) {
        return HostStatus::Active;
    }
    match running {
        Some(true) => HostStatus::Running,
        Some(false) => HostStatus::Stopped,
        None => HostStatus::Idle,
    }
}

/// One open terminal, as the sidebar needs it under its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSession {
    pub tab_index: usize,
    /// `None` attaches under `local` for display.
    pub host_id: Option<String>,
    pub title: String,
    pub active: bool,
    pub closable: bool,
}

pub(super) fn host_item_from_row(
    host: &HostRow,
    open_host_ids: &[String],
    session_count: usize,
) -> HostItem {
    HostItem {
        id: host.id.clone(),
        name: host.name.clone(),
        endpoint: host.endpoint(),
        badge: Badge::Ssh,
        stored: true,
        os_id: host.os_id.clone(),
        status: session_status(&host.id, open_host_ids),
        nested: false,
        session_count,
    }
}

pub(super) fn sessions_for_host<'a>(
    sessions: &'a [OpenSession],
    host_id: &str,
) -> Vec<&'a OpenSession> {
    sessions
        .iter()
        .filter(|s| {
            let id = s.host_id.as_deref().unwrap_or(LOCAL_ID);
            id == host_id
        })
        .collect()
}

/// The sidebar's list, in order:
/// 1. `Local` — this computer + WSL distros (+ their open sessions)
/// 2. `Hosts` — ungrouped hosts and groups interleaved by `sort_order`
///
/// Empty groups stay in the list so a freshly created group is visible.
/// Default backfill puts hosts before groups once; the user can reorder freely.
pub fn sidebar_rows(
    platform: &PlatformFacts,
    hosts: &[HostRow],
    groups: &[(String, String, i64)],
    collapsed: &HashSet<String>,
    collapsed_hosts: &HashSet<String>,
    open_host_ids: &[String],
    sessions: &[OpenSession],
) -> Vec<Row> {
    let mut rows = Vec::with_capacity(
        hosts.len() + platform.distros.len() + groups.len() + sessions.len() + 4,
    );

    let local_sessions = sessions_for_host(sessions, LOCAL_ID);
    let local_endpoint = local_subtitle(&platform.machine);
    let local_os_id = {
        let id = platform.machine.os_id.trim();
        if id.is_empty() || id.eq_ignore_ascii_case("unknown") {
            None
        } else {
            Some(id.to_string())
        }
    };
    rows.push(Row::Section(LOCAL_SECTION.to_string()));
    rows.push(Row::Host(HostItem {
        id: LOCAL_ID.to_string(),
        name: "Local".to_string(),
        endpoint: local_endpoint,
        badge: Badge::Local,
        stored: false,
        os_id: local_os_id,
        status: session_status(LOCAL_ID, open_host_ids),
        nested: false,
        session_count: local_sessions.len(),
    }));

    for distro in &platform.distros {
        let id = format!("{WSL_PREFIX}{}", distro.name);
        let distro_sessions = sessions_for_host(sessions, &id);
        rows.push(Row::Host(HostItem {
            id: id.clone(),
            name: distro.display.clone(),
            endpoint: distro_subtitle(distro),
            badge: Badge::Wsl,
            stored: false,
            os_id: Some(distro.name.clone()),
            // Dot: Active (open tab) > Running/Stopped (WSL VM) > Idle.
            status: wsl_host_status(&id, open_host_ids, distro.running),
            nested: false,
            session_count: distro_sessions.len(),
        }));
    }

    // Sessions are shown as pills of the selected machine, not as rows;
    // rows only carry their count.
    let _ = collapsed_hosts;

    // One section per group (in group order), then "Servers" for the
    // ungrouped hosts — the design's machine sections.
    let mut ordered_groups: Vec<&(String, String, i64)> = groups.iter().collect();
    ordered_groups.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });
    for (group_id, group_name, _) in ordered_groups {
        let mut group_hosts: Vec<_> = hosts
            .iter()
            .filter(|host| host.group_id.as_deref() == Some(group_id.as_str()))
            .collect();
        group_hosts.sort_by(|a, b| {
            a.sort_order
                .cmp(&b.sort_order)
                .then_with(|| a.updated_at.cmp(&b.updated_at))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let is_collapsed = collapsed.contains(group_id);
        let group_session_count: usize = group_hosts
            .iter()
            .map(|h| sessions_for_host(sessions, &h.id).len())
            .sum();
        rows.push(Row::Group {
            id: group_id.to_string(),
            name: group_name.to_string(),
            host_count: group_hosts.len(),
            session_count: group_session_count,
            collapsed: is_collapsed,
        });
        if !is_collapsed {
            for host in group_hosts {
                let count = sessions_for_host(sessions, &host.id).len();
                let mut item = host_item_from_row(host, open_host_ids, count);
                item.nested = true;
                rows.push(Row::Host(item));
            }
        }
    }

    rows.push(Row::Section(HOSTS_SECTION.to_string()));
    let mut ungrouped: Vec<&HostRow> = hosts
        .iter()
        .filter(|host| host.group_id.is_none())
        .collect();
    ungrouped.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    for host in ungrouped {
        let count = sessions_for_host(sessions, &host.id).len();
        rows.push(Row::Host(host_item_from_row(host, open_host_ids, count)));
    }

    rows
}

/// `nixos@NixOS · WSL`, i.e. who and where.
///
/// The distro name is only spelled out when it is not already the hostname —
/// under WSL they are usually the same string, and saying it twice costs the
/// subtitle its width.
pub(super) fn local_subtitle(machine: &LocalMachine) -> String {
    let mut parts: Vec<String> = Vec::new();

    let endpoint = machine.endpoint();
    if !endpoint.is_empty() {
        parts.push(endpoint);
    }

    match &machine.wsl_distro {
        Some(distro) if !distro.eq_ignore_ascii_case(&machine.hostname) => {
            parts.push("WSL".to_string());
            parts.push(distro.clone());
        }
        Some(_) => parts.push("WSL".to_string()),
        None if machine.os_id == "wsl" => parts.push("WSL".to_string()),
        None if machine.os_id.is_empty() => {}
        None => parts.push(machine::os_label(&machine.os_id)),
    }

    parts.join(" · ")
}

/// `WSL`, `WSL · running`, `WSL · default`: what is known about a distro.
///
/// Its state comes from interop alone, so on a machine where interop is
/// unavailable the subtitle simply stops at `WSL` rather than guessing.
pub(super) fn distro_subtitle(distro: &WslDistro) -> String {
    let mut parts = vec!["WSL".to_string()];

    match distro.running {
        Some(true) => parts.push("running".to_string()),
        Some(false) => parts.push("stopped".to_string()),
        None => {}
    }
    if distro.is_default {
        parts.push("default".to_string());
    }

    parts.join(" · ")
}
