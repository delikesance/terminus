use super::probe::classify_ssh_message;
use super::*;
use crate::error::Error;

#[test]
fn pty_defaults_are_usable() {
    let pty = SshPty::default();
    assert_eq!(pty.term, DEFAULT_TERM);
    assert_eq!((pty.cols, pty.rows), (80, 24));
    assert_eq!(SshPty::sized(120, 40).rows, 40);
}

#[test]
fn outcome_helpers_report_acceptance() {
    let known = HostKeyOutcome::Known {
        fingerprint: "SHA256:abc".into(),
    };
    assert!(known.accepted());
    assert_eq!(known.fingerprint(), Some("SHA256:abc"));
    let refused = HostKeyOutcome::Refused {
        reason: "unknown host".into(),
    };
    assert!(!refused.accepted());
    assert_eq!(refused.fingerprint(), None);
}

#[test]
fn unreachable_hosts_are_explained_in_plain_words() {
    let msg = |d: &str| ProbeError::Unreachable(d.into()).user_message();
    let refused = msg("Connection refused (os error 111)");
    assert!(refused.contains("port"), "{refused}");
    assert!(!refused.contains("os error"), "{refused}");
    let dns = msg("failed to lookup address information: Name or service not known");
    assert!(dns.contains("find that host"), "{dns}");
    let timeout = msg("connection timed out");
    assert!(timeout.contains("No answer"), "{timeout}");
    let route = msg("Network is unreachable (os error 101)");
    assert!(route.contains("network"), "{route}");
    // Unknown causes still show their detail.
    assert!(msg("weird thing").contains("weird thing"));
    for m in [&refused, &dns, &timeout, &route] {
        assert!(m.chars().count() <= 56, "fits two error lines: {m}");
    }
}

#[test]
fn probe_error_messages_cover_each_variant() {
    assert!(ProbeError::Unreachable(String::new())
        .user_message()
        .contains("unreachable"));
    assert!(ProbeError::AuthFailed
        .user_message()
        .contains("Authentication failed"));
    assert!(ProbeError::NoKey.user_message().contains("private key"));
    assert_eq!(
        ProbeError::GssapiNoTicket.user_message(),
        Error::GssapiNoTicket.to_string()
    );
    assert_eq!(
        ProbeError::GssapiUnsupported.user_message(),
        Error::GssapiUnsupported.to_string()
    );
    assert_eq!(ProbeError::Other("x".into()).user_message(), "x");
}

#[test]
fn classify_ssh_message_maps_common_failures() {
    assert!(matches!(
        classify_ssh_message("Connection refused"),
        ProbeError::Unreachable(_)
    ));
    assert!(matches!(
        classify_ssh_message("authentication refused for user root"),
        ProbeError::AuthFailed
    ));
    assert!(matches!(
        classify_ssh_message("no SSH private key found"),
        ProbeError::NoKey
    ));
}

#[test]
fn probe_error_from_typed_errors() {
    assert_eq!(
        ProbeError::from_error(Error::GssapiNoTicket),
        ProbeError::GssapiNoTicket
    );
    assert_eq!(
        ProbeError::from_error(Error::GssapiUnsupported),
        ProbeError::GssapiUnsupported
    );
    assert!(matches!(
        ProbeError::from_error(Error::TimeoutError("boom".into())),
        ProbeError::Unreachable(_)
    ));
}
