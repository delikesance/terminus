//! Modal prompt for the vault passphrase when a sealed secret is needed.
//!
//! Shown when connecting with a saved SSH password (or saving one) while the
//! vault is still locked. The passphrase row reuses the Settings field-card
//! geometry (`field_input_in_card` + eye slot).

use crate::components::overlay::wrap_text;
use crate::confirm::{estimate_text_width, BODY_FONT, BUTTON_ADVANCE, BUTTON_FONT};
use crate::geom::Rect;
use crate::text_field::{FieldPaint, TextDraft, TextEdit};

pub const WIDTH: f32 = 400.0;
pub const PAD: f32 = 32.0;
/// Vertical gap between the dialog's blocks.
pub const GAP: f32 = 20.0;
pub const TILE: f32 = 48.0;
pub const TITLE_LINE: f32 = 26.0;
pub const TITLE_BODY_GAP: f32 = 6.0;
pub const BODY_LINE: f32 = 21.0;
pub const FIELD_HEIGHT: f32 = 46.0;
pub const FIELD_GAP: f32 = 12.0;
/// Eye button inside the field (34px square, 6px from the right edge).
pub const EYE_SLOT: f32 = 34.0;
pub const EYE_ICON: f32 = 16.0;
pub const CHECK_SIZE: f32 = 20.0;
pub const CHECK_ROW_HEIGHT: f32 = 20.0;
/// Error / progress line under the checkbox.
pub const HINT_HEIGHT: f32 = 16.0;
pub const HINT_GAP: f32 = 12.0;
pub const BUTTON_HEIGHT: f32 = 44.0;
pub const BUTTON_GAP: f32 = 10.0;
pub const RADIUS: f32 = 18.0;
/// Label shown next to the checkbox.
pub const REMEMBER_LABEL: &str = "Remember on this computer";
/// Vault passphrases shorter than this are refused (matches the worker).
pub const MIN_PASSPHRASE_CHARS: usize = 8;

/// Why the prompt was opened — drives the subtitle and the retry after unlock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingVaultAction {
    OpenHost(String),
    AddHostSession(String),
    SubmitHostForm,
    /// Retry saving the Settings "New SSH Key" draft (keys are sealed).
    SaveSshKey,
    /// Settings → Unlock Vault with no vault yet: just create it.
    CreateVault,
    /// Settings → Unlock Vault over an existing vault: just unlock it.
    UnlockVault,
    /// Retry opening SFTP for a host (`other_pane`: the left side).
    OpenSftp {
        host_id: String,
        other_pane: bool,
    },
}

