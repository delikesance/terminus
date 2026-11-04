use super::*;

pub(super) fn queue_transfer(
    transfers: &tokio::sync::mpsc::UnboundedSender<TransferJob>,
    cancel: &CancelToken,
    mut job: TransferJob,
) {
    job.epoch = cancel.job_queued();
    if transfers.send(job).is_err() {
        cancel.job_done();
        warn!("sftp transfer task stopped");
    }
}

/// Runs queued copies one at a time (they share one conflict channel) so the
/// command loop keeps serving navigation meanwhile.
pub(super) async fn run_transfers(
    mut jobs: tokio::sync::mpsc::UnboundedReceiver<TransferJob>,
    events: Sender<SftpEvent>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    conflicts: Arc<ConflictInbox>,
    cancel: Arc<CancelToken>,
) {
    while let Some(job) = jobs.recv().await {
        // Cancelled while it waited: it never starts.
        if cancel.begin(job.epoch) {
            drain_stale_conflicts(&conflicts);
            run_transfer_job(&job, &events, &wake, &conflicts).await;
        }
        cancel.job_done();
    }
}

pub(super) async fn run_transfer_job(
    job: &TransferJob,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
    conflicts: &ConflictInbox,
) {
    let TransferJob {
        folder,
        from_side,
        from_path,
        to_side,
        to_cwd,
        name,
        left,
        right,
        ..
    } = job;
    let result = if *folder {
        transfer_folder(
            left, right, *from_side, from_path, *to_side, to_cwd, name, events, wake,
            conflicts,
        )
        .await
    } else {
        transfer(
            left, right, *from_side, from_path, *to_side, to_cwd, name, events, wake,
            conflicts,
        )
        .await
    };
    match result {
        Ok(()) => {}
        Err(err) if err == CANCELLED => emit(events, wake, SftpEvent::Cancelled),
        Err(err) => {
            emit(events, wake, SftpEvent::Failed(err));
            return;
        }
    }
    // Success or cancel: what landed before the stop is on the listing.
    if let Some(conn) = conn_ref(left, right, *to_side) {
        emit_listed_remote(events, wake, *to_side, conn, to_cwd).await;
    } else {
        emit_listed_local(events, wake, *to_side, Path::new(to_cwd)).await;
    }
}
