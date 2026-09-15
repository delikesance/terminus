// Copyright (c) 2026-present, Terminus Contributors.
//! Host storage for the UI.
//!
//! The window threads can't await on `terminus_core::Store` — rio runs on
//! `corcovado`'s event loop, while `Store` (SQLx) needs a real `tokio`
//! runtime to drive its pool. So the database lives on its own thread with
//! its own runtime, and the UI talks to it with plain channels:
//!
//! ```text
//! UI thread                 worker thread (tokio)
//! ---------                 -------------------
//! create(draft)  --Command--> upsert_host()
//! drain()        <--HostEvent-- list_hosts()
//! ```
//!
//! Every mutation answers with the freshly-read list, so the sidebar always
//! shows database truth rather than an optimistic local guess. `wake` is
//! called after each answer so the app can repaint even when it is idle.
//!
//! The sidebar shows three things, of which only the last one lives in the
//! database: *this computer* (a local shell), the WSL distros installed on
//! the Windows machine this one is nested in, and the stored SSH hosts. The
//! first two are platform facts, gathered once per refresh by
//! [`terminus_core::machine`] and [`terminus_core::wsl`] on this same worker
//! thread — they read the filesystem, so they must not run on the UI thread.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use terminus_core::machine::{self, LocalMachine};
use terminus_core::models::{Group, Host};
use terminus_core::wsl::{self, WslDistro};
use terminus_core::Store;
use terminus_ui::os_icons::HostStatus;
use terminus_ui::sidebar::{Badge, HostItem, Row, SessionItem};
use uuid::Uuid;

/// Legacy WSL section label (no longer emitted by [`sidebar_rows`]).
pub const WSL_SECTION: &str = "Windows (WSL)";
/// Section label above this computer and WSL distros.
pub const LOCAL_SECTION: &str = "Local";
/// Section label above the stored SSH hosts and groups.
pub const HOSTS_SECTION: &str = "Hosts";
/// The row id of the local machine, resolved by the screen when it opens.
pub const LOCAL_ID: &str = "local";
/// Prefix marking a row as a WSL distro; the rest is the distro's name.
pub const WSL_PREFIX: &str = "wsl:";

/// Default SSH port, applied when the editor's port field is left empty.
pub const DEFAULT_PORT: u16 = 22;

/// Directory holding `terminus.db`.
///
/// `TERMINUS_DATA_DIR` overrides the platform default (mirrors rio's
/// `RIO_CONFIG_HOME` convention) so tests and dev runs can be redirected.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TERMINUS_DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }

    let base = dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("terminus")
}

/// A host as the sidebar needs it — no secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRow {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub group_id: Option<String>,
    pub os_id: Option<String>,
    /// Used to keep newly moved hosts at the end of their group.
    pub updated_at: DateTime<Utc>,
}

impl HostRow {
    fn from_host(host: &Host) -> Self {
        Self {
            id: host.id.to_string(),
            name: host.name.clone(),
            hostname: host.hostname.clone(),
            port: host.port,
            username: host.username.clone(),
            group_id: host.group_id.map(|id| id.to_string()),
            os_id: host.os_id.clone(),
            updated_at: host.updated_at,
        }
    }

    /// `user@host:port`, with the port dropped when it is the SSH default.
    pub fn endpoint(&self) -> String {
        let user = if self.username.is_empty() {
            String::new()
        } else {
            format!("{}@", self.username)
        };
        if self.port == DEFAULT_PORT {
            format!("{}{}", user, self.hostname)
        } else {
            format!("{}{}:{}", user, self.hostname, self.port)
        }
    }
}

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
    let distros = discovery
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

fn session_status(id: &str, open_host_ids: &[String]) -> HostStatus {
    if open_host_ids.iter().any(|open| open == id) {
        HostStatus::Active
    } else {
        HostStatus::Idle
    }
}

