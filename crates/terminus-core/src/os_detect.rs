//! Remote OS detection.
//!
//! The sidebar renders an OS badge per host; that badge is keyed by a *stable*
//! identifier (`ubuntu`, `arch`, `rhel`, `freebsd`, `windows`, ...) rather than
//! by whatever `/etc/os-release` says, because distro ids drift (`ol` →
//! `oracle`, `sles` → `opensuse`).
//!
//! Two entry points:
//!
//! * [`parse_os_id`] — combines `/etc/os-release` with `uname -a` output into a
//!   canonical id (used right after SSH connect);
//! * [`canonical_os_id`] — maps a single raw id/alias onto its icon key.

use std::collections::HashMap;

/// Returned when nothing could be identified.
pub const UNKNOWN_OS: &str = "unknown";

/// Parses `/etc/os-release` (`KEY=value`, quoted or not) into a key/value map.
pub fn parse_os_release(content: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        let mut value = value.trim();
        // Strip a single pair of matching quotes.
        if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value = &value[1..value.len() - 1];
        }
        values.insert(key.to_ascii_uppercase(), value.to_string());
    }

    values
}

/// Detects the OS id from `/etc/os-release` content, falling back to `uname`.
///
/// `uname_output` is the raw output of `uname -a` (or a `{os} {kernel}` string)
/// and is only consulted when the release file is missing/unreadable or does
/// not carry an `ID`.
pub fn parse_os_id(os_release_content: &str, uname_output: &str) -> String {
    let release = parse_os_release(os_release_content);

    // 1. The primary id.
    if let Some(id) = release.get("ID") {
        if !id.trim().is_empty() {
            return canonical_os_id(id);
        }
    }

    // 2. `ID_LIKE` names a family (e.g. "debian ubuntu", "rhel fedora").
    if let Some(like) = release.get("ID_LIKE") {
        for token in like.split_whitespace() {
            let canonical = canonical_os_id(token);
            if canonical != UNKNOWN_OS {
                return canonical;
            }
        }
    }

    // 3. Fall back to the kernel banner.
    detect_from_uname(uname_output)
}

/// Maps a `uname -a` style banner onto a canonical id.
pub fn detect_from_uname(uname_output: &str) -> String {
    let lower = uname_output.to_ascii_lowercase();

    if lower.trim().is_empty() {
        return UNKNOWN_OS.to_string();
    }

    // Kernel names first: they are unambiguous.
    if lower.contains("darwin") || lower.contains("mac os") {
        return "macos".to_string();
    }
    if lower.contains("freebsd") {
        return "freebsd".to_string();
    }
    if lower.contains("openbsd") {
        return "openbsd".to_string();
    }
    if lower.contains("netbsd") {
        return "netbsd".to_string();
    }
    if lower.contains("dragonfly") {
        return "dragonflybsd".to_string();
    }
    if lower.contains("sunos") || lower.contains("solaris") || lower.contains("illumos") {
        return "solaris".to_string();
    }
    if lower.contains("android") {
        return "android".to_string();
    }

    // Windows-ish toolchains (Git Bash, MSYS2, Cygwin, WSL1/2 kernels).
    if lower.contains("mingw")
        || lower.contains("msys")
        || lower.contains("cygwin")
        || lower.contains("windows_nt")
        || lower.contains("windows")
    {
        return "windows".to_string();
    }

    if lower.contains("linux") || lower.contains("gnu/linux") {
        // A Linux kernel running under WSL is still Linux; only report `wsl`
        // when we have nothing better (no distro id at all).
        if is_wsl(uname_output) {
            return "wsl".to_string();
        }
        return "linux".to_string();
    }

    UNKNOWN_OS.to_string()
}

/// Whether the kernel banner looks like WSL (`*microsoft*`, `*WSL*`).
pub fn is_wsl(uname_output: &str) -> bool {
    let lower = uname_output.to_ascii_lowercase();
    lower.contains("microsoft") || lower.contains("wsl")
}

