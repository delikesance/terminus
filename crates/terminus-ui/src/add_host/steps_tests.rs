use super::test_support::*;
use super::*;

#[test]
fn split_address_understands_what_people_paste() {
    let parts = |s: &str| split_address(s);
    assert_eq!(
        parts("tuser@127.0.0.1:2222"),
        AddressParts {
            host: "127.0.0.1".into(),
            user: Some("tuser".into()),
            port: Some("2222".into()),
        }
    );
    assert_eq!(
        parts("ssh -p 2200 bob@box.internal"),
        AddressParts {
            host: "box.internal".into(),
            user: Some("bob".into()),
            port: Some("2200".into()),
        }
    );
    assert_eq!(parts("ssh bob@box -p2200").port.as_deref(), Some("2200"));
    assert_eq!(parts(" box.internal ").host, "box.internal");
    assert_eq!(parts("box.internal").user, None);
    assert_eq!(parts("[::1]:2222").host, "::1");
    assert_eq!(parts("[::1]:2222").port.as_deref(), Some("2222"));
    let v6 = parts("fe80::1");
    assert_eq!((v6.host.as_str(), v6.port), ("fe80::1", None));
    assert_eq!(parts("ssh://alice@h:2022").user.as_deref(), Some("alice"));
    assert_eq!(parts("ssh://alice@h:2022").host, "h");
}

// ------------------------------------------------------------ steps

#[test]
fn the_steps_are_address_sign_in_organise() {
    let labels: Vec<_> = STEPS.iter().map(|s| s.label()).collect();
    assert_eq!(labels, ["Address", "Sign in", "Organise"]);
    assert_eq!(AddHostStep::from_index(1), AddHostStep::Auth);
}

#[test]
fn address_step_holds_address_user_port_and_name_in_tab_order() {
    let mut form = open_form();
    assert_eq!(form.step(), AddHostStep::Target);
    assert_eq!(
        form.visible_fields(),
        vec![Field::Hostname, Field::Username, Field::Port, Field::Name]
    );
    assert_eq!(form.focused_field(), Field::Hostname);
    for expected in [Field::Username, Field::Port, Field::Name, Field::Hostname] {
        form.handle_input(FormInput::Next, "");
        assert_eq!(form.focused_field(), expected);
    }
    form.handle_input(FormInput::Previous, "");
    assert_eq!(form.focused_field(), Field::Name);
}

#[test]
fn sign_in_step_fields_follow_the_method() {
    let mut form = open_form();
    form.set_step(AddHostStep::Auth);
    assert_eq!(
        form.visible_fields(),
        vec![Field::AuthMethod, Field::Identity]
    );
    form.cycle_auth_method(1);
    assert_eq!(
        form.visible_fields(),
        vec![Field::AuthMethod, Field::Password]
    );
    form.cycle_auth_method(1);
    assert_eq!(form.auth_method(), "gssapi");
    assert_eq!(form.visible_fields(), vec![Field::AuthMethod]);
}

#[test]
fn organise_step_holds_group_tags_and_notes() {
    let mut form = open_form();
    form.set_step(AddHostStep::Details);
    assert_eq!(
        form.visible_fields(),
        vec![Field::Group, Field::Tags, Field::Notes]
    );
    assert_eq!(form.focused_field(), Field::Group);
}

#[test]
fn multi_step_advances_and_recedes() {
    let mut form = open_form();
    form.set_identities(vec![("k1".into(), "Key".into())]);
    assert!(!form.next_step());
    assert_eq!(form.error(), Some("Enter a hostname or IP"));
    assert_eq!(form.error_field(), Some(Field::Hostname));
    assert_eq!(form.step(), AddHostStep::Target);

    type_into(&mut form, "myhost.com");
    assert!(form.next_step());
    assert_eq!(form.step(), AddHostStep::Auth);
    assert_eq!(form.focused_field(), Field::AuthMethod);
    assert_eq!(form.error(), None);

    assert!(form.next_step());
    assert_eq!(form.step(), AddHostStep::Details);
    assert_eq!(form.focused_field(), Field::Group);
    assert!(!form.next_step(), "organise is the last step");

    assert!(form.prev_step());
    assert_eq!(form.step(), AddHostStep::Auth);
    assert!(form.prev_step());
    assert_eq!(form.step(), AddHostStep::Target);
    assert_eq!(form.focused_field(), Field::Hostname);
    assert!(!form.prev_step());
}

