//! Tunnels backend: ssh command building, process registry, persistence.
//!
//! Tunnels run with the system OpenSSH (`ssh -N -L/-R/-D …`), exactly like the
//! interactive host tabs (decision "Option A", see `milestone.md`): the caller
//! builds the base `ssh` arguments with the same builder the shells use
//! (`screen::shell::ssh_shell`, so user/host/port/identity and
//! `StrictHostKeyChecking=accept-new` are identical) and this module only adds
//! the forwarding flags.
//!
//! * [`forward_args`] / [`tunnel_ssh_args`] / [`friendly_error`] are pure.
//! * [`TunnelRegistry`] owns the child processes: start, stop, exit
//!   detection, kill-on-drop.
//! * [`TunnelController`] ties the view state
//!   ([`terminus_ui::views::tunnels::TunnelsState`]), the persistence worker
//!   (SQLite on its own thread, like `hosts.rs`) and the registry.


use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use terminus_core::models::PortForward;
use terminus_core::Store;
use terminus_ui::geom::Rect;
use terminus_ui::views::tunnels::{
    FormKey, TunnelAction, TunnelDraft, TunnelItem, TunnelKind, TunnelStatus,
    TunnelsState,
};
use uuid::Uuid;

/// How long a tunnel must survive before it counts as established. With
/// `ExitOnForwardFailure=yes` a bad forward or a refused login ends the
/// process within the connect timeout, so "still alive after a moment" is the
/// only success signal `ssh -N` gives.
pub const DEFAULT_SETTLE: Duration = Duration::from_millis(2500);

// ---------------------------------------------------------------- commands

/// The `-L`/`-R`/`-D` flag pair for a tunnel.
pub fn forward_args(item: &TunnelItem) -> Vec<String> {
    match item.kind {
        TunnelKind::Local => vec![
            "-L".into(),
            format!(
                "{}:{}:{}:{}",
                item.bind_host, item.bind_port, item.dest_host, item.dest_port
            ),
        ],
        TunnelKind::Remote => vec![
            "-R".into(),
            format!(
                "{}:{}:{}:{}",
                item.bind_host, item.bind_port, item.dest_host, item.dest_port
            ),
        ],
        TunnelKind::Dynamic => {
            vec![
                "-D".into(),
                format!("{}:{}", item.bind_host, item.bind_port),
            ]
        }
    }
}

/// Full `ssh` argument list for a tunnel.
///
/// `base` is what the interactive shell would run (options, then the
/// destination as the last element). `has_askpass` says the base command
/// authenticates through `SSH_ASKPASS` (password hosts, passphrase keys): only
/// then may ssh ask anything. In every other case `BatchMode=yes` makes a login
/// that would need a prompt fail immediately with "Permission denied" instead
/// of hanging on a terminal nobody sees.
pub fn tunnel_ssh_args(
    base: &[String],
    has_askpass: bool,
    item: &TunnelItem,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-N".into(),
        "-T".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        "ConnectTimeout=15".into(),
        "-o".into(),
        "ServerAliveInterval=30".into(),
        "-o".into(),
        "ServerAliveCountMax=3".into(),
    ];
    if !has_askpass {
        args.extend(["-o".into(), "BatchMode=yes".into()]);
    }
    args.extend(forward_args(item));
    args.extend(base.iter().cloned());
    args
}

/// Turn what ssh printed before dying into one short sentence.
pub fn friendly_error(stderr: &str, code: Option<i32>) -> String {
    let lower = stderr.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if has(&[
        "address already in use",
        "cannot listen to port",
        "cannot assign requested address",
    ]) {
        return "Port is already in use".into();
    }
    if has(&["remote port forwarding failed"]) {
        return "The server refused the remote port forward".into();
    }
    if has(&["permission denied", "too many authentication failures"]) {
        return "Authentication failed \u{2014} check the key or the saved password"
            .into();
    }
    if has(&["host key verification failed", "identification has changed"]) {
        return "Host key not trusted \u{2014} connect once from a terminal tab".into();
    }
    if has(&[
        "connection refused",
        "timed out",
        "could not resolve",
        "no route to host",
        "network is unreachable",
        "connection closed by",
    ]) {
        return "Could not reach the server".into();
    }
    if has(&[
        "administratively prohibited",
        "open failed",
        "forwarding is disabled",
    ]) {
        return "The server does not allow this forward".into();
    }
    if let Some(line) = stderr.lines().map(str::trim).rev().find(|l| !l.is_empty()) {
        let mut s: String = line.chars().take(140).collect();
        if line.chars().count() > 140 {
            s.push('\u{2026}');
        }
        return s;
    }
    match code {
        Some(c) => format!("ssh exited with code {c}"),
        None => "ssh was terminated".into(),
    }
}

