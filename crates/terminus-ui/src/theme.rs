//! Chrome colors.
//!
//! Everything is derived from the terminal's own palette so the chrome
//! follows the user's theme instead of fighting it: the panel is the
//! background darkened, hover tints are the foreground at low alpha,
//! and selection is the accent. Alpha is carried in the fourth
//! component; the painters expect non-premultiplied RGBA.

/// Colors for the activity bar, the host panel and the dialogs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromeTheme {
    pub rail_bg: [f32; 4],
    pub rail_marker: [f32; 4],
    pub rail_active_bg: [f32; 4],
    pub panel_bg: [f32; 4],
    pub panel_border: [f32; 4],
    pub item_hover: [f32; 4],
    pub item_selected: [f32; 4],
    /// Resting surface of a button (the add-host row), a shade lighter
    /// than the panel so it reads as a control before it is hovered.
    pub button_bg: [f32; 4],
    pub notice_bg: [f32; 4],
    pub field_bg: [f32; 4],
    pub field_border: [f32; 4],
    pub field_border_focus: [f32; 4],
    pub dialog_bg: [f32; 4],
    pub dialog_border: [f32; 4],
    pub scrim: [f32; 4],
    pub accent: [f32; 4],
    pub text: [u8; 4],
    pub text_muted: [u8; 4],
    pub text_faint: [u8; 4],
    pub text_placeholder: [u8; 4],
    pub danger: [u8; 4],
}

impl ChromeTheme {
    /// Build the palette from the terminal's foreground, background and
    /// accent (`colors.cursor` in rio's config).
    pub fn from_terminal(fg: [u8; 3], bg: [u8; 3], accent: [u8; 3]) -> Self {
        let panel_bg = shade(bg, 0.30);
        let rail_bg = shade(bg, 0.55);
        Self {
            rail_bg: rgba(rail_bg, 1.0),
            rail_marker: rgba(accent, 1.0),
            rail_active_bg: rgba(mix(bg, fg, 0.10), 1.0),
            panel_bg: rgba(panel_bg, 1.0),
            panel_border: rgba(mix(bg, fg, 0.14), 1.0),
            item_hover: rgba(mix(bg, fg, 0.07), 1.0),
            item_selected: rgba(mix(bg, accent, 0.20), 1.0),
            button_bg: rgba(mix(bg, fg, 0.03), 1.0),
            notice_bg: rgba(shade(bg, 0.40), 1.0),
            field_bg: rgba(shade(bg, 0.18), 1.0),
            field_border: rgba(mix(bg, fg, 0.20), 1.0),
            field_border_focus: rgba(accent, 1.0),
            dialog_bg: rgba(shade(bg, 0.12), 1.0),
            dialog_border: rgba(mix(bg, fg, 0.22), 1.0),
            scrim: [0.0, 0.0, 0.0, 0.55],
            accent: rgba(accent, 1.0),
            text: [fg[0], fg[1], fg[2], 255],
            text_muted: [
                mix(bg, fg, 0.62)[0],
                mix(bg, fg, 0.62)[1],
                mix(bg, fg, 0.62)[2],
                255,
            ],
            text_faint: [
                mix(bg, fg, 0.42)[0],
                mix(bg, fg, 0.42)[1],
                mix(bg, fg, 0.42)[2],
                255,
            ],
            text_placeholder: [
                mix(bg, fg, 0.30)[0],
                mix(bg, fg, 0.30)[1],
                mix(bg, fg, 0.30)[2],
                255,
            ],
            danger: [
                (accent[0] as u16 + 120).min(255) as u8,
                (accent[1] as u16 / 2).min(255) as u8,
                (accent[2] as u16 / 2).min(255) as u8,
                255,
            ],
        }
    }
}

impl Default for ChromeTheme {
    /// rio's own defaults: `#edede d`-ish text on a near-black bg.
    fn default() -> Self {
        Self::from_terminal([237, 237, 237], [14, 14, 14], [77, 179, 255])
    }
}

fn rgba(rgb: [u8; 3], alpha: f32) -> [f32; 4] {
    [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
        alpha,
    ]
}

/// Darken by `amount` (0 = unchanged, 1 = black).
fn shade(rgb: [u8; 3], amount: f32) -> [u8; 3] {
    mix(rgb, [0, 0, 0], amount)
}

/// Blend `a` toward `b` by `t`.
fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [blend(a[0], b[0]), blend(a[1], b[1]), blend(a[2], b[2])]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_is_darker_than_the_terminal_and_the_rail_darker_still() {
        let theme =
            ChromeTheme::from_terminal([237, 237, 237], [40, 40, 40], [100, 150, 200]);
        let luma = |c: [f32; 4]| c[0] + c[1] + c[2];
        assert!(luma(theme.rail_bg) < luma(theme.panel_bg));
        assert!(luma(theme.panel_bg) < rgba([40, 40, 40], 1.0).iter().sum::<f32>());
    }

    #[test]
    fn text_is_full_opacity_and_brighter_than_the_placeholders() {
        let theme = ChromeTheme::default();
        assert_eq!(theme.text[3], 255);
        let luma = |c: [u8; 4]| c[0] as u32 + c[1] as u32 + c[2] as u32;
        assert!(luma(theme.text) > luma(theme.text_muted));
        assert!(luma(theme.text_muted) > luma(theme.text_faint));
        assert!(luma(theme.text_faint) > luma(theme.text_placeholder));
    }

    #[test]
    fn the_accent_is_carried_through_unblended() {
        let accent = [10, 200, 30];
        let theme = ChromeTheme::from_terminal([255, 255, 255], [0, 0, 0], accent);
        assert_eq!(theme.rail_marker[0], accent[0] as f32 / 255.0);
        assert_eq!(theme.rail_marker[1], accent[1] as f32 / 255.0);
        assert_eq!(theme.rail_marker[2], accent[2] as f32 / 255.0);
    }

    #[test]
    fn a_white_terminal_theme_does_not_produce_negative_or_overflowing_channels() {
        let theme =
            ChromeTheme::from_terminal([0, 0, 0], [255, 255, 255], [255, 255, 255]);
        for color in [
            theme.rail_bg,
            theme.panel_bg,
            theme.dialog_bg,
            theme.item_selected,
            theme.field_border_focus,
        ] {
            for channel in color {
                assert!((0.0..=1.0).contains(&channel), "{color:?}");
            }
        }
    }
}
