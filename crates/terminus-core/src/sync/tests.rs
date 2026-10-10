use super::*;
use crate::error::Error;
use std::sync::Arc;

#[test]
fn state_machine_accepts_valid_transitions() {
    use SyncStatus::*;
    assert!(Unconfigured.can_transition_to(Idle));
    assert!(Idle.can_transition_to(Syncing));
    assert!(Syncing.can_transition_to(Idle));
    assert!(Syncing.can_transition_to(Error));
    assert!(Idle.can_transition_to(Offline));
    assert!(Offline.can_transition_to(Syncing));
    assert!(Error.can_transition_to(Idle));
    // Idempotent transitions are allowed.
    assert!(Idle.can_transition_to(Idle));
}

#[test]
fn state_machine_rejects_invalid_transitions() {
    use SyncStatus::*;
    assert!(!Unconfigured.can_transition_to(Syncing));
    assert!(!Unconfigured.can_transition_to(Offline));
    assert!(!Syncing.can_transition_to(Unconfigured));
}

#[tokio::test]
async fn unconfigured_engine_starts_in_unconfigured() {
    let engine = SyncEngine::default();
    assert_eq!(engine.status().await, SyncStatus::Unconfigured);
    assert!(!engine.is_configured().await);
    assert!(engine.sync_now().await.is_err());
    assert_eq!(engine.status().await, SyncStatus::Unconfigured);
}

#[tokio::test]
async fn config_with_remote_starts_idle() {
    let engine = SyncEngine::new(SyncConfig::remote("sqlite://remote.db"));
    assert_eq!(engine.status().await, SyncStatus::Idle);

    // Without an attached pool a run must fail and move the machine to Error.
    let err = engine.sync_now().await.expect_err("no pool attached");
    assert!(matches!(err, Error::SyncError(_)));
    assert_eq!(engine.status().await, SyncStatus::Error);
    assert!(engine.last_error().await.is_some());

    // Recovery is a valid transition.
    engine.clear_error().await;
    engine
        .transition_to(SyncStatus::Idle)
        .await
        .expect("recover");
    assert_eq!(engine.status().await, SyncStatus::Idle);
}

#[tokio::test]
async fn vault_attachment_gates_secret_sync() {
    let engine = SyncEngine::default();
    assert!(!engine.can_sync_secrets().await);

    let (_header, vault) = crate::vault::create_with_key("passphrase").expect("vault");
    engine.attach_vault(Arc::new(vault)).await;
    assert!(engine.vault().await.is_some());
    // Config has `sync_secrets = false` by default, so still gated.
    assert!(!engine.can_sync_secrets().await);

    engine.detach_vault().await;
    assert!(engine.vault().await.is_none());
}

#[tokio::test]
async fn open_remote_rejects_postgres_and_opens_sqlite() {
    let err = open_remote_sqlite_pool("postgres://user:pass@localhost/db")
        .await
        .expect_err("postgres not wired");
    assert!(err.to_string().contains("PostgreSQL"));

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("remote.db");
    let uri = format!("sqlite:{}", path.display());
    let pool = open_remote_sqlite_pool(&uri).await.expect("sqlite open");
    let engine = SyncEngine::new(SyncConfig::remote(&uri));
    engine.set_remote(pool).await.expect("attach");
    let local = crate::store::Store::open(dir.path().join("local"))
        .await
        .expect("local store");
    engine.attach_local(local.pool().clone()).await;
    let report = engine.sync_now().await.expect("sync");
    assert!(report.finished_at.is_some());
}

#[test]
fn report_absorb_and_telemetry() {
    let mut report = SyncReport::started_now();
    report.absorb(SyncReport {
        pushed: 3,
        pulled: 1,
        skipped: 2,
        errors: vec!["one warning".to_string()],
        ..SyncReport::default()
    });
    let report = report.finish(42);
    assert_eq!(report.transferred(), 4);
    assert_eq!(report.skipped, 2);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.duration_ms, 42);
    assert!(report.finished_at.is_some());
}
