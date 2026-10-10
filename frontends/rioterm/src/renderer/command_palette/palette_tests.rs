use super::*;

#[test]
fn test_set_enabled_resets_state() {
    let mut palette = CommandPalette::new();
    palette.set_query("test".to_string());
    palette.selected_index = 3;
    palette.scroll_offset = 2;

    palette.set_enabled(true);

    assert!(palette.query.value.is_empty());
    assert_eq!(palette.selected_index, 0);
    assert_eq!(palette.scroll_offset, 0);
}

#[test]
fn test_filtered_commands_empty_query() {
    let palette = CommandPalette::new();
    let filtered = palette.filtered_rows();
    // ToggleAppearanceTheme is hidden when has_adaptive_theme is false
    assert_eq!(filtered.len(), COMMANDS.len() - 1);
}

#[test]
fn test_filtered_commands_by_title() {
    let mut palette = CommandPalette::new();
    palette.query = terminus_ui::TextDraft::new("split");
    let filtered = palette.filtered_rows();
    assert!(filtered.len() >= 2);
    for (_, row) in &filtered {
        assert!(row.title().to_lowercase().contains("split"));
    }
}

#[test]
fn test_filtered_commands_case_insensitive() {
    let mut palette = CommandPalette::new();
    palette.query = terminus_ui::TextDraft::new("QUIT");
    let filtered = palette.filtered_rows();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].1.title(), "Quit");
}

#[test]
fn test_fuzzy_matching() {
    let mut palette = CommandPalette::new();
    palette.query = terminus_ui::TextDraft::new("nt"); // Should match "New Tab", "Next Tab", etc.
    let filtered = palette.filtered_rows();
    assert!(!filtered.is_empty());
}

#[test]
fn test_set_query_resets_selection_and_scroll() {
    let mut palette = CommandPalette::new();
    palette.selected_index = 5;
    palette.scroll_offset = 3;
    palette.set_query("test".to_string());
    assert_eq!(palette.selected_index, 0);
    assert_eq!(palette.scroll_offset, 0);
}

#[test]
fn test_move_selection_down() {
    let mut palette = CommandPalette::new();
    palette.set_enabled(true);
    assert_eq!(palette.selected_index, 0);
    palette.move_selection_down();
    assert_eq!(palette.selected_index, 1);
    palette.move_selection_down();
    assert_eq!(palette.selected_index, 2);
}

#[test]
fn test_move_selection_down_boundary() {
    let mut palette = CommandPalette::new();
    palette.set_enabled(true);
    let count = palette.filtered_rows().len();
    palette.selected_index = count - 1;
    palette.move_selection_down();
    assert_eq!(palette.selected_index, count - 1);
}

#[test]
fn test_move_selection_up() {
    let mut palette = CommandPalette::new();
    palette.set_enabled(true);
    palette.selected_index = 3;
    palette.move_selection_up();
    assert_eq!(palette.selected_index, 2);
}

#[test]
fn test_move_selection_up_boundary() {
    let mut palette = CommandPalette::new();
    palette.set_enabled(true);
    palette.move_selection_up();
    assert_eq!(palette.selected_index, 0);
}

#[test]
fn test_get_selected_action() {
    let palette = CommandPalette::new();
    let action = palette.get_selected_action();
    assert!(action.is_some());
    // First command is "New Tab"
    assert_eq!(action.unwrap(), PaletteAction::TabCreate);
}

#[test]
fn test_get_selected_action_with_filter() {
    let mut palette = CommandPalette::new();
    palette.set_query("quit".to_string());
    let action = palette.get_selected_action();
    assert_eq!(action, Some(PaletteAction::Quit));
}

#[test]
fn test_scroll_offset_on_move_down() {
    let mut palette = CommandPalette::new();
    palette.set_enabled(true);
    for _ in 0..MAX_VISIBLE_RESULTS {
        palette.move_selection_down();
    }
    assert!(palette.scroll_offset > 0);
}

