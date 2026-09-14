use argon2::{Argon2, Algorithm, Version, Params};
use base64::Engine as _;
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
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
            wrapped_dek: base64::engine::general_purpose::STANDARD.encode(&wrapped_with_nonce),
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
            .map_err(|e| Error::VaultError(format!("decrypt failed: {e}")))
    }
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
