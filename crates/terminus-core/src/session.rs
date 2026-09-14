//! Session registry (local shells and remote SSH sessions).
//!
//! [`SessionManager`] is the single source of truth for "what is open right
//! now": the sidebar, the tab bar and session recall all read from it, while
//! the PTY/SSH transport reports lifecycle transitions back into it and pushes
//! terminal bytes through an [`OutputSink`].

use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a session is attached to. Mirrors the milestone's
/// `ContextManagerConfig { session: SessionSpec }` hook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionSpec {
    /// A local shell (the default Rio PTY).
    Local,
    /// A remote session living on a stored host.
    Ssh {
        /// Id of the host in the local store.
        host_id: Uuid,
    },
}

impl SessionSpec {
    /// Builds an SSH spec for `host_id`.
    pub fn ssh(host_id: Uuid) -> Self {
        SessionSpec::Ssh { host_id }
    }

    /// Whether this is a local shell.
    pub const fn is_local(&self) -> bool {
        matches!(self, SessionSpec::Local)
    }

    /// Whether this is a remote session.
    pub const fn is_ssh(&self) -> bool {
        matches!(self, SessionSpec::Ssh { .. })
    }

    /// The host id, for SSH sessions.
    pub const fn host_id(&self) -> Option<Uuid> {
        match self {
            SessionSpec::Ssh { host_id } => Some(*host_id),
            SessionSpec::Local => None,
        }
    }

    /// Short label used in tab titles.
    pub const fn kind_str(&self) -> &'static str {
        match self {
            SessionSpec::Local => "local",
            SessionSpec::Ssh { .. } => "ssh",
        }
    }
}

impl fmt::Display for SessionSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionSpec::Local => f.write_str("local"),
            SessionSpec::Ssh { host_id } => write!(f, "ssh:{host_id}"),
        }
    }
}

/// Lifecycle of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Transport is being established.
    Starting,
    /// Connected and streaming.
    Running,
    /// Transport dropped; the session can be reconnected.
    Disconnected,
    /// Closed on purpose; the tab can be removed.
    Closed,
    /// Failed to start (auth error, unreachable host, ...).
    Failed,
}

impl SessionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            SessionState::Starting => "starting",
            SessionState::Running => "running",
            SessionState::Disconnected => "disconnected",
            SessionState::Closed => "closed",
            SessionState::Failed => "failed",
        }
    }

    /// Whether the session still occupies a tab.
    pub const fn is_live(self) -> bool {
        matches!(
            self,
            SessionState::Starting | SessionState::Running | SessionState::Disconnected
        )
    }
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Sink for the bytes and lifecycle events produced by a session's transport
/// (the bridge funnels them into Rio events).
pub trait OutputSink: Send + Sync {
    /// Terminal output for `session_id`.
    fn on_output(&self, session_id: Uuid, data: &[u8]);

    /// The session's process/channel exited.
    fn on_exit(&self, session_id: Uuid, exit_code: Option<i32>) {
        let _ = (session_id, exit_code);
    }

    /// A transport-level error occurred.
    fn on_error(&self, session_id: Uuid, message: &str) {
        let _ = (session_id, message);
    }

    /// The session moved to a new state.
    fn on_state_change(&self, session_id: Uuid, state: SessionState) {
        let _ = (session_id, state);
    }
}

/// An [`OutputSink`] that discards everything; used before the bridge attaches.
#[derive(Debug, Default)]
pub struct NullOutputSink;

impl OutputSink for NullOutputSink {
    fn on_output(&self, _session_id: Uuid, _data: &[u8]) {}
}

/// A registered session.
#[derive(Clone)]
pub struct ManagedSession {
    /// Session id (also the tab id).
    pub id: Uuid,
    /// What the session is attached to.
    pub spec: SessionSpec,
    /// User-visible title.
    pub title: String,
    /// Current lifecycle state.
    pub state: SessionState,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// Last output/state change seen by the manager.
    pub last_activity: DateTime<Utc>,
    /// Exit code, when known.
    pub exit_code: Option<i32>,
    /// Last error message, for failed sessions.
    pub last_error: Option<String>,
    sink: Arc<dyn OutputSink>,
}

impl ManagedSession {
    /// The sink this session's bytes are routed to.
    pub fn sink(&self) -> Arc<dyn OutputSink> {
        Arc::clone(&self.sink)
    }
}

impl fmt::Debug for ManagedSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedSession")
            .field("id", &self.id)
            .field("spec", &self.spec)
            .field("title", &self.title)
            .field("state", &self.state)
            .field("exit_code", &self.exit_code)
            .finish()
    }
}