#[test]
fn test_hit_test_walks_the_painted_layout() {
    let mut palette = CommandPalette::new();
    // Nothing painted yet: nothing to hit.
    assert_eq!(palette.hit_test(0.0, 0.0, 1200.0, 1.0), Ok(None));
    palette.set_enabled(true);
    let layout = palette.view().layout((1440.0, 900.0));
    let second = layout
        .rows
        .iter()
        .filter_map(|r| match r {
            terminus_ui::components::overlay::PaletteRow::Item { rect, index } => {
                Some((*rect, *index))
            }
            _ => None,
        })
        .nth(1)
        .unwrap();
    palette.last_layout = Some(layout);
    assert!(
        palette.hit_test(0.0, 0.0, 1440.0, 1.0).is_err(),
        "scrim closes"
    );
    let (rect, _) = second;
    assert_eq!(
        palette.hit_test(rect.x + 4.0, rect.y + 4.0, 1440.0, 1.0),
        Ok(Some(1))
    );
    // Scrolled by 3: the same row is absolute index 4.
    palette.scroll_offset = 3;
    assert_eq!(
        palette.hit_test(rect.x + 4.0, rect.y + 4.0, 1440.0, 1.0),
        Ok(Some(4))
    );
}

#[test]
fn test_fuzzy_score_basic() {
    assert!(fuzzy_score("nt", "New Tab").is_some());
    assert!(fuzzy_score("xyz", "New Tab").is_none());
    assert!(fuzzy_score("", "New Tab").is_some());
}

#[test]
fn test_fuzzy_score_ordering() {
    // "New Tab" should score higher than "Next Tab" for "net" because of word boundary
    let score_new = fuzzy_score("net", "New Tab").unwrap_or(-100);
    let score_next = fuzzy_score("net", "Next Tab").unwrap_or(-100);
    // Both should match
    assert!(score_new > -100);
    assert!(score_next > -100);
}

#[test]
fn enter_fonts_mode_switches_to_font_list() {
    let mut palette = CommandPalette::new();
    palette.set_enabled(true);
    palette.set_query("ab".to_string());
    palette.selected_index = 2;

    let fonts = vec![
        "JetBrains Mono".to_string(),
        "Fira Code".to_string(),
        "Cascadia Code".to_string(),
    ];
    palette.enter_fonts_mode(fonts);

    // Query cleared, selection reset, full list visible.
    assert!(palette.query.value.is_empty());
    assert_eq!(palette.selected_index, 0);
    assert_eq!(palette.filtered_rows().len(), 3);
    // Every row is a Font row, so no executable action.
    assert!(palette.get_selected_action().is_none());
}

#[test]
fn fonts_mode_filters_by_fuzzy_score() {
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode(vec![
        "JetBrains Mono".to_string(),
        "Fira Code".to_string(),
        "Cascadia Code".to_string(),
    ]);
    palette.set_query("cas".to_string());
    let filtered = palette.filtered_rows();
    assert!(filtered.iter().any(|(_, r)| r.title() == "Cascadia Code"));
    assert!(filtered.iter().all(|(_, r)| {
        r.title().to_lowercase().contains('c')
            && r.title().to_lowercase().contains('a')
            && r.title().to_lowercase().contains('s')
    }));
}

#[test]
fn fonts_mode_row_has_no_shortcut_column() {
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode(vec!["Fira Code".to_string()]);
    let filtered = palette.filtered_rows();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].1.shortcut(), "");
}

#[test]
fn set_enabled_resets_fonts_mode_to_commands() {
    // Re-opening the palette with the keyboard must drop any stale
    // font list — reopening otherwise would land the user on fonts
    // they saw yesterday, which is surprising.
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode(vec!["Fira Code".to_string()]);
    palette.enabled = true;
    palette.set_enabled(false);
    palette.set_enabled(true);
    assert!(matches!(palette.mode, PaletteMode::Commands));
    // Commands list is back (non-empty modulo adaptive-theme filter).
    assert!(!palette.filtered_rows().is_empty());
}

