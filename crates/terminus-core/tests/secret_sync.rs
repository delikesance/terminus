//! Red tests for encrypted credential sync (issue #111 ACs 1–4, 8).
//! No plaintext host passwords / SSH keys / identity secrets on the remote payload.

use terminus_core::models::{Host, Identity, IDENTITY_KIND_AGENT, IDENTITY_KIND_PASSWORD};
use terminus_core::store::Store;
use terminus_core::sync::SyncEngine;
use terminus_core::vault::{
    apply_secret_credentials, collect_secret_credentials, credential_id, strip_host_secrets,
    strip_identity_secrets, KdfParams, UnlockedVault, CREDENTIAL_KIND_HOST_PASSWORD,
    CREDENTIAL_KIND_IDENTITY_PASSWORD, CREDENTIAL_KIND_SSH_PASSPHRASE,
    CREDENTIAL_KIND_SSH_PRIVATE_KEY, OWNER_KIND_HOST, OWNER_KIND_IDENTITY,
};
use terminus_core::Error;

fn test_vault() -> UnlockedVault {
    UnlockedVault::create("correct horse battery", KdfParams::interactive_test())
        .expect("vault")
        .1
}

fn secret_host() -> Host {
    let mut host = Host::new("prod", "10.0.0.8", 22, "deploy");
    host.password = Some("hunter2-host-pass".into());
    host
}

fn key_identity() -> Identity {
    let mut ident = Identity::new("deploy-key");
    ident.private_key = Some("-----BEGIN OPENSSH PRIVATE KEY-----\nLOCAL-KEY\n".into());
    ident.passphrase = Some("key-passphrase-9".into());
    ident
}

fn password_identity() -> Identity {
    let mut ident = Identity::new("jump-pw");
    ident.kind = IDENTITY_KIND_PASSWORD.into();
    ident.passphrase = Some("reusable-password".into());
    ident
}

fn agent_identity() -> Identity {
    let mut ident = Identity::new("agent");
    ident.kind = IDENTITY_KIND_AGENT.into();
    ident
}

#[test]
fn collect_encrypts_host_password_ssh_key_and_identity_password() {
    let vault = test_vault();
    let host = secret_host();
    let key = key_identity();
    let pw = password_identity();
    let agent = agent_identity();

    let creds = collect_secret_credentials(&[host.clone()], &[key.clone(), pw.clone(), agent], &vault)
        .expect("collect");

    let kinds: Vec<_> = creds.iter().map(|c| c.kind.as_str()).collect();
    assert!(kinds.contains(&CREDENTIAL_KIND_HOST_PASSWORD));
    assert!(kinds.contains(&CREDENTIAL_KIND_SSH_PRIVATE_KEY));
    assert!(kinds.contains(&CREDENTIAL_KIND_SSH_PASSPHRASE));
    assert!(kinds.contains(&CREDENTIAL_KIND_IDENTITY_PASSWORD));
    assert_eq!(creds.len(), 4, "agent identities must not emit envelopes");

    let blob = serde_json::to_string(&creds).unwrap();
    for needle in [
        "hunter2-host-pass",
        "LOCAL-KEY",
        "key-passphrase-9",
        "reusable-password",
    ] {
        assert!(!blob.contains(needle), "credential dump leaked {needle}: {blob}");
    }

    assert_eq!(
        creds
            .iter()
            .find(|c| c.kind == CREDENTIAL_KIND_HOST_PASSWORD)
            .unwrap()
            .id,
        credential_id(OWNER_KIND_HOST, &host.id, CREDENTIAL_KIND_HOST_PASSWORD)
    );
}

#[test]
fn apply_restores_secrets_on_replica() {
    let vault = test_vault();
    let host = secret_host();
    let key = key_identity();
    let pw = password_identity();
    let creds =
        collect_secret_credentials(&[host.clone()], &[key.clone(), pw.clone()], &vault).unwrap();

    let mut hosts = [host.clone()];
    strip_host_secrets(&mut hosts[0]);
    let mut idents = [key.clone(), pw.clone()];
    strip_identity_secrets(&mut idents[0]);
    strip_identity_secrets(&mut idents[1]);

    assert!(hosts[0].password.is_none());
    assert!(idents[0].private_key.is_none());
    assert!(idents[0].passphrase.is_none());
    assert!(idents[1].passphrase.is_none());

    apply_secret_credentials(&mut hosts, &mut idents, &creds, &vault).unwrap();

    assert_eq!(hosts[0].password.as_deref(), Some("hunter2-host-pass"));
    assert_eq!(
        idents[0].private_key.as_deref(),
        Some("-----BEGIN OPENSSH PRIVATE KEY-----\nLOCAL-KEY\n")
    );
    assert_eq!(idents[0].passphrase.as_deref(), Some("key-passphrase-9"));
    assert_eq!(idents[1].passphrase.as_deref(), Some("reusable-password"));
}