/// Registry of open sessions.
pub struct SessionManager {
    sessions: DashMap<Uuid, ManagedSession>,
    sink: Arc<dyn OutputSink>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    /// An empty manager whose sessions discard output.
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            sink: Arc::new(NullOutputSink),
        }
    }

    /// An empty manager routing every session to `sink` by default.
    pub fn with_sink(sink: Arc<dyn OutputSink>) -> Self {
        Self {
            sessions: DashMap::new(),
            sink,
        }
    }

    /// Replaces the default sink (sessions created before keep their own).
    pub fn set_sink(&mut self, sink: Arc<dyn OutputSink>) {
        self.sink = sink;
    }

    /// The default sink.
    pub fn sink(&self) -> Arc<dyn OutputSink> {
        Arc::clone(&self.sink)
    }

    /// Registers a new session and returns its id.
    pub fn create(&self, spec: SessionSpec, title: impl Into<String>) -> Uuid {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let session = ManagedSession {
            id,
            spec,
            title: title.into(),
            state: SessionState::Starting,
            created_at: now,
            last_activity: now,
            exit_code: None,
            last_error: None,
            sink: Arc::clone(&self.sink),
        };
        session.sink.on_state_change(id, session.state);
        self.sessions.insert(id, session);
        id
    }

    /// Registers a session under a known id (session recall after restart).
    /// Returns `false` when the id is already taken.
    pub fn register(
        &self,
        id: Uuid,
        spec: SessionSpec,
        title: impl Into<String>,
    ) -> bool {
        let now = Utc::now();
        let session = ManagedSession {
            id,
            spec,
            title: title.into(),
            state: SessionState::Starting,
            created_at: now,
            last_activity: now,
            exit_code: None,
            last_error: None,
            sink: Arc::clone(&self.sink),
        };
        let inserted = match self.sessions.entry(id) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                entry.insert(session);
                true
            }
        };
        if inserted {
            self.sessions
                .get(&id)
                .map(|s| s.sink.on_state_change(id, SessionState::Starting));
        }
        inserted
    }

    /// Opens a local shell session.
    pub fn open_local(&self) -> Uuid {
        self.create(SessionSpec::Local, "Local shell")
    }

    /// Opens a session for `host_id`.
    pub fn open_ssh(&self, host_id: Uuid) -> Uuid {
        self.create(
            SessionSpec::ssh(host_id),
            format!("ssh:{}", &host_id.to_string()[..8]),
        )
    }

    /// A snapshot of one session.
    pub fn info(&self, id: Uuid) -> Option<ManagedSession> {
        self.sessions.get(&id).map(|entry| entry.clone())
    }

    /// The spec of one session.
    pub fn spec(&self, id: Uuid) -> Option<SessionSpec> {
        self.sessions.get(&id).map(|entry| entry.spec.clone())
    }

    /// Every session, oldest first.
    pub fn list(&self) -> Vec<ManagedSession> {
        let mut sessions: Vec<ManagedSession> =
            self.sessions.iter().map(|entry| entry.clone()).collect();
        sessions.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.to_string().cmp(&b.id.to_string()))
        });
        sessions
    }

    /// SSH sessions only (host inspector, "open all in group").
    pub fn ssh_sessions(&self) -> Vec<ManagedSession> {
        self.list()
            .into_iter()
            .filter(|session| session.spec.is_ssh())
            .collect()
    }

    /// Local sessions only.
    pub fn local_sessions(&self) -> Vec<ManagedSession> {
        self.list()
            .into_iter()
            .filter(|session| session.spec.is_local())
            .collect()
    }

    /// Host ids with a *connected* SSH session, ordered by first appearance.
    ///
    /// Only [`SessionState::Running`] counts: sessions still handshaking or
    /// already dropped must not light up a host as connected in the sidebar.
    pub fn connected_host_ids(&self) -> Vec<Uuid> {
        let mut hosts: Vec<Uuid> = Vec::new();
        for session in self.list() {
            if session.state != SessionState::Running {
                continue;
            }
            if let Some(host_id) = session.spec.host_id() {
                if !hosts.contains(&host_id) {
                    hosts.push(host_id);
                }
            }
        }
        hosts
    }

    /// Whether a session is registered.
    pub fn contains(&self, id: Uuid) -> bool {
        self.sessions.contains_key(&id)
    }

    /// Number of registered sessions (live or not).
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether no session is registered.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Number of sessions that still occupy a tab.
    pub fn live_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|entry| entry.state.is_live())
            .count()
    }

    /// Moves a session to `state`, notifying its sink. Returns whether the
    /// session exists.
    pub fn set_state(&self, id: Uuid, state: SessionState) -> bool {
        let Some(mut session) = self.sessions.get_mut(&id) else {
            return false;
        };
        if session.state == state {
            return true;
        }
        session.state = state;
        session.last_activity = Utc::now();
        let sink = Arc::clone(&session.sink);
        drop(session);
        sink.on_state_change(id, state);
        true
    }

    /// Marks a session as connected.
    pub fn mark_running(&self, id: Uuid) -> bool {
        self.set_state(id, SessionState::Running)
    }

    /// Marks a session as disconnected (reconnectable).
    pub fn mark_disconnected(&self, id: Uuid) -> bool {
        self.set_state(id, SessionState::Disconnected)
    }

    /// Marks a session as failed and records the reason.
    pub fn mark_failed(&self, id: Uuid, message: impl Into<String>) -> bool {
        let message = message.into();
        let Some(mut session) = self.sessions.get_mut(&id) else {
            return false;
        };
        session.last_error = Some(message.clone());
        session.state = SessionState::Failed;
        session.last_activity = Utc::now();
        let sink = Arc::clone(&session.sink);
        drop(session);
        sink.on_error(id, &message);
        sink.on_state_change(id, SessionState::Failed);
        true
    }

    /// Renames a session's tab.
    pub fn rename(&self, id: Uuid, title: impl Into<String>) -> bool {
        let Some(mut session) = self.sessions.get_mut(&id) else {
            return false;
        };
        session.title = title.into();
        session.last_activity = Utc::now();
        true
    }

    /// Routes transport output to the session's sink. Returns whether the
    /// session exists.
    pub fn emit_output(&self, id: Uuid, data: &[u8]) -> bool {
        let Some(mut session) = self.sessions.get_mut(&id) else {
            return false;
        };
        session.last_activity = Utc::now();
        let sink = Arc::clone(&session.sink);
        drop(session);
        sink.on_output(id, data);
        true
    }

    /// Closes a session, notifying its sink. Returns whether it existed.
    pub fn close(&self, id: Uuid, exit_code: Option<i32>) -> bool {
        let Some((_, mut session)) = self.sessions.remove(&id) else {
            return false;
        };
        session.state = SessionState::Closed;
        session.exit_code = exit_code;
        session.last_activity = Utc::now();
        session.sink.on_exit(id, exit_code);
        session.sink.on_state_change(id, SessionState::Closed);
        true
    }

    /// Closes every session. Returns how many were closed.
    pub fn close_all(&self) -> usize {
        let ids: Vec<Uuid> = self.sessions.iter().map(|entry| *entry.key()).collect();
        let count = ids.len();
        for id in ids {
            self.close(id, None);
        }
        count
    }
}

