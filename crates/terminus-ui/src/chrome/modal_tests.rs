use super::test_support::*;
use super::*;
use crate::settings::SettingsTab;

#[test]
fn modal_paint_stack_puts_vault_above_host_editor() {
    let mut chrome = chrome_with_hosts(1);
    chrome.open_edit_host(
        crate::add_host::HostFormValues {
            name: "h".into(),
            hostname: "1.2.3.4".into(),
            username: "u".into(),
            port: "22".into(),
            auth_method: "password".into(),
            password: String::new(),
            identity_id: None,
            ..crate::add_host::HostFormValues::default()
        },
        "id-0".into(),
    );
    assert_eq!(chrome.top_modal_paint(), Some(ModalPaintLayer::HostEditor));
    chrome.open_vault_unlock(PendingVaultAction::SubmitHostForm);
    assert_eq!(
        chrome.modal_paint_stack(),
        vec![ModalPaintLayer::HostEditor, ModalPaintLayer::VaultUnlock]
    );
    assert_eq!(chrome.top_modal_paint(), Some(ModalPaintLayer::VaultUnlock));
}

#[test]
fn settings_sits_above_host_editor_in_paint_stack() {
    let mut chrome = chrome_with_hosts(1);
    chrome.open_add_host();
    chrome.open_settings(SettingsTab::Keys);
    assert_eq!(
        chrome.modal_paint_stack(),
        vec![ModalPaintLayer::HostEditor, ModalPaintLayer::Settings]
    );
    assert_eq!(chrome.top_modal_paint(), Some(ModalPaintLayer::Settings));
}

// ---- Polish 4: overflowing machine list ----

#[test]
fn a_machine_selected_elsewhere_is_scrolled_into_view_once() {
    let mut chrome = chrome_with_hosts(30);
    let h = 630.0;
    // Selected from the palette: row 26 ("id-25") is below the fold.
    assert!(chrome.reveal_machine("id-25", h));
    let body = chrome.panel.body_rect(0.0, h);
    assert!(chrome.panel.row_painted(0.0, h, 26));
    assert!(chrome.panel.card_rect(0.0, 26).bottom() <= body.bottom() + 0.01);

    // The user then scrolls away: the same selection does not pull
    // the list back on every frame.
    chrome.panel.scroll = 0.0;
    assert!(!chrome.reveal_machine("id-25", h));
    assert_eq!(chrome.panel.scroll, 0.0);

    // Unknown ids (Local without a row, a collapsed group) are a no-op.
    assert!(!chrome.reveal_machine("nope", h));
}

#[test]
fn pixel_wheel_deltas_scroll_the_list_too() {
    let mut chrome = chrome_with_hosts(30);
    let h = 630.0;
    let row = chrome.panel.item_rect(0.0, 3);
    // Touchpads send pixels, not lines (positive = content moves down).
    assert!(chrome.handle_wheel_pixels(h, row.x + 10.0, row.y + 5.0, -30.0));
    assert_eq!(chrome.panel.scroll, 30.0);
    assert!(chrome.handle_wheel_pixels(h, row.x + 10.0, row.y + 5.0, 100.0));
    assert_eq!(chrome.panel.scroll, 0.0);
    // Outside the sidebar the terminal keeps the wheel.
    assert!(!chrome.handle_wheel_pixels(h, 600.0, 300.0, -30.0));
}

#[test]
fn dragging_a_host_to_the_bottom_edge_auto_scrolls_and_retargets() {
    let mut chrome = chrome_with_hosts(30);
    let h = 630.0;
    chrome.set_window_size(922.0, h);
    let row = chrome.panel.card_rect(0.0, 2);
    chrome.handle_press(1200.0, h, row.x + 20.0, row.y + 20.0);
    let body = chrome.panel.body_rect(0.0, h);
    let edge_y = body.bottom() - 3.0;
    assert!(chrome.handle_drag_move(h, row.x + 20.0, edge_y));
    let before = chrome.panel.drop_target_at(0.0, h, row.x + 20.0, edge_y);
    for _ in 0..30 {
        chrome.tick_host_drag(1.0 / 60.0);
    }
    assert!(
        chrome.panel.scroll > 0.0,
        "the list follows the dragged host"
    );
    let after = chrome.panel.host_drag.as_ref().unwrap().drop_target.clone();
    assert!(after.is_some());
    assert_ne!(after, before, "the drop target follows the scrolled rows");
    // Released there, the drop lands on the row now under the pointer.
    let action = chrome.handle_release(h, row.x + 20.0, edge_y);
    assert!(matches!(action, ChromeAction::ReorderHost { .. }));
}

#[test]
fn copy_button_on_the_add_host_error_copies_the_full_message() {
    let mut chrome = chrome_with_hosts(0);
    chrome.open_add_host();
    let message = "Could not save the host: Database error: error returned from \
                   database: (code: 1) table hosts has no column named os_id";
    chrome.form.set_error(message);
    let layout = chrome.dialog_layout(1200.0, 800.0);
    let copy = layout.copy_error_rect(&chrome.form).expect("copy button");

    let action = chrome.handle_press(1200.0, 800.0, copy.x + 2.0, copy.y + 2.0);

    assert_eq!(action, ChromeAction::CopyText(message.into()));
    assert!(chrome.form.is_open(), "copying keeps the dialog open");
}
