//! Streaming a local file to a temp dir on an SSH host through the tab's own
//! `ssh` command line, and typing the remote path into the tab afterwards.
//! A POSIX host gets `/tmp`; a Windows host (OpenSSH runs `cmd.exe`, which has
//! no `/tmp`) gets the user's `%TEMP%` through PowerShell, which also reports
//! the path it wrote.

use super::clipboard::paste_bytes;
use super::Screen;
use crate::event::Msg;
use crate::ssh_secrets::{launch_secrets, SecretFiles};
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
        Ok(SshUpload {
            program: shell.program.unwrap_or_else(|| "ssh".to_string()),
            tab_args: shell.args,
            env: env.unwrap_or_default(),
            os: RemoteOs::of(os_id),
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
            let path = remote_paste_path(name);
            run_upload(ssh, &posix_upload_command(&path), input)?;
            Ok(path)
        }
        RemoteOs::Windows => {
            let path = run_upload(ssh, &windows_upload_command(name), input)?;
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
        std::io::copy(&mut input, &mut stdin)
            .map_err(|err| format!("sending the file: {err}"))?;
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

pub(super) fn remote_paste_path(name: &str) -> String {
    format!("/tmp/{name}")
}

/// The POSIX upload: stdin is written to `remote_path`, readable by the user only.
pub(super) fn posix_upload_command(remote_path: &str) -> String {
    format!("umask 077 && cat > {}", quote_for_shell(remote_path, false))
}

/// The Windows upload: stdin goes to a new file in `%TEMP%`, whose full path
/// is printed. The name is already sanitised, so it needs no escaping.
pub(super) fn windows_upload_command(name: &str) -> String {
    format!(
        "powershell -NoProfile -NonInteractive -Command \"$f = Join-Path $env:TEMP '{name}'; \
         $out = [IO.File]::Create($f); try {{ [Console]::OpenStandardInput().CopyTo($out) }} \
         finally {{ $out.Close() }}; $f\""
    )
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
    fn upload_reuses_tab_args_and_writes_stdin_to_tmp() {
        let tab = vec!["-p".to_string(), "2222".to_string(), "me@box".to_string()];
        let path = remote_paste_path("terminus-paste-1.png");
        assert_eq!(path, "/tmp/terminus-paste-1.png");
        assert_eq!(
            upload_args(tab, &posix_upload_command(&path)),
            vec![
                "-T",
                "-o",
                "ConnectTimeout=10",
                "-p",
                "2222",
                "me@box",
                "umask 077 && cat > /tmp/terminus-paste-1.png",
            ]
        );
    }

    #[test]
    fn windows_remote_gets_a_powershell_upload_into_temp() {
        assert_eq!(
            windows_upload_command("terminus-drop-1.txt"),
            "powershell -NoProfile -NonInteractive -Command \"$f = Join-Path $env:TEMP \
             'terminus-drop-1.txt'; $out = [IO.File]::Create($f); try { \
             [Console]::OpenStandardInput().CopyTo($out) } finally { $out.Close() }; $f\""
        );
    }

    #[test]
    fn only_a_windows_host_id_selects_the_windows_upload() {
        assert_eq!(RemoteOs::of(Some("windows")), RemoteOs::Windows);
        assert_eq!(RemoteOs::of(Some("linux")), RemoteOs::Posix);
        assert_eq!(RemoteOs::of(None), RemoteOs::Posix);
    }
}
