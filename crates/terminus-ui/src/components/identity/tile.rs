use crate::geom::Rect;
use crate::icons::Icon;
use crate::os_icons::{OsGlyph, SimpleBrand};
use crate::theme::ChromeTheme;

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
        // The lilac mark is too pale on paper: use the muted text ink.
        TileGlyph::Local if theme.is_light() => {
            (theme.raised, crate::theme::unit_color(theme.text_muted))
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
