use super::Application;
use crate::event::ClickState;
use rio_window::event_loop::ActiveEventLoop;
use terminus_ui::chrome::ChromeAction;

impl Application<'_> {
    pub(super) fn chrome_action_1(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        chrome_press: Option<crate::screen::ChromePress>,
        action: ChromeAction,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        match action {
            ChromeAction::OpenPalette => {
                route.window.screen.open_palette();
                route.request_redraw();
            }
            ChromeAction::ViewChanged(_) => {
                route.window.screen.clear_selection();
                route.window.screen.mark_dirty();
                route.request_redraw();
            }
            ChromeAction::Split { down } => {
                if down {
                    route.window.screen.split_down();
                } else {
                    route.window.screen.split_right();
                }
                route.request_redraw();
            }
            ChromeAction::WindowDrag => {
                route
                    .window
                    .screen
                    .on_header_press(&route.window.winit_window, chrome_press);
            }
            // Not on Windows: its Close posts WM_CLOSE,
            // which arrives as `CloseRequested`.
            #[cfg(not(target_os = "windows"))]
            ChromeAction::WindowControl(terminus_ui::shell::WindowButton::Close) => {
                self.close_window(event_loop, window_id);
            }
            ChromeAction::WindowControl(button) => {
                route
                    .window
                    .screen
                    .apply_header_control(&route.window.winit_window, button);
                route.request_redraw();
            }
            ChromeAction::OpenAddSnippet => {
                route.request_overlay_redraw();
            }
            ChromeAction::SubmitAddSnippet(_values) => {
                route.window.screen.submit_snippet_form();
                route.request_overlay_redraw();
            }
            ChromeAction::DeleteSnippet(id) => {
                route.window.screen.host_store.delete_snippet(id);
                route.request_overlay_redraw();
            }
            ChromeAction::RunSnippet(cmd) => {
                // Non-bracketed write + CR so shells treat it as typed Enter.
                let line = format!("{cmd}\r");
                route.window.screen.paste(&line, false);
                route.request_overlay_redraw();
            }
            other => self.chrome_action_2(event_loop, window_id, other),
        }
    }

    pub(super) fn chrome_action_2(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        action: ChromeAction,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        let scale = route.window.screen.sugarloaf.scale_factor();
        let mx = route.window.screen.mouse.x as f32 / scale;
        let my = route.window.screen.mouse.y as f32 / scale;
        match action {
            ChromeAction::AddHost => {
                route.window.screen.chrome.open_add_host();
                route.request_overlay_redraw();
            }
            ChromeAction::SubmitHostForm => {
                route.window.screen.submit_host_form();
                route.request_overlay_redraw();
            }
            ChromeAction::SubmitVaultUnlock => {
                route.window.screen.submit_vault_unlock();
                route.request_overlay_redraw();
            }
            ChromeAction::NewGroup => {
                route.request_overlay_redraw();
            }
            ChromeAction::CreateGroup => {
                let name = route
                    .window
                    .screen
                    .chrome
                    .panel
                    .new_group_name
                    .value
                    .trim()
                    .to_string();
                if !name.is_empty() {
                    route.window.screen.host_store.create_group(&name);
                    route.window.screen.chrome.panel.close_new_group_form();
                }
                route.request_overlay_redraw();
            }
            ChromeAction::CancelNewGroup => {
                route.request_overlay_redraw();
            }
            ChromeAction::FocusNewGroup => {
                route.request_overlay_redraw();
            }
            ChromeAction::ToggleGroup(id) => {
                // Apply to the current window immediately for
                // responsiveness. Determine the target state after
                // the toggle so we know what to persist.
                route.window.screen.chrome.toggle_group_collapsed(&id);
                let target = route
                    .window
                    .screen
                    .chrome
                    .panel
                    .collapsed_groups
                    .contains(&id);
                let _ = route.window.screen.pump_chrome();
                route.request_overlay_redraw();
                // Defer cross-window sync and persistence to
                // `about_to_wait`. At that point `self.router.routes`
                // is freely accessible (no active route borrow).
                self.pending_group_toggle = Some((id, target));
            }
            ChromeAction::FocusSearch => {
                route.window.screen.chrome.panel.filter_focused = true;
                route.request_overlay_redraw();
            }
            // The row was already selected by
            // `chrome_press`; this opens its
            // session in a new tab. Anything that
            // stops it (no `wsl.exe`, no interop,
            // a shell that will not spawn) is
            // reported on the panel's one error
            // line rather than swallowed.
            ChromeAction::OpenHost(id) => {
                match route
                    .window
                    .screen
                    .open_host_session(&id, &mut self.router.clipboard)
                {
                    Ok(()) => {
                        route.window.screen.chrome.panel.error = None;
                        route.window.screen.chrome.panel.notice = None;
                    }
                    Err(err) => {
                        route.window.screen.chrome.panel.error = Some(err);
                    }
                }
                route.request_overlay_redraw();
            }
            ChromeAction::OpenSession(tab_index) => {
                // The first click focused the tab;
                // a double-click on its pill renames it.
                let on_pill = route.window.screen.chrome.shell.hit_test(mx, my)
                    == Some(terminus_ui::shell::ShellHit::Pill(tab_index));
                let double = on_pill
                    && matches!(
                        route.window.screen.mouse.click_state,
                        ClickState::DoubleClick
                    )
                    && route.window.screen.context_manager.current_index() == tab_index;
                if double {
                    route.window.screen.chrome.shell.begin_rename(tab_index);
                } else {
                    route
                        .window
                        .screen
                        .focus_session(tab_index, &mut self.router.clipboard);
                }
                route.request_overlay_redraw();
            }
            ChromeAction::CloseSession(tab_index) => {
                route
                    .window
                    .screen
                    .close_tab_at(tab_index, &mut self.router.clipboard);
                let _ = route.window.screen.pump_chrome();
                route.request_overlay_redraw();
            }
            ChromeAction::AddHostSession(id) => {
                match route
                    .window
                    .screen
                    .add_host_session(&id, &mut self.router.clipboard)
                {
                    Ok(()) => {
                        route.window.screen.chrome.panel.error = None;
                        route.window.screen.chrome.panel.notice = None;
                    }
                    Err(err) => {
                        route.window.screen.chrome.panel.error = Some(err);
                    }
                }
                let _ = route.window.screen.pump_chrome();
                route.request_overlay_redraw();
            }
            ChromeAction::ToggleHost(_id) => {
                let _ = route.window.screen.pump_chrome();
                route.request_overlay_redraw();
            }
            ChromeAction::SetHostGroup { host_id, group_id } => {
                route
                    .window
                    .screen
                    .host_store
                    .set_host_group(&host_id, group_id.as_deref());
                let _ = route.window.screen.pump_chrome();
                route.request_overlay_redraw();
            }
            ChromeAction::ReorderHost {
                host_id,
                before_host_id,
                before_group_id,
            } => {
                route.window.screen.host_store.reorder_host(
                    &host_id,
                    before_host_id.as_deref(),
                    before_group_id.as_deref(),
                );
                let _ = route.window.screen.pump_chrome();
                route.request_overlay_redraw();
            }
            ChromeAction::ReorderGroup {
                group_id,
                before_group_id,
                before_host_id,
            } => {
                route.window.screen.host_store.reorder_group(
                    &group_id,
                    before_group_id.as_deref(),
                    before_host_id.as_deref(),
                );
                let _ = route.window.screen.pump_chrome();
                route.request_overlay_redraw();
            }
            other => self.chrome_action_3(event_loop, window_id, other),
        }
    }
}
