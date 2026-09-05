use base64::Engine;
use serde_json::Value;
use std::sync::Arc;
use terminus_core::models::*;
use terminus_core::session::{OutputSink, SessionManager};
use terminus_core::ssh;
use terminus_core::store::Store;
use terminus_core::sync::SyncEngine;
use terminus_core::Error;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    store: Store,
    sessions: Arc<SessionManager>,
    sync: Arc<SyncEngine>,
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
}

fn map_err(err: Error) -> String {
    err.to_string()
}

#[tauri::command]
async fn session_open_local(
    state: State<'_, AppState>,
    cols: u16,
    rows: u16,
) -> Result<SessionInfo, String> {
    let info = state.sessions.open_local(cols, rows).await.map_err(map_err)?;
    apply_session_theme(&state, &info.id).await;
    Ok(info)
}

#[tauri::command]
async fn session_open_ssh(
    state: State<'_, AppState>,
    host_id: String,
    cols: u16,
    rows: u16,
) -> Result<SessionInfo, String> {
    let info = state
        .sessions
        .open_ssh(&host_id, cols, rows)
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
fn session_resize(state: State<'_, AppState>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    state.sessions.resize(&id, cols, rows).map_err(map_err)
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
fn session_list(state: State<'_, AppState>) -> Vec<SessionInfo> {
    state.sessions.list()
}

#[tauri::command]
async fn hosts_list(state: State<'_, AppState>) -> Result<Vec<Host>, String> {
    state.store.list_hosts().await.map_err(map_err)
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
async fn identities_list(state: State<'_, AppState>) -> Result<Vec<Identity>, String> {
    state.store.list_identities().await.map_err(map_err)
}

#[tauri::command]
async fn identities_upsert(
    state: State<'_, AppState>,
    identity: Identity,
) -> Result<Identity, String> {
    state.store.upsert_identity(&identity).await.map_err(map_err)?;
    Ok(identity)
}

#[tauri::command]
fn ssh_default_keys() -> Vec<String> {
    terminus_core::ssh::default_key_path_strings()
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
) -> Result<Vec<SftpEntry>, String> {
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
    ssh::sftp_list(&host, identity.as_ref(), &path)
        .await
        .map_err(map_err)
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
    state.store.upsert_forward(&forward).await.map_err(map_err)?;
    Ok(forward)
}

#[tauri::command]
async fn forward_start(state: State<'_, AppState>, id: String) -> Result<(), String> {
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
    let identity = match &host.identity_id {
        Some(iid) => state.store.get_identity(iid).await.map_err(map_err)?,
        None => None,
    };
    let dest_host = fwd.dest_host.clone().unwrap_or_else(|| "127.0.0.1".into());
    let dest_port = fwd.dest_port.unwrap_or(22);
    ssh::start_local_forward(
        &host,
        identity.as_ref(),
        &fwd.bind_host,
        fwd.bind_port,
        &dest_host,
        dest_port,
    )
    .await
    .map_err(map_err)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
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
                });
                Ok::<(), terminus_core::Error>(())
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session_open_local,
            session_open_ssh,
            session_write,
            session_resize,
            session_close,
            session_frame,
            session_list,
            hosts_list,
            hosts_upsert,
            hosts_delete,
            groups_list,
            groups_upsert,
            identities_list,
            identities_upsert,
            ssh_default_keys,
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
            sync_now,
            sync_status,
            sftp_list,
            forwards_list,
            forwards_upsert,
            forward_start
        ])
        .run(tauri::generate_context!())
        .expect("error while running Terminus");
}
