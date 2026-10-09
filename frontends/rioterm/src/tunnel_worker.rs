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

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use terminus_core::models::PortForward;
use terminus_core::Store;
use terminus_ui::geom::Rect;
use terminus_ui::views::tunnels::{
    FormKey, TunnelAction, TunnelDraft, TunnelItem, TunnelKind, TunnelStats,
    TunnelStatus, TunnelsState,
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

// ---------------------------------------------------------------- stats

/// One socket row of `/proc/net/tcp{,6}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TcpRow {
    pub local_port: u16,
    pub remote_port: u16,
    pub established: bool,
    pub inode: u64,
}

/// Parse the text of `/proc/net/tcp` or `/proc/net/tcp6`; malformed lines
/// (and the header) are skipped.
pub fn parse_proc_net_tcp(text: &str) -> Vec<TcpRow> {
    // `sl local rem st tx:rx tr:tm retrnsmt uid timeout inode …`; addresses
    // are `HEXIP:HEXPORT` (8 or 32 hex digits of IP), state `01` is
    // ESTABLISHED.
    fn port(addr: &str) -> Option<u16> {
        u16::from_str_radix(addr.rsplit_once(':')?.1, 16).ok()
    }
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || !f[0].ends_with(':') {
                return None;
            }
            Some(TcpRow {
                local_port: port(f[1])?,
                remote_port: port(f[2])?,
                established: f[3] == "01",
                inode: f[9].parse().ok()?,
            })
        })
        .collect()
}

/// Established forwarded connections of one tunnel among `rows`, counting
/// only sockets owned by its ssh process (`owned` = socket inodes of
/// `/proc/<pid>/fd`):
///
/// * `-L` / `-D`: connections ssh accepted, i.e. local port == `bind_port`.
/// * `-R`: connections ssh opened to the destination, i.e. remote port ==
///   `dest_port`.
pub fn count_connections(
    rows: &[TcpRow],
    kind: TunnelKind,
    bind_port: u16,
    dest_port: u16,
    owned: &HashSet<u64>,
) -> u32 {
    rows.iter()
        .filter(|r| r.established && owned.contains(&r.inode))
        .filter(|r| match kind {
            TunnelKind::Local | TunnelKind::Dynamic => r.local_port == bind_port,
            TunnelKind::Remote => r.remote_port == dest_port,
        })
        .count() as u32
}

/// Sample the established connections of the tunnel run by ssh process
/// `pid`. `None` when the platform cannot tell (anything but Linux) or the
/// process is gone.
pub fn sample_connections(
    pid: u32,
    kind: TunnelKind,
    bind_port: u16,
    dest_port: u16,
) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let owned: HashSet<u64> = std::fs::read_dir(format!("/proc/{pid}/fd"))
            .ok()?
            .filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
            .filter_map(|l| {
                let l = l.to_str()?;
                l.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
            })
            .collect();
        let mut rows =
            parse_proc_net_tcp(&std::fs::read_to_string("/proc/net/tcp").ok()?);
        if let Ok(v6) = std::fs::read_to_string("/proc/net/tcp6") {
            rows.extend(parse_proc_net_tcp(&v6));
        }
        Some(count_connections(&rows, kind, bind_port, dest_port, &owned))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (pid, kind, bind_port, dest_port);
        None
    }
}

