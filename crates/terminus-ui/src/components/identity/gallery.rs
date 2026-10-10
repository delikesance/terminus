use super::*;
use crate::geom::Rect;
use crate::os_icons::SimpleBrand;

/// Every tile on the design board, in board order, with its caption.
pub const BOARD: [(TileGlyph, &str); 18] = [
    (TileGlyph::Local, "Local"),
    (TileGlyph::Os(SimpleBrand::Ubuntu), "Ubuntu"),
    (TileGlyph::Os(SimpleBrand::NixOs), "NixOS"),
    (TileGlyph::Os(SimpleBrand::Debian), "Debian"),
    (TileGlyph::Os(SimpleBrand::Fedora), "Fedora"),
    (TileGlyph::Os(SimpleBrand::ArchLinux), "Arch Linux"),
    (TileGlyph::Os(SimpleBrand::AlpineLinux), "Alpine"),
    (TileGlyph::Os(SimpleBrand::OpenSuse), "openSUSE"),
    (TileGlyph::Os(SimpleBrand::RedHat), "Red Hat"),
    (TileGlyph::Os(SimpleBrand::CentOs), "CentOS"),
    (TileGlyph::Os(SimpleBrand::RockyLinux), "Rocky Linux"),
    (TileGlyph::Os(SimpleBrand::LinuxMint), "Linux Mint"),
    (TileGlyph::Os(SimpleBrand::KaliLinux), "Kali"),
    (TileGlyph::Os(SimpleBrand::Gentoo), "Gentoo"),
    (TileGlyph::Os(SimpleBrand::VoidLinux), "Void Linux"),
    (TileGlyph::Os(SimpleBrand::Apple), "macOS"),
    (TileGlyph::Os(SimpleBrand::Linux), "Linux"),
    (TileGlyph::Unknown, "Unknown"),
];

pub const GRID_COLUMNS: usize = 6;
pub const GRID_COL_GAP: f32 = 20.0;
pub const GRID_ROW_GAP: f32 = 28.0;
/// Caption line height and its gap to the tile row.
pub const CAPTION_H: f32 = 16.0;
pub const CAPTION_GAP: f32 = 10.0;
/// Gap between the three tiles of a cell.
pub const CELL_TILE_GAP: f32 = 10.0;
pub const CELL_H: f32 = CAPTION_H + CAPTION_GAP + 36.0;

/// One board cell: caption anchor plus the 28, 36 and selected-28 tiles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellRects {
    pub cell: Rect,
    pub caption: Rect,
    pub small: Rect,
    pub card: Rect,
    pub selected: Rect,
}

/// Width of a cell's tile row.
pub fn cell_tiles_width() -> f32 {
    28.0 + CELL_TILE_GAP + 36.0 + CELL_TILE_GAP + 28.0
}

/// Column width of the 6-column grid inside `width`.
pub fn column_width(width: f32) -> f32 {
    ((width - GRID_COL_GAP * (GRID_COLUMNS as f32 - 1.0)) / GRID_COLUMNS as f32).max(0.0)
}

/// Rects of grid cell `index` (row-major) for a grid at `(x, y)`.
pub fn cell_rects(index: usize, x: f32, y: f32, width: f32) -> CellRects {
    let (col, row) = ((index % GRID_COLUMNS) as f32, (index / GRID_COLUMNS) as f32);
    let cw = column_width(width);
    let cx = x + col * (cw + GRID_COL_GAP);
    let cy = y + row * (CELL_H + GRID_ROW_GAP);
    let ty = cy + CAPTION_H + CAPTION_GAP;
    let small = tile_rect(cx, ty + 4.0, 28.0);
    let card = tile_rect(small.right() + CELL_TILE_GAP, ty, 36.0);
    let selected = tile_rect(card.right() + CELL_TILE_GAP, ty + 4.0, 28.0);
    CellRects {
        cell: Rect::new(cx, cy, cw, CELL_H),
        caption: Rect::new(cx, cy, cw, CAPTION_H),
        small,
        card,
        selected,
    }
}

/// Height of the whole grid of `count` cells.
pub fn grid_height(count: usize) -> f32 {
    let rows = count.div_ceil(GRID_COLUMNS) as f32;
    if rows == 0.0 {
        0.0
    } else {
        rows * CELL_H + (rows - 1.0) * GRID_ROW_GAP
    }
}
