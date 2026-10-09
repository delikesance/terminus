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

use crate::components::input::{self as inp, FieldKind, FieldLayout};
use crate::components::overlay as ov;
use crate::components::selection as sel;
use crate::geom::Rect;
use crate::text_field::{TextDraft, TextEdit, TextMoveKind};

/// Canonical auth-method wire values, in cycle order.
pub const AUTH_METHODS: [&str; 3] = ["key", "password", "gssapi"];

/// Human label for an auth-method wire value.
pub fn auth_method_label(method: &str) -> &'static str {
    match method {
        "password" => "Password Authentication",
        "gssapi" => "Kerberos (GSSAPI)",
        _ => "SSH Cryptographic Key",
    }
}

/// An address as people paste it: `user@host:port`, `ssh -p 2222
/// user@host`, `ssh://user@host:port` or `[::1]:22`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddressParts {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<String>,
}

pub fn split_address(input: &str) -> AddressParts {
    let mut words = input.split_whitespace().peekable();
    if words.peek() == Some(&"ssh") {
        words.next();
    }
    let mut port = None;
    let mut target = None;
    while let Some(word) = words.next() {
        if word == "-p" {
            port = words.next().map(str::to_string);
        } else if let Some(p) = word.strip_prefix("-p").filter(|p| !p.is_empty()) {
            port = Some(p.to_string());
        } else if target.is_none() && !word.starts_with('-') {
            target = Some(word);
        }
    }
    let mut rest = target.unwrap_or("").trim();
    rest = rest.strip_prefix("ssh://").unwrap_or(rest);
    rest = rest.trim_end_matches('/');
    let mut user = None;
    if let Some((u, h)) = rest.rsplit_once('@') {
        if !u.is_empty() {
            user = Some(u.to_string());
        }
        rest = h;
    }
    let host = if let Some(inner) = rest.strip_prefix('[') {
        // [v6]:port
        match inner.split_once(']') {
            Some((h, tail)) => {
                if let Some(p) = tail.strip_prefix(':').filter(|p| !p.is_empty()) {
                    port = Some(p.to_string());
                }
                h
            }
            None => inner,
        }
    } else {
        match rest.split_once(':') {
            // Exactly one colon: host:port. More is a bare IPv6 address.
            Some((h, p)) if !p.contains(':') && !p.is_empty() => {
                port = Some(p.to_string());
                h
            }
            _ => rest,
        }
    };
    AddressParts {
        host: host.to_string(),
        user,
        port,
    }
}

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
const TEXT_SLOTS: usize = 6;

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
    const fn has_helper(self) -> bool {
        matches!(self, Field::Password)
    }

    fn row_height(self) -> f32 {
        let mut h = inp::LABEL_HEIGHT + inp::LABEL_GAP + inp::field_height(self.kind());
        if self.has_helper() {
            h += inp::LABEL_GAP + inp::HELPER_HEIGHT;
        }
        h
    }
}

/// Dialog metrics, in logical pixels (the mock's 560 wide dialog).
pub const WIDTH: f32 = 560.0;
pub const PAD_X: f32 = 28.0;
pub const HEADER_TOP: f32 = 24.0;
pub const CLOSE_SIZE: f32 = 36.0;
pub const CLOSE_RIGHT: f32 = 24.0;
pub const STEPPER_TOP: f32 = 18.0;
pub const BODY_PAD: f32 = 26.0;
pub const ROW_GAP: f32 = 18.0;
pub const PAIR_GAP: f32 = 12.0;
pub const CARD_GAP: f32 = 8.0;
pub const FOOTER_TOP: f32 = 16.0;
pub const FOOTER_BOTTOM: f32 = 24.0;
pub const BUTTON_HEIGHT: f32 = 44.0;
pub const BUTTON_GAP: f32 = 10.0;
/// Gap between the footer hint and the buttons.
pub const FOOTER_TEXT_GAP: f32 = 16.0;
/// One 13px line of hint text under the key select.
pub const HINT_LINE: f32 = 18.0;
pub const MENU_ROW: f32 = 36.0;
pub const MENU_PAD: f32 = 4.0;
/// Title size (Sora SemiBold, violet revision).
pub const TITLE_FONT: f32 = 24.0;
/// Dialog corner radius.
pub const DIALOG_RADIUS: f32 = crate::tokens::radius::DIALOG;

/// Measured label widths (Sora 14, Medium for Cancel/Back, SemiBold for
/// the primary): buttons are `2 * 20` wider than their label.
pub const LABEL_CANCEL: f32 = 47.0;
pub const LABEL_BACK: f32 = 33.0;
pub const LABEL_CONTINUE: f32 = 67.0;
pub const LABEL_SAVE: f32 = 82.0;
pub const LABEL_COPY: f32 = 34.0;
/// Where the "Generate one" link starts / how wide it is on the hint line
/// ("No key yet? " then the link, Sora 13).
pub const HINT_LINK_X: f32 = 76.0;
pub const HINT_LINK_W: f32 = 86.0;

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

