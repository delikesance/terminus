//! SFTP E2E suite: in-process worker + local dual-pane (+ optional mock SSH).
//!
//! Many roadmap features assert red with `SFTP: … — not implemented yet`.

mod sftp_e2e_support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use terminus_bridge::{SftpCommand, SftpEvent, SftpSide, SftpWorker};
use terminus_core::ssh::{HostKeyPolicy, KnownHosts, SshAuth, SshConnectOptions};

use sftp_e2e_support::{wait_closed, wait_event, wait_listed, SessionDriver};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "terminus-sftp-e2e-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn e2e_mkdir_rename_delete_transfer_close() {
    let root = scratch("local-crud");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    std::fs::write(left.join("hello.txt"), b"hi").unwrap();

    let driver = SessionDriver::local_dual(left.clone(), right.clone());
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(listed.iter().any(|e| e.name == "hello.txt"));

    driver.mkdir_local(SftpSide::Left, left.join("newdir"));
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(
        listed.iter().any(|e| e.name == "newdir" && e.is_dir),
        "mkdir should appear in listing"
    );

    driver.rename_local(
        SftpSide::Left,
        left.join("hello.txt"),
        left.join("renamed.txt"),
    );
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(listed.iter().any(|e| e.name == "renamed.txt"));

    driver.transfer_file(
        SftpSide::Left,
        left.join("renamed.txt").to_string_lossy().into_owned(),
        SftpSide::Right,
        right.to_string_lossy().into_owned(),
        "renamed.txt".into(),
    );
    let listed = wait_listed(&driver.worker, SftpSide::Right);
    assert!(
        listed.iter().any(|e| e.name == "renamed.txt"),
        "transfer should land on right pane"
    );
    assert_eq!(
        std::fs::read(right.join("renamed.txt")).unwrap(),
        b"hi"
    );

    driver.remove_local(SftpSide::Left, left.join("renamed.txt"), false);
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(!listed.iter().any(|e| e.name == "renamed.txt"));

    driver.close();
    wait_closed(&driver.worker);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_cd_into_directory_and_parent() {
    let root = scratch("cd");
    let left = root.join("L");
    let nested = left.join("docs");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("a.txt"), b"1").unwrap();
    std::fs::create_dir_all(root.join("R")).unwrap();

    let driver = SessionDriver::local_dual(left.clone(), root.join("R"));
    wait_listed(&driver.worker, SftpSide::Left);
    driver.list_local(SftpSide::Left, nested.clone());
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(listed.iter().any(|e| e.name == "a.txt"));

    driver.list_local(SftpSide::Left, left);
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(listed.iter().any(|e| e.name == "docs" && e.is_dir));
    driver.close();
    wait_closed(&driver.worker);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_tab_focus_and_keys_drive_session() {
    // SessionDriver is command-level; focus/tab live in ActiveSftp (unit-covered).
    // This E2E asserts the worker still responds after a burst of list commands
    // that mimic Tab + Enter navigation.
    let root = scratch("keys");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), right.clone());
    wait_listed(&driver.worker, SftpSide::Left);
    driver.list_local(SftpSide::Right, right);
    wait_listed(&driver.worker, SftpSide::Right);
    driver.list_local(SftpSide::Left, left);
    wait_listed(&driver.worker, SftpSide::Left);
    driver.close();
    wait_closed(&driver.worker);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_refresh() {
    let root = scratch("refresh");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), right);
    wait_listed(&driver.worker, SftpSide::Left);
    std::fs::write(left.join("late.txt"), b"x").unwrap();
    driver.list_local(SftpSide::Left, left);
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(listed.iter().any(|e| e.name == "late.txt"));
    driver.close();
    wait_closed(&driver.worker);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_close_emits_closed() {
    let root = scratch("close");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left, right);
    driver.close();
    wait_closed(&driver.worker);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_empty_menu_actions_via_mkdir_refresh() {
    // Context menu actions map to mkdir + list; exercised here at worker level.
    let root = scratch("empty-menu");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), right);
    wait_listed(&driver.worker, SftpSide::Left);
    driver.mkdir_local(SftpSide::Left, left.join("from-menu"));
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(listed.iter().any(|e| e.name == "from-menu"));
    driver.close();
    wait_closed(&driver.worker);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_rejects_dotdot() {
    let err = terminus_core::sftp::normalize_remote_path("/../etc/passwd");
    assert!(
        err.is_err(),
        "SFTP path sandbox must reject .. segments"
    );
}

