use super::*;
use std::time::Duration;

#[test]
fn a_refused_connection_names_its_pane_in_plain_words() {
    // Port 1 on loopback: nothing listens there.
    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::Connect {
        side: SftpSide::Right,
        opts: terminus_core::SshConnectOptions::password("127.0.0.1", 1, "u", "p"),
    });
    let mut failure = None;
    for _ in 0..100 {
        for event in worker.drain() {
            if let SftpEvent::ConnectFailed { side, message } = event {
                failure = Some((side, message));
            }
        }
        if failure.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let (side, message) = failure.expect("connect failure reported");
    assert_eq!(side, SftpSide::Right);
    assert!(message.contains("port"), "{message}");
    assert!(!message.contains("os error"), "{message}");
}

#[test]
fn local_list_command_routes_without_remote() {
    let worker = SftpWorker::spawn(None);
    let dir =
        std::env::temp_dir().join(format!("terminus-sftp-worker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), b"hi").unwrap();

    worker.send(SftpCommand::ListLocal {
        side: SftpSide::Left,
        path: dir.clone(),
    });

    let mut listed = None;
    for _ in 0..50 {
        for event in worker.drain() {
            if let SftpEvent::Listed {
                side: SftpSide::Left,
                path,
                entries,
            } = event
            {
                listed = Some((path, entries));
            }
        }
        if listed.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let (path, entries) = listed.expect("local list event");
    assert_eq!(Path::new(&path), dir.as_path());
    assert!(entries.iter().any(|e| e.name == "a.txt" && !e.is_dir));

    worker.send(SftpCommand::Close);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parent_remote_strips_last_segment() {
    assert_eq!(parent_remote("/home/alice/file"), "/home/alice");
    assert_eq!(parent_remote("/home"), "/");
    assert_eq!(parent_remote("/"), "/");
}

#[test]
fn join_remote_appends_name() {
    assert_eq!(
        join_remote("/home/alice", "file.txt"),
        "/home/alice/file.txt"
    );
    assert_eq!(join_remote("/", "file.txt"), "/file.txt");
    assert_eq!(join_remote("/home/", "file.txt"), "/home/file.txt");
}

#[test]
fn parse_probe_stdout_reads_flags() {
    let env =
        parse_probe_stdout("tar=1 zip=0 unzip=1 ps=0 family=unix arch=x86_64 tmp=/tmp\n");
    assert!(env.tools.tar && !env.tools.zip && env.tools.unzip);
    assert_eq!(env.family, RemoteFamily::Unix);
    assert_eq!(env.tmp, "/tmp");
    assert_eq!(env.arch, "x86_64");

    let none = parse_probe_stdout(
        "tar=0 zip=0 unzip=0 ps=0 family=unix arch=aarch64 tmp=/var/tmp",
    );
    assert!(!none.tools.tar && !none.tools.zip);
    assert_eq!(none.arch, "aarch64");

    let win = parse_probe_stdout(
        "tar=0 zip=0 unzip=0 ps=1 family=windows arch=AMD64 tmp=C:\\Users\\a\\AppData\\Local\\Temp",
    );
    assert!(win.tools.powershell);
    assert_eq!(win.family, RemoteFamily::Windows);
    assert_eq!(win.arch, "x86_64");
}

#[test]
fn choose_upload_prefers_tar_when_unzip_missing() {
    let nix_like = RemoteArchiveTools {
        tar: true,
        zip: false,
        unzip: false,
        powershell: false,
    };
    assert_eq!(
        choose_upload_archive_kind(&nix_like, RemoteFamily::Unix).unwrap(),
        ArchiveKind::TarGz
    );
    let unzip_only = RemoteArchiveTools {
        tar: false,
        zip: true,
        unzip: true,
        powershell: false,
    };
    assert_eq!(
        choose_upload_archive_kind(&unzip_only, RemoteFamily::Unix).unwrap(),
        ArchiveKind::Zip
    );
    let none = RemoteArchiveTools::none();
    assert!(choose_upload_archive_kind(&none, RemoteFamily::Unix).is_err());
}

#[test]
fn format_dest_write_error_surfaces_permission() {
    let msg = format_dest_write_error("/etc/nixos", "sftp write: Permission denied");
    assert!(msg.contains("Permission denied writing to /etc/nixos"));
    assert!(msg.contains("elevated rights"));
    let other = format_dest_write_error("/tmp", "disk full");
    assert!(other.contains("Cannot write to /tmp"));
    assert!(other.contains("disk full"));
}

#[test]
fn remote_parent_base_splits_path() {
    assert_eq!(remote_parent_base("/src"), ("/".into(), "src".into()));
    assert_eq!(
        remote_parent_base("/home/alice/proj"),
        ("/home/alice".into(), "proj".into())
    );
    assert_eq!(
        remote_parent_base("relative"),
        (".".into(), "relative".into())
    );
}

#[test]
fn join_tmp_respects_windows_style() {
    assert_eq!(join_tmp("/tmp", "a.bin"), "/tmp/a.bin");
    assert_eq!(
        join_tmp(r"C:\Users\x\AppData\Local\Temp", "a.exe"),
        r"C:\Users\x\AppData\Local\Temp\a.exe"
    );
}

#[test]
fn edit_temp_path_keeps_basename_under_terminus_sftp_edit() {
    let name = "config.yaml";
    let dir = std::env::temp_dir()
        .join("terminus-sftp-edit")
        .join("00000000-0000-0000-0000-000000000001");
    let local = dir.join(name);
    assert!(local.to_string_lossy().contains("terminus-sftp-edit"));
    assert_eq!(local.file_name().unwrap(), name);
}

#[test]
fn transfer_folder_command_exists() {
    let worker = SftpWorker::spawn(None);
    let dir = std::env::temp_dir()
        .join(format!("terminus-sftp-xfer-folder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let left = dir.join("L");
    let right = dir.join("R");
    std::fs::create_dir_all(left.join("tree")).unwrap();
    std::fs::write(left.join("tree").join("a.txt"), b"a").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    worker.send(SftpCommand::ListLocal {
        side: SftpSide::Left,
        path: left.clone(),
    });
    worker.send(SftpCommand::ListLocal {
        side: SftpSide::Right,
        path: right.clone(),
    });
    for _ in 0..50 {
        let _ = worker.drain();
        std::thread::sleep(Duration::from_millis(10));
    }
    worker.send(SftpCommand::TransferFolder {
        from_side: SftpSide::Left,
        from_path: left.join("tree").to_string_lossy().into_owned(),
        to_side: SftpSide::Right,
        to_cwd: right.to_string_lossy().into_owned(),
        name: "tree".into(),
    });
    let mut listed = false;
    for _ in 0..80 {
        for event in worker.drain() {
            if let SftpEvent::Listed {
                side: SftpSide::Right,
                entries,
                ..
            } = event
            {
                if entries.iter().any(|e| e.name == "tree") {
                    listed = true;
                }
            }
        }
        if right.join("tree").join("a.txt").is_file() {
            listed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        listed && right.join("tree").join("a.txt").is_file(),
        "TransferFolder should copy the tree"
    );
    worker.send(SftpCommand::Close);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn transfer_queue_events_desired() {
    let worker = SftpWorker::spawn(None);
    let dir =
        std::env::temp_dir().join(format!("terminus-sftp-queue-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let left = dir.join("L");
    let right = dir.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("payload.bin"), b"xyz").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    worker.send(SftpCommand::Transfer {
        from_side: SftpSide::Left,
        from_path: left.join("payload.bin").to_string_lossy().into_owned(),
        to_side: SftpSide::Right,
        to_cwd: right.to_string_lossy().into_owned(),
        name: "payload.bin".into(),
    });
    let mut saw_progress = false;
    for _ in 0..80 {
        for event in worker.drain() {
            if let SftpEvent::TransferProgress { .. } = event {
                saw_progress = true;
            }
        }
        if saw_progress && right.join("payload.bin").is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        saw_progress,
        "transfer should emit TransferProgress queue events"
    );
    assert_eq!(std::fs::read(right.join("payload.bin")).unwrap(), b"xyz");
    worker.send(SftpCommand::Close);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn remove_local_file() {
    let worker = SftpWorker::spawn(None);
    let dir =
        std::env::temp_dir().join(format!("terminus-sftp-rm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("x.txt");
    std::fs::write(&file, b"x").unwrap();
    worker.send(SftpCommand::RemoveLocal {
        side: SftpSide::Left,
        path: file.clone(),
        recursive: false,
    });
    let mut listed = false;
    for _ in 0..50 {
        for event in worker.drain() {
            if let SftpEvent::Listed { entries, .. } = event {
                assert!(!entries.iter().any(|e| e.name == "x.txt"));
                listed = true;
            }
        }
        if listed {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(listed);
    assert!(!file.exists());
    worker.send(SftpCommand::Close);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn transfer_local_to_local() {
    let worker = SftpWorker::spawn(None);
    let root = std::env::temp_dir()
        .join(format!("terminus-sftp-xfer-ll-{}", std::process::id()));
    let left = root.join("L");
    let right = root.join("R");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    std::fs::write(left.join("p.txt"), b"payload").unwrap();
    worker.send(SftpCommand::Transfer {
        from_side: SftpSide::Left,
        from_path: left.join("p.txt").to_string_lossy().into_owned(),
        to_side: SftpSide::Right,
        to_cwd: right.to_string_lossy().into_owned(),
        name: "p.txt".into(),
    });
    let mut ok = false;
    for _ in 0..80 {
        for event in worker.drain() {
            if let SftpEvent::Listed {
                side: SftpSide::Right,
                entries,
                ..
            } = event
            {
                if entries.iter().any(|e| e.name == "p.txt") {
                    ok = true;
                }
            }
        }
        if ok {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(ok);
    assert_eq!(std::fs::read(right.join("p.txt")).unwrap(), b"payload");
    worker.send(SftpCommand::Close);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn resolve_conflict_command_does_not_block_command_queue() {
    // ResolveConflict is routed onto the conflict channel in `send`, so a
    // subsequent ListLocal still arrives on the command queue.
    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::ResolveConflict {
        id: 1,
        action: ConflictAction::Keep,
        apply_to_all: true,
    });
    let dir = std::env::temp_dir().join(format!(
        "terminus-sftp-conflict-route-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    worker.send(SftpCommand::ListLocal {
        side: SftpSide::Left,
        path: dir.clone(),
    });
    let mut listed = false;
    for _ in 0..50 {
        for event in worker.drain() {
            if matches!(event, SftpEvent::Listed { .. }) {
                listed = true;
            }
        }
        if listed {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(listed, "ListLocal should still be processed");
    worker.send(SftpCommand::Close);
    let _ = std::fs::remove_dir_all(&dir);
}
fn wait_for_event<T>(
    worker: &SftpWorker,
    mut pick: impl FnMut(SftpEvent) -> Option<T>,
) -> Option<T> {
    for _ in 0..150 {
        for event in worker.drain() {
            if let Some(found) = pick(event) {
                return Some(found);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

#[test]
fn listing_is_served_while_a_transfer_waits_on_a_conflict() {
    let dir = std::env::temp_dir().join(format!(
        "terminus-sftp-nav-during-transfer-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let (src, dst) = (dir.join("src"), dir.join("dst"));
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(src.join("a.txt"), b"new").unwrap();
    std::fs::write(dst.join("a.txt"), b"old").unwrap();

    let worker = SftpWorker::spawn(None);
    worker.send(SftpCommand::Transfer {
        from_side: SftpSide::Left,
        from_path: src.join("a.txt").to_string_lossy().into_owned(),
        to_side: SftpSide::Right,
        to_cwd: dst.to_string_lossy().into_owned(),
        name: "a.txt".into(),
    });
    let conflict = wait_for_event(&worker, |e| {
        matches!(e, SftpEvent::Conflict { .. }).then_some(())
    });
    assert!(
        conflict.is_some(),
        "transfer should be waiting on a conflict"
    );

    worker.send(SftpCommand::ListLocal {
        side: SftpSide::Left,
        path: src.clone(),
    });
    let listed = wait_for_event(&worker, |e| {
        matches!(e, SftpEvent::Listed { .. }).then_some(())
    });
    assert!(
        listed.is_some(),
        "navigation must not wait for the transfer"
    );

    worker.send(SftpCommand::CancelTransfer);
    worker.send(SftpCommand::Close);
    let _ = std::fs::remove_dir_all(&dir);
}