impl fmt::Debug for SessionManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionManager")
            .field("sessions", &self.len())
            .field("live", &self.live_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Sink that records everything the manager reports.
    #[derive(Default)]
    struct RecordingSink {
        output: Mutex<Vec<(Uuid, Vec<u8>)>>,
        exits: Mutex<Vec<(Uuid, Option<i32>)>>,
        errors: Mutex<Vec<(Uuid, String)>>,
        states: Mutex<Vec<(Uuid, SessionState)>>,
    }

    impl OutputSink for RecordingSink {
        fn on_output(&self, session_id: Uuid, data: &[u8]) {
            self.output
                .lock()
                .unwrap()
                .push((session_id, data.to_vec()));
        }

        fn on_exit(&self, session_id: Uuid, exit_code: Option<i32>) {
            self.exits.lock().unwrap().push((session_id, exit_code));
        }

        fn on_error(&self, session_id: Uuid, message: &str) {
            self.errors
                .lock()
                .unwrap()
                .push((session_id, message.to_string()));
        }

        fn on_state_change(&self, session_id: Uuid, state: SessionState) {
            self.states.lock().unwrap().push((session_id, state));
        }
    }

    #[test]
    fn specs_expose_their_kind_and_host() {
        let host_id = Uuid::new_v4();
        let local = SessionSpec::Local;
        let ssh = SessionSpec::ssh(host_id);

        assert!(local.is_local());
        assert!(!local.is_ssh());
        assert_eq!(local.host_id(), None);
        assert!(ssh.is_ssh());
        assert_eq!(ssh.host_id(), Some(host_id));
        assert_eq!(ssh.kind_str(), "ssh");
        assert_eq!(ssh.to_string(), format!("ssh:{host_id}"));
    }

    #[test]
    fn session_spec_serializes_with_a_kind_tag() {
        let host_id = Uuid::new_v4();
        let json = serde_json::to_string(&SessionSpec::ssh(host_id)).unwrap();
        assert!(json.contains("\"kind\":\"ssh\""));
        let parsed: SessionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SessionSpec::ssh(host_id));

        let json = serde_json::to_string(&SessionSpec::Local).unwrap();
        assert_eq!(
            serde_json::from_str::<SessionSpec>(&json).unwrap(),
            SessionSpec::Local
        );
    }

    #[test]
    fn create_list_and_close_sessions() {
        let manager = SessionManager::new();
        assert!(manager.is_empty());

        let local = manager.open_local();
        let host_id = Uuid::new_v4();
        let ssh = manager.open_ssh(host_id);

        assert_eq!(manager.len(), 2);
        assert_eq!(manager.local_sessions().len(), 1);
        assert_eq!(manager.ssh_sessions().len(), 1);
        assert_eq!(manager.info(ssh).unwrap().spec, SessionSpec::ssh(host_id));

        // Live counting follows the lifecycle.
        assert_eq!(manager.live_count(), 2);
        assert!(manager.mark_running(local));
        assert!(manager.mark_disconnected(ssh));
        assert_eq!(manager.live_count(), 2);
        assert!(manager.set_state(ssh, SessionState::Failed));
        assert_eq!(manager.live_count(), 1);

        assert!(manager.close(local, Some(0)));
        assert!(!manager.close(local, Some(0)));
        assert_eq!(manager.len(), 1);
        assert_eq!(manager.close_all(), 1);
        assert!(manager.is_empty());
    }

    #[test]
    fn register_refuses_duplicate_ids_and_recall_works() {
        let manager = SessionManager::new();
        let id = Uuid::new_v4();

        // Registering with a caller-supplied id preserves persisted sessions.
        assert!(manager.register(id, SessionSpec::Local, "recalled"));
        assert!(!manager.register(id, SessionSpec::Local, "duplicate"));
        assert_eq!(manager.info(id).unwrap().title, "recalled");
    }

    #[test]
    fn connected_host_ids_track_live_ssh_sessions() {
        let manager = SessionManager::new();
        let host_id = Uuid::new_v4();
        let session = manager.open_ssh(host_id);
        manager.open_local();

        assert!(manager.connected_host_ids().is_empty()); // still Starting
        manager.mark_running(session);
        assert_eq!(manager.connected_host_ids(), vec![host_id]);

        manager.set_state(session, SessionState::Failed);
        assert!(manager.connected_host_ids().is_empty());
    }

    #[test]
    fn rename_reports_missing_sessions() {
        let manager = SessionManager::new();
        assert!(!manager.rename(Uuid::new_v4(), "nope"));

        let id = manager.open_local();
        assert!(manager.rename(id, "zsh"));
        assert_eq!(manager.info(id).unwrap().title, "zsh");
    }

    #[test]
    fn output_and_lifecycle_events_reach_the_sink() {
        let sink = Arc::new(RecordingSink::default());
        let manager = SessionManager::with_sink(sink.clone());

        let id = manager.open_local();
        assert!(manager.emit_output(id, b"hello"));
        assert!(!manager.emit_output(Uuid::new_v4(), b"orphan"));

        manager.mark_running(id);
        manager.mark_failed(id, "connection reset");
        manager.close(id, Some(130));

        assert_eq!(sink.output.lock().unwrap().len(), 1);
        let states = sink.states.lock().unwrap().clone();
        assert_eq!(states[0], (id, SessionState::Starting));
        assert!(states.contains(&(id, SessionState::Running)));
        assert!(states.contains(&(id, SessionState::Failed)));
        assert_eq!(sink.errors.lock().unwrap().len(), 1);
        assert_eq!(*sink.exits.lock().unwrap(), vec![(id, Some(130))]);
    }

    #[test]
    fn null_sink_swallows_everything() {
        let sink = NullOutputSink;
        let manager = SessionManager::with_sink(Arc::new(sink));
        let id = manager.open_local();
        manager.emit_output(id, b"ignored");
        manager.close(id, None);
        assert!(manager.is_empty());
    }

    #[test]
    fn session_state_display_and_liveness() {
        assert_eq!(SessionState::Running.as_str(), "running");
        assert_eq!(SessionState::Closed.to_string(), "closed");
        assert!(SessionState::Starting.is_live());
        assert!(!SessionState::Failed.is_live());
    }
}
