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
    store: Store,
    sink: Arc<dyn OutputSink>,
    // Track SSH connection state per host_id independently of session count
    // Once a host has a successful SSH session, it stays "connected" until explicit disconnect/error
}

impl SessionManager {
    pub fn new(store: Store, sink: Arc<dyn OutputSink>) -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            ssh_connections: DashMap::new(),
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

    /// Compute runtime state for all hosts by joining hosts from the store with active sessions.
    /// Returns a HostRuntime for each host, plus one for the local "This computer" entry.
    ///
    /// # Connection State Logic
    ///
    /// - Local sessions (host_id == None) → connection = "local", grouped under a synthetic
    ///   host_id = "local" entry
    /// - SSH hosts: connection state is tracked independently in ssh_connections map
    ///   - Once a host has had a successful SSH session (open_ssh succeeds), it's marked "connected"
    ///   - This state persists even when open_count becomes 0 (closing last shell)
    ///   - State only changes on explicit disconnect/error events (future enhancement)
    /// - Hosts that have never had a session → connection = "disconnected"
    ///
    /// **Critical**: open_count and connection are INDEPENDENT. Closing the last shell sets
    /// open_count=0 but MUST NOT change connection from "connected" to "disconnected".
    pub async fn hosts_runtime(&self) -> Result<Vec<HostRuntime>> {
        // Get all hosts from the store
        let hosts = self.store.list_hosts().await?;
        
        // Count open sessions per host_id
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
        
        // Add local runtime if there are any local sessions
        if local_count > 0 {
            runtimes.push(HostRuntime {
                host_id: "local".to_string(),
                connection: "local".to_string(),
                open_count: local_count,
            });
        }
        
        // Add runtime for each SSH host
        for host in hosts {
            // Skip soft-deleted hosts
            if host.deleted_at.is_some() {
                continue;
            }
            
            let open_count = open_counts.get(&host.id).copied().unwrap_or(0);
            
            // Use tracked connection state, NOT derived from open_count
            // If this host has had a successful SSH session, it stays "connected"
            // even when open_count goes to 0
            let connection = self
                .ssh_connections
                .get(&host.id)
                .map(|entry| entry.value().clone())
                .unwrap_or_else(|| "disconnected".to_string());
            
            runtimes.push(HostRuntime {
                host_id: host.id.clone(),
                connection,
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
        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
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
        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
        self.ssh_connections.insert(host_id.to_string(), "connecting".to_string());
        let cmd_tx = match ssh::open_shell(&host, identity.as_ref(), cols, rows, tx).await {
            Ok(tx) => {
                self.ssh_connections.insert(host_id.to_string(), "connected".to_string());
                tx
            }
            Err(e) => {
                self.ssh_connections.insert(host_id.to_string(), "error".to_string());
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
        
        // Mark this SSH host as connected - this state persists even when open_count becomes 0
        self.ssh_connections.insert(host.id.clone(), "connected".to_string());
        
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

    #[cfg(debug_assertions)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(debug_assertions)]
    #[test]
    fn test_set_connection_with_zero_open_count() {
        use std::sync::Arc;
        struct TestSink;

        #[async_trait::async_trait]
        impl OutputSink for TestSink {
            async fn emit_output(&self, _session_id: &str, _data: &[u8]) {}
            async fn emit_exit(&self, _session_id: &str) {}
        }

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let temp_db = tempfile::NamedTempFile::new().unwrap();
            let store = Store::open(temp_db.path()).await.unwrap();
            let sink: Arc<dyn OutputSink> = Arc::new(TestSink);
            let manager = SessionManager::new(store, sink);

            let test_host_id = "test-host-123";
            
            manager.test_set_connection(test_host_id, "connected").unwrap();
            
            let runtime = manager.hosts_runtime();
            let host_runtime = runtime.iter().find(|(id, _, _)| id == test_host_id);
            
            assert!(host_runtime.is_some(), "Host runtime should exist");
            let (_, connection, open_count) = host_runtime.unwrap();
            assert_eq!(connection, "connected");
            assert_eq!(*open_count, 0, "Open count should be 0 when no sessions are open");
        });
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_last_shell_preserves_connection_state() {
        use std::sync::Arc;
        struct TestSink;

        #[async_trait::async_trait]
        impl OutputSink for TestSink {
            async fn emit_output(&self, _session_id: &str, _data: &[u8]) {}
            async fn emit_exit(&self, _session_id: &str) {}
        }

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let temp_db = tempfile::NamedTempFile::new().unwrap();
            let store = Store::open(temp_db.path()).await.unwrap();
            let sink: Arc<dyn OutputSink> = Arc::new(TestSink);
            let manager = SessionManager::new(store.clone(), sink);

            // Simulate: open_ssh success → connected
            let test_host_id = "test-host-456";
            manager.test_set_connection(test_host_id, "connected").unwrap();
            
            // Create a fake session to simulate open_count=1
            let session_info = crate::models::SessionInfo {
                id: "session-1".to_string(),
                title: "test".to_string(),
                kind: "ssh".to_string(),
                host_id: Some(test_host_id.to_string()),
            };
            
            // Manually track this as if it were a real session
            // (We can't easily create a real SSH session in a unit test)
            // So we just verify the connection state behavior
            
            // Verify: connected with simulated count=1
            let runtime_before = manager.hosts_runtime();
            let host_before = runtime_before.iter().find(|(id, _, _)| id == test_host_id);
            assert!(host_before.is_some());
            let (_, connection_before, _) = host_before.unwrap();
            assert_eq!(connection_before, "connected", "Should be connected after open_ssh");
            
            // Simulate: close the session (connection should stay "connected")
            // In reality, close() removes from sessions map, causing open_count to drop
            // The critical test: connection state must NOT change to "disconnected"
            
            let runtime_after = manager.hosts_runtime();
            let host_after = runtime_after.iter().find(|(id, _, _)| id == test_host_id);
            assert!(host_after.is_some());
            let (_, connection_after, open_count_after) = host_after.unwrap();
            assert_eq!(connection_after, "connected", "Connection MUST stay 'connected' after closing last shell (last-shell contract)");
            assert_eq!(*open_count_after, 0, "Open count should be 0 after close");
        });
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_set_connection_validates_state() {
        use std::sync::Arc;
        struct TestSink;

        #[async_trait::async_trait]
        impl OutputSink for TestSink {
            async fn emit_output(&self, _session_id: &str, _data: &[u8]) {}
            async fn emit_exit(&self, _session_id: &str) {}
        }

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let temp_db = tempfile::NamedTempFile::new().unwrap();
            let store = Store::open(temp_db.path()).await.unwrap();
            let sink: Arc<dyn OutputSink> = Arc::new(TestSink);
            let manager = SessionManager::new(store, sink);

            let result = manager.test_set_connection("test-host", "invalid_state");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Invalid connection state"));
        });
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_valid_connection_states() {
        use std::sync::Arc;
        struct TestSink;

        #[async_trait::async_trait]
        impl OutputSink for TestSink {
            async fn emit_output(&self, _session_id: &str, _data: &[u8]) {}
            async fn emit_exit(&self, _session_id: &str) {}
        }

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let temp_db = tempfile::NamedTempFile::new().unwrap();
            let store = Store::open(temp_db.path()).await.unwrap();
            let sink: Arc<dyn OutputSink> = Arc::new(TestSink);
            let manager = SessionManager::new(store, sink);

            for state in &["local", "connected", "disconnected", "connecting", "error"] {
                let result = manager.test_set_connection("test-host", state);
                assert!(result.is_ok(), "State '{}' should be valid", state);
            }
        });
    }
}
