use super::*;

/// A managed SSH key as Settings and the add-host picker list it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRow {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    pub created: String,
    /// OpenSSH public key line (empty when none is stored).
    pub public_key: String,
}

pub(super) fn list_identities(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
) -> HostEvent {
    match runtime.block_on(store.list_identities()) {
        Ok(idents) => {
            let rows: Vec<IdentityRow> = idents
                .into_iter()
                .filter(|i| i.deleted_at.is_none())
                .map(|i| {
                    let fingerprint = i
                        .public_key
                        .as_deref()
                        .map(terminus_core::fingerprint_from_public_openssh)
                        .unwrap_or_else(|| "no public key".into());
                    IdentityRow {
                        id: i.id.to_string(),
                        name: i.name,
                        fingerprint,
                        created: i.created_at.format("%Y-%m-%d").to_string(),
                        public_key: i.public_key.unwrap_or_default(),
                    }
                })
                .collect();
            HostEvent::IdentitiesLoaded(rows)
        }
        Err(err) => HostEvent::Failed(format!("Could not read identities: {err}")),
    }
}

/// The import field takes a pasted private key or the path to one
/// (`~/.ssh/id_ed25519`): most people already have a key on disk.
pub(super) fn resolve_private_key_input(
    input: &str,
    home: Option<&Path>,
) -> Result<String, String> {
    let input = input.trim();
    if input.contains("-----BEGIN") {
        return Ok(input.to_string());
    }
    let path = match (input.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(input),
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|_| format!("No file at {input}. Paste the key or its path."))?;
    if text.contains("-----BEGIN") {
        Ok(text)
    } else if text.trim_start().starts_with("ssh-") || input.ends_with(".pub") {
        Err(format!(
            "{input} is the public half. Use the file without .pub."
        ))
    } else {
        Err(format!("{input} is not an OpenSSH private key."))
    }
}

pub(super) fn create_ssh_key(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    vault: Option<&terminus_core::UnlockedVault>,
    name: &str,
    pem: Option<&str>,
    passphrase: Option<&str>,
) -> Result<String, String> {
    // Private keys are sealed at rest like host passwords, so saving one
    // needs the vault.
    let Some(vault) = vault else {
        return Err(msg::VAULT_FOR_SSH_KEY.into());
    };
    let identity = match pem.map(str::trim).filter(|p| !p.is_empty()) {
        Some(input) => {
            let pem = resolve_private_key_input(input, dirs::home_dir().as_deref())?;
            terminus_core::import_openssh_identity(name, &pem, passphrase)
                .map_err(|e| e.to_string())?
        }
        None => {
            terminus_core::generate_ed25519_identity(name).map_err(|e| e.to_string())?
        }
    };
    let label = identity.name.clone();
    let imported = pem.map(str::trim).is_some_and(|p| !p.is_empty());
    let sealed =
        terminus_core::seal_identity(vault, &identity).map_err(|e| e.to_string())?;
    runtime
        .block_on(store.upsert_identity(&sealed))
        .map_err(|e| e.to_string())?;
    Ok(if imported {
        format!("SSH key “{label}” imported")
    } else {
        format!("SSH key “{label}” created")
    })
}

pub(super) fn delete_ssh_key(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
) -> Result<String, String> {
    let uuid = uuid::Uuid::parse_str(id).map_err(|_| "Invalid SSH key id".to_string())?;
    let existing = runtime
        .block_on(store.get_identity(uuid))
        .map_err(|e| e.to_string())?;
    let Some(ident) = existing else {
        return Err("SSH key not found".into());
    };
    let label = ident.name.clone();
    runtime
        .block_on(store.delete_identity(uuid))
        .map_err(|e| e.to_string())?;
    Ok(format!("SSH key “{label}” deleted"))
}

pub(super) fn delete_host(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
) -> Result<String, String> {
    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid host id".to_string())?;
    let hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|err| format!("Could not read hosts: {err}"))?;
    let label = hosts
        .iter()
        .find(|h| h.id == uuid && h.deleted_at.is_none())
        .map(|h| h.name.clone())
        .ok_or_else(|| "Host not found".to_string())?;
    runtime
        .block_on(store.delete_host(uuid))
        .map_err(|err| format!("Could not delete the host: {err}"))?;
    Ok(format!("Deleted {label}"))
}

pub(super) fn delete_group(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
) -> Result<String, String> {
    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid group id".to_string())?;
    let groups = runtime
        .block_on(store.list_groups())
        .map_err(|err| format!("Could not read groups: {err}"))?;
    let label = groups
        .iter()
        .find(|g| g.id == uuid && g.deleted_at.is_none())
        .map(|g| g.name.clone())
        .ok_or_else(|| "Group not found".to_string())?;
    runtime
        .block_on(store.delete_group(uuid))
        .map_err(|err| format!("Could not delete the group: {err}"))?;
    Ok(format!("Deleted group {label}"))
}

pub(super) fn rename_host(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
    name: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid host id".to_string())?;
    let mut hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|err| format!("Could not read hosts: {err}"))?;
    let host = hosts
        .iter_mut()
        .find(|h| h.id == uuid && h.deleted_at.is_none())
        .ok_or_else(|| "Host not found".to_string())?;
    host.name = name.to_string();
    host.updated_at = Utc::now();
    let label = host.name.clone();
    runtime
        .block_on(store.upsert_host(host))
        .map_err(|err| format!("Could not rename the host: {err}"))?;
    Ok(format!("Renamed {label}"))
}

