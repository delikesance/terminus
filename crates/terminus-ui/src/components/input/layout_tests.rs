use super::*;
use crate::geom::Rect;
use crate::icons::Icon;
use crate::theme::ChromeTheme;

#[test]
fn bare_layout_reserves_trailing_slot_in_text_width() {
    let rect = Rect::new(10.0, 20.0, 300.0, FIELD_HEIGHT);
    let plain = bare_field_layout(&rect, 0.0);
    let reserved = bare_field_layout(&rect, 40.0);
    assert_eq!(plain.box_rect, rect);
    assert_eq!(plain.label, None);
    assert_eq!(reserved.text.width, plain.text.width - 40.0);
}

fn lay(kind: FieldKind, label: bool, helper: bool) -> FieldLayout {
    field_layout((10.0, 20.0), 300.0, kind, label, helper)
}

#[test]
fn box_is_46_tall_below_label() {
    let l = lay(FieldKind::Text, true, false);
    let label = l.label.unwrap();
    assert_eq!(label.height, LABEL_HEIGHT);
    assert_eq!(l.box_rect.y, 20.0 + LABEL_HEIGHT + LABEL_GAP);
    assert_eq!(l.box_rect.height, 46.0);
    assert_eq!(l.box_rect.width, 300.0);
    assert!(l.helper.is_none());
    assert_eq!(l.total.height, LABEL_HEIGHT + LABEL_GAP + 46.0);
}

#[test]
fn without_label_box_starts_at_origin() {
    let l = lay(FieldKind::Text, false, false);
    assert!(l.label.is_none());
    assert_eq!(l.box_rect.y, 20.0);
}

#[test]
fn helper_sits_below_box_with_gap() {
    let l = lay(FieldKind::Text, true, true);
    let h = l.helper.unwrap();
    assert_eq!(h.y, l.box_rect.bottom() + LABEL_GAP);
    assert_eq!(h.height, HELPER_HEIGHT);
    assert_eq!(l.total.bottom(), h.bottom());
}

#[test]
fn textarea_is_92_tall_and_text_starts_12_down() {
    let l = lay(FieldKind::Textarea, true, false);
    assert_eq!(l.box_rect.height, 92.0);
    assert_eq!(l.text.y, l.box_rect.y + TEXTAREA_PAD_TOP);
}

#[test]
fn single_line_text_is_vertically_centered() {
    let l = lay(FieldKind::Text, false, false);
    let mid = l.text.y + l.text.height / 2.0;
    assert!((mid - (l.box_rect.y + 23.0)).abs() < 0.01);
    assert_eq!(l.text.x, l.box_rect.x + PAD_LEFT);
}

#[test]
fn plain_text_has_no_trailing_slot_and_text_fills_box() {
    let l = lay(FieldKind::Text, false, false);
    assert!(l.trailing.is_none());
    assert_eq!(l.text.right(), l.box_rect.right() - PAD_RIGHT);
}

#[test]
fn password_and_select_have_trailing_slot_at_right_edge() {
    for kind in [FieldKind::Password, FieldKind::Select] {
        let l = lay(kind, false, false);
        let t = l.trailing.unwrap();
        assert_eq!(t.width, TRAILING_ICON);
        assert_eq!(t.right(), l.box_rect.right() - PAD_RIGHT);
        assert!((t.y + t.height / 2.0 - (l.box_rect.y + 23.0)).abs() < 0.01);
        assert_eq!(l.text.right(), t.x - INNER_GAP);
    }
}

#[test]
fn trailing_icons() {
    assert_eq!(FieldKind::Password.trailing_icon(false), Some(Icon::Eye));
    assert_eq!(FieldKind::Password.trailing_icon(true), Some(Icon::EyeOff));
    assert_eq!(
        FieldKind::Select.trailing_icon(false),
        Some(Icon::ChevronDown)
    );
    assert_eq!(FieldKind::Text.trailing_icon(false), None);
    assert_eq!(FieldKind::Mono.trailing_icon(false), None);
}

#[test]
fn value_fonts() {
    assert_eq!(FieldKind::Text.value_font(), 15.0);
    assert_eq!(FieldKind::Mono.value_font(), 14.0);
    assert_eq!(FieldKind::Textarea.value_font(), 14.0);
}

#[test]
fn ring_expands_box_by_three() {
    let l = lay(FieldKind::Text, true, false);
    assert_eq!(l.ring.x, l.box_rect.x - 3.0);
    assert_eq!(l.ring.width, l.box_rect.width + 6.0);
    assert_eq!(l.ring.height, l.box_rect.height + 6.0);
}

#[test]
fn caret_follows_measured_prefix_and_clamps() {
    let l = lay(FieldKind::Text, false, false);
    let c = caret_rect(&l, FieldKind::Text, 40.0);
    assert_eq!(c.x, l.text.x + 40.0);
    assert_eq!(c.width, CARET_WIDTH);
    assert_eq!(c.y, l.text.y);
    let far = caret_rect(&l, FieldKind::Text, 9999.0);
    assert!(far.right() <= l.text.right() + 0.01);
}