fn wsl_host_status(id: &str, open_host_ids: &[String], running: Option<bool>) -> HostStatus {
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

fn host_item_from_row(
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

fn sessions_for_host<'a>(
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

fn push_sessions(rows: &mut Vec<Row>, sessions: &[&OpenSession], host_id: &str) {
    for session in sessions {
        rows.push(Row::Session(SessionItem {
            tab_index: session.tab_index,
            host_id: host_id.to_string(),
            title: session.title.clone(),
            active: session.active,
            closable: session.closable,
        }));
    }
}

/// The sidebar's list, in order:
/// 1. `Local` — this computer + WSL distros (+ their open sessions)
/// 2. `Hosts` — mixed root hosts and groups (sorted by name), always shown
///
/// Empty groups stay in the list so a freshly created group is visible.
pub fn sidebar_rows(
    platform: &PlatformFacts,
    hosts: &[HostRow],
    groups: &[(String, String)],
    collapsed: &HashSet<String>,
    collapsed_hosts: &HashSet<String>,
    open_host_ids: &[String],
    sessions: &[OpenSession],
) -> Vec<Row> {
    let mut rows = Vec::with_capacity(
        hosts.len() + platform.distros.len() + groups.len() + sessions.len() + 4,
    );

    let local_sessions = sessions_for_host(sessions, LOCAL_ID);
    rows.push(Row::Section(LOCAL_SECTION.to_string()));
    rows.push(Row::Host(HostItem {
        id: LOCAL_ID.to_string(),
        name: "This computer".to_string(),
        endpoint: local_subtitle(&platform.machine),
        badge: Badge::Local,
        stored: false,
        os_id: {
            let id = platform.machine.os_id.trim();
            if id.is_empty() || id.eq_ignore_ascii_case("unknown") {
                None
            } else {
                Some(id.to_string())
            }
        },
        status: session_status(LOCAL_ID, open_host_ids),
        nested: false,
        session_count: local_sessions.len(),
    }));
    if !collapsed_hosts.contains(LOCAL_ID) {
        push_sessions(&mut rows, &local_sessions, LOCAL_ID);
    }

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
        if !collapsed_hosts.contains(&id) {
            push_sessions(&mut rows, &distro_sessions, &id);
        }
    }

    rows.push(Row::Section(HOSTS_SECTION.to_string()));

    enum RootItem<'a> {
        Host(&'a HostRow),
        Group(&'a str, &'a str, Vec<&'a HostRow>),
    }

    let mut root: Vec<(String, RootItem<'_>)> = Vec::new();
    for host in hosts.iter().filter(|host| host.group_id.is_none()) {
        root.push((host.name.clone(), RootItem::Host(host)));
    }
    for (group_id, group_name) in groups {
        let mut group_hosts: Vec<_> = hosts
            .iter()
            .filter(|host| host.group_id.as_deref() == Some(group_id.as_str()))
            .collect();
        // Oldest first → a just-moved host (fresh updated_at) lands at the end.
        group_hosts.sort_by(|a, b| {
            a.updated_at
                .cmp(&b.updated_at)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        root.push((
            group_name.clone(),
            RootItem::Group(group_id.as_str(), group_name.as_str(), group_hosts),
        ));
    }
    root.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));

    for (_, item) in root {
        match item {
            RootItem::Host(host) => {
                let host_sessions = sessions_for_host(sessions, &host.id);
                rows.push(Row::Host(host_item_from_row(
                    host,
                    open_host_ids,
                    host_sessions.len(),
                )));
                if !collapsed_hosts.contains(&host.id) {
                    push_sessions(&mut rows, &host_sessions, &host.id);
                }
            }
            RootItem::Group(group_id, group_name, group_hosts) => {
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
                        let host_sessions = sessions_for_host(sessions, &host.id);
                        let mut item =
                            host_item_from_row(host, open_host_ids, host_sessions.len());
                        item.nested = true;
                        rows.push(Row::Host(item));
                        if !collapsed_hosts.contains(&host.id) {
                            push_sessions(&mut rows, &host_sessions, &host.id);
                        }
                    }
                }
            }
        }
    }

    rows
}

/// `nixos@NixOS · WSL`, i.e. who and where.
///
/// The distro name is only spelled out when it is not already the hostname —
/// under WSL they are usually the same string, and saying it twice costs the
/// subtitle its width.
fn local_subtitle(machine: &LocalMachine) -> String {
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
fn distro_subtitle(distro: &WslDistro) -> String {
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

/// What the host editor collected, before it becomes a stored [`Host`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostDraft {
    pub name: String,
    pub hostname: String,
    pub username: String,
    /// Raw text, so an empty field can mean "the default port".
    pub port: String,
}

impl HostDraft {
    /// Validate and normalise into a storable host.
    ///
    /// Empty `name`/`username` fall back to the hostname and `root`; only a
    /// missing hostname or an unparsable port are hard errors.
    pub fn normalize(&self) -> Result<HostDraft, String> {
        let hostname = self.hostname.trim();
        if hostname.is_empty() {
            return Err("Hostname is required".to_string());
        }
        let port = parse_port(&self.port)?;
        let name = match self.name.trim() {
            "" => hostname.to_string(),
            name => name.to_string(),
        };
        let username = match self.username.trim() {
            "" => "root".to_string(),
            user => user.to_string(),
        };

        Ok(HostDraft {
            name,
            hostname: hostname.to_string(),
            username,
            port: port.to_string(),
        })
    }

    /// Port as a number, with [`DEFAULT_PORT`] for an empty field.
    pub fn resolved_port(&self) -> Result<u16, String> {
        parse_port(&self.port)
    }
}

/// Parse the port field: empty means the SSH default.
pub fn parse_port(text: &str) -> Result<u16, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(DEFAULT_PORT);
    }
    text.parse::<u16>()
        .map_err(|_| format!("'{text}' is not a valid port"))
}

