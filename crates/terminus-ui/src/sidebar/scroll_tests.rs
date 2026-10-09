use super::test_support::*;
use super::*;
use crate::components::input::TextMoveKind;

#[test]
fn connecting_indicator_tracks_the_host_id() {
    let mut panel = HostPanel::default();
    panel.set_rows(grouped());
    panel.begin_connecting("wsl:Ubuntu-24.04");
    assert!(panel.is_connecting("wsl:Ubuntu-24.04"));
    assert!(!panel.is_connecting("local"));

    let index = panel.row_of_host("wsl:Ubuntu-24.04").unwrap();
    assert!(panel.connecting_center(0.0, index).is_some());
    assert!(panel.connecting_shimmer_track(0.0, index).is_some());

    // Survives a regrouping that keeps the host.
    panel.set_rows(grouped());
    assert!(panel.is_connecting("wsl:Ubuntu-24.04"));

    // Drops when the host vanishes.
    panel.set_rows(vec![]);
    assert!(!panel.any_connecting());
}

#[test]
fn several_hosts_can_connect_at_once() {
    let mut panel = HostPanel::default();
    panel.set_rows(grouped());
    panel.begin_connecting("wsl:Ubuntu-24.04");
    panel.begin_connecting("local");
    panel.begin_connecting("local");
    assert!(panel.is_connecting("wsl:Ubuntu-24.04") && panel.is_connecting("local"));

    panel.end_connecting("local");
    assert!(panel.is_connecting("wsl:Ubuntu-24.04") && !panel.is_connecting("local"));

    panel.end_all_connecting();
    assert!(!panel.any_connecting());
}

#[test]
fn rename_draft_moves_caret_and_inserts_spaces() {
    let mut panel = HostPanel::default();
    panel.begin_rename("h1".into(), false, "web");
    let draft = panel.rename.as_mut().unwrap();
    assert_eq!(draft.text.caret, 3);
    // End drops the initial select-all, caret stays at the end.
    draft.text.move_end(TextMoveKind::Collapse);
    assert_eq!(draft.text.selection_range(), None);
    assert!(draft.text.move_left(TextMoveKind::Collapse, false));
    assert_eq!(draft.text.caret, 2);
    assert!(draft.text.insert(" ", 64, false));
    assert_eq!(draft.text.value, "we b");
    assert_eq!(draft.text.caret, 3);
    assert!(draft.text.insert("01", 64, false));
    assert_eq!(draft.text.value, "we 01b");
    assert_eq!(draft.text.prefix(), "we 01");
    assert!(draft.text.move_home(TextMoveKind::Collapse));
    assert_eq!(draft.text.caret, 0);
    assert!(draft.text.move_end(TextMoveKind::Collapse));
    assert_eq!(draft.text.caret, draft.text.value.chars().count());
    assert!(draft.text.backspace(false));
    assert_eq!(draft.text.value, "we 01");
}

#[test]
fn rename_draft_shift_selects_and_ctrl_skips_words() {
    let mut panel = HostPanel::default();
    panel.begin_rename("h1".into(), false, "main-server box");
    let draft = panel.rename.as_mut().unwrap();
    draft.text.move_home(TextMoveKind::Collapse);
    assert!(draft.text.move_right(TextMoveKind::Extend, false));
    assert!(draft.text.move_right(TextMoveKind::Extend, false));
    assert!(draft.text.move_right(TextMoveKind::Extend, false));
    assert_eq!(draft.text.selection_range(), Some((0, 3)));
    assert!(draft.text.move_right(TextMoveKind::Collapse, true));
    // Collapse to end of selection then word-jump would need two steps;
    // after collapse-by-right with selection, caret is at 3.
    assert_eq!(draft.text.caret, 3);
    assert!(draft.text.selection_range().is_none());
    assert!(draft.text.move_right(TextMoveKind::Collapse, true));
    assert_eq!(draft.text.caret, 4); // after "main" (shared word rules)
    assert!(draft.text.select_all());
    assert_eq!(
        draft.text.selection_range(),
        Some((0, draft.text.value.chars().count()))
    );
    assert!(draft.text.insert("foo bar", 64, false));
    assert_eq!(draft.text.value, "foo bar");
    draft.text.move_end(TextMoveKind::Collapse);
    assert!(draft.text.backspace(true));
    assert_eq!(draft.text.value, "foo ");
}

