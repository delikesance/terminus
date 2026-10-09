use super::*;
use crate::geom::Rect;

impl SettingsModal {
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

    pub(super) fn sql_content_origin(
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

    /// Passphrase card for an encrypted key import (under the PEM card).
    pub fn key_draft_passphrase_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Option<Rect> {
        let draft = self.key_draft_rect(window_width, window_height)?;
        Some(Rect::new(
            draft.x + FIELD_CARD_PAD,
            draft.y + FIELD_CARD_PAD + FIELD_CARD_STEP * 2.0,
            (draft.width - 2.0 * FIELD_CARD_PAD).max(40.0),
            FIELD_CARD_HEIGHT,
        ))
    }

    pub fn key_draft_generate_rect(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> Option<Rect> {
        let _draft = self.key_draft_rect(window_width, window_height)?;
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
        // Sit below the passphrase card; leave room for the error banner when shown.
        let mut actions_y = draft.y + FIELD_CARD_PAD + FIELD_CARD_STEP * 3.0;
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

    pub(super) fn keys_list_origin_y(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> f32 {
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

    /// "Copy public key" control, left of Delete on a key row.
    pub fn key_copy_rect(
        &self,
        window_width: f32,
        window_height: f32,
        index: usize,
    ) -> Rect {
        let delete = self.key_delete_rect(window_width, window_height, index);
        const W: f32 = 118.0;
        const GAP: f32 = 8.0;
        Rect::new(delete.x - GAP - W, delete.y, W, delete.height)
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
        let msg = self.sync_error.as_deref()?;
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
}
