use crate::error::{Error, Result};
use crate::models::{ColorTheme, Host, HostRuntime, Identity, SessionInfo};
use crate::pty::LocalPty;
use crate::ssh::{self, SshCommand};
use crate::store::Store;
use crate::term::{pack_frame, TerminalEmulator};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

#[async_trait::async_trait]
pub trait OutputSink: Send + Sync {
    async fn emit_output(&self, session_id: &str, data: &[u8]);
    async fn emit_exit(&self, session_id: &str);
    /// Called when a session backend reports a failure (e.g. local PTY reader).
    /// Default is a no-op so existing sinks keep compiling; override to surface errors.
    async fn emit_error(&self, _session_id: &str, _message: &str) {}
    async fn emit_hosts_changed(&self) {}
    /// Host connection/runtime snapshot changed (`hosts://runtime` on the UI side).
    async fn emit_host_runtime(&self, _runtime: &HostRuntime) {}
}

/// Why an SSH connect attempt failed — drives sticky sidebar state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectFailKind {
    /// TOFU / known_hosts unknown or mismatch — never sticky-error.
    HostKey,
    /// Auth, transport, or other failure — sticky `error` when idle.
    Other,
}

enum Backend {
    Local(Arc<LocalPty>),
    Ssh(mpsc::UnboundedSender<SshCommand>),
}

struct LiveSession {
    info: SessionInfo,
    backend: Backend,
    input_buf: parking_lot::Mutex<String>,
    emulator: Arc<parking_lot::Mutex<TerminalEmulator>>,
}

pub struct SessionManager {
    sessions: DashMap<String, LiveSession>,
    ssh_connections: DashMap<String, String>,
    /// In-flight `open_ssh` attempts per host (Architect aggregation).
    ssh_inflight: DashMap<String, usize>,
    /// Last reader/backend failure per session id (visible to the session layer).
    session_errors: DashMap<String, String>,
    store: Store,
    sink: Arc<dyn OutputSink>,
    os_probe_tried: DashMap<String, ()>,
}

