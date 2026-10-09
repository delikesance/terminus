//! Shell integration: the snippets that make a shell emit OSC 133 / OSC 7,
//! and the environment that loads them into local sessions.
//!
//! The snippets live in `misc/shell-integration/` and are embedded so the
//! installed app can write them out; remote machines are documented, never
//! injected.

use std::path::{Path, PathBuf};

pub const BASH: &str = include_str!("../../../misc/shell-integration/terminus.bash");
pub const ZSH: &str = include_str!("../../../misc/shell-integration/terminus.zsh");
pub const FISH: &str = include_str!("../../../misc/shell-integration/terminus.fish");
const ZSHENV_SHIM: &str = include_str!("../../../misc/shell-integration/zshenv-shim");

/// OSC 133 / OSC 7 forms the snippets emit, as the shell's printf spells them.
pub const PROMPT_START: &str = r"\e]133;A\a";
pub const PROMPT_END: &str = r"\e]133;B\a";
pub const COMMAND_SUBMITTED: &str = r"\e]133;C\a";
pub const COMMAND_FINISHED: &str = r"\e]133;D;%s\a";
pub const CURRENT_DIR: &str = r"\e]7;file://%s%s\a";
/// The command line itself (VS Code's OSC 633;E), escaped by the snippet.
pub const COMMAND_LINE: &str = r"\e]633;E;%s\a";

/// Env var set in local sessions to the directory holding the snippets.
pub const ENV_DIR: &str = "TERMINUS_SHELL_INTEGRATION";

/// Write the snippets under `<base>/shell-integration` (rewriting only files
/// whose content changed) and return that directory.
pub fn install(base: &Path) -> std::io::Result<PathBuf> {
    let dir = base.join("shell-integration");
    std::fs::create_dir_all(dir.join("zsh"))?;
    for (rel, body) in [
        ("terminus.bash", BASH),
        ("terminus.zsh", ZSH),
        ("terminus.fish", FISH),
        ("zsh/.zshenv", ZSHENV_SHIM),
    ] {
        let path = dir.join(rel);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(body) {
            std::fs::write(&path, body)?;
        }
    }
    Ok(dir)
}

/// Environment that loads the integration into a local bash (via a one-shot
/// `PROMPT_COMMAND`) or zsh (via a `ZDOTDIR` shim). `orig_zdotdir` is the
/// caller's current `ZDOTDIR`, restored by the shim.
pub fn local_env(dir: &Path, orig_zdotdir: Option<&str>) -> Vec<(String, String)> {
    vec![
        (ENV_DIR.into(), dir.to_string_lossy().into_owned()),
        (
            "PROMPT_COMMAND".into(),
            r#". "$TERMINUS_SHELL_INTEGRATION/terminus.bash""#.into(),
        ),
        (
            "ZDOTDIR".into(),
            dir.join("zsh").to_string_lossy().into_owned(),
        ),
        (
            "TERMINUS_ORIG_ZDOTDIR".into(),
            orig_zdotdir.unwrap_or("").to_string(),
        ),
    ]
}

