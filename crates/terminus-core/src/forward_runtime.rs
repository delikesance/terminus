//! In-memory tracking of live local port-forward tasks.
//!
//! Definitions live in SQLite (`PortForward`); only the running JoinHandles
//! are held here so Start/Stop can be exposed to the UI (#26).

use dashmap::DashMap;
use std::collections::HashSet;
use tokio::task::JoinHandle;

/// Runtime registry for started local forwards (not persisted, not synced).
#[derive(Default)]
pub struct ForwardRuntime {
    running: DashMap<String, JoinHandle<()>>,
}

impl ForwardRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a freshly started forward. Refuses if `id` is already running.
    pub fn insert(&self, id: impl Into<String>, handle: JoinHandle<()>) -> Result<(), String> {
        let id = id.into();
        if self.running.contains_key(&id) {
            handle.abort();
            return Err(format!("forward {id} is already running"));
        }
        self.running.insert(id, handle);
        Ok(())
    }

    /// Abort and remove a running forward. Returns true if it was running.
    pub fn stop(&self, id: &str) -> bool {
        if let Some((_, handle)) = self.running.remove(id) {
            handle.abort();
            true
        } else {
            false
        }
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.running.contains_key(id)
    }

    pub fn running_ids(&self) -> HashSet<String> {
        self.running.iter().map(|e| e.key().clone()).collect()
    }

    /// Stop before re-registering (edit/delete-while-running path).
    pub fn stop_if_running(&self, id: &str) {
        let _ = self.stop(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    fn sleeper() -> JoinHandle<()> {
        tokio::spawn(async {
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        })
    }

    #[test]
    fn ac5_ac6_insert_marks_running_and_stop_clears() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fwd = Arc::new(ForwardRuntime::new());
            let id = "fwd-ac5";
            assert!(!fwd.is_running(id));
            fwd.insert(id, sleeper()).expect("insert");
            assert!(fwd.is_running(id));
            assert!(fwd.running_ids().contains(id));
            assert!(fwd.stop(id));
            assert!(!fwd.is_running(id));
            assert!(!fwd.stop(id), "second stop is a no-op");
        });
    }

    #[test]
    fn ac5_duplicate_start_is_rejected() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fwd = ForwardRuntime::new();
            let id = "fwd-dup";
            fwd.insert(id, sleeper()).unwrap();
            let err = fwd.insert(id, sleeper()).expect_err("duplicate");
            assert!(err.contains("already running"), "{err}");
            assert!(fwd.is_running(id));
            fwd.stop(id);
        });
    }

    #[test]
    fn ac8_stop_if_running_is_idempotent() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let fwd = ForwardRuntime::new();
            fwd.stop_if_running("missing");
            fwd.insert("fwd-ac8", sleeper()).unwrap();
            fwd.stop_if_running("fwd-ac8");
            assert!(!fwd.is_running("fwd-ac8"));
            fwd.stop_if_running("fwd-ac8");
        });
    }
}
