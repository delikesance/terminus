//! `Screen` keys surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::bindings::kitty_keyboard::build_key_sequence;
use crate::bindings::{
    Action as Act, BindingKey, BindingMode, FontSizeAction, SearchAction, ViAction,
};
use crate::context;
use crate::crosswords::grid::Dimensions;
use crate::crosswords::grid::Scroll;
use crate::crosswords::pos::Side;
use crate::crosswords::vi_mode::ViMotion;
use crate::crosswords::Mode;
use crate::hosts;
use crate::renderer::island;
use crate::selection::{Selection, SelectionType};
use rio_backend::clipboard::{Clipboard, ClipboardType};
use rio_backend::crosswords::pos::Direction;
use rio_window::event::{ElementState, Modifiers, MouseButton};
#[cfg(target_os = "macos")]
use rio_window::keyboard::ModifiersKeyState;
use rio_window::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};
use rio_window::platform::modifier_supplement::KeyEventExtModifierSupplement;

impl Screen<'_> {
    /// Window-level IME preedit update, with the side effects composing
    /// implies. Returns whether a repaint is needed.
    ///
    /// Composing always snaps out of scrollback: the overlay renders
    /// only at `display_offset == 0` while the preedit key gate
    /// swallows input, so a scrolled viewport would mean a live but
    /// invisible composition and a terminal that looks frozen. This
    /// holds during search too; the snap composes with the relative
    /// `Scroll::Delta` restore in `search_reset_state` exactly like a
    /// manual mid-search scroll does, and committing the query re-runs
    /// `goto_match`, which scrolls back to the focused match. The snap
    /// runs on every composition event, not only on changes: candidate
    /// paging re-reports identical text, and that event must still
    /// restore visibility after a mid-composition scroll.
    ///
    /// Selection follows what the equivalent plain typing does: plain
    /// input drops it (`send_bytes`), search typing drops it only
    /// outside vi mode (`search_input` keeps a vi visual selection).
    pub fn set_ime_preedit(&mut self, preedit: Option<crate::ime::Preedit>) -> bool {
        let composing = preedit.is_some();
        let changed = self.ime.preedit() != preedit.as_ref();
        if changed {
            self.ime.set_preedit(preedit);
        }

        let mut needs_render = changed;
        if composing {
            let mut terminal = self.ctx_mut().current_mut().terminal.lock();
            let snapped_offset = terminal.display_offset();
            if snapped_offset != 0 {
                terminal.scroll_display(Scroll::Bottom);
                needs_render = true;
            }
            drop(terminal);
            if snapped_offset != 0 && self.search_active() {
                // Keep the vi-origin restore honest: `search_reset_state`
                // applies a relative `Scroll::Delta`, so the snap's
                // displacement must be recorded the way `goto_match`
                // records its own scrolls, or Esc after composing lands
                // the viewport clamped at the bottom instead of at the
                // vi origin.
                self.search_state.display_offset_delta += snapped_offset as i32;
            }

            if changed {
                if self.search_active() {
                    if !self.get_mode().contains(Mode::VI) {
                        // Clear selection so we do not obstruct any matches.
                        self.context_manager.current_mut().set_selection(None);
                    }
                } else {
                    self.clear_selection();
                }
            }
        }

        if changed {
            self.mark_dirty();
        }
        needs_render
    }

    #[inline]
    pub fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
    }

    #[inline]
    pub fn process_key_event(
        &mut self,
        key: &rio_window::event::KeyEvent,
        clipboard: &mut Clipboard,
    ) {
        if self.ime.preedit().is_some() {
            return;
        }

        if self.sftp.is_some() && key.state == ElementState::Pressed {
            use crate::sftp_ui::SftpKey;
            use rio_window::keyboard::Key as WKey;
            use rio_window::keyboard::NamedKey;

            // Inline name editor captures typing first.
            if self
                .sftp
                .as_ref()
                .is_some_and(|s| s.state.name_edit.is_some())
            {
                match key.logical_key.as_ref() {
                    WKey::Named(NamedKey::Escape) => {
                        if let Some(s) = self.sftp.as_mut() {
                            s.state.cancel_name_edit();
                            self.mark_dirty();
                        }
                        return;
                    }
                    WKey::Named(NamedKey::Enter) => {
                        if let Some(s) = self.sftp.as_mut() {
                            s.commit_name_edit();
                            self.mark_dirty();
                        }
                        return;
                    }
                    WKey::Named(NamedKey::Backspace) => {
                        if let Some(s) = self.sftp.as_mut() {
                            let _ = s.name_edit_backspace();
                            self.mark_dirty();
                        }
                        return;
                    }
                    _ => {
                        if let Some(text) = key.text.as_ref() {
                            if let Some(s) = self.sftp.as_mut() {
                                if s.type_name_edit(text) {
                                    self.mark_dirty();
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            let action = match key.logical_key.as_ref() {
                WKey::Named(NamedKey::Tab) => Some(SftpKey::Tab),
                WKey::Named(NamedKey::ArrowUp) => Some(SftpKey::Up),
                WKey::Named(NamedKey::ArrowDown) => Some(SftpKey::Down),
                WKey::Named(NamedKey::Enter) => Some(SftpKey::Enter),
                WKey::Named(NamedKey::Backspace) => Some(SftpKey::Backspace),
                WKey::Named(NamedKey::Delete) => Some(SftpKey::Delete),
                WKey::Named(NamedKey::F2) => Some(SftpKey::Rename),
                WKey::Named(NamedKey::Escape) => {
                    self.close_sftp_pane();
                    return;
                }
                WKey::Character("n") | WKey::Character("N")
                    if self.modifiers.state().control_key() =>
                {
                    Some(SftpKey::Mkdir)
                }
                WKey::Character("u") | WKey::Character("U")
                    if self.modifiers.state().control_key() =>
                {
                    Some(SftpKey::Upload)
                }
                WKey::Character("d") | WKey::Character("D")
                    if self.modifiers.state().control_key() =>
                {
                    Some(SftpKey::Download)
                }
                _ => None,
            };
            if let Some(action) = action {
                if let Some(session) = self.sftp.as_mut() {
                    if session.handle_key(action) {
                        self.mark_dirty();
                    }
                }
                return;
            }
        }

        let mode = self.get_mode();
        let mods = self.modifiers.state();

        if key.state == ElementState::Released {
            if !mode.contains(Mode::REPORT_EVENT_TYPES)
                || mode.contains(Mode::VI)
                || self.search_active()
                || self.hint_state.is_active()
            {
                return;
            }

            // Mask `Alt` modifier from input when we won't send esc.
            let text = key.text_with_all_modifiers().unwrap_or_default();
            let mods = if self.alt_send_esc(key, text) {
                mods
            } else {
                mods & !ModifiersState::ALT
            };

            let bytes = match key.logical_key.as_ref() {
                Key::Named(NamedKey::Enter)
                | Key::Named(NamedKey::Tab)
                | Key::Named(NamedKey::Backspace)
                    if !mode.contains(Mode::REPORT_ALL_KEYS_AS_ESC) =>
                {
                    return
                }
                _ => build_key_sequence(key, mods, mode),
            };

            self.ctx_mut().current_mut().messenger.send_write(bytes);

            return;
        }

        // All key bindings are disabled while a hint is being selected (like Alacritty)
        if self.hint_state.is_active() {
            // Handle special keys first
            match key.logical_key {
                rio_window::keyboard::Key::Named(
                    rio_window::keyboard::NamedKey::Escape,
                ) => {
                    self.hint_state.stop();
                    self.update_hint_state();
                    self.mark_dirty();
                    return;
                }
                rio_window::keyboard::Key::Named(
                    rio_window::keyboard::NamedKey::Backspace,
                ) => {
                    let terminal = self.context_manager.current().terminal.lock();
                    self.hint_state.keyboard_input(&*terminal, '\x08');
                    drop(terminal);
                    self.update_hint_state();
                    self.mark_dirty();
                    return;
                }
                _ => {}
            }

            // Handle text input
            let text = key.text_with_all_modifiers().unwrap_or_default();
            for character in text.chars() {
                let terminal = self.context_manager.current().terminal.lock();
                if let Some(hint_match) =
                    self.hint_state.keyboard_input(&*terminal, character)
                {
                    drop(terminal);
                    self.execute_hint_action(&hint_match, clipboard);
                    // Stop hint mode and update state with proper damage tracking
                    self.hint_state.stop();
                    self.update_hint_state();
                    self.mark_dirty();
                    return;
                }
                drop(terminal);
            }
            self.update_hint_state();
            self.mark_dirty();
            return;
        }

        let ignore_chars = self.process_key_bindings(key, &mode, mods, clipboard);
        if ignore_chars {
            return;
        }

        let text = key.text_with_all_modifiers().unwrap_or_default();

        if self.search_active() {
            for character in text.chars() {
                self.search_input(character);
            }

            self.mark_dirty();
            return;
        }

        // Vi mode on its own doesn't have any input, the search input was done before.
        if mode.contains(Mode::VI) {
            return;
        }

        // Mask `Alt` modifier from input when we won't send esc.
        let mods = if self.alt_send_esc(key, text) {
            mods
        } else {
            mods & !ModifiersState::ALT
        };

        let build_key_sequence = Self::should_build_sequence(key, text, mode, mods);

        // Legacy ctrl encoding runs before trusting the platform text:
        // the OS is inconsistent about synthesizing C0 characters for
        // combos like ctrl+6 or ctrl+/ (macOS reports the plain char,
        // Windows reports nothing), so the byte is computed from the
        // kitty C0 table directly. Gated on the exact flag set that
        // makes `build_key_sequence` produce CSI u (`kitty_seq`), so
        // kitty-protocol encoding is untouched in every mode where it
        // applies.
        let kitty_seq = mode.intersects(
            Mode::REPORT_ALL_KEYS_AS_ESC
                | Mode::DISAMBIGUATE_ESC_CODES
                | Mode::REPORT_EVENT_TYPES,
        );
        let ctrl_c0 = if kitty_seq {
            None
        } else {
            crate::bindings::ctrl_seq(&key.logical_key, text, mods)
        };

        let bytes = if let Some(c0) = ctrl_c0 {
            if mods.alt_key() {
                vec![b'\x1b', c0]
            } else {
                vec![c0]
            }
        } else if build_key_sequence {
            crate::bindings::kitty_keyboard::build_key_sequence(key, mods, mode)
        } else {
            let mut bytes = Vec::with_capacity(text.len() + 1);
            if mods.alt_key() {
                bytes.push(b'\x1b');
            }

            bytes.extend_from_slice(text.as_bytes());
            bytes
        };

        if !bytes.is_empty() {
            self.scroll_bottom_when_cursor_not_visible();
            self.clear_selection();

            self.ctx_mut().current_mut().messenger.send_write(bytes);
        }
    }

    /// Check whether we should try to build escape sequence for the [`KeyEvent`].
    pub(super) fn should_build_sequence(
        key: &rio_window::event::KeyEvent,
        text: &str,
        mode: Mode,
        mods: ModifiersState,
    ) -> bool {
        if mode.contains(Mode::REPORT_ALL_KEYS_AS_ESC) {
            return true;
        }

        let disambiguate = mode.contains(Mode::DISAMBIGUATE_ESC_CODES)
            && (key.logical_key == Key::Named(NamedKey::Escape)
                || key.location == KeyLocation::Numpad
                || (!mods.is_empty()
                    && (mods != ModifiersState::SHIFT
                        || matches!(
                            key.logical_key,
                            Key::Named(NamedKey::Tab)
                                | Key::Named(NamedKey::Enter)
                                | Key::Named(NamedKey::Backspace)
                        ))));

        match key.logical_key {
            _ if disambiguate => true,
            // Exclude all the named keys unless they have textual representation.
            Key::Named(named) => named.to_text().is_none(),
            _ => text.is_empty(),
        }
    }

    #[inline]
    pub fn process_mouse_bindings(
        &mut self,
        button: MouseButton,
        clipboard: &mut Clipboard,
    ) {
        let mode = self.get_mode();
        let binding_mode = BindingMode::new(&mode, self.search_active());
        let mouse_mode = self.mouse_mode();
        let mods = self.modifiers.state();

        for i in 0..self.mouse_bindings.len() {
            let mut binding = self.mouse_bindings[i].clone();

            // Require shift for all modifiers when mouse mode is active.
            if mouse_mode {
                binding.mods |= ModifiersState::SHIFT;
            }

            if binding.is_triggered_by(binding_mode.to_owned(), mods, &button)
                && binding.action == Act::PasteSelection
            {
                let content = clipboard.get(ClipboardType::Selection);
                self.paste(&content, true);
            }
        }
    }

    pub fn process_key_bindings(
        &mut self,
        key: &rio_window::event::KeyEvent,
        mode: &Mode,
        mods: ModifiersState,
        clipboard: &mut Clipboard,
    ) -> bool {
        let search_active = self.search_active();
        let binding_mode = BindingMode::new(mode, search_active);
        let mut ignore_chars = None;

        for i in 0..self.bindings.len() {
            let binding = &self.bindings[i];
            let trigger = &binding.trigger;
            let action = binding.action.clone();

            // We don't want the key without modifier, because it means something else most of
            // the time. However what we want is to manually lowercase the character to account
            // for both small and capital letters on regular characters at the same time.
            let logical_key = if let Key::Character(ch) = key.logical_key.as_ref() {
                // Match `Alt` bindings without `Alt` being applied, otherwise they use the
                // composed chars, which are not intuitive to bind.
                //
                // On Windows, the `Ctrl + Alt` mangles `logical_key` to unidentified values, thus
                // preventing them from being used in bindings
                //
                // For more see https://github.com/rust-windowing/winit/issues/2945.
                // if (cfg!(target_os = "macos") || (cfg!(windows) && mods.control_key()))
                // && mods.alt_key()
                if (mods.shift_key() || mods.alt_key())
                    || mods.alt_key() && (cfg!(windows) && mods.control_key())
                {
                    key.key_without_modifiers()
                } else {
                    Key::Character(ch.to_lowercase().into())
                }
            } else {
                key.logical_key.clone()
            };

            let key_match = match (&trigger, logical_key) {
                (BindingKey::Scancode(_), _) => BindingKey::Scancode(key.physical_key),
                (_, code) => BindingKey::Keycode {
                    key: code,
                    location: key.location,
                },
            };

            if binding.is_triggered_by(binding_mode.to_owned(), mods, &key_match) {
                *ignore_chars.get_or_insert(true) &= action != Act::ReceiveChar;

                match &action {
                    Act::Run(program) => self.exec(program.program(), program.args()),
                    Act::Esc(s) => {
                        self.paste(s, false);
                    }
                    Act::Paste => {
                        let content = clipboard.get(ClipboardType::Clipboard);
                        self.paste(&content, true);
                    }
                    Act::ClearSelection => {
                        self.clear_selection();
                    }
                    Act::PasteSelection => {
                        let content = clipboard.get(ClipboardType::Selection);
                        self.paste(&content, true);
                    }
                    Act::Copy => {
                        self.copy_selection(ClipboardType::Clipboard, clipboard);
                    }
                    Act::SelectAll => {
                        self.select_all();
                    }
                    Act::Hint(hint_config) => {
                        self.start_hint_mode(hint_config.clone());
                    }
                    Act::SearchForward => {
                        self.start_search(Direction::Right);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::SearchBackward => {
                        self.start_search(Direction::Left);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchConfirm) => {
                        self.confirm_search(clipboard);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchCancel) => {
                        self.cancel_search(clipboard);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchClear) => {
                        let direction = self.search_state.direction;
                        self.cancel_search(clipboard);
                        self.start_search(direction);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchFocusNext) => {
                        self.advance_search_origin(self.search_state.direction);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchFocusPrevious) => {
                        let direction = self.search_state.direction.opposite();
                        self.advance_search_origin(direction);
                        self.resize_top_or_bottom_line(self.ctx().len());
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchDeleteWord) => {
                        self.search_pop_word();
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchHistoryPrevious) => {
                        self.search_history_previous();
                        self.mark_dirty();
                    }
                    Act::Search(SearchAction::SearchHistoryNext) => {
                        self.search_history_next();
                        self.mark_dirty();
                    }
                    Act::ToggleViMode => {
                        let context = self.context_manager.current_mut();
                        let mut terminal = context.terminal.lock();
                        terminal.toggle_vi_mode();
                        let has_vi_mode_enabled = terminal.mode().contains(Mode::VI);
                        drop(terminal);
                        context
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.renderer.set_vi_mode(has_vi_mode_enabled);
                        self.mark_dirty();
                    }
                    Act::ViMotion(motion) => {
                        let context = self.context_manager.current_mut();
                        let mut terminal = context.terminal.lock();
                        if terminal.mode().contains(Mode::VI) {
                            terminal.vi_motion(*motion);
                        }

                        if let Some(selection) = &terminal.selection {
                            context.renderable_content.selection_range =
                                selection.to_range(&terminal);
                        };
                        drop(terminal);
                        context
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::CenterAroundViCursor) => {
                        let context = self.context_manager.current_mut();
                        let mut terminal = context.terminal.lock();
                        let display_offset = terminal.display_offset() as i32;
                        let target =
                            -display_offset + terminal.grid.screen_lines() as i32 / 2 - 1;
                        let line = terminal.vi_mode_cursor.pos.row;
                        let scroll_lines = target - line.0;

                        terminal.scroll_display(Scroll::Delta(scroll_lines));
                        drop(terminal);
                        context
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::ToggleNormalSelection) => {
                        self.toggle_selection(
                            SelectionType::Simple,
                            Side::Left,
                            clipboard,
                        );
                        self.context_manager
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::ToggleLineSelection) => {
                        self.toggle_selection(
                            SelectionType::Lines,
                            Side::Left,
                            clipboard,
                        );
                        self.context_manager
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::ToggleBlockSelection) => {
                        self.toggle_selection(
                            SelectionType::Block,
                            Side::Left,
                            clipboard,
                        );
                        self.context_manager
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::Vi(ViAction::ToggleSemanticSelection) => {
                        self.toggle_selection(
                            SelectionType::Semantic,
                            Side::Left,
                            clipboard,
                        );
                        self.context_manager
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Full,
                            );
                        self.mark_dirty();
                    }
                    Act::SplitRight => {
                        self.split_right();
                    }
                    Act::SplitDown => {
                        self.split_down();
                    }
                    Act::MoveDividerUp => {
                        // User wants divider to move up visually, which means expanding the bottom split
                        self.move_divider_down();
                    }
                    Act::MoveDividerDown => {
                        // User wants divider to move down visually, which means expanding the top split
                        self.move_divider_up();
                    }
                    Act::MoveDividerLeft => {
                        self.move_divider_left();
                    }
                    Act::MoveDividerRight => {
                        self.move_divider_right();
                    }
                    Act::ConfigEditor => {
                        self.context_manager.switch_to_settings();
                    }
                    Act::WindowCreateNew => {
                        self.context_manager.create_new_window();
                    }
                    Act::ToggleQuake => {
                        self.context_manager.toggle_quake();
                    }
                    Act::CloseCurrentSplitOrTab => {
                        self.close_split_or_tab(clipboard);
                    }
                    Act::TabCreateNew => {
                        self.create_tab(clipboard);
                    }
                    Act::TabCloseCurrent => {
                        self.close_tab(clipboard);
                    }
                    Act::TabCloseUnfocused => {
                        self.clear_selection();
                        self.cancel_search(clipboard);
                        if self.ctx().len() <= 1 {
                            return true;
                        }
                        self.context_manager
                            .close_unfocused_tabs(&mut self.sugarloaf);
                        if let Some(ref mut island) = self.renderer.island {
                            island.dismiss_color_picker();
                        }
                        self.resize_top_or_bottom_line(1);
                        self.mark_dirty();
                    }
                    Act::Quit => {
                        self.context_manager.quit();
                    }
                    Act::IncreaseFontSize => {
                        self.change_font_size(FontSizeAction::Increase);
                    }
                    Act::DecreaseFontSize => {
                        self.change_font_size(FontSizeAction::Decrease);
                    }
                    Act::ResetFontSize => {
                        self.change_font_size(FontSizeAction::Reset);
                    }
                    Act::ScrollToPrevPrompt => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_to_prompt(false);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollToNextPrompt => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_to_prompt(true);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollPageUp => {
                        // Move vi mode cursor.
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        let scroll_lines = terminal.grid.screen_lines() as i32;
                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);
                        terminal.scroll_display(Scroll::PageUp);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollPageDown => {
                        // Move vi mode cursor.
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        let scroll_lines = -(terminal.grid.screen_lines() as i32);

                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);

                        terminal.scroll_display(Scroll::PageDown);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollHalfPageUp => {
                        // Move vi mode cursor.
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        let scroll_lines = terminal.grid.screen_lines() as i32 / 2;

                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);

                        terminal.scroll_display(Scroll::Delta(scroll_lines));
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollHalfPageDown => {
                        // Move vi mode cursor.
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        let scroll_lines = -(terminal.grid.screen_lines() as i32 / 2);

                        terminal.vi_mode_cursor =
                            terminal.vi_mode_cursor.scroll(&terminal, scroll_lines);

                        terminal.scroll_display(Scroll::Delta(scroll_lines));
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollToTop => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_display(Scroll::Top);

                        let topmost_line = terminal.grid.topmost_line();
                        terminal.vi_mode_cursor.pos.row = topmost_line;
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ScrollToBottom => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_display(Scroll::Bottom);

                        // Move vi mode cursor.
                        terminal.vi_mode_cursor.pos.row = terminal.grid.bottommost_line();

                        // Move to beginning twice, to always jump across linewraps.
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        terminal.vi_motion(ViMotion::FirstOccupied);
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::Scroll(delta) => {
                        let current = self.context_manager.current_mut();
                        let rtid = current.rich_text_id;
                        let mut terminal = current.terminal.lock();
                        terminal.scroll_display(Scroll::Delta(*delta));
                        drop(terminal);
                        self.renderer.scrollbar.notify_scroll(rtid);
                        self.mark_dirty();
                    }
                    Act::ClearHistory => {
                        let mut terminal =
                            self.context_manager.current_mut().terminal.lock();
                        terminal.clear_saved_history();
                        drop(terminal);
                        self.mark_dirty();
                    }
                    Act::ToggleFullscreen => self.context_manager.toggle_full_screen(),
                    Act::ToggleAppearanceTheme => {
                        self.context_manager.toggle_appearance_theme();
                    }
                    Act::OpenCommandPalette => {
                        // One-way "open": the action never closes an
                        // already-visible palette. Users close it via
                        // Esc (handled inside the palette's own key
                        // dispatcher in `router::mod`). Idempotent —
                        // re-firing while the palette is already open
                        // must NOT wipe the user's in-progress query.
                        if !self.renderer.command_palette.is_enabled() {
                            let hosts = self.palette_host_items();
                            self.renderer.command_palette.set_hosts(hosts);
                            self.renderer.command_palette.set_enabled(true);
                            self.mark_dirty();
                        }
                    }
                    Act::Minimize => {
                        self.context_manager.minimize();
                    }
                    Act::Hide => {
                        self.context_manager.hide();
                    }
                    #[cfg(target_os = "macos")]
                    Act::HideOtherApplications => {
                        self.context_manager.hide_other_apps();
                    }
                    Act::SelectNextSplit => {
                        self.cancel_search(clipboard);
                        self.context_manager.select_next_split();
                        self.mark_dirty();
                    }
                    Act::SelectPrevSplit => {
                        self.cancel_search(clipboard);
                        self.context_manager.select_prev_split();
                        self.mark_dirty();
                    }
                    Act::SelectNextSplitOrTab => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.switch_to_next_split_or_tab();
                        let new_index = self.context_manager.current_index();
                        self.switch_visible_context(old_index, new_index);
                        self.mark_dirty();
                    }
                    Act::SelectPrevSplitOrTab => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.switch_to_prev_split_or_tab();
                        let new_index = self.context_manager.current_index();
                        self.switch_visible_context(old_index, new_index);
                        self.mark_dirty();
                    }
                    Act::SelectTab(tab_index) => {
                        let old_index = self.context_manager.current_index();
                        self.context_manager.select_tab(*tab_index);
                        let new_index = self.context_manager.current_index();
                        self.switch_visible_context(old_index, new_index);
                        self.cancel_search(clipboard);
                        self.mark_dirty();
                    }
                    Act::SelectLastTab => {
                        self.cancel_search(clipboard);
                        let old_index = self.context_manager.current_index();
                        self.context_manager.select_last_tab();
                        let new_index = self.context_manager.current_index();
                        self.switch_visible_context(old_index, new_index);
                        self.mark_dirty();
                    }
                    Act::SelectNextTab => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.switch_to_next();
                        let new_index = self.context_manager.current_index();
                        self.switch_visible_context(old_index, new_index);
                        self.mark_dirty();
                    }
                    Act::MoveCurrentTabToPrev => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.move_current_to_prev();
                        let new_index = self.context_manager.current_index();
                        self.switch_visible_context(old_index, new_index);
                        let layout = self.island_tab_layout(self.context_manager.len());
                        let tab_width =
                            layout.width_at(old_index).max(layout.width_at(new_index));
                        if let Some(ref mut island) = self.renderer.island {
                            island.remap_tab_swap(old_index, new_index, tab_width);
                        }
                        self.mark_dirty();
                    }
                    Act::MoveCurrentTabToNext => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.move_current_to_next();
                        let new_index = self.context_manager.current_index();
                        self.switch_visible_context(old_index, new_index);
                        let layout = self.island_tab_layout(self.context_manager.len());
                        let tab_width =
                            layout.width_at(old_index).max(layout.width_at(new_index));
                        if let Some(ref mut island) = self.renderer.island {
                            island.remap_tab_swap(old_index, new_index, tab_width);
                        }
                        self.mark_dirty();
                    }
                    Act::SelectPrevTab => {
                        self.cancel_search(clipboard);
                        self.clear_selection();
                        let old_index = self.context_manager.current_index();
                        self.context_manager.switch_to_prev();
                        let new_index = self.context_manager.current_index();
                        self.switch_visible_context(old_index, new_index);
                        self.mark_dirty();
                    }
                    Act::ReceiveChar | Act::None => (),
                    _ => (),
                }
            }
        }

        ignore_chars.unwrap_or(false)
    }

    /// Whether we should send `ESC` due to `Alt` being pressed.
    pub(super) fn alt_send_esc(
        &mut self,
        key: &rio_window::event::KeyEvent,
        text: &str,
    ) -> bool {
        #[cfg(not(target_os = "macos"))]
        let alt_send_esc = self.modifiers.state().alt_key();

        #[cfg(target_os = "macos")]
        let alt_send_esc = {
            let option_as_alt = &self.renderer.option_as_alt;
            self.modifiers.state().alt_key()
                && (option_as_alt == "both"
                    || (option_as_alt == "left"
                        && self.modifiers.lalt_state() == ModifiersKeyState::Pressed)
                    || (option_as_alt == "right"
                        && self.modifiers.ralt_state() == ModifiersKeyState::Pressed))
        };

        match key.logical_key {
            Key::Named(named) => {
                if named.to_text().is_some() {
                    alt_send_esc
                } else {
                    // Treat `Alt` as modifier for named keys without text, like ArrowUp.
                    self.modifiers.state().alt_key()
                }
            }
            _ => alt_send_esc && text.chars().count() == 1,
        }
    }
}
