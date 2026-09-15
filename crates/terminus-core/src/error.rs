//! Crate-wide error type for `terminus-core`.
use thiserror::Error;

/// Errors returned by the `terminus-core` domain services.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("SSH connection error: {0}")]
    SshError(String),

    #[error("Vault error: {0}")]
    VaultError(String),

    #[error("Sync error: {0}")]
    SyncError(String),

    #[error("File system error: {0}")]
    FileSystemError(String),

    /// Local or remote I/O failures (file reads, socket writes, ...).
    #[error("I/O error: {0}")]
    IoError(String),

    /// Cryptographic failures: key derivation, envelope sealing/opening.
    #[error("Crypto error: {0}")]
    CryptoError(String),

    /// An operation exceeded its deadline (SFTP operations use a 30s budget).
    #[error("Timed out: {0}")]
    TimeoutError(String),

    /// A path tried to escape the sandbox root of an SFTP pane.
    #[error("Path escapes the allowed root: {0}")]
    PathTraversalError(String),

    /// The requested entity does not exist (host, session, file, ...).
    #[error("Not found: {0}")]
    NotFoundError(String),

    /// Serialization/deserialization failures (JSON payloads stored in the vault).
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Generic message (GSSAPI / probe helpers). Prefer typed variants when possible.
    #[error("{0}")]
    Message(String),

    /// Private key material could not be parsed. Never includes raw secret.
    #[error("invalid SSH identity key: {reason}")]
    IdentityKeyInvalid { reason: String },

    /// No TGT / expired ccache for GSSAPI (Unix). Exact UI copy.
    #[error("No Kerberos ticket found. Run kinit, then try again.")]
    GssapiNoTicket,

    /// GSSAPI not implemented on this OS (Windows v1). Exact UI copy.
    #[error("Kerberos (GSSAPI) is not supported on Windows.")]
    GssapiUnsupported,

    /// Vault unlock failed (wrong passphrase).
    #[error("vault unlock failed")]
    VaultUnlockFailed,

    /// Vault decrypt failed (tamper / wrong AAD).
    #[error("vault decrypt failed")]
    VaultDecryptFailed,

    /// Vault is locked — cannot seal or open secrets.
    #[error("vault is locked")]
    VaultLocked,

    /// No vault header configured yet.
    #[error("vault is not configured")]
    VaultNotConfigured,

    /// Passphrase rejected by policy.
    #[error("invalid vault passphrase")]
    InvalidPassphrase,
}

impl Error {
    /// Build a generic [`Error::Message`].
    pub fn msg(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::SerializationError(err.to_string())
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::DatabaseError(err.to_string())
    }
}

impl From<russh::Error> for Error {
    fn from(err: russh::Error) -> Self {
        Error::SshError(err.to_string())
    }
}

impl From<russh::keys::Error> for Error {
    fn from(err: russh::keys::Error) -> Self {
        Error::SshError(err.to_string())
    }
}

impl From<russh::SendError> for Error {
    fn from(_: russh::SendError) -> Self {
        Error::SshError("SSH send error".into())
    }
}

impl From<russh_sftp::client::error::Error> for Error {
    fn from(err: russh_sftp::client::error::Error) -> Self {
        Error::SshError(err.to_string())
    }
}

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;