/// True when `127.0.0.1:port` can be bound right now.
pub fn local_port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

// ---------------------------------------------------------------- registry

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelEvent {
    /// The process survived the settle delay.
    Running(String),
    /// The process ended without being asked to.
    Exited { id: String, message: String },
}

struct Proc {
    child: Child,
    started: Instant,
    stderr: Arc<Mutex<String>>,
    reader: Option<std::thread::JoinHandle<()>>,
    reported: bool,
}

/// Child `ssh` processes keyed by tunnel id. Dropping the registry kills
/// them all.
pub struct TunnelRegistry {
    procs: HashMap<String, Proc>,
    settle: Duration,
}

impl TunnelRegistry {
    pub fn new(settle: Duration) -> Self {
        Self {
            procs: HashMap::new(),
            settle,
        }
    }

    /// Spawn `cmd` for tunnel `id`, replacing any process already there.
    pub fn start(&mut self, id: &str, mut cmd: Command) -> Result<(), String> {
        self.stop(id);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        // A killed app (SIGTERM/SIGKILL never run Drop) must not leave
        // `ssh -N` holding its ports: the child gets SIGTERM when the
        // thread that started it (the UI thread) goes away.
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            // SAFETY: prctl is async-signal-safe; nothing else runs here.
            unsafe {
                cmd.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Could not start ssh: {e}"))?;
        let stderr = Arc::new(Mutex::new(String::new()));
        let reader = child.stderr.take().map(|mut pipe| {
            let sink = stderr.clone();
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = [0u8; 1024];
                while let Ok(n) = pipe.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut s) = sink.lock() {
                        if s.len() < 8192 {
                            s.push_str(&String::from_utf8_lossy(&buf[..n]));
                        }
                    }
                }
            })
        });
        self.procs.insert(
            id.to_string(),
            Proc {
                child,
                started: Instant::now(),
                stderr,
                reader,
                reported: false,
            },
        );
        Ok(())
    }

    /// Kill and reap the process of `id`; false when there was none.
    pub fn stop(&mut self, id: &str) -> bool {
        match self.procs.remove(id) {
            Some(mut p) => {
                let _ = p.child.kill();
                let _ = p.child.wait();
                true
            }
            None => false,
        }
    }

    pub fn stop_all(&mut self) {
        let ids: Vec<String> = self.procs.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.procs.contains_key(id)
    }

    #[cfg(test)]
    pub fn active_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.procs.keys().cloned().collect();
        ids.sort();
        ids
    }

    #[cfg(test)]
    pub fn pid(&self, id: &str) -> Option<u32> {
        self.procs.get(id).map(|p| p.child.id())
    }

    /// Detect exits and settled tunnels. Call on every UI tick.
    pub fn poll(&mut self) -> Vec<TunnelEvent> {
        let mut events = Vec::new();
        let mut gone = Vec::new();
        for (id, p) in self.procs.iter_mut() {
            match p.child.try_wait() {
                Ok(Some(status)) => {
                    // The reader thread usually has the last line in flight.
                    if let Some(reader) = p.reader.take() {
                        let end = Instant::now() + Duration::from_millis(200);
                        while !reader.is_finished() && Instant::now() < end {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        if reader.is_finished() {
                            let _ = reader.join();
                        }
                    }
                    let text = p.stderr.lock().map(|s| s.clone()).unwrap_or_default();
                    events.push(TunnelEvent::Exited {
                        id: id.clone(),
                        message: friendly_error(&text, status.code()),
                    });
                    gone.push(id.clone());
                }
                Ok(None) => {
                    if !p.reported && p.started.elapsed() >= self.settle {
                        p.reported = true;
                        events.push(TunnelEvent::Running(id.clone()));
                    }
                }
                Err(_) => gone.push(id.clone()),
            }
        }
        for id in gone {
            self.procs.remove(&id);
        }
        events
    }
}

impl Drop for TunnelRegistry {
    fn drop(&mut self) {
        self.stop_all();
    }
}

// ---------------------------------------------------------------- persistence worker

enum Cmd {
    List { host_id: String },
    Save { host_id: String, draft: TunnelDraft },
    Delete { host_id: String, id: String },
}

