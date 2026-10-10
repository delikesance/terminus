use super::*;
use crate::geom::Rect;
use crate::theme::ChromeTheme;

fn lbl(size: ButtonSize, icon: bool) -> ButtonSpec {
    ButtonSpec::label((10.0, 20.0), ButtonKind::Primary, size, 60.0, icon)
}

#[test]
fn size_metrics_match_board() {
    let m = |s: ButtonSize| (s.height(), s.radius(), s.padding_x(), s.font_size());
    assert_eq!(m(ButtonSize::Large), (44.0, 10.0, 20.0, 14.0));
    assert_eq!(m(ButtonSize::Medium), (36.0, 8.0, 14.0, 13.0));
    assert_eq!(m(ButtonSize::Small), (30.0, 7.0, 10.0, 12.0));
}

#[test]
fn weights_semibold_for_primary_and_danger_only() {
    let semi: Vec<_> = ButtonKind::ALL.iter().map(|k| k.semibold()).collect();
    assert_eq!(semi, [true, false, true, false, false]);
}

#[test]
fn state_resolution_priority() {
    use ButtonState::*;
    assert_eq!(ButtonState::resolve(false, false, false, false), Default);
    assert_eq!(ButtonState::resolve(true, false, false, false), Hover);
    assert_eq!(ButtonState::resolve(true, true, false, false), Pressed);
    assert_eq!(ButtonState::resolve(false, false, true, false), Focus);
    assert_eq!(ButtonState::resolve(true, false, true, false), Hover);
    assert_eq!(ButtonState::resolve(true, true, true, true), Disabled);
    assert_eq!(Disabled.opacity(), 0.4);
    assert_eq!(Default.opacity(), 1.0);
    assert!(Focus.shows_focus_ring() && !Hover.shows_focus_ring());
}

#[test]
fn width_is_padding_plus_label_plus_icon() {
    assert_eq!(lbl(ButtonSize::Large, false).width(), 40.0 + 60.0);
    assert_eq!(
        lbl(ButtonSize::Medium, true).width(),
        28.0 + 15.0 + 8.0 + 60.0
    );
    assert_eq!(
        lbl(ButtonSize::Small, true).width(),
        20.0 + 13.0 + 8.0 + 60.0
    );
    let r = lbl(ButtonSize::Large, false).rect();
    assert_eq!((r.x, r.y, r.height), (10.0, 20.0, 44.0));
}

#[test]
fn label_origin_follows_icon() {
    let s = lbl(ButtonSize::Large, true);
    let ic = s.icon_rect().unwrap();
    assert_eq!(ic.x, 30.0);
    assert_eq!(ic.y, 20.0 + (44.0 - 15.0) / 2.0);
    let (lx, ly) = s.label_origin().unwrap();
    assert_eq!(lx, 30.0 + 15.0 + 8.0);
    assert_eq!(ly, 20.0 + (44.0 - 14.0) / 2.0);
    let plain = lbl(ButtonSize::Large, false);
    assert!(plain.icon_rect().is_none());
    assert_eq!(plain.label_origin().unwrap().0, 30.0);
}

#[test]
fn icon_only_is_square_and_centred() {
    for (size, solo) in [
        (ButtonSize::Medium, 16.0),
        (ButtonSize::Small, 14.0),
        (ButtonSize::Large, 16.0),
    ] {
        let s = ButtonSpec::icon_only((0.0, 0.0), ButtonKind::Quiet, size);
        let r = s.rect();
        assert_eq!(r.width, r.height);
        assert_eq!(r.height, size.height());
        let ic = s.icon_rect().unwrap();
        assert_eq!(ic.width, solo);
        assert_eq!(ic.x - r.x, r.right() - ic.right());
        assert_eq!(ic.y - r.y, r.bottom() - ic.bottom());
        assert!(s.label_origin().is_none());
        assert_eq!(s.tooltip("Close"), Some("Close"));
    }
    assert_eq!(lbl(ButtonSize::Large, false).tooltip("x"), None);
}

#[test]
fn focus_ring_is_two_gap_plus_two_ring() {
    let s = lbl(ButtonSize::Medium, false);
    let r = s.rect();
    let gap = s.focus_gap_rect();
    let ring = s.focus_ring_rect();
    assert_eq!((gap.x, gap.width), (r.x - 2.0, r.width + 4.0));
    assert_eq!((ring.x, ring.height), (r.x - 4.0, r.height + 8.0));
    assert_eq!(s.focus_gap_radius(), 10.0);
    assert_eq!(s.focus_ring_radius(), 12.0);
}

