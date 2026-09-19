//! Settings modal: SSH Keys + Remote SQL Sync.

use crate::geom::Rect;
use crate::text_field::{FieldPaint, TextDraft};

pub const MAX_WIDTH: f32 = 768.0;
pub const SIDEBAR_WIDTH: f32 = 224.0;
pub const HEIGHT_RATIO: f32 = 0.78;
pub const RADIUS: f32 = 16.0;

/// Equal inset around the input inside a SqlSync field card.
pub const FIELD_CARD_PAD: f32 = 12.0;
/// Space from card top to the value input (label sits above).
pub const FIELD_INPUT_TOP: f32 = 30.0;
pub const FIELD_INPUT_HEIGHT: f32 = 28.0;
/// Card height: top pad for label + input + matching bottom pad.
pub const FIELD_CARD_HEIGHT: f32 = FIELD_INPUT_TOP + FIELD_INPUT_HEIGHT + FIELD_CARD_PAD;
/// Gap between stacked field cards.
pub const FIELD_CARD_GAP: f32 = 12.0;
pub const FIELD_CARD_STEP: f32 = FIELD_CARD_HEIGHT + FIELD_CARD_GAP;
/// Horizontal inset for value text / placeholder inside the input.
pub const FIELD_TEXT_INSET: f32 = 10.0;
/// Trailing eye-toggle slot inside the passphrase input (keeps text clear).
pub const FIELD_EYE_SLOT: f32 = 28.0;
pub const FIELD_EYE_ICON: f32 = 16.0;
/// Soft error banner under the SqlSync action row (only when flagged).
pub const ERROR_BANNER_HEIGHT: f32 = 44.0;
pub const ERROR_BANNER_GAP: f32 = 10.0;
/// Status text line inside the SqlSync footer block (full content width).
pub const STATUS_TEXT_HEIGHT: f32 = 22.0;
/// Gap between the status line and the Unlock / Test Sync buttons.
pub const STATUS_ACTIONS_GAP: f32 = 10.0;
/// Button row height inside the SqlSync footer block.
pub const STATUS_ACTIONS_HEIGHT: f32 = 28.0;
/// Footer block: status on its own line, then the action buttons.
pub const STATUS_BLOCK_HEIGHT: f32 = FIELD_CARD_PAD
    + STATUS_TEXT_HEIGHT
    + STATUS_ACTIONS_GAP
    + STATUS_ACTIONS_HEIGHT
    + FIELD_CARD_PAD;
/// Shared Unlock / Test Sync button width.
pub const SYNC_ACTION_BTN_WIDTH: f32 = 110.0;
pub const SYNC_ACTION_BTN_GAP: f32 = 8.0;
/// "Forget saved passphrase" when a remembered vault secret exists.
pub const FORGET_PASSPHRASE_BTN_WIDTH: f32 = 168.0;
/// Dashed "New SSH Key" CTA height on the Keys tab.
pub const KEY_CTA_HEIGHT: f32 = 56.0;
pub const KEY_CTA_GAP: f32 = 12.0;
/// Stored key row height.
pub const KEY_ROW_HEIGHT: f32 = 56.0;
pub const KEY_ROW_GAP: f32 = 8.0;
/// Inline generate form under the CTA (two field cards + actions + error).
pub const KEY_DRAFT_HEIGHT: f32 = 252.0;
pub const KEY_DRAFT_GENERATE_WIDTH: f32 = 96.0;
pub const KEY_DRAFT_CANCEL_WIDTH: f32 = 72.0;
/// Max bytes for a pasted OpenSSH private key.
pub const KEY_PEM_MAX_BYTES: usize = 16_384;
pub const KEY_LABEL_MAX_BYTES: usize = 64;
/// Max bytes accepted by the SqlSync connection-URI field.
pub const SQL_URI_MAX_BYTES: usize = 1024;
/// Max bytes accepted by the SqlSync encryption-passphrase field.
pub const SQL_PASSPHRASE_MAX_BYTES: usize = 256;

/// Shared input row geometry inside a SqlSync field card (equal card pad).
pub fn field_input_in_card(card: Rect) -> Rect {
    Rect::new(
        card.x + FIELD_CARD_PAD,
        card.y + FIELD_INPUT_TOP,
        (card.width - 2.0 * FIELD_CARD_PAD).max(0.0),
        FIELD_INPUT_HEIGHT,
    )
}

/// Remote database engines exposed by the SqlSync selector.
pub const SQL_ENGINES: [&str; 2] = ["SQLite", "PostgreSQL"];

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
    /// Error shown under the generate form (empty label, worker failure, …).
    pub key_draft_error: Option<String>,
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
            key_draft_error: None,
            key_delete_hover: None,
            key_row_hover: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    Consume,
    Close,
    Tab(SettingsTab),
    NewKey,
    DeleteKey(usize),
    /// Focus the inline generate-key name field.
    FocusKeyDraft,
    /// Focus the optional PEM paste field on the generate form.
    FocusKeyPem,
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

impl SettingsModal {
    /// Replace the SSH key list (from `Store::list_identities`).
    pub fn set_keys(&mut self, keys: Vec<SshKeyItem>) {
        self.keys = keys;
    }

    /// Apply worker-driven sync/vault status onto the pane.
    pub fn apply_sync_status(&mut self, snap: SyncUiStatus) {
        // Never wipe a non-empty local draft with an empty worker URI.
        // While the URI field is focused, leave the draft alone entirely.
        if self.sql_focus != SqlSyncFocus::Uri {
            if !snap.uri.is_empty() {
                self.sql_uri = TextDraft::new(snap.uri);
            }
        }
        self.sync_connected = snap.connected;
        self.vault_unlocked = snap.vault_unlocked;
        if !snap.status_line.is_empty() {
            if snap.is_error {
                // Keep the action-row baseline; banner owns the error text.
                self.sync_error = Some(snap.status_line);
            } else {
                self.sync_error = None;
                self.sync_status = snap.status_line;
            }
        }
    }

    /// Reflect whether a passphrase is currently remembered ("Forget" button).
    pub fn set_passphrase_remembered(&mut self, remembered: bool) {
        self.passphrase_remembered = remembered;
    }

    /// Vault unlock/create feedback: errors go to the banner only.
    pub fn apply_vault_feedback(&mut self, message: String, unlocked: bool) {
        self.vault_unlocked = unlocked;
        if unlocked {
            self.sync_error = None;
            self.sync_status = message;
        } else {
            self.sync_error = Some(message);
        }
    }

