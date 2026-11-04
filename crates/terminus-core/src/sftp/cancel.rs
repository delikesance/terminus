//! Cooperative cancellation of queued and running SFTP transfers.
//!
//! One token is shared by a worker and every connection it opens. Transfers
//! are jobs: [`CancelToken::job_queued`] stamps a job with the current epoch,
//! [`CancelToken::cancel`] bumps the epoch (only while a job exists, so an
//! idle click leaves nothing armed), and the chunk loops ask
//! [`CancelToken::is_cancelled`]. A job stamped with an older epoch never
//! starts. Edits and other non-transfer reads are never affected: a job's end
//! re-arms the token to the current epoch.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct CancelToken {
    epoch: AtomicU64,
    /// Epoch the running job started under.
    armed: AtomicU64,
    /// Jobs queued or running.
    pending: AtomicUsize,
}

impl CancelToken {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A job entered the queue. Returns the epoch it is stamped with.
    pub fn job_queued(&self) -> u64 {
        self.pending.fetch_add(1, Ordering::SeqCst);
        self.epoch.load(Ordering::SeqCst)
    }

    /// The job left the queue (finished, failed or skipped).
    pub fn job_done(&self) {
        let _ = self
            .pending
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                Some(n.saturating_sub(1))
            });
        self.armed
            .store(self.epoch.load(Ordering::SeqCst), Ordering::SeqCst);
    }

    /// Start a job stamped `queued_epoch`. `false` when it was cancelled while
    /// it waited: the caller must skip it.
    pub fn begin(&self, queued_epoch: u64) -> bool {
        if self.epoch.load(Ordering::SeqCst) != queued_epoch {
            return false;
        }
        self.armed.store(queued_epoch, Ordering::SeqCst);
        true
    }

    /// Cancel every queued job and the running one. `false` when none exists.
    pub fn cancel(&self) -> bool {
        if self.pending.load(Ordering::SeqCst) == 0 {
            return false;
        }
        self.epoch.fetch_add(1, Ordering::SeqCst);
        true
    }

    pub fn is_cancelled(&self) -> bool {
        self.epoch.load(Ordering::SeqCst) != self.armed.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_stops_the_running_job_and_skips_queued_ones() {
        let t = CancelToken::new();
        let a = t.job_queued();
        let b = t.job_queued();
        assert!(t.begin(a));
        assert!(!t.is_cancelled());
        assert!(t.cancel());
        assert!(t.is_cancelled());
        t.job_done();
        assert!(!t.is_cancelled(), "a finished job re-arms the token");
        assert!(!t.begin(b), "a job queued before the cancel never starts");
        t.job_done();
        let c = t.job_queued();
        assert!(t.begin(c), "later jobs run normally");
        t.job_done();
    }

    #[test]
    fn cancelling_while_idle_does_nothing() {
        let t = CancelToken::new();
        assert!(!t.cancel());
        let a = t.job_queued();
        assert!(t.begin(a));
        assert!(!t.is_cancelled());
    }
}
