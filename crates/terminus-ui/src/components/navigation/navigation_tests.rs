use super::*;
use crate::geom::Rect;
use crate::theme::ChromeTheme;

#[test]
fn server_row_geometry_matches_board() {
    let row = Rect::new(10.0, 20.0, 248.0, server_row::HEIGHT);
    assert_eq!(server_row::HEIGHT, 44.0);
    let tile = server_row::tile_rect(&row);
    assert_eq!((tile.x, tile.width, tile.height), (18.0, 28.0, 28.0));
    assert_eq!(tile.y, 20.0 + 8.0);
    assert_eq!(server_row::name_x(&row), 18.0 + 28.0 + 12.0);
    assert_eq!(server_row::meta_right(&row), 258.0 - 8.0);
    let (outer, inner) = server_row::focus_ring(&row);
    assert_eq!(inner.x, row.x - 2.0);
    assert_eq!(outer.x, row.x - 4.0);
    assert_eq!(outer.height, row.height + 8.0);
}

#[test]
fn server_row_styles_per_state() {
    let t = ChromeTheme::default();
    let d = server_row::style(&t, RowState::Default);
    assert_eq!(d.bg, None);
    assert!(!d.medium && !d.tile_active && !d.shadow && !d.ring);
    assert_eq!(server_row::style(&t, RowState::Hover).bg, Some(t.surface));
    let s = server_row::style(&t, RowState::Selected);
    assert_eq!(s.bg, Some(t.selected));
    assert!(s.medium && s.tile_active);
    assert!(server_row::style(&t, RowState::Focus).ring);
    let g = server_row::style(&t, RowState::Dragging);
    assert_eq!(g.bg, Some(t.raised));
    assert!(g.shadow);
}

#[test]
fn meta_colour_by_tone_and_state() {
    let t = ChromeTheme::default();
    assert_eq!(
        server_row::meta_color(&t, MetaTone::Success, RowState::Default),
        crate::theme::text_color(t.success)
    );
    assert_eq!(
        server_row::meta_color(&t, MetaTone::Muted, RowState::Default),
        t.text_muted
    );
    assert_eq!(
        server_row::meta_color(&t, MetaTone::Muted, RowState::Selected),
        t.selected_subtle_text
    );
}

#[test]
fn row_meta_labels() {
    assert_eq!(RowMeta::None.label(), None);
    assert_eq!(RowMeta::Running.label().as_deref(), Some("Running"));
    assert_eq!(RowMeta::Sessions(2).label().as_deref(), Some("2"));
    assert_eq!(RowMeta::Running.tone(), MetaTone::Success);
    assert_eq!(RowMeta::Sessions(2).tone(), MetaTone::Muted);
}

#[test]
fn row_hit_test() {
    let row = Rect::new(0.0, 0.0, 248.0, 44.0);
    assert!(row_hit(&row, 100.0, 22.0));
    assert!(!row_hit(&row, 100.0, 44.0));
    let rows = stack_rows(0.0, 0.0, 248.0, 3, 2.0);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1].y, 46.0);
    assert_eq!(hit_row(&rows, 5.0, 50.0), Some(1));
    assert_eq!(hit_row(&rows, 5.0, 45.0), None); // in the gap
}

#[test]
fn headers_and_drop_target() {
    let h = Rect::new(0.0, 0.0, 248.0, section_header::HEIGHT);
    let a = section_header::action_rect(&h, 60.0);
    assert_eq!(a.right(), h.right());
    assert_eq!(a.width, 60.0);
    assert_eq!(a.height, h.height);
    let t = drop_target::rect(0.0, 0.0, 248.0);
    assert_eq!((t.width, t.height), (248.0, 44.0));
    assert_eq!(drop_target::RADIUS, 10.0);
    assert!((drop_target::FILL_ALPHA - 0.08).abs() < 1e-6);
    let f = drop_target::fill(&ChromeTheme::default());
    assert!((f[3] - 0.08).abs() < 1e-6);
}

