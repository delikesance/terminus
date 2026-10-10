use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Wakes the UI once a second while a tunnel of the machine on screen runs,
/// so its uptime moves even in an otherwise idle window. One thread at most;
/// it ends when nothing runs or the controller is dropped.
pub(super) struct Ticker {
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    shared: Arc<TickShared>,
}

#[derive(Default)]
struct TickShared {
    active: AtomicBool,
    running: AtomicBool,
    dead: AtomicBool,
}

impl Ticker {
    pub(super) fn new(wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        Self {
            wake,
            shared: Arc::new(TickShared::default()),
        }
    }

    pub(super) fn set_active(&self, active: bool) {
        self.shared.active.store(active, Ordering::SeqCst);
        let Some(wake) = self.wake.clone() else {
            return;
        };
        if !active || self.shared.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.shared.clone();
        let spawned = std::thread::Builder::new()
            .name("terminus-tunnel-tick".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(1));
                if shared.dead.load(Ordering::SeqCst) {
                    break;
                }
                if !shared.active.load(Ordering::SeqCst) {
                    shared.running.store(false, Ordering::SeqCst);
                    // `set_active(true)` may have raced the store above.
                    if shared.active.load(Ordering::SeqCst)
                        && !shared.running.swap(true, Ordering::SeqCst)
                    {
                        continue;
                    }
                    break;
                }
                wake();
            });
        if spawned.is_err() {
            self.shared.running.store(false, Ordering::SeqCst);
        }
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        self.shared.dead.store(true, Ordering::SeqCst);
    }
}
