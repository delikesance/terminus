use super::test_support::*;
use super::*;
use crate::geom::Rect;

#[test]
fn heights_follow_the_mock_rows() {
    let mut form = open_form();
    // 24 + 36 + 18 + 27 + 26 | 3 x 70 + 2 x 18 | 26 + 16 + 44 + 24
    assert_eq!(form.height(), 131.0 + 246.0 + 110.0);
    form.set_step(AddHostStep::Auth);
    let key = form.height();
    form.cycle_auth_method(1);
    let password = form.height();
    form.cycle_auth_method(1);
    let kerberos = form.height();
    assert!(kerberos < password && password < key);
    form.set_step(AddHostStep::Details);
    assert_eq!(form.height(), 131.0 + (70.0 + 70.0 + 116.0 + 36.0) + 110.0);
    for method in 0..3 {
        for step in STEPS {
            form.select_auth_method(method);
            form.set_step(step);
            assert!(form.height() <= form.anchor_height());
        }
    }
}

#[test]
fn the_dialog_top_stays_put_between_steps() {
    let mut form = open_form();
    let top = layout_for(&form).rect(form.height()).y;
    for step in STEPS {
        form.set_step(step);
        assert_eq!(layout_for(&form).rect(form.height()).y, top);
    }
}

#[test]
fn the_dialog_is_centered_horizontally_and_matches_the_mock_width() {
    let form = open_form();
    let layout = layout_for(&form);
    let dialog = layout.rect(form.height());
    assert_eq!(dialog.width, 560.0);
    assert!((dialog.x - (1440.0 - 560.0) / 2.0).abs() < f32::EPSILON);
}

#[test]
fn a_window_smaller_than_the_dialog_does_not_go_negative() {
    let form = open_form();
    let layout = AddHostLayout::centered(100.0, 60.0, form.anchor_height());
    assert_eq!((layout.x, layout.y), (0.0, 0.0));
}

#[test]
fn header_stepper_and_body_stack_without_overlap() {
    let form = open_form();
    let layout = layout_for(&form);
    let dialog = layout.rect(form.height());
    let title = layout.title_rect();
    let close = layout.close_button_rect();
    let stepper = layout.stepper_rect();
    assert_eq!(title.x, dialog.x + 28.0);
    assert_eq!(title.y, dialog.y + 24.0);
    assert!(close.right() <= dialog.right() - 24.0 + 0.01);
    assert_eq!((close.width, close.height), (36.0, 36.0));
    assert!(title.right() <= close.x);
    assert!(stepper.y >= title.bottom());
    let first = layout
        .field_layout(&form, Field::Hostname)
        .unwrap()
        .label
        .unwrap();
    assert!(first.y >= stepper.bottom());
}

#[test]
fn rows_do_not_overlap_and_stay_inside_the_dialog_on_every_step() {
    let mut form = open_form();
    for method in 0..3 {
        for step in STEPS {
            form.select_auth_method(method);
            form.set_step(step);
            let layout = layout_for(&form);
            let dialog = layout.rect(form.height());
            let fields = form.visible_fields();
            let mut boxes: Vec<Rect> = Vec::new();
            for &field in &fields {
                let Some(b) = layout.input_rect(&form, field) else {
                    continue; // the cards are laid out separately
                };
                assert!(b.x >= dialog.x && b.right() <= dialog.right());
                assert!(b.y >= dialog.y && b.bottom() <= dialog.bottom());
                boxes.push(b);
            }
            for (i, a) in boxes.iter().enumerate() {
                for b in boxes.iter().skip(i + 1) {
                    assert!(
                        !crate::overlap::rects_overlap(*a, *b),
                        "{step:?}/{method}: {a:?} overlaps {b:?}"
                    );
                }
            }
            let footer = layout.footer_text_rect(&form);
            let buttons = [
                layout.secondary_button_rect(&form),
                layout.primary_button_rect(&form),
            ];
            for b in &boxes {
                assert!(b.bottom() < footer.y, "{step:?}: field reaches footer");
            }
            for button in buttons {
                assert!(!crate::overlap::rects_overlap(button, footer));
                assert_eq!(button.height, 44.0);
                assert_eq!(button.bottom(), dialog.bottom() - 24.0);
                assert!(button.right() <= dialog.right() - 28.0 + 0.01);
            }
        }
    }
}

