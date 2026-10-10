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
    // `std::env::consts::OS` on Apple hosts is the token "macos", not "darwin".
    assert_eq!(parse_os_id("", "macos"), "macos");
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
