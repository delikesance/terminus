//! Runtime registry for live port-forwarding tasks.
//!
//! The forwarding *plumbing* (ssh `direct-tcpip` channels) lives in the SSH
//! layer; this type only owns the lifecycle of the tokio task driving each
//! tunnel so the UI can start/stop/list them without touching the transport.
//!
//! `DashMap` is used instead of a `Mutex<HashMap>`: `is_running()` and
//! `running_ids()` are on the sidebar render path and must never block a
//! forwarding task that is being inserted or stopped.

use dashmap::DashMap;
use tokio::task::JoinHandle;

/// Keeps track of the tokio task behind every active tunnel, keyed by the
/// [`PortForward`](crate::models::PortForward) id (a UUID rendered as string).
#[derive(Default)]
pub struct ForwardRuntime {
    tasks: DashMap<String, JoinHandle<()>>,
}

impl ForwardRuntime {
    /// Creates an empty runtime.
    pub fn new() -> Self {
        Self {
            tasks: DashMap::new(),
        }
    }

    /// Registers `handle` under `id`, aborting a previously registered task with
    /// the same id. Returns the replaced handle, if any.
    pub fn insert(
        &self,
        id: impl Into<String>,
        handle: JoinHandle<()>,
    ) -> Option<JoinHandle<()>> {
        let id = id.into();
        let previous = self.tasks.insert(id, handle);
        if let Some(previous) = &previous {
            previous.abort();
        }
        previous
    }

    /// Takes a handle out of the registry without aborting it.
    pub fn take(&self, id: &str) -> Option<JoinHandle<()>> {
        self.tasks.remove(id).map(|(_, handle)| handle)
    }

    /// Aborts the task registered under `id`. Returns whether one was found.
    pub fn stop(&self, id: &str) -> bool {
        match self.tasks.remove(id) {
            Some((_, handle)) => {
                handle.abort();
                true
            }
            None => false,
        }
    }

    /// Whether a tunnel task for `id` exists *and* is still alive.
    pub fn is_running(&self, id: &str) -> bool {
        self.tasks
            .get(id)
            .is_some_and(|handle| !handle.is_finished())
    }

    /// Whether `id` is registered at all (even if its task has finished).
    pub fn contains(&self, id: &str) -> bool {
        self.tasks.contains_key(id)
    }

    /// Ids of every tunnel whose task has not finished, sorted for stable UI.
    pub fn running_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .tasks
            .iter()
            .filter(|entry| !entry.value().is_finished())
            .map(|entry| entry.key().clone())
            .collect();
        ids.sort();
        ids
    }

    /// Ids of every registered tunnel, including finished tasks that have not
    /// been reaped yet, sorted for stable UI.
    pub fn registered_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.tasks.iter().map(|e| e.key().clone()).collect();
        ids.sort();
        ids
    }

    /// Stops the tunnel registered under `id`, if it is running.
    pub fn stop_if_running(&self, id: &str) -> bool {
        if self.is_running(id) {
            self.stop(id)
        } else {
            false
        }
    }

    /// Aborts every registered task. Returns how many were aborted.
    pub fn stop_all(&self) -> usize {
        let aborted = self.tasks.len();
        for entry in self.tasks.iter() {
            entry.value().abort();
        }
        self.tasks.clear();
        aborted
    }

    /// Removes finished handles without aborting anything. Returns how many
    /// stale entries were reaped.
    pub fn reap_finished(&self) -> usize {
        let stale: Vec<String> = self
            .tasks
            .iter()
            .filter(|entry| entry.value().is_finished())
            .map(|entry| entry.key().clone())
            .collect();

        for id in &stale {
            // Only remove if still finished (another thread may have replaced it).
            let should_remove = self
                .tasks
                .get(id)
                .is_some_and(|handle| handle.is_finished());
            if should_remove {
                self.tasks.remove(id);
            }
        }
        stale.len()
    }

    /// Number of registered tunnels (running or not).
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether no tunnel is registered.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Number of tunnels whose task is still alive.
    pub fn running_count(&self) -> usize {
        self.running_ids().len()
    }
}

impl std::fmt::Debug for ForwardRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForwardRuntime")
            .field("registered", &self.len())
            .field("running", &self.running_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Spawns a task that stays alive until aborted.
    fn long_lived_task() -> JoinHandle<()> {
        tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        })
    }

    #[tokio::test]
    async fn insert_and_query() {
        let runtime = ForwardRuntime::new();
        assert!(runtime.is_empty());

        runtime.insert("forward-1", long_lived_task());
        runtime.insert("forward-2", long_lived_task());

        assert_eq!(runtime.len(), 2);
        assert!(runtime.is_running("forward-1"));
        assert!(runtime.contains("forward-2"));
        assert_eq!(runtime.running_ids(), vec!["forward-1", "forward-2"]);
        assert!(!runtime.is_running("nope"));
    }

    #[tokio::test]
    async fn insert_replaces_and_aborts_previous_task() {
        let runtime = ForwardRuntime::new();
        let first = long_lived_task();
        runtime.insert("forward-1", first);

        let second = long_lived_task();
        let replaced = runtime.insert("forward-1", second);
        assert!(replaced.is_some());
        assert_eq!(runtime.len(), 1);
        assert!(runtime.is_running("forward-1"));
    }

    #[tokio::test]
    async fn stop_removes_the_task() {
        let runtime = ForwardRuntime::new();
        runtime.insert("forward-1", long_lived_task());

        assert!(runtime.stop("forward-1"));
        assert!(!runtime.is_running("forward-1"));
        assert!(!runtime.contains("forward-1"));
        // Stopping twice reports "nothing was running".
        assert!(!runtime.stop("forward-1"));
    }

    #[tokio::test]
    async fn stop_if_running_only_stops_live_tasks() {
        let runtime = ForwardRuntime::new();
        assert!(!runtime.stop_if_running("unknown"));

        runtime.insert("forward-1", long_lived_task());
        assert!(runtime.stop_if_running("forward-1"));
        assert!(runtime.is_empty());
    }

    #[tokio::test]
    async fn stop_all_aborts_every_tunnel() {
        let runtime = ForwardRuntime::new();
        runtime.insert("a", long_lived_task());
        runtime.insert("b", long_lived_task());

        assert_eq!(runtime.stop_all(), 2);
        assert!(runtime.is_empty());
        assert!(runtime.running_ids().is_empty());
        assert_eq!(runtime.stop_all(), 0);
    }

    #[tokio::test]
    async fn finished_tasks_are_not_reported_as_running() {
        let runtime = ForwardRuntime::new();
        let handle = tokio::spawn(async {});
        runtime.insert("short", handle);

        // Let the task complete.
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(runtime.contains("short"));
        assert!(!runtime.is_running("short"));
        assert!(runtime.running_ids().is_empty());
        assert_eq!(runtime.reap_finished(), 1);
        assert!(runtime.is_empty());
    }
}
