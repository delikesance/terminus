// MIT License
// Copyright 2022-present Raphael Amorim
//
// The functions (including comments) and logic of process_key_event, build_key_sequence, process_mouse_bindings, copy_selection, start_selection, update_selection_scrolling,
// side_by_pos, on_left_click, paste, sgr_mouse_report, mouse_report, normal_mouse_report, scroll,
// were retired from https://github.com/alacritty/alacritty/blob/c39c3c97f1a1213418c3629cc59a1d46e34070e0/alacritty/src/input.rs
// which is licensed under Apache 2.0 license.

mod chrome;
mod chrome_input;
mod clipboard;
mod config;
pub mod hint;
mod hint_actions;
mod island;
mod keys;
mod mouse;
mod palette;
mod render;
mod scrollbar;
mod search;
mod selection;
mod sessions;
mod sftp;
mod shell;
pub mod touch;

use crate::bindings::MouseBinding;
use crate::context;
use crate::context::renderable::Cursor;
use crate::context::{next_rich_text_id, process_open_url, ContextManager};
use crate::crosswords::grid::Scroll;
use crate::crosswords::pos::Pos;
use crate::crosswords::Mode;
use crate::hints::HintState;
use crate::hosts;
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
use rio_window::keyboard::ModifiersState;
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
    /// Terminus chrome: activity rail, host panel and add-host editor.
    pub chrome: terminus_ui::chrome::Chrome,
    /// Host label to highlight once its insert comes back from the
    /// worker. `create` hands out no id, so the row is matched by name
    /// on the next refresh instead of guessing an index.
    pending_host_select: Option<String>,
    /// After a vault-unlock prompt succeeds, retry this action once.
    pending_vault_continue: Option<terminus_ui::PendingVaultAction>,
    /// When the sidebar's connecting indicator started. Drives the orbit
    /// phase and the clear-when-ready timer. Paired with
    /// `chrome.panel.connecting_id` / `chrome.connection`.
    connecting_started: Option<std::time::Instant>,
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
    /// Last known maximized state — drives the Windows caption restore icon.
    pub window_maximized: bool,
    last_chrome_press: Option<ChromePress>,
    last_close_press: Option<(std::time::Instant, f32)>,
    pub grids: rustc_hash::FxHashMap<usize, rio_backend::sugarloaf::grid::GridRenderer>,
    pub grid_rasterizer: rio_grid::GridGlyphRasterizer,
    /// Active dual-pane SFTP browser (replaces terminal paint on the current leaf).
    pub sftp: Option<crate::sftp_ui::ActiveSftp>,
    /// Wake the event loop when the SFTP worker emits (same as host_store).
    sftp_wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
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

        let padding_y_bottom = config.margin.bottom;
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
        let chrome = {
            let mut chrome = terminus_ui::chrome::Chrome::default();
            // The rail starts under the tab strip rather than behind
            // it, so it lines up with the terminal's own top margin.
            chrome.top_inset = padding_y_top;
            chrome
        };
        let chrome_left = chrome.reserved_width();

        let margin = Margin::new(
            padding_y_top,
            config.margin.right,
            padding_y_bottom,
            config.margin.left + chrome_left,
        );
        let scaled_margin = Margin::new(
            padding_y_top * scale as f32,
            config.margin.right * scale as f32,
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
            host_store: crate::hosts::HostRepository::spawn(
                crate::hosts::data_dir(),
                host_wake,
            ),
            pending_host_select: None,
            pending_vault_continue: None,
            connecting_started: None,
            connecting_step_at: None,
            connecting_success_at: None,
            host_drag_anim_at: None,
            chrome,
            bindings,
            last_ime_cursor_pos: None,
            resize_state: None,
            allow_manual_dragging: config.navigation.is_enabled(),
            window_maximized: false,
            last_chrome_press: None,
            last_close_press: None,
            grids: rustc_hash::FxHashMap::default(),
            grid_rasterizer: rio_grid::GridGlyphRasterizer::new(),
            sftp: None,
            sftp_wake,
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

mod tests {
    use super::hint_actions::post_process_hyperlink_uri;
    use super::shell::{ssh_shell, GSSAPI_SSH_OPTIONS};
    use super::*;
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
            os_id: None,
            sort_order: 0,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn gssapi_ssh_options_force_kerberos_mic() {
        assert!(GSSAPI_SSH_OPTIONS.contains(&"GSSAPIAuthentication=yes"));
        assert!(GSSAPI_SSH_OPTIONS.contains(&"PreferredAuthentications=gssapi-with-mic"));
        assert!(GSSAPI_SSH_OPTIONS.contains(&"PubkeyAuthentication=no"));
        assert!(GSSAPI_SSH_OPTIONS.contains(&"PasswordAuthentication=no"));
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
        assert!(env.is_none());
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
        assert!(env.is_none());
        // Cleanup the temp identity we just wrote.
        if let Some(path) = shell.args.windows(2).find(|w| w[0] == "-i").map(|w| &w[1]) {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn password_ssh_shell_disables_pubkey() {
        let host = host_row("password");
        let (shell, env) =
            ssh_shell(&host, Some("secret"), None, None).expect("password shell");
        assert!(shell
            .args
            .iter()
            .any(|a| a == "PreferredAuthentications=password"));
        assert!(shell.args.iter().any(|a| a == "PubkeyAuthentication=no"));
        assert!(!shell.args.windows(2).any(|w| w[0] == "-i"));
        let env = env.expect("askpass env");
        assert!(env.iter().any(|(k, _)| k == "SSH_ASKPASS"));
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
