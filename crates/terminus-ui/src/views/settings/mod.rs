//! Settings page contents: SSH keys, Sync, Appearance and Updates.
//!
//! The shell owns the Settings header and its tab strip; this module owns
//! what each tab shows inside the content rect. Everything is pure: state,
//! geometry (computed from the `content` rect and a text-measure closure),
//! hit-testing and keyboard semantics. The painter lives in
//! `frontends/rioterm/src/renderer/views/settings/` and walks the same
//! `*_layout` functions the pointer hit-tests, so a control cannot be drawn
//! where it cannot be clicked.
//!
//! Persistent effects are returned as a [`SettingsAction`] for the app to
//! execute (host worker calls, config file edits, updater commands).

pub mod appearance;
pub mod config_edit;
pub mod keys;
pub mod sync;
pub mod updates;

use crate::components::button::{ButtonKind, ButtonSize, ButtonSpec};
use crate::components::overlay::{
    self as ov, dialog_layout_at, wrap_text, DialogFocus, DialogHit, DialogKind,
    DialogLayout,
};
use crate::geom::Rect;
use crate::text_field::{TextDraft, TextEdit, TextMoveKind};

pub use appearance::{AppearanceState, CursorStyle, ThemeChoice};
pub use keys::{DraftField, DraftMode, KeyDraft, KeysState};
pub use sync::SyncState;
pub use updates::{UpdateStatus, UpdatesState};

/// Outer padding of every Settings page.
pub const PAD: f32 = 28.0;
/// Gap between stacked cards / rows.
pub const ROW_GAP: f32 = 12.0;

/// Text measure supplied by the painter: `(text, font_size, semibold) -> width`.
pub type Measure<'a> = &'a mut dyn FnMut(&str, f32, bool) -> f32;

/// The four Settings tabs (the shell draws the tab strip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Keys,
    Sync,
    Appearance,
    Updates,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Keys, Page::Sync, Page::Appearance, Page::Updates];

    /// Tab label, as in the mock.
    pub fn label(self) -> &'static str {
        match self {
            Page::Keys => "SSH keys",
            Page::Sync => "Sync",
            Page::Appearance => "Appearance",
            Page::Updates => "Updates",
        }
    }

    /// `TERMINUS_VIEW_PREVIEW=settings-<name>`.
    pub fn from_preview(name: &str) -> Option<Page> {
        Some(match name {
            "settings-keys" => Page::Keys,
            "settings-sync" => Page::Sync,
            "settings-appearance" => Page::Appearance,
            "settings-updates" => Page::Updates,
            _ => return None,
        })
    }
}

/// What the app must do after an input event.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsAction {
    // ---- SSH keys
    /// Send the draft to the host worker (`host_store.import_ssh_key`).
    SubmitKey {
        name: String,
        pem: Option<String>,
        passphrase: Option<String>,
    },
    /// Put the public key of managed key `id` on the clipboard.
    CopyPublicKey {
        id: String,
    },
    /// Delete managed key `id` (already confirmed).
    DeleteKey {
        id: String,
    },
    // ---- Sync
    /// Persist the URI and sync (`host_store.test_sync(&uri)`).
    SaveSync {
        uri: String,
    },
    /// Run a sync with the stored URI (`host_store.test_sync(&uri)`).
    SyncNow {
        uri: String,
    },
    /// The vault is locked: open the unlock prompt.
    UnlockVault,
    // ---- Appearance (persisted with [`SettingsAction::config_edit`])
    SetFont(String),
    SetFontSize(f32),
    SetCursor(CursorStyle),
    // ---- Updates
    CheckUpdates,
    InstallUpdate,
    SetCheckUpdates(bool),
    SetAutoInstall(bool),
}

