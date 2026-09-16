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

/// Settings key for the persisted [`terminus_core::sync::SyncConfig`] JSON.
const SYNC_CONFIG_SETTING: &str = "sync_config";

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
    /// Manual order among peers (same group / ungrouped). Lower first.
    pub sort_order: i64,
    /// Used as a tie-break after sort_order within a group.
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
            sort_order: host.sort_order,
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

    let mut root: Vec<(i64, String, RootItem<'_>)> = Vec::new();
    for host in hosts.iter().filter(|host| host.group_id.is_none()) {
        root.push((
            host.sort_order,
            host.name.to_lowercase(),
            RootItem::Host(host),
        ));
    }
    for (group_id, group_name, group_order) in groups {
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
        root.push((
            *group_order,
            group_name.to_lowercase(),
            RootItem::Group(group_id.as_str(), group_name.as_str(), group_hosts),
        ));
    }
    root.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    for (_, _, item) in root {
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

    // #region agent log
    {
        let mut order: Vec<String> = Vec::new();
        for row in &rows {
            match row {
                Row::Host(h) if h.stored && !h.nested => {
                    order.push(format!("host:{}", h.name.replace('"', "")));
                }
                Row::Group { name, .. } => {
                    order.push(format!("group:{}", name.replace('"', "")));
                }
                _ => {}
            }
        }
        crate::agent_debug::log(
            "H4",
            "hosts.rs:sidebar_rows",
            "hosts section root order",
            &format!(
                r#"{{"order":[{}],"runId":"post-fix"}}"#,
                order
                    .iter()
                    .map(|s| format!(r#""{s}""#))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        );
    }
    // #endregion

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
    /// `key` | `password` | `gssapi`.
    pub auth_method: String,
    /// Selected identity id when `auth_method == "key"`.
    pub identity_id: Option<String>,
    /// Plaintext password (memory only) when `auth_method == "password"`.
    pub password: String,
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

        let method = match terminus_core::parse_host_auth_method(&self.auth_method) {
            Ok(ok) => ok.method,
            Err(_) => {
                // Empty / unset defaults to key (mock HIG default).
                if self.auth_method.trim().is_empty() {
                    terminus_core::HostAuthMethod::Key
                } else {
                    return Err(format!(
                        "Unknown authentication method '{}'",
                        self.auth_method.trim()
                    ));
                }
            }
        };

        let identity_id = match method {
            terminus_core::HostAuthMethod::Key => {
                let id = self
                    .identity_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                if id.is_none() {
                    return Err(
                        "Select a saved SSH key (Settings → Managed SSH Keys)".to_string(),
                    );
                }
                id
            }
            _ => None,
        };

        let password = match method {
            terminus_core::HostAuthMethod::Password => {
                let pw = self.password.clone();
                if pw.is_empty() {
                    return Err("Password is required".to_string());
                }
                pw
            }
            _ => String::new(),
        };

        Ok(HostDraft {
            name,
            hostname: hostname.to_string(),
            username,
            port: port.to_string(),
            auth_method: method.as_str().to_string(),
            identity_id,
            password,
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

/// Build the row the store will write. Password is never stored on the host
/// row — it is sealed into a credential after a successful probe.
fn host_from_draft(draft: &HostDraft) -> Host {
    let now = Utc::now();
    let identity_id = draft
        .identity_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    Host {
        id: Uuid::new_v4(),
        name: draft.name.clone(),
        hostname: draft.hostname.clone(),
        port: draft.resolved_port().unwrap_or(DEFAULT_PORT),
        username: draft.username.clone(),
        auth_method: draft.auth_method.clone(),
        password: None,
        identity_id,
        group_id: None,
        tags: Vec::new(),
        notes: String::new(),
        os_id: None,
        sort_order: 0,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

enum Command {
    Refresh,
    /// Persist without SSH probe (tests / legacy).
    Create(HostDraft),
    /// Probe SSH, then persist (+ seal password) on success.
    ProbeAndCreate(HostDraft),
    CreateGroup(String),
    /// Move a stored host into a group (`Some`) or out to the root list (`None`).
    SetHostGroup {
        host_id: String,
        group_id: Option<String>,
    },
    /// Place an ungrouped host before another root host or group.
    ReorderHost {
        host_id: String,
        before_host_id: Option<String>,
        before_group_id: Option<String>,
    },
    /// Place a group before another group or root host.
    ReorderGroup {
        group_id: String,
        before_group_id: Option<String>,
        before_host_id: Option<String>,
    },
    /// Unlock or create the vault with a passphrase (Settings).
    UnlockVault(String),
    /// Generate and persist a new Ed25519 managed SSH key.
    CreateSshKey {
        name: String,
        /// When set, import this OpenSSH PEM instead of generating.
        pem: Option<String>,
    },
    /// Soft-delete a managed SSH key.
    DeleteSshKey { id: String },
    /// Soft-delete a stored SSH host.
    DeleteHost { id: String },
    /// Soft-delete a host group (hosts inside become ungrouped).
    DeleteGroup { id: String },
    /// Rename a stored SSH host.
    RenameHost { id: String, name: String },
    /// Rename a host group.
    RenameGroup { id: String, name: String },
    /// Persist remote URI and run SyncEngine::sync_now.
    TestSync {
        uri: String,
    },
}

/// Answers coming back from the worker.
#[derive(Debug)]
enum HostEvent {
    Loaded(Vec<HostRow>),
    GroupsLoaded(Vec<(String, String, i64)>),
    IdentitiesLoaded(Vec<(String, String, String, String)>),
    /// Loaded sync URI + status for the Settings pane.
    SyncStatus {
        uri: String,
        connected: bool,
        vault_unlocked: bool,
        status_line: String,
        is_error: bool,
    },
    /// This machine and the Windows-side distros. Sent per refresh, after
    /// `Loaded`, and deliberately not counted as an answer to a command:
    /// the list is what a pending command is waiting for.
    Platform(PlatformFacts),
    /// A mutation succeeded; carries the label to report in the sidebar.
    Stored(String),
    Failed(String),
    /// Vault unlock/create result for the Settings passphrase field.
    VaultStatus {
        unlocked: bool,
        message: Option<String>,
    },
}

/// UI-side handle to the host database.
pub struct HostRepository {
    commands: Sender<Command>,
    events: Receiver<HostEvent>,
    hosts: Vec<HostRow>,
    groups: Vec<(String, String, i64)>,
    /// `(id, name, fingerprint)` for Settings + add-host picker.
    identities: Vec<(String, String, String, String)>,
    platform: PlatformFacts,
    loading: bool,
    /// Commands sent but not yet answered.
    in_flight: usize,
    notice: Option<String>,
    error: Option<String>,
    /// Last vault unlock status message (Settings).
    vault_message: Option<String>,
    vault_unlocked: bool,
    /// Last known sync URI from the store.
    sync_uri: String,
    sync_connected: bool,
    sync_status_line: String,
    sync_status_is_error: bool,
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
            identities: Vec::new(),
            platform: PlatformFacts::default(),
            loading: true,
            in_flight: 1,
            notice: None,
            error: None,
            vault_message: None,
            vault_unlocked: false,
            sync_uri: String::new(),
            sync_connected: false,
            sync_status_line: "Not configured".into(),
            sync_status_is_error: false,
        }
    }

    /// Whether a first load is still outstanding.
    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn hosts(&self) -> &[HostRow] {
        &self.hosts
    }

    pub fn groups(&self) -> &[(String, String, i64)] {
        &self.groups
    }

    pub fn identities(&self) -> &[(String, String, String, String)] {
        &self.identities
    }

    pub fn vault_unlocked(&self) -> bool {
        self.vault_unlocked
    }

    pub fn take_vault_message(&mut self) -> Option<String> {
        self.vault_message.take()
    }

    pub fn vault_message(&self) -> Option<&str> {
        self.vault_message.as_deref()
    }

    pub fn sync_uri(&self) -> &str {
        &self.sync_uri
    }

    pub fn sync_connected(&self) -> bool {
        self.sync_connected
    }

    pub fn sync_status_line(&self) -> &str {
        &self.sync_status_line
    }

    pub fn sync_status_is_error(&self) -> bool {
        self.sync_status_is_error
    }

    /// Persist URI and run a sync test against SyncEngine.
    pub fn test_sync(&mut self, uri: &str) {
        // Not counted in `in_flight`: SyncStatus is also emitted on Refresh.
        if self
            .commands
            .send(Command::TestSync {
                uri: uri.to_string(),
            })
            .is_err()
        {
            self.error = Some("Host store is unavailable".to_string());
        }
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

    /// Reorder an ungrouped host before a root host or group.
    pub fn reorder_host(
        &mut self,
        host_id: &str,
        before_host_id: Option<&str>,
        before_group_id: Option<&str>,
    ) {
        self.send(Command::ReorderHost {
            host_id: host_id.to_string(),
            before_host_id: before_host_id.map(str::to_string),
            before_group_id: before_group_id.map(str::to_string),
        });
    }

    /// Reorder a group before another group or root host.
    pub fn reorder_group(
        &mut self,
        group_id: &str,
        before_group_id: Option<&str>,
        before_host_id: Option<&str>,
    ) {
        self.send(Command::ReorderGroup {
            group_id: group_id.to_string(),
            before_group_id: before_group_id.map(str::to_string),
            before_host_id: before_host_id.map(str::to_string),
        });
    }

    /// Unlock or create the vault (Settings passphrase).
    pub fn unlock_vault(&mut self, passphrase: &str) {
        self.send(Command::UnlockVault(passphrase.to_string()));
    }

    /// Generate and persist a new Ed25519 managed SSH key.
    pub fn create_ssh_key(&mut self, name: &str) {
        self.create_ssh_key_with_pem(name, None);
    }

    /// Generate, or import `pem` when provided.
    pub fn create_ssh_key_with_pem(&mut self, name: &str, pem: Option<String>) {
        self.send(Command::CreateSshKey {
            name: name.to_string(),
            pem,
        });
    }

    /// Soft-delete a managed SSH key by id.
    pub fn delete_ssh_key(&mut self, id: &str) {
        self.send(Command::DeleteSshKey {
            id: id.to_string(),
        });
    }

    /// Soft-delete a stored SSH host by id.
    pub fn delete_host(&mut self, id: &str) {
        self.send(Command::DeleteHost {
            id: id.to_string(),
        });
    }

    /// Soft-delete a host group by id.
    pub fn delete_group(&mut self, id: &str) {
        self.send(Command::DeleteGroup {
            id: id.to_string(),
        });
    }

    /// Rename a stored SSH host.
    pub fn rename_host(&mut self, id: &str, name: &str) {
        self.send(Command::RenameHost {
            id: id.to_string(),
            name: name.to_string(),
        });
    }

    /// Rename a host group.
    pub fn rename_group(&mut self, id: &str, name: &str) {
        self.send(Command::RenameGroup {
            id: id.to_string(),
            name: name.to_string(),
        });
    }

    /// Persist without probing (tests). Prefer [`Self::probe_and_create`] in UI.
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

    /// Probe SSH with the draft credentials, then persist on success.
    pub fn probe_and_create(&mut self, draft: &HostDraft) -> Result<(), String> {
        match draft.normalize() {
            Ok(normalized) => {
                self.send(Command::ProbeAndCreate(normalized));
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
                Ok(HostEvent::IdentitiesLoaded(identities)) => {
                    self.identities = identities;
                    changed = true;
                }
                Ok(HostEvent::SyncStatus {
                    uri,
                    connected,
                    vault_unlocked,
                    status_line,
                    is_error,
                }) => {
                    self.sync_uri = uri;
                    self.sync_connected = connected;
                    self.vault_unlocked = vault_unlocked;
                    self.sync_status_line = status_line;
                    self.sync_status_is_error = is_error;
                    changed = true;
                }
                Ok(HostEvent::Platform(platform)) => {
                    self.platform = platform;
                    changed = true;
                }
                Ok(HostEvent::Stored(label)) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.notice = Some(label);
                    self.error = None;
                    changed = true;
                }
                Ok(HostEvent::Failed(message)) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.loading = false;
                    self.error = Some(message);
                    changed = true;
                }
                Ok(HostEvent::VaultStatus { unlocked, message }) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    self.vault_unlocked = unlocked;
                    if let Some(msg) = message {
                        // Surface vault unlock/create results on the SqlSync
                        // status row (not the host-list notice band).
                        self.sync_status_line = msg.clone();
                        self.sync_status_is_error = !unlocked;
                        self.vault_message = Some(msg);
                    }
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
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

    let store = match runtime.block_on(Store::open(data_dir.clone())) {
        Ok(store) => store,
        Err(err) => {
            let _ = events.send(HostEvent::Failed(format!(
                "Could not open the host database: {err}"
            )));
            wake();
            return;
        }
    };

    let mut vault: Option<Arc<terminus_core::UnlockedVault>> = None;
    let mut sync_engine =
        terminus_core::SyncEngine::new(terminus_core::sync::SyncConfig::default());

    // Restore persisted sync config + detect vault header.
    if let Ok(Some(raw)) = runtime.block_on(store.get_setting(SYNC_CONFIG_SETTING)) {
        if let Ok(cfg) = terminus_core::sync::SyncConfig::from_json(&raw) {
            let url = cfg.remote_url.clone();
            sync_engine = terminus_core::SyncEngine::new(cfg);
            if let Some(uri) = url.filter(|u| !u.trim().is_empty()) {
                if let Err(err) = runtime.block_on(sync_engine.attach_remote_uri(&uri)) {
                    tracing::warn!(%err, "could not reopen sync remote on startup");
                }
            }
        }
    }
    if let Ok(Some(raw)) =
        runtime.block_on(store.get_setting(terminus_core::VAULT_HEADER_SETTING))
    {
        if terminus_core::parse_vault_header(&raw).is_ok() {
            let _ = events.send(HostEvent::VaultStatus {
                unlocked: false,
                message: None,
            });
        }
    }

    while let Ok(command) = commands.recv() {
        match command {
            Command::Refresh => {
                let _ = events.send(list(&runtime, &store));
                let _ = events.send(list_groups(&runtime, &store));
                let _ = events.send(list_identities(&runtime, &store));
                let _ = events.send(sync_status_event(
                    &runtime,
                    &sync_engine,
                    vault.is_some(),
                ));
                let _ = events.send(HostEvent::Platform(discover_platform()));
            }
            Command::Create(draft) => {
                // Test / skip-probe path: no SSH round-trip.
                let mut host = host_from_draft(&draft);
                match runtime.block_on(store.next_host_sort_order(None)) {
                    Ok(order) => host.sort_order = order,
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(format!(
                            "Could not save the host: {err}"
                        )));
                        continue;
                    }
                }
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
            Command::ProbeAndCreate(draft) => {
                match probe_and_persist(&runtime, &store, &mut vault, draft) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(format!("Added {label}")));
                        let _ = events.send(list(&runtime, &store));
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(message) => {
                        let _ = events.send(HostEvent::Failed(message));
                    }
                }
            }
            Command::CreateGroup(name) => {
                let now = Utc::now();
                let sort_order = match runtime.block_on(store.next_group_sort_order()) {
                    Ok(order) => order,
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(format!(
                            "Could not save the group: {err}"
                        )));
                        continue;
                    }
                };
                let group = Group {
                    id: Uuid::new_v4(),
                    name,
                    parent_id: None,
                    sort_order,
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
            Command::ReorderHost {
                host_id,
                before_host_id,
                before_group_id,
            } => {
                match reorder_host(
                    &runtime,
                    &store,
                    &host_id,
                    before_host_id.as_deref(),
                    before_group_id.as_deref(),
                ) {
                    Ok(label) => {
                        // #region agent log
                        crate::agent_debug::log(
                            "H2",
                            "hosts.rs:ReorderHost",
                            "reorder persisted",
                            &format!(
                                r#"{{"hostId":"{}","beforeHost":"{}","beforeGroup":"{}","runId":"post-fix"}}"#,
                                host_id.replace('"', ""),
                                before_host_id.as_deref().unwrap_or("").replace('"', ""),
                                before_group_id.as_deref().unwrap_or("").replace('"', "")
                            ),
                        );
                        // #endregion
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list(&runtime, &store));
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::ReorderGroup {
                group_id,
                before_group_id,
                before_host_id,
            } => {
                match reorder_group(
                    &runtime,
                    &store,
                    &group_id,
                    before_group_id.as_deref(),
                    before_host_id.as_deref(),
                ) {
                    Ok(label) => {
                        // #region agent log
                        crate::agent_debug::log(
                            "H3",
                            "hosts.rs:ReorderGroup",
                            "group reorder persisted",
                            &format!(
                                r#"{{"groupId":"{}","beforeGroup":"{}","beforeHost":"{}","runId":"post-fix"}}"#,
                                group_id.replace('"', ""),
                                before_group_id.as_deref().unwrap_or("").replace('"', ""),
                                before_host_id.as_deref().unwrap_or("").replace('"', "")
                            ),
                        );
                        // #endregion
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list_groups(&runtime, &store));
                        let _ = events.send(list(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::UnlockVault(passphrase) => {
                match unlock_or_create_vault(&runtime, &store, &passphrase) {
                    Ok(unlocked) => {
                        let shared = Arc::new(unlocked);
                        runtime.block_on(sync_engine.attach_vault(Arc::clone(&shared)));
                        vault = Some(shared);
                        let _ = events.send(HostEvent::VaultStatus {
                            unlocked: true,
                            message: Some("Vault unlocked".into()),
                        });
                        let _ = events.send(sync_status_event(
                            &runtime,
                            &sync_engine,
                            true,
                        ));
                    }
                    Err(message) => {
                        let _ = events.send(HostEvent::VaultStatus {
                            unlocked: false,
                            message: Some(message),
                        });
                    }
                }
            }
            Command::CreateSshKey { name, pem } => {
                match create_ssh_key(&runtime, &store, &name, pem.as_deref()) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list_identities(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::DeleteSshKey { id } => {
                match delete_ssh_key(&runtime, &store, &id) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list_identities(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::DeleteHost { id } => {
                // #region agent log
                crate::agent_debug::log(
                    "H4",
                    "hosts.rs:DeleteHost",
                    "worker received DeleteHost",
                    &format!(r#"{{"id":"{}"}}"#, id.replace('"', "")),
                );
                // #endregion
                match delete_host(&runtime, &store, &id) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::DeleteGroup { id } => {
                // #region agent log
                crate::agent_debug::log(
                    "H4",
                    "hosts.rs:DeleteGroup",
                    "worker received DeleteGroup",
                    &format!(r#"{{"id":"{}"}}"#, id.replace('"', "")),
                );
                // #endregion
                match delete_group(&runtime, &store, &id) {
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
            Command::RenameHost { id, name } => {
                match rename_host(&runtime, &store, &id, &name) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::RenameGroup { id, name } => {
                match rename_group(&runtime, &store, &id, &name) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list_groups(&runtime, &store));
                        let _ = events.send(list(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::TestSync { uri } => {
                let status = runtime.block_on(run_test_sync(
                    &store,
                    &mut sync_engine,
                    vault.as_ref().map(Arc::clone),
                    &uri,
                ));
                let _ = events.send(status);
            }
        }
        wake();
    }
}

fn unlock_or_create_vault(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    passphrase: &str,
) -> Result<terminus_core::UnlockedVault, String> {
    if passphrase.trim().len() < 8 {
        return Err("Vault passphrase must be at least 8 characters".into());
    }
    let existing = runtime
        .block_on(store.get_setting(terminus_core::VAULT_HEADER_SETTING))
        .map_err(|e| e.to_string())?;
    if let Some(raw) = existing {
        let header = terminus_core::parse_vault_header(&raw)
            .map_err(|e| format!("Corrupt vault header: {e}"))?;
        terminus_core::UnlockedVault::unlock(passphrase, &header).map_err(|_| {
            "vault unlock failed".to_string()
        })
    } else {
        let (header, vault) =
            terminus_core::create_with_key(passphrase).map_err(|e| e.to_string())?;
        let json = terminus_core::encode_vault_header(&header).map_err(|e| e.to_string())?;
        runtime
            .block_on(store.set_setting(terminus_core::VAULT_HEADER_SETTING, &json))
            .map_err(|e| e.to_string())?;
        Ok(vault)
    }
}

fn probe_and_persist(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    vault: &mut Option<Arc<terminus_core::UnlockedVault>>,
    draft: HostDraft,
) -> Result<String, String> {
    use terminus_core::{
        probe_options_from_host, probe_ssh_auth, seal_host_password, HostAuthMethod,
    };

    let method = terminus_core::parse_host_auth_method(&draft.auth_method)
        .map(|ok| ok.method)
        .map_err(|e| format!("Unknown authentication method '{}'", e.raw))?;

    if method == HostAuthMethod::Password && vault.is_none() {
        return Err(
            "Unlock the vault before saving a password (Settings → Remote SQL Sync passphrase)"
                .into(),
        );
    }

    let mut host = host_from_draft(&draft);
    host.sort_order = runtime
        .block_on(store.next_host_sort_order(None))
        .map_err(|e| format!("Could not save the host: {e}"))?;
    // Probe uses in-memory password when present.
    if method == HostAuthMethod::Password {
        host.password = Some(draft.password.clone());
    }

    let identity = match method {
        HostAuthMethod::Key => {
            let id = host
                .identity_id
                .ok_or_else(|| "Select a saved SSH key".to_string())?;
            Some(
                runtime
                    .block_on(store.get_identity(id))
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "Selected SSH key was not found".to_string())?,
            )
        }
        _ => None,
    };

    let opts = probe_options_from_host(&host, identity.as_ref());
    if let Err(err) = runtime.block_on(probe_ssh_auth(&opts)) {
        return Err(err.user_message());
    }

    // Never persist plaintext password on the host row.
    host.password = None;
    host.updated_at = Utc::now();

    runtime
        .block_on(store.upsert_host(&host))
        .map_err(|e| format!("Could not save the host: {e}"))?;

    if method == HostAuthMethod::Password {
        let unlocked = vault
            .as_ref()
            .ok_or_else(|| "Unlock the vault before saving a password".to_string())?;
        let cred = seal_host_password(unlocked.as_ref(), host.id, &draft.password)
            .map_err(|e| format!("Could not encrypt the password: {e}"))?;
        runtime
            .block_on(store.upsert_credential(&cred))
            .map_err(|e| format!("Could not store the encrypted password: {e}"))?;
    }

    Ok(host.name)
}

/// Snapshot the SyncEngine into a UI event.
fn sync_status_event(
    runtime: &tokio::runtime::Runtime,
    engine: &terminus_core::SyncEngine,
    vault_unlocked: bool,
) -> HostEvent {
    runtime.block_on(async {
        let status = engine.status().await;
        let last_error = engine.last_error().await;
        let last_sync = engine.last_sync().await;
        let configured = engine.is_configured().await;
        let uri = engine.config.remote_url.clone().unwrap_or_default();

        let connected = configured
            && matches!(
                status,
                terminus_core::sync::SyncStatus::Idle | terminus_core::sync::SyncStatus::Syncing
            );

        let (status_line, is_error) = if let Some(err) = last_error {
            (err, true)
        } else if let Some(ts) = last_sync {
            (
                format!("Last synced {}", ts.format("%Y-%m-%d %H:%M UTC")),
                false,
            )
        } else if !engine.config.has_remote() {
            ("Not configured".to_string(), false)
        } else if !configured {
            (
                format!("Remote set but not attached ({})", status.as_str()),
                false,
            )
        } else {
            (format!("Ready ({})", status.as_str()), false)
        };

        HostEvent::SyncStatus {
            uri,
            connected,
            vault_unlocked,
            status_line,
            is_error,
        }
    })
}

async fn run_test_sync(
    store: &Store,
    engine: &mut terminus_core::SyncEngine,
    vault: Option<Arc<terminus_core::UnlockedVault>>,
    uri: &str,
) -> HostEvent {
    let uri = uri.trim().to_string();
    let vault_unlocked = vault.is_some();

    if let Some(v) = vault {
        engine.attach_vault(v).await;
    }

    if uri.is_empty() {
        // Test Sync with an empty field is a validation error — do not clear
        // a previously configured remote, and do not return a non-error
        // "Not configured" that silently dismisses the error banner.
        let connected = engine.is_configured().await
            && matches!(
                engine.status().await,
                terminus_core::sync::SyncStatus::Idle
                    | terminus_core::sync::SyncStatus::Syncing
            );
        return HostEvent::SyncStatus {
            uri: engine.config.remote_url.clone().unwrap_or_default(),
            connected,
            vault_unlocked,
            status_line: "Enter a connection URI before testing sync".into(),
            is_error: true,
        };
    }

    // Preserve device_id across reconfiguration.
    let device_id = engine.config.device_id.clone();
    engine.config = terminus_core::sync::SyncConfig {
        enabled: true,
        remote_url: Some(uri.clone()),
        device_id,
        interval_secs: engine.config.interval_secs,
        sync_secrets: engine.config.sync_secrets,
        last_sync: engine.config.last_sync,
    };

    if let Err(err) = persist_sync_config(store, &engine.config).await {
        return HostEvent::SyncStatus {
            uri: uri.clone(),
            connected: false,
            vault_unlocked,
            status_line: err,
            is_error: true,
        };
    }

    if let Err(err) = engine.attach_remote_uri(&uri).await {
        engine.clear_remote().await;
        let _ = engine.mark_error(err.to_string()).await;
        return HostEvent::SyncStatus {
            uri: uri.clone(),
            connected: false,
            vault_unlocked,
            status_line: err.to_string(),
            is_error: true,
        };
    }

    // Recover from a previous Error state so sync_now can run.
    if engine.status().await == terminus_core::sync::SyncStatus::Error {
        engine.clear_error().await;
        let _ = engine
            .transition_to(terminus_core::sync::SyncStatus::Idle)
            .await;
    }

    let (status_line, is_error) = match engine.sync_now().await {
        Ok(report) => {
            engine.config.last_sync = report.finished_at;
            let _ = persist_sync_config(store, &engine.config).await;
            (
                format!(
                    "Sync ok — pushed {}, pulled {}",
                    report.pushed, report.pulled
                ),
                false,
            )
        }
        Err(err) => (err.to_string(), true),
    };

    let connected = engine.is_configured().await
        && matches!(
            engine.status().await,
            terminus_core::sync::SyncStatus::Idle | terminus_core::sync::SyncStatus::Syncing
        );

    HostEvent::SyncStatus {
        uri,
        connected,
        vault_unlocked,
        status_line,
        is_error,
    }
}

async fn persist_sync_config(
    store: &Store,
    config: &terminus_core::sync::SyncConfig,
) -> Result<(), String> {
    let json = config.to_json().map_err(|e| e.to_string())?;
    store
        .set_setting(SYNC_CONFIG_SETTING, &json)
        .await
        .map_err(|e| e.to_string())
}

fn list_identities(runtime: &tokio::runtime::Runtime, store: &Store) -> HostEvent {
    match runtime.block_on(store.list_identities()) {
        Ok(idents) => {
            let rows: Vec<(String, String, String, String)> = idents
                .into_iter()
                .filter(|i| i.deleted_at.is_none())
                .map(|i| {
                    let fp = i
                        .public_key
                        .as_deref()
                        .map(terminus_core::fingerprint_from_public_openssh)
                        .unwrap_or_else(|| "no public key".into());
                    let created = i.created_at.format("%Y-%m-%d").to_string();
                    (i.id.to_string(), i.name, fp, created)
                })
                .collect();
            HostEvent::IdentitiesLoaded(rows)
        }
        Err(err) => HostEvent::Failed(format!("Could not read identities: {err}")),
    }
}

fn create_ssh_key(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    name: &str,
    pem: Option<&str>,
) -> Result<String, String> {
    let identity = match pem.map(str::trim).filter(|p| !p.is_empty()) {
        Some(pem) => {
            terminus_core::import_openssh_identity(name, pem, None).map_err(|e| e.to_string())?
        }
        None => {
            terminus_core::generate_ed25519_identity(name).map_err(|e| e.to_string())?
        }
    };
    let label = identity.name.clone();
    let imported = pem.map(str::trim).is_some_and(|p| !p.is_empty());
    runtime
        .block_on(store.upsert_identity(&identity))
        .map_err(|e| e.to_string())?;
    Ok(if imported {
        format!("SSH key “{label}” imported")
    } else {
        format!("SSH key “{label}” created")
    })
}

fn delete_ssh_key(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
) -> Result<String, String> {
    let uuid = uuid::Uuid::parse_str(id).map_err(|_| "Invalid SSH key id".to_string())?;
    let existing = runtime
        .block_on(store.get_identity(uuid))
        .map_err(|e| e.to_string())?;
    let Some(ident) = existing else {
        return Err("SSH key not found".into());
    };
    let label = ident.name.clone();
    runtime
        .block_on(store.delete_identity(uuid))
        .map_err(|e| e.to_string())?;
    Ok(format!("SSH key “{label}” deleted"))
}

fn delete_host(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
) -> Result<String, String> {
    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid host id".to_string())?;
    let hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|err| format!("Could not read hosts: {err}"))?;
    let label = hosts
        .iter()
        .find(|h| h.id == uuid && h.deleted_at.is_none())
        .map(|h| h.name.clone())
        .ok_or_else(|| "Host not found".to_string())?;
    runtime
        .block_on(store.delete_host(uuid))
        .map_err(|err| format!("Could not delete the host: {err}"))?;
    Ok(format!("Deleted {label}"))
}

fn delete_group(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
) -> Result<String, String> {
    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid group id".to_string())?;
    let groups = runtime
        .block_on(store.list_groups())
        .map_err(|err| format!("Could not read groups: {err}"))?;
    let label = groups
        .iter()
        .find(|g| g.id == uuid && g.deleted_at.is_none())
        .map(|g| g.name.clone())
        .ok_or_else(|| "Group not found".to_string())?;
    runtime
        .block_on(store.delete_group(uuid))
        .map_err(|err| format!("Could not delete the group: {err}"))?;
    Ok(format!("Deleted group {label}"))
}

fn rename_host(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
    name: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid host id".to_string())?;
    let mut hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|err| format!("Could not read hosts: {err}"))?;
    let host = hosts
        .iter_mut()
        .find(|h| h.id == uuid && h.deleted_at.is_none())
        .ok_or_else(|| "Host not found".to_string())?;
    host.name = name.to_string();
    host.updated_at = Utc::now();
    let label = host.name.clone();
    runtime
        .block_on(store.upsert_host(host))
        .map_err(|err| format!("Could not rename the host: {err}"))?;
    Ok(format!("Renamed {label}"))
}

fn rename_group(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
    name: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid group id".to_string())?;
    let mut groups = runtime
        .block_on(store.list_groups())
        .map_err(|err| format!("Could not read groups: {err}"))?;
    let group = groups
        .iter_mut()
        .find(|g| g.id == uuid && g.deleted_at.is_none())
        .ok_or_else(|| "Group not found".to_string())?;
    group.name = name.to_string();
    group.updated_at = Utc::now();
    let label = group.name.clone();
    runtime
        .block_on(store.upsert_group(group))
        .map_err(|err| format!("Could not rename the group: {err}"))?;
    Ok(format!("Renamed group {label}"))
}

fn list_groups(runtime: &tokio::runtime::Runtime, store: &Store) -> HostEvent {
    match runtime.block_on(store.list_groups()) {
        Ok(groups) => {
            let mut rows: Vec<(i64, String, String)> = groups
                .into_iter()
                .filter(|group| group.deleted_at.is_none())
                .map(|group| (group.sort_order, group.id.to_string(), group.name))
                .collect();
            rows.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| a.2.to_lowercase().cmp(&b.2.to_lowercase()))
            });
            HostEvent::GroupsLoaded(
                rows.into_iter()
                    .map(|(order, id, name)| (id, name, order))
                    .collect(),
            )
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
    host.sort_order = runtime
        .block_on(store.next_host_sort_order(group_uuid))
        .map_err(|err| format!("Could not move the host: {err}"))?;
    host.updated_at = Utc::now();
    let label = host.name.clone();
    runtime
        .block_on(store.upsert_host(host))
        .map_err(|err| format!("Could not move the host: {err}"))?;
    Ok(format!("Moved {label}"))
}

fn reorder_host(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    host_id: &str,
    before_host_id: Option<&str>,
    before_group_id: Option<&str>,
) -> Result<String, String> {
    let id = Uuid::parse_str(host_id).map_err(|_| "Invalid host id".to_string())?;
    let (before_is_group, before_id) = if let Some(b) = before_group_id {
        (
            Some(true),
            Some(Uuid::parse_str(b).map_err(|_| "Invalid group id".to_string())?),
        )
    } else if let Some(b) = before_host_id {
        (
            Some(false),
            Some(Uuid::parse_str(b).map_err(|_| "Invalid host id".to_string())?),
        )
    } else {
        (None, None)
    };
    runtime
        .block_on(store.reorder_root(false, id, before_is_group, before_id))
        .map_err(|err| format!("Could not reorder the host: {err}"))?;
    Ok("Reordered".to_string())
}

fn reorder_group(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    group_id: &str,
    before_group_id: Option<&str>,
    before_host_id: Option<&str>,
) -> Result<String, String> {
    let id = Uuid::parse_str(group_id).map_err(|_| "Invalid group id".to_string())?;
    let (before_is_group, before_id) = if let Some(b) = before_host_id {
        (
            Some(false),
            Some(Uuid::parse_str(b).map_err(|_| "Invalid host id".to_string())?),
        )
    } else if let Some(b) = before_group_id {
        (
            Some(true),
            Some(Uuid::parse_str(b).map_err(|_| "Invalid group id".to_string())?),
        )
    } else {
        (None, None)
    };
    runtime
        .block_on(store.reorder_root(true, id, before_is_group, before_id))
        .map_err(|err| format!("Could not reorder the group: {err}"))?;
    Ok("Reordered group".to_string())
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
            sort_order: 0,
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
    fn ungrouped_hosts_come_before_groups() {
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
        assert_eq!(root, vec!["mainserver", "jeremy", "test"]);
    }

    #[test]
    fn group_can_sort_above_host_via_sort_order() {
        let platform = PlatformFacts {
            machine: machine("NixOS", "nixos", Some("NixOS")),
            distros: Vec::new(),
            unnamed_distros: 0,
        };
        let mut host = host_row("h1", "mainserver");
        host.sort_order = 1;
        let hosts = vec![host];
        let groups = vec![("g1".into(), "jeremy".into(), 0)];
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
        assert_eq!(root, vec!["jeremy", "mainserver"]);
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
            sort_order: 0,
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
            auth_method: "gssapi".to_string(),
            ..HostDraft::default()
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
        assert_eq!(
            repo.take_notice().as_deref(),
            Some("Added web-01.example.com")
        );

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

    #[test]
    fn worker_test_sync_opens_sqlite_and_rejects_postgres() {
        let dir = temp_dir("sync");
        let mut repo = HostRepository::spawn(dir.clone(), None);
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            !repo.loading()
        }));

        // Wait for the initial SyncStatus from Refresh.
        let _ = drain_until(&mut repo, Duration::from_secs(2), |repo| {
            !repo.sync_status_line().is_empty()
        });

        repo.test_sync("postgres://localhost/terminus");
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.sync_status_line().contains("PostgreSQL")
        }));
        assert!(!repo.sync_connected());
        assert!(
            repo.sync_status_is_error(),
            "postgres reject must flag sync_status_is_error"
        );

        // Empty URI must surface as an error banner, not silently clear state.
        repo.test_sync("");
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.sync_status_line().contains("Enter a connection")
        }));
        assert!(
            repo.sync_status_is_error(),
            "empty URI Test Sync must flag an error"
        );
        // Previous postgres URI should still be reported (remote not cleared).
        assert!(repo.sync_uri().contains("postgres"));

        let remote = dir.join("remote.db");
        let uri = format!("sqlite:{}", remote.display());
        repo.test_sync(&uri);
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.sync_connected() && repo.sync_status_line().contains("Sync ok")
        }));
        assert_eq!(repo.sync_uri(), uri);
        assert!(remote.exists());

        // Persist: reopen and see the URI restored.
        drop(repo);
        let mut reopened = HostRepository::spawn(dir.clone(), None);
        assert!(drain_until(&mut reopened, Duration::from_secs(10), |repo| {
            !repo.loading() && repo.sync_uri() == uri
        }));
        assert!(reopened.sync_connected());

        drop(reopened);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worker_create_and_delete_managed_ssh_key() {
        let dir = temp_dir("ssh-key");
        let mut repo = HostRepository::spawn(dir.clone(), None);
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            !repo.loading()
        }));

        repo.create_ssh_key("Laptop Ed25519");
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.identities().iter().any(|(id, name, fp, created)| {
                !id.is_empty()
                    && name == "Laptop Ed25519"
                    && fp.starts_with("SHA256:")
                    && !created.is_empty()
            })
        }));
        assert!(repo
            .take_notice()
            .is_some_and(|n| n.contains("Laptop Ed25519")));

        let id = repo.identities()[0].0.clone();
        repo.delete_ssh_key(&id);
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.identities().is_empty()
        }));

        // Import path: generate a PEM then re-import under a new label.
        let generated = terminus_core::generate_ed25519_identity("tmp").expect("pem");
        let pem = generated.private_key.expect("private");
        repo.create_ssh_key_with_pem("Imported", Some(pem));
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.identities()
                .iter()
                .any(|(_, name, fp, _)| name == "Imported" && fp.starts_with("SHA256:"))
        }));

        drop(repo);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worker_unlock_vault_from_passphrase() {
        let dir = temp_dir("vault");
        let mut repo = HostRepository::spawn(dir.clone(), None);
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            !repo.loading()
        }));

        repo.unlock_vault("short");
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.sync_status_line().contains("at least 8")
        }));
        assert!(!repo.vault_unlocked());
        assert!(repo.sync_status_is_error());
        let _ = repo.take_vault_message();

        repo.unlock_vault("long-enough-passphrase");
        assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
            repo.vault_unlocked()
        }));
        assert_eq!(
            repo.take_vault_message().as_deref(),
            Some("Vault unlocked")
        );

        drop(repo);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
