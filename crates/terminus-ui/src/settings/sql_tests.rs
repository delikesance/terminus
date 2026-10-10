use super::*;

#[test]
fn defaults_are_empty_not_mock_connected() {
    let s = SettingsModal::default();
    assert!(s.sql_uri.value.is_empty());
    assert!(s.sql_passphrase.value.is_empty());
    assert_eq!(s.sql_uri.caret, 0);
    assert!(!s.sync_connected);
    assert_eq!(s.sync_status, "Not configured");
    assert!(!s.vault_unlocked);
}

#[test]
fn sql_text_edits_go_to_the_focused_field() {
    let mut s = SettingsModal::default();
    assert!(!s.insert_sql_text("x"));
    s.focus_uri();
    assert!(s.insert_sql_text("sqlite:./remote.db"));
    assert_eq!(s.sql_uri.value, "sqlite:./remote.db");
    s.focus_passphrase();
    assert!(s.insert_sql_text("secretpass"));
    assert_eq!(s.sql_passphrase.value, "secretpass");
    assert!(s
        .sql_draft_active()
        .expect("passphrase owns the caret")
        .backspace(false));
    assert_eq!(s.sql_passphrase.value, "secretpas");
}

#[test]
fn sql_caret_moves_and_typing_lands_at_the_caret() {
    use crate::TextMoveKind;
    let mut s = SettingsModal::default();
    s.focus_uri();
    assert!(s.insert_sql_text("sqlite:./remote.db"));
    assert_eq!(s.sql_uri.caret, "sqlite:./remote.db".chars().count());

    let draft = s.sql_draft_active().expect("uri owns the caret");
    assert!(draft.move_home(TextMoveKind::Collapse));
    draft.move_right(TextMoveKind::Collapse, false);
    assert_eq!(s.sql_uri.caret, 1);
    assert!(s.insert_sql_text("X"));
    assert_eq!(s.sql_uri.value, "sXqlite:./remote.db");
    assert_eq!(s.sql_uri.caret, 2);

    let draft = s.sql_draft_active().expect("uri owns the caret");
    assert!(draft.move_end(TextMoveKind::Collapse));
    assert_eq!(s.sql_uri.caret, s.sql_uri.value.chars().count());
}

#[test]
fn sql_word_jumps_move_by_whole_words() {
    use crate::TextMoveKind;
    let mut s = SettingsModal::default();
    s.focus_uri();
    s.insert_sql_text("postgres://host/db");
    {
        let draft = s.sql_draft_active().expect("uri owns the caret");
        assert!(draft.move_left(TextMoveKind::Collapse, true));
    }
    assert_eq!(s.sql_uri.caret, "postgres://host/".chars().count());
    {
        let draft = s.sql_draft_active().expect("uri owns the caret");
        assert!(draft.move_right(TextMoveKind::Collapse, true));
    }
    assert_eq!(s.sql_uri.caret, s.sql_uri.value.chars().count());
}

#[test]
fn sql_shift_arrows_select_and_typing_replaces_the_selection() {
    use crate::TextMoveKind;
    let mut s = SettingsModal::default();
    s.focus_uri();
    s.insert_sql_text("abcdef");
    {
        let draft = s.sql_draft_active().expect("uri owns the caret");
        draft.move_left(TextMoveKind::Extend, false);
        draft.move_left(TextMoveKind::Extend, false);
    }
    assert_eq!(s.sql_uri.selection_range(), Some((4, 6)));
    assert!(s.insert_sql_text("XY"));
    assert_eq!(s.sql_uri.value, "abcdXY");
    assert_eq!(s.sql_uri.selection_range(), None);
}

#[test]
fn sql_select_all_then_typing_replaces_the_whole_value() {
    let mut s = SettingsModal::default();
    s.focus_passphrase();
    s.insert_sql_text("secret");
    let draft = s.sql_draft_active().expect("passphrase owns the caret");
    assert!(draft.select_all());
    assert!(s.insert_sql_text("new"));
    assert_eq!(s.sql_passphrase.value, "new");
    assert_eq!(s.sql_passphrase.caret, 3);
    assert_eq!(s.sql_passphrase.selection_range(), None);
}

#[test]
fn sql_backspace_and_delete_follow_the_caret() {
    use crate::TextMoveKind;
    let mut s = SettingsModal::default();
    s.focus_uri();
    s.insert_sql_text("abc");
    {
        let draft = s.sql_draft_active().expect("uri owns the caret");
        draft.move_home(TextMoveKind::Collapse);
    }
    assert!(!s
        .sql_draft_active()
        .expect("uri owns the caret")
        .backspace(false));
    {
        let draft = s.sql_draft_active().expect("uri owns the caret");
        draft.move_end(TextMoveKind::Collapse);
    }
    assert!(s
        .sql_draft_active()
        .expect("uri owns the caret")
        .backspace(false));
    assert_eq!(s.sql_uri.value, "ab");
    {
        let draft = s.sql_draft_active().expect("uri owns the caret");
        draft.move_home(TextMoveKind::Collapse);
        assert!(draft.delete_forward(false));
    }
    assert_eq!(s.sql_uri.value, "b");
}

#[test]
fn sql_word_backspace_removes_a_whole_word() {
    let mut s = SettingsModal::default();
    s.focus_uri();
    s.insert_sql_text("sqlite:./remote.db");
    let draft = s.sql_draft_active().expect("uri owns the caret");
    assert!(draft.backspace(true));
    assert_eq!(s.sql_uri.value, "sqlite:./remote.");
}