/// Maps a raw distro id (or an alias such as `ol`, `pop_os`, `opensuse-leap`)
/// onto the stable identifier used to pick the sidebar icon.
pub fn canonical_os_id(raw: &str) -> String {
    let normalized = normalize_id(raw);

    if normalized.is_empty() {
        return UNKNOWN_OS.to_string();
    }

    let canonical = match normalized.as_str() {
        // --- Debian family -------------------------------------------------
        "debian" | "debian_gnu_linux" => "debian",
        "ubuntu" | "ubuntu_core" | "ubuntu-core" => "ubuntu",
        "linuxmint" | "mint" | "lmde" => "linuxmint",
        "pop" | "pop_os" | "popos" | "system76" => "popos",
        "elementary" | "elementary_os" | "elementaryos" => "elementary",
        "kali" | "kali_linux" | "kalilinux" => "kali",
        "raspbian" | "raspberrypi" | "raspberry_pi_os" | "raspberry_pi" => "raspberrypi",
        "armbian" => "armbian",
        "deepin" | "deepin_linux" => "deepin",
        "zorin" | "zorin_os" => "zorin",
        "parrot" | "parrot_os" | "parrotsec" => "parrot",
        "devuan" => "devuan",
        "neon" | "kdeneon" | "kde_neon" => "neon",
        "mx" | "mx_linux" | "mxlinux" => "mxlinux",
        "proxmox" | "proxmox_ve" | "pve" => "proxmox",

        // --- Arch family ---------------------------------------------------
        "arch" | "archlinux" | "arch_linux" => "arch",
        "manjaro" | "manjaro_linux" | "manjarolinux" => "manjaro",
        "endeavouros" | "endeavour" | "endeavour_os" => "endeavouros",
        "garuda" | "garuda_linux" => "garuda",
        "artix" | "artix_linux" => "artix",
        "cachyos" => "cachyos",
        "arcolinux" | "arco" => "arcolinux",
        "steamos" | "steam_os" => "steamos",

        // --- Red Hat family ------------------------------------------------
        "rhel"
        | "redhat"
        | "red_hat"
        | "red_hat_enterprise_linux"
        | "ol"
        | "oracle"
        | "oraclelinux"
        | "oracle_linux" => "rhel",
        "centos" | "centos_linux" | "centos_stream" => "centos",
        "rocky" | "rockylinux" | "rocky_linux" => "rockylinux",
        "almalinux" | "alma" | "almalinux_deb" => "almalinux",
        "fedora" | "fedora_linux" | "fedora_coreos" | "nobara" => "fedora",
        "amzn" | "amazon" | "amazonlinux" | "amazon_linux" => "amazonlinux",
        "scientific" | "scientific_linux" | "sles_sap" => "rhel",

        // --- SUSE ----------------------------------------------------------
        "opensuse"
        | "opensuse_leap"
        | "opensuse_tumbleweed"
        | "opensuseleap"
        | "opensuse_tumbleweed"
        | "suse"
        | "sles"
        | "sled"
        | "suse_linux_enterprise_server" => "opensuse",

        // --- Independent distros ------------------------------------------
        "nixos" | "nix" => "nixos",
        "alpine" | "alpinelinux" => "alpine",
        "void" | "void_linux" | "voidlinux" => "void",
        "gentoo" | "funtoo" | "sabayon" => "gentoo",
        "slackware" | "slackware_linux" => "slackware",
        "clear" | "clear_linux" | "clearlinux" | "clear-linux-os" => "clearlinux",
        "coreos" | "container_linux" | "flatcar" | "flatcar_container_linux" => "flatcar",
        "photon" | "photon_os" | "vmware_photon" => "photonos",
        "bottlerocket" | "bottlerocket_os" => "bottlerocket",
        "kylin" | "openkylin" | "kylin_linux" => "kylin",
        "openeuler" | "open_euler" | "euleros" => "openeuler",
        "anolis" | "anolis_os" => "anolis",
        "tencentos" | "tencent_os" => "tencentos",
        "alios" | "alibaba" | "alinux" => "alinux",
        "uos" | "uniontechos" | "deepinos" => "uos",

        // --- Non-Linux families -------------------------------------------
        "freebsd" => "freebsd",
        "openbsd" => "openbsd",
        "netbsd" => "netbsd",
        "dragonfly" | "dragonflybsd" => "dragonflybsd",
        "macos" | "mac" | "osx" | "mac_os_x" | "darwin" => "macos",
        "windows" | "windows_nt" | "win32" | "winnt" | "cygwin" | "msys" | "mingw"
        | "mingw64_nt" | "mingw32_nt" => "windows",
        "solaris" | "sunos" | "opensolaris" | "solaris_studio" => "solaris",
        "illumos" | "omnios" | "openindiana" => "illumos",
        "android" | "termux" | "android_os" => "android",
        "wsl" | "wsl2" | "wsl1" => "wsl",
        "linux" | "gnu_linux" | "gnu" => "linux",

        _ => return guess_from_substring(&normalized),
    };

    canonical.to_string()
}