enum Ev {
    Listed {
        host_id: String,
        items: Vec<TunnelItem>,
    },
    Failed(String),
}

fn item_from_forward(pf: &PortForward) -> TunnelItem {
    TunnelItem {
        id: pf.id.to_string(),
        name: pf.name.clone(),
        kind: TunnelKind::parse(&pf.kind),
        bind_host: pf.bind_host.clone(),
        bind_port: pf.bind_port,
        dest_host: pf.dest_host.clone().unwrap_or_default(),
        dest_port: pf.dest_port.unwrap_or(0),
        status: TunnelStatus::Stopped,
        error: None,
    }
}

fn list_items(
    rt: &tokio::runtime::Runtime,
    store: &Store,
    host: Uuid,
) -> Result<Vec<TunnelItem>, String> {
    let mut rows = rt
        .block_on(store.list_forwards(Some(host)))
        .map_err(|e| format!("Could not read tunnels: {e}"))?;
    rows.sort_by_key(|r| (r.created_at, r.id));
    Ok(rows.iter().map(item_from_forward).collect())
}

fn handle(
    rt: &tokio::runtime::Runtime,
    store: &Store,
    cmd: Cmd,
) -> Result<(String, Vec<TunnelItem>), String> {
    let parse = |s: &str| {
        Uuid::parse_str(s).map_err(|_| "Tunnels need an SSH server".to_string())
    };
    match cmd {
        Cmd::List { host_id } => {
            let items = list_items(rt, store, parse(&host_id)?)?;
            Ok((host_id, items))
        }
        Cmd::Save { host_id, draft } => {
            let host = parse(&host_id)?;
            let now = Utc::now();
            let id = match &draft.id {
                Some(id) => {
                    Uuid::parse_str(id).map_err(|_| "Invalid tunnel id".to_string())?
                }
                None => Uuid::new_v4(),
            };
            let pf = PortForward {
                id,
                host_id: host,
                kind: draft.kind.as_str().into(),
                name: draft.name.clone(),
                bind_host: draft.bind_host.clone(),
                bind_port: draft.bind_port,
                dest_host: (!draft.dest_host.is_empty()).then(|| draft.dest_host.clone()),
                dest_port: (draft.dest_port != 0).then_some(draft.dest_port),
                created_at: now,
                updated_at: now,
                deleted_at: None,
            };
            rt.block_on(store.upsert_forward(&pf))
                .map_err(|e| format!("Could not save the tunnel: {e}"))?;
            Ok((host_id, list_items(rt, store, host)?))
        }
        Cmd::Delete { host_id, id } => {
            let host = parse(&host_id)?;
            let id = Uuid::parse_str(&id).map_err(|_| "Invalid tunnel id".to_string())?;
            rt.block_on(store.delete_forward(id))
                .map_err(|e| format!("Could not delete the tunnel: {e}"))?;
            Ok((host_id, list_items(rt, store, host)?))
        }
    }
}

