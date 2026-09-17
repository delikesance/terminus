//! The add-snippet editor, using the generic DialogForm component.

use crate::dialog_form::{DialogFormLayout, DynamicFormHit, DynamicFormState};
use crate::text_field::TextMoveKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormInput {
    Text,
    Backspace { by_word: bool },
    Delete { by_word: bool },
    Next,
    Previous,
    Left { kind: TextMoveKind, by_word: bool },
    Right { kind: TextMoveKind, by_word: bool },
    Home { kind: TextMoveKind },
    End { kind: TextMoveKind },
    SelectAll,
    Enter,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormOutcome {
    Changed,
    Ignored,
    Cancel,
    Submit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddSnippetForm {
    pub inner: DynamicFormState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetFormValues {
    pub name: String,
    pub command: String,
    pub description: String,
}

impl Default for AddSnippetForm {
    fn default() -> Self {
        Self {
            inner: DynamicFormState::new("Configure New Snippet", "Save")
                .with_field("name", "Snippet Name", "")
                .with_field("command", "Command (e.g. docker ps)", "")
                .with_field("description", "Description", ""),
        }
    }
}

impl AddSnippetForm {
    pub fn is_open(&self) -> bool {
        self.inner.is_open()
    }

    pub fn values(&self) -> SnippetFormValues {
        SnippetFormValues {
            name: self.inner.get_value("name").unwrap_or("").to_string(),
            command: self.inner.get_value("command").unwrap_or("").to_string(),
            description: self.inner.get_value("description").unwrap_or("").to_string(),
        }
    }

    pub fn handle_input(&mut self, input: FormInput, text: &str) -> FormOutcome {
        match input {
            FormInput::Text => {
                if self.inner.insert_text(text) {
                    FormOutcome::Changed
                } else {
                    FormOutcome::Ignored
                }
            }
            FormInput::Backspace { by_word } => {
                if self.inner.backspace(by_word) {
                    FormOutcome::Changed
                } else {
                    FormOutcome::Ignored
                }
            }
            FormInput::Delete { by_word } => {
                if self.inner.delete_forward(by_word) {
                    FormOutcome::Changed
                } else {
                    FormOutcome::Ignored
                }
            }
            FormInput::Left { kind, by_word } => {
                if self.inner.move_left(kind, by_word) {
                    FormOutcome::Changed
                } else {
                    FormOutcome::Ignored
                }
            }
            FormInput::Right { kind, by_word } => {
                if self.inner.move_right(kind, by_word) {
                    FormOutcome::Changed
                } else {
                    FormOutcome::Ignored
                }
            }
            FormInput::Home { kind } => {
                if self.inner.move_home(kind) {
                    FormOutcome::Changed
                } else {
                    FormOutcome::Ignored
                }
            }
            FormInput::End { kind } => {
                if self.inner.move_end(kind) {
                    FormOutcome::Changed
                } else {
                    FormOutcome::Ignored
                }
            }
            FormInput::SelectAll => {
                if self.inner.select_all() {
                    FormOutcome::Changed
                } else {
                    FormOutcome::Ignored
                }
            }
            FormInput::Next => {
                self.inner.cycle_focus(false);
                FormOutcome::Changed
            }
            FormInput::Previous => {
                self.inner.cycle_focus(true);
                FormOutcome::Changed
            }
            FormInput::Enter => FormOutcome::Submit,
            FormInput::Escape => FormOutcome::Cancel,
        }
    }
}

pub type AddSnippetLayout = DialogFormLayout;
pub type AddSnippetHit = DynamicFormHit;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_and_arrows_edit_command_field() {
        let mut form = AddSnippetForm::default();
        form.inner.focused_index = 1; // command
        assert_eq!(
            form.handle_input(FormInput::Text, "docker"),
            FormOutcome::Changed
        );
        assert_eq!(
            form.handle_input(FormInput::Text, " "),
            FormOutcome::Changed
        );
        assert_eq!(
            form.handle_input(FormInput::Text, "ps"),
            FormOutcome::Changed
        );
        assert_eq!(form.inner.get_value("command"), Some("docker ps"));
        assert_eq!(
            form.handle_input(
                FormInput::Left {
                    kind: TextMoveKind::Collapse,
                    by_word: true
                },
                ""
            ),
            FormOutcome::Changed
        );
        assert_eq!(form.inner.focused_draft().unwrap().caret, 7);
        assert_eq!(
            form.handle_input(FormInput::SelectAll, ""),
            FormOutcome::Changed
        );
        assert_eq!(
            form.inner.focused_draft().unwrap().selection_range(),
            Some((0, 9))
        );
    }
}
