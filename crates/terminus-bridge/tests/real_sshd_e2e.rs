//! Real-sshd E2E: drives terminus-core SSH/SFTP and the SFTP worker against a
//! live OpenSSH server. Skipped unless `TERMINUS_E2E_SSHD` is set.
//!
//! Env:
//! * `TERMINUS_E2E_SSHD`        `host:port` of a password+pubkey sshd (user/pass below)
//! * `TERMINUS_E2E_USER`/`_PASS` credentials (default `tuser` / `hunter2`)
//! * `TERMINUS_E2E_KEY`         path to an unencrypted ed25519 key authorized for the user
//! * `TERMINUS_E2E_KBD_SSHD`    `host:port` of an sshd with only keyboard-interactive
//! * `TERMINUS_E2E_SLOW_SSHD`   `host:port` of a throttled proxy to the first sshd

use std::path::PathBuf;
use std::time::{Duration, Instant};

use terminus_bridge::{SftpCommand, SftpEvent, SftpSide, SftpWorker};
use terminus_core::ssh::{
    HostKeyOutcome, HostKeyPolicy, KnownHosts, SshAuth, SshConnectOptions, SshPty,
    SshSession,
};
use terminus_core::HostAuthMethod;

fn target(var: &str) -> Option<(String, u16)> {
    let raw = std::env::var(var).ok()?;
    let (h, p) = raw.rsplit_once(':')?;
    Some((h.to_string(), p.parse().ok()?))
}

fn user() -> String {
    std::env::var("TERMINUS_E2E_USER").unwrap_or_else(|_| "tuser".into())
}
fn pass() -> String {
    std::env::var("TERMINUS_E2E_PASS").unwrap_or_else(|_| "hunter2".into())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("terminus-real-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn opts(var: &str, kh: &str) -> Option<SshConnectOptions> {
    let (host, port) = target(var)?;
    let mut o = SshConnectOptions::password(host, port, user(), pass());
    o.auth.method = Some(HostAuthMethod::Password);
    o.known_hosts = KnownHosts::at(scratch(kh).join("known_hosts"));
    Some(o)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

macro_rules! need {
    ($e:expr) => {
        match $e {
            Some(v) => v,
            None => {
                eprintln!("skipped: sshd env not set");
                return;
            }
        }
    };
}

// ---------------------------------------------------------------- auth ----

#[test]
fn password_auth_and_wrong_password_classification() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "pw"));
    let rt = rt();
    rt.block_on(terminus_core::ssh::probe_ssh_auth(&o))
        .expect("password auth should succeed");
    let mut bad = o.clone();
    bad.auth.password = Some("nope".into());
    let err = rt
        .block_on(terminus_core::ssh::probe_ssh_auth(&bad))
        .unwrap_err();
    assert_eq!(err, terminus_core::ssh::ProbeError::AuthFailed, "{err:?}");
}

#[test]
fn managed_key_pem_auth() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "key"));
    let key = need!(std::env::var("TERMINUS_E2E_KEY").ok());
    let pem = std::fs::read_to_string(key).unwrap();
    let mut k = o.clone();
    k.auth = SshAuth {
        username: user(),
        identity_pem: Some(pem),
        method: Some(HostAuthMethod::Key),
        ..SshAuth::default()
    };
    rt().block_on(terminus_core::ssh::probe_ssh_auth(&k))
        .expect("key auth should succeed");
}

#[test]
fn generated_managed_key_authenticates_once_authorized() {
    // Settings → Managed SSH Keys → Generate, then paste the public key into
    // authorized_keys, then connect with the stored PEM.
    let o = need!(opts("TERMINUS_E2E_SSHD", "genkey"));
    let ident = terminus_core::generate_ed25519_identity("e2e-gen").unwrap();
    let public = ident.public_key.clone().unwrap();
    let rt = rt();
    let conn = rt
        .block_on(terminus_core::ssh::connect_sftp(&o))
        .expect("sftp");
    let (code, _, err) = rt
        .block_on(conn.exec(&format!(
            "printf '%s\\n' '{public}' >> ~/.ssh/authorized_keys"
        )))
        .unwrap();
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let mut k = o.clone();
    k.auth = SshAuth {
        username: user(),
        identity_pem: ident.private_key.clone(),
        method: Some(HostAuthMethod::Key),
        ..SshAuth::default()
    };
    rt.block_on(terminus_core::ssh::probe_ssh_auth(&k))
        .expect("generated key should authenticate");
}

