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

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;

use chrono::Utc;
use terminus_core::models::Host;
use terminus_core::Store;
use uuid::Uuid;

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

/// A host as the sidebar needs it — no secrets, no timestamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRow {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub group_id: Option<String>,
    pub os_id: Option<String>,
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
}

/// Answers coming back from the worker.
#[derive(Debug)]
enum HostEvent {
    Loaded(Vec<HostRow>),
    /// A mutation succeeded; carries the label to report in the sidebar.
    Stored(String),
    Failed(String),
}

/// UI-side handle to the host database.
pub struct HostRepository {
    commands: Sender<Command>,
    events: Receiver<HostEvent>,
    hosts: Vec<HostRow>,
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
            }
            Command::Create(draft) => {
                let host = host_from_draft(&draft);
                let label = host.name.clone();
                match runtime.block_on(store.upsert_host(&host)) {
                    Ok(()) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(format!(
                            "Could not save the host: {err}"
                        )));
                    }
                }
            }
        }
        wake();
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
