use super::test_support::*;
use super::*;
use crate::os_icons::HostStatus;

#[test]
fn section_label_and_geometry_helpers_are_consistent() {
    let (_oy, _) = tall();
    let rect = Rect::new(0.0, 100.0, 100.0, SECTION_HEIGHT);
    // 12px label centred in the 15px line above the 6px gap.
    assert_eq!(section_label_y(rect), 100.0 + 1.5);

    let card = Rect::new(0.0, 100.0, 100.0, ITEM_HEIGHT);
    assert_eq!(host_item_separator_y(card), 100.0 + 44.0 - 1.0);

    let badge = host_badge_rect(card);
    assert_eq!(badge.y, 100.0 + (44.0 - 28.0) / 2.0);
}

#[test]
fn rows_are_hit_at_their_painted_position() {
    let panel = panel(3);
    let (oy, h) = tall();
    for index in 0..3 {
        let rect = panel.item_rect(oy, index);
        let (x, y) = (rect.x + 10.0, rect.y + rect.height / 2.0);
        assert_eq!(panel.hit_test(oy, h, x, y), Some(PanelHit::Item(index)));
    }
}

#[test]
fn the_add_button_is_its_own_target() {
    let panel = panel(2);
    let (oy, h) = tall();
    let button = panel.add_button_rect(oy, h);
    let (x, y) = (
        button.x + button.width / 2.0,
        button.y + button.height / 2.0,
    );
    assert_eq!(panel.hit_test(oy, h, x, y), Some(PanelHit::AddHost));

    // The footer strip beside the button is not the button.
    let beside = Rect::new(panel.footer_rect(oy, h).x, y, 4.0, 1.0);
    assert_eq!(
        panel.hit_test(oy, h, beside.x + 1.0, y),
        Some(PanelHit::Background)
    );
}

#[test]
fn layout_constants_match_the_violet_ink_shell() {
    assert_eq!(WIDTH, 260.0);
    assert_eq!(ORIGIN_X, 0.0);
    assert_eq!(PAD_X, 12.0);
    assert_eq!(ITEM_HEIGHT, 44.0);
    assert_eq!(CARD_GAP, 2.0);
    assert_eq!(SECTION_GAP, 16.0);
    assert_eq!(SECTION_HEIGHT, 21.0);
    assert_eq!(HEADER_HEIGHT, crate::shell::sidebar::list_top());
    assert_eq!(HOST_BADGE_TILE, 28.0);
}

#[test]
fn the_panel_claims_nothing_left_of_itself() {
    let panel = panel(3);
    let (oy, h) = tall();
    let row = panel.item_rect(oy, 0);
    assert_eq!(panel.hit_test(oy, h, ORIGIN_X - 1.0, row.y + 1.0), None);
    assert_eq!(panel.hit_test(oy, h, ORIGIN_X + WIDTH, row.y + 1.0), None);
}

#[test]
fn scrolling_moves_the_hit_targets_with_the_pixels() {
    let mut panel = panel(20);
    let (oy, h) = (0.0, 400.0);
    let body = panel.body_rect(oy, h);
    assert_eq!(
        panel.max_scroll(oy, h),
        panel.content_height() - body.height
    );

    let y0 = {
        let mut unscrolled = panel.clone();
        unscrolled.scroll = 0.0;
        unscrolled.item_rect(oy, 0).y
    };

    panel.scroll_rows(WHEEL_ROWS, oy, h);
    assert_eq!(panel.scroll, WHEEL_ROWS * (ITEM_HEIGHT + CARD_GAP));

    // The pixel that held row 0 before the scroll now holds row 3.
    assert!(panel.item_rect(oy, 0).bottom() < y0 + 2.0);
    assert_eq!(
        panel.hit_test(oy, h, ORIGIN_X + PAD_X + 10.0, y0 + 2.0),
        Some(PanelHit::Item(3))
    );
}

#[test]
fn scroll_is_clamped_to_the_content() {
    let mut panel = panel(3);
    let (oy, h) = tall();
    panel.scroll_rows(100.0, oy, h);
    assert_eq!(panel.scroll, 0.0);

    let (oy, h) = (0.0, 200.0);
    panel.scroll_rows(100.0, oy, h);
    assert_eq!(panel.scroll, panel.max_scroll(oy, h));
}