#[test]
fn hit_test_half_open_and_disabled() {
    let s = lbl(ButtonSize::Large, false);
    let r = s.rect();
    assert!(s.hit_test(r.x, r.y, false));
    assert!(s.hit_test(r.right() - 0.1, r.bottom() - 0.1, false));
    assert!(!s.hit_test(r.right(), r.y, false));
    assert!(!s.hit_test(r.x, r.y - 0.1, false));
    assert!(!s.hit_test(r.x + 5.0, r.y + 5.0, true));
}

#[test]
fn colors_match_board() {
    let t = ChromeTheme::violet_ink();
    let c = |k, s| colors(&t, k, s);
    use ButtonKind::*;
    use ButtonState::*;
    assert_eq!(c(Primary, Default).fill, Some(t.accent));
    assert_eq!(c(Primary, Hover).fill, Some(t.accent_hover));
    assert_eq!(c(Primary, Pressed).fill, Some(t.accent_press));
    assert_eq!(c(Primary, Focus), c(Primary, Default));
    assert_eq!(c(Primary, Disabled), c(Primary, Default));
    assert_eq!(c(Primary, Default).fg, t.on_accent);
    assert_eq!(c(Secondary, Default).fill, Some(t.raised));
    assert_eq!(c(Secondary, Hover).fill, Some(t.selected));
    assert_eq!(c(Secondary, Pressed).fill, Some(t.line));
    assert_eq!(c(Danger, Default).fill, Some(t.danger_fill));
    assert_eq!(c(Danger, Hover).fill, Some(t.danger_hover));
    assert_eq!(c(Danger, Pressed).fill, Some(t.danger_press));
    assert_eq!(c(Danger, Default).fg, t.on_danger);
    assert_eq!(c(Text, Default).fill, None);
    assert_eq!(c(Text, Default).fg, t.accent);
    assert_eq!(c(Text, Hover).fill, Some(t.surface));
    assert_eq!(c(Text, Hover).fg, t.accent_hover);
    assert_eq!(c(Text, Pressed).fill, Some(t.raised));
    assert_eq!(c(Text, Pressed).fg, t.accent);
    assert_eq!(c(Quiet, Default).fill, None);
    assert_eq!(c(Quiet, Default).fg, rgba(t.text_muted));
    assert_eq!(c(Quiet, Hover).fill, Some(t.surface));
    assert_eq!(c(Quiet, Hover).fg, rgba(t.text));
    assert_eq!(c(Quiet, Pressed).fill, Some(t.raised));
}

#[test]
fn in_rect_keeps_width_and_centres_vertically() {
    let rect = Rect::new(10.0, 20.0, 100.0, 40.0);
    let r = ButtonSpec::in_rect(rect, ButtonKind::Primary, ButtonSize::Medium).rect();
    assert_eq!((r.x, r.width, r.height), (10.0, 100.0, 36.0));
    assert_eq!(r.y, 22.0);
}

#[test]
fn dashed_cta_badge_is_vertically_centered() {
    let cta = Rect::new(0.0, 0.0, 200.0, 56.0);
    let badge = dashed_cta_badge(cta, 32.0, 12.0);
    assert!((badge.x - 12.0).abs() < 0.01);
    assert!((badge.y - 12.0).abs() < 0.01);
    assert!((badge.width - 32.0).abs() < 0.01);
}

#[test]
fn hover_changes_primary_and_secondary_fill() {
    let t = ChromeTheme::violet_ink();
    for kind in [ButtonKind::Primary, ButtonKind::Secondary] {
        let rest = colors(&t, kind, ButtonState::Default).fill;
        assert_ne!(colors(&t, kind, ButtonState::Hover).fill, rest);
    }
}

#[test]
fn primary_fill_is_accent_and_secondary_label_is_text() {
    let t = ChromeTheme::violet_ink();
    let primary = colors(&t, ButtonKind::Primary, ButtonState::Default);
    assert_eq!(primary.fill, Some(t.accent));
    let secondary = colors(&t, ButtonKind::Secondary, ButtonState::Default);
    assert_eq!(secondary.fg, rgba(t.text));
}

#[test]
fn quiet_label_is_muted_until_hovered() {
    let t = ChromeTheme::violet_ink();
    let rest = colors(&t, ButtonKind::Quiet, ButtonState::Default).fg;
    assert_eq!(rest, rgba(t.text_muted));
}
