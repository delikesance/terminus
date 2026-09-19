//! `Screen` sftp surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::context::renderable::Cursor;
use crate::hosts;
use rio_window::window::CursorIcon;

impl Screen<'_> {
    /// Resolve password / identity for an SSH host used by SFTP.
    pub(super) fn resolve_sftp_auth(
        &self,
        host: &hosts::HostRow,
    ) -> Result<(Option<String>, Option<(String, Option<String>)>), String> {
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

    /// Open the dual-pane SFTP browser for `host_id` on the current leaf pane.
    pub fn open_sftp_pane(&mut self, host_id: &str) -> Result<(), String> {
        let host = self.sftp_host_row(host_id)?;
        let (password, identity) = self.resolve_sftp_auth(&host)?;
        let wake = self.sftp_wake.clone();

        if let Some(prev) = self.sftp.take() {
            prev.close();
        }

        let session = crate::sftp_ui::ActiveSftp::start(&host, password, identity, wake)?;

        {
            let grid = self.context_manager.current_grid_mut();
            if let Some(item) = grid.current_item_mut() {
                item.set_pane_kind(crate::layout::PaneKind::Sftp);
            }
        }

        self.sftp = Some(session);
        self.mark_dirty();
        Ok(())
    }

    /// Open `host_id` on the other SFTP pane (left). Starts SFTP if none is open.
    pub fn open_sftp_other_pane(&mut self, host_id: &str) -> Result<(), String> {
        if self.sftp.is_none() {
            return self.open_sftp_pane(host_id);
        }
        let host = self.sftp_host_row(host_id)?;
        let (password, identity) = self.resolve_sftp_auth(&host)?;
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
        let grid = self.context_manager.current_grid_mut();
        if let Some(item) = grid.current_item_mut() {
            item.set_pane_kind(crate::layout::PaneKind::Terminal);
        }
        self.mark_dirty();
    }

    /// Logical bounds of the current leaf for SFTP layout / hit-testing.
    pub fn sftp_bounds(&self) -> Option<terminus_ui::Rect> {
        let grid = self.context_manager.current_grid();
        let item = grid.current_item()?;
        if item.pane_kind != crate::layout::PaneKind::Sftp {
            return None;
        }
        let scale = self.sugarloaf.scale_factor().max(1.0);
        let margin = grid.get_scaled_margin();
        let r = item.layout_rect;
        Some(terminus_ui::Rect::new(
            (r[0] + margin.left) / scale,
            (r[1] + margin.top) / scale,
            r[2] / scale,
            r[3] / scale,
        ))
    }

    pub fn handle_sftp_click(
        &mut self,
        x: f32,
        y: f32,
        double: bool,
    ) -> terminus_ui::SftpClickResult {
        let Some(bounds) = self.sftp_bounds() else {
            return terminus_ui::SftpClickResult::Miss;
        };
        let Some(session) = self.sftp.as_mut() else {
            return terminus_ui::SftpClickResult::Miss;
        };
        let layout = terminus_ui::SftpPaneLayout::from_state(bounds, &session.state);
        let hit = layout.hit_test(&session.state, x, y);
        let result = session.handle_click(&layout, x, y, double);
        if matches!(result, terminus_ui::SftpClickResult::Handled) {
            session.drag_press(hit, x, y);
        }
        match result {
            terminus_ui::SftpClickResult::Close => {
                self.close_sftp_pane();
            }
            terminus_ui::SftpClickResult::Handled => {
                self.mark_dirty();
            }
            terminus_ui::SftpClickResult::Miss => {}
        }
        result
    }

    pub fn handle_sftp_drag_move(&mut self, x: f32, y: f32) -> bool {
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        if session.drag_move(x, y) {
            self.mark_dirty();
            true
        } else {
            false
        }
    }

    pub fn handle_sftp_drag_release(&mut self, x: f32, y: f32) -> bool {
        let Some(bounds) = self.sftp_bounds() else {
            if let Some(session) = self.sftp.as_mut() {
                session.cancel_drag();
            }
            return false;
        };
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        let layout = terminus_ui::SftpPaneLayout::from_state(bounds, &session.state);
        if session.drag_release(&layout, x, y) {
            self.mark_dirty();
            true
        } else {
            false
        }
    }

    pub fn handle_sftp_hover(&mut self, x: f32, y: f32) -> bool {
        let Some(bounds) = self.sftp_bounds() else {
            return false;
        };
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        let layout = terminus_ui::SftpPaneLayout::from_state(bounds, &session.state);
        if session.handle_hover(&layout, x, y) {
            self.mark_dirty();
            true
        } else {
            false
        }
    }

    /// Cursor over the SFTP pane. `None` when the pointer is outside it.
    pub fn sftp_cursor_at(&self, x: f32, y: f32) -> Option<CursorIcon> {
        let bounds = self.sftp_bounds()?;
        let session = self.sftp.as_ref()?;
        let layout = terminus_ui::SftpPaneLayout::from_state(bounds, &session.state);
        let hit = layout.hit_test(&session.state, x, y);
        if matches!(hit, terminus_ui::SftpHit::Miss) {
            return None;
        }
        if terminus_ui::hit_is_text(&hit) {
            Some(CursorIcon::Text)
        } else if terminus_ui::hit_is_clickable(&hit) {
            Some(CursorIcon::Pointer)
        } else {
            Some(CursorIcon::Default)
        }
    }

    pub fn handle_sftp_scroll(&mut self, x: f32, y: f32, delta_y: f32) -> bool {
        let Some(bounds) = self.sftp_bounds() else {
            return false;
        };
        let Some(session) = self.sftp.as_mut() else {
            return false;
        };
        let layout = terminus_ui::SftpPaneLayout::from_state(bounds, &session.state);
        if session.scroll(&layout, x, y, delta_y) {
            self.mark_dirty();
            true
        } else {
            false
        }
    }
}
