use crate::geom::Rect;

pub const HEIGHT: f32 = 20.0;
pub const SIZE: f32 = 12.0;

/// Hit rect of the right-aligned text action of width `text_w`.
pub fn action_rect(header: &Rect, text_w: f32) -> Rect {
    Rect::new(header.right() - text_w, header.y, text_w, header.height)
}
