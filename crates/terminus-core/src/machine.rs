//! The machine terminus itself runs on.
//!
//! The sidebar pins one row above the stored hosts — *this computer* — and
//! that row needs the same facts a stored host gets: a hostname, a user, an
//! OS badge and, under WSL, the distro this session belongs to. Everything
//! here is read once from the filesystem and the environment; nothing is
//! inferred from a network round-trip.
//!
//! See also [`crate::wsl`], which discovers the *other* WSL distros installed
//! on the Windows machine this one is nested in.

use crate::os_detect::{is_wsl, local_uname_hint, parse_os_id, UNKNOWN_OS};

/// Facts about the local machine that are worth showing in the UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalMachine {
    /// Kernel hostname, e.g. `NixOS`. Empty when it could not be read.
    pub hostname: String,
    /// Login name, e.g. `nixos`. Empty when the environment does not say.
    pub username: String,
    /// Canonical OS id, same vocabulary as [`crate::os_detect`].
    pub os_id: String,
    /// The WSL distro this session is in, e.g. `NixOS`. `None` outside WSL.
    pub wsl_distro: Option<String>,
    /// The kernel announced itself as a WSL one.
    ///
    /// Kept apart from `wsl_distro` because the name comes from the
    /// environment (`WSL_DISTRO_NAME`), which a sandboxed or `su`-ed shell
    /// may not carry — while the kernel keeps saying WSL either way.
    pub wsl_kernel: bool,
}

impl LocalMachine {
    /// True when this machine is a WSL distro inside a Windows host.
    pub fn is_wsl(&self) -> bool {
        self.wsl_kernel || self.wsl_distro.is_some() || self.os_id == "wsl"
    }

    /// `user@host`, or just the host when the login name is unknown.
    pub fn endpoint(&self) -> String {
        match (self.username.is_empty(), self.hostname.is_empty()) {
            (false, false) => format!("{}@{}", self.username, self.hostname),
            (true, false) => self.hostname.clone(),
            (false, true) => self.username.clone(),
            (true, true) => String::new(),
        }
    }

    /// The second sidebar line: where the machine sits, then what it runs.
    ///
    /// Under WSL the distro name *is* the interesting part (it is what
    /// `wsl -d` takes, and what tells two sessions on the same box apart), so
    /// it leads; otherwise the OS id is the whole story.
    pub fn detail(&self) -> String {
        match &self.wsl_distro {
            Some(distro) => format!("WSL · {distro}"),
            None if self.os_id == "wsl" => "WSL".to_string(),
            None => os_label(&self.os_id),
        }
    }
}

/// A human name for a canonical OS id, for badges and subtitles.
///
/// Ids in the wild are inconsistent in case and spacing (`macos`, `nixos`,
/// `rockylinux`), so the handful that have a house style are spelled out and
/// everything else is merely capitalized.
pub fn os_label(os_id: &str) -> String {
    let known = match os_id {
        "macos" => "macOS",
        "nixos" => "NixOS",
        "rockylinux" => "Rocky Linux",
        "opensuse" => "openSUSE",
        "amazonlinux" => "Amazon Linux",
        "raspberrypi" => "Raspberry Pi OS",
        "linuxmint" => "Linux Mint",
        "popos" => "Pop!_OS",
        "wsl" => "WSL",
        "" | UNKNOWN_OS => return "Unknown".to_string(),
        other => {
            let mut chars = other.chars();
            return match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Unknown".to_string(),
            };
        }
    };

    known.to_string()
}

/// Builds a [`LocalMachine`] from already-gathered inputs.
///
/// Kept separate from [`detect`] so the parsing half can be tested: gathering
/// half gathers, this half decides, and only this half needs a test.
pub fn from_parts(
    os_release: &str,
    uname: &str,
    hostname: &str,
    username: &str,
    wsl_distro: Option<&str>,
) -> LocalMachine {
    let os_id = parse_os_id(os_release, uname);
    let wsl_distro = wsl_distro
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string);

    LocalMachine {
        hostname: hostname.trim().to_string(),
        username: username.trim().to_string(),
        os_id,
        wsl_distro,
        wsl_kernel: kernel_is_wsl(uname),
    }
}

/// Reads the local machine's facts from `/etc/os-release`, the kernel and the
/// environment. Never fails: unknown fields are left empty.
pub fn detect() -> LocalMachine {
    let os_release = read_first(&["/etc/os-release", "/usr/lib/os-release"]);
    let uname = local_uname_hint();
    let hostname = hostname_from(
        read_first(&["/proc/sys/kernel/hostname"]).as_str(),
        std::env::var("HOSTNAME").ok().as_deref(),
        std::env::var("COMPUTERNAME").ok().as_deref(),
    );
    let username = username_from([
        std::env::var("USER").ok(),
        std::env::var("LOGNAME").ok(),
        std::env::var("USERNAME").ok(),
    ]);

    from_parts(
        &os_release,
        &uname,
        &hostname,
        &username,
        std::env::var("WSL_DISTRO_NAME").ok().as_deref(),
    )
}

