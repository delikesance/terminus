use super::test_support::*;
use super::*;
use crate::os_icons::HostStatus;

#[test]
fn an_empty_list_says_how_to_add_a_host() {
    let empty = panel(0);
    let hint = empty.empty_hint().expect("hint");
    assert_eq!(hint.title, "No saved hosts yet");
    assert!(hint.body.contains("Add server"), "{}", hint.body);
    assert!(panel(2).empty_hint().is_none());
}

#[test]
fn hosts_in_a_collapsed_group_still_count() {
    // Collapsed groups carry their hosts in `host_count`, not as rows.
    let mut panel = HostPanel::default();
    panel.set_rows(vec![
        Row::Section("Hosts".into()),
        Row::Group {
            id: "g1".into(),
            name: "prod".into(),
            host_count: 3,
            session_count: 0,
            collapsed: true,
        },
    ]);
    assert_eq!(panel.host_count(), 3);
    assert_eq!(panel.count_label(), "3 hosts");
    assert!(panel.empty_hint().is_none(), "not 'No saved hosts yet'");
    // Filtering on the group's name shows the group: not "No matches".
    panel.filter = TextDraft::new("prod");
    assert!(panel.empty_hint().is_none());
}

#[test]
fn a_filter_with_no_match_says_so_and_how_to_clear_it() {
    let mut panel = panel(2);
    panel.filter = TextDraft::new("zzz");
    let hint = panel.empty_hint().expect("hint");
    assert_eq!(hint.title, "No matches");
    assert!(
        hint.body.contains("zzz") && hint.body.contains("Esc"),
        "{}",
        hint.body
    );
    panel.filter = TextDraft::new("host-1");
    assert!(panel.empty_hint().is_none(), "a match hides the hint");
}

#[test]
fn the_hint_sits_below_the_list() {
    let (oy, h) = tall();
    let panel = panel(0);
    let rect = panel.empty_hint_rect(oy, h).expect("rect");
    let body = panel.body_rect(oy, h);
    assert!(rect.y >= body.y + panel.content_height() - 0.01);
    assert!(rect.x >= body.x && rect.right() <= body.right() + 0.01);
}

#[test]
fn escape_clears_the_filter_before_leaving_it() {
    let mut panel = panel(2);
    panel.filter = TextDraft::new("zzz");
    panel.filter_focused = true;
    panel.escape_filter();
    assert_eq!(panel.filter.value, "");
    assert!(panel.filter_focused, "first Esc only clears");
    panel.escape_filter();
    assert!(!panel.filter_focused);
}

#[test]
fn a_saved_host_is_found_by_name_and_endpoint() {
    let mut hosts = items(2);
    // Same endpoint, different names (e.g. one per key).
    hosts[1].endpoint = hosts[0].endpoint.clone();
    let panel = {
        let mut p = HostPanel::default();
        p.set_items(hosts);
        p
    };
    let second = panel
        .find_saved_host("host-1", "deploy@host-0:2222")
        .expect("row");
    assert_eq!(panel.rows[second].host().unwrap().id, "id-1");
    let first = panel
        .find_saved_host("renamed", "deploy@host-0:2222")
        .expect("row");
    assert_eq!(
        panel.rows[first].host().unwrap().id,
        "id-0",
        "endpoint fallback"
    );
    assert_eq!(panel.find_saved_host("x", "nobody@nowhere"), None);
}

#[test]
fn renaming_starts_with_the_old_name_selected() {
    let mut panel = panel(1);
    panel.begin_rename("id-0".into(), false, "old-name");
    let draft = panel.rename.as_mut().unwrap();
    assert_eq!(draft.text.selection_range(), Some((0, 8)));
    draft.text.insert("web", 64, false);
    assert_eq!(draft.text.value, "web", "typing replaces the old name");
}

#[test]
fn count_label_is_pluralised() {
    let mut panel = panel(0);
    assert_eq!(panel.count_label(), "empty");
    panel.set_items(items(1));
    assert_eq!(panel.count_label(), "1 host");
    panel.set_items(items(7));
    assert_eq!(panel.count_label(), "7 hosts");
}

