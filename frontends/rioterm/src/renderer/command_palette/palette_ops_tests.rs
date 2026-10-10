use super::*;

#[test]
fn scrollbar_hidden_until_first_scroll() {
    // Long list, palette just opened — no scroll event has happened,
    // so the fade timer is `None` and the scrollbar stays invisible
    // despite the list being taller than the visible window.
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode((0..50).map(|i| format!("Family {i:02}")).collect());
    assert!(palette.last_scroll_time.is_none());
}

#[test]
fn scrollbar_triggered_only_when_offset_actually_changes() {
    // The first few `move_selection_down` calls don't change
    // `scroll_offset` (selection walks within the visible window).
    // Only when selection crosses the window boundary does
    // `scroll_offset` bump, and only then does the scrollbar wake.
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode((0..50).map(|i| format!("Family {i:02}")).collect());
    for _ in 0..MAX_VISIBLE_RESULTS {
        palette.move_selection_down();
    }
    // At this point selection has just crossed into scroll territory.
    assert!(palette.last_scroll_time.is_some());
}

#[test]
fn scrollbar_timer_reset_on_query_change() {
    // Typing re-filters the list, which can shrink it below the
    // visible window. Any stale scrollbar timer must clear so a
    // leftover thumb doesn't linger over the new short list.
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode((0..50).map(|i| format!("Family {i:02}")).collect());
    for _ in 0..MAX_VISIBLE_RESULTS {
        palette.move_selection_down();
    }
    assert!(palette.last_scroll_time.is_some());
    palette.set_query("Family 00".to_string());
    assert!(palette.last_scroll_time.is_none());
}

#[test]
fn scrollbar_timer_reset_on_palette_reopen() {
    // Closing and re-opening the palette must drop any lingering
    // scrollbar state so the user doesn't see a fading thumb on a
    // fresh palette.
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode((0..50).map(|i| format!("Family {i:02}")).collect());
    for _ in 0..MAX_VISIBLE_RESULTS {
        palette.move_selection_down();
    }
    palette.set_enabled(false);
    palette.set_enabled(true);
    assert!(palette.last_scroll_time.is_none());
}

#[test]
fn list_fonts_command_is_present_and_actionable() {
    // Confirms `List Fonts` shows up in the command list and
    // reports the correct action when selected.
    let mut palette = CommandPalette::new();
    palette.set_query("list fonts".to_string());
    let filtered = palette.filtered_rows();
    assert!(!filtered.is_empty());
    assert_eq!(filtered[0].1.title(), "List Fonts");
    palette.selected_index = 0;
    assert_eq!(
        palette.get_selected_action(),
        Some(PaletteAction::ListFonts)
    );
}

#[test]
fn sftp_action_listed() {
    let mut palette = CommandPalette::new();
    palette.set_query("sftp".to_string());
    let filtered = palette.filtered_rows();
    let found = filtered.iter().any(|(_, row)| {
        matches!(row.action(), Some(PaletteAction::OpenSftp))
            || row.title().to_lowercase().contains("sftp")
    });
    assert!(found, "SFTP: Palette >sftp — not implemented yet");
}

#[test]
fn shortcuts_shown_are_the_live_bindings() {
    let mut palette = CommandPalette::new();
    palette.set_shortcuts(vec![(PaletteAction::TabCreate, "Ctrl+Shift+T".into())]);
    palette.set_query("new tab".to_string());
    let rows = palette.filtered_rows();
    assert_eq!(rows[0].1.title(), "New Tab");
    assert_eq!(rows[0].1.shortcut(), "Ctrl+Shift+T");
    palette.set_query("split right".to_string());
    assert_eq!(
        palette.filtered_rows()[0].1.shortcut(),
        "",
        "unbound: no guess"
    );
}

#[test]
fn commands_map_to_their_key_binding_actions() {
    use crate::bindings::Action;
    assert_eq!(
        PaletteAction::TabCreate.binding_action(),
        Some(Action::TabCreateNew)
    );
    assert_eq!(PaletteAction::Copy.binding_action(), Some(Action::Copy));
    assert_eq!(
        PaletteAction::CloseCurrentSplitOrTab.binding_action(),
        Some(Action::CloseCurrentSplitOrTab)
    );
    assert_eq!(PaletteAction::ListHosts.binding_action(), None);
}

