use super::*;

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
