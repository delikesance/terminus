use crate::error::{Error, Result};
use crate::models::{Host, Identity, SftpEntry};
use russh::client::{self, Handle};
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg};
use russh::{Channel, ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct AcceptAll;

impl client::Handler for AcceptAll {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        Ok(true)
    }
}

pub(crate) async fn connect_handle(
    host: &Host,
    identity: Option<&Identity>,
) -> Result<Handle<AcceptAll>> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });
    let mut session = client::connect(config, (host.hostname.as_str(), host.port), AcceptAll).await?;
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
    session: &mut Handle<AcceptAll>,
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
    output: mpsc::UnboundedSender<Vec<u8>>,
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
                            if output.send(data.to_vec()).is_err() {
                                break;
                            }
                        }
                        Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                            let _ = output.send(data.to_vec());
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
) -> Result<(Handle<AcceptAll>, SftpSession)> {
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
