use super::*;

impl SettingsModal {
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
            if let Some(pass) =
                self.key_draft_passphrase_rect(window_width, window_height)
            {
                if pass.contains(x, y) {
                    return SettingsHit::FocusKeyPassphrase;
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
                if !self.keys[i].public_key.is_empty()
                    && self
                        .key_copy_rect(window_width, window_height, i)
                        .contains(x, y)
                {
                    return SettingsHit::CopyPublicKey(i);
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
            | SettingsHit::FocusKeyPem
            | SettingsHit::FocusKeyPassphrase => ChromeCursor::Text,
            SettingsHit::Consume => ChromeCursor::Default,
            SettingsHit::Close
            | SettingsHit::Done
            | SettingsHit::Tab(_)
            | SettingsHit::NewKey
            | SettingsHit::DeleteKey(_)
            | SettingsHit::CopyPublicKey(_)
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
