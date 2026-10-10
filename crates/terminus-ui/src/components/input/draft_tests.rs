use super::*;

#[test]
fn pem_insert_keeps_newlines() {
    let mut d = TextDraft::default();
    assert!(d.insert(
        "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----\n",
        4096,
        true
    ));
    assert!(d.value.contains('\n'));
    assert!(d.value.contains("BEGIN OPENSSH"));
}

#[test]
fn apply_routes_every_edit_the_same_way() {
    let mut d = TextDraft::new("abc");
    assert!(d.apply(TextEdit::Backspace { by_word: false }));
    assert_eq!(d.value, "ab");
    assert!(d.apply(TextEdit::Home {
        kind: TextMoveKind::Collapse
    }));
    assert!(d.apply(TextEdit::Delete { by_word: false }));
    assert_eq!(d.value, "b");
    assert!(d.apply(TextEdit::SelectAll));
    assert!(d.apply(TextEdit::Backspace { by_word: false }));
    assert_eq!(d.value, "");
    assert!(!d.apply(TextEdit::Backspace { by_word: false }));
}

#[test]
fn label_rejects_newlines() {
    let mut d = TextDraft::default();
    assert!(!d.insert("a\nb", 64, false));
    assert!(d.insert("ab", 64, false));
}

#[test]
fn masked_paint_keeps_caret_and_selection_aligned() {
    let mut d = TextDraft::new("secret");
    d.move_home(TextMoveKind::Collapse);
    assert!(d.move_right(TextMoveKind::Extend, false));
    assert!(d.move_right(TextMoveKind::Extend, false));

    let masked = FieldPaint::from_draft_masked(&d, "Enter passphrase", true, true);
    assert_eq!(masked.text, "••••••");
    assert_eq!(masked.caret_prefix, "••");
    assert_eq!(masked.selection, Some((0, 2)));
    assert!(masked.show_caret);
    assert!(!masked.placeholder);

    let plain = FieldPaint::from_draft_masked(&d, "Enter passphrase", true, false);
    assert_eq!(plain.text, "secret");
    assert_eq!(plain.caret_prefix, "se");
    assert_eq!(plain.selection, Some((0, 2)));

    let idle = FieldPaint::from_draft_masked(
        &TextDraft::default(),
        "Enter passphrase",
        false,
        true,
    );
    assert!(idle.placeholder);
    assert_eq!(idle.text, "Enter passphrase");
    assert!(!idle.show_caret);
    assert_eq!(idle.selection, None);
}
