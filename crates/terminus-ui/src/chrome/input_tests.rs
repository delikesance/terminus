use super::test_support::*;
use super::*;
use crate::add_host::Field;

#[test]
fn a_press_outside_the_chrome_is_left_for_the_terminal() {
    let mut chrome = chrome_with_hosts(3);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, 600.0, 400.0),
        ChromeAction::Ignored
    );
    // Including the sidebar list's own background, below its rows.
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, 130.0, 600.0),
        ChromeAction::Consumed
    );
}

#[test]
fn a_press_outside_the_chrome_leaves_the_notice_alone() {
    let mut chrome = chrome_with_hosts(3);
    chrome.panel.notice = Some("Added web-01".to_string());

    chrome.handle_press(1200.0, 800.0, 600.0, 400.0);
    assert!(chrome.panel.notice.is_some(), "typing must not eat it");

    let row = chrome.panel.item_rect(0.0, 0);
    chrome.handle_press(1200.0, 800.0, row.x + 20.0, row.y + 20.0);
    assert_eq!(chrome.panel.notice, None);
}

#[test]
fn the_open_dialog_swallows_clicks_and_the_scrim_dismisses_it() {
    let mut chrome = chrome_with_hosts(2);
    chrome.open_add_host();
    let layout = chrome.dialog_layout(1200.0, 800.0);
    let input = layout.input_rect(&chrome.form, Field::Hostname).unwrap();

    assert_eq!(
        chrome.handle_press(1200.0, 800.0, input.x + 5.0, input.y + 5.0),
        ChromeAction::Consumed
    );
    assert!(chrome.add_host_is_open());
    assert_eq!(chrome.form.focused_field(), Field::Hostname);

    let cancel = layout.secondary_button_rect(&chrome.form);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, cancel.x + 4.0, cancel.y + 4.0),
        ChromeAction::Consumed
    );
    assert!(!chrome.add_host_is_open());

    chrome.open_add_host();
    chrome.form.insert("srv.local");
    let next_btn = layout.primary_button_rect(&chrome.form);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, next_btn.x + 4.0, next_btn.y + 4.0),
        ChromeAction::Consumed
    );
    assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Auth);

    chrome.form.set_step(crate::add_host::AddHostStep::Details);
    let layout_details = chrome.dialog_layout(1200.0, 800.0);
    let connect = layout_details.primary_button_rect(&chrome.form);
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, connect.x + 4.0, connect.y + 4.0),
        ChromeAction::SubmitHostForm
    );
    assert!(chrome.add_host_is_open());

    // A click far outside the dialog dismisses it instead of
    // focusing the terminal.
    assert_eq!(
        chrome.handle_press(1200.0, 800.0, 20.0, 780.0),
        ChromeAction::Consumed
    );
    assert!(!chrome.add_host_is_open());
}

#[test]
fn wheel_over_the_panel_scrolls_it_and_over_the_terminal_does_not() {
    let mut chrome = chrome_with_hosts(50);
    let (w, h) = (1200.0, 400.0);
    let _ = w;

    assert!(chrome.handle_wheel(h, 120.0, 300.0, -3.0));
    assert_eq!(
        chrome.panel.scroll,
        3.0 * (crate::sidebar::ITEM_HEIGHT + crate::sidebar::CARD_GAP)
    );

    // The terminal's half of the window is untouched.
    assert!(!chrome.handle_wheel(h, 900.0, 300.0, -3.0));
    assert_eq!(
        chrome.panel.scroll,
        3.0 * (crate::sidebar::ITEM_HEIGHT + crate::sidebar::CARD_GAP)
    );

    // And the wheel clamps at the top.
    chrome.handle_wheel(h, 120.0, 300.0, 99.0);
    assert_eq!(chrome.panel.scroll, 0.0);
}

#[test]
fn keyboard_input_only_reaches_the_editor_while_it_is_open() {
    let mut chrome = chrome_with_hosts(1);
    assert_eq!(
        chrome.handle_form_input(FormInput::Enter, ""),
        None,
        "a closed editor must not swallow Enter"
    );

    chrome.open_add_host();
    assert_eq!(
        chrome.handle_form_input(FormInput::Text, "w"),
        Some(FormOutcome::Consumed)
    );
    assert_eq!(chrome.form.value(Field::Hostname), "w");

    // Step 1 (Target) Enter advances to Auth
    assert_eq!(
        chrome.handle_form_input(FormInput::Enter, ""),
        Some(FormOutcome::Consumed)
    );
    assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Auth);

    // Configure valid auth for Auth step
    chrome.form.select_auth_method(2);

    // Step 2 (Auth) Enter advances to Details
    assert_eq!(
        chrome.handle_form_input(FormInput::Enter, ""),
        Some(FormOutcome::Consumed)
    );
    assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Details);

    // Step 3 (Details) Enter submits
    assert_eq!(
        chrome.handle_form_input(FormInput::Enter, ""),
        Some(FormOutcome::Submit)
    );
    // Enter is a request to save, not a dismissal: the caller closes
    // the form only once the repository accepted the host.
    assert!(chrome.add_host_is_open());

    // Escape on Details goes back to Auth, then Target, then dismisses
    assert_eq!(
        chrome.handle_form_input(FormInput::Escape, ""),
        Some(FormOutcome::Consumed)
    );
    assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Auth);

    assert_eq!(
        chrome.handle_form_input(FormInput::Escape, ""),
        Some(FormOutcome::Consumed)
    );
    assert_eq!(chrome.form.step(), crate::add_host::AddHostStep::Target);

    assert_eq!(
        chrome.handle_form_input(FormInput::Escape, ""),
        Some(FormOutcome::Cancel)
    );
    assert!(!chrome.add_host_is_open());
}

#[test]
fn hovering_a_row_is_reported_once() {
    let mut chrome = chrome_with_hosts(3);
    let row = chrome.panel.item_rect(0.0, 2);
    let (x, y) = (row.x + 20.0, row.y + 20.0);

    assert!(chrome.handle_hover(800.0, x, y));
    assert_eq!(chrome.panel.hover, Some(2));
    // Same row again: no repaint.
    assert!(!chrome.handle_hover(800.0, x, y));
    // Up in the header, which is not a row: the highlight clears.
    assert!(chrome.handle_hover(800.0, x, 10.0));
    assert_eq!(chrome.panel.hover, None);
}

#[test]
fn hovering_the_add_host_row_highlights_it_without_selecting_a_host() {
    let mut chrome = chrome_with_hosts(3);
    let button = chrome.panel.add_button_rect(0.0, 800.0);
    let (x, y) = (button.x + 20.0, button.y + button.height / 2.0);

    assert!(chrome.handle_hover(800.0, x, y));
    assert!(chrome.panel.add_hover);
    // Hovering a control must not look like hovering a host, and
    // must not select one either.
    assert_eq!(chrome.panel.hover, None);
    assert_eq!(chrome.panel.selected, None);

    // Moving onto a row moves the highlight off the button.
    let row = chrome.panel.item_rect(0.0, 2);
    assert!(chrome.handle_hover(800.0, row.x + 20.0, row.y + 20.0));
    assert!(!chrome.panel.add_hover);
    assert_eq!(chrome.panel.hover, Some(2));
}
