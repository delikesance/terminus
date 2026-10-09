//! Updates that go through the system: a package manager (with a polkit
//! password prompt) or the user's Nix profile.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{Result, UpdateError};

const PKEXEC: &str = "/usr/bin/pkexec";
const NIX_PROFILE_ELEMENT: &str = "terminus";
const LAUNCHER_NAME: &str = "terminus";

/// The system package manager that owns a `.deb` / `.rpm` install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Apt,
    Dnf,
}

impl PackageManager {
    fn binary(self) -> &'static str {
        match self {
            Self::Apt => "/usr/bin/apt-get",
            Self::Dnf => "/usr/bin/dnf",
        }
    }

    /// The elevated command that installs `package`. `--disable-internal-agent`
    /// makes pkexec fail without a graphical polkit agent instead of
    /// prompting on a terminal the worker thread cannot answer.
    pub fn install_argv(self, package: &Path) -> Vec<OsString> {
        vec![
            PKEXEC.into(),
            "--disable-internal-agent".into(),
            self.binary().into(),
            "install".into(),
            "-y".into(),
            package.into(),
        ]
    }

    /// The command to type when the elevated install is not possible.
    pub fn manual_command(self, package: &Path) -> String {
        let tool = match self {
            Self::Apt => "apt",
            Self::Dnf => "dnf",
        };
        format!("sudo {tool} install {}", package.display())
    }
}

/// Upgrade the Nix profile entry Terminus was installed as.
pub fn nix_upgrade_argv() -> Vec<OsString> {
    [
        "nix",
        "profile",
        "upgrade",
        "--refresh",
        NIX_PROFILE_ELEMENT,
    ]
    .map(OsString::from)
    .to_vec()
}

/// Profile launchers a `nix profile install` may have created, in lookup order.
fn nix_launcher_candidates(
    home: Option<&Path>,
    state_home: Option<&Path>,
) -> Vec<PathBuf> {
    let default_state = home.map(|h| h.join(".local/state"));
    let state = state_home.map(Path::to_path_buf).or(default_state);
    [
        home.map(|h| h.join(".nix-profile")),
        state.map(|s| s.join("nix/profile")),
    ]
    .into_iter()
    .flatten()
    .map(|profile| profile.join("bin").join(LAUNCHER_NAME))
    .collect()
}

/// The profile launcher that runs the store binary `exe`. It keeps pointing
/// at the newest build after `nix profile upgrade`, unlike `exe` itself.
pub fn nix_profile_launcher(exe: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let state_home = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
    nix_launcher_candidates(home.as_deref(), state_home.as_deref())
        .into_iter()
        .find(|launcher| std::fs::canonicalize(launcher).is_ok_and(|t| t == exe))
}

/// Run an install command to completion without a terminal, reporting its
/// stderr when it fails.
pub fn run_install(argv: &[OsString]) -> Result<()> {
    let Some((program, args)) = argv.split_first() else {
        return Err(UpdateError::Io("empty install command".into()));
    };
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| UpdateError::Io(format!("{}: {e}", program.to_string_lossy())))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.lines().last().unwrap_or("").trim();
    Err(UpdateError::Io(format!(
        "{} exited with {}: {detail}",
        program.to_string_lossy(),
        output.status
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dnf_install_is_one_argv_through_pkexec() {
        let argv = PackageManager::Dnf.install_argv(Path::new("/tmp/t.rpm"));
        assert_eq!(
            argv,
            [
                "/usr/bin/pkexec",
                "--disable-internal-agent",
                "/usr/bin/dnf",
                "install",
                "-y",
                "/tmp/t.rpm"
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn apt_uses_apt_get_not_the_interactive_apt() {
        let argv = PackageManager::Apt.install_argv(Path::new("/tmp/t.deb"));
        assert_eq!(argv[2], OsString::from("/usr/bin/apt-get"));
    }

    #[test]
    fn manual_commands_name_the_package_manager() {
        let path = Path::new("/tmp/t.deb");
        assert_eq!(
            PackageManager::Apt.manual_command(path),
            "sudo apt install /tmp/t.deb"
        );
        assert_eq!(
            PackageManager::Dnf.manual_command(Path::new("/tmp/t.rpm")),
            "sudo dnf install /tmp/t.rpm"
        );
    }

    #[test]
    fn nix_upgrade_targets_the_terminus_profile_entry() {
        assert_eq!(
            nix_upgrade_argv(),
            ["nix", "profile", "upgrade", "--refresh", "terminus"].map(OsString::from)
        );
    }

    #[test]
    fn nix_launchers_cover_both_profile_locations() {
        let found =
            nix_launcher_candidates(Some(Path::new("/home/me")), Some(Path::new("/s")));
        assert_eq!(
            found,
            [
                PathBuf::from("/home/me/.nix-profile/bin/terminus"),
                PathBuf::from("/s/nix/profile/bin/terminus"),
            ]
        );
        let defaulted = nix_launcher_candidates(Some(Path::new("/home/me")), None);
        assert_eq!(
            defaulted[1],
            PathBuf::from("/home/me/.local/state/nix/profile/bin/terminus")
        );
        assert!(nix_launcher_candidates(None, None).is_empty());
    }

    #[test]
    fn a_failing_command_reports_its_last_stderr_line() {
        let argv = ["sh", "-c", "echo boom >&2; exit 3"].map(OsString::from);
        let err = run_install(&argv).unwrap_err().to_string();
        assert!(err.contains("boom"), "{err}");
    }

    #[test]
    fn a_missing_program_is_an_error() {
        assert!(run_install(&["/nonexistent/prog".into()]).is_err());
        assert!(run_install(&[]).is_err());
    }
}
