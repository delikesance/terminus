use super::metrics::*;
use super::steps::{AddHostStep, Field};
use crate::components::input as inp;
use crate::components::overlay as ov;
use crate::components::selection as sel;

/// What a mouse press on the open add-host dialog hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddHostHit {
    /// An input box: focus that field.
    Field(Field),
    /// A stepper segment: switch step.
    StepPill(AddHostStep),
    /// A sign-in choice card.
    SelectAuth(usize),
    /// Open / close the Key select.
    ToggleIdentityMenu,
    /// Pick a managed key from the open select.
    SelectIdentity(usize),
    /// Open / close the Group select.
    ToggleGroupMenu,
    /// Pick a group from the open select (0 is "No group").
    SelectGroup(usize),
    /// Eye toggle on the password field.
    TogglePasswordVisible,
    /// "Generate one" under the key select.
    GenerateKey,
    /// The × in the header (same as Cancel).
    Close,
    /// Dismiss without saving.
    Cancel,
    /// Go back to the previous step.
    Back,
    /// "Copy" beside the footer error: put the full message on the clipboard.
    CopyError,
    /// Continue to the next step.
    Next,
    /// Persist the draft (same as Enter on the last step).
    Connect,
    /// Dialog chrome / padding: swallow, keep open.
    Consume,
}

/// What the form collected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFormValues {
    pub name: String,
    pub hostname: String,
    pub username: String,
    pub port: String,
    pub auth_method: String,
    pub identity_id: Option<String>,
    pub password: String,
    pub group_id: Option<String>,
    /// Comma separated, as typed.
    pub tags: String,
    pub notes: String,
}

impl Default for HostFormValues {
    fn default() -> Self {
        Self {
            name: String::new(),
            hostname: String::new(),
            username: String::new(),
            port: String::new(),
            auth_method: "key".to_string(),
            identity_id: None,
            password: String::new(),
            group_id: None,
            tags: String::new(),
            notes: String::new(),
        }
    }
}

/// One keyboard input, platform-neutral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormInput {
    /// Committed text: a single keystroke's characters, or an IME commit.
    Text,
    Backspace,
    Delete,
    /// Tab, or the down arrow: next field.
    Next,
    /// Shift+Tab, or the up arrow: previous field.
    Previous,
    Left,
    Right,
    Home,
    End,
    Enter,
    Escape,
}

/// What the caller must do after an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormOutcome {
    /// The form swallowed the input; repaint if the caret moved.
    Consumed,
    /// Dismiss without saving.
    Cancel,
    /// Save, then close only if the repository accepts the values.
    Submit,
}

/// Which select has its list open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMenu {
    Identity,
    Group,
}

/// One row of the dialog body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Single(Field),
    /// Two fields sharing a row, two thirds and one third.
    Pair(Field, Field),
    /// "Sign in with" and its three cards.
    Choices,
    /// "No key yet? Generate one ..." under the key select.
    KeyHint,
    /// One line about Kerberos.
    KerberosHint,
}

impl Row {
    pub fn height(self) -> f32 {
        match self {
            Row::Single(f) | Row::Pair(f, _) => f.row_height(),
            Row::Choices => inp::LABEL_HEIGHT + inp::LABEL_GAP + sel::CHOICE_HEIGHT,
            Row::KeyHint | Row::KerberosHint => HINT_LINE,
        }
    }
}

/// Body rows of `step` for an auth method.
pub fn rows_for(step: AddHostStep, auth_method: &str) -> Vec<Row> {
    match step {
        AddHostStep::Target => vec![
            Row::Single(Field::Hostname),
            Row::Pair(Field::Username, Field::Port),
            Row::Single(Field::Name),
        ],
        AddHostStep::Auth => {
            let mut rows = vec![Row::Choices];
            match auth_method {
                "password" => rows.push(Row::Single(Field::Password)),
                "gssapi" => rows.push(Row::KerberosHint),
                _ => {
                    rows.push(Row::Single(Field::Identity));
                    rows.push(Row::KeyHint);
                }
            }
            rows
        }
        AddHostStep::Details => vec![
            Row::Single(Field::Group),
            Row::Single(Field::Tags),
            Row::Single(Field::Notes),
        ],
    }
}

pub(super) fn dialog_height_for(rows: &[Row]) -> f32 {
    let body: f32 = rows.iter().map(|r| r.height()).sum::<f32>()
        + ROW_GAP * rows.len().saturating_sub(1) as f32;
    HEADER_TOP
        + CLOSE_SIZE
        + STEPPER_TOP
        + ov::STEPPER_HEIGHT
        + BODY_PAD
        + body
        + BODY_PAD
        + FOOTER_TOP
        + BUTTON_HEIGHT
        + FOOTER_BOTTOM
}

/// Classify a repository / probe message by the field it is about.
///
/// Only the messages the repository is known to raise for one field are
/// matched, so a connection failure never gets pinned on an input.
pub(super) fn classify_error(message: &str) -> Option<Field> {
    let m = message.trim().to_ascii_lowercase();
    if m.starts_with("hostname is required") || m.starts_with("enter a hostname") {
        Some(Field::Hostname)
    } else if m.ends_with("is not a valid port") {
        Some(Field::Port)
    } else if m.starts_with("select an ssh key")
        || m.starts_with("select a saved ssh key")
    {
        Some(Field::Identity)
    } else if m.starts_with("password is required") || m.starts_with("enter a password") {
        Some(Field::Password)
    } else if m.starts_with("unknown authentication method") {
        Some(Field::AuthMethod)
    } else {
        None
    }
}
