//! Copy / Cut / Paste for every text field outside the terminal.
//!
//! Plain Ctrl/Cmd+C, X and V (and Ctrl+Shift+C / V, the terminal's chord)
//! act on the focused field of whatever overlay, drawer, view or SFTP editor
//! owns the keyboard. With the terminal in front nothing here applies: the
//! chords reach the usual bindings and `^C` / `^V` still go to the shell.

use super::{Modal, Route};
use crate::screen::clipboard::read_clipboard_text;
use rio_backend::clipboard::{Clipboard, ClipboardType};
use rio_window::event::{ElementState, KeyEvent};
use rio_window::keyboard::{Key, ModifiersState};
use terminus_ui::{normalize_paste, TextDraft, TextEdit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldChord {
    Copy,
    Cut,
    Paste,
}

/// The clipboard chord `key` + `mods` spell, if any.
pub(crate) fn field_chord(key: &Key, mods: ModifiersState) -> Option<FieldChord> {
    if !(mods.control_key() || mods.super_key()) || mods.alt_key() {
        return None;
    }
    let Key::Character(ch) = key else {
        return None;
    };
    match ch.to_ascii_lowercase().as_str() {
        "c" => Some(FieldChord::Copy),
        "v" => Some(FieldChord::Paste),
        "x" if !mods.shift_key() => Some(FieldChord::Cut),
        _ => None,
    }
}

/// Which text sink a chord goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sink {
    /// Drawer fields, overlays and views: the `overlay_commit_text` family.
    Overlay,
    /// The SFTP name editor or quick-find box.
    Sftp,
}

fn looks_like_pem(text: &str) -> bool {
    let trimmed = text.trim_start();
    (text.contains("BEGIN") && text.contains("PRIVATE"))
        || trimmed.starts_with("b3BlbnNzaC1rZXk")
}

