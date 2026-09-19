//! `Screen` shell surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::hosts;
use rio_backend::config::Shell;

impl Screen<'_> {
    /// Resolve a row id into the shell its session should run.
    ///
    /// `None` shell means "the app's own shell", which is the local row and the
    /// only case the terminal already knows how to start. The optional env is
    /// for SSH password hosts (`SSH_ASKPASS`).
    pub(super) fn shell_for_row(
        &self,
        id: &str,
    ) -> Result<(Option<Shell>, Option<Vec<(String, String)>>), String> {
        if id == hosts::LOCAL_ID {
            return Ok((None, None));
        }

        if let Some(distro) = self.host_store.platform().distro_named(id) {
            // A distro is a Windows process: starting one goes through
            // `wsl.exe`, which needs WSL interop. Checking here rather than
            // letting the spawn fail is what lets the panel name the reason
            // — interop is missing for a reason worth reporting (a sandbox
            // that replaced `/init`, a WSL build without interop enabled).
            let exe = terminus_core::wsl::WindowsRoots::detect()
                .and_then(|roots| roots.wsl_exe())
                .ok_or_else(|| "No wsl.exe on the Windows drive".to_string())?;
            if let Err(err) = terminus_core::wsl::interop_ready(&exe) {
                // The panel has one line; the whole chain goes to the log.
                tracing::warn!("cannot open {}: {err}", distro.name);
                return Err(format!(
                    "{}: WSL interop is off — {}",
                    distro.display,
                    terminus_core::wsl::interop_short_hint()
                ));
            }

            return Ok((
                Some(Shell {
                    program: Some(exe.to_string_lossy().to_string()),
                    args: distro.launch_args(),
                }),
                None,
            ));
        }

        match self.host_store.hosts().iter().find(|host| host.id == id) {
            Some(host) => {
                let password = if host.auth_method == "password" {
                    match self.host_store.resolve_host_password(id)? {
                        Some(pw) => Some(pw),
                        None => {
                            return Err(
                                "No saved password — edit the host and save one (vault unlocked)"
                                    .into(),
                            );
                        }
                    }
                } else {
                    None
                };
                let identity = if host.auth_method == "password"
                    || host.auth_method == "gssapi"
                {
                    None
                } else {
                    match self.host_store.resolve_host_identity(id)? {
                        Some(pair) => Some(pair),
                        None => {
                            return Err(
                                "No saved SSH key — edit the host and select one (Settings → Managed SSH Keys)"
                                    .into(),
                            );
                        }
                    }
                };
                let (shell, env) = ssh_shell(
                    host,
                    password.as_deref(),
                    identity.as_ref().map(|(pem, _)| pem.as_str()),
                    identity.as_ref().and_then(|(_, pass)| pass.as_deref()),
                )?;
                Ok((Some(shell), env))
            }
            None => Err(format!("No session is configured for {id}")),
        }
    }
}

/// Build the local PTY program for an SSH host tab (**accepted MVP path**).
///
/// **Architecture decision (Option A, see `milestone.md`):** interactive shells
/// spawn the system `ssh` binary in a local PTY. They do **not** use
/// [`terminus_bridge::ssh_transport::SshTransport`] / `SessionSpec::Ssh`. SFTP
/// keeps its dedicated russh worker. Unifying on russh is tracked as roadmap
/// item **1.4-debt**.
///
/// The port is always passed, default included: the row shows a port and the
/// command uses the same one, so a host that is *not* on 22 cannot silently
/// connect somewhere else. A bare `ssh` is resolved on `PATH` by the spawn,
/// exactly like the shell itself.
///
/// When `password` is set, OpenSSH is pointed at a small askpass helper so
/// the sealed vault secret is used instead of an interactive prompt.
///
/// When `identity_pem` is set, the PEM is written to a temp IdentityFile and
/// passed with `-i` (IdentitiesOnly) so managed keys actually authenticate.
///
/// When the host's auth method is `gssapi`, OpenSSH is forced onto
/// `gssapi-with-mic` (Kerberos ticket cache) with pubkey/password disabled.
pub(super) fn ssh_shell(
    host: &hosts::HostRow,
    password: Option<&str>,
    identity_pem: Option<&str>,
    identity_passphrase: Option<&str>,
) -> Result<(Shell, Option<Vec<(String, String)>>), String> {
    let destination = if host.username.is_empty() {
        host.hostname.clone()
    } else {
        format!("{}@{}", host.username, host.hostname)
    };

    let mut args = vec!["-p".to_string(), host.port.to_string()];
    let mut env = None;

    if let Some(password) = password {
        let (askpass, secret_file) = write_ssh_askpass(password)?;
        args.extend([
            "-o".into(),
            "PreferredAuthentications=password".into(),
            "-o".into(),
            "PubkeyAuthentication=no".into(),
            "-o".into(),
            "NumberOfPasswordPrompts=1".into(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
        ]);
        env = Some(vec![
            ("SSH_ASKPASS".into(), askpass.to_string_lossy().into_owned()),
            ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
            (
                "TERMINUS_SSH_ASKPASS_FILE".into(),
                secret_file.to_string_lossy().into_owned(),
            ),
            // Some OpenSSH builds still gate askpass on DISPLAY.
            ("DISPLAY".into(), "terminus:0".into()),
        ]);
        // Best-effort cleanup of the secret file after askpass has had time
        // to run (OpenSSH may call it more than once during handshake).
        let cleanup = secret_file.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(120));
            let _ = std::fs::remove_file(cleanup);
        });
    } else if host.auth_method.eq_ignore_ascii_case("gssapi") {
        push_o_options(&mut args, GSSAPI_SSH_OPTIONS);
    } else if let Some(pem) = identity_pem {
        let key_file = write_ssh_identity_file(pem)?;
        args.extend([
            "-i".into(),
            key_file.to_string_lossy().into_owned(),
            "-o".into(),
            "IdentitiesOnly=yes".into(),
            "-o".into(),
            "PreferredAuthentications=publickey".into(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
        ]);
        if let Some(passphrase) = identity_passphrase.filter(|p| !p.is_empty()) {
            let (askpass, secret_file) = write_ssh_askpass(passphrase)?;
            env = Some(vec![
                ("SSH_ASKPASS".into(), askpass.to_string_lossy().into_owned()),
                ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
                (
                    "TERMINUS_SSH_ASKPASS_FILE".into(),
                    secret_file.to_string_lossy().into_owned(),
                ),
                ("DISPLAY".into(), "terminus:0".into()),
            ]);
            let cleanup_secret = secret_file.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(120));
                let _ = std::fs::remove_file(cleanup_secret);
            });
        }
        let cleanup_key = key_file.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(120));
            let _ = std::fs::remove_file(cleanup_key);
        });
    }

    args.push(destination);

    Ok((
        Shell {
            program: Some("ssh".to_string()),
            args,
        },
        env,
    ))
}

