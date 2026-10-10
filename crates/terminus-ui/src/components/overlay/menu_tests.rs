use super::*;
use crate::geom::Rect;

fn sample() -> Menu {
    Menu::open(
        10.0,
        10.0,
        vec![
            MenuEntry::item("New session"),
            MenuEntry::item("Open files"),
            MenuEntry::item("Copy SSH command"),
            MenuEntry::item("Open in the other pane").disabled(),
            MenuEntry::separator(),
            MenuEntry::item("Edit server"),
            MenuEntry::item("Delete").danger(),
        ],
    )
    .unwrap()
}

#[test]
fn menu_height_is_derived_from_entries() {
    let m = sample();
    let expected = MENU_PAD * 2.0 + 6.0 * MENU_ITEM_HEIGHT + SEPARATOR_HEIGHT;
    assert!((m.height() - expected).abs() < 0.01);
    assert_eq!(m.rect().width, MENU_WIDTH);
    assert_eq!(Menu::open(0.0, 0.0, vec![]), None);
}

#[test]
fn menu_widens_for_a_long_label_and_stays_clamped() {
    let label = "A very long context menu label that overflows the default width";
    let m = Menu::open(900.0, 0.0, vec![MenuEntry::item(label)]).unwrap();
    assert!(m.rect().width > MENU_WIDTH);
    let r = m.item_rect(0).unwrap();
    assert_eq!(r.width, m.rect().width - 2.0 * MENU_PAD);
    let clamped = m.clamped(1000.0, 700.0);
    assert!(clamped.rect().right() <= 1000.0 - 8.0 + 0.01);
}

#[test]
fn menu_rows_stack_inside_padding() {
    let m = sample();
    let first = m.item_rect(0).unwrap();
    assert_eq!((first.x, first.y), (10.0 + MENU_PAD, 10.0 + MENU_PAD));
    assert_eq!(first.height, MENU_ITEM_HEIGHT);
    let after_sep = m.item_rect(5).unwrap();
    let before = m.item_rect(3).unwrap();
    assert!((after_sep.y - before.bottom() - SEPARATOR_HEIGHT).abs() < 0.01);
    assert!(m.separator_rect(4).is_some() && m.separator_rect(0).is_none());
    assert!(m.item_rect(4).is_none(), "separators are not item rows");
}

#[test]
fn menu_is_clamped_inside_window() {
    let m = Menu::open(
        990.0,
        690.0,
        vec![MenuEntry::item("A"), MenuEntry::item("B")],
    )
    .unwrap()
    .clamped(1000.0, 700.0);
    let r = m.rect();
    assert!(r.right() <= 1000.0 && r.bottom() <= 700.0 && r.x >= 0.0 && r.y >= 0.0);
}

#[test]
fn menu_hit_test_and_hover_ignore_disabled_and_separators() {
    let mut m = sample();
    let c = |r: Rect| (r.x + 4.0, r.y + 4.0);
    let (x, y) = c(m.item_rect(1).unwrap());
    assert_eq!(m.hit_test(x, y), MenuHit::Item(1));
    let (x, y) = c(m.item_rect(3).unwrap());
    assert_eq!(
        m.hit_test(x, y),
        MenuHit::Consume,
        "disabled rows swallow the click"
    );
    let sep = m.separator_rect(4).unwrap();
    assert_eq!(m.hit_test(sep.x + 2.0, sep.y + 2.0), MenuHit::Consume);
    assert_eq!(m.hit_test(900.0, 900.0), MenuHit::Dismiss);
    assert!(!m.hover_at(x, y) && m.hover.is_none());
    let (x, y) = c(m.item_rect(1).unwrap());
    assert!(m.hover_at(x, y));
    assert_eq!(m.hover, Some(1));
}

#[test]
fn menu_keyboard_skips_disabled_and_separators_and_wraps() {
    let mut m = sample();
    m.move_hover(1);
    assert_eq!(m.hover, Some(0));
    m.move_hover(1);
    m.move_hover(1);
    assert_eq!(m.hover, Some(2));
    m.move_hover(1);
    assert_eq!(m.hover, Some(5), "skips disabled item and separator");
    m.move_hover(1);
    assert_eq!(m.hover, Some(6));
    m.move_hover(1);
    assert_eq!(m.hover, Some(0), "wraps");
    m.move_hover(-1);
    assert_eq!(m.hover, Some(6), "wraps back");
    m.move_hover(-1);
    m.move_hover(-1);
    assert_eq!(m.hover, Some(2), "up skips too");
}

#[test]
fn menu_activation_refuses_disabled() {
    let m = sample();
    assert!(m.activate(0));
    assert!(!m.activate(3));
    assert!(!m.activate(4));
    assert!(!m.activate(99));
}

#[test]
fn menu_visual_state_follows_design() {
    let mut m = sample();
    m.hover = Some(0);
    assert_eq!(m.visual(0), MenuVisual::Hover);
    assert_eq!(m.visual(1), MenuVisual::Default);
    assert_eq!(m.visual(3), MenuVisual::Disabled);
    assert_eq!(m.visual(6), MenuVisual::Danger);
    m.hover = Some(6);
    assert_eq!(m.visual(6), MenuVisual::DangerHover);
}
