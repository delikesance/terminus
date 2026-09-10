//! Zero-knowledge vault: Argon2id (MK) wrapping an XChaCha20-Poly1305 DEK.
//! Secrets are sealed with the DEK and bound to AAD so blobs cannot be swapped.

use crate::error::{Error, Result};
use crate::models::{
    Credential, Host, Identity, IDENTITY_KIND_AGENT, IDENTITY_KIND_PASSWORD,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use chrono::Utc;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::ZeroizeOnDrop;

pub const CREDENTIAL_KIND_HOST_PASSWORD: &str = "host_password";
pub const CREDENTIAL_KIND_SSH_PRIVATE_KEY: &str = "ssh_private_key";
pub const CREDENTIAL_KIND_SSH_PASSPHRASE: &str = "ssh_passphrase";
pub const CREDENTIAL_KIND_IDENTITY_PASSWORD: &str = "identity_password";
pub const OWNER_KIND_HOST: &str = "host";
pub const OWNER_KIND_IDENTITY: &str = "identity";

pub const ALG_XCHACHA20POLY1305: &str = "xchacha20poly1305";
pub const KDF_ARGON2ID: &str = "argon2id";
pub const VAULT_VERSION: u8 = 1;
pub const MIN_PASSPHRASE_LEN: usize = 8;

const DEK_AAD: &[u8] = b"terminus:vault:dek:v1";
const SALT_LEN: usize = 16;
const DEK_LEN: usize = 32;
const NONCE_LEN: usize = 24;

#[derive(Debug, Clone, Copy)]
pub struct KdfParams {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl KdfParams {
    /// OWASP minimum interactive Argon2id (19 MiB, 2 iterations).
    pub fn production() -> Self {
        Self {
            m_kib: 19 * 1024,
            t: 2,
            p: 1,
        }
    }

    /// Fast params for unit tests (encoded in the vault header, never implied).
    pub fn interactive_test() -> Self {
        Self {
            m_kib: 8,
            t: 1,
            p: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretEnvelope {
    pub v: u8,
    pub alg: String,
    pub nonce: String,
    pub ct: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultHeader {
    pub v: u8,
    pub kdf: String,
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
    pub salt: String,
    pub key_id: String,
    pub wrapped_dek: SecretEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatus {
    pub configured: bool,
    pub unlocked: bool,
    pub key_id: Option<String>,
}

#[derive(ZeroizeOnDrop)]
pub struct UnlockedVault {
    #[zeroize(skip)]
    key_id: String,
    dek: [u8; DEK_LEN],
    #[zeroize(skip)]
    header: VaultHeader,
}

impl fmt::Debug for UnlockedVault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnlockedVault")
            .field("key_id", &self.key_id)
            .field("dek", &"[redacted]")
            .field("header", &self.header)
            .finish()
    }
}

impl UnlockedVault {
    pub fn create(passphrase: &str, params: KdfParams) -> Result<(VaultHeader, Self)> {
        validate_passphrase(passphrase)?;
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let mk = derive_mk(passphrase, &salt, params)?;
        let mut dek = [0u8; DEK_LEN];
        OsRng.fill_bytes(&mut dek);
        let key_id = uuid::Uuid::new_v4().to_string();
        let wrapped_dek = seal_with_key(&mk, &dek, DEK_AAD)?;
        let header = VaultHeader {
            v: VAULT_VERSION,
            kdf: KDF_ARGON2ID.into(),
            m_kib: params.m_kib,
            t: params.t,
            p: params.p,
            salt: STANDARD.encode(salt),
            key_id: key_id.clone(),
            wrapped_dek,
        };
        Ok((
            header.clone(),
            Self {
                key_id,
                dek,
                header,
            },
        ))
    }

    pub fn unlock(header: &VaultHeader, passphrase: &str) -> Result<Self> {
        if header.kdf != KDF_ARGON2ID || header.v != VAULT_VERSION {
            return Err(Error::VaultUnlockFailed);
        }
        let salt = STANDARD
            .decode(&header.salt)
            .map_err(|_| Error::VaultUnlockFailed)?;
        let params = KdfParams {
            m_kib: header.m_kib,
            t: header.t,
            p: header.p,
        };
        let mk = derive_mk(passphrase, &salt, params).map_err(|_| Error::VaultUnlockFailed)?;
        let dek_bytes = open_with_key(&mk, &header.wrapped_dek, DEK_AAD)
            .map_err(|_| Error::VaultUnlockFailed)?;
        if dek_bytes.len() != DEK_LEN {
            return Err(Error::VaultUnlockFailed);
        }
        let mut dek = [0u8; DEK_LEN];
        dek.copy_from_slice(&dek_bytes);
        Ok(Self {
            key_id: header.key_id.clone(),
            dek,
            header: header.clone(),
        })
    }

    pub fn rewrap(&self, new_passphrase: &str) -> Result<VaultHeader> {
        validate_passphrase(new_passphrase)?;
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let params = KdfParams {
            m_kib: self.header.m_kib,
            t: self.header.t,
            p: self.header.p,
        };
        let mk = derive_mk(new_passphrase, &salt, params)?;
        let wrapped_dek = seal_with_key(&mk, &self.dek, DEK_AAD)?;
        Ok(VaultHeader {
            v: VAULT_VERSION,
            kdf: KDF_ARGON2ID.into(),
            m_kib: params.m_kib,
            t: params.t,
            p: params.p,
            salt: STANDARD.encode(salt),
            key_id: self.key_id.clone(),
            wrapped_dek,
        })
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn header(&self) -> &VaultHeader {
        &self.header
    }

    pub fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Result<SecretEnvelope> {
        seal_with_key(&self.dek, plaintext, aad)
    }

    pub fn open(&self, envelope: &SecretEnvelope, aad: &[u8]) -> Result<Vec<u8>> {
        open_with_key(&self.dek, envelope, aad)
    }
}

pub fn validate_passphrase(passphrase: &str) -> Result<()> {
    if passphrase.chars().count() < MIN_PASSPHRASE_LEN {
        return Err(Error::InvalidPassphrase);
    }
    Ok(())
}

pub fn secret_aad(owner_kind: &str, owner_id: &str, kind: &str) -> Vec<u8> {
    format!("{owner_kind}\n{owner_id}\n{kind}").into_bytes()
}

pub fn credential_id(owner_kind: &str, owner_id: &str, kind: &str) -> String {
    format!("{owner_kind}:{owner_id}:{kind}")
}

pub fn strip_host_secrets(host: &mut Host) {
    host.password = None;
}

pub fn strip_identity_secrets(identity: &mut Identity) {
    identity.private_key = None;
    identity.passphrase = None;
}

pub fn collect_secret_credentials(
    hosts: &[Host],
    identities: &[Identity],
    vault: &UnlockedVault,
) -> Result<Vec<Credential>> {
    let now = Utc::now();
    let mut out = Vec::new();
    for host in hosts {
        if let Some(password) = host.password.as_deref().filter(|s| !s.is_empty()) {
            out.push(seal_credential(
                vault,
                OWNER_KIND_HOST,
                &host.id,
                CREDENTIAL_KIND_HOST_PASSWORD,
                password.as_bytes(),
                now,
            )?);
        }
    }
    for identity in identities {
        let mut ident = identity.clone();
        ident.normalize_kind();
        if ident.kind == IDENTITY_KIND_AGENT {
            continue;
        }
        if ident.kind == IDENTITY_KIND_PASSWORD {
            if let Some(password) = ident.passphrase.as_deref().filter(|s| !s.is_empty()) {
                out.push(seal_credential(
                    vault,
                    OWNER_KIND_IDENTITY,
                    &ident.id,
                    CREDENTIAL_KIND_IDENTITY_PASSWORD,
                    password.as_bytes(),
                    now,
                )?);
            }
            continue;
        }
        if let Some(key) = ident.private_key.as_deref().filter(|s| !s.is_empty()) {
            out.push(seal_credential(
                vault,
                OWNER_KIND_IDENTITY,
                &ident.id,
                CREDENTIAL_KIND_SSH_PRIVATE_KEY,
                key.as_bytes(),
                now,
            )?);
        }
        if let Some(pass) = ident.passphrase.as_deref().filter(|s| !s.is_empty()) {
            out.push(seal_credential(
                vault,
                OWNER_KIND_IDENTITY,
                &ident.id,
                CREDENTIAL_KIND_SSH_PASSPHRASE,
                pass.as_bytes(),
                now,
            )?);
        }
    }
    Ok(out)
}

pub fn apply_secret_credentials(
    hosts: &mut [Host],
    identities: &mut [Identity],
    creds: &[Credential],
    vault: &UnlockedVault,
) -> Result<()> {
    for cred in creds {
        let aad = secret_aad(&cred.owner_kind, &cred.owner_id, &cred.kind);
        let envelope: SecretEnvelope = serde_json::from_str(&cred.envelope)
            .map_err(|_| Error::VaultDecryptFailed)?;
        let pt = vault.open(&envelope, &aad)?;
        let text = String::from_utf8(pt).map_err(|_| Error::VaultDecryptFailed)?;
        match cred.kind.as_str() {
            CREDENTIAL_KIND_HOST_PASSWORD => {
                if let Some(host) = hosts.iter_mut().find(|h| h.id == cred.owner_id) {
                    host.password = Some(text);
                }
            }
            CREDENTIAL_KIND_SSH_PRIVATE_KEY => {
                if let Some(ident) = identities.iter_mut().find(|i| i.id == cred.owner_id) {
                    ident.private_key = Some(text);
                }
            }
            CREDENTIAL_KIND_SSH_PASSPHRASE | CREDENTIAL_KIND_IDENTITY_PASSWORD => {
                if let Some(ident) = identities.iter_mut().find(|i| i.id == cred.owner_id) {
                    ident.passphrase = Some(text);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn seal_credential(
    vault: &UnlockedVault,
    owner_kind: &str,
    owner_id: &str,
    kind: &str,
    plaintext: &[u8],
    now: chrono::DateTime<Utc>,
) -> Result<Credential> {
    let aad = secret_aad(owner_kind, owner_id, kind);
    let envelope = vault.seal(plaintext, &aad)?;
    Ok(Credential {
        id: credential_id(owner_kind, owner_id, kind),
        kind: kind.into(),
        owner_kind: owner_kind.into(),
        owner_id: owner_id.into(),
        envelope: serde_json::to_string(&envelope)?,
        key_id: vault.key_id().into(),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    })
}

fn derive_mk(passphrase: &str, salt: &[u8], params: KdfParams) -> Result<[u8; DEK_LEN]> {
    let argon_params = argon2::Params::new(params.m_kib, params.t, params.p, Some(DEK_LEN))
        .map_err(|e| Error::msg(format!("argon2 params: {e}")))?;
    let argon = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon_params,
    );
    let mut mk = [0u8; DEK_LEN];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut mk)
        .map_err(|e| Error::msg(format!("argon2: {e}")))?;
    Ok(mk)
}

fn seal_with_key(key: &[u8; DEK_LEN], plaintext: &[u8], aad: &[u8]) -> Result<SecretEnvelope> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, Payload { msg: plaintext, aad })
        .map_err(|_| Error::VaultDecryptFailed)?;
    Ok(SecretEnvelope {
        v: VAULT_VERSION,
        alg: ALG_XCHACHA20POLY1305.into(),
        nonce: STANDARD.encode(nonce_bytes),
        ct: STANDARD.encode(ct),
    })
}

fn open_with_key(key: &[u8; DEK_LEN], envelope: &SecretEnvelope, aad: &[u8]) -> Result<Vec<u8>> {
    if envelope.alg != ALG_XCHACHA20POLY1305 || envelope.v != VAULT_VERSION {
        return Err(Error::VaultDecryptFailed);
    }
    let nonce_bytes = STANDARD
        .decode(&envelope.nonce)
        .map_err(|_| Error::VaultDecryptFailed)?;
    let ct = STANDARD
        .decode(&envelope.ct)
        .map_err(|_| Error::VaultDecryptFailed)?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(Error::VaultDecryptFailed);
    }
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XNonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, Payload { msg: &ct, aad })
        .map_err(|_| Error::VaultDecryptFailed)
}
