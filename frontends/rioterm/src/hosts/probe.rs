use super::*;

pub(super) fn probe_and_persist(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    vault: &mut Option<Arc<terminus_core::UnlockedVault>>,
    draft: HostDraft,
) -> Result<String, String> {
    use terminus_core::{
        probe_options_from_host, probe_ssh_auth, seal_host_password, HostAuthMethod,
    };

    let method = terminus_core::parse_host_auth_method(&draft.auth_method)
        .map(|ok| ok.method)
        .map_err(|e| format!("Unknown authentication method '{}'", e.raw))?;

    if method == HostAuthMethod::Password && vault.is_none() {
        return Err(msg::VAULT_FOR_PASSWORD.into());
    }

    let mut host = host_from_draft(&draft);
    host.sort_order = runtime
        .block_on(store.next_host_sort_order(host.group_id))
        .map_err(|e| format!("Could not save the host: {e}"))?;
    // Probe uses in-memory password when present.
    if method == HostAuthMethod::Password {
        host.password = Some(draft.password.clone());
    }

    let identity = match method {
        HostAuthMethod::Key => {
            let id = host
                .identity_id
                .ok_or_else(|| "Select a saved SSH key".to_string())?;
            Some(load_open_identity(runtime, store, vault.as_ref(), id)?)
        }
        _ => None,
    };

    let opts = probe_options_from_host(&host, identity.as_ref());
    if let Err(err) = runtime.block_on(probe_ssh_auth(&opts)) {
        return Err(err.user_message());
    }

    // Never persist plaintext password on the host row.
    host.password = None;
    host.updated_at = Utc::now();

    if let Ok(os_id) = runtime.block_on(terminus_core::detect_remote_os(&opts)) {
        if os_id != terminus_core::UNKNOWN_OS {
            host.os_id = Some(os_id);
        }
    }

    runtime
        .block_on(store.upsert_host(&host))
        .map_err(|e| format!("Could not save the host: {e}"))?;

    if method == HostAuthMethod::Password {
        let unlocked = vault
            .as_ref()
            .ok_or_else(|| "Unlock the vault before saving a password".to_string())?;
        let cred = seal_host_password(unlocked.as_ref(), host.id, &draft.password)
            .map_err(|e| format!("Could not encrypt the password: {e}"))?;
        runtime
            .block_on(store.upsert_credential(&cred))
            .map_err(|e| format!("Could not store the encrypted password: {e}"))?;
    }

    Ok(host.name)
}

pub(super) fn probe_and_update(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    vault: &mut Option<Arc<terminus_core::UnlockedVault>>,
    id: &str,
    draft: HostDraft,
) -> Result<String, String> {
    use terminus_core::{seal_host_password, HostAuthMethod, OWNER_KIND_HOST};

    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid host id".to_string())?;
    let mut hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|e| format!("Could not read hosts: {e}"))?;
    let Some(existing) = hosts.iter_mut().find(|h| h.id == uuid) else {
        return Err("Host not found".into());
    };

    let method = terminus_core::parse_host_auth_method(&draft.auth_method)
        .map(|ok| ok.method)
        .map_err(|e| format!("Unknown authentication method '{}'", e.raw))?;

    let identity_id = draft
        .identity_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());

    if method == HostAuthMethod::Key && identity_id.is_none() {
        return Err("Select a saved SSH key".into());
    }

    if method == HostAuthMethod::Password && draft.password.is_empty() {
        let creds = runtime
            .block_on(store.list_credentials_for_owner(OWNER_KIND_HOST, uuid))
            .map_err(|e| e.to_string())?;
        let has_pw = creds
            .iter()
            .any(|c| c.kind == terminus_core::CREDENTIAL_KIND_HOST_PASSWORD);
        if !has_pw {
            return Err("Enter a password to save".into());
        }
    }

    if method == HostAuthMethod::Password && !draft.password.is_empty() && vault.is_none()
    {
        return Err(msg::VAULT_FOR_PASSWORD.into());
    }

    existing.name = draft.name.clone();
    existing.hostname = draft.hostname.clone();
    existing.port = draft.resolved_port().unwrap_or(DEFAULT_PORT);
    existing.username = draft.username.clone();
    existing.auth_method = draft.auth_method.clone();
    existing.identity_id = identity_id;
    existing.password = None;
    let group_id = draft
        .group_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    if existing.group_id != group_id {
        // Joins the end of its new group.
        existing.sort_order = runtime
            .block_on(store.next_host_sort_order(group_id))
            .map_err(|e| format!("Could not save the host: {e}"))?;
    }
    existing.group_id = group_id;
    existing.tags = draft.tags.clone();
    existing.notes = draft.notes.clone();
    existing.updated_at = Utc::now();

    let label = existing.name.clone();
    let mut host = existing.clone();

    // Re-probe OS when credentials allow (best-effort; keep prior os_id on failure).
    {
        use terminus_core::{probe_options_from_host, HostAuthMethod as HAM};
        let mut probe_host = host.clone();
        if method == HAM::Password {
            if !draft.password.is_empty() {
                probe_host.password = Some(draft.password.clone());
            } else if let Ok(Some(pw)) =
                resolve_host_password(runtime, store, vault.as_ref(), id)
            {
                probe_host.password = Some(pw);
            }
        }
        let identity = match method {
            HAM::Key => {
                if let Some(iid) = identity_id {
                    load_open_identity(runtime, store, vault.as_ref(), iid).ok()
                } else {
                    None
                }
            }
            _ => None,
        };
        let opts = probe_options_from_host(&probe_host, identity.as_ref());
        if let Ok(os_id) = runtime.block_on(terminus_core::detect_remote_os(&opts)) {
            if os_id != terminus_core::UNKNOWN_OS {
                host.os_id = Some(os_id);
            }
        }
    }

    runtime
        .block_on(store.upsert_host(&host))
        .map_err(|e| format!("Could not save the host: {e}"))?;

    if method == HostAuthMethod::Password && !draft.password.is_empty() {
        let unlocked = vault
            .as_ref()
            .ok_or_else(|| "Unlock the vault before saving a password".to_string())?;
        let cred = seal_host_password(unlocked.as_ref(), uuid, &draft.password)
            .map_err(|e| format!("Could not encrypt the password: {e}"))?;
        runtime
            .block_on(store.upsert_credential(&cred))
            .map_err(|e| format!("Could not store the encrypted password: {e}"))?;
    }

    Ok(label)
}

