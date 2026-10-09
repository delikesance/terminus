use super::*;
use crate::components::input::FieldPaint;
use crate::geom::Rect;

impl SettingsModal {
    /// Replace the SSH key list (from `Store::list_identities`).
    pub fn set_keys(&mut self, keys: Vec<SshKeyItem>) {
        self.keys = keys;
    }

    /// Apply worker-driven sync/vault status onto the pane.
    pub fn apply_sync_status(&mut self, snap: SyncUiStatus) {
        // Never wipe a non-empty local draft with an empty worker URI.
        // While the URI field is focused, leave the draft alone entirely.
        if self.sql_focus != SqlSyncFocus::Uri && !snap.uri.is_empty() {
            self.sql_uri = TextDraft::new(snap.uri);
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
        self.key_draft_passphrase_focused = false;
        self.key_label.clear();
        self.key_pem.clear();
        self.key_passphrase.clear();
        self.key_draft_error = None;
        self.sql_focus = SqlSyncFocus::None;
    }

    pub fn close_key_draft(&mut self) {
        self.key_drafting = false;
        self.key_draft_focused = false;
        self.key_draft_pem_focused = false;
        self.key_draft_passphrase_focused = false;
        self.key_label.clear();
        self.key_pem.clear();
        self.key_passphrase.clear();
        self.key_draft_error = None;
    }

    pub fn focus_key_draft(&mut self) {
        if self.key_drafting {
            self.key_draft_focused = true;
            self.key_draft_pem_focused = false;
            self.key_draft_passphrase_focused = false;
            self.sql_focus = SqlSyncFocus::None;
        }
    }

    pub fn focus_key_pem(&mut self) {
        if self.key_drafting {
            self.key_draft_pem_focused = true;
            self.key_draft_focused = false;
            self.key_draft_passphrase_focused = false;
            self.sql_focus = SqlSyncFocus::None;
        }
    }

    pub fn focus_key_passphrase(&mut self) {
        if self.key_drafting {
            self.key_draft_passphrase_focused = true;
            self.key_draft_focused = false;
            self.key_draft_pem_focused = false;
            self.sql_focus = SqlSyncFocus::None;
        }
    }

    /// Tab order through the draft form: label → PEM → passphrase → label.
    pub fn focus_next_key_field(&mut self) {
        if self.key_draft_focused {
            self.focus_key_pem();
        } else if self.key_draft_pem_focused {
            self.focus_key_passphrase();
        } else {
            self.focus_key_draft();
        }
    }

    /// Passphrase to decrypt the pasted key, when one was typed.
    pub fn key_draft_passphrase(&self) -> Option<String> {
        let value = self.key_passphrase.value.clone();
        (!value.is_empty()).then_some(value)
    }

    /// Active text draft while key drafting, if any.
    pub fn key_draft_active(&mut self) -> Option<&mut TextDraft> {
        if !self.key_drafting {
            return None;
        }
        if self.key_draft_pem_focused {
            Some(&mut self.key_pem)
        } else if self.key_draft_passphrase_focused {
            Some(&mut self.key_passphrase)
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
        } else if self.key_draft_passphrase_focused {
            self.key_passphrase
                .insert(text, KEY_LABEL_MAX_BYTES * 4, false)
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
        if self.key_draft_passphrase_focused {
            return self.key_passphrase.backspace(false);
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
        let pass = self.key_draft_passphrase_rect(window_width, window_height)?;
        Some(Rect::new(
            pass.x,
            pass.bottom() + 8.0,
            pass.width,
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
}
