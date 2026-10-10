use super::*;

impl Island {
    /// Toggle the color picker for a given tab index
    pub fn toggle_color_picker(
        &mut self,
        tab_index: usize,
        current_title: &str,
        context_manager: &mut ContextManager<EventProxy>,
    ) {
        if self.color_picker_tab == Some(tab_index) {
            self.apply_rename(context_manager);
            self.color_picker_tab = None;
        } else {
            self.color_picker_tab = Some(tab_index);
            // Initialize rename input with custom title or current displayed title
            self.rename_input = terminus_ui::TextDraft::new(
                context_manager
                    .custom_title(tab_index)
                    .map(str::to_string)
                    .unwrap_or_else(|| current_title.to_string()),
            );
            self.rename_caret_time = Instant::now();
        }
    }

    /// Close the color picker, applying any pending rename
    pub fn close_color_picker(
        &mut self,
        context_manager: &mut ContextManager<EventProxy>,
    ) {
        if self.color_picker_tab.is_some() {
            self.apply_rename(context_manager);
        }
        self.color_picker_tab = None;
    }

    /// Dismiss the picker WITHOUT committing a pending rename. Used when the
    /// tab set changes underneath it (e.g. a tab close), where the anchored
    /// index may no longer point at the same tab.
    pub fn dismiss_color_picker(&mut self) {
        self.color_picker_tab = None;
    }

    /// Apply the rename input as a custom title for the current picker tab
    pub(super) fn apply_rename(
        &mut self,
        context_manager: &mut ContextManager<EventProxy>,
    ) {
        if let Some(tab) = self.color_picker_tab {
            let trimmed = self.rename_input.value.trim().to_string();
            let title = (!trimmed.is_empty()).then_some(trimmed);
            context_manager.set_custom_title(tab, title);
        }
    }

    /// Handle keyboard input while the color picker (with rename field) is open.
    /// Returns true if input was consumed.
    /// Handle one key event for the rename input. The caller
    /// (`has_key_wait`'s `Modal::IslandRename` arm) already verified
    /// the picker is open via `active_modal` and consumes the event
    /// unconditionally, so there is nothing to return.
    pub fn handle_rename_input(
        &mut self,
        key_event: &rio_window::event::KeyEvent,
        modifiers: rio_window::keyboard::ModifiersState,
        context_manager: &mut ContextManager<EventProxy>,
    ) {
        use rio_window::event::ElementState;
        use rio_window::keyboard::{Key, NamedKey};

        if key_event.state != ElementState::Pressed {
            return; // consume release events too
        }

        match &key_event.logical_key {
            Key::Named(NamedKey::Escape) => {
                // Cancel — discard input, close picker
                self.color_picker_tab = None;
            }
            Key::Named(NamedKey::Enter) => {
                // Confirm — apply rename and close
                self.apply_rename(context_manager);
                self.color_picker_tab = None;
            }
            other => {
                // Same edit routing as every other text field.
                let mods = modifiers;
                if let Some(edit) =
                    crate::screen::chrome_input::text_edit_for_key(other, mods)
                {
                    if self.rename_input.apply(edit) {
                        self.rename_caret_time = Instant::now();
                    }
                } else if let Some(text) = key_event.text.as_ref() {
                    self.append_rename_text(text.as_str());
                }
            }
        }
    }

    /// Append committed or typed text to the rename input, applying
    /// the shared overlay input policy (`is_printable_text`) so the
    /// key path and the IME commit path can never drift. Returns
    /// whether text was actually appended (same contract as
    /// `CommandPalette::append_query`).
    pub fn append_rename_text(&mut self, text: &str) -> bool {
        if self.color_picker_tab.is_none() || !crate::renderer::is_printable_text(text) {
            return false;
        }
        self.rename_input.insert(text, usize::MAX, false);
        self.rename_caret_time = Instant::now();
        true
    }

