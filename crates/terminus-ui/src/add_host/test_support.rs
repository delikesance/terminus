use super::*;

pub(super) fn open_form() -> AddHostForm {
    let mut form = AddHostForm::default();
    form.open();
    form.select_auth_method(0);
    form
}

pub(super) fn type_into(form: &mut AddHostForm, text: &str) {
    for ch in text.chars() {
        let buf = ch.to_string();
        form.handle_input(FormInput::Text, &buf);
    }
}

pub(super) fn layout_for(form: &AddHostForm) -> AddHostLayout {
    AddHostLayout::centered(1440.0, 900.0, form.anchor_height())
}