fn dialog_height_for(rows: &[Row]) -> f32 {
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
fn classify_error(message: &str) -> Option<Field> {
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

impl AddHostForm {
    /// Body rows of the current step.
    pub fn rows(&self) -> Vec<Row> {
        rows_for(self.step, &self.auth_method)
    }

    /// Fields currently in the tab order / paint order for the active step.
    pub fn visible_fields(&self) -> Vec<Field> {
        let mut fields = Vec::new();
        for row in self.rows() {
            match row {
                Row::Single(f) => fields.push(f),
                Row::Pair(a, b) => {
                    fields.push(a);
                    fields.push(b);
                }
                Row::Choices => fields.push(Field::AuthMethod),
                Row::KeyHint | Row::KerberosHint => {}
            }
        }
        fields
    }

    pub fn shows_identity(&self) -> bool {
        self.step == AddHostStep::Auth
            && self.auth_method != "password"
            && self.auth_method != "gssapi"
    }

    pub fn shows_password(&self) -> bool {
        self.step == AddHostStep::Auth && self.auth_method == "password"
    }

    /// Current wizard step.
    pub fn step(&self) -> AddHostStep {
        self.step
    }

    /// Set step directly (e.g. from clicking a stepper segment).
    pub fn set_step(&mut self, step: AddHostStep) {
        self.step = step;
        self.close_menu();
        self.clamp_focus_to_visible();
    }

    /// Can we advance from the current step?
    pub fn can_advance(&self) -> bool {
        match self.step {
            AddHostStep::Target => {
                let address = split_address(&self.values[Field::Hostname.index()].value);
                !address.host.trim().is_empty()
            }
            AddHostStep::Auth => self.auth_validation_error().is_none(),
            AddHostStep::Details => true,
        }
    }

    /// Advance to the next wizard step. Returns true if step changed.
    pub fn next_step(&mut self) -> bool {
        match self.step {
            AddHostStep::Target => {
                self.settle_address();
                let address = split_address(&self.values[Field::Hostname.index()].value);
                if address.host.trim().is_empty() {
                    self.set_field_error(Field::Hostname, "Enter a hostname or IP");
                    return false;
                }
                self.enter_step(AddHostStep::Auth);
                true
            }
            AddHostStep::Auth => {
                if let Some(err) = self.auth_validation_error() {
                    let field = if err == "Enter a password" {
                        Field::Password
                    } else {
                        Field::Identity
                    };
                    self.set_field_error(field, err);
                    return false;
                }
                self.enter_step(AddHostStep::Details);
                true
            }
            AddHostStep::Details => false,
        }
    }

    /// Return to previous wizard step. Returns true if step changed.
    pub fn prev_step(&mut self) -> bool {
        match self.step {
            AddHostStep::Target => false,
            AddHostStep::Auth => {
                self.enter_step(AddHostStep::Target);
                true
            }
            AddHostStep::Details => {
                self.enter_step(AddHostStep::Auth);
                true
            }
        }
    }

    fn enter_step(&mut self, step: AddHostStep) {
        self.step = step;
        self.close_menu();
        self.focus = step.first_field();
        self.clear_error();
    }

    /// Total dialog height for the current step.
    pub fn height(&self) -> f32 {
        dialog_height_for(&self.rows())
    }

    /// Height of the tallest step: the dialog's top is anchored on it so
    /// the title does not jump while stepping.
    pub fn anchor_height(&self) -> f32 {
        let mut tallest = 0.0_f32;
        for step in STEPS {
            for method in AUTH_METHODS {
                tallest = tallest.max(dialog_height_for(&rows_for(step, method)));
            }
        }
        tallest
    }

    /// "Step n of 3", left of the footer buttons.
    pub fn step_hint(&self) -> String {
        format!("Step {} of {}", self.step.index() + 1, STEPS.len())
    }

    /// Footer Back / Cancel label.
    pub fn secondary_label(&self) -> &'static str {
        if self.step == AddHostStep::Target {
            "Cancel"
        } else {
            "Back"
        }
    }

    /// Footer Continue / Save label.
    pub fn primary_label(&self) -> &'static str {
        if self.step == AddHostStep::Details {
            "Save server"
        } else {
            "Continue"
        }
    }

    /// Show the form, empty, focused on the first field (Address).
    ///
    /// Always a fresh form: a half-typed host from a previous attempt
    /// reappearing unasked is worse than retyping two fields.
    pub fn open(&mut self) {
        self.reset_values();
        // Nothing to pick in the Key select yet: start where the user can
        // actually connect.
        self.auth_method = if self.identities.is_empty() {
            "password".to_string()
        } else {
            "key".to_string()
        };
        self.identity_id = self.identities.first().map(|(id, _)| id.clone());
        self.editing_id = None;
        self.open = true;
    }

    fn reset_values(&mut self) {
        self.values = Default::default();
        self.values[3] = TextDraft::new("22");
        self.port_dirty = false;
        self.password.clear();
        self.password_visible = false;
        self.group_id = None;
        self.step = AddHostStep::Target;
        self.focus = Field::Hostname;
        self.error = None;
        self.error_field = None;
        self.menu = None;
        self.menu_hover = None;
        self.hover = None;
    }

    /// Prefill the form for editing an existing host. Password stays empty
    /// (leave blank to keep the stored credential).
    pub fn open_edit(&mut self, values: HostFormValues, host_id: String) {
        self.reset_values();
        let port = if values.port.trim().is_empty() {
            "22".to_string()
        } else {
            values.port
        };
        self.port_dirty = port != "22";
        self.values = [
            values.name,
            values.hostname,
            values.username,
            port,
            values.tags,
            values.notes,
        ]
        .map(TextDraft::new);
        self.auth_method = if values.auth_method.trim().is_empty() {
            "key".to_string()
        } else {
            values.auth_method
        };
        self.identity_id = values
            .identity_id
            .or_else(|| self.identities.first().map(|(id, _)| id.clone()));
        let still_valid = self
            .identity_id
            .as_ref()
            .is_some_and(|id| self.identities.iter().any(|(i, _)| i == id));
        if !still_valid {
            self.identity_id = self.identities.first().map(|(id, _)| id.clone());
        }
        self.group_id = values
            .group_id
            .filter(|id| self.groups.iter().any(|(g, _)| g == id));
        self.editing_id = Some(host_id);
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.error = None;
        self.error_field = None;
        self.menu = None;
        self.menu_hover = None;
        self.hover = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Host id being edited, if any.
    pub fn editing_id(&self) -> Option<&str> {
        self.editing_id.as_deref()
    }

    pub fn is_editing(&self) -> bool {
        self.editing_id.is_some()
    }

    pub fn password_visible(&self) -> bool {
        self.password_visible
    }

    pub fn toggle_password_visible(&mut self) {
        self.password_visible = !self.password_visible;
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The input the current error is about, when it is about one.
    pub fn error_field(&self) -> Option<Field> {
        self.error_field
    }

    /// Message from the repository (or a progress note). An error about a
    /// field takes the dialog to that field's step and focuses it.
    pub fn set_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        match classify_error(&message) {
            Some(field) => self.set_field_error(field, message),
            None => {
                self.error = Some(message);
                self.error_field = None;
            }
        }
    }

    /// Error about one input.
    pub fn set_field_error(&mut self, field: Field, message: impl Into<String>) {
        if field.step() != self.step {
            self.set_step(field.step());
        }
        self.focus_input(field);
        self.error = Some(message.into());
        self.error_field = Some(field);
    }

    fn clear_error(&mut self) {
        self.error = None;
        self.error_field = None;
    }

    /// Replace the managed-key options (id, display name).
    pub fn set_identities(&mut self, identities: Vec<(String, String)>) {
        self.identities = identities;
        let still_valid = self
            .identity_id
            .as_ref()
            .is_some_and(|id| self.identities.iter().any(|(i, _)| i == id));
        if !still_valid {
            self.identity_id = self.identities.first().map(|(id, _)| id.clone());
        }
    }

    pub fn identities(&self) -> &[(String, String)] {
        &self.identities
    }

    /// Replace the group options (id, display name).
    pub fn set_groups(&mut self, groups: Vec<(String, String)>) {
        self.groups = groups;
        if !self
            .group_id
            .as_ref()
            .is_some_and(|id| self.groups.iter().any(|(g, _)| g == id))
        {
            self.group_id = None;
        }
    }

    pub fn groups(&self) -> &[(String, String)] {
        &self.groups
    }

    pub fn group_id(&self) -> Option<&str> {
        self.group_id.as_deref()
    }

    pub fn selected_group_name(&self) -> Option<&str> {
        let id = self.group_id.as_ref()?;
        self.groups
            .iter()
            .find(|(g, _)| g == id)
            .map(|(_, name)| name.as_str())
    }

    pub fn auth_method(&self) -> &str {
        &self.auth_method
    }

    /// Pick a sign-in method by index into [`AUTH_METHODS`]; focus moves
    /// to the field it needs.
    pub fn select_auth_method(&mut self, index: usize) {
        if index >= AUTH_METHODS.len() {
            return;
        }
        self.auth_method = AUTH_METHODS[index].to_string();
        self.close_menu();
        self.clear_error();
        if self.step == AddHostStep::Auth {
            self.focus = match self.auth_method.as_str() {
                "password" => Field::Password,
                "gssapi" => Field::AuthMethod,
                _ => Field::Identity,
            };
        }
    }

    // ---- select lists ----

    pub fn menu(&self) -> Option<SelectMenu> {
        self.menu
    }

    pub fn identity_menu_open(&self) -> bool {
        self.menu == Some(SelectMenu::Identity)
    }

    pub fn group_menu_open(&self) -> bool {
        self.menu == Some(SelectMenu::Group)
    }

    pub fn menu_hover(&self) -> Option<usize> {
        self.menu_hover
    }

    /// Number of options in the open list.
    pub fn menu_len(&self) -> usize {
        match self.menu {
            Some(SelectMenu::Identity) => self.identities.len(),
            Some(SelectMenu::Group) => self.groups.len() + 1,
            None => 0,
        }
    }

    pub fn toggle_identity_menu(&mut self) {
        if !self.shows_identity() {
            return;
        }
        self.toggle_menu(SelectMenu::Identity, Field::Identity);
    }

    pub fn toggle_group_menu(&mut self) {
        if self.step != AddHostStep::Details {
            return;
        }
        self.toggle_menu(SelectMenu::Group, Field::Group);
    }

    fn toggle_menu(&mut self, menu: SelectMenu, field: Field) {
        self.menu = if self.menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
        self.menu_hover = None;
        self.focus = field;
    }

    pub fn close_menu(&mut self) {
        self.menu = None;
        self.menu_hover = None;
    }

    pub fn set_menu_hover(&mut self, index: Option<usize>) -> bool {
        if self.menu_hover == index {
            return false;
        }
        self.menu_hover = index;
        true
    }

    /// Pick a managed key by index into [`Self::identities`].
    pub fn select_identity(&mut self, index: usize) {
        if index >= self.identities.len() {
            return;
        }
        self.identity_id = Some(self.identities[index].0.clone());
        self.close_menu();
        self.clear_error();
    }

    /// Pick a group: 0 is "No group", `n` is `groups()[n - 1]`.
    pub fn select_group(&mut self, index: usize) {
        if index > self.groups.len() {
            return;
        }
        self.group_id = index.checked_sub(1).map(|i| self.groups[i].0.clone());
        self.close_menu();
        self.clear_error();
    }

    pub fn identity_id(&self) -> Option<&str> {
        self.identity_id.as_deref()
    }

    pub fn selected_identity_name(&self) -> Option<&str> {
        let id = self.identity_id.as_ref()?;
        self.identities
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, name)| name.as_str())
    }

    /// What the pointer rests on (buttons, cards), for hover states.
    pub fn hover(&self) -> Option<AddHostHit> {
        self.hover
    }

    /// Returns whether the hover target changed (so the caller repaints).
    pub fn set_hover(&mut self, hover: Option<AddHostHit>) -> bool {
        if self.hover == hover {
            return false;
        }
        self.hover = hover;
        true
    }

    pub fn password(&self) -> &str {
        &self.password.value
    }

    pub fn focused_field(&self) -> Field {
        self.focus
    }

    /// Index of the focused field in [`Self::visible_fields`].
    pub fn focus(&self) -> usize {
        self.visible_fields()
            .iter()
            .position(|&f| f == self.focus)
            .unwrap_or(0)
    }

    pub fn value(&self, field: Field) -> &str {
        match field {
            Field::AuthMethod => &self.auth_method,
            Field::Identity => self.identity_id.as_deref().unwrap_or(""),
            Field::Group => self.group_id.as_deref().unwrap_or(""),
            Field::Password => &self.password.value,
            f => &self.values[f.index()].value,
        }
    }

    pub fn cursor(&self, field: Field) -> usize {
        self.draft(field).map_or(0, |d| d.caret)
    }

    /// The shared text draft behind a text field (`None` for selects).
    pub fn draft(&self, field: Field) -> Option<&TextDraft> {
        match field {
            Field::Password => Some(&self.password),
            f if f.base_index().is_some() => Some(&self.values[f.index()]),
            _ => None,
        }
    }

    fn draft_mut(&mut self, field: Field) -> Option<&mut TextDraft> {
        match field {
            Field::Password => Some(&mut self.password),
            f if f.base_index().is_some() => Some(&mut self.values[f.index()]),
            _ => None,
        }
    }

    /// The caret's byte offset inside `field`'s value.
    pub fn cursor_byte(&self, field: Field) -> usize {
        let value = self.value(field);
        let chars = self.cursor(field).min(value.chars().count());
        char_byte_offset(value, chars)
    }

    /// Text before / after the caret, for painting the caret inline.
    pub fn split_at_cursor(&self) -> (&str, &str) {
        let field = self.focused_field();
        let value = self.value(field);
        let byte = self.cursor_byte(field);
        value.split_at(byte)
    }

    /// The port the form will save: a typed port wins over the address's.
    fn effective_port(&self, from_address: Option<String>) -> String {
        let typed = &self.values[3].value;
        match from_address {
            Some(port) if !self.port_dirty || typed.trim().is_empty() => port,
            _ => typed.clone(),
        }
    }

    pub fn values(&self) -> HostFormValues {
        let address = split_address(&self.values[1].value);
        let username = if self.values[2].value.trim().is_empty() {
            address.user.unwrap_or_default()
        } else {
            self.values[2].value.clone()
        };
        HostFormValues {
            name: self.values[0].value.clone(),
            hostname: address.host,
            username,
            port: self.effective_port(address.port),
            auth_method: self.auth_method.clone(),
            identity_id: self.identity_id.clone(),
            password: self.password.value.clone(),
            group_id: self.group_id.clone(),
            tags: self.values[4].value.clone(),
            notes: self.values[5].value.clone(),
        }
    }

    /// Auth-only readiness (host fields still belong to the repository).
    pub fn auth_validation_error(&self) -> Option<&'static str> {
        match self.auth_method.as_str() {
            "key" if self.identity_id.is_none() => Some("Select an SSH key"),
            // When editing, an empty password means "keep the stored one".
            "password" if self.password.value.is_empty() && self.editing_id.is_none() => {
                Some("Enter a password")
            }
            "key" | "password" | "gssapi" => None,
            _ => Some("Unknown authentication method"),
        }
    }

    /// Cycle key, password, gssapi.
    pub fn cycle_auth_method(&mut self, delta: isize) {
        let i = AUTH_METHODS
            .iter()
            .position(|&m| m == self.auth_method)
            .unwrap_or(0);
        let next = (i as isize + delta).rem_euclid(AUTH_METHODS.len() as isize) as usize;
        self.auth_method = AUTH_METHODS[next].to_string();
        self.close_menu();
        self.clear_error();
        self.clamp_focus_to_visible();
    }

    /// Cycle through managed keys (no-op when empty).
    pub fn cycle_identity(&mut self, delta: isize) {
        if self.identities.is_empty() {
            return;
        }
        let i = self
            .identity_id
            .as_ref()
            .and_then(|id| self.identities.iter().position(|(i, _)| i == id))
            .unwrap_or(0);
        let next =
            (i as isize + delta).rem_euclid(self.identities.len() as isize) as usize;
        self.identity_id = Some(self.identities[next].0.clone());
        self.clear_error();
    }

    /// Cycle through "No group" and the groups.
    pub fn cycle_group(&mut self, delta: isize) {
        let n = self.groups.len() as isize + 1;
        let i = self
            .group_id
            .as_ref()
            .and_then(|id| self.groups.iter().position(|(g, _)| g == id))
            .map_or(0, |p| p as isize + 1);
        let next = (i + delta).rem_euclid(n) as usize;
        self.select_group(next);
    }

    fn clamp_focus_to_visible(&mut self) {
        if !self.visible_fields().contains(&self.focus) {
            self.focus = self.step.first_field();
            if !self.visible_fields().contains(&self.focus) {
                self.focus = self
                    .visible_fields()
                    .first()
                    .copied()
                    .unwrap_or(Field::Hostname);
            }
        }
    }

    /// Insert text at the caret. Empty or control-bearing text is
    /// rejected, the same policy every other text sink in rio applies.
    pub fn insert(&mut self, text: &str) -> bool {
        let field = self.focused_field();
        if !field.is_text() {
            return false;
        }
        if field == Field::Port && !self.port_dirty {
            // Typing over the untouched default replaces it, unless the
            // text would be refused anyway.
            if text.is_empty() || text.chars().any(char::is_control) {
                return false;
            }
            self.values[3].clear();
        }
        let Some(draft) = self.draft_mut(field) else {
            return false;
        };
        if !draft.insert(text, usize::MAX, false) {
            return false;
        }
        if field == Field::Port {
            self.port_dirty = true;
        }
        self.clear_error();
        true
    }

    /// Shared text editing (Backspace, Delete, caret, selection, by word).
    /// Returns whether anything changed.
    pub fn edit(&mut self, edit: TextEdit) -> bool {
        let field = self.focused_field();
        let Some(draft) = self.draft_mut(field) else {
            return false;
        };
        let before = draft.value.len();
        if !draft.apply(edit) {
            return false;
        }
        let edited = draft.value.len() != before;
        if edited {
            if field == Field::Port {
                self.port_dirty = true;
            }
            self.clear_error();
        }
        true
    }

    /// Delete the character before the caret. Returns whether anything
    /// changed (a backspace at offset 0 must not be reported as an edit,
    /// or the caller repaints on every stray keypress).
    pub fn backspace(&mut self) -> bool {
        self.edit(TextEdit::Backspace { by_word: false })
    }

    /// Delete the character after the caret.
    pub fn delete(&mut self) -> bool {
        self.edit(TextEdit::Delete { by_word: false })
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let edit = if delta < 0 {
            TextEdit::Left {
                kind: TextMoveKind::Collapse,
                by_word: false,
            }
        } else {
            TextEdit::Right {
                kind: TextMoveKind::Collapse,
                by_word: false,
            }
        };
        for _ in 0..delta.unsigned_abs() {
            self.edit(edit);
        }
    }

    pub fn cursor_home(&mut self) {
        self.edit(TextEdit::Home {
            kind: TextMoveKind::Collapse,
        });
    }

    pub fn cursor_end(&mut self) {
        self.edit(TextEdit::End {
            kind: TextMoveKind::Collapse,
        });
    }

    /// Leaving the address: show the user and port it carried in their
    /// own (empty) fields, so what gets saved is what the form shows.
    pub fn settle_address(&mut self) {
        if self.focus != Field::Hostname {
            return;
        }
        let parsed = self.values();
        for (i, value) in [(1, parsed.hostname), (2, parsed.username), (3, parsed.port)] {
            if self.values[i].value != value {
                if i == 3 {
                    self.port_dirty = true;
                }
                self.values[i] = TextDraft::new(value);
            }
        }
    }

    /// Move focus `delta` fields forward, wrapping across visible rows.
    fn focus_by(&mut self, delta: isize) {
        self.settle_address();
        let fields = self.visible_fields();
        let len = fields.len() as isize;
        let i = fields.iter().position(|&f| f == self.focus).unwrap_or(0) as isize;
        let next = (i + delta).rem_euclid(len) as usize;
        self.focus_input(fields[next]);
        self.clear_error();
    }

    /// Focus `field`, closing a list that belongs to another one.
    fn focus_input(&mut self, field: Field) {
        self.focus = field;
        let keep = match self.menu {
            Some(SelectMenu::Identity) => field == Field::Identity,
            Some(SelectMenu::Group) => field == Field::Group,
            None => true,
        };
        if !keep {
            self.close_menu();
        }
    }

    /// Focus a specific field (mouse click into an input).
    pub fn focus_field(&mut self, field: Field) {
        if field != Field::Hostname {
            self.settle_address();
        }
        if self.visible_fields().contains(&field) {
            self.focus_input(field);
            self.clear_error();
        }
    }

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

