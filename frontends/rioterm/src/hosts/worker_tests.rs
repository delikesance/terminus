use super::test_support::*;
use super::*;
use std::time::Duration;

#[test]
fn worker_loads_creates_and_persists_hosts() {
    let dir = temp_dir("roundtrip");
    let mut repo = HostRepository::spawn(dir.clone(), None);

    // First answer is the (empty) initial list.
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        !repo.loading()
    }));
    assert!(repo.is_empty());
    assert_eq!(repo.error(), None);

    repo.create(&HostDraft {
        name: "  ".to_string(),
        hostname: "web-01.example.com".to_string(),
        username: "deploy".to_string(),
        port: "2222".to_string(),
        auth_method: "gssapi".to_string(),
        ..HostDraft::default()
    })
    .expect("valid draft");

    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.len() == 1
    }));
    assert_eq!(repo.error(), None);
    assert_eq!(repo.len(), 1);
    let row = &repo.hosts()[0];
    assert_eq!(row.name, "web-01.example.com");
    assert_eq!(row.hostname, "web-01.example.com");
    assert_eq!(row.username, "deploy");
    assert_eq!(row.port, 2222);
    assert_eq!(row.endpoint(), "deploy@web-01.example.com:2222");
    assert_eq!(
        repo.take_notice().as_deref(),
        Some("Added web-01.example.com")
    );

    // A second repository over the same directory proves the row is on
    // disk, not just cached in the first connection.
    drop(repo);
    let mut reopened = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(
        &mut reopened,
        Duration::from_secs(10),
        |repo| { repo.len() == 1 }
    ));
    assert_eq!(reopened.len(), 1);
    assert_eq!(reopened.hosts()[0].hostname, "web-01.example.com");
    drop(reopened);

    assert!(database_path(&dir).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn create_rejects_an_invalid_draft_without_touching_the_store() {
    let dir = temp_dir("invalid");
    let mut repo = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        !repo.loading()
    }));
    assert_eq!(repo.error(), None);

    // No hostname: the editor must keep itself open and show why.
    let message = repo.create(&HostDraft::default()).unwrap_err();
    assert!(message.contains("Hostname"));
    assert_eq!(repo.error(), Some("Hostname is required"));
    assert_eq!(repo.len(), 0);

    // Nothing was queued, so no answer — and no list — ever arrives.
    assert!(!drain_until(
        &mut repo,
        Duration::from_millis(100),
        |repo| { !repo.is_empty() }
    ));
    assert_eq!(repo.error(), Some("Hostname is required"));

    drop(repo);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worker_reports_a_failure_for_a_broken_database_path() {
    // A file where the data directory should be makes `Store::open` fail;
    // the UI must surface that instead of spinning on "loading".
    let dir = temp_dir("broken");
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::write(&dir, b"not a directory").unwrap();

    let mut repo = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.error().is_some()
    }));
    assert!(!repo.loading());
    assert!(repo.error().is_some());

    let _ = std::fs::remove_file(&dir);
}

