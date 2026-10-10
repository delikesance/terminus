use super::Application;
use crate::event::ClickState;
use rio_window::event::{ElementState, MouseButton};
use terminus_ui::chrome::ChromeAction;

impl Application<'_> {
    pub(super) fn press_left_content(
        &mut self,
        window_id: rio_backend::event::WindowId,
        chrome_press: Option<crate::screen::ChromePress>,
    ) -> bool {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return false;
        };
        // A non-terminal view owns its content area.
        {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            if route.window.screen.view_owns(mx, my) {
                let double = matches!(
                    route.window.screen.mouse.click_state,
                    ClickState::DoubleClick | ClickState::TripleClick
                );
                let input = terminus_ui::screens::ViewInput::Press {
                    x: mx,
                    y: my,
                    double,
                };
                if route
                    .window
                    .screen
                    .view_input(input, &mut self.router.clipboard)
                {
                    route.request_redraw();
                }
                return true;
            }
        }

        let handled_by_island = route.window.screen.handle_island_click(
            &route.window.winit_window,
            &mut self.router.clipboard,
            false,
            chrome_press,
        );

        if handled_by_island {
            route.request_redraw();
            return true;
        }

        if route.window.screen.allow_manual_dragging {
            use crate::renderer::island::CONTEXT_BAR_HEIGHT;
            let scale = route.window.screen.sugarloaf.scale_factor();
            if route.window.screen.mouse.y <= (CONTEXT_BAR_HEIGHT * scale) as f64 {
                route
                    .window
                    .screen
                    .start_window_drag(&route.window.winit_window);
            }
        }

        if route.window.screen.handle_scrollbar_click() {
            route.request_redraw();
            return true;
        }
        false
    }

    pub(super) fn press_right(
        &mut self,
        window_id: rio_backend::event::WindowId,
        chrome_press: Option<crate::screen::ChromePress>,
    ) -> bool {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return false;
        };
        {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;

            // SFTP file/folder context menu first when over the pane.
            if route.window.screen.sftp.is_some()
                && route
                    .window
                    .screen
                    .sftp_bounds()
                    .is_some_and(|b| b.contains(mx, my))
                && route.window.screen.handle_sftp_context_press(mx, my)
            {
                route.request_overlay_redraw();
                return true;
            }

            match route.window.screen.chrome_context_press(mx, my) {
                ChromeAction::OpenAddSnippet => {
                    route.request_overlay_redraw();
                    return true;
                }
                ChromeAction::SubmitAddSnippet(_values) => {
                    route.window.screen.submit_snippet_form();
                    route.request_overlay_redraw();
                    return true;
                }
                ChromeAction::DeleteSnippet(id) => {
                    route.window.screen.host_store.delete_snippet(id);
                    route.request_overlay_redraw();
                    return true;
                }
                ChromeAction::RunSnippet(cmd) => {
                    // Non-bracketed write + CR so shells treat it as typed Enter.
                    let line = format!("{cmd}\r");
                    route.window.screen.paste(&line, false);
                    route.request_overlay_redraw();
                    return true;
                }
                ChromeAction::Ignored => {}
                ChromeAction::Consumed => {
                    route.request_overlay_redraw();
                    return true;
                }
                _other => {
                    route.request_overlay_redraw();
                    return true;
                }
            }
            if route.window.screen.view_owns(mx, my) {
                let input =
                    terminus_ui::screens::ViewInput::ContextPress { x: mx, y: my };
                if route
                    .window
                    .screen
                    .view_input(input, &mut self.router.clipboard)
                {
                    route.request_redraw();
                }
                return true;
            }
        }

        // Frameless Linux window: the compositor's window
        // menu on the empty header.
        if route.window.screen.frameless_header_menu(
            &route.window.winit_window,
            self.router.quake_window_id == Some(window_id),
        ) {
            return true;
        }

        let handled_by_island = route.window.screen.handle_island_click(
            &route.window.winit_window,
            &mut self.router.clipboard,
            true,
            chrome_press,
        );

        if handled_by_island {
            route.request_redraw();
            return true;
        }
        false
    }

    pub(super) fn press_terminal(
        &mut self,
        window_id: rio_backend::event::WindowId,
        button: MouseButton,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        // Dual-pane SFTP: swallow mouse when the active leaf is SFTP.
        if route.window.screen.sftp.is_some() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            let double = matches!(
                route.window.screen.mouse.click_state,
                ClickState::DoubleClick | ClickState::TripleClick
            );
            if button == MouseButton::Left {
                match route.window.screen.handle_sftp_click(mx, my, double) {
                    terminus_ui::SftpClickResult::Close
                    | terminus_ui::SftpClickResult::Handled => {
                        route.request_redraw();
                        return;
                    }
                    terminus_ui::SftpClickResult::Miss => {}
                }
            }
            if button == MouseButton::Left || button == MouseButton::Right {
                // Still consume clicks over the SFTP leaf so they
                // don't reach the underlying PTY.
                if route
                    .window
                    .screen
                    .sftp_bounds()
                    .is_some_and(|b| b.contains(mx, my))
                {
                    route.request_redraw();
                    return;
                }
            }
        }

        // The terminal is covered by another view: nothing in
        // the content area may reach it.
        {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            let shell = &route.window.screen.chrome.shell;
            if !shell.view().shows_terminal() && shell.layout().main.contains(mx, my) {
                return;
            }
        }

        // Always try panel switching first: if the click
        // targets a different panel, switch to it regardless
        // of mouse mode (e.g. neovim capturing clicks).
        //
        // A left click on a highlighted hint bypasses mouse
        // reporting the same way shift does: the hint's mods
        // are held, so the user is following the link, not
        // clicking inside the application. The hint itself is
        // latched for the release handler: re-evaluating
        // there would split a press from its release when
        // the modifier changes mid-click, and the release
        // must know which hint the press landed on.
        let latched = if button == MouseButton::Left {
            route.window.screen.highlighted_hint().cloned()
        } else {
            None
        };
        let hint_click = latched.is_some();
        if button == MouseButton::Left {
            route.window.screen.mouse.hint_click_latched = latched;
        }

        if route.window.screen.select_current_based_on_mouse() {
            route.request_redraw();
        } else if !route.window.screen.modifiers.state().shift_key()
            && !hint_click
            && route.window.screen.mouse_mode()
        {
            // Process mouse press before bindings to update the `click_state`.
            route.window.screen.mouse.click_state = ClickState::None;

            let code = match button {
                MouseButton::Left => 0,
                MouseButton::Middle => 1,
                MouseButton::Right => 2,
                // Can't properly report more than three buttons..
                MouseButton::Back | MouseButton::Forward | MouseButton::Other(_) => {
                    return
                }
            };

            route
                .window
                .screen
                .mouse_report(code, ElementState::Pressed);

            route
                .window
                .screen
                .process_mouse_bindings(button, &mut self.router.clipboard);
        } else {
            // Load mouse point, treating message bar and padding as the closest square.
            let display_offset = route.window.screen.display_offset();

            if let MouseButton::Left = button {
                let pos = route.window.screen.mouse_position(display_offset);
                route
                    .window
                    .screen
                    .on_left_click(pos, &mut self.router.clipboard);
            }

            route.request_redraw();
        }
        route
            .window
            .screen
            .process_mouse_bindings(button, &mut self.router.clipboard);
    }
}
