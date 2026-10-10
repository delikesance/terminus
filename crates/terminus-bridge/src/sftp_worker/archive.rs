use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteFamily {
    Unix,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PackEngine {
    NativeTar,
    NativeZip,
    /// Windows built-in `Compress-Archive` / `Expand-Archive`.
    PowerShell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RemoteArchiveTools {
    pub(super) tar: bool,
    pub(super) zip: bool,
    pub(super) unzip: bool,
    pub(super) powershell: bool,
}

impl RemoteArchiveTools {
    pub(super) fn none() -> Self {
        Self {
            tar: false,
            zip: false,
            unzip: false,
            powershell: false,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RemoteEnv {
    pub(super) tools: RemoteArchiveTools,
    pub(super) family: RemoteFamily,
    #[allow(dead_code)]
    pub(super) arch: String,
    pub(super) tmp: String,
}

pub(super) const NO_REMOTE_ARCHIVE_TOOLS: &str =
    "Remote has no tar, zip/unzip, or PowerShell Compress-Archive; cannot transfer folders.";

/// Pick archive format for **local → remote** uploads based on what the remote
/// can extract. Prefer tar.gz when `tar` is present (common on NixOS without unzip).
pub(super) fn choose_upload_archive_kind(
    tools: &RemoteArchiveTools,
    family: RemoteFamily,
) -> Result<ArchiveKind, String> {
    if tools.tar {
        return Ok(ArchiveKind::TarGz);
    }
    if tools.unzip {
        return Ok(ArchiveKind::Zip);
    }
    if family == RemoteFamily::Windows && tools.powershell {
        return Ok(ArchiveKind::Zip);
    }
    Err(NO_REMOTE_ARCHIVE_TOOLS.into())
}

/// Human-readable destination write failure (permission vs other).
pub(super) fn format_dest_write_error(path: &str, err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("permission")
        || lower.contains("denied")
        || lower.contains("permission_denied")
    {
        format!(
            "Permission denied writing to {path}. You may need elevated rights (e.g. root/sudo) on the remote host."
        )
    } else {
        format!("Cannot write to {path}: {err}")
    }
}

/// Verify the remote cwd is writable before packing a large archive.
pub(super) async fn ensure_remote_cwd_writable(
    conn: &SftpConnection,
    to_cwd: &str,
) -> Result<(), String> {
    let probe = join_remote(
        to_cwd,
        &format!(".terminus-write-probe-{}", std::process::id()),
    );
    match conn.write(&probe, b"x").await {
        Ok(()) => {
            let _ = conn.remove(&probe).await;
            Ok(())
        }
        Err(err) => Err(format_dest_write_error(to_cwd, &err.to_string())),
    }
}

/// Active pack/extract capability for one remote (native tools only).
pub(super) struct RemotePackSession {
    pub(super) env: RemoteEnv,
    pub(super) engine: PackEngine,
    pub(super) kind: ArchiveKind,
}

pub(super) fn parse_probe_stdout(stdout: &str) -> RemoteEnv {
    let mut tools = RemoteArchiveTools::none();
    let mut family = RemoteFamily::Unix;
    let mut arch = "x86_64".to_string();
    let mut tmp = "/tmp".to_string();
    for part in stdout.split_whitespace() {
        if let Some(v) = part.strip_prefix("tar=") {
            tools.tar = v == "1" || v.eq_ignore_ascii_case("true");
        } else if let Some(v) = part.strip_prefix("zip=") {
            tools.zip = v == "1" || v.eq_ignore_ascii_case("true");
        } else if let Some(v) = part.strip_prefix("unzip=") {
            tools.unzip = v == "1" || v.eq_ignore_ascii_case("true");
        } else if let Some(v) = part.strip_prefix("ps=") {
            tools.powershell = v == "1" || v.eq_ignore_ascii_case("true");
        } else if let Some(v) = part.strip_prefix("family=") {
            family = if v.eq_ignore_ascii_case("windows") {
                RemoteFamily::Windows
            } else {
                RemoteFamily::Unix
            };
        } else if let Some(v) = part.strip_prefix("arch=") {
            arch = normalize_arch(v);
        } else if let Some(v) = part.strip_prefix("tmp=") {
            if !v.is_empty() {
                tmp = v.to_string();
            }
        }
    }
    RemoteEnv {
        tools,
        family,
        arch,
        tmp,
    }
}

pub(super) fn normalize_arch(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "x86_64" | "amd64" | "x64" => "x86_64".into(),
        "aarch64" | "arm64" => "aarch64".into(),
        "i386" | "i686" | "x86" => "x86".into(),
        other => other.to_string(),
    }
}

pub(super) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub(super) fn remote_parent_base(path: &str) -> (String, String) {
    let path = path.trim_end_matches('/');
    match path.rsplit_once('/') {
        Some(("", base)) => ("/".into(), base.to_string()),
        Some((parent, base)) => (parent.to_string(), base.to_string()),
        None => (".".into(), path.to_string()),
    }
}

pub(super) fn archive_ext(kind: ArchiveKind) -> &'static str {
    match kind {
        ArchiveKind::TarGz => "tar.gz",
        ArchiveKind::Zip => "zip",
    }
}

pub(super) fn join_tmp(tmp: &str, name: &str) -> String {
    let tmp = tmp.trim_end_matches(['/', '\\']);
    if tmp.contains('\\') || tmp.chars().nth(1) == Some(':') {
        format!("{tmp}\\{name}")
    } else if tmp == "/" {
        format!("/{name}")
    } else {
        format!("{tmp}/{name}")
    }
}
