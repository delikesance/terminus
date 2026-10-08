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

/// Swap `config.colors` to the palette of `theme_opt` and record which
/// one is showing in `config.active_theme` (the chrome follows it). A theme
/// with no palette behind it leaves both untouched.
#[inline]
pub fn update_colors_based_on_theme(config: &mut Config, theme_opt: Option<Theme>) {
    use rio_backend::config::theme::AppearanceTheme;
    let Some(theme) = theme_opt else {
        return;
    };
    let Some(adaptive_colors) = &config.adaptive_colors else {
        return;
    };
    let colors = match theme {
        Theme::Light => adaptive_colors.light,
        Theme::Dark => adaptive_colors.dark,
    };
    if let Some(colors) = colors {
        config.colors = colors;
        config.active_theme = Some(AppearanceTheme::from_window_theme(theme));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_backend::config::appearance::light_colors;
    use rio_backend::config::theme::AppearanceTheme;

    fn resolved(src: &str) -> Config {
        let mut config: Config = toml::from_str(src).unwrap();
        config.resolve_appearance();
        config
    }

    #[test]
    fn os_theme_swaps_the_terminal_palette_and_records_it() {
        let mut config = resolved("[appearance]\ntheme = \"system\"\n");
        let dark = config.colors;
        update_colors_based_on_theme(&mut config, Some(Theme::Light));
        assert_eq!(config.colors, light_colors());
        assert_eq!(config.active_theme, Some(AppearanceTheme::Light));
        update_colors_based_on_theme(&mut config, Some(Theme::Dark));
        assert_eq!(config.colors, dark);
        assert_eq!(config.active_theme, Some(AppearanceTheme::Dark));
    }

    #[test]
    fn no_palette_for_the_theme_leaves_colors_and_chrome_dark() {
        let mut config = Config::default();
        let before = config.colors;
        update_colors_based_on_theme(&mut config, Some(Theme::Light));
        assert_eq!(config.colors, before);
        assert_eq!(config.active_theme, None);
    }
}
