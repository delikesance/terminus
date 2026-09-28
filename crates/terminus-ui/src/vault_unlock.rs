//! Modal prompt for the vault passphrase when a sealed secret is needed.
//!
//! Shown when connecting with a saved SSH password (or saving one) while the
//! vault is still locked. The passphrase row reuses the Settings field-card
//! geometry (`field_input_in_card` + eye slot).

use crate::geom::Rect;
use crate::settings::{
    field_input_in_card, FIELD_CARD_HEIGHT, FIELD_CARD_PAD, FIELD_EYE_ICON,
    FIELD_EYE_SLOT,
};

pub const WIDTH: f32 = 400.0;
pub const HEIGHT: f32 = 292.0;
pub const PAD: f32 = 24.0;
pub const TITLE_HEIGHT: f32 = 28.0;
pub const CHECK_SIZE: f32 = 16.0;
pub const CHECK_ROW_HEIGHT: f32 = 28.0;
pub const BUTTON_HEIGHT: f32 = 32.0;
pub const BUTTON_WIDTH: f32 = 88.0;
pub const CANCEL_WIDTH: f32 = 72.0;
pub const BUTTON_GAP: f32 = 8.0;
pub const RADIUS: f32 = 16.0;
pub const INPUT_RADIUS: f32 = 12.0;
/// Extra height for the confirm card when creating a vault.
pub const CONFIRM_EXTRA: f32 = FIELD_CARD_HEIGHT + 12.0;
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
    passphrase: String,
    visible: bool,
    /// Persist passphrase in the OS keyring after a successful unlock.
    remember: bool,
    error: Option<String>,
    pending: Option<PendingVaultAction>,
    unlocking: bool,
    /// No vault exists yet: this prompt creates one (asks twice).
    creating: bool,
    confirm: String,
    confirm_focused: bool,
}

impl Default for VaultUnlockPrompt {
    fn default() -> Self {
        Self {
            creating: false,
            confirm: String::new(),
            confirm_focused: false,
            open: false,
            passphrase: String::new(),
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
        &self.passphrase
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

    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.confirm_focused {
            self.confirm.push_str(text);
        } else {
            self.passphrase.push_str(text);
        }
        self.error = None;
    }

    pub fn backspace(&mut self) {
        if self.confirm_focused {
            self.confirm.pop();
        } else {
            self.passphrase.pop();
        }
        self.error = None;
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
            "Create Vault"
        } else {
            "Unlock Vault"
        }
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
        if self.passphrase.trim().is_empty() {
            return Err("Enter your vault passphrase".into());
        }
        if self.creating {
            if self.passphrase.trim().chars().count() < MIN_PASSPHRASE_CHARS {
                return Err(format!("Use at least {MIN_PASSPHRASE_CHARS} characters"));
            }
            if self.confirm != self.passphrase {
                return Err("Passphrases do not match".into());
            }
        }
        Ok(self.passphrase.clone())
    }

    /// Paint model for the confirm field.
    pub fn confirm_field_paint(&self) -> crate::text_field::FieldPaint {
        use crate::text_field::FieldPaint;
        if self.confirm.is_empty() {
            FieldPaint {
                text: "Repeat passphrase…".into(),
                placeholder: true,
                show_caret: self.confirm_focused,
                caret_prefix: String::new(),
                selection: None,
            }
        } else {
            let text = if self.visible {
                self.confirm.clone()
            } else {
                "•".repeat(self.confirm.chars().count())
            };
            FieldPaint {
                caret_prefix: text.clone(),
                text,
                placeholder: false,
                show_caret: self.confirm_focused,
                selection: None,
            }
        }
    }

    /// Display string for the shared Settings field painter (masking included).
    pub fn field_paint_text(&self) -> (String, bool) {
        let paint = self.field_paint();
        (paint.text, paint.placeholder)
    }

    /// Shared [`FieldPaint`] model (same path as Settings / SFTP fields).
    pub fn field_paint(&self) -> crate::text_field::FieldPaint {
        use crate::text_field::FieldPaint;
        let caret = !self.confirm_focused;
        if self.passphrase.is_empty() {
            FieldPaint {
                text: "Enter passphrase…".into(),
                placeholder: true,
                show_caret: caret,
                caret_prefix: String::new(),
                selection: None,
            }
        } else if self.visible {
            FieldPaint {
                text: self.passphrase.clone(),
                placeholder: false,
                show_caret: caret,
                caret_prefix: self.passphrase.clone(),
                selection: None,
            }
        } else {
            let masked = "•".repeat(self.passphrase.chars().count());
            FieldPaint {
                text: masked.clone(),
                placeholder: false,
                show_caret: caret,
                caret_prefix: masked,
                selection: None,
            }
        }
    }

