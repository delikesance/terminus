//! Settings page (`terminus_ui::views::settings`): data feed, input
//! routing and the executor of its actions (host worker, config file,
//! updater, clipboard).

use super::super::Screen;
use crate::renderer::views::settings as painter;
use rio_backend::clipboard::{Clipboard, ClipboardType};
use terminus_ui::screens::{ViewInput, ViewKey, ViewOutcome};
use terminus_ui::shell::WorkspaceView;
use terminus_ui::views::settings::sync::SyncStatus;
use terminus_ui::views::settings::{Key, Page, SettingsAction};

impl Screen<'_> {
    /// Keep the Settings page in step with the shell tab and the stores.
    /// Cheap; runs every frame.
    pub(super) fn sync_settings_view(&mut self) {
        let WorkspaceView::Settings(page) = self.chrome.shell.view() else {
            return;
        };
        let view = &mut self.settings_view;
        view.set_page(Page::from(page));

        let keys: Vec<terminus_ui::settings::SshKeyItem> = self
            .host_store
            .identities()
            .iter()
            .map(|k| terminus_ui::settings::SshKeyItem {
                id: k.id.clone(),
                name: k.name.clone(),
                fingerprint: k.fingerprint.clone(),
                created: k.created.clone(),
                public_key: k.public_key.clone(),
            })
            .collect();
        if keys != view.keys.keys {
            view.keys.set_keys(keys);
        }

        let status = SyncStatus {
            connected: self.host_store.sync_connected(),
            vault_unlocked: self.host_store.vault_unlocked(),
            line: self.host_store.sync_status_line().to_string(),
            is_error: self.host_store.sync_status_is_error(),
        };
        let uri = self.host_store.sync_uri();
        if status != view.sync.status
            || (!view.sync.focused && uri != view.sync.uri.value)
        {
            view.sync.apply_snapshot(uri, status);
        }

        view.updates.status = painter::update_status(self.updater.state());

        if view.page == Page::Appearance && view.appearance.fonts.is_empty() {
            let names = self.sugarloaf.font_family_names();
            self.settings_view.appearance.set_fonts(names);
        }
    }

    /// Route one input to the Settings page.
    pub(super) fn settings_view_input(
        &mut self,
        input: &ViewInput,
        clipboard: &mut Clipboard,
    ) -> ViewOutcome {
        let content = self.chrome.shell.content_rect();
        let redraw = |changed: bool| {
            if changed {
                ViewOutcome::Redraw
            } else {
                ViewOutcome::Consumed
            }
        };
        match input {
            ViewInput::Press { x, y, .. } => {
                if let Some(action) = painter::press(
                    &mut self.sugarloaf,
                    content,
                    &mut self.settings_view,
                    *x,
                    *y,
                ) {
                    self.execute_settings_action(action, clipboard);
                }
                ViewOutcome::Redraw
            }
            ViewInput::Move { x, y, .. } => redraw(painter::hover(
                &mut self.sugarloaf,
                content,
                &mut self.settings_view,
                *x,
                *y,
            )),
            // Shell: lines > 0 scrolls towards the end; the page: dy > 0.
            ViewInput::Wheel { x, y, lines } => redraw(painter::wheel(
                &mut self.sugarloaf,
                content,
                &mut self.settings_view,
                *x,
                *y,
                *lines,
            )),
            ViewInput::Release { .. } | ViewInput::ContextPress { .. } => {
                ViewOutcome::Consumed
            }
            ViewInput::Key { key, mods } => {
                let captured = self.settings_view.captures_keyboard();
                let key = match key {
                    ViewKey::Text(t) if mods.ctrl && t.eq_ignore_ascii_case("a") => {
                        Key::SelectAll
                    }
                    ViewKey::Text(t) if !mods.ctrl && !mods.logo => {
                        return redraw(self.settings_view.insert_text(t));
                    }
                    ViewKey::Text(_) => return ViewOutcome::Ignored,
                    ViewKey::Enter => Key::Enter,
                    // Nothing to close: leave Esc to the shell.
                    ViewKey::Escape if !captured => return ViewOutcome::Ignored,
                    ViewKey::Escape => Key::Escape,
                    ViewKey::Backspace => Key::Backspace,
                    ViewKey::Delete => Key::Delete,
                    ViewKey::Tab if mods.shift => Key::ShiftTab,
                    ViewKey::Tab => Key::Tab,
                    ViewKey::Up => Key::Up,
                    ViewKey::Down => Key::Down,
                    ViewKey::Left => Key::Left,
                    ViewKey::Right => Key::Right,
                    ViewKey::Home => Key::Home,
                    ViewKey::End => Key::End,
                    ViewKey::PageUp | ViewKey::PageDown | ViewKey::F2 => {
                        return ViewOutcome::Ignored
                    }
                };
                if let Some(action) = self.settings_view.key(key) {
                    self.execute_settings_action(action, clipboard);
                }
                ViewOutcome::Redraw
            }
        }
    }

    /// Carry out one Settings action.
    pub(crate) fn execute_settings_action(
        &mut self,
        action: SettingsAction,
        clipboard: &mut Clipboard,
    ) {
        match action {
            SettingsAction::SubmitKey {
                name,
                pem,
                passphrase,
            } => self.host_store.import_ssh_key(&name, pem, passphrase),
            SettingsAction::CopyPublicKey { id } => {
                let key = self
                    .host_store
                    .identities()
                    .iter()
                    .find(|k| k.id == id)
                    .map(|k| k.public_key.clone())
                    .filter(|k| !k.is_empty());
                match key {
                    Some(key) => {
                        clipboard.set(ClipboardType::Clipboard, key);
                        self.chrome.panel.notice = Some("Public key copied".into());
                    }
                    None => {
                        self.chrome.panel.error =
                            Some("This key has no public key stored".into())
                    }
                }
            }
            SettingsAction::DeleteKey { id } => self.host_store.delete_ssh_key(&id),
            SettingsAction::SaveSync { uri } | SettingsAction::SyncNow { uri } => {
                self.host_store.test_sync(&uri)
            }
            SettingsAction::UnlockVault => {
                let configured = self.host_store.vault_configured();
                self.chrome.vault_configured = configured;
                self.open_vault_unlock_for(
                    terminus_ui::PendingVaultAction::settings_unlock(configured),
                )
            }
            SettingsAction::SetFont(_)
            | SettingsAction::SetFontSize(_)
            | SettingsAction::SetCursor(_)
            | SettingsAction::SetTheme(_)
            | SettingsAction::SetCheckUpdates(_)
            | SettingsAction::SetAutoInstall(_) => {
                // The config watcher hot-reloads the file: live preview.
                if let Err(err) = painter::persist(&action) {
                    self.chrome.panel.error =
                        Some(format!("Could not save the config file: {err}"));
                }
            }
            SettingsAction::CheckUpdates => self.updater.check_now(),
            SettingsAction::InstallUpdate => self.updater.install(),
        }
    }

    /// The vault was unlocked for a pending "Save key": resubmit the
    /// Settings page draft. False when the page has no draft open.
    pub(crate) fn resubmit_settings_key_draft(&mut self) -> bool {
        let Some(action) = self
            .settings_view
            .keys
            .draft
            .as_mut()
            .and_then(|d| d.submit())
        else {
            return false;
        };
        if let SettingsAction::SubmitKey {
            name,
            pem,
            passphrase,
        } = action
        {
            self.host_store.import_ssh_key(&name, pem, passphrase);
        }
        true
    }
}
