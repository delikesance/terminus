use super::test_support::*;
use super::*;

#[test]
fn control_and_empty_text_is_rejected_outright() {
    let mut form = open_form();
    assert!(!form.insert(""));
    assert!(!form.insert("\u{1b}"));
    assert!(!form.insert("a\nb"));
    assert_eq!(form.value(Field::Hostname), "");
    assert_eq!(
        form.handle_input(FormInput::Text, "\u{7f}"),
        FormOutcome::Consumed
    );
}

// -------------------------------------------------------- copy error

#[test]
fn copy_button_only_exists_while_an_error_is_shown() {
    let mut form = open_form();
    let layout = layout_for(&form);
    assert_eq!(layout.copy_error_rect(&form), None);

    form.set_error("Could not save the host: Database error");
    let copy = layout.copy_error_rect(&form).expect("copy button");
    let secondary = layout.secondary_button_rect(&form);
    assert!(copy.right() <= secondary.x, "left of Back/Cancel");
    assert_eq!(copy.y, secondary.y);
}

#[test]
fn footer_text_stops_before_the_copy_button() {
    let mut form = open_form();
    let layout = layout_for(&form);
    let without = layout.footer_text_rect(&form);

    form.set_error("Could not save the host: Database error");
    let with = layout.footer_text_rect(&form);
    let copy = layout.copy_error_rect(&form).unwrap();
    assert!(with.right() <= copy.x, "text never runs under the button");
    assert!(with.width < without.width);
}

#[test]
fn pressing_copy_hits_copy_error() {
    let mut form = open_form();
    form.set_error("boom");
    let layout = layout_for(&form);
    let copy = layout.copy_error_rect(&form).unwrap();
    assert_eq!(
        layout.hit_test(&form, copy.x + 2.0, copy.y + 2.0),
        AddHostHit::CopyError
    );
}

// -------------------------------------------------------- errors

#[test]
fn a_rejection_keeps_the_form_open_with_its_text() {
    let mut form = open_form();
    type_into(&mut form, "web-01");
    form.set_error("Connection refused");
    assert!(form.is_open());
    assert_eq!(form.value(Field::Hostname), "web-01");
    assert_eq!(form.error(), Some("Connection refused"));
    assert_eq!(form.error_field(), None, "not about any one field");
    form.handle_input(FormInput::Text, "2");
    assert_eq!(form.error(), None);
    assert_eq!(form.error_field(), None);
}

#[test]
fn repository_errors_land_on_their_field_and_step() {
    let mut form = open_form();
    form.set_identities(vec![("k1".into(), "Key".into())]);
    type_into(&mut form, "box");
    form.next_step();
    form.next_step();
    assert_eq!(form.step(), AddHostStep::Details);
    form.set_error("'abc' is not a valid port");
    assert_eq!(form.step(), AddHostStep::Target, "jumps back to the port");
    assert_eq!(form.focused_field(), Field::Port);
    assert_eq!(form.error_field(), Some(Field::Port));
    assert_eq!(form.error(), Some("'abc' is not a valid port"));

    form.set_step(AddHostStep::Auth);
    form.set_error("Hostname is required");
    assert_eq!(form.step(), AddHostStep::Target);
    assert_eq!(form.error_field(), Some(Field::Hostname));
    form.set_error("Select a saved SSH key (Settings → Managed SSH Keys)");
    assert_eq!(form.step(), AddHostStep::Auth);
    assert_eq!(form.error_field(), Some(Field::Identity));
    form.set_error("Password is required");
    assert_eq!(form.error_field(), Some(Field::Password));
}

#[test]
fn progress_messages_stay_on_the_current_step() {
    let mut form = open_form();
    form.set_step(AddHostStep::Details);
    form.set_error("Connecting…");
    assert_eq!(form.step(), AddHostStep::Details);
    assert_eq!(form.error_field(), None);
}

#[test]
fn opening_always_starts_from_an_empty_form() {
    let mut form = open_form();
    type_into(&mut form, "stale");
    form.set_error("boom");
    form.cycle_auth_method(1);
    form.open();
    assert_eq!(
        form.values(),
        HostFormValues {
            port: "22".into(),
            auth_method: "password".into(),
            ..HostFormValues::default()
        }
    );
    assert_eq!(form.focused_field(), Field::Hostname);
    assert_eq!(form.step(), AddHostStep::Target);
    assert_eq!(form.error(), None);
}

#[test]
fn open_selects_the_first_identity_when_any() {
    let mut form = AddHostForm::default();
    form.set_identities(vec![
        ("id-a".into(), "Alpha".into()),
        ("id-b".into(), "Beta".into()),
    ]);
    form.open();
    assert_eq!(form.identity_id(), Some("id-a"));
    assert_eq!(form.selected_identity_name(), Some("Alpha"));
}

fn edit_values() -> HostFormValues {
    HostFormValues {
        name: "web".into(),
        hostname: "web.example".into(),
        username: "deploy".into(),
        port: "2222".into(),
        auth_method: "key".into(),
        identity_id: Some("k1".into()),
        password: String::new(),
        group_id: Some("g1".into()),
        tags: "prod, web".into(),
        notes: "Primary box".into(),
    }
}