#[test]
fn update_commands_are_listed() {
    for (query, action) in [
        ("check for updates", PaletteAction::CheckForUpdates),
        ("install update", PaletteAction::InstallUpdate),
        ("restart to update", PaletteAction::RestartToUpdate),
    ] {
        let mut palette = CommandPalette::new();
        palette.set_query(query.to_string());
        assert_eq!(palette.get_selected_action(), Some(action), "{query}");
    }
}

fn host(id: &str, title: &str) -> HostPaletteItem {
    HostPaletteItem {
        id: id.into(),
        title: title.into(),
        subtitle: format!("root@{title}"),
    }
}

#[test]
fn servers_list_after_commands_so_each_group_is_contiguous() {
    let mut palette = CommandPalette::new();
    palette.set_hosts(vec![host("1", "tab-server")]);
    palette.query = terminus_ui::TextDraft::new("tab");
    let specs = palette.row_specs();
    let first_server = specs.iter().position(|r| r.group == "Servers").unwrap();
    assert!(first_server > 0);
    assert!(specs[..first_server].iter().all(|r| r.group == "Commands"));
    assert!(specs[first_server..].iter().all(|r| r.group == "Servers"));
    assert_eq!(specs[first_server].hint, "root@tab-server");
}

#[test]
fn view_windows_the_rows_and_names_the_placeholder() {
    let mut palette = CommandPalette::new();
    palette.set_enabled(true);
    for _ in 0..(MAX_VISIBLE_RESULTS + 2) {
        palette.move_selection_down();
    }
    let view = palette.view();
    assert_eq!(view.item_count(), MAX_VISIBLE_RESULTS);
    assert_eq!(view.selected, MAX_VISIBLE_RESULTS - 1);
    assert_eq!(
        view.placeholder.as_deref(),
        Some("Search servers and commands")
    );
}

#[test]
fn nothing_matching_offers_add_server_with_the_query() {
    let mut palette = CommandPalette::new();
    assert_eq!(
        palette.add_server_query(),
        None,
        "empty query never offers it"
    );
    palette.set_query("zzzqqq".to_string());
    assert_eq!(palette.add_server_query().as_deref(), Some("zzzqqq"));
    palette.set_query("quit".to_string());
    assert_eq!(palette.add_server_query(), None, "there are matches");
}

fn tunnel(id: &str, name: &str, active: bool) -> TunnelPaletteItem {
    TunnelPaletteItem {
        id: id.into(),
        name: name.into(),
        hint: format!("route-{name}"),
        active,
    }
}

fn with_tunnels(query: &str) -> CommandPalette {
    let mut palette = CommandPalette::new();
    palette.set_hosts(vec![host("h1", "prod"), host("h2", "staging")]);
    palette.set_tunnels(vec![
        tunnel("t1", "db", true),
        tunnel("t2", "web", false),
        tunnel("t3", "docs", false),
    ]);
    palette.set_query(query.to_string());
    palette
}

fn titles(palette: &CommandPalette) -> Vec<String> {
    palette
        .filtered_rows()
        .iter()
        .map(|(_, r)| r.title().to_string())
        .collect()
}

#[test]
fn sftp_op_lists_only_hosts_and_confirms_as_sftp() {
    let palette = with_tunnels(">sftp");
    assert_eq!(titles(&palette), ["prod", "staging"]);
    assert_eq!(palette.host_pick(), HostPick::Sftp);
    assert_eq!(palette.get_selected_host_id().as_deref(), Some("h1"));
    assert_eq!(palette.get_selected_action(), None);
}

#[test]
fn sftp_op_filters_hosts_by_its_argument() {
    let palette = with_tunnels(">sftp stag");
    assert_eq!(titles(&palette), ["staging"]);
    assert_eq!(palette.get_selected_host_id().as_deref(), Some("h2"));
}

#[test]
fn plain_query_still_confirms_hosts_as_sessions() {
    let palette = with_tunnels("prod");
    assert_eq!(palette.host_pick(), HostPick::Session);
}

#[test]
fn forward_list_shows_every_tunnel_and_jumps_to_the_view() {
    let palette = with_tunnels(">forward");
    assert_eq!(titles(&palette), ["db", "web", "docs"]);
    assert_eq!(
        palette.get_selected_tunnel(),
        Some(("t1".to_string(), TunnelOp::Show))
    );
    assert_eq!(palette.row_specs()[0].hint, "route-db");
    assert_eq!(palette.row_specs()[0].group, "Tunnels");
}