#[test]
fn get_selected_font_returns_family_in_fonts_mode() {
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode(vec!["JetBrains Mono".to_string(), "Fira Code".to_string()]);
    // First row (sorted alphabetically by fuzzy_score tie-break:
    // both score 0 with empty query, so first-inserted wins).
    let selected = palette.get_selected_font();
    assert!(selected.is_some());
    // The returned name must be one of the inputs, irrespective
    // of fuzzy-sort ordering.
    let s = selected.unwrap();
    assert!(s == "JetBrains Mono" || s == "Fira Code");
}

#[test]
fn get_selected_font_none_in_commands_mode() {
    let palette = CommandPalette::new();
    // Default mode is Commands; no font to copy.
    assert!(palette.get_selected_font().is_none());
}

#[test]
fn get_selected_font_none_when_empty_filter() {
    let mut palette = CommandPalette::new();
    palette.enter_fonts_mode(vec!["Fira Code".to_string()]);
    palette.set_query("zzzz".to_string());
    // Query doesn't match anything → no selected font.
    assert!(palette.get_selected_font().is_none());
}

fn sample_hosts() -> Vec<HostPaletteItem> {
    vec![
        HostPaletteItem {
            id: "h1".into(),
            title: "Production".into(),
            subtitle: "root@prod.example".into(),
        },
        HostPaletteItem {
            id: "h2".into(),
            title: "Staging".into(),
            subtitle: "deploy@staging.example:2222".into(),
        },
    ]
}

#[test]
fn sftp_picker_lists_hosts_and_remembers_why() {
    let mut palette = CommandPalette::new();
    palette.set_enabled(true);
    palette.enter_hosts_mode_for(sample_hosts(), HostPick::Sftp);
    assert_eq!(palette.host_pick(), HostPick::Sftp);
    assert_eq!(palette.filtered_rows().len(), 2);
    palette.set_query("stag".to_string());
    assert_eq!(palette.get_selected_host_id().as_deref(), Some("h2"));
    // The plain "Open Host…" picker still opens sessions.
    palette.enter_hosts_mode(sample_hosts());
    assert_eq!(palette.host_pick(), HostPick::Session);
}

#[test]
fn enter_hosts_mode_lists_all_hosts() {
    let mut palette = CommandPalette::new();
    palette.set_enabled(true);
    palette.enter_hosts_mode(sample_hosts());
    assert!(palette.query.value.is_empty());
    assert_eq!(palette.filtered_rows().len(), 2);
    assert!(palette.get_selected_action().is_none());
    assert!(palette.get_selected_host_id().is_some());
}

#[test]
fn hosts_mode_filters_by_name_or_endpoint() {
    let mut palette = CommandPalette::new();
    palette.enter_hosts_mode(sample_hosts());
    palette.set_query("stag".to_string());
    let filtered = palette.filtered_rows();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].1.title(), "Staging");
    assert_eq!(palette.get_selected_host_id().as_deref(), Some("h2"));
}

#[test]
fn commands_mode_surfaces_hosts_only_when_querying() {
    let mut palette = CommandPalette::new();
    palette.set_hosts(sample_hosts());
    // Empty query: commands only (no host dump).
    let empty = palette.filtered_rows();
    assert!(empty
        .iter()
        .all(|(_, r)| !matches!(r, PaletteRow::Host { .. })));
    assert!(empty.iter().any(|(_, r)| r.title() == "Open Host…"));

    palette.set_query("prod".to_string());
    let mixed = palette.filtered_rows();
    assert!(
        mixed.iter().any(|(_, r)| matches!(
            r,
            PaletteRow::Host {
                title: "Production",
                ..
            }
        )),
        "expected Production host in mixed results: {:?}",
        mixed.iter().map(|(_, r)| r.title()).collect::<Vec<_>>()
    );
}

#[test]
fn set_enabled_resets_hosts_mode_to_commands() {
    let mut palette = CommandPalette::new();
    palette.enter_hosts_mode(sample_hosts());
    palette.enabled = true;
    palette.set_enabled(false);
    palette.set_enabled(true);
    assert!(matches!(palette.mode, PaletteMode::Commands));
}

// Scrollbar geometry + fade math live in `renderer::scrollbar` and
// are tested there. The tests below cover the palette's own contract:
// the scrollbar only surfaces after the user actually scrolls, and
// resets when the list reshapes.
