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
async fn snippet_host_scope_roundtrips() {
    let dir = temp_dir("snippet-scope");
    let store = Store::open(dir.clone()).await.expect("open");
    let host_id = Uuid::new_v4();
    let now = Utc::now();
    let snippet = |host_id| terminus_core::models::Snippet {
        id: Uuid::new_v4(),
        title: "t".into(),
        content: "c".into(),
        tags: Vec::new(),
        shortcut: None,
        host_id,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    store.upsert_snippet(&snippet(Some(host_id))).await.unwrap();
    store.upsert_snippet(&snippet(None)).await.unwrap();
    let mut scopes: Vec<_> = store
        .list_snippets()
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.host_id)
        .collect();
    scopes.sort();
    assert_eq!(scopes, [None, Some(host_id)]);
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

fn history(
    command: &str,
    host: Option<Uuid>,
    kind: &str,
    age_secs: i64,
) -> terminus_core::models::HistoryEntry {
    terminus_core::models::HistoryEntry {
        id: Uuid::new_v4(),
        command: command.to_string(),
        cwd: Some("/srv".to_string()),
        host_id: host,
        session_kind: kind.to_string(),
        created_at: Utc::now() - chrono::Duration::seconds(age_secs),
    }
}

#[tokio::test]
async fn history_is_listed_per_machine_newest_first() {
    let dir = temp_dir("history");
    let store = Store::open(dir).await.expect("open");
    let ssh = Uuid::new_v4();
    store
        .insert_history(&history("old", None, "local", 30))
        .await
        .unwrap();
    store
        .insert_history(&history("new", None, "local", 5))
        .await
        .unwrap();
    store
        .insert_history(&history("remote", Some(ssh), "ssh", 10))
        .await
        .unwrap();
    store
        .insert_history(&history("distro", None, "wsl:Ubuntu", 10))
        .await
        .unwrap();

    let local = store.list_history_for_machine("local", 10).await.unwrap();
    assert_eq!(
        local.iter().map(|e| e.command.as_str()).collect::<Vec<_>>(),
        ["new", "old"]
    );
    let remote = store
        .list_history_for_machine(&ssh.to_string(), 10)
        .await
        .unwrap();
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].command, "remote");
    let wsl = store
        .list_history_for_machine("wsl:Ubuntu", 10)
        .await
        .unwrap();
    assert_eq!(wsl[0].command, "distro");
    let limited = store.list_history_for_machine("local", 1).await.unwrap();
    assert_eq!(limited.len(), 1);
}

/// Create `terminus.db` in `dir` with a `hosts` table from before the given
/// columns existed, holding one old row.
async fn seed_old_hosts_db(dir: &std::path::Path, columns: &str) {
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::ConnectOptions;

    std::fs::create_dir_all(dir).unwrap();
    let mut conn = SqliteConnectOptions::new()
        .filename(dir.join("terminus.db"))
        .create_if_missing(true)
        .connect()
        .await
        .unwrap();
    sqlx::query(&format!("CREATE TABLE hosts ({columns})"))
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO hosts (id, name, hostname, port, username, auth_method, tags, notes, created_at, updated_at) \
         VALUES (?, 'old', 'old.example.com', 22, 'root', 'password', '[]', '', '2026-09-14T00:00:00Z', '2026-09-14T00:00:00Z')",
    )
    .bind(Uuid::new_v4().to_string())
    .execute(&mut conn)
    .await
    .unwrap();
}

const HOSTS_WITHOUT_OS_ID: &str =
    "id TEXT PRIMARY KEY, name TEXT NOT NULL, hostname TEXT NOT NULL, \
    port INTEGER NOT NULL DEFAULT 22, username TEXT NOT NULL, \
    auth_method TEXT NOT NULL DEFAULT 'password', password TEXT, identity_id TEXT, \
    group_id TEXT, tags TEXT NOT NULL DEFAULT '[]', notes TEXT NOT NULL DEFAULT '', \
    sort_order INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, \
    updated_at TEXT NOT NULL, deleted_at TEXT";

#[tokio::test]
async fn open_adds_os_id_to_a_hosts_table_that_predates_it() {
    // "table hosts has no column named os_id" when saving a host.
    let dir = temp_dir("no-os-id");
    seed_old_hosts_db(&dir, HOSTS_WITHOUT_OS_ID).await;

    let store = Store::open(dir).await.expect("open upgrades the schema");
    store
        .upsert_host(&sample_host("new"))
        .await
        .expect("saving a host works on the upgraded table");

    let names: Vec<String> = store
        .list_hosts()
        .await
        .unwrap()
        .into_iter()
        .map(|h| h.name)
        .collect();
    assert!(
        names.contains(&"old".to_string()),
        "old rows survive: {names:?}"
    );
    assert!(names.contains(&"new".to_string()), "{names:?}");
}

#[tokio::test]
async fn open_adds_every_column_a_very_old_hosts_table_lacks() {
    let dir = temp_dir("bare-hosts");
    seed_old_hosts_db(
        &dir,
        "id TEXT PRIMARY KEY, name TEXT NOT NULL, hostname TEXT NOT NULL, \
         port INTEGER NOT NULL DEFAULT 22, username TEXT NOT NULL, \
         auth_method TEXT NOT NULL DEFAULT 'password', \
         tags TEXT NOT NULL DEFAULT '[]', notes TEXT NOT NULL DEFAULT '', \
         created_at TEXT NOT NULL, updated_at TEXT NOT NULL",
    )
    .await;

    let store = Store::open(dir).await.expect("open upgrades the schema");
    store
        .upsert_host(&sample_host("new"))
        .await
        .expect("saving a host works on the upgraded table");
    assert_eq!(store.list_hosts().await.unwrap().len(), 2);
}

#[tokio::test]
async fn open_uses_write_ahead_logging() {
    let store = Store::open(temp_dir("wal")).await.expect("open");

    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(store.pool())
        .await
        .expect("pragma");

    assert_eq!(mode, "wal");
}
