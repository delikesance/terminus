use super::Application;
use crate::event::ClickState;
use rio_window::event::MouseButton;
use rio_window::event_loop::ActiveEventLoop;
use std::time::Instant;
use terminus_ui::chrome::ChromeAction;

impl Application<'_> {
    pub(super) fn handle_mouse_press(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        button: MouseButton,
    ) {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return;
        };
        // Calculate time since the last click to handle double/triple clicks.
        // Do this early so island clicks can use the click state
        let now = Instant::now();
        let elapsed = now - route.window.screen.mouse.last_click_timestamp;
        route.window.screen.mouse.last_click_timestamp = now;

        let threshold = crate::constants::MULTI_CLICK_THRESHOLD;
        let mouse = &route.window.screen.mouse;
        route.window.screen.mouse.click_state = match mouse.click_state {
            // Reset click state if button has changed.
            _ if button != mouse.last_click_button => {
                route.window.screen.mouse.last_click_button = button;
                ClickState::Click
            }
            ClickState::Click if elapsed < threshold => ClickState::DoubleClick,
            ClickState::DoubleClick if elapsed < threshold => ClickState::TripleClick,
            _ => ClickState::Click,
        };

        let chrome_press = route.window.screen.take_chrome_press();

        if let MouseButton::Left = button {
            if self.press_left(event_loop, window_id, chrome_press) {
                return;
            }
        } else if let MouseButton::Right = button {
            if self.press_right(window_id, chrome_press) {
                return;
            }
        }

        self.press_terminal(window_id, button);
    }

    pub(super) fn press_left(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        chrome_press: Option<crate::screen::ChromePress>,
    ) -> bool {
        if self.press_left_modals(window_id) {
            return true;
        }

        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return false;
        };
        let scale = route.window.screen.sugarloaf.scale_factor();
        let mx = route.window.screen.mouse.x as f32 / scale;
        let my = route.window.screen.mouse.y as f32 / scale;
        match route.window.screen.chrome_press(mx, my) {
            ChromeAction::Ignored => {
                // Click outside the drawer: drop search focus
                // so keys reach the terminal again.
                let panel = &mut route.window.screen.chrome.panel;
                if panel.filter_focused || panel.new_group_focused {
                    panel.filter_focused = false;
                    panel.new_group_focused = false;
                    route.request_overlay_redraw();
                }
            }
            other => {
                self.chrome_action_1(event_loop, window_id, chrome_press, other);
                return true;
            }
        }

        self.press_left_content(window_id, chrome_press)
    }

    pub(super) fn press_left_modals(
        &mut self,
        window_id: rio_backend::event::WindowId,
    ) -> bool {
        let Some(route) = self.router.routes.get_mut(&window_id) else {
            return false;
        };
        // Vault unlock sits above every other chrome modal.
        if route.window.screen.chrome.vault_unlock_is_open() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            let action = route.window.screen.chrome_press(mx, my);
            if matches!(action, ChromeAction::SubmitVaultUnlock) {
                route.window.screen.submit_vault_unlock();
            }
            route.request_overlay_redraw();
            return true;
        }

        // The add-host editor is modal: while it is
        // open every press belongs to it — a click on
        // the dialog is swallowed, a click on the
        // scrim dismisses — and nothing behind it
        // (panel borders, tabs, the shell) may react.
        if route.window.screen.chrome.add_host_is_open() {
            let scale = route.window.screen.sugarloaf.scale_factor();
            let mx = route.window.screen.mouse.x as f32 / scale;
            let my = route.window.screen.mouse.y as f32 / scale;
            match route.window.screen.chrome_press(mx, my) {
                ChromeAction::SubmitHostForm => {
                    route.window.screen.submit_host_form();
                }
                // The footer "Copy" button: this modal path
                // returns early, so it must copy here.
                ChromeAction::CopyText(text) => {
                    self.router
                        .clipboard
                        .set(rio_backend::clipboard::ClipboardType::Clipboard, text);
                }
                _ => {}
            }
            route.request_overlay_redraw();
            return true;
        }

        // Check if clicking on a panel border to start resize
        if route.window.screen.chrome.shell.view().shows_terminal() {
            let mx = route.window.screen.mouse.x as f32;
            let my = route.window.screen.mouse.y as f32;
            let grid = route.window.screen.context_manager.current_grid();
            if let Some(border) = grid.find_border_at_position(mx, my) {
                let start_pos = match border.direction {
                    crate::layout::BorderDirection::Vertical => mx,
                    crate::layout::BorderDirection::Horizontal => my,
                };
                let size_a = grid.get_panel_size(border.left_or_top, border.direction);
                let size_b =
                    grid.get_panel_size(border.right_or_bottom, border.direction);
                route.window.screen.resize_state = Some(crate::layout::ResizeState {
                    border,
                    start_pos,
                    original_sizes: (size_a, size_b),
                });
                return true;
            }
        }

        if route.window.screen.handle_assistant_click() {
            route.request_redraw();
            return true;
        }

        if route
            .window
            .screen
            .handle_palette_click(&mut self.router.clipboard)
        {
            route.request_redraw();
            return true;
        }

        if route
            .window
            .screen
            .handle_search_click(&mut self.router.clipboard)
        {
            route.request_redraw();
            return true;
        }
        false
    }
}