/// OpenSSH `-o` values that force Kerberos `gssapi-with-mic` for a session.
pub(super) const GSSAPI_SSH_OPTIONS: &[&str] = &[
    "GSSAPIAuthentication=yes",
    "PreferredAuthentications=gssapi-with-mic",
    "PubkeyAuthentication=no",
    "PasswordAuthentication=no",
    "StrictHostKeyChecking=accept-new",
];

pub(super) fn push_o_options(args: &mut Vec<String>, options: &[&str]) {
    for opt in options {
        args.push("-o".into());
        args.push((*opt).into());
    }
}

/// Write a managed OpenSSH private key to a temp IdentityFile (mode 0600).
pub(super) fn write_ssh_identity_file(pem: &str) -> Result<std::path::PathBuf, String> {
    use std::io::Write;

    let dir = std::env::temp_dir().join("terminus-ssh-identity");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create identity dir: {e}"))?;

    let id = uuid::Uuid::new_v4();
    let path = dir.join(format!("{id}.pem"));
    {
        let mut f = std::fs::File::create(&path)
            .map_err(|e| format!("Could not write identity file: {e}"))?;
        f.write_all(pem.as_bytes())
            .map_err(|e| format!("Could not write identity file: {e}"))?;
        if !pem.ends_with('\n') {
            f.write_all(b"\n")
                .map_err(|e| format!("Could not write identity file: {e}"))?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)
            .map_err(|e| format!("Could not chmod identity file: {e}"))?
            .permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms)
            .map_err(|e| format!("Could not chmod identity file: {e}"))?;
    }
    Ok(path)
}

/// Write a one-shot askpass helper + secret file under the temp directory.
pub(super) fn write_ssh_askpass(
    password: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    use std::io::Write;

    let dir = std::env::temp_dir().join("terminus-ssh-askpass");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create askpass dir: {e}"))?;

    let id = uuid::Uuid::new_v4();
    let secret_file = dir.join(format!("{id}.secret"));
    {
        let mut f = std::fs::File::create(&secret_file)
            .map_err(|e| format!("Could not write askpass secret: {e}"))?;
        f.write_all(password.as_bytes())
            .map_err(|e| format!("Could not write askpass secret: {e}"))?;
        f.write_all(b"\n")
            .map_err(|e| format!("Could not write askpass secret: {e}"))?;
    }

    #[cfg(windows)]
    let askpass = {
        let path = dir.join("askpass.cmd");
        if !path.exists() {
            std::fs::write(
                &path,
                "@echo off\r\nif not defined TERMINUS_SSH_ASKPASS_FILE exit /b 1\r\ntype \"%TERMINUS_SSH_ASKPASS_FILE%\"\r\n",
            )
            .map_err(|e| format!("Could not write askpass helper: {e}"))?;
        }
        path
    };

    #[cfg(not(windows))]
    let askpass = {
        let path = dir.join("askpass.sh");
        if !path.exists() {
            std::fs::write(
                &path,
                "#!/bin/sh\n# Terminus SSH_ASKPASS helper — prints the sealed password file.\nif [ -z \"$TERMINUS_SSH_ASKPASS_FILE\" ] || [ ! -f \"$TERMINUS_SSH_ASKPASS_FILE\" ]; then\n  exit 1\nfi\ncat \"$TERMINUS_SSH_ASKPASS_FILE\"\n",
            )
            .map_err(|e| format!("Could not write askpass helper: {e}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&path)
                    .map_err(|e| format!("Could not chmod askpass helper: {e}"))?
                    .permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&path, perms)
                    .map_err(|e| format!("Could not chmod askpass helper: {e}"))?;
            }
        }
        path
    };

    Ok((askpass, secret_file))
}