/// Byte offset of the `chars`-th character (or the length).
fn char_byte_offset(value: &str, chars: usize) -> usize {
    value
        .char_indices()
        .nth(chars)
        .map(|(byte, _)| byte)
        .unwrap_or(value.len())
}

/// Screen geometry of the dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AddHostLayout {
    /// Top-left of the whole dialog.
    pub x: f32,
    pub y: f32,
}

impl AddHostLayout {
    /// Center the dialog in a `width` x `height` window, in logical pixels.
    /// `anchor_height` is [`AddHostForm::anchor_height`], so every step
    /// shares the same top edge.
    pub fn centered(width: f32, height: f32, anchor_height: f32) -> Self {
        Self {
            x: ((width - WIDTH) / 2.0).max(0.0),
            y: ((height - anchor_height) / 2.0).max(0.0),
        }
    }

    pub fn rect(&self, dialog_height: f32) -> Rect {
        Rect::new(self.x, self.y, WIDTH, dialog_height)
    }

    fn inner_x(&self) -> f32 {
        self.x + PAD_X
    }

    fn inner_width(&self) -> f32 {
        WIDTH - 2.0 * PAD_X
    }

    pub fn title_rect(&self) -> Rect {
        Rect::new(
            self.inner_x(),
            self.y + HEADER_TOP,
            self.inner_width() - CLOSE_SIZE,
            CLOSE_SIZE,
        )
    }

