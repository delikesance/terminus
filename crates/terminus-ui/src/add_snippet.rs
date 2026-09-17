//! The add-snippet editor, using the generic DialogForm component.

use crate::dialog_form::{DynamicFormState, FormFieldData, DialogFormLayout, DynamicFormHit};
use crate::geom::Rect;

#[derive(PartialEq)]
pub enum FormInput {
    Text,
    Backspace,
    Delete,
    Next,
    Previous,
    Left,
    Right,
    Home,
    End,
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
                if !text.is_empty() {
                    self.inner.handle_key(text);
                    return FormOutcome::Changed;
                }
            }
            FormInput::Backspace => {
                self.inner.handle_backspace();
                return FormOutcome::Changed;
            }
            FormInput::Delete => {
                self.inner.handle_delete();
                return FormOutcome::Changed;
            }
            FormInput::Left => {
                self.inner.move_caret(-1);
                return FormOutcome::Changed;
            }
            FormInput::Right => {
                self.inner.move_caret(1);
                return FormOutcome::Changed;
            }
            FormInput::Home => {
                if let Some(d) = self.inner.focused_draft_mut() {
                    d.caret = 0;
                }
                return FormOutcome::Changed;
            }
            FormInput::End => {
                if let Some(d) = self.inner.focused_draft_mut() {
                    d.caret = d.value.len();
                }
                return FormOutcome::Changed;
            }
            FormInput::Next => {
                self.inner.cycle_focus(false);
                return FormOutcome::Changed;
            }
            FormInput::Previous => {
                self.inner.cycle_focus(true);
                return FormOutcome::Changed;
            }
            FormInput::Enter => {
                return FormOutcome::Submit;
            }
            FormInput::Escape => {
                return FormOutcome::Cancel;
            }
        }
        FormOutcome::Ignored
    }
}

pub type AddSnippetLayout = DialogFormLayout;
pub type AddSnippetHit = DynamicFormHit;