/// Whether a session started with `program` (`None` = the app's own shell)
/// is a local POSIX shell the integration env can load into. `ssh`, `wsl.exe`
/// and other programs are never injected.
pub fn is_local_posix_shell(program: Option<&str>) -> bool {
    let Some(program) = program else {
        return true;
    };
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    matches!(name, "bash" | "zsh" | "fish" | "sh")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_posix_shells_get_the_env() {
        assert!(is_local_posix_shell(None));
        assert!(is_local_posix_shell(Some("/run/current-system/sw/bin/zsh")));
        assert!(is_local_posix_shell(Some("bash")));
        assert!(!is_local_posix_shell(Some("ssh")));
        assert!(!is_local_posix_shell(Some(
            "C:\\Windows\\System32\\wsl.exe"
        )));
    }

    #[test]
    fn bash_and_zsh_emit_every_mark_with_exact_escapes() {
        for (name, body) in [("bash", BASH), ("zsh", ZSH)] {
            assert!(body.contains(PROMPT_START), "{name} A");
            assert!(body.contains(PROMPT_END), "{name} B");
            assert!(body.contains(COMMAND_SUBMITTED), "{name} C");
            assert!(body.contains(COMMAND_FINISHED), "{name} D");
            assert!(body.contains(CURRENT_DIR), "{name} OSC 7");
        }
    }

    #[test]
    fn fish_emits_every_mark_with_exact_escapes() {
        assert!(FISH.contains(PROMPT_START));
        assert!(FISH.contains(PROMPT_END));
        assert!(FISH.contains(COMMAND_SUBMITTED));
        assert!(FISH.contains(COMMAND_FINISHED));
        assert!(FISH.contains(CURRENT_DIR));
    }

    #[test]
    fn every_shell_sends_the_command_line_escaped() {
        for (name, body) in [("bash", BASH), ("zsh", ZSH), ("fish", FISH)] {
            assert!(body.contains(COMMAND_LINE), "{name} OSC 633;E");
            assert!(body.contains("__terminus_escape"), "{name} escapes it");
            assert!(body.contains(r"x3b"), "{name} escapes ;");
        }
        // E must reach the terminal before C (bash: PS0 runs it first).
        assert!(BASH.contains(r"PS0='$(__terminus_cmdline)\e]133;C\a'"));
    }

    #[test]
    fn zsh_never_assigns_its_read_only_status() {
        assert!(!ZSH.contains("local status"));
    }

    #[test]
    fn local_env_bootstraps_bash_and_zsh() {
        let env = local_env(Path::new("/d/shell-integration"), Some("/home/u/.zsh"));
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get(ENV_DIR), Some("/d/shell-integration"));
        assert_eq!(
            get("PROMPT_COMMAND"),
            Some(r#". "$TERMINUS_SHELL_INTEGRATION/terminus.bash""#)
        );
        assert_eq!(get("ZDOTDIR"), Some("/d/shell-integration/zsh"));
        assert_eq!(get("TERMINUS_ORIG_ZDOTDIR"), Some("/home/u/.zsh"));
    }

    #[test]
    fn install_writes_files_once() {
        let base =
            std::env::temp_dir().join(format!("terminus-si-{}", uuid::Uuid::new_v4()));
        let dir = install(&base).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("terminus.bash")).unwrap(),
            BASH
        );
        assert!(dir.join("zsh/.zshenv").exists());
        install(&base).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Runs a real interactive bash with the bootstrap and checks the byte
    /// stream a terminal would see. Skipped when bash is missing.
    #[cfg(unix)]
    #[test]
    fn bash_session_emits_marks_in_order() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let base =
            std::env::temp_dir().join(format!("terminus-si-{}", uuid::Uuid::new_v4()));
        let dir = install(&base).unwrap();
        let Ok(mut child) = Command::new("sh")
            // `--noediting`: without readline the output is the same whether
            // bash was built with it (Ubuntu, CI) or not (nixpkgs `bash`), so
            // typed input is never echoed back into the stream.
            .args(["-c", "exec bash --noprofile --norc --noediting -i 2>&1"])
            .envs(local_env(&dir, None))
            .env("PS1", "$ ")
            .env("HISTCONTROL", "ignorespace")
            .env("HISTFILE", "/dev/null")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        else {
            return;
        };
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"echo hi\nfalse\necho \"a;b\\\\c\"\n echo secret\nexit\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        // Without a tty bash does not strip the `\[ \]` readline markers.
        let text = String::from_utf8_lossy(&out.stdout)
            .replace("\\[", "")
            .replace("\\]", "");
        let _ = std::fs::remove_dir_all(&base);
        let a = "\x1b]133;A\x07";
        let b = "\x1b]133;B\x07";
        let c = "\x1b]133;C\x07";
        // A comes before the first prompt; what bash prints ahead of it
        // (job-control warnings without a tty) varies by environment.
        let first_a = text.find(a).expect("no prompt start mark");
        assert!(first_a < text.find(&format!("$ {b}")).unwrap(), "{text:?}");
        assert!(
            text.contains(&format!("$ {b}\x1b]633;E;echo hi\x07{c}hi")),
            "{text:?}"
        );
        assert!(text.contains(&format!("{c}hi")), "{text:?}");
        assert!(text.contains("\x1b]133;D;0\x07"), "{text:?}");
        assert!(text.contains("\x1b]133;D;1\x07"), "{text:?}");
        assert!(text.contains("\x1b]7;file://"), "{text:?}");
        // The bootstrap must not linger in PROMPT_COMMAND.
        assert_eq!(text.matches("\x1b]133;A\x07").count(), 5, "{text:?}");
        // The command line goes out explicitly, escaped, right before C,
        // from the very first command on.
        let e = |cmd: &str| format!("\x1b]633;E;{cmd}\x07{c}");
        assert!(text.contains(&e("echo hi")), "{text:?}");
        assert!(text.contains(&e("false")), "{text:?}");
        assert!(text.contains(&e(r#"echo "a\x3bb\\\\c""#)), "{text:?}");
        // A command bash keeps out of its history is not sent.
        assert!(!text.contains("secret\x07"), "{text:?}");
        assert_eq!(text.matches("\x1b]633;E;").count(), 4, "{text:?}");
    }

    /// Same with a real zsh, when one is installed.
    #[cfg(unix)]
    #[test]
    fn zsh_session_sends_the_command_line() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let base =
            std::env::temp_dir().join(format!("terminus-si-{}", uuid::Uuid::new_v4()));
        let dir = install(&base).unwrap();
        let Ok(mut child) = Command::new("zsh")
            .args(["-f", "-i", "-s"])
            .env("HOME", &base)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        else {
            let _ = std::fs::remove_dir_all(&base);
            return;
        };
        let script = format!(
            "source '{}'\necho \"a;b\"\nfalse\nexit\n",
            dir.join("terminus.zsh").display()
        );
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        let _ = std::fs::remove_dir_all(&base);
        let c = "\x1b]133;C\x07";
        assert!(
            text.contains(&format!("\x1b]633;E;echo \"a\\x3bb\"\x07{c}")),
            "{text:?}"
        );
        assert!(
            text.contains(&format!("\x1b]633;E;false\x07{c}")),
            "{text:?}"
        );
        assert!(text.contains("\x1b]133;D;1\x07"), "{text:?}");
        assert!(!text.contains("read-only"), "{text:?}");
    }

    /// A directory holding a fake `ssh` that prints the `TERM` it was
    /// started with, i.e. the one a real `ssh` would put in its pty request.
    #[cfg(unix)]
    fn fake_ssh(base: &Path) -> String {
        use std::os::unix::fs::PermissionsExt;
        let bin = base.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let ssh = bin.join("ssh");
        std::fs::write(&ssh, "#!/bin/sh\necho \"remote-term=$TERM args=$*\"\n").unwrap();
        std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        )
    }

    /// Feed `script` to an interactive shell and return what it printed.
    /// `None` when the shell is not installed.
    #[cfg(unix)]
    fn run_shell(
        program: &str,
        args: &[&str],
        env: &[(String, String)],
        script: &str,
    ) -> Option<String> {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = Command::new(program)
            .args(args)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .env("HISTFILE", "/dev/null")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        Some(
            String::from_utf8_lossy(&out.stdout).into_owned()
                + &String::from_utf8_lossy(&out.stderr),
        )
    }

    /// `ssh` typed (or pasted from "Copy SSH command") in a local tab must
    /// not hand the remote our own `xterm-rio`: no host has its terminfo, so
    /// readline, `tput` and `htop` break there. Other TERMs pass untouched.
    #[cfg(unix)]
    fn assert_ssh_downgrades_rio_term(name: &str, text: &str) {
        let remote = format!("remote-term={} args=-p 22 box", crate::ssh::DEFAULT_TERM);
        assert_eq!(text.matches(&remote).count(), 2, "{name}: {text:?}");
        assert!(
            text.contains("remote-term=screen args=box"),
            "{name}: {text:?}"
        );
        assert!(!text.contains("remote-term=xterm-rio"), "{name}: {text:?}");
        assert!(!text.contains("remote-term=rio "), "{name}: {text:?}");
        // Only the ssh child sees the change, not the local shell.
        assert!(text.contains("local-term=xterm-rio"), "{name}: {text:?}");
    }

    #[cfg(unix)]
    const SSH_SCRIPT: &str = "ssh -p 22 box\n\
        TERM=rio ssh -p 22 box\n\
        TERM=screen ssh box\n\
        echo local-term=$TERM\n\
        exit\n";

    #[cfg(unix)]
    #[test]
    fn bash_ssh_advertises_a_term_remotes_know() {
        let base =
            std::env::temp_dir().join(format!("terminus-si-{}", uuid::Uuid::new_v4()));
        let dir = install(&base).unwrap();
        let mut env = local_env(&dir, None);
        env.push(("PATH".into(), fake_ssh(&base)));
        env.push(("TERM".into(), "xterm-rio".into()));
        let text = run_shell("bash", &["--noprofile", "--norc", "-i"], &env, SSH_SCRIPT);
        let _ = std::fs::remove_dir_all(&base);
        let Some(text) = text else { return };
        assert_ssh_downgrades_rio_term("bash", &text);
    }

    #[cfg(unix)]
    #[test]
    fn zsh_ssh_advertises_a_term_remotes_know() {
        let base =
            std::env::temp_dir().join(format!("terminus-si-{}", uuid::Uuid::new_v4()));
        let dir = install(&base).unwrap();
        let env = vec![
            ("HOME".to_string(), base.to_string_lossy().into_owned()),
            ("PATH".into(), fake_ssh(&base)),
            ("TERM".into(), "xterm-rio".into()),
        ];
        let script = format!(
            "source '{}'\n{SSH_SCRIPT}",
            dir.join("terminus.zsh").display()
        );
        let text = run_shell("zsh", &["-f", "-i", "-s"], &env, &script);
        let _ = std::fs::remove_dir_all(&base);
        let Some(text) = text else { return };
        assert_ssh_downgrades_rio_term("zsh", &text);
    }

    #[cfg(unix)]
    #[test]
    fn fish_ssh_advertises_a_term_remotes_know() {
        let base =
            std::env::temp_dir().join(format!("terminus-si-{}", uuid::Uuid::new_v4()));
        let dir = install(&base).unwrap();
        let env = vec![
            ("HOME".to_string(), base.to_string_lossy().into_owned()),
            (
                "XDG_CONFIG_HOME".into(),
                base.join("config").to_string_lossy().into_owned(),
            ),
            (
                "XDG_DATA_HOME".into(),
                base.join("data").to_string_lossy().into_owned(),
            ),
            ("PATH".into(), fake_ssh(&base)),
            ("TERM".into(), "xterm-rio".into()),
        ];
        let script = format!(
            "source '{}'\n{}",
            dir.join("terminus.fish").display(),
            SSH_SCRIPT.replace("$TERM", "$TERM;")
        );
        let text = run_shell("fish", &["--no-config", "-i"], &env, &script);
        let _ = std::fs::remove_dir_all(&base);
        let Some(text) = text else { return };
        assert_ssh_downgrades_rio_term("fish", &text);
    }

    /// A user's `ssh` alias stays in charge, and our definition must not
    /// trip over it: shells alias-expand `ssh() {`, which is a syntax error
    /// printed at every new tab.
    #[cfg(unix)]
    #[test]
    fn bash_and_zsh_keep_a_user_ssh_alias() {
        let base =
            std::env::temp_dir().join(format!("terminus-si-{}", uuid::Uuid::new_v4()));
        let dir = install(&base).unwrap();
        let rc = base.join("bashrc");
        std::fs::write(&rc, "alias ssh='echo aliased'\n").unwrap();
        let mut env = local_env(&dir, None);
        env.push(("PATH".into(), fake_ssh(&base)));
        env.push(("TERM".into(), "xterm-rio".into()));
        let rc = rc.to_string_lossy().into_owned();
        let bash = run_shell(
            "bash",
            &["--noprofile", "--rcfile", &rc, "-i"],
            &env,
            "ssh box\nexit\n",
        );
        let zsh_env = vec![
            ("HOME".to_string(), base.to_string_lossy().into_owned()),
            ("PATH".into(), fake_ssh(&base)),
            ("TERM".into(), "xterm-rio".into()),
        ];
        let zsh = run_shell(
            "zsh",
            &["-f", "-i", "-s"],
            &zsh_env,
            &format!(
                "alias ssh='echo aliased'\nsource '{}'\nssh box\nexit\n",
                dir.join("terminus.zsh").display()
            ),
        );
        let _ = std::fs::remove_dir_all(&base);
        for (name, text) in [("bash", bash), ("zsh", zsh)] {
            let Some(text) = text else { continue };
            assert!(text.contains("aliased box"), "{name}: {text:?}");
            assert!(!text.contains("syntax error"), "{name}: {text:?}");
            assert!(!text.contains("parse error"), "{name}: {text:?}");
            assert!(!text.contains("remote-term="), "{name}: {text:?}");
        }
    }

    /// A user's own `ssh` function (or alias) wins over ours.
    #[cfg(unix)]
    #[test]
    fn bash_keeps_a_user_defined_ssh() {
        let base =
            std::env::temp_dir().join(format!("terminus-si-{}", uuid::Uuid::new_v4()));
        let dir = install(&base).unwrap();
        let rc = base.join("bashrc");
        std::fs::write(&rc, "ssh() { echo mine; }\n").unwrap();
        let mut env = local_env(&dir, None);
        env.push(("PATH".into(), fake_ssh(&base)));
        env.push(("TERM".into(), "xterm-rio".into()));
        let rc = rc.to_string_lossy().into_owned();
        let text = run_shell(
            "bash",
            &["--noprofile", "--rcfile", &rc, "-i"],
            &env,
            "ssh box\nexit\n",
        );
        let _ = std::fs::remove_dir_all(&base);
        let Some(text) = text else { return };
        assert!(text.contains("mine"), "{text:?}");
        assert!(!text.contains("remote-term="), "{text:?}");
    }
}
