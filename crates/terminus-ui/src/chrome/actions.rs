/// Overlay dialog paint / input stacking (back → front).
///
/// Sugarloaf composites **one** overlay pass as all overlay quads, then all
/// overlay text. Nested modals therefore cannot rely on paint call order
/// alone: lower-modal glyphs would float above a higher modal's panel.
/// Painters must emit glyphs only for [`Chrome::top_modal_paint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalPaintLayer {
    HostEditor,
    AddSnippet,
    Settings,
    VaultUnlock,
    /// Delete-host / delete-group confirmation (above everything).
    Confirm,
}

/// Mouse cursor affordance for chrome hit targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChromeCursor {
    #[default]
    Default,
    /// Buttons, rows, tabs, links.
    Pointer,
    /// Editable text fields.
    Text,
}

/// What a mouse press on the chrome did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChromeAction {
    /// The press missed the chrome; the terminal gets it.
    Ignored,
    /// The press hit the chrome and was consumed.
    Consumed,
    /// The add-host row was pressed: open the editor.
    AddHost,
    OpenAddSnippet,
    SubmitAddSnippet(crate::add_snippet::SnippetFormValues),
    DeleteSnippet(String),
    /// The new-group control was pressed: toggle the inline form.
    NewGroup,
    /// Confirm creating a group from the inline form.
    CreateGroup,
    /// Cancel the inline new-group form.
    CancelNewGroup,
    /// A group header was pressed: toggle collapse.
    ToggleGroup(String),
    /// The search field was pressed: focus it.
    FocusSearch,
    /// The new-group name field was pressed: focus it.
    FocusNewGroup,
    /// A host row was pressed: connect to that host.
    OpenHost(String),
    /// Open session on a host: focus that tab.
    OpenSession(usize),
    /// Close × on a session row.
    CloseSession(usize),
    /// "+" on a host: open another session for that host.
    AddHostSession(String),
    /// Footer Connect on the add-host dialog (same as Enter).
    SubmitHostForm,
    /// Vault unlock prompt: submit passphrase.
    SubmitVaultUnlock,
    /// Settings: unlock / create vault with the SQL Sync passphrase field.
    UnlockVault,
    /// Settings: clear a remembered vault passphrase from the keyring.
    ForgetVaultPassphrase,
    /// Settings: persist remote URI and run SyncEngine::sync_now.
    TestSync,
    /// Settings: generate a new Ed25519 managed SSH key.
    GenerateSshKey,
    /// Settings: soft-delete a managed SSH key by id.
    DeleteSshKey(String),
    /// Put this OpenSSH public key on the clipboard.
    CopyPublicKey(String),
    /// Put this stored host's `ssh …` command on the clipboard.
    CopySshCommand(String),
    /// Put this text (an error message, say) on the clipboard.
    CopyText(String),
    /// Soft-delete a stored host (context menu).
    DeleteHost(String),
    /// Soft-delete a host group (context menu).
    DeleteGroup(String),
    /// Edit a stored host in the add-host dialog (context menu).
    EditHost(String),
    /// Open the dual-pane SFTP browser for a stored host (context menu).
    OpenSftp(String),
    /// Open a stored host in the other SFTP pane (context menu).
    OpenSftpOtherPane(String),
    /// Begin renaming a stored host (context menu).
    RenameHost(String),
    /// Begin renaming a host group (context menu).
    RenameGroup(String),
    /// Open a session on every host of a group (context menu).
    OpenGroup(String),
    /// Close the sessions of every host of a group (context menu).
    CloseGroup(String),
    /// Commit an inline rename with the draft name.
    CommitRename {
        id: String,
        is_group: bool,
        name: String,
    },
    /// Context menu: copy.
    ContextCopy,
    /// Context menu: paste.
    ContextPaste,
    /// SFTP context: new folder on focused side.
    SftpNewFolder,
    /// SFTP context: rename selection.
    SftpRename,
    /// SFTP context: delete selection.
    SftpDelete,
    /// The SFTP delete confirmation was accepted.
    SftpDeleteConfirmed,
    /// SFTP context: transfer selection to the other pane.
    SftpTransfer,
    /// SFTP context: edit remote file via temp + default app.
    SftpEdit,
    /// SFTP context: enter selected directory.
    SftpOpen,
    /// SFTP context: refresh focused pane.
    SftpRefresh,
    /// Settings: focus the connection URI field.
    FocusSqlUri,
    /// Settings: focus the vault passphrase field.
    FocusSqlPassphrase,
    /// Settings: focus the generate-key label field.
    FocusKeyDraft,
    /// Expand/collapse sessions under a host.
    ToggleHost(String),
    /// Move a stored host into a group (`Some`) or out to the root list (`None`).
    SetHostGroup {
        host_id: String,
        group_id: Option<String>,
    },
    /// Place a host before another ungrouped host or before a group.
    ReorderHost {
        host_id: String,
        before_host_id: Option<String>,
        before_group_id: Option<String>,
    },
    /// Place a group before another group or before a root host.
    ReorderGroup {
        group_id: String,
        before_group_id: Option<String>,
        before_host_id: Option<String>,
    },
    /// Close the connection-progress modal.
    DismissConnection,
    /// Lost-connection card: reopen the host of this terminal (route id)
    /// in place of the dead tab.
    ReconnectSession(usize),
    /// Lost-connection card: close the dead tab of this terminal (route id).
    CloseLostSession(usize),
    /// Run a snippet command in the active terminal.
    RunSnippet(String),
    /// Settings modal was dismissed.
    DismissSettings,
    /// Shell: open the command palette ("Search or run…").
    OpenPalette,
    /// Shell: the workspace view changed (header tab, brand, Settings).
    ViewChanged(crate::shell::WorkspaceView),
    /// Shell: a header window control was pressed.
    WindowControl(crate::shell::WindowButton),
    /// Shell: press on the empty header — drag the window (double-click
    /// maximizes).
    WindowDrag,
    /// Shell: split the focused session (`down` = horizontal divider).
    Split {
        down: bool,
    },
}