#[test]
fn toggle_host_expansion_toggles_collapsed_hosts() {
    let mut panel = panel_with_sessions();
    assert!(!panel.collapsed_hosts.contains("host-a"));
    assert_eq!(panel.visible_row_indices(), vec![0, 1, 2]);

    // First toggle: collapses session rows.
    panel.toggle_host_collapsed("host-a");
    assert!(panel.collapsed_hosts.contains("host-a"));
    assert_eq!(panel.visible_row_indices(), vec![0], "sessions hidden");

    // Second toggle: expands them back.
    panel.toggle_host_collapsed("host-a");
    assert!(!panel.collapsed_hosts.contains("host-a"));
    assert_eq!(
        panel.visible_row_indices(),
        vec![0, 1, 2],
        "sessions restored"
    );
}

#[test]
fn toggle_group_collapses_and_expands() {
    let mut panel = panel_with_group();
    // Group visible with its nested child: indices 0 (group) and 1 (nested host).
    assert_eq!(panel.visible_row_indices(), vec![0, 1]);

    // Collapse: nested host disappears.
    panel.collapsed_groups.insert("grp-1".into());
    assert_eq!(panel.visible_row_indices(), vec![0], "nested host hidden");

    // Expand: nested host is back.
    panel.collapsed_groups.remove("grp-1");
    assert_eq!(
        panel.visible_row_indices(),
        vec![0, 1],
        "nested host restored"
    );
}

#[test]
fn groups_above_servers_are_part_of_the_stored_area() {
    let mut panel = HostPanel::default();
    panel.set_rows(shell_rows());
    assert_eq!(panel.hosts_section_index(), Some(4));
    assert_eq!(panel.stored_area_index(), Some(2));
    let (oy, h) = tall();
    let h2 = panel.card_rect(oy, 5);
    panel.host_drag = Some(HostDrag {
        host_id: "h2".into(),
        host_name: "h2".into(),
        endpoint: String::new(),
        kind: HostDragKind::Host,
        row_index: 5,
        press_x: h2.x,
        press_y: h2.y,
        current_x: h2.x,
        current_y: h2.y,
        grab_dx: 0.0,
        grab_dy: 0.0,
        source_rect: h2,
        ghost_rect: h2,
        phase: HostDragPhase::Dragging,
        drop_target: None,
    });
    // Onto the grouped host: join the group.
    let prod = panel.card_rect(oy, 3);
    assert_eq!(
        panel.drop_target_at(oy, h, prod.x + 10.0, prod.y + 10.0),
        Some(HostDropTarget::Group("g1".into()))
    );
    // Onto the top of the group header: reorder before it.
    let g = panel.card_rect(oy, 2);
    assert_eq!(
        panel.drop_target_at(oy, h, g.x + 10.0, g.y + 2.0),
        Some(HostDropTarget::BeforeGroup("g1".into()))
    );
    // The local machines above are not a drop zone.
    let local = panel.card_rect(oy, 1);
    assert_eq!(
        panel.drop_target_at(oy, h, local.x + 10.0, local.y + 10.0),
        None
    );
}

#[test]
fn headers_and_rows_follow_the_shell_rhythm() {
    let mut panel = HostPanel::default();
    panel.set_rows(shell_rows());
    let oy = 0.0;
    let first = panel.card_rect(oy, 0);
    assert_eq!(first.y, crate::shell::sidebar::list_top());
    assert_eq!(first.height, SECTION_HEIGHT);
    let local = panel.card_rect(oy, 1);
    assert_eq!(local.y, first.bottom());
    assert_eq!((local.x, local.width, local.height), (12.0, 236.0, 44.0));
    let group = panel.card_rect(oy, 2);
    assert_eq!(group.y, local.bottom() + CARD_GAP + SECTION_GAP);
    // The New group action rides the Servers header.
    let action = panel.new_group_button_rect(oy, 900.0);
    let servers = panel.card_rect(oy, 4);
    assert_eq!(action.right(), servers.right());
    assert_eq!(action.y, servers.y);
}

#[test]
fn the_filter_field_only_takes_room_while_filtering() {
    let mut panel = HostPanel::default();
    panel.set_rows(shell_rows());
    let (oy, h) = tall();
    let y0 = panel.card_rect(oy, 0).y;
    assert_eq!(panel.search_rect(oy).width, 0.0);
    panel.filter_focused = true;
    let field = panel.search_rect(oy);
    assert!(field.width > 0.0);
    assert_eq!(panel.card_rect(oy, 0).y, y0 + FILTER_BAND_HEIGHT);
    assert_eq!(
        panel.hit_test(oy, h, field.x + 5.0, field.y + 5.0),
        Some(PanelHit::Search)
    );
    panel.filter_focused = false;
    panel.filter = TextDraft::new("prod");
    assert!(panel.filter_visible(), "a live filter stays visible");
    panel.escape_filter();
    assert!(!panel.filter_visible());
}