#[test]
fn sections_take_their_own_height_and_are_not_targets() {
    let mut panel = HostPanel::default();
    panel.set_rows(grouped());
    let (oy, h) = tall();

    // First header drops its top gap.
    assert_eq!(
        panel.content_height(),
        SECTION_HEIGHT + (SECTION_GAP + SECTION_HEIGHT) + 4.0 * (ITEM_HEIGHT + CARD_GAP)
    );
    // Local + two platform hosts + Hosts + two stored hosts.
    assert_eq!(panel.rows.len(), 6);
    assert_eq!(panel.host_count(), 2);
    assert_eq!(panel.count_label(), "2 hosts");

    // Rows stack in order: Local, local, wsl, Hosts, then stored hosts.
    assert_eq!(panel.item_rect(oy, 1).y, panel.item_rect(oy, 0).bottom());
    assert_eq!(
        panel.item_rect(oy, 0).height,
        SECTION_HEIGHT,
        "first header"
    );
    assert_eq!(
        panel.item_rect(oy, 3).height,
        SECTION_GAP + SECTION_HEIGHT,
        "later headers carry the gap above them"
    );
    assert_eq!(
        panel.item_rect(oy, 4).y,
        panel.item_rect(oy, 3).bottom(),
        "the first stored host sits right under the Hosts label"
    );

    // A press on a label (left side) is background, not a host.
    let label = panel.card_rect(oy, 3);
    assert_eq!(
        panel.hit_test(oy, h, ORIGIN_X + PAD_X + 10.0, label.y + label.height / 2.0),
        Some(PanelHit::Background)
    );
    // The New group control sits on the Hosts header's trailing edge.
    let new_group = panel.new_group_button_rect(oy, h);
    assert_eq!(
        panel.hit_test(oy, h, new_group.x + 4.0, new_group.y + 4.0),
        Some(PanelHit::NewGroup)
    );
}

