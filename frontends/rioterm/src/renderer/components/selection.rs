//! Selection controls painter: segmented control, toggle, checkbox, choice
//! cards, plus the gallery section. Geometry comes from
//! `terminus_ui::components::selection`.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::selection::{
    choice_row, focus_ring_rects, toggle_knob_rect, toggle_rect, CheckboxLayout,
    ControlState, SegmentedLayout, SegmentedSize, CHECKBOX_BORDER, CHECKBOX_RADIUS,
    CHOICE_PAD, CHOICE_RADIUS, CHOICE_SUB_SIZE, CHOICE_TITLE_SIZE, SEGMENT_RADIUS,
    SEGMENT_TRACK_RADIUS, TOGGLE_HEIGHT,
};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::ChromeTheme;

use crate::renderer::chrome::{draw_icon, paint_surface_stroke};
use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};

const DEPTH: f32 = 0.06;
const ORDER: u8 = 7;

const fn hex(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

/// Choice-card hover border (`#463C5E`).
const CHOICE_HOVER_LINE: [f32; 4] = hex(0x46, 0x3c, 0x5e);
/// Selected choice-card background (`#251E3A`).
const CHOICE_SELECTED_BG: [f32; 4] = hex(0x25, 0x1e, 0x3a);
/// Selected choice-card subtitle (`#CFC4E6`).
const CHOICE_SELECTED_SUB: [u8; 4] = [0xcf, 0xc4, 0xe6, 255];

/// Blend `c` toward `backdrop` so a disabled control reads as 40% opaque
/// without translucent layers showing the border through the fill.
fn fade(c: [f32; 4], backdrop: [f32; 4], state: ControlState) -> [f32; 4] {
    let a = state.opacity();
    if a >= 1.0 {
        return c;
    }
    [
        backdrop[0] + (c[0] - backdrop[0]) * a,
        backdrop[1] + (c[1] - backdrop[1]) * a,
        backdrop[2] + (c[2] - backdrop[2]) * a,
        1.0,
    ]
}

fn fade_text(c: [u8; 4], state: ControlState) -> [u8; 4] {
    [c[0], c[1], c[2], (c[3] as f32 * state.opacity()).round() as u8]
}

fn fill(sugarloaf: &mut Sugarloaf, r: &Rect, color: [f32; 4], radius: f32, d: f32) {
    sugarloaf.rounded_rect(None, r.x, r.y, r.width, r.height, color, DEPTH + d, radius, ORDER);
}

/// Focus ring (2px canvas gap + 2px accent) around `r` with corner `radius`.
fn paint_ring(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme, r: &Rect, radius: f32) {
    let (inner, outer) = focus_ring_rects(r);
    fill(sugarloaf, &outer, theme.accent, radius + 4.0, 0.0);
    fill(sugarloaf, &inner, theme.canvas, radius + 2.0, 0.01);
}

fn text_top(center_y: f32, size: f32) -> f32 {
    center_y - size * 0.62
}

/// Paint a segmented control; returns its layout (for hit-testing).
#[allow(clippy::too_many_arguments)]
pub fn paint_segmented(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    options: &[&str],
    selected: usize,
    size: SegmentedSize,
) -> SegmentedLayout {
    let widths: Vec<f32> = options
        .iter()
        .map(|o| measure_ui_text(sugarloaf, o, size.font_size(), UiWeight::Medium))
        .collect();
    let layout = SegmentedLayout::new(origin.0, origin.1, &widths, size);
    fill(sugarloaf, &layout.track, theme.surface, SEGMENT_TRACK_RADIUS, 0.0);
    for (i, (seg, label)) in layout.segments.iter().zip(options).enumerate() {
        let on = i == selected;
        if on {
            fill(sugarloaf, seg, theme.selected, SEGMENT_RADIUS, 0.01);
        }
        let color = if on { theme.text } else { theme.text_muted };
        draw_ui_text(
            sugarloaf,
            seg.x + size.pad_x(),
            text_top(seg.y + seg.height * 0.5, size.font_size()),
            label,
            size.font_size(),
            color,
            UiWeight::Medium,
        );
    }
    layout
}

/// Paint a toggle switch at `origin`; returns its track rect.
pub fn paint_toggle(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    on: bool,
    state: ControlState,
) -> Rect {
    let track = toggle_rect(origin.0, origin.1);
    let knob = toggle_knob_rect(&track, on);
    if state == ControlState::Focus {
        paint_ring(sugarloaf, theme, &track, TOGGLE_HEIGHT * 0.5);
    }
    let (bg, kc) = if on {
        (theme.accent, theme.on_accent)
    } else {
        (theme.line, rgba_u8(theme.text_muted))
    };
    fill(sugarloaf, &track, fade(bg, theme.canvas, state), TOGGLE_HEIGHT * 0.5, 0.02);
    fill(sugarloaf, &knob, fade(kc, theme.canvas, state), knob.width * 0.5, 0.03);
    track
}

fn rgba_u8(c: [u8; 4]) -> [f32; 4] {
    c.map(|v| v as f32 / 255.0)
}

/// Paint a checkbox with its label; returns the layout (for hit-testing).
pub fn paint_checkbox(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    label: &str,
    checked: bool,
    state: ControlState,
) -> CheckboxLayout {
    let label_w = measure_ui_text(sugarloaf, label, 14.0, UiWeight::Regular);
    let l = CheckboxLayout::new(origin.0, origin.1, label_w);
    let b = l.box_rect;
    if state == ControlState::Focus {
        paint_ring(sugarloaf, theme, &b, CHECKBOX_RADIUS);
    }
    let (bg, border) = if checked {
        (theme.accent, theme.accent)
    } else if state == ControlState::Focus {
        (theme.field, theme.accent)
    } else {
        (theme.field, theme.line)
    };
    paint_surface_stroke(
        sugarloaf,
        &b,
        fade(bg, theme.canvas, state),
        Some(fade(border, theme.canvas, state)),
        CHECKBOX_RADIUS,
        CHECKBOX_BORDER,
        DEPTH + 0.02,
        ORDER,
        false,
    );
    if checked {
        let m = l.mark_rect();
        draw_icon(
            sugarloaf,
            Icon::Check,
            IconPlacement::new(m.x, m.y, m.width),
            fade(theme.on_accent, theme.canvas, state),
            sugarloaf.scale_factor(),
        );
    }
    draw_ui_text(
        sugarloaf,
        l.label_x,
        text_top(b.y + b.height * 0.5, 14.0),
        label,
        14.0,
        fade_text(theme.text, state),
        UiWeight::Regular,
    );
    l
}

/// Paint one choice card into `rect`.
pub fn paint_choice(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    title: &str,
    sub: &str,
    state: ControlState,
) {
    let selected = state == ControlState::Selected;
    let border = if selected {
        theme.accent
    } else if state == ControlState::Hover {
        CHOICE_HOVER_LINE
    } else {
        theme.line
    };
    let bg = if selected {
        CHOICE_SELECTED_BG
    } else if state == ControlState::Hover {
        theme.surface
    } else {
        theme.canvas
    };
    paint_surface_stroke(
        sugarloaf,
        rect,
        fade(bg, theme.canvas, state),
        Some(fade(border, theme.canvas, state)),
        CHOICE_RADIUS,
        1.0,
        DEPTH + 0.02,
        ORDER,
        false,
    );
    draw_ui_text(
        sugarloaf,
        rect.x + CHOICE_PAD,
        rect.y + CHOICE_PAD,
        title,
        CHOICE_TITLE_SIZE,
        fade_text(theme.text, state),
        UiWeight::Medium,
    );
    let sub_color = if selected { CHOICE_SELECTED_SUB } else { theme.text_muted };
    draw_ui_text(
        sugarloaf,
        rect.x + CHOICE_PAD,
        rect.y + CHOICE_PAD + CHOICE_TITLE_SIZE * 1.45 + 4.0,
        sub,
        CHOICE_SUB_SIZE,
        fade_text(sub_color, state),
        UiWeight::Regular,
    );
}

// ---------- gallery ----------

const PANEL_PAD: f32 = 28.0;
const LABEL_COL: f32 = 170.0;
const COL_GAP: f32 = 24.0;
const ROW_GAP: f32 = 20.0;

/// Section card: title + note on a canvas panel. Returns the y of the body.
fn begin_section(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    panel: &Rect,
    title: &str,
    note: &str,
) -> f32 {
    fill(sugarloaf, panel, theme.canvas, 16.0, -0.02);
    let x = panel.x + PANEL_PAD;
    let y = panel.y + PANEL_PAD;
    draw_ui_text(sugarloaf, x, y, title, 18.0, theme.text, UiWeight::SemiBold);
    draw_ui_text(sugarloaf, x, y + 28.0, note, 13.0, theme.text_muted, UiWeight::Regular);
    y + 28.0 + 19.0 + ROW_GAP
}

/// Column header captions; returns the next y.
fn headers(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    x: f32,
    y: f32,
    col_w: f32,
    cols: &[&str],
) -> f32 {
    for (i, c) in cols.iter().enumerate() {
        let cx = x + LABEL_COL + COL_GAP + i as f32 * (col_w + COL_GAP);
        draw_ui_text(sugarloaf, cx, y, c, 12.0, theme.text_muted, UiWeight::Medium);
    }
    y + 12.0 * 1.4 + ROW_GAP
}

fn row_label(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme, x: f32, cy: f32, s: &str) {
    draw_ui_text(
        sugarloaf,
        x,
        text_top(cy, 13.0),
        s,
        13.0,
        theme.text,
        UiWeight::Medium,
    );
}

/// Paint the `selection` gallery at `origin` within `width`; returns the height used.
pub fn paint_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
) -> f32 {
    let head = super::paint_section_title(sugarloaf, theme, origin, "Selection");
    let mut y = origin.1 + head + 12.0;
    let inner = (width - 2.0 * PANEL_PAD).max(0.0);
    let x = origin.0 + PANEL_PAD;

    // ---- Segmented control ----
    {
        let cols = ["First selected", "Middle selected", "Last selected"];
        let col_w = (inner - LABEL_COL - cols.len() as f32 * COL_GAP) / cols.len() as f32;
        let top = y;
        let panel_h = PANEL_PAD * 2.0 + 28.0 + 19.0 + ROW_GAP + 12.0 * 1.4 + ROW_GAP
            + 40.0 + ROW_GAP + 40.0 + ROW_GAP + 34.0;
        let panel = Rect::new(origin.0, top, width, panel_h);
        let mut by = begin_section(
            sugarloaf,
            theme,
            &panel,
            "Segmented control",
            "For 2\u{2013}4 mutually exclusive choices that switch something immediately.",
        );
        by = headers(sugarloaf, theme, x, by, col_w, &cols);
        let rows: [(&str, Vec<(Vec<&str>, usize)>, SegmentedSize); 3] = [
            (
                "Three options",
                (0..3).map(|i| (vec!["Local", "Remote", "Dynamic"], i)).collect(),
                SegmentedSize::Medium,
            ),
            (
                "Two options",
                vec![
                    (vec!["SQLite", "PostgreSQL"], 0),
                    (vec!["SQLite", "PostgreSQL"], 1),
                    (vec!["Dark", "Light"], 1),
                ],
                SegmentedSize::Medium,
            ),
            (
                "Small",
                vec![
                    (vec!["Snippets", "History"], 0),
                    (vec!["Block", "Underline", "Beam"], 1),
                    (vec!["Grid", "List"], 1),
                ],
                SegmentedSize::Small,
            ),
        ];
        for (label, cells, size) in rows {
            let h = size.segment_height() + 6.0;
            row_label(sugarloaf, theme, x, by + h * 0.5, label);
            for (i, (opts, sel)) in cells.iter().enumerate() {
                let cx = x + LABEL_COL + COL_GAP + i as f32 * (col_w + COL_GAP);
                paint_segmented(sugarloaf, theme, (cx, by), opts, *sel, size);
            }
            by += h + ROW_GAP;
        }
        y = top + panel_h + 20.0;
    }

    // ---- Toggle ----
    {
        let cols = ["On", "Off", "Focus", "Disabled on", "Disabled off"];
        let col_w = (inner - LABEL_COL - cols.len() as f32 * COL_GAP) / cols.len() as f32;
        let panel_h = PANEL_PAD * 2.0 + 28.0 + 19.0 + ROW_GAP + 12.0 * 1.4 + ROW_GAP
            + TOGGLE_HEIGHT;
        let panel = Rect::new(origin.0, y, width, panel_h);
        let mut by = begin_section(
            sugarloaf,
            theme,
            &panel,
            "Toggle",
            "For a setting that takes effect at once (Check for updates, Install automatically).",
        );
        by = headers(sugarloaf, theme, x, by, col_w, &cols);
        row_label(sugarloaf, theme, x, by + TOGGLE_HEIGHT * 0.5, "Switch");
        let cells = [
            (true, ControlState::Default),
            (false, ControlState::Default),
            (true, ControlState::Focus),
            (true, ControlState::Disabled),
            (false, ControlState::Disabled),
        ];
        for (i, (on, st)) in cells.into_iter().enumerate() {
            let cx = x + LABEL_COL + COL_GAP + i as f32 * (col_w + COL_GAP);
            paint_toggle(sugarloaf, theme, (cx, by), on, st);
        }
        y += panel_h + 20.0;
    }

    // ---- Checkbox ----
    {
        let cols = ["Checked", "Unchecked", "Focus", "Disabled"];
        let col_w = (inner - LABEL_COL - cols.len() as f32 * COL_GAP) / cols.len() as f32;
        let panel_h = PANEL_PAD * 2.0 + 28.0 + 19.0 + ROW_GAP + 12.0 * 1.4 + ROW_GAP + 20.0;
        let panel = Rect::new(origin.0, y, width, panel_h);
        let mut by = begin_section(
            sugarloaf,
            theme,
            &panel,
            "Checkbox",
            "For a choice confirmed by a button (Remember on this computer, Do this for every conflict).",
        );
        by = headers(sugarloaf, theme, x, by, col_w, &cols);
        row_label(sugarloaf, theme, x, by + 10.0, "With label");
        let cells = [
            (true, ControlState::Default),
            (false, ControlState::Default),
            (true, ControlState::Focus),
            (false, ControlState::Disabled),
        ];
        for (i, (on, st)) in cells.into_iter().enumerate() {
            let cx = x + LABEL_COL + COL_GAP + i as f32 * (col_w + COL_GAP);
            paint_checkbox(sugarloaf, theme, (cx, by), "Remember on this computer", on, st);
        }
        y += panel_h + 20.0;
    }

    // ---- Choice cards ----
    {
        let cols = ["Selected", "Hover", "Default", "Disabled"];
        let col_w = (inner - LABEL_COL - cols.len() as f32 * COL_GAP) / cols.len() as f32;
        let panel_h = PANEL_PAD * 2.0 + 28.0 + 19.0 + ROW_GAP + 12.0 * 1.4 + ROW_GAP
            + terminus_ui::components::selection::CHOICE_HEIGHT;
        let panel = Rect::new(origin.0, y, width, panel_h);
        let mut by = begin_section(
            sugarloaf,
            theme,
            &panel,
            "Choice cards",
            "Used when each option needs a one-line explanation.",
        );
        by = headers(sugarloaf, theme, x, by, col_w, &cols);
        row_label(
            sugarloaf,
            theme,
            x,
            by + terminus_ui::components::selection::CHOICE_HEIGHT * 0.5,
            "Auth method",
        );
        let rects = choice_row(
            x + LABEL_COL + COL_GAP,
            by,
            inner - LABEL_COL - COL_GAP,
            COL_GAP,
            4,
        );
        let cards = [
            ("SSH key", "Recommended", ControlState::Selected),
            ("Password", "Kept in your vault", ControlState::Hover),
            ("Kerberos", "GSSAPI", ControlState::Default),
            ("Kerberos", "Not available here", ControlState::Disabled),
        ];
        for (r, (t, s, st)) in rects.iter().zip(cards) {
            paint_choice(sugarloaf, theme, r, t, s, st);
        }
        y += panel_h;
    }

    y - origin.1
}
