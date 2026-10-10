use crate::components::input::{self as inp, FieldKind};
use crate::components::overlay as ov;

/// The step of the add/edit host wizard: Address, Sign in, Organise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddHostStep {
    /// Address, user, port and name.
    #[default]
    Target,
    /// Sign-in method and its credential.
    Auth,
    /// Group, tags and notes.
    Details,
}

pub const STEPS: [AddHostStep; 3] =
    [AddHostStep::Target, AddHostStep::Auth, AddHostStep::Details];

impl AddHostStep {
    /// Stepper label (the Overlays stepper's own copy).
    pub const fn label(self) -> &'static str {
        ov::STEPPER_STEPS[self.index()]
    }

    pub const fn index(self) -> usize {
        match self {
            AddHostStep::Target => 0,
            AddHostStep::Auth => 1,
            AddHostStep::Details => 2,
        }
    }

    pub const fn from_index(index: usize) -> Self {
        match index {
            0 => AddHostStep::Target,
            1 => AddHostStep::Auth,
            _ => AddHostStep::Details,
        }
    }

    /// The field that takes focus when the step is entered.
    pub const fn first_field(self) -> Field {
        match self {
            AddHostStep::Target => Field::Hostname,
            AddHostStep::Auth => Field::AuthMethod,
            AddHostStep::Details => Field::Group,
        }
    }
}

/// The fields, in tab order within their step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Hostname,
    Username,
    Port,
    /// The three sign-in choice cards (one focus stop).
    AuthMethod,
    Identity,
    Password,
    Group,
    Tags,
    Notes,
}

/// Text fields stored in the form's value array (name, address, user,
/// port); the Organise text fields follow at indexes 4 and 5.
pub const BASE_FIELDS: [Field; 4] =
    [Field::Name, Field::Hostname, Field::Username, Field::Port];

/// Back-compat alias: base text fields only.
pub const FIELDS: [Field; 4] = BASE_FIELDS;

/// Slots in the value array.
pub(super) const TEXT_SLOTS: usize = 6;

impl Field {
    pub const fn label(self) -> &'static str {
        match self {
            Field::Name => "Name",
            Field::Hostname => "Address",
            Field::Username => "User",
            Field::Port => "Port",
            Field::AuthMethod => "Sign in with",
            Field::Identity => "Key",
            Field::Password => "Password",
            Field::Group => "Group",
            Field::Tags => "Tags",
            Field::Notes => "Notes",
        }
    }

    pub const fn placeholder(self) -> &'static str {
        match self {
            Field::Name => "Shown in your list",
            Field::Hostname => "137.74.42.224 or server.example.com",
            Field::Username => "ubuntu",
            Field::Port => "22",
            Field::AuthMethod => "",
            Field::Identity => "No SSH keys saved",
            Field::Password => "Enter your password",
            Field::Group => "No group",
            Field::Tags => "production, web",
            Field::Notes => "Anything you want to remember about this server",
        }
    }

    pub const fn step(self) -> AddHostStep {
        match self {
            Field::Hostname | Field::Username | Field::Port | Field::Name => {
                AddHostStep::Target
            }
            Field::AuthMethod | Field::Identity | Field::Password => AddHostStep::Auth,
            Field::Group | Field::Tags | Field::Notes => AddHostStep::Details,
        }
    }

    /// Which input component draws the field.
    pub const fn kind(self) -> FieldKind {
        match self {
            Field::Port => FieldKind::Mono,
            Field::Password => FieldKind::Password,
            Field::Identity | Field::Group => FieldKind::Select,
            Field::Notes => FieldKind::Textarea,
            _ => FieldKind::Text,
        }
    }

    /// Slot in the form's value array, for the fields it stores.
    pub const fn base_index(self) -> Option<usize> {
        match self {
            Field::Name => Some(0),
            Field::Hostname => Some(1),
            Field::Username => Some(2),
            Field::Port => Some(3),
            Field::Tags => Some(4),
            Field::Notes => Some(5),
            Field::AuthMethod | Field::Identity | Field::Password | Field::Group => None,
        }
    }

    pub const fn is_text(self) -> bool {
        matches!(
            self,
            Field::Name
                | Field::Hostname
                | Field::Username
                | Field::Port
                | Field::Password
                | Field::Tags
                | Field::Notes
        )
    }

    pub const fn is_select(self) -> bool {
        matches!(self, Field::Identity | Field::Group)
    }

    /// Legacy index into the value array (fields it does not store map to 0).
    pub const fn index(self) -> usize {
        match self.base_index() {
            Some(i) => i,
            None => 0,
        }
    }

    pub const fn from_index(index: usize) -> Self {
        BASE_FIELDS[index % BASE_FIELDS.len()]
    }

    /// The password field carries the vault note under its box.
    pub(super) const fn has_helper(self) -> bool {
        matches!(self, Field::Password)
    }

    pub(super) fn row_height(self) -> f32 {
        let mut h = inp::LABEL_HEIGHT + inp::LABEL_GAP + inp::field_height(self.kind());
        if self.has_helper() {
            h += inp::LABEL_GAP + inp::HELPER_HEIGHT;
        }
        h
    }
}
