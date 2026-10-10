mod bell;
mod chrome_actions;
mod chrome_actions_hosts;
mod chrome_actions_sftp;
mod cursor_hover;
mod cursor_moved;
mod group_collapse;
mod lifecycle;
mod menus;
mod mouse_input;
mod mouse_press;
mod mouse_press_other;
mod mouse_release;
mod mouse_wheel;
mod quake;
mod redraw;
mod user_event;
mod user_event_terminal;
mod user_event_window;
mod window_event;

use crate::event::{EventPayload, EventProxy};
use crate::router::window::{close_action, exits_after_close, CloseAction};
use crate::router::Router;
use crate::scheduler::Scheduler;
use crate::watcher::configuration_file_updates;
use raw_window_handle::HasDisplayHandle;
use rio_backend::clipboard::Clipboard;
use rio_window::application::ApplicationHandler;
use rio_window::event::{StartCause, WindowEvent};
use rio_window::event_loop::ActiveEventLoop;
use rio_window::event_loop::{DeviceEvents, EventLoop};
use rio_window::window::WindowId;
use std::error::Error;

pub struct Application<'a> {
    config: rio_backend::config::Config,
    event_proxy: EventProxy,
    router: Router<'a>,
    scheduler: Scheduler,
    app_id: Option<String>,
    global_hotkey: Option<crate::global_hotkey::GlobalHotkeys>,
    /// Frontmost app when the quake window was shown, re-activated
    /// when it hides so focus returns where the user was.
    #[cfg(target_os = "macos")]
    quake_previous_app: Option<i32>,
    /// Application-owned persistence handle for group-collapse state.
    /// Initialized lazily from the first route's worker; stable thereafter.
    /// All persistence commands go through this handle — never through an
    /// arbitrary route selection at dispatch time.
    host_persistence: Option<crate::hosts::HostPersistHandle>,
    /// Group-collapse toggle deferred from `ChromeAction::ToggleGroup`.
    /// Processed in `about_to_wait` (where `self.router.routes` is freely
    /// accessible without an active route borrow). Holds `(group_id, target_collapsed)`.
    pending_group_toggle: Option<(String, bool)>,
}

impl Application<'_> {
    pub fn new<'app>(
        config: rio_backend::config::Config,
        config_error: Option<rio_backend::config::ConfigError>,
        event_loop: &EventLoop<EventPayload>,
        app_id: Option<String>,
    ) -> Application<'app> {
        // SAFETY: Since this takes a pointer to the winit event loop, it MUST be dropped first,
        // which is done in `exiting`.
        let clipboard =
            unsafe { Clipboard::new(event_loop.display_handle().unwrap().as_raw()) };

        let mut router = Router::new(config.fonts.to_owned(), clipboard);
        if let Some(error) = config_error {
            router.propagate_error_to_next_route(error.into());
        }

        let proxy = event_loop.create_proxy();
        let event_proxy = EventProxy::new(proxy.clone());
        let _ = configuration_file_updates(
            rio_backend::config::config_dir_path(),
            event_proxy.clone(),
        );
        #[cfg(target_os = "linux")]
        crate::power::start_resume_monitor(event_proxy.clone());
        let scheduler = Scheduler::new(proxy);
        event_loop.listen_device_events(DeviceEvents::Never);

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        event_loop.set_confirm_before_quit(config.confirm_before_quit);

        rio_notifier::request_authorization();

        Application {
            config,
            event_proxy,
            router,
            scheduler,
            app_id,
            global_hotkey: None,
            #[cfg(target_os = "macos")]
            quake_previous_app: None,
            host_persistence: None,
            pending_group_toggle: None,
        }
    }

    /// Close one window, as a native close request does. Shared by
    /// `CloseRequested` and the in-app Close button so both honour
    /// confirm-before-quit (asked for the last window only, as Windows'
    /// WM_CLOSE does) and leave the other windows running; the app exits
    /// with its last window.
    fn close_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
    ) {
        // macOS: Cmd+Q quit confirmation is handled by
        // `applicationShouldTerminate` in rio-window.
        // Windows: per-window close confirmation is handled
        // by `MessageBoxW` in rio-window's WM_CLOSE handler
        // (see `set_confirm_before_quit` plumbing).
        // Either way, by the time we see `CloseRequested`
        // the user has already confirmed — just close.
        let native_confirm = cfg!(any(target_os = "macos", target_os = "windows"));
        let is_last_window = self.router.routes.len() <= 1;
        match close_action(
            native_confirm,
            self.config.confirm_before_quit,
            is_last_window,
        ) {
            CloseAction::Confirm => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.confirm_quit();
                }
            }
            CloseAction::Close => {
                self.router.routes.remove(&window_id);
                if exits_after_close(self.router.routes.len()) {
                    event_loop.exit();
                }
            }
        }
    }

    fn skip_window_event(event: &WindowEvent) -> bool {
        matches!(
            event,
            WindowEvent::KeyboardInput {
                is_synthetic: true,
                ..
            } | WindowEvent::ActivationTokenDone { .. }
                | WindowEvent::DoubleTapGesture { .. }
                | WindowEvent::TouchpadPressure { .. }
                | WindowEvent::RotationGesture { .. }
                | WindowEvent::CursorEntered { .. }
                | WindowEvent::PinchGesture { .. }
                | WindowEvent::AxisMotion { .. }
                | WindowEvent::PanGesture { .. }
                | WindowEvent::HoveredFileCancelled
                | WindowEvent::Destroyed
                | WindowEvent::HoveredFile(_)
                | WindowEvent::Moved(_)
        )
    }

    pub fn run(
        &mut self,
        event_loop: EventLoop<EventPayload>,
    ) -> Result<(), Box<dyn Error>> {
        let result = event_loop.run_app(self);
        result.map_err(Into::into)
    }
}

impl ApplicationHandler<EventPayload> for Application<'_> {
    fn resumed(&mut self, _active_event_loop: &ActiveEventLoop) {
        // Also fired on Windows after sleep/hibernate (Linux sends
        // `RioEvent::SystemResumed` instead).
        self.rebuild_gpu_caches();
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.handle_new_events(event_loop, cause);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EventPayload) {
        self.handle_user_event(event_loop, event.window_id, event.payload);
    }

    #[cfg(target_os = "macos")]
    fn open_urls(&mut self, active_event_loop: &ActiveEventLoop, urls: Vec<String>) {
        self.handle_open_urls(active_event_loop, urls);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.handle_window_event(event_loop, window_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.handle_about_to_wait(event_loop);
    }

    fn open_config(&mut self, event_loop: &ActiveEventLoop) {
        self.handle_open_config(event_loop);
    }

    fn hook_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        key: &rio_window::event::KeyEvent,
        modifiers: &rio_window::event::Modifiers,
    ) {
        self.handle_hook_event(event_loop, key, modifiers);
    }

    // Emitted when the event loop is being shut down.
    // This is irreversible - if this event is emitted, it is guaranteed to be the last event that gets emitted.
    // You generally want to treat this as an “do on quit” event.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.handle_exiting();
    }
}
