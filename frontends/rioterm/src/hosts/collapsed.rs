use super::*;

/// Outcome of a [`Command::SetGroupCollapsed`] persistence operation.
///
/// Delivered to the [`HostPersistHandle`] that sent the command.
/// `error: None` means success; `Some(msg)` means the write failed and the
/// caller should revert local visual state and surface `msg` to the user.
#[derive(Debug)]
pub struct GroupCollapseOutcome {
    pub group_id: String,
    /// The state we attempted to persist (`true` = collapsed, `false` = expanded).
    pub collapsed: bool,
    pub error: Option<String>,
}

/// Application-owned persistence handle for group-collapse state.
///
/// Created once via [`HostRepository::persist_handle`] and held by the
/// `Application`. All group-collapse persistence commands flow through this
/// handle so the Application is the explicit owner, not an arbitrary route.
///
/// The worker that receives these commands is the same one owned by the
/// `HostRepository` from which this handle was created. All `Sender<Command>`
/// clones reach the same worker thread.
pub struct HostPersistHandle {
    commands: Sender<Command>,
    outcomes_tx: Sender<GroupCollapseOutcome>,
    outcomes_rx: Receiver<GroupCollapseOutcome>,
}

impl HostPersistHandle {
    pub(super) fn new(commands: Sender<Command>) -> Self {
        let (outcomes_tx, outcomes_rx) = channel();
        Self {
            commands,
            outcomes_tx,
            outcomes_rx,
        }
    }

    /// Persist `group_id` as explicitly collapsed (`true`) or expanded (`false`).
    ///
    /// Idempotent: sending the same value twice produces the same stored state.
    pub fn set_group_collapsed(&self, group_id: &str, collapsed: bool) {
        let _ = self.commands.send(Command::SetGroupCollapsed {
            group_id: group_id.to_string(),
            collapsed,
            reply: self.outcomes_tx.clone(),
        });
    }

    /// Non-blocking poll for the outcome of the most recent operation.
    ///
    /// Returns `None` if no outcome has arrived yet. The caller should
    /// poll this in its event loop (e.g. `about_to_wait`) and on `Some`:
    /// - `outcome.error == None` → success, retain local state
    /// - `outcome.error == Some(msg)` → failure, revert local state, show `msg`
    pub fn poll_outcome(&self) -> Option<GroupCollapseOutcome> {
        self.outcomes_rx.try_recv().ok()
    }
}

pub(super) fn load_collapsed_groups(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
) -> HashSet<String> {
    match runtime.block_on(store.get_setting(COLLAPSED_GROUPS_SETTING)) {
        Ok(Some(raw)) => decode_collapsed_groups(&raw),
        _ => HashSet::new(),
    }
}

/// Set `group_id` in the persisted collapsed-groups set to an explicit state.
///
/// Idempotent: inserting an already-present id or removing an absent id is a
/// no-op on the set, so the final stored state matches `collapsed` exactly.
/// Returns `Err(message)` if the write fails; the caller should revert local
/// visual state and surface the message.
pub(super) fn set_collapsed_group_setting(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    group_id: &str,
    collapsed: bool,
) -> Result<(), String> {
    let mut groups = load_collapsed_groups(runtime, store);
    if collapsed {
        groups.insert(group_id.to_string());
    } else {
        groups.remove(group_id);
    }
    let encoded = encode_collapsed_groups(&groups);
    runtime
        .block_on(store.set_setting(COLLAPSED_GROUPS_SETTING, &encoded))
        .map_err(|err| {
            tracing::warn!(
                group_id = %group_id,
                collapsed = %collapsed,
                error = %err,
                "could not persist group collapse state"
            );
            err.to_string()
        })
}

pub(super) fn encode_collapsed_groups(groups: &HashSet<String>) -> String {
    let mut ids: Vec<&str> = groups.iter().map(String::as_str).collect();
    ids.sort_unstable();
    ids.join("\n")
}

pub(super) fn decode_collapsed_groups(raw: &str) -> HashSet<String> {
    raw.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
