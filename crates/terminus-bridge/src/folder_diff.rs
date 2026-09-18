//! Pure differential planner for SFTP folder downloads.
//!
//! Quick mode (default):
//! 1. Compare relative path + type + size + mtime.
//! 2. Matching size+mtime ⇒ equal (no content read).
//! 3. Same size but mtime diverges/unknown ⇒ content hash/compare.
//! 4. Missing local subdir ⇒ ZipSubtree without descending (early stop).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Sidecar written after a successful identical sync / empty differential plan.
pub const FOLDER_SYNC_CACHE_NAME: &str = ".terminus-folder-sync";

/// Cheap tree fingerprint: file count + total byte size (no content).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TreeSignature {
    pub file_count: u64,
    pub total_bytes: u64,
}

impl TreeSignature {
    pub fn from_sizes<'a, I>(sizes: I) -> Self
    where
        I: IntoIterator<Item = &'a u64>,
    {
        let mut file_count = 0u64;
        let mut total_bytes = 0u64;
        for sz in sizes {
            file_count += 1;
            total_bytes += *sz;
        }
        Self {
            file_count,
            total_bytes,
        }
    }
}

/// Cached folder sync state. A hit requires unchanged local signature and
/// unchanged remote metadata fingerprint (path + size + mtime) — no content re-hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSyncCache {
    pub signature: TreeSignature,
    pub remote_meta_hex: String,
    pub content_sha256_hex: String,
}

impl FolderSyncCache {
    pub fn matches_signature(&self, sig: TreeSignature) -> bool {
        self.signature == sig
    }

    pub fn matches_remote_meta(&self, remote_meta_hex: &str) -> bool {
        self.remote_meta_hex.eq_ignore_ascii_case(remote_meta_hex)
    }

    pub fn encode(&self) -> String {
        format!(
            "v2 {} {} {} {}\n",
            self.signature.file_count,
            self.signature.total_bytes,
            self.remote_meta_hex,
            self.content_sha256_hex
        )
    }

    pub fn decode(text: &str) -> Option<Self> {
        let line = text.lines().next()?.trim();
        let mut parts = line.split_whitespace();
        let ver = parts.next()?;
        if ver == "v2" {
            let file_count: u64 = parts.next()?.parse().ok()?;
            let total_bytes: u64 = parts.next()?.parse().ok()?;
            let remote_meta_hex = parts.next()?.to_lowercase();
            let content_sha256_hex = parts.next()?.to_lowercase();
            if remote_meta_hex.len() != 64
                || content_sha256_hex.len() != 64
                || !remote_meta_hex.chars().all(|c| c.is_ascii_hexdigit())
                || !content_sha256_hex.chars().all(|c| c.is_ascii_hexdigit())
                || parts.next().is_some()
            {
                return None;
            }
            return Some(Self {
                signature: TreeSignature {
                    file_count,
                    total_bytes,
                },
                remote_meta_hex,
                content_sha256_hex,
            });
        }
        // Legacy v1: treat as miss (force one content verify to upgrade).
        if ver == "v1" {
            return None;
        }
        None
    }
}

/// BLAKE3 digest of a local file (streaming).
pub fn hash_local_file(path: &Path) -> Result<[u8; 32], String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Hash an in-memory byte slice (remote stream / tests).
pub fn hash_bytes(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// Compare two files that both exist (quick mode).
///
/// 1. Size mismatch ⇒ different.
/// 2. Matching size + matching mtime ⇒ equal (no content read).
/// 3. Otherwise digests decide; missing digests ⇒ treat as different
///    (caller should hash content suspects first).
pub fn files_differ(
    remote_size: u64,
    local_size: u64,
    remote_mtime_ns: Option<i128>,
    local_mtime_ns: Option<i128>,
    remote_digest: Option<&[u8; 32]>,
    local_digest: Option<&[u8; 32]>,
) -> bool {
    if remote_size != local_size {
        return true;
    }
    if mtimes_prove_equal(remote_mtime_ns, local_mtime_ns) {
        return false;
    }
    match (remote_digest, local_digest) {
        (Some(r), Some(l)) => r != l,
        _ => true,
    }
}

fn mtimes_prove_equal(a: Option<i128>, b: Option<i128>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x == y)
}

