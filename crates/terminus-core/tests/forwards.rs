//! Port-forward persistence: upsert, list per host, soft delete.

use chrono::Utc;
use terminus_core::models::PortForward;
use terminus_core::Store;
use uuid::Uuid;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("terminus-core-fwd-{tag}-{}", Uuid::new_v4()))
}

fn forward(host_id: Uuid, name: &str, port: u16) -> PortForward {
    let now = Utc::now();
    PortForward {
        id: Uuid::new_v4(),
        host_id,
        kind: "local".into(),
        name: name.into(),
        bind_host: "127.0.0.1".into(),
        bind_port: port,
        dest_host: Some("localhost".into()),
        dest_port: Some(port),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

#[tokio::test]
async fn forwards_round_trip_per_host() {
    let store = Store::open(temp_dir("rt")).await.unwrap();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    store.upsert_forward(&forward(a, "db", 5432)).await.unwrap();
    store
        .upsert_forward(&forward(b, "web", 8080))
        .await
        .unwrap();
    let on_a = store.list_forwards(Some(a)).await.unwrap();
    assert_eq!(on_a.len(), 1);
    assert_eq!(on_a[0].name, "db");
    assert_eq!(store.list_forwards(None).await.unwrap().len(), 2);
}

#[tokio::test]
async fn delete_forward_is_a_soft_delete() {
    let store = Store::open(temp_dir("del")).await.unwrap();
    let host = Uuid::new_v4();
    let pf = forward(host, "db", 5432);
    store.upsert_forward(&pf).await.unwrap();
    store.delete_forward(pf.id).await.unwrap();
    assert!(store.list_forwards(Some(host)).await.unwrap().is_empty());
    let row: (Option<String>,) =
        sqlx::query_as("SELECT deleted_at FROM port_forwards WHERE id = ?")
            .bind(pf.id.to_string())
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert!(row.0.is_some(), "the tombstone stays for sync");
}

#[tokio::test]
async fn deleting_a_host_tombstones_its_forwards() {
    let store = Store::open(temp_dir("host")).await.unwrap();
    let host = Uuid::new_v4();
    store
        .upsert_forward(&forward(host, "db", 5432))
        .await
        .unwrap();
    store.delete_host(host).await.unwrap();
    assert!(store.list_forwards(Some(host)).await.unwrap().is_empty());
}
