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
                            return Err(crate::hosts::msg::NO_PASSWORD.into());
                        }
                    }
                } else {
                    None
                };
                let identity =
                    if host.auth_method == "password" || host.auth_method == "gssapi" {
                        None
                    } else {
                        match self.host_store.resolve_host_identity(id)? {
                            Some(pair) => Some(pair),
                            None => {
                                return Err(crate::hosts::msg::NO_SSH_KEY.into());
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

/// Terminal type advertised to SSH remotes; see [`ssh_shell`].
pub(super) const REMOTE_TERM: &str = "xterm-256color";

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
    push_o_options(&mut args, KEEPALIVE_SSH_OPTIONS);
    // `ssh` forwards our `$TERM` in its pty request. The local one is
    // usually `xterm-rio`, which remote hosts have no terminfo for: readline
    // then can't move the cursor and history recall piles lines on top of
    // each other. Advertise the entry every remote ships instead.
    let mut env = vec![("TERM".to_string(), REMOTE_TERM.to_string())];

    if let Some(password) = password {
        let (askpass, secret_file) = write_ssh_askpass(password)?;
        args.extend([
            "-o".into(),
            "PreferredAuthentications=password,keyboard-interactive".into(),
            "-o".into(),
            "PubkeyAuthentication=no".into(),
            "-o".into(),
            "NumberOfPasswordPrompts=1".into(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
        ]);
        env.extend([
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
        remove_after_ttl(secret_file.clone());
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
            env.extend([
                ("SSH_ASKPASS".into(), askpass.to_string_lossy().into_owned()),
                ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
                (
                    "TERMINUS_SSH_ASKPASS_FILE".into(),
                    secret_file.to_string_lossy().into_owned(),
                ),
                ("DISPLAY".into(), "terminus:0".into()),
            ]);
            remove_after_ttl(secret_file.clone());
        }
        remove_after_ttl(key_file.clone());
    }

    args.push(destination);

    Ok((
        Shell {
            program: Some("ssh".to_string()),
            args,
        },
        Some(env),
    ))
}

/// OpenSSH `-o` values that make a dead link end the session.
///
/// After a sleep or a server restart nothing ever arrives on the socket,
/// and without keepalives ssh waits on it forever: the tab just freezes.
/// Probing every 15s and giving up after 3 misses makes ssh exit with 255
/// within about 45s, which the tab turns into "Connection lost" with a
/// Reconnect button (`Screen::note_child_exit`).
pub(super) const KEEPALIVE_SSH_OPTIONS: &[&str] =
    &["ServerAliveInterval=15", "ServerAliveCountMax=3"];

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

/// How long an askpass secret / temp identity may live before a sweep
/// removes it (OpenSSH reads them during the handshake only).
const SSH_TEMP_SECRET_TTL: std::time::Duration = std::time::Duration::from_secs(120);

/// Per-user private directory for SSH temp secrets (`0700`, owned by us).
///
/// Lives under `$XDG_RUNTIME_DIR` when set (already per-user), else the temp
/// dir with the uid in the name. An existing directory is only reused when it
/// is a real directory we own (never a symlink) and gets its mode tightened,
/// so another local user cannot pre-create it, read the secrets or swap the
/// askpass helper.
pub(super) fn private_temp_dir(name: &str) -> Result<std::path::PathBuf, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        let uid = unsafe { libc::getuid() };
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("terminus-{name}-{uid}"));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(format!("Could not create {}: {err}", dir.display())),
        }
        let meta = std::fs::symlink_metadata(&dir)
            .map_err(|e| format!("Could not inspect {}: {e}", dir.display()))?;
        if !meta.file_type().is_dir() || meta.uid() != uid {
            return Err(format!(
                "Refusing to use {}: not a directory owned by this user",
                dir.display()
            ));
        }
        if meta.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("Could not secure {}: {e}", dir.display()))?;
        }
        Ok(dir)
    }
    #[cfg(not(unix))]
    {
        // %TEMP% is already per-user on Windows.
        let dir = std::env::temp_dir().join(format!("terminus-{name}"));
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
        Ok(dir)
    }
}

