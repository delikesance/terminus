use super::ops::{is_no_such_file, parent_path};
use super::*;

#[test]
fn normalization_keeps_clean_absolute_paths() {
    assert_eq!(normalize_remote_path("/srv/data").unwrap(), "/srv/data");
    assert_eq!(normalize_remote_path("/srv//data/").unwrap(), "/srv/data");
    assert_eq!(normalize_remote_path("/srv/./data").unwrap(), "/srv/data");
    assert_eq!(normalize_remote_path("/").unwrap(), "/");
    assert_eq!(normalize_remote_path("data/logs").unwrap(), "data/logs");
}

#[test]
fn normalization_keeps_edge_whitespace_and_literal_backslashes() {
    // POSIX file names may end in spaces or contain `\`.
    assert_eq!(
        normalize_remote_path("/home/u/trail ").unwrap(),
        "/home/u/trail "
    );
    assert_eq!(
        normalize_remote_path("/home/u/ lead").unwrap(),
        "/home/u/ lead"
    );
    assert_eq!(
        normalize_remote_path("/home/u/a\\b.txt").unwrap(),
        "/home/u/a\\b.txt"
    );
    // Windows-style paths (no `/`) still use `\` as the separator.
    assert_eq!(normalize_remote_path("\\Users\\me").unwrap(), "/Users/me");
    // …and `..` hidden behind backslashes is still traversal.
    assert!(normalize_remote_path("/srv/..\\etc").is_err());
    assert!(normalize_remote_path("/srv/x\\..\\..\\etc").is_err());
}

#[test]
fn normalization_collapses_inner_parent_segments() {
    assert_eq!(normalize_remote_path("/srv/a/../b").unwrap(), "/srv/b");
    assert_eq!(normalize_remote_path("a/b/../../c").unwrap(), "c");
}

#[test]
fn traversal_attempts_are_rejected() {
    assert!(normalize_remote_path("/..").is_err());
    assert!(normalize_remote_path("/../etc/passwd").is_err());
    assert!(normalize_remote_path("../../etc/passwd").is_err());
    // Windows-style separators must not sneak past the check.
    assert!(normalize_remote_path("..\\..\\windows\\system32").is_err());
    assert!(normalize_remote_path("/srv/../../etc").is_err());
    assert!(normalize_remote_path("").is_err());
    assert!(normalize_remote_path("   ").is_err());
    assert!(normalize_remote_path("/srv/\0evil").is_err());
    // Relative paths that escape their own root.
    assert!(normalize_remote_path("a/../..").is_err());
}

#[test]
fn sandbox_root_contains_paths() {
    let root = Some("/srv");

    assert_eq!(sandbox_path(root, "logs").unwrap(), "/srv/logs");
    assert_eq!(sandbox_path(root, "/srv/logs").unwrap(), "/srv/logs");
    assert_eq!(sandbox_path(root, "/srv").unwrap(), "/srv");
    assert_eq!(sandbox_path(root, "a/../b").unwrap(), "/srv/b");

    assert!(sandbox_path(root, "/etc/passwd").is_err());
    assert!(sandbox_path(root, "/srv/../etc/passwd").is_err());
    assert!(sandbox_path(root, "../etc/passwd").is_err());
    assert!(sandbox_path(root, "/srvdata").is_err());

    // No root configured: normalization only.
    assert_eq!(sandbox_path(None, "/etc/passwd").unwrap(), "/etc/passwd");
    assert!(sandbox_path(None, "/../etc").is_err());
}

#[test]
fn root_slash_contains_everything() {
    assert!(is_within_root("/", "/etc/passwd"));
    assert!(!is_within_root("/", "relative"));
    assert!(is_within_root("/srv", "/srv"));
    assert!(is_within_root("/srv", "/srv/a/b"));
    assert!(!is_within_root("/srv", "/srv-old/a"));
}

#[test]
fn parent_paths_resolve_like_posix() {
    assert_eq!(parent_path("/srv/a").as_deref(), Some("/srv"));
    assert_eq!(parent_path("/srv").as_deref(), Some("/"));
    assert_eq!(parent_path("/").as_deref(), None);
    assert_eq!(parent_path("/a/").as_deref(), Some("/"));
}

#[test]
fn entry_extension_is_lowercased() {
    let entry = SftpEntry {
        name: "Report.TXT".to_string(),
        path: "/srv/Report.TXT".to_string(),
        is_dir: false,
        size: 12,
        modified: None,
    };
    assert_eq!(entry.extension().as_deref(), Some("txt"));
}

#[test]
fn only_no_such_file_counts_as_already_gone() {
    use russh_sftp::client::error::Error as SftpError;
    use russh_sftp::protocol::{Status, StatusCode};

    let status = |status_code| {
        SftpError::Status(Status {
            id: 0,
            status_code,
            error_message: String::new(),
            language_tag: String::new(),
        })
    };
    assert!(is_no_such_file(&status(StatusCode::NoSuchFile)));
    assert!(!is_no_such_file(&status(StatusCode::PermissionDenied)));
    assert!(!is_no_such_file(&SftpError::Timeout));
}

#[test]
fn remove_remote_recursive_desired() {
    // Live recursive delete is covered by bridge `e2e_delete_remote_dir_tree`.
    // Guard against the Phase-A stub error string returning to the impl body.
    let src = include_str!("ops.rs");
    let impl_start = src
        .find("pub async fn remove_recursive")
        .expect("remove_recursive API must remain public");
    let impl_body = &src[impl_start..];
    let impl_end = impl_body
        .find("\n    pub async fn mkdir")
        .unwrap_or(impl_body.len());
    let body = &impl_body[..impl_end];
    assert!(
        !body.contains("not implemented yet"),
        "remove_recursive must list+delete instead of returning a stub error"
    );
    assert!(
        body.contains("self.list(") && body.contains("self.remove("),
        "remove_recursive should walk entries then remove"
    );
}
