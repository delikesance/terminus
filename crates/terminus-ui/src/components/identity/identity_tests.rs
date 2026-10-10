use super::*;
use crate::icons::Icon;
use crate::os_icons::{OsGlyph, SimpleBrand};
use crate::theme::ChromeTheme;
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
fn local_mark_is_dark_ink_on_the_light_theme() {
    let t = ChromeTheme::violet_paper();
    let (bg, fg) = tile_style_with(&t, TileGlyph::Local, false);
    assert_eq!(bg, t.raised);
    assert!(relative_luminance(fg) < 0.2, "{fg:?}");
    assert_eq!(
        tile_style_with(&ChromeTheme::violet_ink(), TileGlyph::Local, false).1,
        LOCAL_FG
    );
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
