use super::test_support::*;
use super::*;
use crate::sidebar::Badge;

#[test]
fn right_click_host_opens_delete_menu_and_selects() {
    let mut chrome = chrome_with_hosts(2);
    let row = chrome.panel.item_rect(0.0, 1);
    let open =
        chrome.handle_context_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0, false);
    assert_eq!(open, ChromeAction::Consumed);
    let menu = chrome.context_menu.as_ref().expect("menu open");
    let delete = (0..menu.entries.len())
        .position(|i| matches!(menu.take_action(i), Some(ContextAction::DeleteHost(_))))
        .expect("delete item");
    let item = menu.item_rect(delete).unwrap();
    // The destructive row opens a confirmation instead of deleting.
    let action = chrome.handle_press(1200.0, 800.0, item.x + 4.0, item.y + 4.0);
    assert_eq!(action, ChromeAction::Consumed);
    assert!(chrome.context_menu.is_none(), "menu closes");
    let prompt = chrome.confirm.as_ref().expect("confirm dialog open");
    assert_eq!(prompt.spec.title, "Delete host-0?");
    assert_eq!(
        chrome.top_modal_paint(),
        Some(ModalPaintLayer::Confirm),
        "the dialog paints above everything"
    );
    // Enter on the default (Cancel) focus keeps the host.
    assert_eq!(
        chrome.handle_confirm_key(DialogKey::Enter),
        Some(ChromeAction::Consumed)
    );
    assert!(chrome.confirm.is_none());
}

#[test]
fn confirming_the_delete_dialog_emits_the_delete_action() {
    let mut chrome = chrome_with_hosts(2);
    chrome.open_confirm(ConfirmPrompt::delete_host("id-0", "host-0", 0));
    let confirm = chrome
        .confirm
        .as_ref()
        .unwrap()
        .layout((1200.0, 800.0))
        .dialog
        .confirm;
    let action = chrome.handle_press(1200.0, 800.0, confirm.x + 4.0, confirm.y + 4.0);
    assert_eq!(action, ChromeAction::DeleteHost("id-0".to_string()));
    assert!(chrome.confirm.is_none());

    chrome.open_confirm(ConfirmPrompt::delete_group("g", "prod", 1));
    assert_eq!(
        chrome.handle_confirm_key(DialogKey::Tab),
        Some(ChromeAction::Consumed)
    );
    assert_eq!(
        chrome.handle_confirm_key(DialogKey::Enter),
        Some(ChromeAction::DeleteGroup("g".to_string()))
    );
}

#[test]
fn sftp_delete_runs_once_on_confirm_and_never_on_cancel() {
    let mut chrome = chrome_with_hosts(1);
    chrome.open_confirm(ConfirmPrompt::sftp_delete("logs", true));
    let prompt = chrome.confirm.as_ref().unwrap();
    assert_eq!(prompt.spec.title, "Delete logs?");
    assert!(prompt.spec.body.contains("everything inside"));
    assert_eq!(
        chrome.handle_confirm_key(DialogKey::Enter),
        Some(ChromeAction::Consumed),
        "the default focus is Cancel"
    );
    assert!(chrome.confirm.is_none());

    chrome.open_confirm(ConfirmPrompt::sftp_delete("a.txt", false));
    assert_eq!(
        chrome.handle_confirm_key(DialogKey::Escape),
        Some(ChromeAction::Consumed)
    );
    assert!(chrome.confirm.is_none());

    chrome.open_confirm(ConfirmPrompt::sftp_delete("a.txt", false));
    chrome.handle_confirm_key(DialogKey::Tab);
    assert_eq!(
        chrome.handle_confirm_key(DialogKey::Enter),
        Some(ChromeAction::SftpDeleteConfirmed)
    );
    assert_eq!(chrome.handle_confirm_key(DialogKey::Enter), None);
}

#[test]
fn the_scrim_and_escape_dismiss_the_delete_dialog() {
    let mut chrome = chrome_with_hosts(1);
    chrome.open_confirm(ConfirmPrompt::delete_host("id-0", "host-0", 0));
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, 3.0, 3.0),
        ChromeAction::Consumed
    );
    assert!(chrome.confirm.is_none());
    chrome.open_confirm(ConfirmPrompt::delete_host("id-0", "host-0", 0));
    assert_eq!(
        chrome.handle_confirm_key(DialogKey::Escape),
        Some(ChromeAction::Consumed)
    );
    assert!(chrome.confirm.is_none());
    assert_eq!(chrome.handle_confirm_key(DialogKey::Escape), None);
}

