use crate::error::{Error, Result};

/// Normalizes a remote path and rejects attempts to escape it.
///
/// Names are kept byte-for-byte: POSIX file names may start or end with
/// spaces and may contain `\\`, so neither is trimmed nor split in a `/` path.
/// A path without any `/` that uses `\\` is a Windows-style path (OpenSSH for
/// Windows accepts both separators) and is split on `\\`. Either way `..`
/// hidden behind a backslash (`..\\..\\etc`) is rejected as traversal.
pub fn normalize_remote_path(raw: &str) -> Result<String> {
    if raw.trim().is_empty() {
        return Err(Error::PathTraversalError(
            "remote path is empty".to_string(),
        ));
    }
    if raw.contains('\0') {
        return Err(Error::PathTraversalError(
            "remote path contains a NUL byte".to_string(),
        ));
    }

    let windows_style = !raw.contains('/') && raw.contains('\\');
    let path = if windows_style {
        std::borrow::Cow::Owned(raw.replace('\\', "/"))
    } else {
        std::borrow::Cow::Borrowed(raw)
    };
    let absolute = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                if segments.pop().is_none() {
                    return Err(Error::PathTraversalError(format!(
                        "`..` escapes the root of {raw:?}"
                    )));
                }
            }
            segment if segment.split('\\').any(|part| part == "..") => {
                return Err(Error::PathTraversalError(format!(
                    "backslash `..` traversal in {raw:?}"
                )));
            }
            segment => segments.push(segment),
        }
    }

    if segments.is_empty() {
        if absolute {
            return Ok("/".to_string());
        }
        return Err(Error::PathTraversalError(format!(
            "remote path {raw:?} resolves to nothing"
        )));
    }

    let mut normalized = String::new();
    if absolute {
        normalized.push('/');
    }
    normalized.push_str(&segments.join("/"));
    Ok(normalized)
}

/// True when `candidate` is `root` itself or lives below it. Both paths must
/// already be normalized with [`normalize_remote_path`].
pub fn is_within_root(root: &str, candidate: &str) -> bool {
    if root == "/" {
        return candidate.starts_with('/');
    }
    candidate == root || candidate.starts_with(&format!("{root}/"))
}

/// Resolves `raw` against an optional sandbox `root`.
///
/// * `root == None` — the path is only normalized (`..` still rejected).
/// * `root == Some("/srv")` — relative paths are joined to the root, absolute
///   paths must already live inside it.
pub fn sandbox_path(root: Option<&str>, raw: &str) -> Result<String> {
    let normalized = normalize_remote_path(raw)?;

    let Some(root) = root else {
        return Ok(normalized);
    };
    let root = normalize_remote_path(root)?;

    let candidate = if normalized.starts_with('/') {
        normalized
    } else {
        normalize_remote_path(&format!("{root}/{}", normalized.trim_start_matches('/')))?
    };

    if is_within_root(&root, &candidate) {
        Ok(candidate)
    } else {
        Err(Error::PathTraversalError(format!(
            "{raw:?} resolves to {candidate:?}, outside of the SFTP root {root:?}"
        )))
    }
}
