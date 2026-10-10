use super::*;

/// Operations whose outcome is the same when run again; only these are replayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Operation {
    /// Lands in a temp sibling, then renames over the target.
    Download,
    /// Streams into a temp sibling, then renames over the target.
    Upload,
    /// Same temp-then-replace write on the destination.
    RemoteFileCopy,
    ListRemote,
    /// Writes straight onto the target: a replay could leave a mix.
    LocalCopy,
    RecursiveRemove,
    RemoteArchiveCopy,
}

impl Operation {
    pub(super) fn is_idempotent(self) -> bool {
        matches!(
            self,
            Self::Download | Self::Upload | Self::RemoteFileCopy | Self::ListRemote
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RetryPolicy {
    pub(super) max_attempts: usize,
    pub(super) delay: Duration,
}

impl RetryPolicy {
    pub(super) const DEFAULT: Self = Self {
        max_attempts: 3,
        delay: Duration::from_millis(500),
    };
}

pub(super) async fn run_with_retry<T, F, Fut>(
    op: Operation,
    policy: RetryPolicy,
    mut attempt: F,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let max_attempts = if op.is_idempotent() {
        policy.max_attempts
    } else {
        1
    };
    let mut tried = 1;
    loop {
        match attempt().await {
            Err(err) if tried < max_attempts => {
                warn!(?op, tried, error = %err, "sftp operation failed, retrying");
                tokio::time::sleep(policy.delay * tried as u32).await;
                tried += 1;
            }
            outcome => return outcome,
        }
    }
}