impl SessionManager {
    pub fn new(store: Store, sink: Arc<dyn OutputSink>) -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            ssh_connections: DashMap::new(),
            ssh_inflight: DashMap::new(),
            session_errors: DashMap::new(),
            store,
            sink,
            os_probe_tried: DashMap::new(),
        })
    }

    /// Take the last recorded backend/reader error for a session, if any.
    pub fn take_session_error(&self, session_id: &str) -> Option<String> {
        self.session_errors.remove(session_id).map(|(_, err)| err)
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions
            .iter()
            .map(|s| s.info.clone())
            .collect()
    }

    fn open_count_for(&self, host_id: &str) -> usize {
        self.sessions
            .iter()
            .filter(|s| s.info.host_id.as_deref() == Some(host_id))
            .count()
    }

    fn inflight_for(&self, host_id: &str) -> usize {
        self.ssh_inflight
            .get(host_id)
            .map(|e| *e.value())
            .unwrap_or(0)
    }

    fn tracked_for(&self, host_id: &str) -> String {
        self.ssh_connections
            .get(host_id)
            .map(|e| e.value().clone())
            .unwrap_or_else(|| "disconnected".to_string())
    }

    fn aggregate_connection(open_count: usize, inflight: usize, tracked: &str) -> String {
        if inflight > 0 {
            return "connecting".to_string();
        }
        if open_count > 0 {
            return "connected".to_string();
        }
        match tracked {
            "error" => "error".to_string(),
            "connecting" => "connecting".to_string(),
            "connected" => "connected".to_string(),
            _ => "disconnected".to_string(),
        }
    }

    fn host_runtime_snapshot(&self, host_id: &str) -> HostRuntime {
        let open_count = self.open_count_for(host_id);
        let inflight = self.inflight_for(host_id);
        let tracked = self.tracked_for(host_id);
        HostRuntime {
            host_id: host_id.to_string(),
            connection: Self::aggregate_connection(open_count, inflight, &tracked),
            open_count,
        }
    }

    fn emit_runtime_now(&self, host_id: &str) {
        let runtime = self.host_runtime_snapshot(host_id);
        // Emit futures are immediate (mutex push / Tauri emit); block_on is safe here.
        futures::executor::block_on(self.sink.emit_host_runtime(&runtime));
    }

    /// Start an SSH connect attempt: bump inflight, set `connecting`, emit runtime.
    pub fn begin_ssh_connect(&self, host_id: &str) {
        let next = self.inflight_for(host_id).saturating_add(1);
        self.ssh_inflight.insert(host_id.to_string(), next);
        self.ssh_connections
            .insert(host_id.to_string(), "connecting".to_string());
        self.emit_runtime_now(host_id);
    }

    fn dec_inflight(&self, host_id: &str) {
        let next = self.inflight_for(host_id).saturating_sub(1);
        if next == 0 {
            self.ssh_inflight.remove(host_id);
        } else {
            self.ssh_inflight.insert(host_id.to_string(), next);
        }
    }

    /// Connect succeeded — clear inflight, mark `connected`, emit.
    pub fn end_ssh_connect_ok(&self, host_id: &str) {
        self.dec_inflight(host_id);
        self.ssh_connections
            .insert(host_id.to_string(), "connected".to_string());
        self.emit_runtime_now(host_id);
    }

    /// Connect failed — classify HostKey vs Other; preserve live shells.
    pub fn end_ssh_connect_err(&self, host_id: &str, kind: ConnectFailKind) {
        self.dec_inflight(host_id);
        let open_count = self.open_count_for(host_id);
        let inflight = self.inflight_for(host_id);
        let state = if inflight > 0 {
            "connecting"
        } else if open_count > 0 {
            "connected"
        } else {
            match kind {
                ConnectFailKind::HostKey => "disconnected",
                ConnectFailKind::Other => "error",
            }
        };
        self.ssh_connections
            .insert(host_id.to_string(), state.to_string());
        self.emit_runtime_now(host_id);
    }

    /// Compute runtime state for all hosts by joining hosts from the store with active sessions.
    /// Returns a HostRuntime for each host, plus one for the local "This computer" entry.
    ///
    /// # Connection State Logic
    ///
    /// - Local sessions (host_id == None) → connection = "local"
    /// - `inflight > 0` → `connecting`
    /// - SSH shells open → `connected`
    /// - Idle sticky `error` preserved; HostKey failures → `disconnected`
    pub async fn hosts_runtime(&self) -> Result<Vec<HostRuntime>> {
        let hosts = self.store.list_hosts().await?;

        let mut open_counts = std::collections::HashMap::<String, usize>::new();
        let mut local_count = 0usize;

        for session in self.sessions.iter() {
            match &session.info.host_id {
                Some(host_id) => {
                    *open_counts.entry(host_id.clone()).or_insert(0) += 1;
                }
                None => {
                    local_count += 1;
                }
            }
        }

        let mut runtimes = Vec::new();

        if local_count > 0 {
            runtimes.push(HostRuntime {
                host_id: "local".to_string(),
                connection: "local".to_string(),
                open_count: local_count,
            });
        }

        for host in hosts {
            if host.deleted_at.is_some() {
                continue;
            }

            let open_count = open_counts.get(&host.id).copied().unwrap_or(0);
            let inflight = self.inflight_for(&host.id);
            let tracked = self.tracked_for(&host.id);
            let connection = Self::aggregate_connection(open_count, inflight, &tracked);

            runtimes.push(HostRuntime {
                host_id: host.id.clone(),
                connection,
                open_count,
            });
        }

        // WSL sessions use synthetic `wsl:<distro>` host ids (not SQLite hosts).
        for (host_id, open_count) in open_counts {
            if !crate::wsl::is_wsl_host_id(&host_id) {
                continue;
            }
            runtimes.push(HostRuntime {
                host_id,
                connection: "local".to_string(),
                open_count,
            });
        }

        Ok(runtimes)
    }

    pub async fn open_local(
        self: &Arc<Self>,
        cols: u16,
        rows: u16,
        scale: f32,
    ) -> Result<SessionInfo> {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel::<Result<Vec<u8>>>();
        let pty = LocalPty::spawn(cols, rows, tx)?;
        let emulator = Arc::new(parking_lot::Mutex::new(
            self.open_emulator(cols, rows, scale).await?,
        ));
        let info = SessionInfo {
            id: id.clone(),
            title: "local".into(),
            kind: "local".into(),
            host_id: None,
        };
        self.sessions.insert(
            id.clone(),
            LiveSession {
                info: info.clone(),
                backend: Backend::Local(pty),
                input_buf: parking_lot::Mutex::new(String::new()),
                emulator,
            },
        );
        self.spawn_reader(id, rx);
        Ok(info)
    }

    /// Open a WSL distro shell via `wsl.exe -d <distro> --cd ~` on a local PTY (#82 / #84).
    pub async fn open_wsl(
        self: &Arc<Self>,
        distro: &str,
        cols: u16,
        rows: u16,
        scale: f32,
    ) -> Result<SessionInfo> {
        let distro = distro.trim();
        if distro.is_empty()
            || distro.contains('\0')
            || distro.contains('\n')
            || distro.contains('\r')
        {
            return Err(Error::msg("invalid WSL distro name"));
        }
        let host_id = crate::wsl::wsl_host_id(distro);
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel::<Result<Vec<u8>>>();
        let args = crate::wsl::shell_args(distro);
        let pty = tokio::task::spawn_blocking(move || {
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            // No Windows cwd — `--cd ~` selects the Linux home inside the distro.
            LocalPty::spawn_program_with_cwd(Some("wsl.exe"), &arg_refs, cols, rows, tx, None)
        })
        .await
        .map_err(|err| Error::msg(format!("WSL spawn join failed: {err}")))??;
        let emulator = Arc::new(parking_lot::Mutex::new(
            self.open_emulator(cols, rows, scale).await?,
        ));
        let info = SessionInfo {
            id: id.clone(),
            title: distro.to_string(),
            kind: "wsl".into(),
            host_id: Some(host_id),
        };
        self.sessions.insert(
            id.clone(),
            LiveSession {
                info: info.clone(),
                backend: Backend::Local(pty),
                input_buf: parking_lot::Mutex::new(String::new()),
                emulator,
            },
        );
        self.spawn_reader(id, rx);
        Ok(info)
    }

    pub async fn open_ssh(
        self: &Arc<Self>,
        host_id: &str,
        cols: u16,
        rows: u16,
        scale: f32,
    ) -> Result<SessionInfo> {
        let host = self
            .store
            .get_host(host_id)
            .await?
            .ok_or_else(|| Error::msg("host not found"))?;
        let identity = match &host.identity_id {
            Some(id) => self.store.get_identity(id).await?,
            None => None,
        };
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel::<Result<Vec<u8>>>();
        self.begin_ssh_connect(host_id);
        let cmd_tx = match ssh::open_shell(&host, identity.as_ref(), cols, rows, tx).await {
            Ok(tx) => tx,
            Err(e) => {
                let kind = match &e {
                    Error::HostKeyUnknown { .. } | Error::HostKeyMismatch { .. } => {
                        ConnectFailKind::HostKey
                    }
                    _ => ConnectFailKind::Other,
                };
                self.end_ssh_connect_err(host_id, kind);
                return Err(e);
            }
        };
        let emulator = Arc::new(parking_lot::Mutex::new(
            self.open_emulator(cols, rows, scale).await?,
        ));
        let info = SessionInfo {
            id: id.clone(),
            title: host.name.clone(),
            kind: "ssh".into(),
            host_id: Some(host.id.clone()),
        };
        self.sessions.insert(
            id.clone(),
            LiveSession {
                info: info.clone(),
                backend: Backend::Ssh(cmd_tx),
                input_buf: parking_lot::Mutex::new(String::new()),
                emulator,
            },
        );

        self.end_ssh_connect_ok(&host.id);

        self.spawn_reader(id, rx);
        self.spawn_os_probe(host.id.clone());
        Ok(info)
    }

    /// A successful SFTP session is SSH too — mark the host connected even
    /// when no shell tab is open.
    pub fn mark_ssh_connected(&self, host_id: &str) {
        self.ssh_connections
            .insert(host_id.to_string(), "connected".to_string());
        self.emit_runtime_now(host_id);
    }

    /// Probe `/etc/os-release` in the background the first time we reach a host.
    pub fn spawn_os_probe(self: &Arc<Self>, host_id: String) {
        if self.os_probe_tried.contains_key(&host_id) {
            return;
        }
        self.os_probe_tried.insert(host_id.clone(), ());
        let mgr = Arc::clone(self);
        tokio::spawn(async move {
            let _ = mgr.probe_host_os(&host_id).await;
        });
    }

    async fn probe_host_os(&self, host_id: &str) -> Result<()> {
        let host = match self.store.get_host(host_id).await? {
            Some(h) => h,
            None => return Ok(()),
        };
        if host.os_id.is_some() {
            return Ok(());
        }
        let identity = match &host.identity_id {
            Some(id) => self.store.get_identity(id).await?,
            None => None,
        };
        let os_id = ssh::detect_os(&host, identity.as_ref()).await?;
        if os_id.is_empty() || os_id == "unknown" {
            return Ok(());
        }
        let mut host = host;
        host.os_id = Some(os_id);
        host.updated_at = chrono::Utc::now();
        self.store.upsert_host(&host).await?;
        self.sink.emit_hosts_changed().await;
        Ok(())
    }

    pub async fn write(&self, session_id: &str, data: &[u8]) -> Result<()> {
        let (kind, host_id, command) = {
            let Some(session) = self.sessions.get(session_id) else {
                return Err(Error::SessionNotFound(session_id.into()));
            };
            let command = feed_history(&session, data);
            match &session.backend {
                Backend::Local(pty) => pty.write(data)?,
                Backend::Ssh(tx) => {
                    tx.send(SshCommand::Data(data.to_vec()))
                        .map_err(|_| Error::msg("ssh session closed"))?;
                }
            }
            (session.info.kind.clone(), session.info.host_id.clone(), command)
        };
        if let Some(command) = command {
            let mut entry = crate::models::HistoryEntry::new(command, kind);
            entry.host_id = host_id;
            self.store.add_history(&entry).await?;
        }
        Ok(())
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16, scale: Option<f32>) -> Result<()> {
        let Some(session) = self.sessions.get(session_id) else {
            return Err(Error::SessionNotFound(session_id.into()));
        };
        {
            let mut emu = session.emulator.lock();
            if let Some(scale) = scale {
                emu.set_scale(scale);
            }
            emu.resize(cols, rows);
        }
        match &session.backend {
            Backend::Local(pty) => pty.resize(cols, rows)?,
            Backend::Ssh(tx) => {
                tx.send(SshCommand::Resize { cols, rows })
                    .map_err(|_| Error::msg("ssh session closed"))?;
            }
        }
        Ok(())
    }

    pub fn apply_theme(&self, session_id: &str, theme: &ColorTheme) -> Result<()> {
        let Some(session) = self.sessions.get(session_id) else {
            return Err(Error::SessionNotFound(session_id.into()));
        };
        session.emulator.lock().apply_theme(theme);
        Ok(())
    }

    pub fn apply_theme_all(&self, theme: &ColorTheme) {
        for session in self.sessions.iter() {
            session.emulator.lock().apply_theme(theme);
        }
    }

    pub fn apply_style_all(&self, font_px: f32, line_height: f32) {
        for session in self.sessions.iter() {
            session.emulator.lock().set_style(font_px, line_height);
        }
    }

    async fn open_emulator(&self, cols: u16, rows: u16, scale: f32) -> Result<TerminalEmulator> {
        let appearance = self.store.appearance().await.ok().unwrap_or_default();
        TerminalEmulator::new_with_scale(
            cols,
            rows,
            appearance.font_size,
            appearance.line_height,
            scale,
        )
    }

    pub fn take_frame(&self, session_id: &str) -> Result<Option<Vec<u8>>> {
        self.frame(session_id, false)
    }

    pub fn frame(&self, session_id: &str, force: bool) -> Result<Option<Vec<u8>>> {
        let Some(session) = self.sessions.get(session_id) else {
            return Err(Error::SessionNotFound(session_id.into()));
        };
        let packed = {
            let mut emu = session.emulator.lock();
            let (cw, ch) = emu.cell_size();
            emu.capture_frame(force)
                .map(|frame| pack_frame(&frame, cw, ch))
        };
        Ok(packed)
    }

    pub fn extract_text(&self, session_id: &str, r0: u16, c0: u16, r1: u16, c1: u16) -> Result<String> {
        let Some(session) = self.sessions.get(session_id) else {
            return Err(Error::SessionNotFound(session_id.into()));
        };
        let text = session.emulator.lock().extract_text(r0, c0, r1, c1);
        Ok(text)
    }

    pub fn cell_size(&self, session_id: &str) -> Result<(u32, u32)> {
        let Some(session) = self.sessions.get(session_id) else {
            return Err(Error::SessionNotFound(session_id.into()));
        };
        let size = session.emulator.lock().cell_size();
        Ok(size)
    }

    pub fn close(&self, session_id: &str) -> Result<()> {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            let host_id = session.info.host_id.clone();
            match session.backend {
                Backend::Local(pty) => {
                    // Always drop the session from the map; a kill race on an
                    // already-dead PTY must not leave the shell "open".
                    let _ = pty.kill();
                }
                Backend::Ssh(tx) => {
                    let _ = tx.send(SshCommand::Close);
                }
            }
            self.clear_connection_if_idle(host_id.as_deref());
        }
        Ok(())
    }

    /// When the last shell for a host is gone, drop the sticky "connected" flag.
    fn clear_connection_if_idle(&self, host_id: Option<&str>) {
        let Some(host_id) = host_id else { return };
        let still_open = self
            .sessions
            .iter()
            .any(|s| s.info.host_id.as_deref() == Some(host_id));
        if still_open {
            return;
        }
        if let Some(entry) = self.ssh_connections.get(host_id) {
            let state = entry.value().clone();
            drop(entry);
            if state == "connected" || state == "connecting" {
                self.ssh_connections
                    .insert(host_id.to_string(), "disconnected".to_string());
                self.emit_runtime_now(host_id);
            }
        }
    }

    /// Test helper: retag an open local session as a WSL session (no `wsl.exe` required).
    #[cfg(test)]
    pub fn test_retag_as_wsl(&self, session_id: &str, distro: &str) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.info.kind = "wsl".into();
            session.info.title = distro.to_string();
            session.info.host_id = Some(crate::wsl::wsl_host_id(distro));
        }
    }

    /// Test/E2E helper: force connection state. Available in debug unit tests
    /// and when built with `TERMINUS_E2E=1` (release CI Playwright).
    #[cfg(any(debug_assertions, terminus_e2e))]
    pub fn test_set_connection(&self, host_id: &str, state: &str) -> Result<()> {
        let valid_states = ["local", "connected", "disconnected", "connecting", "error"];
        if !valid_states.contains(&state) {
            return Err(Error::msg(&format!(
                "Invalid connection state: {}. Must be one of: {}",
                state,
                valid_states.join(", ")
            )));
        }
        self.ssh_connections.insert(host_id.to_string(), state.to_string());
        Ok(())
    }

    fn spawn_reader(
        self: &Arc<Self>,
        id: String,
        mut rx: mpsc::UnboundedReceiver<Result<Vec<u8>>>,
    ) {
        let sink = self.sink.clone();
        let sessions = self.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let mut buf = Vec::new();
                let mut reader_err = None;
                match msg {
                    Ok(chunk) => buf = chunk,
                    Err(err) => reader_err = Some(err),
                }
                if reader_err.is_none() {
                    while let Ok(more) = rx.try_recv() {
                        match more {
                            Ok(chunk) => {
                                buf.extend_from_slice(&chunk);
                                if buf.len() >= 256 * 1024 {
                                    break;
                                }
                            }
                            Err(err) => {
                                reader_err = Some(err);
                                break;
                            }
                        }
                    }
                }
                if !buf.is_empty() {
                    let replies = if let Some(session) = sessions.sessions.get(&id) {
                        session.emulator.lock().feed(&buf)
                    } else {
                        Vec::new()
                    };
                    if !replies.is_empty() {
                        if let Some(session) = sessions.sessions.get(&id) {
                            match &session.backend {
                                Backend::Local(pty) => {
                                    if let Err(err) = pty.write(&replies) {
                                        tracing::warn!(
                                            session_id = %id,
                                            error = %err,
                                            "pty write of emulator reply failed"
                                        );
                                    }
                                }
                                Backend::Ssh(tx) => {
                                    if tx.send(SshCommand::Data(replies)).is_err() {
                                        tracing::warn!(
                                            session_id = %id,
                                            "ssh write of emulator reply failed"
                                        );
                                    }
                                }
                            }
                        }
                    }
                    sink.emit_output(&id, &buf).await;
                }
                if let Some(err) = reader_err {
                    let message = err.to_string();
                    tracing::warn!(session_id = %id, error = %message, "session reader failed");
                    sessions.session_errors.insert(id.clone(), message.clone());
                    sink.emit_error(&id, &message).await;
                    break;
                }
            }
            sink.emit_exit(&id).await;
            if let Some((_, session)) = sessions.sessions.remove(&id) {
                sessions.clear_connection_if_idle(session.info.host_id.as_deref());
            }
        });
    }
}