/// Create `path` exclusively with owner-only permissions and write `data`.
fn write_private_file(path: &std::path::Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    file.write_all(data)
        .map_err(|e| format!("Could not write {}: {e}", path.display()))
}

/// Remove secrets older than [`SSH_TEMP_SECRET_TTL`] from `dir` (leftovers of
/// a session whose cleanup thread died with the app).
fn sweep_stale_secrets(dir: &std::path::Path, extensions: &[&str]) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let wanted = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.contains(&e));
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > SSH_TEMP_SECRET_TTL);
        if wanted && stale {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Remove world-readable secrets left in the shared temp dirs used by older
/// builds (`/tmp/terminus-ssh-askpass`, `/tmp/terminus-ssh-identity`).
fn sweep_legacy_secret_dirs() {
    for (name, ext) in [
        ("terminus-ssh-askpass", "secret"),
        ("terminus-ssh-identity", "pem"),
    ] {
        let dir = std::env::temp_dir().join(name);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some(ext) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// Delete `path` after [`SSH_TEMP_SECRET_TTL`] on a background thread.
fn remove_after_ttl(path: std::path::PathBuf) {
    std::thread::spawn(move || {
        std::thread::sleep(SSH_TEMP_SECRET_TTL);
        let _ = std::fs::remove_file(path);
    });
}

/// Write a managed OpenSSH private key to a temp IdentityFile (mode 0600,
/// in a per-user 0700 directory).
pub(super) fn write_ssh_identity_file(pem: &str) -> Result<std::path::PathBuf, String> {
    sweep_legacy_secret_dirs();
    let dir = private_temp_dir("ssh-identity")?;
    sweep_stale_secrets(&dir, &["pem"]);
    let path = dir.join(format!("{}.pem", uuid::Uuid::new_v4()));
    let mut body = pem.as_bytes().to_vec();
    if !pem.ends_with('\n') {
        body.push(b'\n');
    }
    write_private_file(&path, &body)?;
    Ok(path)
}

/// Write a one-shot askpass helper + secret file into a per-user private
/// directory (secret `0600`, directory `0700`).
pub(super) fn write_ssh_askpass(
    password: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    sweep_legacy_secret_dirs();
    let dir = private_temp_dir("ssh-askpass")?;
    sweep_stale_secrets(&dir, &["secret"]);

    let secret_file = dir.join(format!("{}.secret", uuid::Uuid::new_v4()));
    let mut body = password.as_bytes().to_vec();
    body.push(b'\n');
    write_private_file(&secret_file, &body)?;

    #[cfg(windows)]
    let (name, script) = (
        "askpass.cmd",
        "@echo off\r\nif not defined TERMINUS_SSH_ASKPASS_FILE exit /b 1\r\ntype \"%TERMINUS_SSH_ASKPASS_FILE%\"\r\n",
    );
    #[cfg(not(windows))]
    let (name, script) = (
        "askpass.sh",
        "#!/bin/sh\n# Terminus SSH_ASKPASS helper — prints the sealed password file.\nif [ -z \"$TERMINUS_SSH_ASKPASS_FILE\" ] || [ ! -f \"$TERMINUS_SSH_ASKPASS_FILE\" ]; then\n  exit 1\nfi\ncat \"$TERMINUS_SSH_ASKPASS_FILE\"\n",
    );
    // Always rewrite the helper: the directory is private, and the content
    // must be ours even if an older build left a different one behind.
    let askpass = dir.join(name);
    let tmp = dir.join(format!("{name}.{}", uuid::Uuid::new_v4()));
    write_private_file(&tmp, script.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("Could not chmod askpass helper: {e}"))?;
    }
    std::fs::rename(&tmp, &askpass)
        .map_err(|e| format!("Could not install askpass helper: {e}"))?;

    Ok((askpass, secret_file))
}
