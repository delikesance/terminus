//! Input component painter and gallery: text field (6 states), variants
//! (password, mono, select, search, command bar, textarea) and the port pair.
//!
//! All geometry comes from `terminus_ui::components::input`; this file only
//! turns rects into sugarloaf primitives.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::input::{
    self as ui, FieldKind, FieldLayout, FieldState, SearchKind, SearchLayout,
};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, radius, space};

use super::Layer;
use crate::renderer::chrome::{draw_icon, paint_flat, paint_surface_stroke};
use crate::renderer::ui_text::{
    draw_mono_text, draw_ui_text, measure_mono_text, measure_ui_text, UiWeight,
};

const ORDER: u8 = 7;
const DEPTH_RING: f32 = 0.05;
const BOX_ABOVE_RING: f32 = 0.05;
const CARET_ABOVE_BOX: f32 = 0.02;
const DEPTH_BOX: f32 = DEPTH_RING + BOX_ABOVE_RING;

/// Content of one text field.
pub struct FieldContent<'a> {
    pub kind: FieldKind,
    pub state: FieldState,
    pub label: Option<&'a str>,
    /// Text as displayed (already masked for passwords). Empty shows the placeholder.
    pub value: &'a str,
    pub placeholder: &'a str,
    pub helper: Option<&'a str>,
    /// Password eye toggled on.
    pub revealed: bool,
    /// Measured width of the text before the caret; `Some` draws the caret.
    pub caret_prefix_width: Option<f32>,
}

fn f32c(c: [u8; 4]) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

/// `fg` at `alpha` over opaque `bg`.
fn over(bg: [f32; 4], fg: [f32; 4], alpha: f32) -> [f32; 4] {
    let a = alpha * fg[3];
    [
        fg[0] * a + bg[0] * (1.0 - a),
        fg[1] * a + bg[1] * (1.0 - a),
        fg[2] * a + bg[2] * (1.0 - a),
        1.0,
    ]
}

fn over_u8(bg: [f32; 4], fg: [u8; 4], alpha: f32) -> [u8; 4] {
    let c = over(bg, f32c(fg), alpha);
    [
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
        255,
    ]
}

fn text_top(rect: &Rect, size: f32) -> f32 {
    rect.y + (rect.height - size) / 2.0 - size * 0.12
}

/// Paint one labelled field into `layout`. `backdrop` is the opaque colour
/// behind it (used to fake the 50% disabled opacity).
pub fn paint_field(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    layout: &FieldLayout,
    c: &FieldContent,
    backdrop: [f32; 4],
    device_scale: f32,
) {
    let layer = Layer {
        order: ORDER,
        depth: DEPTH_RING,
        backdrop,
    };
    paint_field_at(sugarloaf, theme, layout, c, layer, device_scale);
}