#[test]
fn tabs_layout_and_style() {
    let t = ChromeTheme::default();
    let tabs = view_tabs::layout(
        0.0,
        0.0,
        &[
            TabSize {
                label_w: 40.0,
                badge_w: 0.0,
            },
            TabSize {
                label_w: 50.0,
                badge_w: 8.0,
            },
        ],
    );
    assert_eq!(tabs[0].width, 40.0);
    assert_eq!(tabs[0].height, view_tabs::HEIGHT);
    assert_eq!(tabs[1].x, 40.0 + view_tabs::GAP);
    assert_eq!(tabs[1].width, 50.0 + view_tabs::BADGE_GAP + 8.0);
    assert_eq!(view_tabs::hit(&tabs, 45.0, 5.0), None);
    assert_eq!(view_tabs::hit(&tabs, 70.0, 5.0), Some(1));
    let u = view_tabs::underline_rect(&tabs[0]);
    assert_eq!((u.height, u.bottom()), (2.0, tabs[0].bottom()));
    let a = view_tabs::style(&t, TabState::Active);
    assert!(a.medium);
    assert_eq!(a.underline, Some(t.accent));
    assert_eq!(a.text, t.text);
    let h = view_tabs::style(&t, TabState::Hover);
    assert_eq!(
        (h.text, h.underline, h.medium),
        (t.text, Some(t.line), false)
    );
    let d = view_tabs::style(&t, TabState::Default);
    assert_eq!((d.text, d.underline), (t.text_muted, None));
}

#[test]
fn pill_geometry_and_hits() {
    assert_eq!(session_pill::HEIGHT, 30.0);
    assert_eq!(session_pill::width(40.0, false, false), 12.0 + 40.0 + 12.0);
    assert_eq!(
        session_pill::width(40.0, true, false),
        12.0 + 40.0 + 8.0 + 12.0 + 8.0
    );
    assert_eq!(
        session_pill::width(40.0, false, true),
        12.0 + 6.0 + 8.0 + 40.0 + 12.0
    );
    let r = Rect::new(100.0, 0.0, session_pill::width(40.0, true, false), 30.0);
    let c = session_pill::close_rect(&r);
    assert_eq!(
        c.right(),
        r.right() - 8.0 + (session_pill::CLOSE_HIT - session_pill::CLOSE_ICON) / 2.0
    );
    assert_eq!(c.y + c.height / 2.0, 15.0);
    assert_eq!(
        session_pill::hit(&r, true, c.x + 1.0, 15.0),
        Some(PillHit::Close)
    );
    assert_eq!(
        session_pill::hit(&r, true, 105.0, 15.0),
        Some(PillHit::Body)
    );
    assert_eq!(
        session_pill::hit(&r, false, r.right() - 10.0, 15.0),
        Some(PillHit::Body)
    );
    assert_eq!(session_pill::hit(&r, true, 0.0, 15.0), None);
    let dot = session_pill::dot_rect(&r);
    assert_eq!((dot.x, dot.width), (112.0, 6.0));
    assert_eq!(dot.y + 3.0, 15.0);
    assert_eq!(session_pill::text_x(&r, true), 100.0 + 12.0 + 6.0 + 8.0);
    assert_eq!(session_pill::text_x(&r, false), 112.0);
}

#[test]
fn pill_styles() {
    let t = ChromeTheme::default();
    let d = session_pill::style(&t, PillState::Default);
    assert_eq!((d.bg, d.close, d.dot), (None, false, false));
    assert_eq!(d.text, t.text_muted);
    let h = session_pill::style(&t, PillState::Hover);
    assert_eq!((h.bg, h.close, h.text), (Some(t.surface), true, t.text));
    let a = session_pill::style(&t, PillState::Active);
    assert_eq!((a.bg, a.close, a.text), (Some(t.divider), true, t.text));
    let n = session_pill::style(&t, PillState::NewOutput);
    assert_eq!((n.bg, n.close, n.dot), (None, false, true));
    assert_eq!(n.text, t.text_muted);
}
