//! Button painter and gallery (kinds x states, sizes, icon buttons).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{
    colors, ButtonKind, ButtonSize, ButtonSpec, ButtonState,
};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::{text_color, ChromeTheme};
use terminus_ui::tokens::{font_size, space};

use super::Layer;
use crate::renderer::chrome::draw_icon;
use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};

const WELL_PAD: f32 = 28.0;
const WELL_RADIUS: f32 = 16.0;
const WELL_GAP: f32 = 20.0;
const FIRST_COL: f32 = 170.0;
const COL_GAP: f32 = 24.0;
const ROW_GAP: f32 = 20.0;

const ORDER: u8 = 2;
const DEPTH: f32 = 0.06;

fn with_alpha(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0], c[1], c[2], c[3] * k]
}

fn weight(kind: ButtonKind) -> UiWeight {
    if kind.semibold() {
        UiWeight::SemiBold
    } else {
        UiWeight::Medium
    }
}

/// Measure `label` for `kind`/`size` and build the spec at `origin`.
pub fn label_spec(
    sugarloaf: &mut Sugarloaf,
    origin: (f32, f32),
    kind: ButtonKind,
    size: ButtonSize,
    label: &str,
    icon: bool,
) -> ButtonSpec {
    let w = measure_ui_text(sugarloaf, label, size.font_size(), weight(kind));
    ButtonSpec::label(origin, kind, size, w, icon)
}

/// Paint one button. `label` is the visible text, or the accessible name
/// (not drawn) for an icon-only button.
pub fn paint_button(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    spec: &ButtonSpec,
    state: ButtonState,
    label: &str,
    icon: Option<Icon>,
) {
    let layer = Layer {
        order: ORDER,
        depth: DEPTH,
        backdrop: theme.canvas,
    };
    paint_button_on(sugarloaf, theme, spec, state, label, icon, layer);
}

/// [`paint_button`] on an explicit [`Layer`] (inside a dialog, a menu...).
pub fn paint_button_on(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    spec: &ButtonSpec,
    state: ButtonState,
    label: &str,
    icon: Option<Icon>,
    layer: Layer,
) {
    let (order, depth) = (layer.order, layer.depth);
    let opacity = state.opacity();
    let c = colors(theme, spec.kind, state);
    let rect = spec.rect();
    if state.shows_focus_ring() {
        let ring = spec.focus_ring_rect();
        sugarloaf.rounded_rect(
            None,
            ring.x,
            ring.y,
            ring.width,
            ring.height,
            theme.accent,
            depth,
            spec.focus_ring_radius(),
            order,
        );
        let gap = spec.focus_gap_rect();
        sugarloaf.rounded_rect(
            None,
            gap.x,
            gap.y,
            gap.width,
            gap.height,
            layer.backdrop,
            depth + 0.001,
            spec.focus_gap_radius(),
            order,
        );
    }
    if let Some(fill) = c.fill {
        sugarloaf.rounded_rect(
            None,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            with_alpha(fill, opacity),
            depth + 0.002,
            spec.radius(),
            order,
        );
    }
    let fg = with_alpha(c.fg, opacity);
    if let (Some(icon), Some(r)) = (icon, spec.icon_rect()) {
        let scale = sugarloaf.scale_factor();
        draw_icon(
            sugarloaf,
            icon,
            IconPlacement::new(r.x, r.y, r.width),
            fg,
            scale,
        );
    }
    if let Some((x, y)) = spec.label_origin() {
        draw_ui_text(
            sugarloaf,
            x,
            y,
            label,
            spec.size.font_size(),
            text_color(fg),
            weight(spec.kind),
        );
    }
}

/// Paint a `kind`/`size` button that fills the layout `rect` (width shared
/// with hit-testing, centred vertically) on an explicit [`Layer`].
#[allow(clippy::too_many_arguments)]
pub fn paint_button_in_rect(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    kind: ButtonKind,
    size: ButtonSize,
    state: ButtonState,
    label: &str,
    layer: Layer,
) {
    let spec = ButtonSpec::in_rect(*rect, kind, size);
    paint_button_on(sugarloaf, theme, &spec, state, label, None, layer);
}

