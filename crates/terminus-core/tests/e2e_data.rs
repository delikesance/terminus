//! E2E over the persistence features the UI drives: Store, Vault, Sync, keys.

use std::sync::Arc;

use chrono::Utc;
use terminus_core::models::Host;
use terminus_core::sync::{open_remote_sqlite_pool, SyncConfig};
use terminus_core::{Store, SyncEngine};
use uuid::Uuid;

fn sample_host(name: &str) -> Host {
    let now = Utc::now();
    Host {
        id: Uuid::new_v4(),
        name: name.to_string(),
        hostname: format!("{name}.example.com"),
        port: 22,
        username: "root".to_string(),
        auth_method: "password".to_string(),
        password: None,
        identity_id: None,
        group_id: None,
        tags: vec![],
        notes: String::new(),
        os_id: None,
        sort_order: 0,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

/// Settings → Remote SQL Sync → "Test Sync": the UI then shows
/// "Sync ok — pushed N, pulled M" and "Last synced …". Does data move?
#[tokio::test]
#[ignore = "BUG: SyncEngine push/pull are stubs but report success"]
async fn sync_now_actually_pushes_hosts_to_remote() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("local")).await.unwrap();
    store.upsert_host(&sample_host("prod-db")).await.unwrap();

    let remote = dir.path().join("remote.db");
    let uri = format!("sqlite:{}", remote.display());
    let engine = SyncEngine::new(SyncConfig::remote(&uri));
    engine.attach_remote_uri(&uri).await.unwrap();
    let report = engine.sync_now().await.expect("sync reports success");
    assert!(report.finished_at.is_some());

    let pool = open_remote_sqlite_pool(&uri).await.unwrap();
    let tables: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        report.pushed > 0 && !tables.is_empty(),
        "sync_now returned Ok (pushed {}, pulled {}) but remote has tables {:?}",
        report.pushed,
        report.pulled,
        tables
    );
}

/// Soft delete must bump `updated_at`, or last-writer-wins sync can never
/// order the tombstone after the live copy on another device.
#[tokio::test]
#[ignore = "BUG: delete_host does not bump updated_at"]
async fn delete_host_bumps_updated_at() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).await.unwrap();
    let mut h = sample_host("old");
    h.updated_at = Utc::now() - chrono::Duration::days(3);
    store.upsert_host(&h).await.unwrap();
    store.delete_host(h.id).await.unwrap();
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite:{}",
        dir.path().join("terminus.db").display()
    ))
    .await
    .unwrap();
    let (updated, deleted): (String, Option<String>) =
        sqlx::query_as("SELECT updated_at, deleted_at FROM hosts WHERE id = ?")
            .bind(h.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(deleted.is_some());
    assert_eq!(
        Some(updated.as_str()),
        deleted.as_deref(),
        "tombstone older than the row it deletes"
    );
}

/// One foreign-format row (another tool, a sync peer, a hand edit) must not
/// take down the whole host list.
#[tokio::test]
#[ignore = "BUG: one malformed row panics list_hosts"]
async fn list_hosts_survives_one_malformed_row() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).await.unwrap();
    store.upsert_host(&sample_host("good")).await.unwrap();
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite:{}",
        dir.path().join("terminus.db").display()
    ))
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO hosts (id,name,hostname,port,username,created_at,updated_at) \
         VALUES (?, 'bad', 'bad.example', 22, 'u', '2026-09-28 12:00:00', '2026-09-28 12:00:00')",
    )
    .bind(Uuid::new_v4().to_string())
    .execute(&pool)
    .await
    .unwrap();
    let store2 = store.clone();
    let r = tokio::spawn(async move { store2.list_hosts().await }).await;
    assert!(
        matches!(r, Ok(Ok(ref v)) if v.iter().any(|h| h.name == "good")),
        "list_hosts panicked or failed: {:?}",
        r.map(|x| x.map(|v| v.len()))
    );
}

/// Store file holds managed private keys; it must not be world-readable.
#[cfg(unix)]
#[tokio::test]
#[ignore = "BUG: terminus.db is created world-readable"]
async fn store_file_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("data")).await.unwrap();
    let ident = terminus_core::generate_ed25519_identity("k").unwrap();
    store.upsert_identity(&ident).await.unwrap();
    let mode = std::fs::metadata(dir.path().join("data/terminus.db"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode & 0o077, 0, "terminus.db mode is {mode:o}");
}