/// Connect via russh, classify remote OS, persist `os_id` when it changed.
///
/// Returns `Ok(Some(os_id))` when stored, `Ok(None)` when unknown/unchanged.
/// The quick, local half of an OS probe (runs on the worker): read the host
/// and unseal its credentials. The network half is [`probe_and_store_os`].
pub(super) fn prepare_os_probe(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    vault: Option<&Arc<terminus_core::UnlockedVault>>,
    id: &str,
) -> Result<(Host, terminus_core::ssh::SshConnectOptions), String> {
    use terminus_core::{probe_options_from_host, HostAuthMethod};

    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid host id".to_string())?;
    let hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|e| format!("Could not read hosts: {e}"))?;
    let Some(host) = hosts.iter().find(|h| h.id == uuid).cloned() else {
        return Err("Host not found".into());
    };

    let method = terminus_core::parse_host_auth_method(&host.auth_method)
        .map(|ok| ok.method)
        .unwrap_or(HostAuthMethod::Key);

    let mut probe_host = host.clone();
    if method == HostAuthMethod::Password {
        let pw = resolve_host_password(runtime, store, vault, id)?;
        probe_host.password = pw;
    }

    let identity = match method {
        HostAuthMethod::Key => {
            let Some(iid) = host.identity_id else {
                return Err("No SSH key on host".into());
            };
            Some(load_open_identity(runtime, store, vault, iid)?)
        }
        _ => None,
    };

    // AcceptAll: OS probe runs after the user already opened an SSH session;
    // host-key UX is handled by the shell path / TOFU modal, not here.
    let opts = probe_options_from_host(&probe_host, identity.as_ref());
    Ok((host, opts))
}

/// The network half of an OS probe. It runs as a task on the worker's
/// runtime, never on the worker loop itself: a slow or unreachable server
/// keeps it waiting up to the connect timeout, and the UI thread's
/// short blocking requests (reading a key for SFTP) must not queue behind
/// it.
pub(super) async fn probe_and_store_os(
    store: &Store,
    host: Host,
    opts: &terminus_core::ssh::SshConnectOptions,
) -> Result<Option<String>, String> {
    use terminus_core::{detect_remote_os, UNKNOWN_OS};

    let os_id = detect_remote_os(opts).await.map_err(|e| e.to_string())?;
    if os_id == UNKNOWN_OS {
        return Ok(None);
    }
    if host.os_id.as_deref() == Some(os_id.as_str()) {
        return Ok(Some(os_id)); // still report so UI can refresh tabs
    }

    let mut updated = host;
    updated.os_id = Some(os_id.clone());
    updated.updated_at = Utc::now();
    store
        .upsert_host(&updated)
        .await
        .map_err(|e| format!("Could not save os_id: {e}"))?;
    Ok(Some(os_id))
}

/// Apply a non-empty detected `os_id` onto the in-memory host list.
///
/// Returns `true` when a row was updated (caller should repaint).
pub(super) fn apply_os_detected(hosts: &mut [HostRow], id: &str, os_id: &str) -> bool {
    if os_id.is_empty() {
        return false;
    }
    if let Some(host) = hosts.iter_mut().find(|h| h.id == id) {
        if host.os_id.as_deref() != Some(os_id) {
            host.os_id = Some(os_id.to_string());
            return true;
        }
    }
    false
}
