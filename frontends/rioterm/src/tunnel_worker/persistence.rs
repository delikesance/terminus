use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use chrono::Utc;
use terminus_core::models::PortForward;
use terminus_core::Store;
use terminus_ui::views::tunnels::{TunnelDraft, TunnelItem, TunnelKind, TunnelStatus};
use uuid::Uuid;

pub(super) enum Cmd {
    List { host_id: String },
    Save { host_id: String, draft: TunnelDraft },
    Delete { host_id: String, id: String },
}

pub(super) enum Ev {
    Listed {
        host_id: String,
        items: Vec<TunnelItem>,
    },
    Failed(String),
}

fn item_from_forward(pf: &PortForward) -> TunnelItem {
    TunnelItem {
        id: pf.id.to_string(),
        name: pf.name.clone(),
        kind: TunnelKind::parse(&pf.kind),
        bind_host: pf.bind_host.clone(),
        bind_port: pf.bind_port,
        dest_host: pf.dest_host.clone().unwrap_or_default(),
        dest_port: pf.dest_port.unwrap_or(0),
        status: TunnelStatus::Stopped,
        error: None,
        stats: None,
    }
}

fn list_items(
    rt: &tokio::runtime::Runtime,
    store: &Store,
    host: Uuid,
) -> Result<Vec<TunnelItem>, String> {
    let mut rows = rt
        .block_on(store.list_forwards(Some(host)))
        .map_err(|e| format!("Could not read tunnels: {e}"))?;
    rows.sort_by_key(|r| (r.created_at, r.id));
    Ok(rows.iter().map(item_from_forward).collect())
}

fn handle(
    rt: &tokio::runtime::Runtime,
    store: &Store,
    cmd: Cmd,
) -> Result<(String, Vec<TunnelItem>), String> {
    let parse = |s: &str| {
        Uuid::parse_str(s).map_err(|_| "Tunnels need an SSH server".to_string())
    };
    match cmd {
        Cmd::List { host_id } => {
            let items = list_items(rt, store, parse(&host_id)?)?;
            Ok((host_id, items))
        }
        Cmd::Save { host_id, draft } => {
            let host = parse(&host_id)?;
            let now = Utc::now();
            let id = match &draft.id {
                Some(id) => {
                    Uuid::parse_str(id).map_err(|_| "Invalid tunnel id".to_string())?
                }
                None => Uuid::new_v4(),
            };
            let pf = PortForward {
                id,
                host_id: host,
                kind: draft.kind.as_str().into(),
                name: draft.name.clone(),
                bind_host: draft.bind_host.clone(),
                bind_port: draft.bind_port,
                dest_host: (!draft.dest_host.is_empty()).then(|| draft.dest_host.clone()),
                dest_port: (draft.dest_port != 0).then_some(draft.dest_port),
                created_at: now,
                updated_at: now,
                deleted_at: None,
            };
            rt.block_on(store.upsert_forward(&pf))
                .map_err(|e| format!("Could not save the tunnel: {e}"))?;
            Ok((host_id, list_items(rt, store, host)?))
        }
        Cmd::Delete { host_id, id } => {
            let host = parse(&host_id)?;
            let id = Uuid::parse_str(&id).map_err(|_| "Invalid tunnel id".to_string())?;
            rt.block_on(store.delete_forward(id))
                .map_err(|e| format!("Could not delete the tunnel: {e}"))?;
            Ok((host_id, list_items(rt, store, host)?))
        }
    }
}

pub(super) fn worker(
    data_dir: PathBuf,
    commands: Receiver<Cmd>,
    events: Sender<Ev>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) {
    let send = |ev: Ev| {
        let _ = events.send(ev);
        if let Some(w) = &wake {
            w();
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return send(Ev::Failed(format!("Tunnels: no runtime ({e})"))),
    };
    let store = match rt.block_on(Store::open(data_dir)) {
        Ok(s) => s,
        Err(e) => {
            return send(Ev::Failed(format!(
                "Tunnels: cannot open the database ({e})"
            )))
        }
    };
    while let Ok(cmd) = commands.recv() {
        match handle(&rt, &store, cmd) {
            Ok((host_id, items)) => send(Ev::Listed { host_id, items }),
            Err(e) => send(Ev::Failed(e)),
        }
    }
}