#[test]
fn missing_credentials_block_the_sign_in_step_on_the_right_field() {
    let mut form = open_form();
    type_into(&mut form, "box");
    form.next_step();
    assert!(!form.next_step());
    assert_eq!(form.error(), Some("Select an SSH key"));
    assert_eq!(form.error_field(), Some(Field::Identity));
    form.cycle_auth_method(1);
    assert!(!form.next_step());
    assert_eq!(form.error(), Some("Enter a password"));
    assert_eq!(form.error_field(), Some(Field::Password));
    form.cycle_auth_method(1);
    assert!(form.next_step(), "kerberos needs nothing else");
}

#[test]
fn keyboard_enter_advances_steps_and_submits_at_end() {
    let mut form = open_form();
    form.set_identities(vec![("k1".into(), "Key".into())]);
    type_into(&mut form, "vps.example.com");
    assert_eq!(
        form.handle_input(FormInput::Enter, ""),
        FormOutcome::Consumed
    );
    assert_eq!(form.step(), AddHostStep::Auth);
    assert_eq!(
        form.handle_input(FormInput::Enter, ""),
        FormOutcome::Consumed
    );
    assert_eq!(form.step(), AddHostStep::Details);
    assert_eq!(form.handle_input(FormInput::Enter, ""), FormOutcome::Submit);
}

#[test]
fn keyboard_escape_recedes_step_or_cancels() {
    let mut form = open_form();
    form.set_identities(vec![("k1".into(), "Key".into())]);
    type_into(&mut form, "vps.example.com");
    form.next_step();
    form.next_step();
    assert_eq!(form.step(), AddHostStep::Details);
    assert_eq!(
        form.handle_input(FormInput::Escape, ""),
        FormOutcome::Consumed
    );
    assert_eq!(form.step(), AddHostStep::Auth);
    assert_eq!(
        form.handle_input(FormInput::Escape, ""),
        FormOutcome::Consumed
    );
    assert_eq!(form.step(), AddHostStep::Target);
    assert_eq!(
        form.handle_input(FormInput::Escape, ""),
        FormOutcome::Cancel
    );
}

#[test]
fn escape_closes_an_open_select_before_leaving_the_step() {
    let mut form = open_form();
    form.set_identities(vec![("k1".into(), "Key".into())]);
    form.set_step(AddHostStep::Auth);
    form.focus_field(Field::Identity);
    form.toggle_identity_menu();
    assert!(form.identity_menu_open());
    assert_eq!(
        form.handle_input(FormInput::Escape, ""),
        FormOutcome::Consumed
    );
    assert!(!form.identity_menu_open());
    assert_eq!(form.step(), AddHostStep::Auth);
}

// ------------------------------------------------- address handling

#[test]
fn leaving_the_address_fills_empty_user_and_port() {
    let mut form = open_form();
    type_into(&mut form, "tuser@127.0.0.1:2222");
    form.handle_input(FormInput::Next, "");
    assert_eq!(form.value(Field::Hostname), "127.0.0.1");
    assert_eq!(form.value(Field::Username), "tuser");
    assert_eq!(form.value(Field::Port), "2222");
    assert_eq!(form.focused_field(), Field::Username);
}

#[test]
fn typed_user_and_port_win_over_the_address() {
    let mut form = open_form();
    form.focus_field(Field::Username);
    type_into(&mut form, "admin");
    form.focus_field(Field::Port);
    type_into(&mut form, "2022");
    form.focus_field(Field::Hostname);
    type_into(&mut form, "bob@box:2200");
    let values = form.values();
    assert_eq!(values.hostname, "box");
    assert_eq!(values.username, "admin");
    assert_eq!(values.port, "2022");
}

#[test]
fn the_port_starts_at_22_and_typing_replaces_it() {
    let mut form = open_form();
    assert_eq!(form.value(Field::Port), "22");
    assert_eq!(form.values().port, "22");
    form.focus_field(Field::Port);
    type_into(&mut form, "2222");
    assert_eq!(form.value(Field::Port), "2222");
    assert_eq!(form.cursor(Field::Port), 4);
}

