//! `Screen` chrome input surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::context;
use crate::hosts;
use rio_window::event::ElementState;
use rio_window::keyboard::{Key, NamedKey};

/// The one winit-key → [`TextEdit`] mapping for text fields: Backspace,
/// Delete, caret moves (Shift extends, Ctrl/Alt jumps by word) and
/// Ctrl/Cmd+A. Every field routes keys through this so they cannot drift.
pub(crate) fn text_edit_for_key(
    key: &Key,
    mods: rio_window::keyboard::ModifiersState,
) -> Option<terminus_ui::TextEdit> {
    use terminus_ui::{TextEdit, TextMoveKind};
    let by_word = mods.control_key() || mods.alt_key();
    let kind = if mods.shift_key() {
        TextMoveKind::Extend
    } else {
        TextMoveKind::Collapse
    };
    Some(match key {
        Key::Named(NamedKey::Backspace) => TextEdit::Backspace { by_word },
        Key::Named(NamedKey::Delete) => TextEdit::Delete { by_word },
        Key::Named(NamedKey::ArrowLeft) => TextEdit::Left { kind, by_word },
        Key::Named(NamedKey::ArrowRight) => TextEdit::Right { kind, by_word },
        Key::Named(NamedKey::Home) => TextEdit::Home { kind },
        Key::Named(NamedKey::End) => TextEdit::End { kind },
        Key::Character(ch)
            if (mods.control_key() || mods.super_key())
                && ch.eq_ignore_ascii_case("a") =>
        {
            TextEdit::SelectAll
        }
        _ => return None,
    })
}

