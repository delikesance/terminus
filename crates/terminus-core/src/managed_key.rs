//! Generate and import OpenSSH identities for Managed SSH Keys.

use chrono::Utc;
use russh::keys::{ssh_key::LineEnding, Algorithm, PrivateKey};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::models::Identity;
use crate::ssh::fingerprint_of;

/// Build a new Ed25519 [`Identity`] ready to persist.
///
/// Private key material is stored as an OpenSSH PEM so
/// [`crate::ssh`] can load it with `decode_secret_key` during probe.
pub fn generate_ed25519_identity(name: impl Into<String>) -> Result<Identity> {
    let name = name.into().trim().to_string();
    if name.is_empty() {
        return Err(Error::IdentityKeyInvalid {
            reason: "key label is empty".into(),
        });
    }

    let key = PrivateKey::random(&mut rand_keygen::rng(), Algorithm::Ed25519).map_err(|e| {
        Error::IdentityKeyInvalid {
            reason: format!("generate ed25519: {e}"),
        }
    })?;

    identity_from_private_key(name, &key, None)
}

/// Import an OpenSSH private key PEM (optionally passphrase-protected).
pub fn import_openssh_identity(
    name: impl Into<String>,
    pem: &str,
    passphrase: Option<&str>,
) -> Result<Identity> {
    let name = name.into().trim().to_string();
    if name.is_empty() {
        return Err(Error::IdentityKeyInvalid {
            reason: "key label is empty".into(),
        });
    }
    let pem = pem.trim();
    if pem.is_empty() {
        return Err(Error::IdentityKeyInvalid {
            reason: "private key PEM is empty".into(),
        });
    }

    let key = russh::keys::decode_secret_key(pem, passphrase).map_err(|e| {
        Error::IdentityKeyInvalid {
            reason: format!("import openssh key: {e}"),
        }
    })?;

    // Prefer the PEM the user provided so passphrase-encrypted material stays
    // encrypted at rest when they supplied a passphrase.
    let now = Utc::now();
    let public_key = key.public_key().to_openssh().map_err(|e| {
        Error::IdentityKeyInvalid {
            reason: format!("encode public key: {e}"),
        }
    })?;
    let _ = fingerprint_of(key.public_key());

    Ok(Identity {
        id: Uuid::new_v4(),
        name,
        kind: "key".into(),
        public_key: Some(public_key),
        private_key: Some(pem.to_string()),
        passphrase: passphrase.map(str::to_string),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    })
}

fn identity_from_private_key(
    name: String,
    key: &PrivateKey,
    passphrase: Option<String>,
) -> Result<Identity> {
    let private_key = key
        .to_openssh(LineEnding::LF)
        .map_err(|e| Error::IdentityKeyInvalid {
            reason: format!("encode private key: {e}"),
        })?
        .to_string();
    let public_key = key.public_key().to_openssh().map_err(|e| {
        Error::IdentityKeyInvalid {
            reason: format!("encode public key: {e}"),
        }
    })?;
    let now = Utc::now();
    Ok(Identity {
        id: Uuid::new_v4(),
        name,
        kind: "key".into(),
        public_key: Some(public_key),
        private_key: Some(private_key),
        passphrase,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    })
}

/// `SHA256:…` fingerprint from an OpenSSH public-key line, or a short fallback.
pub fn fingerprint_from_public_openssh(public_key: &str) -> String {
    let trimmed = public_key.trim();
    if let Ok(pk) = russh::keys::PublicKey::from_openssh(trimmed) {
        return fingerprint_of(&pk);
    }
    // Fall back to base64 body only (known_hosts style).
    if let Some(b64) = trimmed.split_whitespace().nth(1) {
        if let Ok(pk) = russh::keys::parse_public_key_base64(b64) {
            return fingerprint_of(&pk);
        }
    }
    if trimmed.len() > 28 {
        format!("{}…", &trimmed[..28])
    } else if trimmed.is_empty() {
        "no public key".into()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_roundtrips_through_decode_secret_key() {
        let ident = generate_ed25519_identity("laptop").expect("generate");
        assert_eq!(ident.name, "laptop");
        assert_eq!(ident.kind, "key");
        let pem = ident.private_key.as_deref().expect("pem");
        assert!(pem.contains("BEGIN OPENSSH PRIVATE KEY"));
        let pub_line = ident.public_key.as_deref().expect("pub");
        assert!(pub_line.starts_with("ssh-ed25519 "));
        let fp = fingerprint_from_public_openssh(pub_line);
        assert!(fp.starts_with("SHA256:"), "fp={fp}");

        let key = russh::keys::decode_secret_key(pem, None).expect("decode");
        assert_eq!(fingerprint_of(key.public_key()), fp);
    }

    #[test]
    fn empty_label_is_rejected() {
        assert!(generate_ed25519_identity("  ").is_err());
    }
}
