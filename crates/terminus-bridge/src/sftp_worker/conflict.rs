use super::*;

pub(super) fn drain_stale_conflicts(conflicts: &ConflictInbox) {
    while conflicts.try_recv().is_ok() {}
}

pub(super) fn wait_conflict_reply(
    conflicts: &ConflictInbox,
    expect_id: u64,
) -> Result<(ConflictAction, bool), String> {
    loop {
        match conflicts.recv_timeout(Duration::from_millis(250)) {
            Ok(ConflictReply::Resolve {
                id,
                action,
                apply_to_all,
            }) if id == expect_id => return Ok((action, apply_to_all)),
            Ok(ConflictReply::Resolve { .. }) => continue,
            Ok(ConflictReply::Cancel) => return Err("transfer cancelled".into()),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err("conflict channel closed".into());
            }
        }
    }
}

pub(super) fn join_rel_path(root: &Path, relative: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    for part in relative.split('/') {
        if !part.is_empty() && part != "." {
            p.push(part);
        }
    }
    p
}

pub(super) fn join_rel_remote(root: &str, relative: &str) -> String {
    let mut out = root.trim_end_matches('/').to_string();
    for part in relative.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if out.is_empty() || out == "/" {
            out = format!("/{part}");
        } else {
            out = format!("{out}/{part}");
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn ask_conflict(
    conflicts: &ConflictInbox,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
    policy: &mut ConflictPolicy,
    next_id: &mut u64,
    kind: ConflictKind,
    relative: &str,
    remote_path: &str,
    local_path: &str,
) -> Result<ConflictAction, String> {
    if let Some(auto) = policy.pending_auto() {
        return Ok(auto);
    }
    let id = *next_id;
    *next_id += 1;
    emit(
        events,
        wake,
        SftpEvent::Conflict {
            id,
            kind,
            relative_path: relative.to_string(),
            remote_path: remote_path.to_string(),
            local_path: local_path.to_string(),
        },
    );
    let (action, apply_to_all) =
        tokio::task::block_in_place(|| wait_conflict_reply(conflicts, id))?;
    Ok(policy.resolve(action, apply_to_all))
}
