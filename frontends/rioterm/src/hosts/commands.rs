use super::*;

pub(super) enum Command {
    Refresh,
    CreateSnippet(terminus_ui::snippets::SnippetItem),
    DeleteSnippet(String),
    /// Persist without SSH probe (tests / legacy).
    #[allow(dead_code)]
    Create(HostDraft),
    /// Probe SSH, then persist (+ seal password) on success.
    ProbeAndCreate(HostDraft),
    /// Probe SSH, then update an existing host.
    ProbeAndUpdate {
        id: String,
        draft: HostDraft,
    },
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
    UnlockVault {
        passphrase: String,
        /// Persist passphrase in the OS keyring after a successful unlock.
        remember: bool,
    },
    /// Generate and persist a new Ed25519 managed SSH key.
    CreateSshKey {
        name: String,
        /// When set, import this OpenSSH PEM instead of generating.
        pem: Option<String>,
        /// Passphrase of an encrypted `pem`.
        passphrase: Option<String>,
    },
    /// Soft-delete a managed SSH key.
    DeleteSshKey {
        id: String,
    },
    /// Soft-delete a stored SSH host.
    DeleteHost {
        id: String,
    },
    /// Soft-delete a host group (hosts inside become ungrouped).
    DeleteGroup {
        id: String,
    },
    /// Rename a stored SSH host.
    RenameHost {
        id: String,
        name: String,
    },
    /// Rename a host group.
    RenameGroup {
        id: String,
        name: String,
    },
    /// Persist remote URI and run SyncEngine::sync_now.
    TestSync {
        uri: String,
    },
    /// Unseal a stored host password (vault must be unlocked).
    ResolveHostPassword {
        id: String,
        reply: Sender<Result<Option<String>, String>>,
    },
    /// Load the managed private key PEM for a host (key auth).
    ResolveHostIdentity {
        id: String,
        #[allow(clippy::type_complexity)]
        reply: Sender<Result<Option<(String, Option<String>)>, String>>,
    },
    /// SFTP credentials for a host, answered as [`HostEvent::SftpAuth`]
    /// (the UI thread never waits for them).
    ResolveSftpAuth {
        id: String,
        auth_method: String,
    },
    /// After SSH connect: probe remote OS and persist `os_id` for the sidebar icon.
    DetectOs {
        id: String,
    },
    /// Set the persisted collapsed state for a sidebar group to an explicit value.
    ///
    /// Unlike a toggle, this command is idempotent: sending the same
    /// `{ group_id, collapsed }` twice produces the same final stored state.
    /// The reply channel receives the outcome; on error the caller reverts
    /// local visual state and surfaces the error.
    SetGroupCollapsed {
        group_id: String,
        collapsed: bool,
        reply: Sender<GroupCollapseOutcome>,
    },
}

/// Answers coming back from the worker.
#[derive(Debug)]
pub(super) enum HostEvent {
    /// Answer to [`Command::ResolveSftpAuth`].
    SftpAuth {
        id: String,
        result: Result<SftpAuth, String>,
    },
    SnippetsLoaded(Vec<terminus_ui::snippets::SnippetItem>),
    Loaded(Vec<HostRow>),
    GroupsLoaded(Vec<(String, String, i64)>),
    IdentitiesLoaded(Vec<IdentityRow>),
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
        /// A vault header exists (unlock), or not yet (the prompt creates one).
        configured: bool,
        message: Option<String>,
    },
    /// Remote OS classified after connect (sidebar / tab badge).
    OsDetected {
        id: String,
        os_id: String,
    },
    /// Collapsed group IDs loaded from settings on startup (or any Refresh).
    /// Only the first emission is applied; subsequent ones are ignored because
    /// local visual state is authoritative after the initial seed.
    CollapsedGroupsLoaded(HashSet<String>),
}