#[test]
fn forward_start_offers_only_stopped_tunnels() {
    let mut palette = with_tunnels(">forward start");
    assert_eq!(titles(&palette), ["web", "docs"]);
    assert_eq!(
        palette.get_selected_tunnel(),
        Some(("t2".to_string(), TunnelOp::Start))
    );
    palette.set_query(">forward start do".to_string());
    assert_eq!(titles(&palette), ["docs"]);
    assert_eq!(
        palette.get_selected_tunnel(),
        Some(("t3".to_string(), TunnelOp::Start))
    );
}

#[test]
fn forward_stop_offers_only_running_tunnels() {
    let palette = with_tunnels(">forward stop");
    assert_eq!(titles(&palette), ["db"]);
    assert_eq!(
        palette.get_selected_tunnel(),
        Some(("t1".to_string(), TunnelOp::Stop))
    );
    let none = with_tunnels(">forward stop web");
    assert!(none.filtered_rows().is_empty(), "web is not running");
    assert_eq!(none.get_selected_tunnel(), None);
}

#[test]
fn op_queries_never_offer_add_server() {
    for query in [">forward start nope", ">sftp nope", ">foo", ">forward star"] {
        let palette = with_tunnels(query);
        assert_eq!(palette.add_server_query(), None, "{query}");
    }
    assert_eq!(
        with_tunnels("zzzqqq").add_server_query().as_deref(),
        Some("zzzqqq")
    );
}

#[test]
fn op_queries_do_not_mix_in_commands_or_inline_hosts() {
    let palette = with_tunnels(">forward");
    assert!(!palette.filtered_rows().is_empty());
    assert!(palette
        .filtered_rows()
        .iter()
        .all(|(_, r)| matches!(r, PaletteRow::Tunnel { .. })));
    let hints = with_tunnels(">");
    assert!(!hints.filtered_rows().is_empty());
    assert!(hints
        .filtered_rows()
        .iter()
        .all(|(_, r)| matches!(r, PaletteRow::OpHint { .. })));
    assert!(with_tunnels(">foo").filtered_rows().is_empty());
}

#[test]
fn a_partial_op_offers_completions_that_fill_the_query() {
    let mut palette = with_tunnels(">");
    assert_eq!(palette.filtered_rows().len(), 4);
    assert_eq!(palette.get_selected_action(), None);
    palette.set_query(">sf".to_string());
    assert!(palette.complete_selected_op());
    assert_eq!(palette.query.value, ">sftp ");
    assert!(!palette.complete_selected_op(), "now a real op, not a hint");
    palette.set_query(">forward st".to_string());
    palette.selected_index = 1;
    assert!(palette.complete_selected_op());
    assert_eq!(palette.query.value, ">forward stop ");
}

#[test]
fn chevron_is_literal_in_the_fonts_and_hosts_lists() {
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode(vec!["A>B".into(), "Other".into()]);
    palette.set_query(">".to_string());
    assert_eq!(titles(&palette), ["A>B"]);

    let mut palette = CommandPalette::new();
    palette.enter_hosts_mode(vec![host("h", "prod")]);
    palette.set_query(">sftp".to_string());
    assert_eq!(palette.host_pick(), HostPick::Session);
    assert!(palette.filtered_rows().is_empty());
}

#[test]
fn start_and_stop_tunnel_entries_prefill_the_query() {
    let mut palette = CommandPalette::new();
    palette.set_query("tunnel".to_string());
    let prefills: Vec<&str> = palette
        .filtered_rows()
        .iter()
        .filter_map(|(_, r)| match r.action() {
            Some(PaletteAction::Prefill(text)) => Some(text),
            _ => None,
        })
        .collect();
    assert!(prefills.contains(&">forward start "));
    assert!(prefills.contains(&">forward stop "));
    for text in prefills {
        assert!(matches!(
            terminus_ui::palette_query::parse(text),
            terminus_ui::palette_query::Query::Op(_)
        ));
    }
}

#[test]
fn font_rows_hint_copy() {
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode(vec!["Fira Code".into()]);
    let specs = palette.row_specs();
    assert_eq!((specs[0].group, specs[0].hint.as_str()), ("Fonts", "Copy"));
}
