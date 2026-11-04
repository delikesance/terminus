//! Cancel stops an upload between chunks and leaves nothing behind.

#[allow(dead_code)]
mod sftp_e2e_support;

use std::time::Duration;

use terminus_bridge::{SftpCommand, SftpEvent, SftpSide, SftpWorker};
use terminus_core::ssh::{HostKeyPolicy, KnownHosts, SshAuth, SshConnectOptions};

use sftp_e2e_support::{wait_closed, wait_event};

#[test]
fn cancelling_an_upload_stops_it_and_removes_the_partial_file() {
    let server = sftp_e2e_support::mock_server::try_spawn().expect("mock sftp");
    let scratch = std::env::temp_dir()
        .join(format!("terminus-sftp-cancel-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    // Sparse, so the test is cheap to set up and slow enough to interrupt.
    let big = std::fs::File::create(scratch.join("big.bin")).unwrap();
    big.set_len(2 * 1024 * 1024 * 1024).unwrap();
    drop(big);

    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::Connect {
        side: SftpSide::Right,
        opts: SshConnectOptions {
            hostname: "127.0.0.1".into(),
            port: server.port,
            auth: SshAuth {
                username: "test".into(),
                password: Some("test".into()),
                ..SshAuth::default()
            },
            policy: HostKeyPolicy::AcceptAll,
            known_hosts: KnownHosts::default(),
            connect_timeout: Duration::from_secs(5),
            keepalive_interval: None,
        },
    });
    wait_event(&worker, |e| matches!(e, SftpEvent::Ready { .. }));

    worker.send(SftpCommand::Transfer {
        from_side: SftpSide::Left,
        from_path: scratch.join("big.bin").to_string_lossy().into_owned(),
        to_side: SftpSide::Right,
        to_cwd: "/".into(),
        name: "big.bin".into(),
    });
    wait_event(
        &worker,
        |e| matches!(e, SftpEvent::TransferProgress { done, .. } if *done > 0),
    );
    worker.send(SftpCommand::CancelTransfer);
    wait_event(&worker, |e| matches!(e, SftpEvent::Cancelled));

    let leftovers: Vec<_> = std::fs::read_dir(&server.root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("big.bin"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "partial upload left behind: {leftovers:?}"
    );

    worker.send(SftpCommand::Close);
    wait_closed(&worker);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&scratch);
}