#[test]
fn an_untouched_default_port_yields_to_the_address() {
    let mut form = open_form();
    type_into(&mut form, "box:2200");
    assert_eq!(form.values().port, "2200");
    form.handle_input(FormInput::Next, "");
    assert_eq!(form.value(Field::Port), "2200");
}

// --------------------------------------------------------- editing

#[test]
fn without_saved_keys_the_form_starts_on_password() {
    let mut form = AddHostForm::default();
    form.open();
    assert_eq!(form.auth_method(), "password");
    let mut with_keys = AddHostForm::default();
    with_keys.set_identities(vec![("id1".into(), "laptop".into())]);
    with_keys.open();
    assert_eq!(with_keys.auth_method(), "key");
}

#[test]
fn typing_lands_in_the_focused_field() {
    let mut form = open_form();
    type_into(&mut form, "example.com");
    assert_eq!(form.value(Field::Hostname), "example.com");
    assert_eq!(form.cursor(Field::Hostname), 11);
    form.handle_input(FormInput::Next, "");
    assert_eq!(form.focused_field(), Field::Username);
    type_into(&mut form, "deploy");
    assert_eq!(form.value(Field::Username), "deploy");
}

#[test]
fn tags_and_notes_are_text_fields() {
    let mut form = open_form();
    form.set_step(AddHostStep::Details);
    form.focus_field(Field::Tags);
    type_into(&mut form, "prod, web");
    form.focus_field(Field::Notes);
    type_into(&mut form, "Primary box");
    assert_eq!(form.value(Field::Tags), "prod, web");
    assert_eq!(form.value(Field::Notes), "Primary box");
    form.handle_input(FormInput::Left, "");
    assert!(form.backspace());
    assert_eq!(form.value(Field::Notes), "Primary bx");
    assert_eq!(form.values().tags, "prod, web");
    assert_eq!(form.values().notes, "Primary bx");
}

#[test]
fn backspace_edits_at_the_caret_not_the_tail() {
    let mut form = open_form();
    type_into(&mut form, "web-99");
    form.handle_input(FormInput::Left, "");
    form.handle_input(FormInput::Left, "");
    assert!(form.backspace());
    assert_eq!(form.value(Field::Hostname), "web99");
    assert_eq!(form.cursor(Field::Hostname), 3);
}

#[test]
fn backspace_at_the_beginning_is_not_an_edit() {
    let mut form = open_form();
    type_into(&mut form, "abc");
    form.handle_input(FormInput::Home, "");
    assert!(!form.backspace());
    assert_eq!(form.value(Field::Hostname), "abc");
}

#[test]
fn delete_removes_the_character_after_the_caret() {
    let mut form = open_form();
    type_into(&mut form, "abc");
    form.handle_input(FormInput::Home, "");
    assert!(form.delete());
    assert_eq!(form.value(Field::Hostname), "bc");
    form.cursor_end();
    assert!(!form.delete());
}

#[test]
fn insert_splices_at_the_caret() {
    let mut form = open_form();
    type_into(&mut form, "web01");
    form.handle_input(FormInput::Left, "");
    form.handle_input(FormInput::Left, "");
    assert!(form.insert("-"));
    assert_eq!(form.value(Field::Hostname), "web-01");
    assert_eq!(form.cursor(Field::Hostname), 4);
}

#[test]
fn carets_are_character_indices_not_byte_offsets() {
    let mut form = open_form();
    type_into(&mut form, "café");
    assert!(form.backspace());
    assert_eq!(form.value(Field::Hostname), "caf");
    form.handle_input(FormInput::Home, "");
    assert_eq!(form.cursor_byte(Field::Hostname), 0);
    assert!(form.insert("é"));
    assert_eq!(form.value(Field::Hostname), "écaf");
}

#[test]
fn split_at_cursor_bounds_the_painted_caret() {
    let mut form = open_form();
    type_into(&mut form, "web");
    form.handle_input(FormInput::Left, "");
    assert_eq!(form.split_at_cursor(), ("we", "b"));
}