#[test]
fn losing_hosts_pulls_the_list_back_into_view() {
    let mut panel = panel(20);
    let (oy, h) = (0.0, 400.0);
    panel.scroll_rows(20.0, oy, h);
    let scrolled = panel.scroll;
    assert!(scrolled > 0.0);

    // Every host but two is gone, so there is no longer anything to
    // scroll to — without the clamp the panel would sit empty.
    panel.set_items(items(2));
    panel.clamp_scroll(oy, h);
    assert_eq!(panel.scroll, 0.0);
    assert_eq!(panel.max_scroll(oy, h), 0.0);
    assert_eq!(
        panel.hit_test(
            oy,
            h,
            ORIGIN_X + PAD_X + 10.0,
            panel.item_rect(oy, 0).y + 2.0
        ),
        Some(PanelHit::Item(0))
    );
}

#[test]
fn a_shrinking_window_keeps_the_scroll_legal() {
    let mut panel = panel(20);
    let (oy, h) = (0.0, 400.0);
    panel.scroll_rows(20.0, oy, h);
    assert_eq!(panel.scroll, panel.max_scroll(oy, h));

    // A shorter window means a smaller viewport and therefore a
    // larger maximum, so the offset stays legal — but it must stay
    // inside the new bounds, which is what the clamp guarantees
    // before every paint.
    let short = 200.0;
    panel.clamp_scroll(oy, short);
    let max = panel.max_scroll(oy, short);
    assert!(panel.scroll <= max, "{} > {max}", panel.scroll);
    assert!(panel.scroll > 0.0);

    // And a viewport taller than the content pins the list back to
    // the top instead of leaving it scrolled into empty space.
    panel.clamp_scroll(oy, 4000.0);
    assert_eq!(panel.scroll, 0.0);
}

