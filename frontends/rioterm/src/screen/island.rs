//! `Screen` island surface, split out of `screen/mod.rs`.

use super::{ChromePress, Screen};
use crate::renderer::island;
use crate::renderer::island::{TabStripLayout, CONTEXT_BAR_HEIGHT};
use raw_window_handle::RawWindowHandle;
use rio_backend::clipboard::Clipboard;
use rio_window::event::ElementState;

impl Screen<'_> {
    #[inline]
    pub(super) fn island_tab_layout(&self, num_tabs: usize) -> TabStripLayout {
        if let Some(island) = self.renderer.island.as_ref() {
            if island.cached_layout().len() == num_tabs {
                return island.cached_layout().clone();
            }
            return island::tab_strip_layout(
                self.sugarloaf.window_size().width,
                self.sugarloaf.scale_factor(),
                num_tabs,
                island.max_tab_width,
            );
        }
        let max_tab_width = rio_backend::config::navigation::default_max_tab_width();
        island::tab_strip_layout(
            self.sugarloaf.window_size().width,
            self.sugarloaf.scale_factor(),
            num_tabs,
            max_tab_width,
        )
    }

    pub fn start_window_drag(&mut self, window: &rio_window::window::Window) {
        self.mouse.left_button_state = ElementState::Released;
        let _ = window.drag_window();
    }

    pub(super) fn is_close_press_tail(&self, x_unscaled: f32) -> bool {
        const CLOSE_TAIL_SLOP: f32 = 16.0;
        self.last_close_press.is_some_and(|(at, press_x)| {
            at.elapsed() <= crate::constants::MULTI_CLICK_THRESHOLD
                && (x_unscaled - press_x).abs() <= CLOSE_TAIL_SLOP
        })
    }

    pub(super) fn apply_tab_hover(&mut self, tab: Option<usize>, on_close: bool) -> bool {
        let changed = self
            .renderer
            .island
            .as_mut()
            .is_some_and(|island| island.set_tab_hover(tab, on_close));
        if changed {
            self.mark_dirty();
        }
        changed
    }

    #[cfg(target_os = "windows")]
    pub(super) fn apply_window_control_hover(
        &mut self,
        hover: Option<crate::renderer::window_controls::WindowControl>,
    ) -> bool {
        let changed = self
            .renderer
            .island
            .as_mut()
            .is_some_and(|island| island.set_window_control_hover(hover));
        if changed {
            self.mark_dirty();
        }
        changed
    }

    #[cfg(target_os = "windows")]
    pub(super) fn apply_window_control(
        &mut self,
        window: &rio_window::window::Window,
        control: crate::renderer::window_controls::WindowControl,
    ) {
        use crate::renderer::window_controls::WindowControl;
        match control {
            WindowControl::Minimize => {
                window.set_minimized(true);
            }
            WindowControl::Maximize => {
                let next = !window.is_maximized();
                window.set_maximized(next);
                self.window_maximized = next;
            }
            WindowControl::Close => {
                // Post WM_CLOSE so the native confirm-before-quit path
                // (MessageBoxW on the last window) still runs.
                request_windows_close(window);
            }
        }
    }

    pub fn update_close_button_hover(&mut self, mouse_x: f64, mouse_y: f64) -> bool {
        let num_tabs = self.context_manager.len();
        let scale_factor = self.sugarloaf.scale_factor();
        let x_unscaled = mouse_x as f32 / scale_factor;
        let in_band = mouse_y <= (CONTEXT_BAR_HEIGHT * scale_factor) as f64;

        #[cfg(target_os = "windows")]
        let control_changed = {
            let y_unscaled = mouse_y as f32 / scale_factor;
            let hover = if self.renderer.navigation.is_enabled() && in_band {
                let logical_w = self.sugarloaf.window_size().width / scale_factor;
                crate::renderer::window_controls::hit_test(
                    logical_w, x_unscaled, y_unscaled,
                )
            } else {
                None
            };
            self.apply_window_control_hover(hover)
        };
        #[cfg(not(target_os = "windows"))]
        let control_changed = false;

        let layout = self.island_tab_layout(num_tabs);
        let hovered_tab = if num_tabs >= 1
            && self.renderer.navigation.island_visible(num_tabs)
            && in_band
        {
            island::tab_index_at(&layout, x_unscaled, num_tabs)
        } else {
            None
        };
        let on_close = hovered_tab.is_some_and(|tab| {
            !self.context_manager.is_pinned(tab)
                && island::close_button_hit(&layout, tab, x_unscaled)
        });

        self.apply_tab_hover(hovered_tab, on_close) || control_changed
    }

    #[inline]
    pub fn clear_close_button_hover(&mut self) -> bool {
        #[cfg(target_os = "windows")]
        let control = self.apply_window_control_hover(None);
        #[cfg(not(target_os = "windows"))]
        let control = false;
        self.apply_tab_hover(None, false) || control
    }

    pub fn handle_island_click(
        &mut self,
        window: &rio_window::window::Window,
        clipboard: &mut Clipboard,
        is_right_click: bool,
        chrome_press: Option<ChromePress>,
    ) -> bool {
        // Only handle if navigation is enabled
        if !self.renderer.navigation.is_enabled() {
            return false;
        }

        let mouse_x = self.mouse.x;
        let mouse_y = self.mouse.y;

        let scale_factor = self.sugarloaf.scale_factor();
        let island_height_px = (CONTEXT_BAR_HEIGHT * scale_factor) as f64;

        let window_width = self.sugarloaf.window_size().width;
        let num_tabs = self.context_manager.len();
        let island_visible = self.renderer.navigation.island_visible(num_tabs);

        if let Some(ref mut island) = self.renderer.island {
            if island.is_color_picker_open() {
                let consumed = island.handle_color_picker_click(
                    mouse_x as f32,
                    mouse_y as f32,
                    scale_factor,
                    window_width,
                    num_tabs,
                    &mut self.context_manager,
                );
                if consumed {
                    self.mark_dirty();
                    return true;
                }
            }
        }

        // Check if click is within island height
        if mouse_y > island_height_px {
            // Close picker if clicking outside
            if let Some(ref mut island) = self.renderer.island {
                if island.is_color_picker_open() {
                    island.close_color_picker(&mut self.context_manager);
                    self.mark_dirty();
                }
            }
            return false;
        }

        let mouse_x_unscaled = mouse_x as f32 / scale_factor;

        // Windows custom caption buttons own the right edge of the band —
        // handle them before tab / chrome drag logic.
        #[cfg(target_os = "windows")]
        {
            let mouse_y_unscaled = mouse_y as f32 / scale_factor;
            let window_width_logical = window_width / scale_factor;
            if let Some(control) = crate::renderer::window_controls::hit_test(
                window_width_logical,
                mouse_x_unscaled,
                mouse_y_unscaled,
            ) {
                if !is_right_click {
                    self.apply_window_control(window, control);
                }
                return true;
            }
        }

        // Logo / + / search sit in the title bar outside the tab strip.
        {
            let mouse_y_unscaled = mouse_y as f32 / scale_factor;
            let window_width_logical = window_width / scale_factor;
            if let Some(action) = island::title_bar_hit(
                window_width_logical,
                mouse_x_unscaled,
                mouse_y_unscaled,
            ) {
                if !is_right_click {
                    match action {
                        island::TitleBarAction::Logo => {}
                        island::TitleBarAction::NewTab => {
                            self.create_tab(clipboard);
                        }
                        island::TitleBarAction::Search => {
                            self.chrome.panel.filter_focused = true;
                            self.chrome.activity.selected = terminus_ui::Section::Servers;
                            self.chrome.activity.collapsed = false;
                            self.chrome.panel_visible = true;
                        }
                    }
                    self.mark_dirty();
                }
                return true;
            }
        }

        // Island isn't painted (hide_if_single + single tab). On macOS and
        // Windows the terminal still starts below this band, so it remains
        // custom window chrome and must keep drag / double-click (and on
        // Windows, the system menu). Linux renders the terminal from the
        // top when the island is hidden.
        if !island_visible {
            // …unless a ×-close just hid the strip (2 tabs → 1 with
            // hide-if-single): the tail press of a double-click on the
            // × would otherwise leak into the terminal as a selection,
            // or start a window drag via the band fallback.
            if self.is_close_press_tail(mouse_x_unscaled) {
                return true;
            }

            if self.renderer.navigation.chrome_band_reserved(num_tabs) {
                // Same contract as the visible island's chrome regions:
                // left starts a drag / validates a double-click, right
                // is consumed without an action. Letting a right-click
                // fall through would act on the first terminal row
                // while the pointer is over window chrome.
                if is_right_click {
                    #[cfg(target_os = "windows")]
                    {
                        window.show_window_menu(rio_window::dpi::PhysicalPosition::new(
                            mouse_x, mouse_y,
                        ));
                    }
                } else {
                    self.on_chrome_press(window, chrome_press);
                }
                return true;
            }

            return false;
        }

        let layout = self.island_tab_layout(num_tabs);
        let x_in_tabs = mouse_x_unscaled - layout.left_margin;

        if !is_right_click
            && self.is_close_press_tail(mouse_x_unscaled)
            && !island::close_button_hit(
                &layout,
                self.context_manager.current_index(),
                mouse_x_unscaled,
            )
        {
            return true;
        }

        // Left-aligned pills: empty band (and past the last pill) is window chrome.
        let past_last_tab = x_in_tabs >= layout.tabs_width();
        if x_in_tabs < 0.0 || past_last_tab {
            if is_right_click {
                #[cfg(target_os = "windows")]
                {
                    window.show_window_menu(rio_window::dpi::PhysicalPosition::new(
                        mouse_x, mouse_y,
                    ));
                }
            } else {
                self.on_chrome_press(window, chrome_press);
            }
            return true;
        }

        let clicked_tab = island::tab_index_at(&layout, mouse_x_unscaled, num_tabs)
            .unwrap_or(num_tabs.saturating_sub(1));

        #[cfg(target_os = "macos")]
        if !is_right_click && self.modifiers.state().super_key() {
            if self.allow_manual_dragging {
                self.start_window_drag(window);
            }
            return true;
        }

        // Right-click or Control + left-click → toggle color picker for that tab
        if is_right_click || self.modifiers.state().control_key() {
            // Get current displayed title for the rename input
            let current_title = self
                .context_manager
                .title(clicked_tab)
                .and_then(|t| {
                    if !t.content.is_empty() {
                        Some(t.content.clone())
                    } else {
                        t.extra.as_ref().and_then(|e| {
                            if !e.program.is_empty() {
                                Some(e.program.clone())
                            } else {
                                None
                            }
                        })
                    }
                })
                .unwrap_or_else(|| String::from("~"));
            if let Some(ref mut island) = self.renderer.island {
                island.toggle_color_picker(
                    clicked_tab,
                    &current_title,
                    &mut self.context_manager,
                );
                self.mark_dirty();
            }
            return true;
        }

        if num_tabs == 1 {
            // Pill is left-aligned; empty band still drags the window.
            if island::tab_index_at(&layout, mouse_x_unscaled, 1).is_none() {
                self.on_chrome_press(window, chrome_press);
            }
            return true;
        }

        if island::close_button_hit(&layout, clicked_tab, mouse_x_unscaled) {
            if self.context_manager.is_pinned(clicked_tab) {
                return true;
            }
            self.stop_hint_mode_if_active();
            self.last_close_press = Some((std::time::Instant::now(), mouse_x_unscaled));
            self.close_tab_at(clicked_tab, clipboard);
            return true;
        }

        if clicked_tab != self.context_manager.current_index() {
            self.stop_hint_mode_if_active();
            self.cancel_search(clipboard);
            self.clear_selection();
            let old_index = self.context_manager.current_index();
            self.context_manager.set_current(clicked_tab);
            let new_index = self.context_manager.current_index();
            self.switch_visible_context(old_index, new_index);

            self.mark_dirty();
        }

        if let Some(ref mut island) = self.renderer.island {
            if island.is_color_picker_open() {
                island.close_color_picker(&mut self.context_manager);
                self.mark_dirty();
            }
        }

        let can_reorder = self.allow_manual_dragging;
        if num_tabs > 1 && can_reorder {
            if let Some(ref mut island) = self.renderer.island {
                let tab_left = layout.slot_x(clicked_tab);
                island.start_drag(
                    clicked_tab,
                    mouse_x_unscaled - tab_left,
                    mouse_x_unscaled,
                );
            }
        }

        true
    }

    pub fn handle_tab_drag_move(&mut self, x_unscaled: f32) {
        let num_tabs = self.context_manager.len();

        // A tab closed mid-drag invalidates the armed indices.
        if num_tabs < 2 {
            if let Some(ref mut island) = self.renderer.island {
                island.cancel_drag();
            }
            return;
        }

        let layout = self.island_tab_layout(num_tabs);

        let (drag_idx, center) = match self.renderer.island.as_mut() {
            Some(island) => {
                if !island.update_drag(x_unscaled) {
                    // armed but still below the drag threshold
                    return;
                }
                match (island.drag_index(), island.drag_center(&layout)) {
                    (Some(idx), Some(center)) => (idx, center),
                    _ => return,
                }
            }
            None => return,
        };

        let old_index = self.context_manager.current_index();
        if drag_idx != old_index {
            if let Some(ref mut island) = self.renderer.island {
                island.cancel_drag();
            }
            self.mark_dirty();
            return;
        }

        let target = island::tab_index_at(&layout, center, num_tabs).unwrap_or(old_index);
        if target != old_index {
            self.context_manager.move_current_tab_to(target);
            let new_index = self.context_manager.current_index();
            self.switch_visible_context(old_index, new_index);
            if let Some(ref mut island) = self.renderer.island {
                let moved_w = layout.width_at(old_index);
                island.remap_tab_move(old_index, new_index, moved_w);
            }
        }
        self.mark_dirty();
    }

    pub fn handle_tab_drag_release(&mut self) -> bool {
        let num_tabs = self.context_manager.len();
        let layout = self.island_tab_layout(num_tabs);

        if let Some(ref mut island) = self.renderer.island {
            let started = island.drag_index().is_some();
            island.end_drag(&layout);
            if started {
                self.mark_dirty();
            }
            return started;
        }
        false
    }
}

/// Ask Windows to close the window via `WM_CLOSE`, so the native
/// confirm-before-quit MessageBox still runs for the last window.
#[cfg(target_os = "windows")]
pub(super) fn request_windows_close(window: &rio_window::window::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    unsafe {
        PostMessageW(win32.hwnd.get() as _, WM_CLOSE, 0, 0);
    }
}