#[test]
fn worker_test_sync_moves_hosts_between_devices() {
    let dir = temp_dir("sync-two");
    let remote = dir.join("remote.db");
    let uri = format!("sqlite:{}", remote.display());

    let mut a = HostRepository::spawn(dir.join("a"), None);
    assert!(drain_until(&mut a, Duration::from_secs(10), |r| !r.loading()));
    a.create(&HostDraft {
        name: "synced-box".to_string(),
        hostname: "box.example.com".to_string(),
        username: "deploy".to_string(),
        auth_method: "gssapi".to_string(),
        ..HostDraft::default()
    })
    .unwrap();
    assert!(drain_until(&mut a, Duration::from_secs(10), |r| r.len() == 1));
    a.test_sync(&uri);
    assert!(drain_until(&mut a, Duration::from_secs(10), |r| {
        r.sync_status_line().contains("pushed 1")
    }));

    let mut b = HostRepository::spawn(dir.join("b"), None);
    assert!(drain_until(&mut b, Duration::from_secs(10), |r| !r.loading()));
    b.test_sync(&uri);
    assert!(drain_until(&mut b, Duration::from_secs(10), |r| {
        r.hosts().iter().any(|h| h.name == "synced-box")
    }));
    assert!(
        b.sync_status_line().contains("pulled 1"),
        "{}",
        b.sync_status_line()
    );

    drop(a);
    drop(b);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worker_test_sync_opens_sqlite_and_rejects_postgres() {
    let dir = temp_dir("sync");
    let mut repo = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        !repo.loading()
    }));

    // Wait for the initial SyncStatus from Refresh.
    let _ = drain_until(&mut repo, Duration::from_secs(2), |repo| {
        !repo.sync_status_line().is_empty()
    });

    repo.test_sync("postgres://localhost/terminus");
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.sync_status_line().contains("PostgreSQL")
    }));
    assert!(!repo.sync_connected());
    assert!(
        repo.sync_status_is_error(),
        "postgres reject must flag sync_status_is_error"
    );

    // Empty URI must surface as an error banner, not silently clear state.
    repo.test_sync("");
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.sync_status_line().contains("Enter a connection")
    }));
    assert!(
        repo.sync_status_is_error(),
        "empty URI Test Sync must flag an error"
    );
    // Previous postgres URI should still be reported (remote not cleared).
    assert!(repo.sync_uri().contains("postgres"));

    let remote = dir.join("remote.db");
    let uri = format!("sqlite:{}", remote.display());
    repo.test_sync(&uri);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.sync_connected() && repo.sync_status_line().contains("Sync ok")
    }));
    assert_eq!(repo.sync_uri(), uri);
    assert!(remote.exists());

    // Persist: reopen and see the URI restored.
    drop(repo);
    let mut reopened = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(
        &mut reopened,
        Duration::from_secs(10),
        |repo| { !repo.loading() && repo.sync_uri() == uri }
    ));
    assert!(reopened.sync_connected());

    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worker_create_and_delete_managed_ssh_key() {
    let dir = temp_dir("ssh-key");
    let mut repo = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        !repo.loading()
    }));

    // Keys are sealed at rest, so saving one needs the vault.
    repo.create_ssh_key("Locked");
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.error().is_some_and(|n| n.contains("Unlock the vault"))
    }));
    assert!(repo.identities().is_empty());
    repo.unlock_vault("long-enough-passphrase");
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.vault_unlocked()
    }));
    let _ = repo.take_notice();

    repo.create_ssh_key("Laptop Ed25519");
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.identities().iter().any(|key| {
            !key.id.is_empty()
                && key.name == "Laptop Ed25519"
                && key.fingerprint.starts_with("SHA256:")
                && !key.created.is_empty()
                && key.public_key.starts_with("ssh-ed25519 ")
        })
    }));
    assert!(repo
        .take_notice()
        .is_some_and(|n| n.contains("Laptop Ed25519")));

    let raw = std::fs::read(dir.join("terminus.db")).unwrap_or_default();
    let needle = b"BEGIN OPENSSH PRIVATE KEY";
    assert!(
        !raw.windows(needle.len()).any(|w| w == needle),
        "managed key stored in plaintext"
    );

    let id = repo.identities()[0].id.clone();
    repo.delete_ssh_key(&id);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.identities().is_empty()
    }));

    // Import path: generate a PEM then re-import under a new label.
    let generated = terminus_core::generate_ed25519_identity("tmp").expect("pem");
    let pem = generated.private_key.expect("private");
    repo.create_ssh_key_with_pem("Imported", Some(pem));
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.identities()
            .iter()
            .any(|key| key.name == "Imported" && key.fingerprint.starts_with("SHA256:"))
    }));

    drop(repo);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worker_unlock_seals_legacy_plaintext_keys() {
    let dir = temp_dir("seal-legacy");
    {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let store = rt.block_on(Store::open(dir.clone())).unwrap();
        let ident = terminus_core::generate_ed25519_identity("legacy").unwrap();
        rt.block_on(store.upsert_identity(&ident)).unwrap();
    }
    let mut repo = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        !repo.loading()
    }));
    repo.unlock_vault("long-enough-passphrase");
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.vault_unlocked()
    }));
    drop(repo);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let store = rt.block_on(Store::open(dir.clone())).unwrap();
    let ids = rt.block_on(store.list_identities()).unwrap();
    assert_eq!(ids.len(), 1);
    assert!(
        terminus_core::identity_needs_vault(&ids[0]),
        "legacy key not sealed"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worker_unlock_vault_from_passphrase() {
    let dir = temp_dir("vault");
    let mut repo = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        !repo.loading()
    }));

    repo.unlock_vault("short");
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.sync_status_line().contains("at least 8")
    }));
    assert!(!repo.vault_unlocked());
    assert!(repo.sync_status_is_error());
    let _ = repo.take_vault_message();

    repo.unlock_vault("long-enough-passphrase");
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        repo.vault_unlocked()
    }));
    assert_eq!(repo.take_vault_message().as_deref(), Some("Vault unlocked"));

    drop(repo);
    let _ = std::fs::remove_dir_all(&dir);
}
