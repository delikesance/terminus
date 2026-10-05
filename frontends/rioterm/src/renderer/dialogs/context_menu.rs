//! Right-click context menu in the Overlay menu look.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::overlay as ov;
use terminus_ui::context_menu::{ContextMenu, MENU_RADIUS};
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;

use super::{paint_shadow, text_y, DEPTH, ORDER};
use crate::renderer::chrome::{paint_flat, paint_surface_stroke};
use crate::renderer::ui_text::{draw_ui_text, UiWeight};

/// Paint `menu` above everything (overlay layer: quads, then text).
pub fn paint_context_menu(
    sugarloaf: &mut Sugarloaf,
    menu: &ContextMenu,
    theme: &ChromeTheme,
) {
    sugarloaf.begin_overlay();
    let r = menu.rect();
    paint_shadow(sugarloaf, &r, MENU_RADIUS, DEPTH);
    paint_surface_stroke(
        sugarloaf,
        &r,
        theme.surface,
        Some(theme.line),
        MENU_RADIUS,
        1.0,
        DEPTH + 0.02,
        ORDER,
        false,
    );
    for (i, item) in menu.items.iter().enumerate() {
        if let Some(sep) = menu.separator_rect(i) {
            paint_flat(
                sugarloaf,
                &Rect::new(sep.x + 2.0, sep.y + 5.0, sep.width - 4.0, 1.0),
                theme.line,
                DEPTH + 0.05,
                ORDER,
            );
        }
        let Some(row) = menu.item_rect(i) else {
            continue;
        };
        if menu.hover == Some(i) {
            sugarloaf.rounded_rect(
                None,
                row.x,
                row.y,
                row.width,
                row.height,
                theme.selected,
                DEPTH + 0.05,
                ov::MENU_ITEM_RADIUS,
                ORDER,
            );
        }
        let color = if item.danger {
            theme.danger_text
        } else {
            theme.text
        };
        draw_ui_text(
            sugarloaf,
            row.x + ov::MENU_ITEM_PAD_X,
            text_y(&row, 14.0),
            &item.label,
            14.0,
            color,
            UiWeight::Regular,
        );
    }
    sugarloaf.end_overlay();
}