#[test]
fn open_edit_prefills_and_marks_editing() {
    let mut form = AddHostForm::default();
    form.set_identities(vec![("k1".into(), "Prod".into())]);
    form.set_groups(vec![("g1".into(), "jeremy".into())]);
    form.open_edit(edit_values(), "host-id".into());
    assert!(form.is_open() && form.is_editing());
    assert_eq!(form.editing_id(), Some("host-id"));
    assert_eq!(form.values(), edit_values());
    assert_eq!(form.selected_group_name(), Some("jeremy"));
    assert_eq!(form.step(), AddHostStep::Target);
    assert_eq!(form.auth_validation_error(), None);

    form.set_step(AddHostStep::Auth);
    form.select_auth_method(1);
    assert_eq!(
        form.auth_validation_error(),
        None,
        "blank keeps the password"
    );
}

#[test]
fn open_edit_with_a_default_port_shows_22() {
    let mut form = AddHostForm::default();
    form.open_edit(
        HostFormValues {
            port: String::new(),
            ..edit_values()
        },
        "h".into(),
    );
    assert_eq!(form.value(Field::Port), "22");
}

#[test]
fn closing_clears_the_error() {
    let mut form = open_form();
    form.set_error("boom");
    form.close();
    assert!(!form.is_open());
    assert_eq!(form.error(), None);
}

#[test]
fn values_round_trip_every_field() {
    let mut form = open_form();
    form.set_identities(vec![("k1".into(), "Prod".into())]);
    form.set_groups(vec![("g1".into(), "jeremy".into())]);
    type_into(&mut form, "web-01.example.com");
    form.focus_field(Field::Username);
    type_into(&mut form, "deploy");
    form.focus_field(Field::Port);
    type_into(&mut form, "2222");
    form.focus_field(Field::Name);
    type_into(&mut form, "web-01");
    assert!(form.next_step());

    form.cycle_auth_method(1);
    form.focus_field(Field::Password);
    type_into(&mut form, "s3cret");
    assert!(form.next_step());

    form.select_group(1);
    form.focus_field(Field::Tags);
    type_into(&mut form, "prod");
    form.focus_field(Field::Notes);
    type_into(&mut form, "n");

    assert_eq!(
        form.values(),
        HostFormValues {
            name: "web-01".into(),
            hostname: "web-01.example.com".into(),
            username: "deploy".into(),
            port: "2222".into(),
            auth_method: "password".into(),
            identity_id: Some("k1".into()),
            password: "s3cret".into(),
            group_id: Some("g1".into()),
            tags: "prod".into(),
            notes: "n".into(),
        }
    );
}

#[test]
fn left_right_cycle_auth_identity_and_group() {
    let mut form = open_form();
    form.set_identities(vec![("a".into(), "A".into()), ("b".into(), "B".into())]);
    form.set_groups(vec![("g".into(), "G".into())]);
    form.set_step(AddHostStep::Auth);
    form.focus_field(Field::AuthMethod);
    form.handle_input(FormInput::Right, "");
    assert_eq!(form.auth_method(), "password");
    form.handle_input(FormInput::Right, "");
    assert_eq!(form.auth_method(), "gssapi");
    form.handle_input(FormInput::Right, "");
    assert_eq!(form.auth_method(), "key");
    form.focus_field(Field::Identity);
    form.handle_input(FormInput::Right, "");
    assert_eq!(form.identity_id(), Some("b"));
    form.handle_input(FormInput::Left, "");
    assert_eq!(form.identity_id(), Some("a"));

    form.set_step(AddHostStep::Details);
    form.focus_field(Field::Group);
    assert_eq!(form.group_id(), None);
    form.handle_input(FormInput::Right, "");
    assert_eq!(form.group_id(), Some("g"));
    form.handle_input(FormInput::Right, "");
    assert_eq!(form.group_id(), None, "wraps through 'No group'");
}

#[test]
fn space_opens_a_focused_select() {
    let mut form = open_form();
    form.set_step(AddHostStep::Details);
    form.handle_input(FormInput::Text, " ");
    assert!(form.group_menu_open());
    form.handle_input(FormInput::Text, " ");
    assert!(!form.group_menu_open());
}

#[test]
fn groups_vanish_from_the_selection_when_deleted() {
    let mut form = open_form();
    form.set_groups(vec![("g".into(), "G".into())]);
    form.select_group(1);
    assert_eq!(form.group_id(), Some("g"));
    form.set_groups(Vec::new());
    assert_eq!(form.group_id(), None);
}

#[test]
fn auth_validation_helpers() {
    let mut form = open_form();
    form.set_step(AddHostStep::Auth);
    assert_eq!(form.auth_validation_error(), Some("Select an SSH key"));
    form.set_identities(vec![("k".into(), "Key".into())]);
    assert_eq!(form.auth_validation_error(), None);
    form.cycle_auth_method(1);
    assert_eq!(form.auth_validation_error(), Some("Enter a password"));
    form.focus_field(Field::Password);
    type_into(&mut form, "x");
    assert_eq!(form.auth_validation_error(), None);
    form.cycle_auth_method(1);
    assert_eq!(form.auth_method(), "gssapi");
    assert_eq!(form.auth_validation_error(), None);
}

#[test]
fn selecting_a_card_focuses_its_detail_field() {
    let mut form = open_form();
    form.set_step(AddHostStep::Auth);
    form.select_auth_method(1);
    assert_eq!(form.focused_field(), Field::Password);
    form.select_auth_method(0);
    assert_eq!(form.focused_field(), Field::Identity);
    form.select_auth_method(2);
    assert_eq!(form.focused_field(), Field::AuthMethod);
}

// ---------------------------------------------------------- layout
