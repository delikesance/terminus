use base64::Engine;
use serde_json::Value;
use std::sync::Arc;
use terminus_core::forward_runtime::ForwardRuntime;
use terminus_core::local_fs;
use terminus_core::models::*;
use terminus_core::session::{OutputSink, SessionManager};
use terminus_core::ssh;
use terminus_core::store::Store;
use terminus_core::sync::SyncEngine;
use terminus_core::Error;
use terminus_core::VaultStatus;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

struct AppState {
    store: Store,
    sessions: Arc<SessionManager>,
    sync: Arc<SyncEngine>,
    forwards: Arc<ForwardRuntime>,
}

struct TauriSink {
    app: AppHandle,
}

#[async_trait::async_trait]
impl OutputSink for TauriSink {
    async fn emit_output(&self, session_id: &str, data: &[u8]) {
        let payload = serde_json::json!({
            "id": session_id,
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        });
        let _ = self.app.emit("session://output", payload);
    }

    async fn emit_exit(&self, session_id: &str) {
        let _ = self.app.emit("session://exit", serde_json::json!({ "id": session_id }));
    }

    async fn emit_error(&self, session_id: &str, message: &str) {
        let _ = self.app.emit(
            "session://error",
            serde_json::json!({ "id": session_id, "error": message }),
        );
    }

    async fn emit_hosts_changed(&self) {
        let _ = self.app.emit("hosts://changed", ());
    }

    async fn emit_host_runtime(&self, runtime: &HostRuntime) {
        let _ = self.app.emit("hosts://runtime", runtime);
    }
}

fn map_err(err: Error) -> String {
    err.to_ipc_string()
}

#[tauri::command]
fn local_os_id() -> String {
    terminus_core::os_detect::detect_local_os_id()
}

#[tauri::command]
async fn session_open_local(
    state: State<'_, AppState>,
    cols: u16,
    rows: u16,
    scale: Option<f32>,
) -> Result<SessionInfo, String> {
    let info = state
        .sessions
        .open_local(cols, rows, scale.unwrap_or(1.0))
        .await
        .map_err(map_err)?;
    apply_session_theme(&state, &info.id).await;
    Ok(info)
}

#[tauri::command]
fn wsl_list_distros(force: Option<bool>) -> Result<Vec<terminus_core::wsl::WslDistro>, String> {
    terminus_core::wsl::list_distros_cached(force.unwrap_or(false)).map_err(map_err)
}

#[tauri::command]
async fn session_open_wsl(
    state: State<'_, AppState>,
    distro: String,
    cols: u16,
    rows: u16,
    scale: Option<f32>,
) -> Result<SessionInfo, String> {
    #[cfg(not(windows))]
    {
        let _ = (state, distro, cols, rows, scale);
        return Err("WSL sessions are only available on Windows".into());
    }
    #[cfg(windows)]
    {
        let info = state
            .sessions
            .open_wsl(&distro, cols, rows, scale.unwrap_or(1.0))
            .await
            .map_err(map_err)?;
        apply_session_theme(&state, &info.id).await;
        Ok(info)
    }
}

#[tauri::command]
async fn session_open_ssh(
    state: State<'_, AppState>,
    host_id: String,
    cols: u16,
    rows: u16,
    scale: Option<f32>,
) -> Result<SessionInfo, String> {
    let info = state
        .sessions
        .open_ssh(&host_id, cols, rows, scale.unwrap_or(1.0))
        .await
        .map_err(map_err)?;
    apply_session_theme(&state, &info.id).await;
    Ok(info)
}

async fn apply_session_theme(state: &State<'_, AppState>, session_id: &str) {
    if let Ok(appearance) = state.store.appearance().await {
        if let Some(theme) = ColorTheme::builtins()
            .into_iter()
            .find(|t| t.id == appearance.theme_id)
        {
            let _ = state.sessions.apply_theme(session_id, &theme);
        }
    }
}

#[tauri::command]
async fn session_write(state: State<'_, AppState>, id: String, data: String) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| e.to_string())?;
    state.sessions.write(&id, &bytes).await.map_err(map_err)
}

#[tauri::command]
fn session_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
    scale: Option<f32>,
) -> Result<(), String> {
    state.sessions.resize(&id, cols, rows, scale).map_err(map_err)
}