#[test]
fn deleting_a_host_with_open_sessions_says_so() {
    let mut chrome = chrome_with_hosts(1);
    if let Some(Row::Host(h)) = chrome
        .panel
        .rows
        .iter_mut()
        .find(|r| matches!(r, Row::Host(_)))
    {
        h.session_count = 2;
    }
    let row = chrome.panel.item_rect(0.0, 1);
    chrome.handle_context_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0, false);
    let menu = chrome.context_menu.as_ref().unwrap();
    let delete = (0..menu.entries.len())
        .position(|i| matches!(menu.take_action(i), Some(ContextAction::DeleteHost(_))))
        .unwrap();
    let item = menu.item_rect(delete).unwrap();
    chrome.handle_press(1200.0, 800.0, item.x + 4.0, item.y + 4.0);
    assert!(chrome
        .confirm
        .as_ref()
        .unwrap()
        .spec
        .body
        .contains("2 open sessions"));
}

#[test]
fn pressing_a_host_row_arms_drag_and_opens_on_release() {
    let mut chrome = chrome_with_hosts(3);
    let row = chrome.panel.item_rect(0.0, 2);
    let action = chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0);
    assert_eq!(action, ChromeAction::Consumed);
    assert_eq!(chrome.panel.selected, Some(2));
    assert!(chrome.panel.host_drag.is_some());
    let release = chrome.handle_release(800.0, row.x + 20.0, row.y + 20.0);
    assert_eq!(release, ChromeAction::OpenHost("id-1".to_string()));
    assert!(chrome.panel.host_drag.is_none());
}

#[test]
fn group_menu_open_and_close_all_reach_the_app() {
    let group_row = Row::Group {
        id: "g1".to_string(),
        name: "jeremy".to_string(),
        host_count: 2,
        session_count: 0,
        collapsed: false,
    };
    for (item, expected) in [
        (0, ChromeAction::OpenGroup("g1".to_string())),
        (1, ChromeAction::CloseGroup("g1".to_string())),
    ] {
        let mut chrome = Chrome::default();
        chrome.set_rows(vec![Row::Section("Hosts".to_string()), group_row.clone()]);
        let card = chrome.panel.card_rect(0.0, 1);
        chrome.handle_context_press(
            1200.0,
            800.0,
            card.x + 20.0,
            card.y + card.height / 2.0,
            false,
        );
        let rect = chrome
            .context_menu
            .as_ref()
            .expect("group menu open")
            .item_rect(item)
            .unwrap();
        let action = chrome.handle_press(1200.0, 800.0, rect.x + 4.0, rect.y + 4.0);
        assert_eq!(action, expected);
    }
}

#[test]
fn dragging_a_host_onto_a_group_emits_set_host_group() {
    let mut chrome = Chrome::default();
    chrome.set_rows(vec![
        Row::Section("Hosts".to_string()),
        Row::Group {
            id: "g1".to_string(),
            name: "jeremy".to_string(),
            host_count: 0,
            session_count: 0,
            collapsed: false,
        },
        Row::Host(HostItem {
            id: "solo".to_string(),
            name: "solo".to_string(),
            endpoint: "root@solo".to_string(),
            badge: Badge::Ssh,
            stored: true,
            os_id: None,
            status: crate::os_icons::HostStatus::Idle,
            nested: false,
            session_count: 0,
        }),
    ]);
    let host = chrome.panel.card_rect(0.0, 2);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, host.x + 20.0, host.y + 20.0),
        ChromeAction::Consumed
    );
    let group = chrome.panel.card_rect(0.0, 1);
    let gy = group.y + group.height / 2.0;
    assert!(chrome.handle_drag_move(800.0, group.x + 20.0, gy));
    assert!(chrome.panel.host_drag.as_ref().is_some_and(|d| d.started()));
    let action = chrome.handle_release(800.0, group.x + 20.0, gy);
    assert_eq!(
        action,
        ChromeAction::SetHostGroup {
            host_id: "solo".to_string(),
            group_id: Some("g1".to_string()),
        }
    );
    assert!(chrome.panel.host_drag.is_none());
}

#[test]
fn pressing_add_host_opens_the_editor_not_a_connection() {
    let mut chrome = chrome_with_hosts(3);
    let button = chrome.panel.add_button_rect(0.0, 800.0);
    let action = chrome.handle_press(1200.0, 800.0, button.x + 10.0, button.y + 5.0);
    assert_eq!(action, ChromeAction::AddHost);

    chrome.open_add_host();
    assert!(chrome.add_host_is_open());
}