/// Connections are re-sampled at most this often (`/proc` reads).
const SAMPLE_EVERY: Duration = Duration::from_secs(1);

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
    /// Temp key / askpass files of this ssh, removed when it is reaped.
    _secrets: crate::ssh_secrets::SecretFiles,
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
        // Before spawning: a failed spawn drops it and removes the files.
        let secrets =
            crate::ssh_secrets::SecretFiles::new(crate::ssh_secrets::launch_secrets(
                cmd.get_args().filter_map(|a| a.to_str()),
                cmd.get_envs()
                    .filter_map(|(k, v)| Some((k.to_str()?, v?.to_str()?))),
            ));
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
                _secrets: secrets,
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

    pub fn pid(&self, id: &str) -> Option<u32> {
        self.procs.get(id).map(|p| p.child.id())
    }

    /// How long the process of `id` has been alive.
    pub fn uptime(&self, id: &str) -> Option<Duration> {
        self.procs.get(id).map(|p| p.started.elapsed())
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
                Err(err) => {
                    let _ = p.child.kill();
                    let _ = p.child.wait();
                    events.push(TunnelEvent::Exited {
                        id: id.clone(),
                        message: format!("Lost track of the ssh process: {err}"),
                    });
                    gone.push(id.clone());
                }
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
        stats: None,
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

/// Ids designated by `key` among `(id, name)` candidates: an exact id, else
/// every name equal to it ignoring case.
fn matching_ids(candidates: &[(&str, &str)], key: &str) -> Vec<String> {
    let key = key.trim();
    if let Some((id, _)) = candidates.iter().find(|(id, _)| *id == key) {
        return vec![id.to_string()];
    }
    let key = key.to_lowercase();
    candidates
        .iter()
        .filter(|(_, name)| name.to_lowercase() == key)
        .map(|(id, _)| id.to_string())
        .collect()
}

fn one_match(key: &str, mut ids: Vec<String>) -> Result<String, String> {
    match ids.len() {
        0 => Err(format!("No tunnel named \"{}\"", key.trim())),
        1 => Ok(ids.remove(0)),
        _ => Err(format!(
            "More than one tunnel is named \"{}\"; use its id",
            key.trim()
        )),
    }
}

/// Wakes the UI once a second while a tunnel of the machine on screen runs,
/// so its uptime moves even in an otherwise idle window. One thread at most;
/// it ends when nothing runs or the controller is dropped.
struct Ticker {
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    shared: Arc<TickShared>,
}

#[derive(Default)]
struct TickShared {
    active: AtomicBool,
    running: AtomicBool,
    dead: AtomicBool,
}

impl Ticker {
    fn new(wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        Self {
            wake,
            shared: Arc::new(TickShared::default()),
        }
    }

    fn set_active(&self, active: bool) {
        self.shared.active.store(active, Ordering::SeqCst);
        let Some(wake) = self.wake.clone() else {
            return;
        };
        if !active || self.shared.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.shared.clone();
        let spawned = std::thread::Builder::new()
            .name("terminus-tunnel-tick".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(1));
                if shared.dead.load(Ordering::SeqCst) {
                    break;
                }
                if !shared.active.load(Ordering::SeqCst) {
                    shared.running.store(false, Ordering::SeqCst);
                    // `set_active(true)` may have raced the store above.
                    if shared.active.load(Ordering::SeqCst)
                        && !shared.running.swap(true, Ordering::SeqCst)
                    {
                        continue;
                    }
                    break;
                }
                wake();
            });
        if spawned.is_err() {
            self.shared.running.store(false, Ordering::SeqCst);
        }
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.shared.dead.store(true, Ordering::SeqCst);
    }
}

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
    /// Live numbers of the active tunnels of the machine on screen.
    stats: HashMap<String, TunnelStats>,
    last_sample: Option<Instant>,
    ticker: Ticker,
}