    pub fn close_button_rect(&self) -> Rect {
        Rect::new(
            self.x + WIDTH - CLOSE_RIGHT - CLOSE_SIZE,
            self.y + HEADER_TOP,
            CLOSE_SIZE,
            CLOSE_SIZE,
        )
    }

    /// The Overlays stepper under the title.
    pub fn stepper_rect(&self) -> Rect {
        Rect::new(
            self.inner_x(),
            self.y + HEADER_TOP + CLOSE_SIZE + STEPPER_TOP,
            self.inner_width(),
            ov::STEPPER_HEIGHT,
        )
    }

    /// Bar and label of one stepper segment (the click target).
    pub fn step_rect(&self, step: AddHostStep) -> Rect {
        let seg =
            ov::stepper_segments(self.stepper_rect(), step.index() + 1)[step.index()];
        Rect::new(seg.bar.x, seg.bar.y, seg.bar.width, ov::STEPPER_HEIGHT)
    }

    fn body_top(&self) -> f32 {
        self.stepper_rect().bottom() + BODY_PAD
    }

    /// Top of each body row.
    fn row_tops(&self, form: &AddHostForm) -> Vec<(Row, f32)> {
        let mut y = self.body_top();
        form.rows()
            .into_iter()
            .map(|row| {
                let top = y;
                y += row.height() + ROW_GAP;
                (row, top)
            })
            .collect()
    }