#[tauri::command]
fn session_close(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.sessions.close(&id).map_err(map_err)
}

#[tauri::command]
fn session_frame(
    state: State<'_, AppState>,
    id: String,
    force: Option<bool>,
) -> Result<Response, String> {
    let bytes = state
        .sessions
        .frame(&id, force.unwrap_or(false))
        .map_err(map_err)?
        .unwrap_or_default();
    Ok(Response::new(bytes))
}

#[tauri::command]
fn session_selection_text(
    state: State<'_, AppState>,
    id: String,
    r0: u16,
    c0: u16,
    r1: u16,
    c1: u16,
) -> Result<String, String> {
    state
        .sessions
        .extract_text(&id, r0, c0, r1, c1)
        .map_err(map_err)
}

#[tauri::command]
fn session_scroll(
    state: State<'_, AppState>,
    id: String,
    lines: i32,
) -> Result<bool, String> {
    state.sessions.scroll(&id, lines).map_err(map_err)
}

#[tauri::command]
fn session_list(state: State<'_, AppState>) -> Vec<SessionInfo> {
    state.sessions.list()
}

#[tauri::command]
async fn hosts_list(state: State<'_, AppState>) -> Result<Vec<Host>, String> {
    state.store.list_hosts().await.map_err(map_err)
}

#[tauri::command]
async fn hosts_runtime(state: State<'_, AppState>) -> Result<Vec<HostRuntime>, String> {
    state.sessions.hosts_runtime().await.map_err(map_err)
}

#[tauri::command]
async fn hosts_upsert(state: State<'_, AppState>, mut host: Host) -> Result<Host, String> {
    host.updated_at = chrono::Utc::now();
    state.store.upsert_host(&host).await.map_err(map_err)?;
    Ok(host)
}

#[tauri::command]
async fn hosts_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.store.delete_host(&id).await.map_err(map_err)
}

#[tauri::command]
async fn groups_list(state: State<'_, AppState>) -> Result<Vec<Group>, String> {
    state.store.list_groups().await.map_err(map_err)
}

#[tauri::command]
async fn groups_upsert(state: State<'_, AppState>, group: Group) -> Result<Group, String> {
    state.store.upsert_group(&group).await.map_err(map_err)?;
    Ok(group)
}

#[tauri::command]
async fn groups_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.store.delete_group(&id).await.map_err(map_err)
}

#[tauri::command]
async fn identities_list(state: State<'_, AppState>) -> Result<Vec<Identity>, String> {
    state.store.list_identities().await.map_err(map_err)
}

#[tauri::command]
async fn identities_upsert(
    state: State<'_, AppState>,
    identity: Identity,
) -> Result<Identity, String> {
    let mut identity = identity;
    identity.normalize_kind();
    if identity.kind == "key" {
        if let Some(material) = identity.private_key.as_deref() {
            let trimmed = material.trim();
            if !trimmed.is_empty() {
                ssh::validate_identity_key(trimmed, identity.passphrase.as_deref())
                    .map_err(map_err)?;
            }
        }
    }
    state.store.upsert_identity(&identity).await.map_err(map_err)?;
    Ok(identity)
}

#[tauri::command]
async fn identities_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.store.delete_identity(&id).await.map_err(map_err)
}

#[tauri::command]
fn ssh_default_keys() -> Vec<String> {
    terminus_core::ssh::default_key_path_strings()
}

#[tauri::command]
fn ssh_host_key_fingerprint(public_key: String) -> Result<Value, String> {
    let fp = ssh::host_key_fingerprint(&public_key).map_err(map_err)?;
    Ok(serde_json::json!({
        "algo": fp.algo,
        "sha256": fp.sha256,
    }))
}

/// Atomically append `public_key` to known_hosts (`TERMINUS_KNOWN_HOSTS` or `~/.ssh/known_hosts`).
/// When `replace_line` is set (mismatch), remove that 1-indexed line first, then append.
#[tauri::command]
fn ssh_host_key_trust(
    host: String,
    port: u16,
    public_key: String,
    replace_line: Option<usize>,
) -> Result<(), String> {
    ssh::trust_host_key(&host, port, &public_key, replace_line, None).map_err(map_err)
}

