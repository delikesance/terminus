//! In-process russh + russh-sftp fixture backed by a temp directory.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Data, File as SftpFile, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode,
    Version,
};
use tokio::sync::Mutex as AsyncMutex;

pub struct MockSftpServer {
    pub port: u16,
    pub root: PathBuf,
    shutdown: Arc<AtomicBool>,
    _join: Option<std::thread::JoinHandle<()>>,
}

impl MockSftpServer {
    pub fn shutdown(self) {
        self.shutdown.store(true, Ordering::SeqCst);
        // Nudge the accept loop.
        let _ = std::net::TcpStream::connect(("127.0.0.1", self.port));
        if let Some(join) = self._join {
            let _ = join.join();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn try_spawn() -> Result<MockSftpServer, String> {
    let root = std::env::temp_dir().join(format!(
        "terminus-mock-sftp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    // Seed a visible file so listing `/` is non-empty and editable.
    fs::write(root.join("hello.txt"), b"hello from mock").map_err(|e| e.to_string())?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_flag = Arc::clone(&shutdown);
    let root_for_server = root.clone();

    let (port_tx, port_rx) = std::sync::mpsc::channel();

    let join = std::thread::Builder::new()
        .name("terminus-mock-sftp".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("mock sftp runtime");
            rt.block_on(async move {
                if let Err(err) =
                    run_server(root_for_server, shutdown_flag, port_tx).await
                {
                    eprintln!("mock sftp server stopped: {err}");
                }
            });
        })
        .map_err(|e| e.to_string())?;

    let port = port_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "mock sftp failed to bind".to_string())?;

    Ok(MockSftpServer {
        port,
        root,
        shutdown,
        _join: Some(join),
    })
}

async fn run_server(
    root: PathBuf,
    shutdown: Arc<AtomicBool>,
    port_tx: std::sync::mpsc::Sender<u16>,
) -> Result<(), String> {
    let key = russh::keys::PrivateKey::random(
        &mut rand_keygen::rng(),
        russh::keys::Algorithm::Ed25519,
    )
    .map_err(|e| e.to_string())?;

    let config = russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        keys: vec![key],
        ..Default::default()
    };
    let config = Arc::new(config);
    let mut server = MockServer {
        root: Arc::new(root),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let _ = port_tx.send(port);

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let accept =
            tokio::time::timeout(Duration::from_millis(100), listener.accept()).await;
        match accept {
            Ok(Ok((stream, addr))) => {
                let config = Arc::clone(&config);
                let handler = server.new_client(Some(addr));
                tokio::spawn(async move {
                    let _ = russh::server::run_stream(config, stream, handler).await;
                });
            }
            Ok(Err(err)) => return Err(err.to_string()),
            Err(_) => continue, // timeout — recheck shutdown
        }
    }
    Ok(())
}

#[derive(Clone)]
struct MockServer {
    root: Arc<PathBuf>,
}

impl russh::server::Server for MockServer {
    type Handler = MockHandler;

    fn new_client(&mut self, _: Option<SocketAddr>) -> Self::Handler {
        MockHandler {
            root: Arc::clone(&self.root),
            channels: Arc::new(AsyncMutex::new(HashMap::new())),
        }
    }
}

struct MockHandler {
    root: Arc<PathBuf>,
    channels: Arc<AsyncMutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl MockHandler {
    async fn take_channel(&self, id: ChannelId) -> Option<Channel<Msg>> {
        self.channels.lock().await.remove(&id)
    }
}

impl russh::server::Handler for MockHandler {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<Auth, Self::Error> {
        if user == "test" && password == "test" {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            })
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.lock().await.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            let _ = session.channel_failure(channel_id);
            return Ok(());
        }
        let Some(channel) = self.take_channel(channel_id).await else {
            let _ = session.channel_failure(channel_id);
            return Ok(());
        };
        let _ = session.channel_success(channel_id);
        let root = Arc::clone(&self.root);
        let sftp = FsSftp {
            root,
            next_handle: 1,
            handles: HashMap::new(),
        };
        russh_sftp::server::run(channel.into_stream(), sftp).await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let cmd = String::from_utf8_lossy(data).into_owned();
        let _ = session.channel_success(channel);
        let root = Arc::clone(&self.root);
        let (code, stdout, stderr) =
            tokio::task::spawn_blocking(move || mock_run_exec(&root, &cmd))
                .await
                .unwrap_or_else(|e| (255, Vec::new(), format!("join: {e}").into_bytes()));

        if !stdout.is_empty() {
            let _ = session.data(channel, stdout);
        }
        if !stderr.is_empty() {
            let _ = session.extended_data(channel, 1, stderr);
        }
        let _ = session.exit_status_request(channel, code);
        let _ = session.eof(channel);
        let _ = session.close(channel);
        Ok(())
    }
}

/// Run archive create/extract scripts against the mock root filesystem.
fn mock_run_exec(root: &Path, script: &str) -> (u32, Vec<u8>, Vec<u8>) {
    let meta = parse_archive_meta(script);
    let Some(meta) = meta else {
        // Fallback: run shell with absolute paths remapped into root.
        let remapped = remap_abs_paths(root, script);
        return match std::process::Command::new("sh")
            .arg("-c")
            .arg(&remapped)
            .output()
        {
            Ok(out) => (
                out.status.code().unwrap_or(255) as u32,
                out.stdout,
                out.stderr,
            ),
            Err(e) => (255, Vec::new(), format!("{e}").into_bytes()),
        };
    };

    let parent = map_mock_path(root, &meta.parent);
    let out = map_mock_path(root, &meta.out);
    if let Some(parent_dir) = out.parent() {
        let _ = fs::create_dir_all(parent_dir);
    }

    match meta.mode.as_str() {
        "create" => {
            let src = parent.join(&meta.base);
            if !src.exists() {
                return (
                    1,
                    Vec::new(),
                    format!("source missing: {}", src.display()).into_bytes(),
                );
            }
            let is_tar = meta.out.ends_with(".tar.gz") || meta.out.ends_with(".tgz");
            let root_name = if meta.name.is_empty() {
                meta.base.clone()
            } else {
                meta.name.clone()
            };
            let result = if is_tar {
                mock_tar_create(&parent, &meta.base, &root_name, &out)
            } else if which_zip() && root_name == meta.base {
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg(format!(
                        "cd {} && zip -rq {} {}",
                        shell_quote(&parent.to_string_lossy()),
                        shell_quote(&out.to_string_lossy()),
                        shell_quote(&meta.base)
                    ))
                    .status()
                    .map(|s| {
                        if s.success() {
                            Ok(())
                        } else {
                            Err("zip command failed".into())
                        }
                    })
                    .unwrap_or_else(|e| Err(e.to_string()))
            } else {
                mock_zip_dir(&src, &root_name, &out)
            };
            match result {
                Ok(()) => (0, format!("{}\n", meta.out).into_bytes(), Vec::new()),
                Err(e) => (1, Vec::new(), e.into_bytes()),
            }
        }
        "extract" => {
            let dest = map_mock_path(root, &meta.parent);
            let _ = fs::create_dir_all(&dest);
            let is_tar = meta.out.ends_with(".tar.gz") || meta.out.ends_with(".tgz");
            let result = if is_tar {
                std::process::Command::new("tar")
                    .arg("-C")
                    .arg(&dest)
                    .arg("-xzf")
                    .arg(&out)
                    .status()
                    .map(|s| {
                        if s.success() {
                            Ok(())
                        } else {
                            Err("tar extract failed".into())
                        }
                    })
                    .unwrap_or_else(|e| Err(e.to_string()))
            } else if which_unzip() {
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg(format!(
                        "mkdir -p {dest} && unzip -qo {zip} -d {dest}",
                        dest = shell_quote(&dest.to_string_lossy()),
                        zip = shell_quote(&out.to_string_lossy()),
                    ))
                    .status()
                    .map(|s| {
                        if s.success() {
                            Ok(())
                        } else {
                            Err("unzip command failed".into())
                        }
                    })
                    .unwrap_or_else(|e| Err(e.to_string()))
            } else {
                mock_unzip_to(&out, &dest)
            };
            match result {
                Ok(()) => (0, Vec::new(), Vec::new()),
                Err(e) => (1, Vec::new(), e.into_bytes()),
            }
        }
        other => (
            1,
            Vec::new(),
            format!("unknown archive mode: {other}").into_bytes(),
        ),
    }
}

fn mock_tar_create(
    parent: &Path,
    base: &str,
    root_name: &str,
    out: &Path,
) -> Result<(), String> {
    if root_name == base {
        return std::process::Command::new("tar")
            .arg("-C")
            .arg(parent)
            .arg("-czhf")
            .arg(out)
            .arg(base)
            .status()
            .map(|s| {
                if s.success() {
                    Ok(())
                } else {
                    Err("tar create failed".into())
                }
            })
            .unwrap_or_else(|e| Err(e.to_string()));
    }

    // Pack under a different root name: symlink on Unix, temp rename elsewhere.
    let link = parent.join(root_name);
    let src = parent.join(base);
    #[cfg(unix)]
    {
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(base, &link).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        if link.exists() {
            return Err(format!(
                "renamed tar root target already exists: {}",
                link.display()
            ));
        }
        fs::rename(&src, &link).map_err(|e| e.to_string())?;
    }
    let status = std::process::Command::new("tar")
        .arg("-C")
        .arg(parent)
        .arg("-czhf")
        .arg(out)
        .arg(root_name)
        .status();
    #[cfg(unix)]
    {
        let _ = fs::remove_file(&link);
    }
    #[cfg(not(unix))]
    {
        let _ = fs::rename(&link, &src);
    }
    status
        .map(|s| {
            if s.success() {
                Ok(())
            } else {
                Err("tar create failed".into())
            }
        })
        .unwrap_or_else(|e| Err(e.to_string()))
}

fn mock_zip_dir(src: &Path, root_name: &str, zip_path: &Path) -> Result<(), String> {
    let file = File::create(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.add_directory(format!("{root_name}/"), opts)
        .map_err(|e| e.to_string())?;
    fn add(
        zip: &mut zip::ZipWriter<File>,
        dir: &Path,
        prefix: &str,
        opts: zip::write::SimpleFileOptions,
    ) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let rel = format!("{prefix}/{name}");
            let path = entry.path();
            let ft = entry.file_type().map_err(|e| e.to_string())?;
            if ft.is_dir() {
                zip.add_directory(format!("{rel}/"), opts)
                    .map_err(|e| e.to_string())?;
                add(zip, &path, &rel, opts)?;
            } else if ft.is_file() {
                zip.start_file(&rel, opts).map_err(|e| e.to_string())?;
                let mut input = File::open(&path).map_err(|e| e.to_string())?;
                std::io::copy(&mut input, zip).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    add(&mut zip, src, root_name, opts)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn mock_unzip_to(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

struct ArchiveMeta {
    mode: String,
    parent: String,
    base: String,
    /// Desired root name inside the archive (defaults to `base`).
    name: String,
    out: String,
}

fn parse_archive_meta(script: &str) -> Option<ArchiveMeta> {
    if !script.contains("terminus-sftp-archive-v1") {
        return None;
    }
    let mut mode = None;
    let mut parent = None;
    let mut base = None;
    let mut name = None;
    let mut out = None;
    for line in script.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("TERMINUS_ARCHIVE_MODE=") {
            mode = Some(unquote(v));
        } else if let Some(v) = line.strip_prefix("TERMINUS_ARCHIVE_PARENT=") {
            parent = Some(unquote(v));
        } else if let Some(v) = line.strip_prefix("TERMINUS_ARCHIVE_BASE=") {
            base = Some(unquote(v));
        } else if let Some(v) = line.strip_prefix("TERMINUS_ARCHIVE_NAME=") {
            name = Some(unquote(v));
        } else if let Some(v) = line.strip_prefix("TERMINUS_ARCHIVE_OUT=") {
            out = Some(unquote(v));
        }
    }
    let base = base.unwrap_or_default();
    let name = name.unwrap_or_else(|| base.clone());
    Some(ArchiveMeta {
        mode: mode?,
        parent: parent?,
        base,
        name,
        out: out?,
    })
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        v[1..v.len() - 1].replace("'\\''", "'")
    } else {
        v.to_string()
    }
}

fn map_mock_path(root: &Path, remote: &str) -> PathBuf {
    let rel = remote.trim_start_matches('/');
    if rel.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel)
    }
}

fn remap_abs_paths(root: &Path, script: &str) -> String {
    // Best-effort: prefix bare absolute paths after spaces/quotes with root.
    // Archive scripts should use the structured meta path above instead.
    script.replace(" cd /", &format!(" cd {}/", root.display()))
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn which_zip() -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg("command -v zip >/dev/null 2>&1")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn which_unzip() -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg("command -v unzip >/dev/null 2>&1")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

enum Opened {
    Dir {
        path: PathBuf,
        entries: Vec<String>,
        sent: bool,
    },
    File {
        file: Mutex<File>,
    },
}

struct FsSftp {
    root: Arc<PathBuf>,
    next_handle: u64,
    handles: HashMap<String, Opened>,
}

impl FsSftp {
    fn map_path(&self, remote: &str) -> Result<PathBuf, StatusCode> {
        let trimmed = remote.trim();
        let rel = trimmed.trim_start_matches('/');
        if rel.contains("..") {
            return Err(StatusCode::PermissionDenied);
        }
        let path = if rel.is_empty() {
            self.root.as_path().to_path_buf()
        } else {
            self.root.join(rel)
        };
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.as_path().to_path_buf());
        if let Ok(canon) = path.canonicalize() {
            if !canon.starts_with(&root) {
                return Err(StatusCode::PermissionDenied);
            }
            return Ok(canon);
        }
        // Not yet existing (create / mkdir): ensure parent stays under root.
        if let Some(parent) = path.parent() {
            if let Ok(p) = parent.canonicalize() {
                if !p.starts_with(&root) {
                    return Err(StatusCode::PermissionDenied);
                }
            }
        }
        Ok(path)
    }

    fn attrs_for(path: &Path) -> FileAttributes {
        match fs::metadata(path) {
            Ok(meta) => {
                let mut attrs = FileAttributes::from(&meta);
                if meta.is_dir() {
                    attrs.set_dir(true);
                } else {
                    attrs.set_regular(true);
                }
                attrs
            }
            Err(_) => FileAttributes::dummy(),
        }
    }

    fn ok(id: u32) -> Status {
        Status {
            id,
            status_code: StatusCode::Ok,
            error_message: "Ok".into(),
            language_tag: "en-US".into(),
        }
    }

    fn alloc_handle(&mut self) -> String {
        let id = self.next_handle;
        self.next_handle = self.next_handle.saturating_add(1);
        format!("h{id}")
    }
}

impl russh_sftp::server::Handler for FsSftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _version: u32,
        _extensions: HashMap<String, String>,
    ) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles.remove(&handle);
        Ok(Self::ok(id))
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let mapped = self.map_path(&path)?;
        let display = if mapped == *self.root {
            "/".to_string()
        } else {
            format!(
                "/{}",
                mapped
                    .strip_prefix(self.root.as_path())
                    .unwrap_or(&mapped)
                    .to_string_lossy()
                    .replace('\\', "/")
            )
        };
        Ok(Name {
            id,
            files: vec![SftpFile::dummy(display)],
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let mapped = self.map_path(&path)?;
        let meta = fs::metadata(&mapped).map_err(|_| StatusCode::NoSuchFile)?;
        if !meta.is_dir() {
            return Err(StatusCode::Failure);
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&mapped).map_err(|_| StatusCode::Failure)? {
            let entry = entry.map_err(|_| StatusCode::Failure)?;
            entries.push(entry.file_name().to_string_lossy().into_owned());
        }
        let handle = self.alloc_handle();
        self.handles.insert(
            handle.clone(),
            Opened::Dir {
                path: mapped,
                entries,
                sent: false,
            },
        );
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let opened = self.handles.get_mut(&handle).ok_or(StatusCode::Failure)?;
        let Opened::Dir {
            path,
            entries,
            sent,
        } = opened
        else {
            return Err(StatusCode::Failure);
        };
        if *sent {
            return Err(StatusCode::Eof);
        }
        *sent = true;
        let mut files = Vec::new();
        for name in entries.iter() {
            let child = path.join(name);
            files.push(SftpFile::new(name.clone(), Self::attrs_for(&child)));
        }
        Ok(Name { id, files })
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let mapped = self.map_path(&filename)?;
        if let Some(parent) = mapped.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let opts: OpenOptions = pflags.into();
        let file = opts.open(&mapped).map_err(|_| StatusCode::Failure)?;
        let handle = self.alloc_handle();
        self.handles.insert(
            handle.clone(),
            Opened::File {
                file: Mutex::new(file),
            },
        );
        Ok(Handle { id, handle })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let opened = self.handles.get_mut(&handle).ok_or(StatusCode::Failure)?;
        let Opened::File { file } = opened else {
            return Err(StatusCode::Failure);
        };
        let mut file = file.lock().map_err(|_| StatusCode::Failure)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| StatusCode::Failure)?;
        let mut buf = vec![0u8; len as usize];
        let n = file.read(&mut buf).map_err(|_| StatusCode::Failure)?;
        if n == 0 {
            return Err(StatusCode::Eof);
        }
        buf.truncate(n);
        Ok(Data { id, data: buf })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let opened = self.handles.get_mut(&handle).ok_or(StatusCode::Failure)?;
        let Opened::File { file } = opened else {
            return Err(StatusCode::Failure);
        };
        let mut file = file.lock().map_err(|_| StatusCode::Failure)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| StatusCode::Failure)?;
        file.write_all(&data).map_err(|_| StatusCode::Failure)?;
        Ok(Self::ok(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        let mapped = self.map_path(&path)?;
        fs::create_dir_all(&mapped).map_err(|_| StatusCode::Failure)?;
        Ok(Self::ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        let mapped = self.map_path(&filename)?;
        fs::remove_file(&mapped).map_err(|_| StatusCode::Failure)?;
        Ok(Self::ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        let mapped = self.map_path(&path)?;
        fs::remove_dir(&mapped).map_err(|_| StatusCode::Failure)?;
        Ok(Self::ok(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let from = self.map_path(&oldpath)?;
        let to = self.map_path(&newpath)?;
        fs::rename(from, to).map_err(|_| StatusCode::Failure)?;
        Ok(Self::ok(id))
    }

    async fn stat(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
        let mapped = self.map_path(&path)?;
        let meta = fs::metadata(&mapped).map_err(|_| StatusCode::NoSuchFile)?;
        Ok(russh_sftp::protocol::Attrs {
            id,
            attrs: {
                let mut attrs = FileAttributes::from(&meta);
                if meta.is_dir() {
                    attrs.set_dir(true);
                } else {
                    attrs.set_regular(true);
                }
                attrs
            },
        })
    }

    async fn lstat(
        &mut self,
        id: u32,
        path: String,
    ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
        let mapped = self.map_path(&path)?;
        let meta = fs::symlink_metadata(&mapped).map_err(|_| StatusCode::NoSuchFile)?;
        Ok(russh_sftp::protocol::Attrs {
            id,
            attrs: {
                let mut attrs = FileAttributes::from(&meta);
                if meta.is_dir() {
                    attrs.set_dir(true);
                } else {
                    attrs.set_regular(true);
                }
                attrs
            },
        })
    }

    async fn fstat(
        &mut self,
        id: u32,
        handle: String,
    ) -> Result<russh_sftp::protocol::Attrs, Self::Error> {
        let opened = self.handles.get(&handle).ok_or(StatusCode::Failure)?;
        let attrs = match opened {
            Opened::Dir { path, .. } => Self::attrs_for(path),
            Opened::File { file } => {
                let file = file.lock().map_err(|_| StatusCode::Failure)?;
                let meta = file.metadata().map_err(|_| StatusCode::Failure)?;
                let mut attrs = FileAttributes::from(&meta);
                attrs.set_regular(true);
                attrs
            }
        };
        Ok(russh_sftp::protocol::Attrs { id, attrs })
    }
}
