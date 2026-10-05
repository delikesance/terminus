//! Identity: OS tiles and the machine / workspace header.
//!
//! Pure geometry and colour rules (no sugarloaf): the painter in
//! `rioterm::renderer::components::identity` walks the rects below.

use crate::geom::Rect;
use crate::icons::Icon;
use crate::os_icons::{OsGlyph, SimpleBrand};
use crate::theme::ChromeTheme;

// ---------------------------------------------------------------- tile ----

/// Smallest tile corner radius.
pub const TILE_MIN_RADIUS: f32 = 6.0;
/// Mark side as a fraction of the tile side.
pub const MARK_RATIO: f32 = 0.57;
/// Minimum relative luminance (WCAG) of a mark on the dark chrome; darker
/// brand colours are lifted towards white until they reach it.
pub const MIN_MARK_LUMINANCE: f32 = 0.19;
/// Neutral icon colour on a Local tile (`#C3BAD6`).
pub const LOCAL_FG: [f32; 4] = [195.0 / 255.0, 186.0 / 255.0, 214.0 / 255.0, 1.0];

/// What a tile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileGlyph {
    /// This computer (Lucide monitor on a raised background).
    Local,
    /// A real brand mark.
    Os(SimpleBrand),
    /// Anything unrecognised, Windows included: neutral server icon.
    Unknown,
}

impl TileGlyph {
    pub fn from_os_glyph(glyph: OsGlyph) -> Self {
        match glyph {
            OsGlyph::Brand(b) => Self::Os(b),
            OsGlyph::Unknown => Self::Unknown,
        }
    }

    /// The vendored glyph for the brand mark, if any.
    pub fn os_glyph(self) -> Option<OsGlyph> {
        match self {
            Self::Os(b) => Some(OsGlyph::Brand(b)),
            _ => None,
        }
    }

    /// Lucide icon for the mark-less variants.
    pub fn icon(self) -> Option<Icon> {
        match self {
            Self::Local => Some(Icon::Monitor),
            Self::Unknown => Some(Icon::Server),
            Self::Os(_) => None,
        }
    }
}

fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