#[test]
fn collapsing_a_group_hides_only_its_nested_hosts() {
    let mut panel = HostPanel::default();
    panel.set_rows(vec![
        Row::Section("Hosts".to_string()),
        Row::Group {
            id: "g1".to_string(),
            name: "jeremy".to_string(),
            host_count: 1,
            session_count: 0,
            collapsed: true,
        },
        // Ungrouped host that follows the collapsed group — must stay.
        Row::Host(HostItem {
            id: "solo".to_string(),
            name: "solo".to_string(),
            endpoint: "root@solo".to_string(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Idle,
            nested: false,
            session_count: 0,
        }),
    ]);
    let visible = panel.visible_row_indices();
    assert_eq!(visible, vec![0, 1, 2], "ungrouped hosts survive a collapse");

    panel.set_rows(vec![
        Row::Section("Hosts".to_string()),
        Row::Group {
            id: "g1".to_string(),
            name: "jeremy".to_string(),
            host_count: 1,
            session_count: 0,
            collapsed: false,
        },
        Row::Host(HostItem {
            id: "nested".to_string(),
            name: "nested".to_string(),
            endpoint: "root@n".to_string(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Idle,
            nested: true,
            session_count: 0,
        }),
        Row::Host(HostItem {
            id: "solo".to_string(),
            name: "solo".to_string(),
            endpoint: "root@solo".to_string(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Idle,
            nested: false,
            session_count: 0,
        }),
    ]);
    assert_eq!(panel.visible_row_indices(), vec![0, 1, 2, 3]);
    panel.collapsed_groups.insert("g1".to_string());
    assert_eq!(
        panel.visible_row_indices(),
        vec![0, 1, 3],
        "only the nested child disappears"
    );
}

#[test]
fn group_tray_spans_header_and_nested_hosts() {
    let mut panel = HostPanel::default();
    panel.set_rows(vec![
        Row::Group {
            id: "g1".to_string(),
            name: "jeremy".to_string(),
            host_count: 1,
            session_count: 0,
            collapsed: false,
        },
        Row::Host(HostItem {
            id: "nested".to_string(),
            name: "jerem prod".to_string(),
            endpoint: "ubuntu@1".to_string(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Idle,
            nested: true,
            session_count: 0,
        }),
        Row::Host(HostItem {
            id: "solo".to_string(),
            name: "solo".to_string(),
            endpoint: "root@solo".to_string(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: HostStatus::Idle,
            nested: false,
            session_count: 0,
        }),
    ]);
    let (oy, _) = tall();
    let tray = panel.group_tray_rect(oy, 0).expect("tray");
    let header = panel.card_rect(oy, 0);
    let nested = panel.card_rect(oy, 1);
    let solo = panel.card_rect(oy, 2);
    assert!((tray.y - header.y).abs() < 0.5);
    assert!(tray.bottom() >= nested.bottom() - 0.5);
    assert!(
        tray.bottom() < solo.y,
        "tray must not swallow the next ungrouped host"
    );
    // Grouped hosts line up with every other row (no indent).
    assert_eq!(nested.x, tray.x);
    assert_eq!(nested.right(), tray.right());
    assert_eq!(panel.group_nested_host_indices(0), vec![1]);

    // Tray bottom + CARD_GAP must land on the next root card (same as
    // collapsed→collapsed spacing).
    let gap_after_open = solo.y - tray.bottom();
    assert!(
        (gap_after_open - CARD_GAP).abs() < 0.5,
        "open-group→next gap={gap_after_open}, want CARD_GAP={CARD_GAP}"
    );

    panel.collapsed_groups.insert("g1".to_string());
    let collapsed = panel.group_tray_rect(oy, 0).expect("collapsed tray");
    assert!((collapsed.height - header.height).abs() < 1.0);
    let solo_after = panel.card_rect(oy, 2);
    // solo is still index 2 in rows but may not be visible... wait when
    // collapsed, nested is hidden so solo is still at index 2 in rows,
    // visible indices change offset. card_rect uses offset_of which uses
    // visible rows — solo should move up.
    let gap_after_closed = solo_after.y - collapsed.bottom();
    // A collapsed group is just its header label; the next row
    // follows straight under it, like rows under a section label.
    assert!(
        gap_after_closed.abs() < 0.5,
        "closed-group→next gap={gap_after_closed}"
    );
}

#[test]
fn selection_survives_a_regrouping() {
    let mut panel = HostPanel::default();
    panel.set_rows(grouped());
    assert!(panel.select_id("wsl:Ubuntu-24.04"));
    assert_eq!(panel.selected, Some(2));
    assert_eq!(
        panel.selected_item().map(|item| item.name.as_str()),
        Some("Ubuntu 24.04 LTS")
    );

    // Same hosts, no sections any more.
    panel.set_rows(vec![Row::Host(HostItem {
        id: "wsl:Ubuntu-24.04".to_string(),
        name: "Ubuntu 24.04 LTS".to_string(),
        endpoint: "windows".to_string(),
        badge: Badge::Wsl,
        stored: false,
        os_id: None,
        status: HostStatus::Running,
        nested: false,
        session_count: 0,
    })]);
    assert_eq!(panel.selected, Some(0));

    // A host that is gone is not left selected.
    panel.set_rows(vec![]);
    assert_eq!(panel.selected, None);
}

#[test]
fn new_group_form_expands_content_and_accepts_hits() {
    let mut panel = HostPanel::default();
    panel.set_rows(grouped());
    let (oy, h) = tall();
    let before = panel.content_height();
    panel.open_new_group_form();
    assert!(panel.new_group_drafting);
    assert!(panel.new_group_focused);
    assert!(panel.content_height() > before);

    let field = panel.new_group_field_rect(oy).expect("field");
    assert_eq!(
        panel.hit_test(oy, h, field.x + 4.0, field.y + 4.0),
        Some(PanelHit::NewGroupField)
    );
    let create = panel.new_group_create_rect(oy).expect("create");
    assert_eq!(
        panel.hit_test(oy, h, create.x + 4.0, create.y + 4.0),
        Some(PanelHit::NewGroupCreate)
    );
    panel.close_new_group_form();
    assert!(!panel.new_group_drafting);
    assert_eq!(panel.content_height(), before);
}
