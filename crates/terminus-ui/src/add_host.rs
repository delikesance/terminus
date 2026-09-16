//! The add-host editor: a small form with one focused field.
//!
//! The form owns the text, the focus and the caret, and nothing else —
//! no storage. Submitting hands the raw values to the host repository,
//! which is the single place that decides whether a host is storable; a
//! rejection comes back as [`AddHostForm::set_error`] so the form stays
//! open with its text intact.
//!
//! Auth methods are plain strings (`"key"` | `"password"` | `"gssapi"`)
//! so this crate stays independent of terminus-core's typed enum.

use crate::geom::Rect;

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

/// The fields, in tab order when all are visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Hostname,
    Username,
    Port,
    AuthMethod,
    Identity,
    Password,
}

/// Always-visible text fields before the auth block.
pub const BASE_FIELDS: [Field; 4] =
    [Field::Name, Field::Hostname, Field::Username, Field::Port];

/// Back-compat alias: base text fields only (auth rows are conditional).
pub const FIELDS: [Field; 4] = BASE_FIELDS;

impl Field {
    pub const fn label(self) -> &'static str {
        match self {
            Field::Name => "Host Name / Label",
            Field::Hostname => "IP Address or Hostname",
            Field::Username => "Username",
            Field::Port => "Port",
            Field::AuthMethod => "Authentication Method",
            Field::Identity => "Select Saved SSH Key",
            Field::Password => "SSH Password",
        }
    }

    pub const fn placeholder(self) -> &'static str {
        match self {
            Field::Name => "e.g. AWS Production Cluster",
            Field::Hostname => "192.168.1.50",
            Field::Username => "root",
            Field::Port => "22",
            Field::AuthMethod => "",
            Field::Identity => "No SSH keys saved",
            Field::Password => "Enter secure password",
        }
    }

    /// Index among the four base text fields, or `None` for auth rows.
    pub const fn base_index(self) -> Option<usize> {
        match self {
            Field::Name => Some(0),
            Field::Hostname => Some(1),
            Field::Username => Some(2),
            Field::Port => Some(3),
            Field::AuthMethod | Field::Identity | Field::Password => None,
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
        )
    }

    /// Legacy index into the base text array (auth fields map to 0).
    pub const fn index(self) -> usize {
        match self {
            Field::Name => 0,
            Field::Hostname => 1,
            Field::Username => 2,
            Field::Port => 3,
            Field::AuthMethod | Field::Identity | Field::Password => 0,
        }
    }

    pub const fn from_index(index: usize) -> Self {
        BASE_FIELDS[index % BASE_FIELDS.len()]
    }
}

/// Dialog metrics, in logical pixels (`max-w-md` ≈ 448).
pub const WIDTH: f32 = 448.0;
/// Height of one field row: a caption plus its input box.
pub const FIELD_HEIGHT: f32 = 52.0;
/// Top of the input box inside a field row.
pub const INPUT_TOP: f32 = 18.0;
/// Height of the input box itself.
pub const INPUT_HEIGHT: f32 = 32.0;
pub const FIELD_GAP: f32 = 12.0;
pub const PAD: f32 = 24.0;
pub const TITLE_HEIGHT: f32 = 28.0;
pub const HINT_HEIGHT: f32 = 44.0;
/// Corner radius (`rounded-2xl`).
pub const DIALOG_RADIUS: f32 = 16.0;
/// Input corner radius (`rounded-xl`).
pub const INPUT_RADIUS: f32 = 12.0;
/// Footer Cancel / Connect button height.
pub const BUTTON_HEIGHT: f32 = 32.0;
pub const CONNECT_BUTTON_WIDTH: f32 = 84.0;
pub const CANCEL_BUTTON_WIDTH: f32 = 72.0;
pub const BUTTON_GAP: f32 = 8.0;

/// What a mouse press on the open add-host dialog hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddHostHit {
    /// An input box — focus that field.
    Field(Field),
    /// Open / close the Authentication Method dropdown.
    ToggleAuthMenu,
    /// Pick an auth method from the open dropdown.
    SelectAuth(usize),
    /// Open / close the Select Saved SSH Key dropdown.
    ToggleIdentityMenu,
    /// Pick a saved identity from the open dropdown.
    SelectIdentity(usize),
    /// Eye toggle on the password field.
    TogglePasswordVisible,
    /// Dismiss without saving.
    Cancel,
    /// Persist the draft (same as Enter).
    Connect,
    /// Dialog chrome / padding — swallow, keep open.
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

/// The add-host editor's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddHostForm {
    open: bool,
    /// When set, the dialog updates this host instead of creating one.
    editing_id: Option<String>,
    values: [String; 4],
    /// Caret position, in characters (not bytes), per base text field.
    carets: [usize; 4],
    auth_method: String,
    identity_id: Option<String>,
    identities: Vec<(String, String)>,
    password: String,
    password_caret: usize,
    /// Whether the password field shows plaintext (eye toggle).
    password_visible: bool,
    focus: Field,
    error: Option<String>,
    /// Authentication Method dropdown is expanded.
    auth_menu_open: bool,
    /// Hovered option in the auth dropdown (`None` = none).
    auth_menu_hover: Option<usize>,
    /// Select Saved SSH Key dropdown is expanded.
    identity_menu_open: bool,
    /// Hovered option in the identity dropdown (`None` = none).
    identity_menu_hover: Option<usize>,
}

