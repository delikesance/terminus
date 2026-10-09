//! Scrollbar painter: fills the thumb rect computed by
//! `terminus_ui::components::scroll`.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;

/// Fill `thumb`; a zero `radius` paints a square-cornered thumb.
pub fn paint_thumb(
    sugarloaf: &mut Sugarloaf,
    thumb: &Rect,
    color: [f32; 4],
    radius: f32,
    depth: f32,
    order: u8,
) {
    if radius > 0.0 {
        sugarloaf.rounded_rect(
            None,
            thumb.x,
            thumb.y,
            thumb.width,
            thumb.height,
            color,
            depth,
            radius,
            order,
        );
    } else {
        sugarloaf.rect(
            None,
            thumb.x,
            thumb.y,
            thumb.width,
            thumb.height,
            color,
            depth,
            order,
        );
    }
}
