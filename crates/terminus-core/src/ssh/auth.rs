use std::sync::Arc;

use russh::client::Handle;
use russh::keys::PrivateKeyWithHashAlg;

use super::{ClientHandler, SshAuth};
use crate::auth_method::HostAuthMethod;
use crate::error::{Error, Result};
use crate::gssapi;
use crate::models::Host;

/// Runs authentication for the selected method. GSSAPI never falls through
/// to password; key never falls through either.
pub(super) async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    hostname: &str,
    port: u16,
    auth: &SshAuth,
) -> Result<()> {
    let method = auth.method.unwrap_or_else(|| {
        if auth.identity_pem.is_some() || auth.identity_path.is_some() {
            HostAuthMethod::Key
        } else {
            HostAuthMethod::Password
        }
    });

    match method {
        HostAuthMethod::Gssapi => {
            let host = Host {
                id: uuid::Uuid::nil(),
                name: hostname.to_string(),
                hostname: hostname.to_string(),
                port,
                username: auth.username.clone(),
                auth_method: HostAuthMethod::Gssapi.as_str().to_string(),
                password: None,
                identity_id: None,
                group_id: None,
                tags: Vec::new(),
                notes: String::new(),
                os_id: None,
                sort_order: 0,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                deleted_at: None,
            };
            let ok = gssapi::authenticate(handle, &host).await?;
            if ok {
                return Ok(());
            }
            Err(Error::SshError(format!(
                "authentication refused for user {}",
                auth.username
            )))
        }
        HostAuthMethod::Key => authenticate_key(handle, auth).await,
        HostAuthMethod::Password => authenticate_password(handle, auth).await,
    }
}

async fn authenticate_key(
    handle: &mut Handle<ClientHandler>,
    auth: &SshAuth,
) -> Result<()> {
    let key = if let Some(pem) = auth.identity_pem.as_deref() {
        russh::keys::decode_secret_key(pem, auth.identity_passphrase.as_deref()).map_err(
            |e| Error::IdentityKeyInvalid {
                reason: e.to_string(),
            },
        )?
    } else if let Some(path) = &auth.identity_path {
        russh::keys::load_secret_key(path, auth.identity_passphrase.as_deref()).map_err(
            |e| Error::SshError(format!("cannot load key {}: {e}", path.display())),
        )?
    } else {
        return Err(Error::SshError(
            "no SSH private key found (save a key in Settings, then try again)".into(),
        ));
    };

    let hash_alg = handle
        .best_supported_rsa_hash()
        .await
        .ok()
        .flatten()
        .flatten();
    let result = handle
        .authenticate_publickey(
            auth.username.clone(),
            PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
        )
        .await
        .map_err(|e| Error::SshError(format!("public key auth failed: {e}")))?;
    if result.success() {
        return Ok(());
    }
    Err(Error::SshError(format!(
        "authentication refused for user {}",
        auth.username
    )))
}

async fn authenticate_password(
    handle: &mut Handle<ClientHandler>,
    auth: &SshAuth,
) -> Result<()> {
    let Some(password) = auth.password.as_ref() else {
        return Err(Error::SshError(format!(
            "authentication refused for user {}",
            auth.username
        )));
    };
    let result = handle
        .authenticate_password(auth.username.clone(), password.clone())
        .await
        .map_err(|e| Error::SshError(format!("password auth failed: {e}")))?;
    let offers_keyboard_interactive = match &result {
        russh::client::AuthResult::Success => return Ok(()),
        russh::client::AuthResult::Failure {
            remaining_methods, ..
        } => remaining_methods
            .iter()
            .any(|m| <&str>::from(m) == "keyboard-interactive"),
    };
    // PAM / 2FA front-ends (and macOS by default) disable the `password`
    // method and ask for the same password through `keyboard-interactive`.
    if offers_keyboard_interactive
        && authenticate_keyboard_interactive(handle, &auth.username, password).await?
    {
        return Ok(());
    }
    Err(Error::SshError(format!(
        "authentication refused for user {}",
        auth.username
    )))
}

/// Answer `keyboard-interactive` prompts with the password.
///
/// Every hidden prompt gets the password; echoed prompts (not secrets) get an
/// empty answer. A server asking more rounds than any sane password flow
/// (e.g. a one-time code the user must type) fails rather than loops.
async fn authenticate_keyboard_interactive(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    password: &str,
) -> Result<bool> {
    use russh::client::KeyboardInteractiveAuthResponse as Kbd;
    let err = |e: russh::Error| {
        Error::SshError(format!("keyboard-interactive auth failed: {e}"))
    };
    let mut reply = handle
        .authenticate_keyboard_interactive_start(username.to_string(), None)
        .await
        .map_err(err)?;
    for _ in 0..4 {
        match reply {
            Kbd::Success => return Ok(true),
            Kbd::Failure { .. } => return Ok(false),
            Kbd::InfoRequest { prompts, .. } => {
                let answers = prompts
                    .iter()
                    .map(|p| {
                        if p.echo {
                            String::new()
                        } else {
                            password.to_string()
                        }
                    })
                    .collect();
                reply = handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(err)?;
            }
        }
    }
    Ok(false)
}