/// Last-resort heuristic for ids we have never seen (e.g. `mycompany_ubuntu`).
fn guess_from_substring(normalized: &str) -> String {
    const HINTS: [(&str, &str); 12] = [
        ("ubuntu", "ubuntu"),
        ("debian", "debian"),
        ("arch", "arch"),
        ("fedora", "fedora"),
        ("centos", "centos"),
        ("redhat", "rhel"),
        ("rhel", "rhel"),
        ("suse", "opensuse"),
        ("alpine", "alpine"),
        ("nix", "nixos"),
        ("freebsd", "freebsd"),
        ("windows", "windows"),
    ];

    for (needle, canonical) in HINTS {
        if normalized.contains(needle) {
            return canonical.to_string();
        }
    }

    UNKNOWN_OS.to_string()
}

/// Lower-cases an id and turns separators into a single `_`.
fn normalize_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_was_separator = false;

    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator && !out.is_empty() {
            out.push('_');
            last_was_separator = true;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// Shell snippet run on a remote after SSH connect to classify the OS icon.
///
/// Markers let [`parse_remote_os_probe`] split release file vs `uname` even when
/// either side is empty or prints errors on stderr (discarded by callers).
pub const REMOTE_OS_PROBE_SCRIPT: &str = concat!(
    "printf '%s\\n' '---OSRELEASE---'\n",
    "cat /etc/os-release 2>/dev/null || cat /usr/lib/os-release 2>/dev/null || true\n",
    "printf '%s\\n' '---UNAME---'\n",
    "uname -a 2>/dev/null || true\n",
);

/// Parse stdout from [`REMOTE_OS_PROBE_SCRIPT`] into a canonical `os_id`.
pub fn parse_remote_os_probe(stdout: &str) -> String {
    let mut release = String::new();
    let mut uname = String::new();
    let mut section = None::<&str>;
    for line in stdout.lines() {
        match line.trim_end() {
            "---OSRELEASE---" => {
                section = Some("release");
                continue;
            }
            "---UNAME---" => {
                section = Some("uname");
                continue;
            }
            _ => {}
        }
        match section {
            Some("release") => {
                release.push_str(line);
                release.push('\n');
            }
            Some("uname") => {
                if uname.is_empty() {
                    uname.push_str(line.trim());
                }
            }
            None | Some(_) => {}
        }
    }
    parse_os_id(&release, &uname)
}

/// Detects the OS of the *local* machine (the one running terminus).
///
/// Reads `/etc/os-release` (Linux) and falls back to [`std::env::consts::OS`]
/// plus the running kernel release, which is enough to flag WSL.
pub async fn detect_local_os() -> String {
    let release = match tokio::fs::read_to_string("/etc/os-release").await {
        Ok(content) => content,
        Err(_) => tokio::fs::read_to_string("/usr/lib/os-release")
            .await
            .unwrap_or_default(),
    };

    parse_os_id(&release, &local_uname_hint())
}

pub fn local_uname_hint() -> String {
    let platform = std::env::consts::OS;

    #[cfg(target_os = "linux")]
    let kernel =
        std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    #[cfg(not(target_os = "linux"))]
    let kernel = String::new();

    format!("{platform} {kernel}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const UBUNTU_RELEASE: &str = r#"
# /etc/os-release
PRETTY_NAME="Ubuntu 24.04.1 LTS"
NAME="Ubuntu"
VERSION_ID="24.04"
ID=ubuntu
ID_LIKE=debian
HOME_URL="https://www.ubuntu.com/"
"#;

    const ALPINE_RELEASE: &str = r#"
NAME="Alpine Linux"
ID=alpine
VERSION_ID=3.20.3
PRETTY_NAME="Alpine Linux v3.20"
"#;

    #[test]
    fn parses_key_values_including_quotes() {
        let values = parse_os_release(UBUNTU_RELEASE);
        assert_eq!(values.get("ID").map(String::as_str), Some("ubuntu"));
        assert_eq!(values.get("ID_LIKE").map(String::as_str), Some("debian"));
        assert_eq!(
            values.get("PRETTY_NAME").map(String::as_str),
            Some("Ubuntu 24.04.1 LTS")
        );
        assert_eq!(values.get("VERSION_ID").map(String::as_str), Some("24.04"));
        // Comments are ignored.
        assert!(!values.contains_key("# /ETC/OS-RELEASE"));
    }

    #[test]
    fn parse_os_id_prefers_id_then_id_like() {
        assert_eq!(parse_os_id(UBUNTU_RELEASE, "Linux ubuntu 6.8.0"), "ubuntu");
        assert_eq!(parse_os_id(ALPINE_RELEASE, "Linux alpine 6.6"), "alpine");

        // No ID -> ID_LIKE family.
        let no_id = "NAME=Foo\nID_LIKE=\"rhel fedora\"\n";
        assert_eq!(parse_os_id(no_id, "Linux foo 6.1"), "rhel");

        // Nothing usable -> uname fallback.
        assert_eq!(parse_os_id("", "Darwin MacBook-Pro 23.5.0 arm64"), "macos");
        assert_eq!(
            parse_os_id("", "Linux 5.15.153.1-microsoft-standard-WSL2"),
            "wsl"
        );
        assert_eq!(
            parse_os_id("", "MINGW64_NT-10.0-22631 runner 3.4.10"),
            "windows"
        );
        assert_eq!(parse_os_id("", "FreeBSD host 14.1-RELEASE"), "freebsd");
        assert_eq!(parse_os_id("", ""), UNKNOWN_OS);
    }

    #[test]
    fn wsl_kernel_is_detected() {
        assert!(is_wsl("Linux 5.15.153.1-microsoft-standard-WSL2 x86_64"));
        assert!(is_wsl("Linux 4.4.0-19041-Microsoft"));
        assert!(!is_wsl("Linux 6.8.0-45-generic x86_64"));
    }

    #[test]
    fn canonical_ids_map_aliases_to_icon_keys() {
        let cases = [
            ("ubuntu", "ubuntu"),
            ("Ubuntu", "ubuntu"),
            ("debian", "debian"),
            ("arch", "arch"),
            ("archlinux", "arch"),
            ("manjaro", "manjaro"),
            ("fedora", "fedora"),
            ("rhel", "rhel"),
            ("Red Hat Enterprise Linux", "rhel"),
            ("ol", "rhel"),
            ("oracle", "rhel"),
            ("centos", "centos"),
            ("rocky", "rockylinux"),
            ("almalinux", "almalinux"),
            ("amzn", "amazonlinux"),
            ("opensuse-leap", "opensuse"),
            ("sles", "opensuse"),
            ("nixos", "nixos"),
            ("alpine", "alpine"),
            ("void", "void"),
            ("gentoo", "gentoo"),
            ("slackware", "slackware"),
            ("raspbian", "raspberrypi"),
            ("linuxmint", "linuxmint"),
            ("pop_os", "popos"),
            ("elementary-os", "elementary"),
            ("kali", "kali"),
            ("steamos", "steamos"),
            ("freebsd", "freebsd"),
            ("openbsd", "openbsd"),
            ("netbsd", "netbsd"),
            ("darwin", "macos"),
            ("macos", "macos"),
            ("windows_nt", "windows"),
            ("mingw64_nt", "windows"),
            ("solaris", "solaris"),
            ("illumos", "illumos"),
            ("android", "android"),
            ("wsl", "wsl"),
            ("linux", "linux"),
        ];

        for (raw, expected) in cases {
            assert_eq!(canonical_os_id(raw), expected, "canonical_os_id({raw:?})");
        }
    }

    #[test]
    fn unknown_ids_fall_back_to_a_family_hint_then_unknown() {
        assert_eq!(canonical_os_id("mycompany-ubuntu-24.04"), "ubuntu");
        assert_eq!(canonical_os_id("totally_custom_os"), UNKNOWN_OS);
        assert_eq!(canonical_os_id(""), UNKNOWN_OS);
        assert_eq!(canonical_os_id("   "), UNKNOWN_OS);
        assert_eq!(parse_os_id("ID=some-custom-distro\n", ""), UNKNOWN_OS);
    }

    #[test]
    fn parse_remote_os_probe_reads_markers() {
        let stdout = concat!(
            "---OSRELEASE---\n",
            "ID=nixos\n",
            "ID_LIKE=\"\"\n",
            "---UNAME---\n",
            "Linux nixos 6.12.0 x86_64\n",
        );
        assert_eq!(parse_remote_os_probe(stdout), "nixos");

        let uname_only = "---OSRELEASE---\n---UNAME---\nDarwin MacBook.local 23.5.0\n";
        assert_eq!(parse_remote_os_probe(uname_only), "macos");

        let empty = "---OSRELEASE---\n---UNAME---\n";
        assert_eq!(parse_remote_os_probe(empty), UNKNOWN_OS);
    }

    #[tokio::test]
    async fn local_detection_returns_a_known_key() {
        let detected = detect_local_os().await;
        assert!(!detected.is_empty());
        assert_ne!(
            detected, UNKNOWN_OS,
            "linux/macos/windows hosts must resolve"
        );
    }
}