#[test]
fn keyboard_interactive_only_server_accepts_password() {
    // Many servers (PAM, macOS, 2FA front-ends) disable the `password` method and
    // only offer `keyboard-interactive`. OpenSSH CLI handles both.
    let o = need!(opts("TERMINUS_E2E_KBD_SSHD", "kbd"));
    let r = rt().block_on(terminus_core::ssh::probe_ssh_auth(&o));
    assert!(r.is_ok(), "password host on kbd-interactive server: {r:?}");
}

// ---------------------------------------------------------- host keys ----

#[test]
fn tofu_records_then_knows_then_detects_change() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "tofu"));
    let rt = rt();
    let mut s = rt.block_on(SshSession::connect(&o)).expect("first connect");
    assert!(matches!(
        s.host_key_outcome(),
        Some(HostKeyOutcome::Recorded { .. })
    ));
    rt.block_on(s.disconnect()).ok();
    let mut s = rt
        .block_on(SshSession::connect(&o))
        .expect("second connect");
    assert!(matches!(
        s.host_key_outcome(),
        Some(HostKeyOutcome::Known { .. })
    ));
    rt.block_on(s.disconnect()).ok();

    let path = o.known_hosts.path().to_path_buf();
    let content = std::fs::read_to_string(&path).unwrap();
    let forged = content
        .lines()
        .map(|l| {
            let mut f: Vec<String> = l.split(' ').map(String::from).collect();
            f[2] = "AAAAC3NzaC1lZDI1NTE5AAAAIPkQ0q0d9kXcbbGm0Yl7eV0x1N0dJ3oM7ZC2wN3b2aYz"
                .into();
            f.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, forged + "\n").unwrap();
    let err = rt
        .block_on(SshSession::connect(&o))
        .err()
        .expect("must refuse");
    assert!(err.to_string().contains("host key changed"), "{err}");
}

#[test]
fn tofu_with_openssh_recorded_other_algorithm_is_not_a_mitm() {
    // A user who already connected with the OpenSSH CLI (which the shell tab
    // uses!) typically has only an ecdsa or rsa line for the host. The server
    // still has that key; it merely also has ed25519. That is not a MITM.
    let o = need!(opts("TERMINUS_E2E_SSHD", "algo"));
    let (host, port) = target("TERMINUS_E2E_SSHD").unwrap();
    let out = std::process::Command::new("ssh-keyscan")
        .args(["-p", &port.to_string(), "-t", "ecdsa", &host])
        .output()
        .expect("ssh-keyscan");
    std::fs::write(o.known_hosts.path(), &out.stdout).unwrap();
    assert!(!out.stdout.is_empty(), "keyscan produced nothing");
    let r = rt().block_on(SshSession::connect(&o));
    assert!(
        r.is_ok(),
        "known_hosts with the server's real ecdsa key rejected: {:?}",
        r.err()
    );
}

#[test]
fn strict_accepts_hashed_known_hosts_entry() {
    // OpenSSH on Debian/Ubuntu writes hashed entries (HashKnownHosts yes).
    let mut o = need!(opts("TERMINUS_E2E_SSHD", "hashed"));
    let (host, port) = target("TERMINUS_E2E_SSHD").unwrap();
    let out = std::process::Command::new("ssh-keyscan")
        .args(["-H", "-p", &port.to_string(), &host])
        .output()
        .expect("ssh-keyscan");
    std::fs::write(o.known_hosts.path(), &out.stdout).unwrap();
    o.policy = HostKeyPolicy::Strict;
    let r = rt().block_on(SshSession::connect(&o));
    assert!(r.is_ok(), "hashed entry not recognised: {:?}", r.err());
}

// -------------------------------------------------------------- shell ----

#[test]
fn russh_shell_roundtrip_and_resize() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "shell"));
    let rt = rt();
    rt.block_on(async {
        let mut s = SshSession::connect(&o).await.unwrap();
        s.open_shell(SshPty::sized(100, 30)).await.unwrap();
        s.resize(120, 40).await.unwrap();
        s.write(b"stty size; echo MARK-$((6*7))\n").await.unwrap();
        let mut buf = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_secs(2), s.next_event()).await {
                Ok(Some(terminus_core::ssh::SshEvent::Data(d))) => buf.extend(d),
                Ok(Some(_)) | Err(_) => {}
                Ok(None) => break,
            }
            if String::from_utf8_lossy(&buf).contains("MARK-42") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&buf);
        assert!(text.contains("MARK-42"), "{text}");
        assert!(text.contains("40 120"), "resize not applied: {text}");
        s.disconnect().await.unwrap();
    });
}

