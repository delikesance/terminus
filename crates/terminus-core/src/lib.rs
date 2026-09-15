pub mod auth_method;
pub mod error;
pub mod forward_runtime;
pub mod gssapi;
pub mod local_fs;
pub mod machine;
pub mod models;
pub mod os_detect;
pub mod session;
pub mod sftp;
pub mod ssh;
pub mod store;
pub mod sync;
pub mod vault;
pub mod wsl;

pub use auth_method::{
    host_auth_ui, parse_host_auth_method, HostAuthMethod, HostAuthUi, ParseHostAuthErr,
    ParseHostAuthOk,
};
pub use error::{Error, Result};
pub use forward_runtime::ForwardRuntime;
pub use models::*;
pub use session::{OutputSink, SessionManager, SessionSpec};
pub use ssh::{
    default_known_hosts_path, fingerprint_of, probe_options_from_host, probe_ssh_auth,
    HostKeyOutcome, HostKeyPolicy, KnownHosts, ProbeError, SshAuth, SshConnectOptions, SshEvent,
    SshPty, SshSession, CHUNK_SIZE, DEFAULT_CONNECT_TIMEOUT, DEFAULT_KEEPALIVE_INTERVAL,
    DEFAULT_TERM,
};
pub use store::Store;
pub use sync::SyncEngine;
pub use vault::{
    create_with_key, encode_vault_header, open_host_password, parse_vault_header,
    seal_host_password, SecretEnvelope, UnlockedVault, VaultHeader, VaultStatus,
    CREDENTIAL_KIND_HOST_PASSWORD, OWNER_KIND_HOST, VAULT_HEADER_SETTING,
};
