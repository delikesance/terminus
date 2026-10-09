//! Re-skinned modals and overlays (J5 violet ink): confirmation dialogs,
//! vault unlock, palette, context menu, toasts, connection progress.
//!
//! Geometry always comes from `terminus_ui` (the same rects the pointer
//! hit-tests); this module only walks them. The shared pieces here are the
//! "kit": a rect-driven button and checkbox row painted at a given order so
//! they sit above the dialog panel (the gallery painters in `components/`
//! are pinned to the gallery's own order).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;

use crate::renderer::components::Layer;

pub mod confirm;
pub mod connection;
pub mod context_menu;
pub mod snippet;
pub mod vault;

/// Draw order of every re-skinned modal (above the chrome's own dialogs).
pub const ORDER: u8 = 30;
/// Base depth of a modal panel; children add small offsets.
pub const DEPTH: f32 = 0.1;

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

/// Shell of a re-skinned modal: optional scrim, shadow, bordered surface.
pub fn paint_dialog_frame(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    scrim: Option<&Rect>,
    dialog: &Rect,
) {
    use crate::renderer::chrome::{paint_flat, paint_surface_stroke};
    use terminus_ui::components::overlay::SCRIM;
    use terminus_ui::tokens::radius;

    if let Some(scrim) = scrim {
        paint_flat(sugarloaf, scrim, SCRIM, DEPTH - 0.02, ORDER);
    }
    paint_shadow(sugarloaf, dialog, radius::DIALOG, DEPTH);
    paint_surface_stroke(
        sugarloaf,
        dialog,
        theme.dialog,
        Some(theme.dialog_line),
        radius::DIALOG,
        1.0,
        DEPTH + 0.02,
        ORDER,
        false,
    );
}

/// Layer of the buttons inside a re-skinned modal.
pub fn dialog_layer(theme: &ChromeTheme) -> Layer {
    Layer {
        order: ORDER,
        depth: DEPTH + 0.05,
        backdrop: theme.dialog,
    }
}

/// Dialog input box: the Input component's text field at the dialog order.
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
    use crate::renderer::components::input::{paint_field_at, FieldContent};
    use crate::renderer::ui_text::{measure_ui_text, UiWeight};
    use terminus_ui::components::input::{
        bare_field_layout, FieldKind, FieldState, SANS_VALUE_FONT,
    };

    let layout = bare_field_layout(rect, trailing);
    let caret_prefix_width = caret_prefix.map(|prefix| {
        measure_ui_text(sugarloaf, prefix, SANS_VALUE_FONT, UiWeight::Regular)
    });
    let (value, placeholder) = if placeholder {
        ("", value)
    } else {
        (value, "")
    };
    let state = if focused {
        FieldState::Focus
    } else {
        FieldState::Default
    };
    let content = FieldContent {
        kind: FieldKind::Text,
        state,
        label: None,
        value,
        placeholder,
        helper: None,
        revealed: false,
        caret_prefix_width,
    };
    let layer = Layer {
        order: ORDER,
        depth,
        backdrop: theme.dialog,
    };
    paint_field_at(sugarloaf, theme, &layout, &content, layer, 1.0);
}
