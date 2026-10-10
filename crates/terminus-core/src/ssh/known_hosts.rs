use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use tracing::info;

use russh::keys::{HashAlg, PublicKey};

/// Path of the OpenSSH `known_hosts` file used for TOFU.
///
/// Honours `$SSH_KNOWN_HOSTS` for tests and sandboxes, then falls back to
/// `~/.ssh/known_hosts`.
pub fn default_known_hosts_path() -> PathBuf {
    if let Some(p) = std::env::var_os("SSH_KNOWN_HOSTS") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
        .join("known_hosts")
}

/// `SHA256:…` fingerprint of a public key, the format shown in TOFU modals and
/// by `ssh-keygen -lf`.
pub fn fingerprint_of(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// What to do with the key a server presents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Trust on first use: record unknown keys, reject a key that *changed*
    /// (that is the man-in-the-middle case worth stopping on).
    Tofu,
    /// Same as [`HostKeyPolicy::Tofu`], but an unknown key is only accepted —
    /// and recorded — when its fingerprint matches the one the user approved
    /// in the TOFU modal.
    ApproveFingerprint(String),
    /// Never record anything; accept only hosts already present in
    /// `known_hosts`.
    Strict,
    /// Never accept: capture the presented fingerprint and refuse, so the UI
    /// can run a trust-on-first-use modal and reconnect with
    /// [`HostKeyPolicy::ApproveFingerprint`] once the user has decided.
    Probe,
    /// Accept any key, record nothing. Tests and throwaway containers only.
    AcceptAll,
}

/// Outcome of the host-key check, surfaced to the UI after a connection
/// attempt (accepted, recorded, changed or refused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyOutcome {
    /// The presented key was already trusted.
    Known {
        /// `SHA256:…` fingerprint of the accepted key.
        fingerprint: String,
    },
    /// The key was unknown and got recorded (first use).
    Recorded {
        /// `SHA256:…` fingerprint written to `known_hosts`.
        fingerprint: String,
    },
    /// The user approved this exact fingerprint in the TOFU modal.
    Approved {
        /// `SHA256:…` fingerprint of the accepted key.
        fingerprint: String,
    },
    /// The server presented a key different from the recorded one.
    Changed {
        /// Fingerprint currently recorded in `known_hosts`.
        expected: String,
        /// Fingerprint actually presented by the server.
        presented: String,
    },
    /// Verification failed (unknown host under `Strict`, refusal, bad file).
    Refused {
        /// Human-readable reason, safe to show in a modal.
        reason: String,
    },
    /// [`HostKeyPolicy::Probe`] captured the presented key and refused it, so
    /// the UI can ask the user. Nothing was written to `known_hosts`.
    Unknown {
        /// `SHA256:…` fingerprint the server presented.
        fingerprint: String,
    },
}

impl HostKeyOutcome {
    /// Whether the key was accepted.
    pub const fn accepted(&self) -> bool {
        matches!(
            self,
            HostKeyOutcome::Known { .. }
                | HostKeyOutcome::Recorded { .. }
                | HostKeyOutcome::Approved { .. }
        )
    }

    /// The accepted key fingerprint, when there is one.
    ///
    /// [`HostKeyOutcome::Unknown`] reports the *presented* fingerprint: the key
    /// was refused, but the modal needs it to ask the user.
    pub fn fingerprint(&self) -> Option<&str> {
        match self {
            HostKeyOutcome::Known { fingerprint }
            | HostKeyOutcome::Recorded { fingerprint }
            | HostKeyOutcome::Approved { fingerprint } => Some(fingerprint),
            HostKeyOutcome::Changed { presented, .. } => Some(presented),
            HostKeyOutcome::Unknown { fingerprint } => Some(fingerprint),
            HostKeyOutcome::Refused { .. } => None,
        }
    }
}

/// A single `known_hosts` entry: which key we trust for a `host:port`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownHostEntry {
    /// Key algorithm as written by OpenSSH (`ssh-ed25519`, `rsa-sha2-512`, …).
    pub algorithm: String,
    /// Base64 body of the public key.
    pub key: String,
}

/// OpenSSH-compatible `known_hosts` reader/writer (TOFU storage).
#[derive(Debug, Clone)]
pub struct KnownHosts {
    path: PathBuf,
}

impl Default for KnownHosts {
    fn default() -> Self {
        Self::at(default_known_hosts_path())
    }
}