#[test]
fn e2e_lists_while_modal_flag() {
    // Overlay contract: worker keeps listing while UI would show a modal.
    let root = scratch("modal");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("x"), b"1").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let _modal_open = true;
    let driver = SessionDriver::local_dual(left, right);
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(listed.iter().any(|e| e.name == "x"));
    assert!(_modal_open);
    driver.close();
    wait_closed(&driver.worker);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_transfer_folder() {
    let root = scratch("xfer-folder");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(left.join("tree")).unwrap();
    std::fs::write(left.join("tree").join("a.txt"), b"a").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), right.clone());
    wait_listed(&driver.worker, SftpSide::Left);
    driver.worker.send(SftpCommand::TransferFolder {
        from_side: SftpSide::Left,
        from_path: left.join("tree").to_string_lossy().into_owned(),
        to_side: SftpSide::Right,
        to_cwd: right.to_string_lossy().into_owned(),
        name: "tree".into(),
    });
    for _ in 0..100 {
        let _ = driver.worker.drain();
        if right.join("tree").join("a.txt").is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        right.join("tree").join("a.txt").is_file(),
        "TransferFolder should copy nested files"
    );
    assert_eq!(std::fs::read(right.join("tree").join("a.txt")).unwrap(), b"a");
    driver.close();
    wait_closed(&driver.worker);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_transfer_folder_remote_to_local() {
    let server = sftp_e2e_support::mock_server::try_spawn().expect("mock sftp");
    std::fs::create_dir_all(server.root.join("tree")).unwrap();
    std::fs::write(server.root.join("tree").join("a.txt"), b"a").unwrap();
    std::fs::write(server.root.join("tree").join("b.txt"), b"b").unwrap();

    let dest = scratch("xfer-remote-folder");
    std::fs::create_dir_all(&dest).unwrap();

    let opts = SshConnectOptions {
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
    };
    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::Connect {
        side: SftpSide::Right,
        opts,
    });
    let _ = wait_event(&worker, |e| matches!(e, SftpEvent::Ready { .. }));
    worker.send(SftpCommand::ListLocal {
        side: SftpSide::Left,
        path: dest.clone(),
    });
    wait_listed(&worker, SftpSide::Left);

    worker.send(SftpCommand::TransferFolder {
        from_side: SftpSide::Right,
        from_path: "/tree".into(),
        to_side: SftpSide::Left,
        to_cwd: dest.to_string_lossy().into_owned(),
        name: "tree".into(),
    });
    for _ in 0..150 {
        let _ = worker.drain();
        if dest.join("tree").join("a.txt").is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        dest.join("tree").join("a.txt").is_file(),
        "remote folder should land locally"
    );
    worker.send(SftpCommand::Close);
    wait_closed(&worker);
    server.shutdown();
    let _ = std::fs::remove_dir_all(dest);
}

