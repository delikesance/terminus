use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_command(
    cmd: SftpCommand,
    left_conn: &mut Option<Arc<SftpConnection>>,
    right_conn: &mut Option<Arc<SftpConnection>>,
    edit_sessions: &mut Vec<EditSession>,
    next_edit_id: &mut u64,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
    transfers: &tokio::sync::mpsc::UnboundedSender<TransferJob>,
) -> bool {
    match cmd {
        SftpCommand::ResolveConflict { .. } | SftpCommand::CancelTransfer => {
            // Routed via `SftpWorker::send` onto the conflict channel.
            false
        }
        SftpCommand::Connect { side, opts } => {
            drop_edit_sessions_for_side(edit_sessions, side);
            *conn_mut(left_conn, right_conn, side) = None;
            match connect_sftp_for_host(&opts).await {
                Ok(conn) => {
                    *conn_mut(left_conn, right_conn, side) = Some(Arc::new(conn));
                    emit(events, wake, SftpEvent::Ready { side });
                }
                Err(err) => {
                    let message =
                        terminus_core::ProbeError::from_error(err).user_message();
                    emit(events, wake, SftpEvent::ConnectFailed { side, message });
                }
            }
            false
        }
        SftpCommand::Disconnect { side } => {
            drop_edit_sessions_for_side(edit_sessions, side);
            drop(conn_mut(left_conn, right_conn, side).take());
            debug!(?side, "sftp worker disconnected side");
            false
        }
        SftpCommand::ListLocal { side, path } => {
            emit_listed_local(events, wake, side, &path).await;
            false
        }
        SftpCommand::ListRemote { side, path } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            emit_listed_remote(events, wake, side, conn, &path).await;
            false
        }
        SftpCommand::MkdirLocal { side, path } => {
            match local_fs::create_dir_all(&path).await {
                Ok(()) => {
                    if let Some(parent) = path.parent() {
                        emit_listed_local(events, wake, side, parent).await;
                    }
                }
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::MkdirRemote { side, path } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            match conn.mkdir(&path).await {
                Ok(()) => {
                    let parent = parent_remote(&path);
                    emit_listed_remote(events, wake, side, conn, &parent).await;
                }
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::RemoveLocal {
            side,
            path,
            recursive,
        } => {
            let parent = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("/"));
            match local_fs::remove_local_path(&path, recursive).await {
                Ok(()) => emit_listed_local(events, wake, side, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::RemoveRemote { side, path } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            let parent = parent_remote(&path);
            match conn.remove(&path).await {
                Ok(()) => emit_listed_remote(events, wake, side, conn, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::RenameLocal { side, from, to } => {
            let parent = to
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("/"));
            match local_fs::rename_local(&from, &to).await {
                Ok(()) => emit_listed_local(events, wake, side, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::RenameRemote { side, from, to } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            let parent = parent_remote(&to);
            match conn.rename(&from, &to).await {
                Ok(()) => emit_listed_remote(events, wake, side, conn, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::Transfer {
            from_side,
            from_path,
            to_side,
            to_cwd,
            name,
        } => {
            queue_transfer(
                transfers,
                TransferJob {
                    folder: false,
                    from_side,
                    from_path,
                    to_side,
                    to_cwd,
                    name,
                    left: left_conn.clone(),
                    right: right_conn.clone(),
                },
            );
            false
        }
        SftpCommand::EditRemote {
            side,
            remote_path,
            name,
        } => {
            match start_edit_remote(
                left_conn,
                right_conn,
                side,
                &remote_path,
                &name,
                edit_sessions,
                next_edit_id,
                events,
                wake,
            )
            .await
            {
                Ok(()) => {}
                Err(err) => emit(events, wake, SftpEvent::Failed(err)),
            }
            false
        }
        SftpCommand::TransferFolder {
            from_side,
            from_path,
            to_side,
            to_cwd,
            name,
        } => {
            queue_transfer(
                transfers,
                TransferJob {
                    folder: true,
                    from_side,
                    from_path,
                    to_side,
                    to_cwd,
                    name,
                    left: left_conn.clone(),
                    right: right_conn.clone(),
                },
            );
            false
        }
        SftpCommand::RemoveRemoteRecursive { side, path } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            let parent = parent_remote(&path);
            let removed = run_with_retry(
                Operation::RecursiveRemove,
                RetryPolicy::DEFAULT,
                || async {
                    conn.remove_recursive(&path)
                        .await
                        .map_err(|e| e.to_string())
                },
            )
            .await;
            match removed {
                Ok(()) => emit_listed_remote(events, wake, side, conn, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err)),
            }
            false
        }
        SftpCommand::Close => {
            drop_all_edit_sessions(edit_sessions);
            drop(left_conn.take());
            drop(right_conn.take());
            emit(events, wake, SftpEvent::Closed);
            debug!("sftp worker closed");
            true
        }
    }
}