impl SettingsAction {
    /// The `(section, key, toml literal)` this action writes to the config
    /// file, for the actions that persist settings.
    pub fn config_edit(&self) -> Option<(&'static str, &'static str, String)> {
        Some(match self {
            SettingsAction::SetFont(f) => ("fonts", "family", config_edit::quote(f)),
            SettingsAction::SetFontSize(s) => ("fonts", "size", format_size(*s)),
            SettingsAction::SetCursor(c) => {
                ("cursor", "shape", config_edit::quote(c.config_value()))
            }
            SettingsAction::SetCheckUpdates(on) => ("updates", "check", on.to_string()),
            SettingsAction::SetAutoInstall(on) => {
                ("updates", "auto-install", on.to_string())
            }
            _ => return None,
        })
    }
}

/// `14` for whole sizes, `14.5` otherwise.
pub fn format_size(size: f32) -> String {
    if (size - size.round()).abs() < 0.05 {
        format!("{}", size.round() as i32)
    } else {
        format!("{size:.1}")
    }
}

/// Keys the Settings page reacts to (the app maps its key events to these).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Enter,
    Escape,
    Tab,
    ShiftTab,
    SelectAll,
    Up,
    Down,
}

/// Button spec for `label` at `origin`, measured with `m`.
pub fn button_spec(
    m: Measure,
    origin: (f32, f32),
    kind: ButtonKind,
    size: ButtonSize,
    label: &str,
) -> ButtonSpec {
    let w = m(label, size.font_size(), kind.semibold());
    ButtonSpec::label(origin, kind, size, w, false)
}

/// Icon-only button spec.
pub fn icon_button_spec(
    origin: (f32, f32),
    kind: ButtonKind,
    size: ButtonSize,
) -> ButtonSpec {
    ButtonSpec::icon_only(origin, kind, size)
}

/// The usable column: content minus padding.
pub fn column(content: Rect) -> Rect {
    Rect::new(
        content.x + PAD,
        content.y + PAD,
        (content.width - 2.0 * PAD).max(0.0),
        (content.height - 2.0 * PAD).max(0.0),
    )
}

/// Route a text editing key to a draft. Returns whether the draft changed.
pub fn edit_draft(draft: &mut TextDraft, key: Key) -> bool {
    let collapse = TextMoveKind::Collapse;
    let edit = match key {
        Key::Backspace => TextEdit::Backspace { by_word: false },
        Key::Delete => TextEdit::Delete { by_word: false },
        Key::Left => TextEdit::Left {
            kind: collapse,
            by_word: false,
        },
        Key::Right => TextEdit::Right {
            kind: collapse,
            by_word: false,
        },
        Key::Home => TextEdit::Home { kind: collapse },
        Key::End => TextEdit::End { kind: collapse },
        Key::SelectAll => TextEdit::SelectAll,
        _ => return false,
    };
    draft.apply(edit)
}

// ---------------------------------------------------------------- confirm

/// A destructive confirm dialog shown over the content rect.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirm {
    pub title: String,
    pub body: String,
    pub confirm: String,
    pub cancel: String,
    pub focus: DialogFocus,
}

