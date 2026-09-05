use crate::error::{Error, Result};
use crate::models::{ColorTheme, Host, Identity, SessionInfo};
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
    store: Store,
    sink: Arc<dyn OutputSink>,
}

impl SessionManager {
    pub fn new(store: Store, sink: Arc<dyn OutputSink>) -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            store,
            sink,
        })
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        self.sessions
            .iter()
            .map(|s| s.info.clone())
            .collect()
    }

    pub async fn open_local(self: &Arc<Self>, cols: u16, rows: u16) -> Result<SessionInfo> {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let pty = LocalPty::spawn(cols, rows, tx)?;
        let emulator = Arc::new(parking_lot::Mutex::new(self.open_emulator(cols, rows).await?));
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

    pub async fn open_ssh(
        self: &Arc<Self>,
        host_id: &str,
        cols: u16,
        rows: u16,
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
        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let cmd_tx = ssh::open_shell(&host, identity.as_ref(), cols, rows, tx).await?;
        let emulator = Arc::new(parking_lot::Mutex::new(self.open_emulator(cols, rows).await?));
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
        self.spawn_reader(id, rx);
        Ok(info)
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

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        let Some(session) = self.sessions.get(session_id) else {
            return Err(Error::SessionNotFound(session_id.into()));
        };
        session.emulator.lock().resize(cols, rows);
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

    async fn open_emulator(&self, cols: u16, rows: u16) -> Result<TerminalEmulator> {
        let appearance = self.store.appearance().await.ok().unwrap_or_default();
        TerminalEmulator::new_with_style(cols, rows, appearance.font_size, appearance.line_height)
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

    pub fn cell_size(&self, session_id: &str) -> Result<(u32, u32)> {
        let Some(session) = self.sessions.get(session_id) else {
            return Err(Error::SessionNotFound(session_id.into()));
        };
        let size = session.emulator.lock().cell_size();
        Ok(size)
    }

    pub fn close(&self, session_id: &str) -> Result<()> {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            match session.backend {
                Backend::Local(pty) => pty.kill()?,
                Backend::Ssh(tx) => {
                    let _ = tx.send(SshCommand::Close);
                }
            }
        }
        Ok(())
    }

    fn spawn_reader(self: &Arc<Self>, id: String, mut rx: mpsc::UnboundedReceiver<Vec<u8>>) {
        let sink = self.sink.clone();
        let sessions = self.clone();
        tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                let mut buf = chunk;
                while let Ok(more) = rx.try_recv() {
                    buf.extend_from_slice(&more);
                    if buf.len() >= 256 * 1024 {
                        break;
                    }
                }
                if let Some(session) = sessions.sessions.get(&id) {
                    session.emulator.lock().feed(&buf);
                }
                sink.emit_output(&id, &buf).await;
            }
            sink.emit_exit(&id).await;
            sessions.sessions.remove(&id);
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
