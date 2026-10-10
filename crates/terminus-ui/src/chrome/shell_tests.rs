use super::test_support::*;
use super::*;
use crate::settings::SettingsTab;

#[test]
fn a_key_row_copies_its_public_key() {
    let mut chrome = Chrome::default();
    chrome.open_settings(SettingsTab::Keys);
    chrome.settings.set_keys(vec![crate::settings::SshKeyItem {
        id: "k1".into(),
        name: "laptop".into(),
        fingerprint: "SHA256:abc".into(),
        created: "2026-09-28".into(),
        public_key: "ssh-ed25519 AAAA laptop".into(),
    }]);
    let copy = chrome.settings.key_copy_rect(1200.0, 800.0, 0);
    let delete = chrome.settings.key_delete_rect(1200.0, 800.0, 0);
    assert!(copy.right() <= delete.x, "beside delete, not over it");
    let action = chrome.handle_press(1200.0, 800.0, copy.x + 4.0, copy.y + 4.0);
    assert_eq!(
        action,
        ChromeAction::CopyPublicKey("ssh-ed25519 AAAA laptop".into())
    );
    let notice = chrome.settings.keys_notice.clone().expect("confirmation");
    assert!(notice.contains("authorized_keys"), "{notice}");
}

#[test]
fn the_reserved_width_is_the_sidebar() {
    let chrome = chrome_with_hosts(1);
    assert_eq!(chrome.reserved_width(), crate::shell::grid_insets().left);
    assert!(chrome.reserved_width() > crate::shell::layout::SIDEBAR_WIDTH);
    assert_eq!(chrome.origin_y(), 0.0);
}

#[test]
fn settings_button_opens_the_settings_page_and_its_dialog() {
    let mut chrome = chrome_with_hosts(1);
    let (x, y) = centre(&crate::shell::sidebar::settings_rect(800.0));
    let action = chrome.handle_press(1200.0, 800.0, x, y);
    let page = crate::shell::WorkspaceView::Settings(crate::shell::SettingsPage::Keys);
    assert_eq!(action, ChromeAction::ViewChanged(page));
    assert_eq!(chrome.shell.view(), page);
    // The J5 Settings page replaces the legacy dialog.
    assert!(!chrome.settings_is_open());
}

#[test]
fn settings_pages_never_open_the_legacy_dialog_and_leaving_closes_it() {
    use crate::shell::{SettingsPage, WorkspaceView};
    let mut chrome = chrome_with_hosts(1);
    for page in SettingsPage::ALL {
        chrome.show_view(WorkspaceView::Settings(page));
        assert!(!chrome.settings_is_open(), "{page:?}");
    }
    // Opened some other way (palette): leaving Settings closes it.
    chrome.open_settings(SettingsTab::Keys);
    chrome.show_view(WorkspaceView::Terminal);
    assert!(!chrome.settings_is_open());
}

#[test]
fn the_command_bar_opens_the_palette_and_brand_goes_home() {
    let mut chrome = chrome_with_hosts(1);
    let (x, y) = centre(&crate::shell::sidebar::command_bar_rect());
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::OpenPalette
    );
    let (x, y) = centre(&crate::shell::sidebar::brand_rect());
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::ViewChanged(crate::shell::WorkspaceView::Home)
    );
    assert_eq!(chrome.shell.view(), crate::shell::WorkspaceView::Home);
}

#[test]
fn header_tabs_switch_views_and_cover_the_terminal() {
    use crate::shell::WorkspaceView;
    let mut chrome = chrome_with_hosts(1);
    let tab = chrome.shell.header_geom().tabs[3];
    let (x, y) = centre(&tab);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::ViewChanged(WorkspaceView::Snippets)
    );
    // The content is now the view's, not the terminal's.
    assert!(chrome.shell.view_owns(700.0, 400.0));
    // Esc on a view that ignores it returns to the terminal.
    let out = chrome.view_input(&crate::screens::ViewInput::Key {
        key: crate::screens::ViewKey::Escape,
        mods: Default::default(),
    });
    assert_eq!(out, crate::screens::ViewOutcome::Redraw);
    assert_eq!(chrome.shell.view(), WorkspaceView::Terminal);
}

