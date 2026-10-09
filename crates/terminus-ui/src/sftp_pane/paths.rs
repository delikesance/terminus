/// Parent directory for a Unix-style remote path.
pub fn parent_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return "/".into();
    }
    match trimmed.rsplit_once('/') {
        Some(("", _)) => "/".into(),
        Some((parent, _)) if !parent.is_empty() => parent.to_string(),
        _ => "/".into(),
    }
}

/// Join `name` onto a remote directory path.
pub fn join_remote(cwd: &str, name: &str) -> String {
    if cwd == "/" {
        format!("/{name}")
    } else {
        format!("{cwd}/{name}")
    }
}

/// Split `cwd` into breadcrumb `(label, path)` pairs.
pub fn crumb_segments_for_cwd(cwd: &str, is_local: bool) -> Vec<(String, String)> {
    let cwd = if cwd.is_empty() {
        if is_local {
            return vec![(".".into(), ".".into())];
        }
        "/"
    } else {
        cwd
    };
    if !is_local && (cwd == "/" || cwd.is_empty()) {
        return vec![("/".into(), "/".into())];
    }
    let mut out = Vec::new();
    if is_local {
        #[cfg(windows)]
        {
            // Tests and WSL-style paths keep POSIX `/…` cwds; only use
            // `std::path` for drive-letter Windows paths (e.g. `C:\Users`).
            let looks_posix =
                cwd.starts_with('/') || (!cwd.contains('\\') && !cwd.contains(':'));
            if looks_posix {
                if cwd == "/" {
                    return vec![("/".into(), "/".into())];
                }
                out.push(("/".into(), "/".into()));
                let mut acc = String::new();
                for part in cwd
                    .trim_start_matches('/')
                    .split('/')
                    .filter(|p| !p.is_empty())
                {
                    acc.push('/');
                    acc.push_str(part);
                    out.push((part.to_string(), acc.clone()));
                }
                return out;
            }
            let path = std::path::Path::new(cwd);
            let mut acc = std::path::PathBuf::new();
            for (i, comp) in path.components().enumerate() {
                acc.push(comp.as_os_str());
                let label = if i == 0 {
                    acc.to_string_lossy().into_owned()
                } else {
                    comp.as_os_str().to_string_lossy().into_owned()
                };
                out.push((label, acc.to_string_lossy().into_owned()));
            }
            if out.is_empty() {
                out.push((cwd.to_string(), cwd.to_string()));
            }
            return out;
        }
        #[cfg(not(windows))]
        {
            if cwd == "/" {
                return vec![("/".into(), "/".into())];
            }
            out.push(("/".into(), "/".into()));
            let mut acc = String::new();
            for part in cwd
                .trim_start_matches('/')
                .split('/')
                .filter(|p| !p.is_empty())
            {
                acc.push('/');
                acc.push_str(part);
                out.push((part.to_string(), acc.clone()));
            }
            return out;
        }
    }
    // Remote POSIX-style
    if cwd == "/" {
        return vec![("/".into(), "/".into())];
    }
    out.push(("/".into(), "/".into()));
    let mut acc = String::new();
    for part in cwd
        .trim_start_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
    {
        acc.push('/');
        acc.push_str(part);
        out.push((part.to_string(), acc.clone()));
    }
    out
}
