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
    if lower.contains("darwin") || lower.contains("macos") || lower.contains("mac os") {
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

mod canonical;
#[cfg(test)]
mod tests;

pub use canonical::canonical_os_id;
