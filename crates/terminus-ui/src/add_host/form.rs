use super::*;

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

    pub(super) fn enter_step(&mut self, step: AddHostStep) {
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

    pub(super) fn reset_values(&mut self) {
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

    pub(super) fn clear_error(&mut self) {
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
}