#[test]
fn detect_remote_os_reports_distro() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "os"));
    let os = rt()
        .block_on(terminus_core::ssh::detect_remote_os(&o))
        .unwrap();
    let local = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let expect = terminus_core::os_detect::parse_os_id(&local, "Linux");
    assert_eq!(os, expect);
}

// ----------------------------------------------------- sftp session ----

#[test]
fn sftp_lists_symlinked_directory_as_directory() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "symlink"));
    let rt = rt();
    let conn = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
    let home = remote_home(&o);
    let rt2 = rt.block_on(conn.exec("mkdir -p ~/tree && ln -sfn tree ~/link-to-tree"));
    rt2.unwrap();
    let entries = rt.block_on(conn.list(&home)).unwrap();
    let link = entries
        .iter()
        .find(|e| e.name == "link-to-tree")
        .expect("link listed");
    assert!(link.is_dir, "symlink to a directory shown as a file");
}

#[test]
fn sftp_handles_names_with_edge_whitespace_and_backslash() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "names"));
    let rt = rt();
    let conn = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
    let home = remote_home(&o);
    rt.block_on(conn.exec(
        "cd ~ && printf trailing > 'trail ' && printf back > 'a\\b.txt' && printf lead > ' lead.txt'",
    ))
    .unwrap();
    let entries = rt.block_on(conn.list(&home)).unwrap();
    let mut failures = Vec::new();
    for (name, body) in [
        ("trail ", "trailing"),
        ("a\\b.txt", "back"),
        (" lead.txt", "lead"),
    ] {
        let entry = entries.iter().find(|e| e.name == name);
        let Some(entry) = entry else {
            failures.push(format!("{name:?} not listed"));
            continue;
        };
        match rt.block_on(conn.read(&entry.path)) {
            Ok(b) if b == body.as_bytes() => {}
            Ok(b) => failures.push(format!(
                "{name:?} read wrong bytes {:?}",
                String::from_utf8_lossy(&b)
            )),
            Err(e) => failures.push(format!("{name:?} read failed: {e}")),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// ------------------------------------------------------- sftp worker ----

fn wait<F: FnMut(&SftpEvent) -> bool>(
    w: &SftpWorker,
    secs: u64,
    mut f: F,
) -> Result<SftpEvent, Vec<SftpEvent>> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        for ev in w.drain() {
            if f(&ev) {
                return Ok(ev);
            }
            seen.push(ev);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(seen)
}

fn connect_worker(o: SshConnectOptions, local: PathBuf) -> SftpWorker {
    let w = SftpWorker::spawn(None);
    w.send(SftpCommand::ListLocal {
        side: SftpSide::Left,
        path: local,
    });
    w.send(SftpCommand::Connect {
        side: SftpSide::Right,
        opts: o,
    });
    wait(&w, 20, |e| {
        matches!(
            e,
            SftpEvent::Ready {
                side: SftpSide::Right
            }
        )
    })
    .expect("remote ready");
    w
}

fn remote_home(o: &SshConnectOptions) -> String {
    let rt = rt();
    let c = rt.block_on(terminus_core::ssh::connect_sftp(o)).unwrap();
    let (_, out, _) = rt.block_on(c.exec("pwd")).unwrap();
    String::from_utf8(out).unwrap().trim().to_string()
}

fn is_failed(e: &SftpEvent) -> bool {
    matches!(e, SftpEvent::Failed(_))
}

#[test]
fn worker_upload_download_roundtrip_large_file() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "w-big"));
    let home = remote_home(&o);
    let dir = scratch("w-big-local");
    let data: Vec<u8> = (0..20_000_000u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
        .collect();
    std::fs::write(dir.join("up.bin"), &data).unwrap();
    {
        let rt = rt();
        let c = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
        let _ = rt.block_on(c.remove(&format!("{home}/up.bin")));
    }
    let w = connect_worker(o, dir.clone());
    w.send(SftpCommand::Transfer {
        from_side: SftpSide::Left,
        from_path: dir.join("up.bin").to_string_lossy().into(),
        to_side: SftpSide::Right,
        to_cwd: home.clone(),
        name: "up.bin".into(),
    });
    let ev = wait(&w, 60, |e| {
        is_failed(e)
            || matches!(
                e,
                SftpEvent::Listed {
                    side: SftpSide::Right,
                    ..
                }
            )
    });
    assert!(matches!(ev, Ok(SftpEvent::Listed { .. })), "upload: {ev:?}");
    std::fs::remove_file(dir.join("up.bin")).unwrap();
    w.send(SftpCommand::Transfer {
        from_side: SftpSide::Right,
        from_path: format!("{home}/up.bin"),
        to_side: SftpSide::Left,
        to_cwd: dir.to_string_lossy().into(),
        name: "up.bin".into(),
    });
    let ev = wait(&w, 60, |e| {
        is_failed(e)
            || matches!(
                e,
                SftpEvent::Listed {
                    side: SftpSide::Left,
                    ..
                }
            )
    });
    assert!(
        matches!(ev, Ok(SftpEvent::Listed { .. })),
        "download: {ev:?}"
    );
    assert!(
        std::fs::read(dir.join("up.bin")).unwrap() == data,
        "checksum mismatch"
    );
}

