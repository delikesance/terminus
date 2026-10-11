use super::*;

fn row(name: &str) -> SftpRow {
    SftpRow {
        name: name.into(),
        path: format!("/{name}"),
        is_dir: false,
        size: 1,
        modified: None,
    }
}

fn side() -> SftpSideState {
    let mut side = SftpSideState::local("/");
    side.set_listed(
        "/".into(),
        vec![row("Readme.md"), row("main.rs"), row("notes.txt")],
    );
    side
}

fn names(side: &SftpSideState) -> Vec<&str> {
    side.entries.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn typing_filters_by_case_insensitive_substring() {
    let mut side = side();
    side.type_filter("EA");
    assert_eq!(names(&side), ["Readme.md"]);
    side.type_filter("d");
    assert_eq!(names(&side), ["Readme.md"]);
}

#[test]
fn backspace_widens_the_filter() {
    let mut side = side();
    side.type_filter("no");
    assert_eq!(names(&side), ["notes.txt"]);
    side.filter_backspace();
    side.filter_backspace();
    assert_eq!(side.entries.len(), 3);
}

#[test]
fn clear_filter_restores_the_listing_and_unfocuses() {
    let mut side = side();
    side.filter_focused = true;
    side.type_filter("main");
    assert!(side.clear_filter());
    assert_eq!(side.entries.len(), 3);
    assert!(!side.filter_focused);
    assert!(!side.clear_filter());
}

#[test]
fn a_new_listing_resets_the_filter() {
    let mut side = side();
    side.type_filter("main");
    side.set_listed("/x".into(), vec![row("a"), row("b")]);
    assert_eq!(side.entries.len(), 2);
    assert!(side.filter.value.is_empty());
}

#[test]
fn refreshing_the_same_directory_keeps_the_filter() {
    let mut side = side();
    side.filter_focused = true;
    side.type_filter("no");
    side.set_listed(
        "/".into(),
        vec![row("notes.txt"), row("nope.md"), row("main.rs")],
    );
    assert_eq!(names(&side), ["notes.txt", "nope.md"]);
    assert_eq!(side.filter.value, "no");
    assert!(side.filter_focused);
}

#[test]
fn escape_clears_the_filter_only_while_it_owns_keys() {
    let mut st = SftpPaneState::new_local_local("/a", "/b");
    st.focus_filter(SftpFocus::Left);
    st.left.type_filter("x");
    st.blur_filters();
    assert!(!st.escape_filter());
    assert_eq!(st.left.filter.value, "x");
    st.focus_filter(SftpFocus::Left);
    assert!(st.escape_filter());
    assert!(st.left.filter.value.is_empty());
}

#[test]
fn filtering_resets_selection_and_scroll() {
    let mut side = side();
    side.selected = Some(2);
    side.scroll = 60.0;
    side.type_filter("r");
    assert_eq!(side.selected, None);
    assert_eq!(side.scroll, 0.0);
}

#[test]
fn focusing_a_filter_blurs_the_other_and_takes_pane_focus() {
    let mut st = SftpPaneState::new_local_local("/a", "/b");
    st.focus_filter(SftpFocus::Left);
    st.focus_filter(SftpFocus::Right);
    assert!(!st.left.filter_focused);
    assert!(st.right.filter_focused);
    assert_eq!(st.focus, SftpFocus::Right);
    assert!(st.filter_owns_keys());
    st.blur_filters();
    assert!(!st.filter_owns_keys());
}
