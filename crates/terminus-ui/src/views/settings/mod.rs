//! Settings page contents: SSH keys, Sync, Appearance, Updates and Uploads.
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
pub mod uploads;

use crate::components::button::{ButtonKind, ButtonSize, ButtonSpec};
use crate::components::input::{TextDraft, TextEdit, TextMoveKind};
use crate::components::overlay::{
    self as ov, dialog_layout_at, wrap_text, DialogFocus, DialogHit, DialogKind,
    DialogLayout,
};
use crate::geom::Rect;

pub use appearance::{AppearanceState, CursorStyle, ThemeChoice};
pub use keys::{DraftField, DraftMode, KeyDraft, KeysState};
pub use sync::SyncState;
pub use updates::{UpdateStatus, UpdatesState};
pub use uploads::{UploadsField, UploadsState};

/// Outer padding of every Settings page.
pub const PAD: f32 = 28.0;
/// Gap between stacked cards / rows.
pub const ROW_GAP: f32 = 12.0;

/// Text measure supplied by the painter: `(text, font_size, semibold) -> width`.
pub type Measure<'a> = &'a mut dyn FnMut(&str, f32, bool) -> f32;

/// The five Settings tabs (the shell draws the tab strip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Keys,
    Sync,
    Appearance,
    Updates,
    Uploads,
}

impl Page {
    pub const ALL: [Page; 5] = [
        Page::Keys,
        Page::Sync,
        Page::Appearance,
        Page::Updates,
        Page::Uploads,
    ];

    /// Tab label, as in the mock.
    pub fn label(self) -> &'static str {
        match self {
            Page::Keys => "SSH keys",
            Page::Sync => "Sync",
            Page::Appearance => "Appearance",
            Page::Updates => "Updates",
            Page::Uploads => "Uploads",
        }
    }

    /// `TERMINUS_VIEW_PREVIEW=settings-<name>`.
    pub fn from_preview(name: &str) -> Option<Page> {
        Some(match name {
            "settings-keys" => Page::Keys,
            "settings-sync" => Page::Sync,
            "settings-appearance" => Page::Appearance,
            "settings-updates" => Page::Updates,
            "settings-uploads" => Page::Uploads,
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
    SetTheme(ThemeChoice),
    // ---- Updates
    CheckUpdates,
    InstallUpdate,
    SetCheckUpdates(bool),
    SetAutoInstall(bool),
    // ---- Uploads
    SetUploadDir {
        field: UploadsField,
        value: String,
    },
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
            SettingsAction::SetTheme(t) => {
                ("appearance", "theme", config_edit::quote(t.config_value()))
            }
            SettingsAction::SetCheckUpdates(on) => ("updates", "check", on.to_string()),
            SettingsAction::SetAutoInstall(on) => {
                ("updates", "auto-install", on.to_string())
            }
            SettingsAction::SetUploadDir { field, value } => {
                ("uploads", field.config_key(), config_edit::quote(value))
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
    pub uploads: UploadsState,
}

impl SettingsView {
    pub fn new(version: &str) -> Self {
        Self {
            page: Page::Keys,
            keys: KeysState::default(),
            sync: SyncState::default(),
            appearance: AppearanceState::default(),
            updates: UpdatesState::new(version),
            uploads: UploadsState::default(),
        }
    }

    /// Switch tab; returns the edit a text field was still holding.
    pub fn set_page(&mut self, page: Page) -> Option<SettingsAction> {
        if self.page == page {
            return None;
        }
        self.page = page;
        self.keys.blur();
        self.sync.focused = false;
        self.appearance.font_menu_open = false;
        self.uploads.blur()
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
            Page::Uploads => self.uploads.press(content, m, x, y),
        }
    }

    /// Pointer move; true when the hover changed (repaint).
    pub fn hover(&mut self, content: Rect, m: Measure, x: f32, y: f32) -> bool {
        match self.page {
            Page::Keys => self.keys.hover(content, m, x, y),
            Page::Sync => self.sync.hover(content, m, x, y),
            Page::Appearance => self.appearance.hover(content, m, x, y),
            Page::Updates => self.updates.hover(content, m, x, y),
            Page::Uploads => self.uploads.hover(content, m, x, y),
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
            Page::Uploads => {
                let l = self.uploads.layout(content, m);
                match self.uploads.hit(&l, x, y) {
                    Some(uploads::UploadsTarget::Field(_)) => ChromeCursor::Text,
                    t => hand(t.is_some()),
                }
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
            Page::Uploads => self.uploads.key(key),
        }
    }

    /// Typed or pasted text; true when a field took it.
    pub fn insert_text(&mut self, text: &str) -> bool {
        match self.page {
            Page::Keys => self.keys.insert_text(text),
            Page::Sync => self.sync.insert_text(text),
            Page::Uploads => self.uploads.insert_text(text),
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
            Page::Uploads => self.uploads.captures_keyboard(),
        }
    }
}

#[cfg(test)]
pub(crate) fn test_measure(text: &str, size: f32, _semibold: bool) -> f32 {
    text.chars().count() as f32 * size * 0.55
}

#[cfg(test)]
mod tests;
