use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("not connected to a remote database")]
    SyncNotConfigured,
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
