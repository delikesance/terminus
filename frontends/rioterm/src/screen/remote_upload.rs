//! Streaming a local file to a temp dir on an SSH host through the tab's own
//! `ssh` command line, and typing the remote path into the tab afterwards.
//! A POSIX host gets `/tmp`; a Windows host (OpenSSH runs `cmd.exe`, which has
//! no `/tmp`) gets the user's `%TEMP%` through PowerShell, which also reports
//! the path it wrote.

use super::clipboard::paste_bytes;
use super::Screen;
use crate::event::Msg;
use crate::ssh_secrets::{launch_secrets, SecretFiles};
use rio_backend::config::uploads::Uploads;
use std::io::Read;
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RemoteOs {
    Posix,
    Windows,
}

impl RemoteOs {
    pub(super) fn of(os_id: Option<&str>) -> Self {
        if os_id == Some("windows") {
            Self::Windows
        } else {
            Self::Posix
        }
    }
}

/// The tab's own `ssh` command line, reused for one-shot uploads.
pub(super) struct SshUpload {
    pub program: String,
    pub tab_args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub os: RemoteOs,
    /// The `[uploads]` directory for this host's OS; `None` is the default.
    pub dir: Option<String>,
}

impl RemoteOs {
    fn configured_dir(self, uploads: &Uploads) -> Option<String> {
        let dir = match self {
            Self::Posix => &uploads.dir,
            Self::Windows => &uploads.windows_dir,
        };
        (!dir.is_empty()).then(|| dir.clone())
    }
}

impl Screen<'_> {
    pub(super) fn ssh_upload_for(&self, id: &str) -> Result<SshUpload, String> {
        let (shell, env) = self.plain_shell_for_row(id)?;
        let shell = shell.ok_or("not an SSH host")?;
        let os_id = self
            .host_store
            .hosts()
            .iter()
            .find(|host| host.id == id)
            .and_then(|host| host.os_id.as_deref());
        let os = RemoteOs::of(os_id);
        Ok(SshUpload {
            program: shell.program.unwrap_or_else(|| "ssh".to_string()),
            tab_args: shell.args,
            env: env.unwrap_or_default(),
            os,
            dir: os.configured_dir(&self.context_manager.config.uploads),
        })
    }
}

/// Runs `upload` on its own thread, since the UI must not wait on the network.
/// The text it returns is typed into the tab; an error goes to `report`.
pub(super) fn paste_upload_result(
    label: &'static str,
    input: corcovado::channel::Sender<Msg>,
    bracketed: bool,
    report: impl Fn(String) + Send + 'static,
    upload: impl FnOnce() -> Result<String, String> + Send + 'static,
) -> Result<(), String> {
    std::thread::Builder::new()
        .name("terminus-upload".into())
        .spawn(move || match upload() {
            Ok(text) => {
                let bytes = paste_bytes(&text, true, bracketed);
                let _ = input.send(Msg::Input(bytes.into()));
            }
            Err(err) => report(format!("{label}: {err}")),
        })
        .map(|_| ())
        .map_err(|err| format!("could not start the upload: {err}"))
}

/// Streams `input` into a file called `name` in the host's temp dir and
/// returns the remote path of that file.
pub(super) fn upload_file(
    ssh: &SshUpload,
    name: &str,
    input: impl Read,
) -> Result<String, String> {
    match ssh.os {
        RemoteOs::Posix => {
            let dir = posix_upload_dir(ssh.dir.as_deref())?;
            let path = posix_remote_path(&dir, name);
            run_upload(ssh, &posix_upload_command(&dir, &path), input)?;
            Ok(path)
        }
        RemoteOs::Windows => {
            let command = windows_upload_command(ssh.dir.as_deref(), name)?;
            let path = run_upload(ssh, &command, input)?;
            if path.is_empty() {
                return Err("the host did not report where the file went".into());
            }
            Ok(path)
        }
    }
}

/// Runs the upload command and returns its trimmed stdout.
fn run_upload(
    ssh: &SshUpload,
    remote_command: &str,
    mut input: impl Read,
) -> Result<String, String> {
    let args = upload_args(ssh.tab_args.clone(), remote_command);
    // Same cleanup as a tab: the askpass / key files go once ssh is done.
    let _secrets = SecretFiles::new(launch_secrets(
        args.iter().map(String::as_str),
        ssh.env.iter().map(|(k, v)| (k.as_str(), v.as_str())),
    ));
    let mut cmd = Command::new(&ssh.program);
    cmd.args(&args)
        .envs(ssh.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|err| format!("{}: {err}", ssh.program))?;
    if let Some(mut stdin) = child.stdin.take() {
        // A failed copy means ssh already exited: its stderr says why.
        let _ = std::io::copy(&mut input, &mut stdin);
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("{}: {err}", ssh.program))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Quote `path` for the tab's shell only when it needs it (a temp dir under
/// a user name with a space). POSIX shells get single quotes, Windows ones
/// double quotes, which `cmd`, PowerShell and Claude Code all strip.
pub(super) fn quote_for_shell(path: &str, windows: bool) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "/\\:._-+~,@%".contains(c);
    if path.chars().all(plain) {
        return path.to_string();
    }
    if windows {
        format!("\"{path}\"")
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    }
}

