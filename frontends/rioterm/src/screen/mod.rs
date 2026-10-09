// MIT License
// Copyright 2022-present Raphael Amorim
//
// The functions (including comments) and logic of process_key_event, build_key_sequence, process_mouse_bindings, copy_selection, start_selection, update_selection_scrolling,
// side_by_pos, on_left_click, paste, sgr_mouse_report, mouse_report, normal_mouse_report, scroll,
// were retired from https://github.com/alacritty/alacritty/blob/c39c3c97f1a1213418c3629cc59a1d46e34070e0/alacritty/src/input.rs
// which is licensed under Apache 2.0 license.

mod chrome;
pub(crate) mod chrome_input;
mod clipboard;
mod config;
mod frameless;
pub mod hint;
mod hint_actions;
mod history;
mod image_paste;
mod island;
mod keys;
mod mouse;
mod palette;
mod palette_tunnels;
mod render;
mod saved_tabs;
mod scrollbar;
mod search;
mod selection;
mod sessions;
mod sftp;
mod shell;
pub mod touch;
mod tunnels;
mod workspace;

use crate::bindings::MouseBinding;
use crate::context;
use crate::context::renderable::Cursor;
use crate::context::{next_rich_text_id, process_open_url, ContextManager};
use crate::crosswords::grid::Scroll;
use crate::crosswords::pos::Pos;
use crate::crosswords::Mode;
use crate::hints::HintState;
use crate::layout::ContextDimension;
use crate::mouse::{calculate_mouse_position, Mouse};
use crate::renderer::utils::padding_top_from_config;
use crate::renderer::Renderer;
use raw_window_handle::RawDisplayHandle;
use raw_window_handle::RawWindowHandle;
use rio_backend::config::layout::Margin;
use rio_backend::config::renderer::Backend;
use rio_backend::crosswords::pos::CursorState;
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};
use rio_backend::event::{EventProxy, SearchState};
use rio_backend::sugarloaf::layout::RootStyle;
use rio_backend::sugarloaf::{
    Sugarloaf, SugarloafBackend, SugarloafErrors, SugarloafRenderer, SugarloafWindow,
    SugarloafWindowSize,
};
use rio_window::event::Modifiers;
use std::error::Error;
use touch::TouchPurpose;

