//! Re-skinned modals and overlays (J5 violet ink): confirmation dialogs,
//! vault unlock, palette, context menu, toasts, connection progress.
//!
//! Geometry always comes from `terminus_ui` (the same rects the pointer
//! hit-tests); this module only walks them. The shared pieces here are the
//! "kit": a rect-driven button and checkbox row painted at a given order so
//! they sit above the dialog panel (the gallery painters in `components/`
//! are pinned to the gallery's own order).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{
    colors, ButtonKind, ButtonSize, ButtonState, FOCUS_GAP, FOCUS_RING,
};
use terminus_ui::components::selection::{
    CHECKBOX_BORDER, CHECKBOX_LABEL_GAP, CHECKBOX_MARK, CHECKBOX_RADIUS, CHECKBOX_SIZE,
};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::{text_color, ChromeTheme};

use crate::renderer::chrome::{draw_icon, paint_surface_stroke};
use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};

pub mod confirm;

/// Draw order of every re-skinned modal (above the chrome's own dialogs).
pub const ORDER: u8 = 30;
/// Base depth of a modal panel; children add small offsets.
pub const DEPTH: f32 = 0.1;

pub fn rgba8(c: [u8; 4]) -> [f32; 4] {
    c.map(|v| v as f32 / 255.0)
}

/// Y that vertically centres a text line of `size` in `rect`.
pub fn text_y(rect: &Rect, size: f32) -> f32 {
    rect.y + (rect.height - size * 1.25) / 2.0
}

/// Soft drop shadow under a rounded panel (layered translucent rects).
pub fn paint_shadow(sugarloaf: &mut Sugarloaf, r: &Rect, rad: f32, depth: f32) {
    for (grow, dy, a) in [(14.0, 14.0, 0.07), (8.0, 12.0, 0.10), (3.0, 10.0, 0.14)] {
        sugarloaf.rounded_rect(
            None,
            r.x - grow,
            r.y + dy - grow,
            r.width + 2.0 * grow,
            r.height + 2.0 * grow,
            [0.0, 0.0, 0.0, a],
            depth,
            rad + grow,
            ORDER,
        );
    }
}

/// Paint a button into `rect` (the layout's rect, not a measured one).
///
/// `backdrop` is the colour behind the button, used for the focus ring gap.
#[allow(clippy::too_many_arguments)]
pub fn paint_button_rect(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    kind: ButtonKind,
    size: ButtonSize,
    state: ButtonState,
    label: &str,
    backdrop: [f32; 4],
    depth: f32,
) {
    let opacity = state.opacity();
    let c = colors(theme, kind, state);
    let fade = |col: [f32; 4]| [col[0], col[1], col[2], col[3] * opacity];
    let radius = size.radius();
    if state.shows_focus_ring() {
        let g = FOCUS_GAP + FOCUS_RING;
        sugarloaf.rounded_rect(
            None,
            rect.x - g,
            rect.y - g,
            rect.width + 2.0 * g,
            rect.height + 2.0 * g,
            theme.accent,
            depth,
            radius + g,
            ORDER,
        );
        sugarloaf.rounded_rect(
            None,
            rect.x - FOCUS_GAP,
            rect.y - FOCUS_GAP,
            rect.width + 2.0 * FOCUS_GAP,
            rect.height + 2.0 * FOCUS_GAP,
            backdrop,
            depth + 0.001,
            radius + FOCUS_GAP,
            ORDER,
        );
    }
    if let Some(fill) = c.fill {
        sugarloaf.rounded_rect(
            None,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            fade(fill),
            depth + 0.002,
            radius,
            ORDER,
        );
    }
    let weight = if kind.semibold() {
        UiWeight::SemiBold
    } else {
        UiWeight::Medium
    };
    let fs = size.font_size();
    let w = measure_ui_text(sugarloaf, label, fs, weight);
    draw_ui_text(
        sugarloaf,
        rect.x + (rect.width - w) / 2.0,
        text_y(rect, fs),
        label,
        fs,
        text_color(fade(c.fg)),
        weight,
    );
}

/// Button state from the flags a dialog tracks.
pub fn button_state(focused: bool, hovered: bool) -> ButtonState {
    ButtonState::resolve(hovered, false, focused, false)
}

/// Checkbox + label at the top-left of `row`.
pub fn paint_checkbox_row(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    row: &Rect,
    label: &str,
    checked: bool,
    depth: f32,
) {
    let b = Rect::new(row.x, row.y, CHECKBOX_SIZE, CHECKBOX_SIZE);
    let (bg, border) = if checked {
        (theme.accent, theme.accent)
    } else {
        (theme.field, theme.line)
    };
    paint_surface_stroke(
        sugarloaf,
        &b,
        bg,
        Some(border),
        CHECKBOX_RADIUS,
        CHECKBOX_BORDER,
        depth,
        ORDER,
        false,
    );
    if checked {
        let o = (CHECKBOX_SIZE - CHECKBOX_MARK) * 0.5;
        let scale = sugarloaf.scale_factor();
        draw_icon(
            sugarloaf,
            Icon::Check,
            IconPlacement::new(b.x + o, b.y + o, CHECKBOX_MARK),
            theme.on_accent,
            scale,
        );
    }
    draw_ui_text(
        sugarloaf,
        b.right() + CHECKBOX_LABEL_GAP,
        text_y(&b, 14.0),
        label,
        14.0,
        theme.text,
        UiWeight::Regular,
    );
}