#[test]
fn pills_focus_close_and_add_sessions_of_the_selected_machine() {
    let mut chrome = chrome_with_hosts(1);
    chrome.shell.machine = Some(crate::shell::MachineInfo {
        id: "id-0".into(),
        name: "host-0".into(),
        address: "root@host-0".into(),
    });
    chrome.shell.pills = vec![crate::shell::SessionPill {
        tab_index: 7,
        label: "shell".into(),
        active: false,
        new_output: false,
        closable: true,
    }];
    let g = chrome.shell.pills_geom().unwrap();
    let (x, y) = centre(&g.pills[0]);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::OpenSession(7)
    );
    let (x, y) = centre(&g.plus);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::AddHostSession("id-0".into())
    );
    let (x, y) = centre(&g.split_right);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::Split { down: false }
    );
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, 900.0, 30.0),
        ChromeAction::WindowDrag
    );
}

#[test]
fn right_clicking_a_pill_offers_rename_and_close() {
    let mut chrome = chrome_with_pills();
    let g = chrome.shell.pills_geom().unwrap();
    let (x, y) = centre(&g.pills[1]);
    assert_eq!(
        chrome.handle_context_press(1200.0, 800.0, x, y, false),
        ChromeAction::Consumed
    );
    let menu = chrome.context_menu.clone().expect("session menu");
    let labels: Vec<&str> = menu.entries.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, ["Rename", "Close"]);

    // "Rename" starts the inline draft on that pill.
    let (x, y) = centre(&menu.item_rect(0).unwrap());
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::Consumed
    );
    assert!(chrome.context_menu.is_none());
    assert!(chrome.shell.is_renaming(7));

    // "Close" closes the session like its ×.
    let g = chrome.shell.pills_geom().unwrap();
    let (x, y) = centre(&g.pills[1]);
    chrome.handle_context_press(1200.0, 800.0, x, y, false);
    let menu = chrome.context_menu.clone().unwrap();
    let (x, y) = centre(&menu.item_rect(1).unwrap());
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::CloseSession(7)
    );
}

#[test]
fn a_pinned_pill_can_be_renamed_but_not_closed() {
    let mut chrome = chrome_with_pills();
    let g = chrome.shell.pills_geom().unwrap();
    let (x, y) = centre(&g.pills[0]);
    chrome.handle_context_press(1200.0, 800.0, x, y, false);
    let labels: Vec<String> = chrome
        .context_menu
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .map(|e| e.label.clone())
        .collect();
    assert_eq!(labels, ["Rename"]);
}

#[test]
fn pressing_the_pill_being_renamed_keeps_editing() {
    let mut chrome = chrome_with_pills();
    chrome.shell.begin_rename(7);
    let g = chrome.shell.pills_geom().unwrap();
    let (x, y) = centre(&g.pills[1]);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::Consumed
    );
    assert!(chrome.shell.is_renaming(7));
    // The field covers the pill: its × does not close mid-edit.
    let c = crate::components::navigation::session_pill::close_rect(&g.pills[1]);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, c.x + 2.0, c.y + 2.0),
        ChromeAction::Consumed
    );
    // Another pill still focuses its session.
    let (x, y) = centre(&g.pills[0]);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, x, y),
        ChromeAction::OpenSession(0)
    );
}

#[test]
fn hovering_shell_controls_repaints_and_sets_the_cursor() {
    let mut chrome = chrome_with_hosts(1);
    let (x, y) = centre(&crate::shell::sidebar::add_server_rect(800.0));
    assert!(chrome.handle_hover(800.0, x, y));
    assert_eq!(chrome.shell.hover, Some(crate::shell::ShellHit::AddServer));
    assert_eq!(chrome.cursor_at(1200.0, 800.0, x, y), ChromeCursor::Pointer);
    assert_eq!(
        chrome.cursor_at(1200.0, 800.0, 900.0, 30.0),
        ChromeCursor::Default
    );
}
