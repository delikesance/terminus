use super::*;

/// What the host editor collected, before it becomes a stored [`Host`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostDraft {
    /// When set, update this host instead of creating a new one.
    pub id: Option<String>,
    pub name: String,
    pub hostname: String,
    pub username: String,
    /// Raw text, so an empty field can mean "the default port".
    pub port: String,
    /// `key` | `password` | `gssapi`.
    pub auth_method: String,
    /// Selected identity id when `auth_method == "key"`.
    pub identity_id: Option<String>,
    /// Plaintext password (memory only) when `auth_method == "password"`.
    /// Empty while editing means keep the existing sealed credential.
    pub password: String,
    /// Group the host is filed under; `None` keeps it at the root.
    pub group_id: Option<String>,
    pub tags: Vec<String>,
    pub notes: String,
}

/// Split the editor's comma separated tags: trimmed, no blanks, no repeats.
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in text.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
            tags.push(tag.to_string());
        }
    }
    tags
}

/// A value ssh would take for an option (`-oProxyCommand=…`) or split on.
pub(super) fn reject_option_like(label: &str, value: &str) -> Result<(), String> {
    if value.starts_with('-')
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(format!("{label} cannot start with '-' or contain spaces"));
    }
    Ok(())
}

impl HostDraft {
    /// Validate and normalise into a storable host.
    ///
    /// Empty `name`/`username` fall back to the hostname and `root`; only a
    /// missing hostname or an unparsable port are hard errors.
    pub fn normalize(&self) -> Result<HostDraft, String> {
        let hostname = self.hostname.trim();
        if hostname.is_empty() {
            return Err("Hostname is required".to_string());
        }
        reject_option_like("Hostname", hostname)?;
        let port = parse_port(&self.port)?;
        let name = match self.name.trim() {
            "" => hostname.to_string(),
            name => name.to_string(),
        };
        let username = match self.username.trim() {
            "" => "root".to_string(),
            user => user.to_string(),
        };
        reject_option_like("Username", &username)?;

        let method = match terminus_core::parse_host_auth_method(&self.auth_method) {
            Ok(ok) => ok.method,
            Err(_) => {
                // Empty / unset defaults to key (mock HIG default).
                if self.auth_method.trim().is_empty() {
                    terminus_core::HostAuthMethod::Key
                } else {
                    return Err(format!(
                        "Unknown authentication method '{}'",
                        self.auth_method.trim()
                    ));
                }
            }
        };

        let identity_id = match method {
            terminus_core::HostAuthMethod::Key => {
                let id = self
                    .identity_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                if id.is_none() {
                    return Err(msg::PICK_SSH_KEY.to_string());
                }
                id
            }
            _ => None,
        };

        let editing = self.id.is_some();
        let password = match method {
            terminus_core::HostAuthMethod::Password => {
                let pw = self.password.clone();
                if pw.is_empty() && !editing {
                    return Err("Password is required".to_string());
                }
                pw
            }
            _ => String::new(),
        };

        Ok(HostDraft {
            id: self.id.clone(),
            name,
            hostname: hostname.to_string(),
            username,
            port: port.to_string(),
            auth_method: method.as_str().to_string(),
            identity_id,
            password,
            group_id: self
                .group_id
                .as_deref()
                .map(str::trim)
                .filter(|g| !g.is_empty())
                .map(str::to_string),
            tags: parse_tags(&self.tags.join(",")),
            notes: self.notes.trim().to_string(),
        })
    }

    /// Port as a number, with [`DEFAULT_PORT`] for an empty field.
    pub fn resolved_port(&self) -> Result<u16, String> {
        parse_port(&self.port)
    }

    /// `user@host[:port]`, as the stored row's [`HostRow::endpoint`] shows
    /// it. Names repeat (they default to the hostname); endpoints don't.
    pub fn endpoint(&self) -> String {
        let user = if self.username.is_empty() {
            String::new()
        } else {
            format!("{}@", self.username)
        };
        match self.resolved_port() {
            Ok(DEFAULT_PORT) | Err(_) => format!("{user}{}", self.hostname),
            Ok(port) => format!("{user}{}:{port}", self.hostname),
        }
    }
}

/// Parse the port field: empty means the SSH default.
pub fn parse_port(text: &str) -> Result<u16, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(DEFAULT_PORT);
    }
    text.parse::<u16>()
        .map_err(|_| format!("'{text}' is not a valid port"))
}

/// Build the row the store will write. Password is never stored on the host
/// row — it is sealed into a credential after a successful probe.
pub(super) fn host_from_draft(draft: &HostDraft) -> Host {
    let now = Utc::now();
    let identity_id = draft
        .identity_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    Host {
        id: Uuid::new_v4(),
        name: draft.name.clone(),
        hostname: draft.hostname.clone(),
        port: draft.resolved_port().unwrap_or(DEFAULT_PORT),
        username: draft.username.clone(),
        auth_method: draft.auth_method.clone(),
        password: None,
        identity_id,
        group_id: draft
            .group_id
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok()),
        tags: draft.tags.clone(),
        notes: draft.notes.clone(),
        os_id: None,
        sort_order: 0,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}