    #[inline]
    pub fn sync_status_is_error(&self) -> bool {
        self.sync_error.is_some()
    }

    /// Paint model for the Connection URI field.
    pub fn uri_field_paint(&self) -> SqlFieldPaint {
        FieldPaint::from_draft(
            &self.sql_uri,
            self.uri_placeholder(),
            self.sql_focus == SqlSyncFocus::Uri,
        )
    }

    /// Paint model for the passphrase field (masking included). Masking
    /// rewrites only the visible glyphs: the caret prefix and the selection
    /// range keep their display positions.
    pub fn passphrase_field_paint(&self) -> SqlFieldPaint {
        let focused = self.sql_focus == SqlSyncFocus::Passphrase;
        FieldPaint::from_draft_masked(
            &self.sql_passphrase,
            self.passphrase_placeholder(),
            focused,
            !self.passphrase_visible,
        )
    }

    pub fn open_tab(&mut self, tab: SettingsTab) {
        self.tab = tab;
        self.open = true;
        if tab != SettingsTab::SqlSync {
            self.sql_focus = SqlSyncFocus::None;
        }
        if tab != SettingsTab::Keys {
            self.close_key_draft();
            self.key_delete_hover = None;
            self.key_row_hover = None;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.sql_focus = SqlSyncFocus::None;
        self.engine_menu_open = false;
        self.engine_menu_hover = None;
        self.key_delete_hover = None;
        self.key_row_hover = None;
        self.close_key_draft();
    }

    /// Open the inline generate-key form and focus the label field.
    pub fn open_key_draft(&mut self) {
        self.tab = SettingsTab::Keys;
        self.key_drafting = true;
        self.key_draft_focused = true;
        self.key_draft_pem_focused = false;
        self.key_label.clear();
        self.key_pem.clear();
        self.key_draft_error = None;
        self.sql_focus = SqlSyncFocus::None;
    }

    pub fn close_key_draft(&mut self) {
        self.key_drafting = false;
        self.key_draft_focused = false;
        self.key_draft_pem_focused = false;
        self.key_label.clear();
        self.key_pem.clear();
        self.key_draft_error = None;
    }

    pub fn focus_key_draft(&mut self) {
        if self.key_drafting {
            self.key_draft_focused = true;
            self.key_draft_pem_focused = false;
            self.sql_focus = SqlSyncFocus::None;
        }
    }

    pub fn focus_key_pem(&mut self) {
        if self.key_drafting {
            self.key_draft_pem_focused = true;
            self.key_draft_focused = false;
            self.sql_focus = SqlSyncFocus::None;
        }
    }

    /// Active text draft while key drafting, if any.
    pub fn key_draft_active(&mut self) -> Option<&mut TextDraft> {
        if !self.key_drafting {
            return None;
        }
        if self.key_draft_pem_focused {
            Some(&mut self.key_pem)
        } else if self.key_draft_focused {
            Some(&mut self.key_label)
        } else {
            None
        }
    }

    /// Insert into the focused generate-key field (label or PEM).
    pub fn insert_key_draft_text(&mut self, text: &str) -> bool {
        if !self.key_drafting || text.is_empty() {
            return false;
        }
        let ok = if self.key_draft_pem_focused {
            self.key_pem.insert(text, KEY_PEM_MAX_BYTES, true)
        } else if self.key_draft_focused {
            self.key_label.insert(text, KEY_LABEL_MAX_BYTES, false)
        } else {
            false
        };
        if ok {
            self.key_draft_error = None;
        }
        ok
    }

    pub fn key_draft_backspace(&mut self) -> bool {
        if !self.key_drafting {
            return false;
        }
        if self.key_draft_pem_focused {
            return self.key_pem.backspace(false);
        }
        if !self.key_draft_focused {
            return false;
        }
        self.key_label.backspace(false)
    }

    /// Validate the draft label before asking the worker to generate/import.
    pub fn take_key_draft_label(&mut self) -> Result<String, String> {
        let name = self.key_label.value.trim().to_string();
        if name.is_empty() {
            let msg = "Enter a label for the new SSH key".to_string();
            self.key_draft_error = Some(msg.clone());
            return Err(msg);
        }
        Ok(name)
    }

    /// Whether the draft should import a pasted PEM instead of generating.
    pub fn key_draft_wants_import(&self) -> bool {
        !self.key_pem.value.trim().is_empty()
    }

    /// Soft error banner between the PEM card and the action buttons.
    pub fn key_draft_error_banner_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Option<Rect> {
        if !self.key_drafting || self.key_draft_error.is_none() {
            return None;
        }
        let pem = self.key_draft_pem_rect(window_width, window_height)?;
        Some(Rect::new(
            pem.x,
            pem.bottom() + 8.0,
            pem.width,
            ERROR_BANNER_HEIGHT,
        ))
    }

    pub fn focus_uri(&mut self) {
        self.tab = SettingsTab::SqlSync;
        self.sql_focus = SqlSyncFocus::Uri;
        self.engine_menu_open = false;
    }

    pub fn focus_passphrase(&mut self) {
        self.tab = SettingsTab::SqlSync;
        self.sql_focus = SqlSyncFocus::Passphrase;
        self.engine_menu_open = false;
    }

    pub fn clear_sql_focus(&mut self) {
        self.sql_focus = SqlSyncFocus::None;
    }

    pub fn toggle_passphrase_visible(&mut self) {
        self.passphrase_visible = !self.passphrase_visible;
    }

    /// Cycle `sql_engine` through [`SQL_ENGINES`].
    pub fn cycle_engine(&mut self) {
        self.sql_engine = (self.sql_engine + 1) % SQL_ENGINES.len();
        self.sql_focus = SqlSyncFocus::None;
        self.engine_menu_open = false;
    }

    /// Open or close the Database Engine dropdown.
    pub fn toggle_engine_menu(&mut self) {
        self.engine_menu_open = !self.engine_menu_open;
        self.sql_focus = SqlSyncFocus::None;
        if !self.engine_menu_open {
            self.engine_menu_hover = None;
        }
    }

    pub fn close_engine_menu(&mut self) {
        self.engine_menu_open = false;
        self.engine_menu_hover = None;
    }

    /// Select an engine from the dropdown and close it.
    pub fn select_engine(&mut self, index: usize) {
        if index < SQL_ENGINES.len() {
            self.sql_engine = index;
        }
        self.engine_menu_open = false;
        self.engine_menu_hover = None;
    }

    /// Update dropdown hover; returns whether the highlight changed.
    pub fn set_engine_menu_hover(&mut self, index: Option<usize>) -> bool {
        if self.engine_menu_hover == index {
            return false;
        }
        self.engine_menu_hover = index;
        true
    }

    /// Label for the current engine selection.
    pub fn engine_label(&self) -> &'static str {
        SQL_ENGINES
            .get(self.sql_engine)
            .copied()
            .unwrap_or(SQL_ENGINES[0])
    }

