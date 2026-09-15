//! Settings modal: SSH Keys + Remote SQL Sync.

use crate::geom::Rect;

pub const MAX_WIDTH: f32 = 768.0;
pub const SIDEBAR_WIDTH: f32 = 224.0;
pub const HEIGHT_RATIO: f32 = 0.78;
pub const RADIUS: f32 = 16.0;

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
    pub sql_uri: String,
    pub sql_passphrase: String,
    pub passphrase_visible: bool,
    pub sync_connected: bool,
    pub sync_status: String,
    pub vault_unlocked: bool,
    pub sql_focus: SqlSyncFocus,
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
            sql_uri: String::new(),
            sql_passphrase: String::new(),
            passphrase_visible: false,
            sync_connected: false,
            sync_status: "Not configured".into(),
            vault_unlocked: false,
            sql_focus: SqlSyncFocus::None,
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
    /// Open / close the Database Engine dropdown.
    ToggleEngineMenu,
    /// Pick an engine from the open dropdown.
    SelectEngine(usize),
    FocusUri,
    FocusPassphrase,
    TogglePassphrase,
    /// Unlock / create the Argon2 vault with the passphrase field.
    UnlockVault,
    /// Persist URI and run SyncEngine::sync_now.
    TestSync,
    Done,
}

/// What one SqlSync text field should paint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlFieldPaint {
    pub text: String,
    pub placeholder: bool,
    pub show_caret: bool,
}

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
                self.sql_uri = snap.uri;
            }
        }
        self.sync_connected = snap.connected;
        self.vault_unlocked = snap.vault_unlocked;
        if !snap.status_line.is_empty() {
            self.sync_status = snap.status_line;
        }
    }

    /// Paint model for the Connection URI field.
    pub fn uri_field_paint(&self) -> SqlFieldPaint {
        let focused = self.sql_focus == SqlSyncFocus::Uri;
        if self.sql_uri.is_empty() && !focused {
            SqlFieldPaint {
                text: self.uri_placeholder().to_string(),
                placeholder: true,
                show_caret: false,
            }
        } else {
            SqlFieldPaint {
                text: self.sql_uri.clone(),
                placeholder: false,
                show_caret: focused,
            }
        }
    }

    /// Paint model for the passphrase field (masking included).
    pub fn passphrase_field_paint(&self) -> SqlFieldPaint {
        let focused = self.sql_focus == SqlSyncFocus::Passphrase;
        if self.sql_passphrase.is_empty() && !focused {
            SqlFieldPaint {
                text: self.passphrase_placeholder().to_string(),
                placeholder: true,
                show_caret: false,
            }
        } else if self.sql_passphrase.is_empty() {
            SqlFieldPaint {
                text: String::new(),
                placeholder: false,
                show_caret: focused,
            }
        } else if self.passphrase_visible {
            SqlFieldPaint {
                text: self.sql_passphrase.clone(),
                placeholder: false,
                show_caret: focused,
            }
        } else {
            SqlFieldPaint {
                text: "•".repeat(self.sql_passphrase.chars().count()),
                placeholder: false,
                show_caret: focused,
            }
        }
    }

    pub fn open_tab(&mut self, tab: SettingsTab) {
        self.tab = tab;
        self.open = true;
        if tab != SettingsTab::SqlSync {
            self.sql_focus = SqlSyncFocus::None;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.sql_focus = SqlSyncFocus::None;
        self.engine_menu_open = false;
        self.engine_menu_hover = None;
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

    /// Insert text into the focused SqlSync field.
    pub fn insert_sql_text(&mut self, text: &str) -> bool {
        if text.is_empty() || text.chars().any(char::is_control) {
            return false;
        }
        match self.sql_focus {
            SqlSyncFocus::Uri => {
                self.sql_uri.push_str(text);
                true
            }
            SqlSyncFocus::Passphrase => {
                self.sql_passphrase.push_str(text);
                true
            }
            SqlSyncFocus::None => false,
        }
    }

    /// Backspace in the focused SqlSync field.
    pub fn sql_backspace(&mut self) -> bool {
        match self.sql_focus {
            SqlSyncFocus::Uri => {
                if self.sql_uri.pop().is_some() {
                    true
                } else {
                    false
                }
            }
            SqlSyncFocus::Passphrase => self.sql_passphrase.pop().is_some(),
            SqlSyncFocus::None => false,
        }
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
        let y = dialog.y + 72.0 + match tab {
            SettingsTab::Keys => 0.0,
            SettingsTab::SqlSync => 40.0,
        };
        Rect::new(dialog.x + 12.0, y, SIDEBAR_WIDTH - 24.0, 36.0)
    }

    pub fn done_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let dialog = self.dialog_rect(window_width, window_height);
        Rect::new(dialog.right() - 88.0, dialog.bottom() - 40.0, 72.0, 28.0)
    }

    fn sql_content_origin(&self, window_width: f32, window_height: f32) -> (f32, f32, f32) {
        let dialog = self.dialog_rect(window_width, window_height);
        let content_x = dialog.x + SIDEBAR_WIDTH + 24.0;
        let content_y = dialog.y + 80.0;
        let card_w = dialog.right() - content_x - 24.0;
        (content_x, content_y, card_w)
    }

    /// Engine card (read-only display / dropdown trigger).
    pub fn engine_card_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let (x, y, w) = self.sql_content_origin(window_width, window_height);
        Rect::new(x, y + 52.0, w, 64.0)
    }

    /// Clickable input row inside the engine card (opens the dropdown).
    pub fn engine_input_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let card = self.engine_card_rect(window_width, window_height);
        Rect::new(card.x + 16.0, card.y + 30.0, card.width - 32.0, 26.0)
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
        Rect::new(menu.x + 4.0, menu.y + 4.0 + index as f32 * 32.0, menu.width - 8.0, 32.0)
    }

    /// Connection URI field card.
    pub fn uri_card_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let (x, y, w) = self.sql_content_origin(window_width, window_height);
        Rect::new(x, y + 52.0 + 76.0, w, 64.0)
    }

    pub fn uri_input_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let card = self.uri_card_rect(window_width, window_height);
        Rect::new(card.x + 16.0, card.y + 30.0, card.width - 32.0, 26.0)
    }

    /// Passphrase field card.
    pub fn passphrase_card_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let (x, y, w) = self.sql_content_origin(window_width, window_height);
        Rect::new(x, y + 52.0 + 2.0 * 76.0, w, 64.0)
    }

    pub fn passphrase_input_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let card = self.passphrase_card_rect(window_width, window_height);
        Rect::new(card.x + 16.0, card.y + 30.0, card.width - 56.0, 26.0)
    }

    pub fn passphrase_toggle_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let card = self.passphrase_card_rect(window_width, window_height);
        Rect::new(card.right() - 36.0, card.y + 30.0, 24.0, 26.0)
    }

    /// Status row under the three field cards.
    pub fn status_row_rect(&self, window_width: f32, window_height: f32) -> Rect {
        let (x, y, w) = self.sql_content_origin(window_width, window_height);
        Rect::new(x, y + 52.0 + 3.0 * 76.0, w, 48.0)
    }

    pub fn unlock_vault_button_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Rect {
        let row = self.status_row_rect(window_width, window_height);
        let btn_w = 110.0;
        Rect::new(row.right() - btn_w - 12.0 - 118.0, row.y + 10.0, btn_w, 28.0)
    }

    pub fn test_sync_button_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Rect {
        let row = self.status_row_rect(window_width, window_height);
        let btn_w = 110.0;
        Rect::new(row.right() - btn_w - 12.0, row.y + 10.0, btn_w, 28.0)
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
        if self.close_button_rect(window_width, window_height).contains(x, y)
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
            SettingsHit::FocusUri | SettingsHit::FocusPassphrase => ChromeCursor::Text,
            SettingsHit::Consume => ChromeCursor::Default,
            SettingsHit::Close | SettingsHit::Done | SettingsHit::Tab(_)
            | SettingsHit::NewKey
            | SettingsHit::DeleteKey(_)
            | SettingsHit::ToggleEngineMenu
            | SettingsHit::SelectEngine(_)
            | SettingsHit::TogglePassphrase
            | SettingsHit::UnlockVault
            | SettingsHit::TestSync => ChromeCursor::Pointer,
        }
    }

    /// Hover handling for the engine dropdown; returns whether to repaint.
    pub fn handle_hover(&mut self, window_width: f32, window_height: f32, x: f32, y: f32) -> bool {
        if !self.engine_menu_open {
            return self.set_engine_menu_hover(None);
        }
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
        self.set_engine_menu_hover(hover)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_empty_not_mock_connected() {
        let s = SettingsModal::default();
        assert!(s.sql_uri.is_empty());
        assert!(s.sql_passphrase.is_empty());
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
        assert_eq!(s.sql_uri, "sqlite:./remote.db");
        s.focus_passphrase();
        assert!(s.insert_sql_text("secretpass"));
        assert_eq!(s.sql_passphrase, "secretpass");
        assert!(s.sql_backspace());
        assert_eq!(s.sql_passphrase, "secretpas");
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
        });
        assert!(s.sync_connected);
        assert!(s.vault_unlocked);
        assert_eq!(s.sync_status, "Last synced just now");
        assert_eq!(s.sql_uri, "sqlite:./remote.db");
    }

    #[test]
    fn empty_worker_uri_does_not_wipe_local_draft() {
        let mut s = SettingsModal::default();
        s.sql_uri = "sqlite:./draft.db".into();
        s.apply_sync_status(SyncUiStatus {
            uri: String::new(),
            connected: false,
            vault_unlocked: false,
            status_line: "Not configured".into(),
        });
        assert_eq!(s.sql_uri, "sqlite:./draft.db");
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
}
