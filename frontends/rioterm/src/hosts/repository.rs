use super::*;

mod ops;

/// UI-side handle to the host database.
pub struct HostRepository {
    pub(super) commands: Sender<Command>,
    events: Receiver<HostEvent>,
    hosts: Vec<HostRow>,
    groups: Vec<(String, String, i64)>,
    /// `(id, name, fingerprint)` for Settings + add-host picker.
    identities: Vec<IdentityRow>,
    platform: PlatformFacts,
    pub snippet_items: Vec<terminus_ui::snippets::SnippetItem>,
    loading: bool,
    /// Commands sent but not yet answered.
    in_flight: usize,
    notice: Option<String>,
    error: Option<String>,
    /// Last vault unlock status message (Settings).
    vault_message: Option<String>,
    vault_configured: bool,
    vault_unlocked: bool,
    /// Last known sync URI from the store.
    sync_uri: String,
    sync_connected: bool,
    sync_status_line: String,
    sync_status_is_error: bool,
    /// One-shot seed: set when `CollapsedGroupsLoaded` first arrives; taken by
    /// `pump_chrome` to initialize `HostPanel::collapsed_groups`.
    collapsed_groups_seed: Option<HashSet<String>>,
    /// True after the first `CollapsedGroupsLoaded` has been applied.
    pub(crate) collapsed_groups_seeded: bool,
    /// SFTP credentials answered by the worker, not yet taken.
    pub(crate) sftp_auth_replies: Vec<(String, Result<SftpAuth, String>)>,
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
            snippet_items: Vec::new(),
            platform: PlatformFacts::default(),
            loading: true,
            in_flight: 1,
            notice: None,
            error: None,
            vault_message: None,
            vault_configured: false,
            vault_unlocked: false,
            sync_uri: String::new(),
            sync_connected: false,
            sync_status_line: "Not configured".into(),
            sync_status_is_error: false,
            collapsed_groups_seed: None,
            collapsed_groups_seeded: false,
            sftp_auth_replies: Vec::new(),
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

    pub fn identities(&self) -> &[IdentityRow] {
        &self.identities
    }

    /// Whether a vault exists yet (else unlocking creates it).
    pub fn vault_configured(&self) -> bool {
        self.vault_configured
    }

    pub fn vault_unlocked(&self) -> bool {
        self.vault_unlocked
    }

    pub fn create_snippet(&mut self, snippet: terminus_ui::snippets::SnippetItem) {
        let _ = self.commands.send(Command::CreateSnippet(snippet));
    }

    pub fn delete_snippet(&mut self, id: String) {
        let _ = self.commands.send(Command::DeleteSnippet(id));
    }

    pub fn take_vault_message(&mut self) -> Option<String> {
        self.vault_message.take()
    }

    #[allow(dead_code)]
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

    /// Create an [`HostPersistHandle`] backed by this repository's worker.
    ///
    /// The Application calls this once to obtain the canonical persistence
    /// handle. All group-collapse commands must flow through this handle, not
    /// through an arbitrary route's repository.
    pub fn persist_handle(&self) -> HostPersistHandle {
        HostPersistHandle::new(self.commands.clone())
    }

    /// Returns the collapsed-groups set loaded from settings on startup, once.
    ///
    /// `pump_chrome` calls this to seed `HostPanel::collapsed_groups` after the
    /// first `Refresh`. After the first call, always returns `None`.
    pub fn take_collapsed_groups_seed(&mut self) -> Option<HashSet<String>> {
        self.collapsed_groups_seed.take()
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

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    #[allow(dead_code)]
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
}
