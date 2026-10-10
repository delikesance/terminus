use super::{
    HostKeyPolicy, KnownHosts, SshAuth, SshConnectOptions, SshSession,
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_KEEPALIVE_INTERVAL,
};
use crate::auth_method::HostAuthMethod;
use crate::error::Error;
use crate::models::{Host, Identity};

/// Typed failure from [`probe_ssh_auth`] — mapped to add-host error copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// TCP / DNS / timeout before auth.
    Unreachable(String),
    /// Server rejected the credentials.
    AuthFailed,
    /// Key auth requested but no usable private key was available.
    NoKey,
    /// Kerberos ticket cache missing / expired.
    GssapiNoTicket,
    /// GSSAPI not available on this platform.
    GssapiUnsupported,
    /// Anything else (host key, protocol, …).
    Other(String),
}

impl ProbeError {
    /// User-facing message for the add-host error line.
    pub fn user_message(&self) -> String {
        match self {
            Self::Unreachable(detail) => {
                let lower = detail.to_ascii_lowercase();
                let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
                if detail.is_empty() {
                    "Host unreachable".into()
                } else if has(&["refused"]) {
                    "Nothing answers on that port. Is SSH running?".into()
                } else if has(&["lookup", "name or service", "resolve", "nodename"]) {
                    "Can't find that host. Check the address.".into()
                } else if has(&["timed out", "timeout"]) {
                    "No answer from the host (firewall or VPN?)".into()
                } else if has(&["network is unreachable", "no route"]) {
                    "Can't reach that network. Check VPN/Wi-Fi.".into()
                } else {
                    format!("Host unreachable: {detail}")
                }
            }
            Self::AuthFailed => {
                "Authentication failed (wrong password, username, or key)".into()
            }
            Self::NoKey => {
                "No SSH private key selected. Save a key in Settings, then try again."
                    .into()
            }
            Self::GssapiNoTicket => Error::GssapiNoTicket.to_string(),
            Self::GssapiUnsupported => Error::GssapiUnsupported.to_string(),
            Self::Other(msg) => msg.clone(),
        }
    }

    /// Map a core [`Error`] from connect/auth into a probe failure.
    pub fn from_error(err: Error) -> Self {
        match err {
            Error::GssapiNoTicket => Self::GssapiNoTicket,
            Error::GssapiUnsupported => Self::GssapiUnsupported,
            Error::TimeoutError(msg) => Self::Unreachable(msg),
            Error::IoError(msg) => Self::Unreachable(msg),
            Error::SshError(msg) => classify_ssh_message(&msg),
            Error::Message(msg) => classify_ssh_message(&msg),
            Error::IdentityKeyInvalid { reason } => {
                Self::Other(format!("invalid SSH key: {reason}"))
            }
            other => Self::Other(other.to_string()),
        }
    }
}

pub(super) fn classify_ssh_message(msg: &str) -> ProbeError {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("connection refused")
        || lower.contains("network is unreachable")
        || lower.contains("name or service not known")
        || lower.contains("no route to host")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("could not resolve")
    {
        return ProbeError::Unreachable(msg.to_string());
    }
    if lower.contains("no ssh private key")
        || lower.contains("cannot load key")
        || lower.contains("no usable credentials")
    {
        return ProbeError::NoKey;
    }
    if lower.contains("authentication refused")
        || lower.contains("auth failed")
        || lower.contains("public key rejected")
        || lower.contains("password auth failed")
        || lower.contains("authentication failed")
    {
        return ProbeError::AuthFailed;
    }
    ProbeError::Other(msg.to_string())
}

/// Build connect options for an add-host probe (accept any host key).
pub fn probe_options_from_host(
    host: &Host,
    identity: Option<&Identity>,
) -> SshConnectOptions {
    let method = crate::auth_method::parse_host_auth_method(&host.auth_method)
        .map(|ok| ok.method)
        .unwrap_or(HostAuthMethod::Password);

    let mut auth = SshAuth {
        username: host.username.clone(),
        method: Some(method),
        ..SshAuth::default()
    };

    match method {
        HostAuthMethod::Password => {
            auth.password = host.password.clone();
        }
        HostAuthMethod::Key => {
            if let Some(ident) = identity {
                auth.identity_pem = ident.private_key.clone();
                auth.identity_passphrase = ident.passphrase.clone();
            }
        }
        HostAuthMethod::Gssapi => {}
    }

    SshConnectOptions {
        hostname: host.hostname.clone(),
        port: host.port,
        auth,
        policy: HostKeyPolicy::AcceptAll,
        known_hosts: KnownHosts::default(),
        connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        keepalive_interval: Some(DEFAULT_KEEPALIVE_INTERVAL),
    }
}

/// TCP + auth only — no PTY. Used by add-host Connect before save.
pub async fn probe_ssh_auth(
    opts: &SshConnectOptions,
) -> std::result::Result<(), ProbeError> {
    match SshSession::connect(opts).await {
        Ok(mut session) => {
            let _ = session.disconnect().await;
            Ok(())
        }
        Err(err) => Err(ProbeError::from_error(err)),
    }
}