fn worker(
    data_dir: PathBuf,
    commands: Receiver<Cmd>,
    events: Sender<Ev>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) {
    let send = |ev: Ev| {
        let _ = events.send(ev);
        if let Some(w) = &wake {
            w();
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return send(Ev::Failed(format!("Tunnels: no runtime ({e})"))),
    };
    let store = match rt.block_on(Store::open(data_dir)) {
        Ok(s) => s,
        Err(e) => {
            return send(Ev::Failed(format!(
                "Tunnels: cannot open the database ({e})"
            )))
        }
    };
    while let Ok(cmd) = commands.recv() {
        match handle(&rt, &store, cmd) {
            Ok((host_id, items)) => send(Ev::Listed { host_id, items }),
            Err(e) => send(Ev::Failed(e)),
        }
    }
}

// ---------------------------------------------------------------- controller

/// Builds the `ssh` command of a tunnel (the screen glue resolves the host's
/// credentials and calls [`tunnel_ssh_args`]).
pub type SpawnFn<'a> = &'a mut dyn FnMut(&TunnelItem) -> Result<Command, String>;

/// Everything the Tunnels view needs, owned in one place.
pub struct TunnelController {
    commands: Sender<Cmd>,
    events: Receiver<Ev>,
    registry: TunnelRegistry,
    state: TunnelsState,
    host_id: Option<String>,
    /// Status of every tunnel that has been started this session.
    statuses: HashMap<String, (TunnelStatus, Option<String>)>,
    host_of: HashMap<String, String>,
    names: HashMap<String, String>,
    notices: Vec<String>,
    /// A tunnel just created from "Start tunnel": start it once listed.
    pending_start: Option<String>,
}

impl TunnelController {
    /// Start the persistence worker. `wake` runs on the worker thread after
    /// each answer (the app passes a redraw request).
    pub fn spawn(data_dir: PathBuf, wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let (ev_tx, ev_rx) = channel();
        let _ = std::thread::Builder::new()
            .name("terminus-tunnels".into())
            .spawn(move || worker(data_dir, cmd_rx, ev_tx, wake));
        Self {
            commands: cmd_tx,
            events: ev_rx,
            registry: TunnelRegistry::new(DEFAULT_SETTLE),
            state: TunnelsState::new(""),
            host_id: None,
            statuses: HashMap::new(),
            host_of: HashMap::new(),
            names: HashMap::new(),
            notices: Vec::new(),
            pending_start: None,
        }
    }

    #[cfg(test)]
    pub fn with_settle(mut self, settle: Duration) -> Self {
        self.registry.settle = settle;
        self
    }

    pub fn state(&self) -> &TunnelsState {
        &self.state
    }

    pub fn host_id(&self) -> Option<&str> {
        self.host_id.as_deref()
    }

    /// Show the tunnels of `host_id` (a stored host id). Running tunnels of
    /// other machines keep running.
    pub fn select_machine(&mut self, host_id: &str, label: &str) {
        if self.host_id.as_deref() != Some(host_id) {
            self.state = TunnelsState::new(label);
            self.host_id = Some(host_id.to_string());
        } else {
            self.state.host_label = label.to_string();
        }
        let _ = self.commands.send(Cmd::List {
            host_id: host_id.to_string(),
        });
    }

    /// The machine on screen has no SSH server (this computer, WSL): show
    /// no tunnels and persist nothing. Running tunnels keep running.
    pub fn clear_machine(&mut self, label: &str) {
        if self.host_id.is_some() || self.state.host_label != label {
            self.state = TunnelsState::new(label);
            self.host_id = None;
        }
    }

    /// Running tunnels of the machine on screen: the Tunnels tab badge.
    pub fn running_count(&self) -> usize {
        self.state.running_count()
    }

    #[cfg(test)]
    /// Running tunnels of any machine (sidebar badges).
    pub fn running_count_for(&self, host_id: &str) -> usize {
        self.statuses
            .iter()
            .filter(|(id, (st, _))| {
                *st == TunnelStatus::Running
                    && self.host_of.get(*id).map(String::as_str) == Some(host_id)
            })
            .count()
    }

    /// Persist a validated tunnel for the current machine.
    pub fn save(&mut self, draft: &TunnelDraft) {
        if let Some(host_id) = self.host_id.clone() {
            let _ = self.commands.send(Cmd::Save {
                host_id,
                draft: draft.clone(),
            });
        }
    }

    /// Stop (if running) and soft-delete a tunnel.
    pub fn delete(&mut self, id: &str) {
        self.registry.stop(id);
        self.statuses.remove(id);
        if let Some(host_id) = self.host_id.clone() {
            let _ = self.commands.send(Cmd::Delete {
                host_id,
                id: id.to_string(),
            });
        }
        self.state.items.retain(|t| t.id != id);
    }

    /// Start a stopped/failed tunnel, stop an active one.
    pub fn toggle(&mut self, id: &str, spawn: SpawnFn) {
        let Some(item) = self.state.items.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        if self.registry.is_active(id) {
            self.registry.stop(id);
            self.statuses.remove(id);
        } else {
            match spawn(&item).and_then(|cmd| self.registry.start(id, cmd)) {
                Ok(()) => {
                    self.statuses
                        .insert(id.to_string(), (TunnelStatus::Starting, None));
                }
                Err(msg) => {
                    self.notices.push(format!("{}: {msg}", item.name));
                    self.statuses
                        .insert(id.to_string(), (TunnelStatus::Failed, Some(msg)));
                }
            }
        }
        self.apply_statuses();
    }

    /// Start the tunnel created by the last "Start tunnel" once the worker
    /// has listed it. Call after [`tick`](Self::tick); true when it started.
    pub fn start_pending(&mut self, spawn: SpawnFn) -> bool {
        let Some(id) = self.pending_start.clone() else {
            return false;
        };
        if !self.state.items.iter().any(|t| t.id == id) {
            return false;
        }
        self.pending_start = None;
        self.toggle(&id, spawn);
        true
    }

    /// The machine was deleted: stop its tunnels.
    pub fn host_deleted(&mut self, host_id: &str) {
        let ids: Vec<String> = self
            .host_of
            .iter()
            .filter(|(_, h)| h.as_str() == host_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.registry.stop(&id);
            self.statuses.remove(&id);
        }
        if self.host_id.as_deref() == Some(host_id) {
            self.state.items.clear();
        }
    }

    /// App quit: stop every process.
    pub fn shutdown(&mut self) {
        self.registry.stop_all();
        self.statuses.clear();
        self.apply_statuses();
    }

    /// Error messages worth a toast, once each.
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    fn apply_statuses(&mut self) {
        for item in &mut self.state.items {
            match self.statuses.get(&item.id) {
                Some((st, err)) => {
                    item.status = *st;
                    item.error = err.clone();
                }
                None => {
                    item.status = TunnelStatus::Stopped;
                    item.error = None;
                }
            }
        }
    }

    /// Drain worker answers and process events. True when the view changed.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok(ev) = self.events.try_recv() {
            changed = true;
            match ev {
                Ev::Listed { host_id, items } => {
                    for t in &items {
                        self.host_of.insert(t.id.clone(), host_id.clone());
                        self.names.insert(t.id.clone(), t.name.clone());
                    }
                    if self.host_id.as_deref() == Some(host_id.as_str()) {
                        self.state.items = items;
                        self.apply_statuses();
                    }
                }
                Ev::Failed(msg) => self.notices.push(msg),
            }
        }
        for ev in self.registry.poll() {
            changed = true;
            match ev {
                TunnelEvent::Running(id) => {
                    self.statuses.insert(id, (TunnelStatus::Running, None));
                }
                TunnelEvent::Exited { id, message } => {
                    let name = self
                        .names
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| "Tunnel".into());
                    self.notices.push(format!("{name}: {message}"));
                    self.statuses
                        .insert(id, (TunnelStatus::Failed, Some(message)));
                }
            }
        }
        if changed {
            self.apply_statuses();
        }
        changed
    }

    /// Probe used by the form: a local port is acceptable when nothing
    /// listens on it, or when it is the one the edited tunnel already holds,
    /// and no other tunnel of this machine is configured on it.
    fn port_probe(&self) -> impl Fn(u16) -> bool {
        let editing = self.state.form.as_ref().and_then(|f| f.id.clone());
        let own_active = editing
            .as_deref()
            .and_then(|id| self.state.items.iter().find(|t| t.id == id))
            .filter(|t| t.status.is_active())
            .map(|t| t.bind_port);
        let others: Vec<u16> = self
            .state
            .items
            .iter()
            .filter(|t| t.kind != TunnelKind::Remote && Some(&t.id) != editing.as_ref())
            .map(|t| t.bind_port)
            .collect();
        move |p| !others.contains(&p) && (own_active == Some(p) || local_port_free(p))
    }

    fn run(&mut self, action: TunnelAction, spawn: SpawnFn) -> bool {
        match action {
            TunnelAction::None => false,
            TunnelAction::Redraw => true,
            TunnelAction::Save(mut draft) => {
                if draft.id.is_none() {
                    let id = Uuid::new_v4().to_string();
                    draft.id = Some(id.clone());
                    self.pending_start = Some(id);
                }
                self.save(&draft);
                true
            }
            TunnelAction::Toggle(id) => {
                self.toggle(&id, spawn);
                true
            }
            TunnelAction::Delete(id) => {
                self.delete(&id);
                true
            }
        }
    }

    /// Pointer press inside `content`; true when a repaint is needed.
    pub fn press(&mut self, content: Rect, x: f32, y: f32, spawn: SpawnFn) -> bool {
        let probe = self.port_probe();
        let action = self.state.press(content, x, y, &probe);
        self.run(action, spawn)
    }

    /// Key while the dialog is open; true when a repaint is needed.
    pub fn key(&mut self, key: FormKey, spawn: SpawnFn) -> bool {
        let probe = self.port_probe();
        let action = self.state.key(key, &probe);
        self.run(action, spawn)
    }

    pub fn hover(&mut self, content: Rect, x: f32, y: f32) -> bool {
        self.state.hover(content, x, y)
    }
}

