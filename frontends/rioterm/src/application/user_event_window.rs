use super::Application;
use crate::event::{RioEvent, RioEventType};
use crate::renderer::utils::update_colors_based_on_theme;
use rio_backend::config::colors::NamedColor;
use rio_window::event_loop::ActiveEventLoop;
#[cfg(target_os = "macos")]
use rio_window::platform::macos::ActiveEventLoopExtMacOS;
#[cfg(target_os = "macos")]
use rio_window::platform::macos::WindowExtMacOS;
use rio_window::window::Fullscreen;

impl Application<'_> {
    pub(super) fn user_event_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: rio_backend::event::WindowId,
        payload: RioEventType,
    ) {
        match payload {
            RioEventType::Rio(RioEvent::CreateWindow) => {
                self.router.create_window(
                    event_loop,
                    self.event_proxy.clone(),
                    &self.config,
                    None,
                    self.app_id.as_deref(),
                );
            }
            RioEventType::Rio(RioEvent::ToggleQuake) => {
                self.toggle_quake_window(event_loop);
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::CreateNativeTab(working_dir_overwrite)) => {
                if let Some(route) = self.router.routes.get(&window_id) {
                    // This case happens only for native tabs
                    // every time that a new tab is created through context
                    // it also reaches for the foreground process path if
                    // config.use_current_path is true
                    // For these case we need to make a workaround
                    let config = if working_dir_overwrite.is_some() {
                        rio_backend::config::Config {
                            working_dir: working_dir_overwrite,
                            ..self.config.clone()
                        }
                    } else {
                        self.config.clone()
                    };

                    self.router.create_native_tab(
                        event_loop,
                        self.event_proxy.clone(),
                        &config,
                        Some(&route.window.winit_window.tabbing_identifier()),
                        None,
                    );
                }
            }
            RioEventType::Rio(RioEvent::CreateConfigEditor) => {
                if self.config.navigation.open_config_with_split {
                    self.router.open_config_split(&self.config);
                } else {
                    self.router.open_config_window(
                        event_loop,
                        self.event_proxy.clone(),
                        &self.config,
                    );
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::CloseWindow) => {
                self.router.routes.remove(&window_id);
                if self.router.routes.is_empty() && !self.config.confirm_before_quit {
                    event_loop.exit();
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::SelectNativeTabByIndex(tab_index)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.winit_window.select_tab_at_index(tab_index);
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::SelectNativeTabLast) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route
                        .window
                        .winit_window
                        .select_tab_at_index(route.window.winit_window.num_tabs() - 1);
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::SelectNativeTabNext) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.winit_window.select_next_tab();
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::SelectNativeTabPrev) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.winit_window.select_previous_tab();
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::Hide) => {
                event_loop.hide_application();
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::HideOtherApplications) => {
                event_loop.hide_other_applications();
            }
            RioEventType::Rio(RioEvent::Minimize(set_minimize)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.winit_window.set_minimized(set_minimize);
                }
            }
            RioEventType::Rio(RioEvent::ToggleFullScreen) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    match route.window.winit_window.fullscreen() {
                        None => route
                            .window
                            .winit_window
                            .set_fullscreen(Some(Fullscreen::Borderless(None))),
                        _ => route.window.winit_window.set_fullscreen(None),
                    }
                }
            }
            RioEventType::Rio(RioEvent::ToggleAppearanceTheme) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    use rio_backend::config::theme::AppearanceTheme;
                    let current = self
                        .config
                        .force_theme
                        .or_else(|| {
                            route
                                .window
                                .winit_window
                                .theme()
                                .map(AppearanceTheme::from_window_theme)
                        })
                        .unwrap_or(AppearanceTheme::Dark);
                    let toggled = current.toggled();
                    self.config.force_theme = Some(toggled);
                    update_colors_based_on_theme(
                        &mut self.config,
                        Some(toggled.to_window_theme()),
                    );
                    route.window.screen.update_config(
                        &self.config,
                        &self.router.font_library,
                        false,
                    );
                    route.window.configure_window(&self.config);
                }
            }
            RioEventType::Rio(RioEvent::ColorChange(route_id, index, color)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    let screen = &mut route.window.screen;
                    // Background color is index 1 relative to NamedColor::Foreground
                    if index == NamedColor::Foreground as usize + 1 {
                        let grid = screen.context_manager.current_grid_mut();
                        // The event carries a `route_id: usize` (global
                        // counter). `ContextGrid::get_mut` is keyed on
                        // taffy `NodeId` — a different identifier space,
                        // so `get_mut(route_id.into())` effectively
                        // never matches. Look the panel up by its
                        // actual route id.
                        if let Some(context_item) = grid.get_by_route_id(route_id) {
                            use crate::context::renderable::BackgroundState;
                            context_item.context_mut().renderable_content.background =
                                Some(match color {
                                    Some(c) => BackgroundState::Set(c.to_wgpu()),
                                    None => BackgroundState::Reset,
                                });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