impl PendingVaultAction {
    pub fn subtitle(&self) -> &'static str {
        match self {
            Self::OpenHost(_) | Self::AddHostSession(_) | Self::OpenSftp { .. } => {
                "Enter your vault passphrase to use the saved SSH password."
            }
            Self::SubmitHostForm => {
                "Enter your vault passphrase to encrypt and save the password."
            }
            Self::SaveSshKey => {
                "Enter your vault passphrase to encrypt and save the key."
            }
            Self::CreateVault => "Choose a passphrase for the new vault.",
            Self::UnlockVault => {
                "Enter your vault passphrase to use saved passwords and keys."
            }
        }
    }

    /// What Settings → Unlock vault asks for: unlock the vault when one
    /// exists, create it otherwise.
    pub fn settings_unlock(vault_configured: bool) -> Self {
        if vault_configured {
            Self::UnlockVault
        } else {
            Self::CreateVault
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultUnlockHit {
    Field,
    /// Confirm-passphrase field (vault creation only).
    ConfirmField,
    ToggleVisible,
    ToggleRemember,
    Unlock,
    Cancel,
    Consume,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VaultUnlockPrompt {
    open: bool,
    passphrase: TextDraft,
    visible: bool,
    /// Persist passphrase in the OS keyring after a successful unlock.
    remember: bool,
    error: Option<String>,
    pending: Option<PendingVaultAction>,
    unlocking: bool,
    /// No vault exists yet: this prompt creates one (asks twice).
    creating: bool,
    confirm: TextDraft,
    confirm_focused: bool,
    /// Name of the host this unlock is for, when known.
    host_label: Option<String>,
}

impl Default for VaultUnlockPrompt {
    fn default() -> Self {
        Self {
            creating: false,
            confirm: TextDraft::default(),
            confirm_focused: false,
            host_label: None,
            open: false,
            passphrase: TextDraft::default(),
            visible: false,
            remember: false,
            error: None,
            pending: None,
            unlocking: false,
        }
    }
}

impl VaultUnlockPrompt {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, pending: PendingVaultAction) {
        self.open = true;
        self.creating = false;
        self.confirm.clear();
        self.confirm_focused = false;
        self.passphrase.clear();
        self.visible = false;
        self.error = None;
        self.unlocking = false;
        self.pending = Some(pending);
        self.host_label = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.confirm.clear();
        self.confirm_focused = false;
        self.passphrase.clear();
        self.visible = false;
        self.error = None;
        self.unlocking = false;
        self.pending = None;
    }

    /// Close the prompt and return the pending action (for retry after unlock).
    pub fn take_pending_on_success(&mut self) -> Option<PendingVaultAction> {
        let pending = self.pending.take();
        self.close();
        pending
    }

    pub fn pending(&self) -> Option<&PendingVaultAction> {
        self.pending.as_ref()
    }

    pub fn passphrase(&self) -> &str {
        &self.passphrase.value
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn remember(&self) -> bool {
        self.remember
    }

    pub fn set_remember(&mut self, remember: bool) {
        self.remember = remember;
    }

    pub fn toggle_remember(&mut self) {
        self.remember = !self.remember;
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn unlocking(&self) -> bool {
        self.unlocking
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.unlocking = false;
        self.error = Some(message.into());
    }

    pub fn set_unlocking(&mut self) {
        self.unlocking = true;
        self.error = None;
    }

    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    fn focused_draft(&mut self) -> &mut TextDraft {
        if self.confirm_focused {
            &mut self.confirm
        } else {
            &mut self.passphrase
        }
    }

    pub fn insert(&mut self, text: &str) {
        if self.focused_draft().insert(text, usize::MAX, false) {
            self.error = None;
        }
    }

    /// Shared text editing (Backspace, Delete, caret, selection).
    pub fn edit(&mut self, edit: TextEdit) {
        if self.focused_draft().apply(edit) {
            self.error = None;
        }
    }

    pub fn backspace(&mut self) {
        self.edit(TextEdit::Backspace { by_word: false });
    }

    /// Whether this prompt creates a new vault (no vault header yet).
    pub fn creating(&self) -> bool {
        self.creating
    }

    pub fn set_creating(&mut self, creating: bool) {
        self.creating = creating;
        if !creating {
            self.confirm.clear();
            self.confirm_focused = false;
        }
    }

    pub fn title(&self) -> &'static str {
        if self.creating {
            "Create your vault"
        } else {
            "Unlock your vault"
        }
    }

    /// Name the host in the body ("jerem prod uses a saved password...").
    pub fn set_host_label(&mut self, label: Option<String>) {
        self.host_label = label;
    }

    pub fn action_label(&self) -> &'static str {
        if self.creating {
            "Create"
        } else {
            "Unlock"
        }
    }

    /// Subtitle: why the vault is needed, plus a warning when creating.
    pub fn subtitle(&self) -> String {
        if let (
            Some(name),
            false,
            Some(
                PendingVaultAction::OpenHost(_)
                | PendingVaultAction::AddHostSession(_)
                | PendingVaultAction::OpenSftp { .. },
            ),
        ) = (
            self.host_label.as_deref(),
            self.creating,
            self.pending.as_ref(),
        ) {
            return format!(
                "{name} uses a saved password. Enter your vault passphrase to connect."
            );
        }
        let why = self
            .pending
            .as_ref()
            .map(PendingVaultAction::subtitle)
            .unwrap_or("Enter your vault passphrase.");
        if self.creating {
            "Choose a passphrase for the new vault. It encrypts saved passwords and keys and cannot be recovered.".to_string()
        } else {
            why.to_string()
        }
    }

    pub fn confirm_focused(&self) -> bool {
        self.confirm_focused
    }

    pub fn focus_confirm(&mut self) {
        if self.creating {
            self.confirm_focused = true;
        }
    }

    pub fn focus_passphrase(&mut self) {
        self.confirm_focused = false;
    }

    /// Tab: move between the passphrase and confirm fields.
    pub fn toggle_field(&mut self) {
        if self.creating {
            self.confirm_focused = !self.confirm_focused;
        }
    }

    /// The passphrase to submit, or the message to show instead.
    pub fn validate(&self) -> Result<String, String> {
        let pass = &self.passphrase.value;
        if pass.trim().is_empty() {
            return Err("Enter your vault passphrase".into());
        }
        if self.creating {
            if pass.trim().chars().count() < MIN_PASSPHRASE_CHARS {
                return Err(format!("Use at least {MIN_PASSPHRASE_CHARS} characters"));
            }
            if self.confirm.value != *pass {
                return Err("Passphrases do not match".into());
            }
        }
        Ok(pass.clone())
    }

    /// Paint model for the confirm field.
    pub fn confirm_field_paint(&self) -> FieldPaint {
        FieldPaint::from_draft_masked(
            &self.confirm,
            "Repeat passphrase…",
            self.confirm_focused,
            !self.visible,
        )
    }

    /// Display string for the shared Settings field painter (masking included).
    pub fn field_paint_text(&self) -> (String, bool) {
        let paint = self.field_paint();
        (paint.text, paint.placeholder)
    }

    /// Shared [`FieldPaint`] model (same path as Settings / SFTP fields).
    pub fn field_paint(&self) -> FieldPaint {
        FieldPaint::from_draft_masked(
            &self.passphrase,
            "Enter passphrase…",
            !self.confirm_focused,
            !self.visible,
        )
    }

    pub fn display_value(&self) -> String {
        self.field_paint_text().0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VaultUnlockLayout {
    pub x: f32,
    pub y: f32,
    /// Create mode: a confirm field below the passphrase field.
    pub creating: bool,
    /// Wrapped body lines (the host name makes this vary).
    pub body_lines: usize,
    /// Room for the error / progress line.
    pub with_hint: bool,
}

/// Width of a button for `label` (shared estimate, see `confirm`).
pub fn button_width(label: &str) -> f32 {
    label.chars().count() as f32 * BUTTON_FONT * BUTTON_ADVANCE + 40.0
}

impl VaultUnlockLayout {
    pub fn centered(window_width: f32, window_height: f32) -> Self {
        Self::build(window_width, window_height, false, 2, false)
    }

    /// Layout matching `prompt` (create mode adds the confirm field; the
    /// body wraps with the shared width estimate).
    pub fn for_prompt(
        window_width: f32,
        window_height: f32,
        prompt: &VaultUnlockPrompt,
    ) -> Self {
        let lines = wrap_text(&prompt.subtitle(), WIDTH - 2.0 * PAD, |s| {
            estimate_text_width(s, BODY_FONT)
        })
        .len();
        let hint = prompt.error().is_some() || prompt.unlocking();
        Self::build(window_width, window_height, prompt.creating(), lines, hint)
    }

    pub fn for_creating(window_width: f32, window_height: f32, creating: bool) -> Self {
        Self::build(window_width, window_height, creating, 2, false)
    }

    fn build(w: f32, h: f32, creating: bool, body_lines: usize, with_hint: bool) -> Self {
        let mut l = Self {
            x: 0.0,
            y: 0.0,
            creating,
            body_lines: body_lines.max(1),
            with_hint,
        };
        let height = l.height();
        l.x = ((w - WIDTH) * 0.5).round().max(8.0);
        l.y = ((h - height) * 0.5).round().max(8.0);
        l
    }

    pub fn height(&self) -> f32 {
        let confirm = if self.creating {
            FIELD_GAP + FIELD_HEIGHT
        } else {
            0.0
        };
        let hint = if self.with_hint {
            HINT_GAP + HINT_HEIGHT
        } else {
            0.0
        };
        PAD * 2.0
            + TILE
            + GAP
            + TITLE_LINE
            + TITLE_BODY_GAP
            + self.body_lines as f32 * BODY_LINE
            + GAP
            + FIELD_HEIGHT
            + confirm
            + GAP
            + CHECK_ROW_HEIGHT
            + hint
            + GAP
            + BUTTON_HEIGHT
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, WIDTH, self.height())
    }

    fn inner_w() -> f32 {
        WIDTH - 2.0 * PAD
    }

    /// Lock tile at the top-left.
    pub fn tile_rect(&self) -> Rect {
        Rect::new(self.x + PAD, self.y + PAD, TILE, TILE)
    }

    pub fn title_rect(&self) -> Rect {
        Rect::new(
            self.x + PAD,
            self.tile_rect().bottom() + GAP,
            Self::inner_w(),
            TITLE_LINE,
        )
    }

    pub fn subtitle_rect(&self) -> Rect {
        Rect::new(
            self.x + PAD,
            self.title_rect().bottom() + TITLE_BODY_GAP,
            Self::inner_w(),
            self.body_lines as f32 * BODY_LINE,
        )
    }

    /// Passphrase field box.
    pub fn passphrase_card_rect(&self) -> Rect {
        Rect::new(
            self.x + PAD,
            self.subtitle_rect().bottom() + GAP,
            Self::inner_w(),
            FIELD_HEIGHT,
        )
    }

    /// Confirm-passphrase field (create mode only).
    pub fn confirm_card_rect(&self) -> Option<Rect> {
        self.creating.then(|| {
            let pass = self.passphrase_card_rect();
            Rect::new(pass.x, pass.bottom() + FIELD_GAP, pass.width, FIELD_HEIGHT)
        })
    }

    pub fn input_rect(&self) -> Rect {
        self.passphrase_card_rect()
    }

    /// Text area of the passphrase field (left of the eye).
    pub fn text_rect(&self) -> Rect {
        let f = self.passphrase_card_rect();
        Rect::new(f.x, f.y, (f.width - EYE_SLOT - 6.0).max(0.0), f.height)
    }

    pub fn eye_rect(&self) -> Rect {
        let f = self.passphrase_card_rect();
        Rect::new(
            f.right() - 6.0 - EYE_SLOT,
            f.y + (f.height - EYE_SLOT) / 2.0,
            EYE_SLOT,
            EYE_SLOT,
        )
    }

    pub fn eye_icon_size() -> f32 {
        EYE_ICON
    }

    /// Full hit row for the "Remember on this computer" checkbox.
    pub fn remember_row_rect(&self) -> Rect {
        let above = self
            .confirm_card_rect()
            .unwrap_or_else(|| self.passphrase_card_rect());
        Rect::new(
            self.x + PAD,
            above.bottom() + GAP,
            Self::inner_w(),
            CHECK_ROW_HEIGHT,
        )
    }

    pub fn remember_box_rect(&self) -> Rect {
        let row = self.remember_row_rect();
        Rect::new(row.x, row.y, CHECK_SIZE, CHECK_SIZE)
    }

    /// Error / progress line (zero height when nothing is reserved).
    pub fn hint_rect(&self) -> Rect {
        let row = self.remember_row_rect();
        if self.with_hint {
            Rect::new(row.x, row.bottom() + HINT_GAP, Self::inner_w(), HINT_HEIGHT)
        } else {
            Rect::new(row.x, row.bottom(), Self::inner_w(), 0.0)
        }
    }

    pub fn unlock_button_rect(&self) -> Rect {
        self.unlock_button_rect_for("Unlock")
    }

    pub fn cancel_button_rect(&self) -> Rect {
        self.button_rects("Cancel", "Unlock").0
    }

    /// Confirm button sized for `label` ("Unlock" / "Create").
    pub fn unlock_button_rect_for(&self, label: &str) -> Rect {
        self.button_rects("Cancel", label).1
    }

    /// `(cancel, confirm)` rects for the given labels.
    pub fn button_rects(&self, cancel: &str, confirm: &str) -> (Rect, Rect) {
        let d = self.rect();
        let y = d.bottom() - PAD - BUTTON_HEIGHT;
        let cw = button_width(confirm);
        let kw = button_width(cancel);
        let confirm_r = Rect::new(d.right() - PAD - cw, y, cw, BUTTON_HEIGHT);
        let cancel_r = Rect::new(confirm_r.x - BUTTON_GAP - kw, y, kw, BUTTON_HEIGHT);
        (cancel_r, confirm_r)
    }

    pub fn hit_test(&self, x: f32, y: f32) -> VaultUnlockHit {
        self.hit_test_labels(x, y, "Unlock")
    }

    /// Hit test with the confirm button sized for `confirm` ("Create" differs).
    pub fn hit_test_labels(&self, x: f32, y: f32, confirm: &str) -> VaultUnlockHit {
        if !self.rect().contains(x, y) {
            return VaultUnlockHit::Cancel;
        }
        let (cancel, ok) = self.button_rects("Cancel", confirm);
        if ok.contains(x, y) {
            return VaultUnlockHit::Unlock;
        }
        if cancel.contains(x, y) {
            return VaultUnlockHit::Cancel;
        }
        if self.remember_row_rect().contains(x, y) {
            return VaultUnlockHit::ToggleRemember;
        }
        if self.eye_rect().contains(x, y) {
            return VaultUnlockHit::ToggleVisible;
        }
        if self.confirm_card_rect().is_some_and(|r| r.contains(x, y)) {
            return VaultUnlockHit::ConfirmField;
        }
        if self.passphrase_card_rect().contains(x, y) {
            return VaultUnlockHit::Field;
        }
        VaultUnlockHit::Consume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_unlock_vault_unlocks_an_existing_vault_and_creates_a_missing_one() {
        let unlock = PendingVaultAction::settings_unlock(true);
        assert_eq!(unlock, PendingVaultAction::UnlockVault);
        assert!(!unlock.subtitle().contains("new vault"));
        assert_eq!(
            PendingVaultAction::settings_unlock(false),
            PendingVaultAction::CreateVault
        );

        // Opened over an existing vault, the prompt reads as an unlock.
        let mut p = VaultUnlockPrompt::default();
        p.open(PendingVaultAction::settings_unlock(true));
        p.set_creating(false);
        assert_eq!(p.title(), "Unlock your vault");
        assert_eq!(p.action_label(), "Unlock");
        assert!(!p.subtitle().contains("new vault"), "{}", p.subtitle());
    }

    #[test]
    fn hit_test_finds_unlock_eye_and_remember() {
        let layout = VaultUnlockLayout::centered(1200.0, 800.0);
        let unlock = layout.unlock_button_rect();
        assert_eq!(
            layout.hit_test(unlock.x + 2.0, unlock.y + 2.0),
            VaultUnlockHit::Unlock
        );
        let eye = layout.eye_rect();
        assert_eq!(
            layout.hit_test(eye.x + 2.0, eye.y + 2.0),
            VaultUnlockHit::ToggleVisible
        );
        let row = layout.remember_row_rect();
        assert_eq!(
            layout.hit_test(row.x + 2.0, row.y + 2.0),
            VaultUnlockHit::ToggleRemember
        );
        let text = layout.text_rect();
        assert_eq!(
            layout.hit_test(text.x + 2.0, text.y + 2.0),
            VaultUnlockHit::Field
        );
    }

    #[test]
    fn body_names_the_host_and_the_dialog_follows_the_mock_metrics() {
        let mut prompt = VaultUnlockPrompt::default();
        prompt.open(PendingVaultAction::OpenHost("h1".into()));
        prompt.set_host_label(Some("jerem prod".into()));
        assert_eq!(
            prompt.subtitle(),
            "jerem prod uses a saved password. Enter your vault passphrase to connect."
        );
        let l = VaultUnlockLayout::for_prompt(1440.0, 900.0, &prompt);
        assert_eq!(l.rect().width, 400.0);
        assert_eq!(l.tile_rect().width, 48.0);
        assert_eq!(l.passphrase_card_rect().height, 46.0);
        let (cancel, ok) = l.button_rects("Cancel", "Unlock");
        assert_eq!((cancel.height, ok.height), (44.0, 44.0));
        assert!((ok.right() - (l.rect().right() - 32.0)).abs() < 0.01);
        assert!((ok.bottom() - (l.rect().bottom() - 32.0)).abs() < 0.01);
        assert!(l.rect().contains(l.rect().x + 1.0, l.rect().y + 1.0));
        // A non-host reason keeps its generic copy.
        prompt.open(PendingVaultAction::SubmitHostForm);
        assert!(prompt.subtitle().contains("encrypt and save"));
    }

    #[test]
    fn an_error_line_makes_room_above_the_buttons() {
        let mut prompt = VaultUnlockPrompt::default();
        prompt.open(PendingVaultAction::SubmitHostForm);
        let plain = VaultUnlockLayout::for_prompt(1200.0, 800.0, &prompt);
        prompt.set_error("Wrong passphrase");
        let err = VaultUnlockLayout::for_prompt(1200.0, 800.0, &prompt);
        assert_eq!(
            err.rect().height - plain.rect().height,
            HINT_GAP + HINT_HEIGHT
        );
        assert!(err.hint_rect().bottom() <= err.unlock_button_rect().y);
    }

    #[test]
    fn take_pending_clears_prompt() {
        let mut prompt = VaultUnlockPrompt::default();
        prompt.open(PendingVaultAction::OpenHost("h1".into()));
        prompt.set_remember(true);
        assert!(prompt.is_open());
        let pending = prompt.take_pending_on_success();
        assert_eq!(pending, Some(PendingVaultAction::OpenHost("h1".into())));
        assert!(!prompt.is_open());
        assert!(prompt.remember());
    }

    #[test]
    fn first_use_creates_the_vault_with_a_confirmed_passphrase() {
        let mut prompt = VaultUnlockPrompt::default();
        prompt.open(PendingVaultAction::SubmitHostForm);
        prompt.set_creating(true);
        assert_eq!(prompt.title(), "Create your vault");
        assert_eq!(prompt.action_label(), "Create");
        prompt.insert("correct horse");
        prompt.focus_confirm();
        prompt.insert("correct hose");
        assert_eq!(prompt.validate().unwrap_err(), "Passphrases do not match");
        prompt.backspace();
        prompt.backspace();
        prompt.insert("rse");
        assert_eq!(prompt.validate().unwrap(), "correct horse");

        // Unlocking an existing vault needs no confirmation.
        let mut unlock = VaultUnlockPrompt::default();
        unlock.open(PendingVaultAction::SubmitHostForm);
        assert_eq!(unlock.title(), "Unlock your vault");
        unlock.insert("anything");
        assert_eq!(unlock.validate().unwrap(), "anything");
    }

    #[test]
    fn create_layout_has_a_confirm_field_that_hits() {
        let layout = VaultUnlockLayout::for_creating(1200.0, 800.0, true);
        let confirm = layout.confirm_card_rect().expect("confirm card");
        assert!(confirm.y >= layout.passphrase_card_rect().bottom());
        assert!(layout.remember_row_rect().y >= confirm.bottom());
        assert!(layout.unlock_button_rect().y >= layout.hint_rect().bottom());
        assert_eq!(
            layout.rect().height,
            VaultUnlockLayout::centered(1200.0, 800.0).rect().height
                + FIELD_GAP
                + FIELD_HEIGHT
        );
        assert_eq!(
            layout.hit_test(confirm.x + 4.0, confirm.y + 30.0),
            VaultUnlockHit::ConfirmField
        );
        assert!(VaultUnlockLayout::centered(1200.0, 800.0)
            .confirm_card_rect()
            .is_none());
    }
}
