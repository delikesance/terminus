use super::*;

pub(super) fn restore_vault(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    sync_engine: &mut terminus_core::SyncEngine,
    events: &Sender<HostEvent>,
) -> Option<Arc<terminus_core::UnlockedVault>> {
    let mut vault = None;
    if let Ok(Some(raw)) =
        runtime.block_on(store.get_setting(terminus_core::VAULT_HEADER_SETTING))
    {
        if terminus_core::parse_vault_header(&raw).is_ok() {
            // Remembered passphrase → unlock without prompting.
            if let Some(passphrase) = crate::vault_remember::load_remembered_passphrase()
            {
                match unlock_or_create_vault(runtime, store, &passphrase) {
                    Ok(unlocked) => {
                        let shared = Arc::new(unlocked);
                        runtime.block_on(sync_engine.attach_vault(Arc::clone(&shared)));
                        vault = Some(shared);
                        let _ = events.send(HostEvent::VaultStatus {
                            unlocked: true,
                            configured: true,
                            message: None,
                        });
                    }
                    Err(_) => {
                        crate::vault_remember::forget_passphrase();
                        let _ = events.send(HostEvent::VaultStatus {
                            unlocked: false,
                            configured: true,
                            message: None,
                        });
                    }
                }
            } else {
                let _ = events.send(HostEvent::VaultStatus {
                    unlocked: false,
                    configured: true,
                    message: None,
                });
            }
        }
    } else {
        // No vault yet: the first prompt creates one (asks twice).
        let _ = events.send(HostEvent::VaultStatus {
            unlocked: false,
            configured: false,
            message: None,
        });
    }
    vault
}

pub(super) fn unlock_or_create_vault(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    passphrase: &str,
) -> Result<terminus_core::UnlockedVault, String> {
    if passphrase.trim().len() < 8 {
        return Err("Vault passphrase must be at least 8 characters".into());
    }
    let existing = runtime
        .block_on(store.get_setting(terminus_core::VAULT_HEADER_SETTING))
        .map_err(|e| e.to_string())?;
    if let Some(raw) = existing {
        let header = terminus_core::parse_vault_header(&raw)
            .map_err(|e| format!("Corrupt vault header: {e}"))?;
        terminus_core::UnlockedVault::unlock(passphrase, &header)
            .map_err(|_| "vault unlock failed".to_string())
    } else {
        let (header, vault) =
            terminus_core::create_with_key(passphrase).map_err(|e| e.to_string())?;
        let json =
            terminus_core::encode_vault_header(&header).map_err(|e| e.to_string())?;
        runtime
            .block_on(store.set_setting(terminus_core::VAULT_HEADER_SETTING, &json))
            .map_err(|e| e.to_string())?;
        Ok(vault)
    }
}

pub(super) fn resolve_host_password(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    vault: Option<&Arc<terminus_core::UnlockedVault>>,
    id: &str,
) -> Result<Option<String>, String> {
    use terminus_core::{
        open_host_password, CREDENTIAL_KIND_HOST_PASSWORD, OWNER_KIND_HOST,
    };

    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid host id".to_string())?;
    let hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|e| format!("Could not read hosts: {e}"))?;
    let Some(host) = hosts.iter().find(|h| h.id == uuid) else {
        return Err("Host not found".into());
    };

    let method = terminus_core::parse_host_auth_method(&host.auth_method)
        .map(|ok| ok.method)
        .unwrap_or(terminus_core::HostAuthMethod::Key);
    if method != terminus_core::HostAuthMethod::Password {
        return Ok(None);
    }

    let creds = runtime
        .block_on(store.list_credentials_for_owner(OWNER_KIND_HOST, uuid))
        .map_err(|e| e.to_string())?;
    let Some(cred) = creds
        .iter()
        .find(|c| c.kind == CREDENTIAL_KIND_HOST_PASSWORD)
    else {
        return Ok(None);
    };

    let Some(unlocked) = vault else {
        return Err(msg::VAULT_FOR_CONNECT.into());
    };

    let password = open_host_password(unlocked.as_ref(), uuid, cred)
        .map_err(|e| format!("Could not decrypt the password: {e}"))?;
    Ok(Some(password))
}

pub(super) fn resolve_host_identity(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    vault: Option<&Arc<terminus_core::UnlockedVault>>,
    id: &str,
) -> Result<Option<(String, Option<String>)>, String> {
    use terminus_core::HostAuthMethod;

    let uuid = Uuid::parse_str(id).map_err(|_| "Invalid host id".to_string())?;
    let hosts = runtime
        .block_on(store.list_hosts())
        .map_err(|e| format!("Could not read hosts: {e}"))?;
    let Some(host) = hosts.iter().find(|h| h.id == uuid) else {
        return Err("Host not found".into());
    };

    let method = terminus_core::parse_host_auth_method(&host.auth_method)
        .map(|ok| ok.method)
        .unwrap_or(HostAuthMethod::Key);
    if method != HostAuthMethod::Key {
        return Ok(None);
    }

    let Some(identity_id) = host.identity_id else {
        return Ok(None);
    };

    let identity = load_open_identity(runtime, store, vault, identity_id)?;
    let passphrase = identity.passphrase.filter(|p| !p.is_empty());
    let Some(pem) = identity.private_key.filter(|p| !p.trim().is_empty()) else {
        return Err("Selected SSH key has no private key material".into());
    };

    Ok(Some((pem, passphrase)))
}

/// Load a managed key with its private key and passphrase unsealed.
///
/// Sealed keys need the vault; the error names "Unlock the vault" so the UI
/// opens the unlock modal instead of failing the connection.
pub(super) fn load_open_identity(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    vault: Option<&Arc<terminus_core::UnlockedVault>>,
    identity_id: Uuid,
) -> Result<terminus_core::Identity, String> {
    let mut identity = runtime
        .block_on(store.get_identity(identity_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Selected SSH key was not found".to_string())?;
    if terminus_core::identity_needs_vault(&identity) && vault.is_none() {
        return Err(msg::VAULT_FOR_CONNECT.into());
    }
    let (pem, passphrase) =
        terminus_core::open_identity_secrets(vault.map(|v| v.as_ref()), &identity)
            .map_err(|e| format!("Could not decrypt the SSH key: {e}"))?;
    identity.private_key = pem;
    identity.passphrase = passphrase;
    Ok(identity)
}

/// Seal every managed key still stored as plaintext (keys saved before the
/// vault sealed them). Runs on each unlock; already-sealed rows are skipped.
pub(super) fn seal_plaintext_identities(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    vault: &terminus_core::UnlockedVault,
) -> Result<usize, String> {
    let identities = runtime
        .block_on(store.list_identities())
        .map_err(|e| e.to_string())?;
    let mut sealed = 0;
    for identity in identities {
        let plaintext = identity
            .private_key
            .as_deref()
            .is_some_and(|k| !k.is_empty() && !terminus_core::vault::is_sealed(k))
            || identity
                .passphrase
                .as_deref()
                .is_some_and(|p| !p.is_empty() && !terminus_core::vault::is_sealed(p));
        if !plaintext {
            continue;
        }
        let mut row =
            terminus_core::seal_identity(vault, &identity).map_err(|e| e.to_string())?;
        row.updated_at = Utc::now();
        runtime
            .block_on(store.upsert_identity(&row))
            .map_err(|e| e.to_string())?;
        sealed += 1;
    }
    Ok(sealed)
}
