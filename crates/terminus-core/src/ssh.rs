use crate::error::{Error, Result};
use crate::models::{Host, Identity, SftpEntry};
use russh::client::{self, Handle};
use russh::keys::ssh_key::PublicKey;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg};
use russh::{Channel, ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// russh client handler that verifies the server host key against known_hosts
/// (fail closed: unknown and mismatched keys are rejected).
pub struct HostKeyVerifier {
    hostname: String,
    port: u16,
    /// When set, check this known_hosts file instead of `~/.ssh/known_hosts`.
    known_hosts_path: Option<PathBuf>,
}

impl HostKeyVerifier {
    pub fn new(hostname: impl Into<String>, port: u16) -> Self {
        Self {
            hostname: hostname.into(),
            port,
            known_hosts_path: std::env::var_os("TERMINUS_KNOWN_HOSTS").map(PathBuf::from),
        }
    }
}

impl client::Handler for HostKeyVerifier {
    type Error = Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        verify_host_key(
            &self.hostname,
            self.port,
            server_public_key,
            self.known_hosts_path.as_deref(),
        )
    }
}

/// SHA256 fingerprint + algorithm for a presented host key.
#[derive(Debug, Clone, Serialize)]
pub struct HostKeyFingerprint {
    pub algo: String,
    pub sha256: String,
}

/// Resolve known_hosts path: `TERMINUS_KNOWN_HOSTS` or `~/.ssh/known_hosts`.
pub fn resolve_known_hosts_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("TERMINUS_KNOWN_HOSTS") {
        return Ok(PathBuf::from(path));
    }
    let home = dirs::home_dir().ok_or_else(|| Error::msg("home directory required for known_hosts"))?;
    Ok(home.join(".ssh").join("known_hosts"))
}

fn describe_public_key(key: &PublicKey) -> Result<(String, String, String)> {
    let public_key = key
        .to_openssh()
        .map_err(|e| Error::msg(format!("encode public key: {e}")))?;
    let algo = key.algorithm().to_string();
    let fingerprint = format!("{}", key.fingerprint(HashAlg::Sha256));
    Ok((public_key, algo, fingerprint))
}

/// Parse an OpenSSH public key line (`algo base64 [comment]`) or bare base64 blob.
pub fn parse_presented_public_key(public_key: &str) -> Result<PublicKey> {
    let trimmed = public_key.trim();
    if trimmed.is_empty() {
        return Err(Error::msg("empty public key"));
    }
    if let Ok(key) = PublicKey::from_openssh(trimmed) {
        return Ok(key);
    }
    // Accept "algo base64…" even if from_openssh is picky about trailing bits.
    let mut parts = trimmed.split_whitespace();
    let first = parts.next().unwrap_or("");
    let second = parts.next();
    if let Some(b64) = second {
        if first.starts_with("ssh-") || first.starts_with("ecdsa-") || first == "rsa-sha2-256" || first == "rsa-sha2-512" {
            return russh::keys::parse_public_key_base64(b64)
                .map_err(|e| Error::msg(format!("parse public key: {e}")));
        }
    }
    russh::keys::parse_public_key_base64(trimmed)
        .map_err(|e| Error::msg(format!("parse public key: {e}")))
}

/// Compute `{ algo, sha256 }` for a presented public key string.
pub fn host_key_fingerprint(public_key: &str) -> Result<HostKeyFingerprint> {
    let key = parse_presented_public_key(public_key)?;
    let (_, algo, sha256) = describe_public_key(&key)?;
    Ok(HostKeyFingerprint { algo, sha256 })
}

fn format_known_hosts_line(host: &str, port: u16, pubkey: &PublicKey) -> Result<String> {
    let encoded = pubkey
        .to_openssh()
        .map_err(|e| Error::msg(format!("encode public key: {e}")))?;
    if port != 22 {
        Ok(format!("[{host}]:{port} {encoded}"))
    } else {
        Ok(format!("{host} {encoded}"))
    }
}