    /// Label, box, trailing slot and helper of one field.
    pub fn field_layout(&self, form: &AddHostForm, field: Field) -> Option<FieldLayout> {
        let x = self.inner_x();
        let width = self.inner_width();
        for (row, top) in self.row_tops(form) {
            match row {
                Row::Single(f) if f == field => {
                    return Some(inp::field_layout(
                        (x, top),
                        width,
                        f.kind(),
                        true,
                        f.has_helper(),
                    ));
                }
                Row::Pair(a, b) if a == field || b == field => {
                    let col = (width - 2.0 * PAIR_GAP) / 3.0;
                    let wide = 2.0 * col + PAIR_GAP;
                    let (fx, fw) = if a == field {
                        (x, wide)
                    } else {
                        (x + wide + PAIR_GAP, col)
                    };
                    return Some(inp::field_layout(
                        (fx, top),
                        fw,
                        field.kind(),
                        true,
                        false,
                    ));
                }
                _ => {}
            }
        }
        None
    }

    /// The whole field (label, box, helper).
    pub fn field_rect(&self, form: &AddHostForm, field: Field) -> Option<Rect> {
        self.field_layout(form, field).map(|l| l.total)
    }

    /// The bordered input box.
    pub fn input_rect(&self, form: &AddHostForm, field: Field) -> Option<Rect> {
        self.field_layout(form, field).map(|l| l.box_rect)
    }

