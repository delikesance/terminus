//! Red tests for the passphrase vault (Argon2id + XChaCha20-Poly1305).
//! Maps to issue #111 ACs 5–7 and passphrase validation.

use terminus_core::vault::{
    secret_aad, KdfParams, UnlockedVault, CREDENTIAL_KIND_SSH_PRIVATE_KEY, OWNER_KIND_IDENTITY,
};
use terminus_core::Error;

fn test_vault(passphrase: &str) -> (terminus_core::vault::VaultHeader, UnlockedVault) {
    UnlockedVault::create(passphrase, KdfParams::interactive_test()).expect("create vault")
}

#[test]
fn create_rejects_empty_passphrase() {
    let err = UnlockedVault::create("", KdfParams::interactive_test()).unwrap_err();
    assert!(matches!(err, Error::InvalidPassphrase));
}

#[test]
fn create_rejects_short_passphrase() {
    let err = UnlockedVault::create("1234567", KdfParams::interactive_test()).unwrap_err();
    assert!(matches!(err, Error::InvalidPassphrase));
}

#[test]
fn unlock_roundtrip_with_correct_passphrase() {
    let (header, vault) = test_vault("correct horse battery");
    assert_eq!(header.kdf, "argon2id");
    assert_eq!(header.v, 1);
    let unlocked = UnlockedVault::unlock(&header, "correct horse battery").expect("unlock");
    assert_eq!(unlocked.key_id(), vault.key_id());
}

#[test]
fn wrong_passphrase_fails_closed() {
    let (header, _) = test_vault("correct horse battery");
    let err = UnlockedVault::unlock(&header, "wrong passphrase!!").unwrap_err();
    assert!(matches!(err, Error::VaultUnlockFailed));
}

#[test]
fn envelope_roundtrip_and_never_contains_plaintext() {
    let (_, vault) = test_vault("correct horse battery");
    let secret = "-----BEGIN OPENSSH PRIVATE KEY-----\nSUPER-SECRET-KEY-MATERIAL\n";
    let aad = secret_aad(
        OWNER_KIND_IDENTITY,
        "ident-1",
        CREDENTIAL_KIND_SSH_PRIVATE_KEY,
    );
    let envelope = vault.seal(secret.as_bytes(), &aad).expect("seal");
    assert_eq!(envelope.alg, "xchacha20poly1305");
    assert_eq!(envelope.v, 1);

    let json = serde_json::to_string(&envelope).expect("json");
    assert!(
        !json.contains("SUPER-SECRET-KEY-MATERIAL"),
        "ciphertext JSON leaked plaintext: {json}"
    );

    let opened = vault.open(&envelope, &aad).expect("open");
    assert_eq!(opened, secret.as_bytes());
}

#[test]
fn decrypt_fails_when_aad_owner_is_swapped() {
    let (_, vault) = test_vault("correct horse battery");
    let aad = secret_aad(OWNER_KIND_IDENTITY, "ident-1", CREDENTIAL_KIND_SSH_PRIVATE_KEY);
    let envelope = vault.seal(b"private-key-bytes", &aad).expect("seal");
    let swapped = secret_aad(OWNER_KIND_IDENTITY, "ident-2", CREDENTIAL_KIND_SSH_PRIVATE_KEY);
    let err = vault.open(&envelope, &swapped).unwrap_err();
    assert!(
        matches!(err, Error::VaultDecryptFailed),
        "swap should not yield plaintext, got {err}"
    );
}

#[test]
fn decrypt_fails_when_ciphertext_is_flipped() {
    let (_, vault) = test_vault("correct horse battery");
    let aad = secret_aad(OWNER_KIND_IDENTITY, "ident-1", CREDENTIAL_KIND_SSH_PRIVATE_KEY);
    let mut envelope = vault.seal(b"private-key-bytes", &aad).expect("seal");
    let mut bytes = envelope.ct.clone().into_bytes();
    if let Some(b) = bytes.last_mut() {
        *b ^= 0xff;
    }
    envelope.ct = String::from_utf8_lossy(&bytes).into_owned();
    let err = vault.open(&envelope, &aad).unwrap_err();
    assert!(matches!(err, Error::VaultDecryptFailed));
}

#[test]
fn passphrase_change_rewraps_dek_without_resealing_items() {
    let (header, vault) = test_vault("correct horse battery");
    let aad = secret_aad(OWNER_KIND_IDENTITY, "ident-1", CREDENTIAL_KIND_SSH_PRIVATE_KEY);
    let envelope = vault.seal(b"still-the-same-secret", &aad).expect("seal");

    let new_header = vault.rewrap("new passphrase ok").expect("rewrap");
    assert_eq!(new_header.key_id, header.key_id);
    assert_ne!(new_header.salt, header.salt);
    assert_ne!(
        new_header.wrapped_dek.ct, header.wrapped_dek.ct,
        "wrapped DEK must change"
    );

    let relocked = UnlockedVault::unlock(&new_header, "new passphrase ok").expect("unlock new");
    let opened = relocked.open(&envelope, &aad).expect("open with new passphrase");
    assert_eq!(opened, b"still-the-same-secret");

    let err = UnlockedVault::unlock(&new_header, "correct horse battery").unwrap_err();
    assert!(matches!(err, Error::VaultUnlockFailed));
}

#[test]
fn production_params_are_stronger_than_test_params() {
    let prod = KdfParams::production();
    let test = KdfParams::interactive_test();
    assert!(prod.m_kib >= 19 * 1024, "OWASP Argon2id memory >= 19 MiB");
    assert!(prod.t >= 2);
    assert!(prod.p >= 1);
    assert!(test.m_kib < prod.m_kib || test.t < prod.t);
}
