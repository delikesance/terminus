use super::test_support::*;
use super::*;
use std::time::{Duration, Instant};

/// Opening Files on a host the OS probe is still connecting to (a slow
/// or unreachable server) read its key behind that probe: the UI
/// thread waited 5 s, then showed "Timed out reading the stored SSH
/// key". The probe's network round-trip must not hold the worker.
#[test]
fn reading_a_key_does_not_wait_behind_a_slow_os_probe() {
    // Accepts TCP but never speaks SSH: the probe waits for a banner
    // until its connect timeout.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = silent.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for conn in silent.incoming() {
            held.push(conn);
        }
    });

    let dir = temp_dir("slow-probe");
    let host_id = {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let store = rt.block_on(Store::open(dir.clone())).unwrap();
        let ident = terminus_core::generate_ed25519_identity("slow").unwrap();
        rt.block_on(store.upsert_identity(&ident)).unwrap();
        let mut host = host_from_draft(&HostDraft {
            name: "slow box".into(),
            hostname: "127.0.0.1".into(),
            username: "nixos".into(),
            auth_method: "key".into(),
            identity_id: Some(ident.id.to_string()),
            ..HostDraft::default()
        });
        host.port = port;
        rt.block_on(store.upsert_host(&host)).unwrap();
        host.id.to_string()
    };

    let mut repo = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        !repo.loading()
    }));
    repo.detect_os(&host_id);
    // Let the worker pick the probe up first.
    std::thread::sleep(Duration::from_millis(300));

    let started = Instant::now();
    let key = repo.resolve_host_identity(&host_id);
    let waited = started.elapsed();
    assert!(matches!(key, Ok(Some(_))), "key: {key:?}");
    assert!(
        waited < Duration::from_secs(1),
        "the key read waited {waited:?} behind the probe"
    );

    drop(repo);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The SFTP open path asked for the key with a blocking 5 s
/// `recv_timeout` on the UI thread: behind a slow probe or sync on
/// the worker loop the window froze, then failed. The request must
/// return at once and the credentials arrive later through `drain`.
#[test]
fn sftp_credentials_arrive_later_while_the_worker_is_busy() {
    // Accepts TCP but never speaks SSH: a probe waits its timeout.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = silent.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for conn in silent.incoming() {
            held.push(conn);
        }
    });

    let dir = temp_dir("sftp-auth-async");
    let (host_id, ident_id) = {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let store = rt.block_on(Store::open(dir.clone())).unwrap();
        let ident = terminus_core::generate_ed25519_identity("e2e").unwrap();
        rt.block_on(store.upsert_identity(&ident)).unwrap();
        let host = host_from_draft(&HostDraft {
            name: "e2e local".into(),
            hostname: "127.0.0.1".into(),
            username: "nixos".into(),
            auth_method: "key".into(),
            identity_id: Some(ident.id.to_string()),
            ..HostDraft::default()
        });
        rt.block_on(store.upsert_host(&host)).unwrap();
        (host.id.to_string(), ident.id.to_string())
    };

    let mut repo = HostRepository::spawn(dir.clone(), None);
    assert!(drain_until(&mut repo, Duration::from_secs(10), |repo| {
        !repo.loading()
    }));
    // Occupy the worker loop with a probe that hangs.
    repo.probe_and_create(&HostDraft {
        name: "slow".into(),
        hostname: "127.0.0.1".into(),
        port: port.to_string(),
        username: "nixos".into(),
        auth_method: "key".into(),
        identity_id: Some(ident_id),
        ..HostDraft::default()
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));

    let started = Instant::now();
    repo.request_sftp_auth(&host_id, "key");
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "the request must not wait for the worker"
    );
    assert!(repo.take_sftp_auth_replies().is_empty());

    assert!(drain_until(&mut repo, Duration::from_secs(40), |repo| {
        !repo.sftp_auth_replies.is_empty()
    }));
    let replies = repo.take_sftp_auth_replies();
    assert_eq!(replies.len(), 1);
    let (id, auth) = &replies[0];
    assert_eq!(id, &host_id);
    assert!(
        matches!(auth, Ok((None, Some(_)))),
        "key auth: no password, the key: {auth:?}"
    );

    drop(repo);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sftp_auth_reads_only_what_the_auth_method_needs() {
    let pw = || Ok(Some("pw".to_string()));
    let key = || Ok(Some(("PEM".to_string(), None)));
    assert_eq!(
        sftp_auth_for("password", pw, key),
        Ok((Some("pw".into()), None))
    );
    assert_eq!(
        sftp_auth_for("key", || panic!("no password read"), key),
        Ok((None, Some(("PEM".into(), None))))
    );
    assert_eq!(
        sftp_auth_for("gssapi", || panic!(), || panic!()),
        Ok((None, None))
    );
    assert_eq!(
        sftp_auth_for("password", || Ok(None), key),
        Err(msg::NO_PASSWORD.to_string())
    );
    assert_eq!(
        sftp_auth_for("key", pw, || Ok(None)),
        Err(msg::NO_SSH_KEY.to_string())
    );
    assert_eq!(
        sftp_auth_for("key", pw, || Err("Unlock the vault".into())),
        Err("Unlock the vault".to_string())
    );
}