#[test]
fn user_and_port_share_a_row_in_two_thirds_one_third() {
    let form = open_form();
    let layout = layout_for(&form);
    let user = layout.input_rect(&form, Field::Username).unwrap();
    let port = layout.input_rect(&form, Field::Port).unwrap();
    let address = layout.input_rect(&form, Field::Hostname).unwrap();
    assert_eq!(user.y, port.y);
    assert_eq!(user.x, address.x);
    assert_eq!(port.right(), address.right());
    assert!((port.x - user.right() - 12.0).abs() < 0.01);
    assert!((user.width - 2.0 * port.width - 12.0).abs() < 0.01);
    assert_eq!(user.height, 46.0);
}

#[test]
fn the_notes_box_is_a_textarea() {
    let mut form = open_form();
    form.set_step(AddHostStep::Details);
    let layout = layout_for(&form);
    let notes = layout.input_rect(&form, Field::Notes).unwrap();
    assert_eq!(notes.height, 92.0);
    let tags = layout.input_rect(&form, Field::Tags).unwrap();
    assert_eq!(tags.height, 46.0);
}

#[test]
fn three_choice_cards_sit_in_one_row_and_select_on_click() {
    let mut form = open_form();
    form.set_step(AddHostStep::Auth);
    let layout = layout_for(&form);
    let cards = layout.choice_rects(&form).expect("cards on sign in");
    assert_eq!(cards.len(), 3);
    assert!(cards.iter().all(|c| c.height == 68.0 && c.y == cards[0].y));
    assert!((cards[1].x - cards[0].right() - 8.0).abs() < 0.01);
    for (i, c) in cards.iter().enumerate() {
        assert_eq!(
            layout.hit_test(&form, c.x + 4.0, c.y + 4.0),
            AddHostHit::SelectAuth(i)
        );
    }
    let mut address = open_form();
    address.set_step(AddHostStep::Target);
    assert!(layout_for(&address).choice_rects(&address).is_none());
}

#[test]
fn key_select_hint_link_and_vault_note() {
    let mut form = open_form();
    form.set_identities(vec![
        ("k1".into(), "Prod".into()),
        ("k2".into(), "Staging".into()),
    ]);
    form.set_step(AddHostStep::Auth);
    let layout = layout_for(&form);
    let key = layout.input_rect(&form, Field::Identity).unwrap();
    assert_eq!(
        layout.hit_test(&form, key.x + 2.0, key.y + 2.0),
        AddHostHit::ToggleIdentityMenu
    );
    let link = layout
        .generate_link_rect(&form)
        .expect("link under the key");
    assert!(link.y >= key.bottom());
    assert_eq!(
        layout.hit_test(&form, link.x + 2.0, link.y + 2.0),
        AddHostHit::GenerateKey
    );
    form.toggle_identity_menu();
    assert!(form.identity_menu_open());
    let opt = layout.menu_option_rect(&form, 1).unwrap();
    assert_eq!(
        layout.hit_test(&form, opt.x + 2.0, opt.y + 2.0),
        AddHostHit::SelectIdentity(1)
    );
    form.select_identity(1);
    assert_eq!(form.identity_id(), Some("k2"));
    assert!(!form.identity_menu_open());

    form.select_auth_method(1);
    let layout = layout_for(&form);
    assert!(layout.generate_link_rect(&form).is_none());
    assert!(layout.input_rect(&form, Field::Identity).is_none());
    let pw = layout.field_layout(&form, Field::Password).unwrap();
    assert!(pw.helper.is_some(), "vault note under the password");
    assert!(pw.trailing.is_some());
}

#[test]
fn group_select_lists_no_group_first() {
    let mut form = open_form();
    form.set_groups(vec![("g1".into(), "jeremy".into())]);
    form.set_step(AddHostStep::Details);
    let layout = layout_for(&form);
    let group = layout.input_rect(&form, Field::Group).unwrap();
    assert_eq!(
        layout.hit_test(&form, group.x + 2.0, group.y + 2.0),
        AddHostHit::ToggleGroupMenu
    );
    form.toggle_group_menu();
    assert!(form.group_menu_open() && !form.identity_menu_open());
    let menu = layout.menu_rect(&form).unwrap();
    assert!(menu.y >= group.bottom());
    let second = layout.menu_option_rect(&form, 1).unwrap();
    assert_eq!(
        layout.hit_test(&form, second.x + 2.0, second.y + 2.0),
        AddHostHit::SelectGroup(1)
    );
    assert!(layout.menu_option_rect(&form, 2).is_none());
    form.select_group(1);
    assert_eq!(form.group_id(), Some("g1"));
    form.toggle_group_menu();
    form.select_group(0);
    assert_eq!(form.group_id(), None);
}

