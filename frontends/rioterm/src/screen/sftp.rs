//! `Screen` sftp surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::hosts;
use crate::renderer::views::files::{self as files_view, FilesAction};
use rio_window::window::CursorIcon;
use terminus_ui::views::files::FilesHit;

use crate::hosts::SftpAuth;

impl Screen<'_> {
    /// Sidebar id of the selected machine (the current tab's).
    fn selected_machine_id(&self) -> String {
        self.chrome
            .shell
            .machine
            .as_ref()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| hosts::LOCAL_ID.to_string())
    }

    /// Whether `host_id` already has a browser (shown or parked).
    fn has_sftp_browser(&self, host_id: &str) -> bool {
        self.sftp.as_ref().is_some_and(|s| s.machine_id == host_id)
            || self.sftp_parked.contains(host_id)
    }

    /// Ask the host-store worker for `host`'s SFTP credentials without
    /// waiting (a slow sync or probe may hold it): the browser opens in
    /// [`Self::settle_sftp_auth`] when they arrive. Meanwhile Files shows
    /// "Connecting to …".
    fn request_sftp_auth(&mut self, host: &hosts::HostRow, other_pane: bool) {
        let selected = self.selected_machine_id();
        if !self
            .sftp_pending
            .iter()
            .any(|(id, other, _)| *id == host.id && *other == other_pane)
        {
            self.sftp_pending.push((host.id.clone(), other_pane, selected));
            self.host_store
                .request_sftp_auth(&host.id, &host.auth_method);
        }
        if !other_pane {
            self.chrome.screens.files.begin_connecting(&host.id);
        }
        self.mark_dirty();
    }

    /// Open the browsers whose credentials arrived (after a store drain).
    /// A locked vault opens the unlock prompt and retries once unlocked;
    /// other failures go to the Files view (with Retry) or the sidebar.
    pub(super) fn settle_sftp_auth(&mut self) -> bool {
        let replies = self.host_store.take_sftp_auth_replies();
        let any = !replies.is_empty();
        for (id, result) in replies {
            let waiting: Vec<(bool, String)> = self
                .sftp_pending
                .iter()
                .filter(|(pid, _, _)| *pid == id)
                .map(|(_, other, selected)| (*other, selected.clone()))
                .collect();
            self.sftp_pending.retain(|(pid, _, _)| *pid != id);
            for (other_pane, selected_then) in waiting {
                // The user moved to another machine while the key was
                // read: open the browser parked, don't pull them back.
                let focus = self.selected_machine_id() == selected_then;
                if !other_pane {
                    self.chrome.screens.files.end_connecting(&id);
                }
                let opened = match result.clone() {
                    Ok(auth) => self.sftp_host_row(&id).and_then(|host| {
                        if other_pane && self.sftp.is_some() {
                            let (password, identity) = auth;
                            let session = self.sftp.as_mut().expect("checked");
                            session.open_other_host(&host, password, identity)
                        } else {
                            self.show_sftp_browser(&host, Some(auth), focus)
                        }
                    }),
                    Err(err) if err.contains("Unlock the vault") => {
                        self.open_vault_unlock_for(
                            terminus_ui::PendingVaultAction::OpenSftp {
                                host_id: id.clone(),
                                other_pane,
                            },
                        );
                        Ok(())
                    }
                    Err(err) => Err(err),
                };
                if let Err(err) = opened {
                    let files = &mut self.chrome.screens.files;
                    if !other_pane && files.machine_id == id {
                        files.fail(err);
                    } else {
                        self.chrome.panel.error = Some(err);
                    }
                }
            }
        }
        if any {
            self.mark_dirty();
        }
        any
    }

    pub(super) fn sftp_host_row(&self, host_id: &str) -> Result<hosts::HostRow, String> {
        if host_id == hosts::LOCAL_ID || host_id.starts_with(hosts::WSL_PREFIX) {
            return Err("SFTP is only available for SSH hosts".into());
        }
        self.host_store
            .hosts()
            .iter()
            .find(|h| h.id == host_id)
            .cloned()
            .ok_or_else(|| format!("No stored host {host_id}"))
    }

    /// Open (or bring back) the SFTP browser of `host_id`. A machine
    /// that has one gets it back at once; otherwise its credentials are
    /// requested and the browser opens when they arrive — the UI thread
    /// never waits on the host store.
    pub fn open_sftp_pane(&mut self, host_id: &str) -> Result<(), String> {
        let host = self.sftp_host_row(host_id)?;
        if self.has_sftp_browser(host_id) {
            return self.show_sftp_browser(&host, None, true);
        }
        self.request_sftp_auth(&host, false);
        Ok(())
    }

    /// Show `host`'s browser: its existing one, or a new one started with
    /// `auth`.
    fn show_sftp_browser(
        &mut self,
        host: &hosts::HostRow,
        auth: Option<SftpAuth>,
        focus: bool,
    ) -> Result<(), String> {
        let host_id = host.id.as_str();
        let wake = self.sftp_wake.clone();
        let owner = crate::sftp_ui::ActiveSftp::owner;

        // One browser per machine: the one on screen is parked (never
        // closed: its transfers go on), and a machine that already has a
        // browser gets it back instead of a new connection.
        self.sftp_parked.follow(&mut self.sftp, owner, host_id);
        if self.sftp.is_none() {
            let Some((password, identity)) = auth else {
                return Err("SFTP credentials are missing".into());
            };
            let session =
                crate::sftp_ui::ActiveSftp::start(host, password, identity, wake)?;
            self.sftp = Some(session);
        }

        // The browser lives in the Files view of its machine: bring that
        // machine forward when it has a tab; otherwise the browser waits,
        // parked, until the machine is selected.
        let shown = (focus && self.focus_machine_tab(host_id))
            || self.selected_machine_id() == host_id;
        if !shown {
            if let Some(session) = self.sftp.take() {
                if let Some(old) = self.sftp_parked.park(session, owner) {
                    old.close();
                }
            }
            self.chrome.panel.notice = Some(format!(
                "Files on {} is ready: select it in the sidebar",
                host.name
            ));
            self.mark_dirty();
            return Ok(());
        }
        self.show_view(terminus_ui::shell::WorkspaceView::Files);
        self.mark_dirty();
        Ok(())
    }

    /// Make `host_id` the selected machine by bringing one of its tabs
    /// forward. Returns false when it has no open tab.
    fn focus_machine_tab(&mut self, host_id: &str) -> bool {
        let current = self.context_manager.current_index();
        let tab_host = |screen: &mut Self, i: usize| {
            screen
                .context_manager
                .contexts_mut()
                .get(i)
                .and_then(|g| g.current().host_id.clone())
                .unwrap_or_else(|| hosts::LOCAL_ID.to_string())
        };
        if tab_host(self, current) == host_id {
            return true;
        }
        let Some(idx) =
            (0..self.context_manager.len()).rfind(|&i| tab_host(self, i) == host_id)
        else {
            return false;
        };
        self.stop_hint_mode_if_active();
        self.clear_selection();
        self.context_manager.set_current(idx);
        self.switch_visible_context(current, idx);
        self.sync_sidebar_selection();
        true
    }

    /// Open `host_id` on the other SFTP pane (left). Starts SFTP if none is open.
    pub fn open_sftp_other_pane(&mut self, host_id: &str) -> Result<(), String> {
        if self.sftp.is_none() {
            return self.open_sftp_pane(host_id);
        }
        let host = self.sftp_host_row(host_id)?;
        self.request_sftp_auth(&host, true);
        Ok(())
    }

    /// Close the SFTP pane and restore terminal paint on the current leaf.
    pub fn close_sftp_pane(&mut self) {
        if let Some(session) = self.sftp.take() {
            session.close();
        }
        if self.chrome.shell.view() == terminus_ui::shell::WorkspaceView::Files {
            self.show_view(terminus_ui::shell::WorkspaceView::Terminal);
        }
        self.mark_dirty();
    }

    /// Logical bounds of the Files view (two SFTP panes): the content
    /// rect while a session is open and Files is shown.
    pub fn sftp_bounds(&self) -> Option<terminus_ui::Rect> {
        self.sftp_bridged()
            .then(|| self.chrome.shell.content_rect())
    }

    fn files_redraw(&mut self, action: &FilesAction) -> bool {
        if action.needs_redraw() {
            self.mark_dirty();
        }
        action.needs_redraw()
    }

    pub fn handle_sftp_click(
        &mut self,
        x: f32,
        y: f32,
        double: bool,
    ) -> terminus_ui::SftpClickResult {
        let Some(content) = self.sftp_bounds() else {
            return terminus_ui::SftpClickResult::Miss;
        };
        let Some(session) = self.sftp.as_mut() else {
            return terminus_ui::SftpClickResult::Miss;
        };
        let action = files_view::pointer_press(
            &mut self.sugarloaf,
            session,
            &mut self.files_view,
            content,
            x,
            y,
            double,
        );
        if self.files_redraw(&action) {
            terminus_ui::SftpClickResult::Handled
        } else {
            terminus_ui::SftpClickResult::Miss
        }
    }

    pub fn handle_sftp_drag_move(&mut self, x: f32, y: f32) -> bool {
        let Some(content) = self.sftp_bounds() else {
            return false;
        };
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        let action =
            files_view::pointer_move(session, &mut self.files_view, content, x, y, true);
        self.files_redraw(&action)
    }

    pub fn handle_sftp_drag_release(&mut self, x: f32, y: f32) -> bool {
        let Some(content) = self.sftp_bounds() else {
            if let Some(session) = self.sftp.as_mut() {
                session.cancel_drag();
            }
            return false;
        };
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        let action = files_view::pointer_release(session, content, x, y);
        self.files_redraw(&action)
    }

    pub fn handle_sftp_hover(&mut self, x: f32, y: f32) -> bool {
        let Some(content) = self.sftp_bounds() else {
            return false;
        };
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        let action =
            files_view::pointer_move(session, &mut self.files_view, content, x, y, false);
        self.files_redraw(&action)
    }

    /// Cursor over the Files view. `None` when the pointer is outside it.
    pub fn sftp_cursor_at(&self, x: f32, y: f32) -> Option<CursorIcon> {
        let content = self.sftp_bounds()?;
        let session = self.sftp.as_ref()?;
        if session.state.conflict.is_some() {
            return Some(CursorIcon::Default);
        }
        match files_view::hit_at(content, &session.state, x, y) {
            FilesHit::Miss => None,
            hit if hit.is_clickable() => Some(CursorIcon::Pointer),
            _ => Some(CursorIcon::Default),
        }
    }

    /// Wheel over the Files view; `lines` as winit reports them (positive
    /// = towards the top).
    pub fn handle_sftp_scroll(&mut self, x: f32, y: f32, lines: f32) -> bool {
        let Some(content) = self.sftp_bounds() else {
            return false;
        };
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        let action = files_view::wheel(session, content, x, y, -lines);
        self.files_redraw(&action)
    }

    /// Key for the conflict dialog, when one is pending. Returns whether
    /// it was consumed.
    pub(super) fn sftp_conflict_key(
        &mut self,
        key: terminus_ui::components::overlay::DialogKey,
    ) -> bool {
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        match files_view::conflict_key(session, &mut self.files_view, key) {
            Some(action) => {
                self.files_redraw(&action);
                true
            }
            None => false,
        }
    }
}
