use super::Application;
use rio_window::event_loop::ActiveEventLoop;

impl Application<'_> {
    /// Register a system-wide hotkey for every `ToggleQuake` binding
    /// in the config, so the quake window opens while Rio is
    /// unfocused. No-op when quake is not bound; pure Wayland has no
    /// global hotkey API, the compositor keybinding + a regular
    /// binding cover it there.
    pub(super) fn setup_quake_hotkey(&mut self) {
        // Drop any previous manager first: registering a chord the old
        // manager still holds fails on Windows and X11.
        self.global_hotkey = None;
        self.global_hotkey = crate::global_hotkey::setup(
            self.event_proxy.clone(),
            &self.config.bindings.keys,
        );
    }

    /// The monitor the quake window should drop down on: the one
    /// under the mouse cursor where the platform can tell us, the
    /// primary monitor otherwise.
    pub(super) fn quake_monitor(
        &self,
        event_loop: &ActiveEventLoop,
    ) -> Option<rio_window::monitor::MonitorHandle> {
        event_loop
            .cursor_monitor()
            .or_else(|| event_loop.primary_monitor())
    }

    /// Anchor the quake window to the top of `monitor`, horizontally
    /// centered, sized by the configured percentages, then show it.
    pub(super) fn show_quake_window(
        &mut self,
        id: rio_backend::event::WindowId,
        event_loop: &ActiveEventLoop,
    ) {
        #[cfg(target_os = "macos")]
        {
            self.quake_previous_app =
                rio_window::platform::macos::frontmost_application_pid();
        }

        let Some(route) = self.router.routes.get(&id) else {
            return;
        };
        let window = &route.window.winit_window;
        if let Some(monitor) = self.quake_monitor(event_loop) {
            let msize = monitor.size();
            let mpos = monitor.position();
            let width = (msize.width as f32
                * self.config.window.quake_width_percentage.clamp(0.1, 1.0))
                as u32;
            let height = (msize.height as f32
                * self.config.window.quake_height_percentage.clamp(0.1, 1.0))
                as u32;
            let x = mpos.x + (msize.width.saturating_sub(width) / 2) as i32;
            #[cfg(target_os = "macos")]
            {
                let scale = monitor.scale_factor();
                let size: rio_window::dpi::LogicalSize<f64> =
                    rio_window::dpi::PhysicalSize::new(width, height).to_logical(scale);
                let pos: rio_window::dpi::LogicalPosition<f64> =
                    rio_window::dpi::PhysicalPosition::new(x, mpos.y).to_logical(scale);
                let _ = window.request_inner_size(size);
                window.set_outer_position(pos);
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = window.request_inner_size(rio_window::dpi::PhysicalSize::new(
                    width, height,
                ));
                window.set_outer_position(rio_window::dpi::PhysicalPosition::new(
                    x, mpos.y,
                ));
            }
        }
        window.set_visible(true);
        window.focus_window();
    }

    /// Show, focus or hide the quake window; create it on first use.
    pub(super) fn toggle_quake_window(&mut self, event_loop: &ActiveEventLoop) {
        let quake_id = self
            .router
            .quake_window_id
            .filter(|id| self.router.routes.contains_key(id));

        let Some(id) = quake_id else {
            self.router.quake_window_id = None;
            self.router.create_quake_window(
                event_loop,
                self.event_proxy.clone(),
                &self.config,
            );
            if let Some(id) = self.router.quake_window_id {
                self.show_quake_window(id, event_loop);
            }
            return;
        };

        if let Some(route) = self.router.routes.get_mut(&id) {
            let window = &route.window.winit_window;
            let visible = window.is_visible().unwrap_or(true);
            if !visible {
                self.show_quake_window(id, event_loop);
            } else if window.has_focus() {
                window.set_visible(false);
                #[cfg(target_os = "macos")]
                if let Some(pid) = self.quake_previous_app.take() {
                    rio_window::platform::macos::activate_application(pid);
                }
            } else {
                self.show_quake_window(id, event_loop);
            }
        }
    }
}