#[test]
fn worker_upload_over_slow_link_does_not_hit_30s_whole_file_timeout() {
    // 40 MB at ~1 MB/s ≈ 40 s: any per-transfer (not per-chunk) 30 s budget fails.
    let o = need!(opts("TERMINUS_E2E_SLOW_SSHD", "w-slow"));
    let home = remote_home(&o);
    let dir = scratch("w-slow-local");
    std::fs::write(dir.join("slow.bin"), vec![7u8; 40_000_000]).unwrap();
    {
        let rt = rt();
        let c = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
        let _ = rt.block_on(c.remove(&format!("{home}/slow.bin")));
    }
    let w = connect_worker(o, dir.clone());
    let started = Instant::now();
    w.send(SftpCommand::Transfer {
        from_side: SftpSide::Left,
        from_path: dir.join("slow.bin").to_string_lossy().into(),
        to_side: SftpSide::Right,
        to_cwd: home,
        name: "slow.bin".into(),
    });
    let ev = wait(&w, 180, |e| {
        is_failed(e)
            || matches!(
                e,
                SftpEvent::Listed {
                    side: SftpSide::Right,
                    ..
                }
            )
    });
    eprintln!("slow upload finished after {:?}: {ev:?}", started.elapsed());
    assert!(
        matches!(ev, Ok(SftpEvent::Listed { .. })),
        "slow upload: {ev:?}"
    );
}

#[test]
fn worker_single_file_transfer_prompts_before_overwrite() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "w-ovr"));
    let home = remote_home(&o);
    let rt = rt();
    let c = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
    let target = format!("{home}/precious.txt");
    rt.block_on(c.write(&target, b"REMOTE ORIGINAL")).unwrap();
    let dir = scratch("w-ovr-local");
    std::fs::write(dir.join("precious.txt"), b"local").unwrap();
    let w = connect_worker(o, dir.clone());
    let send = || {
        w.send(SftpCommand::Transfer {
            from_side: SftpSide::Left,
            from_path: dir.join("precious.txt").to_string_lossy().into(),
            to_side: SftpSide::Right,
            to_cwd: home.clone(),
            name: "precious.txt".into(),
        })
    };
    let prompt = |w: &SftpWorker| {
        wait(w, 20, |e| {
            is_failed(e)
                || matches!(e, SftpEvent::Conflict { .. })
                || matches!(
                    e,
                    SftpEvent::Listed {
                        side: SftpSide::Right,
                        ..
                    }
                )
        })
    };

    send();
    let ev = prompt(&w);
    let Ok(SftpEvent::Conflict { id, .. }) = ev else {
        let now = rt.block_on(c.read(&target)).unwrap();
        panic!(
            "no conflict prompt ({ev:?}); remote now {:?}",
            String::from_utf8_lossy(&now)
        );
    };
    assert_eq!(rt.block_on(c.read(&target)).unwrap(), b"REMOTE ORIGINAL");

    // Keep: the remote file is untouched.
    w.send(SftpCommand::ResolveConflict {
        id,
        action: terminus_bridge::ConflictAction::Keep,
        apply_to_all: false,
    });
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(rt.block_on(c.read(&target)).unwrap(), b"REMOTE ORIGINAL");
    let _ = w.drain(); // the Keep round's refresh

    // Overwrite: replaced once confirmed.
    send();
    let Ok(SftpEvent::Conflict { id, .. }) = prompt(&w) else {
        panic!("second prompt missing")
    };
    w.send(SftpCommand::ResolveConflict {
        id,
        action: terminus_bridge::ConflictAction::Overwrite,
        apply_to_all: false,
    });
    let ev = wait(&w, 20, |e| {
        is_failed(e)
            || matches!(
                e,
                SftpEvent::Listed {
                    side: SftpSide::Right,
                    ..
                }
            )
    });
    assert!(matches!(ev, Ok(SftpEvent::Listed { .. })), "{ev:?}");
    assert_eq!(rt.block_on(c.read(&target)).unwrap(), b"local");
}