impl Drop for TunnelController {
    fn drop(&mut self) {
        self.registry.stop_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};
    use terminus_ui::views::tunnels::{
        TunnelDraft, TunnelItem, TunnelKind, TunnelStatus,
    };

    fn item(kind: TunnelKind) -> TunnelItem {
        TunnelItem {
            id: "t1".into(),
            name: "db".into(),
            kind,
            bind_host: "127.0.0.1".into(),
            bind_port: 5432,
            dest_host: "db.internal".into(),
            dest_port: 5433,
            status: TunnelStatus::Stopped,
            error: None,
        }
    }

    fn base() -> Vec<String> {
        [
            "-p",
            "2222",
            "-i",
            "/k",
            "-o",
            "IdentitiesOnly=yes",
            "me@example.com",
        ]
        .map(String::from)
        .to_vec()
    }

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    fn wait_for(mut f: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn forward_flags_per_kind() {
        assert_eq!(
            forward_args(&item(TunnelKind::Local)),
            ["-L", "127.0.0.1:5432:db.internal:5433"]
        );
        assert_eq!(
            forward_args(&item(TunnelKind::Remote)),
            ["-R", "127.0.0.1:5432:db.internal:5433"]
        );
        assert_eq!(
            forward_args(&item(TunnelKind::Dynamic)),
            ["-D", "127.0.0.1:5432"]
        );
    }

    #[test]
    fn the_command_is_a_non_interactive_forward_only_ssh() {
        let args = tunnel_ssh_args(&base(), false, &item(TunnelKind::Local));
        assert_eq!(
            args.last().unwrap(),
            "me@example.com",
            "destination stays last"
        );
        assert!(args.contains(&"-N".to_string()));
        let joined = args.join(" ");
        assert!(joined.contains("ExitOnForwardFailure=yes"));
        assert!(
            joined.contains("BatchMode=yes"),
            "never prompt without askpass"
        );
        assert!(joined.contains("-L 127.0.0.1:5432:db.internal:5433"));
        // The host's own options are preserved verbatim.
        assert!(joined.contains("-p 2222 -i /k -o IdentitiesOnly=yes"));
    }

    #[test]
    fn askpass_hosts_do_not_get_batch_mode() {
        let args = tunnel_ssh_args(&base(), true, &item(TunnelKind::Dynamic));
        assert!(!args.join(" ").contains("BatchMode"));
        assert!(args.join(" ").contains("-D 127.0.0.1:5432"));
    }

    #[test]
    fn ssh_errors_become_short_sentences() {
        assert!(friendly_error(
            "bind [127.0.0.1]:80: Address already in use\n",
            Some(255)
        )
        .contains("already in use"));
        assert!(
            friendly_error("me@h: Permission denied (publickey).", Some(255))
                .contains("Authentication")
        );
        assert!(friendly_error(
            "ssh: connect to host h port 22: Connection refused",
            Some(255)
        )
        .contains("Could not reach"));
        assert!(
            friendly_error("ssh: Could not resolve hostname x", Some(255))
                .contains("Could not reach")
        );
        assert!(friendly_error("Host key verification failed.", Some(255))
            .contains("Host key"));
        assert!(friendly_error(
            "Warning: remote port forwarding failed for listen port 80",
            Some(255)
        )
        .contains("refused"));
        assert_eq!(friendly_error("some\nodd line  \n", Some(7)), "odd line");
        assert_eq!(friendly_error("", Some(3)), "ssh exited with code 3");
        assert_eq!(friendly_error("", None), "ssh was terminated");
    }

    /// A killed app (SIGTERM/SIGKILL skip Drop) must not leave `ssh -N`
    /// holding ports: the child dies with the thread that started it.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_tunnel_dies_with_the_thread_that_started_it() {
        let mut r = TunnelRegistry::new(Duration::from_millis(10));
        let mut r = std::thread::spawn(move || {
            r.start("orphan", sh("sleep 30")).unwrap();
            r
        })
        .join()
        .unwrap();
        let mut events = Vec::new();
        assert!(
            wait_for(|| {
                events.extend(r.poll());
                events
                    .iter()
                    .any(|e| matches!(e, TunnelEvent::Exited { id, .. } if id == "orphan"))
            }),
            "child outlived its parent thread: {events:?}"
        );
    }

