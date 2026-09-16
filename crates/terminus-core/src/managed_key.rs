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
    let pem = normalize_openssh_pem(pem);
    if pem.is_empty() {
        return Err(Error::IdentityKeyInvalid {
            reason: "private key PEM is empty".into(),
        });
    }

    let key = russh::keys::decode_secret_key(&pem, passphrase).map_err(|e| {
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
        private_key: Some(pem),
        passphrase: passphrase.map(str::to_string),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    })
}

/// Normalize clipboard / editor PEM noise so `decode_secret_key` can read it.
fn normalize_openssh_pem(raw: &str) -> String {
    let mut s = raw
        .trim()
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    // If the paste collapsed to a single line, re-wrap the base64 body.
    if s.contains("BEGIN") && s.contains("END") && !s.contains('\n') {
        if let Some(rewrapped) = rewrap_single_line_pem(&s) {
            return rewrapped;
        }
    }
    // Clipboard sometimes copies only the base64 body (no BEGIN/END).
    // `b3BlbnNzaC1rZXk` is base64 for `openssh-key-v1`.
    if !s.contains("BEGIN") {
        if let Some(wrapped) = wrap_openssh_body_if_needed(&s) {
            return wrapped;
        }
    }
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

fn wrap_openssh_body_if_needed(s: &str) -> Option<String> {
    let body: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if body.len() < 64 {
        return None;
    }
    let looks_openssh = body.starts_with("b3BlbnNzaC1rZXk");
    let looks_b64 = body
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
    if !looks_openssh || !looks_b64 {
        return None;
    }
    let mut out = String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n");
    for chunk in body.as_bytes().chunks(70) {
        out.push_str(std::str::from_utf8(chunk).ok()?);
        out.push('\n');
    }
    out.push_str("-----END OPENSSH PRIVATE KEY-----\n");
    Some(out)
}

fn rewrap_single_line_pem(s: &str) -> Option<String> {
    const BEGIN: &str = "-----BEGIN";
    const END: &str = "-----END";
    let b = s.find(BEGIN)?;
    let e = s.find(END)?;
    if e <= b {
        return None;
    }
    let after_begin = b + BEGIN.len();
    let header_close = s[after_begin..].find("-----")?;
    let header_end = after_begin + header_close + 5;
    if header_end >= e {
        return None;
    }
    let header = s[b..header_end].trim();
    let body: String = s[header_end..e].chars().filter(|c| !c.is_whitespace()).collect();
    let footer = s[e..].trim();
    let mut out = String::new();
    out.push_str(header);
    out.push('\n');
    for chunk in body.as_bytes().chunks(70) {
        out.push_str(std::str::from_utf8(chunk).ok()?);
        out.push('\n');
    }
    out.push_str(footer);
    out.push('\n');
    Some(out)
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

    #[test]
    fn import_accepts_headerless_openssh_body() {
        let generated = generate_ed25519_identity("tmp").expect("generate");
        let pem = generated.private_key.expect("pem");
        // Strip BEGIN/END lines — keep only the base64 body (common clipboard paste).
        let body: String = pem
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!body.contains("BEGIN"));
        assert!(body.trim_start().starts_with("b3BlbnNzaC1rZXk"));
        let imported =
            import_openssh_identity("from-body", &body, None).expect("import body");
        assert_eq!(imported.name, "from-body");
    }
}