/// Atomically replace `path` with `contents` via a same-directory temp file.
///
/// - Unix: `rename(tmp, dest)` (replaces existing dest).
/// - Windows: `rename` cannot overwrite, so remove dest (if present) then rename.
///   No extra deps; best-effort atomicity without `MoveFileEx` bindings.
fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("known_hosts");
    let tmp_name = format!(".{file_name}.terminus.tmp");
    let tmp_path = path
        .parent()
        .map(|p| p.join(&tmp_name))
        .unwrap_or_else(|| PathBuf::from(&tmp_name));

    let write_tmp = || -> Result<()> {
        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        Ok(())
    };
    if let Err(err) = write_tmp() {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    let replace = || -> Result<()> {
        #[cfg(windows)]
        {
            // Windows `rename` cannot overwrite an existing destination. Remove
            // first (ignore NotFound), then rename — dep-free cross-platform replace.
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
            fs::rename(&tmp_path, path)?;
        }
        #[cfg(not(windows))]
        {
            fs::rename(&tmp_path, path)?;
        }
        Ok(())
    };

    if let Err(err) = replace() {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

/// Remove a 1-indexed line from known_hosts (single atomic rewrite).
pub fn remove_known_hosts_line(path: &Path, line: usize) -> Result<()> {
    if line == 0 {
        return Err(Error::msg("known_hosts line must be 1-indexed"));
    }
    if !path.exists() {
        return Err(Error::msg("known_hosts file does not exist"));
    }
    let contents = fs::read_to_string(path)?;
    let mut out = String::with_capacity(contents.len());
    let mut found = false;
    for (idx, row) in contents.lines().enumerate() {
        if idx + 1 == line {
            found = true;
            continue;
        }
        out.push_str(row);
        out.push('\n');
    }
    if !found {
        return Err(Error::msg(format!("known_hosts line {line} not found")));
    }
    atomic_write(path, &out)
}

/// Build known_hosts contents: optionally drop `replace_line`, then append `entry`.
fn known_hosts_contents_with_trust(
    existing: &str,
    entry: &str,
    replace_line: Option<usize>,
) -> Result<String> {
    let mut contents = String::with_capacity(existing.len() + entry.len() + 2);
    if let Some(line) = replace_line {
        if line == 0 {
            return Err(Error::msg("known_hosts line must be 1-indexed"));
        }
        let mut found = false;
        for (idx, row) in existing.lines().enumerate() {
            if idx + 1 == line {
                found = true;
                continue;
            }
            contents.push_str(row);
            contents.push('\n');
        }
        if !existing.is_empty() && !found {
            return Err(Error::msg(format!("known_hosts line {line} not found")));
        }
    } else {
        contents.push_str(existing);
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
    }
    contents.push_str(entry);
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    Ok(contents)
}

/// Trust a presented host key with a **single** atomic known_hosts rewrite.
///
/// When `replace_line` is set (mismatch), that 1-indexed line is omitted and the
/// new key is appended in the same write — never a separate remove then append.
///
/// Path = `TERMINUS_KNOWN_HOSTS` or `~/.ssh/known_hosts`. When `path` is `Some`, that
/// file is used instead (tests).
pub fn trust_host_key(
    host: &str,
    port: u16,
    public_key: &str,
    replace_line: Option<usize>,
    path: Option<&Path>,
) -> Result<()> {
    let resolved = match path {
        Some(p) => p.to_path_buf(),
        None => resolve_known_hosts_path()?,
    };
    let key = parse_presented_public_key(public_key)?;
    let entry = format_known_hosts_line(host, port, &key)?;

    let existing = if resolved.exists() {
        fs::read_to_string(&resolved)?
    } else {
        String::new()
    };
    let contents = known_hosts_contents_with_trust(&existing, &entry, replace_line)?;
    atomic_write(&resolved, &contents)
}

/// Verify `server_public_key` against OpenSSH known_hosts.
///
/// Returns `Ok(true)` only when the key matches. Unknown hosts and algorithm
/// mismatches that do not match any recorded key yield [`Error::HostKeyUnknown`].
/// Same-algorithm key changes yield [`Error::HostKeyMismatch`].
pub fn verify_host_key(
    host: &str,
    port: u16,
    server_public_key: &PublicKey,
    known_hosts_path: Option<&Path>,
) -> Result<bool> {
    let (public_key, algo, fingerprint) = describe_public_key(server_public_key)?;
    let checked = match known_hosts_path {
        Some(path) => {
            russh::keys::known_hosts::check_known_hosts_path(host, port, server_public_key, path)
        }
        None => russh::keys::known_hosts::check_known_hosts(host, port, server_public_key),
    };
    match checked {
        Ok(true) => Ok(true),
        Ok(false) => Err(Error::HostKeyUnknown {
            host: host.to_string(),
            port,
            public_key,
            algo,
            fingerprint,
        }),
        Err(russh::keys::Error::KeyChanged { line }) => Err(Error::HostKeyMismatch {
            host: host.to_string(),
            port,
            line,
            public_key,
            algo,
            fingerprint,
        }),
        Err(err) => Err(Error::Ssh(err.into())),
    }
}

pub(crate) async fn connect_handle(
    host: &Host,
    identity: Option<&Identity>,
) -> Result<Handle<HostKeyVerifier>> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });
    let handler = HostKeyVerifier::new(host.hostname.clone(), host.port);
    let mut session =
        client::connect(config, (host.hostname.as_str(), host.port), handler).await?;
    let ok = match host.auth_method.as_str() {
        "key" => authenticate_key(&mut session, host, identity).await?,
        "agent" | "auto" => {
            if authenticate_key(&mut session, host, identity).await.unwrap_or(false) {
                true
            } else {
                let password = host.password.clone().unwrap_or_default();
                !password.is_empty()
                    && session
                        .authenticate_password(&host.username, password)
                        .await?
                        .success()
            }
        }
        _ => {
            let password = host.password.clone().unwrap_or_default();
            session
                .authenticate_password(&host.username, password)
                .await?
                .success()
        }
    };
    if !ok {
        return Err(Error::msg("SSH authentication failed"));
    }
    Ok(session)
}

