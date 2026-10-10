//! Settings modal: SSH Keys + Remote SQL Sync.

mod geometry;
mod hit;
mod metrics;
mod state;
mod types;

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod sql_tests;

pub use metrics::*;
pub use types::*;

use crate::components::input::TextDraft;

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsModal {
    pub open: bool,
    pub tab: SettingsTab,
    pub keys: Vec<SshKeyItem>,
    pub sql_engine: usize,
    /// Whether the Database Engine dropdown list is expanded.
    pub engine_menu_open: bool,
    /// Hovered option index inside the engine dropdown.
    pub engine_menu_hover: Option<usize>,
    /// Connection string draft (caret, selection, word jumps).
    pub sql_uri: TextDraft,
    /// Encryption passphrase draft (same editing model as the URI).
    pub sql_passphrase: TextDraft,
    pub passphrase_visible: bool,
    pub sync_connected: bool,
    /// Baseline status shown on the action row (never replaced by errors).
    pub sync_status: String,
    /// Soft error banner message; `None` hides the banner.
    pub sync_error: Option<String>,
    pub vault_unlocked: bool,
    /// True when a passphrase is stored in the OS keyring / local fallback.
    pub passphrase_remembered: bool,
    pub sql_focus: SqlSyncFocus,
    /// Inline "New SSH Key" draft form is open.
    pub key_drafting: bool,
    /// Label field (same editing model as rename / other chrome fields).
    pub key_label: TextDraft,
    /// Optional OpenSSH private-key PEM; empty means generate Ed25519.
    pub key_pem: TextDraft,
    /// Whether the draft name field owns the caret.
    pub key_draft_focused: bool,
    /// Whether the PEM paste field owns the caret.
    pub key_draft_pem_focused: bool,
    /// Passphrase for an encrypted OpenSSH key being imported (masked).
    pub key_passphrase: TextDraft,
    /// Whether the key passphrase field owns the caret.
    pub key_draft_passphrase_focused: bool,
    /// Error shown under the generate form (empty label, worker failure, …).
    pub key_draft_error: Option<String>,
    /// Confirmation under the key list (public key copied, …).
    pub keys_notice: Option<String>,
    /// Hovered Delete control index on the Keys list (red label).
    pub key_delete_hover: Option<usize>,
    /// Hovered key row index — Delete is only painted for this row.
    pub key_row_hover: Option<usize>,
}

impl Default for SettingsModal {
    fn default() -> Self {
        Self {
            open: false,
            tab: SettingsTab::Keys,
            keys: Vec::new(),
            sql_engine: 0,
            engine_menu_open: false,
            engine_menu_hover: None,
            sql_uri: TextDraft::default(),
            sql_passphrase: TextDraft::default(),
            passphrase_visible: false,
            sync_connected: false,
            sync_status: "Not configured".into(),
            sync_error: None,
            vault_unlocked: false,
            passphrase_remembered: false,
            sql_focus: SqlSyncFocus::None,
            key_drafting: false,
            key_label: TextDraft::default(),
            key_pem: TextDraft::default(),
            key_draft_focused: false,
            key_draft_pem_focused: false,
            key_passphrase: TextDraft::default(),
            key_draft_passphrase_focused: false,
            key_draft_error: None,
            keys_notice: None,
            key_delete_hover: None,
            key_row_hover: None,
        }
    }
}