pub(super) fn rename_group(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    id: &str,
    name: &str,
) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid group id".to_string())?;
    let mut groups = runtime
        .block_on(store.list_groups())
        .map_err(|err| format!("Could not read groups: {err}"))?;
    let group = groups
        .iter_mut()
        .find(|g| g.id == uuid && g.deleted_at.is_none())
        .ok_or_else(|| "Group not found".to_string())?;
    group.name = name.to_string();
    group.updated_at = Utc::now();
    let label = group.name.clone();
    runtime
        .block_on(store.upsert_group(group))
        .map_err(|err| format!("Could not rename the group: {err}"))?;
    Ok(format!("Renamed group {label}"))
}

pub(super) fn list_groups(runtime: &tokio::runtime::Runtime, store: &Store) -> HostEvent {
    match runtime.block_on(store.list_groups()) {
        Ok(groups) => {
            let mut rows: Vec<(i64, String, String)> = groups
                .into_iter()
                .filter(|group| group.deleted_at.is_none())
                .map(|group| (group.sort_order, group.id.to_string(), group.name))
                .collect();
            rows.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| a.2.to_lowercase().cmp(&b.2.to_lowercase()))
            });
            HostEvent::GroupsLoaded(
                rows.into_iter()
                    .map(|(order, id, name)| (id, name, order))
                    .collect(),
            )
        }
        Err(err) => HostEvent::Failed(format!("Could not read groups: {err}")),
    }
}

pub(super) fn list(runtime: &tokio::runtime::Runtime, store: &Store) -> HostEvent {
    match runtime.block_on(store.list_hosts()) {
        Ok(hosts) => {
            let mut rows: Vec<HostRow> = hosts
                .iter()
                .filter(|host| host.deleted_at.is_none())
                .map(HostRow::from_host)
                .collect();
            rows.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.hostname.cmp(&b.hostname))
            });
            HostEvent::Loaded(rows)
        }
        Err(err) => HostEvent::Failed(format!("Could not read hosts: {err}")),
    }
}

pub(super) fn set_host_group(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    host_id: &str,
    group_id: Option<&str>,
) -> Result<String, String> {
    let id = Uuid::parse_str(host_id).map_err(|_| "Invalid host id".to_string())?;
    let group_uuid = match group_id {
        Some(g) => Some(Uuid::parse_str(g).map_err(|_| "Invalid group id".to_string())?),
        None => None,
    };
    let mut hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|err| format!("Could not read hosts: {err}"))?;
    let host = hosts
        .iter_mut()
        .find(|h| h.id == id && h.deleted_at.is_none())
        .ok_or_else(|| "Host not found".to_string())?;
    host.group_id = group_uuid;
    host.sort_order = runtime
        .block_on(store.next_host_sort_order(group_uuid))
        .map_err(|err| format!("Could not move the host: {err}"))?;
    host.updated_at = Utc::now();
    let label = host.name.clone();
    runtime
        .block_on(store.upsert_host(host))
        .map_err(|err| format!("Could not move the host: {err}"))?;
    Ok(format!("Moved {label}"))
}

pub(super) fn reorder_host(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    host_id: &str,
    before_host_id: Option<&str>,
    before_group_id: Option<&str>,
) -> Result<String, String> {
    let id = Uuid::parse_str(host_id).map_err(|_| "Invalid host id".to_string())?;
    let (before_is_group, before_id) = if let Some(b) = before_group_id {
        (
            Some(true),
            Some(Uuid::parse_str(b).map_err(|_| "Invalid group id".to_string())?),
        )
    } else if let Some(b) = before_host_id {
        (
            Some(false),
            Some(Uuid::parse_str(b).map_err(|_| "Invalid host id".to_string())?),
        )
    } else {
        (None, None)
    };
    runtime
        .block_on(store.reorder_root(false, id, before_is_group, before_id))
        .map_err(|err| format!("Could not reorder the host: {err}"))?;
    Ok("Reordered".to_string())
}

pub(super) fn reorder_group(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    group_id: &str,
    before_group_id: Option<&str>,
    before_host_id: Option<&str>,
) -> Result<String, String> {
    let id = Uuid::parse_str(group_id).map_err(|_| "Invalid group id".to_string())?;
    let (before_is_group, before_id) = if let Some(b) = before_host_id {
        (
            Some(false),
            Some(Uuid::parse_str(b).map_err(|_| "Invalid host id".to_string())?),
        )
    } else if let Some(b) = before_group_id {
        (
            Some(true),
            Some(Uuid::parse_str(b).map_err(|_| "Invalid group id".to_string())?),
        )
    } else {
        (None, None)
    };
    runtime
        .block_on(store.reorder_root(true, id, before_is_group, before_id))
        .map_err(|err| format!("Could not reorder the group: {err}"))?;
    Ok("Reordered group".to_string())
}

pub(super) fn list_snippets(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
) -> HostEvent {
    let mut mapped = Vec::new();
    if let Ok(snippets) = runtime.block_on(store.list_snippets()) {
        for s in snippets {
            mapped.push(terminus_ui::snippets::SnippetItem {
                id: s.id.to_string(),
                name: s.title,
                cmd: s.content,
                desc: s.shortcut.unwrap_or_default(),
                host_id: s.host_id.map(|h| h.to_string()),
            });
        }
    }
    HostEvent::SnippetsLoaded(mapped)
}