/// WCAG relative luminance of an sRGB colour (alpha ignored).
pub fn relative_luminance(c: [f32; 4]) -> f32 {
    let lin = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// Mix `c` towards white in small steps until its relative luminance is at
/// least `min`. Colours already bright enough come back unchanged.
pub fn lift_to_min_luminance(c: [f32; 4], min: f32) -> [f32; 4] {
    let mut out = [c[0], c[1], c[2], 1.0];
    for step in 0..=100 {
        let t = step as f32 / 100.0;
        out = [
            c[0] + (1.0 - c[0]) * t,
            c[1] + (1.0 - c[1]) * t,
            c[2] + (1.0 - c[2]) * t,
            1.0,
        ];
        if relative_luminance(out) >= min {
            break;
        }
    }
    out
}

/// The design board's fill for each brand (already lifted where needed).
/// `None` for brands not on the board; those use the lifting rule.
fn board_fill(brand: SimpleBrand) -> Option<u32> {
    use SimpleBrand::*;
    Some(match brand {
        NixOs => 0x5B7EC6,
        Ubuntu => 0xE95420,
        Debian => 0xC26170,
        Fedora => 0x51A2DA,
        ArchLinux => 0x1793D1,
        AlpineLinux => 0x568BA5,
        OpenSuse => 0x73BA25,
        RedHat => 0xF12626,
        CentOs => 0xC2C1F2,
        RockyLinux => 0x10B981,
        LinuxMint => 0x86BE43,
        KaliLinux => 0x9BCBE8,
        Gentoo => 0x877FA2,
        VoidLinux => 0x598D71,
        Apple => 0xE4DEEF,
        Linux => 0xFCC624,
        Postgresql => return None,
    })
}

/// Mark colour of a brand on the dark chrome.
pub fn brand_fill(brand: SimpleBrand) -> [f32; 4] {
    match board_fill(brand) {
        Some(hex) => {
            let c = rgb(hex);
            [c[0], c[1], c[2], 1.0]
        }
        None => {
            let c = rgb(u32::from_str_radix(brand.hex(), 16).unwrap_or(0x808080));
            lift_to_min_luminance([c[0], c[1], c[2], 1.0], MIN_MARK_LUMINANCE)
        }
    }
}

/// Tint alpha of a brand background (macOS, being near-white, is subtler).
pub fn brand_tint_alpha(brand: SimpleBrand) -> f32 {
    if brand == SimpleBrand::Apple {
        0.12
    } else {
        0.16
    }
}

/// `(background, mark/icon colour)` of a tile for an explicit theme.
pub fn tile_style_with(
    theme: &ChromeTheme,
    glyph: TileGlyph,
    selected: bool,
) -> ([f32; 4], [f32; 4]) {
    if selected {
        return (theme.accent, theme.on_accent);
    }
    match glyph {
        TileGlyph::Os(brand) => {
            let fill = brand_fill(brand);
            ([fill[0], fill[1], fill[2], brand_tint_alpha(brand)], fill)
        }
        TileGlyph::Local => (theme.raised, LOCAL_FG),
        TileGlyph::Unknown => {
            let f = theme.text_faint;
            (
                theme.raised,
                [
                    f[0] as f32 / 255.0,
                    f[1] as f32 / 255.0,
                    f[2] as f32 / 255.0,
                    1.0,
                ],
            )
        }
    }
}

/// [`tile_style_with`] for the default (violet ink) theme.
pub fn tile_style(glyph: TileGlyph, selected: bool) -> ([f32; 4], [f32; 4]) {
    tile_style_with(&ChromeTheme::default(), glyph, selected)
}

/// Corner radius of a tile of side `size`: `round(size * 0.29)`, min 6.
pub fn tile_radius(size: f32) -> f32 {
    (size * 0.29).round().max(TILE_MIN_RADIUS)
}

/// Side of the mark inside a tile of side `size` (about 57 %).
pub fn mark_size(size: f32) -> f32 {
    (size * MARK_RATIO).round()
}

/// The tile square at `(x, y)`.
pub fn tile_rect(x: f32, y: f32, size: f32) -> Rect {
    Rect::new(x, y, size, size)
}

/// The mark box, centred in `tile`.
pub fn mark_rect(tile: &Rect) -> Rect {
    let m = mark_size(tile.width);
    Rect::new(
        tile.x + ((tile.width - m) / 2.0).floor(),
        tile.y + ((tile.height - m) / 2.0).floor(),
        m,
        m,
    )
}

// ------------------------------------------------------------- gallery ----

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

// -------------------------------------------------------------- header ----

pub const HEADER_PAD_LEFT: f32 = 4.0;
pub const HEADER_GAP: f32 = 28.0;
pub const HEADER_TITLE_SIZE: f32 = 28.0;
pub const HEADER_ADDRESS_SIZE: f32 = 11.0;
/// Line box of the address row.
pub const HEADER_ADDRESS_H: f32 = 14.0;
pub const HEADER_TITLE_ADDRESS_GAP: f32 = 4.0;
pub const HEADER_TITLE_PAD_BOTTOM: f32 = 14.0;
/// Height of a view tab (text, 14 gap, 2 underline).
pub const HEADER_TAB_H: f32 = 34.0;
/// Gap between view tabs.
pub const HEADER_TAB_GAP: f32 = 22.0;
pub const HEADER_BORDER: f32 = 1.0;
/// Text line box is 1.25 x font size; the mock's name uses `line-height: 1`,
/// so the title is drawn this much higher than its rect top.
pub const HEADER_TITLE_INK_DY: f32 = -HEADER_TITLE_SIZE * 0.25 / 2.0;

/// Header rects. The tabs row sits on the bottom border.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeaderRects {
    pub bounds: Rect,
    pub title: Rect,
    /// `None` for the Settings variant (title only).
    pub address: Option<Rect>,
    pub tabs: Rect,
    pub border: Rect,
}

/// Total header height (including the 1px border).
pub fn header_height(has_address: bool) -> f32 {
    let text = HEADER_TITLE_SIZE
        + if has_address {
            HEADER_TITLE_ADDRESS_GAP + HEADER_ADDRESS_H
        } else {
            0.0
        };
    text + HEADER_TITLE_PAD_BOTTOM + HEADER_BORDER
}

