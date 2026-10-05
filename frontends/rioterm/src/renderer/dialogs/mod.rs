//! Re-skinned modals and overlays (J5 violet ink): confirmation dialogs,
//! vault unlock, palette, context menu, toasts, connection progress.
//!
//! Geometry always comes from `terminus_ui` (the same rects the pointer
//! hit-tests); this module only walks them. The shared pieces here are the
//! "kit": a rect-driven button and checkbox row painted at a given order so
//! they sit above the dialog panel (the gallery painters in `components/`
//! are pinned to the gallery's own order).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonSpec, ButtonState};
use terminus_ui::components::selection::ControlState;
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;

use crate::renderer::components::button::paint_button_on;
use crate::renderer::components::selection::paint_checkbox_on;
use crate::renderer::components::Layer;

pub mod confirm;
pub mod conflict;
pub mod context_menu;
pub mod vault;

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

/// Paint a button whose width came from `terminus_ui::confirm` (an
/// estimate, shared with hit-testing) through the shared Button painter.
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
    // Same width the layout used, so the spec rect equals the hit rect.
    let label_w = rect.width - 2.0 * size.padding_x();
    let spec = ButtonSpec::label((rect.x, rect.y), kind, size, label_w, false);
    paint_button_on(
        sugarloaf,
        theme,
        &spec,
        state,
        label,
        None,
        Layer {
            order: ORDER,
            depth,
            backdrop,
        },
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
    paint_checkbox_on(
        sugarloaf,
        theme,
        (row.x, row.y),
        label,
        checked,
        ControlState::Default,
        Layer {
            order: ORDER,
            depth,
            backdrop: theme.dialog,
        },
    );
}

/// Input box in the Input component's look (46px field: fill, border,
/// focus ring, 15px value, caret), at the dialog order.
///
/// `caret_prefix` is the text before the caret (already masked); `None`
/// hides the caret. `trailing` reserves room on the right (eye button).
#[allow(clippy::too_many_arguments)]
pub fn paint_text_field(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    value: &str,
    placeholder: bool,
    focused: bool,
    caret_prefix: Option<&str>,
    trailing: f32,
    depth: f32,
) {
    use terminus_ui::components::input as ui;
    use terminus_ui::tokens::radius;

    use crate::renderer::chrome::{paint_flat, paint_surface_stroke};
    use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};

    if focused {
        let g = ui::FOCUS_RING;
        let c = theme.accent;
        let ring = [
            c[0] * ui::FOCUS_RING_ALPHA + theme.dialog[0] * (1.0 - ui::FOCUS_RING_ALPHA),
            c[1] * ui::FOCUS_RING_ALPHA + theme.dialog[1] * (1.0 - ui::FOCUS_RING_ALPHA),
            c[2] * ui::FOCUS_RING_ALPHA + theme.dialog[2] * (1.0 - ui::FOCUS_RING_ALPHA),
            1.0,
        ];
        paint_surface_stroke(
            sugarloaf,
            &Rect::new(
                rect.x - g,
                rect.y - g,
                rect.width + 2.0 * g,
                rect.height + 2.0 * g,
            ),
            ring,
            None,
            radius::CONTROL + g,
            0.0,
            depth,
            ORDER,
            false,
        );
    }
    paint_surface_stroke(
        sugarloaf,
        rect,
        theme.field,
        Some(if focused { theme.accent } else { theme.line }),
        radius::CONTROL,
        1.0,
        depth + 0.01,
        ORDER,
        false,
    );
    let font = ui::SANS_VALUE_FONT;
    let tx = rect.x + ui::PAD_LEFT;
    let ty = rect.y + (rect.height - font * 1.25) / 2.0;
    let color = if placeholder {
        theme.text_faint
    } else {
        theme.text
    };
    draw_ui_text(sugarloaf, tx, ty, value, font, color, UiWeight::Regular);
    let _ = trailing;
    if let Some(prefix) = caret_prefix {
        let w = measure_ui_text(sugarloaf, prefix, font, UiWeight::Regular);
        let max_x = rect.right() - trailing - ui::PAD_RIGHT;
        paint_flat(
            sugarloaf,
            &Rect::new(
                (tx + w).min(max_x),
                ty,
                ui::CARET_WIDTH,
                (font * 1.25).round(),
            ),
            theme.accent,
            depth + 0.03,
            ORDER,
        );
    }
}
