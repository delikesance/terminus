use std::sync::{Arc, Mutex};
use tracing::warn;

use russh::client;
use russh::keys::PublicKey;

use super::{fingerprint_of, HostKeyOutcome, HostKeyPolicy, KnownHosts};
use crate::error::{Error, Result};

/// russh client handler: verifies the server key and records the decision.
pub struct ClientHandler {
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) policy: HostKeyPolicy,
    pub(super) known_hosts: KnownHosts,
    pub(super) outcome: Arc<Mutex<Option<HostKeyOutcome>>>,
}

impl client::Handler for ClientHandler {
    type Error = Error;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool> {
        let fingerprint = fingerprint_of(server_public_key);
        let recorded = self.known_hosts.lookup(&self.host, self.port);

        let decision = if matches!(self.policy, HostKeyPolicy::Probe) {
            // Capture-only: the caller first probes, shows the user the
            // fingerprint, then reconnects with `ApproveFingerprint`.
            HostKeyOutcome::Unknown { fingerprint }
        } else if recorded.is_empty() {
            match &self.policy {
                HostKeyPolicy::Strict => HostKeyOutcome::Refused {
                    reason: format!(
                        "{host}:{port} is not in {} (policy: strict)",
                        self.known_hosts.path().display(),
                        host = self.host,
                        port = self.port
                    ),
                },
                HostKeyPolicy::ApproveFingerprint(approved) if approved == &fingerprint => {
                    HostKeyOutcome::Approved { fingerprint }
                }
                HostKeyPolicy::ApproveFingerprint(approved) => HostKeyOutcome::Refused {
                    reason: format!("approved fingerprint was {approved}, server presented {fingerprint}"),
                },
                HostKeyPolicy::Tofu => HostKeyOutcome::Recorded { fingerprint },
                HostKeyPolicy::AcceptAll => HostKeyOutcome::Known { fingerprint },
                HostKeyPolicy::Probe => unreachable!("handled above"),
            }
        } else {
            let body = server_public_key
                .to_openssh()
                .ok()
                .and_then(|k| k.split_whitespace().nth(1).map(str::to_string))
                .unwrap_or_default();
            if recorded.iter().any(|e| e.key == body) {
                HostKeyOutcome::Known { fingerprint }
            } else {
                let expected = recorded
                    .first()
                    .map(|e| {
                        format!(
                            "{}:{}",
                            e.algorithm,
                            e.key.chars().take(24).collect::<String>()
                        )
                    })
                    .unwrap_or_default();
                HostKeyOutcome::Changed {
                    expected,
                    presented: fingerprint,
                }
            }
        };

        let accepted = decision.accepted();
        if accepted {
            if matches!(
                decision,
                HostKeyOutcome::Approved { .. } | HostKeyOutcome::Recorded { .. }
            ) {
                if let Err(err) =
                    self.known_hosts
                        .record(&self.host, self.port, server_public_key)
                {
                    warn!(error = %err, host = %self.host, "could not persist approved host key");
                }
            }
        } else {
            warn!(host = %self.host, "host key rejected");
        }

        if let Ok(mut slot) = self.outcome.lock() {
            *slot = Some(decision);
        }
        Ok(accepted)
    }
}

pub(super) fn host_key_aware_error(
    err: Error,
    outcome: &Arc<Mutex<Option<HostKeyOutcome>>>,
) -> Error {
    let decision = outcome.lock().ok().and_then(|o| o.clone());
    match decision {
        Some(HostKeyOutcome::Changed { expected, presented }) => Error::SshError(format!(
            "host key changed: expected {expected}, got {presented} — verify the server before trusting it"
        )),
        Some(HostKeyOutcome::Refused { reason }) => Error::SshError(format!("host key refused: {reason}")),
        Some(HostKeyOutcome::Unknown { fingerprint }) => Error::SshError(format!(
            "unknown host key {fingerprint} — approval required"
        )),
        _ => err,
    }
}