    #[test]
    fn a_process_that_stays_up_becomes_running_then_stops_cleanly() {
        let mut r = TunnelRegistry::new(Duration::from_millis(50));
        r.start("a", sh("sleep 30")).unwrap();
        assert!(r.is_active("a"));
        let mut events = Vec::new();
        assert!(wait_for(|| {
            events.extend(r.poll());
            events.contains(&TunnelEvent::Running("a".into()))
        }));
        assert!(r.stop("a"));
        assert!(!r.is_active("a"));
        assert!(r.poll().is_empty(), "a requested stop is not an error");
        assert!(!r.stop("a"));
    }

    #[test]
    fn an_early_exit_reports_the_error_once() {
        let mut r = TunnelRegistry::new(Duration::from_millis(500));
        r.start("b", sh("echo 'bind: Address already in use' >&2; exit 255"))
            .unwrap();
        let mut got = None;
        assert!(wait_for(|| {
            for e in r.poll() {
                if let TunnelEvent::Exited { id, message } = e {
                    got = Some((id, message));
                }
            }
            got.is_some()
        }));
        let (id, message) = got.unwrap();
        assert_eq!(id, "b");
        assert!(message.contains("already in use"), "{message}");
        assert!(!r.is_active("b"));
        assert!(r.poll().is_empty());
    }