impl Confirm {
    pub fn destructive(
        title: impl Into<String>,
        body: impl Into<String>,
        confirm: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            confirm: confirm.into(),
            cancel: "Cancel".into(),
            focus: DialogKind::Destructive.default_focus(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmLayout {
    pub dialog: DialogLayout,
    pub lines: Vec<String>,
}

/// Dialog centred in `content`; the scrim covers only `content`.
pub fn confirm_layout(content: Rect, m: Measure, c: &Confirm) -> ConfirmLayout {
    let inner = ov::DIALOG_WIDTH - 2.0 * ov::DIALOG_PAD;
    let lines = wrap_text(&c.body, inner, |s| m(s, 14.0, false));
    let cancel_w = confirm_button_width(m, ButtonKind::Secondary, &c.cancel);
    let confirm_w = confirm_button_width(m, ButtonKind::Danger, &c.confirm);
    // Height does not depend on x/y: build once to read it, then centre.
    let probe = dialog_layout_at(
        0.0,
        0.0,
        DialogKind::Destructive,
        lines.len(),
        cancel_w,
        confirm_w,
        (content.width, content.height),
    );
    let x = (content.x + (content.width - ov::DIALOG_WIDTH) / 2.0).round();
    let y = (content.y + (content.height - probe.dialog.height) / 2.0).round();
    let mut dialog = dialog_layout_at(
        x,
        y,
        DialogKind::Destructive,
        lines.len(),
        cancel_w,
        confirm_w,
        (content.width, content.height),
    );
    dialog.scrim = content;
    ConfirmLayout { dialog, lines }
}

fn confirm_button_width(m: Measure, kind: ButtonKind, label: &str) -> f32 {
    button_spec(m, (0.0, 0.0), kind, ButtonSize::Large, label).width()
}

/// Outcome of a press while a confirm dialog is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmHit {
    Confirm,
    Cancel,
    /// Inside the dialog, on nothing: swallow.
    Inside,
}

pub fn confirm_hit(layout: &ConfirmLayout, x: f32, y: f32) -> ConfirmHit {
    match layout.dialog.hit_test(x, y) {
        DialogHit::Confirm => ConfirmHit::Confirm,
        DialogHit::Cancel | DialogHit::Scrim => ConfirmHit::Cancel,
        _ => ConfirmHit::Inside,
    }
}

// ------------------------------------------------------------------ view

/// All Settings page state. The shell holds one of these.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsView {
    pub page: Page,
    pub keys: KeysState,
    pub sync: SyncState,
    pub appearance: AppearanceState,
    pub updates: UpdatesState,
}

impl SettingsView {
    pub fn new(version: &str) -> Self {
        Self {
            page: Page::Keys,
            keys: KeysState::default(),
            sync: SyncState::default(),
            appearance: AppearanceState::default(),
            updates: UpdatesState::new(version),
        }
    }

    pub fn set_page(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.keys.blur();
            self.sync.focused = false;
            self.appearance.font_menu_open = false;
        }
    }

    /// Pointer press at `(x, y)`.
    pub fn press(
        &mut self,
        content: Rect,
        m: Measure,
        x: f32,
        y: f32,
    ) -> Option<SettingsAction> {
        match self.page {
            Page::Keys => self.keys.press(content, m, x, y),
            Page::Sync => self.sync.press(content, m, x, y),
            Page::Appearance => self.appearance.press(content, m, x, y),
            Page::Updates => self.updates.press(content, m, x, y),
        }
    }

    /// Pointer move; true when the hover changed (repaint).
    pub fn hover(&mut self, content: Rect, m: Measure, x: f32, y: f32) -> bool {
        match self.page {
            Page::Keys => self.keys.hover(content, m, x, y),
            Page::Sync => self.sync.hover(content, m, x, y),
            Page::Appearance => self.appearance.hover(content, m, x, y),
            Page::Updates => self.updates.hover(content, m, x, y),
        }
    }

    /// Pointer shape at `(x, y)`: a hand over whatever a press acts on,
    /// an I-beam over text fields (same hit tests as [`Self::press`]).
    pub fn cursor_at(
        &self,
        content: Rect,
        m: Measure,
        x: f32,
        y: f32,
    ) -> crate::chrome::ChromeCursor {
        use crate::chrome::ChromeCursor;
        let hand = |on: bool| {
            if on {
                ChromeCursor::Pointer
            } else {
                ChromeCursor::Default
            }
        };
        match self.page {
            Page::Keys => {
                let l = self.keys.layout(content, m);
                if let Some(cl) = &l.confirm {
                    return hand(matches!(
                        cl.dialog.hit_test(x, y),
                        DialogHit::Confirm | DialogHit::Cancel
                    ));
                }
                match self.keys.hit(&l, x, y) {
                    Some(keys::KeysTarget::Field(_)) => ChromeCursor::Text,
                    t => hand(t.is_some()),
                }
            }
            Page::Sync => {
                let l = self.sync.layout(content, m);
                match self.sync.hit(&l, x, y) {
                    Some(sync::SyncTarget::Field) => ChromeCursor::Text,
                    t => hand(t.is_some()),
                }
            }
            Page::Appearance => {
                let l = self.appearance.layout(content, m);
                hand(self.appearance.hit(&l, x, y).is_some())
            }
            Page::Updates => {
                let l = self.updates.layout(content, m);
                hand(self.updates.hit(&l, x, y).is_some())
            }
        }
    }

