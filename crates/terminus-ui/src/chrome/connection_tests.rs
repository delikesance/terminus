use super::test_support::*;
use super::*;

#[test]
fn connection_progress_fills_the_content_and_leaves_the_sidebar_live() {
    let mut chrome = chrome_with_hosts(3);
    chrome.set_window_size(1200.0, 800.0);
    chrome.connection = Some(ConnectionSequence::start_ssh(
        "id-0",
        "host-0",
        "SSH root@host-0",
    ));
    // Painted by the shell in the Terminal content, not as a window modal.
    assert!(chrome.modal_paint_stack().is_empty());
    let content = chrome.connection_area();
    assert_eq!(content, chrome.shell.content_rect());
    let conn = chrome.connection.clone().unwrap();
    let dialog = conn.dialog_rect_in(content);
    assert!(content.contains(dialog.x, dialog.y));
    assert!(content.contains(dialog.right() - 1.0, dialog.bottom() - 1.0));

    // Cancel, inside the content, dismisses; the rest of it swallows.
    let (cx, cy) = centre(&conn.close_button_rect(dialog));
    assert_eq!(
        chrome.cursor_at(1200.0, 800.0, cx, cy),
        ChromeCursor::Pointer
    );
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, content.x + 4.0, content.bottom() - 4.0),
        ChromeAction::Consumed
    );
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, cx, cy),
        ChromeAction::DismissConnection
    );

    // The sidebar still answers while connecting.
    let row = chrome.panel.item_rect(0.0, 2);
    assert_eq!(
        chrome.cursor_at(1200.0, 800.0, row.x + 20.0, row.y + 20.0),
        ChromeCursor::Pointer
    );
    chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0);
    assert_eq!(
        chrome.handle_release(800.0, row.x + 20.0, row.y + 20.0),
        ChromeAction::OpenHost("id-1".to_string())
    );

    // Only the Terminal view shows it: another view's content is live.
    chrome.show_view(crate::shell::WorkspaceView::Home);
    let home = chrome.connection_area();
    let (hx, hy) = (home.x + home.width / 2.0, home.y + 40.0);
    let conn = chrome.connection.take();
    let without = chrome.handle_press(1200.0, 800.0, hx, hy);
    chrome.connection = conn;
    assert_eq!(chrome.handle_press(1200.0, 800.0, hx, hy), without);
}

#[test]
fn lost_connection_card_reconnects_or_closes_its_tab() {
    let mut chrome = chrome_with_hosts(3);
    chrome.set_window_size(1200.0, 800.0);
    chrome.lost = Some(LostSession::new(9, "id-0", "host-0", &[]));
    // Painted in the Terminal content like the connection progress.
    assert!(chrome.modal_paint_stack().is_empty());
    let area = chrome.lost_area().expect("card shown on the Terminal view");
    assert_eq!(area, chrome.shell.content_rect());
    let layout = chrome.lost.as_ref().unwrap().layout_in(area).dialog;

    let (rx, ry) = centre(&layout.confirm);
    assert_eq!(
        chrome.cursor_at(1200.0, 800.0, rx, ry),
        ChromeCursor::Pointer
    );
    assert!(chrome.handle_hover(800.0, rx, ry));
    assert_eq!(
        chrome.lost.as_ref().unwrap().hover,
        Some(crate::components::overlay::DialogFocus::Confirm)
    );
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, rx, ry),
        ChromeAction::ReconnectSession(9)
    );
    let (cx, cy) = centre(&layout.cancel);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, cx, cy),
        ChromeAction::CloseLostSession(9)
    );
    // The dead terminal around the card swallows the press.
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, area.x + 4.0, area.bottom() - 4.0),
        ChromeAction::Consumed
    );
    assert_eq!(
        chrome.handle_context_press(1200.0, 800.0, area.x + 4.0, area.y + 4.0, false),
        ChromeAction::Ignored
    );
    assert!(chrome.context_menu.is_none());

    // The sidebar stays live.
    let row = chrome.panel.item_rect(0.0, 2);
    chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0);
    assert_eq!(
        chrome.handle_release(800.0, row.x + 20.0, row.y + 20.0),
        ChromeAction::OpenHost("id-1".to_string())
    );
}

