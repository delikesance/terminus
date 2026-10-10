//! Active SFTP dual-pane session owned by [`crate::screen::Screen`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use terminus_bridge::{
    ConflictAction, ConflictKind, SftpCommand, SftpEvent, SftpListEntry, SftpSide,
    SftpWorker,
};
use terminus_core::auth_method::HostAuthMethod;
use terminus_core::local_fs;
use terminus_core::ssh::{
    HostKeyPolicy, KnownHosts, SshAuth, SshConnectOptions, DEFAULT_CONNECT_TIMEOUT,
    DEFAULT_KEEPALIVE_INTERVAL,
};
use terminus_ui::sftp_pane::{
    join_remote, parent_path, SftpBackend, SftpClickResult, SftpConflictKind, SftpDrag,
    SftpFocus, SftpHit, SftpNameKind, SftpPaneState, SftpRow, SftpSideState,
    SFTP_DRAG_THRESHOLD,
};

use crate::hosts::HostRow;

/// Live dual-pane SFTP session (either side local or remote).
pub struct ActiveSftp {
    /// Sidebar id of the machine this browser belongs to (its right pane).
    pub machine_id: String,
    /// [`HostRow::connection_key`] of that machine when the browser was
    /// opened: a change (or the host's deletion) closes the browser.
    pub connection_key: String,
    pub state: SftpPaneState,
    pub worker: SftpWorker,
    last_click: Option<(SftpHit, std::time::Instant)>,
    /// Press origin for DnD threshold (`hit`, `x`, `y`).
    drag_armed: Option<(SftpHit, f32, f32)>,
}

impl ActiveSftp {
    /// Key of [`terminus_ui::screens::files::MachineSessions`].
    pub fn owner(&self) -> &str {
        &self.machine_id
    }

    /// A transfer (or its conflict prompt) is in flight: closing now
    /// would cancel it.
    pub fn busy(&self) -> bool {
        self.state.transfer.is_some() || self.state.conflict.is_some()
    }

    /// Left = local, right = `host`. Connects right and lists both sides.
    pub fn start(
        host: &HostRow,
        password: Option<String>,
        identity_pem: Option<(String, Option<String>)>,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<Self, String> {
        let opts = connect_options_for_host(host, password, identity_pem)?;
        let local_root = local_fs::default_local_root();
        let label = host_label(host);
        let mut state = SftpPaneState::new_local_remote(
            local_root.to_string_lossy().into_owned(),
            host.id.clone(),
            label,
        );
        state.status = format!("Connecting to {}…", host.endpoint());
        state.loading = true;

        let worker = SftpWorker::spawn(wake);
        worker.send(SftpCommand::Connect {
            side: SftpSide::Right,
            opts,
        });
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Left,
            path: local_root,
        });

        Ok(Self {
            machine_id: host.id.clone(),
            connection_key: host.connection_key(),
            state,
            worker,
            last_click: None,
            drag_armed: None,
        })
    }

    /// Both panes local — used by unit/E2E harnesses (no SSH).
    #[allow(dead_code)]
    pub fn start_local_dual(
        left: PathBuf,
        right: PathBuf,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        let mut state = SftpPaneState::new_local_local(
            left.to_string_lossy().into_owned(),
            right.to_string_lossy().into_owned(),
        );
        state.status = "Ready".into();
        state.loading = true;
        let worker = SftpWorker::spawn(wake);
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Left,
            path: left,
        });
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Right,
            path: right,
        });
        Self {
            machine_id: crate::hosts::LOCAL_ID.to_string(),
            connection_key: String::new(),
            state,
            worker,
            last_click: None,
            drag_armed: None,
        }
    }

    /// Connect a second host on the left pane (right stays as-is).
    pub fn open_other_host(
        &mut self,
        host: &HostRow,
        password: Option<String>,
        identity_pem: Option<(String, Option<String>)>,
    ) -> Result<(), String> {
        let opts = connect_options_for_host(host, password, identity_pem)?;
        if matches!(self.state.left.backend, SftpBackend::Remote { .. }) {
            self.worker.send(SftpCommand::Disconnect {
                side: SftpSide::Left,
            });
        }
        self.state.left = SftpSideState::remote(host.id.clone(), host_label(host));
        self.state.focus = SftpFocus::Left;
        self.state.loading = true;
        self.state.status = format!("Connecting to {}…", host.endpoint());
        self.state.error = None;
        self.state.name_edit = None;
        self.state.drag = None;
        self.drag_armed = None;
        self.worker.send(SftpCommand::Connect {
            side: SftpSide::Left,
            opts,
        });
        Ok(())
    }

    pub fn close(&self) {
        self.worker.send(SftpCommand::Close);
    }
}

/// Keyboard actions handled while the SFTP pane owns input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpKey {
    Tab,
    Up,
    Down,
    Enter,
    Backspace,
    Delete,
    Mkdir,
    Rename,
    Upload,
    Download,
}

mod actions;
mod drag;
mod helpers;
mod input;
mod menu;
mod pump;

use helpers::{
    connect_options_for_host, focus_from_side, host_label, open_path_with_default_app,
    row_from_list_entry, side_from_focus,
};

#[cfg(test)]
mod tests;