async fn authenticate_key(
    session: &mut Handle<HostKeyVerifier>,
    host: &Host,
    identity: Option<&Identity>,
) -> Result<bool> {
    let keys = load_auth_keys(identity)?;
    if keys.is_empty() {
        return Err(Error::msg(
            "no SSH private key found (save a key on the host, or put one in ~/.ssh/id_ed25519)",
        ));
    }
    let hash = session.best_supported_rsa_hash().await?.flatten();
    let mut last = String::from("all keys rejected");
    for key in keys {
        match session
            .authenticate_publickey(
                &host.username,
                PrivateKeyWithHashAlg::new(Arc::new(key), hash),
            )
            .await
        {
            Ok(res) if res.success() => return Ok(true),
            Ok(_) => last = "public key rejected by server".into(),
            Err(err) => last = err.to_string(),
        }
    }
    Err(Error::msg(last))
}

fn load_auth_keys(identity: Option<&Identity>) -> Result<Vec<russh::keys::PrivateKey>> {
    let mut keys = Vec::new();
    if let Some(identity) = identity {
        if let Some(material) = identity.private_key.as_deref() {
            keys.push(parse_key(material, identity.passphrase.as_deref())?);
        }
    }
    if keys.is_empty() {
        for path in default_key_paths() {
            if let Ok(key) = load_secret_key(&path, None) {
                keys.push(key);
            }
        }
    }
    Ok(keys)
}

fn parse_key(material: &str, passphrase: Option<&str>) -> Result<russh::keys::PrivateKey> {
    let trimmed = material.trim();
    if trimmed.contains("BEGIN") {
        russh::keys::decode_secret_key(trimmed, passphrase).map_err(|e| Error::msg(e.to_string()))
    } else {
        load_secret_key(expand_path(trimmed), passphrase).map_err(|e| Error::msg(e.to_string()))
    }
}

fn expand_path(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir().unwrap_or_default().join(rest)
    } else if path == "~" {
        dirs::home_dir().unwrap_or_default()
    } else {
        std::path::PathBuf::from(path)
    }
}

pub fn default_key_paths() -> Vec<std::path::PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    ["id_ed25519", "id_ecdsa", "id_rsa", "id_ed25519_sk"]
        .into_iter()
        .map(|name| home.join(".ssh").join(name))
        .filter(|path| path.exists())
        .collect()
}

pub fn default_key_path_strings() -> Vec<String> {
    default_key_paths()
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

pub enum SshCommand {
    Data(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Close,
}

pub async fn open_shell(
    host: &Host,
    identity: Option<&Identity>,
    cols: u16,
    rows: u16,
    output: mpsc::UnboundedSender<Result<Vec<u8>>>,
) -> Result<mpsc::UnboundedSender<SshCommand>> {
    let handle = connect_handle(host, identity).await?;
    let mut channel = handle.channel_open_session().await?;
    channel
        .request_pty(false, "xterm-256color", cols as u32, rows as u32, 0, 0, &[])
        .await?;
    channel.request_shell(true).await?;
    let (tx, mut rx) = mpsc::unbounded_channel::<SshCommand>();
    tokio::spawn(async move {
        let _handle = handle;
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(SshCommand::Data(bytes)) => {
                            let _ = channel.data(&bytes[..]).await;
                        }
                        Some(SshCommand::Resize { cols, rows }) => {
                            let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                        }
                        Some(SshCommand::Close) | None => {
                            let _ = channel.eof().await;
                            break;
                        }
                    }
                }
                msg = channel.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { ref data }) => {
                            if output.send(Ok(data.to_vec())).is_err() {
                                break;
                            }
                        }
                        Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                            let _ = output.send(Ok(data.to_vec()));
                        }
                        Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break,
                        _ => {}
                    }
                }
            }
        }
    });
    Ok(tx)
}

