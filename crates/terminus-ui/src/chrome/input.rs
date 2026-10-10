use super::*;

impl Chrome {
    /// Scroll the selected machine's row into view when the selection
    /// changed (palette, tab switch, a new session). Only once per
    /// change: the user may then scroll the list away freely. An id
    /// without a row (yet) is retried on the next call.
    pub fn reveal_machine(&mut self, id: &str, window_height: f32) -> bool {
        if self.revealed_machine.as_deref() == Some(id) {
            return false;
        }
        let Some(index) = self.panel.row_of_host(id) else {
            return false;
        };
        if !self.panel.visible_row_indices().contains(&index) {
            return false;
        }
        self.revealed_machine = Some(id.to_string());
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        self.panel.reveal_row(index, origin_y, height)
    }

    /// Route a pixel wheel delta (touchpads; positive = content moves
    /// down, i.e. scroll toward the top) over the panel.
    pub fn handle_wheel_pixels(
        &mut self,
        window_height: f32,
        x: f32,
        y: f32,
        dy: f32,
    ) -> bool {
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        if !self.panel.rect(origin_y, height).contains(x, y) {
            return false;
        }
        let before = self.panel.scroll;
        self.panel.scroll_by(-dy, origin_y, height);
        self.panel.scroll != before || self.panel.content_height() == 0.0
    }
    /// Route a wheel notch over the panel; returns whether it was consumed.
    pub fn handle_wheel(
        &mut self,
        window_height: f32,
        x: f32,
        y: f32,
        lines: f32,
    ) -> bool {
        if !self.hosts_visible() {
            return false;
        }
        let origin_y = self.origin_y();
        let height = window_height - origin_y;
        if !self.panel.rect(origin_y, height).contains(x, y) {
            return false;
        }
        let before = self.panel.scroll;
        // Wheel up (negative lines) scrolls toward the top.
        self.panel.scroll_rows(-lines, origin_y, height);
        self.panel.scroll != before || self.panel.content_height() == 0.0
    }

    /// Route a keyboard input to the editor. `None` when it is closed.
    pub fn handle_form_input(
        &mut self,
        input: FormInput,
        text: &str,
    ) -> Option<FormOutcome> {
        if !self.form.is_open() {
            return None;
        }
        let outcome = self.form.handle_input(input, text);
        if outcome == FormOutcome::Cancel {
            self.form.close();
        }
        Some(outcome)
    }

    /// Route a keyboard input to the snippet editor. `None` when it is closed.
    pub fn handle_snippet_form_input(
        &mut self,
        input: crate::add_snippet::FormInput,
        text: &str,
    ) -> Option<crate::add_snippet::FormOutcome> {
        if !self.snippet_form.is_open() {
            return None;
        }
        let outcome = self.snippet_form.handle_input(input, text);
        if outcome == crate::add_snippet::FormOutcome::Cancel {
            self.snippet_form.inner.closing = true;
        }
        Some(outcome)
    }

    /// Shared text edit (Delete, caret, selection…) for the unlock prompt.
    pub fn edit_vault_unlock(
        &mut self,
        edit: crate::components::input::TextEdit,
    ) -> bool {
        if !self.vault_unlock.is_open() {
            return false;
        }
        self.vault_unlock.edit(edit);
        true
    }

    pub fn handle_vault_unlock_input(
        &mut self,
        input: FormInput,
        text: &str,
    ) -> Option<bool> {
        if !self.vault_unlock.is_open() {
            return None;
        }
        match input {
            FormInput::Text => {
                self.vault_unlock.insert(text);
                Some(false)
            }
            FormInput::Backspace => {
                self.vault_unlock.backspace();
                Some(false)
            }
            FormInput::Enter => Some(true),
            FormInput::Escape => {
                self.vault_unlock.close();
                Some(false)
            }
            FormInput::Next | FormInput::Previous => {
                self.vault_unlock.toggle_field();
                Some(false)
            }
            _ => Some(false),
        }
    }

    /// Where the editor dialog sits for this window size.
    pub fn dialog_layout(
        &self,
        window_width: f32,
        window_height: f32,
    ) -> crate::add_host::AddHostLayout {
        crate::add_host::AddHostLayout::centered(
            window_width,
            window_height,
            self.form.anchor_height(),
        )
    }
}
