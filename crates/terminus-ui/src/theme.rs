//! Chrome colors — Apple HIG flat dark palette.
//!
//! Terminus chrome uses a fixed zinc/Apple palette so it matches the
//! product mock rather than fighting terminal theme colors. Terminal
//! content still follows the user's theme.

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
    /// Resting surface of a card / button.
    pub button_bg: [f32; 4],
    pub notice_bg: [f32; 4],
    pub field_bg: [f32; 4],
    pub field_border: [f32; 4],
    pub field_border_focus: [f32; 4],
    pub dialog_bg: [f32; 4],
    pub dialog_border: [f32; 4],
    pub dialog_header: [f32; 4],
    /// Main content / settings pane (`#18181b`).
    pub shell_bg: [f32; 4],
    pub scrim: [f32; 4],
    pub accent: [f32; 4],
    pub accent_soft: [f32; 4],
    pub success: [f32; 4],
    pub terminal_bg: [f32; 4],
    pub text: [u8; 4],
    pub text_muted: [u8; 4],
    pub text_faint: [u8; 4],
    pub text_placeholder: [u8; 4],
    pub danger: [u8; 4],
}

impl ChromeTheme {
    /// Flat Apple HIG dark palette from the Terminus Pro mock.
    pub fn apple_hig() -> Self {
        // #18181b appleBg, #111113 sidebar, #222226 card, #2a2a30 hover,
        // #2f2f35 border, #1b1b1e input, #0a84ff accent, #141416 terminal.
        Self {
            rail_bg: rgba([0x11, 0x11, 0x13], 1.0),
            rail_marker: rgba([0x0a, 0x84, 0xff], 1.0),
            rail_active_bg: rgba([0x0a, 0x84, 0xff], 0.15),
            panel_bg: rgba([0x11, 0x11, 0x13], 1.0),
            panel_border: rgba([0x2f, 0x2f, 0x35], 1.0),
            item_hover: rgba([0x2a, 0x2a, 0x30], 1.0),
            item_selected: rgba([0x0a, 0x84, 0xff], 0.15),
            button_bg: rgba([0x22, 0x22, 0x26], 1.0),
            notice_bg: rgba([0x1b, 0x1b, 0x1e], 1.0),
            field_bg: rgba([0x1b, 0x1b, 0x1e], 1.0),
            field_border: rgba([0x2f, 0x2f, 0x35], 1.0),
            field_border_focus: rgba([0x0a, 0x84, 0xff], 1.0),
            dialog_bg: rgba([0x11, 0x11, 0x13], 1.0),
            dialog_border: rgba([0x2f, 0x2f, 0x35], 1.0),
            dialog_header: rgba([0x22, 0x22, 0x26], 1.0),
            /// Main shell / settings content (`appleBg`).
            shell_bg: rgba([0x18, 0x18, 0x1b], 1.0),
            scrim: [0.0, 0.0, 0.0, 0.60],
            accent: rgba([0x0a, 0x84, 0xff], 1.0),
            accent_soft: rgba([0x0a, 0x84, 0xff], 0.15),
            success: rgba([0x34, 0xd3, 0x99], 1.0), // emerald-400
            terminal_bg: rgba([0x14, 0x14, 0x16], 1.0),
            text: [0xf1, 0xf5, 0xf9, 255],       // slate-100
            text_muted: [0x94, 0xa3, 0xb8, 255], // slate-400
            text_faint: [0x64, 0x74, 0x8b, 255], // slate-500
            text_placeholder: [0x64, 0x74, 0x8b, 255],
            danger: [0xf8, 0x71, 0x71, 255], // red-400
        }
    }

    /// Build a palette from the terminal's colors (legacy / optional).
    pub fn from_terminal(fg: [u8; 3], bg: [u8; 3], accent: [u8; 3]) -> Self {
        let _ = (fg, bg, accent);
        Self::apple_hig()
    }
}

impl Default for ChromeTheme {
    fn default() -> Self {
        Self::apple_hig()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_hig_matches_mock_hex_tokens() {
        let theme = ChromeTheme::apple_hig();
        let eq = |c: [f32; 4], hex: [u8; 3]| {
            assert!((c[0] - hex[0] as f32 / 255.0).abs() < 0.002);
            assert!((c[1] - hex[1] as f32 / 255.0).abs() < 0.002);
            assert!((c[2] - hex[2] as f32 / 255.0).abs() < 0.002);
            assert!((c[3] - 1.0).abs() < 0.002);
        };
        eq(theme.panel_bg, [0x11, 0x11, 0x13]);
        eq(theme.rail_bg, [0x11, 0x11, 0x13]);
        eq(theme.button_bg, [0x22, 0x22, 0x26]);
        eq(theme.item_hover, [0x2a, 0x2a, 0x30]);
        eq(theme.panel_border, [0x2f, 0x2f, 0x35]);
        eq(theme.field_bg, [0x1b, 0x1b, 0x1e]);
        eq(theme.accent, [0x0a, 0x84, 0xff]);
        eq(theme.shell_bg, [0x18, 0x18, 0x1b]);
        eq(theme.terminal_bg, [0x14, 0x14, 0x16]);
        assert!((theme.scrim[3] - 0.60).abs() < 0.001);
        assert!((theme.accent_soft[3] - 0.15).abs() < 0.001);
    }

    #[test]
    fn text_hierarchy_is_bright_to_faint() {
        let theme = ChromeTheme::default();
        let luma = |c: [u8; 4]| c[0] as u32 + c[1] as u32 + c[2] as u32;
        assert!(luma(theme.text) > luma(theme.text_muted));
        assert!(luma(theme.text_muted) > luma(theme.text_faint));
    }

    #[test]
    fn channels_stay_in_unit_range() {
        let theme = ChromeTheme::apple_hig();
        for color in [
            theme.rail_bg,
            theme.panel_bg,
            theme.dialog_bg,
            theme.accent_soft,
            theme.scrim,
        ] {
            for channel in color {
                assert!((0.0..=1.0).contains(&channel), "{color:?}");
            }
        }
    }
}
