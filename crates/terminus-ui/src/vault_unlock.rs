//! Modal prompt for the vault passphrase when a sealed secret is needed.
//!
//! Shown when connecting with a saved SSH password (or saving one) while the
//! vault is still locked. The passphrase row reuses the Settings field-card
//! geometry (`field_input_in_card` + eye slot).

use crate::geom::Rect;
use crate::settings::{
    field_input_in_card, FIELD_CARD_HEIGHT, FIELD_CARD_PAD, FIELD_EYE_ICON, FIELD_EYE_SLOT,
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

/// Why the prompt was opened — drives the subtitle and the retry after unlock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingVaultAction {
    OpenHost(String),
    AddHostSession(String),
    SubmitHostForm,
}

impl PendingVaultAction {
    pub fn subtitle(&self) -> &'static str {
        match self {
            Self::OpenHost(_) | Self::AddHostSession(_) => {
                "Enter your vault passphrase to use the saved SSH password."
            }
            Self::SubmitHostForm => {
                "Enter your vault passphrase to encrypt and save the password."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultUnlockHit {
    Field,
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
}

impl Default for VaultUnlockPrompt {
    fn default() -> Self {
        Self {
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
        self.passphrase.clear();
        self.visible = false;
        self.error = None;
        self.unlocking = false;
        self.pending = Some(pending);
    }

    pub fn close(&mut self) {
        self.open = false;
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
        self.passphrase.push_str(text);
        self.error = None;
    }

    pub fn backspace(&mut self) {
        self.passphrase.pop();
        self.error = None;
    }

    /// Display string for the shared Settings field painter (masking included).
    pub fn field_paint_text(&self) -> (String, bool) {
        if self.passphrase.is_empty() {
            ("Enter passphrase…".into(), true)
        } else if self.visible {
            (self.passphrase.clone(), false)
        } else {
            ("•".repeat(self.passphrase.chars().count()), false)
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
}

impl VaultUnlockLayout {
    pub fn centered(window_width: f32, window_height: f32) -> Self {
        Self {
            x: ((window_width - WIDTH) * 0.5).max(8.0),
            y: ((window_height - HEIGHT) * 0.5).max(8.0),
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, WIDTH, HEIGHT)
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
        Rect::new(
            self.x + PAD,
            self.passphrase_card_rect().bottom() + 12.0,
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
}