/// Build the row the store will write.
fn host_from_draft(draft: &HostDraft) -> Host {
    let now = Utc::now();
    Host {
        id: Uuid::new_v4(),
        name: draft.name.clone(),
        hostname: draft.hostname.clone(),
        port: draft.resolved_port().unwrap_or(DEFAULT_PORT),
        username: draft.username.clone(),
        auth_method: "password".to_string(),
        password: None,
        identity_id: None,
        group_id: None,
        tags: Vec::new(),
        notes: String::new(),
        os_id: None,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

enum Command {
    Refresh,
    Create(HostDraft),
    CreateGroup(String),
    /// Move a stored host into a group (`Some`) or out to the root list (`None`).
    SetHostGroup {
        host_id: String,
        group_id: Option<String>,
    },
}

/// Answers coming back from the worker.
#[derive(Debug)]
enum HostEvent {
    Loaded(Vec<HostRow>),
    GroupsLoaded(Vec<(String, String)>),
    /// This machine and the Windows-side distros. Sent per refresh, after
    /// `Loaded`, and deliberately not counted as an answer to a command:
    /// the list is what a pending command is waiting for.
    Platform(PlatformFacts),
    /// A mutation succeeded; carries the label to report in the sidebar.
    Stored(String),
    Failed(String),
}

/// UI-side handle to the host database.
pub struct HostRepository {
    commands: Sender<Command>,
    events: Receiver<HostEvent>,
    hosts: Vec<HostRow>,
    groups: Vec<(String, String)>,
    platform: PlatformFacts,
    loading: bool,
    /// Commands sent but not yet answered.
    in_flight: usize,
    notice: Option<String>,
    error: Option<String>,
}

impl HostRepository {
    /// Start the worker thread and ask for the first list.
    ///
    /// `wake` runs on the worker thread after every answer — the app passes
    /// a closure that sends the loop a render event, so a completed write
    /// repaints an otherwise idle window.
    pub fn spawn(data_dir: PathBuf, wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        let (command_tx, command_rx) = channel::<Command>();
        let (event_tx, event_rx) = channel::<HostEvent>();

        let thread_dir = data_dir.clone();
        let _ = std::thread::Builder::new()
            .name("terminus-hosts".to_string())
            .spawn(move || worker(thread_dir, command_rx, event_tx, wake));

        let _ = command_tx.send(Command::Refresh);

        Self {
            commands: command_tx,
            events: event_rx,
            hosts: Vec::new(),
            groups: Vec::new(),
            platform: PlatformFacts::default(),
            loading: true,
            in_flight: 1,
            notice: None,
            error: None,
        }
    }

    /// Whether a first load is still outstanding.
    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn hosts(&self) -> &[HostRow] {
        &self.hosts
    }

    pub fn groups(&self) -> &[(String, String)] {
        &self.groups
    }

    /// This machine and the Windows distros, as of the last refresh.
    ///
    /// Empty until the worker's first answer: the panel shows the stored
    /// hosts then, and grows its other two groups a moment later.
    pub fn platform(&self) -> &PlatformFacts {
        &self.platform
    }

    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Last successful-mutation message, cleared once read by the caller.
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// Persist a new host.
    ///
    /// Validation lives here so no caller can store a host without a
    /// hostname or with an unparsable port; the message is both returned (so
    /// an editor can stay open) and kept for the sidebar to display.
    /// Persist a new empty group.
    pub fn create_group(&mut self, name: &str) {
        if self.commands.send(Command::CreateGroup(name.to_string())).is_err() {
            self.error = Some("Host store is unavailable".to_string());
        }
    }

    /// Assign a stored host to a group, or clear membership (`group_id = None`).
    pub fn set_host_group(&mut self, host_id: &str, group_id: Option<&str>) {
        self.send(Command::SetHostGroup {
            host_id: host_id.to_string(),
            group_id: group_id.map(str::to_string),
        });
    }

    pub fn create(&mut self, draft: &HostDraft) -> Result<(), String> {
        match draft.normalize() {
            Ok(normalized) => {
                self.send(Command::Create(normalized));
                Ok(())
            }
            Err(message) => {
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }

    fn send(&mut self, command: Command) {
        if self.commands.send(command).is_ok() {
            self.in_flight += 1;
        } else {
            self.error = Some("Host store is unavailable".to_string());
        }
    }

    /// Apply every answer that has arrived. Returns whether the caller should
    /// repaint.
    ///
    /// Every event counts as a repaint: events are only produced in response
    /// to a command the app itself issued, and the first answer flips `loading`
    /// even when the list comes back empty and unchanged.
    pub fn drain(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.events.try_recv() {
                Ok(HostEvent::Loaded(hosts)) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.loading = false;
                    self.error = None;
                    self.hosts = hosts;
                    changed = true;
                }
                Ok(HostEvent::GroupsLoaded(groups)) => {
                    self.groups = groups;
                    changed = true;
                }
                Ok(HostEvent::Platform(platform)) => {
                    self.platform = platform;
                    changed = true;
                }
                Ok(HostEvent::Stored(label)) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.notice = Some(label);
                    changed = true;
                }
                Ok(HostEvent::Failed(message)) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.loading = false;
                    self.error = Some(message);
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                // Worker gone: stop pretending a load is pending.
                Err(TryRecvError::Disconnected) => {
                    self.in_flight = 0;
                    self.loading = false;
                    break;
                }
            }
        }
        changed
    }
}

/// Worker thread body: own runtime, own pool, sequential commands.
fn worker(
    data_dir: PathBuf,
    commands: Receiver<Command>,
    events: Sender<HostEvent>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) {
    let wake = move || {
        if let Some(wake) = wake.as_ref() {
            wake();
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            let _ = events.send(HostEvent::Failed(format!(
                "Could not start the host store runtime: {err}"
            )));
            wake();
            return;
        }
    };

    let store = match runtime.block_on(Store::open(data_dir)) {
        Ok(store) => store,
        Err(err) => {
            let _ = events.send(HostEvent::Failed(format!(
                "Could not open the host database: {err}"
            )));
            wake();
            return;
        }
    };

    while let Ok(command) = commands.recv() {
        match command {
            Command::Refresh => {
                let _ = events.send(list(&runtime, &store));
                let _ = events.send(list_groups(&runtime, &store));
                // After the hosts, so the panel paints the stored list
                // first and the platform rows follow a moment later.
                let _ = events.send(HostEvent::Platform(discover_platform()));
            }
            Command::Create(draft) => {
                let host = host_from_draft(&draft);
                let label = host.name.clone();
                match runtime.block_on(store.upsert_host(&host)) {
                    Ok(()) => {
                        let _ = events.send(HostEvent::Stored(format!("Added {label}")));
                        let _ = events.send(list(&runtime, &store));
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(format!(
                            "Could not save the host: {err}"
                        )));
                    }
                }
            }
            Command::CreateGroup(name) => {
                let now = Utc::now();
                let group = Group {
                    id: Uuid::new_v4(),
                    name,
                    parent_id: None,
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                };
                match runtime.block_on(store.upsert_group(&group)) {
                    Ok(()) => {
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(format!(
                            "Could not save the group: {err}"
                        )));
                    }
                }
            }
            Command::SetHostGroup { host_id, group_id } => {
                match set_host_group(&runtime, &store, &host_id, group_id.as_deref()) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list(&runtime, &store));
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
        }
        wake();
    }
}

