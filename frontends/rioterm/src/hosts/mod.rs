// Copyright (c) 2026-present, Terminus Contributors.
//! Host storage for the UI.
//!
//! The window threads can't await on `terminus_core::Store` — rio runs on
//! `corcovado`'s event loop, while `Store` (SQLx) needs a real `tokio`
//! runtime to drive its pool. So the database lives on its own thread with
//! its own runtime, and the UI talks to it with plain channels:
//!
//! ```text
//! UI thread                 worker thread (tokio)
//! ---------                 -------------------
//! create(draft)  --Command--> upsert_host()
//! drain()        <--HostEvent-- list_hosts()
//! ```
//!
//! Every mutation answers with the freshly-read list, so the sidebar always
//! shows database truth rather than an optimistic local guess. `wake` is
//! called after each answer so the app can repaint even when it is idle.
//!
//! The sidebar shows three things, of which only the last one lives in the
//! database: *this computer* (a local shell), the WSL distros installed on
//! the Windows machine this one is nested in, and the stored SSH hosts. The
//! first two are platform facts, gathered once per refresh by
//! [`terminus_core::machine`] and [`terminus_core::wsl`] on this same worker
//! thread — they read the filesystem, so they must not run on the UI thread.

mod collapsed;
mod commands;
mod draft;
mod host_row;
mod probe;
mod repository;
mod rows;
mod store_ops;
mod sync;
mod vault;
mod worker;

#[cfg(test)]
mod collapsed_tests;
#[cfg(test)]
mod draft_tests;
#[cfg(test)]
mod host_row_tests;
#[cfg(test)]
mod probe_tests;
#[cfg(test)]
mod rows_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod worker_auth_tests;
#[cfg(test)]
mod worker_tests;

pub use self::collapsed::*;
use self::commands::*;
pub use self::draft::*;
pub use self::host_row::*;
use self::probe::*;
pub use self::repository::*;
pub use self::rows::*;
pub use self::store_ops::*;
use self::sync::*;
use self::vault::*;
use self::worker::*;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use terminus_core::machine::{self, LocalMachine};
use terminus_core::models::{Group, Host};
use terminus_core::wsl::{self, WslDistro};
use terminus_core::Store;
use terminus_ui::os_icons::HostStatus;
use terminus_ui::sidebar::{Badge, HostItem, Row};
use uuid::Uuid;

/// Legacy WSL section label (no longer emitted by [`sidebar_rows`]).
#[allow(dead_code)]
pub const WSL_SECTION: &str = "Windows (WSL)";
/// Section label above this computer and WSL distros.
pub const LOCAL_SECTION: &str = terminus_ui::sidebar::LOCAL_SECTION;
/// Section label above the stored SSH hosts and groups.
pub const HOSTS_SECTION: &str = terminus_ui::sidebar::SERVERS_SECTION;
/// The row id of the local machine, resolved by the screen when it opens.
pub const LOCAL_ID: &str = "local";
/// Prefix marking a row as a WSL distro; the rest is the distro's name.
pub const WSL_PREFIX: &str = "wsl:";

/// Errors the sidebar shows when a server cannot be opened. They name the
/// J5 Settings tabs and stay within the two lines of the error band, in
/// glyphs the UI face (Sora) has.
pub mod msg {
    pub const NO_PASSWORD: &str =
        "No saved password \u{2014} edit the server and save one";
    pub const NO_SSH_KEY: &str =
        "No SSH key for this server \u{2014} edit it and pick one";
    pub const PICK_SSH_KEY: &str = "Pick one of your SSH keys (Settings, SSH keys)";
    pub const VAULT_FOR_PASSWORD: &str =
        "Unlock the vault before saving a password (Settings, Sync)";
    pub const VAULT_FOR_CONNECT: &str =
        "Unlock the vault before connecting (Settings, Sync)";
    pub const VAULT_FOR_SSH_KEY: &str =
        "Unlock the vault before saving an SSH key (Settings, Sync)";
}

/// Default SSH port, applied when the editor's port field is left empty.
pub const DEFAULT_PORT: u16 = 22;

/// Settings key for the persisted [`terminus_core::sync::SyncConfig`] JSON.
const SYNC_CONFIG_SETTING: &str = "sync_config";

/// Settings key for the persisted set of collapsed sidebar group IDs.
///
/// Stored as a sorted newline-delimited list of group-id strings.
/// Versioned in the key so a format change can migrate cleanly.
const COLLAPSED_GROUPS_SETTING: &str = "sidebar_collapsed_groups_v1";

/// Directory holding `terminus.db`.
///
/// `TERMINUS_DATA_DIR` overrides the platform default (mirrors rio's
/// `RIO_CONFIG_HOME` convention) so tests and dev runs can be redirected.
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TERMINUS_DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }

    let base = dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("terminus")
}

/// SFTP credentials for a host: `(password, (private key PEM, passphrase))`.
pub type SftpAuth = (Option<String>, Option<(String, Option<String>)>);

/// SFTP credentials for `auth_method`, reading only what it needs: the
/// sealed password for `password`, the managed key for `key`, nothing for
/// `gssapi`. A missing one is an error the user can act on.
pub fn sftp_auth_for(
    auth_method: &str,
    password: impl FnOnce() -> Result<Option<String>, String>,
    identity: impl FnOnce() -> Result<Option<(String, Option<String>)>, String>,
) -> Result<SftpAuth, String> {
    let password = if auth_method == "password" {
        Some(password()?.ok_or_else(|| msg::NO_PASSWORD.to_string())?)
    } else {
        None
    };
    let identity = if auth_method == "password" || auth_method == "gssapi" {
        None
    } else {
        Some(identity()?.ok_or_else(|| msg::NO_SSH_KEY.to_string())?)
    };
    Ok((password, identity))
}

/// Whether an SFTP browser opened on host `id` with connection `key` must
/// be closed: the host was deleted or its connection settings changed.
pub fn sftp_connection_stale(hosts: &[HostRow], id: &str, key: &str) -> bool {
    hosts
        .iter()
        .find(|h| h.id == id)
        .is_none_or(|h| h.connection_key() != key)
}

/// Where the database file for `dir` lives (used by tests and diagnostics).
#[allow(dead_code)]
pub fn database_path(dir: &Path) -> PathBuf {
    dir.join("terminus.db")
}