    /// Check if a click hits a color swatch in the picker.
    /// Returns true if the click was consumed.
    pub fn handle_color_picker_click(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        scale_factor: f32,
        window_width: f32,
        num_tabs: usize,
        context_manager: &mut ContextManager<EventProxy>,
    ) -> bool {
        let picker_tab = match self.color_picker_tab {
            Some(t) => t,
            None => return false,
        };

        let mouse_x_unscaled = mouse_x / scale_factor;
        let mouse_y_unscaled = mouse_y / scale_factor;

        // Prefer the content-hug cache from the last paint so the picker
        // stays under the same pill the user clicked.
        let layout = if self.layout_cache.len() == num_tabs {
            self.layout_cache.clone()
        } else {
            tab_strip_layout(window_width, scale_factor, num_tabs, self.max_tab_width)
        };
        let tab_x = layout.slot_x(picker_tab);
        let tab_width = layout.width_at(picker_tab);

        // Picker is rendered just below the island
        let picker_y = ISLAND_HEIGHT;

        // Check if click is within picker vertical range
        if mouse_y_unscaled < picker_y || mouse_y_unscaled > picker_y + PICKER_HEIGHT {
            // Click outside picker — apply rename and close
            self.apply_rename(context_manager);
            self.color_picker_tab = None;
            return false;
        }

        // Total picker width — N color swatches + 1 reset swatch
        let slot_count = PICKER_COLORS.len() + 1;
        let total_swatches_width = slot_count as f32 * PICKER_SWATCH_SIZE
            + (slot_count - 1) as f32 * PICKER_SWATCH_GAP;
        let picker_start_x = tab_x + (tab_width - total_swatches_width) / 2.0;

        // Check each swatch
        let swatch_y = picker_y + PICKER_PADDING + PICKER_TOP_PADDING;
        let swatch_y_end = swatch_y + PICKER_SWATCH_SIZE;
        for (i, color) in PICKER_COLORS.iter().enumerate() {
            let swatch_x =
                picker_start_x + i as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
            if mouse_x_unscaled >= swatch_x
                && mouse_x_unscaled <= swatch_x + PICKER_SWATCH_SIZE
                && mouse_y_unscaled >= swatch_y
                && mouse_y_unscaled <= swatch_y_end
            {
                context_manager.set_custom_color(picker_tab, Some(*color));
                self.apply_rename(context_manager);
                self.color_picker_tab = None;
                return true;
            }
        }

        // Reset swatch — clears any custom color for this tab
        let reset_x = picker_start_x
            + PICKER_COLORS.len() as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
        if mouse_x_unscaled >= reset_x
            && mouse_x_unscaled <= reset_x + PICKER_SWATCH_SIZE
            && mouse_y_unscaled >= swatch_y
            && mouse_y_unscaled <= swatch_y_end
        {
            context_manager.set_custom_color(picker_tab, None);
            self.apply_rename(context_manager);
            self.color_picker_tab = None;
            return true;
        }

        // Clicked in picker area but not on a swatch
        true
    }

    /// Whether the color picker is currently open
    pub fn is_color_picker_open(&self) -> bool {
        self.color_picker_tab.is_some()
    }

    /// Get the title text for a specific tab index
    pub(super) fn get_title_for_tab(
        &self,
        context_manager: &ContextManager<EventProxy>,
        tab_index: usize,
    ) -> String {
        // Custom user-set title takes priority
        if let Some(custom) = context_manager.custom_title(tab_index) {
            return custom.to_string();
        }

        if let Some(context_title) = context_manager.title(tab_index) {
            if !context_title.content.is_empty() && !context_title.content.contains("{{")
            {
                return context_title.content.clone();
            }

            // Fallback to program name if title is empty
            if let Some(ref extra) = context_title.extra {
                if !extra.program.is_empty() {
                    return extra.program.clone();
                }
            }
        }

        // Default fallback - show tab number
        String::from("~")
    }
}