/// [`paint_field`] on an explicit order/depth; `layer.depth` is the focus
/// ring, the box and caret sit above it.
pub fn paint_field_at(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    layout: &FieldLayout,
    c: &FieldContent,
    layer: Layer,
    device_scale: f32,
) {
    let Layer {
        order,
        depth,
        backdrop,
    } = layer;
    let box_depth = depth + BOX_ABOVE_RING;
    let disabled = c.state == FieldState::Disabled;
    let k = if disabled { ui::DISABLED_OPACITY } else { 1.0 };
    let col = |x: [f32; 4]| if disabled { over(backdrop, x, k) } else { x };
    let colu = |x: [u8; 4]| if disabled { over_u8(backdrop, x, k) } else { x };

    if let Some(label) = c.label.zip(layout.label) {
        draw_ui_text(
            sugarloaf,
            label.1.x,
            text_top(&label.1, ui::LABEL_FONT),
            label.0,
            ui::LABEL_FONT,
            colu(theme.text),
            UiWeight::Medium,
        );
    }

    if c.state == FieldState::Focus {
        let ring = over(backdrop, theme.accent, ui::FOCUS_RING_ALPHA);
        paint_surface_stroke(
            sugarloaf,
            &layout.ring,
            ring,
            None,
            radius::CONTROL + ui::FOCUS_RING,
            0.0,
            depth,
            order,
            false,
        );
    }
    paint_surface_stroke(
        sugarloaf,
        &layout.box_rect,
        col(theme.field),
        Some(col(ui::border_color(theme, c.state))),
        radius::CONTROL,
        1.0,
        box_depth,
        order,
        false,
    );

    let mono = matches!(c.kind, FieldKind::Mono | FieldKind::Textarea);
    let font = c.kind.value_font();
    let (shown, color) = if c.value.is_empty() {
        (c.placeholder, colu(theme.text_faint))
    } else {
        (c.value, colu(theme.text))
    };
    let t = &layout.text;
    let ty = if c.kind == FieldKind::Textarea {
        t.y - font * 0.12
    } else {
        text_top(t, font)
    };
    if mono {
        draw_mono_text(sugarloaf, t.x, ty, shown, font, color, UiWeight::Regular);
    } else {
        draw_ui_text(sugarloaf, t.x, ty, shown, font, color, UiWeight::Regular);
    }

    if let (Some(w), false) = (c.caret_prefix_width, disabled) {
        paint_flat(
            sugarloaf,
            &ui::caret_rect(layout, c.kind, w),
            theme.accent,
            box_depth + CARET_ABOVE_BOX,
            order,
        );
    }

    if let (Some(slot), Some(icon)) = (layout.trailing, c.kind.trailing_icon(c.revealed))
    {
        draw_icon(
            sugarloaf,
            icon,
            IconPlacement::new(slot.x, slot.y, slot.width),
            col(f32c(theme.text_muted)),
            device_scale,
        );
    }

    if let (Some(text), Some(r)) = (c.helper, layout.helper) {
        let color = if c.state == FieldState::Error {
            theme.danger_text
        } else {
            theme.text_faint
        };
        draw_ui_text(
            sugarloaf,
            r.x,
            text_top(&r, ui::HELPER_FONT),
            text,
            ui::HELPER_FONT,
            colu(color),
            UiWeight::Regular,
        );
    }
}

/// Paint a search box or command bar. `text` empty shows `placeholder`.
pub fn paint_search(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    layout: &SearchLayout,
    kind: SearchKind,
    text: &str,
    placeholder: &str,
    device_scale: f32,
) {
    paint_surface_stroke(
        sugarloaf,
        &layout.box_rect,
        theme.surface,
        None,
        radius::CONTROL,
        0.0,
        DEPTH_BOX,
        ORDER,
        false,
    );
    draw_icon(
        sugarloaf,
        Icon::Search,
        IconPlacement::new(layout.icon.x, layout.icon.y, layout.icon.width),
        f32c(theme.text_muted),
        device_scale,
    );
    let font = if kind == SearchKind::Search {
        ui::SEARCH_FONT
    } else {
        ui::COMMAND_FONT
    };
    let (shown, color) = if text.is_empty() {
        (placeholder, theme.text_muted)
    } else {
        (text, theme.text)
    };
    draw_ui_text(
        sugarloaf,
        layout.text.x,
        text_top(&layout.text, font),
        shown,
        font,
        color,
        UiWeight::Regular,
    );
    if let Some(h) = layout.hint {
        draw_mono_text(
            sugarloaf,
            h.x,
            text_top(&h, ui::COMMAND_HINT_FONT),
            ui::COMMAND_HINT,
            ui::COMMAND_HINT_FONT,
            theme.text_muted,
            UiWeight::Regular,
        );
    }
}

fn caption(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme, x: f32, y: f32, text: &str) {
    draw_ui_text(
        sugarloaf,
        x,
        y,
        text,
        font_size::CAPTION,
        theme.text_faint,
        UiWeight::Regular,
    );
}

const WELL_PAD: f32 = 28.0;
const WELL_RADIUS: f32 = 16.0;
const WELL_TITLE: f32 = 18.0;

/// Paints a well with title/subtitle; returns (content_y, total_height).
fn well(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
    title: &str,
    subtitle: Option<&str>,
    content_h: f32,
) -> f32 {
    let head = WELL_TITLE * 1.3 + subtitle.map_or(0.0, |_| 6.0 + 20.0);
    let h = WELL_PAD + head + space::XL + content_h + WELL_PAD;
    paint_surface_stroke(
        sugarloaf,
        &Rect::new(origin.0, origin.1, width, h),
        theme.canvas,
        None,
        WELL_RADIUS,
        0.0,
        0.02,
        ORDER,
        false,
    );
    draw_ui_text(
        sugarloaf,
        origin.0 + WELL_PAD,
        origin.1 + WELL_PAD,
        title,
        WELL_TITLE,
        theme.text,
        UiWeight::SemiBold,
    );
    if let Some(s) = subtitle {
        draw_ui_text(
            sugarloaf,
            origin.0 + WELL_PAD,
            origin.1 + WELL_PAD + WELL_TITLE * 1.3 + 6.0,
            s,
            font_size::LABEL,
            theme.text_muted,
            UiWeight::Regular,
        );
    }
    h
}