#[test]
fn password_eye_toggles_visibility() {
    let mut form = open_form();
    form.set_step(AddHostStep::Auth);
    form.cycle_auth_method(1);
    let layout = layout_for(&form);
    let eye = layout.password_toggle_rect(&form).expect("eye slot");
    assert_eq!(
        layout.hit_test(&form, eye.x + 2.0, eye.y + 2.0),
        AddHostHit::TogglePasswordVisible
    );
    let pw = layout.input_rect(&form, Field::Password).unwrap();
    assert_eq!(
        layout.hit_test(&form, pw.x + 2.0, pw.y + 2.0),
        AddHostHit::Field(Field::Password)
    );
    assert!(!form.password_visible());
    form.toggle_password_visible();
    assert!(form.password_visible());
}

#[test]
fn footer_buttons_follow_the_step() {
    let mut form = open_form();
    let layout = layout_for(&form);
    let hit = |form: &AddHostForm, r: Rect| {
        layout_for(form).hit_test(form, r.x + 3.0, r.y + 3.0)
    };
    let _ = layout;
    assert_eq!(form.primary_label(), "Continue");
    assert_eq!(form.secondary_label(), "Cancel");
    let l = layout_for(&form);
    assert_eq!(
        hit(&form, l.secondary_button_rect(&form)),
        AddHostHit::Cancel
    );
    assert_eq!(hit(&form, l.primary_button_rect(&form)), AddHostHit::Next);

    form.set_step(AddHostStep::Auth);
    let l = layout_for(&form);
    assert_eq!(form.secondary_label(), "Back");
    assert_eq!(hit(&form, l.secondary_button_rect(&form)), AddHostHit::Back);
    assert_eq!(hit(&form, l.primary_button_rect(&form)), AddHostHit::Next);

    form.set_step(AddHostStep::Details);
    let l = layout_for(&form);
    assert_eq!(form.primary_label(), "Save server");
    assert_eq!(
        hit(&form, l.primary_button_rect(&form)),
        AddHostHit::Connect
    );
    assert_eq!(form.step_hint(), "Step 3 of 3");
    form.set_step(AddHostStep::Target);
    assert_eq!(form.step_hint(), "Step 1 of 3");
}

#[test]
fn close_button_and_stepper_and_chrome_hits() {
    let form = open_form();
    let layout = layout_for(&form);
    let close = layout.close_button_rect();
    assert_eq!(
        layout.hit_test(&form, close.x + 2.0, close.y + 2.0),
        AddHostHit::Close
    );
    for step in STEPS {
        let seg = layout.step_rect(step);
        assert_eq!(
            layout.hit_test(&form, seg.x + 2.0, seg.y + 2.0),
            AddHostHit::StepPill(step)
        );
    }
    let title = layout.title_rect();
    assert_eq!(
        layout.hit_test(&form, title.x + 2.0, title.y + 2.0),
        AddHostHit::Consume
    );
    assert_eq!(layout.hit_test(&form, 1.0, 1.0), AddHostHit::Consume);
    let host = layout.input_rect(&form, Field::Hostname).unwrap();
    assert_eq!(
        layout.hit_test(&form, host.x + 2.0, host.y + 2.0),
        AddHostHit::Field(Field::Hostname)
    );
}

#[test]
fn hover_is_tracked_once_per_change() {
    let mut form = open_form();
    assert!(form.set_hover(Some(AddHostHit::Next)));
    assert!(!form.set_hover(Some(AddHostHit::Next)));
    assert_eq!(form.hover(), Some(AddHostHit::Next));
    assert!(form.set_hover(None));
}

#[test]
fn password_field_accepts_pasted_secret() {
    let mut form = open_form();
    form.set_step(AddHostStep::Auth);
    form.cycle_auth_method(1);
    form.focus_field(Field::Password);
    assert!(form.shows_password());
    assert!(!form.insert("secret\n"));
    assert!(form.insert("s3cret-from-clipboard"));
    assert_eq!(form.password(), "s3cret-from-clipboard");
    let cleaned: String = "p@ss\r\nw0rd\u{7f}"
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    assert_eq!(cleaned, "p@ssw0rd");
}
