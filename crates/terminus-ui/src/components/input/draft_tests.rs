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

#[test]
fn selected_text_and_cut() {
    let mut d = TextDraft::new("hello world");
    assert_eq!(d.selected_text(), None);
    assert_eq!(d.cut(), None);
    d.select_all();
    assert_eq!(d.selected_text(), Some("hello world"));
    d.sel_anchor = Some(6);
    d.caret = 11;
    assert_eq!(d.cut().as_deref(), Some("world"));
    assert_eq!(d.value, "hello ");
    assert_eq!(d.caret, 6);
}

#[test]
fn selected_text_is_char_indexed() {
    let mut d = TextDraft::new("héé x");
    d.sel_anchor = Some(1);
    d.caret = 3;
    assert_eq!(d.selected_text(), Some("éé"));
}

#[test]
fn single_line_paste_drops_trailing_newline_and_flattens() {
    let mut d = TextDraft::new("");
    assert!(d.paste("secret\r\n", 128, false));
    assert_eq!(d.value, "secret");
    let mut d = TextDraft::new("");
    assert!(d.paste("a\tb\x07c\nd\n\n", 128, false));
    assert_eq!(d.value, "a bc d");
}

#[test]
fn multiline_paste_keeps_lf_only() {
    let mut d = TextDraft::new("");
    assert!(d.paste("a\r\nb\rc\x1b\n", 128, true));
    assert_eq!(d.value, "a\nb\nc\n");
}

#[test]
fn paste_replaces_selection_and_honours_the_limit() {
    let mut d = TextDraft::new("abc");
    d.select_all();
    assert!(d.paste("xy", 8, false));
    assert_eq!(d.value, "xy");
    let mut d = TextDraft::new("abc");
    assert!(!d.paste("toolongvalue", 5, false));
    assert!(!d.paste("\n", 5, false));
}
