//! The add-host editor: a three step wizard (Address, Sign in, Organise)
//! with one focused field.
//!
//! The form owns the text, the focus and the caret, and nothing else —
//! no storage. Submitting hands the raw values to the host repository,
//! which is the single place that decides whether a host is storable; a
//! rejection comes back as [`AddHostForm::set_error`] so the form stays
//! open with its text intact.
//!
//! Auth methods are plain strings (`"key"` | `"password"` | `"gssapi"`)
//! so this crate stays independent of terminus-core's typed enum.

mod address;
mod editing;
mod fields;
mod form;
mod input;
mod layout;
mod metrics;
mod steps;
mod types;

#[cfg(test)]
mod form_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod steps_tests;
#[cfg(test)]
mod test_support;

pub use address::*;
pub use layout::*;
pub use metrics::*;
pub use steps::*;
pub use types::*;

use crate::components::input::TextDraft;

/// The add-host editor's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddHostForm {
    open: bool,
    /// Current wizard step.
    step: AddHostStep,
    /// When set, the dialog updates this host instead of creating one.
    editing_id: Option<String>,
    /// Name, address, user, port, tags, notes.
    values: [TextDraft; TEXT_SLOTS],
    /// The port has been typed (or came from the address), so the pristine
    /// "22" default no longer yields to the address.
    port_dirty: bool,
    auth_method: String,
    identity_id: Option<String>,
    identities: Vec<(String, String)>,
    group_id: Option<String>,
    groups: Vec<(String, String)>,
    password: TextDraft,
    /// Whether the password field shows plaintext (eye toggle).
    password_visible: bool,
    focus: Field,
    error: Option<String>,
    /// The input the error is about, when it is about one.
    error_field: Option<Field>,
    /// Select whose list is expanded.
    menu: Option<SelectMenu>,
    /// Hovered option in the open list.
    menu_hover: Option<usize>,
    /// What the pointer rests on, for hover states.
    hover: Option<AddHostHit>,
}

impl Default for AddHostForm {
    fn default() -> Self {
        let mut values: [TextDraft; TEXT_SLOTS] = Default::default();
        values[3] = TextDraft::new("22");
        Self {
            open: false,
            step: AddHostStep::Target,
            editing_id: None,
            values,
            port_dirty: false,
            auth_method: "key".to_string(),
            identity_id: None,
            identities: Vec::new(),
            group_id: None,
            groups: Vec::new(),
            password: TextDraft::default(),
            password_visible: false,
            focus: Field::Hostname,
            error: None,
            error_field: None,
            menu: None,
            menu_hover: None,
            hover: None,
        }
    }
}