#[test]
fn worker_remote_pane_starts_in_home() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "w-home"));
    let home = remote_home(&o);
    let w = connect_worker(o, scratch("w-home-local"));
    w.send(SftpCommand::ListRemote {
        side: SftpSide::Right,
        path: terminus_bridge::REMOTE_HOME.into(),
    });
    let ev = wait(&w, 20, |e| {
        is_failed(e)
            || matches!(
                e,
                SftpEvent::Listed {
                    side: SftpSide::Right,
                    ..
                }
            )
    });
    let Ok(SftpEvent::Listed { path, .. }) = ev else {
        panic!("{ev:?}")
    };
    assert_eq!(path, home);
}

#[test]
fn interrupted_upload_never_truncates_the_existing_file() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "w-atomic"));
    let slow = need!(opts("TERMINUS_E2E_SLOW_SSHD", "w-atomic-slow"));
    let home = remote_home(&o);
    let target = format!("{home}/atomic.bin");
    let rt = rt();
    let c = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
    rt.block_on(c.write(&target, b"ORIGINAL")).unwrap();
    let dir = scratch("w-atomic-local");
    std::fs::write(dir.join("atomic.bin"), vec![1u8; 30_000_000]).unwrap();
    let conn = rt
        .block_on(terminus_core::ssh::connect_sftp(&slow))
        .unwrap();
    // 30 MB over ~1 MB/s cannot finish in 2 s: the upload is cut mid-stream.
    let local = dir.join("atomic.bin");
    let upload = conn.upload_file(&local, &target, |_, _| {});
    let cut =
        rt.block_on(async { tokio::time::timeout(Duration::from_secs(2), upload).await });
    assert!(
        cut.is_err(),
        "upload finished too fast to test interruption"
    );
    assert_eq!(
        rt.block_on(c.read(&target)).unwrap(),
        b"ORIGINAL",
        "an interrupted upload replaced the file"
    );
    // Dropping the upload future skips its own cleanup of the temp sibling.
    let _ = rt.block_on(c.exec("rm -f ~/.atomic.bin.terminus-part-*"));
}

#[test]
fn worker_folder_roundtrip_with_spaces_unicode_and_dir_symlink() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "w-fold"));
    let home = remote_home(&o);
    let dir = scratch("w-fold-local");
    let src = dir.join("my stuff ü");
    std::fs::create_dir_all(src.join("sub dir/deeper")).unwrap();
    std::fs::write(src.join("sub dir/deeper/ünï 'q'.txt"), b"payload").unwrap();
    std::fs::write(src.join("top.txt"), b"top").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("sub dir", src.join("dirlink")).unwrap();
    let w = connect_worker(o.clone(), dir.clone());
    w.send(SftpCommand::TransferFolder {
        from_side: SftpSide::Left,
        from_path: src.to_string_lossy().into(),
        to_side: SftpSide::Right,
        to_cwd: home.clone(),
        name: "my stuff ü".into(),
    });
    let ev = wait(&w, 60, |e| {
        is_failed(e)
            || matches!(
                e,
                SftpEvent::Listed {
                    side: SftpSide::Right,
                    ..
                }
            )
    });
    assert!(
        matches!(ev, Ok(SftpEvent::Listed { .. })),
        "upload folder: {ev:?}"
    );
    let rt = rt();
    let c = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
    let got = rt
        .block_on(c.read(&format!("{home}/my stuff ü/sub dir/deeper/ünï 'q'.txt")))
        .expect("nested file uploaded");
    assert_eq!(got, b"payload");
}

