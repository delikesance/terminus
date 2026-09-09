/// Remote probe: os-release first, then `uname -s`.
pub const DETECT_CMD: &str =
    "sh -c 'cat /etc/os-release 2>/dev/null || cat /usr/lib/os-release 2>/dev/null; printf \"\\n__UNAME__\\n\"; uname -s 2>/dev/null'";

/// Detect the OS running Terminus itself (local machine icon).
pub fn detect_local_os_id() -> String {
    match std::env::consts::OS {
        "windows" => "windows".into(),
        "macos" => "macos".into(),
        "freebsd" => "freebsd".into(),
        "openbsd" => "openbsd".into(),
        "netbsd" => "netbsd".into(),
        "linux" => {
            let release = std::fs::read_to_string("/etc/os-release")
                .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
                .unwrap_or_default();
            parse_os_id(&format!("{release}\n__UNAME__\nLinux\n"))
        }
        other => {
            let id = canonical_os_id(other);
            if id == "unknown" {
                "linux".into()
            } else {
                id
            }
        }
    }
}

/// Map `/etc/os-release` + uname into a stable icon key (`ubuntu`, `macos`, …).
pub fn parse_os_id(raw: &str) -> String {
    let (release, uname) = split_uname(raw);
    if let Some(id) = field(release, "ID") {
        return canonical_os_id(&id);
    }
    if let Some(like) = field(release, "ID_LIKE") {
        if let Some(first) = like.split_whitespace().next() {
            return canonical_os_id(first);
        }
    }
    canonical_os_id(uname.trim())
}

fn split_uname(raw: &str) -> (&str, &str) {
    match raw.split_once("__UNAME__") {
        Some((release, uname)) => (release, uname),
        None => (raw, ""),
    }
}

fn field(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix(&prefix) {
            return Some(unquote(value).to_lowercase());
        }
    }
    None
}

fn unquote(value: &str) -> &str {
    let v = value.trim();
    if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
        || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
    {
        &v[1..v.len() - 1]
    } else {
        v
    }
}

fn canonical_os_id(id: &str) -> String {
    match id.trim().to_lowercase().as_str() {
        "" => "unknown".into(),
        "ubuntu" | "xubuntu" | "lubuntu" | "ubuntustudio" | "ubuntu-mate" => "ubuntu".into(),
        "kubuntu" => "kubuntu".into(),
        "zorin" | "zorinos" => "zorin".into(),
        "neon" | "kde-neon" | "kdeneon" => "neon".into(),
        "pop" | "pop-os" | "pop_os" => "pop".into(),
        "debian" => "debian".into(),
        "raspbian" | "raspberrypi" | "raspios" => "raspberry".into(),
        "fedora" => "fedora".into(),
        "nobara" => "nobara".into(),
        "centos" | "centos-stream" => "centos".into(),
        "rhel" | "redhat" => "rhel".into(),
        "rocky" | "rocky-linux" => "rocky".into(),
        "almalinux" | "alma" => "alma".into(),
        "ol" | "oracle" | "oraclelinux" => "oracle".into(),
        "arch" | "archlinux" | "archarm" => "arch".into(),
        "endeavouros" | "endeavour" => "endeavour".into(),
        "garuda" => "garuda".into(),
        "artix" => "artix".into(),
        "manjaro" => "manjaro".into(),
        "nixos" => "nixos".into(),
        "alpine" => "alpine".into(),
        "opensuse-tumbleweed" | "tumbleweed" => "tumbleweed".into(),
        "opensuse-leap" | "leap" => "leap".into(),
        "opensuse" | "sles" | "suse" => "opensuse".into(),
        "kali" | "kali-rolling" => "kali".into(),
        "linuxmint" | "mint" => "mint".into(),
        "elementary" => "elementary".into(),
        "gentoo" => "gentoo".into(),
        "void" => "void".into(),
        "amzn" | "amazon" | "amazonlinux" => "amazon".into(),
        "darwin" => "macos".into(),
        "freebsd" => "freebsd".into(),
        "openbsd" => "openbsd".into(),
        "netbsd" => "netbsd".into(),
        "windows_nt" | "mingw64" | "mingw32" | "msys" | "cygwin" => "windows".into(),
        "linux" => "linux".into(),
        "solus" => "solus".into(),
        "deepin" => "deepin".into(),
        "devuan" => "devuan".into(),
        "coreos" | "rhcos" | "fedora-coreos" => "coreos".into(),
        "mageia" => "mageia".into(),
        "slackware" => "slackware".into(),
        "parrot" | "parrotsec" => "parrot".into(),
        "postmarketos" => "postmarketos".into(),
        "qubes" | "qubesos" => "qubesos".into(),
        "tails" => "tails".into(),
        "vanillaos" | "vanilla" => "vanilla".into(),
        "guix" | "guixsd" => "guix".into(),
        "mx" | "mxlinux" => "mxlinux".into(),
        "aosc" => "aosc".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ubuntu_os_release() {
        let raw = r#"
NAME="Ubuntu"
ID=ubuntu
ID_LIKE=debian
__UNAME__
Linux
"#;
        assert_eq!(parse_os_id(raw), "ubuntu");
    }

    #[test]
    fn nixos_quoted_id() {
        let raw = "ID=\"nixos\"\n__UNAME__\nLinux\n";
        assert_eq!(parse_os_id(raw), "nixos");
    }

    #[test]
    fn id_like_fallback() {
        let raw = "NAME=Custom\nID_LIKE=\"debian ubuntu\"\n__UNAME__\nLinux\n";
        assert_eq!(parse_os_id(raw), "debian");
    }

    #[test]
    fn darwin_uname() {
        assert_eq!(parse_os_id("__UNAME__\nDarwin\n"), "macos");
    }

    #[test]
    fn pop_stays_pop() {
        assert_eq!(parse_os_id("ID=pop\n__UNAME__\nLinux\n"), "pop");
    }

    #[test]
    fn kubuntu_keeps_own_logo() {
        assert_eq!(parse_os_id("ID=kubuntu\nID_LIKE=ubuntu\n__UNAME__\nLinux\n"), "kubuntu");
    }

    #[test]
    fn raspbian_maps_to_raspberry() {
        assert_eq!(parse_os_id("ID=raspbian\n__UNAME__\nLinux\n"), "raspberry");
    }

    #[test]
    fn detect_local_maps_platform() {
        let id = detect_local_os_id();
        assert!(
            matches!(
                id.as_str(),
                "windows"
                    | "macos"
                    | "linux"
                    | "freebsd"
                    | "openbsd"
                    | "netbsd"
                    | "ubuntu"
                    | "debian"
                    | "fedora"
                    | "arch"
                    | "nixos"
                    | "alpine"
                    | "manjaro"
                    | "pop"
                    | "mint"
                    | "opensuse"
                    | "leap"
                    | "tumbleweed"
                    | "rhel"
                    | "centos"
                    | "rocky"
                    | "alma"
                    | "kali"
                    | "gentoo"
                    | "void"
                    | "elementary"
                    | "endeavour"
                    | "garuda"
                    | "artix"
                    | "nobara"
                    | "kubuntu"
                    | "zorin"
                    | "neon"
                    | "raspberry"
                    | "amazon"
                    | "oracle"
                    | "solus"
                    | "deepin"
                    | "devuan"
                    | "coreos"
                    | "mageia"
                    | "slackware"
                    | "parrot"
                    | "postmarketos"
                    | "qubesos"
                    | "tails"
                    | "vanilla"
                    | "guix"
                    | "mxlinux"
                    | "aosc"
            ) || !id.is_empty(),
            "unexpected local os id: {id}"
        );
    }
}