impl Default for AddHostForm {
    fn default() -> Self {
        Self {
            open: false,
            editing_id: None,
            values: Default::default(),
            carets: [0; 4],
            auth_method: "key".to_string(),
            identity_id: None,
            identities: Vec::new(),
            password: String::new(),
            password_caret: 0,
            password_visible: false,
            focus: Field::Name,
            error: None,
            auth_menu_open: false,
            auth_menu_hover: None,
            identity_menu_open: false,
            identity_menu_hover: None,
        }
    }
}

impl AddHostForm {
    /// Fields currently in the tab order / paint order.
    pub fn visible_fields(&self) -> Vec<Field> {
        let mut fields = Vec::with_capacity(6);
        fields.extend_from_slice(&BASE_FIELDS);
        fields.push(Field::AuthMethod);
        match self.auth_method.as_str() {
            "password" => fields.push(Field::Password),
            "gssapi" => {}
            _ => fields.push(Field::Identity),
        }
        fields
    }

    pub fn shows_identity(&self) -> bool {
        self.auth_method != "password" && self.auth_method != "gssapi"
    }

    pub fn shows_password(&self) -> bool {
        self.auth_method == "password"
    }

    /// Total dialog height, including a hint/error line.
    pub fn height(&self) -> f32 {
        let n = self.visible_fields().len() as f32;
        PAD + TITLE_HEIGHT + n * FIELD_HEIGHT + (n - 1.0) * FIELD_GAP + PAD + HINT_HEIGHT
    }

    /// Show the form, empty, focused on the first field.
    ///
    /// Always a fresh form: a half-typed host from a previous attempt
    /// reappearing unasked is worse than retyping two fields.
    pub fn open(&mut self) {
        self.values = Default::default();
        self.carets = [0; 4];
        self.auth_method = "key".to_string();
        self.password.clear();
        self.password_caret = 0;
        self.password_visible = false;
        self.identity_id = self.identities.first().map(|(id, _)| id.clone());
        self.editing_id = None;
        self.focus = Field::Name;
        self.error = None;
        self.auth_menu_open = false;
        self.auth_menu_hover = None;
        self.identity_menu_open = false;
        self.identity_menu_hover = None;
        self.open = true;
    }

    /// Prefill the form for editing an existing host. Password stays empty
    /// (leave blank to keep the stored credential).
    pub fn open_edit(&mut self, values: HostFormValues, host_id: String) {
        self.values = [
            values.name,
            values.hostname,
            values.username,
            values.port,
        ];
        self.carets = [
            self.values[0].chars().count(),
            self.values[1].chars().count(),
            self.values[2].chars().count(),
            self.values[3].chars().count(),
        ];
        self.auth_method = if values.auth_method.trim().is_empty() {
            "key".to_string()
        } else {
            values.auth_method
        };
        self.password.clear();
        self.password_caret = 0;
        self.password_visible = false;
        self.identity_id = values.identity_id.or_else(|| {
            self.identities.first().map(|(id, _)| id.clone())
        });
        let still_valid = self
            .identity_id
            .as_ref()
            .is_some_and(|id| self.identities.iter().any(|(i, _)| i == id));
        if !still_valid {
            self.identity_id = self.identities.first().map(|(id, _)| id.clone());
        }
        self.editing_id = Some(host_id);
        self.focus = Field::Name;
        self.error = None;
        self.auth_menu_open = false;
        self.auth_menu_hover = None;
        self.identity_menu_open = false;
        self.identity_menu_hover = None;
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.editing_id = None;
        self.error = None;
        self.auth_menu_open = false;
        self.auth_menu_hover = None;
        self.identity_menu_open = false;
        self.identity_menu_hover = None;
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

    /// Rejection from the repository, shown under the fields.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    /// Replace the identity picker options (id, display name).
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

    pub fn auth_method(&self) -> &str {
        &self.auth_method
    }

    pub fn auth_menu_open(&self) -> bool {
        self.auth_menu_open
    }

    pub fn auth_menu_hover(&self) -> Option<usize> {
        self.auth_menu_hover
    }

    pub fn toggle_auth_menu(&mut self) {
        self.close_identity_menu();
        self.auth_menu_open = !self.auth_menu_open;
        if !self.auth_menu_open {
            self.auth_menu_hover = None;
        }
        self.focus = Field::AuthMethod;
    }

    pub fn close_auth_menu(&mut self) {
        self.auth_menu_open = false;
        self.auth_menu_hover = None;
    }

    /// Pick an auth method by index into [`AUTH_METHODS`].
    pub fn select_auth_method(&mut self, index: usize) {
        if index >= AUTH_METHODS.len() {
            return;
        }
        self.auth_method = AUTH_METHODS[index].to_string();
        self.auth_menu_open = false;
        self.auth_menu_hover = None;
        self.close_identity_menu();
        self.clamp_focus_to_visible();
        self.error = None;
    }

    pub fn set_auth_menu_hover(&mut self, index: Option<usize>) -> bool {
        if self.auth_menu_hover == index {
            return false;
        }
        self.auth_menu_hover = index;
        true
    }

    pub fn identity_menu_open(&self) -> bool {
        self.identity_menu_open
    }

    pub fn identity_menu_hover(&self) -> Option<usize> {
        self.identity_menu_hover
    }

    pub fn toggle_identity_menu(&mut self) {
        if !self.shows_identity() {
            return;
        }
        self.close_auth_menu();
        self.identity_menu_open = !self.identity_menu_open;
        if !self.identity_menu_open {
            self.identity_menu_hover = None;
        }
        self.focus = Field::Identity;
    }

    pub fn close_identity_menu(&mut self) {
        self.identity_menu_open = false;
        self.identity_menu_hover = None;
    }

    /// Pick a saved identity by index into [`Self::identities`].
    pub fn select_identity(&mut self, index: usize) {
        if index >= self.identities.len() {
            return;
        }
        self.identity_id = Some(self.identities[index].0.clone());
        self.close_identity_menu();
        self.error = None;
    }

    pub fn set_identity_menu_hover(&mut self, index: Option<usize>) -> bool {
        if self.identity_menu_hover == index {
            return false;
        }
        self.identity_menu_hover = index;
        true
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

    pub fn password(&self) -> &str {
        &self.password
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
            Field::Name => &self.values[0],
            Field::Hostname => &self.values[1],
            Field::Username => &self.values[2],
            Field::Port => &self.values[3],
            Field::AuthMethod => &self.auth_method,
            Field::Identity => self.identity_id.as_deref().unwrap_or(""),
            Field::Password => &self.password,
        }
    }

    pub fn cursor(&self, field: Field) -> usize {
        match field {
            Field::Password => self.password_caret,
            f if f.base_index().is_some() => self.carets[f.index()],
            _ => 0,
        }
    }

    /// The caret's byte offset inside `field`'s value.
    pub fn cursor_byte(&self, field: Field) -> usize {
        let value = self.value(field);
        let chars = self.cursor(field).min(value.chars().count());
        value
            .char_indices()
            .nth(chars)
            .map(|(byte, _)| byte)
            .unwrap_or(value.len())
    }

    /// Text before / after the caret, for painting the caret inline.
    pub fn split_at_cursor(&self) -> (&str, &str) {
        let field = self.focused_field();
        let value = self.value(field);
        let byte = self.cursor_byte(field);
        value.split_at(byte)
    }

    pub fn values(&self) -> HostFormValues {
        HostFormValues {
            name: self.values[0].clone(),
            hostname: self.values[1].clone(),
            username: self.values[2].clone(),
            port: self.values[3].clone(),
            auth_method: self.auth_method.clone(),
            identity_id: self.identity_id.clone(),
            password: self.password.clone(),
        }
    }

    /// Auth-only readiness (host fields still belong to the repository).
    pub fn auth_validation_error(&self) -> Option<&'static str> {
        match self.auth_method.as_str() {
            "key" if self.identity_id.is_none() => Some("Select an SSH key"),
            // When editing, an empty password means "keep the stored one".
            "password" if self.password.is_empty() && self.editing_id.is_none() => {
                Some("Enter a password")
            }
            "key" | "password" | "gssapi" => None,
            _ => Some("Unknown authentication method"),
        }
    }

