use super::*;

impl AddHostForm {
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

    pub(super) fn toggle_menu(&mut self, menu: SelectMenu, field: Field) {
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

    pub(super) fn draft_mut(&mut self, field: Field) -> Option<&mut TextDraft> {
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
    pub(super) fn effective_port(&self, from_address: Option<String>) -> String {
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

    pub(super) fn clamp_focus_to_visible(&mut self) {
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
}

/// Byte offset of the `chars`-th character (or the length).
fn char_byte_offset(value: &str, chars: usize) -> usize {
    value
        .char_indices()
        .nth(chars)
        .map(|(byte, _)| byte)
        .unwrap_or(value.len())
}