impl KnownHosts {
    /// Uses `path` as the backing file (it need not exist yet).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The backing file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `known_hosts` name for `host:port`: bare host on 22, `[host]:port`
    /// otherwise — exactly what OpenSSH writes.
    pub fn entry_name(host: &str, port: u16) -> String {
        if port == 22 {
            host.to_string()
        } else {
            format!("[{host}]:{port}")
        }
    }

    /// Looks up every key recorded for `host:port`.
    pub fn lookup(&self, host: &str, port: u16) -> Vec<KnownHostEntry> {
        let name = Self::entry_name(host, port);
        let Ok(contents) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for line in contents.lines() {
            let line = line.trim();
            // `@revoked` / `@cert-authority` markers are not plain trust
            // entries; never treat their keys as the host's key.
            if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let (Some(hosts), Some(algorithm), Some(key)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let matches = if hosts.starts_with("|1|") {
                hashed_host_matches(hosts, &name)
            } else {
                hosts.split(',').any(|h| h.trim() == name)
            };
            if matches {
                found.push(KnownHostEntry {
                    algorithm: algorithm.to_string(),
                    key: key.to_string(),
                });
            }
        }
        found
    }

    /// Appends `key` for `host:port`, creating `~/.ssh` and the file when
    /// needed. Idempotent: an identical entry is never written twice.
    pub fn record(&self, host: &str, port: u16, key: &PublicKey) -> Result<()> {
        let name = Self::entry_name(host, port);
        let openssh = key
            .to_openssh()
            .map_err(|e| Error::SshError(format!("cannot encode host key: {e}")))?;
        let mut fields = openssh.split_whitespace();
        let algorithm = fields.next().unwrap_or("ssh-ed25519");
        let body = fields.next().unwrap_or_default();
        if body.is_empty() {
            return Err(Error::SshError("host key has no base64 body".into()));
        }

        if self
            .lookup(host, port)
            .iter()
            .any(|e| e.algorithm == algorithm && e.key == body)
        {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut line = format!("{name} {algorithm} {body}\n");
        if let Ok(existing) = std::fs::read_to_string(&self.path) {
            if !existing.is_empty() && !existing.ends_with('\n') {
                line.insert(0, '\n');
            }
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.sync_data()?;
        info!(host = %name, algorithm, "recorded host key in known_hosts");
        Ok(())
    }
}

/// `|1|<salt>|<hash>` entry (OpenSSH `HashKnownHosts`): the hash is
/// HMAC-SHA1 of the entry name keyed with the salt, both base64.
fn hashed_host_matches(field: &str, name: &str) -> bool {
    use base64::Engine as _;
    use hmac::{Hmac, Mac};
    let mut parts = field.trim_start_matches("|1|").splitn(2, '|');
    let (Some(salt), Some(hash)) = (parts.next(), parts.next()) else {
        return false;
    };
    let b64 = base64::engine::general_purpose::STANDARD;
    let (Ok(salt), Ok(hash)) = (b64.decode(salt), b64.decode(hash)) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<sha1::Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(name.as_bytes());
    mac.verify_slice(&hash).is_ok()
}

/// Host-key algorithms to offer, those already recorded for the host first.
///
/// OpenSSH does the same: when `known_hosts` holds, say, the server's ECDSA
/// key, it asks for ECDSA so the server presents the key it can verify.
/// Offering russh's default order instead makes the server pick ed25519 and
/// the check reports a *changed* key: a false man-in-the-middle alarm.
pub fn preferred_host_key_algorithms(
    known: &[KnownHostEntry],
) -> Vec<russh::keys::Algorithm> {
    use std::str::FromStr;
    let known: Vec<russh::keys::Algorithm> = known
        .iter()
        .filter_map(|e| russh::keys::Algorithm::from_str(&e.algorithm).ok())
        .collect();
    let is_known = |algo: &russh::keys::Algorithm| {
        known
            .iter()
            .any(|k| k == algo || (is_rsa(k) && is_rsa(algo)))
    };
    let mut order = russh::Preferred::DEFAULT.key.to_vec();
    order.sort_by_key(|algo| !is_known(algo));
    order
}

pub(super) fn is_rsa(algo: &russh::keys::Algorithm) -> bool {
    matches!(algo, russh::keys::Algorithm::Rsa { .. })
}