fn cell_field_h(has_helper: bool, kind: FieldKind) -> f32 {
    ui::field_layout((0.0, 0.0), 100.0, kind, true, has_helper)
        .total
        .height
}

/// Paint the `input` gallery at `origin` within `width`; returns the height used.
pub fn paint_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
) -> f32 {
    let scale = sugarloaf.scale_factor();
    let mut y = origin.1;
    y += super::paint_section_title(sugarloaf, theme, (origin.0, y), "Input") + space::LG;
    let inner_w = (width - 2.0 * WELL_PAD).max(0.0);
    let cap = ui::CAPTION_HEIGHT + ui::CAPTION_GAP;
    let backdrop = theme.canvas;

    // ---- Text field: 6 states ----
    let row_h = cap + cell_field_h(true, FieldKind::Text);
    let content_h = 2.0 * row_h + ui::GRID_ROW_GAP;
    let h = well(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        "Text field",
        None,
        content_h,
    );
    let gy = y + WELL_PAD + WELL_TITLE * 1.3 + space::XL;
    #[allow(clippy::type_complexity)]
    let states: [(&str, FieldState, &str, &str, Option<&str>, &str); 6] = [
        (
            "Default",
            FieldState::Default,
            "Address",
            "",
            Some("Hostname or IP address."),
            "",
        ),
        ("Hover", FieldState::Hover, "Address", "", None, ""),
        (
            "Focus",
            FieldState::Focus,
            "Address",
            "137.74.42.2",
            None,
            "",
        ),
        (
            "Filled",
            FieldState::Filled,
            "Address",
            "137.74.42.224",
            None,
            "",
        ),
        (
            "Error",
            FieldState::Error,
            "Port",
            "22a",
            Some("Port must be a number between 1 and 65535."),
            "",
        ),
        ("Disabled", FieldState::Disabled, "Address", "", None, ""),
    ];
    for (i, (cap_text, state, label, value, helper, _)) in states.iter().enumerate() {
        let cell = ui::grid_cell((origin.0 + WELL_PAD, gy), inner_w, i, row_h);
        caption(sugarloaf, theme, cell.x, cell.y, cap_text);
        let layout = ui::field_layout(
            (cell.x, cell.y + cap),
            cell.width,
            FieldKind::Text,
            true,
            helper.is_some(),
        );
        let caret = if *state == FieldState::Focus {
            let w =
                measure_ui_text(sugarloaf, value, ui::SANS_VALUE_FONT, UiWeight::Regular);
            Some(w + 1.0)
        } else {
            None
        };
        paint_field(
            sugarloaf,
            theme,
            &layout,
            &FieldContent {
                kind: FieldKind::Text,
                state: *state,
                label: Some(label),
                value,
                placeholder: "server.example.com",
                helper: *helper,
                revealed: false,
                caret_prefix_width: caret,
            },
            backdrop,
            scale,
        );
    }
    y += h + super::SECTION_GAP.min(20.0);

    // ---- Variants ----
    let row1 = cap + cell_field_h(false, FieldKind::Text);
    let row2 = cap + cell_field_h(false, FieldKind::Textarea);
    let content_h = row1 + ui::GRID_ROW_GAP + row2;
    let h = well(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        "Variants",
        Some("Labels sit above the field, always visible. Help text below; it turns into the error message when validation fails."),
        content_h,
    );
    let gy = y + WELL_PAD + WELL_TITLE * 1.3 + 26.0 + space::XL;
    let x0 = origin.0 + WELL_PAD;
    let cell = |i: usize| {
        let row_y = if i < 3 {
            gy
        } else {
            gy + row1 + ui::GRID_ROW_GAP
        };
        ui::grid_cell((x0, row_y), inner_w, i % 3, if i < 3 { row1 } else { row2 })
    };
    let simple = [
        (
            0,
            "Password",
            FieldKind::Password,
            "Vault passphrase",
            ui::mask(12),
            "",
        ),
        (
            1,
            "Mono value",
            FieldKind::Mono,
            "Database file",
            "/srv/terminus/shared.db".into(),
            "",
        ),
        (
            2,
            "Select",
            FieldKind::Select,
            "Key",
            "id_ed25519".into(),
            "",
        ),
    ];
    for (i, cap_text, kind, label, value, _) in simple {
        let c = cell(i);
        caption(sugarloaf, theme, c.x, c.y, cap_text);
        let layout = ui::field_layout((c.x, c.y + cap), c.width, kind, true, false);
        paint_field(
            sugarloaf,
            theme,
            &layout,
            &FieldContent {
                kind,
                state: FieldState::Filled,
                label: Some(label),
                value: &value,
                placeholder: "",
                helper: None,
                revealed: false,
                caret_prefix_width: None,
            },
            backdrop,
            scale,
        );
    }
    for (i, cap_text, kind, text, ph) in [
        (3, "Search", SearchKind::Search, "", "Filter snippets"),
        (
            4,
            "Command bar",
            SearchKind::CommandBar,
            "",
            ui::COMMAND_PLACEHOLDER,
        ),
    ] {
        let c = cell(i);
        caption(sugarloaf, theme, c.x, c.y, cap_text);
        let hint_w = if kind == SearchKind::CommandBar {
            measure_mono_text(
                sugarloaf,
                ui::COMMAND_HINT,
                ui::COMMAND_HINT_FONT,
                UiWeight::Regular,
            )
        } else {
            0.0
        };
        let layout = ui::search_layout((c.x, c.y + cap), c.width, kind, hint_w);
        paint_search(sugarloaf, theme, &layout, kind, text, ph, scale);
    }
    {
        let c = cell(5);
        caption(sugarloaf, theme, c.x, c.y, "Textarea \u{b7} mono");
        let layout =
            ui::field_layout((c.x, c.y + cap), c.width, FieldKind::Textarea, true, false);
        let value = "sudo systemctl restart nginx";
        let w =
            measure_mono_text(sugarloaf, value, ui::MONO_VALUE_FONT, UiWeight::Regular);
        paint_field(
            sugarloaf,
            theme,
            &layout,
            &FieldContent {
                kind: FieldKind::Textarea,
                state: FieldState::Focus,
                label: Some("Command"),
                value,
                placeholder: "",
                helper: None,
                revealed: false,
                caret_prefix_width: Some(w),
            },
            backdrop,
            scale,
        );
    }
    y += h + super::SECTION_GAP.min(20.0);

    // ---- Port pair ----
    let pair_h = cell_field_h(false, FieldKind::Mono);
    let h = well(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        "Port pair",
        Some("Used by the tunnel form."),
        pair_h,
    );
    let gy = y + WELL_PAD + WELL_TITLE * 1.3 + 26.0 + space::XL;
    let p = ui::port_pair_layout((x0, gy));
    for (layout, label, value) in [
        (&p.local, "Local port", "5432"),
        (&p.dest, "Destination on jerem prod", "localhost"),
        (&p.port, "Port", "5432"),
    ] {
        paint_field(
            sugarloaf,
            theme,
            layout,
            &FieldContent {
                kind: FieldKind::Mono,
                state: FieldState::Filled,
                label: Some(label),
                value,
                placeholder: "",
                helper: None,
                revealed: false,
                caret_prefix_width: None,
            },
            backdrop,
            scale,
        );
    }
    // Sora has no arrow glyph (it drew a tofu box); Martian Mono does.
    let aw = measure_mono_text(
        sugarloaf,
        "\u{2192}",
        ui::MONO_VALUE_FONT,
        UiWeight::Regular,
    );
    draw_mono_text(
        sugarloaf,
        p.arrow.x + (p.arrow.width - aw) / 2.0,
        text_top(&p.arrow, ui::MONO_VALUE_FONT),
        "\u{2192}",
        ui::MONO_VALUE_FONT,
        theme.text_muted,
        UiWeight::Regular,
    );
    y += h;

    y - origin.1
}
