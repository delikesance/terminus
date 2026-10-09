//! Sync tab: description, Database segmented (SQLite / PostgreSQL), the
//! database file / connection string field, a status row with Sync now and
//! a Save button.

use super::{button_spec, column, edit_draft, Key, Measure, SettingsAction};
use crate::components::button::{ButtonKind, ButtonSize};
use crate::components::feedback::StatusKind;
use crate::components::input::TextDraft;
use crate::components::input::{field_layout, FieldKind, FieldLayout, HELPER_HEIGHT};
use crate::components::selection::{SegmentedLayout, SegmentedSize};
use crate::geom::Rect;
use crate::settings::{SQL_ENGINES, SQL_URI_MAX_BYTES};

pub const INTRO: &str =
    "Keep your servers, groups and snippets in step across your computers.";
pub const SECTION_GAP: f32 = 26.0;
pub const MAX_WIDTH: f32 = 760.0;
pub const INTRO_HEIGHT: f32 = 20.0;
pub const LABEL_HEIGHT: f32 = 16.0;
pub const LABEL_GAP: f32 = 8.0;
pub const STATUS_HEIGHT: f32 = 68.0;
pub const STATUS_PAD_X: f32 = 18.0;
pub const SEGMENT_LABEL_FONT: f32 = 13.0;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncStatus {
    pub connected: bool,
    pub vault_unlocked: bool,
    /// Worker line ("Synced 2 min ago", an error…). Empty falls back to a default.
    pub line: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTarget {
    Engine(usize),
    Field,
    StatusButton,
    Save,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SyncState {
    /// Index into [`SQL_ENGINES`].
    pub engine: usize,
    pub uri: TextDraft,
    pub focused: bool,
    pub status: SyncStatus,
    /// Inline validation under the field.
    pub field_error: Option<String>,
    pub hover: Option<SyncTarget>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncLayout {
    pub intro: Rect,
    pub database_label: Rect,
    pub segmented: SegmentedLayout,
    pub field: FieldLayout,
    pub status: Rect,
    pub status_dot: Rect,
    pub status_text: Rect,
    pub status_button: Rect,
    pub save: Rect,
}

impl SyncState {
    pub fn engine_label(&self) -> &'static str {
        SQL_ENGINES
            .get(self.engine)
            .copied()
            .unwrap_or(SQL_ENGINES[0])
    }

    pub fn field_label(&self) -> &'static str {
        if self.engine == 1 {
            "Connection string"
        } else {
            "Database file"
        }
    }

    pub fn placeholder(&self) -> &'static str {
        if self.engine == 1 {
            "postgres://user:pass@host:5432/terminus"
        } else {
            "sqlite:./remote.db"
        }
    }

    /// Text of the status row.
    pub fn status_text(&self) -> String {
        if !self.status.line.is_empty() {
            return self.status.line.clone();
        }
        if self.status.connected {
            "Synced".into()
        } else {
            "Not configured".into()
        }
    }

    pub fn status_kind(&self) -> StatusKind {
        if self.status.is_error {
            StatusKind::Error
        } else if self.status.connected {
            StatusKind::Running
        } else {
            StatusKind::Idle
        }
    }

    /// Label of the status-row button; Unlock vault while the vault is locked.
    pub fn status_button_label(&self) -> &'static str {
        if self.status.vault_unlocked {
            "Sync now"
        } else {
            "Unlock vault"
        }
    }

    /// Push a snapshot from the host worker. The URI is only replaced while
    /// the field is not being edited.
    pub fn apply_snapshot(&mut self, uri: &str, status: SyncStatus) {
        if !self.focused {
            self.uri = TextDraft::new(uri);
            if uri.starts_with("postgres") {
                self.engine = 1;
            } else if !uri.is_empty() {
                self.engine = 0;
            }
        }
        self.status = status;
    }

    pub fn layout(&self, content: Rect, m: Measure) -> SyncLayout {
        let col = column(content);
        let w = col.width.min(MAX_WIDTH);
        let x = col.x;
        let mut y = col.y;
        let intro = Rect::new(x, y, w, INTRO_HEIGHT);
        y += INTRO_HEIGHT + SECTION_GAP;
        let database_label = Rect::new(x, y, w, LABEL_HEIGHT);
        y += LABEL_HEIGHT + LABEL_GAP;
        let widths: Vec<f32> = SQL_ENGINES
            .iter()
            .map(|e| m(e, SegmentedSize::Medium.font_size(), false))
            .collect();
        let segmented = SegmentedLayout::new(x, y, &widths, SegmentedSize::Medium);
        y = segmented.track.bottom() + SECTION_GAP;
        let field =
            field_layout((x, y), w, FieldKind::Mono, true, self.field_error.is_some());
        y = field.total.bottom() + SECTION_GAP;
        let status = Rect::new(x, y, w, STATUS_HEIGHT);
        let btn = button_spec(
            m,
            (0.0, 0.0),
            ButtonKind::Secondary,
            ButtonSize::Medium,
            self.status_button_label(),
        );
        let status_button = Rect::new(
            status.right() - STATUS_PAD_X - btn.width(),
            status.y + (STATUS_HEIGHT - ButtonSize::Medium.height()) / 2.0,
            btn.width(),
            ButtonSize::Medium.height(),
        );
        let status_dot = Rect::new(
            status.x + STATUS_PAD_X,
            status.y + (STATUS_HEIGHT - 8.0) / 2.0,
            8.0,
            8.0,
        );
        let tx = status_dot.right() + 14.0;
        let status_text = Rect::new(
            tx,
            status.y,
            (status_button.x - 14.0 - tx).max(0.0),
            STATUS_HEIGHT,
        );
        y = status.bottom() + SECTION_GAP;
        let save_spec = button_spec(
            m,
            (0.0, 0.0),
            ButtonKind::Primary,
            ButtonSize::Large,
            "Save",
        );
        let save = Rect::new(x, y, save_spec.width(), ButtonSize::Large.height());
        let _ = HELPER_HEIGHT;
        SyncLayout {
            intro,
            database_label,
            segmented,
            field,
            status,
            status_dot,
            status_text,
            status_button,
            save,
        }
    }

    pub fn hit(&self, l: &SyncLayout, x: f32, y: f32) -> Option<SyncTarget> {
        if let Some(i) = l.segmented.hit_test(x, y) {
            return Some(SyncTarget::Engine(i));
        }
        if l.field.box_rect.contains(x, y) {
            return Some(SyncTarget::Field);
        }
        if l.status_button.contains(x, y) {
            return Some(SyncTarget::StatusButton);
        }
        l.save.contains(x, y).then_some(SyncTarget::Save)
    }

    pub fn hover(&mut self, content: Rect, m: Measure, x: f32, y: f32) -> bool {
        let l = self.layout(content, m);
        let t = self.hit(&l, x, y);
        let changed = t != self.hover;
        self.hover = t;
        changed
    }

    pub fn press(
        &mut self,
        content: Rect,
        m: Measure,
        x: f32,
        y: f32,
    ) -> Option<SettingsAction> {
        let l = self.layout(content, m);
        let target = self.hit(&l, x, y);
        self.focused = target == Some(SyncTarget::Field);
        match target {
            Some(SyncTarget::Engine(i)) => {
                self.engine = i;
                self.field_error = None;
                None
            }
            Some(SyncTarget::StatusButton) => {
                if self.status.vault_unlocked {
                    self.sync_now()
                } else {
                    Some(SettingsAction::UnlockVault)
                }
            }
            Some(SyncTarget::Save) => self.save(),
            _ => None,
        }
    }

    fn valid_uri(&mut self) -> Option<String> {
        let uri = self.uri.value.trim().to_string();
        if uri.is_empty() {
            self.field_error = Some(match self.engine {
                1 => "Enter a postgres:// connection string".to_string(),
                _ => "Enter the path of the shared database".to_string(),
            });
            return None;
        }
        self.field_error = None;
        Some(uri)
    }

    pub fn save(&mut self) -> Option<SettingsAction> {
        self.valid_uri().map(|uri| SettingsAction::SaveSync { uri })
    }

    pub fn sync_now(&mut self) -> Option<SettingsAction> {
        self.valid_uri().map(|uri| SettingsAction::SyncNow { uri })
    }

    pub fn key(&mut self, key: Key) -> Option<SettingsAction> {
        if !self.focused {
            return None;
        }
        match key {
            Key::Escape => {
                self.focused = false;
                None
            }
            Key::Enter => self.save(),
            Key::Tab | Key::ShiftTab => {
                self.focused = false;
                None
            }
            other => {
                if edit_draft(&mut self.uri, other) {
                    self.field_error = None;
                }
                None
            }
        }
    }

    pub fn insert_text(&mut self, text: &str) -> bool {
        if !self.focused {
            return false;
        }
        self.field_error = None;
        self.uri.insert(text, SQL_URI_MAX_BYTES, false)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_measure;
    use super::*;

    const CONTENT: Rect = Rect::new(260.0, 96.0, 1180.0, 804.0);

    fn center(r: Rect) -> (f32, f32) {
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    #[test]
    fn sections_stack_top_to_bottom_inside_padding() {
        let s = SyncState::default();
        let l = s.layout(CONTENT, &mut test_measure);
        assert_eq!(l.intro.x, CONTENT.x + 28.0);
        assert!(l.database_label.y > l.intro.bottom());
        assert!(l.segmented.track.y > l.database_label.bottom());
        assert!(l.field.total.y > l.segmented.track.bottom());
        assert!(l.status.y > l.field.total.bottom());
        assert!(l.save.y > l.status.bottom());
        assert_eq!(l.save.height, 44.0);
    }

    #[test]
    fn engine_segments_switch_label_and_placeholder() {
        let mut s = SyncState::default();
        assert_eq!(s.field_label(), "Database file");
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.segmented.segments[1]);
        s.press(CONTENT, &mut test_measure, x, y);
        assert_eq!(s.engine, 1);
        assert_eq!(s.field_label(), "Connection string");
        assert!(s.placeholder().starts_with("postgres://"));
    }

    #[test]
    fn save_with_empty_uri_shows_inline_error_and_grows_the_field() {
        let mut s = SyncState::default();
        let before = s.layout(CONTENT, &mut test_measure).save.y;
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.save);
        assert_eq!(s.press(CONTENT, &mut test_measure, x, y), None);
        assert!(s.field_error.is_some());
        let after = s.layout(CONTENT, &mut test_measure).save.y;
        assert!(after > before, "helper line adds height");
    }

    #[test]
    fn typing_then_save_returns_the_uri() {
        let mut s = SyncState::default();
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.field.box_rect);
        s.press(CONTENT, &mut test_measure, x, y);
        assert!(s.focused);
        assert!(s.insert_text("sqlite:./shared.db"));
        assert_eq!(
            s.key(Key::Enter),
            Some(SettingsAction::SaveSync {
                uri: "sqlite:./shared.db".into()
            })
        );
    }

    #[test]
    fn status_button_syncs_or_asks_to_unlock() {
        let mut s = SyncState::default();
        s.apply_snapshot(
            "sqlite:./a.db",
            SyncStatus {
                connected: true,
                vault_unlocked: false,
                ..Default::default()
            },
        );
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.status_button);
        assert_eq!(s.status_button_label(), "Unlock vault");
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::UnlockVault)
        );

        s.status.vault_unlocked = true;
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.status_button);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::SyncNow {
                uri: "sqlite:./a.db".into()
            })
        );
    }

    #[test]
    fn snapshot_does_not_clobber_a_field_being_edited() {
        let mut s = SyncState {
            focused: true,
            uri: TextDraft::new("sqlite:./typing"),
            ..Default::default()
        };
        s.apply_snapshot("sqlite:./stored.db", SyncStatus::default());
        assert_eq!(s.uri.value, "sqlite:./typing");
        s.focused = false;
        s.apply_snapshot("postgres://u@h/db", SyncStatus::default());
        assert_eq!(s.uri.value, "postgres://u@h/db");
        assert_eq!(s.engine, 1);
    }

    #[test]
    fn status_text_and_dot_follow_the_snapshot() {
        let mut s = SyncState::default();
        assert_eq!(s.status_text(), "Not configured");
        assert_eq!(s.status_kind(), StatusKind::Idle);
        s.status.connected = true;
        assert_eq!(s.status_text(), "Synced");
        assert_eq!(s.status_kind(), StatusKind::Running);
        s.status.line = "Could not reach the database".into();
        s.status.is_error = true;
        assert_eq!(s.status_kind(), StatusKind::Error);
        assert_eq!(s.status_text(), "Could not reach the database");
    }

    #[test]
    fn keys_do_nothing_when_the_field_is_not_focused() {
        let mut s = SyncState::default();
        assert!(!s.insert_text("x"));
        assert_eq!(s.key(Key::Enter), None);
    }
}
