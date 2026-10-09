use super::*;

#[test]
fn hit_test_finds_sql_fields_and_buttons() {
    let mut s = SettingsModal::default();
    s.open_tab(SettingsTab::SqlSync);
    let (w, h) = (1000.0, 800.0);
    let uri = s.uri_input_rect(w, h);
    assert_eq!(
        s.hit_test(w, h, uri.x + 2.0, uri.y + 2.0),
        SettingsHit::FocusUri
    );
    let pass = s.passphrase_input_rect(w, h);
    assert_eq!(
        s.hit_test(w, h, pass.x + 2.0, pass.y + 2.0),
        SettingsHit::FocusPassphrase
    );
    let unlock = s.unlock_vault_button_rect(w, h);
    assert_eq!(
        s.hit_test(w, h, unlock.x + 2.0, unlock.y + 2.0),
        SettingsHit::UnlockVault
    );
    let test = s.test_sync_button_rect(w, h);
    assert_eq!(
        s.hit_test(w, h, test.x + 2.0, test.y + 2.0),
        SettingsHit::TestSync
    );
    let eye = s.passphrase_toggle_rect(w, h);
    assert_eq!(
        s.hit_test(w, h, eye.x + 2.0, eye.y + 2.0),
        SettingsHit::TogglePassphrase
    );
    assert!(s.forget_passphrase_button_rect(w, h).is_none());
    s.set_passphrase_remembered(true);
    let forget = s.forget_passphrase_button_rect(w, h).expect("forget btn");
    assert_eq!(
        s.hit_test(w, h, forget.x + 2.0, forget.y + 2.0),
        SettingsHit::ForgetPassphrase
    );
    // Must not overlap Unlock / Test Sync.
    assert!(forget.right() <= unlock.x);
    let engine = s.engine_card_rect(w, h);
    assert_eq!(
        s.hit_test(w, h, engine.x + 2.0, engine.y + 2.0),
        SettingsHit::ToggleEngineMenu
    );
    s.toggle_engine_menu();
    assert!(s.engine_menu_open);
    let opt = s.engine_option_rect(w, h, 1);
    assert_eq!(
        s.hit_test(w, h, opt.x + 2.0, opt.y + 2.0),
        SettingsHit::SelectEngine(1)
    );
    s.select_engine(1);
    assert_eq!(s.engine_label(), "PostgreSQL");
    assert!(!s.engine_menu_open);
}

#[test]
fn apply_sync_status_updates_connected_and_line() {
    let mut s = SettingsModal::default();
    s.apply_sync_status(SyncUiStatus {
        uri: "sqlite:./remote.db".into(),
        connected: true,
        vault_unlocked: true,
        status_line: "Last synced just now".into(),
        is_error: false,
    });
    assert!(s.sync_connected);
    assert!(s.vault_unlocked);
    assert_eq!(s.sync_status, "Last synced just now");
    assert!(!s.sync_status_is_error());
    assert_eq!(s.sql_uri.value, "sqlite:./remote.db");
}

#[test]
fn empty_worker_uri_does_not_wipe_local_draft() {
    let mut s = SettingsModal {
        sql_uri: TextDraft::new("sqlite:./draft.db"),
        ..Default::default()
    };
    s.apply_sync_status(SyncUiStatus {
        uri: String::new(),
        connected: false,
        vault_unlocked: false,
        status_line: "Not configured".into(),
        is_error: false,
    });
    assert_eq!(s.sql_uri.value, "sqlite:./draft.db");
}

#[test]
fn vault_feedback_lands_on_sql_sync_status_not_as_success_when_locked() {
    let mut s = SettingsModal::default();
    assert_eq!(s.sync_status, "Not configured");
    s.apply_vault_feedback(
        "Vault passphrase must be at least 8 characters".into(),
        false,
    );
    assert_eq!(s.sync_status, "Not configured");
    assert_eq!(
        s.sync_error.as_deref(),
        Some("Vault passphrase must be at least 8 characters")
    );
    assert!(s.sync_status_is_error());
    assert!(!s.vault_unlocked);

    s.apply_vault_feedback("Vault unlocked".into(), true);
    assert_eq!(s.sync_status, "Vault unlocked");
    assert!(s.sync_error.is_none());
    assert!(!s.sync_status_is_error());
    assert!(s.vault_unlocked);
}

