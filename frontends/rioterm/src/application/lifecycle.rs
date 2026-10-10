use super::Application;
use crate::event::{EventPayload, RioEvent, RioEventType};
use crate::renderer::utils::update_colors_based_on_theme;
use crate::scheduler::{TimerId, Topic};
use rio_backend::clipboard::Clipboard;
use rio_window::event::StartCause;
use rio_window::event_loop::ActiveEventLoop;
use rio_window::event_loop::ControlFlow;
use std::time::Duration;

impl Application<'_> {
    /// GPU textures may be blank after sleep/hibernate while the glyph
    /// caches still point at them: drop the caches and repaint. No-op at
    /// startup, when no route exists yet.
    pub(super) fn rebuild_gpu_caches(&mut self) {
        for route in self.router.routes.values_mut() {
            route.window.screen.on_system_resume();
            route.request_overlay_redraw();
        }
    }

    pub(super) fn handle_new_events(
        &mut self,
        event_loop: &ActiveEventLoop,
        cause: StartCause,
    ) {
        if cause != StartCause::Init
            && cause != StartCause::CreateWindow
            && cause != StartCause::MacOSReopen
        {
            return;
        }

        if cause == StartCause::MacOSReopen && !self.router.routes.is_empty() {
            // Reopen (dock click) with every window minimized should
            // restore one; otherwise clicking the dock icon does
            // nothing at all.
            let all_minimized = self
                .router
                .routes
                .values()
                .all(|route| route.window.winit_window.is_minimized() == Some(true));
            if all_minimized {
                if let Some(route) = self.router.routes.values().next() {
                    route.window.winit_window.set_minimized(false);
                    route.window.winit_window.focus_window();
                }
            }
            return;
        }

        #[cfg(all(
            any(feature = "x11", feature = "wayland"),
            unix,
            not(any(target_os = "redox", target_family = "wasm", target_os = "macos"))
        ))]
        // Started even under `force-theme`: Settings can switch to System
        // at runtime, and `ThemeChanged` is ignored while a theme is forced.
        if cause == StartCause::Init && self.config.adaptive_colors.is_some() {
            use rio_window::platform::linux::ActiveEventLoopExtLinux;
            event_loop.start_system_theme_monitor();
        }

        let theme = self
            .config
            .force_theme
            .map(|t| t.to_window_theme())
            .or_else(|| event_loop.system_theme());
        update_colors_based_on_theme(&mut self.config, theme);

        self.router.create_window(
            event_loop,
            self.event_proxy.clone(),
            &self.config,
            None,
            self.app_id.as_deref(),
        );

        if cause == StartCause::Init {
            self.setup_quake_hotkey();
        }

        // Schedule title updates every 2s
        let timer_id = TimerId::new(Topic::UpdateTitles, 0);
        if !self.scheduler.scheduled(timer_id) {
            self.scheduler.schedule(
                EventPayload::new(RioEventType::Rio(RioEvent::UpdateTitles), unsafe {
                    rio_window::window::WindowId::dummy().into()
                }),
                Duration::from_secs(2),
                true,
                timer_id,
            );
        }

        tracing::info!("Initialisation complete");
    }

    pub(super) fn handle_about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Apply any deferred group-collapse toggle to all windows and persist.
        // This runs after the current event batch, before the event loop sleeps,
        // so all windows are updated before their scheduled redraws fire.
        if let Some((group_id, target)) = self.pending_group_toggle.take() {
            self.ensure_persist_handle();
            self.apply_group_collapse_to_all_windows(&group_id, target);
        }

        // Check for persistence outcomes: revert all windows and show an error
        // if the last SetGroupCollapsed command failed.
        self.poll_group_collapse_outcome();

        let control_flow = match self.scheduler.update() {
            Some(instant) => ControlFlow::WaitUntil(instant),
            None => ControlFlow::Wait,
        };
        event_loop.set_control_flow(control_flow);
    }

    pub(super) fn handle_exiting(&mut self) {
        // Ensure that all the windows are dropped, so the destructors for
        // Renderer and contexts ran.
        self.router.routes.clear();

        // SAFETY: The clipboard must be dropped before the event loop, so
        // replace it with a safe no-op placeholder.
        self.router.clipboard = Clipboard::new_nop();

        crate::ssh_secrets::shred_all();
        std::process::exit(0);
    }
}