    /// Placeholder for the URI field (engine-aware).
    pub fn uri_placeholder(&self) -> &'static str {
        match self.sql_engine {
            1 => "postgres://user:pass@host:5432/terminus",
            _ => "sqlite:./remote.db",
        }
    }

    /// Placeholder for the passphrase field.
    pub fn passphrase_placeholder(&self) -> &'static str {
        "Enter passphrase…"
    }

    /// Byte budget for the focused SqlSync field.
    pub fn sql_text_max_bytes(&self) -> usize {
        match self.sql_focus {
            SqlSyncFocus::Passphrase => SQL_PASSPHRASE_MAX_BYTES,
            _ => SQL_URI_MAX_BYTES,
        }
    }

    /// The SqlSync draft that owns the caret, if any. Lets the input router
    /// drive the same editing model (arrows, selection, word jumps) the
    /// inline key form already uses.
    pub fn sql_draft_active(&mut self) -> Option<&mut TextDraft> {
        match self.sql_focus {
            SqlSyncFocus::Uri => Some(&mut self.sql_uri),
            SqlSyncFocus::Passphrase => Some(&mut self.sql_passphrase),
            SqlSyncFocus::None => None,
        }
    }

    /// Insert text at the caret of the focused SqlSync field.
    pub fn insert_sql_text(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        let max = self.sql_text_max_bytes();
        self.sql_draft_active()
            .is_some_and(|draft| draft.insert(text, max, false))
    }

    pub fn dialog_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let width = MAX_WIDTH.min(window_width - 32.0).max(320.0);
        let height = (window_height * HEIGHT_RATIO)
            .min(window_height - 32.0)
            .max(360.0);
        Rect::new(
            ((window_width - width) / 2.0).max(0.0),
            ((window_height - height) / 2.0).max(0.0),
            width,
            height,
        )
    }

    pub fn close_button_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let dialog = self.dialog_rect(window_width, window_height);
        Rect::new(dialog.right() - 40.0, dialog.y + 16.0, 28.0, 28.0)
    }

    pub fn tab_rect(
        &self,
        window_width: f32,
        window_height: f32,
        tab: SettingsTab,
    ) -> Rect {
        let dialog = self.dialog_rect(window_width, window_height);
        let y = dialog.y
            + 72.0
            + match tab {
                SettingsTab::Keys => 0.0,
                SettingsTab::SqlSync => 40.0,
            };
        Rect::new(dialog.x + 12.0, y, SIDEBAR_WIDTH - 24.0, 36.0)
    }

    pub fn done_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let dialog = self.dialog_rect(window_width, window_height);
        Rect::new(dialog.right() - 88.0, dialog.bottom() - 40.0, 72.0, 28.0)
    }

    fn sql_content_origin(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> (f32, f32, f32) {
        let dialog = self.dialog_rect(window_width, window_height);
        let content_x = dialog.x + SIDEBAR_WIDTH + 24.0;
        let content_y = dialog.y + 80.0;
        let card_w = dialog.right() - content_x - 24.0;
        (content_x, content_y, card_w)
    }

    /// Content origin shared by Keys + SqlSync panes.
    pub fn keys_content_origin(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> (f32, f32, f32) {
        self.sql_content_origin(window_width, window_height)
    }

    /// Dashed "New SSH Key" CTA.
    pub fn new_key_cta_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let (x, y, w) = self.keys_content_origin(window_width, window_height);
        Rect::new(x, y + 48.0, w, KEY_CTA_HEIGHT)
    }

    /// Inline generate form under the CTA (only while drafting).
    pub fn key_draft_rect(&self, window_width: f32, window_height: f32) -> Option<Rect> {
        if !self.key_drafting {
            return None;
        }
        let cta = self.new_key_cta_rect(window_width, window_height);
        Some(Rect::new(
            cta.x,
            cta.bottom() + KEY_CTA_GAP,
            cta.width,
            KEY_DRAFT_HEIGHT,
        ))
    }

    pub fn key_draft_field_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Option<Rect> {
        let draft = self.key_draft_rect(window_width, window_height)?;
        Some(Rect::new(
            draft.x + FIELD_CARD_PAD,
            draft.y + FIELD_CARD_PAD,
            (draft.width - 2.0 * FIELD_CARD_PAD).max(40.0),
            FIELD_CARD_HEIGHT,
        ))
    }

    pub fn key_draft_pem_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Option<Rect> {
        let draft = self.key_draft_rect(window_width, window_height)?;
        Some(Rect::new(
            draft.x + FIELD_CARD_PAD,
            draft.y + FIELD_CARD_PAD + FIELD_CARD_STEP,
            (draft.width - 2.0 * FIELD_CARD_PAD).max(40.0),
            FIELD_CARD_HEIGHT,
        ))
    }

    pub fn key_draft_generate_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Option<Rect> {
        let draft = self.key_draft_rect(window_width, window_height)?;
        let cancel = self.key_draft_cancel_rect(window_width, window_height)?;
        Some(Rect::new(
            cancel.x - 8.0 - KEY_DRAFT_GENERATE_WIDTH,
            cancel.y,
            KEY_DRAFT_GENERATE_WIDTH,
            FIELD_INPUT_HEIGHT,
        ))
    }

    pub fn key_draft_cancel_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Option<Rect> {
        let draft = self.key_draft_rect(window_width, window_height)?;
        // Sit below the PEM card; leave room for the soft error banner when shown.
        let mut actions_y = draft.y + FIELD_CARD_PAD + FIELD_CARD_STEP * 2.0;
        if self.key_draft_error.is_some() {
            actions_y += ERROR_BANNER_HEIGHT + 8.0;
        }
        Some(Rect::new(
            draft.right() - FIELD_CARD_PAD - KEY_DRAFT_CANCEL_WIDTH,
            actions_y,
            KEY_DRAFT_CANCEL_WIDTH,
            FIELD_INPUT_HEIGHT,
        ))
    }

    fn keys_list_origin_y(&self, window_width: f32, window_height: f32) -> f32 {
        let cta = self.new_key_cta_rect(window_width, window_height);
        let mut y = cta.bottom() + KEY_CTA_GAP;
        if self.key_drafting {
            y += KEY_DRAFT_HEIGHT + KEY_CTA_GAP;
        }
        y
    }

    /// Row for a stored managed key.
    pub fn key_row_rect(
        &self,
        window_width: f32,
        window_height: f32,
        index: usize,
    ) -> Rect {
        let (x, _, w) = self.keys_content_origin(window_width, window_height);
        let y = self.keys_list_origin_y(window_width, window_height)
            + index as f32 * (KEY_ROW_HEIGHT + KEY_ROW_GAP);
        Rect::new(x, y, w, KEY_ROW_HEIGHT)
    }

    /// Delete control on a key row (right-aligned, vertically centred).
    pub fn key_delete_rect(
        &self,
        window_width: f32,
        window_height: f32,
        index: usize,
    ) -> Rect {
        let row = self.key_row_rect(window_width, window_height, index);
        const W: f32 = 56.0;
        const H: f32 = 24.0;
        const PAD: f32 = 14.0;
        Rect::new(row.right() - PAD - W, row.y + (row.height - H) * 0.5, W, H)
    }

    /// Engine card (read-only display / dropdown trigger).
    pub fn engine_card_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let (x, y, w) = self.sql_content_origin(window_width, window_height);
        Rect::new(x, y + 52.0, w, FIELD_CARD_HEIGHT)
    }

    /// Clickable input row inside the engine card (opens the dropdown).
    pub fn engine_input_rect(&self, window_width: f32, window_height: f32) -> Rect {
        field_input_in_card(self.engine_card_rect(window_width, window_height))
    }

    /// Floating dropdown panel under the engine input.
    pub fn engine_menu_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let input = self.engine_input_rect(window_width, window_height);
        let row_h = 32.0;
        Rect::new(
            input.x,
            input.bottom() + 4.0,
            input.width,
            SQL_ENGINES.len() as f32 * row_h + 8.0,
        )
    }

    pub fn engine_option_rect(
        &self,
        window_width: f32,
        window_height: f32,
        index: usize,
    ) -> Rect {
        let menu = self.engine_menu_rect(window_width, window_height);
        Rect::new(
            menu.x + 4.0,
            menu.y + 4.0 + index as f32 * 32.0,
            menu.width - 8.0,
            32.0,
        )
    }

    /// Connection URI field card.
    pub fn uri_card_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let (x, y, w) = self.sql_content_origin(window_width, window_height);
        Rect::new(x, y + 52.0 + FIELD_CARD_STEP, w, FIELD_CARD_HEIGHT)
    }

    pub fn uri_input_rect(&self, window_width: f32, window_height: f32) -> Rect {
        field_input_in_card(self.uri_card_rect(window_width, window_height))
    }

    /// Passphrase field card.
    pub fn passphrase_card_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let (x, y, w) = self.sql_content_origin(window_width, window_height);
        Rect::new(x, y + 52.0 + 2.0 * FIELD_CARD_STEP, w, FIELD_CARD_HEIGHT)
    }

    /// Text editable region (excludes the trailing eye toggle).
    pub fn passphrase_input_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let input =
            field_input_in_card(self.passphrase_card_rect(window_width, window_height));
        Rect::new(
            input.x,
            input.y,
            (input.width - FIELD_EYE_SLOT).max(0.0),
            input.height,
        )
    }

    /// Eye toggle hit target on the right of the passphrase input.
    pub fn passphrase_toggle_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let input =
            field_input_in_card(self.passphrase_card_rect(window_width, window_height));
        Rect::new(
            input.right() - FIELD_EYE_SLOT,
            input.y,
            FIELD_EYE_SLOT,
            input.height,
        )
    }

    /// Footer block under the three field cards: status line + action buttons.
    pub fn status_row_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let (x, y, w) = self.sql_content_origin(window_width, window_height);
        Rect::new(x, y + 52.0 + 3.0 * FIELD_CARD_STEP, w, STATUS_BLOCK_HEIGHT)
    }

    /// Full-width status text band (does not share the row with buttons).
    pub fn status_text_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let block = self.status_row_rect(window_width, window_height);
        Rect::new(
            block.x + FIELD_CARD_PAD,
            block.y + FIELD_CARD_PAD,
            (block.width - 2.0 * FIELD_CARD_PAD).max(0.0),
            STATUS_TEXT_HEIGHT,
        )
    }

    /// Soft red error panel under the action row — only when sync failed.
    pub fn error_banner_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Option<Rect> {
        let Some(msg) = self.sync_error.as_deref() else {
            return None;
        };
        if msg.trim().is_empty() {
            return None;
        }
        let row = self.status_row_rect(window_width, window_height);
        Some(Rect::new(
            row.x,
            row.bottom() + ERROR_BANNER_GAP,
            row.width,
            ERROR_BANNER_HEIGHT,
        ))
    }

    pub fn unlock_vault_button_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Rect {
        let block = self.status_row_rect(window_width, window_height);
        let y = block.y + FIELD_CARD_PAD + STATUS_TEXT_HEIGHT + STATUS_ACTIONS_GAP;
        Rect::new(
            block.right()
                - FIELD_CARD_PAD
                - SYNC_ACTION_BTN_WIDTH
                - SYNC_ACTION_BTN_GAP
                - SYNC_ACTION_BTN_WIDTH,
            y,
            SYNC_ACTION_BTN_WIDTH,
            STATUS_ACTIONS_HEIGHT,
        )
    }

    pub fn test_sync_button_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let block = self.status_row_rect(window_width, window_height);
        let y = block.y + FIELD_CARD_PAD + STATUS_TEXT_HEIGHT + STATUS_ACTIONS_GAP;
        Rect::new(
            block.right() - FIELD_CARD_PAD - SYNC_ACTION_BTN_WIDTH,
            y,
            SYNC_ACTION_BTN_WIDTH,
            STATUS_ACTIONS_HEIGHT,
        )
    }

    /// Left-side action to clear a remembered vault passphrase.
    ///
    /// `None` when nothing is stored — the control is not painted or hit-tested.
    pub fn forget_passphrase_button_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Option<Rect> {
        if !self.passphrase_remembered {
            return None;
        }
        let block = self.status_row_rect(window_width, window_height);
        let y = block.y + FIELD_CARD_PAD + STATUS_TEXT_HEIGHT + STATUS_ACTIONS_GAP;
        Some(Rect::new(
            block.x + FIELD_CARD_PAD,
            y,
            FORGET_PASSPHRASE_BTN_WIDTH,
            STATUS_ACTIONS_HEIGHT,
        ))
    }

    pub fn hit_test(
        &self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> SettingsHit {
        if !self.open {
            return SettingsHit::Consume;
        }
        let dialog = self.dialog_rect(window_width, window_height);
        if !dialog.contains(x, y) {
            return SettingsHit::Close;
        }
        if self
            .close_button_rect(window_width, window_height)
            .contains(x, y)
            || self.done_rect(window_width, window_height).contains(x, y)
        {
            return SettingsHit::Close;
        }
        if self
            .tab_rect(window_width, window_height, SettingsTab::Keys)
            .contains(x, y)
        {
            return SettingsHit::Tab(SettingsTab::Keys);
        }
        if self
            .tab_rect(window_width, window_height, SettingsTab::SqlSync)
            .contains(x, y)
        {
            return SettingsHit::Tab(SettingsTab::SqlSync);
        }
        if self.tab == SettingsTab::Keys {
            if let Some(gen) = self.key_draft_generate_rect(window_width, window_height) {
                if gen.contains(x, y) {
                    return SettingsHit::GenerateKey;
                }
            }
            if let Some(cancel) = self.key_draft_cancel_rect(window_width, window_height)
            {
                if cancel.contains(x, y) {
                    return SettingsHit::CancelKeyDraft;
                }
            }
            if let Some(field) = self.key_draft_field_rect(window_width, window_height) {
                if field.contains(x, y) {
                    return SettingsHit::FocusKeyDraft;
                }
            }
            if let Some(pem) = self.key_draft_pem_rect(window_width, window_height) {
                if pem.contains(x, y) {
                    return SettingsHit::FocusKeyPem;
                }
            }
            if self
                .new_key_cta_rect(window_width, window_height)
                .contains(x, y)
            {
                return SettingsHit::NewKey;
            }
            for i in 0..self.keys.len() {
                if self
                    .key_delete_rect(window_width, window_height, i)
                    .contains(x, y)
                {
                    return SettingsHit::DeleteKey(i);
                }
            }
        }
        if self.tab == SettingsTab::SqlSync {
            // Dropdown options sit above everything else while open.
            if self.engine_menu_open {
                for i in 0..SQL_ENGINES.len() {
                    if self
                        .engine_option_rect(window_width, window_height, i)
                        .contains(x, y)
                    {
                        return SettingsHit::SelectEngine(i);
                    }
                }
            }
            if self
                .test_sync_button_rect(window_width, window_height)
                .contains(x, y)
            {
                return SettingsHit::TestSync;
            }
            if self
                .unlock_vault_button_rect(window_width, window_height)
                .contains(x, y)
            {
                return SettingsHit::UnlockVault;
            }
            if let Some(forget) =
                self.forget_passphrase_button_rect(window_width, window_height)
            {
                if forget.contains(x, y) {
                    return SettingsHit::ForgetPassphrase;
                }
            }
            if self
                .passphrase_toggle_rect(window_width, window_height)
                .contains(x, y)
            {
                return SettingsHit::TogglePassphrase;
            }
            if self
                .uri_input_rect(window_width, window_height)
                .contains(x, y)
                || self
                    .uri_card_rect(window_width, window_height)
                    .contains(x, y)
            {
                return SettingsHit::FocusUri;
            }
            if self
                .passphrase_input_rect(window_width, window_height)
                .contains(x, y)
                || self
                    .passphrase_card_rect(window_width, window_height)
                    .contains(x, y)
            {
                return SettingsHit::FocusPassphrase;
            }
            if self
                .engine_input_rect(window_width, window_height)
                .contains(x, y)
                || self
                    .engine_card_rect(window_width, window_height)
                    .contains(x, y)
            {
                return SettingsHit::ToggleEngineMenu;
            }
        }
        SettingsHit::Consume
    }

    /// Cursor affordance for a point inside the settings dialog.
    pub fn cursor_at(
        &self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> crate::chrome::ChromeCursor {
        use crate::chrome::ChromeCursor;
        if !self.open {
            return ChromeCursor::Default;
        }
        match self.hit_test(window_width, window_height, x, y) {
            SettingsHit::FocusUri
            | SettingsHit::FocusPassphrase
            | SettingsHit::FocusKeyDraft
            | SettingsHit::FocusKeyPem => ChromeCursor::Text,
            SettingsHit::Consume => ChromeCursor::Default,
            SettingsHit::Close
            | SettingsHit::Done
            | SettingsHit::Tab(_)
            | SettingsHit::NewKey
            | SettingsHit::DeleteKey(_)
            | SettingsHit::GenerateKey
            | SettingsHit::CancelKeyDraft
            | SettingsHit::ToggleEngineMenu
            | SettingsHit::SelectEngine(_)
            | SettingsHit::TogglePassphrase
            | SettingsHit::UnlockVault
            | SettingsHit::ForgetPassphrase
            | SettingsHit::TestSync => ChromeCursor::Pointer,
        }
    }

    /// Hover handling for engine dropdown options and Keys Delete controls.
    pub fn handle_hover(
        &mut self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> bool {
        let mut changed = false;

        if self.tab == SettingsTab::Keys {
            let mut row_hover = None;
            let mut del_hover = None;
            for i in 0..self.keys.len() {
                if self
                    .key_row_rect(window_width, window_height, i)
                    .contains(x, y)
                {
                    row_hover = Some(i);
                    if self
                        .key_delete_rect(window_width, window_height, i)
                        .contains(x, y)
                    {
                        del_hover = Some(i);
                    }
                    break;
                }
            }
            changed |= self.set_key_row_hover(row_hover);
            changed |= self.set_key_delete_hover(del_hover);
        } else {
            changed |= self.set_key_row_hover(None);
            changed |= self.set_key_delete_hover(None);
        }

        if self.engine_menu_open {
            let mut hover = None;
            for i in 0..SQL_ENGINES.len() {
                if self
                    .engine_option_rect(window_width, window_height, i)
                    .contains(x, y)
                {
                    hover = Some(i);
                    break;
                }
            }
            changed |= self.set_engine_menu_hover(hover);
        } else {
            changed |= self.set_engine_menu_hover(None);
        }

        changed
    }

    pub fn set_key_delete_hover(&mut self, index: Option<usize>) -> bool {
        if self.key_delete_hover == index {
            return false;
        }
        self.key_delete_hover = index;
        true
    }

    pub fn set_key_row_hover(&mut self, index: Option<usize>) -> bool {
        if self.key_row_hover == index {
            return false;
        }
        self.key_row_hover = index;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_empty_not_mock_connected() {
        let s = SettingsModal::default();
        assert!(s.sql_uri.value.is_empty());
        assert!(s.sql_passphrase.value.is_empty());
        assert_eq!(s.sql_uri.caret, 0);
        assert!(!s.sync_connected);
        assert_eq!(s.sync_status, "Not configured");
        assert!(!s.vault_unlocked);
    }

    #[test]
    fn sql_text_edits_go_to_the_focused_field() {
        let mut s = SettingsModal::default();
        assert!(!s.insert_sql_text("x"));
        s.focus_uri();
        assert!(s.insert_sql_text("sqlite:./remote.db"));
        assert_eq!(s.sql_uri.value, "sqlite:./remote.db");
        s.focus_passphrase();
        assert!(s.insert_sql_text("secretpass"));
        assert_eq!(s.sql_passphrase.value, "secretpass");
        assert!(s
            .sql_draft_active()
            .expect("passphrase owns the caret")
            .backspace(false));
        assert_eq!(s.sql_passphrase.value, "secretpas");
    }

    #[test]
    fn sql_caret_moves_and_typing_lands_at_the_caret() {
        use crate::TextMoveKind;
        let mut s = SettingsModal::default();
        s.focus_uri();
        assert!(s.insert_sql_text("sqlite:./remote.db"));
        assert_eq!(s.sql_uri.caret, "sqlite:./remote.db".chars().count());

        let draft = s.sql_draft_active().expect("uri owns the caret");
        assert!(draft.move_home(TextMoveKind::Collapse));
        draft.move_right(TextMoveKind::Collapse, false);
        assert_eq!(s.sql_uri.caret, 1);
        assert!(s.insert_sql_text("X"));
        assert_eq!(s.sql_uri.value, "sXqlite:./remote.db");
        assert_eq!(s.sql_uri.caret, 2);

        let draft = s.sql_draft_active().expect("uri owns the caret");
        assert!(draft.move_end(TextMoveKind::Collapse));
        assert_eq!(s.sql_uri.caret, s.sql_uri.value.chars().count());
    }

    #[test]
    fn sql_word_jumps_move_by_whole_words() {
        use crate::TextMoveKind;
        let mut s = SettingsModal::default();
        s.focus_uri();
        s.insert_sql_text("postgres://host/db");
        {
            let draft = s.sql_draft_active().expect("uri owns the caret");
            assert!(draft.move_left(TextMoveKind::Collapse, true));
        }
        assert_eq!(s.sql_uri.caret, "postgres://host/".chars().count());
        {
            let draft = s.sql_draft_active().expect("uri owns the caret");
            assert!(draft.move_right(TextMoveKind::Collapse, true));
        }
        assert_eq!(s.sql_uri.caret, s.sql_uri.value.chars().count());
    }

    #[test]
    fn sql_shift_arrows_select_and_typing_replaces_the_selection() {
        use crate::TextMoveKind;
        let mut s = SettingsModal::default();
        s.focus_uri();
        s.insert_sql_text("abcdef");
        {
            let draft = s.sql_draft_active().expect("uri owns the caret");
            draft.move_left(TextMoveKind::Extend, false);
            draft.move_left(TextMoveKind::Extend, false);
        }
        assert_eq!(s.sql_uri.selection_range(), Some((4, 6)));
        assert!(s.insert_sql_text("XY"));
        assert_eq!(s.sql_uri.value, "abcdXY");
        assert_eq!(s.sql_uri.selection_range(), None);
    }

    #[test]
    fn sql_select_all_then_typing_replaces_the_whole_value() {
        let mut s = SettingsModal::default();
        s.focus_passphrase();
        s.insert_sql_text("secret");
        let draft = s.sql_draft_active().expect("passphrase owns the caret");
        assert!(draft.select_all());
        assert!(s.insert_sql_text("new"));
        assert_eq!(s.sql_passphrase.value, "new");
        assert_eq!(s.sql_passphrase.caret, 3);
        assert_eq!(s.sql_passphrase.selection_range(), None);
    }

    #[test]
    fn sql_backspace_and_delete_follow_the_caret() {
        use crate::TextMoveKind;
        let mut s = SettingsModal::default();
        s.focus_uri();
        s.insert_sql_text("abc");
        {
            let draft = s.sql_draft_active().expect("uri owns the caret");
            draft.move_home(TextMoveKind::Collapse);
        }
        assert!(!s
            .sql_draft_active()
            .expect("uri owns the caret")
            .backspace(false));
        {
            let draft = s.sql_draft_active().expect("uri owns the caret");
            draft.move_end(TextMoveKind::Collapse);
        }
        assert!(s
            .sql_draft_active()
            .expect("uri owns the caret")
            .backspace(false));
        assert_eq!(s.sql_uri.value, "ab");
        {
            let draft = s.sql_draft_active().expect("uri owns the caret");
            draft.move_home(TextMoveKind::Collapse);
            assert!(draft.delete_forward(false));
        }
        assert_eq!(s.sql_uri.value, "b");
    }

    #[test]
    fn sql_word_backspace_removes_a_whole_word() {
        let mut s = SettingsModal::default();
        s.focus_uri();
        s.insert_sql_text("sqlite:./remote.db");
        let draft = s.sql_draft_active().expect("uri owns the caret");
        assert!(draft.backspace(true));
        assert_eq!(s.sql_uri.value, "sqlite:./remote.");
    }

    #[test]
    fn sql_fields_accept_spaces_and_reject_control_characters() {
        let mut s = SettingsModal::default();
        s.focus_uri();
        assert!(s.insert_sql_text(" "));
        assert_eq!(s.sql_uri.value, " ");
        assert!(!s.insert_sql_text("a\nb"));
        assert_eq!(s.sql_uri.value, " ");
        s.focus_passphrase();
        assert!(!s.insert_sql_text("\t"));
    }

    #[test]
    fn uri_paint_exposes_caret_prefix_and_selection() {
        use crate::TextMoveKind;
        let mut s = SettingsModal::default();
        s.focus_uri();
        s.insert_sql_text("sqlite");
        {
            let draft = s.sql_draft_active().expect("uri owns the caret");
            draft.move_home(TextMoveKind::Collapse);
            draft.move_right(TextMoveKind::Collapse, false);
        }
        let paint = s.uri_field_paint();
        assert_eq!(paint.text, "sqlite");
        assert_eq!(paint.caret_prefix, "s");
        assert!(paint.show_caret);
        assert!(!paint.placeholder);
        assert_eq!(paint.selection, None);

        {
            let draft = s.sql_draft_active().expect("uri owns the caret");
            draft.move_right(TextMoveKind::Extend, false);
            draft.move_right(TextMoveKind::Extend, false);
        }
        let paint = s.uri_field_paint();
        assert_eq!(paint.selection, Some((1, 3)));
        assert_eq!(paint.caret_prefix, "sql");
    }

    #[test]
    fn passphrase_paint_masks_caret_prefix_and_keeps_selection() {
        let mut s = SettingsModal::default();
        s.focus_passphrase();
        s.insert_sql_text("secret");
        {
            let draft = s.sql_draft_active().expect("passphrase owns the caret");
            draft.select_all();
        }
        let paint = s.passphrase_field_paint();
        assert_eq!(
            paint.text,
            "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
        );
        assert_eq!(
            paint.caret_prefix,
            "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
        );
        assert_eq!(paint.selection, Some((0, 6)));
        assert!(paint.show_caret);

        s.toggle_passphrase_visible();
        let paint = s.passphrase_field_paint();
        assert_eq!(paint.text, "secret");
        assert_eq!(paint.caret_prefix, "secret");
    }

    #[test]
    fn blurring_a_sql_field_hides_its_caret_and_selection() {
        use crate::TextMoveKind;
        let mut s = SettingsModal::default();
        s.focus_uri();
        s.insert_sql_text("sqlite");
        {
            let draft = s.sql_draft_active().expect("uri owns the caret");
            draft.move_home(TextMoveKind::Extend);
        }
        assert!(s.uri_field_paint().show_caret);
        assert!(s.uri_field_paint().selection.is_some());
        s.clear_sql_focus();
        let paint = s.uri_field_paint();
        assert!(!paint.show_caret);
        assert_eq!(paint.selection, None);
    }

    #[test]
    fn hit_test_finds_new_key_cta_and_generate_form() {
        let mut s = SettingsModal::default();
        s.open_tab(SettingsTab::Keys);
        let (w, h) = (1000.0, 800.0);
        let cta = s.new_key_cta_rect(w, h);
        assert_eq!(
            s.hit_test(w, h, cta.x + 4.0, cta.y + 4.0),
            SettingsHit::NewKey
        );
        s.open_key_draft();
        assert!(s.key_drafting);
        let field = s.key_draft_field_rect(w, h).expect("field");
        assert_eq!(
            s.hit_test(w, h, field.x + 2.0, field.y + 2.0),
            SettingsHit::FocusKeyDraft
        );
        let gen = s.key_draft_generate_rect(w, h).expect("generate");
        assert_eq!(
            s.hit_test(w, h, gen.x + 2.0, gen.y + 2.0),
            SettingsHit::GenerateKey
        );
        s.insert_key_draft_text("Laptop");
        assert_eq!(s.take_key_draft_label().unwrap(), "Laptop");
    }

    #[test]
    fn hit_test_finds_sql_fields_and_buttons() {
        let mut s = SettingsModal::default();
        s.open_tab(SettingsTab::SqlSync);
        let (w, h) = (1000.0, 800.0);
        let uri = s.uri_input_rect(w, h);
        assert_eq!(
            s.hit_test(w, h, uri.x + 2.0, uri.y + 2.0),
            SettingsHit::FocusUri
        );
        let pass = s.passphrase_input_rect(w, h);
        assert_eq!(
            s.hit_test(w, h, pass.x + 2.0, pass.y + 2.0),
            SettingsHit::FocusPassphrase
        );
        let unlock = s.unlock_vault_button_rect(w, h);
        assert_eq!(
            s.hit_test(w, h, unlock.x + 2.0, unlock.y + 2.0),
            SettingsHit::UnlockVault
        );
        let test = s.test_sync_button_rect(w, h);
        assert_eq!(
            s.hit_test(w, h, test.x + 2.0, test.y + 2.0),
            SettingsHit::TestSync
        );
        let eye = s.passphrase_toggle_rect(w, h);
        assert_eq!(
            s.hit_test(w, h, eye.x + 2.0, eye.y + 2.0),
            SettingsHit::TogglePassphrase
        );
        assert!(s.forget_passphrase_button_rect(w, h).is_none());
        s.set_passphrase_remembered(true);
        let forget = s.forget_passphrase_button_rect(w, h).expect("forget btn");
        assert_eq!(
            s.hit_test(w, h, forget.x + 2.0, forget.y + 2.0),
            SettingsHit::ForgetPassphrase
        );
        // Must not overlap Unlock / Test Sync.
        assert!(forget.right() <= unlock.x);
        let engine = s.engine_card_rect(w, h);
        assert_eq!(
            s.hit_test(w, h, engine.x + 2.0, engine.y + 2.0),
            SettingsHit::ToggleEngineMenu
        );
        s.toggle_engine_menu();
        assert!(s.engine_menu_open);
        let opt = s.engine_option_rect(w, h, 1);
        assert_eq!(
            s.hit_test(w, h, opt.x + 2.0, opt.y + 2.0),
            SettingsHit::SelectEngine(1)
        );
        s.select_engine(1);
        assert_eq!(s.engine_label(), "PostgreSQL");
        assert!(!s.engine_menu_open);
    }

    #[test]
    fn apply_sync_status_updates_connected_and_line() {
        let mut s = SettingsModal::default();
        s.apply_sync_status(SyncUiStatus {
            uri: "sqlite:./remote.db".into(),
            connected: true,
            vault_unlocked: true,
            status_line: "Last synced just now".into(),
            is_error: false,
        });
        assert!(s.sync_connected);
        assert!(s.vault_unlocked);
        assert_eq!(s.sync_status, "Last synced just now");
        assert!(!s.sync_status_is_error());
        assert_eq!(s.sql_uri.value, "sqlite:./remote.db");
    }

    #[test]
    fn empty_worker_uri_does_not_wipe_local_draft() {
        let mut s = SettingsModal::default();
        s.sql_uri = TextDraft::new("sqlite:./draft.db");
        s.apply_sync_status(SyncUiStatus {
            uri: String::new(),
            connected: false,
            vault_unlocked: false,
            status_line: "Not configured".into(),
            is_error: false,
        });
        assert_eq!(s.sql_uri.value, "sqlite:./draft.db");
    }

    #[test]
    fn vault_feedback_lands_on_sql_sync_status_not_as_success_when_locked() {
        let mut s = SettingsModal::default();
        assert_eq!(s.sync_status, "Not configured");
        s.apply_vault_feedback(
            "Vault passphrase must be at least 8 characters".into(),
            false,
        );
        assert_eq!(s.sync_status, "Not configured");
        assert_eq!(
            s.sync_error.as_deref(),
            Some("Vault passphrase must be at least 8 characters")
        );
        assert!(s.sync_status_is_error());
        assert!(!s.vault_unlocked);

        s.apply_vault_feedback("Vault unlocked".into(), true);
        assert_eq!(s.sync_status, "Vault unlocked");
        assert!(s.sync_error.is_none());
        assert!(!s.sync_status_is_error());
        assert!(s.vault_unlocked);
    }

    #[test]
    fn sync_error_status_is_flagged() {
        let mut s = SettingsModal::default();
        s.apply_sync_status(SyncUiStatus {
            uri: "postgres://x".into(),
            connected: false,
            vault_unlocked: false,
            status_line: "connection refused".into(),
            is_error: true,
        });
        assert_eq!(s.sync_status, "Not configured");
        assert_eq!(s.sync_error.as_deref(), Some("connection refused"));
        assert!(s.sync_status_is_error());
    }

    #[test]
    fn error_banner_only_when_sync_failed() {
        let mut s = SettingsModal::default();
        s.open_tab(SettingsTab::SqlSync);
        let (w, h) = (1000.0, 800.0);
        assert!(s.error_banner_rect(w, h).is_none());

        s.apply_sync_status(SyncUiStatus {
            uri: "sqlite:./x.db".into(),
            connected: false,
            vault_unlocked: false,
            status_line: "connection refused".into(),
            is_error: true,
        });
        let banner = s.error_banner_rect(w, h).expect("banner");
        let row = s.status_row_rect(w, h);
        assert!(banner.y >= row.bottom());
        assert!((banner.width - row.width).abs() < 0.01);
        assert!((banner.height - ERROR_BANNER_HEIGHT).abs() < 0.01);
        assert_eq!(s.sync_status, "Not configured");

        s.apply_sync_status(SyncUiStatus {
            uri: "sqlite:./x.db".into(),
            connected: true,
            vault_unlocked: true,
            status_line: "Sync ok".into(),
            is_error: false,
        });
        assert!(s.error_banner_rect(w, h).is_none());
        assert_eq!(s.sync_status, "Sync ok");
    }

    #[test]
    fn status_text_sits_above_action_buttons_full_width() {
        let mut s = SettingsModal::default();
        s.open_tab(SettingsTab::SqlSync);
        let (w, h) = (1000.0, 800.0);
        let block = s.status_row_rect(w, h);
        let text = s.status_text_rect(w, h);
        let unlock = s.unlock_vault_button_rect(w, h);
        let test = s.test_sync_button_rect(w, h);

        assert!(text.bottom() <= unlock.y);
        assert!(text.width > block.width * 0.7);
        assert!((unlock.y - test.y).abs() < 0.01);
        assert!(test.x > unlock.right());
        assert!(test.right() <= block.right() + 0.01);
    }

    #[test]
    fn field_paint_hides_placeholder_when_focused_and_shows_caret() {
        let mut s = SettingsModal::default();
        let idle = s.uri_field_paint();
        assert!(idle.placeholder);
        assert!(!idle.show_caret);

        s.focus_uri();
        let focused = s.uri_field_paint();
        assert!(!focused.placeholder);
        assert!(focused.show_caret);
        assert!(focused.text.is_empty());

        s.insert_sql_text("sqlite:./x.db");
        let typed = s.uri_field_paint();
        assert_eq!(typed.text, "sqlite:./x.db");
        assert!(!typed.placeholder);
        assert!(typed.show_caret);
    }

    #[test]
    fn passphrase_paint_never_fakes_full_bullet_placeholder() {
        let mut s = SettingsModal::default();
        let idle = s.passphrase_field_paint();
        assert!(idle.placeholder);
        assert_eq!(idle.text, "Enter passphrase…");

        s.focus_passphrase();
        let focused_empty = s.passphrase_field_paint();
        assert!(!focused_empty.placeholder);
        assert!(focused_empty.text.is_empty());
        assert!(focused_empty.show_caret);

        s.insert_sql_text("secret");
        let masked = s.passphrase_field_paint();
        assert_eq!(masked.text, "••••••");
        assert!(!masked.placeholder);

        s.toggle_passphrase_visible();
        let visible = s.passphrase_field_paint();
        assert_eq!(visible.text, "secret");
    }

    #[test]
    fn field_cards_use_equal_padding_and_eye_clears_text() {
        let mut s = SettingsModal::default();
        s.open_tab(SettingsTab::SqlSync);
        let (w, h) = (1000.0, 800.0);
        let card = s.passphrase_card_rect(w, h);
        let input = field_input_in_card(card);
        assert!((input.x - card.x - FIELD_CARD_PAD).abs() < 0.01);
        assert!((card.right() - input.right() - FIELD_CARD_PAD).abs() < 0.01);
        assert!((card.bottom() - input.bottom() - FIELD_CARD_PAD).abs() < 0.01);

        let text = s.passphrase_input_rect(w, h);
        let eye = s.passphrase_toggle_rect(w, h);
        assert!(text.right() <= eye.x + 0.01);
        assert!((eye.right() - input.right()).abs() < 0.01);
        assert!(!crate::overlap::rects_overlap(text, eye));
    }
}