pub struct Screen<'screen> {
    bindings: crate::bindings::KeyBindings,
    mouse_bindings: Vec<MouseBinding>,
    pub modifiers: Modifiers,
    pub mouse: Mouse,
    pub touchpurpose: TouchPurpose,
    pub search_state: SearchState,
    pub hint_state: HintState,
    pub renderer: Renderer,
    /// Handle to the host database. Owned by the screen (not by the
    /// renderer) because the chrome, the keyboard and the painter all
    /// need it, and only the screen sees mouse and key events.
    pub host_store: crate::hosts::HostRepository,
    pub paste_errors: image_paste::PasteErrors,
    /// Background release check / self-update.
    pub updater: crate::updater::Updater,
    /// Terminus chrome: activity rail, host panel and add-host editor.
    pub chrome: terminus_ui::chrome::Chrome,
    /// Host label to highlight once its insert comes back from the
    /// worker. `create` hands out no id, so the row is matched by name
    /// on the next refresh instead of guessing an index.
    /// Endpoint of the host the add/edit dialog just saved.
    pending_host_select: Option<(String, String)>,
    /// The add-host dialog's "Connect": open a session once the new host
    /// is stored (editing only saves).
    pending_host_connect: bool,
    /// After a vault-unlock prompt succeeds, retry this action once.
    pending_vault_continue: Option<terminus_ui::PendingVaultAction>,
    /// Tabs saved for the next launch; see `screen/saved_tabs.rs`.
    saved_tabs: saved_tabs::SavedTabsState,
    /// When the sidebar's connecting indicator started. Drives the orbit
    /// phase and the clear-when-ready timer. Paired with
    /// `chrome.panel.connecting_ids` / `chrome.connection`.
    connecting_started: Option<std::time::Instant>,
    /// Hosts still coming up whose connection modal was taken over by a
    /// later one (Open All), with when they started.
    background_connecting: Vec<(String, std::time::Instant)>,
    /// When the connection modal last advanced a step (or started).
    connecting_step_at: Option<std::time::Instant>,
    /// When the success state was entered — dismiss after a short hold.
    connecting_success_at: Option<std::time::Instant>,
    /// Last time host-drag snap animation was ticked (for `dt`).
    host_drag_anim_at: Option<std::time::Instant>,
    pub sugarloaf: Sugarloaf<'screen>,
    pub context_manager: context::ContextManager<EventProxy>,
    /// IME state is per window, not per context: the platform IME
    /// composes into whichever context is current, and exactly one
    /// composition can exist per view. Keeping it here (instead of on
    /// `Context`) makes a stale preedit on a background tab or split
    /// structurally impossible.
    pub ime: crate::ime::Ime,
    /// Screen row the preedit rendered on last frame, so the row can
    /// be rebuilt when the composition moves or ends even when
    /// terminal damage reports nothing.
    last_preedit_row: Option<usize>,
    last_ime_cursor_pos: Option<(f32, f32, f32)>,
    hints_config: Vec<std::rc::Rc<rio_backend::config::hints::Hint>>,
    /// Hint regexes compiled on first use, keyed by pattern. Hover
    /// hit-testing runs on every mouse move; recompiling the URL
    /// pattern each time is measurable jank.
    hint_regex_cache:
        std::cell::RefCell<std::collections::HashMap<String, std::rc::Rc<onig::Regex>>>,
    /// The viewport cell and modifiers of the last hover-hint probe
    /// that found nothing. Mouse events arrive per pixel; re-probing
    /// the same cell would re-extract and re-scan the logical line for
    /// every one of them. Viewport coordinates so the check needs no
    /// terminal lock, and only an unchanged probe that was not over a
    /// link is skippable, so text changing under a shown underline
    /// still refreshes it. Reset on wheel scroll and highlight clears.
    last_hint_probe: Option<(Pos, rio_window::keyboard::ModifiersState)>,
    pub resize_state: Option<crate::layout::ResizeState>,
    /// Whether Tab navigation may drag the window from the chrome band.
    /// True whenever the custom title bar is active (Tab mode).
    pub allow_manual_dragging: bool,
    /// The window runs without OS decorations (see
    /// `router::window::uses_custom_titlebar`). On Linux this turns on
    /// edge resizing, the header window menu and the caption buttons.
    #[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
    pub custom_titlebar: bool,
    /// The pointer is on a resize edge (resize cursor showing).
    #[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
    on_resize_edge: bool,
    /// Last known maximized state — drives the Windows caption restore icon.
    pub window_maximized: bool,
    last_chrome_press: Option<ChromePress>,
    last_close_press: Option<(std::time::Instant, f32)>,
    pub grids: rustc_hash::FxHashMap<usize, rio_backend::sugarloaf::grid::GridRenderer>,
    pub grid_rasterizer: rio_grid::GridGlyphRasterizer,
    /// Active dual-pane SFTP browser (replaces terminal paint on the current leaf).
    pub sftp: Option<crate::sftp_ui::ActiveSftp>,
    /// SFTP browsers of the machines not selected: still running (their
    /// transfers go on), back in `sftp` when their machine is selected.
    pub sftp_parked:
        terminus_ui::screens::files::MachineSessions<crate::sftp_ui::ActiveSftp>,
    /// Machine ids of the open tabs at the last pump: a machine that
    /// drops out of it lost its last tab, and its browser is closed.
    pub sftp_tab_hosts: Vec<String>,
    /// SFTP browsers waiting for their credentials from the host store:
    /// `(host id, other pane, machine selected when asked)`.
    pub sftp_pending: Vec<(String, bool, String)>,
    /// Hover / dialog focus of the Files view (the rest is `sftp.state`).
    pub files_view: crate::renderer::views::files::FilesView,
    /// Wake the event loop when the SFTP worker emits (same as host_store).
    sftp_wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    /// Settings page state (SSH keys, Sync, Appearance, Updates).
    pub settings_view: terminus_ui::views::settings::SettingsView,
    /// History view state, and the machine its rows were loaded for.
    pub history_view: terminus_ui::views::history::HistoryState,
    pub(crate) history_for: Option<String>,
    /// Tunnels of the selected machine and their `ssh -N` processes
    /// (`None` only while borrowed by `with_tunnels`).
    pub tunnels: Option<crate::tunnel_worker::TunnelController>,
    /// When the pending "tunnel settled" wake-up fires.
    pub(crate) tunnel_wake_at: Option<std::time::Instant>,
}

pub struct ChromePress {
    window_origin: Option<rio_window::dpi::PhysicalPosition<i32>>,
    at: std::time::Instant,
}

impl ChromePress {
    fn validates_double_click(
        &self,
        window_origin: Option<rio_window::dpi::PhysicalPosition<i32>>,
    ) -> bool {
        self.at.elapsed() <= crate::constants::MULTI_CLICK_THRESHOLD
            && self.window_origin == window_origin
    }
}

pub struct ScreenWindowProperties {
    pub size: rio_window::dpi::PhysicalSize<u32>,
    pub scale: f64,
    pub raw_window_handle: RawWindowHandle,
    pub raw_display_handle: RawDisplayHandle,
    pub window_id: rio_window::window::WindowId,
    /// The quake dropdown: no title bar, so no caption buttons.
    #[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
    pub quake: bool,
}

#[inline]
fn window_should_be_opaque(config: &rio_backend::config::Config) -> bool {
    config.window.opacity >= 1.0 && !config.window.blur.is_glass()
}