/// True when both sides exist as files with the same size but metadata
/// cannot prove equality (mtime missing or diverges) — content check needed.
pub fn needs_content_hash(remote: &FileNode, local: &FileNode) -> bool {
    !remote.is_dir
        && !local.is_dir
        && remote.size == local.size
        && !mtimes_prove_equal(remote.mtime_ns, local.mtime_ns)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictAction {
    Overwrite,
    Keep,
}

/// Remembers an "apply to all" choice for the rest of a transfer job.
#[derive(Debug, Clone, Default)]
pub struct ConflictPolicy {
    apply: Option<ConflictAction>,
}

impl ConflictPolicy {
    pub fn pending_auto(&self) -> Option<ConflictAction> {
        self.apply
    }

    pub fn note(&mut self, action: ConflictAction, apply_to_all: bool) {
        if apply_to_all {
            self.apply = Some(action);
        }
    }

    pub fn resolve(
        &mut self,
        action: ConflictAction,
        apply_to_all: bool,
    ) -> ConflictAction {
        if let Some(auto) = self.apply {
            return auto;
        }
        self.note(action, apply_to_all);
        action
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileNode {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Nanoseconds since UNIX epoch when known (quick-mode short-circuit).
    pub mtime_ns: Option<i128>,
    /// Filled only for content suspects before planning.
    pub digest: Option<[u8; 32]>,
    pub children: Vec<FileNode>,
}

impl FileNode {
    pub fn file(name: impl Into<String>, size: u64, digest: [u8; 32]) -> Self {
        Self {
            name: name.into(),
            is_dir: false,
            size,
            mtime_ns: None,
            digest: Some(digest),
            children: Vec::new(),
        }
    }

    /// Metadata-only file node (no content hash yet).
    pub fn file_meta(name: impl Into<String>, size: u64) -> Self {
        Self {
            name: name.into(),
            is_dir: false,
            size,
            mtime_ns: None,
            digest: None,
            children: Vec::new(),
        }
    }

    /// Metadata file node with mtime for quick-mode equality.
    pub fn file_meta_mtime(
        name: impl Into<String>,
        size: u64,
        mtime_ns: Option<i128>,
    ) -> Self {
        Self {
            name: name.into(),
            is_dir: false,
            size,
            mtime_ns,
            digest: None,
            children: Vec::new(),
        }
    }

    pub fn dir(name: impl Into<String>, children: Vec<FileNode>) -> Self {
        Self {
            name: name.into(),
            is_dir: true,
            size: 0,
            mtime_ns: None,
            digest: None,
            children,
        }
    }

    fn child_map(&self) -> BTreeMap<&str, &FileNode> {
        self.children.iter().map(|c| (c.name.as_str(), c)).collect()
    }

    fn child_map_mut(&mut self) -> BTreeMap<String, usize> {
        self.children
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.clone(), i))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffAction {
    Skip { relative: String },
    DownloadFile { relative: String },
    ZipSubtree { relative: String },
    AskFile { relative: String },
    AskDir { relative: String },
}

/// Relative paths of file pairs that need a content check (same size, mtime
/// does not prove equality).
pub fn equal_size_pairs(remote: &FileNode, local: &FileNode) -> Vec<String> {
    let mut out = Vec::new();
    collect_content_suspects("", remote, local, &mut out);
    out
}

fn collect_content_suspects(
    prefix: &str,
    remote: &FileNode,
    local: &FileNode,
    out: &mut Vec<String>,
) {
    let local_map = local.child_map();
    for child in &remote.children {
        let rel = join_rel(prefix, &child.name);
        match local_map.get(child.name.as_str()) {
            Some(local_child) if child.is_dir && local_child.is_dir => {
                collect_content_suspects(&rel, child, local_child, out);
            }
            Some(local_child) if needs_content_hash(child, local_child) => {
                out.push(rel);
            }
            _ => {}
        }
    }
}

/// Set digests on equal-size file pairs using the provided hasher callbacks.
pub fn set_pair_digests(
    remote: &mut FileNode,
    local: &mut FileNode,
    relative: &str,
    remote_digest: [u8; 32],
    local_digest: [u8; 32],
) -> bool {
    let parts: Vec<&str> = relative.split('/').filter(|p| !p.is_empty()).collect();
    set_pair_digests_at(remote, local, &parts, remote_digest, local_digest)
}

fn set_pair_digests_at(
    remote: &mut FileNode,
    local: &mut FileNode,
    parts: &[&str],
    remote_digest: [u8; 32],
    local_digest: [u8; 32],
) -> bool {
    if parts.is_empty() {
        return false;
    }
    let name = parts[0];
    let r_map = remote.child_map_mut();
    let l_map = local.child_map_mut();
    let (Some(&ri), Some(&li)) = (r_map.get(name), l_map.get(name)) else {
        return false;
    };
    if parts.len() == 1 {
        remote.children[ri].digest = Some(remote_digest);
        local.children[li].digest = Some(local_digest);
        return true;
    }
    set_pair_digests_at(
        &mut remote.children[ri],
        &mut local.children[li],
        &parts[1..],
        remote_digest,
        local_digest,
    )
}

/// Plan differential actions for `remote` against an existing `local` tree.
pub fn plan_differential(remote: &FileNode, local: &FileNode) -> Vec<DiffAction> {
    let mut out = Vec::new();
    plan_dir("", remote, local, &mut out);
    out
}

fn join_rel(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn plan_dir(
    prefix: &str,
    remote: &FileNode,
    local: &FileNode,
    out: &mut Vec<DiffAction>,
) {
    let local_map = local.child_map();
    for child in &remote.children {
        let rel = join_rel(prefix, &child.name);
        match local_map.get(child.name.as_str()) {
            None if child.is_dir => out.push(DiffAction::ZipSubtree { relative: rel }),
            None => out.push(DiffAction::DownloadFile { relative: rel }),
            Some(local_child) if child.is_dir && local_child.is_dir => {
                if subtree_wholly_different(child, local_child) {
                    out.push(DiffAction::AskDir { relative: rel });
                } else {
                    plan_dir(&rel, child, local_child, out);
                }
            }
            Some(local_child) if !child.is_dir && !local_child.is_dir => {
                if files_differ(
                    child.size,
                    local_child.size,
                    child.mtime_ns,
                    local_child.mtime_ns,
                    child.digest.as_ref(),
                    local_child.digest.as_ref(),
                ) {
                    out.push(DiffAction::AskFile { relative: rel });
                } else {
                    out.push(DiffAction::Skip { relative: rel });
                }
            }
            Some(_) => {
                out.push(DiffAction::AskFile { relative: rel });
            }
        }
    }
}

fn subtree_wholly_different(remote: &FileNode, local: &FileNode) -> bool {
    let mut saw_pair_or_new = false;
    let mut saw_same = false;
    walk_compare(remote, local, &mut saw_pair_or_new, &mut saw_same);
    saw_pair_or_new && !saw_same
}

fn walk_compare(
    remote: &FileNode,
    local: &FileNode,
    saw_pair_or_new: &mut bool,
    saw_same: &mut bool,
) {
    let local_map = local.child_map();
    for child in &remote.children {
        match local_map.get(child.name.as_str()) {
            None => {
                *saw_pair_or_new = true;
            }
            Some(local_child) if child.is_dir && local_child.is_dir => {
                walk_compare(child, local_child, saw_pair_or_new, saw_same);
            }
            Some(local_child) if !child.is_dir && !local_child.is_dir => {
                *saw_pair_or_new = true;
                if !files_differ(
                    child.size,
                    local_child.size,
                    child.mtime_ns,
                    local_child.mtime_ns,
                    child.digest.as_ref(),
                    local_child.digest.as_ref(),
                ) {
                    *saw_same = true;
                }
            }
            Some(_) => {
                *saw_pair_or_new = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn tree_signature_sums_count_and_bytes() {
        let sizes = [10u64, 20, 5];
        assert_eq!(
            TreeSignature::from_sizes(sizes.iter()),
            TreeSignature {
                file_count: 3,
                total_bytes: 35,
            }
        );
    }

    #[test]
    fn folder_sync_cache_roundtrip_and_match() {
        let cache = FolderSyncCache {
            signature: TreeSignature {
                file_count: 2,
                total_bytes: 40,
            },
            remote_meta_hex: "b".repeat(64),
            content_sha256_hex: "a".repeat(64),
        };
        let decoded = FolderSyncCache::decode(&cache.encode()).unwrap();
        assert_eq!(decoded, cache);
        assert!(decoded.matches_signature(TreeSignature {
            file_count: 2,
            total_bytes: 40,
        }));
        assert!(!decoded.matches_signature(TreeSignature {
            file_count: 3,
            total_bytes: 40,
        }));
        assert!(decoded.matches_remote_meta(&"B".repeat(64)));
        assert!(FolderSyncCache::decode(
            "v1 2 40 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        )
        .is_none());
    }

    #[test]
    fn hash_local_file_same_content_same_digest() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        std::fs::write(&a, b"hello world").unwrap();
        std::fs::write(&b, b"hello world").unwrap();
        assert_eq!(hash_local_file(&a).unwrap(), hash_local_file(&b).unwrap());
    }

    #[test]
    fn hash_local_file_iso_size_mutation_differs() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        std::fs::write(&a, b"aaaa").unwrap();
        std::fs::write(&b, b"bbbb").unwrap();
        assert_eq!(std::fs::metadata(&a).unwrap().len(), 4);
        assert_eq!(std::fs::metadata(&b).unwrap().len(), 4);
        assert_ne!(hash_local_file(&a).unwrap(), hash_local_file(&b).unwrap());
    }

    #[test]
    fn hash_bytes_matches_file_hash() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.bin");
        let data = b"stream-me";
        std::fs::write(&p, data).unwrap();
        assert_eq!(hash_local_file(&p).unwrap(), hash_bytes(data));
    }

    #[test]
    fn size_mismatch_differs_without_digests() {
        assert!(files_differ(10, 11, None, None, None, None));
    }

    #[test]
    fn matching_size_and_mtime_skips_without_digests() {
        assert!(!files_differ(10, 10, Some(100), Some(100), None, None));
    }

    #[test]
    fn matching_size_diverging_mtime_needs_digest_or_differs() {
        assert!(files_differ(10, 10, Some(100), Some(200), None, None));
        let d = [1u8; 32];
        assert!(!files_differ(
            10,
            10,
            Some(100),
            Some(200),
            Some(&d),
            Some(&d)
        ));
    }

    #[test]
    fn equal_mtime_pairs_are_not_content_suspects() {
        let remote = FileNode::dir(
            "proj",
            vec![
                FileNode::file_meta_mtime("same.txt", 4, Some(1)),
                FileNode::file_meta_mtime("diff_mtime.txt", 4, Some(1)),
            ],
        );
        let local = FileNode::dir(
            "proj",
            vec![
                FileNode::file_meta_mtime("same.txt", 4, Some(1)),
                FileNode::file_meta_mtime("diff_mtime.txt", 4, Some(9)),
            ],
        );
        let pairs = equal_size_pairs(&remote, &local);
        assert_eq!(pairs, vec!["diff_mtime.txt".to_string()]);
    }

    #[test]
    fn plan_skips_when_mtime_matches_without_hash() {
        let remote = FileNode::dir(
            "proj",
            vec![FileNode::file_meta_mtime("a.txt", 4, Some(42))],
        );
        let local = FileNode::dir(
            "proj",
            vec![FileNode::file_meta_mtime("a.txt", 4, Some(42))],
        );
        let plan = plan_differential(&remote, &local);
        assert_eq!(
            plan,
            vec![DiffAction::Skip {
                relative: "a.txt".into()
            }]
        );
    }

    #[test]
    fn equal_size_pairs_lists_only_same_size_files() {
        let remote = FileNode::dir(
            "proj",
            vec![
                FileNode::file_meta("same_size.txt", 4),
                FileNode::file_meta("diff_size.txt", 10),
                FileNode::dir("sub", vec![FileNode::file_meta("nested.txt", 2)]),
            ],
        );
        let local = FileNode::dir(
            "proj",
            vec![
                FileNode::file_meta("same_size.txt", 4),
                FileNode::file_meta("diff_size.txt", 9),
                FileNode::dir("sub", vec![FileNode::file_meta("nested.txt", 2)]),
            ],
        );
        let pairs = equal_size_pairs(&remote, &local);
        assert_eq!(
            pairs,
            vec!["same_size.txt".to_string(), "sub/nested.txt".to_string()]
        );
    }

    #[test]
    fn plan_skips_identical_asks_on_digest_mismatch_same_size() {
        let d_a = hash_bytes(b"aaaa");
        let d_b = hash_bytes(b"bbbb");
        let remote = FileNode::dir(
            "proj",
            vec![
                FileNode::file("same.txt", 4, d_a),
                FileNode::file("diff.txt", 4, d_b),
            ],
        );
        let local = FileNode::dir(
            "proj",
            vec![
                FileNode::file("same.txt", 4, d_a),
                FileNode::file("diff.txt", 4, d_a),
            ],
        );
        let plan = plan_differential(&remote, &local);
        assert!(plan.contains(&DiffAction::Skip {
            relative: "same.txt".into()
        }));
        assert!(plan.contains(&DiffAction::AskFile {
            relative: "diff.txt".into()
        }));
    }

    #[test]
    fn plan_size_mismatch_asks_without_digest() {
        let remote = FileNode::dir("proj", vec![FileNode::file_meta("a.txt", 10)]);
        let local = FileNode::dir("proj", vec![FileNode::file_meta("a.txt", 11)]);
        let plan = plan_differential(&remote, &local);
        assert_eq!(
            plan,
            vec![DiffAction::AskFile {
                relative: "a.txt".into()
            }]
        );
    }

    #[test]
    fn plan_new_file_downloads_without_ask() {
        let remote = FileNode::dir("proj", vec![FileNode::file_meta("new.txt", 1)]);
        let local = FileNode::dir("proj", vec![]);
        let plan = plan_differential(&remote, &local);
        assert_eq!(
            plan,
            vec![DiffAction::DownloadFile {
                relative: "new.txt".into()
            }]
        );
    }

    #[test]
    fn plan_new_subdir_zips() {
        let remote = FileNode::dir(
            "proj",
            vec![FileNode::dir(
                "fresh",
                vec![FileNode::file_meta("a.txt", 1)],
            )],
        );
        let local = FileNode::dir("proj", vec![]);
        let plan = plan_differential(&remote, &local);
        assert_eq!(
            plan,
            vec![DiffAction::ZipSubtree {
                relative: "fresh".into()
            }]
        );
    }

    #[test]
    fn plan_wholly_different_subdir_asks_once() {
        let d_a = hash_bytes(b"aaaa");
        let d_b = hash_bytes(b"bbbb");
        let remote = FileNode::dir(
            "proj",
            vec![FileNode::dir(
                "sub",
                vec![
                    FileNode::file("a.txt", 4, d_a),
                    FileNode::file("b.txt", 4, d_b),
                ],
            )],
        );
        let local = FileNode::dir(
            "proj",
            vec![FileNode::dir(
                "sub",
                vec![
                    FileNode::file("a.txt", 4, d_b),
                    FileNode::file("b.txt", 4, d_a),
                ],
            )],
        );
        let plan = plan_differential(&remote, &local);
        assert_eq!(
            plan,
            vec![DiffAction::AskDir {
                relative: "sub".into()
            }]
        );
    }

    #[test]
    fn plan_mixed_subdir_recurses() {
        let d_a = hash_bytes(b"aaaa");
        let d_b = hash_bytes(b"bbbb");
        let remote = FileNode::dir(
            "proj",
            vec![FileNode::dir(
                "sub",
                vec![
                    FileNode::file("same.txt", 4, d_a),
                    FileNode::file("diff.txt", 4, d_b),
                ],
            )],
        );
        let local = FileNode::dir(
            "proj",
            vec![FileNode::dir(
                "sub",
                vec![
                    FileNode::file("same.txt", 4, d_a),
                    FileNode::file("diff.txt", 4, d_a),
                ],
            )],
        );
        let plan = plan_differential(&remote, &local);
        assert!(plan.contains(&DiffAction::Skip {
            relative: "sub/same.txt".into()
        }));
        assert!(plan.contains(&DiffAction::AskFile {
            relative: "sub/diff.txt".into()
        }));
        assert!(!plan.iter().any(|a| matches!(a, DiffAction::AskDir { .. })));
    }

    #[test]
    fn conflict_policy_apply_to_all_locks_action() {
        let mut policy = ConflictPolicy::default();
        assert_eq!(
            policy.resolve(ConflictAction::Overwrite, true),
            ConflictAction::Overwrite
        );
        assert_eq!(policy.pending_auto(), Some(ConflictAction::Overwrite));
        assert_eq!(
            policy.resolve(ConflictAction::Keep, false),
            ConflictAction::Overwrite
        );
    }

    #[test]
    fn conflict_policy_without_apply_asks_each_time() {
        let mut policy = ConflictPolicy::default();
        assert_eq!(
            policy.resolve(ConflictAction::Keep, false),
            ConflictAction::Keep
        );
        assert_eq!(policy.pending_auto(), None);
        assert_eq!(
            policy.resolve(ConflictAction::Overwrite, false),
            ConflictAction::Overwrite
        );
    }

    #[test]
    fn size_shortcut_marks_different_without_digest_equality() {
        let d = hash_bytes(b"x");
        assert!(files_differ(10, 11, None, None, Some(&d), Some(&d)));
        assert!(!files_differ(10, 10, None, None, Some(&d), Some(&d)));
    }

    #[test]
    fn writing_temp_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("w.txt");
        let mut f = File::create(&p).unwrap();
        f.write_all(b"ok").unwrap();
        assert_eq!(hash_local_file(&p).unwrap(), hash_bytes(b"ok"));
    }
}
