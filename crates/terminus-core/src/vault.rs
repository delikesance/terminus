use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;

use crate::error::{Error, Result};

const SALT_LEN: usize = 16;
const DEK_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const ENVELOPE_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultStatus {
    Unconfigured,
    Locked,
    Unlocked,
}

impl VaultStatus {
    pub fn is_unlocked(&self) -> bool {
        matches!(self, Self::Unlocked)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultHeader {
    pub version: u8,
    pub salt: String,
    pub wrapped_dek: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEnvelope {
    pub version: u8,
    pub algorithm: String,
    pub nonce: String,
    pub ciphertext: String,
}

#[derive(ZeroizeOnDrop)]
pub struct UnlockedVault {
    dek: Vec<u8>,
}

impl UnlockedVault {
    pub fn create(passphrase: &str) -> Result<VaultHeader> {
        let mut salt = vec![0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);

        let dek = derive_dek(passphrase, &salt)?;

        let mut nonce_bytes = vec![0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from_slice(&nonce_bytes);

        let cipher = XChaCha20Poly1305::new(dek[..].into());
        let wrapped = cipher
            .encrypt(nonce, chacha20poly1305::aead::Payload::from(dek.as_ref()))
            .map_err(|e| Error::VaultError(format!("wrap failed: {e}")))?;

        let mut wrapped_with_nonce = nonce_bytes;
        wrapped_with_nonce.extend_from_slice(&wrapped);

        Ok(VaultHeader {
            version: ENVELOPE_VERSION,
            salt: base64::engine::general_purpose::STANDARD.encode(&salt),
            wrapped_dek: base64::engine::general_purpose::STANDARD
                .encode(&wrapped_with_nonce),
        })
    }

    pub fn unlock(passphrase: &str, header: &VaultHeader) -> Result<Self> {
        let salt = base64::engine::general_purpose::STANDARD
            .decode(&header.salt)
            .map_err(|e| Error::VaultError(format!("bad salt: {e}")))?;
        let wrapped_raw = base64::engine::general_purpose::STANDARD
            .decode(&header.wrapped_dek)
            .map_err(|e| Error::VaultError(format!("bad wrapped dek: {e}")))?;

        if wrapped_raw.len() < NONCE_LEN {
            return Err(Error::VaultError("wrapped dek too short".into()));
        }

        let dek = derive_dek(passphrase, &salt)?;
        let nonce = XNonce::from_slice(&wrapped_raw[..NONCE_LEN]);
        let ciphertext = &wrapped_raw[NONCE_LEN..];

        let cipher = XChaCha20Poly1305::new(dek[..].into());
        let plaintext = cipher
            .decrypt(nonce, chacha20poly1305::aead::Payload::from(ciphertext))
            .map_err(|e| Error::VaultError(format!("decrypt dek failed: {e}")))?;

        Ok(Self { dek: plaintext })
    }

    pub fn encrypt(
        &self,
        owner_kind: &str,
        owner_id: &str,
        kind: &str,
        plaintext: &[u8],
    ) -> Result<SecretEnvelope> {
        let aad = format!("{owner_kind}\n{owner_id}\n{kind}");
        let mut nonce_bytes = vec![0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from_slice(&nonce_bytes);

        let cipher = XChaCha20Poly1305::new(self.dek[..].into());
        let ciphertext = cipher
            .encrypt(
                nonce,
                chacha20poly1305::aead::Payload {
                    msg: plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|e| Error::VaultError(format!("encrypt failed: {e}")))?;

        Ok(SecretEnvelope {
            version: ENVELOPE_VERSION,
            algorithm: "xchacha20poly1305".into(),
            nonce: base64::engine::general_purpose::STANDARD.encode(&nonce_bytes),
            ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
        })
    }

    pub fn decrypt(
        &self,
        owner_kind: &str,
        owner_id: &str,
        kind: &str,
        envelope: &SecretEnvelope,
    ) -> Result<Vec<u8>> {
        let aad = format!("{owner_kind}\n{owner_id}\n{kind}");
        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(&envelope.nonce)
            .map_err(|e| Error::VaultError(format!("bad nonce: {e}")))?;
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(&envelope.ciphertext)
            .map_err(|e| Error::VaultError(format!("bad ciphertext: {e}")))?;

        let nonce = XNonce::from_slice(&nonce_bytes);
        let cipher = XChaCha20Poly1305::new(self.dek[..].into());
        cipher
            .decrypt(
                nonce,
                chacha20poly1305::aead::Payload {
                    msg: &ciphertext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| Error::VaultDecryptFailed)
    }
}

/// Setting key for the persisted [`VaultHeader`] JSON.
pub const VAULT_HEADER_SETTING: &str = "vault_header";

/// Credential kind for a host SSH password envelope.
pub const CREDENTIAL_KIND_HOST_PASSWORD: &str = "host_password";

/// Owner kind for host-scoped credentials.
pub const OWNER_KIND_HOST: &str = "host";

/// Deterministic credential id for `(owner, kind)`.
pub fn credential_id(owner_kind: &str, owner_id: &uuid::Uuid, kind: &str) -> uuid::Uuid {
    // Stable UUIDv5-ish from namespaced bytes without pulling uuid v5 feature quirks:
    // hash the triple and take the first 16 bytes as a UUID.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    owner_kind.hash(&mut hasher);
    owner_id.hash(&mut hasher);
    kind.hash(&mut hasher);
    let n = hasher.finish();
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&n.to_le_bytes());
    bytes[8..].copy_from_slice(&n.to_be_bytes());
    uuid::Uuid::from_bytes(bytes)
}

/// Seal a host password into a [`crate::models::Credential`] row.
pub fn seal_host_password(
    vault: &UnlockedVault,
    host_id: uuid::Uuid,
    password: &str,
) -> Result<crate::models::Credential> {
    let envelope = vault.encrypt(
        OWNER_KIND_HOST,
        &host_id.to_string(),
        CREDENTIAL_KIND_HOST_PASSWORD,
        password.as_bytes(),
    )?;
    let envelope_json = serde_json::to_string(&envelope)?;
    let now = chrono::Utc::now();
    Ok(crate::models::Credential {
        id: credential_id(OWNER_KIND_HOST, &host_id, CREDENTIAL_KIND_HOST_PASSWORD),
        kind: CREDENTIAL_KIND_HOST_PASSWORD.to_string(),
        owner_kind: OWNER_KIND_HOST.to_string(),
        owner_id: host_id,
        envelope: envelope_json,
        key_id: String::new(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    })
}

/// Open a host password credential envelope.
pub fn open_host_password(
    vault: &UnlockedVault,
    host_id: uuid::Uuid,
    cred: &crate::models::Credential,
) -> Result<String> {
    let envelope: SecretEnvelope = serde_json::from_str(&cred.envelope)?;
    let bytes = vault.decrypt(
        OWNER_KIND_HOST,
        &host_id.to_string(),
        CREDENTIAL_KIND_HOST_PASSWORD,
        &envelope,
    )?;
    String::from_utf8(bytes).map_err(|e| Error::VaultError(format!("password utf8: {e}")))
}

/// Persist / load the vault header JSON via settings.
pub fn parse_vault_header(raw: &str) -> Result<VaultHeader> {
    serde_json::from_str(raw).map_err(|e| Error::VaultError(format!("bad vault header: {e}")))
}

pub fn encode_vault_header(header: &VaultHeader) -> Result<String> {
    serde_json::to_string(header).map_err(|e| Error::VaultError(format!("encode vault header: {e}")))
}

/// Creates a new vault for `passphrase`, returning the header to persist and
/// the unlocked handle in one step.
pub fn create_with_key(passphrase: &str) -> Result<(VaultHeader, UnlockedVault)> {
    let header = UnlockedVault::create(passphrase)?;
    let vault = UnlockedVault::unlock(passphrase, &header)?;
    Ok((header, vault))
}

fn derive_dek(passphrase: &str, salt: &[u8]) -> Result<Vec<u8>> {
    let params = Params::new(19_456, 2, 1, Some(DEK_LEN))
        .map_err(|e| Error::VaultError(format!("argon2 params: {e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut dek = vec![0u8; DEK_LEN];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut dek)
        .map_err(|e| Error::VaultError(format!("argon2 kdf: {e}")))?;
    Ok(dek)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_password_roundtrip_and_never_leaks_plaintext() {
        let (_header, vault) = create_with_key("correct horse battery").unwrap();
        let host_id = uuid::Uuid::new_v4();
        let secret = "s3cret-password!!";
        let cred = seal_host_password(&vault, host_id, secret).unwrap();
        assert!(
            !cred.envelope.contains(secret),
            "envelope leaked plaintext"
        );
        assert_eq!(open_host_password(&vault, host_id, &cred).unwrap(), secret);
    }

    #[test]
    fn decrypt_fails_when_owner_is_swapped() {
        let (_header, vault) = create_with_key("correct horse battery").unwrap();
        let host_id = uuid::Uuid::new_v4();
        let other = uuid::Uuid::new_v4();
        let cred = seal_host_password(&vault, host_id, "pw").unwrap();
        let envelope: SecretEnvelope = serde_json::from_str(&cred.envelope).unwrap();
        let err = vault
            .decrypt(
                OWNER_KIND_HOST,
                &other.to_string(),
                CREDENTIAL_KIND_HOST_PASSWORD,
                &envelope,
            )
            .unwrap_err();
        assert!(matches!(err, Error::VaultDecryptFailed));
    }

    #[test]
    fn credential_id_is_stable_for_same_owner_kind() {
        let id = uuid::Uuid::new_v4();
        assert_eq!(
            credential_id(OWNER_KIND_HOST, &id, CREDENTIAL_KIND_HOST_PASSWORD),
            credential_id(OWNER_KIND_HOST, &id, CREDENTIAL_KIND_HOST_PASSWORD)
        );
    }
}
