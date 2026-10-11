//! Uploads through a real, private `sshd` bound to 127.0.0.1. Skipped when
//! no `sshd` or `ssh-keygen` binary is available (CI without OpenSSH).

use super::{upload_file, RemoteOs, SshUpload};
use std::fs;
use std::io::Cursor;
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PAYLOAD_LEN: usize = 1_100_000;

struct Sshd {
    child: Child,
    dir: PathBuf,
    remote_file: PathBuf,
    port: u16,
    user: String,
}

impl Drop for Sshd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.remote_file);
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn find_binary(name: &str) -> Option<PathBuf> {
    let system = Path::new("/usr/sbin").join(name);
    if system.is_file() {
        return Some(system);
    }
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| candidate.is_file())
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .expect("bind a free port")
}

fn keygen(ssh_keygen: &Path, key: &Path) {
    let status = Command::new(ssh_keygen)
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(key)
        .status()
        .expect("run ssh-keygen");
    assert!(status.success(), "ssh-keygen failed for {}", key.display());
}

fn wait_until_listening(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "sshd did not start listening");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn start_sshd(sshd: &Path, ssh_keygen: &Path, tag: &str, name: &str) -> Sshd {
    let dir =
        std::env::temp_dir().join(format!("terminus-sshd-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test dir");
    let host_key = dir.join("host_key");
    let client_key = dir.join("client_key");
    keygen(ssh_keygen, &host_key);
    keygen(ssh_keygen, &client_key);
    fs::copy(dir.join("client_key.pub"), dir.join("authorized_keys"))
        .expect("authorize key");
    let port = free_port();
    let config = dir.join("sshd_config");
    fs::write(
        &config,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\n\
             AuthorizedKeysFile {}\nPubkeyAuthentication yes\nPasswordAuthentication no\n\
             KbdInteractiveAuthentication no\nUsePAM no\nStrictModes no\n",
            host_key.display(),
            dir.join("sshd.pid").display(),
            dir.join("authorized_keys").display(),
        ),
    )
    .expect("write sshd_config");
    let log = fs::File::create(dir.join("sshd.log")).expect("create sshd log");
    let child = Command::new(sshd)
        .args(["-D", "-e", "-f"])
        .arg(&config)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .expect("spawn sshd");
    wait_until_listening(port);
    Sshd {
        child,
        dir,
        remote_file: std::env::temp_dir().join(name),
        port,
        user: std::env::var("USER").unwrap_or_else(|_| "root".to_string()),
    }
}

impl Sshd {
    fn upload(&self, key: &Path) -> SshUpload {
        SshUpload {
            program: "ssh".to_string(),
            tab_args: vec![
                "-p".into(),
                self.port.to_string(),
                "-i".into(),
                key.display().to_string(),
                "-o".into(),
                "IdentitiesOnly=yes".into(),
                "-o".into(),
                "StrictHostKeyChecking=no".into(),
                "-o".into(),
                "UserKnownHostsFile=/dev/null".into(),
                format!("{}@127.0.0.1", self.user),
            ],
            env: vec![],
            os: RemoteOs::Posix,
        }
    }
}

#[test]
fn upload_file_streams_bytes_through_a_real_sshd() {
    let (Some(sshd), Some(ssh_keygen)) = (find_binary("sshd"), find_binary("ssh-keygen"))
    else {
        eprintln!("skipping: sshd or ssh-keygen not found");
        return;
    };
    let name = format!("terminus-drop-e2e-{}.txt", std::process::id());
    let server = start_sshd(&sshd, &ssh_keygen, "upload", &name);
    let client_key = server.dir.join("client_key");
    let payload: Vec<u8> = (0..PAYLOAD_LEN).map(|i| (i % 251) as u8).collect();

    let path = upload_file(
        &server.upload(&client_key),
        &name,
        Cursor::new(payload.clone()),
    )
    .expect("upload through sshd");

    assert_eq!(path, format!("/tmp/{name}"));
    assert_eq!(fs::read(&path).expect("read remote file"), payload);
    let mode = fs::metadata(&path)
        .expect("stat remote file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn upload_with_a_wrong_key_reports_ssh_stderr() {
    let (Some(sshd), Some(ssh_keygen)) = (find_binary("sshd"), find_binary("ssh-keygen"))
    else {
        eprintln!("skipping: sshd or ssh-keygen not found");
        return;
    };
    let name = format!("terminus-drop-wrongkey-{}.txt", std::process::id());
    let server = start_sshd(&sshd, &ssh_keygen, "wrongkey", &name);
    let wrong_key = server.dir.join("wrong_key");
    keygen(&ssh_keygen, &wrong_key);

    let err = upload_file(
        &server.upload(&wrong_key),
        &name,
        Cursor::new(b"x".to_vec()),
    )
    .expect_err("a key the server does not accept must fail");

    assert!(err.contains("Permission denied"), "stderr was: {err}");
    assert!(!err.contains("Broken pipe"), "stderr was: {err}");
}