#[test]
fn hit_test_box_and_trailing() {
    let l = lay(FieldKind::Password, true, false);
    let t = l.trailing.unwrap();
    assert_eq!(
        hit_test(&l, FieldState::Default, t.x + 1.0, t.y + 1.0),
        Some(FieldHit::Trailing)
    );
    assert_eq!(
        hit_test(
            &l,
            FieldState::Default,
            l.box_rect.x + 5.0,
            l.box_rect.y + 5.0
        ),
        Some(FieldHit::Box)
    );
    assert_eq!(
        hit_test(
            &l,
            FieldState::Default,
            l.label.unwrap().x + 1.0,
            l.label.unwrap().y + 1.0
        ),
        None
    );
    assert_eq!(hit_test(&l, FieldState::Default, -5.0, -5.0), None);
}

#[test]
fn disabled_field_ignores_hits() {
    let l = lay(FieldKind::Text, false, false);
    assert_eq!(
        hit_test(
            &l,
            FieldState::Disabled,
            l.box_rect.x + 5.0,
            l.box_rect.y + 5.0
        ),
        None
    );
}

#[test]
fn border_colours_per_state() {
    let th = ChromeTheme::default();
    assert_eq!(border_color(&th, FieldState::Default), th.line);
    assert_eq!(border_color(&th, FieldState::Filled), th.line);
    assert_eq!(border_color(&th, FieldState::Hover), th.hover_border);
    assert_eq!(border_color(&th, FieldState::Focus), th.accent);
    assert_eq!(border_color(&th, FieldState::Error), th.danger_fill);
    assert_eq!(border_color(&th, FieldState::Disabled), th.divider);
}

#[test]
fn mask_is_bullets() {
    assert_eq!(mask(3), "\u{2022}\u{2022}\u{2022}");
    assert_eq!(mask(0), "");
}

#[test]
fn search_box_geometry() {
    let s = search_layout((0.0, 0.0), 300.0, SearchKind::Search, 0.0);
    assert_eq!(s.box_rect.height, 40.0);
    assert_eq!(s.icon.x, SEARCH_PAD);
    assert_eq!(s.icon.width, SEARCH_ICON);
    assert_eq!(s.text.x, SEARCH_PAD + SEARCH_ICON + INNER_GAP);
    assert!(s.hint.is_none());
    assert_eq!(s.text.right(), 300.0 - SEARCH_PAD);
}

#[test]
fn command_bar_geometry_has_right_aligned_hint() {
    let s = search_layout((0.0, 0.0), 300.0, SearchKind::CommandBar, 40.0);
    assert_eq!(s.box_rect.height, 38.0);
    let h = s.hint.unwrap();
    assert_eq!(h.width, 40.0);
    assert_eq!(h.right(), 300.0 - COMMAND_PAD);
    assert_eq!(s.icon.x, COMMAND_PAD);
    assert_eq!(s.text.right(), h.x - INNER_GAP);
}

#[test]
fn port_pair_widths_and_alignment() {
    let p = port_pair_layout((5.0, 7.0));
    assert_eq!(p.local.box_rect.width, 140.0);
    assert_eq!(p.dest.box_rect.width, 320.0);
    assert_eq!(p.port.box_rect.width, 110.0);
    assert_eq!(p.local.box_rect.x, 5.0);
    assert_eq!(p.arrow.x, p.local.box_rect.right() + PORT_GAP);
    assert_eq!(p.dest.box_rect.x, p.arrow.right() + PORT_GAP);
    assert_eq!(p.port.box_rect.x, p.dest.box_rect.right() + PORT_GAP);
    // arrow is vertically centered on the boxes
    assert_eq!(p.arrow.y, p.local.box_rect.y);
    assert_eq!(p.arrow.height, 46.0);
    assert_eq!(p.dest.box_rect.y, p.port.box_rect.y);
    assert_eq!(p.total.right(), p.port.box_rect.right());
}

#[test]
fn grid_cells_three_columns() {
    let c0 = grid_cell((0.0, 0.0), 1000.0, 0, 100.0);
    let c1 = grid_cell((0.0, 0.0), 1000.0, 1, 100.0);
    let c3 = grid_cell((0.0, 0.0), 1000.0, 3, 100.0);
    let w = (1000.0 - 2.0 * GRID_COL_GAP) / 3.0;
    assert!((c0.width - w).abs() < 0.01);
    assert!((c1.x - (w + GRID_COL_GAP)).abs() < 0.01);
    assert_eq!(c3.x, 0.0);
    assert_eq!(c3.y, 100.0 + GRID_ROW_GAP);
}