#[test]
fn lost_connection_card_keys_reconnect_and_tab_to_close() {
    let mut chrome = Chrome::default();
    assert_eq!(chrome.handle_lost_key(DialogKey::Enter), None);
    chrome.lost = Some(LostSession::new(4, "h", "h", &[]));
    assert_eq!(
        chrome.handle_lost_key(DialogKey::Escape),
        Some(ChromeAction::Consumed)
    );
    assert_eq!(
        chrome.handle_lost_key(DialogKey::Enter),
        Some(ChromeAction::ReconnectSession(4))
    );
    assert_eq!(
        chrome.handle_lost_key(DialogKey::Tab),
        Some(ChromeAction::Consumed)
    );
    assert_eq!(
        chrome.handle_lost_key(DialogKey::Enter),
        Some(ChromeAction::CloseLostSession(4))
    );
}

#[test]
fn split_pane_card_covers_only_its_pane() {
    let mut chrome = chrome_with_hosts(1);
    chrome.set_window_size(1200.0, 800.0);
    let content = chrome.shell.content_rect();
    let pane = crate::geom::Rect {
        x: content.x,
        y: content.y,
        width: content.width / 2.0,
        height: content.height,
    };
    chrome.lost = Some(LostSession::new(5, "id-0", "host-0", &[]));
    let whole_tab_area = chrome.lost_area().unwrap();
    assert_eq!(whole_tab_area, content);

    chrome.lost_pane = Some(pane);
    assert_eq!(chrome.lost_area(), Some(pane));

    // A click in the live half is not swallowed by the card.
    let (lx, ly) = (content.x + content.width * 0.75, content.y + 40.0);
    let with_card = chrome.handle_press(1200.0, 800.0, lx, ly);
    chrome.lost = None;
    let without_card = chrome.handle_press(1200.0, 800.0, lx, ly);
    assert_eq!(with_card, without_card);
    chrome.lost = Some(LostSession::new(5, "id-0", "host-0", &[]));

    // The card's buttons are laid out inside the pane.
    let layout = chrome.lost.as_ref().unwrap().layout_in(pane).dialog;
    assert!(layout.dialog.right() <= pane.right());
    let (rx, ry) = centre(&layout.confirm);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, rx, ry),
        ChromeAction::ReconnectSession(5)
    );
}

#[test]
fn keys_skip_the_card_of_an_unfocused_dead_pane() {
    let mut chrome = chrome_with_hosts(1);
    chrome.set_window_size(1200.0, 800.0);
    chrome.lost = Some(LostSession::new(5, "id-0", "host-0", &[]));
    chrome.lost_pane = Some(chrome.shell.content_rect());
    chrome.lost_pane_unfocused = true;
    assert_eq!(chrome.handle_lost_key(DialogKey::Enter), None);
    assert!(!chrome.lost_takes_keys());
    chrome.lost_pane_unfocused = false;
    assert!(chrome.lost_takes_keys());
    assert_eq!(
        chrome.handle_lost_key(DialogKey::Enter),
        Some(ChromeAction::ReconnectSession(5))
    );
}

#[test]
fn lost_connection_card_hides_behind_other_views_and_connecting() {
    let mut chrome = chrome_with_hosts(1);
    chrome.lost = Some(LostSession::new(1, "id-0", "host-0", &[]));
    assert!(chrome.lost_area().is_some());
    chrome.connection = Some(ConnectionSequence::start_ssh("id-0", "host-0", "SSH"));
    assert!(chrome.lost_area().is_none());
    chrome.connection = None;
    // A dialog over the terminal (add snippet, add host) owns the
    // pointer and keys, not the card under it.
    chrome.snippet_form.inner.closing = false;
    assert!(chrome.lost_area().is_none());
    chrome.snippet_form.inner.closing = true;
    chrome.open_add_host();
    assert!(chrome.lost_area().is_none());
    chrome.form.close();
    assert!(chrome.lost_area().is_some());
    chrome.show_view(crate::shell::WorkspaceView::Home);
    assert!(chrome.lost_area().is_none());
    assert_eq!(chrome.handle_lost_key(DialogKey::Enter), None);
}
