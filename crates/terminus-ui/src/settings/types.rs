use crate::components::input::FieldPaint;

/// Which SqlSync text field owns the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SqlSyncFocus {
    #[default]
    None,
    Uri,
    Passphrase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Keys,
    SqlSync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKeyItem {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub created: String,
    /// OpenSSH public key line, to paste into a server's authorized_keys.
    pub public_key: String,
}

/// Snapshot pushed from the host worker into the SqlSync pane.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncUiStatus {
    pub uri: String,
    pub connected: bool,
    pub vault_unlocked: bool,
    pub status_line: String,
    /// When true, [`status_line`](Self::status_line) is an error to highlight.
    pub is_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    Consume,
    Close,
    Tab(SettingsTab),
    NewKey,
    DeleteKey(usize),
    CopyPublicKey(usize),
    /// Focus the inline generate-key name field.
    FocusKeyDraft,
    /// Focus the optional PEM paste field on the generate form.
    FocusKeyPem,
    /// Focus the passphrase field for an encrypted key import.
    FocusKeyPassphrase,
    /// Confirm generating an Ed25519 identity from the draft name.
    GenerateKey,
    /// Cancel the inline generate-key form.
    CancelKeyDraft,
    /// Open / close the Database Engine dropdown.
    ToggleEngineMenu,
    /// Pick an engine from the open dropdown.
    SelectEngine(usize),
    FocusUri,
    FocusPassphrase,
    TogglePassphrase,
    /// Unlock / create the Argon2 vault with the passphrase field.
    UnlockVault,
    /// Clear a previously remembered vault passphrase from the keyring.
    ForgetPassphrase,
    /// Persist URI and run SyncEngine::sync_now.
    TestSync,
    Done,
}

/// What one SqlSync text field should paint (shared [`FieldPaint`] model).
pub type SqlFieldPaint = FieldPaint;
