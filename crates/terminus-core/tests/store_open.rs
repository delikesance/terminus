//! Regression coverage for `Store::open`.
//!
//! The constructor used to hand sqlx a `sqlite://<path>?foreign_keys=on` URL,
//! but sqlx 0.7's SQLite URL parser only understands the `mode` and `cache`
//! query parameters, so every `Store::open` call failed with "unknown query
//! parameter `foreign_keys`". Nothing called `open` in a test, so the whole
//! persistence layer was unusable at runtime while the suite stayed green.

use chrono::Utc;
use terminus_core::models::Host;
use terminus_core::Store;
use uuid::Uuid;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("terminus-core-store-{tag}-{}", Uuid::new_v4()))
}

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
        tags: vec!["prod".to_string()],
        notes: String::new(),
        os_id: None,
        sort_order: 0,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

#[tokio::test]
async fn open_creates_the_database_in_a_fresh_directory() {
    let dir = temp_dir("fresh");
    assert!(!dir.exists());

    let store = Store::open(dir.clone())
        .await
        .expect("open creates the data dir");
    assert!(store.list_hosts().await.expect("list").is_empty());
    assert!(dir.join("terminus.db").exists());

    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn hosts_survive_reopening_the_store() {
    let dir = temp_dir("reopen");

    let store = Store::open(dir.clone()).await.expect("first open");
    store
        .upsert_host(&sample_host("web-01"))
        .await
        .expect("upsert");
    drop(store);

    let reopened = Store::open(dir.clone()).await.expect("second open");
    let hosts = reopened.list_hosts().await.expect("list");
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].name, "web-01");
    assert_eq!(hosts[0].hostname, "web-01.example.com");
    assert_eq!(hosts[0].tags, vec!["prod".to_string()]);
    drop(reopened);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn deleting_a_host_hides_it_from_the_list() {
    let dir = temp_dir("delete");
    let store = Store::open(dir.clone()).await.expect("open");
    let host = sample_host("to-drop");

    store.upsert_host(&host).await.expect("upsert");
    assert_eq!(store.list_hosts().await.expect("list").len(), 1);

    store.delete_host(host.id).await.expect("delete");
    assert!(store.list_hosts().await.expect("list").is_empty());
    drop(store);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn host_password_credential_roundtrips_without_plaintext_on_host() {
    use terminus_core::{
        create_with_key, open_host_password, seal_host_password, OWNER_KIND_HOST,
    };

    let dir = temp_dir("vault-cred");
    let store = Store::open(dir.clone()).await.expect("open");
    let mut host = sample_host("secret-box");
    host.password = None;
    store.upsert_host(&host).await.expect("upsert host");

    let (_header, vault) = create_with_key("correct horse battery").expect("vault");
    let cred = seal_host_password(&vault, host.id, "s3cret!!").expect("seal");
    assert!(!cred.envelope.contains("s3cret!!"));
    store.upsert_credential(&cred).await.expect("upsert cred");

    let listed = store
        .list_credentials_for_owner(OWNER_KIND_HOST, host.id)
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);
    let got = store
        .get_credential(cred.id)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(
        open_host_password(&vault, host.id, &got).expect("open"),
        "s3cret!!"
    );

    let hosts = store.list_hosts().await.expect("hosts");
    assert!(hosts[0].password.is_none());
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Windows installs already have `settings.updated_at TEXT NOT NULL`. Writing
/// only `(key, value)` used to fail with SQLite 1299 (NOT NULL constraint).
#[tokio::test]
async fn set_setting_writes_updated_at_on_legacy_three_column_schema() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let dir = temp_dir("settings-legacy");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db_path = dir.join("terminus.db");

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open raw");
    sqlx::query(
        r#"
        CREATE TABLE settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("legacy schema");
    drop(pool);

    let store = Store::open(dir.clone()).await.expect("open migrates");
    store
        .set_setting("sync_config", r#"{"enabled":true}"#)
        .await
        .expect("set_setting must supply updated_at");
    let got = store
        .get_setting("sync_config")
        .await
        .expect("get")
        .expect("present");
    assert!(got.contains("enabled"));
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Fresh DBs created without `updated_at` must gain the column on open.
#[tokio::test]
async fn open_adds_updated_at_to_two_column_settings() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::Row;

    let dir = temp_dir("settings-two-col");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db_path = dir.join("terminus.db");

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open raw");
    sqlx::query("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("two-col schema");
    drop(pool);

    let store = Store::open(dir.clone()).await.expect("open");
    store
        .set_setting("vault_header", "{}")
        .await
        .expect("set after alter");
    drop(store);

    let options = SqliteConnectOptions::new().filename(&db_path);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("reopen raw");
    let cols: Vec<String> = sqlx::query("PRAGMA table_info(settings)")
        .fetch_all(&pool)
        .await
        .expect("pragma")
        .into_iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();
    assert!(cols.iter().any(|c| c == "updated_at"));
    drop(pool);
    let _ = std::fs::remove_dir_all(&dir);
}
