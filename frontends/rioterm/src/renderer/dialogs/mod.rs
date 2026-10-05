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