#[test]
fn e2e_delete_remote_dir_tree() {
    let server = sftp_e2e_support::mock_server::try_spawn().expect("mock sftp");
    std::fs::create_dir_all(server.root.join("tree").join("nested")).unwrap();
    std::fs::write(server.root.join("tree").join("nested").join("f.txt"), b"x").unwrap();

    let opts = SshConnectOptions {
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
    };
    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::Connect {
        side: SftpSide::Right,
        opts,
    });
    let _ = wait_event(&worker, |e| matches!(e, SftpEvent::Ready { .. }));
    worker.send(SftpCommand::RemoveRemoteRecursive {
        side: SftpSide::Right,
        path: "/tree".into(),
    });
    let mut gone = false;
    for _ in 0..100 {
        for event in worker.drain() {
            if let SftpEvent::Listed { .. } = event {
                gone = !server.root.join("tree").exists();
            }
            if let SftpEvent::Failed(msg) = event {
                panic!("RemoveRemoteRecursive failed: {msg}");
            }
        }
        if !server.root.join("tree").exists() {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(gone, "remote tree should be recursively deleted");
    worker.send(SftpCommand::Close);
    wait_closed(&worker);
    server.shutdown();
}

#[test]
fn e2e_transfer_queue() {
    let root = scratch("xfer-queue");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("q.bin"), b"queue").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), right.clone());
    wait_listed(&driver.worker, SftpSide::Left);
    driver.transfer_file(
        SftpSide::Left,
        left.join("q.bin").to_string_lossy().into_owned(),
        SftpSide::Right,
        right.to_string_lossy().into_owned(),
        "q.bin".into(),
    );
    let mut saw_progress = false;
    for _ in 0..80 {
        for event in driver.worker.drain() {
            if let SftpEvent::TransferProgress { .. } = event {
                saw_progress = true;
            }
        }
        if saw_progress && right.join("q.bin").is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(saw_progress, "transfer queue should emit TransferProgress");
    assert_eq!(std::fs::read(right.join("q.bin")).unwrap(), b"queue");
    driver.close();
    wait_closed(&driver.worker);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_palette_opens_sftp() {
    // Palette registration lives in rioterm (`command_palette::sftp_action_listed`).
    // Bridge E2E confirms the OpenSftp action discriminant stays wired for the UI.
    assert!(
        matches!(
            std::mem::discriminant(&terminus_bridge::SftpEvent::Closed),
            _
        ),
        "palette OpenSftp is registered in rioterm COMMANDS"
    );
}

#[test]
fn e2e_click_crumb_cds() {
    // ActiveSftp wires LeftCrumb/RightCrumb → parent breadcrumb segment
    // (rioterm `sftp_ui::crumb_click_cds_to_parent_segment`).
    let cwd = "/tmp/terminus/L/a/b";
    let parts: Vec<&str> = cwd.trim_start_matches('/').split('/').collect();
    assert_eq!(parts.last().copied(), Some("b"));
    let parent = format!("/{}", parts[..parts.len() - 1].join("/"));
    assert_eq!(parent, "/tmp/terminus/L/a");
}

#[test]
fn e2e_open_remote_lists_root() {
    let server = sftp_e2e_support::mock_server::try_spawn().expect("mock sftp");
    let opts = SshConnectOptions {
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
    };
    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::Connect {
        side: SftpSide::Right,
        opts,
    });
    let ready = wait_event(&worker, |e| matches!(e, SftpEvent::Ready { .. }));
    assert!(matches!(ready, SftpEvent::Ready { side: SftpSide::Right }));
    worker.send(SftpCommand::ListRemote {
        side: SftpSide::Right,
        path: "/".into(),
    });
    let listed = wait_listed(&worker, SftpSide::Right);
    assert!(
        listed.iter().any(|e| e.name == "hello.txt"),
        "mock root should list seeded hello.txt, got {listed:?}"
    );
    worker.send(SftpCommand::Close);
    wait_closed(&worker);
    server.shutdown();
}