/// The POSIX directory: `/tmp` unless configured. A trailing slash is dropped
/// so `<dir>/<name>` joins cleanly.
pub(super) fn posix_upload_dir(configured: Option<&str>) -> Result<String, String> {
    let dir = configured.unwrap_or("/tmp").trim_end_matches('/');
    if dir.chars().any(char::is_control) {
        return Err("the upload directory contains a control character".into());
    }
    Ok(dir.to_string())
}

pub(super) fn posix_remote_path(dir: &str, name: &str) -> String {
    format!("{dir}/{name}")
}

/// A path as the remote shell writes it: `~/` becomes `$HOME`, which only
/// the inner `sh -c` expands, so the path stays inside its quotes.
fn posix_shell_word(path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => format!("\"$HOME\"/{}", quote_for_shell(rest, false)),
        None => quote_for_shell(path, false),
    }
}

/// The POSIX upload: creates `dir`, then writes stdin to `remote_path`,
/// readable by the user only.
pub(super) fn posix_upload_command(dir: &str, remote_path: &str) -> String {
    let script = format!(
        "mkdir -p {} && umask 077 && cat > {}",
        posix_shell_word(dir),
        posix_shell_word(remote_path)
    );
    format!("sh -c {}", quote_for_shell(&script, false))
}

/// The Windows upload: creates the configured dir (or `%TEMP%`), writes stdin
/// to a new file there and prints its full path. The name is already
/// sanitised; a configured dir is refused when it would break the `cmd.exe`
/// double quotes around the PowerShell command.
pub(super) fn windows_upload_command(
    dir: Option<&str>,
    name: &str,
) -> Result<String, String> {
    let dir_expr = match dir {
        None => "$env:TEMP".to_string(),
        Some(dir) if dir.chars().any(|c| c == '"' || c.is_control()) => {
            return Err(
                "the Windows upload directory contains a quote or a control character"
                    .into(),
            );
        }
        Some(dir) => format!("'{}'", dir.replace('\'', "''")),
    };
    Ok(format!(
        "powershell -NoProfile -NonInteractive -Command \"[Console]::OutputEncoding = \
         New-Object Text.UTF8Encoding $false; $d = {dir_expr}; \
         New-Item -ItemType Directory -Force -Path $d | Out-Null; $f = Join-Path $d '{name}'; \
         $out = [IO.File]::Create($f); try {{ [Console]::OpenStandardInput().CopyTo($out) }} \
         finally {{ $out.Close() }}; $f\""
    ))
}

/// The tab's own `ssh` arguments turned into a one-shot upload: no pty
/// (stdin is the file), a bounded connect, and `remote_command` after the
/// host, which is the last tab argument.
pub(super) fn upload_args(tab_args: Vec<String>, remote_command: &str) -> Vec<String> {
    let mut args = vec![
        "-T".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    args.extend(tab_args);
    args.push(remote_command.to_string());
    args
}

#[cfg(test)]
#[path = "remote_upload_command_tests.rs"]
mod command_tests;

#[cfg(all(test, unix))]
#[path = "remote_upload_sshd_tests.rs"]
mod sshd_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_paths_that_need_it() {
        assert_eq!(
            quote_for_shell("/run/user/1000/terminus-paste-1000/a.png", false),
            "/run/user/1000/terminus-paste-1000/a.png"
        );
        assert_eq!(
            quote_for_shell(r"C:\Users\Jo\AppData\Local\Temp\a.png", true),
            r"C:\Users\Jo\AppData\Local\Temp\a.png"
        );
        assert_eq!(
            quote_for_shell("/tmp/my dir/a.png", false),
            "'/tmp/my dir/a.png'"
        );
        assert_eq!(
            quote_for_shell("/tmp/it's/a.png", false),
            r"'/tmp/it'\''s/a.png'"
        );
        assert_eq!(
            quote_for_shell(r"C:\Users\Jo Doe\a.png", true),
            r#""C:\Users\Jo Doe\a.png""#
        );
    }

    #[test]
    fn only_a_windows_host_id_selects_the_windows_upload() {
        assert_eq!(RemoteOs::of(Some("windows")), RemoteOs::Windows);
        assert_eq!(RemoteOs::of(Some("linux")), RemoteOs::Posix);
        assert_eq!(RemoteOs::of(None), RemoteOs::Posix);
    }
}