#[test]
fn rows_have_no_per_row_session_controls() {
    let mut panel = HostPanel::default();
    let mut rows = shell_rows();
    if let Row::Host(h) = &mut rows[5] {
        h.session_count = 2;
    }
    panel.set_rows(rows);
    assert!(panel.host_add_session_rect(0.0, 5).is_none());
    assert!(panel.host_chevron_rect(0.0, 5).is_none());
}

#[test]
fn filtering_hides_a_section_whose_own_rows_do_not_match() {
    let mut panel = HostPanel::default();
    panel.set_rows(shell_rows());
    panel.filter = TextDraft::new("prod");
    // "This computer" (local only) is hidden; the group with prod stays.
    assert_eq!(panel.visible_row_indices(), vec![2, 3]);
}

#[test]
fn a_row_cut_by_the_viewport_is_neither_painted_nor_hit() {
    let panel = panel(30);
    let (oy, h) = (0.0, SMALL_H);
    let body = panel.body_rect(oy, h);
    let cut = panel
        .visible_row_indices()
        .into_iter()
        .find(|&i| {
            let c = panel.card_rect(oy, i);
            c.y < body.bottom() && c.bottom() > body.bottom()
        })
        .expect("30 rows overflow a 630 px window");
    assert!(!panel.row_painted(oy, h, cut));
    let card = panel.card_rect(oy, cut);
    assert_eq!(
        panel.hit_test(oy, h, card.x + 10.0, card.y + 2.0),
        Some(PanelHit::Background),
        "the visible sliver of an unpainted row must not be clickable"
    );
    assert!(panel.row_painted(oy, h, 0));
}

#[test]
fn the_scroll_thumb_shows_only_when_the_list_overflows() {
    let (oy, h) = (0.0, SMALL_H);
    assert_eq!(panel(3).scroll_thumb(oy, h), None);

    let mut panel = panel(30);
    let body = panel.body_rect(oy, h);
    let top = panel.scroll_thumb(oy, h).expect("overflowing list");
    assert!((top.y - body.y).abs() < 0.01, "at the top when unscrolled");
    assert!(top.height < body.height && top.height >= SCROLL_THUMB_MIN);
    assert!(top.right() <= ORIGIN_X + WIDTH && top.x > ORIGIN_X + WIDTH - PAD_X);
    // Thumb share of the track = viewport share of the content.
    let share = body.height / panel.content_height();
    assert!((top.height - body.height * share).abs() < 0.5);

    panel.scroll = panel.max_scroll(oy, h);
    let bottom = panel.scroll_thumb(oy, h).unwrap();
    assert!((bottom.bottom() - body.bottom()).abs() < 0.01, "at the end");
}

#[test]
fn reveal_scrolls_a_hidden_row_fully_into_view_and_leaves_visible_ones() {
    let (oy, h) = (0.0, SMALL_H);
    let mut panel = panel(30);
    let body = panel.body_rect(oy, h);

    assert!(!panel.reveal_row(2, oy, h), "already visible: no scroll");
    assert_eq!(panel.scroll, 0.0);

    assert!(panel.reveal_row(25, oy, h));
    let card = panel.card_rect(oy, 25);
    assert!(panel.row_painted(oy, h, 25));
    assert!(
        (card.bottom() - body.bottom()).abs() < 0.01,
        "lands at the bottom"
    );

    assert!(panel.reveal_row(0, oy, h));
    assert_eq!(panel.scroll, 0.0);

    // The last row of all: never scrolls past the clamp.
    assert!(panel.reveal_row(29, oy, h));
    // (the slot's trailing card gap may stay below the fold)
    assert!(panel.max_scroll(oy, h) - panel.scroll <= CARD_GAP + 0.01);
    assert!(panel.row_painted(oy, h, 29));
}

#[test]
fn a_drag_near_the_list_edges_asks_for_auto_scroll() {
    let (oy, h) = (0.0, SMALL_H);
    let panel = panel(30);
    let body = panel.body_rect(oy, h);
    assert!(panel.drag_autoscroll_speed(oy, h, body.bottom() - 4.0) > 0.0);
    assert!(panel.drag_autoscroll_speed(oy, h, body.y + 4.0) < 0.0);
    assert_eq!(
        panel.drag_autoscroll_speed(oy, h, body.y + body.height / 2.0),
        0.0
    );
    // Nearer the edge scrolls faster.
    assert!(
        panel.drag_autoscroll_speed(oy, h, body.bottom() - 2.0)
            > panel.drag_autoscroll_speed(oy, h, body.bottom() - 20.0)
    );
    // Nothing to scroll: no auto-scroll.
    assert_eq!(
        panel_fit().drag_autoscroll_speed(oy, h, body.bottom() - 4.0),
        0.0
    );
}
