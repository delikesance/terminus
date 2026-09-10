//! Browse a WSL distro filesystem via Windows UNC `\\wsl$\<Distro>\` (#86).
//! Logical paths stay Unix-style (`/home/…`); OS access uses UNC on Windows.

use crate::error::{Error, Result};
use crate::models::SftpEntry;
use crate::sftp_path::{normalize_sftp_path, resolve_under_root};
use crate::wsl::{distro_from_host_id, is_wsl_host_id};
use std::path::PathBuf;
#[cfg(any(windows, test))]
use std::path::Path;

#[cfg(windows)]
use crate::local_fs;

/// `\\wsl$\Ubuntu` (no trailing slash).
pub fn unc_root(distro: &str) -> PathBuf {
    PathBuf::from(format!(r"\\wsl$\{distro}"))
}

/// Map a logical SFTP path (under `root`) to a Windows UNC path for `distro`.
pub fn logical_to_unc(distro: &str, root: &str, path: &str) -> Result<PathBuf> {
    let logical = resolve_under_root(root, path)?;
    unc_from_logical(distro, &logical)
}

fn unc_from_logical(distro: &str, logical: &str) -> Result<PathBuf> {
    let norm = normalize_sftp_path(logical)?;
    if norm == "/" || norm == "." {
        return Ok(unc_root(distro));
    }
    let rel = norm.trim_start_matches('/');
    // Build UNC with `\` separators explicitly — `PathBuf::push` uses `/` on Unix hosts
    // (unit tests) and can mangle the `\\wsl$\…` prefix.
    let mut s = format!(r"\\wsl$\{distro}");
    for part in rel.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(Error::SftpPathTraversal {
                path: logical.to_string(),
            });
        }
        if part.contains('\\') || part.contains(':') {
            return Err(Error::msg(format!("invalid path segment: {part}")));
        }
        s.push('\\');
        s.push_str(part);
    }
    Ok(PathBuf::from(s))
}

/// Distro name from a `wsl:` host id, or error.
pub fn require_distro(host_id: &str) -> Result<&str> {
    distro_from_host_id(host_id)
        .filter(|d| !d.is_empty())
        .ok_or_else(|| Error::msg("invalid WSL host id"))
}

pub fn is_wsl_files_host(host_id: &str) -> bool {
    is_wsl_host_id(host_id)
}

#[cfg(windows)]
fn local_to_sftp(entry: crate::models::LocalEntry, distro: &str) -> Result<SftpEntry> {
    let logical = unc_path_to_logical(distro, Path::new(&entry.path))?;
    Ok(SftpEntry {
        name: entry.name,
        path: logical,
        is_dir: entry.is_dir,
        size: entry.size,
        mtime: if entry.modified > 0 {
            Some(entry.modified / 1000)
        } else {
            None
        },
    })
}

#[cfg(any(windows, test))]
fn unc_path_to_logical(distro: &str, os_path: &Path) -> Result<String> {
    let root = unc_root(distro);
    let root_s = root.to_string_lossy();
    let path_s = os_path.to_string_lossy();
    let root_trim = root_s.trim_end_matches(['\\', '/']);
    let path_trim = path_s.trim_end_matches(['\\', '/']);
    if path_trim.eq_ignore_ascii_case(root_trim) {
        return Ok("/".into());
    }
    if path_s.len() > root_trim.len() {
        let (head, tail) = path_s.split_at(root_trim.len());
        if head.eq_ignore_ascii_case(root_trim)
            && tail.starts_with(['\\', '/'])
        {
            let rest = tail.trim_start_matches(['\\', '/']);
            if rest.is_empty() {
                return Ok("/".into());
            }
            return normalize_sftp_path(&format!("/{}", rest.replace('\\', "/")));
        }
    }
    Err(Error::msg(format!(
        "path {path_s} is outside WSL root {root_s}"
    )))
}

#[cfg(windows)]
pub fn list(distro: &str, root: &str, path: &str) -> Result<Vec<SftpEntry>> {
    let dir = logical_to_unc(distro, root, path)?;
    let entries = local_fs::local_list(&dir.to_string_lossy())?;
    entries
        .into_iter()
        .map(|e| local_to_sftp(e, distro))
        .collect()
}

#[cfg(not(windows))]
pub fn list(_distro: &str, _root: &str, _path: &str) -> Result<Vec<SftpEntry>> {
    Err(Error::msg("WSL files are only available on Windows"))
}

#[cfg(windows)]
pub async fn read(distro: &str, root: &str, path: &str) -> Result<Vec<u8>> {
    let file = logical_to_unc(distro, root, path)?;
    local_fs::local_read(&file.to_string_lossy()).await
}

#[cfg(not(windows))]
pub async fn read(_distro: &str, _root: &str, _path: &str) -> Result<Vec<u8>> {
    Err(Error::msg("WSL files are only available on Windows"))
}