/// Lay the header out at `(x, y)` over `width`. `block_w` is the measured
/// width of the widest of title and address, `tabs_w` the tabs row width.
pub fn header_rects(
    x: f32,
    y: f32,
    width: f32,
    has_address: bool,
    block_w: f32,
    tabs_w: f32,
) -> HeaderRects {
    let h = header_height(has_address);
    let bounds = Rect::new(x, y, width, h);
    let tx = x + HEADER_PAD_LEFT;
    let title = Rect::new(tx, y, block_w, HEADER_TITLE_SIZE);
    let address = has_address.then(|| {
        Rect::new(
            tx,
            y + HEADER_TITLE_SIZE + HEADER_TITLE_ADDRESS_GAP,
            block_w,
            HEADER_ADDRESS_H,
        )
    });
    let border = Rect::new(x, y + h - HEADER_BORDER, width, HEADER_BORDER);
    let tabs = Rect::new(
        tx + block_w + HEADER_GAP,
        border.y - HEADER_TAB_H,
        tabs_w,
        HEADER_TAB_H,
    );
    HeaderRects {
        bounds,
        title,
        address,
        tabs,
        border,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SimpleBrand::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn centos_and_kali_marks_read_on_their_tint() {
        assert!(relative_luminance(brand_fill(CentOs)) >= 0.5);
        assert!(relative_luminance(brand_fill(KaliLinux)) >= 0.5);
    }

    #[test]
    fn header_title_is_lifted_to_a_line_height_one_box() {
        assert_eq!(HEADER_TITLE_INK_DY, -3.5);
        // address sits 4px under the 28px title box
        let h = header_rects(0.0, 0.0, 800.0, true, 100.0, 200.0);
        assert_eq!(h.address.unwrap().y - h.title.bottom(), 4.0);
    }

    #[test]
    fn radius_is_29_percent_min_6() {
        assert_eq!(tile_radius(28.0), 8.0);
        assert_eq!(tile_radius(36.0), 10.0);
        assert_eq!(tile_radius(10.0), 6.0);
    }

    #[test]
    fn mark_is_57_percent_and_centred() {
        assert_eq!(mark_size(28.0), 16.0);
        assert_eq!(mark_size(36.0), 21.0);
        let t = tile_rect(10.0, 20.0, 28.0);
        let m = mark_rect(&t);
        assert_eq!((m.x, m.y, m.width), (16.0, 26.0, 16.0));
        let t = tile_rect(0.0, 0.0, 36.0);
        let m = mark_rect(&t);
        assert!(m.x >= 7.0 && m.right() <= 36.0 - 7.0);
    }

    #[test]
    fn brand_tile_is_16_percent_tint_of_its_fill() {
        let (bg, fill) = tile_style(TileGlyph::Os(Ubuntu), false);
        assert!(close(fill[0], 233.0 / 255.0) && close(fill[2], 32.0 / 255.0));
        assert_eq!(&bg[..3], &fill[..3]);
        assert!(close(bg[3], 0.16));
    }

    #[test]
    fn dark_brands_use_board_lifted_fills() {
        let hex = |c: [f32; 4]| {
            format!(
                "{:02X}{:02X}{:02X}",
                (c[0] * 255.0).round() as u8,
                (c[1] * 255.0).round() as u8,
                (c[2] * 255.0).round() as u8
            )
        };
        assert_eq!(hex(brand_fill(Debian)), "C26170");
        assert_eq!(hex(brand_fill(AlpineLinux)), "568BA5");
        assert_eq!(hex(brand_fill(CentOs)), "C2C1F2");
        assert_eq!(hex(brand_fill(Gentoo)), "877FA2");
        assert_eq!(hex(brand_fill(Apple)), "E4DEEF");
        let (bg, _) = tile_style(TileGlyph::Os(Apple), false);
        assert!(close(bg[3], 0.12));
    }

    #[test]
    fn every_mark_meets_minimum_luminance() {
        for b in SimpleBrand::ALL {
            let l = relative_luminance(brand_fill(b));
            assert!(l >= MIN_MARK_LUMINANCE - 1e-4, "{b:?} luminance {l}");
        }
    }

    #[test]
    fn lifting_leaves_bright_alone_and_raises_dark() {
        let bright = [0.9, 0.8, 0.2, 1.0];
        assert_eq!(lift_to_min_luminance(bright, 0.19), bright);
        let dark = [0.0, 0.0, 0.0, 1.0];
        let lifted = lift_to_min_luminance(dark, 0.19);
        assert!(relative_luminance(lifted) >= 0.19);
        assert!(relative_luminance(lifted) < 0.25, "lifted minimally");
        // Postgres has no board entry: raw brand colour is dark enough to need no lift or is lifted.
        assert!(relative_luminance(brand_fill(Postgresql)) >= 0.19 - 1e-4);
    }

    #[test]
    fn selected_is_accent_with_on_accent_mark() {
        let t = ChromeTheme::default();
        for g in [TileGlyph::Local, TileGlyph::Unknown, TileGlyph::Os(Debian)] {
            assert_eq!(tile_style(g, true), (t.accent, t.on_accent));
        }
    }

    #[test]
    fn local_and_unknown_are_neutral_on_raised() {
        let t = ChromeTheme::default();
        assert_eq!(tile_style(TileGlyph::Local, false).0, t.raised);
        assert_eq!(tile_style(TileGlyph::Unknown, false).0, t.raised);
        assert_eq!(TileGlyph::Local.icon(), Some(Icon::Monitor));
        assert_eq!(TileGlyph::Unknown.icon(), Some(Icon::Server));
        assert!(TileGlyph::Os(Linux).icon().is_none());
        assert!(TileGlyph::from_os_glyph(OsGlyph::Unknown) == TileGlyph::Unknown);
    }

    #[test]
    fn board_covers_every_brand_once_plus_local_unknown() {
        assert_eq!(BOARD.len(), 18);
        for b in SimpleBrand::ALL {
            if b == Postgresql {
                continue;
            }
            assert_eq!(
                BOARD.iter().filter(|(g, _)| *g == TileGlyph::Os(b)).count(),
                1,
                "{b:?}"
            );
        }
        assert_eq!(BOARD[0].0, TileGlyph::Local);
        assert_eq!(BOARD[17].0, TileGlyph::Unknown);
    }

    #[test]
    fn grid_has_six_columns_and_three_tiles_per_cell() {
        let w = 1200.0;
        let a = cell_rects(0, 0.0, 0.0, w);
        let b = cell_rects(1, 0.0, 0.0, w);
        let g = cell_rects(6, 0.0, 0.0, w);
        assert!(close(b.cell.x - a.cell.x, column_width(w) + GRID_COL_GAP));
        assert_eq!(g.cell.x, a.cell.x);
        assert!(close(g.cell.y - a.cell.y, CELL_H + GRID_ROW_GAP));
        assert_eq!(
            (a.small.width, a.card.width, a.selected.width),
            (28.0, 36.0, 28.0)
        );
        assert!(a.small.right() + CELL_TILE_GAP == a.card.x);
        // Tiles are vertically centred on the 36 row.
        assert!(close(a.small.y + 14.0, a.card.y + 18.0));
        assert!(close(grid_height(18), 3.0 * CELL_H + 2.0 * GRID_ROW_GAP));
        assert_eq!(grid_height(0), 0.0);
        assert!(a.selected.right() - a.cell.x <= column_width(w) + 1e-3 || w < 700.0);
    }

    #[test]
    fn header_tabs_sit_on_the_bottom_border() {
        let h = header_rects(10.0, 50.0, 900.0, true, 120.0, 400.0);
        assert_eq!(h.bounds.height, header_height(true));
        assert_eq!(h.bounds.height, 28.0 + 4.0 + 14.0 + 14.0 + 1.0);
        assert!(close(h.tabs.bottom(), h.border.y));
        assert!(close(h.border.bottom(), h.bounds.bottom()));
        assert_eq!(h.tabs.x, 10.0 + 4.0 + 120.0 + 28.0);
        let a = h.address.unwrap();
        assert!(a.y >= h.title.bottom() && a.bottom() <= h.border.y - 14.0 + 1e-3);
        assert!(h.tabs.y >= h.bounds.y);
    }

    #[test]
    fn settings_header_is_title_only_and_shorter() {
        let s = header_rects(0.0, 0.0, 900.0, false, 90.0, 300.0);
        assert!(s.address.is_none());
        assert!(header_height(false) < header_height(true));
        assert_eq!(header_height(false), 28.0 + 14.0 + 1.0);
        assert!(close(s.tabs.bottom(), s.border.y));
    }
}
