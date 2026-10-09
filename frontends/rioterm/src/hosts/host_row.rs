use super::*;

/// A host as the sidebar needs it — no secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRow {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: String,
    /// `key` | `password` | `gssapi`.
    pub auth_method: String,
    pub identity_id: Option<String>,
    pub group_id: Option<String>,
    pub tags: Vec<String>,
    pub notes: String,
    pub os_id: Option<String>,
    /// Manual order among peers (same group / ungrouped). Lower first.
    pub sort_order: i64,
    /// Used as a tie-break after sort_order within a group.
    pub updated_at: DateTime<Utc>,
}

/// Ids of the hosts filed under `group_id`, in list order.
pub fn host_ids_in_group(hosts: &[HostRow], group_id: &str) -> Vec<String> {
    hosts
        .iter()
        .filter(|host| host.group_id.as_deref() == Some(group_id))
        .map(|host| host.id.clone())
        .collect()
}

impl HostRow {
    /// What an open SFTP connection to this host depends on: address,
    /// port, user, auth method and key. A change makes it stale.
    pub fn connection_key(&self) -> String {
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.hostname,
            self.port,
            self.username,
            self.auth_method,
            self.identity_id.as_deref().unwrap_or("")
        )
    }

    pub(super) fn from_host(host: &Host) -> Self {
        Self {
            id: host.id.to_string(),
            name: host.name.clone(),
            hostname: host.hostname.clone(),
            port: host.port,
            username: host.username.clone(),
            auth_method: host.auth_method.clone(),
            identity_id: host.identity_id.map(|id| id.to_string()),
            group_id: host.group_id.map(|id| id.to_string()),
            tags: host.tags.clone(),
            notes: host.notes.clone(),
            os_id: host.os_id.clone(),
            sort_order: host.sort_order,
            updated_at: host.updated_at,
        }
    }

    /// `user@host:port`, with the port dropped when it is the SSH default.
    pub fn endpoint(&self) -> String {
        let user = if self.username.is_empty() {
            String::new()
        } else {
            format!("{}@", self.username)
        };
        if self.port == DEFAULT_PORT {
            format!("{}{}", user, self.hostname)
        } else {
            format!("{}{}:{}", user, self.hostname, self.port)
        }
    }

    /// The OpenSSH command that reaches this host from any terminal.
    pub fn ssh_command(&self) -> String {
        let target = if self.username.is_empty() {
            self.hostname.clone()
        } else {
            format!("{}@{}", self.username, self.hostname)
        };
        if self.port == DEFAULT_PORT {
            format!("ssh {target}")
        } else {
            format!("ssh -p {} {target}", self.port)
        }
    }
}
