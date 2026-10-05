use crate::constants;
use crate::layout::ContextDimension;
use rio_backend::config::navigation::Navigation;
use rio_backend::config::Config;
use rio_window::window::Theme;

#[inline]
pub fn padding_top_from_config(
    navigation: &Navigation,
    padding_y_top: f32,
    #[allow(unused)] num_tabs: usize,
    #[allow(unused)] macos_use_unified_titlebar: bool,
) -> f32 {
    // Tab mode: the Terminus shell's header + session pills sit above
    // the grid (`terminus_ui::shell::layout::grid_insets`).
    if navigation.is_enabled() {
        return terminus_ui::shell::grid_insets().top + padding_y_top;
    }

    let default_padding = constants::PADDING_Y + padding_y_top;

    #[cfg(target_os = "macos")]
    {
        use rio_backend::config::navigation::NavigationMode;
        if navigation.mode == NavigationMode::NativeTab {
            let additional = if macos_use_unified_titlebar {
                constants::ADDITIONAL_PADDING_Y_ON_UNIFIED_TITLEBAR
            } else {
                0.0
            };
            return additional + padding_y_top;
        }
    }

    default_padding
}

/// Bottom grid margin: the config margin plus the shell's card gutter.
#[inline]
pub fn padding_bottom_from_config(navigation: &Navigation, padding_y_bottom: f32) -> f32 {
    if navigation.is_enabled() {
        terminus_ui::shell::grid_insets().bottom + padding_y_bottom
    } else {
        padding_y_bottom
    }
}

/// Right grid margin: the config margin plus the shell's card gutter.
#[inline]
pub fn padding_right_from_config(navigation: &Navigation, padding_right: f32) -> f32 {
    if navigation.is_enabled() {
        terminus_ui::shell::grid_insets().right + padding_right
    } else {
        padding_right
    }
}

#[inline]
pub fn terminal_dimensions(layout: &ContextDimension) -> rio_backend::event::WindowSize {
    let width = layout.width - layout.margin.left - layout.margin.right;
    let height = layout.height - layout.margin.top - layout.margin.bottom;
    rio_backend::event::WindowSize {
        width: width as u16,
        height: height as u16,
        cols: layout.columns as u16,
        rows: layout.lines as u16,
    }
}

#[inline]
pub fn update_colors_based_on_theme(config: &mut Config, theme_opt: Option<Theme>) {
    if let Some(theme) = theme_opt {
        if let Some(adaptive_colors) = &config.adaptive_colors {
            match theme {
                Theme::Light => {
                    if let Some(light_colors) = adaptive_colors.light {
                        config.colors = light_colors;
                    }
                }
                Theme::Dark => {
                    if let Some(darkcolors) = adaptive_colors.dark {
                        config.colors = darkcolors;
                    }
                }
            }
        }
    }
}