#[cfg(windows)]
pub async fn write(distro: &str, root: &str, path: &str, data: &[u8]) -> Result<()> {
    let file = logical_to_unc(distro, root, path)?;
    local_fs::local_write(&file.to_string_lossy(), data).await
}

#[cfg(not(windows))]
pub async fn write(_distro: &str, _root: &str, _path: &str, _data: &[u8]) -> Result<()> {
    Err(Error::msg("WSL files are only available on Windows"))
}

#[cfg(windows)]
pub async fn mkdir(distro: &str, root: &str, path: &str) -> Result<()> {
    let dir = logical_to_unc(distro, root, path)?;
    local_fs::local_mkdir(&dir.to_string_lossy()).await
}

#[cfg(not(windows))]
pub async fn mkdir(_distro: &str, _root: &str, _path: &str) -> Result<()> {
    Err(Error::msg("WSL files are only available on Windows"))
}

#[cfg(windows)]
pub fn remove(distro: &str, root: &str, path: &str, is_dir: bool) -> Result<()> {
    let p = logical_to_unc(distro, root, path)?;
    local_fs::local_remove(&p.to_string_lossy(), is_dir)
}

#[cfg(not(windows))]
pub fn remove(_distro: &str, _root: &str, _path: &str, _is_dir: bool) -> Result<()> {
    Err(Error::msg("WSL files are only available on Windows"))
}

#[cfg(windows)]
pub fn rename(distro: &str, root: &str, from: &str, to: &str) -> Result<()> {
    let a = logical_to_unc(distro, root, from)?;
    let b = logical_to_unc(distro, root, to)?;
    local_fs::local_rename(&a.to_string_lossy(), &b.to_string_lossy())
}

#[cfg(not(windows))]
pub fn rename(_distro: &str, _root: &str, _from: &str, _to: &str) -> Result<()> {
    Err(Error::msg("WSL files are only available on Windows"))
}

#[cfg(windows)]
pub fn rmtree(distro: &str, root: &str, path: &str) -> Result<()> {
    remove(distro, root, path, true)
}

#[cfg(not(windows))]
pub fn rmtree(_distro: &str, _root: &str, _path: &str) -> Result<()> {
    Err(Error::msg("WSL files are only available on Windows"))
}

/// Resolve `.` / relative paths to an absolute Linux path (prefer `$HOME`).
pub fn realpath(distro: &str, path: &str) -> Result<String> {
    let query = if path.is_empty() { "." } else { path };
    if query == "." || query == "~" {
        return distro_home(distro);
    }
    if query.starts_with('/') {
        return normalize_sftp_path(query);
    }
    let home = distro_home(distro)?;
    normalize_sftp_path(&format!("{}/{}", home.trim_end_matches('/'), query))
}

#[cfg(windows)]
fn distro_home(distro: &str) -> Result<String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("wsl.exe")
        .args(["-d", distro, "-e", "printenv", "HOME"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| Error::msg(format!("WSL home: {e}")))?;
    let home = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if home.starts_with('/') {
        return Ok(home);
    }
    Ok("/".into())
}

#[cfg(not(windows))]
fn distro_home(_distro: &str) -> Result<String> {
    Ok("/".into())
}

/// Absolute UNC path for opening with the default Windows app.
pub fn open_os_path(distro: &str, root: &str, path: &str) -> Result<PathBuf> {
    logical_to_unc(distro, root, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unc_root_format() {
        assert_eq!(unc_root("Ubuntu"), PathBuf::from(r"\\wsl$\Ubuntu"));
    }

    #[test]
    fn logical_to_unc_joins_segments() {
        let p = logical_to_unc("Ubuntu", "/", "/home/nixos/.bashrc").unwrap();
        assert_eq!(p, PathBuf::from(r"\\wsl$\Ubuntu\home\nixos\.bashrc"));
    }

    #[test]
    fn logical_root_and_dot() {
        assert_eq!(logical_to_unc("Debian", "/", "/").unwrap(), unc_root("Debian"));
        assert_eq!(
            logical_to_unc("Debian", "/", ".").unwrap(),
            unc_root("Debian")
        );
    }

    #[test]
    fn rejects_drive_like_segment() {
        assert!(logical_to_unc("Ubuntu", "/", "/c:windows").is_err());
    }

    #[test]
    fn unc_path_to_logical_roundtrip() {
        let os = PathBuf::from(r"\\wsl$\Ubuntu\home\a\b");
        assert_eq!(unc_path_to_logical("Ubuntu", &os).unwrap(), "/home/a/b");
        assert_eq!(
            unc_path_to_logical("Ubuntu", &unc_root("Ubuntu")).unwrap(),
            "/"
        );
    }

    #[test]
    fn require_distro_from_host_id() {
        assert_eq!(require_distro("wsl:Ubuntu").unwrap(), "Ubuntu");
        assert!(require_distro("not-wsl").is_err());
    }
}
