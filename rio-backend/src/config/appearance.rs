//! `[appearance]`: the Terminus Dark / Light / System choice that
//! Settings > Appearance writes.
//!
//! It is resolved onto Rio's own machinery once the file is loaded:
//! Dark and Light become `force-theme`, System leaves it unset so the
//! window follows the OS, and `adaptive_colors` always carries both
//! halves (the configured colors for dark, [`light_colors`] for light
//! unless an `adaptive-theme` provides its own).

use super::colors::{hex_to_color_arr, hex_to_color_wgpu, Colors};
use super::theme::{AdaptiveColors, AppearanceTheme};
use super::Config;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Dark,
    Light,
    System,
}

impl ThemePreference {
    /// Value of `[appearance] theme`.
    pub fn config_value(self) -> &'static str {
        match self {
            ThemePreference::Dark => "dark",
            ThemePreference::Light => "light",
            ThemePreference::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    /// Unset keeps the Rio behaviour (`force-theme` / `adaptive-theme`),
    /// dark when neither is configured.
    #[serde(default)]
    pub theme: Option<ThemePreference>,
}

impl Config {
    /// Map `[appearance] theme` onto `force-theme` and fill the adaptive
    /// palettes. Call once after decoding the file.
    pub fn resolve_appearance(&mut self) {
        let user_adaptive = self.adaptive_colors.is_some();
        match self.appearance.theme {
            Some(ThemePreference::Dark) => self.force_theme = Some(AppearanceTheme::Dark),
            Some(ThemePreference::Light) => {
                self.force_theme = Some(AppearanceTheme::Light)
            }
            Some(ThemePreference::System) => self.force_theme = None,
            // Terminus was dark-only before the setting existed.
            None if !user_adaptive => {
                self.force_theme.get_or_insert(AppearanceTheme::Dark);
            }
            None => {}
        }
        let adaptive = self.adaptive_colors.get_or_insert(AdaptiveColors {
            dark: None,
            light: None,
        });
        adaptive.dark.get_or_insert(self.colors);
        adaptive.light.get_or_insert_with(light_colors);
    }

    /// The choice Settings > Appearance shows for this (resolved) config.
    pub fn theme_preference(&self) -> ThemePreference {
        self.appearance.theme.unwrap_or(match self.force_theme {
            Some(AppearanceTheme::Dark) => ThemePreference::Dark,
            Some(AppearanceTheme::Light) => ThemePreference::Light,
            None => ThemePreference::System,
        })
    }
}

/// Terminal palette paired with the light chrome ("violet paper"): the
/// background matches the chrome canvas, ANSI colours are darkened so
/// they read on it.
pub fn light_colors() -> Colors {
    let c = hex_to_color_arr;
    Colors {
        background: (c("#FBFAFD"), hex_to_color_wgpu("#FBFAFD")),
        foreground: c("#2A2438"),
        cursor: c("#6F55E0"),
        vi_cursor: c("#1D8A99"),
        tabs: c("#E6E2EF"),
        tabs_active: c("#1E1A29"),
        black: c("#2A2438"),
        red: c("#C4364A"),
        green: c("#2C7D4F"),
        yellow: c("#8F6400"),
        blue: c("#2F64C8"),
        magenta: c("#8A44CF"),
        cyan: c("#17798A"),
        white: c("#8C859E"),
        light_black: c("#5C5570"),
        light_red: c("#D9475A"),
        light_green: c("#36925E"),
        light_yellow: c("#A67600"),
        light_blue: c("#3D74DA"),
        light_magenta: c("#9B57DE"),
        light_cyan: c("#1F8C9E"),
        light_white: c("#B4AEC4"),
        selection_background: c("#DCD3FA"),
        selection_foreground: c("#1E1A29"),
        split: c("#D8D2E6"),
        split_active: c("#6F55E0"),
        search_match_background: c("#F2D58A"),
        search_match_foreground: c("#1E1A29"),
        search_focused_match_background: c("#E8A23A"),
        search_focused_match_foreground: c("#1E1A29"),
        hint_background: c("#6F55E0"),
        hint_foreground: c("#FFFFFF"),
        ..Colors::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Config {
        let mut config: Config = toml::from_str(src).unwrap();
        config.resolve_appearance();
        config
    }

    #[test]
    fn dark_and_light_force_the_window_theme() {
        let c = parse("[appearance]\ntheme = \"light\"\n");
        assert_eq!(c.force_theme, Some(AppearanceTheme::Light));
        assert_eq!(c.theme_preference(), ThemePreference::Light);
        let c = parse("[appearance]\ntheme = \"dark\"\n");
        assert_eq!(c.force_theme, Some(AppearanceTheme::Dark));
        assert_eq!(c.theme_preference(), ThemePreference::Dark);
    }

    #[test]
    fn system_follows_the_os_even_over_force_theme() {
        let c = parse("force-theme = \"dark\"\n[appearance]\ntheme = \"system\"\n");
        assert_eq!(c.force_theme, None);
        assert_eq!(c.theme_preference(), ThemePreference::System);
    }

    #[test]
    fn unset_stays_dark_unless_rio_settings_say_otherwise() {
        let c = parse("");
        assert_eq!(c.force_theme, Some(AppearanceTheme::Dark));
        assert_eq!(c.theme_preference(), ThemePreference::Dark);
        let c = parse("force-theme = \"light\"\n");
        assert_eq!(c.theme_preference(), ThemePreference::Light);
    }

    #[test]
    fn both_palettes_are_available_after_resolving() {
        let c = parse("[colors]\nbackground = \"#101010\"\n");
        let adaptive = c.adaptive_colors.unwrap();
        assert_eq!(
            adaptive.dark,
            Some(c.colors),
            "dark keeps the configured colors"
        );
        assert_eq!(adaptive.light, Some(light_colors()));
    }

    #[test]
    fn light_palette_is_light_with_dark_text() {
        let l = light_colors();
        let luma = |c: [f32; 4]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        assert!(luma(l.background.0) > 0.9);
        assert!(luma(l.foreground) < 0.2);
        for c in [l.red, l.green, l.blue, l.magenta, l.cyan, l.yellow, l.black] {
            assert!(luma(c) < 0.5, "{c:?} too pale for a light background");
        }
    }
}