#[test]
fn sync_error_status_is_flagged() {
    let mut s = SettingsModal::default();
    s.apply_sync_status(SyncUiStatus {
        uri: "postgres://x".into(),
        connected: false,
        vault_unlocked: false,
        status_line: "connection refused".into(),
        is_error: true,
    });
    assert_eq!(s.sync_status, "Not configured");
    assert_eq!(s.sync_error.as_deref(), Some("connection refused"));
    assert!(s.sync_status_is_error());
}

#[test]
fn error_banner_only_when_sync_failed() {
    let mut s = SettingsModal::default();
    s.open_tab(SettingsTab::SqlSync);
    let (w, h) = (1000.0, 800.0);
    assert!(s.error_banner_rect(w, h).is_none());

    s.apply_sync_status(SyncUiStatus {
        uri: "sqlite:./x.db".into(),
        connected: false,
        vault_unlocked: false,
        status_line: "connection refused".into(),
        is_error: true,
    });
    let banner = s.error_banner_rect(w, h).expect("banner");
    let row = s.status_row_rect(w, h);
    assert!(banner.y >= row.bottom());
    assert!((banner.width - row.width).abs() < 0.01);
    assert!((banner.height - ERROR_BANNER_HEIGHT).abs() < 0.01);
    assert_eq!(s.sync_status, "Not configured");

    s.apply_sync_status(SyncUiStatus {
        uri: "sqlite:./x.db".into(),
        connected: true,
        vault_unlocked: true,
        status_line: "Sync ok".into(),
        is_error: false,
    });
    assert!(s.error_banner_rect(w, h).is_none());
    assert_eq!(s.sync_status, "Sync ok");
}

#[test]
fn status_text_sits_above_action_buttons_full_width() {
    let mut s = SettingsModal::default();
    s.open_tab(SettingsTab::SqlSync);
    let (w, h) = (1000.0, 800.0);
    let block = s.status_row_rect(w, h);
    let text = s.status_text_rect(w, h);
    let unlock = s.unlock_vault_button_rect(w, h);
    let test = s.test_sync_button_rect(w, h);

    assert!(text.bottom() <= unlock.y);
    assert!(text.width > block.width * 0.7);
    assert!((unlock.y - test.y).abs() < 0.01);
    assert!(test.x > unlock.right());
    assert!(test.right() <= block.right() + 0.01);
}

#[test]
fn field_paint_hides_placeholder_when_focused_and_shows_caret() {
    let mut s = SettingsModal::default();
    let idle = s.uri_field_paint();
    assert!(idle.placeholder);
    assert!(!idle.show_caret);

    s.focus_uri();
    let focused = s.uri_field_paint();
    assert!(!focused.placeholder);
    assert!(focused.show_caret);
    assert!(focused.text.is_empty());

    s.insert_sql_text("sqlite:./x.db");
    let typed = s.uri_field_paint();
    assert_eq!(typed.text, "sqlite:./x.db");
    assert!(!typed.placeholder);
    assert!(typed.show_caret);
}

#[test]
fn passphrase_paint_never_fakes_full_bullet_placeholder() {
    let mut s = SettingsModal::default();
    let idle = s.passphrase_field_paint();
    assert!(idle.placeholder);
    assert_eq!(idle.text, "Enter passphrase…");

    s.focus_passphrase();
    let focused_empty = s.passphrase_field_paint();
    assert!(!focused_empty.placeholder);
    assert!(focused_empty.text.is_empty());
    assert!(focused_empty.show_caret);

    s.insert_sql_text("secret");
    let masked = s.passphrase_field_paint();
    assert_eq!(masked.text, "••••••");
    assert!(!masked.placeholder);

    s.toggle_passphrase_visible();
    let visible = s.passphrase_field_paint();
    assert_eq!(visible.text, "secret");
}

#[test]
fn field_cards_use_equal_padding_and_eye_clears_text() {
    let mut s = SettingsModal::default();
    s.open_tab(SettingsTab::SqlSync);
    let (w, h) = (1000.0, 800.0);
    let card = s.passphrase_card_rect(w, h);
    let input = field_input_in_card(card);
    assert!((input.x - card.x - FIELD_CARD_PAD).abs() < 0.01);
    assert!((card.right() - input.right() - FIELD_CARD_PAD).abs() < 0.01);
    assert!((card.bottom() - input.bottom() - FIELD_CARD_PAD).abs() < 0.01);

    let text = s.passphrase_input_rect(w, h);
    let eye = s.passphrase_toggle_rect(w, h);
    assert!(text.right() <= eye.x + 0.01);
    assert!((eye.right() - input.right()).abs() < 0.01);
    assert!(!crate::overlap::rects_overlap(text, eye));
}
