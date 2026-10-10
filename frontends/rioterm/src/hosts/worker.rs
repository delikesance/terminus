use super::*;

/// Worker thread body: own runtime, own pool, sequential commands.
pub(super) fn worker(
    data_dir: PathBuf,
    commands: Receiver<Command>,
    events: Sender<HostEvent>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) {
    // OS probes finish on the runtime, after the command loop moved on.
    let probe_wake = wake.clone();
    let wake = move || {
        if let Some(wake) = wake.as_ref() {
            wake();
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            let _ = events.send(HostEvent::Failed(format!(
                "Could not start the host store runtime: {err}"
            )));
            wake();
            return;
        }
    };

    let store = match runtime.block_on(Store::open(data_dir.clone())) {
        Ok(store) => store,
        Err(err) => {
            let _ = events.send(HostEvent::Failed(format!(
                "Could not open the host database: {err}"
            )));
            wake();
            return;
        }
    };

    let mut sync_engine =
        terminus_core::SyncEngine::new(terminus_core::sync::SyncConfig::default());

    // Restore persisted sync config + detect vault header.
    if let Ok(Some(raw)) = runtime.block_on(store.get_setting(SYNC_CONFIG_SETTING)) {
        if let Ok(cfg) = terminus_core::sync::SyncConfig::from_json(&raw) {
            let url = cfg.remote_url.clone();
            sync_engine = terminus_core::SyncEngine::new(cfg);
            runtime.block_on(sync_engine.attach_local(store.pool().clone()));
            if let Some(uri) = url.filter(|u| !u.trim().is_empty()) {
                if let Err(err) = runtime.block_on(sync_engine.attach_remote_uri(&uri)) {
                    tracing::warn!(%err, "could not reopen sync remote on startup");
                }
            }
        }
    }
    let mut vault = restore_vault(&runtime, &store, &mut sync_engine, &events);
    runtime.block_on(sync_engine.attach_local(store.pool().clone()));
    // Pull what other devices changed while this one was closed.
    background_sync(&runtime, &store, &mut sync_engine, vault.is_some(), &events);
    wake();

    loop {
        // Idle ticks drive the periodic sync; commands still land at once.
        let interval = std::time::Duration::from_secs(
            sync_engine.config.interval_secs.max(MIN_SYNC_INTERVAL_SECS),
        );
        let command = match commands.recv_timeout(interval) {
            Ok(command) => command,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if background_sync(
                    &runtime,
                    &store,
                    &mut sync_engine,
                    vault.is_some(),
                    &events,
                ) {
                    wake();
                }
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match command {
            Command::Refresh => {
                let _ = events.send(list(&runtime, &store));
                let _ = events.send(list_groups(&runtime, &store));
                let _ = events.send(list_identities(&runtime, &store));
                let _ = events.send(list_snippets(&runtime, &store));
                let _ = events.send(sync_status_event(
                    &runtime,
                    &sync_engine,
                    vault.is_some(),
                ));
                let _ = events.send(HostEvent::Platform(discover_platform()));
                let _ = events.send(HostEvent::CollapsedGroupsLoaded(
                    load_collapsed_groups(&runtime, &store),
                ));
            }
            Command::Create(draft) => {
                // Test / skip-probe path: no SSH round-trip.
                let mut host = host_from_draft(&draft);
                match runtime.block_on(store.next_host_sort_order(host.group_id)) {
                    Ok(order) => host.sort_order = order,
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(format!(
                            "Could not save the host: {err}"
                        )));
                        continue;
                    }
                }
                let label = host.name.clone();
                match runtime.block_on(store.upsert_host(&host)) {
                    Ok(()) => {
                        let _ = events.send(HostEvent::Stored(format!("Added {label}")));
                        let _ = events.send(list(&runtime, &store));
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(format!(
                            "Could not save the host: {err}"
                        )));
                    }
                }
            }
            Command::ProbeAndCreate(draft) => {
                match probe_and_persist(&runtime, &store, &mut vault, draft) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(format!("Added {label}")));
                        let _ = events.send(list(&runtime, &store));
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(message) => {
                        let _ = events.send(HostEvent::Failed(message));
                    }
                }
            }
            Command::ProbeAndUpdate { id, draft } => {
                match probe_and_update(&runtime, &store, &mut vault, &id, draft) {
                    Ok(label) => {
                        let _ =
                            events.send(HostEvent::Stored(format!("Updated {label}")));
                        let _ = events.send(list(&runtime, &store));
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(message) => {
                        let _ = events.send(HostEvent::Failed(message));
                    }
                }
            }

            Command::CreateSnippet(item) => {
                let r = runtime.block_on(async {
                    let snippet = terminus_core::models::Snippet {
                        id: uuid::Uuid::new_v4(),
                        title: item.name,
                        content: item.cmd,
                        tags: Vec::new(),
                        shortcut: Some(item.desc),
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                        deleted_at: None,
                    };
                    store.upsert_snippet(&snippet).await
                });
                match r {
                    Ok(_) => {
                        let _ = events.send(HostEvent::Stored("Saved snippet".into()));
                        let _ = events.send(list_snippets(&runtime, &store));
                    }
                    Err(e) => {
                        let _ = events.send(HostEvent::Failed(e.to_string()));
                    }
                }
            }
            Command::DeleteSnippet(id_str) => {
                let r = runtime.block_on(async {
                    if let Ok(id_uuid) = uuid::Uuid::parse_str(&id_str) {
                        let all = store.list_snippets().await.unwrap_or_default();
                        if let Some(mut snip) = all.into_iter().find(|s| s.id == id_uuid)
                        {
                            let now = chrono::Utc::now();
                            snip.deleted_at = Some(now);
                            snip.updated_at = now;
                            return store.upsert_snippet(&snip).await;
                        }
                    }
                    Ok(())
                });
                match r {
                    Ok(_) => {
                        let _ = events.send(list_snippets(&runtime, &store));
                    }
                    Err(e) => {
                        let _ = events.send(HostEvent::Failed(e.to_string()));
                    }
                }
            }

            Command::CreateGroup(name) => {
                let now = Utc::now();
                let sort_order = match runtime.block_on(store.next_group_sort_order()) {
                    Ok(order) => order,
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(format!(
                            "Could not save the group: {err}"
                        )));
                        continue;
                    }
                };
                let group = Group {
                    id: Uuid::new_v4(),
                    name,
                    parent_id: None,
                    sort_order,
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                };
                match runtime.block_on(store.upsert_group(&group)) {
                    Ok(()) => {
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(format!(
                            "Could not save the group: {err}"
                        )));
                    }
                }
            }
            Command::SetHostGroup { host_id, group_id } => {
                match set_host_group(&runtime, &store, &host_id, group_id.as_deref()) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list(&runtime, &store));
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::ReorderHost {
                host_id,
                before_host_id,
                before_group_id,
            } => {
                match reorder_host(
                    &runtime,
                    &store,
                    &host_id,
                    before_host_id.as_deref(),
                    before_group_id.as_deref(),
                ) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list(&runtime, &store));
                        let _ = events.send(list_groups(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::ReorderGroup {
                group_id,
                before_group_id,
                before_host_id,
            } => {
                match reorder_group(
                    &runtime,
                    &store,
                    &group_id,
                    before_group_id.as_deref(),
                    before_host_id.as_deref(),
                ) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list_groups(&runtime, &store));
                        let _ = events.send(list(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::UnlockVault {
                passphrase,
                remember,
            } => match unlock_or_create_vault(&runtime, &store, &passphrase) {
                Ok(unlocked) => {
                    let shared = Arc::new(unlocked);
                    if let Err(err) = seal_plaintext_identities(&runtime, &store, &shared)
                    {
                        tracing::warn!("could not seal stored SSH keys: {err}");
                    }
                    runtime.block_on(sync_engine.attach_vault(Arc::clone(&shared)));
                    vault = Some(shared);
                    if remember {
                        if let Err(err) =
                            crate::vault_remember::remember_passphrase(&passphrase)
                        {
                            tracing::warn!("vault remember failed: {err}");
                        }
                    } else {
                        crate::vault_remember::forget_passphrase();
                    }
                    let _ = events.send(HostEvent::VaultStatus {
                        unlocked: true,
                        configured: true,
                        message: Some("Vault unlocked".into()),
                    });
                    let _ = events.send(sync_status_event(&runtime, &sync_engine, true));
                }
                Err(message) => {
                    let configured = runtime
                        .block_on(store.get_setting(terminus_core::VAULT_HEADER_SETTING))
                        .ok()
                        .flatten()
                        .is_some();
                    let _ = events.send(HostEvent::VaultStatus {
                        unlocked: false,
                        configured,
                        message: Some(message),
                    });
                }
            },
            Command::CreateSshKey {
                name,
                pem,
                passphrase,
            } => {
                match create_ssh_key(
                    &runtime,
                    &store,
                    vault.as_deref(),
                    &name,
                    pem.as_deref(),
                    passphrase.as_deref(),
                ) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list_identities(&runtime, &store));
                        let _ = events.send(list_snippets(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::DeleteSshKey { id } => match delete_ssh_key(&runtime, &store, &id) {
                Ok(label) => {
                    let _ = events.send(HostEvent::Stored(label));
                    let _ = events.send(list_identities(&runtime, &store));
                    let _ = events.send(list_snippets(&runtime, &store));
                }
                Err(err) => {
                    let _ = events.send(HostEvent::Failed(err));
                }
            },
            Command::DeleteHost { id } => match delete_host(&runtime, &store, &id) {
                Ok(label) => {
                    let _ = events.send(HostEvent::Stored(label));
                    let _ = events.send(list(&runtime, &store));
                }
                Err(err) => {
                    let _ = events.send(HostEvent::Failed(err));
                }
            },
            Command::DeleteGroup { id } => match delete_group(&runtime, &store, &id) {
                Ok(label) => {
                    let _ = events.send(HostEvent::Stored(label));
                    let _ = events.send(list(&runtime, &store));
                    let _ = events.send(list_groups(&runtime, &store));
                }
                Err(err) => {
                    let _ = events.send(HostEvent::Failed(err));
                }
            },
            Command::RenameHost { id, name } => {
                match rename_host(&runtime, &store, &id, &name) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::RenameGroup { id, name } => {
                match rename_group(&runtime, &store, &id, &name) {
                    Ok(label) => {
                        let _ = events.send(HostEvent::Stored(label));
                        let _ = events.send(list_groups(&runtime, &store));
                        let _ = events.send(list(&runtime, &store));
                    }
                    Err(err) => {
                        let _ = events.send(HostEvent::Failed(err));
                    }
                }
            }
            Command::TestSync { uri } => {
                let status = runtime.block_on(run_test_sync(
                    &store,
                    &mut sync_engine,
                    vault.as_ref().map(Arc::clone),
                    &uri,
                ));
                let _ = events.send(status);
                // A pull may have changed any list.
                send_all_lists(&runtime, &store, &events);
            }
            Command::ResolveHostPassword { id, reply } => {
                let result = resolve_host_password(&runtime, &store, vault.as_ref(), &id);
                let _ = reply.send(result);
            }
            Command::ResolveHostIdentity { id, reply } => {
                let result = resolve_host_identity(&runtime, &store, vault.as_ref(), &id);
                let _ = reply.send(result);
            }
            Command::ResolveSftpAuth { id, auth_method } => {
                let result = sftp_auth_for(
                    &auth_method,
                    || resolve_host_password(&runtime, &store, vault.as_ref(), &id),
                    || resolve_host_identity(&runtime, &store, vault.as_ref(), &id),
                );
                let _ = events.send(HostEvent::SftpAuth { id, result });
            }
            Command::DetectOs { id } => {
                match prepare_os_probe(&runtime, &store, vault.as_ref(), &id) {
                    Ok((host, opts)) => {
                        let store = store.clone();
                        let events = events.clone();
                        let wake = probe_wake.clone();
                        runtime.spawn(async move {
                            let probed = probe_and_store_os(&store, host, &opts).await;
                            let os_id = probed.unwrap_or_else(|err| {
                                tracing::debug!(host = %id, error = %err, "OS detect skipped");
                                None
                            });
                            let os_id = os_id.unwrap_or_default();
                            // Empty = unchanged or unknown; still clears in_flight.
                            let _ = events.send(HostEvent::OsDetected { id, os_id });
                            if let Some(wake) = wake.as_ref() {
                                wake();
                            }
                        });
                    }
                    Err(err) => {
                        tracing::debug!(host = %id, error = %err, "remote OS detect skipped");
                        let _ = events.send(HostEvent::OsDetected {
                            id,
                            os_id: String::new(),
                        });
                    }
                }
            }
            Command::SetGroupCollapsed {
                group_id,
                collapsed,
                reply,
            } => {
                let result =
                    set_collapsed_group_setting(&runtime, &store, &group_id, collapsed);
                let _ = reply.send(GroupCollapseOutcome {
                    group_id,
                    collapsed,
                    error: result.err(),
                });
            }
        }
        wake();
    }
}