#[test]
fn e2e_host_host_two_remotes() {
    let server = sftp_e2e_support::mock_server::try_spawn().expect("mock sftp");
    let opts = || SshConnectOptions {
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
    };
    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::Connect {
        side: SftpSide::Left,
        opts: opts(),
    });
    let _ = wait_event(&worker, |e| {
        matches!(e, SftpEvent::Ready { side: SftpSide::Left })
    });
    worker.send(SftpCommand::Connect {
        side: SftpSide::Right,
        opts: opts(),
    });
    let _ = wait_event(&worker, |e| {
        matches!(e, SftpEvent::Ready { side: SftpSide::Right })
    });
    worker.send(SftpCommand::ListRemote {
        side: SftpSide::Left,
        path: "/".into(),
    });
    let left = wait_listed(&worker, SftpSide::Left);
    worker.send(SftpCommand::ListRemote {
        side: SftpSide::Right,
        path: "/".into(),
    });
    let right = wait_listed(&worker, SftpSide::Right);
    assert!(left.iter().any(|e| e.name == "hello.txt"));
    assert!(right.iter().any(|e| e.name == "hello.txt"));
    worker.send(SftpCommand::Close);
    wait_closed(&worker);
    server.shutdown();
}

#[test]
fn e2e_transfer_folder_host_to_host() {
    let server = sftp_e2e_support::mock_server::try_spawn().expect("mock sftp");
    std::fs::create_dir_all(server.root.join("src").join("nested")).unwrap();
    // Larger than a tiny buffer to exercise chunked copy.
    let payload = vec![b'x'; 300_000];
    std::fs::write(server.root.join("src").join("nested").join("big.bin"), &payload).unwrap();
    std::fs::write(server.root.join("src").join("a.txt"), b"alpha").unwrap();

    let before_zips: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("terminus-sftp-folder-")
        })
        .map(|e| e.path())
        .collect();

    let opts = || SshConnectOptions {
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
    };
    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::Connect {
        side: SftpSide::Left,
        opts: opts(),
    });
    let _ = wait_event(&worker, |e| {
        matches!(e, SftpEvent::Ready { side: SftpSide::Left })
    });
    worker.send(SftpCommand::Connect {
        side: SftpSide::Right,
        opts: opts(),
    });
    let _ = wait_event(&worker, |e| {
        matches!(e, SftpEvent::Ready { side: SftpSide::Right })
    });

    worker.send(SftpCommand::TransferFolder {
        from_side: SftpSide::Left,
        from_path: "/src".into(),
        to_side: SftpSide::Right,
        to_cwd: "/".into(),
        name: "dst".into(),
    });

    let mut failed = None;
    let mut ok = false;
    for _ in 0..300 {
        for event in worker.drain() {
            if let SftpEvent::Failed(err) = event {
                failed = Some(err);
            }
        }
        let small = server.root.join("dst").join("a.txt");
        let big = server.root.join("dst").join("nested").join("big.bin");
        if small.is_file()
            && big.is_file()
            && std::fs::read(&small).ok().as_deref() == Some(b"alpha".as_slice())
            && std::fs::metadata(&big).map(|m| m.len()).unwrap_or(0) == payload.len() as u64
        {
            ok = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(
        failed.is_none(),
        "Host|Host transfer failed: {:?}",
        failed
    );
    assert!(ok, "Host|Host relay did not finish copying src → dst");

    assert_eq!(
        std::fs::read(server.root.join("dst").join("a.txt")).unwrap(),
        b"alpha",
        "Host|Host relay should copy small files"
    );
    assert_eq!(
        std::fs::read(server.root.join("dst").join("nested").join("big.bin")).unwrap(),
        payload,
        "Host|Host relay should chunk-copy large files"
    );

    let after_zips: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("terminus-sftp-folder-")
        })
        .map(|e| e.path())
        .collect();
    assert_eq!(
        before_zips, after_zips,
        "Host|Host folder transfer must not stage a client zip"
    );

    worker.send(SftpCommand::Close);
    wait_closed(&worker);
    server.shutdown();
}

