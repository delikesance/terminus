//! Chrome colors — the "violet ink" palette (default) plus the legacy
//! Apple HIG palette.
//!
//! Terminus chrome uses a fixed palette so it matches the product mock
//! rather than fighting terminal theme colors. Terminal content still
//! follows the user's theme.
//!
//! The first block of [`ChromeTheme`] fields are the design tokens
//! (`frame`, `canvas`, `surface`, ...); the rest are legacy names that
//! existing painters use, mapped onto the tokens by [`violet_ink`].
//!
//! [`violet_ink`]: ChromeTheme::violet_ink

/// Colors for the activity bar, the host panel and the dialogs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromeTheme {
    // ---- design tokens (violet ink) ----
    /// App frame: sidebar / panel background (`#14111C`).
    pub frame: [f32; 4],
    /// Deepest surface: terminal canvas, gallery wells (`#0D0B13`).
    pub canvas: [f32; 4],
    /// Input field fill (`#110E18`).
    pub field: [f32; 4],
    /// Card / resting button surface (`#1F1A2B`).
    pub surface: [f32; 4],
    /// Raised surface: secondary button, menus (`#272136`).
    pub raised: [f32; 4],
    /// Selected row / pressed surface (`#2C2440`).
    pub selected: [f32; 4],
    /// Borders and strokes (`#342C48`).
    pub line: [f32; 4],
    /// Hairline dividers (`#221C30`).
    pub divider: [f32; 4],
    /// Dialog background (`#1A1624`).
    pub dialog: [f32; 4],
    /// Dialog border (`#2E2740`).
    pub dialog_line: [f32; 4],
    pub accent_hover: [f32; 4],
    pub accent_press: [f32; 4],
    /// Text / icon colour on an accent fill.
    pub on_accent: [f32; 4],
    /// Destructive fill (`#E8604F`).
    pub danger_fill: [f32; 4],
    pub danger_hover: [f32; 4],
    /// Destructive text on dark surfaces (`#FF8E80`).
    pub danger_text: [u8; 4],
    pub info: [f32; 4],
    pub warning: [f32; 4],
    // ---- legacy names (kept so existing painters compile) ----
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
        let legacy = Self::violet_ink();
        // #18181b appleBg, #111113 sidebar, #222226 card, #2a2a30 hover,
        // #2f2f35 border, #1b1b1e input, #0a84ff accent, #141416 terminal.
        Self {
            // design tokens: the legacy palette has no violet tokens,
            // so reuse the violet ones (only the legacy fields below
            // define the Apple look).
            ..legacy
        }
        .with_apple_legacy()
    }

    fn with_apple_legacy(self) -> Self {
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
            ..self
        }
    }

    /// The "violet ink" palette from the approved redesign. Default.
    pub fn violet_ink() -> Self {
        let frame = rgba([0x14, 0x11, 0x1c], 1.0);
        let canvas = rgba([0x0d, 0x0b, 0x13], 1.0);
        let field = rgba([0x11, 0x0e, 0x18], 1.0);
        let surface = rgba([0x1f, 0x1a, 0x2b], 1.0);
        let raised = rgba([0x27, 0x21, 0x36], 1.0);
        let selected = rgba([0x2c, 0x24, 0x40], 1.0);
        let line = rgba([0x34, 0x2c, 0x48], 1.0);
        let divider = rgba([0x22, 0x1c, 0x30], 1.0);
        let dialog = rgba([0x1a, 0x16, 0x24], 1.0);
        let dialog_line = rgba([0x2e, 0x27, 0x40], 1.0);
        let accent = rgba([0xb9, 0xa4, 0xff], 1.0);
        let danger_text = [0xff, 0x8e, 0x80, 255];
        Self {
            frame,
            canvas,
            field,
            surface,
            raised,
            selected,
            line,
            divider,
            dialog,
            dialog_line,
            accent_hover: rgba([0xcb, 0xbb, 0xff], 1.0),
            accent_press: rgba([0xa4, 0x8d, 0xf5], 1.0),
            on_accent: rgba([0x15, 0x10, 0x2a], 1.0),
            danger_fill: rgba([0xe8, 0x60, 0x4f], 1.0),
            danger_hover: rgba([0xf0, 0x75, 0x63], 1.0),
            danger_text,
            info: rgba([0x8d, 0xbb, 0xf5], 1.0),
            warning: rgba([0xf2, 0xc4, 0x6d], 1.0),
            // legacy mapping
            rail_bg: canvas,
            rail_marker: accent,
            rail_active_bg: rgba([0xb9, 0xa4, 0xff], 0.15),
            panel_bg: frame,
            panel_border: divider,
            item_hover: surface,
            item_selected: selected,
            button_bg: surface,
            notice_bg: field,
            field_bg: field,
            field_border: line,
            field_border_focus: accent,
            dialog_bg: dialog,
            dialog_border: dialog_line,
            dialog_header: surface,
            shell_bg: frame,
            scrim: [0.0, 0.0, 0.0, 0.60],
            accent,
            accent_soft: rgba([0xb9, 0xa4, 0xff], 0.15),
            success: rgba([0x9e, 0xd9, 0xb5], 1.0),
            terminal_bg: canvas,
            text: [0xee, 0xea, 0xf6, 255],
            text_muted: [0xab, 0xa4, 0xbd, 255],
            text_faint: [0x9a, 0x92, 0xae, 255],
            text_placeholder: [0x9a, 0x92, 0xae, 255],
            danger: danger_text,
        }
    }

    /// Build a palette from the terminal's colors (legacy / optional).
    pub fn from_terminal(fg: [u8; 3], bg: [u8; 3], accent: [u8; 3]) -> Self {
        let _ = (fg, bg, accent);
        Self::violet_ink()
    }
}