impl Route<'_> {
    fn drawer_field_open(&self) -> bool {
        let chrome = &self.window.screen.chrome;
        if chrome.add_host_is_open() {
            return false;
        }
        if chrome.shell.rename.is_some() {
            return true;
        }
        chrome.hosts_visible()
            && (chrome.panel.filter_focused
                || chrome.panel.new_group_focused
                || chrome.panel.rename.as_ref().is_some_and(|r| r.focused))
    }

    fn field_sink(&self) -> Option<Sink> {
        if self.drawer_field_open() {
            return Some(Sink::Overlay);
        }
        match self.active_modal() {
            Some(
                Modal::IslandRename
                | Modal::CommandPalette
                | Modal::HostEditor
                | Modal::AddSnippet
                | Modal::VaultUnlock
                | Modal::Settings,
            ) => Some(Sink::Overlay),
            Some(_) => None,
            None => {
                let screen = &self.window.screen;
                if !screen.view_takes_keys() {
                    return None;
                }
                if screen.sftp_bridged() {
                    screen
                        .sftp
                        .as_ref()
                        .is_some_and(|s| s.state.text_sink_active())
                        .then_some(Sink::Sftp)
                } else {
                    Some(Sink::Overlay)
                }
            }
        }
    }

    /// Handle a Copy / Cut / Paste key press aimed at a text field. Returns
    /// whether the key was consumed; `false` leaves it to the usual routing.
    pub(super) fn field_clipboard_key(
        &mut self,
        key_event: &KeyEvent,
        clipboard: &mut Clipboard,
    ) -> bool {
        if key_event.state != ElementState::Pressed {
            return false;
        }
        let mods = self.window.screen.modifiers.state();
        let Some(chord) = field_chord(&key_event.logical_key, mods) else {
            return false;
        };
        let Some(sink) = self.field_sink() else {
            return false;
        };
        match chord {
            FieldChord::Paste => self.paste_into_field(sink, clipboard),
            FieldChord::Copy | FieldChord::Cut => {
                let cut = chord == FieldChord::Cut;
                let text = match sink {
                    Sink::Overlay => self.overlay_selection(cut),
                    Sink::Sftp => self.sftp_selection(cut),
                };
                if let Some(text) = text.filter(|t| !t.is_empty()) {
                    clipboard.set(ClipboardType::Clipboard, text);
                }
            }
        }
        self.request_overlay_redraw();
        true
    }

    fn paste_into_field(&mut self, sink: Sink, clipboard: &mut Clipboard) {
        let raw = match read_clipboard_text(clipboard) {
            Ok(raw) => raw,
            Err(message) => {
                self.window.screen.report_paste_failure(&message);
                return;
            }
        };
        match sink {
            Sink::Sftp => {
                if let Some(session) = self.window.screen.sftp.as_mut() {
                    session.state.paste_text(&raw);
                }
            }
            Sink::Overlay => {
                let settings = &self.window.screen.chrome.settings;
                let multiline = self.active_modal() == Some(Modal::Settings)
                    && settings.key_drafting
                    && (settings.key_draft_pem_focused || looks_like_pem(&raw));
                let text = normalize_paste(&raw, multiline);
                if !text.is_empty() {
                    self.overlay_commit_text(&text);
                }
            }
        }
    }

    fn sftp_selection(&mut self, cut: bool) -> Option<String> {
        let state = &mut self.window.screen.sftp.as_mut()?.state;
        if cut {
            state.cut_text()
        } else {
            state.copy_text()
        }
    }

    /// The selected text of the focused overlay field; with `cut` it is
    /// removed from the field too. Secrets (passwords, passphrases) are
    /// never copied.
    fn overlay_selection(&mut self, cut: bool) -> Option<String> {
        let text = {
            let draft = self.overlay_draft_mut()?;
            draft.selected_text()?.to_string()
        };
        if cut {
            self.delete_overlay_selection();
        }
        Some(text)
    }

    fn overlay_draft_mut(&mut self) -> Option<&mut TextDraft> {
        if self.drawer_field_open() {
            let chrome = &mut self.window.screen.chrome;
            if chrome.shell.rename.is_some() {
                return chrome.shell.rename.as_mut().map(|r| &mut r.text);
            }
            if chrome.panel.filter_focused {
                return Some(&mut chrome.panel.filter);
            }
            if chrome.panel.new_group_focused {
                return Some(&mut chrome.panel.new_group_name);
            }
            return chrome.panel.rename.as_mut().map(|r| &mut r.text);
        }
        match self.active_modal()? {
            Modal::IslandRename => self
                .window
                .screen
                .renderer
                .island
                .as_mut()
                .and_then(|i| i.rename_draft_mut()),
            Modal::CommandPalette => {
                Some(&mut self.window.screen.renderer.command_palette.query)
            }
            Modal::HostEditor => self.window.screen.chrome.form.copyable_draft_mut(),
            Modal::AddSnippet => self
                .window
                .screen
                .chrome
                .snippet_form
                .inner
                .focused_draft_mut(),
            Modal::Settings => {
                let settings = &mut self.window.screen.chrome.settings;
                if settings.key_drafting {
                    if settings.key_draft_passphrase_focused {
                        None
                    } else {
                        settings.key_draft_active()
                    }
                } else if settings.sql_focus == terminus_ui::SqlSyncFocus::Passphrase {
                    None
                } else {
                    settings.sql_draft_active()
                }
            }
            _ => None,
        }
    }

    /// Cut: drop the selection through the field's own edit path, so side
    /// effects (palette results, host form error) stay what typing gives.
    fn delete_overlay_selection(&mut self) {
        let backspace = TextEdit::Backspace { by_word: false };
        if !self.drawer_field_open() {
            match self.active_modal() {
                Some(Modal::CommandPalette) => {
                    self.window
                        .screen
                        .renderer
                        .command_palette
                        .edit_query(backspace);
                    return;
                }
                Some(Modal::HostEditor) => {
                    self.window.screen.chrome.form.edit(backspace);
                    return;
                }
                _ => {}
            }
        }
        if let Some(draft) = self.overlay_draft_mut() {
            draft.delete_selection();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(c: &str) -> Key {
        Key::Character(c.into())
    }

    #[test]
    fn plain_and_shifted_chords_are_recognised() {
        let ctrl = ModifiersState::CONTROL;
        assert_eq!(field_chord(&ch("c"), ctrl), Some(FieldChord::Copy));
        assert_eq!(field_chord(&ch("x"), ctrl), Some(FieldChord::Cut));
        assert_eq!(field_chord(&ch("v"), ctrl), Some(FieldChord::Paste));
        let cs = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert_eq!(field_chord(&ch("C"), cs), Some(FieldChord::Copy));
        assert_eq!(field_chord(&ch("V"), cs), Some(FieldChord::Paste));
        assert_eq!(field_chord(&ch("X"), cs), None);
        assert_eq!(
            field_chord(&ch("v"), ModifiersState::SUPER),
            Some(FieldChord::Paste)
        );
    }

    #[test]
    fn other_keys_and_modifiers_are_not_chords() {
        assert_eq!(field_chord(&ch("v"), ModifiersState::empty()), None);
        assert_eq!(field_chord(&ch("a"), ModifiersState::CONTROL), None);
        assert_eq!(
            field_chord(&ch("v"), ModifiersState::CONTROL | ModifiersState::ALT),
            None
        );
    }

    #[test]
    fn pem_detection_matches_headers_and_raw_openssh_bodies() {
        assert!(looks_like_pem("-----BEGIN OPENSSH PRIVATE KEY-----\nabc"));
        assert!(looks_like_pem("  b3BlbnNzaC1rZXktdjEAAAAA"));
        assert!(!looks_like_pem("ssh-ed25519 AAAA"));
    }
}