fn list_groups(runtime: &tokio::runtime::Runtime, store: &Store) -> HostEvent {
    match runtime.block_on(store.list_groups()) {
        Ok(groups) => {
            let mut rows: Vec<(String, String)> = groups
                .into_iter()
                .filter(|group| group.deleted_at.is_none())
                .map(|group| (group.id.to_string(), group.name))
                .collect();
            rows.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
            HostEvent::GroupsLoaded(rows)
        }
        Err(err) => HostEvent::Failed(format!("Could not read groups: {err}")),
    }
}

fn list(runtime: &tokio::runtime::Runtime, store: &Store) -> HostEvent {
    match runtime.block_on(store.list_hosts()) {
        Ok(hosts) => {
            let mut rows: Vec<HostRow> = hosts
                .iter()
                .filter(|host| host.deleted_at.is_none())
                .map(HostRow::from_host)
                .collect();
            rows.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.hostname.cmp(&b.hostname))
            });
            HostEvent::Loaded(rows)
        }
        Err(err) => HostEvent::Failed(format!("Could not read hosts: {err}")),
    }
}

fn set_host_group(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    host_id: &str,
    group_id: Option<&str>,
) -> Result<String, String> {
    let id = Uuid::parse_str(host_id).map_err(|_| "Invalid host id".to_string())?;
    let group_uuid = match group_id {
        Some(g) => Some(Uuid::parse_str(g).map_err(|_| "Invalid group id".to_string())?),
        None => None,
    };
    let mut hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|err| format!("Could not read hosts: {err}"))?;
    let host = hosts
        .iter_mut()
        .find(|h| h.id == id && h.deleted_at.is_none())
        .ok_or_else(|| "Host not found".to_string())?;
    host.group_id = group_uuid;
    host.updated_at = Utc::now();
    let label = host.name.clone();
    runtime
        .block_on(store.upsert_host(host))
        .map_err(|err| format!("Could not move the host: {err}"))?;
    Ok(format!("Moved {label}"))
}

