use crate::geom::Rect;
use crate::theme::ChromeTheme;
use crate::tokens::{font_size, height, radius};

pub const HEIGHT: f32 = height::ROW;
pub const RADIUS: f32 = radius::CONTROL;
pub const FILL_ALPHA: f32 = 0.08;
pub const LABEL_SIZE: f32 = font_size::LABEL;

pub fn rect(x: f32, y: f32, width: f32) -> Rect {
    Rect::new(x, y, width, HEIGHT)
}

pub fn fill(theme: &ChromeTheme) -> [f32; 4] {
    let a = theme.accent;
    [a[0], a[1], a[2], FILL_ALPHA]
}