    /// Eye toggle on the right of the password box.
    pub fn password_toggle_rect(&self, form: &AddHostForm) -> Option<Rect> {
        let input = self.input_rect(form, Field::Password)?;
        Some(Rect::new(input.right() - 44.0, input.y, 44.0, input.height))
    }

    /// "Sign in with" caption above the cards.
    pub fn choices_label_rect(&self, form: &AddHostForm) -> Option<Rect> {
        self.row_tops(form)
            .into_iter()
            .find(|(r, _)| *r == Row::Choices)
            .map(|(_, top)| {
                Rect::new(self.inner_x(), top, self.inner_width(), inp::LABEL_HEIGHT)
            })
    }

    /// The three sign-in cards (key, password, Kerberos).
    pub fn choice_rects(&self, form: &AddHostForm) -> Option<Vec<Rect>> {
        let label = self.choices_label_rect(form)?;
        Some(sel::choice_row(
            label.x,
            label.bottom() + inp::LABEL_GAP,
            label.width,
            CARD_GAP,
            AUTH_METHODS.len(),
        ))
    }

    /// The hint line under the key select / the Kerberos note.
    pub fn hint_rect(&self, form: &AddHostForm) -> Option<Rect> {
        self.row_tops(form)
            .into_iter()
            .find(|(r, _)| matches!(r, Row::KeyHint | Row::KerberosHint))
            .map(|(_, top)| Rect::new(self.inner_x(), top, self.inner_width(), HINT_LINE))
    }

