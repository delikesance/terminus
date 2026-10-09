use super::color::{as_f32, as_u8};
use super::notices::{rect_union, text_blocked_by};
use super::{
    DEPTH_DIALOG, DEPTH_DIALOG_BG, DEPTH_GHOST, ORDER_CONTENT, ORDER_DIALOG, ORDER_GHOST,
};
use terminus_ui::geom::Rect;

#[test]
fn text_blocked_only_when_cover_overlaps() {
    let cover = Rect::new(100.0, 100.0, 200.0, 200.0);
    let under = Rect::new(120.0, 120.0, 40.0, 40.0);
    let aside = Rect::new(10.0, 10.0, 40.0, 40.0);
    assert!(text_blocked_by(Some(&cover), &under));
    assert!(!text_blocked_by(Some(&cover), &aside));
    assert!(!text_blocked_by(None, &under));
}

#[test]
fn rect_union_expands_to_bounds() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    let b = Rect::new(5.0, 5.0, 20.0, 20.0);
    let u = rect_union(a, b);
    assert_eq!(u.x, 0.0);
    assert_eq!(u.y, 0.0);
    assert_eq!(u.right(), 25.0);
    assert_eq!(u.bottom(), 25.0);
}

#[test]
fn colors_convert_from_bytes_without_clipping() {
    assert_eq!(as_f32([0, 0, 0, 0]), [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(as_f32([255, 255, 255, 255]), [1.0, 1.0, 1.0, 1.0]);
    let half = as_f32([128, 128, 128, 128]);
    for channel in half {
        assert!((channel - 0.502).abs() < 0.01, "{channel}");
    }
}

/// The round trip has to be exact, or a theme colour would drift every
/// time it passed through the icon path.
#[test]
fn colors_survive_the_round_trip_to_bytes() {
    for value in 0u8..=255 {
        let byte = [value, value, value, value];
        assert_eq!(as_u8(as_f32(byte)), byte, "{value} drifted");
    }
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn chrome_orders_sit_above_the_grid_and_below_the_overlays() {
    // The terminal grid paints at order 3; the command palette,
    // search and hint tooltip at 20. Chrome goes in between, and the
    // editor above them all so it is never occluded. The host-drag
    // ghost sits above dialogs so the phantom is always on top.
    assert!(ORDER_CONTENT > 3);
    assert!(ORDER_CONTENT < 20);
    assert!(ORDER_DIALOG > 20);
    assert!(ORDER_GHOST > ORDER_DIALOG);
    assert!(DEPTH_GHOST > DEPTH_DIALOG_BG);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn the_dialog_depth_is_above_the_scrim() {
    assert!(DEPTH_DIALOG_BG > DEPTH_DIALOG);
}
