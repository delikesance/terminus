use super::*;

pub(super) async fn zip_remote_subtree_to_local(
    conn: &SftpConnection,
    remote_dir: &str,
    folder_name: &str,
    extract_cwd: &Path,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    let staging = std::env::temp_dir().join(format!(
        "terminus-sftp-diff-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let kind =
        try_remote_pack_to_local(conn, remote_dir, folder_name, &staging, events, wake)
            .await?;
    let archive = staging_with_ext(&staging, kind);
    let extract = {
        let archive = archive.clone();
        let dest = extract_cwd.to_path_buf();
        tokio::task::spawn_blocking(move || extract_local_archive(&archive, &dest, kind))
            .await
            .map_err(|e| e.to_string())?
    };
    let _ = tokio::fs::remove_file(&archive).await;
    extract
}

pub(super) fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

pub(super) fn file_tree_from_walk_meta(
    name: &str,
    meta: &terminus_walk::MetaMap,
    digests: &terminus_walk::DigestMap,
) -> FileNode {
    let mut root = FileNode::dir(name, Vec::new());
    for (rel, entry) in meta {
        insert_file_node_mtime(
            &mut root,
            rel,
            entry.size,
            entry.mtime_ns,
            digests.get(rel).copied(),
        );
    }
    fn sort_tree(n: &mut FileNode) {
        n.children.sort_by(|a, b| a.name.cmp(&b.name));
        for c in &mut n.children {
            if c.is_dir {
                sort_tree(c);
            }
        }
    }
    sort_tree(&mut root);
    root
}

pub(super) fn insert_file_node_mtime(
    root: &mut FileNode,
    relative: &str,
    size: u64,
    mtime_ns: Option<i128>,
    digest: Option<[u8; 32]>,
) {
    let parts: Vec<&str> = relative.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return;
    }
    let mut node = root;
    for (i, part) in parts.iter().enumerate() {
        let is_last = i + 1 == parts.len();
        if is_last {
            let mut file = FileNode::file_meta_mtime(*part, size, mtime_ns);
            file.digest = digest;
            node.children.push(file);
            return;
        }
        if let Some(idx) = node
            .children
            .iter()
            .position(|c| c.is_dir && c.name == *part)
        {
            node = &mut node.children[idx];
        } else {
            node.children.push(FileNode::dir(*part, Vec::new()));
            let idx = node.children.len() - 1;
            node = &mut node.children[idx];
        }
    }
}

/// Quick mode via uploaded `terminus-walk` (jwalk) on both sides.
///
/// Local jwalk and remote SSH-exec of the same binary run **concurrently**
/// (`tokio::join!`); results are compared in-process (no per-file SFTP reads).
pub(super) async fn plan_via_exec_manifest(
    conn: &SftpConnection,
    remote_root: &str,
    local_root: &Path,
    name: &str,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<Vec<DiffAction>, String> {
    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: "Preparing walker…".into(),
            done: 0,
            total: 0,
        },
    );
    let (walk_bin, remote_env) = walk_remote::ensure_remote_walk(conn).await?;

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Indexing {name}…"),
            done: 0,
            total: 0,
        },
    );
    // Parallel: remote SSH exec + local jwalk.
    let remote_fut = walk_remote::remote_walk_meta(conn, &walk_bin, remote_root);
    let local_fut = tokio::task::spawn_blocking({
        let p = local_root.to_path_buf();
        move || walk_remote::local_walk_meta(&p)
    });
    let (remote_meta, local_join) = tokio::join!(remote_fut, local_fut);
    let remote_meta = remote_meta?;
    let local_meta = local_join.map_err(|e| e.to_string())??;

    if remote_meta == local_meta {
        return Ok(Vec::new());
    }

    let suspects = walk_remote::content_suspects(&remote_meta, &local_meta);

    let mut remote_digests = terminus_walk::DigestMap::new();
    let mut local_digests = terminus_walk::DigestMap::new();

    if !suspects.is_empty() {
        emit(
            events,
            wake,
            SftpEvent::TransferProgress {
                label: format!("Hashing {} files…", suspects.len()),
                done: 0,
                total: suspects.len() as u64,
            },
        );
        let remote_fut = walk_remote::remote_walk_hash(
            conn,
            &walk_bin,
            remote_root,
            &suspects,
            &remote_env.tmp,
        );
        let local_fut = tokio::task::spawn_blocking({
            let p = local_root.to_path_buf();
            let s = suspects.clone();
            move || walk_remote::local_walk_hash(&p, &s)
        });
        let (rd, lj) = tokio::join!(remote_fut, local_fut);
        remote_digests = rd?;
        local_digests = lj.map_err(|e| e.to_string())??;
    }

    let remote_tree = file_tree_from_walk_meta(name, &remote_meta, &remote_digests);
    let local_tree = file_tree_from_walk_meta(name, &local_meta, &local_digests);
    Ok(plan_differential(&remote_tree, &local_tree))
}