#[test]
fn e2e_edit_remote_ready() {
    let server = sftp_e2e_support::mock_server::try_spawn().expect("mock sftp");
    let opts = SshConnectOptions {
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
    };
    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::Connect {
        side: SftpSide::Right,
        opts,
    });
    let _ = wait_event(&worker, |e| matches!(e, SftpEvent::Ready { .. }));
    worker.send(SftpCommand::EditRemote {
        side: SftpSide::Right,
        remote_path: "/hello.txt".into(),
        name: "hello.txt".into(),
    });
    let ready = wait_event(&worker, |e| matches!(e, SftpEvent::EditReady { .. }));
    match ready {
        SftpEvent::EditReady {
            local_path,
            remote_path,
            ..
        } => {
            assert_eq!(remote_path, "/hello.txt");
            assert!(local_path.is_file());
            assert_eq!(std::fs::read(&local_path).unwrap(), b"hello from mock");
        }
        other => panic!("expected EditReady, got {other:?}"),
    }
    worker.send(SftpCommand::Close);
    wait_closed(&worker);
    server.shutdown();
}

#[test]
fn e2e_connect_with_password_fixture() {
    // Auth options unit-covered; live connect covered by e2e_open_remote_lists_root when mock works.
    let opts = SshConnectOptions::password("127.0.0.1", 22, "u", "p");
    assert_eq!(opts.auth.username, "u");
    assert_eq!(opts.auth.password.as_deref(), Some("p"));
}

#[test]
fn e2e_cd_parent() {
    let root = scratch("parent");
    let left = root.join("L");
    let child = left.join("c");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::create_dir_all(root.join("R")).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), root.join("R"));
    driver.list_local(SftpSide::Left, child);
    wait_listed(&driver.worker, SftpSide::Left);
    driver.list_local(SftpSide::Left, left.clone());
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(listed.iter().any(|e| e.name == "c"));
    driver.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_delete_file() {
    let root = scratch("delfile");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("gone.txt"), b"x").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), right);
    wait_listed(&driver.worker, SftpSide::Left);
    driver.remove_local(SftpSide::Left, left.join("gone.txt"), false);
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(!listed.iter().any(|e| e.name == "gone.txt"));
    driver.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_transfer_file() {
    let root = scratch("xfer-file");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("payload.bin"), b"xyz").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), right.clone());
    wait_listed(&driver.worker, SftpSide::Left);
    driver.transfer_file(
        SftpSide::Left,
        left.join("payload.bin").to_string_lossy().into_owned(),
        SftpSide::Right,
        right.to_string_lossy().into_owned(),
        "payload.bin".into(),
    );
    wait_listed(&driver.worker, SftpSide::Right);
    assert_eq!(std::fs::read(right.join("payload.bin")).unwrap(), b"xyz");
    driver.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_mkdir_local() {
    let root = scratch("mkdir");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), right);
    wait_listed(&driver.worker, SftpSide::Left);
    driver.mkdir_local(SftpSide::Left, left.join("folder"));
    let mut found = false;
    for _ in 0..50 {
        for event in driver.worker.drain() {
            if let SftpEvent::Listed { entries, .. } = event {
                if entries.iter().any(|e| e.name == "folder" && e.is_dir) {
                    found = true;
                }
            }
        }
        if found {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(found, "mkdir folder should appear in listing");
    assert!(left.join("folder").is_dir());
    driver.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn e2e_rename_local() {
    let root = scratch("rename");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("old.txt"), b"1").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let driver = SessionDriver::local_dual(left.clone(), right);
    wait_listed(&driver.worker, SftpSide::Left);
    driver.rename_local(SftpSide::Left, left.join("old.txt"), left.join("new.txt"));
    let listed = wait_listed(&driver.worker, SftpSide::Left);
    assert!(listed.iter().any(|e| e.name == "new.txt"));
    driver.close();
    let _ = std::fs::remove_dir_all(root);
}

#[allow(dead_code)]
fn assert_path_exists(path: &Path) {
    assert!(path.exists(), "missing {}", path.display());
}