impl Default for ChromeTheme {
    fn default() -> Self {
        Self::violet_ink()
    }
}

/// Convert a unit-float colour to the byte form `draw_text` takes.
pub fn text_color(c: [f32; 4]) -> [u8; 4] {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [b(c[0]), b(c[1]), b(c[2]), b(c[3])]
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

    fn hex(c: [f32; 4], hex: [u8; 3]) {
        for i in 0..3 {
            assert!((c[i] - hex[i] as f32 / 255.0).abs() < 0.002, "{c:?} vs {hex:?}");
        }
        assert!((c[3] - 1.0).abs() < 0.002);
    }

    #[test]
    fn violet_ink_matches_design_tokens() {
        let t = ChromeTheme::violet_ink();
        hex(t.frame, [0x14, 0x11, 0x1c]);
        hex(t.canvas, [0x0d, 0x0b, 0x13]);
        hex(t.field, [0x11, 0x0e, 0x18]);
        hex(t.surface, [0x1f, 0x1a, 0x2b]);
        hex(t.raised, [0x27, 0x21, 0x36]);
        hex(t.selected, [0x2c, 0x24, 0x40]);
        hex(t.line, [0x34, 0x2c, 0x48]);
        hex(t.divider, [0x22, 0x1c, 0x30]);
        hex(t.dialog, [0x1a, 0x16, 0x24]);
        hex(t.dialog_line, [0x2e, 0x27, 0x40]);
        hex(t.accent, [0xb9, 0xa4, 0xff]);
        hex(t.accent_hover, [0xcb, 0xbb, 0xff]);
        hex(t.accent_press, [0xa4, 0x8d, 0xf5]);
        hex(t.on_accent, [0x15, 0x10, 0x2a]);
        hex(t.success, [0x9e, 0xd9, 0xb5]);
        hex(t.danger_fill, [0xe8, 0x60, 0x4f]);
        hex(t.danger_hover, [0xf0, 0x75, 0x63]);
        hex(t.info, [0x8d, 0xbb, 0xf5]);
        hex(t.warning, [0xf2, 0xc4, 0x6d]);
        assert_eq!(t.text, [0xee, 0xea, 0xf6, 255]);
        assert_eq!(t.text_muted, [0xab, 0xa4, 0xbd, 255]);
        assert_eq!(t.text_faint, [0x9a, 0x92, 0xae, 255]);
        assert_eq!(t.danger_text, [0xff, 0x8e, 0x80, 255]);
    }

    #[test]
    fn violet_ink_is_the_default_and_remaps_legacy_fields() {
        let t = ChromeTheme::default();
        assert_eq!(t, ChromeTheme::violet_ink());
        hex(t.panel_bg, [0x14, 0x11, 0x1c]);
        hex(t.terminal_bg, [0x0d, 0x0b, 0x13]);
        hex(t.field_bg, [0x11, 0x0e, 0x18]);
        hex(t.item_selected, [0x2c, 0x24, 0x40]);
        hex(t.dialog_bg, [0x1a, 0x16, 0x24]);
        hex(t.accent, [0xb9, 0xa4, 0xff]);
        assert_eq!(t.danger, t.danger_text);
    }

    #[test]
    fn surface_ladder_gets_lighter() {
        let t = ChromeTheme::violet_ink();
        let l = |c: [f32; 4]| c[0] + c[1] + c[2];
        assert!(l(t.canvas) < l(t.frame));
        assert!(l(t.frame) < l(t.surface));
        assert!(l(t.surface) < l(t.raised));
        assert!(l(t.raised) < l(t.selected));
        assert!(l(t.selected) < l(t.line));
    }

    #[test]
    fn text_color_converts_unit_floats_to_bytes() {
        assert_eq!(text_color([1.0, 0.0, 0.5, 1.0]), [255, 0, 128, 255]);
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
