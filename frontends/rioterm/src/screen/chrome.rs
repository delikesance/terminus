//! `Screen` chrome surface, split out of `screen/mod.rs`.

use super::{ChromePress, Screen};
use crate::context;
use crate::hosts;
use rio_backend::config::layout::Margin;
use rio_backend::event::ClickState;
use rio_window::window::CursorIcon;
use terminus_ui::sidebar::Badge;

impl Screen<'_> {
    /// Window size in logical pixels, the space `terminus_ui` lays out in.
    pub(super) fn chrome_viewport(&self) -> (f32, f32) {
        let size = self.sugarloaf.window_size();
        let scale = self.sugarloaf.scale_factor().max(1.0);
        (size.width / scale, size.height / scale)
    }

    /// Apply anything the host worker has sent, and refresh session washes
    /// from the open tabs. Returns whether the chrome changed.
    pub fn pump_chrome(&mut self) -> bool {
        let store_changed = self.host_store.drain();
        let sftp_changed = self.sftp.as_mut().is_some_and(|s| s.pump());
        let store_changed = store_changed || sftp_changed;

        let open_host_ids: Vec<String> = self
            .context_manager
            .contexts_mut()
            .iter()
            .filter_map(|tab| tab.current().host_id.clone())
            .collect();
        let current = self.context_manager.current_index();
        let len = self.context_manager.len();
        let sessions: Vec<hosts::OpenSession> = (0..len)
            .map(|i| {
                let host_id = self
                    .context_manager
                    .contexts_mut()
                    .get(i)
                    .and_then(|grid| grid.current().host_id.clone());
                let title = self
                    .context_manager
                    .custom_title(i)
                    .map(str::to_string)
                    .or_else(|| self.context_manager.title(i).map(|t| t.content.clone()))
                    .unwrap_or_else(|| format!("Session {}", i + 1));
                let closable = !self.context_manager.is_pinned(i);
                hosts::OpenSession {
                    tab_index: i,
                    host_id,
                    title,
                    active: i == current,
                    closable,
                }
            })
            .collect();
        let rows = hosts::sidebar_rows(
            self.host_store.platform(),
            self.host_store.hosts(),
            self.host_store.groups(),
            &self.chrome.panel.collapsed_groups,
            &self.chrome.panel.collapsed_hosts,
            &open_host_ids,
            &sessions,
        );
        let mut rows_changed = rows != self.chrome.panel.rows;
        if rows_changed {
            self.chrome.set_rows(rows);
        }

        // Keep open-tab OS glyphs in sync after DetectOs / host reload.
        if store_changed {
            let host_os: Vec<(String, Option<String>)> = self
                .host_store
                .hosts()
                .iter()
                .map(|h| (h.id.clone(), h.os_id.clone()))
                .collect();
            let tab_count = self.context_manager.len();
            for i in 0..tab_count {
                let host_id = self
                    .context_manager
                    .contexts_mut()
                    .get(i)
                    .and_then(|g| g.current().host_id.clone());
                let Some(hid) = host_id else {
                    continue;
                };
                let Some((_, os)) = host_os.iter().find(|(id, _)| id == &hid) else {
                    continue;
                };
                if self.context_manager.tab_os_id(i) != os.as_deref() {
                    self.context_manager.set_tab_os_id(i, os.clone());
                    rows_changed = true;
                }
            }
        }

        let snips = self.host_store.snippet_items.clone();
        if snips != self.chrome.snippets.items {
            self.chrome.snippets.items = snips;
            self.chrome.snippet_form.inner.closing = true;
            rows_changed = true;
        }

        if !store_changed && !rows_changed {
            // Still refresh the Forget button when Settings is open.
            if self.chrome.settings.open {
                let remembered = crate::vault_remember::has_remembered_passphrase();
                if self.chrome.settings.passphrase_remembered != remembered {
                    self.chrome.settings.set_passphrase_remembered(remembered);
                    return true;
                }
            }
            return false;
        }

        // `create` hands out no id, so the row inserted a moment ago is
        // found by the label the worker echoed back.
        if store_changed {
            // Keep Settings keys + add-host picker in sync with the store.
            let key_items: Vec<terminus_ui::settings::SshKeyItem> = self
                .host_store
                .identities()
                .iter()
                .map(|(id, name, fingerprint, created)| {
                    terminus_ui::settings::SshKeyItem {
                        id: id.clone(),
                        name: name.clone(),
                        fingerprint: fingerprint.clone(),
                        created: created.clone(),
                    }
                })
                .collect();
            self.chrome.settings.set_keys(key_items.clone());
            self.chrome
                .form
                .set_identities(key_items.into_iter().map(|k| (k.id, k.name)).collect());

            if let Some(notice) = self.host_store.take_notice() {
                if notice.starts_with("SSH key") {
                    self.chrome.settings.close_key_draft();
                }
                self.chrome.panel.notice = Some(notice);
                self.chrome.panel.error = None;
                if let Some(label) = self.pending_host_select.take() {
                    if let Some(index) = self.chrome.panel.rows.iter().position(|row| {
                        row.host().is_some_and(|item| {
                            item.name == label && item.badge == Badge::Ssh
                        })
                    }) {
                        self.chrome.panel.selected = Some(index);
                    }
                }
                if self.chrome.add_host_is_open() {
                    self.chrome.form.close();
                }
                if self.chrome.add_snippet_is_open() {
                    self.chrome.snippet_form.inner.closing = true;
                }
            }
            if let Some(message) = self.host_store.error().map(str::to_string) {
                if self.chrome.settings.key_drafting {
                    self.chrome.settings.key_draft_error = Some(message.clone());
                }
                if self.chrome.add_host_is_open() {
                    if message.contains("Unlock the vault")
                        && !self.chrome.vault_unlock_is_open()
                    {
                        self.open_vault_unlock_for(
                            terminus_ui::PendingVaultAction::SubmitHostForm,
                        );
                    } else {
                        self.chrome.form.set_error(message);
                    }
                } else if !self.chrome.settings.key_drafting {
                    self.chrome.panel.error = Some(message);
                }
            }
            // Vault unlock feedback is already folded into sync_status_line
            // by the repository; drop the one-shot so it is not also shown
            // on the host-list notice band.
            if let Some(msg) = self.host_store.take_vault_message() {
                if self.chrome.vault_unlock_is_open() {
                    if self.host_store.vault_unlocked() {
                        self.pending_vault_continue =
                            self.chrome.vault_unlock.take_pending_on_success();
                    } else {
                        self.chrome.vault_unlock.set_error(msg);
                    }
                }
            }
            self.chrome
                .settings
                .apply_sync_status(terminus_ui::SyncUiStatus {
                    uri: self.host_store.sync_uri().to_string(),
                    connected: self.host_store.sync_connected(),
                    vault_unlocked: self.host_store.vault_unlocked(),
                    status_line: self.host_store.sync_status_line().to_string(),
                    is_error: self.host_store.sync_status_is_error(),
                });
            self.chrome.settings.set_passphrase_remembered(
                crate::vault_remember::has_remembered_passphrase(),
            );
        }
        true
    }

    /// Consume a post-unlock retry queued by [`Self::pump_chrome`].
    pub fn take_pending_vault_continue(
        &mut self,
    ) -> Option<terminus_ui::PendingVaultAction> {
        self.pending_vault_continue.take()
    }

    pub(super) fn open_vault_unlock_for(
        &mut self,
        pending: terminus_ui::PendingVaultAction,
    ) {
        if crate::vault_remember::has_remembered_passphrase() {
            self.chrome.vault_unlock.set_remember(true);
        }
        self.chrome.open_vault_unlock(pending);
    }

    /// Route a mouse press given in logical pixels.
    pub fn chrome_press(&mut self, x: f32, y: f32) -> terminus_ui::chrome::ChromeAction {
        let (width, height) = self.chrome_viewport();
        let reserved_before = self.chrome.reserved_width();
        let menu_was_open = self.chrome.context_menu.is_some();
        let action = self.chrome.handle_press(width, height, x, y);
        // Collapsing the rail or toggling the panel changes how much of
        // the window the terminal may use.
        if self.chrome.reserved_width() != reserved_before {
            self.reapply_chrome_inset();
        }
        action
    }

    /// Right-click on chrome: open or dismiss a context menu.
    pub fn chrome_context_press(
        &mut self,
        x: f32,
        y: f32,
    ) -> terminus_ui::chrome::ChromeAction {
        let (width, height) = self.chrome_viewport();
        let sftp_open = self.sftp.is_some();
        let action = self
            .chrome
            .handle_context_press(width, height, x, y, sftp_open);
        action
    }

    /// Right-click inside the SFTP pane: open a file/folder context menu.
    pub fn handle_sftp_context_press(&mut self, x: f32, y: f32) -> bool {
        let Some(bounds) = self.sftp_bounds() else {
            return false;
        };
        let (width, height) = self.chrome_viewport();
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        let layout = terminus_ui::SftpPaneLayout::from_state(bounds, &session.state);
        let hit = layout.hit_test(&session.state, x, y);
        if matches!(hit, terminus_ui::SftpHit::Miss) {
            return false;
        }

        let menu = session.context_menu_for_hit(&layout, hit, x, y);
        if let Some(menu) = menu {
            self.chrome.context_menu = Some(menu.clamped(width, height));
            self.mark_dirty();
            true
        } else {
            self.chrome.close_context_menu();
            true // consume right-click over SFTP chrome even without a menu
        }
    }

    /// Update host→group drag while the left button is held.
    pub fn chrome_drag_move(&mut self, x: f32, y: f32) -> bool {
        let (_, height) = self.chrome_viewport();
        self.chrome.handle_drag_move(height, x, y)
    }

    /// Finish a host press/drag on left-button release.
    pub fn chrome_release(
        &mut self,
        x: f32,
        y: f32,
    ) -> terminus_ui::chrome::ChromeAction {
        let (_, height) = self.chrome_viewport();
        self.chrome.handle_release(height, x, y)
    }

    /// Re-flow the grid after the chrome's reserved width changed.
    ///
    /// Every tab shares the window, so every grid needs the new margin
    /// and a layout pass, not just the current one.
    pub fn reapply_chrome_inset(&mut self) {
        let scale = self.sugarloaf.scale_factor();
        let left = (self.renderer.margin.left + self.chrome.reserved_width()) * scale;
        for context_grid in self.context_manager.contexts_mut() {
            let margin = context_grid.scaled_margin;
            context_grid.update_scaled_margin(Margin::new(
                margin.top,
                margin.right,
                margin.bottom,
                left,
            ));
            context_grid.update_dimensions(&mut self.sugarloaf);
        }
        self.renderer.trail_cursor.snap();
    }

    /// Route a mouse move. Returns whether the chrome changed.
    pub fn chrome_hover(&mut self, x: f32, y: f32) -> bool {
        let (width, height) = self.chrome_viewport();
        self.chrome.set_window_size(width, height);
        self.chrome.handle_hover(height, x, y)
    }

    /// Route a wheel notch. Returns whether the chrome consumed it.
    pub fn chrome_wheel(&mut self, x: f32, y: f32, lines: f32) -> bool {
        let (_, height) = self.chrome_viewport();
        self.chrome.handle_wheel(height, x, y, lines)
    }

    /// Chrome-aware cursor under logical `(x, y)`. `None` means the
    /// pointer is over the terminal grid and [`Self::mouse_cursor_icon`]
    /// should decide.
    pub fn chrome_cursor_at(&self, x: f32, y: f32) -> Option<CursorIcon> {
        let (width, height) = self.chrome_viewport();
        let over_modal = self.chrome_overlay_dialog_open();
        let over_rail =
            !self.chrome.activity.collapsed && x < self.chrome.reserved_width();
        if !over_modal && !over_rail {
            return None;
        }
        Some(match self.chrome.cursor_at(width, height, x, y) {
            terminus_ui::ChromeCursor::Pointer => CursorIcon::Pointer,
            terminus_ui::ChromeCursor::Text => CursorIcon::Text,
            terminus_ui::ChromeCursor::Default => CursorIcon::Default,
        })
    }

    /// True when a chrome dialog must own the cursor over the grid/SFTP.
    pub fn chrome_overlay_dialog_open(&self) -> bool {
        self.chrome.settings_is_open()
            || self.chrome.add_host_is_open()
            || self.chrome.add_snippet_is_open()
            || self.chrome.vault_unlock_is_open()
            || self.chrome.connection.is_some()
    }

    pub(super) fn on_chrome_press(
        &mut self,
        window: &rio_window::window::Window,
        prev: Option<ChromePress>,
    ) {
        let window_origin = window.outer_position().ok();
        let double = matches!(self.mouse.click_state, ClickState::DoubleClick)
            && prev.is_some_and(|p| p.validates_double_click(window_origin));
        if double {
            let is_maximized = window.is_maximized();
            window.set_maximized(!is_maximized);
            self.window_maximized = !is_maximized;
            return;
        }

        self.last_chrome_press = Some(ChromePress {
            window_origin,
            at: std::time::Instant::now(),
        });
        if self.allow_manual_dragging {
            self.start_window_drag(window);
        }
    }

    #[inline]
    pub fn take_chrome_press(&mut self) -> Option<ChromePress> {
        self.last_chrome_press.take()
    }
}
