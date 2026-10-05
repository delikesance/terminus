//! `Screen` sftp surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::hosts;
use crate::renderer::views::files::{self as files_view, FilesAction};
use rio_window::window::CursorIcon;
use terminus_ui::views::files::FilesHit;

/// SFTP credentials for a host: `(password, (private key PEM, passphrase))`.
pub(super) type SftpAuth = (Option<String>, Option<(String, Option<String>)>);

impl Screen<'_> {
    /// Resolve password / identity for an SSH host used by SFTP.
    pub(super) fn resolve_sftp_auth(
        &self,
        host: &hosts::HostRow,
    ) -> Result<SftpAuth, String> {
        let password = if host.auth_method == "password" {
            match self.host_store.resolve_host_password(&host.id)? {
                Some(pw) => Some(pw),
                None => {
                    return Err(
                        "No saved password — edit the host and save one (vault unlocked)"
                            .into(),
                    );
                }
            }
        } else {
            None
        };

        let identity = if host.auth_method == "password" || host.auth_method == "gssapi" {
            None
        } else {
            match self.host_store.resolve_host_identity(&host.id)? {
                Some(pair) => Some(pair),
                None => {
                    return Err(
                        "No saved SSH key — edit the host and select one (Settings → Managed SSH Keys)"
                            .into(),
                    );
                }
            }
        };

        Ok((password, identity))
    }

    /// Open the dual-pane SFTP browser for `host_id` on the current leaf pane.
    /// Resolve SFTP credentials; a locked vault opens the unlock modal and
    /// retries once unlocked (`Ok(None)`), like opening a shell does.
    fn resolve_sftp_auth_or_unlock(
        &mut self,
        host: &crate::hosts::HostRow,
        other_pane: bool,
    ) -> Result<Option<SftpAuth>, String> {
        match self.resolve_sftp_auth(host) {
            Ok(auth) => Ok(Some(auth)),
            Err(err) if err.contains("Unlock the vault") => {
                self.open_vault_unlock_for(terminus_ui::PendingVaultAction::OpenSftp {
                    host_id: host.id.clone(),
                    other_pane,
                });
                Ok(None)
            }
            Err(err) => Err(err),
        }
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

    pub fn open_sftp_pane(&mut self, host_id: &str) -> Result<(), String> {
        let host = self.sftp_host_row(host_id)?;
        let Some((password, identity)) =
            self.resolve_sftp_auth_or_unlock(&host, false)?
        else {
            return Ok(());
        };
        let wake = self.sftp_wake.clone();

        if let Some(prev) = self.sftp.take() {
            prev.close();
        }

        let session = crate::sftp_ui::ActiveSftp::start(&host, password, identity, wake)?;

        // The browser lives in the Files view now; the terminal pane stays
        // a terminal underneath.
        self.sftp = Some(session);
        self.show_view(terminus_ui::shell::WorkspaceView::Files);
        self.mark_dirty();
        Ok(())
    }

    /// Open `host_id` on the other SFTP pane (left). Starts SFTP if none is open.
    pub fn open_sftp_other_pane(&mut self, host_id: &str) -> Result<(), String> {
        if self.sftp.is_none() {
            return self.open_sftp_pane(host_id);
        }
        let host = self.sftp_host_row(host_id)?;
        let Some((password, identity)) = self.resolve_sftp_auth_or_unlock(&host, true)?
        else {
            return Ok(());
        };
        let session = self.sftp.as_mut().expect("checked above");
        session.open_other_host(&host, password, identity)?;
        self.mark_dirty();
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