#[test]
fn refreshing_keeps_the_selected_host_selected() {
    let mut panel = panel(3);
    panel.selected = Some(1);
    assert_eq!(panel.selected_id(), Some("id-1"));

    // A new host is prepended: index 1 is no longer the same host.
    let mut next = items(3);
    next.insert(
        0,
        HostItem {
            id: "new".to_string(),
            name: "new".to_string(),
            endpoint: "root@new".to_string(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Idle,
            nested: false,
            session_count: 0,
        },
    );
    panel.set_items(next);
    assert_eq!(panel.selected_id(), Some("id-1"));
    assert_eq!(panel.selected, Some(2));
    assert_eq!(panel.hover, None);
}

#[test]
fn a_deleted_selection_is_dropped_rather_than_moved() {
    let mut panel = panel(3);
    panel.selected = Some(2);
    panel.set_items(items(1));
    assert_eq!(panel.selected, None);
    assert_eq!(panel.selected_id(), None);
}

#[test]
fn last_session_gets_extra_gap_before_next_host() {
    let mut panel = HostPanel::default();
    panel.set_rows(vec![
        Row::Host(HostItem {
            id: "a".into(),
            name: "A".into(),
            endpoint: "a".into(),
            badge: Badge::Local,
            stored: false,
            os_id: None,
            status: HostStatus::Active,
            nested: false,
            session_count: 1,
        }),
        Row::Session(SessionItem {
            tab_index: 0,
            host_id: "a".into(),
            title: "shell".into(),
            active: true,
            closable: false,
        }),
        Row::Host(HostItem {
            id: "b".into(),
            name: "B".into(),
            endpoint: "b".into(),
            badge: Badge::Wsl,
            stored: false,
            os_id: None,
            status: HostStatus::Idle,
            nested: false,
            session_count: 0,
        }),
    ]);
    assert_eq!(panel.row_slot_height(1), SESSION_HEIGHT + SESSION_AFTER_GAP);
    let oy = 0.0;
    let session_bottom = panel.card_rect(oy, 1).bottom();
    let next_top = panel.card_rect(oy, 2).y;
    assert!(
        next_top - session_bottom >= SESSION_AFTER_GAP - 0.5,
        "gap {} < SESSION_AFTER_GAP",
        next_top - session_bottom
    );

    let (oy, h) = tall();
    // Breathing gap after the session is not a hit target.
    let gap_y = session_bottom + SESSION_AFTER_GAP * 0.5;
    assert_eq!(
        panel.hit_test(oy, h, ORIGIN_X + PAD_X + 10.0, gap_y),
        Some(PanelHit::Background)
    );
    let next = panel.card_rect(oy, 2);
    assert_eq!(
        panel.hit_test(oy, h, next.x + 10.0, next.y + next.height / 2.0),
        Some(PanelHit::Item(2))
    );
}

#[test]
fn sessions_nest_under_hosts_and_collapse_with_them() {
    let mut panel = HostPanel::default();
    panel.set_rows(vec![
        Row::Host(HostItem {
            id: "local".to_string(),
            name: "This computer".to_string(),
            endpoint: "nixos".to_string(),
            badge: Badge::Local,
            stored: false,
            os_id: None,
            status: HostStatus::Idle,
            nested: false,
            session_count: 2,
        }),
        Row::Session(SessionItem {
            tab_index: 0,
            host_id: "local".to_string(),
            title: "This computer".to_string(),
            active: true,
            closable: false,
        }),
        Row::Session(SessionItem {
            tab_index: 1,
            host_id: "local".to_string(),
            title: "shell".to_string(),
            active: false,
            closable: true,
        }),
    ]);
    let (oy, h) = tall();
    assert_eq!(panel.visible_row_indices(), vec![0, 1, 2]);
    let session = panel.card_rect(oy, 1);
    assert_eq!(
        panel.hit_test(oy, h, session.x + 10.0, session.y + 4.0),
        Some(PanelHit::Session(1))
    );
    let close = panel.session_close_rect(oy, 2).expect("closable");
    assert_eq!(
        panel.hit_test(oy, h, close.x + 2.0, close.y + 2.0),
        Some(PanelHit::CloseSession(2))
    );
    panel.toggle_host_collapsed("local");
    assert_eq!(panel.visible_row_indices(), vec![0]);
    assert!(panel.select_session(1));
    assert_eq!(panel.selected_session, Some(1));
    assert_eq!(panel.visible_row_indices(), vec![0, 1, 2]);
}

#[test]
fn following_a_vanished_host_clears_the_highlight() {
    let mut panel = panel(3);
    panel.follow_host("id-2");
    assert_eq!(panel.selected_id(), Some("id-2"));

    // The host behind the highlight is gone: the highlight goes with it
    // instead of staying on whichever row now sits at that index.
    panel.follow_host("id-9");
    assert_eq!(panel.selected, None);
    assert_eq!(panel.selected_id(), None);
}

#[test]
fn the_error_banner_sits_at_the_bottom_without_pushing_rows() {
    let mut panel = panel(2);
    let (oy, h) = tall();
    let without = panel.item_rect(oy, 0).y;

    panel.error = Some("'x' is not a valid port".to_string());
    let with = panel.item_rect(oy, 0).y;
    assert_eq!(with, without, "sticky footer must not shift rows");

    let banner = panel.notice_rect(oy, h).expect("error banner");
    assert!(
        (banner.bottom() - panel.list_bottom(oy, h)).abs() < 0.01,
        "banner should hug the bottom of the list"
    );
    assert!((banner.height - ERROR_BANNER_HEIGHT).abs() < 0.01);
    assert!(panel.body_rect(oy, h).bottom() <= banner.y + 0.01);

    let row = panel.item_rect(oy, 0);
    assert_eq!(
        panel.hit_test(oy, h, ORIGIN_X + PAD_X + 4.0, row.y + 2.0),
        Some(PanelHit::Item(0))
    );
}

#[test]
fn the_notice_never_overlaps_the_add_row() {
    // A wide range of window heights: the sticky notice is anchored to the
    // bottom and the body must never extend under it.
    for height in [120.0, 200.0, 400.0, 1000.0] {
        let mut panel = panel(30);
        panel.notice = Some("Added web-01".to_string());
        let body = panel.body_rect(0.0, height);
        let footer = panel.footer_rect(0.0, height);
        assert!(body.bottom() <= footer.y, "height {height}");
        assert!(body.height >= 0.0, "height {height}");
        if let Some(notice) = panel.notice_rect(0.0, height) {
            assert!(body.bottom() <= notice.y + 0.01, "height {height}");
        }
    }
}

#[test]
fn a_long_notice_grows_to_fit_up_to_three_lines() {
    let mut panel = panel(3);
    panel.notice = Some("Added web-01".to_string());
    assert_eq!(panel.notice_lines(), 1);
    let short = panel.notice_rect(0.0, 800.0).expect("notice");
    assert!((short.height - NOTICE_HEIGHT).abs() < 0.01);

    panel.notice = Some("Terminus 9.9.9 is installed. Restart to update".to_string());
    assert_eq!(panel.notice_lines(), 2);
    let two = panel.notice_rect(0.0, 800.0).expect("notice");
    assert!((two.height - (NOTICE_HEIGHT + NOTICE_LINE_STEP)).abs() < 0.01);
    assert!(
        (two.bottom() - short.bottom()).abs() < 0.01,
        "stays anchored"
    );
    assert!(panel.body_rect(0.0, 800.0).bottom() <= two.y + 0.01);

    panel.notice = Some("x ".repeat(200));
    assert_eq!(panel.notice_lines(), 3, "capped");
}
