use super::*;

impl AddHostForm {
    /// Route one input. `text` is only read for [`FormInput::Text`].
    pub fn handle_input(&mut self, input: FormInput, text: &str) -> FormOutcome {
        use FormOutcome::{Cancel, Consumed, Submit};
        match input {
            FormInput::Text => {
                if text == " " && self.focused_field().is_select() {
                    match self.focused_field() {
                        Field::Identity => self.toggle_identity_menu(),
                        _ => self.toggle_group_menu(),
                    }
                } else {
                    let _ = self.insert(text);
                }
                Consumed
            }
            FormInput::Backspace => {
                self.backspace();
                Consumed
            }
            FormInput::Delete => {
                self.delete();
                Consumed
            }
            FormInput::Next => {
                self.focus_by(1);
                Consumed
            }
            FormInput::Previous => {
                self.focus_by(-1);
                Consumed
            }
            FormInput::Left | FormInput::Right => {
                let delta = if input == FormInput::Left { -1 } else { 1 };
                match self.focused_field() {
                    Field::AuthMethod => self.cycle_auth_method(delta),
                    Field::Identity => self.cycle_identity(delta),
                    Field::Group => self.cycle_group(delta),
                    _ => self.move_cursor(delta),
                }
                Consumed
            }
            FormInput::Home => {
                self.cursor_home();
                Consumed
            }
            FormInput::End => {
                self.cursor_end();
                Consumed
            }
            FormInput::Enter => {
                if let Some(menu) = self.menu {
                    // Enter with a list open settles the list, not the step.
                    if let Some(i) = self.menu_hover {
                        match menu {
                            SelectMenu::Identity => self.select_identity(i),
                            SelectMenu::Group => self.select_group(i),
                        }
                    } else {
                        self.close_menu();
                    }
                    return Consumed;
                }
                match self.step {
                    AddHostStep::Target | AddHostStep::Auth => {
                        let _ = self.next_step();
                        Consumed
                    }
                    AddHostStep::Details => Submit,
                }
            }
            FormInput::Escape => {
                if self.menu.is_some() {
                    self.close_menu();
                    Consumed
                } else if self.prev_step() {
                    Consumed
                } else {
                    Cancel
                }
            }
        }
    }
}
