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

        if nonce_bytes.len() != NONCE_LEN {
            return Err(Error::VaultDecryptFailed);
        }
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
///
/// UUIDv5 over the triple: the id is persisted and synced between devices, so
/// it must not depend on `DefaultHasher`, whose algorithm is unspecified and
/// may change between Rust releases.
pub fn credential_id(owner_kind: &str, owner_id: &uuid::Uuid, kind: &str) -> uuid::Uuid {
    const NAMESPACE: uuid::Uuid =
        uuid::Uuid::from_u128(0x7465_726d_696e_7573_2d63_7265_6465_6e74);
    uuid::Uuid::new_v5(
        &NAMESPACE,
        format!("{owner_kind}\n{owner_id}\n{kind}").as_bytes(),
    )
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

/// Owner kind for managed SSH identity secrets.
pub const OWNER_KIND_IDENTITY: &str = "identity";
/// Envelope kind for a managed private key PEM.
pub const CREDENTIAL_KIND_PRIVATE_KEY: &str = "private_key";
/// Envelope kind for a managed private key's passphrase.
pub const CREDENTIAL_KIND_KEY_PASSPHRASE: &str = "key_passphrase";
/// Marker on an identity column holding a sealed [`SecretEnvelope`] (JSON).
pub const SEALED_PREFIX: &str = "terminus-sealed:v1:";

/// Whether a stored column value is a sealed envelope.
pub fn is_sealed(value: &str) -> bool {
    value.starts_with(SEALED_PREFIX)
}

/// True when opening `identity`'s secrets requires an unlocked vault.
pub fn identity_needs_vault(identity: &crate::models::Identity) -> bool {
    identity.private_key.as_deref().is_some_and(is_sealed)
        || identity.passphrase.as_deref().is_some_and(is_sealed)
}

fn seal_field(
    vault: &UnlockedVault,
    owner_id: &uuid::Uuid,
    kind: &str,
    value: Option<&str>,
) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    if is_sealed(value) || value.is_empty() {
        return Ok(Some(value.to_string()));
    }
    let envelope = vault.encrypt(
        OWNER_KIND_IDENTITY,
        &owner_id.to_string(),
        kind,
        value.as_bytes(),
    )?;
    Ok(Some(format!(
        "{SEALED_PREFIX}{}",
        serde_json::to_string(&envelope)?
    )))
}

fn open_field(
    vault: Option<&UnlockedVault>,
    owner_id: &uuid::Uuid,
    kind: &str,
    value: Option<&str>,
) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    let Some(json) = value.strip_prefix(SEALED_PREFIX) else {
        return Ok((!value.is_empty()).then(|| value.to_string()));
    };
    let vault = vault.ok_or_else(|| {
        Error::VaultError("the vault is locked; unlock it to use this SSH key".into())
    })?;
    let envelope: SecretEnvelope = serde_json::from_str(json)?;
    let bytes =
        vault.decrypt(OWNER_KIND_IDENTITY, &owner_id.to_string(), kind, &envelope)?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|e| Error::VaultError(format!("sealed key is not utf-8: {e}")))
}

/// Copy of `identity` with its private key and passphrase sealed by `vault`.
/// Already-sealed fields are kept as they are.
pub fn seal_identity(
    vault: &UnlockedVault,
    identity: &crate::models::Identity,
) -> Result<crate::models::Identity> {
    let mut sealed = identity.clone();
    sealed.private_key = seal_field(
        vault,
        &identity.id,
        CREDENTIAL_KIND_PRIVATE_KEY,
        identity.private_key.as_deref(),
    )?;
    sealed.passphrase = seal_field(
        vault,
        &identity.id,
        CREDENTIAL_KIND_KEY_PASSPHRASE,
        identity.passphrase.as_deref(),
    )?;
    Ok(sealed)
}

/// `(private key PEM, passphrase)` of `identity`, unsealing when needed.
///
/// Legacy plaintext rows open without a vault; sealed ones need `vault`.
pub fn open_identity_secrets(
    vault: Option<&UnlockedVault>,
    identity: &crate::models::Identity,
) -> Result<(Option<String>, Option<String>)> {
    Ok((
        open_field(
            vault,
            &identity.id,
            CREDENTIAL_KIND_PRIVATE_KEY,
            identity.private_key.as_deref(),
        )?,
        open_field(
            vault,
            &identity.id,
            CREDENTIAL_KIND_KEY_PASSPHRASE,
            identity.passphrase.as_deref(),
        )?,
    ))
}

/// Persist / load the vault header JSON via settings.
pub fn parse_vault_header(raw: &str) -> Result<VaultHeader> {
    serde_json::from_str(raw)
        .map_err(|e| Error::VaultError(format!("bad vault header: {e}")))
}

pub fn encode_vault_header(header: &VaultHeader) -> Result<String> {
    serde_json::to_string(header)
        .map_err(|e| Error::VaultError(format!("encode vault header: {e}")))
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
        assert!(!cred.envelope.contains(secret), "envelope leaked plaintext");
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
    fn credential_id_is_a_fixed_uuid_v5() {
        let id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let cid = credential_id(OWNER_KIND_HOST, &id, CREDENTIAL_KIND_HOST_PASSWORD);
        assert_eq!(cid.get_version_num(), 5);
        assert_ne!(
            cid,
            credential_id(OWNER_KIND_HOST, &id, "other_kind"),
            "kind must be part of the id"
        );
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