    pub fn display_value(&self) -> String {
        self.field_paint_text().0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VaultUnlockLayout {
    pub x: f32,
    pub y: f32,
    /// Create mode: a confirm card below the passphrase card.
    pub creating: bool,
}

impl VaultUnlockLayout {
    pub fn centered(window_width: f32, window_height: f32) -> Self {
        Self::for_creating(window_width, window_height, false)
    }

    /// Layout matching `prompt` (create mode adds the confirm card).
    pub fn for_prompt(
        window_width: f32,
        window_height: f32,
        prompt: &VaultUnlockPrompt,
    ) -> Self {
        Self::for_creating(window_width, window_height, prompt.creating())
    }

    pub fn for_creating(window_width: f32, window_height: f32, creating: bool) -> Self {
        let height = Self::height_for(creating);
        Self {
            x: ((window_width - WIDTH) * 0.5).max(8.0),
            y: ((window_height - height) * 0.5).max(8.0),
            creating,
        }
    }

    fn height_for(creating: bool) -> f32 {
        if creating {
            HEIGHT + CONFIRM_EXTRA
        } else {
            HEIGHT
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, WIDTH, Self::height_for(self.creating))
    }

    /// Confirm-passphrase card (create mode only).
    pub fn confirm_card_rect(&self) -> Option<Rect> {
        self.creating.then(|| {
            let pass = self.passphrase_card_rect();
            Rect::new(pass.x, pass.bottom() + 12.0, pass.width, FIELD_CARD_HEIGHT)
        })
    }

    pub fn title_rect(&self) -> Rect {
        Rect::new(self.x + PAD, self.y + PAD, WIDTH - 2.0 * PAD, TITLE_HEIGHT)
    }

    pub fn subtitle_rect(&self) -> Rect {
        Rect::new(
            self.x + PAD,
            self.y + PAD + TITLE_HEIGHT,
            WIDTH - 2.0 * PAD,
            36.0,
        )
    }

    /// Settings-style field card hosting the passphrase input + eye.
    pub fn passphrase_card_rect(&self) -> Rect {
        Rect::new(
            self.x + PAD,
            self.y + PAD + TITLE_HEIGHT + 40.0,
            WIDTH - 2.0 * PAD,
            FIELD_CARD_HEIGHT,
        )
    }

    pub fn input_rect(&self) -> Rect {
        field_input_in_card(self.passphrase_card_rect())
    }

    pub fn text_rect(&self) -> Rect {
        let input = self.input_rect();
        Rect::new(
            input.x,
            input.y,
            (input.width - FIELD_EYE_SLOT).max(0.0),
            input.height,
        )
    }

    pub fn eye_rect(&self) -> Rect {
        let input = self.input_rect();
        Rect::new(
            input.right() - FIELD_EYE_SLOT,
            input.y,
            FIELD_EYE_SLOT,
            input.height,
        )
    }

    pub fn eye_icon_size() -> f32 {
        FIELD_EYE_ICON
    }

    /// Full hit row for the "Remember on this device" checkbox.
    pub fn remember_row_rect(&self) -> Rect {
        let above = self
            .confirm_card_rect()
            .unwrap_or_else(|| self.passphrase_card_rect());
        Rect::new(
            self.x + PAD,
            above.bottom() + 12.0,
            WIDTH - 2.0 * PAD,
            CHECK_ROW_HEIGHT,
        )
    }

    pub fn remember_box_rect(&self) -> Rect {
        let row = self.remember_row_rect();
        Rect::new(
            row.x,
            row.y + (row.height - CHECK_SIZE) * 0.5,
            CHECK_SIZE,
            CHECK_SIZE,
        )
    }

    pub fn hint_rect(&self) -> Rect {
        Rect::new(
            self.x + PAD,
            self.remember_row_rect().bottom() + 4.0,
            WIDTH - 2.0 * PAD,
            18.0,
        )
    }

    pub fn unlock_button_rect(&self) -> Rect {
        let dialog = self.rect();
        Rect::new(
            dialog.right() - PAD - BUTTON_WIDTH,
            dialog.bottom() - PAD - BUTTON_HEIGHT,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
        )
    }

    pub fn cancel_button_rect(&self) -> Rect {
        let unlock = self.unlock_button_rect();
        Rect::new(
            unlock.x - BUTTON_GAP - CANCEL_WIDTH,
            unlock.y,
            CANCEL_WIDTH,
            BUTTON_HEIGHT,
        )
    }

    pub fn hit_test(&self, x: f32, y: f32) -> VaultUnlockHit {
        let dialog = self.rect();
        if !dialog.contains(x, y) {
            return VaultUnlockHit::Cancel;
        }
        if self.unlock_button_rect().contains(x, y) {
            return VaultUnlockHit::Unlock;
        }
        if self.cancel_button_rect().contains(x, y) {
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
        if self.text_rect().contains(x, y)
            || self.input_rect().contains(x, y)
            || self.passphrase_card_rect().contains(x, y)
        {
            return VaultUnlockHit::Field;
        }
        VaultUnlockHit::Consume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let _ = FIELD_CARD_PAD;
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
        assert_eq!(prompt.title(), "Create Vault");
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
        assert_eq!(unlock.title(), "Unlock Vault");
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
            layout.hit_test(confirm.x + 4.0, confirm.y + 30.0),
            VaultUnlockHit::ConfirmField
        );
        assert!(VaultUnlockLayout::centered(1200.0, 800.0)
            .confirm_card_rect()
            .is_none());
    }
}