#[test]
fn strip_keeps_public_metadata() {
    let mut ident = key_identity();
    ident.public_key = Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIpublic".into());
    strip_identity_secrets(&mut ident);
    assert!(ident.private_key.is_none());
    assert!(ident.passphrase.is_none());
    assert_eq!(
        ident.public_key.as_deref(),
        Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIpublic")
    );
    assert_eq!(ident.name, "deploy-key");
}

#[tokio::test]
async fn store_roundtrips_encrypted_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("t.db")).await.unwrap();
    let vault = test_vault();
    let host = secret_host();
    let creds = collect_secret_credentials(&[host], &[], &vault).unwrap();
    store.upsert_credential(&creds[0]).await.unwrap();
    let listed = store.list_credentials().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].kind, CREDENTIAL_KIND_HOST_PASSWORD);
    assert!(!listed[0].envelope.contains("hunter2-host-pass"));
}

#[tokio::test]
async fn enabling_secret_sync_without_vault_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("t.db")).await.unwrap();
    let engine = SyncEngine::new(store);
    let err = engine.set_sync_secrets(true).await.unwrap_err();
    assert!(
        matches!(err, Error::VaultRequired),
        "expected VaultRequired, got {err}"
    );
    let status = engine.status().await;
    assert!(!status.sync_secrets);
    assert!(!status.vault_configured);
    assert!(!status.vault_unlocked);
}

#[tokio::test]
async fn vault_create_unlock_lock_and_wrong_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("t.db")).await.unwrap();
    let engine = SyncEngine::new(store);

    let created = engine
        .vault_create("correct horse battery")
        .await
        .expect("create");
    assert!(created.configured);
    assert!(created.unlocked);

    engine.vault_lock().await;
    let locked = engine.vault_status().await;
    assert!(locked.configured);
    assert!(!locked.unlocked);

    let err = engine.vault_unlock("not the passphrase").await.unwrap_err();
    assert!(matches!(err, Error::VaultUnlockFailed));
    assert!(!engine.vault_status().await.unlocked);

    let unlocked = engine
        .vault_unlock("correct horse battery")
        .await
        .expect("unlock");
    assert!(unlocked.unlocked);

    engine.set_sync_secrets(true).await.expect("opt-in after vault");
    assert!(engine.status().await.sync_secrets);
}

#[tokio::test]
async fn locked_vault_preserves_local_secrets_on_prepare_pull() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("t.db")).await.unwrap();
    let mut host = secret_host();
    store.upsert_host(&host).await.unwrap();
    let mut ident = key_identity();
    store.upsert_identity(&ident).await.unwrap();

    let engine = SyncEngine::new(store.clone());
    engine
        .vault_create("correct horse battery")
        .await
        .unwrap();
    engine.set_sync_secrets(true).await.unwrap();
    engine.vault_lock().await;

    // Simulate a remote row that stripped secrets (nulls).
    host.password = None;
    ident.private_key = None;
    ident.passphrase = None;
    let host_id = host.id.clone();
    let ident_id = ident.id.clone();
    terminus_core::sync::preserve_local_secrets_if_locked(
        true,
        false,
        &mut host,
        store.get_host(&host_id).await.unwrap().as_ref(),
    );
    terminus_core::sync::preserve_local_identity_secrets_if_locked(
        true,
        false,
        &mut ident,
        store.get_identity(&ident_id).await.unwrap().as_ref(),
    );

    assert_eq!(host.password.as_deref(), Some("hunter2-host-pass"));
    assert_eq!(
        ident.private_key.as_deref(),
        Some("-----BEGIN OPENSSH PRIVATE KEY-----\nLOCAL-KEY\n")
    );
}

#[test]
fn filter_helpers_never_emit_plaintext_when_secret_sync_on() {
    let mut host = serde_json::json!({
        "id": "h1",
        "password": "hunter2-host-pass",
        "name": "prod"
    });
    let mut ident = serde_json::json!({
        "id": "i1",
        "private_key": "LOCAL-KEY",
        "passphrase": "key-passphrase-9",
        "public_key": "ssh-ed25519 AAAA"
    });
    terminus_core::sync::strip_secret_json(&mut host);
    terminus_core::sync::strip_secret_json(&mut ident);
    assert!(host.get("password").unwrap().is_null());
    assert!(ident.get("private_key").unwrap().is_null());
    assert!(ident.get("passphrase").unwrap().is_null());
    assert_eq!(ident.get("public_key").unwrap().as_str(), Some("ssh-ed25519 AAAA"));
}
