use super::*;

/// Floor for the automatic sync period, whatever the config says.
pub(super) const MIN_SYNC_INTERVAL_SECS: u64 = 15;

/// Re-send every list the panel shows (after a sync pulled rows).
pub(super) fn send_all_lists(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    events: &Sender<HostEvent>,
) {
    let _ = events.send(list(runtime, store));
    let _ = events.send(list_groups(runtime, store));
    let _ = events.send(list_identities(runtime, store));
    let _ = events.send(list_snippets(runtime, store));
}

/// Automatic sync (startup + periodic). Does nothing when sync is not set
/// up. Returns true when it sent events the UI should pick up.
pub(super) fn background_sync(
    runtime: &tokio::runtime::Runtime,
    store: &Store,
    engine: &mut terminus_core::SyncEngine,
    vault_unlocked: bool,
    events: &Sender<HostEvent>,
) -> bool {
    if !engine.config.enabled || !runtime.block_on(engine.is_configured()) {
        return false;
    }
    if runtime.block_on(engine.status()) == terminus_core::sync::SyncStatus::Error {
        // Retry after a failed run: Error → Syncing is a valid transition.
        runtime.block_on(engine.clear_error());
    }
    match runtime.block_on(engine.sync_now()) {
        Ok(report) => {
            engine.config.last_sync = report.finished_at;
            let _ = runtime.block_on(persist_sync_config(store, &engine.config));
            if report.pulled > 0 {
                send_all_lists(runtime, store, events);
            }
        }
        Err(err) => tracing::warn!(%err, "background sync failed"),
    }
    let _ = events.send(sync_status_event(runtime, engine, vault_unlocked));
    true
}

/// Snapshot the SyncEngine into a UI event.
pub(super) fn sync_status_event(
    runtime: &tokio::runtime::Runtime,
    engine: &terminus_core::SyncEngine,
    vault_unlocked: bool,
) -> HostEvent {
    runtime.block_on(async {
        let status = engine.status().await;
        let last_error = engine.last_error().await;
        let last_sync = engine.last_sync().await;
        let configured = engine.is_configured().await;
        let uri = engine.config.remote_url.clone().unwrap_or_default();

        let connected = configured
            && matches!(
                status,
                terminus_core::sync::SyncStatus::Idle
                    | terminus_core::sync::SyncStatus::Syncing
            );

        let (status_line, is_error) = if let Some(err) = last_error {
            (err, true)
        } else if let Some(ts) = last_sync {
            (
                format!("Last synced {}", ts.format("%Y-%m-%d %H:%M UTC")),
                false,
            )
        } else if !engine.config.has_remote() {
            ("Not configured".to_string(), false)
        } else if !configured {
            (
                format!("Remote set but not attached ({})", status.as_str()),
                false,
            )
        } else {
            (format!("Ready ({})", status.as_str()), false)
        };

        HostEvent::SyncStatus {
            uri,
            connected,
            vault_unlocked,
            status_line,
            is_error,
        }
    })
}

pub(super) async fn run_test_sync(
    store: &Store,
    engine: &mut terminus_core::SyncEngine,
    vault: Option<Arc<terminus_core::UnlockedVault>>,
    uri: &str,
) -> HostEvent {
    let uri = uri.trim().to_string();
    let vault_unlocked = vault.is_some();

    if let Some(v) = vault {
        engine.attach_vault(v).await;
    }

    if uri.is_empty() {
        // Test Sync with an empty field is a validation error — do not clear
        // a previously configured remote, and do not return a non-error
        // "Not configured" that silently dismisses the error banner.
        let connected = engine.is_configured().await
            && matches!(
                engine.status().await,
                terminus_core::sync::SyncStatus::Idle
                    | terminus_core::sync::SyncStatus::Syncing
            );
        return HostEvent::SyncStatus {
            uri: engine.config.remote_url.clone().unwrap_or_default(),
            connected,
            vault_unlocked,
            status_line: "Enter a connection URI before testing sync".into(),
            is_error: true,
        };
    }

    // Preserve device_id across reconfiguration.
    let device_id = engine.config.device_id.clone();
    engine.config = terminus_core::sync::SyncConfig {
        enabled: true,
        remote_url: Some(uri.clone()),
        device_id,
        interval_secs: engine.config.interval_secs,
        sync_secrets: engine.config.sync_secrets,
        last_sync: engine.config.last_sync,
    };

    if let Err(err) = persist_sync_config(store, &engine.config).await {
        return HostEvent::SyncStatus {
            uri: uri.clone(),
            connected: false,
            vault_unlocked,
            status_line: err,
            is_error: true,
        };
    }

    if let Err(err) = engine.attach_remote_uri(&uri).await {
        engine.clear_remote().await;
        let _ = engine.mark_error(err.to_string()).await;
        return HostEvent::SyncStatus {
            uri: uri.clone(),
            connected: false,
            vault_unlocked,
            status_line: err.to_string(),
            is_error: true,
        };
    }

    // Recover from a previous Error state so sync_now can run.
    if engine.status().await == terminus_core::sync::SyncStatus::Error {
        engine.clear_error().await;
        let _ = engine
            .transition_to(terminus_core::sync::SyncStatus::Idle)
            .await;
    }

    let (status_line, is_error) = match engine.sync_now().await {
        Ok(report) => {
            engine.config.last_sync = report.finished_at;
            let _ = persist_sync_config(store, &engine.config).await;
            let summary = format!(
                "Sync ok — pushed {}, pulled {}",
                report.pushed, report.pulled
            );
            if report.errors.is_empty() {
                (summary, false)
            } else {
                (format!("{summary} · {}", report.errors.join("; ")), true)
            }
        }
        Err(err) => (err.to_string(), true),
    };

    let connected = engine.is_configured().await
        && matches!(
            engine.status().await,
            terminus_core::sync::SyncStatus::Idle
                | terminus_core::sync::SyncStatus::Syncing
        );

    HostEvent::SyncStatus {
        uri,
        connected,
        vault_unlocked,
        status_line,
        is_error,
    }
}

pub(super) async fn persist_sync_config(
    store: &Store,
    config: &terminus_core::sync::SyncConfig,
) -> Result<(), String> {
    let json = config.to_json().map_err(|e| e.to_string())?;
    store
        .set_setting(SYNC_CONFIG_SETTING, &json)
        .await
        .map_err(|e| e.to_string())
}