/// Whether the kernel string says we are under WSL. Cheap, for callers that
/// already have the uname line and only need the flag.
pub fn kernel_is_wsl(uname: &str) -> bool {
    is_wsl(uname)
}

fn read_first(paths: &[&str]) -> String {
    for path in paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            return content;
        }
    }
    String::new()
}

fn hostname_from(
    proc_hostname: &str,
    env_hostname: Option<&str>,
    env_computername: Option<&str>,
) -> String {
    [Some(proc_hostname), env_hostname, env_computername]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn username_from(candidates: [Option<String>; 3]) -> String {
    candidates
        .into_iter()
        .flatten()
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NIXOS_RELEASE: &str = r#"
NAME=NixOS
ID=nixos
VERSION_ID="25.05"
PRETTY_NAME="NixOS 25.05 (Warbler)"
"#;

    const UBUNTU_RELEASE: &str = r#"
PRETTY_NAME="Ubuntu 24.04.1 LTS"
NAME="Ubuntu"
VERSION_ID="24.04"
ID=ubuntu
ID_LIKE=debian
"#;

    const WSL_UNAME: &str = "Linux 5.15.153.1-microsoft-standard-WSL2 #1 SMP x86_64";

    #[test]
    fn wsl_machine_reports_its_distro() {
        let machine =
            from_parts(NIXOS_RELEASE, WSL_UNAME, "NixOS\n", "nixos", Some("NixOS"));

        assert_eq!(machine.os_id, "nixos");
        assert_eq!(machine.hostname, "NixOS");
        assert_eq!(machine.endpoint(), "nixos@NixOS");
        assert_eq!(machine.detail(), "WSL · NixOS");
        assert!(machine.is_wsl());
    }

    #[test]
    fn plain_linux_machine_has_no_distro() {
        let machine = from_parts(
            UBUNTU_RELEASE,
            "Linux 6.8.0-45-generic x86_64",
            "web-01",
            "deploy",
            None,
        );

        assert_eq!(machine.os_id, "ubuntu");
        assert_eq!(machine.endpoint(), "deploy@web-01");
        assert_eq!(machine.detail(), "Ubuntu");
        assert!(!machine.is_wsl());
    }

    #[test]
    fn blank_distro_name_is_not_a_distro() {
        let machine = from_parts(NIXOS_RELEASE, WSL_UNAME, "NixOS", "nixos", Some("   "));
        assert_eq!(machine.wsl_distro, None);
        assert_eq!(machine.detail(), "NixOS");
        // The kernel still says WSL, so the flag survives a missing name.
        assert!(machine.is_wsl());
    }

    #[test]
    fn missing_user_is_tolerated() {
        let machine = from_parts(UBUNTU_RELEASE, "", "  srv-9 ", "", None);
        assert_eq!(machine.endpoint(), "srv-9");
        assert_eq!(machine.hostname, "srv-9");
    }

    #[test]
    fn hostname_falls_back_through_env() {
        assert_eq!(hostname_from("box\n", None, None), "box");
        assert_eq!(hostname_from("", Some("from-env"), None), "from-env");
        assert_eq!(
            hostname_from("  ", None, Some("windows-name")),
            "windows-name"
        );
        assert_eq!(hostname_from("", None, None), "");
    }

    #[test]
    fn os_labels_read_well() {
        assert_eq!(os_label("nixos"), "NixOS");
        assert_eq!(os_label("macos"), "macOS");
        assert_eq!(os_label("ubuntu"), "Ubuntu");
        assert_eq!(os_label("rockylinux"), "Rocky Linux");
        assert_eq!(os_label(""), "Unknown");
        assert_eq!(os_label(UNKNOWN_OS), "Unknown");
    }

    #[test]
    fn kernel_probe_matches_os_detect() {
        assert!(kernel_is_wsl(WSL_UNAME));
        assert!(!kernel_is_wsl("Linux 6.8.0-45-generic x86_64"));
    }

    #[test]
    fn detect_on_this_host_is_coherent() {
        let machine = detect();
        assert!(!machine.os_id.is_empty());
        // Whatever the machine is, the parts agree with each other.
        if machine.is_wsl() && machine.wsl_distro.is_some() {
            assert!(machine.detail().starts_with("WSL · "));
        }
    }
}
