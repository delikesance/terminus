use super::super::test_measure;
use super::*;

const CONTENT: Rect = Rect::new(260.0, 96.0, 1180.0, 804.0);

fn center(r: Rect) -> (f32, f32) {
    (r.x + r.width / 2.0, r.y + r.height / 2.0)
}

fn press(s: &mut UploadsState, r: Rect) -> Option<SettingsAction> {
    let (x, y) = center(r);
    s.press(CONTENT, &mut test_measure, x, y)
}

fn focused(field: UploadsField) -> UploadsState {
    let mut s = UploadsState::default();
    let l = s.layout(CONTENT, &mut test_measure);
    press(&mut s, l.rows[field as usize].field.box_rect);
    s
}

fn set(field: UploadsField, value: &str) -> SettingsAction {
    SettingsAction::SetUploadDir {
        field,
        value: value.into(),
    }
}

#[test]
fn rows_stack_under_the_intro_and_reset_sits_on_the_label_row() {
    let s = UploadsState::default();
    let l = s.layout(CONTENT, &mut test_measure);
    assert_eq!(l.intro.x, CONTENT.x + 28.0);
    let [linux, windows] = &l.rows;
    assert!(linux.field.total.y > l.intro.bottom());
    assert!(windows.field.total.y > linux.field.total.bottom());
    for row in &l.rows {
        assert!(row.reset.bottom() <= row.field.box_rect.y);
        assert_eq!(row.reset.right(), row.field.box_rect.right());
    }
}

#[test]
fn fields_start_from_the_config_values() {
    let mut s = UploadsState::default();
    s.load("/srv/drop", "D:\\drop");
    assert_eq!(s.draft(UploadsField::Linux).value, "/srv/drop");
    assert_eq!(s.draft(UploadsField::Windows).value, "D:\\drop");
}

#[test]
fn enter_persists_the_typed_directory_once() {
    let mut s = focused(UploadsField::Linux);
    assert!(s.insert_text("/srv/drop"));
    assert_eq!(
        s.key(Key::Enter),
        Some(set(UploadsField::Linux, "/srv/drop"))
    );
    assert!(!s.captures_keyboard());
    assert_eq!(s.blur(), None);
}

#[test]
fn clicking_elsewhere_persists_the_pending_edit() {
    let mut s = focused(UploadsField::Windows);
    s.insert_text("D:\\drop");
    let l = s.layout(CONTENT, &mut test_measure);
    assert_eq!(
        press(&mut s, l.intro),
        Some(set(UploadsField::Windows, "D:\\drop"))
    );
    assert!(!s.captures_keyboard());
}

#[test]
fn blur_without_a_change_writes_nothing() {
    let mut s = focused(UploadsField::Linux);
    assert_eq!(s.blur(), None);
}

#[test]
fn reset_empties_the_field_and_writes_an_empty_string() {
    let mut s = UploadsState::default();
    s.load("/srv/drop", "");
    let l = s.layout(CONTENT, &mut test_measure);
    assert_eq!(
        press(&mut s, l.rows[0].reset),
        Some(set(UploadsField::Linux, ""))
    );
    assert_eq!(s.draft(UploadsField::Linux).value, "");
    assert_eq!(press(&mut s, l.rows[0].reset), None);
}

#[test]
fn escape_discards_the_edit_and_tab_moves_to_the_next_field() {
    let mut s = focused(UploadsField::Linux);
    s.insert_text("/x");
    assert_eq!(s.key(Key::Escape), None);
    assert_eq!(s.draft(UploadsField::Linux).value, "");
    assert!(!s.captures_keyboard());

    let mut s = focused(UploadsField::Linux);
    s.insert_text("/x");
    assert_eq!(s.key(Key::Tab), Some(set(UploadsField::Linux, "/x")));
    assert_eq!(s.focus, Some(UploadsField::Windows));
}

#[test]
fn keys_do_nothing_when_no_field_is_focused() {
    let mut s = UploadsState::default();
    assert!(!s.insert_text("x"));
    assert_eq!(s.key(Key::Enter), None);
}

#[test]
fn hover_targets_follow_the_layout() {
    let mut s = UploadsState::default();
    let l = s.layout(CONTENT, &mut test_measure);
    let (x, y) = center(l.rows[1].field.box_rect);
    assert!(s.hover(CONTENT, &mut test_measure, x, y));
    assert_eq!(s.hover, Some(UploadsTarget::Field(UploadsField::Windows)));
    let (x, y) = center(l.rows[0].reset);
    s.hover(CONTENT, &mut test_measure, x, y);
    assert_eq!(s.hover, Some(UploadsTarget::Reset(UploadsField::Linux)));
}

#[test]
fn config_edit_writes_the_uploads_section_with_escaped_paths() {
    assert_eq!(
        set(UploadsField::Linux, "/srv/drop").config_edit(),
        Some(("uploads", "dir", "\"/srv/drop\"".to_string()))
    );
    assert_eq!(
        set(UploadsField::Windows, "D:\\drop").config_edit(),
        Some(("uploads", "windows-dir", "\"D:\\\\drop\"".to_string()))
    );
}
