//! Command history persistence on its own thread (the UI never blocks on
//! SQLite). Capture arrives as `RioEvent::CommandSubmitted`; the History view
//! reads back per machine.

use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

use chrono::Utc;
use terminus_core::history::{
    machine_columns, recording_enabled_value, should_record, ENV_DISABLE, SETTING_KEY,
};
use terminus_core::models::HistoryEntry;
use terminus_core::Store;
use uuid::Uuid;

enum Job {
    Record(HistoryEntry),
    Load {
        row_id: String,
        limit: usize,
        reply: Sender<Vec<HistoryEntry>>,
    },
}

static WORKER: OnceLock<Sender<Job>> = OnceLock::new();

fn worker() -> &'static Sender<Job> {
    WORKER.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        std::thread::Builder::new()
            .name("terminus-history".into())
            .spawn(move || {
                let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
                else {
                    return;
                };
                let store = match rt.block_on(Store::open(crate::hosts::data_dir())) {
                    Ok(s) => s,
                    Err(err) => {
                        tracing::warn!("history store unavailable: {err}");
                        return;
                    }
                };
                // Persisted switch, checked once per record batch.
                while let Ok(job) = rx.recv() {
                    match job {
                        Job::Record(entry) => {
                            let off = rt
                                .block_on(store.get_setting(SETTING_KEY))
                                .ok()
                                .flatten();
                            if !recording_enabled_value(off.as_deref()) {
                                continue;
                            }
                            if let Err(err) = rt.block_on(store.insert_history(&entry)) {
                                tracing::warn!("history insert failed: {err}");
                            }
                        }
                        Job::Load {
                            row_id,
                            limit,
                            reply,
                        } => {
                            let rows = rt
                                .block_on(store.list_history_for_machine(&row_id, limit))
                                .unwrap_or_default();
                            let _ = reply.send(rows);
                        }
                    }
                }
            })
            .ok();
        tx
    })
}

/// `TERMINUS_HISTORY=0` (or `off`/`false`/`no`) turns recording off.
pub fn env_recording_enabled() -> bool {
    recording_enabled_value(std::env::var(ENV_DISABLE).ok().as_deref())
}

/// Record a submitted command for the machine `row_id` (`local`,
/// `wsl:<distro>` or a host uuid). Applies the privacy rules.
pub fn record(row_id: &str, command: &str, cwd: Option<String>) {
    if !env_recording_enabled() || !should_record(command) {
        return;
    }
    let (host_id, session_kind) = machine_columns(row_id);
    let entry = HistoryEntry {
        id: Uuid::new_v4(),
        command: command.to_string(),
        cwd,
        host_id,
        session_kind,
        created_at: Utc::now(),
    };
    let _ = worker().send(Job::Record(entry));
}

/// Newest-first entries of one machine; waits for the worker (short).
pub fn load_blocking(row_id: &str, limit: usize) -> Vec<HistoryEntry> {
    let (reply, rx) = channel();
    if worker()
        .send(Job::Load {
            row_id: row_id.to_string(),
            limit,
            reply,
        })
        .is_err()
    {
        return Vec::new();
    }
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .unwrap_or_default()
}

/// Entries as the view shows them.
pub fn to_items(
    entries: &[HistoryEntry],
    home: Option<&str>,
) -> Vec<terminus_ui::views::history::HistoryItem> {
    entries
        .iter()
        .map(|e| terminus_ui::views::history::HistoryItem {
            id: e.id.to_string(),
            command: e.command.clone(),
            cwd: e
                .cwd
                .as_deref()
                .map(|c| terminus_ui::views::history::display_cwd(c, home))
                .unwrap_or_default(),
            at: e.created_at.timestamp(),
        })
        .collect()
}

/// Env for a new session: adds the shell-integration variables for local
/// POSIX shells (when recording is on); everything else passes through.
#[cfg(not(target_os = "windows"))]
pub fn session_env(
    program: Option<&str>,
    env: Option<Vec<(String, String)>>,
) -> Option<Vec<(String, String)>> {
    use std::path::PathBuf;
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    if !env_recording_enabled()
        || !terminus_core::shell_integration::is_local_posix_shell(program)
    {
        return env;
    }
    let Some(dir) = DIR
        .get_or_init(|| terminus_core::shell_integration::install(&crate::hosts::data_dir()).ok())
        .as_ref()
    else {
        return env;
    };
    let orig = std::env::var("ZDOTDIR").ok();
    let mut merged = env.unwrap_or_default();
    merged.extend(terminus_core::shell_integration::local_env(dir, orig.as_deref()));
    Some(merged)
}
