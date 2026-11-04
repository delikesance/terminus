use super::tests::wait_for_event;
use super::*;

fn transfer(src: &Path, dst: &Path, name: &str) -> SftpCommand {
    SftpCommand::Transfer {
        from_side: SftpSide::Left,
        from_path: src.join(name).to_string_lossy().into_owned(),
        to_side: SftpSide::Right,
        to_cwd: dst.to_string_lossy().into_owned(),
        name: name.into(),
    }
}

#[test]
fn cancel_releases_the_prompt_and_drops_queued_transfers() {
    let dir = std::env::temp_dir()
        .join(format!("terminus-sftp-cancel-queue-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (src, dst) = (dir.join("src"), dir.join("dst"));
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(src.join("a.txt"), b"new").unwrap();
    std::fs::write(src.join("b.txt"), b"queued").unwrap();
    std::fs::write(src.join("c.txt"), b"later").unwrap();
    std::fs::write(dst.join("a.txt"), b"old").unwrap();

    let worker = SftpWorker::spawn(None);
    // `a` stops on a name conflict; `b` waits behind it in the queue.
    worker.send(transfer(&src, &dst, "a.txt"));
    worker.send(transfer(&src, &dst, "b.txt"));
    wait_for_event(&worker, |e| {
        matches!(e, SftpEvent::Conflict { .. }).then_some(())
    })
    .expect("a.txt waits on a conflict");

    worker.send(SftpCommand::CancelTransfer);
    let (mut cancelled, mut listed) = (false, false);
    for _ in 0..150 {
        for event in worker.drain() {
            cancelled |= matches!(event, SftpEvent::Cancelled);
            listed |= matches!(event, SftpEvent::Listed { .. });
        }
        if cancelled && listed {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(cancelled, "the running transfer reports the cancel");
    assert!(listed, "the destination is listed again");
    assert_eq!(std::fs::read(dst.join("a.txt")).unwrap(), b"old");
    assert!(!dst.join("b.txt").exists(), "the queued transfer never ran");

    // The queue is usable again.
    worker.send(transfer(&src, &dst, "c.txt"));
    wait_for_event(&worker, |e| {
        matches!(e, SftpEvent::Listed { .. }).then_some(())
    })
    .expect("a later transfer completes");
    assert_eq!(std::fs::read(dst.join("c.txt")).unwrap(), b"later");
    assert!(!dst.join("b.txt").exists());

    worker.send(SftpCommand::Close);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_with_nothing_to_cancel_leaves_no_stale_cancel() {
    let dir = std::env::temp_dir()
        .join(format!("terminus-sftp-cancel-idle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (src, dst) = (dir.join("src"), dir.join("dst"));
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(src.join("a.txt"), b"new").unwrap();
    std::fs::write(dst.join("a.txt"), b"old").unwrap();

    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::CancelTransfer);
    worker.send(transfer(&src, &dst, "a.txt"));
    // The conflict prompt must not have been auto-cancelled.
    let event = wait_for_event(&worker, |e| match e {
        SftpEvent::Conflict { .. } => Some("conflict"),
        SftpEvent::Cancelled | SftpEvent::Failed(_) => Some("stopped"),
        _ => None,
    });
    assert_eq!(event, Some("conflict"));

    worker.send(SftpCommand::CancelTransfer);
    worker.send(SftpCommand::Close);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_cancelled_transfer_is_not_retried() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let result: Result<(), String> = run_with_retry(
        Operation::Upload,
        RetryPolicy {
            max_attempts: 3,
            delay: Duration::ZERO,
        },
        || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(CANCELLED.to_string())
        },
    )
    .await;
    assert_eq!(result, Err(CANCELLED.to_string()));
    assert_eq!(calls.into_inner(), 1);
    assert_eq!(terminus_core::Error::Cancelled.to_string(), CANCELLED);
}