impl Screen<'_> {
    pub fn new<'screen>(
        window_properties: ScreenWindowProperties,
        config: &rio_backend::config::Config,
        event_proxy: EventProxy,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        open_url: Option<String>,
    ) -> Result<Screen<'screen>, Box<dyn Error>> {
        let size = window_properties.size;
        let scale = window_properties.scale;
        let raw_window_handle = window_properties.raw_window_handle;
        let raw_display_handle = window_properties.raw_display_handle;
        let window_id = window_properties.window_id;

        let padding_y_top = padding_top_from_config(
            &config.navigation,
            config.margin.top,
            1,
            config.window.macos_use_unified_titlebar,
        );

        let padding_y_bottom = crate::renderer::utils::padding_bottom_from_config(
            &config.navigation,
            config.margin.bottom,
        );
        let sugarloaf_layout =
            RootStyle::new(scale as f32, config.fonts.size, config.line_height);

        let mut sugarloaf_errors: Option<SugarloafErrors> = None;

        let sugarloaf_window = SugarloafWindow {
            handle: raw_window_handle,
            display: raw_display_handle,
            scale: scale as f32,
            size: SugarloafWindowSize {
                width: size.width as f32,
                height: size.height as f32,
            },
        };

        let backend = if config.renderer.use_cpu {
            SugarloafBackend::Cpu
        } else {
            // `wgpu_backend` (see build.rs): rioterm's own `wgpu`
            // feature, or Windows, where sugarloaf and rio-backend get
            // the feature through the target-specific dependency
            // override so the wgpu code paths always exist. Without the
            // Windows half, default builds there silently fall back to
            // the CPU rasterizer.
            match config.renderer.backend {
                // `Backend::Vulkan` from the user config means the
                // native ash backend on Linux. Other OSes fall through
                // to the wgpu Vulkan path when wgpu is available;
                // otherwise we degrade to CPU rasterizer.
                #[cfg(target_os = "linux")]
                Backend::Vulkan => SugarloafBackend::Vulkan,
                #[cfg(all(not(target_os = "linux"), wgpu_backend))]
                Backend::Vulkan => SugarloafBackend::Wgpu(wgpu::Backends::VULKAN),
                #[cfg(all(not(target_os = "linux"), not(wgpu_backend)))]
                Backend::Vulkan => SugarloafBackend::Cpu,
                #[cfg(target_os = "macos")]
                Backend::Metal => SugarloafBackend::Metal,
                #[cfg(all(wgpu_backend, target_arch = "wasm32"))]
                Backend::Webgpu => SugarloafBackend::Wgpu(
                    wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
                ),
                #[cfg(all(wgpu_backend, not(target_arch = "wasm32")))]
                Backend::Webgpu => SugarloafBackend::Wgpu(wgpu::Backends::all()),
                #[cfg(not(wgpu_backend))]
                Backend::Webgpu => SugarloafBackend::Cpu,
            }
        };

        let sugarloaf_renderer = SugarloafRenderer {
            backend,
            font_features: config.fonts.features.clone(),
            colorspace: config.window.colorspace.to_sugarloaf_colorspace(),
            // The exact predicate the rest of the frontend uses: glass
            // blur forces the window bg alpha to 0 regardless of opacity,
            // so it needs an alpha-carrying surface exactly like
            // `window.opacity < 1` does. Fixed at surface creation; a
            // live-reloaded opacity change takes effect on restart.
            prefer_alpha_capable_adapter: !window_should_be_opaque(config),
        };

        let mut sugarloaf: Sugarloaf = match Sugarloaf::new(
            sugarloaf_window,
            sugarloaf_renderer,
            font_library,
            sugarloaf_layout,
        ) {
            Ok(instance) => instance,
            Err(instance_with_errors) => {
                sugarloaf_errors = Some(instance_with_errors.errors);
                instance_with_errors.instance
            }
        };

        #[cfg(wgpu_backend)]
        sugarloaf.update_filters(config.renderer.filters.as_slice());

        let mut renderer = Renderer::new(config);

        let bindings = crate::bindings::default_key_bindings(config);

        let is_native = config.navigation.is_native();

        let (shell, working_dir) = process_open_url(
            config.shell.to_owned(),
            config.working_dir.to_owned(),
            config.editor.to_owned(),
            open_url.as_deref(),
        );

        let context_manager_config = context::ContextManagerConfig {
            #[cfg(test)]
            dead_pty: false,
            cwd: config.navigation.current_working_directory,
            shell,
            env: None,
            working_dir,
            spawn_performer: true,
            #[cfg(not(target_os = "windows"))]
            use_fork: config.use_fork,
            is_native,
            // When navigation does not contain any color rule
            // does not make sense fetch for foreground process names/path
            should_update_title_extra: !config.navigation.color_automation.is_empty(),
            split_color: config.colors.split,
            split_active_color: config.colors.split_active,
            panel: config.panel,
            title: config.title.clone(),
            keyboard: config.keyboard.clone(),
            scrollback_history_limit: config.scrollback_history_limit,
            grapheme_clustering: config.grapheme_clustering,
        };

        let rich_text_id = next_rich_text_id();

        // The host worker runs on its own thread; when a write lands it
        // pings the event loop so an idle window repaints with the new
        // list. Same `RioEvent::Render` the PTY path uses.
        let host_wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>> = {
            let proxy = event_proxy.clone();
            let id = window_id.into();
            Some(std::sync::Arc::new(move || {
                proxy.send_event(
                    rio_backend::event::RioEventType::Rio(
                        rio_backend::event::RioEvent::Render,
                    ),
                    id,
                );
            }))
        };
        let sftp_wake = host_wake.clone();

        // The chrome reserves its own strip on the left; the grid margin
        // carries it so the terminal reflows beside the rail instead of
        // being painted over.
        // The rail starts under the tab strip rather than behind
        // it, so it lines up with the terminal's own top margin.
        #[allow(unused_mut)]
        let mut chrome = terminus_ui::chrome::Chrome {
            top_inset: padding_y_top,
            ..Default::default()
        };
        // Caption buttons only where the window has no OS decorations.
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            chrome.shell.window_controls = crate::router::window::shows_caption_buttons(
                crate::router::window::uses_custom_titlebar(config),
                window_properties.quake,
            );
        }
        let chrome_left = chrome.reserved_width();

        let padding_right = crate::renderer::utils::padding_right_from_config(
            &config.navigation,
            config.margin.right,
        );
        let margin = Margin::new(
            padding_y_top,
            padding_right,
            padding_y_bottom,
            config.margin.left + chrome_left,
        );
        let scaled_margin = Margin::new(
            padding_y_top * scale as f32,
            padding_right * scale as f32,
            padding_y_bottom * scale as f32,
            (config.margin.left + chrome_left) * scale as f32,
        );
        let (text_dimensions, cell_metrics) = sugarloaf.compute_cell_metrics(
            config.fonts.size,
            config.line_height,
            scale as f32,
        );
        let context_dimension = ContextDimension::build(
            size.width as f32,
            size.height as f32,
            text_dimensions,
            cell_metrics,
            config.line_height,
            config.fonts.size,
            margin,
        );

        let cursor = Cursor {
            content: config.cursor.shape.into(),
            state: CursorState::new(config.cursor.shape.into()),
        };

        let context_manager = context::ContextManager::start(
            // config.cursor.blinking
            (&cursor, config.cursor.blinking),
            event_proxy,
            window_id.into(),
            0,
            rich_text_id,
            context_manager_config,
            context_dimension,
            scaled_margin,
            sugarloaf_errors,
        )?;

        sugarloaf.set_window_opaque(window_should_be_opaque(config));
        sugarloaf.set_background_color(Some(renderer.dynamic_background.1));

        if let Some(image) = &config.window.background_image {
            if let Err(message) = sugarloaf.set_background_image(image) {
                renderer.assistant.set_error(RioError {
                    level: RioErrorLevel::Warning,
                    report: RioErrorType::BackgroundImageLoadFailure(message),
                });
            }
        } else {
            sugarloaf.clear_background_image();
        }

        let mut settings_view =
            terminus_ui::views::settings::SettingsView::new(env!("CARGO_PKG_VERSION"));
        crate::renderer::views::settings::load_config(&mut settings_view, config);

        Ok(Screen {
            search_state: SearchState::default(),
            hint_state: HintState::new(config.hints.alphabet.clone()),
            hints_config: config
                .hints
                .rules
                .iter()
                .map(|h| std::rc::Rc::new(h.clone()))
                .collect(),
            hint_regex_cache: Default::default(),
            last_hint_probe: None,
            mouse_bindings: crate::bindings::default_mouse_bindings(),
            modifiers: Modifiers::default(),
            context_manager,
            ime: crate::ime::Ime::new(),
            last_preedit_row: None,
            sugarloaf,
            mouse: Mouse::new(config.scroll.multiplier, config.scroll.divider),
            touchpurpose: TouchPurpose::default(),
            renderer,
            updater: crate::updater::Updater::spawn(
                config.updates.into(),
                host_wake.clone(),
            ),
            paste_errors: image_paste::PasteErrors::new(host_wake.clone()),
            tunnels: Some(crate::tunnel_worker::TunnelController::spawn(
                crate::hosts::data_dir(),
                host_wake.clone(),
            )),
            host_store: crate::hosts::HostRepository::spawn(
                crate::hosts::data_dir(),
                host_wake,
            ),
            pending_host_select: None,
            pending_host_connect: false,
            pending_vault_continue: None,
            saved_tabs: saved_tabs::SavedTabsState::default(),
            connecting_started: None,
            background_connecting: Vec::new(),
            connecting_step_at: None,
            connecting_success_at: None,
            host_drag_anim_at: None,
            chrome,
            bindings,
            last_ime_cursor_pos: None,
            resize_state: None,
            allow_manual_dragging: config.navigation.is_enabled(),
            custom_titlebar: crate::router::window::uses_custom_titlebar(config),
            on_resize_edge: false,
            window_maximized: false,
            last_chrome_press: None,
            last_close_press: None,
            grids: rustc_hash::FxHashMap::default(),
            grid_rasterizer: rio_grid::GridGlyphRasterizer::new(),
            sftp: None,
            sftp_parked: Default::default(),
            sftp_tab_hosts: Vec::new(),
            sftp_pending: Vec::new(),
            files_view: Default::default(),
            sftp_wake,
            settings_view,
            history_view: terminus_ui::views::history::HistoryState::new(
                Vec::new(),
                crate::history_worker::env_recording_enabled(),
            ),
            history_for: None,
            tunnel_wake_at: None,
        })
    }

    #[inline]
    pub fn ensure_grid(&mut self, route_id: usize, cols: u32, rows: u32) {
        use std::collections::hash_map::Entry;
        match self.grids.entry(route_id) {
            Entry::Occupied(mut e) => e.get_mut().resize(cols, rows),
            Entry::Vacant(e) => {
                e.insert(rio_backend::sugarloaf::grid::GridRenderer::new(
                    &self.sugarloaf.ctx,
                    cols,
                    rows,
                ));
            }
        }
    }

    #[inline]
    pub fn ctx(&self) -> &ContextManager<EventProxy> {
        &self.context_manager
    }

    #[inline]
    pub fn ctx_mut(&mut self) -> &mut ContextManager<EventProxy> {
        &mut self.context_manager
    }

    /// After sleep/hibernate the GPU can hand back blank textures while the
    /// CPU-side caches still believe their glyphs are uploaded (boxes
    /// instead of text). Forget every cached glyph so the next frame
    /// re-rasterizes and re-uploads.
    pub fn on_system_resume(&mut self) {
        self.sugarloaf.invalidate_gpu_caches();
        self.grid_rasterizer.clear_font_caches();
        for grid in self.grids.values_mut() {
            grid.clear_atlas();
        }
        self.mark_dirty();
    }

    #[inline]
    pub fn mark_dirty(&mut self) {
        self.context_manager
            .current_mut()
            .renderable_content
            .pending_update
            .set_dirty();
    }

    #[inline]
    pub fn search_active(&self) -> bool {
        self.search_state.history_index.is_some()
    }

    #[inline]
    pub fn reset_mouse(&mut self) {
        self.mouse.accumulated_scroll = crate::mouse::AccumulatedScroll::default();
    }

    #[inline]
    pub fn mouse_position(&self, display_offset: usize) -> Pos {
        let current_grid = self.context_manager.current_grid();
        let (context, margin) = current_grid.current_context_with_computed_dimension();
        let context_dimension = context.dimension;
        calculate_mouse_position(
            &self.mouse,
            display_offset,
            (context_dimension.columns, context_dimension.lines),
            margin.left,
            margin.top,
            (
                context_dimension.cell.cell_width,
                context_dimension.cell.cell_height,
            ),
        )
    }

    #[inline]
    pub fn touch_purpose(&mut self) -> &mut TouchPurpose {
        &mut self.touchpurpose
    }

    #[inline]
    pub fn scroll_bottom_when_cursor_not_visible(&mut self) {
        let mut terminal = self.ctx_mut().current_mut().terminal.lock();
        if terminal.display_offset() != 0 {
            terminal.scroll_display(Scroll::Bottom);
        }
        drop(terminal);
    }

    #[inline]
    pub fn mouse_mode(&self) -> bool {
        let mode = self.get_mode();
        mode.intersects(Mode::MOUSE_MODE) && !mode.contains(Mode::VI)
    }

    #[inline]
    pub fn display_offset(&self) -> usize {
        let terminal = self.ctx().current().terminal.lock();
        let display_offset = terminal.display_offset();
        drop(terminal);
        display_offset
    }

    #[inline]
    pub fn get_mode(&self) -> Mode {
        let terminal = self.ctx().current().terminal.lock();
        let mode = terminal.mode();
        drop(terminal);
        mode
    }
}