#[test]
fn sql_fields_accept_spaces_and_reject_control_characters() {
    let mut s = SettingsModal::default();
    s.focus_uri();
    assert!(s.insert_sql_text(" "));
    assert_eq!(s.sql_uri.value, " ");
    assert!(!s.insert_sql_text("a\nb"));
    assert_eq!(s.sql_uri.value, " ");
    s.focus_passphrase();
    assert!(!s.insert_sql_text("\t"));
}

#[test]
fn uri_paint_exposes_caret_prefix_and_selection() {
    use crate::TextMoveKind;
    let mut s = SettingsModal::default();
    s.focus_uri();
    s.insert_sql_text("sqlite");
    {
        let draft = s.sql_draft_active().expect("uri owns the caret");
        draft.move_home(TextMoveKind::Collapse);
        draft.move_right(TextMoveKind::Collapse, false);
    }
    let paint = s.uri_field_paint();
    assert_eq!(paint.text, "sqlite");
    assert_eq!(paint.caret_prefix, "s");
    assert!(paint.show_caret);
    assert!(!paint.placeholder);
    assert_eq!(paint.selection, None);

    {
        let draft = s.sql_draft_active().expect("uri owns the caret");
        draft.move_right(TextMoveKind::Extend, false);
        draft.move_right(TextMoveKind::Extend, false);
    }
    let paint = s.uri_field_paint();
    assert_eq!(paint.selection, Some((1, 3)));
    assert_eq!(paint.caret_prefix, "sql");
}

#[test]
fn passphrase_paint_masks_caret_prefix_and_keeps_selection() {
    let mut s = SettingsModal::default();
    s.focus_passphrase();
    s.insert_sql_text("secret");
    {
        let draft = s.sql_draft_active().expect("passphrase owns the caret");
        draft.select_all();
    }
    let paint = s.passphrase_field_paint();
    assert_eq!(
        paint.text,
        "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
    );
    assert_eq!(
        paint.caret_prefix,
        "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
    );
    assert_eq!(paint.selection, Some((0, 6)));
    assert!(paint.show_caret);

    s.toggle_passphrase_visible();
    let paint = s.passphrase_field_paint();
    assert_eq!(paint.text, "secret");
    assert_eq!(paint.caret_prefix, "secret");
}

#[test]
fn blurring_a_sql_field_hides_its_caret_and_selection() {
    use crate::TextMoveKind;
    let mut s = SettingsModal::default();
    s.focus_uri();
    s.insert_sql_text("sqlite");
    {
        let draft = s.sql_draft_active().expect("uri owns the caret");
        draft.move_home(TextMoveKind::Extend);
    }
    assert!(s.uri_field_paint().show_caret);
    assert!(s.uri_field_paint().selection.is_some());
    s.clear_sql_focus();
    let paint = s.uri_field_paint();
    assert!(!paint.show_caret);
    assert_eq!(paint.selection, None);
}

#[test]
fn hit_test_finds_new_key_cta_and_generate_form() {
    let mut s = SettingsModal::default();
    s.open_tab(SettingsTab::Keys);
    let (w, h) = (1000.0, 800.0);
    let cta = s.new_key_cta_rect(w, h);
    assert_eq!(
        s.hit_test(w, h, cta.x + 4.0, cta.y + 4.0),
        SettingsHit::NewKey
    );
    s.open_key_draft();
    assert!(s.key_drafting);
    let field = s.key_draft_field_rect(w, h).expect("field");
    assert_eq!(
        s.hit_test(w, h, field.x + 2.0, field.y + 2.0),
        SettingsHit::FocusKeyDraft
    );
    let gen = s.key_draft_generate_rect(w, h).expect("generate");
    assert_eq!(
        s.hit_test(w, h, gen.x + 2.0, gen.y + 2.0),
        SettingsHit::GenerateKey
    );
    s.insert_key_draft_text("Laptop");
    assert_eq!(s.take_key_draft_label().unwrap(), "Laptop");
}

#[test]
fn key_draft_has_passphrase_field_for_encrypted_imports() {
    let mut s = SettingsModal::default();
    s.open_tab(SettingsTab::Keys);
    let (w, h) = (1000.0, 800.0);
    s.open_key_draft();
    let pem = s.key_draft_pem_rect(w, h).expect("pem");
    let pass = s.key_draft_passphrase_rect(w, h).expect("passphrase");
    assert!(
        pass.y >= pem.bottom(),
        "passphrase card sits below the PEM card"
    );
    let draft = s.key_draft_rect(w, h).unwrap();
    let cancel = s.key_draft_cancel_rect(w, h).unwrap();
    assert!(
        cancel.y >= pass.bottom(),
        "actions sit below the passphrase card"
    );
    assert!(
        cancel.bottom() <= draft.bottom(),
        "actions stay inside the form"
    );
    assert_eq!(
        s.hit_test(w, h, pass.x + 2.0, pass.y + 2.0),
        SettingsHit::FocusKeyPassphrase
    );
    s.focus_key_passphrase();
    s.insert_key_draft_text("hunter2");
    assert_eq!(s.key_passphrase.value, "hunter2");
    assert!(s.key_label.value.is_empty() && s.key_pem.value.is_empty());
    assert!(s.key_draft_backspace());
    assert_eq!(s.key_passphrase.value, "hunter");
    s.close_key_draft();
    assert!(s.key_passphrase.value.is_empty(), "secret cleared on close");
}