/// Managed keys should be sealed at rest like host passwords are.
#[tokio::test]
#[ignore = "BUG: managed private keys are stored as plaintext PEM"]
async fn managed_private_key_not_stored_in_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).await.unwrap();
    let ident = terminus_core::generate_ed25519_identity("k").unwrap();
    store.upsert_identity(&ident).await.unwrap();
    let raw = std::fs::read(dir.path().join("terminus.db")).unwrap();
    let needle = b"BEGIN OPENSSH PRIVATE KEY";
    assert!(
        !raw.windows(needle.len()).any(|w| w == needle),
        "private key PEM found in plaintext in terminus.db"
    );
}

/// Settings → Import PEM passes no passphrase (hosts.rs `create_ssh_key`).
#[test]
#[ignore = "BUG: Settings import cannot take a key passphrase"]
fn import_encrypted_pem_without_passphrase_field() {
    let Ok(path) = std::env::var("TERMINUS_E2E_ENC_KEY") else {
        eprintln!("skipped");
        return;
    };
    let pem = std::fs::read_to_string(path).unwrap();
    let r = terminus_core::import_openssh_identity("enc", &pem, None);
    assert!(
        r.is_ok(),
        "encrypted key cannot be imported from Settings: {:?}",
        r.err()
    );
}

/// A corrupted/foreign credential envelope must fail cleanly, not panic.
#[test]
#[ignore = "BUG: short nonce panics in XNonce::from_slice"]
fn vault_decrypt_with_bad_nonce_length_does_not_panic() {
    let (_h, vault) = terminus_core::create_with_key("pw").unwrap();
    let id = Uuid::new_v4();
    let mut cred = terminus_core::seal_host_password(&vault, id, "secret").unwrap();
    let mut env: terminus_core::SecretEnvelope =
        serde_json::from_str(&cred.envelope).unwrap();
    env.nonce = "AAAAAAAAAAAAAAAA".into(); // 12 bytes, not 24
    cred.envelope = serde_json::to_string(&env).unwrap();
    let vault = Arc::new(vault);
    let r =
        std::panic::catch_unwind(|| terminus_core::open_host_password(&vault, id, &cred));
    assert!(matches!(r, Ok(Err(_))), "decrypt panicked on a short nonce");
}

#[test]
fn vault_wrong_passphrase_is_rejected_and_right_one_opens() {
    let (header, vault) = terminus_core::create_with_key("correct").unwrap();
    let id = Uuid::new_v4();
    let cred = terminus_core::seal_host_password(&vault, id, "pw").unwrap();
    assert!(terminus_core::UnlockedVault::unlock("wrong", &header).is_err());
    let again = terminus_core::UnlockedVault::unlock("correct", &header).unwrap();
    assert_eq!(
        terminus_core::open_host_password(&again, id, &cred).unwrap(),
        "pw"
    );
}

#[tokio::test]
async fn groups_reorder_and_delete_keep_hosts() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).await.unwrap();
    let now = Utc::now();
    let g = terminus_core::models::Group {
        id: Uuid::new_v4(),
        name: "grp".into(),
        parent_id: None,
        sort_order: store.next_group_sort_order().await.unwrap(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    store.upsert_group(&g).await.unwrap();
    let a = sample_host("a");
    let mut b = sample_host("b");
    b.group_id = Some(g.id);
    store.upsert_host(&a).await.unwrap();
    store.upsert_host(&b).await.unwrap();
    store
        .reorder_root(true, g.id, Some(false), Some(a.id))
        .await
        .unwrap();
    let groups = store.list_groups().await.unwrap();
    let hosts = store.list_hosts().await.unwrap();
    let ga = groups.iter().find(|x| x.id == g.id).unwrap().sort_order;
    let ha = hosts.iter().find(|x| x.id == a.id).unwrap().sort_order;
    assert!(ga < ha, "group should now precede host a");
    store.delete_group(g.id).await.unwrap();
    let hosts = store.list_hosts().await.unwrap();
    let hb = hosts
        .iter()
        .find(|x| x.id == b.id)
        .expect("host survives group delete");
    assert!(hb.group_id.is_none());
}