impl TunnelController {
    /// Start the persistence worker. `wake` runs on the worker thread after
    /// each answer (the app passes a redraw request).
    pub fn spawn(data_dir: PathBuf, wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let (ev_tx, ev_rx) = channel();
        let ticker = Ticker::new(wake.clone());
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
            stats: HashMap::new(),
            last_sample: None,
            ticker,
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

    /// Id of the tunnel of the machine on screen designated by `key`: an
    /// exact id, else a name (case-insensitive). Unknown and ambiguous keys
    /// are errors.
    pub fn resolve_current(&self, key: &str) -> Result<String, String> {
        let candidates: Vec<(&str, &str)> = self
            .state
            .items
            .iter()
            .map(|t| (t.id.as_str(), t.name.as_str()))
            .collect();
        one_match(key, matching_ids(&candidates, key))
    }

    /// Start a tunnel of the machine on screen by id or name (what a command
    /// palette calls). Already running or starting is a no-op `Ok`. On
    /// failure the card turns Failed and `Err` carries a message fit for a
    /// toast (no separate notice is queued: the caller shows the `Err`).
    ///
    /// `spawn` builds the `ssh` command for the machine on screen, so only
    /// that machine's tunnels can be started.
    pub fn start_tunnel(&mut self, key: &str, spawn: SpawnFn) -> Result<(), String> {
        let id = self.resolve_current(key)?;
        if self.registry.is_active(&id) {
            return Ok(());
        }
        let Some(item) = self.state.items.iter().find(|t| t.id == id).cloned() else {
            return Err(format!("No tunnel \"{key}\""));
        };
        let result = self.launch(&item, spawn);
        self.apply_statuses();
        result
    }

    /// Stop a tunnel by id or name. Works for any machine; stopping one that
    /// is not running is a no-op `Ok`. `Err` for an unknown or ambiguous key.
    pub fn stop_tunnel(&mut self, key: &str) -> Result<(), String> {
        let candidates: Vec<(&str, &str)> = self
            .names
            .iter()
            .map(|(id, name)| (id.as_str(), name.as_str()))
            .collect();
        let mut ids = matching_ids(&candidates, key);
        if ids.len() > 1 {
            // The same name on several machines: the running one is meant.
            ids.retain(|id| self.registry.is_active(id));
            if ids.is_empty() {
                // Same name on several machines and none is running.
                return Ok(());
            }
        }
        let id = one_match(key, ids)?;
        self.registry.stop(&id);
        self.statuses.remove(&id);
        self.apply_statuses();
        Ok(())
    }

    /// Spawn the process of `item` and record Starting or Failed.
    fn launch(&mut self, item: &TunnelItem, spawn: SpawnFn) -> Result<(), String> {
        match spawn(item).and_then(|cmd| self.registry.start(&item.id, cmd)) {
            Ok(()) => {
                self.statuses
                    .insert(item.id.clone(), (TunnelStatus::Starting, None));
                Ok(())
            }
            Err(msg) => {
                self.statuses
                    .insert(item.id.clone(), (TunnelStatus::Failed, Some(msg.clone())));
                Err(msg)
            }
        }
    }

    /// Stop (if running) and soft-delete a tunnel.
    pub fn delete(&mut self, id: &str) {
        self.registry.stop(id);
        self.statuses.remove(id);
        self.names.remove(id);
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
        } else if let Err(msg) = self.launch(&item, spawn) {
            self.notices.push(format!("{}: {msg}", item.name));
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
            self.names.remove(&id);
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
            item.stats = if item.status.is_active() {
                self.stats.get(&item.id).copied()
            } else {
                None
            };
        }
    }

