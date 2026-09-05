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
    HostKeyUnknown { host: String, port: u16 },
    /// Server host key differs from the key recorded in known_hosts (fail closed).
    #[error("SSH host key mismatch for {host}:{port} (known_hosts line {line})")]
    HostKeyMismatch {
        host: String,
        port: u16,
        line: usize,
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
}

pub type Result<T> = std::result::Result<T, Error>;