    /// The "Generate one" link inside the key hint.
    pub fn generate_link_rect(&self, form: &AddHostForm) -> Option<Rect> {
        if !form.shows_identity() {
            return None;
        }
        let hint = self.hint_rect(form)?;
        Some(Rect::new(
            hint.x + HINT_LINK_X,
            hint.y,
            HINT_LINK_W,
            hint.height,
        ))
    }

    /// The open select's list, under its box.
    pub fn menu_rect(&self, form: &AddHostForm) -> Option<Rect> {
        let field = match form.menu()? {
            SelectMenu::Identity => Field::Identity,
            SelectMenu::Group => Field::Group,
        };
        let input = self.input_rect(form, field)?;
        let rows = form.menu_len().max(1) as f32;
        Some(Rect::new(
            input.x,
            input.bottom() + 4.0,
            input.width,
            rows * MENU_ROW + 2.0 * MENU_PAD,
        ))
    }

    pub fn menu_option_rect(&self, form: &AddHostForm, index: usize) -> Option<Rect> {
        if index >= form.menu_len() {
            return None;
        }
        let menu = self.menu_rect(form)?;
        Some(Rect::new(
            menu.x + MENU_PAD,
            menu.y + MENU_PAD + index as f32 * MENU_ROW,
            menu.width - 2.0 * MENU_PAD,
            MENU_ROW,
        ))
    }

    fn button_y(&self, dialog_height: f32) -> f32 {
        self.y + dialog_height - FOOTER_BOTTOM - BUTTON_HEIGHT
    }

    /// Continue / Save server, right aligned.
    pub fn primary_button_rect(&self, form: &AddHostForm) -> Rect {
        let label = if form.step() == AddHostStep::Details {
            LABEL_SAVE
        } else {
            LABEL_CONTINUE
        };
        let w = ov::action_width(label);
        Rect::new(
            self.x + WIDTH - PAD_X - w,
            self.button_y(form.height()),
            w,
            BUTTON_HEIGHT,
        )
    }