pub async fn exec_command(
    host: &Host,
    identity: Option<&Identity>,
    command: &str,
) -> Result<String> {
    let handle = connect_handle(host, identity).await?;
    let mut channel = handle.channel_open_session().await?;
    channel.exec(true, command).await?;
    let mut out = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { ref data } => out.extend_from_slice(data),
            ChannelMsg::ExtendedData { ref data, .. } => out.extend_from_slice(data),
            ChannelMsg::Eof | ChannelMsg::Close | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    let _ = handle
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

pub async fn sftp_list(
    host: &Host,
    identity: Option<&Identity>,
    path: &str,
) -> Result<Vec<SftpEntry>> {
    let (_handle, sftp) = sftp_session(host, identity).await?;
    let mut entries = Vec::new();
    let dir = sftp.read_dir(path).await.map_err(|e| Error::msg(e.to_string()))?;
    for entry in dir {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let meta = entry.metadata();
        entries.push(SftpEntry {
            name: name.to_string(),
            path: format!("{}/{}", path.trim_end_matches('/'), name),
            is_dir: meta.is_dir(),
            size: meta.size.unwrap_or(0),
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

pub async fn sftp_read(host: &Host, identity: Option<&Identity>, path: &str) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let (_handle, sftp) = sftp_session(host, identity).await?;
    let mut file = sftp
        .open(path)
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    Ok(buf)
}

pub async fn sftp_write(
    host: &Host,
    identity: Option<&Identity>,
    path: &str,
    data: &[u8],
) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let (_handle, sftp) = sftp_session(host, identity).await?;
    let mut file = sftp
        .create(path)
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    file.write_all(data)
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    file.flush()
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    file.shutdown()
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    Ok(())
}

pub async fn sftp_roundtrip(
    host: &Host,
    identity: Option<&Identity>,
    path: &str,
    data: &[u8],
) -> Result<Vec<u8>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (_handle, sftp) = sftp_session(host, identity).await?;
    {
        let mut file = sftp
            .create(path)
            .await
            .map_err(|e| Error::msg(e.to_string()))?;
        file.write_all(data)
            .await
            .map_err(|e| Error::msg(e.to_string()))?;
        file.shutdown()
            .await
            .map_err(|e| Error::msg(e.to_string()))?;
    }
    let mut file = sftp
        .open(path)
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    Ok(buf)
}

async fn sftp_session(
    host: &Host,
    identity: Option<&Identity>,
) -> Result<(Handle<HostKeyVerifier>, SftpSession)> {
    let handle = connect_handle(host, identity).await?;
    let channel = handle.channel_open_session().await?;
    channel.request_subsystem(true, "sftp").await?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    Ok((handle, sftp))
}

pub async fn start_local_forward(
    host: &Host,
    identity: Option<&Identity>,
    bind_host: &str,
    bind_port: u16,
    dest_host: &str,
    dest_port: u16,
) -> Result<tokio::task::JoinHandle<()>> {
    let handle = connect_handle(host, identity).await?;
    let listener = tokio::net::TcpListener::bind((bind_host, bind_port)).await?;
    let dest_host = dest_host.to_string();
    Ok(tokio::spawn(async move {
        loop {
            let Ok((mut inbound, _)) = listener.accept().await else {
                break;
            };
            let Ok(mut channel) = handle
                .channel_open_direct_tcpip(&dest_host, dest_port as u32, "127.0.0.1", 0)
                .await
            else {
                continue;
            };
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 8192];
                loop {
                    tokio::select! {
                        read = inbound.read(&mut buf) => {
                            match read {
                                Ok(0) | Err(_) => {
                                    let _ = channel.eof().await;
                                    break;
                                }
                                Ok(n) => {
                                    if channel.data(&buf[..n]).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        msg = channel.wait() => {
                            match msg {
                                Some(ChannelMsg::Data { ref data }) => {
                                    if inbound.write_all(data).await.is_err() {
                                        break;
                                    }
                                }
                                Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break,
                                _ => {}
                            }
                        }
                    }
                }
            });
        }
    }))
}

#[allow(dead_code)]
fn _channel_type(_: &Channel<client::Msg>) {}
