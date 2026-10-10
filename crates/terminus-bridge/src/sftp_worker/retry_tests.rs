use super::*;
use std::time::Duration;

const NO_DELAY: RetryPolicy = RetryPolicy {
    max_attempts: 3,
    delay: Duration::ZERO,
};

async fn calls_until_ok(op: Operation, failures: usize) -> (Result<(), String>, usize) {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let result = run_with_retry(op, NO_DELAY, || async {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        if n < failures {
            Err("connection reset".to_string())
        } else {
            Ok(())
        }
    })
    .await;
    (result, calls.into_inner())
}

#[tokio::test]
async fn idempotent_operations_are_replayed_until_they_succeed() {
    for op in [
        Operation::Download,
        Operation::Upload,
        Operation::RemoteFileCopy,
        Operation::ListRemote,
    ] {
        assert_eq!(calls_until_ok(op, 2).await, (Ok(()), 3), "{op:?}");
    }
}

#[tokio::test]
async fn replay_stops_at_the_attempt_limit_and_keeps_the_last_error() {
    let (result, calls) = calls_until_ok(Operation::Download, 10).await;
    assert_eq!(result, Err("connection reset".to_string()));
    assert_eq!(calls, NO_DELAY.max_attempts);
}

#[tokio::test]
async fn non_idempotent_operations_are_never_replayed() {
    for op in [
        Operation::LocalCopy,
        Operation::RecursiveRemove,
        Operation::RemoteArchiveCopy,
    ] {
        let (result, calls) = calls_until_ok(op, 1).await;
        assert!(result.is_err(), "{op:?}");
        assert_eq!(calls, 1, "{op:?}");
    }
}