#[cfg(test)]
mod tests {
    use super::hint_actions::post_process_hyperlink_uri;
    use super::shell::{ssh_shell, GSSAPI_SSH_OPTIONS};
    use super::*;
    use crate::hosts;
    use chrono::Utc;

    fn host_row(auth_method: &str) -> hosts::HostRow {
        hosts::HostRow {
            id: "host-1".into(),
            name: "Box".into(),
            hostname: "box.example".into(),
            port: 22,
            username: "alice".into(),
            auth_method: auth_method.into(),
            identity_id: None,
            group_id: None,
            tags: Vec::new(),
            notes: String::new(),
            os_id: None,
            sort_order: 0,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn split_target_follows_the_focused_pane_host() {
        use super::sessions::{split_target, SplitTarget};
        assert_eq!(split_target(None), SplitTarget::Local);
        assert_eq!(split_target(Some(hosts::LOCAL_ID)), SplitTarget::Local);
        assert_eq!(
            split_target(Some("9c1e")),
            SplitTarget::Row("9c1e".to_string())
        );
        let wsl = format!("{}Ubuntu", hosts::WSL_PREFIX);
        assert_eq!(split_target(Some(&wsl)), SplitTarget::Row(wsl.clone()));
    }

    #[test]
    fn gssapi_ssh_options_force_kerberos_mic() {
        assert!(GSSAPI_SSH_OPTIONS.contains(&"GSSAPIAuthentication=yes"));
        assert!(GSSAPI_SSH_OPTIONS.contains(&"PreferredAuthentications=gssapi-with-mic"));
        assert!(GSSAPI_SSH_OPTIONS.contains(&"PubkeyAuthentication=no"));
        assert!(GSSAPI_SSH_OPTIONS.contains(&"PasswordAuthentication=no"));
    }

    #[test]
    fn ssh_shell_ends_options_before_the_destination() {
        let mut host = host_row("gssapi");
        host.hostname = "-oProxyCommand=x".into();
        host.username = String::new();
        let (shell, _) = ssh_shell(&host, None, None, None).expect("shell");
        let n = shell.args.len();
        assert_eq!(&shell.args[n - 2..], ["--", "-oProxyCommand=x"]);
    }

    #[test]
    fn gssapi_ssh_shell_sets_gssapi_options_without_identity_file() {
        let host = host_row("gssapi");
        let (shell, env) = ssh_shell(&host, None, None, None).expect("gssapi shell");
        assert_eq!(shell.program.as_deref(), Some("ssh"));
        assert!(shell.args.iter().any(|a| a == "GSSAPIAuthentication=yes"));
        assert!(shell
            .args
            .iter()
            .any(|a| a == "PreferredAuthentications=gssapi-with-mic"));
        assert!(shell.args.iter().any(|a| a == "PubkeyAuthentication=no"));
        assert!(!shell.args.windows(2).any(|w| w[0] == "-i"));
        assert_eq!(
            shell.args.last().map(String::as_str),
            Some("alice@box.example")
        );
        assert!(
            env.is_some_and(|e| e.iter().all(|(k, _)| k == "TERM" || k == "COLORTERM"))
        );
    }

    /// `ssh` forwards the local `$TERM` in its pty request. Ours is
    /// `xterm-rio`, which remote hosts don't have a terminfo entry for, so
    /// readline falls back to dumb mode and garbles history recall.
    #[test]
    fn ssh_shell_advertises_a_term_remotes_know() {
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----\n";
        for (auth, password, identity) in [
            ("gssapi", None, None),
            ("key", None, Some(pem)),
            ("password", Some("secret"), None),
        ] {
            let host = host_row(auth);
            let (shell, env) =
                ssh_shell(&host, password, identity, None).expect("ssh shell");
            let env = env.expect("ssh env");
            let term = env
                .iter()
                .find(|(k, _)| k == "TERM")
                .map(|(_, v)| v.as_str());
            assert_eq!(term, Some("xterm-256color"), "{auth}");
            if let Some(path) =
                shell.args.windows(2).find(|w| w[0] == "-i").map(|w| &w[1])
            {
                let _ = std::fs::remove_file(path);
            }
            if let Some((_, secret)) =
                env.iter().find(|(k, _)| k == "TERMINUS_SSH_ASKPASS_FILE")
            {
                let _ = std::fs::remove_file(secret);
            }
        }
    }

    /// A remote program only knows the terminal does truecolor if
    /// `COLORTERM` reaches it; `ssh` sends nothing unless asked. Without it
    /// the same program that shows exact colors in a local tab drops to the
    /// 256-color palette over SSH (flat greys, washed-out reds).
    #[test]
    fn ssh_shell_forwards_colorterm_so_remotes_keep_truecolor() {
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----\n";
        for (auth, password, identity) in [
            ("gssapi", None, None),
            ("key", None, Some(pem)),
            ("password", Some("secret"), None),
        ] {
            let host = host_row(auth);
            let (shell, env) =
                ssh_shell(&host, password, identity, None).expect("ssh shell");
            let env = env.expect("ssh env");
            assert!(
                shell
                    .args
                    .windows(2)
                    .any(|w| w[0] == "-o" && w[1] == "SendEnv=COLORTERM"),
                "{auth}: ssh must be told to send COLORTERM"
            );
            let colorterm = env
                .iter()
                .find(|(k, _)| k == "COLORTERM")
                .map(|(_, v)| v.as_str());
            assert_eq!(colorterm, Some("truecolor"), "{auth}");
            if let Some(path) =
                shell.args.windows(2).find(|w| w[0] == "-i").map(|w| &w[1])
            {
                let _ = std::fs::remove_file(path);
            }
            if let Some((_, secret)) =
                env.iter().find(|(k, _)| k == "TERMINUS_SSH_ASKPASS_FILE")
            {
                let _ = std::fs::remove_file(secret);
            }
        }
    }

    /// Without keepalives a laptop that slept leaves ssh waiting forever on
    /// a dead socket: the tab freezes instead of saying the link is gone.
    #[test]
    fn ssh_shell_sends_keepalives_so_a_dead_link_ends_the_session() {
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----\n";
        for (auth, password, identity) in [
            ("gssapi", None, None),
            ("key", None, Some(pem)),
            ("password", Some("secret"), None),
        ] {
            let host = host_row(auth);
            let (shell, env) =
                ssh_shell(&host, password, identity, None).expect("ssh shell");
            let has =
                |opt: &str| shell.args.windows(2).any(|w| w[0] == "-o" && w[1] == opt);
            assert!(has("ServerAliveInterval=15"), "{auth}");
            assert!(has("ServerAliveCountMax=3"), "{auth}");
            if let Some(path) =
                shell.args.windows(2).find(|w| w[0] == "-i").map(|w| &w[1])
            {
                let _ = std::fs::remove_file(path);
            }
            if let Some((_, secret)) = env
                .iter()
                .flatten()
                .find(|(k, _)| k == "TERMINUS_SSH_ASKPASS_FILE")
            {
                let _ = std::fs::remove_file(secret);
            }
        }
    }

    #[test]
    fn key_ssh_shell_passes_identity_file() {
        let host = host_row("key");
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----\n";
        let (shell, env) = ssh_shell(&host, None, Some(pem), None).expect("key shell");
        assert!(shell.args.windows(2).any(|w| w[0] == "-i"));
        assert!(shell.args.iter().any(|a| a == "IdentitiesOnly=yes"));
        assert!(shell
            .args
            .iter()
            .any(|a| a == "PreferredAuthentications=publickey"));
        assert!(
            env.is_some_and(|e| e.iter().all(|(k, _)| k == "TERM" || k == "COLORTERM"))
        );
        // Cleanup the temp identity we just wrote.
        if let Some(path) = shell.args.windows(2).find(|w| w[0] == "-i").map(|w| &w[1]) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// The key only has to exist until ssh has authenticated: ssh's own
    /// `LocalCommand` deletes it (and the passphrase file) at that point.
    #[cfg(unix)]
    #[test]
    fn key_ssh_shell_deletes_its_secrets_once_connected() {
        let host = host_row("key");
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----\n";
        let (shell, env) =
            ssh_shell(&host, None, Some(pem), Some("pass")).expect("key shell");
        let key = shell
            .args
            .windows(2)
            .find(|w| w[0] == "-i")
            .map(|w| w[1].clone())
            .unwrap();
        let env = env.unwrap();
        let secret = env
            .iter()
            .find(|(k, _)| k == "TERMINUS_SSH_ASKPASS_FILE")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert!(shell.args.iter().any(|a| a == "PermitLocalCommand=yes"));
        let local = shell
            .args
            .iter()
            .find_map(|a| a.strip_prefix("LocalCommand="))
            .expect("LocalCommand option");
        assert!(local.contains(&key), "{local}");
        assert!(local.contains(&secret), "{local}");
        // The destination stays last (tunnels rely on it).
        assert_eq!(
            shell.args.last().map(String::as_str),
            Some("alice@box.example")
        );
        crate::ssh_secrets::shred(std::path::Path::new(&key));
        crate::ssh_secrets::shred(std::path::Path::new(&secret));
    }

    #[cfg(unix)]
    #[test]
    fn password_ssh_shell_deletes_its_secret_once_connected() {
        let host = host_row("password");
        let (shell, env) =
            ssh_shell(&host, Some("secret"), None, None).expect("password shell");
        let secret = env
            .unwrap()
            .into_iter()
            .find(|(k, _)| k == "TERMINUS_SSH_ASKPASS_FILE")
            .map(|(_, v)| v)
            .unwrap();
        let local = shell
            .args
            .iter()
            .find_map(|a| a.strip_prefix("LocalCommand="))
            .expect("LocalCommand option");
        assert!(local.contains(&secret), "{local}");
        crate::ssh_secrets::shred(std::path::Path::new(&secret));
    }

    #[test]
    fn password_ssh_shell_disables_pubkey() {
        let host = host_row("password");
        let (shell, env) =
            ssh_shell(&host, Some("secret"), None, None).expect("password shell");
        // keyboard-interactive too: PAM / 2FA servers (and macOS) disable
        // the `password` method and prompt through it instead.
        assert!(shell
            .args
            .iter()
            .any(|a| a == "PreferredAuthentications=password,keyboard-interactive"));
        assert!(shell.args.iter().any(|a| a == "PubkeyAuthentication=no"));
        assert!(!shell.args.windows(2).any(|w| w[0] == "-i"));
        let env = env.expect("askpass env");
        assert!(env.iter().any(|(k, _)| k == "SSH_ASKPASS"));
        let secret = env
            .iter()
            .find(|(k, _)| k == "TERMINUS_SSH_ASKPASS_FILE")
            .map(|(_, v)| std::path::PathBuf::from(v))
            .expect("secret file");
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "secret\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &std::path::Path| {
                std::fs::metadata(p).unwrap().permissions().mode() & 0o777
            };
            assert_eq!(mode(&secret), 0o600, "askpass secret readable by others");
            assert_eq!(
                mode(secret.parent().unwrap()),
                0o700,
                "askpass dir not private"
            );
            let helper = env
                .iter()
                .find(|(k, _)| k == "SSH_ASKPASS")
                .map(|(_, v)| std::path::PathBuf::from(v))
                .unwrap();
            assert_eq!(
                helper.parent(),
                secret.parent(),
                "helper outside private dir"
            );
            assert_eq!(
                mode(&helper) & 0o022,
                0,
                "askpass helper writable by others"
            );
        }
        let _ = std::fs::remove_file(secret);
    }

    #[cfg(unix)]
    #[test]
    fn private_temp_dir_tightens_a_loose_existing_dir() {
        use std::os::unix::fs::PermissionsExt;
        let name = format!("test-loose-{}", uuid::Uuid::new_v4());
        let dir = crate::ssh_secrets::private_temp_dir(&name).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        let again = crate::ssh_secrets::private_temp_dir(&name).unwrap();
        assert_eq!(again, dir);
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn key_ssh_shell_identity_file_is_private() {
        let host = host_row("key");
        let pem = "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----\n";
        let (shell, _) = ssh_shell(&host, None, Some(pem), None).expect("key shell");
        let path = shell
            .args
            .windows(2)
            .find(|w| w[0] == "-i")
            .map(|w| std::path::PathBuf::from(&w[1]))
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &std::path::Path| {
                std::fs::metadata(p).unwrap().permissions().mode() & 0o777
            };
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(path.parent().unwrap()), 0o700);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn chrome_press_validates_double_click() {
        use rio_window::dpi::PhysicalPosition;
        let origin = Some(PhysicalPosition::new(10, 20));

        // Same origin, fresh → a chrome double-click.
        let fresh = ChromePress {
            window_origin: origin,
            at: std::time::Instant::now(),
        };
        assert!(fresh.validates_double_click(origin));

        // Window moved between the presses (a re-grab after a window
        // drag) → keep dragging, don't maximize.
        assert!(!fresh.validates_double_click(Some(PhysicalPosition::new(110, 20))));

        // Unreported origin on both presses (Wayland) → time guard
        // alone decides; a reported-vs-unreported mix never validates.
        let unknown = ChromePress {
            window_origin: None,
            at: std::time::Instant::now(),
        };
        assert!(unknown.validates_double_click(None));
        assert!(!unknown.validates_double_click(origin));

        // Stale press → expired even at the same origin.
        let stale = ChromePress {
            window_origin: origin,
            at: std::time::Instant::now() - crate::constants::MULTI_CLICK_THRESHOLD * 2,
        };
        assert!(!stale.validates_double_click(origin));
    }

    #[test]
    fn test_post_process_hyperlink_uri() {
        // Test removing trailing parenthesis
        assert_eq!(
            post_process_hyperlink_uri("https://example.com)"),
            "https://example.com"
        );

        // Test removing trailing comma
        assert_eq!(
            post_process_hyperlink_uri("https://example.com,"),
            "https://example.com"
        );

        // Test removing trailing period
        assert_eq!(
            post_process_hyperlink_uri("https://example.com."),
            "https://example.com"
        );

        // Test handling balanced parentheses (should keep them)
        assert_eq!(
            post_process_hyperlink_uri("https://example.com/path(with)parens"),
            "https://example.com/path(with)parens"
        );

        // Test handling unbalanced parentheses
        assert_eq!(
            post_process_hyperlink_uri("https://example.com/path)"),
            "https://example.com/path"
        );

        // Test handling multiple trailing delimiters
        assert_eq!(
            post_process_hyperlink_uri("https://example.com.'),"),
            "https://example.com"
        );

        // Test markdown-style URLs
        assert_eq!(
            post_process_hyperlink_uri("https://example.com)"),
            "https://example.com"
        );

        // Test handling unbalanced brackets
        assert_eq!(
            post_process_hyperlink_uri("https://example.com/path]"),
            "https://example.com/path"
        );

        // Test balanced brackets (should keep them)
        assert_eq!(
            post_process_hyperlink_uri("https://example.com/path[with]brackets"),
            "https://example.com/path[with]brackets"
        );
    }
}