    #[test]
    fn starting_twice_replaces_the_old_process() {
        let mut r = TunnelRegistry::new(Duration::from_millis(10));
        r.start("a", sh("sleep 30")).unwrap();
        r.start("a", sh("sleep 30")).unwrap();
        assert_eq!(r.active_ids(), vec!["a".to_string()]);
        r.stop_all();
        assert!(r.active_ids().is_empty());
    }

    #[test]
    fn a_missing_program_is_a_start_error() {
        let mut r = TunnelRegistry::new(Duration::from_millis(10));
        let err = r
            .start("a", Command::new("definitely-not-a-real-binary-xyz"))
            .unwrap_err();
        assert!(err.contains("ssh") || err.contains("start"), "{err}");
        assert!(!r.is_active("a"));
    }

    #[test]
    fn dropping_the_registry_kills_the_children() {
        let pid;
        {
            let mut r = TunnelRegistry::new(Duration::from_millis(10));
            r.start("a", sh("sleep 30")).unwrap();
            pid = r.pid("a").unwrap();
        }
        assert!(wait_for(|| unsafe { libc::kill(pid as i32, 0) } != 0));
    }

    fn dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("terminus-tunnels-{tag}-{}", uuid::Uuid::new_v4()))
    }

    fn draft(name: &str, port: u16) -> TunnelDraft {
        TunnelDraft {
            id: None,
            kind: TunnelKind::Local,
            name: name.into(),
            bind_host: "127.0.0.1".into(),
            bind_port: port,
            dest_host: "localhost".into(),
            dest_port: port,
        }
    }

    fn pump(
        c: &mut TunnelController,
        mut done: impl FnMut(&TunnelController) -> bool,
    ) -> bool {
        wait_for(|| {
            c.tick();
            done(c)
        })
    }

    #[test]
    fn the_controller_persists_and_reloads_per_machine() {
        let d = dir("persist");
        let host = uuid::Uuid::new_v4().to_string();
        {
            let mut c = TunnelController::spawn(d.clone(), None);
            c.select_machine(&host, "prod");
            c.save(&draft("db", 5432));
            assert!(pump(&mut c, |c| c.state().items.len() == 1));
            assert_eq!(c.state().items[0].name, "db");
            assert_eq!(c.state().items[0].status, TunnelStatus::Stopped);
        }
        let mut c = TunnelController::spawn(d, None);
        c.select_machine(&host, "prod");
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
        c.select_machine(&uuid::Uuid::new_v4().to_string(), "other");
        assert!(pump(&mut c, |c| c.state().items.is_empty()));
    }

    #[test]
    fn a_machine_without_ssh_shows_no_tunnels_and_saves_nothing() {
        let mut c = TunnelController::spawn(dir("nossh"), None);
        let host = uuid::Uuid::new_v4().to_string();
        c.select_machine(&host, "prod");
        c.save(&draft("db", 5432));
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
        c.clear_machine("This computer");
        assert_eq!(c.host_id(), None);
        assert!(c.state().items.is_empty());
        assert_eq!(c.state().host_label, "This computer");
        c.save(&draft("ignored", 5433));
        assert!(!pump(&mut c, |c| !c.state().items.is_empty()));
    }

    #[test]
    fn editing_updates_in_place_and_delete_removes() {
        let mut c = TunnelController::spawn(dir("edit"), None);
        let host = uuid::Uuid::new_v4().to_string();
        c.select_machine(&host, "prod");
        c.save(&draft("db", 5432));
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
        let id = c.state().items[0].id.clone();
        let mut edited = draft("db2", 5433);
        edited.id = Some(id.clone());
        c.save(&edited);
        assert!(pump(&mut c, |c| c
            .state()
            .items
            .first()
            .is_some_and(|t| t.name == "db2")));
        assert_eq!(c.state().items.len(), 1);
        c.delete(&id);
        assert!(pump(&mut c, |c| c.state().items.is_empty()));
    }

    #[test]
    fn toggle_starts_runs_and_stops_a_tunnel_with_a_fake_command() {
        let mut c = TunnelController::spawn(dir("run"), None)
            .with_settle(Duration::from_millis(30));
        let host = uuid::Uuid::new_v4().to_string();
        c.select_machine(&host, "prod");
        c.save(&draft("db", 5432));
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
        let id = c.state().items[0].id.clone();
        c.toggle(&id, &mut |_| Ok(sh("sleep 30")));
        assert!(c.state().items[0].status.is_active());
        assert!(pump(&mut c, |c| c.state().running_count() == 1));
        assert_eq!(c.running_count_for(&host), 1);
        c.toggle(&id, &mut |_| unreachable!("stopping must not spawn"));
        assert_eq!(c.state().items[0].status, TunnelStatus::Stopped);
        assert_eq!(c.state().running_count(), 0);
    }

    #[test]
    fn a_dying_tunnel_becomes_failed_with_a_notice() {
        let mut c = TunnelController::spawn(dir("fail"), None)
            .with_settle(Duration::from_millis(500));
        let host = uuid::Uuid::new_v4().to_string();
        c.select_machine(&host, "prod");
        c.save(&draft("db", 5432));
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
        let id = c.state().items[0].id.clone();
        c.toggle(&id, &mut |_| {
            Ok(sh("echo 'Permission denied (publickey).' >&2; exit 255"))
        });
        assert!(pump(&mut c, |c| c.state().items[0].status
            == TunnelStatus::Failed));
        assert!(c.state().items[0]
            .error
            .as_deref()
            .unwrap()
            .contains("Authentication"));
        let notices = c.take_notices();
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("db"), "{notices:?}");
        assert!(c.take_notices().is_empty());
        // Failed tunnels can be started again.
        c.toggle(&id, &mut |_| Ok(sh("sleep 30")));
        assert!(c.state().items[0].status.is_active());
    }

    #[test]
    fn a_spawn_error_is_a_notice_and_a_failed_card() {
        let mut c = TunnelController::spawn(dir("spawnerr"), None);
        let host = uuid::Uuid::new_v4().to_string();
        c.select_machine(&host, "prod");
        c.save(&draft("db", 5432));
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
        let id = c.state().items[0].id.clone();
        c.toggle(&id, &mut |_| Err("No saved password".into()));
        assert_eq!(c.state().items[0].status, TunnelStatus::Failed);
        assert_eq!(c.take_notices().len(), 1);
    }

    #[test]
    fn host_deletion_and_shutdown_stop_processes() {
        let mut c = TunnelController::spawn(dir("kill"), None)
            .with_settle(Duration::from_millis(10));
        let host = uuid::Uuid::new_v4().to_string();
        c.select_machine(&host, "prod");
        c.save(&draft("db", 5432));
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
        let id = c.state().items[0].id.clone();
        c.toggle(&id, &mut |_| Ok(sh("sleep 30")));
        assert!(pump(&mut c, |c| c.running_count_for(&host) == 1));
        c.host_deleted(&host);
        assert_eq!(c.running_count_for(&host), 0);
        c.toggle(&id, &mut |_| Ok(sh("sleep 30")));
        c.shutdown();
        assert_eq!(c.running_count_for(&host), 0);
    }

    #[test]
    fn a_new_tunnel_saved_from_the_form_starts_once_listed() {
        let mut c = TunnelController::spawn(dir("pending"), None)
            .with_settle(Duration::from_millis(10));
        let host = uuid::Uuid::new_v4().to_string();
        c.select_machine(&host, "prod");
        c.run(
            TunnelAction::Save(draft("db", 5432)),
            &mut |_| unreachable!(),
        );
        let mut started = false;
        assert!(pump(&mut c, |c| {
            let _ = c;
            true
        }));
        assert!(wait_for(|| {
            c.tick();
            started = started || c.start_pending(&mut |_| Ok(sh("sleep 30")));
            started
        }));
        assert!(c.state().items[0].status.is_active());
    }

    #[test]
    fn the_free_port_probe_sees_a_bound_port() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(!local_port_free(port));
        drop(l);
        assert!(local_port_free(port));
    }
}
