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
