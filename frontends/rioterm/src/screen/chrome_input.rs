//! `Screen` chrome input surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::context;
use crate::hosts;
use rio_window::event::ElementState;
use rio_window::keyboard::{Key, NamedKey};

impl Screen<'_> {
    /// Route a key to the add-host editor. `None` when it is closed.
    ///
    /// Committed IME text never arrives here — it has no key event; the
    /// router forwards it through `Chrome::handle_form_input` directly.
    pub fn chrome_key_input(
        &mut self,
        key_event: &rio_window::event::KeyEvent,
    ) -> Option<terminus_ui::add_host::FormOutcome> {
        use rio_window::event::ElementState;
        use rio_window::keyboard::{Key, NamedKey};
        use terminus_ui::add_host::{FormInput, FormOutcome};

        // Context menu: Escape dismisses without reaching the terminal.
        if self.chrome.context_menu.is_some() {
            if key_event.state != ElementState::Pressed {
                return Some(FormOutcome::Consumed);
            }
            if matches!(key_event.logical_key, Key::Named(NamedKey::Escape)) {
                self.chrome.close_context_menu();
            }
            return Some(FormOutcome::Consumed);
        }

        // Inline rename from the context menu.
        if self.chrome.panel.rename.is_some() && !self.chrome.add_host_is_open() {
            if key_event.state != ElementState::Pressed {
                return Some(FormOutcome::Consumed);
            }
            let mods = self.modifiers.state();
            let shift = mods.shift_key();
            // Windows/Linux: Ctrl+Arrow = word. macOS: Option/Alt = word.
            let word = mods.control_key() || mods.alt_key();
            let select_mod = mods.control_key() || mods.super_key();
            let move_kind = if shift {
                terminus_ui::sidebar::RenameMoveKind::Extend
            } else {
                terminus_ui::sidebar::RenameMoveKind::Collapse
            };
            match &key_event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.chrome.panel.cancel_rename();
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(draft) = self.chrome.panel.rename.clone() {
                        let name = draft.name.trim().to_string();
                        if !name.is_empty() {
                            if draft.is_group {
                                self.host_store.rename_group(&draft.id, &name);
                            } else {
                                self.host_store.rename_host(&draft.id, &name);
                            }
                        }
                        self.chrome.panel.cancel_rename();
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::Backspace) => {
                    if let Some(draft) = self.chrome.panel.rename.as_mut() {
                        draft.backspace(word);
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::Delete) => {
                    if let Some(draft) = self.chrome.panel.rename.as_mut() {
                        draft.delete_forward(word);
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    if let Some(draft) = self.chrome.panel.rename.as_mut() {
                        draft.move_left(move_kind, word);
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::ArrowRight) => {
                    if let Some(draft) = self.chrome.panel.rename.as_mut() {
                        draft.move_right(move_kind, word);
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::Home) => {
                    if let Some(draft) = self.chrome.panel.rename.as_mut() {
                        draft.move_home(move_kind);
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::End) => {
                    if let Some(draft) = self.chrome.panel.rename.as_mut() {
                        draft.move_end(move_kind);
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::Space) => {
                    if let Some(draft) = self.chrome.panel.rename.as_mut() {
                        draft.insert(" ", 64);
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Character(ch) => {
                    // Ctrl/Cmd+A select-all (layout-independent via character).
                    if select_mod && ch.eq_ignore_ascii_case("a") {
                        if let Some(draft) = self.chrome.panel.rename.as_mut() {
                            draft.select_all();
                        }
                        return Some(FormOutcome::Consumed);
                    }
                    if select_mod {
                        // Leave other Ctrl/Cmd chords alone (no insert of "c"/"v").
                        return Some(FormOutcome::Consumed);
                    }
                    let text = key_event.text.as_deref().unwrap_or(ch.as_str());
                    if crate::renderer::is_printable_text(text) {
                        if let Some(draft) = self.chrome.panel.rename.as_mut() {
                            draft.insert(text, 64);
                        }
                    }
                    return Some(FormOutcome::Consumed);
                }
                _ => {
                    if let Some(text) = key_event.text.as_ref() {
                        if !select_mod && crate::renderer::is_printable_text(text) {
                            if let Some(draft) = self.chrome.panel.rename.as_mut() {
                                draft.insert(text, 64);
                            }
                        }
                    }
                    return Some(FormOutcome::Consumed);
                }
            }
        }

        // Host-list search field: type to filter, Esc clears focus.
        if self.chrome.panel.filter_focused && !self.chrome.add_host_is_open() {
            if key_event.state != ElementState::Pressed {
                return Some(FormOutcome::Consumed);
            }
            match &key_event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.chrome.panel.filter_focused = false;
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::Backspace) => {
                    self.chrome.panel.filter.pop();
                    return Some(FormOutcome::Consumed);
                }
                Key::Character(ch) => {
                    let text = key_event.text.as_deref().unwrap_or(ch.as_str());
                    if crate::renderer::is_printable_text(text) {
                        self.chrome.panel.filter.push_str(text);
                    }
                    return Some(FormOutcome::Consumed);
                }
                _ => return Some(FormOutcome::Consumed),
            }
        }

        // Inline new-group name field.
        if self.chrome.panel.new_group_focused && !self.chrome.add_host_is_open() {
            if key_event.state != ElementState::Pressed {
                return Some(FormOutcome::Consumed);
            }
            match &key_event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.chrome.panel.close_new_group_form();
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::Enter) => {
                    let name = self.chrome.panel.new_group_name.trim().to_string();
                    if !name.is_empty() {
                        self.host_store.create_group(&name);
                        self.chrome.panel.close_new_group_form();
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Named(NamedKey::Backspace) => {
                    self.chrome.panel.new_group_name.pop();
                    return Some(FormOutcome::Consumed);
                }
                Key::Character(ch) => {
                    let text = key_event.text.as_deref().unwrap_or(ch.as_str());
                    if crate::renderer::is_printable_text(text)
                        && self.chrome.panel.new_group_name.len() < 32
                    {
                        self.chrome.panel.new_group_name.push_str(text);
                    }
                    return Some(FormOutcome::Consumed);
                }
                _ => return Some(FormOutcome::Consumed),
            }
        }

        if !self.chrome.add_host_is_open() {
            return None;
        }
        if key_event.state != ElementState::Pressed {
            // Swallow releases as well: a key held while the editor
            // opens must not leak its release to the shell.
            return Some(FormOutcome::Consumed);
        }

        let input = match &key_event.logical_key {
            Key::Named(NamedKey::Backspace) => FormInput::Backspace,
            Key::Named(NamedKey::Delete) => FormInput::Delete,
            Key::Named(NamedKey::Enter) => FormInput::Enter,
            Key::Named(NamedKey::Escape) => FormInput::Escape,
            Key::Named(NamedKey::Tab) => {
                if self.modifiers.state().shift_key() {
                    FormInput::Previous
                } else {
                    FormInput::Next
                }
            }
            Key::Named(NamedKey::ArrowLeft) => FormInput::Left,
            Key::Named(NamedKey::ArrowRight) => FormInput::Right,
            Key::Named(NamedKey::ArrowUp) => FormInput::Previous,
            Key::Named(NamedKey::ArrowDown) => FormInput::Next,
            Key::Named(NamedKey::Home) => FormInput::Home,
            Key::Named(NamedKey::End) => FormInput::End,
            Key::Character(ch) => {
                let select_mod = self.modifiers.state().control_key()
                    || self.modifiers.state().super_key();
                if select_mod {
                    // Ctrl/Cmd chords (paste is handled by Modal::HostEditor).
                    // Do not insert a literal "v" / "c" / "a".
                    return Some(FormOutcome::Consumed);
                }
                let _ = ch;
                FormInput::Text
            }
            // Every other key is consumed and ignored: the editor is a
            // text sink, so nothing may reach the PTY behind it.
            _ => return Some(FormOutcome::Consumed),
        };

        let text = if input == FormInput::Text {
            key_event.text.as_deref().unwrap_or_default()
        } else {
            ""
        };
        self.chrome.handle_form_input(input, text)
    }

    pub fn chrome_snippet_key_input(
        &mut self,
        key_event: &rio_window::event::KeyEvent,
    ) -> Option<terminus_ui::add_snippet::FormOutcome> {
        use rio_window::event::ElementState;
        use rio_window::keyboard::{Key, NamedKey};
        use terminus_ui::add_snippet::{FormInput, FormOutcome};
        use terminus_ui::TextMoveKind;

        if key_event.state != ElementState::Pressed {
            return None;
        }

        let mods = self.modifiers.state();
        let shift = mods.shift_key();
        // Windows/Linux: Ctrl+Arrow = word. macOS: Option/Alt = word.
        let by_word = mods.control_key() || mods.alt_key();
        let select_mod = mods.control_key() || mods.super_key();
        let move_kind = if shift {
            TextMoveKind::Extend
        } else {
            TextMoveKind::Collapse
        };

        let input = match &key_event.logical_key {
            Key::Named(NamedKey::Backspace) => FormInput::Backspace { by_word },
            Key::Named(NamedKey::Delete) => FormInput::Delete { by_word },
            Key::Named(NamedKey::Tab) => {
                if shift {
                    FormInput::Previous
                } else {
                    FormInput::Next
                }
            }
            Key::Named(NamedKey::ArrowLeft) => FormInput::Left {
                kind: move_kind,
                by_word,
            },
            Key::Named(NamedKey::ArrowRight) => FormInput::Right {
                kind: move_kind,
                by_word,
            },
            Key::Named(NamedKey::Home) => FormInput::Home { kind: move_kind },
            Key::Named(NamedKey::End) => FormInput::End { kind: move_kind },
            Key::Named(NamedKey::ArrowDown) => FormInput::Next,
            Key::Named(NamedKey::ArrowUp) => FormInput::Previous,
            Key::Named(NamedKey::Enter) => FormInput::Enter,
            Key::Named(NamedKey::Escape) => FormInput::Escape,
            Key::Named(NamedKey::Space) => FormInput::Text,
            Key::Character(ch) => {
                if select_mod && ch.eq_ignore_ascii_case("a") {
                    FormInput::SelectAll
                } else if select_mod {
                    // Leave other Ctrl/Cmd chords alone (no insert of "c"/"v").
                    return Some(FormOutcome::Ignored);
                } else if ch.chars().all(|c| c.is_ascii_control()) {
                    return None;
                } else {
                    FormInput::Text
                }
            }
            _ => return Some(FormOutcome::Ignored),
        };

        let text = if input == FormInput::Text {
            match &key_event.logical_key {
                Key::Named(NamedKey::Space) => " ",
                _ => key_event.text.as_deref().unwrap_or_default(),
            }
        } else {
            ""
        };

        self.chrome.handle_snippet_form_input(input, text)
    }

    pub fn chrome_snippet_commit_text(&mut self, text: &str) -> bool {
        if !self.chrome.add_snippet_is_open() {
            return false;
        }
        matches!(
            self.chrome.handle_snippet_form_input(
                terminus_ui::add_snippet::FormInput::Text,
                text
            ),
            Some(terminus_ui::add_snippet::FormOutcome::Changed)
        )
    }

    pub fn submit_snippet_form(&mut self) {
        let values = self.chrome.snippet_form.values();
        self.host_store
            .create_snippet(terminus_ui::snippets::SnippetItem {
                id: "".to_string(), // new UUID generated in host thread
                name: values.name,
                cmd: values.command,
                desc: values.description,
            });
        self.chrome.snippet_form.inner.closing = true;
    }

    pub fn chrome_commit_text(&mut self, text: &str) -> bool {
        if !self.chrome.add_host_is_open() {
            return false;
        }
        self.chrome
            .handle_form_input(terminus_ui::add_host::FormInput::Text, text);
        true
    }

    /// Keyboard for the vault unlock prompt. Returns whether Unlock should run.
    pub fn chrome_vault_unlock_key(
        &mut self,
        key_event: &rio_window::event::KeyEvent,
    ) -> bool {
        use rio_window::event::ElementState;
        use rio_window::keyboard::{Key, NamedKey};
        use terminus_ui::add_host::FormInput;

        if key_event.state != ElementState::Pressed {
            return false;
        }
        let input = match &key_event.logical_key {
            Key::Named(NamedKey::Enter) => FormInput::Enter,
            Key::Named(NamedKey::Escape) => FormInput::Escape,
            Key::Named(NamedKey::Backspace) => FormInput::Backspace,
            _ => {
                let text = key_event.text.as_deref().unwrap_or("");
                if text.is_empty() {
                    return false;
                }
                FormInput::Text
            }
        };
        let text = if input == FormInput::Text {
            key_event.text.as_deref().unwrap_or_default()
        } else {
            ""
        };
        self.chrome
            .handle_vault_unlock_input(input, text)
            .unwrap_or(false)
    }

    /// IME commit into the vault unlock passphrase field.
    pub fn chrome_vault_unlock_commit_text(&mut self, text: &str) -> bool {
        self.chrome
            .handle_vault_unlock_input(terminus_ui::add_host::FormInput::Text, text)
            .is_some()
    }

    /// Submit the vault unlock passphrase from the prompt.
    pub fn submit_vault_unlock(&mut self) {
        let passphrase = self.chrome.vault_unlock.passphrase().to_string();
        if passphrase.trim().is_empty() {
            self.chrome
                .vault_unlock
                .set_error("Enter your vault passphrase");
            return;
        }
        self.chrome.vault_unlock.set_unlocking();
        self.host_store
            .unlock_vault_remember(&passphrase, self.chrome.vault_unlock.remember());
    }

    /// Persist the editor's fields.
    ///
    /// The store is the only validator, so a rejection comes back as the
    /// dialog's error line and the form stays open with its text.
    pub fn submit_host_form(&mut self) {
        let values = self.chrome.form.values();
        let editing_id = self.chrome.form.editing_id().map(str::to_string);
        let draft = crate::hosts::HostDraft {
            id: editing_id.clone(),
            name: values.name,
            hostname: values.hostname,
            username: values.username,
            port: values.port,
            auth_method: values.auth_method,
            identity_id: values.identity_id,
            password: values.password,
        };
        let result = if let Some(id) = editing_id.as_deref() {
            self.host_store.probe_and_update(id, &draft)
        } else {
            self.host_store.probe_and_create(&draft)
        };
        match result {
            Ok(()) => {
                // Stay open until the worker reports Stored or Failed.
                self.pending_host_select = draft.normalize().ok().map(|d| d.name);
                self.chrome.panel.error = None;
                self.chrome.form.set_error(if editing_id.is_some() {
                    "Saving…"
                } else {
                    "Connecting…"
                });
            }
            Err(message) => {
                self.chrome.form.set_error(message);
            }
        }
    }
}