#[test]
fn worker_remote_folder_download_with_quotes_in_name() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "w-q"));
    let home = remote_home(&o);
    let rt = rt();
    let c = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
    let (code, _, err) = rt
        .block_on(c.exec("rm -rf ~/\"it's \\$HOME \\`x\\`\" && mkdir -p ~/\"it's \\$HOME \\`x\\`/in\" && echo hi > ~/\"it's \\$HOME \\`x\\`/in/f\""))
        .unwrap();
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let name = "it's $HOME `x`";
    let dir = scratch("w-q-local");
    let w = connect_worker(o, dir.clone());
    w.send(SftpCommand::TransferFolder {
        from_side: SftpSide::Right,
        from_path: format!("{home}/{name}"),
        to_side: SftpSide::Left,
        to_cwd: dir.to_string_lossy().into(),
        name: name.into(),
    });
    let ev = wait(&w, 60, |e| {
        is_failed(e)
            || matches!(
                e,
                SftpEvent::Listed {
                    side: SftpSide::Left,
                    ..
                }
            )
    });
    assert!(
        matches!(ev, Ok(SftpEvent::Listed { .. })),
        "download folder: {ev:?}"
    );
    assert_eq!(std::fs::read(dir.join(name).join("in/f")).unwrap(), b"hi\n");
}

#[test]
fn worker_edit_remote_roundtrip_uploads_saved_changes() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "w-edit"));
    let home = remote_home(&o);
    let rt = rt();
    let c = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
    rt.block_on(c.write(&format!("{home}/edit-me.txt"), b"v1"))
        .unwrap();
    let dir = scratch("w-edit-local");
    let w = connect_worker(o, dir);
    w.send(SftpCommand::EditRemote {
        side: SftpSide::Right,
        remote_path: format!("{home}/edit-me.txt"),
        name: "edit-me.txt".into(),
    });
    let ev = wait(&w, 20, |e| {
        is_failed(e) || matches!(e, SftpEvent::EditReady { .. })
    })
    .expect("edit ready");
    let SftpEvent::EditReady { local_path, .. } = ev else {
        panic!("{ev:?}")
    };
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&local_path, b"v2 edited").unwrap();
    let ev = wait(&w, 20, |e| {
        is_failed(e) || matches!(e, SftpEvent::EditSaved { .. })
    });
    assert!(matches!(ev, Ok(SftpEvent::EditSaved { .. })), "{ev:?}");
    assert_eq!(
        rt.block_on(c.read(&format!("{home}/edit-me.txt"))).unwrap(),
        b"v2 edited"
    );
}

#[test]
fn worker_remove_remote_recursive() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "w-rm"));
    let home = remote_home(&o);
    let rt = rt();
    let c = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
    rt.block_on(c.exec("rm -rf ~/rmme && mkdir -p ~/rmme/a/b && touch ~/rmme/a/b/f ~/rmme/.hidden && ln -s /etc ~/rmme/etclink")).unwrap();
    let dir = scratch("w-rm-local");
    let w = connect_worker(o, dir);
    w.send(SftpCommand::RemoveRemoteRecursive {
        side: SftpSide::Right,
        path: format!("{home}/rmme"),
    });
    let ev = wait(&w, 30, |e| {
        is_failed(e)
            || matches!(
                e,
                SftpEvent::Listed {
                    side: SftpSide::Right,
                    ..
                }
            )
    });
    assert!(matches!(ev, Ok(SftpEvent::Listed { .. })), "{ev:?}");
    assert!(!rt.block_on(c.exists(&format!("{home}/rmme"))).unwrap());
    assert!(
        std::path::Path::new("/etc/passwd").exists(),
        "followed symlink out of tree!"
    );
}

#[test]
fn overwrite_keeps_permissions_and_symlinks() {
    let o = need!(opts("TERMINUS_E2E_SSHD", "w-perm"));
    let home = remote_home(&o);
    let rt = rt();
    let c = rt.block_on(terminus_core::ssh::connect_sftp(&o)).unwrap();
    let (code, _, err) = rt
        .block_on(c.exec(
            "cd ~ && rm -rf permtest && mkdir permtest && cd permtest && \
             printf old > run.sh && chmod 755 run.sh && printf real > real.txt && ln -s real.txt link.txt",
        ))
        .unwrap();
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    rt.block_on(c.write(&format!("{home}/permtest/run.sh"), b"#!/bin/sh\necho new\n"))
        .unwrap();
    rt.block_on(c.write(&format!("{home}/permtest/link.txt"), b"through link"))
        .unwrap();
    let (_, out, _) = rt
        .block_on(c.exec(
            "cd ~/permtest && stat -c %a run.sh && test -L link.txt && echo LINK && cat real.txt",
        ))
        .unwrap();
    let out = String::from_utf8_lossy(&out);
    assert!(out.starts_with("755"), "mode lost: {out}");
    assert!(out.contains("LINK"), "symlink replaced by a file: {out}");
    assert!(out.contains("through link"), "{out}");
}