    /// Update uptime (every call) and connection samples (about once a
    /// second) of the active tunnels on screen. True when a number changed.
    fn refresh_stats(&mut self) -> bool {
        let due = self.last_sample.is_none_or(|t| t.elapsed() >= SAMPLE_EVERY);
        let mut next = HashMap::new();
        for item in &self.state.items {
            let Some(up) = self.registry.uptime(&item.id) else {
                continue;
            };
            let connections = match self.stats.get(&item.id) {
                Some(prev) if !due => prev.connections,
                _ => self.registry.pid(&item.id).and_then(|pid| {
                    sample_connections(pid, item.kind, item.bind_port, item.dest_port)
                }),
            };
            next.insert(
                item.id.clone(),
                TunnelStats {
                    uptime_secs: up.as_secs(),
                    connections,
                },
            );
        }
        if due {
            self.last_sample = Some(Instant::now());
        }
        self.ticker.set_active(!next.is_empty());
        if next == self.stats {
            return false;
        }
        self.stats = next;
        true
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
        changed |= self.refresh_stats();
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
            stats: None,
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
            "--",
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
        assert_eq!(args[args.len() - 2], "--", "options end before the host");
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
                events.iter().any(
                    |e| matches!(e, TunnelEvent::Exited { id, .. } if id == "orphan"),
                )
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

    /// A login that fails never reaches ssh's `LocalCommand`: reaping the
    /// process is what removes its temp key and askpass secret.
    #[test]
    fn a_failed_tunnel_removes_its_temp_secrets() {
        use crate::ssh_secrets::{private_temp_dir, write_private_file};
        let key = private_temp_dir(crate::ssh_secrets::IDENTITY_DIR)
            .unwrap()
            .join(format!("{}.pem", uuid::Uuid::new_v4()));
        write_private_file(&key, b"key").unwrap();
        let secret = private_temp_dir(crate::ssh_secrets::ASKPASS_DIR)
            .unwrap()
            .join(format!("{}.secret", uuid::Uuid::new_v4()));
        write_private_file(&secret, b"pass").unwrap();
        let mut cmd = sh("echo 'Permission denied (publickey).' >&2; exit 255");
        cmd.arg("-i").arg(&key);
        cmd.env(crate::ssh_secrets::ASKPASS_FILE_ENV, &secret);
        let mut r = TunnelRegistry::new(Duration::from_millis(500));
        r.start("k", cmd).unwrap();
        assert!(key.exists(), "removed before ssh could read it");
        assert!(wait_for(|| r
            .poll()
            .iter()
            .any(|e| matches!(e, TunnelEvent::Exited { .. }))));
        assert!(!key.exists(), "temp key left on disk after a failed login");
        assert!(!secret.exists(), "askpass secret left on disk");
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

    // ------------------------------------------------------------ stats

    const PROC_TCP: &str = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1538 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 11111 1 0000000000000000 100 0 0 10 0
   1: 0100007F:1538 0100007F:C350 01 00000000:00000000 00:00000000 00000000  1000        0 22222 1 0000000000000000 20 4 30 10 -1
   2: 0100007F:C350 0100007F:1538 01 00000000:00000000 00:00000000 00000000  1000        0 33333 1 0000000000000000 20 4 30 10 -1
   3: garbage
";

    const PROC_TCP6: &str = "\
  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:1538 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 44444 1 0000000000000000 100 0 0 10 0
   1: 00000000000000000000000001000000:1538 00000000000000000000000001000000:D431 01 00000000:00000000 00:00000000 00000000  1000        0 55555 1 0000000000000000 20 4 30 10 -1
";

    fn owned(inodes: &[u64]) -> HashSet<u64> {
        inodes.iter().copied().collect()
    }

    #[test]
    fn proc_net_tcp_rows_are_parsed_and_junk_is_skipped() {
        let rows = parse_proc_net_tcp(PROC_TCP);
        assert_eq!(
            rows,
            vec![
                TcpRow {
                    local_port: 5432,
                    remote_port: 0,
                    established: false,
                    inode: 11111
                },
                TcpRow {
                    local_port: 5432,
                    remote_port: 50000,
                    established: true,
                    inode: 22222
                },
                TcpRow {
                    local_port: 50000,
                    remote_port: 5432,
                    established: true,
                    inode: 33333
                },
            ]
        );
        let rows6 = parse_proc_net_tcp(PROC_TCP6);
        assert_eq!(rows6.len(), 2);
        assert_eq!(rows6[1].local_port, 5432);
        assert_eq!(rows6[1].remote_port, 0xD431);
        assert!(rows6[1].established && !rows6[0].established);
        assert!(parse_proc_net_tcp("").is_empty());
    }

    #[test]
    fn local_and_dynamic_count_accepted_connections_only() {
        let rows = parse_proc_net_tcp(PROC_TCP);
        let all = owned(&[11111, 22222, 33333]);
        for kind in [TunnelKind::Local, TunnelKind::Dynamic] {
            // LISTEN is not a connection; the client end (remote port ==
            // bind port) is not ssh's accepted socket.
            assert_eq!(count_connections(&rows, kind, 5432, 0, &all), 1, "{kind:?}");
        }
        assert_eq!(
            count_connections(&rows, TunnelKind::Local, 5432, 0, &owned(&[])),
            0
        );
        assert_eq!(
            count_connections(&rows, TunnelKind::Local, 9999, 0, &all),
            0
        );
    }

    #[test]
    fn remote_counts_connections_opened_to_the_destination() {
        let rows = parse_proc_net_tcp(PROC_TCP);
        assert_eq!(
            count_connections(&rows, TunnelKind::Remote, 7000, 5432, &owned(&[33333])),
            1
        );
        // Sockets of other processes are not ours.
        assert_eq!(
            count_connections(&rows, TunnelKind::Remote, 7000, 5432, &owned(&[22222])),
            0
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_live_accepted_connection_is_sampled_from_proc() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let me = std::process::id();
        assert_eq!(sample_connections(me, TunnelKind::Local, port, 0), Some(0));
        let _client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let (_server, _) = l.accept().unwrap();
        assert_eq!(sample_connections(me, TunnelKind::Local, port, 0), Some(1));
        assert_eq!(
            sample_connections(0x7fff_fff0, TunnelKind::Local, port, 0),
            None
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn connections_are_unavailable_off_linux() {
        assert_eq!(sample_connections(1, TunnelKind::Local, 1, 0), None);
    }

    #[test]
    fn the_registry_reports_uptime_until_the_stop() {
        let mut r = TunnelRegistry::new(Duration::from_millis(10));
        assert_eq!(r.uptime("a"), None);
        r.start("a", sh("sleep 30")).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        let up = r.uptime("a").unwrap();
        assert!(up >= Duration::from_millis(30) && up < Duration::from_secs(5));
        r.stop("a");
        assert_eq!(r.uptime("a"), None);
    }

    /// A controller with two saved tunnels ("db" and "web") on one machine.
    fn two_tunnels(tag: &str) -> (TunnelController, String, String) {
        let mut c = TunnelController::spawn(dir(tag), None)
            .with_settle(Duration::from_millis(10));
        c.select_machine(&uuid::Uuid::new_v4().to_string(), "prod");
        c.save(&draft("db", 5432));
        c.save(&draft("web", 8080));
        assert!(pump(&mut c, |c| c.state().items.len() == 2));
        let id = |name: &str| {
            c.state()
                .items
                .iter()
                .find(|t| t.name == name)
                .unwrap()
                .id
                .clone()
        };
        let (db, web) = (id("db"), id("web"));
        (c, db, web)
    }

    #[test]
    fn a_running_tunnel_carries_uptime_and_a_connection_sample() {
        let (mut c, db, web) = two_tunnels("stats");
        assert!(c.state().items.iter().all(|t| t.stats.is_none()));
        c.start_tunnel("db", &mut |_| Ok(sh("sleep 30"))).unwrap();
        assert!(pump(&mut c, |c| c
            .state()
            .items
            .iter()
            .any(|t| t.stats.is_some())));
        let by = |c: &TunnelController, id: &str| {
            c.state().items.iter().find(|t| t.id == id).unwrap().clone()
        };
        let stats = by(&c, &db).stats.unwrap();
        assert!(stats.uptime_secs < 5);
        // `sh` owns no socket: 0 on Linux, unknown elsewhere.
        let expected = if cfg!(target_os = "linux") {
            Some(0)
        } else {
            None
        };
        assert_eq!(stats.connections, expected);
        assert!(by(&c, &web).stats.is_none(), "stopped tunnels have none");
        c.stop_tunnel("db").unwrap();
        assert!(by(&c, &db).stats.is_none(), "stats end with the process");
    }

    #[test]
    fn start_and_stop_resolve_ids_and_names() {
        let (mut c, db, _web) = two_tunnels("api");
        c.start_tunnel(&db, &mut |_| Ok(sh("sleep 30"))).unwrap();
        assert!(c.registry.is_active(&db));
        let pid = c.registry.pid(&db);
        // Starting a running tunnel keeps the same process.
        c.start_tunnel("DB", &mut |_| unreachable!("already running"))
            .unwrap();
        assert_eq!(c.registry.pid(&db), pid);
        c.stop_tunnel("db").unwrap();
        assert!(!c.registry.is_active(&db));
        assert_eq!(c.state().items[0].status, TunnelStatus::Stopped);
        // Stopping a stopped tunnel is fine; unknown keys are not.
        c.stop_tunnel(&db).unwrap();
        assert!(c.stop_tunnel("nope").is_err());
        assert!(c.start_tunnel("nope", &mut |_| unreachable!()).is_err());
    }

    #[test]
    fn a_failed_start_reports_the_error_and_marks_the_card() {
        let (mut c, db, _web) = two_tunnels("fail");
        let err = c
            .start_tunnel("db", &mut |_| Err("no ssh".to_string()))
            .unwrap_err();
        assert_eq!(err, "no ssh");
        let t = c.state().items.iter().find(|t| t.id == db).unwrap();
        assert_eq!(t.status, TunnelStatus::Failed);
        assert!(c.take_notices().is_empty(), "the caller shows the Err");
    }

    #[test]
    fn an_ambiguous_name_is_refused() {
        let mut c = TunnelController::spawn(dir("ambig"), None)
            .with_settle(Duration::from_millis(10));
        c.select_machine(&uuid::Uuid::new_v4().to_string(), "prod");
        c.save(&draft("db", 5432));
        c.save(&draft("db", 5433));
        assert!(pump(&mut c, |c| c.state().items.len() == 2));
        let err = c.start_tunnel("db", &mut |_| unreachable!()).unwrap_err();
        assert!(err.contains("More than one"), "{err}");
        let id = c.state().items[0].id.clone();
        assert_eq!(c.resolve_current(&id).as_deref(), Ok(id.as_str()));
    }

    #[test]
    fn an_active_tunnel_wakes_the_ui_about_once_a_second() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let wakes = Arc::new(AtomicUsize::new(0));
        let w = wakes.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        });
        let mut c = TunnelController::spawn(dir("wake"), Some(wake))
            .with_settle(Duration::from_millis(10));
        c.select_machine(&uuid::Uuid::new_v4().to_string(), "prod");
        c.save(&draft("db", 5432));
        assert!(pump(&mut c, |c| c.state().items.len() == 1));
        std::thread::sleep(Duration::from_millis(100));
        let before = wakes.load(Ordering::SeqCst);
        c.start_tunnel("db", &mut |_| Ok(sh("sleep 30"))).unwrap();
        c.tick();
        let end = Instant::now() + Duration::from_millis(2500);
        while Instant::now() < end && wakes.load(Ordering::SeqCst) == before {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            wakes.load(Ordering::SeqCst) > before,
            "no tick while running"
        );
        // Idle again once nothing runs: the ticker stops waking.
        c.stop_tunnel("db").unwrap();
        c.tick();
        std::thread::sleep(Duration::from_millis(1300));
        let settled = wakes.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(1300));
        assert_eq!(wakes.load(Ordering::SeqCst), settled, "woke with no tunnel");
    }

    #[test]
    fn stopping_a_name_shared_by_idle_tunnels_of_two_machines_is_a_no_op() {
        let mut c = TunnelController::spawn(dir("shared"), None)
            .with_settle(Duration::from_millis(10));
        for label in ["a", "b"] {
            c.select_machine(&uuid::Uuid::new_v4().to_string(), label);
            c.save(&draft("db", 5432));
            assert!(pump(&mut c, |c| c.state().items.len() == 1));
        }
        assert_eq!(c.stop_tunnel("db"), Ok(()));
    }
}
