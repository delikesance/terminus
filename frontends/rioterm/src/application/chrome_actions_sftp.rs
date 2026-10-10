use super::Application;
use rio_window::event_loop::ActiveEventLoop;
use terminus_ui::chrome::ChromeAction;

impl Application<'_> {
    pub(super) fn chrome_action_4(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        action: ChromeAction,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        match action {
            ChromeAction::OpenSftp(id) => {
                match route.window.screen.open_sftp_pane(&id) {
                    Ok(()) => {
                        // Overlay redraw: a locked vault opens the unlock modal instead.
                        route.request_overlay_redraw();
                    }
                    Err(err) => {
                        route.window.screen.chrome.panel.notice = Some(err);
                        route.request_overlay_redraw();
                    }
                }
            }
            ChromeAction::OpenSftpOtherPane(id) => {
                match route.window.screen.open_sftp_other_pane(&id) {
                    Ok(()) => {
                        // Overlay redraw: a locked vault opens the unlock modal instead.
                        route.request_overlay_redraw();
                    }
                    Err(err) => {
                        route.window.screen.chrome.panel.notice = Some(err);
                        route.request_overlay_redraw();
                    }
                }
            }
            ChromeAction::SftpNewFolder => {
                if let Some(s) = route.window.screen.sftp.as_mut() {
                    s.begin_mkdir_focused();
                    route.window.screen.mark_dirty();
                    route.request_redraw();
                }
            }
            ChromeAction::SftpRename => {
                if let Some(s) = route.window.screen.sftp.as_mut() {
                    let _ = s.begin_rename_focused();
                    route.window.screen.mark_dirty();
                    route.request_redraw();
                }
            }
            ChromeAction::SftpDelete => {
                let screen = &mut route.window.screen;
                let prompt = screen
                    .sftp
                    .as_ref()
                    .and_then(crate::sftp_ui::ActiveSftp::delete_prompt);
                if let Some(prompt) = prompt {
                    screen.chrome.open_confirm(prompt);
                    screen.mark_dirty();
                    route.request_redraw();
                }
            }
            ChromeAction::SftpDeleteConfirmed => {
                if let Some(s) = route.window.screen.sftp.as_mut() {
                    s.remove_focused();
                    route.window.screen.mark_dirty();
                    route.request_redraw();
                }
            }
            ChromeAction::SftpTransfer => {
                if let Some(s) = route.window.screen.sftp.as_mut() {
                    s.transfer_selected();
                    route.window.screen.mark_dirty();
                    route.request_redraw();
                }
            }
            ChromeAction::SftpEdit => {
                if let Some(s) = route.window.screen.sftp.as_mut() {
                    s.edit_selected();
                    route.window.screen.mark_dirty();
                    route.request_redraw();
                }
            }
            ChromeAction::SftpOpen => {
                if let Some(s) = route.window.screen.sftp.as_mut() {
                    s.open_selected_dir();
                    route.window.screen.mark_dirty();
                    route.request_redraw();
                }
            }
            ChromeAction::SftpRefresh => {
                if let Some(s) = route.window.screen.sftp.as_mut() {
                    s.refresh_focused();
                    route.window.screen.mark_dirty();
                    route.request_redraw();
                }
            }
            ChromeAction::RenameHost(id) => {
                let name = route
                    .window
                    .screen
                    .chrome
                    .panel
                    .rows
                    .iter()
                    .find_map(|r| r.host().filter(|h| h.id == id).map(|h| h.name.clone()))
                    .unwrap_or_default();
                route
                    .window
                    .screen
                    .chrome
                    .panel
                    .begin_rename(id, false, &name);
                route.request_overlay_redraw();
            }
            ChromeAction::RenameGroup(id) => {
                let name =
                    route
                        .window
                        .screen
                        .chrome
                        .panel
                        .rows
                        .iter()
                        .find_map(|r| match r {
                            terminus_ui::sidebar::Row::Group {
                                id: gid, name, ..
                            } if gid == &id => Some(name.clone()),
                            _ => None,
                        })
                        .unwrap_or_default();
                route
                    .window
                    .screen
                    .chrome
                    .panel
                    .begin_rename(id, true, &name);
                route.request_overlay_redraw();
            }
            ChromeAction::CommitRename { id, is_group, name } => {
                if is_group {
                    route.window.screen.host_store.rename_group(&id, &name);
                } else {
                    route.window.screen.host_store.rename_host(&id, &name);
                }
                route.window.screen.chrome.panel.cancel_rename();
                route.request_overlay_redraw();
            }
            ChromeAction::ContextCopy | ChromeAction::ContextPaste => {
                // Terminal/field copy-paste menu items — wired later.
                route.request_overlay_redraw();
            }
            ChromeAction::FocusSqlUri
            | ChromeAction::FocusSqlPassphrase
            | ChromeAction::FocusKeyDraft => {
                route.request_overlay_redraw();
            }
            // Toggling the panel changes the margin
            // `chrome_press` already re-applied; the
            // grid re-layout marks itself dirty, but a
            // pure section switch does not.
            ChromeAction::Consumed => {
                // Panel press (incl. armed host/group drag)
                // must not leave a terminal selection live
                // under the still-held LMB.
                if route.window.screen.chrome.panel.host_drag.is_some() {
                    route.window.screen.clear_selection();
                }
                route.request_overlay_redraw();
            }
            _ => {}
        }
    }
}