    /// Cycle key → password → gssapi → key.
    pub fn cycle_auth_method(&mut self, delta: isize) {
        let i = AUTH_METHODS
            .iter()
            .position(|&m| m == self.auth_method)
            .unwrap_or(0);
        let next = (i as isize + delta).rem_euclid(AUTH_METHODS.len() as isize) as usize;
        self.auth_method = AUTH_METHODS[next].to_string();
        self.auth_menu_open = false;
        self.auth_menu_hover = None;
        self.close_identity_menu();
        self.clamp_focus_to_visible();
        self.error = None;
    }

    /// Cycle through saved identities (no-op when empty).
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
        self.error = None;
    }

    fn clamp_focus_to_visible(&mut self) {
        let visible = self.visible_fields();
        if visible.contains(&self.focus) {
            return;
        }
        self.focus = visible
            .iter()
            .copied()
            .find(|f| matches!(f, Field::Identity | Field::Password))
            .or_else(|| visible.iter().copied().find(|f| *f == Field::AuthMethod))
            .unwrap_or(Field::Name);
    }

    /// Insert text at the caret. Empty or control-bearing text is
    /// rejected, the same policy every other text sink in rio applies.
    pub fn insert(&mut self, text: &str) -> bool {
        if text.is_empty() || text.chars().any(char::is_control) {
            return false;
        }
        let field = self.focused_field();
        if !field.is_text() {
            return false;
        }
        let byte = self.cursor_byte(field);
        match field {
            Field::Password => {
                self.password.insert_str(byte, text);
                self.password_caret += text.chars().count();
            }
            _ => {
                self.values[field.index()].insert_str(byte, text);
                self.carets[field.index()] += text.chars().count();
            }
        }
        self.error = None;
        true
    }

    /// Delete the character before the caret. Returns whether anything
    /// changed (a backspace at offset 0 must not be reported as an edit,
    /// or the caller repaints on every stray keypress).
    pub fn backspace(&mut self) -> bool {
        let field = self.focused_field();
        if !field.is_text() {
            return false;
        }
        let chars = self.cursor(field);
        if chars == 0 {
            return false;
        }
        match field {
            Field::Password => {
                let start = char_byte_offset(&self.password, chars - 1);
                let end = char_byte_offset(&self.password, chars);
                self.password.replace_range(start..end, "");
                self.password_caret = chars - 1;
            }
            _ => {
                let value = &mut self.values[field.index()];
                let start = char_byte_offset(value, chars - 1);
                let end = char_byte_offset(value, chars);
                value.replace_range(start..end, "");
                self.carets[field.index()] = chars - 1;
            }
        }
        self.error = None;
        true
    }

    /// Delete the character after the caret.
    pub fn delete(&mut self) -> bool {
        let field = self.focused_field();
        if !field.is_text() {
            return false;
        }
        let chars = self.cursor(field);
        let len = self.value(field).chars().count();
        if chars >= len {
            return false;
        }
        match field {
            Field::Password => {
                let start = char_byte_offset(&self.password, chars);
                let end = char_byte_offset(&self.password, chars + 1);
                self.password.replace_range(start..end, "");
            }
            _ => {
                let value = &mut self.values[field.index()];
                let start = char_byte_offset(value, chars);
                let end = char_byte_offset(value, chars + 1);
                value.replace_range(start..end, "");
            }
        }
        self.error = None;
        true
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let field = self.focused_field();
        if !field.is_text() {
            return;
        }
        let len = self.value(field).chars().count();
        let next = (self.cursor(field) as isize + delta).clamp(0, len as isize) as usize;
        match field {
            Field::Password => self.password_caret = next,
            _ => self.carets[field.index()] = next,
        }
    }

    pub fn cursor_home(&mut self) {
        let field = self.focused_field();
        match field {
            Field::Password => self.password_caret = 0,
            f if f.base_index().is_some() => self.carets[f.index()] = 0,
            _ => {}
        }
    }

    pub fn cursor_end(&mut self) {
        let field = self.focused_field();
        match field {
            Field::Password => self.password_caret = self.password.chars().count(),
            f if f.base_index().is_some() => {
                self.carets[f.index()] = self.value(f).chars().count();
            }
            _ => {}
        }
    }

    /// Move focus `delta` fields forward, wrapping across visible rows.
    fn focus_by(&mut self, delta: isize) {
        let fields = self.visible_fields();
        let len = fields.len() as isize;
        let i = fields.iter().position(|&f| f == self.focus).unwrap_or(0) as isize;
        let next = (i + delta).rem_euclid(len) as usize;
        self.focus = fields[next];
        if self.focus != Field::AuthMethod {
            self.close_auth_menu();
        }
        if self.focus != Field::Identity {
            self.close_identity_menu();
        }
        self.error = None;
    }

    /// Focus a specific field (mouse click into an input).
    pub fn focus_field(&mut self, field: Field) {
        if self.visible_fields().contains(&field) {
            self.focus = field;
            if field != Field::AuthMethod {
                self.close_auth_menu();
            }
            if field != Field::Identity {
                self.close_identity_menu();
            }
            self.error = None;
        }
    }

    /// Route one input. `text` is only read for [`FormInput::Text`].
    pub fn handle_input(&mut self, input: FormInput, text: &str) -> FormOutcome {
        use FormOutcome::{Cancel, Consumed, Submit};
        match input {
            FormInput::Text => {
                let _ = self.insert(text);
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
            FormInput::Left => match self.focused_field() {
                Field::AuthMethod => {
                    self.cycle_auth_method(-1);
                    Consumed
                }
                Field::Identity => {
                    self.cycle_identity(-1);
                    Consumed
                }
                _ => {
                    self.move_cursor(-1);
                    Consumed
                }
            },
            FormInput::Right => match self.focused_field() {
                Field::AuthMethod => {
                    self.cycle_auth_method(1);
                    Consumed
                }
                Field::Identity => {
                    self.cycle_identity(1);
                    Consumed
                }
                _ => {
                    self.move_cursor(1);
                    Consumed
                }
            },
            FormInput::Home => {
                self.cursor_home();
                Consumed
            }
            FormInput::End => {
                self.cursor_end();
                Consumed
            }
            FormInput::Enter => Submit,
            FormInput::Escape => {
                if self.auth_menu_open {
                    self.close_auth_menu();
                    Consumed
                } else if self.identity_menu_open {
                    self.close_identity_menu();
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
    pub fn centered(width: f32, height: f32, dialog_height: f32) -> Self {
        Self {
            x: ((width - WIDTH) / 2.0).max(0.0),
            y: ((height - dialog_height) / 2.0).max(0.0),
        }
    }

    pub fn rect(&self, dialog_height: f32) -> Rect {
        Rect::new(self.x, self.y, WIDTH, dialog_height)
    }

    pub fn title_rect(&self) -> Rect {
        Rect::new(self.x + PAD, self.y + PAD, WIDTH - 2.0 * PAD, TITLE_HEIGHT)
    }

    fn row_top(&self, row: usize) -> f32 {
        self.y + PAD + TITLE_HEIGHT + row as f32 * (FIELD_HEIGHT + FIELD_GAP)
    }

    /// The field row for `field`, or `None` when that auth detail is hidden.
    pub fn field_rect(&self, form: &AddHostForm, field: Field) -> Option<Rect> {
        let row = form.visible_fields().iter().position(|&f| f == field)?;
        Some(Rect::new(
            self.x + PAD,
            self.row_top(row),
            WIDTH - 2.0 * PAD,
            FIELD_HEIGHT,
        ))
    }

    /// The bordered input box inside a field row, below its caption.
    pub fn input_rect(&self, form: &AddHostForm, field: Field) -> Option<Rect> {
        let row = self.field_rect(form, field)?;
        Some(Rect::new(row.x, row.y + INPUT_TOP, row.width, INPUT_HEIGHT))
    }

    /// Editable text region of the password input (excludes the eye slot).
    pub fn password_text_rect(&self, form: &AddHostForm) -> Option<Rect> {
        let input = self.input_rect(form, Field::Password)?;
        Some(Rect::new(
            input.x,
            input.y,
            (input.width - crate::settings::FIELD_EYE_SLOT).max(0.0),
            input.height,
        ))
    }

    /// Eye toggle on the right of the password input.
    pub fn password_toggle_rect(&self, form: &AddHostForm) -> Option<Rect> {
        let input = self.input_rect(form, Field::Password)?;
        Some(Rect::new(
            input.right() - crate::settings::FIELD_EYE_SLOT,
            input.y,
            crate::settings::FIELD_EYE_SLOT,
            input.height,
        ))
    }

    /// The caption line above a field's input box.
    pub fn caption_rect(&self, form: &AddHostForm, field: Field) -> Option<Rect> {
        let row = self.field_rect(form, field)?;
        Some(Rect::new(row.x + 2.0, row.y, row.width - 4.0, INPUT_TOP))
    }

    /// The hint / error line under the fields.
    pub fn hint_rect(&self, dialog_height: f32) -> Rect {
        Rect::new(
            self.x + PAD,
            self.y + dialog_height - PAD - HINT_HEIGHT,
            WIDTH - 2.0 * PAD,
            HINT_HEIGHT,
        )
    }

    /// Y of the Cancel / Connect row (shared by paint + hit-test).
    pub fn button_y(&self, dialog_height: f32) -> f32 {
        self.hint_rect(dialog_height).y + 6.0
    }

    pub fn connect_button_rect(&self, dialog_height: f32) -> Rect {
        let dialog = self.rect(dialog_height);
        Rect::new(
            dialog.right() - PAD - CONNECT_BUTTON_WIDTH,
            self.button_y(dialog_height),
            CONNECT_BUTTON_WIDTH,
            BUTTON_HEIGHT,
        )
    }

    pub fn cancel_button_rect(&self, dialog_height: f32) -> Rect {
        let connect = self.connect_button_rect(dialog_height);
        Rect::new(
            connect.x - BUTTON_GAP - CANCEL_BUTTON_WIDTH,
            connect.y,
            CANCEL_BUTTON_WIDTH,
            BUTTON_HEIGHT,
        )
    }

    /// Floating dropdown panel under the Authentication Method input.
    pub fn auth_menu_rect(&self, form: &AddHostForm) -> Option<Rect> {
        let input = self.input_rect(form, Field::AuthMethod)?;
        let row_h = 32.0;
        Some(Rect::new(
            input.x,
            input.bottom() + 4.0,
            input.width,
            AUTH_METHODS.len() as f32 * row_h + 8.0,
        ))
    }

    pub fn auth_option_rect(&self, form: &AddHostForm, index: usize) -> Option<Rect> {
        let menu = self.auth_menu_rect(form)?;
        Some(Rect::new(
            menu.x + 4.0,
            menu.y + 4.0 + index as f32 * 32.0,
            menu.width - 8.0,
            32.0,
        ))
    }

    /// Floating dropdown panel under the Select Saved SSH Key input.
    pub fn identity_menu_rect(&self, form: &AddHostForm) -> Option<Rect> {
        if !form.shows_identity() {
            return None;
        }
        let input = self.input_rect(form, Field::Identity)?;
        let n = form.identities().len().max(1) as f32;
        let row_h = 32.0;
        Some(Rect::new(
            input.x,
            input.bottom() + 4.0,
            input.width,
            n * row_h + 8.0,
        ))
    }

    pub fn identity_option_rect(&self, form: &AddHostForm, index: usize) -> Option<Rect> {
        if index >= form.identities().len() {
            return None;
        }
        let menu = self.identity_menu_rect(form)?;
        Some(Rect::new(
            menu.x + 4.0,
            menu.y + 4.0 + index as f32 * 32.0,
            menu.width - 8.0,
            32.0,
        ))
    }

    /// Hit-test inside an open dialog. Coordinates are logical pixels.
    pub fn hit_test(&self, form: &AddHostForm, x: f32, y: f32) -> AddHostHit {
        let dialog_height = form.height();
        let dialog = self.rect(dialog_height);
        // Auth / identity dropdowns may extend past the dialog bottom; still
        // accept option hits so the popover stays clickable.
        if form.auth_menu_open() {
            if let Some(menu) = self.auth_menu_rect(form) {
                if menu.contains(x, y) {
                    for i in 0..AUTH_METHODS.len() {
                        if let Some(opt) = self.auth_option_rect(form, i) {
                            if opt.contains(x, y) {
                                return AddHostHit::SelectAuth(i);
                            }
                        }
                    }
                    return AddHostHit::Consume;
                }
            }
        }
        if form.identity_menu_open() {
            if let Some(menu) = self.identity_menu_rect(form) {
                if menu.contains(x, y) {
                    for i in 0..form.identities().len() {
                        if let Some(opt) = self.identity_option_rect(form, i) {
                            if opt.contains(x, y) {
                                return AddHostHit::SelectIdentity(i);
                            }
                        }
                    }
                    return AddHostHit::Consume;
                }
            }
        }
        if !dialog.contains(x, y) {
            return AddHostHit::Consume;
        }
        if self.connect_button_rect(dialog_height).contains(x, y) {
            return AddHostHit::Connect;
        }
        if self.cancel_button_rect(dialog_height).contains(x, y) {
            return AddHostHit::Cancel;
        }
        for field in form.visible_fields() {
            if field == Field::Password {
                if let Some(eye) = self.password_toggle_rect(form) {
                    if eye.contains(x, y) {
                        return AddHostHit::TogglePasswordVisible;
                    }
                }
                if let Some(text) = self.password_text_rect(form) {
                    if text.contains(x, y) {
                        return AddHostHit::Field(Field::Password);
                    }
                }
                continue;
            }
            if let Some(input) = self.input_rect(form, field) {
                if input.contains(x, y) {
                    return match field {
                        Field::AuthMethod => AddHostHit::ToggleAuthMenu,
                        Field::Identity => AddHostHit::ToggleIdentityMenu,
                        other => AddHostHit::Field(other),
                    };
                }
            }
        }
        AddHostHit::Consume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_form() -> AddHostForm {
        let mut form = AddHostForm::default();
        form.open();
        form
    }

    fn type_into(form: &mut AddHostForm, text: &str) {
        for ch in text.chars() {
            let buf = ch.to_string();
            form.handle_input(FormInput::Text, &buf);
        }
    }

    #[test]
    fn typing_lands_in_the_focused_field() {
        let mut form = open_form();
        type_into(&mut form, "web-01");
        assert_eq!(form.value(Field::Name), "web-01");
        assert_eq!(form.value(Field::Hostname), "");
        assert_eq!(form.cursor(Field::Name), 6);

        form.handle_input(FormInput::Next, "");
        type_into(&mut form, "example.com");
        assert_eq!(form.focused_field(), Field::Hostname);
        assert_eq!(form.value(Field::Hostname), "example.com");
        assert_eq!(form.value(Field::Name), "web-01");
    }

    #[test]
    fn tab_wraps_around_the_visible_fields() {
        let mut form = open_form();
        let n = form.visible_fields().len();
        for _ in 0..n {
            form.handle_input(FormInput::Next, "");
        }
        assert_eq!(form.focused_field(), Field::Name);

        form.handle_input(FormInput::Previous, "");
        assert_eq!(form.focused_field(), Field::Identity);
    }

    #[test]
    fn tab_order_includes_auth_and_conditional_detail() {
        let mut form = open_form();
        form.focus_field(Field::Port);
        form.handle_input(FormInput::Next, "");
        assert_eq!(form.focused_field(), Field::AuthMethod);
        form.handle_input(FormInput::Next, "");
        assert_eq!(form.focused_field(), Field::Identity);

        form.cycle_auth_method(1); // password
        assert_eq!(form.auth_method(), "password");
        assert!(form.visible_fields().contains(&Field::Password));
        assert!(!form.visible_fields().contains(&Field::Identity));
        assert_eq!(form.focused_field(), Field::Password);

        form.cycle_auth_method(1); // gssapi
        assert_eq!(form.auth_method(), "gssapi");
        assert!(!form.shows_identity());
        assert!(!form.shows_password());
        assert_eq!(form.focused_field(), Field::AuthMethod);
    }

    #[test]
    fn backspace_edits_at_the_caret_not_the_tail() {
        let mut form = open_form();
        type_into(&mut form, "web-99");
        form.handle_input(FormInput::Left, "");
        form.handle_input(FormInput::Left, "");
        // Caret now sits after "web", so backspace eats the '-'.
        assert!(form.backspace());
        assert_eq!(form.value(Field::Name), "web99");
        assert_eq!(form.cursor(Field::Name), 3);
    }

    #[test]
    fn backspace_at_the_beginning_is_not_an_edit() {
        let mut form = open_form();
        type_into(&mut form, "abc");
        form.handle_input(FormInput::Home, "");
        assert!(!form.backspace());
        assert_eq!(form.value(Field::Name), "abc");
    }

    #[test]
    fn delete_removes_the_character_after_the_caret() {
        let mut form = open_form();
        type_into(&mut form, "abc");
        form.handle_input(FormInput::Home, "");
        assert!(form.delete());
        assert_eq!(form.value(Field::Name), "bc");
        assert_eq!(form.cursor(Field::Name), 0);
        // Caret already at the end: nothing left to delete.
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
        assert_eq!(form.value(Field::Name), "web-01");
        assert_eq!(form.cursor(Field::Name), 4);
    }

    #[test]
    fn carets_are_character_indices_not_byte_offsets() {
        let mut form = open_form();
        // Multi-byte characters must not split a code point.
        type_into(&mut form, "café");
        assert_eq!(form.value(Field::Name), "café");
        assert!(form.backspace());
        assert_eq!(form.value(Field::Name), "caf");
        form.handle_input(FormInput::Home, "");
        assert_eq!(form.cursor_byte(Field::Name), 0);
        assert!(form.insert("é"));
        assert_eq!(form.value(Field::Name), "écaf");
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
        assert_eq!(form.value(Field::Name), "");
        // But the sink still swallows the keystroke.
        assert_eq!(
            form.handle_input(FormInput::Text, "\u{7f}"),
            FormOutcome::Consumed
        );
    }

    #[test]
    fn enter_submits_and_escape_cancels() {
        let mut form = open_form();
        assert_eq!(form.handle_input(FormInput::Enter, ""), FormOutcome::Submit);
        assert_eq!(
            form.handle_input(FormInput::Escape, ""),
            FormOutcome::Cancel
        );
    }

    #[test]
    fn a_rejection_keeps_the_form_open_with_its_text() {
        let mut form = open_form();
        type_into(&mut form, "web-01");
        form.set_error("'abc' is not a valid port");
        assert!(form.is_open());
        assert_eq!(form.value(Field::Name), "web-01");
        assert_eq!(form.error(), Some("'abc' is not a valid port"));

        // Editing clears the stale message.
        form.handle_input(FormInput::Text, "2");
        assert_eq!(form.error(), None);
    }

    #[test]
    fn opening_always_starts_from_an_empty_form() {
        let mut form = open_form();
        type_into(&mut form, "stale");
        form.set_error("boom");
        form.cycle_auth_method(1);
        form.open();
        assert_eq!(form.values(), HostFormValues::default());
        assert_eq!(form.focused_field(), Field::Name);
        assert_eq!(form.error(), None);
        assert_eq!(form.auth_method(), "key");
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

    #[test]
    fn open_edit_prefills_and_marks_editing() {
        let mut form = AddHostForm::default();
        form.set_identities(vec![("k1".into(), "Prod".into())]);
        form.open_edit(
            HostFormValues {
                name: "web".into(),
                hostname: "web.example".into(),
                username: "deploy".into(),
                port: "2222".into(),
                auth_method: "key".into(),
                identity_id: Some("k1".into()),
                password: String::new(),
            },
            "host-id".into(),
        );
        assert!(form.is_open());
        assert!(form.is_editing());
        assert_eq!(form.editing_id(), Some("host-id"));
        assert_eq!(form.values().name, "web");
        assert_eq!(form.values().hostname, "web.example");
        assert_eq!(form.values().port, "2222");
        assert_eq!(form.identity_id(), Some("k1"));
        assert_eq!(form.auth_validation_error(), None);
        form.select_auth_method(
            AUTH_METHODS
                .iter()
                .position(|&m| m == "password")
                .expect("password"),
        );
        // Empty password allowed while editing.
        assert_eq!(form.auth_validation_error(), None);
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
        type_into(&mut form, "web-01");
        form.focus_by(1);
        type_into(&mut form, "web-01.example.com");
        form.focus_by(1);
        type_into(&mut form, "deploy");
        form.focus_by(1);
        type_into(&mut form, "2222");
        form.cycle_auth_method(1);
        form.focus_field(Field::Password);
        type_into(&mut form, "s3cret");

        assert_eq!(
            form.values(),
            HostFormValues {
                name: "web-01".to_string(),
                hostname: "web-01.example.com".to_string(),
                username: "deploy".to_string(),
                port: "2222".to_string(),
                auth_method: "password".to_string(),
                identity_id: Some("k1".to_string()),
                password: "s3cret".to_string(),
            }
        );
    }

    #[test]
    fn left_right_cycle_auth_and_identity() {
        let mut form = open_form();
        form.set_identities(vec![("a".into(), "A".into()), ("b".into(), "B".into())]);
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
    }

    #[test]
    fn auth_validation_helpers() {
        let mut form = open_form();
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
    fn height_changes_per_auth_method() {
        let mut form = open_form();
        let key_h = form.height();
        form.cycle_auth_method(1);
        let password_h = form.height();
        form.cycle_auth_method(1);
        let gssapi_h = form.height();

        assert_eq!(key_h, password_h, "key and password both show a detail row");
        assert!(gssapi_h < key_h, "gssapi has no detail row");
        assert_eq!(
            key_h - gssapi_h,
            FIELD_HEIGHT + FIELD_GAP,
            "one field row difference"
        );
    }

    #[test]
    fn fields_are_laid_out_inside_the_dialog_and_do_not_overlap() {
        let form = open_form();
        let layout = AddHostLayout::centered(1200.0, 800.0, form.height());
        let dialog = layout.rect(form.height());
        let visible = form.visible_fields();

        for &field in &visible {
            let rect = layout.field_rect(&form, field).expect("visible");
            assert!(rect.x >= dialog.x && rect.right() <= dialog.right());
            assert!(rect.y >= dialog.y && rect.bottom() <= dialog.bottom());
        }

        for pair in visible.windows(2) {
            let a = layout.field_rect(&form, pair[0]).unwrap();
            let b = layout.field_rect(&form, pair[1]).unwrap();
            assert!(a.bottom() <= b.y, "{:?} overlaps {:?}", pair[0], pair[1]);
        }

        let last = *visible.last().unwrap();
        let hint = layout.hint_rect(form.height());
        assert!(hint.y >= layout.field_rect(&form, last).unwrap().bottom());
        assert!(hint.bottom() <= dialog.bottom());
    }

    #[test]
    fn the_dialog_is_centered() {
        let form = open_form();
        let layout = AddHostLayout::centered(1200.0, 800.0, form.height());
        let dialog = layout.rect(form.height());
        assert!((dialog.x - (1200.0 - WIDTH) / 2.0).abs() < f32::EPSILON);
        assert!((dialog.y - (800.0 - form.height()) / 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_window_smaller_than_the_dialog_does_not_go_negative() {
        let form = open_form();
        let layout = AddHostLayout::centered(100.0, 60.0, form.height());
        assert_eq!(layout.x, 0.0);
        assert_eq!(layout.y, 0.0);
    }

    #[test]
    fn auth_dropdown_toggle_and_select() {
        let mut form = open_form();
        let layout = AddHostLayout::centered(1200.0, 800.0, form.height());
        let auth = layout.input_rect(&form, Field::AuthMethod).unwrap();
        assert_eq!(
            layout.hit_test(&form, auth.x + 2.0, auth.y + 2.0),
            AddHostHit::ToggleAuthMenu
        );
        form.toggle_auth_menu();
        assert!(form.auth_menu_open());
        let opt = layout.auth_option_rect(&form, 1).unwrap();
        assert_eq!(
            layout.hit_test(&form, opt.x + 2.0, opt.y + 2.0),
            AddHostHit::SelectAuth(1)
        );
        form.select_auth_method(1);
        assert_eq!(form.auth_method(), "password");
        assert!(!form.auth_menu_open());
        assert!(form.visible_fields().contains(&Field::Password));
    }

    #[test]
    fn identity_dropdown_toggle_and_select() {
        let mut form = open_form();
        form.set_identities(vec![
            ("k1".into(), "Prod".into()),
            ("k2".into(), "Staging".into()),
        ]);
        let layout = AddHostLayout::centered(1200.0, 800.0, form.height());
        let identity = layout.input_rect(&form, Field::Identity).unwrap();
        assert_eq!(
            layout.hit_test(&form, identity.x + 2.0, identity.y + 2.0),
            AddHostHit::ToggleIdentityMenu
        );
        form.toggle_identity_menu();
        assert!(form.identity_menu_open());
        assert!(!form.auth_menu_open());
        let opt = layout.identity_option_rect(&form, 1).unwrap();
        assert_eq!(
            layout.hit_test(&form, opt.x + 2.0, opt.y + 2.0),
            AddHostHit::SelectIdentity(1)
        );
        form.select_identity(1);
        assert_eq!(form.identity_id(), Some("k2"));
        assert_eq!(form.selected_identity_name(), Some("Staging"));
        assert!(!form.identity_menu_open());
    }

    #[test]
    fn hit_test_finds_fields_and_footer_buttons() {
        let form = open_form();
        let layout = AddHostLayout::centered(1200.0, 800.0, form.height());

        let name = layout.input_rect(&form, Field::Name).unwrap();
        assert_eq!(
            layout.hit_test(&form, name.x + 2.0, name.y + 2.0),
            AddHostHit::Field(Field::Name)
        );

        let auth = layout.input_rect(&form, Field::AuthMethod).unwrap();
        assert_eq!(
            layout.hit_test(&form, auth.x + 2.0, auth.y + 2.0),
            AddHostHit::ToggleAuthMenu
        );

        let identity = layout.input_rect(&form, Field::Identity).unwrap();
        assert_eq!(
            layout.hit_test(&form, identity.x + 2.0, identity.y + 2.0),
            AddHostHit::ToggleIdentityMenu
        );

        let h = form.height();
        let cancel = layout.cancel_button_rect(h);
        assert_eq!(
            layout.hit_test(&form, cancel.x + 2.0, cancel.y + 2.0),
            AddHostHit::Cancel
        );

        let connect = layout.connect_button_rect(h);
        assert_eq!(
            layout.hit_test(&form, connect.x + 2.0, connect.y + 2.0),
            AddHostHit::Connect
        );

        let title = layout.title_rect();
        assert_eq!(
            layout.hit_test(&form, title.x + 2.0, title.y + 2.0),
            AddHostHit::Consume
        );
    }

    #[test]
    fn hit_test_finds_password_when_selected() {
        let mut form = open_form();
        form.cycle_auth_method(1);
        let layout = AddHostLayout::centered(1200.0, 800.0, form.height());
        let password = layout.input_rect(&form, Field::Password).unwrap();
        assert_eq!(
            layout.hit_test(&form, password.x + 2.0, password.y + 2.0),
            AddHostHit::Field(Field::Password)
        );
        assert!(layout.input_rect(&form, Field::Identity).is_none());
    }

    #[test]
    fn hit_test_password_eye_toggles_visibility() {
        let mut form = open_form();
        form.cycle_auth_method(1); // password
        let layout = AddHostLayout::centered(1200.0, 800.0, form.height());
        let eye = layout.password_toggle_rect(&form).expect("eye slot");
        assert_eq!(
            layout.hit_test(&form, eye.x + 2.0, eye.y + 2.0),
            AddHostHit::TogglePasswordVisible
        );
        assert!(!form.password_visible());
        form.toggle_password_visible();
        assert!(form.password_visible());
    }

    #[test]
    fn focus_field_jumps_without_wrapping() {
        let mut form = open_form();
        form.focus_field(Field::Port);
        assert_eq!(form.focused_field(), Field::Port);
        form.focus_field(Field::Hostname);
        assert_eq!(form.focused_field(), Field::Hostname);
        form.focus_field(Field::AuthMethod);
        assert_eq!(form.focused_field(), Field::AuthMethod);
    }
}
