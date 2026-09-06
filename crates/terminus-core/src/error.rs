use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("not connected to a remote database")]
    SyncNotConfigured,
    /// Server host key is not present in known_hosts (fail closed).
    #[error("SSH host key unknown for {host}:{port}")]
    HostKeyUnknown {
        host: String,
        port: u16,
        /// OpenSSH-formatted public key presented by the server.
        public_key: String,
        /// Key algorithm (e.g. `ssh-ed25519`).
        algo: String,
        /// SHA256 fingerprint (`SHA256:…`).
        fingerprint: String,
    },
    /// Server host key differs from the key recorded in known_hosts (fail closed).
    #[error("SSH host key mismatch for {host}:{port} (known_hosts line {line})")]
    HostKeyMismatch {
        host: String,
        port: u16,
        line: usize,
        /// OpenSSH-formatted public key presented by the server.
        public_key: String,
        /// Key algorithm (e.g. `ssh-ed25519`).
        algo: String,
        /// SHA256 fingerprint (`SHA256:…`).
        fingerprint: String,
    },
    #[error("pty reader failed: {0}")]
    PtyReader(String),
    #[error("pty kill failed: {0}")]
    PtyKill(String),
    /// Private key material could not be parsed (PEM decode / format). Never includes raw secret.
    #[error("invalid SSH identity key: {reason}")]
    IdentityKeyInvalid { reason: String },
    /// SFTP path escaped the session root via `..` or absolute jump.
    #[error("SFTP path traversal blocked: {path}")]
    SftpPathTraversal { path: String },
    /// SFTP operation exceeded the client timeout; session was disconnected.
    #[error("SFTP operation timed out")]
    SftpTimeout,
    /// Typed SFTP I/O / protocol failure for UI surfacing (not silent).
    #[error("SFTP {kind}: {message}")]
    Sftp {
        kind: &'static str,
        message: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Ssh(#[from] russh::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl Error {
    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }

    /// Serialize host-key errors as structured JSON for the UI; otherwise Display.
    pub fn to_ipc_string(&self) -> String {
        match self {
            Self::HostKeyUnknown {
                host,
                port,
                public_key,
                algo,
                fingerprint,
            } => serde_json::to_string(&HostKeyIpc {
                kind: "HostKeyUnknown",
                host: host.clone(),
                port: *port,
                line: None,
                public_key: public_key.clone(),
                algo: algo.clone(),
                fingerprint: fingerprint.clone(),
            })
            .unwrap_or_else(|_| self.to_string()),
            Self::HostKeyMismatch {
                host,
                port,
                line,
                public_key,
                algo,
                fingerprint,
            } => serde_json::to_string(&HostKeyIpc {
                kind: "HostKeyMismatch",
                host: host.clone(),
                port: *port,
                line: Some(*line),
                public_key: public_key.clone(),
                algo: algo.clone(),
                fingerprint: fingerprint.clone(),
            })
            .unwrap_or_else(|_| self.to_string()),
            Self::IdentityKeyInvalid { reason } => serde_json::to_string(&IdentityKeyIpc {
                kind: "IdentityKeyInvalid",
                reason: reason.clone(),
            })
            .unwrap_or_else(|_| self.to_string()),
            Self::SftpPathTraversal { path } => serde_json::to_string(&SftpIpc {
                kind: "SftpPathTraversal",
                message: format!("path traversal blocked: {path}"),
                path: Some(path.clone()),
            })
            .unwrap_or_else(|_| self.to_string()),
            Self::SftpTimeout => serde_json::to_string(&SftpIpc {
                kind: "SftpTimeout",
                message: "SFTP operation timed out".into(),
                path: None,
            })
            .unwrap_or_else(|_| self.to_string()),
            Self::Sftp { kind, message } => serde_json::to_string(&SftpIpc {
                kind,
                message: message.clone(),
                path: None,
            })
            .unwrap_or_else(|_| self.to_string()),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct HostKeyIpc {
    kind: &'static str,
    host: String,
    port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    public_key: String,
    algo: String,
    fingerprint: String,
}

#[derive(Debug, Serialize)]
struct IdentityKeyIpc {
    kind: &'static str,
    reason: String,
}

#[derive(Debug, Serialize)]
struct SftpIpc {
    kind: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

pub type Result<T> = std::result::Result<T, Error>;