#[tauri::command]
async fn identity_import_path(
    state: State<'_, AppState>,
    name: String,
    path: String,
    passphrase: Option<String>,
) -> Result<Identity, String> {
    let mut identity = Identity::new(if name.is_empty() {
        std::path::Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("ssh-key")
            .to_string()
    } else {
        name
    });
    identity.private_key = Some(path);
    identity.passphrase = passphrase.filter(|s| !s.is_empty());
    identity.kind = "key".into();
    state.store.upsert_identity(&identity).await.map_err(map_err)?;
    Ok(identity)
}

#[tauri::command]
async fn snippets_list(state: State<'_, AppState>) -> Result<Vec<Snippet>, String> {
    state.store.list_snippets().await.map_err(map_err)
}

#[tauri::command]
async fn snippets_upsert(state: State<'_, AppState>, snippet: Snippet) -> Result<Snippet, String> {
    state.store.upsert_snippet(&snippet).await.map_err(map_err)?;
    Ok(snippet)
}

#[tauri::command]
async fn snippets_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.store.delete_snippet(&id).await.map_err(map_err)
}

#[tauri::command]
async fn history_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<HistoryEntry>, String> {
    state
        .store
        .search_history(&query, limit.unwrap_or(80))
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn history_add(state: State<'_, AppState>, entry: HistoryEntry) -> Result<(), String> {
    state.store.add_history(&entry).await.map_err(map_err)
}

#[tauri::command]
async fn settings_get_all(state: State<'_, AppState>) -> Result<Value, String> {
    Ok(Value::Object(state.store.all_settings().await.map_err(map_err)?))
}

#[tauri::command]
async fn settings_set(state: State<'_, AppState>, key: String, value: Value) -> Result<(), String> {
    state.store.set_setting(&key, &value).await.map_err(map_err)
}

#[tauri::command]
async fn appearance_get(state: State<'_, AppState>) -> Result<TerminalAppearance, String> {
    state.store.appearance().await.map_err(map_err)
}

#[tauri::command]
async fn appearance_set(
    state: State<'_, AppState>,
    appearance: TerminalAppearance,
) -> Result<(), String> {
    state.store.set_appearance(&appearance).await.map_err(map_err)?;
    if let Some(theme) = ColorTheme::builtins()
        .into_iter()
        .find(|t| t.id == appearance.theme_id)
    {
        state.sessions.apply_theme_all(&theme);
    }
    state
        .sessions
        .apply_style_all(appearance.font_size, appearance.line_height);
    Ok(())
}

#[tauri::command]
async fn keybindings_get(state: State<'_, AppState>) -> Result<Value, String> {
    Ok(Value::Object(state.store.keybindings().await.map_err(map_err)?))
}

#[tauri::command]
fn themes_list() -> Vec<ColorTheme> {
    ColorTheme::builtins()
}

#[tauri::command]
async fn sync_configure(state: State<'_, AppState>, config: SyncConfig) -> Result<(), String> {
    state.sync.configure(config).await.map_err(map_err)
}

#[tauri::command]
async fn sync_set_secrets(state: State<'_, AppState>, sync_secrets: bool) -> Result<(), String> {
    state
        .sync
        .set_sync_secrets(sync_secrets)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn vault_create(state: State<'_, AppState>, passphrase: String) -> Result<VaultStatus, String> {
    state.sync.vault_create(&passphrase).await.map_err(map_err)
}

#[tauri::command]
async fn vault_unlock(state: State<'_, AppState>, passphrase: String) -> Result<VaultStatus, String> {
    state.sync.vault_unlock(&passphrase).await.map_err(map_err)
}

#[tauri::command]
async fn vault_lock(state: State<'_, AppState>) -> Result<(), String> {
    state.sync.vault_lock().await;
    Ok(())
}

#[tauri::command]
async fn vault_status(state: State<'_, AppState>) -> Result<VaultStatus, String> {
    Ok(state.sync.vault_status().await)
}

#[tauri::command]
async fn vault_change_passphrase(
    state: State<'_, AppState>,
    passphrase: String,
) -> Result<VaultStatus, String> {
    state
        .sync
        .vault_change_passphrase(&passphrase)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<Value, String> {
    state.sync.sync_now().await.map_err(map_err)
}

#[tauri::command]
async fn sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    Ok(state.sync.status().await)
}

#[tauri::command]
async fn sftp_list(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    root: Option<String>,
) -> Result<Vec<SftpEntry>, String> {
    let root = root.unwrap_or_else(|| {
        if path.starts_with('/') {
            "/".into()
        } else {
            ".".into()
        }
    });
    if terminus_core::wsl_fs::is_wsl_files_host(&host_id) {
        let distro = terminus_core::wsl_fs::require_distro(&host_id).map_err(map_err)?;
        return terminus_core::wsl_fs::list(distro, &root, &path).map_err(map_err);
    }
    let host = state
        .store
        .get_host(&host_id)
        .await
        .map_err(map_err)?
        .ok_or_else(|| "host not found".to_string())?;
    let identity = match &host.identity_id {
        Some(id) => state.store.get_identity(id).await.map_err(map_err)?,
        None => None,
    };
    let entries = ssh::sftp_list(&host, identity.as_ref(), &root, &path)
        .await
        .map_err(map_err)?;
    state.sessions.mark_ssh_connected(&host_id);
    state.sessions.spawn_os_probe(host_id.clone());
    Ok(entries)
}

#[tauri::command]
async fn sftp_read(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    root: Option<String>,
) -> Result<Vec<u8>, String> {
    if terminus_core::wsl_fs::is_wsl_files_host(&host_id) {
        let distro = terminus_core::wsl_fs::require_distro(&host_id).map_err(map_err)?;
        let root = root.unwrap_or_else(|| "/".into());
        return terminus_core::wsl_fs::read(distro, &root, &path)
            .await
            .map_err(map_err);
    }
    let (host, identity, root) = sftp_ctx(&state, &host_id, &path, root).await?;
    ssh::sftp_read(&host, identity.as_ref(), &root, &path)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn sftp_write(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    data: Vec<u8>,
    root: Option<String>,
) -> Result<(), String> {
    if terminus_core::wsl_fs::is_wsl_files_host(&host_id) {
        let distro = terminus_core::wsl_fs::require_distro(&host_id).map_err(map_err)?;
        let root = root.unwrap_or_else(|| "/".into());
        return terminus_core::wsl_fs::write(distro, &root, &path, &data)
            .await
            .map_err(map_err);
    }
    let (host, identity, root) = sftp_ctx(&state, &host_id, &path, root).await?;
    ssh::sftp_write(&host, identity.as_ref(), &root, &path, &data)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn sftp_rename(
    state: State<'_, AppState>,
    host_id: String,
    from: String,
    to: String,
    root: Option<String>,
) -> Result<(), String> {
    if terminus_core::wsl_fs::is_wsl_files_host(&host_id) {
        let distro = terminus_core::wsl_fs::require_distro(&host_id).map_err(map_err)?;
        let root = root.unwrap_or_else(|| "/".into());
        return terminus_core::wsl_fs::rename(distro, &root, &from, &to).map_err(map_err);
    }
    let (host, identity, root) = sftp_ctx(&state, &host_id, &from, root).await?;
    ssh::sftp_rename(&host, identity.as_ref(), &root, &from, &to)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn sftp_realpath(
    state: State<'_, AppState>,
    host_id: String,
    path: Option<String>,
) -> Result<String, String> {
    let query = path.unwrap_or_else(|| ".".into());
    if terminus_core::wsl_fs::is_wsl_files_host(&host_id) {
        let distro = terminus_core::wsl_fs::require_distro(&host_id).map_err(map_err)?;
        return terminus_core::wsl_fs::realpath(distro, &query).map_err(map_err);
    }
    let (host, identity, _) = sftp_ctx(&state, &host_id, &query, Some(".".into())).await?;
    let resolved = ssh::sftp_realpath(&host, identity.as_ref(), &query)
        .await
        .map_err(map_err)?;
    state.sessions.mark_ssh_connected(&host_id);
    state.sessions.spawn_os_probe(host_id.clone());
    Ok(resolved)
}

#[tauri::command]
async fn sftp_open(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    root: Option<String>,
) -> Result<String, String> {
    if terminus_core::wsl_fs::is_wsl_files_host(&host_id) {
        let distro = terminus_core::wsl_fs::require_distro(&host_id).map_err(map_err)?;
        let root = root.unwrap_or_else(|| "/".into());
        let os = terminus_core::wsl_fs::open_os_path(distro, &root, &path).map_err(map_err)?;
        open_path_with_default_app(&os)?;
        return Ok(os.display().to_string());
    }
    let (host, identity, root) = sftp_ctx(&state, &host_id, &path, root).await?;
    let bytes = ssh::sftp_read(&host, identity.as_ref(), &root, &path)
        .await
        .map_err(map_err)?;
    let dest = write_sftp_temp(&path, &bytes)?;
    open_path_with_default_app(&dest)?;
    Ok(dest.display().to_string())
}

fn spawn_detached(program: impl AsRef<OsStr>, args: &[impl AsRef<OsStr>]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn wsl_windows_path(path: &Path) -> Option<String> {
    for wslpath in ["wslpath", "/sbin/wslpath", "/usr/bin/wslpath"] {
        let Ok(out) = Command::new(wslpath).arg("-w").arg(path).output() else {
            continue;
        };
        if !out.status.success() {
            continue;
        }
        let converted = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !converted.is_empty() {
            return Some(converted);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn open_path_impl(path: &Path) -> bool {
    spawn_detached("cmd", &["/C", "start", "", &path.display().to_string()])
}

#[cfg(target_os = "macos")]
fn open_path_impl(path: &Path) -> bool {
    spawn_detached("open", &[path.as_os_str()])
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn open_path_impl(path: &Path) -> bool {
    let file = path.as_os_str();
    if std::env::var_os("WSL_DISTRO_NAME").is_some() {
        if let Some(win) = wsl_windows_path(path) {
            for explorer in ["explorer.exe", "/mnt/c/Windows/explorer.exe"] {
                if spawn_detached(explorer, &[OsStr::new(&win)]) {
                    return true;
                }
            }
        }
    }
    if spawn_detached("xdg-open", &[file]) {
        return true;
    }
    if spawn_detached("gio", &[OsStr::new("open"), file]) {
        return true;
    }
    for bin in [
        "/usr/bin/xdg-open",
        "/run/current-system/sw/bin/xdg-open",
        "/usr/bin/gio",
    ] {
        let args: Vec<&OsStr> = if bin.ends_with("gio") {
            vec![OsStr::new("open"), file]
        } else {
            vec![file]
        };
        if spawn_detached(bin, &args) {
            return true;
        }
    }
    false
}

fn open_path_with_default_app(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!(
            "couldn't open with the default app: missing file {}",
            path.display()
        ));
    }
    if open_path_impl(path) {
        Ok(())
    } else {
        Err(format!(
            "couldn't open with the default app: no opener found. File saved to {}",
            path.display()
        ))
    }
}

fn write_sftp_temp(remote_path: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let name = Path::new(remote_path)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty() && *n != "." && *n != "..")
        .unwrap_or("file");
    let safe: String = name
        .chars()
        .filter(|c| *c != '/' && *c != '\\')
        .collect();
    let safe = if safe.is_empty() { "file".into() } else { safe };
    let dest_dir = std::env::temp_dir().join("terminus-sftp-open");
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("temp dir: {e}"))?;
    let path = Path::new(&safe);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let unique = format!("{stem}-{}{ext}", &Uuid::new_v4().to_string()[..8]);
    let dest = dest_dir.join(unique);
    std::fs::write(&dest, bytes).map_err(|e| format!("temp write: {e}"))?;
    Ok(dest)
}

#[tauri::command]
async fn sftp_remove(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    is_dir: bool,
    root: Option<String>,
) -> Result<(), String> {
    if terminus_core::wsl_fs::is_wsl_files_host(&host_id) {
        let distro = terminus_core::wsl_fs::require_distro(&host_id).map_err(map_err)?;
        let root = root.unwrap_or_else(|| "/".into());
        return terminus_core::wsl_fs::remove(distro, &root, &path, is_dir).map_err(map_err);
    }
    let (host, identity, root) = sftp_ctx(&state, &host_id, &path, root).await?;
    ssh::sftp_remove(&host, identity.as_ref(), &root, &path, is_dir)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn sftp_mkdir(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    root: Option<String>,
) -> Result<(), String> {
    if terminus_core::wsl_fs::is_wsl_files_host(&host_id) {
        let distro = terminus_core::wsl_fs::require_distro(&host_id).map_err(map_err)?;
        let root = root.unwrap_or_else(|| "/".into());
        return terminus_core::wsl_fs::mkdir(distro, &root, &path)
            .await
            .map_err(map_err);
    }
    let (host, identity, root) = sftp_ctx(&state, &host_id, &path, root).await?;
    ssh::sftp_mkdir(&host, identity.as_ref(), &root, &path)
        .await
        .map_err(map_err)
}

#[tauri::command]
async fn sftp_rmtree(
    state: State<'_, AppState>,
    host_id: String,
    path: String,
    root: Option<String>,
) -> Result<(), String> {
    if terminus_core::wsl_fs::is_wsl_files_host(&host_id) {
        let distro = terminus_core::wsl_fs::require_distro(&host_id).map_err(map_err)?;
        let root = root.unwrap_or_else(|| "/".into());
        return terminus_core::wsl_fs::rmtree(distro, &root, &path).map_err(map_err);
    }
    let (host, identity, root) = sftp_ctx(&state, &host_id, &path, root).await?;
    ssh::sftp_rmtree(&host, identity.as_ref(), &root, &path)
        .await
        .map_err(map_err)
}

#[tauri::command]
fn local_home() -> Result<String, String> {
    local_fs::local_home().map_err(map_err)
}

#[tauri::command]
async fn local_list(path: String) -> Result<Vec<LocalEntry>, String> {
    local_fs::local_list(&path).map_err(map_err)
}

#[tauri::command]
async fn local_read(path: String) -> Result<Vec<u8>, String> {
    local_fs::local_read(&path).await.map_err(map_err)
}

#[tauri::command]
async fn local_write(path: String, data: Vec<u8>) -> Result<(), String> {
    local_fs::local_write(&path, &data).await.map_err(map_err)
}

#[tauri::command]
async fn local_mkdir(path: String) -> Result<(), String> {
    local_fs::local_mkdir(&path).await.map_err(map_err)
}

#[tauri::command]
fn local_remove(path: String, is_dir: bool) -> Result<(), String> {
    local_fs::local_remove(&path, is_dir).map_err(map_err)
}

#[tauri::command]
fn local_rename(from: String, to: String) -> Result<(), String> {
    local_fs::local_rename(&from, &to).map_err(map_err)
}

async fn sftp_ctx(
    state: &State<'_, AppState>,
    host_id: &str,
    path: &str,
    root: Option<String>,
) -> Result<(Host, Option<Identity>, String), String> {
    let host = state
        .store
        .get_host(host_id)
        .await
        .map_err(map_err)?
        .ok_or_else(|| "host not found".to_string())?;
    let identity = match &host.identity_id {
        Some(id) => state.store.get_identity(id).await.map_err(map_err)?,
        None => None,
    };
    let root = root.unwrap_or_else(|| {
        if path.starts_with('/') {
            "/".into()
        } else {
            ".".into()
        }
    });
    Ok((host, identity, root))
}

#[tauri::command]
async fn forwards_list(state: State<'_, AppState>) -> Result<Vec<PortForward>, String> {
    state.store.list_forwards().await.map_err(map_err)
}

#[tauri::command]
async fn forwards_upsert(
    state: State<'_, AppState>,
    forward: PortForward,
) -> Result<PortForward, String> {
    // AC8: editing a running forward auto-stops first.
    state.forwards.stop_if_running(&forward.id);
    state.store.upsert_forward(&forward).await.map_err(map_err)?;
    Ok(forward)
}

#[tauri::command]
async fn forwards_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut fwd = state
        .store
        .list_forwards()
        .await
        .map_err(map_err)?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| "forward not found".to_string())?;
    state.forwards.stop_if_running(&id);
    fwd.deleted_at = Some(chrono::Utc::now());
    fwd.updated_at = chrono::Utc::now();
    state.store.upsert_forward(&fwd).await.map_err(map_err)?;
    Ok(())
}

#[tauri::command]
fn forwards_running(state: State<'_, AppState>) -> Vec<String> {
    state.forwards.running_ids().into_iter().collect()
}

#[tauri::command]
async fn forward_start(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.forwards.is_running(&id) {
        return Err(format!("forward {id} is already running"));
    }
    let fwd = state
        .store
        .list_forwards()
        .await
        .map_err(map_err)?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| "forward not found".to_string())?;
    let host = state
        .store
        .get_host(&fwd.host_id)
        .await
        .map_err(map_err)?
        .ok_or_else(|| "host not found".to_string())?;
    if host.deleted_at.is_some() {
        return Err("host was deleted".to_string());
    }
    let identity = match &host.identity_id {
        Some(iid) => state.store.get_identity(iid).await.map_err(map_err)?,
        None => None,
    };
    let dest_host = fwd.dest_host.clone().unwrap_or_else(|| "127.0.0.1".into());
    let dest_port = fwd
        .dest_port
        .ok_or_else(|| "destination port is required".to_string())?;
    let handle = ssh::start_local_forward(
        &host,
        identity.as_ref(),
        &fwd.bind_host,
        fwd.bind_port,
        &dest_host,
        dest_port,
    )
    .await
    .map_err(map_err)?;
    state.forwards.insert(id, handle)?;
    Ok(())
}

#[tauri::command]
fn forward_stop(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.forwards.stop(&id) {
        Ok(())
    } else {
        Err("forward is not running".to_string())
    }
}

/// Playwright / E2E-only: set a host's connection state without a real SSH dial.
/// Compiled in **only** when `TERMINUS_E2E=1` at build time (see `build.rs`).
#[cfg(terminus_e2e)]
#[tauri::command]
fn test_set_host_connection(
    state: State<'_, AppState>,
    host_id: String,
    connection_state: String,
) -> Result<(), String> {
    state
        .sessions
        .test_set_connection(&host_id, &connection_state)
        .map_err(map_err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let store = Store::open_default().await?;
                let sync = Arc::new(SyncEngine::new(store.clone()));
                let _ = sync.restore().await;
                let sink: Arc<dyn OutputSink> = Arc::new(TauriSink {
                    app: handle.clone(),
                });
                let sessions = SessionManager::new(store.clone(), sink);
                handle.manage(AppState {
                    store,
                    sessions,
                    sync,
                    forwards: Arc::new(ForwardRuntime::new()),
                });
                Ok::<(), terminus_core::Error>(())
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            local_os_id,
            session_open_local,
            wsl_list_distros,
            session_open_wsl,
            session_open_ssh,
            session_write,
            session_resize,
            session_close,
            session_frame,
            session_selection_text,
            session_scroll,
            session_list,
            hosts_list,
            hosts_runtime,
            hosts_upsert,
            hosts_delete,
            groups_list,
            groups_upsert,
            groups_delete,
            identities_list,
            identities_upsert,
            identities_delete,
            ssh_default_keys,
            ssh_host_key_fingerprint,
            ssh_host_key_trust,
            identity_import_path,
            snippets_list,
            snippets_upsert,
            snippets_delete,
            history_search,
            history_add,
            settings_get_all,
            settings_set,
            appearance_get,
            appearance_set,
            keybindings_get,
            themes_list,
            sync_configure,
            sync_set_secrets,
            vault_create,
            vault_unlock,
            vault_lock,
            vault_status,
            vault_change_passphrase,
            sync_now,
            sync_status,
            sftp_list,
            sftp_read,
            sftp_write,
            sftp_rename,
            sftp_realpath,
            sftp_open,
            sftp_remove,
            sftp_mkdir,
            sftp_rmtree,
            local_home,
            local_list,
            local_read,
            local_write,
            local_mkdir,
            local_remove,
            local_rename,
            forwards_list,
            forwards_upsert,
            forwards_delete,
            forwards_running,
            forward_start,
            forward_stop,
            #[cfg(terminus_e2e)]
            test_set_host_connection
        ])
        .run(tauri::generate_context!())
        .expect("error while running Terminus");
}