/// Where the database file for `dir` lives (used by tests and diagnostics).
pub fn database_path(dir: &Path) -> PathBuf {
    dir.join("terminus.db")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("terminus-hosts-{tag}-{}", Uuid::new_v4()))
    }

    /// Poll `drain` until `done` holds or the deadline passes.
    ///
    /// Draining is not one-event-per-call — `drain` applies everything that
    /// has arrived — so tests wait on observable state rather than counting
    /// events.
    fn drain_until(
        repo: &mut HostRepository,
        timeout: Duration,
        done: impl Fn(&HostRepository) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            repo.drain();
            if done(repo) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn parses_ports_with_default_for_empty() {
        assert_eq!(parse_port(""), Ok(22));
        assert_eq!(parse_port("   "), Ok(22));
        assert_eq!(parse_port("2222"), Ok(2222));
        assert!(parse_port("ssh").is_err());
        assert!(parse_port("70000").is_err());
    }

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

    fn host_row(id: &str, name: &str) -> HostRow {
        HostRow {
            id: id.to_string(),
            name: name.to_string(),
            hostname: format!("{name}.internal"),
            port: 22,
            username: "root".to_string(),
            group_id: None,
            os_id: None,
            updated_at: Utc::now(),
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
        assert_eq!(local.name, "This computer");
        assert_eq!(local.badge, Badge::Local);
        assert_eq!(local.endpoint, "nixos@NixOS · WSL");
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
            vec!["This computer", "Ubuntu 24.04 LTS", "Alpine", "web-01"]
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
    fn open_sessions_attach_under_matching_hosts_with_local_fallback() {
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
        let session_titles: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Session(s) => Some((s.host_id.as_str(), s.tab_index, s.title.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            session_titles,
            vec![("local", 0, "This computer"), ("9c1e", 1, "web-01")]
        );
        let local = rows.iter().find_map(Row::host).unwrap();
        assert_eq!(local.session_count, 1);
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

    #[test]
    fn normalize_fills_defaults_and_rejects_missing_hostname() {
        let draft = HostDraft {
            name: "  ".to_string(),
            hostname: " box.internal ".to_string(),
            username: String::new(),
            port: String::new(),
        }
        .normalize()
        .expect("hostname present");
        assert_eq!(draft.name, "box.internal");
        assert_eq!(draft.username, "root");
        assert_eq!(draft.hostname, "box.internal");
        assert_eq!(draft.resolved_port(), Ok(22));

        assert!(HostDraft::default().normalize().is_err());
        let bad_port = HostDraft {
            hostname: "box".to_string(),
            port: "not-a-port".to_string(),
            ..HostDraft::default()
        };
        assert!(bad_port.normalize().is_err());
    }

    #[test]
    fn endpoint_hides_the_default_port() {
        let mut row = HostRow {
            id: "id".to_string(),
            name: "Box".to_string(),
            hostname: "box.internal".to_string(),
            port: 22,
            username: "root".to_string(),
            group_id: None,
            os_id: None,
            updated_at: Utc::now(),
        };
        assert_eq!(row.endpoint(), "root@box.internal");
        row.port = 2222;
        assert_eq!(row.endpoint(), "root@box.internal:2222");
        row.username = String::new();
        assert_eq!(row.endpoint(), "box.internal:2222");
    }

    #[test]
    fn worker_loads_creates_and_persists_hosts() {
        let dir = temp_dir("roundtrip");
        let mut repo = HostRepository::spawn(dir.clone(), None);

        // First answer is the (empty) initial list.
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            !repo.loading()
        }));
        assert!(repo.is_empty());
        assert_eq!(repo.error(), None);

        repo.create(&HostDraft {
            name: "  ".to_string(),
            hostname: "web-01.example.com".to_string(),
            username: "deploy".to_string(),
            port: "2222".to_string(),
        })
        .expect("valid draft");

        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.len() == 1
        }));
        assert_eq!(repo.error(), None);
        assert_eq!(repo.len(), 1);
        let row = &repo.hosts()[0];
        assert_eq!(row.name, "web-01.example.com");
        assert_eq!(row.hostname, "web-01.example.com");
        assert_eq!(row.username, "deploy");
        assert_eq!(row.port, 2222);
        assert_eq!(row.endpoint(), "deploy@web-01.example.com:2222");
        assert_eq!(repo.take_notice().as_deref(), Some("web-01.example.com"));

        // A second repository over the same directory proves the row is on
        // disk, not just cached in the first connection.
        drop(repo);
        let mut reopened = HostRepository::spawn(dir.clone(), None);
        assert!(drain_until(
            &mut reopened,
            Duration::from_secs(10),
            |repo| { repo.len() == 1 }
        ));
        assert_eq!(reopened.len(), 1);
        assert_eq!(reopened.hosts()[0].hostname, "web-01.example.com");
        drop(reopened);

        assert!(database_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_rejects_an_invalid_draft_without_touching_the_store() {
        let dir = temp_dir("invalid");
        let mut repo = HostRepository::spawn(dir.clone(), None);
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            !repo.loading()
        }));
        assert_eq!(repo.error(), None);

        // No hostname: the editor must keep itself open and show why.
        let message = repo.create(&HostDraft::default()).unwrap_err();
        assert!(message.contains("Hostname"));
        assert_eq!(repo.error(), Some("Hostname is required"));
        assert_eq!(repo.len(), 0);

        // Nothing was queued, so no answer — and no list — ever arrives.
        assert!(!drain_until(
            &mut repo,
            Duration::from_millis(100),
            |repo| { !repo.is_empty() }
        ));
        assert_eq!(repo.error(), Some("Hostname is required"));

        drop(repo);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worker_reports_a_failure_for_a_broken_database_path() {
        // A file where the data directory should be makes `Store::open` fail;
        // the UI must surface that instead of spinning on "loading".
        let dir = temp_dir("broken");
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        std::fs::write(&dir, b"not a directory").unwrap();

        let mut repo = HostRepository::spawn(dir.clone(), None);
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.error().is_some()
        }));
        assert!(!repo.loading());
        assert!(repo.error().is_some());

        let _ = std::fs::remove_file(&dir);
    }
}
