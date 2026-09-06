//! Safe SFTP path normalization — blocks `..` escape outside the session root.

use crate::error::{Error, Result};

/// Normalize a remote SFTP path: collapse `.` / empty, apply `..`, reject escape.
///
/// Absolute paths stay absolute (`/…`). Relative paths stay relative; the empty
/// relative result is `.` (session cwd). A `..` that would leave the path's
/// own root (above `/` or above the relative stack) is a traversal error.
pub fn normalize_sftp_path(path: &str) -> Result<String> {
    if path.is_empty() {
        return Ok(".".into());
    }
    let absolute = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return Err(Error::SftpPathTraversal {
                        path: path.to_string(),
                    });
                }
            }
            other => stack.push(other),
        }
    }
    if absolute {
        Ok(if stack.is_empty() {
            "/".into()
        } else {
            format!("/{}", stack.join("/"))
        })
    } else if stack.is_empty() {
        Ok(".".into())
    } else {
        Ok(stack.join("/"))
    }
}

/// Resolve `path` under `root` (both remote SFTP paths). Rejects escape.
///
/// - Absolute `path` must still lie under `root`.
/// - Relative `path` is joined onto `root` then normalized.
pub fn resolve_under_root(root: &str, path: &str) -> Result<String> {
    let root_n = normalize_sftp_path(root)?;
    let candidate = if path.starts_with('/') {
        normalize_sftp_path(path)?
    } else if root_n == "." {
        normalize_sftp_path(path)?
    } else if root_n == "/" {
        normalize_sftp_path(&format!("/{path}"))?
    } else {
        normalize_sftp_path(&format!(
            "{}/{}",
            root_n.trim_end_matches('/'),
            path.trim_start_matches('/')
        ))?
    };
    if !is_under_root(&root_n, &candidate) {
        return Err(Error::SftpPathTraversal {
            path: path.to_string(),
        });
    }
    Ok(candidate)
}

/// Parent directory of a normalized path, or `None` at session root.
pub fn parent_path(path: &str) -> Option<String> {
    let Ok(norm) = normalize_sftp_path(path) else {
        return None;
    };
    if norm == "/" || norm == "." {
        return None;
    }
    if let Some(idx) = norm.rfind('/') {
        if idx == 0 {
            return Some("/".into());
        }
        return Some(norm[..idx].to_string());
    }
    Some(".".into())
}

fn is_under_root(root: &str, path: &str) -> bool {
    if root == "/" {
        return path.starts_with('/');
    }
    if root == "." {
        // Relative session: absolute paths are outside; `..` already rejected.
        return !path.starts_with('/');
    }
    path == root || path.starts_with(&format!("{root}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_blocks_relative_escape() {
        assert!(normalize_sftp_path("..").is_err());
        assert!(normalize_sftp_path("../etc").is_err());
        assert!(normalize_sftp_path("a/../../b").is_err());
        assert!(normalize_sftp_path("/..").is_err());
        assert!(normalize_sftp_path("/../../etc").is_err());
    }

    #[test]
    fn normalize_collapses_dot_segments() {
        assert_eq!(normalize_sftp_path(".").unwrap(), ".");
        assert_eq!(normalize_sftp_path("./a/./b").unwrap(), "a/b");
        assert_eq!(normalize_sftp_path("/a/./b/../c").unwrap(), "/a/c");
        assert_eq!(normalize_sftp_path("/").unwrap(), "/");
        assert_eq!(normalize_sftp_path("").unwrap(), ".");
    }

    #[test]
    fn resolve_under_root_blocks_escape() {
        assert!(resolve_under_root("/home/user", "/home/user/../../etc").is_err());
        assert!(resolve_under_root("/home/user", "../secret").is_err());
        assert!(resolve_under_root(".", "/etc/passwd").is_err());
        assert_eq!(
            resolve_under_root("/home/user", "docs/../docs/a").unwrap(),
            "/home/user/docs/a"
        );
        assert_eq!(resolve_under_root(".", "docs/x").unwrap(), "docs/x");
        assert_eq!(resolve_under_root("/", "/var/log").unwrap(), "/var/log");
    }

    #[test]
    fn parent_at_roots() {
        assert_eq!(parent_path("/"), None);
        assert_eq!(parent_path("."), None);
        assert_eq!(parent_path("/a/b").as_deref(), Some("/a"));
        assert_eq!(parent_path("/a").as_deref(), Some("/"));
        assert_eq!(parent_path("a/b").as_deref(), Some("a"));
        assert_eq!(parent_path("a").as_deref(), Some("."));
    }
}
