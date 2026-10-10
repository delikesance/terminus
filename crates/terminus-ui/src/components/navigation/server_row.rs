use super::*;
use crate::geom::Rect;
use crate::theme::{text_color, ChromeTheme};
use crate::tokens::{height, radius, tile};

pub const HEIGHT: f32 = height::ROW;
pub const RADIUS: f32 = radius::CONTROL;
pub const PAD_X: f32 = 8.0;
pub const TILE: f32 = tile::SM;
pub const GAP: f32 = 12.0;
pub const NAME_SIZE: f32 = 14.0;
pub const META_SIZE: f32 = 12.0;
/// Focus ring: canvas-coloured gap then accent band, 2px each.
pub const RING_GAP: f32 = 2.0;
pub const RING_WIDTH: f32 = 2.0;

pub fn tile_rect(row: &Rect) -> Rect {
    Rect::new(row.x + PAD_X, row.y + (row.height - TILE) * 0.5, TILE, TILE)
}

pub fn name_x(row: &Rect) -> f32 {
    row.x + PAD_X + TILE + GAP
}

/// Right edge (text is right-aligned to it) of the meta slot.
pub fn meta_right(row: &Rect) -> f32 {
    row.right() - PAD_X
}

/// `(outer, inner)` rects of the focus ring: fill `outer` with the
/// accent, then `inner` with the canvas, then draw the row on top.
pub fn focus_ring(row: &Rect) -> (Rect, Rect) {
    let grow = |by: f32| {
        Rect::new(
            row.x - by,
            row.y - by,
            row.width + 2.0 * by,
            row.height + 2.0 * by,
        )
    };
    (grow(RING_GAP + RING_WIDTH), grow(RING_GAP))
}

pub fn style(theme: &ChromeTheme, state: RowState) -> RowStyle {
    RowStyle {
        bg: match state {
            RowState::Default | RowState::Focus => None,
            RowState::Hover => Some(theme.surface),
            RowState::Selected => Some(theme.selected),
            RowState::Dragging => Some(theme.raised),
        },
        medium: state == RowState::Selected,
        tile_active: state == RowState::Selected,
        shadow: state == RowState::Dragging,
        ring: state == RowState::Focus,
    }
}

pub fn meta_color(theme: &ChromeTheme, tone: MetaTone, state: RowState) -> [u8; 4] {
    match tone {
        MetaTone::Success => text_color(theme.success),
        MetaTone::Muted if state == RowState::Selected => theme.selected_subtle_text,
        MetaTone::Muted => theme.text_muted,
    }
}