    /// Wheel (`dy` > 0 scrolls down); true when something moved.
    pub fn wheel(&mut self, content: Rect, m: Measure, x: f32, y: f32, dy: f32) -> bool {
        match self.page {
            Page::Appearance => self.appearance.wheel(content, m, x, y, dy),
            _ => false,
        }
    }

    pub fn key(&mut self, key: Key) -> Option<SettingsAction> {
        match self.page {
            Page::Keys => self.keys.key(key),
            Page::Sync => self.sync.key(key),
            Page::Appearance => {
                self.appearance.key(key);
                None
            }
            Page::Updates => None,
        }
    }

    /// Typed or pasted text; true when a field took it.
    pub fn insert_text(&mut self, text: &str) -> bool {
        match self.page {
            Page::Keys => self.keys.insert_text(text),
            Page::Sync => self.sync.insert_text(text),
            _ => false,
        }
    }

    /// Whether a text field or dialog owns the keyboard (so the app must not
    /// forward keys to the terminal).
    pub fn captures_keyboard(&self) -> bool {
        match self.page {
            Page::Keys => self.keys.captures_keyboard(),
            Page::Sync => self.sync.focused,
            Page::Appearance => self.appearance.font_menu_open,
            Page::Updates => false,
        }
    }
}

#[cfg(test)]
pub(crate) fn test_measure(text: &str, size: f32, _semibold: bool) -> f32 {
    text.chars().count() as f32 * size * 0.55
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_edits_map_to_the_keys_the_app_reads() {
        assert_eq!(
            SettingsAction::SetFontSize(14.0).config_edit(),
            Some(("fonts", "size", "14".to_string()))
        );
        assert_eq!(
            SettingsAction::SetFontSize(14.5).config_edit(),
            Some(("fonts", "size", "14.5".to_string()))
        );
        assert_eq!(
            SettingsAction::SetFont("Fira Code".into()).config_edit(),
            Some(("fonts", "family", "\"Fira Code\"".to_string()))
        );
        assert_eq!(
            SettingsAction::SetCursor(CursorStyle::Beam).config_edit(),
            Some(("cursor", "shape", "\"beam\"".to_string()))
        );
        assert_eq!(
            SettingsAction::SetCheckUpdates(false).config_edit(),
            Some(("updates", "check", "false".to_string()))
        );
        assert_eq!(
            SettingsAction::SetAutoInstall(true).config_edit(),
            Some(("updates", "auto-install", "true".to_string()))
        );
        assert_eq!(SettingsAction::CheckUpdates.config_edit(), None);
    }

    #[test]
    fn preview_names_map_to_pages() {
        assert_eq!(Page::from_preview("settings-sync"), Some(Page::Sync));
        assert_eq!(Page::from_preview("files"), None);
    }

    #[test]
    fn confirm_dialog_is_centred_in_content_and_cancel_wins_scrim() {
        let content = Rect::new(260.0, 96.0, 1180.0, 804.0);
        let c = Confirm::destructive(
            "Delete id_ed25519?",
            "Servers using it will ask for a password.",
            "Delete",
        );
        let l = confirm_layout(content, &mut test_measure, &c);
        let d = l.dialog.dialog;
        assert!((d.x + d.width / 2.0 - (content.x + content.width / 2.0)).abs() <= 1.0);
        assert_eq!(
            confirm_hit(&l, content.x + 2.0, content.y + 2.0),
            ConfirmHit::Cancel
        );
        let cf = l.dialog.confirm;
        assert_eq!(confirm_hit(&l, cf.x + 2.0, cf.y + 2.0), ConfirmHit::Confirm);
        assert_eq!(c.focus, DialogFocus::Cancel);
    }

    #[test]
    fn the_pointer_is_a_hand_over_what_a_press_acts_on() {
        use crate::chrome::ChromeCursor;
        let content = Rect::new(260.0, 96.0, 1180.0, 804.0);
        let mut v = SettingsView::new("1.0.0");
        v.keys.set_keys(vec![crate::settings::SshKeyItem {
            id: "k1".into(),
            name: "id_ed25519".into(),
            fingerprint: "SHA256:k1".into(),
            created: "2026-01-02".into(),
            public_key: "ssh-ed25519 AAAA".into(),
        }]);
        let mid = |r: Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);

        // Keys: header buttons are hands, the empty corner is not.
        let l = v.keys.layout(content, &mut test_measure);
        let (x, y) = mid(l.generate);
        assert_eq!(
            v.cursor_at(content, &mut test_measure, x, y),
            ChromeCursor::Pointer
        );
        let (x, y) = mid(l.import);
        assert_eq!(
            v.cursor_at(content, &mut test_measure, x, y),
            ChromeCursor::Pointer
        );
        assert_eq!(
            v.cursor_at(
                content,
                &mut test_measure,
                content.x + 2.0,
                content.bottom() - 2.0
            ),
            ChromeCursor::Default
        );
        // A draft's text fields show the I-beam.
        let _ = v.press(
            content,
            &mut test_measure,
            mid(l.generate).0,
            mid(l.generate).1,
        );
        let l = v.keys.layout(content, &mut test_measure);
        let (_, field) = &l.draft.as_ref().unwrap().fields[0];
        let (x, y) = mid(field.box_rect);
        assert_eq!(
            v.cursor_at(content, &mut test_measure, x, y),
            ChromeCursor::Text
        );

        // Sync: engine segments and Save are hands, the URI field is text.
        v.set_page(Page::Sync);
        let l = v.sync.layout(content, &mut test_measure);
        let (x, y) = mid(l.save);
        assert_eq!(
            v.cursor_at(content, &mut test_measure, x, y),
            ChromeCursor::Pointer
        );
        let (x, y) = mid(l.field.box_rect);
        assert_eq!(
            v.cursor_at(content, &mut test_measure, x, y),
            ChromeCursor::Text
        );

        // Updates: the whole toggle row is a hand.
        v.set_page(Page::Updates);
        let l = v.updates.layout(content, &mut test_measure);
        let (x, y) = mid(l.check.card);
        assert_eq!(
            v.cursor_at(content, &mut test_measure, x, y),
            ChromeCursor::Pointer
        );

        // Appearance: the size steppers are hands.
        v.set_page(Page::Appearance);
        let l = v.appearance.layout(content, &mut test_measure);
        let (x, y) = mid(l.size_plus);
        assert_eq!(
            v.cursor_at(content, &mut test_measure, x, y),
            ChromeCursor::Pointer
        );
    }

    #[test]
    fn the_delete_confirm_buttons_are_hands() {
        use crate::chrome::ChromeCursor;
        let content = Rect::new(260.0, 96.0, 1180.0, 804.0);
        let mut v = SettingsView::new("1.0.0");
        v.keys.set_keys(vec![crate::settings::SshKeyItem {
            id: "k1".into(),
            name: "id_ed25519".into(),
            fingerprint: "SHA256:k1".into(),
            created: "2026-01-02".into(),
            public_key: "ssh-ed25519 AAAA".into(),
        }]);
        let l = v.keys.layout(content, &mut test_measure);
        let row = &l.rows[0];
        // Trash is the last action on the card.
        let trash = crate::components::list::card_layout(
            row.card.rect,
            &crate::components::list::CardSpec {
                has_dot: false,
                meta_width: row.meta_width,
                action_widths: &[row.copy_width, row.trash_width],
            },
        )
        .actions[1]
            .expect("trash slot");
        let _ = v.press(
            content,
            &mut test_measure,
            trash.x + trash.width / 2.0,
            trash.y + trash.height / 2.0,
        );
        let l = v.keys.layout(content, &mut test_measure);
        let cf = l.confirm.as_ref().expect("confirm open").dialog.confirm;
        assert_eq!(
            v.cursor_at(content, &mut test_measure, cf.x + 2.0, cf.y + 2.0),
            ChromeCursor::Pointer
        );
    }
}
