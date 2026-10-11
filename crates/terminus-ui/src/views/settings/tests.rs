use super::*;

#[test]
fn config_edits_map_to_the_keys_the_app_reads() {
    assert_eq!(
        SettingsAction::SetFontSize(14.0).config_edit(),
        Some(("fonts", "size", "14".to_string()))
    );
    assert_eq!(
        SettingsAction::SetFontSize(14.5).config_edit(),
        Some(("fonts", "size", "14.5".to_string()))
    );
    assert_eq!(
        SettingsAction::SetFont("Fira Code".into()).config_edit(),
        Some(("fonts", "family", "\"Fira Code\"".to_string()))
    );
    assert_eq!(
        SettingsAction::SetCursor(CursorStyle::Beam).config_edit(),
        Some(("cursor", "shape", "\"beam\"".to_string()))
    );
    assert_eq!(
        SettingsAction::SetCheckUpdates(false).config_edit(),
        Some(("updates", "check", "false".to_string()))
    );
    assert_eq!(
        SettingsAction::SetAutoInstall(true).config_edit(),
        Some(("updates", "auto-install", "true".to_string()))
    );
    assert_eq!(
        SettingsAction::SetTheme(ThemeChoice::System).config_edit(),
        Some(("appearance", "theme", "\"system\"".to_string()))
    );
    assert_eq!(SettingsAction::CheckUpdates.config_edit(), None);
}

#[test]
fn preview_names_map_to_pages() {
    assert_eq!(Page::from_preview("settings-sync"), Some(Page::Sync));
    assert_eq!(Page::from_preview("files"), None);
}

#[test]
fn confirm_dialog_is_centred_in_content_and_cancel_wins_scrim() {
    let content = Rect::new(260.0, 96.0, 1180.0, 804.0);
    let c = Confirm::destructive(
        "Delete id_ed25519?",
        "Servers using it will ask for a password.",
        "Delete",
    );
    let l = confirm_layout(content, &mut test_measure, &c);
    let d = l.dialog.dialog;
    assert!((d.x + d.width / 2.0 - (content.x + content.width / 2.0)).abs() <= 1.0);
    assert_eq!(
        confirm_hit(&l, content.x + 2.0, content.y + 2.0),
        ConfirmHit::Cancel
    );
    let cf = l.dialog.confirm;
    assert_eq!(confirm_hit(&l, cf.x + 2.0, cf.y + 2.0), ConfirmHit::Confirm);
    assert_eq!(c.focus, DialogFocus::Cancel);
}

#[test]
fn the_pointer_is_a_hand_over_what_a_press_acts_on() {
    use crate::chrome::ChromeCursor;
    let content = Rect::new(260.0, 96.0, 1180.0, 804.0);
    let mut v = SettingsView::new("1.0.0");
    v.keys.set_keys(vec![crate::settings::SshKeyItem {
        id: "k1".into(),
        name: "id_ed25519".into(),
        fingerprint: "SHA256:k1".into(),
        created: "2026-01-02".into(),
        public_key: "ssh-ed25519 AAAA".into(),
    }]);
    let mid = |r: Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);

    // Keys: header buttons are hands, the empty corner is not.
    let l = v.keys.layout(content, &mut test_measure);
    let (x, y) = mid(l.generate);
    assert_eq!(
        v.cursor_at(content, &mut test_measure, x, y),
        ChromeCursor::Pointer
    );
    let (x, y) = mid(l.import);
    assert_eq!(
        v.cursor_at(content, &mut test_measure, x, y),
        ChromeCursor::Pointer
    );
    assert_eq!(
        v.cursor_at(
            content,
            &mut test_measure,
            content.x + 2.0,
            content.bottom() - 2.0
        ),
        ChromeCursor::Default
    );
    // A draft's text fields show the I-beam.
    let _ = v.press(
        content,
        &mut test_measure,
        mid(l.generate).0,
        mid(l.generate).1,
    );
    let l = v.keys.layout(content, &mut test_measure);
    let (_, field) = &l.draft.as_ref().unwrap().fields[0];
    let (x, y) = mid(field.box_rect);
    assert_eq!(
        v.cursor_at(content, &mut test_measure, x, y),
        ChromeCursor::Text
    );

    // Sync: engine segments and Save are hands, the URI field is text.
    v.set_page(Page::Sync);
    let l = v.sync.layout(content, &mut test_measure);
    let (x, y) = mid(l.save);
    assert_eq!(
        v.cursor_at(content, &mut test_measure, x, y),
        ChromeCursor::Pointer
    );
    let (x, y) = mid(l.field.box_rect);
    assert_eq!(
        v.cursor_at(content, &mut test_measure, x, y),
        ChromeCursor::Text
    );

    // Updates: the whole toggle row is a hand.
    v.set_page(Page::Updates);
    let l = v.updates.layout(content, &mut test_measure);
    let (x, y) = mid(l.check.card);
    assert_eq!(
        v.cursor_at(content, &mut test_measure, x, y),
        ChromeCursor::Pointer
    );

    // Appearance: the size steppers are hands.
    v.set_page(Page::Appearance);
    let l = v.appearance.layout(content, &mut test_measure);
    let (x, y) = mid(l.size_plus);
    assert_eq!(
        v.cursor_at(content, &mut test_measure, x, y),
        ChromeCursor::Pointer
    );
}

#[test]
fn the_delete_confirm_buttons_are_hands() {
    use crate::chrome::ChromeCursor;
    let content = Rect::new(260.0, 96.0, 1180.0, 804.0);
    let mut v = SettingsView::new("1.0.0");
    v.keys.set_keys(vec![crate::settings::SshKeyItem {
        id: "k1".into(),
        name: "id_ed25519".into(),
        fingerprint: "SHA256:k1".into(),
        created: "2026-01-02".into(),
        public_key: "ssh-ed25519 AAAA".into(),
    }]);
    let l = v.keys.layout(content, &mut test_measure);
    let row = &l.rows[0];
    // Trash is the last action on the card.
    let trash = crate::components::list::card_layout(
        row.card.rect,
        &crate::components::list::CardSpec {
            has_dot: false,
            meta_width: row.meta_width,
            action_widths: &[row.copy_width, row.trash_width],
        },
    )
    .actions[1]
        .expect("trash slot");
    let _ = v.press(
        content,
        &mut test_measure,
        trash.x + trash.width / 2.0,
        trash.y + trash.height / 2.0,
    );
    let l = v.keys.layout(content, &mut test_measure);
    let cf = l.confirm.as_ref().expect("confirm open").dialog.confirm;
    assert_eq!(
        v.cursor_at(content, &mut test_measure, cf.x + 2.0, cf.y + 2.0),
        ChromeCursor::Pointer
    );
}
