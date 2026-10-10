use std::sync::{Arc, Mutex};

use russh::client;
use russh::keys::PublicKey;

use super::known_hosts_tests::temp_known_hosts;
use super::*;
use crate::auth_method::HostAuthMethod;
use crate::models::Host;

#[test]
fn connect_sftp_options_from_host_carry_auth() {
    let host = Host {
        id: uuid::Uuid::nil(),
        name: "demo".into(),
        hostname: "sftp.example".into(),
        port: 2222,
        username: "alice".into(),
        auth_method: "password".into(),
        password: Some("secret".into()),
        identity_id: None,
        group_id: None,
        tags: Vec::new(),
        notes: String::new(),
        os_id: None,
        sort_order: 0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deleted_at: None,
    };
    let opts = probe_options_from_host(&host, None);
    assert_eq!(opts.hostname, "sftp.example");
    assert_eq!(opts.port, 2222);
    assert_eq!(opts.auth.username, "alice");
    assert_eq!(opts.auth.password.as_deref(), Some("secret"));
    assert_eq!(opts.auth.method, Some(HostAuthMethod::Password));
}

#[tokio::test]
async fn connect_sftp_unreachable_host_errors() {
    let opts = SshConnectOptions {
        hostname: "127.0.0.1".into(),
        port: 1,
        auth: SshAuth {
            username: "nobody".into(),
            password: Some("x".into()),
            method: Some(HostAuthMethod::Password),
            ..SshAuth::default()
        },
        policy: HostKeyPolicy::AcceptAll,
        known_hosts: temp_known_hosts("sftp-unreach"),
        connect_timeout: Duration::from_millis(200),
        keepalive_interval: None,
    };
    let err = connect_sftp_for_host(&opts).await.expect_err("must fail");
    let msg = err.to_string().to_ascii_lowercase();
    assert!(
        msg.contains("timeout")
            || msg.contains("refused")
            || msg.contains("unreachable")
            || msg.contains("connection")
            || msg.contains("ssh"),
        "unexpected error: {err}"
    );
}

const RECORDED_KEY: &str =
    "AAAAC3NzaC1lZDI1NTE5AAAAIHAaua3lZIlkQMUKDa8Ix39k1KNRzX1FvOmM3finEvB9";
const OTHER_KEY: &str =
    "AAAAC3NzaC1lZDI1NTE5AAAAIJzqndyNUcW5Uxz66ydEvKfL5UfoVCGq0L/u4y6+0GJ9";

async fn check_with_policy(
    policy: HostKeyPolicy,
    recorded: Option<&str>,
    presented: &str,
    tag: &str,
) -> (bool, Option<HostKeyOutcome>) {
    let known_hosts = temp_known_hosts(tag);
    if let Some(key) = recorded {
        std::fs::write(known_hosts.path(), format!("h.test ssh-ed25519 {key}\n"))
            .unwrap();
    }
    let outcome = Arc::new(Mutex::new(None));
    let mut handler = ClientHandler {
        host: "h.test".into(),
        port: 22,
        policy,
        known_hosts: known_hosts.clone(),
        outcome: Arc::clone(&outcome),
    };
    let key = PublicKey::from_openssh(&format!("ssh-ed25519 {presented}")).unwrap();
    let accepted = client::Handler::check_server_key(&mut handler, &key)
        .await
        .unwrap();
    let _ = std::fs::remove_file(known_hosts.path());
    let decision = outcome.lock().unwrap().clone();
    (accepted, decision)
}

#[tokio::test]
async fn accept_all_rejects_a_changed_host_key() {
    let (accepted, decision) = check_with_policy(
        HostKeyPolicy::AcceptAll,
        Some(RECORDED_KEY),
        OTHER_KEY,
        "aa-changed",
    )
    .await;
    assert!(!accepted);
    assert!(matches!(decision, Some(HostKeyOutcome::Changed { .. })));
}

#[tokio::test]
async fn accept_all_accepts_unknown_and_matching_hosts() {
    let (unknown, _) =
        check_with_policy(HostKeyPolicy::AcceptAll, None, OTHER_KEY, "aa-unknown").await;
    let (matching, _) = check_with_policy(
        HostKeyPolicy::AcceptAll,
        Some(RECORDED_KEY),
        RECORDED_KEY,
        "aa-match",
    )
    .await;
    assert!(unknown && matching);
}