    /// Cancel (first step) / Back, left of the primary button.
    pub fn secondary_button_rect(&self, form: &AddHostForm) -> Rect {
        let label = if form.step() == AddHostStep::Target {
            LABEL_CANCEL
        } else {
            LABEL_BACK
        };
        let w = ov::action_width(label);
        let primary = self.primary_button_rect(form);
        Rect::new(primary.x - BUTTON_GAP - w, primary.y, w, BUTTON_HEIGHT)
    }

    /// "Copy" beside Back / Cancel, only while an error is shown.
    pub fn copy_error_rect(&self, form: &AddHostForm) -> Option<Rect> {
        form.error()?;
        let secondary = self.secondary_button_rect(form);
        let w = ov::action_width(LABEL_COPY);
        Some(Rect::new(
            secondary.x - BUTTON_GAP - w,
            secondary.y,
            w,
            BUTTON_HEIGHT,
        ))
    }

    /// "Step n of 3" / validation message, left of the buttons.
    pub fn footer_text_rect(&self, form: &AddHostForm) -> Rect {
        let buttons_left = self
            .copy_error_rect(form)
            .unwrap_or_else(|| self.secondary_button_rect(form))
            .x;
        Rect::new(
            self.inner_x(),
            self.secondary_button_rect(form).y,
            (buttons_left - FOOTER_TEXT_GAP - self.inner_x()).max(0.0),
            BUTTON_HEIGHT,
        )
    }

    /// Hit-test inside an open dialog. Coordinates are logical pixels.
    pub fn hit_test(&self, form: &AddHostForm, x: f32, y: f32) -> AddHostHit {
        let dialog = self.rect(form.height());
        // An open list may extend past the dialog bottom; its options stay
        // clickable.
        if let Some(menu) = self.menu_rect(form) {
            if menu.contains(x, y) {
                for i in 0..form.menu_len() {
                    if let Some(opt) = self.menu_option_rect(form, i) {
                        if opt.contains(x, y) {
                            return match form.menu() {
                                Some(SelectMenu::Group) => AddHostHit::SelectGroup(i),
                                _ => AddHostHit::SelectIdentity(i),
                            };
                        }
                    }
                }
                return AddHostHit::Consume;
            }
        }
        if !dialog.contains(x, y) {
            return AddHostHit::Consume;
        }
        if self.close_button_rect().contains(x, y) {
            return AddHostHit::Close;
        }
        for &step in &STEPS {
            if self.step_rect(step).contains(x, y) {
                return AddHostHit::StepPill(step);
            }
        }
        if self.copy_error_rect(form).is_some_and(|r| r.contains(x, y)) {
            return AddHostHit::CopyError;
        }
        if self.secondary_button_rect(form).contains(x, y) {
            return if form.step() == AddHostStep::Target {
                AddHostHit::Cancel
            } else {
                AddHostHit::Back
            };
        }
        if self.primary_button_rect(form).contains(x, y) {
            return if form.step() == AddHostStep::Details {
                AddHostHit::Connect
            } else {
                AddHostHit::Next
            };
        }
        if let Some(cards) = self.choice_rects(form) {
            if let Some(i) = cards.iter().position(|c| c.contains(x, y)) {
                return AddHostHit::SelectAuth(i);
            }
        }
        if self
            .generate_link_rect(form)
            .is_some_and(|r| r.contains(x, y))
        {
            return AddHostHit::GenerateKey;
        }
        for field in form.visible_fields() {
            if field == Field::AuthMethod {
                continue;
            }
            let Some(layout) = self.field_layout(form, field) else {
                continue;
            };
            if !layout.box_rect.contains(x, y) {
                continue;
            }
            return match field {
                Field::Password
                    if self
                        .password_toggle_rect(form)
                        .is_some_and(|eye| eye.contains(x, y)) =>
                {
                    AddHostHit::TogglePasswordVisible
                }
                Field::Identity => AddHostHit::ToggleIdentityMenu,
                Field::Group => AddHostHit::ToggleGroupMenu,
                other => AddHostHit::Field(other),
            };
        }
        AddHostHit::Consume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Key auth, the mode most tests exercise (a fresh form with no saved
    /// keys starts on password instead).
    fn open_form() -> AddHostForm {
        let mut form = AddHostForm::default();
        form.open();
        form.select_auth_method(0);
        form
    }

    fn type_into(form: &mut AddHostForm, text: &str) {
        for ch in text.chars() {
            let buf = ch.to_string();
            form.handle_input(FormInput::Text, &buf);
        }
    }

    fn layout_for(form: &AddHostForm) -> AddHostLayout {
        AddHostLayout::centered(1440.0, 900.0, form.anchor_height())
    }

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
}
