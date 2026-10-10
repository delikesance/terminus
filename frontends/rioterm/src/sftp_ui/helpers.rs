use super::*;

pub(super) fn host_label(host: &HostRow) -> String {
    if host.name.trim().is_empty() {
        host.endpoint()
    } else {
        host.name.clone()
    }
}

pub(super) fn focus_from_side(side: SftpSide) -> SftpFocus {
    match side {
        SftpSide::Left => SftpFocus::Left,
        SftpSide::Right => SftpFocus::Right,
    }
}

pub(super) fn side_from_focus(focus: SftpFocus) -> SftpSide {
    match focus {
        SftpFocus::Left => SftpSide::Left,
        SftpFocus::Right => SftpSide::Right,
    }
}

pub(super) fn row_from_list_entry(entry: SftpListEntry) -> SftpRow {
    SftpRow {
        name: entry.name,
        path: entry.path,
        is_dir: entry.is_dir,
        size: entry.size,
        modified: entry.modified,
    }
}

pub(super) fn connect_options_for_host(
    host: &HostRow,
    password: Option<String>,
    identity_pem: Option<(String, Option<String>)>,
) -> Result<SshConnectOptions, String> {
    let method = match host.auth_method.as_str() {
        "password" => HostAuthMethod::Password,
        "gssapi" => HostAuthMethod::Gssapi,
        _ => HostAuthMethod::Key,
    };

    let mut auth = SshAuth {
        username: host.username.clone(),
        method: Some(method),
        ..SshAuth::default()
    };

    match method {
        HostAuthMethod::Password => {
            auth.password = password.or_else(|| {
                // Empty password still attempts auth; worker surfaces failure.
                Some(String::new())
            });
        }
        HostAuthMethod::Key => {
            let (pem, passphrase) =
                identity_pem.ok_or_else(|| crate::hosts::msg::NO_SSH_KEY.to_string())?;
            auth.identity_pem = Some(pem);
            auth.identity_passphrase = passphrase;
        }
        HostAuthMethod::Gssapi => {}
    }

    Ok(SshConnectOptions {
        hostname: host.hostname.clone(),
        port: host.port,
        auth,
        policy: HostKeyPolicy::Tofu,
        known_hosts: KnownHosts::default(),
        connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        keepalive_interval: Some(DEFAULT_KEEPALIVE_INTERVAL),
    })
}

/// Open a local path with the OS default application for its file type.
pub(super) fn open_path_with_default_app(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        if let Err(err) = std::process::Command::new("open").arg(path).spawn() {
            tracing::warn!(error = %err, path = %path.display(), "failed to open edited file");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Err(err) = std::process::Command::new("xdg-open").arg(path).spawn() {
            tracing::warn!(error = %err, path = %path.display(), "failed to open edited file");
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide_target: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let result = unsafe {
            windows_sys::Win32::UI::Shell::ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                wide_target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            )
        };
        if (result as isize) <= 32 {
            tracing::warn!(
                path = %path.display(),
                code = result as isize,
                "ShellExecuteW could not open edited file"
            );
        }
    }
}