fn feed_history(session: &LiveSession, data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    let mut buf = session.input_buf.lock();
    let mut command = None;
    for ch in text.chars() {
        match ch {
            '\r' | '\n' => {
                let next = buf.trim().to_string();
                buf.clear();
                if !next.is_empty() && !next.starts_with('\u{1b}') {
                    command = Some(next);
                }
            }
            '\u{7f}' | '\u{08}' => {
                buf.pop();
            }
            c if !c.is_control() => buf.push(c),
            _ => {}
        }
    }
    command
}

pub async fn resolve_identity(store: &Store, host: &Host) -> Result<Option<Identity>> {
    match &host.identity_id {
        Some(id) => store.get_identity(id).await,
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct TestSink {
        errors: parking_lot::Mutex<Vec<(String, String)>>,
        runtimes: parking_lot::Mutex<Vec<HostRuntime>>,
    }

    impl TestSink {
        fn new() -> Self {
            Self {
                errors: parking_lot::Mutex::new(Vec::new()),
                runtimes: parking_lot::Mutex::new(Vec::new()),
            }
        }

        fn runtime_emits(&self) -> Vec<HostRuntime> {
            self.runtimes.lock().clone()
        }

        fn last_runtime_for(&self, host_id: &str) -> Option<HostRuntime> {
            self.runtimes
                .lock()
                .iter()
                .rev()
                .find(|r| r.host_id == host_id)
                .cloned()
        }
    }

    #[async_trait::async_trait]
    impl OutputSink for TestSink {
        async fn emit_output(&self, _session_id: &str, _data: &[u8]) {}
        async fn emit_exit(&self, _session_id: &str) {}
        async fn emit_error(&self, session_id: &str, message: &str) {
            self.errors
                .lock()
                .push((session_id.to_string(), message.to_string()));
        }
        async fn emit_host_runtime(&self, runtime: &HostRuntime) {
            self.runtimes.lock().push(runtime.clone());
        }
    }

    async fn test_manager() -> (
        Arc<SessionManager>,
        Arc<TestSink>,
        Store,
        tempfile::NamedTempFile,
    ) {
        let temp_db = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(temp_db.path()).await.unwrap();
        let sink = Arc::new(TestSink::new());
        let manager = SessionManager::new(store.clone(), sink.clone());
        (manager, sink, store, temp_db)
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_set_connection_with_zero_open_count() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, _sink, store, _tmp) = test_manager().await;
            let mut host = Host::new("test", "127.0.0.1", 22, "user");
            host.id = "test-host-123".into();
            store.upsert_host(&host).await.unwrap();

            manager
                .test_set_connection(&host.id, "connected")
                .unwrap();

            let runtime = manager.hosts_runtime().await.unwrap();
            let host_runtime = runtime.iter().find(|r| r.host_id == host.id);
            assert!(host_runtime.is_some(), "Host runtime should exist");
            let host_runtime = host_runtime.unwrap();
            // Tracked `connected` is visible even with zero shells (SFTP / post-handshake).
            assert_eq!(host_runtime.connection, "connected");
            assert_eq!(
                host_runtime.open_count, 0,
                "Open count should be 0 when no sessions are open"
            );

            manager.test_set_connection(&host.id, "error").unwrap();
            let runtime = manager.hosts_runtime().await.unwrap();
            let host_runtime = runtime.iter().find(|r| r.host_id == host.id).unwrap();
            assert_eq!(host_runtime.connection, "error");
        });
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_last_shell_clears_connection_state() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, _sink, store, _tmp) = test_manager().await;
            let mut host = Host::new("test", "127.0.0.1", 22, "user");
            host.id = "test-host-456".into();
            store.upsert_host(&host).await.unwrap();

            manager
                .test_set_connection(&host.id, "connected")
                .unwrap();

            let runtime_before = manager.hosts_runtime().await.unwrap();
            let host_before = runtime_before.iter().find(|r| r.host_id == host.id);
            assert!(host_before.is_some());
            // Sticky "connected" with zero shells is visible until clear_connection_if_idle.
            assert_eq!(
                host_before.unwrap().connection, "connected",
                "Tracked connected remains visible with no open shells"
            );

            // Simulate open_count via a local-only path: force connected with a fake
            // session is hard without SSH; instead verify clear_connection_if_idle.
            manager
                .test_set_connection(&host.id, "connected")
                .unwrap();
            manager.clear_connection_if_idle(Some(&host.id));

            let runtime_after = manager.hosts_runtime().await.unwrap();
            let host_after = runtime_after.iter().find(|r| r.host_id == host.id);
            assert!(host_after.is_some());
            let host_after = host_after.unwrap();
            assert_eq!(
                host_after.connection, "disconnected",
                "Closing last shell should clear the green connected state"
            );
            assert_eq!(host_after.open_count, 0, "Open count should be 0 after close");
        });
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_set_connection_validates_state() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, _sink, _store, _tmp) = test_manager().await;
            let result = manager.test_set_connection("test-host", "invalid_state");
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Invalid connection state"));
        });
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_valid_connection_states() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, _sink, _store, _tmp) = test_manager().await;
            for state in &["local", "connected", "disconnected", "connecting", "error"] {
                let result = manager.test_set_connection("test-host", state);
                assert!(result.is_ok(), "State '{state}' should be valid");
            }
        });
    }

    #[test]
    fn local_close_propagates_pty_kill_ok() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, _sink, _store, _tmp) = test_manager().await;
            let info = manager.open_local(80, 24, 1.0).await.expect("open local");
            manager.close(&info.id).expect("close should propagate kill Result");
        });
    }

    #[test]
    fn hosts_runtime_includes_wsl_open_sessions() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, _sink, _store, _tmp) = test_manager().await;
            let info = manager.open_local(80, 24, 1.0).await.expect("open local");
            manager.test_retag_as_wsl(&info.id, "Ubuntu");
            let runtime = manager.hosts_runtime().await.unwrap();
            let wsl = runtime.iter().find(|r| r.host_id == "wsl:Ubuntu");
            assert!(wsl.is_some(), "expected wsl:Ubuntu runtime, got {runtime:?}");
            let wsl = wsl.unwrap();
            assert_eq!(wsl.connection, "local");
            assert_eq!(wsl.open_count, 1);
            // True local must not count the retagged WSL session.
            assert!(
                !runtime.iter().any(|r| r.host_id == "local" && r.open_count > 0),
                "local aggregate should exclude WSL host_id sessions"
            );
            manager.close(&info.id).ok();
        });
    }

    #[test]
    fn open_wsl_rejects_empty_distro() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, _sink, _store, _tmp) = test_manager().await;
            let err = manager.open_wsl("", 80, 24, 1.0).await.unwrap_err();
            assert!(err.to_string().contains("invalid"));
        });
    }

    #[test]
    fn reader_error_surfaces_to_session_layer() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, sink, _store, _tmp) = test_manager().await;
            let (tx, rx) = mpsc::unbounded_channel::<Result<Vec<u8>>>();
            let id = "reader-err-session".to_string();
            manager.spawn_reader(id.clone(), rx);

            tx.send(Err(Error::PtyReader("synthetic read failure".into())))
                .unwrap();
            drop(tx);

            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let mut recorded = None;
            while std::time::Instant::now() < deadline {
                if let Some(err) = manager.take_session_error(&id) {
                    recorded = Some(err);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            let recorded = recorded.expect("session layer should record reader error");
            assert!(recorded.contains("synthetic read failure"));

            let errors = sink.errors.lock().clone();
            assert!(
                errors.iter().any(|(sid, msg)| sid == &id && msg.contains("synthetic")),
                "OutputSink.emit_error should see the failure: {errors:?}"
            );
        });
    }

    /// AC1 — begin_ssh_connect must expose `connecting` and emit `hosts://runtime`.
    #[test]
    fn ac1_begin_ssh_connect_emits_connecting_runtime() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, sink, store, _tmp) = test_manager().await;
            let mut host = Host::new("ac1", "127.0.0.1", 22, "user");
            host.id = "host-ac1-connecting".into();
            store.upsert_host(&host).await.unwrap();

            manager.begin_ssh_connect(&host.id);

            let runtime = manager.hosts_runtime().await.unwrap();
            let host_rt = runtime
                .iter()
                .find(|r| r.host_id == host.id)
                .expect("host runtime present");
            assert_eq!(
                host_rt.connection, "connecting",
                "inflight connect must surface connecting (AC1)"
            );

            let emitted = sink.last_runtime_for(&host.id);
            assert!(
                emitted.is_some(),
                "begin_ssh_connect must emit_host_runtime (AC1); got {:?}",
                sink.runtime_emits()
            );
            assert_eq!(emitted.unwrap().connection, "connecting");
        });
    }

    /// AC2 — transport/auth failure with no shells → sticky `error` + emit.
    #[test]
    fn ac2_end_ssh_connect_other_sets_sticky_error_and_emits() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, sink, store, _tmp) = test_manager().await;
            let mut host = Host::new("ac2", "127.0.0.1", 22, "user");
            host.id = "host-ac2-error".into();
            store.upsert_host(&host).await.unwrap();

            manager.begin_ssh_connect(&host.id);
            manager.end_ssh_connect_err(&host.id, ConnectFailKind::Other);

            let runtime = manager.hosts_runtime().await.unwrap();
            let host_rt = runtime
                .iter()
                .find(|r| r.host_id == host.id)
                .expect("host runtime present");
            assert_eq!(
                host_rt.connection, "error",
                "auth/transport failure must sticky-error when idle (AC2)"
            );

            let emitted = sink.last_runtime_for(&host.id);
            assert!(
                emitted.is_some(),
                "end_ssh_connect_err must emit_host_runtime (AC2); got {:?}",
                sink.runtime_emits()
            );
            assert_eq!(emitted.unwrap().connection, "error");
        });
    }

    /// AC3 — sticky error cleared by a successful connect.
    #[test]
    fn ac3_success_after_error_becomes_connected_and_emits() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, sink, store, _tmp) = test_manager().await;
            let mut host = Host::new("ac3", "127.0.0.1", 22, "user");
            host.id = "host-ac3-recover".into();
            store.upsert_host(&host).await.unwrap();

            manager.begin_ssh_connect(&host.id);
            manager.end_ssh_connect_err(&host.id, ConnectFailKind::Other);
            assert_eq!(
                manager
                    .hosts_runtime()
                    .await
                    .unwrap()
                    .iter()
                    .find(|r| r.host_id == host.id)
                    .unwrap()
                    .connection,
                "error"
            );

            manager.begin_ssh_connect(&host.id);
            let mid = manager
                .hosts_runtime()
                .await
                .unwrap()
                .iter()
                .find(|r| r.host_id == host.id)
                .unwrap()
                .connection
                .clone();
            assert_eq!(
                mid, "connecting",
                "new attempt must clear sticky error via connecting (AC3)"
            );

            manager.end_ssh_connect_ok(&host.id);
            let host_rt = manager
                .hosts_runtime()
                .await
                .unwrap()
                .iter()
                .find(|r| r.host_id == host.id)
                .unwrap()
                .clone();
            assert_eq!(host_rt.connection, "connected");

            let emitted = sink.last_runtime_for(&host.id).expect("emit on success");
            assert_eq!(emitted.connection, "connected");
        });
    }

    /// AC4 — HostKey failure must land on `disconnected`, never sticky `error`.
    #[test]
    fn ac4_host_key_failure_sets_disconnected_not_error() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let (manager, sink, store, _tmp) = test_manager().await;
            let mut host = Host::new("ac4", "127.0.0.1", 22, "user");
            host.id = "host-ac4-hostkey".into();
            store.upsert_host(&host).await.unwrap();

            manager.begin_ssh_connect(&host.id);
            manager.end_ssh_connect_err(&host.id, ConnectFailKind::HostKey);

            let host_rt = manager
                .hosts_runtime()
                .await
                .unwrap()
                .iter()
                .find(|r| r.host_id == host.id)
                .expect("host runtime present")
                .clone();
            assert_eq!(
                host_rt.connection, "disconnected",
                "HostKeyUnknown/Mismatch must not sticky-error (AC4); got {}",
                host_rt.connection
            );
            assert_ne!(host_rt.connection, "error");

            let emitted = sink.last_runtime_for(&host.id).expect("emit on host-key fail");
            assert_eq!(emitted.connection, "disconnected");
        });
    }
}