/// Resolve and paint with the pointer / keyboard flags a screen tracks.
#[allow(clippy::too_many_arguments, dead_code)]
pub fn paint_button_live(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    spec: &ButtonSpec,
    hovered: bool,
    pressed: bool,
    focused: bool,
    disabled: bool,
    label: &str,
    icon: Option<Icon>,
) {
    let state = ButtonState::resolve(hovered, pressed, focused, disabled);
    paint_button(sugarloaf, theme, spec, state, label, icon);
}

fn kind_name(k: ButtonKind) -> &'static str {
    match k {
        ButtonKind::Primary => "Primary",
        ButtonKind::Secondary => "Secondary",
        ButtonKind::Danger => "Danger",
        ButtonKind::Text => "Text",
        ButtonKind::Quiet => "Quiet",
    }
}

fn state_name(s: ButtonState) -> &'static str {
    match s {
        ButtonState::Default => "Default",
        ButtonState::Hover => "Hover",
        ButtonState::Pressed => "Pressed",
        ButtonState::Focus => "Focus",
        ButtonState::Disabled => "Disabled",
    }
}

struct Well {
    rect: Rect,
    /// Top of the content area (below title and note).
    y: f32,
    x: f32,
    cols: Vec<f32>,
}

#[allow(clippy::too_many_arguments)]
fn begin_well(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
    height: f32,
    title: &str,
    note: &str,
    ncols: usize,
) -> Well {
    sugarloaf.rounded_rect(
        None,
        origin.0,
        origin.1,
        width,
        height,
        theme.canvas,
        0.05,
        WELL_RADIUS,
        1,
    );
    let x = origin.0 + WELL_PAD;
    let mut y = origin.1 + WELL_PAD;
    draw_ui_text(sugarloaf, x, y, title, 18.0, theme.text, UiWeight::SemiBold);
    y += 18.0 + 6.0;
    draw_ui_text(
        sugarloaf,
        x,
        y,
        note,
        font_size::LABEL,
        theme.text_muted,
        UiWeight::Regular,
    );
    y += font_size::LABEL + 4.0 + WELL_GAP;
    let inner = width - 2.0 * WELL_PAD;
    let col_w = ((inner - FIRST_COL - COL_GAP * ncols as f32) / ncols as f32).max(40.0);
    let mut cols = Vec::new();
    for i in 0..ncols {
        cols.push(x + FIRST_COL + COL_GAP + i as f32 * (col_w + COL_GAP));
    }
    Well {
        rect: Rect::new(origin.0, origin.1, width, height),
        y,
        x,
        cols,
    }
}

fn header_row(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    well: &Well,
    heads: &[&str],
) -> f32 {
    for (i, h) in heads.iter().enumerate() {
        draw_ui_text(
            sugarloaf,
            well.cols[i],
            well.y,
            h,
            font_size::CAPTION,
            theme.text_muted,
            UiWeight::Medium,
        );
    }
    font_size::CAPTION + ROW_GAP
}

fn row_label(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    well: &Well,
    y: f32,
    h: f32,
    s: &str,
) {
    draw_ui_text(
        sugarloaf,
        well.x,
        y + (h - font_size::LABEL) * 0.5,
        s,
        font_size::LABEL,
        theme.text,
        UiWeight::Medium,
    );
}

/// Height of a well with `rows` rows of height `row_h` and a header.
fn well_height(rows: usize, row_h: f32, has_header: bool) -> f32 {
    let head = if has_header {
        font_size::CAPTION + ROW_GAP
    } else {
        0.0
    };
    let body = rows as f32 * row_h + rows.saturating_sub(1) as f32 * ROW_GAP;
    // title + note block, then grid, with focus-ring slack under the last row.
    2.0 * WELL_PAD + 18.0 + 6.0 + font_size::LABEL + 4.0 + WELL_GAP + head + body
}

const KIND_NOTE: &str = "One primary per surface. Danger only for an action that destroys something; Text for a secondary action that sits inside a list.";
const SIZE_NOTE: &str =
    "Large in dialogs and empty states. Medium inside list rows. Small only in dense toolbars.";
const ICON_NOTE: &str =
    "Every icon button carries an accessible name and shows it as a tooltip on hover.";