impl Screen<'_> {
    fn rename_insert(&mut self, text: &str) {
        if crate::renderer::is_printable_text(text) {
            if let Some(draft) = self.chrome.panel.rename.as_mut() {
                draft.text.insert(text, 64, false);
            }
        }
    }

    /// Type into the session pill rename field.
    pub(crate) fn session_rename_insert(&mut self, text: &str) -> bool {
        if !crate::renderer::is_printable_text(text) {
            return false;
        }
        self.chrome
            .shell
            .rename
            .as_mut()
            .is_some_and(|r| r.text.insert(text, 64, false))
    }

    fn ctrl_or_super(&self) -> bool {
        let mods = self.modifiers.state();
        mods.control_key() || mods.super_key()
    }

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

        // Session pill rename (double-click or its context menu).
        if self.chrome.shell.rename.is_some() && !self.chrome.add_host_is_open() {
            if key_event.state != ElementState::Pressed {
                return Some(FormOutcome::Consumed);
            }
            match &key_event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.chrome.shell.cancel_rename();
                }
                Key::Named(NamedKey::Enter) => self.commit_session_rename(),
                Key::Character(ch) if !self.ctrl_or_super() => {
                    let text = key_event.text.as_deref().unwrap_or(ch.as_str());
                    self.session_rename_insert(text);
                }
                Key::Named(NamedKey::Space) => {
                    self.session_rename_insert(" ");
                }
                other => {
                    if let Some(edit) = text_edit_for_key(other, self.modifiers.state()) {
                        if let Some(r) = self.chrome.shell.rename.as_mut() {
                            r.text.apply(edit);
                        }
                    } else if !self.ctrl_or_super() {
                        if let Some(text) = key_event.text.as_deref() {
                            self.session_rename_insert(text);
                        }
                    }
                }
            }
            return Some(FormOutcome::Consumed);
        }

        // Inline rename from the context menu.
        if self.chrome.panel.rename.is_some() && !self.chrome.add_host_is_open() {
            if key_event.state != ElementState::Pressed {
                return Some(FormOutcome::Consumed);
            }
            match &key_event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.chrome.panel.cancel_rename();
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(draft) = self.chrome.panel.rename.clone() {
                        let name = draft.text.value.trim().to_string();
                        if !name.is_empty() {
                            if draft.is_group {
                                self.host_store.rename_group(&draft.id, &name);
                            } else {
                                self.host_store.rename_host(&draft.id, &name);
                            }
                        }
                        self.chrome.panel.cancel_rename();
                    }
                }
                Key::Character(ch) if !self.ctrl_or_super() => {
                    let text = key_event.text.as_deref().unwrap_or(ch.as_str());
                    self.rename_insert(text);
                }
                Key::Named(NamedKey::Space) => self.rename_insert(" "),
                other => {
                    if let Some(edit) = text_edit_for_key(other, self.modifiers.state()) {
                        if let Some(draft) = self.chrome.panel.rename.as_mut() {
                            draft.text.apply(edit);
                        }
                    } else if !self.ctrl_or_super() {
                        if let Some(text) = key_event.text.as_deref() {
                            self.rename_insert(text);
                        }
                    }
                }
            }
            return Some(FormOutcome::Consumed);
        }

        // Host-list search field: type to filter, Esc clears focus.
        if self.chrome.panel.filter_focused && !self.chrome.add_host_is_open() {
            if key_event.state != ElementState::Pressed {
                return Some(FormOutcome::Consumed);
            }
            match &key_event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.chrome.panel.escape_filter();
                    return Some(FormOutcome::Consumed);
                }
                Key::Character(ch) if !self.ctrl_or_super() => {
                    let text = key_event.text.as_deref().unwrap_or(ch.as_str());
                    if crate::renderer::is_printable_text(text) {
                        self.chrome.panel.filter.insert(text, usize::MAX, false);
                    }
                    return Some(FormOutcome::Consumed);
                }
                other => {
                    if let Some(edit) = text_edit_for_key(other, self.modifiers.state()) {
                        self.chrome.panel.filter.apply(edit);
                    }
                    return Some(FormOutcome::Consumed);
                }
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
                    let name = self.chrome.panel.new_group_name.value.trim().to_string();
                    if !name.is_empty() {
                        self.host_store.create_group(&name);
                        self.chrome.panel.close_new_group_form();
                    }
                    return Some(FormOutcome::Consumed);
                }
                Key::Character(ch) if !self.ctrl_or_super() => {
                    let text = key_event.text.as_deref().unwrap_or(ch.as_str());
                    if crate::renderer::is_printable_text(text) {
                        self.chrome.panel.new_group_name.insert(text, 32, false);
                    }
                    return Some(FormOutcome::Consumed);
                }
                other => {
                    if let Some(edit) = text_edit_for_key(other, self.modifiers.state()) {
                        self.chrome.panel.new_group_name.apply(edit);
                    }
                    return Some(FormOutcome::Consumed);
                }
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

        // Text fields edit through the shared TextDraft router (word jumps,
        // selection, Delete…); selects keep the arrow-cycling path below.
        if self.chrome.form.focused_field().is_text() {
            if let Some(edit) =
                text_edit_for_key(&key_event.logical_key, self.modifiers.state())
            {
                self.chrome.form.edit(edit);
                return Some(FormOutcome::Consumed);
            }
        }

        let input = match &key_event.logical_key {
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
            // winit reports Space as a named key: it is text here (names,
            // tags, notes, passwords may hold spaces).
            Key::Named(NamedKey::Space) => FormInput::Text,
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
            match &key_event.logical_key {
                Key::Named(NamedKey::Space) => " ",
                _ => key_event.text.as_deref().unwrap_or_default(),
            }
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
        if let Some(edit) =
            text_edit_for_key(&key_event.logical_key, self.modifiers.state())
        {
            self.chrome.edit_vault_unlock(edit);
            return false;
        }
        let input = match &key_event.logical_key {
            Key::Named(NamedKey::Enter) => FormInput::Enter,
            Key::Named(NamedKey::Escape) => FormInput::Escape,
            Key::Named(NamedKey::Tab) => FormInput::Next,
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
        // Creating a vault asks twice: a typo would otherwise lock every
        // secret saved afterwards behind a passphrase nobody knows.
        let passphrase = match self.chrome.vault_unlock.validate() {
            Ok(passphrase) => passphrase,
            Err(message) => {
                self.chrome.vault_unlock.set_error(message);
                return;
            }
        };
        self.chrome.vault_unlock.set_unlocking();
        self.host_store
            .unlock_vault_remember(&passphrase, self.chrome.vault_unlock.remember());
    }

    /// Persist the editor's fields.
    ///
    /// The store is the only validator, so a rejection comes back as the
    /// dialog's error line and the form stays open with its text.
    /// Settings → Unlock Vault: unlock with the Settings passphrase field, or,
    /// with no vault yet, open the create prompt (passphrase asked twice).
    pub fn settings_unlock_vault(&mut self) {
        if !self.host_store.vault_configured() {
            self.open_vault_unlock_for(terminus_ui::PendingVaultAction::CreateVault);
            return;
        }
        let passphrase = self.chrome.settings.sql_passphrase.value.clone();
        self.host_store.unlock_vault(&passphrase);
    }

    /// Send the Settings "New SSH Key" draft to the worker (generate or import).
    pub fn submit_key_draft(&mut self) {
        let settings = &mut self.chrome.settings;
        let Ok(name) = settings.take_key_draft_label() else {
            return;
        };
        let pem = settings.key_pem.value.clone();
        let pem = (!pem.trim().is_empty()).then_some(pem);
        let passphrase = settings.key_draft_passphrase();
        self.host_store.import_ssh_key(&name, pem, passphrase);
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
            group_id: values.group_id,
            tags: crate::hosts::parse_tags(&values.tags),
            notes: values.notes,
        };
        let result = if let Some(id) = editing_id.as_deref() {
            self.host_store.probe_and_update(id, &draft)
        } else {
            self.host_store.probe_and_create(&draft)
        };
        match result {
            Ok(()) => {
                // Stay open until the worker reports Stored or Failed.
                self.pending_host_select = draft
                    .normalize()
                    .ok()
                    .map(|d| (d.name.clone(), d.endpoint()));
                self.pending_host_connect = editing_id.is_none();
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