pub fn paint_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
) -> f32 {
    let mut y = origin.1
        + super::paint_section_title(sugarloaf, theme, origin, "Button")
        + space::LG;

    // ---- Kinds x states ----
    let rows: [(ButtonKind, &str, Option<Icon>); 5] = [
        (ButtonKind::Primary, "Save server", None),
        (ButtonKind::Secondary, "Cancel", None),
        (ButtonKind::Danger, "Delete", None),
        (ButtonKind::Text, "Add tunnel", Some(Icon::Plus)),
        (ButtonKind::Quiet, "Settings", None),
    ];
    let h = well_height(rows.len(), ButtonSize::Large.height(), true);
    let well = begin_well(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        h,
        "Kinds × states",
        KIND_NOTE,
        5,
    );
    let mut ry =
        well.y + header_row(sugarloaf, theme, &well, &ButtonState::ALL.map(state_name));
    for (kind, label, icon) in rows {
        let bh = ButtonSize::Large.height();
        row_label(sugarloaf, theme, &well, ry, bh, kind_name(kind));
        for (i, state) in ButtonState::ALL.iter().enumerate() {
            let spec = label_spec(
                sugarloaf,
                (well.cols[i], ry),
                kind,
                ButtonSize::Large,
                label,
                icon.is_some(),
            );
            paint_button(sugarloaf, theme, &spec, *state, label, icon);
        }
        ry += bh + ROW_GAP;
    }
    y += well.rect.height + space::XL;

    // ---- Sizes ----
    let rows: [(&str, ButtonKind, &str, Option<Icon>); 3] = [
        ("Primary", ButtonKind::Primary, "New snippet", None),
        ("Secondary", ButtonKind::Secondary, "Copy public key", None),
        (
            "With icon",
            ButtonKind::Primary,
            "Add server",
            Some(Icon::Plus),
        ),
    ];
    let h = well_height(rows.len(), ButtonSize::Large.height(), true);
    let well = begin_well(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        h,
        "Sizes",
        SIZE_NOTE,
        3,
    );
    let mut ry = well.y
        + header_row(
            sugarloaf,
            theme,
            &well,
            &["Large · 44", "Medium · 36", "Small · 30"],
        );
    for (name, kind, label, icon) in rows {
        let bh = ButtonSize::Large.height();
        row_label(sugarloaf, theme, &well, ry, bh, name);
        for (i, size) in ButtonSize::ALL.iter().enumerate() {
            // Align centres of the three sizes on the row.
            let by = ry + (bh - size.height()) * 0.5;
            let spec = label_spec(
                sugarloaf,
                (well.cols[i], by),
                kind,
                *size,
                label,
                icon.is_some(),
            );
            paint_button(sugarloaf, theme, &spec, ButtonState::Default, label, icon);
        }
        ry += bh + ROW_GAP;
    }
    y += well.rect.height + space::XL;

    // ---- Icon buttons ----
    let rows: [(&str, ButtonKind, ButtonSize, &str, Icon); 3] = [
        (
            "Quiet · 36",
            ButtonKind::Quiet,
            ButtonSize::Medium,
            "Split right",
            Icon::Columns2,
        ),
        (
            "Secondary · 36",
            ButtonKind::Secondary,
            ButtonSize::Medium,
            "Delete key",
            Icon::Trash2,
        ),
        (
            "Quiet · 30",
            ButtonKind::Quiet,
            ButtonSize::Small,
            "Close",
            Icon::X,
        ),
    ];
    let h = well_height(rows.len(), ButtonSize::Medium.height(), true);
    let well = begin_well(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        h,
        "Icon buttons",
        ICON_NOTE,
        5,
    );
    let mut ry =
        well.y + header_row(sugarloaf, theme, &well, &ButtonState::ALL.map(state_name));
    for (name, kind, size, label, icon) in rows {
        let bh = ButtonSize::Medium.height();
        row_label(sugarloaf, theme, &well, ry, bh, name);
        for (i, state) in ButtonState::ALL.iter().enumerate() {
            let by = ry + (bh - size.height()) * 0.5;
            let spec = ButtonSpec::icon_only((well.cols[i], by), kind, size);
            paint_button(sugarloaf, theme, &spec, *state, label, Some(icon));
        }
        ry += bh + ROW_GAP;
    }
    y += well.rect.height;

    y - origin.1
}
