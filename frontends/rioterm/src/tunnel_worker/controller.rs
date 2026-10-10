use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use super::persistence::{worker, Cmd, Ev};
use super::registry::{TunnelEvent, TunnelRegistry};
use super::stats::{local_port_free, sample_connections, SAMPLE_EVERY};
use super::ticker::Ticker;
use super::DEFAULT_SETTLE;
use terminus_ui::geom::Rect;
use terminus_ui::views::tunnels::{
    FormKey, TunnelAction, TunnelDraft, TunnelItem, TunnelKind, TunnelStats,
    TunnelStatus, TunnelsState,
};
use uuid::Uuid;

/// Builds the `ssh` command of a tunnel (the screen glue resolves the host's
/// credentials and calls [`tunnel_ssh_args`]).
pub type SpawnFn<'a> = &'a mut dyn FnMut(&TunnelItem) -> Result<Command, String>;

/// Ids designated by `key` among `(id, name)` candidates: an exact id, else
/// every name equal to it ignoring case.
fn matching_ids(candidates: &[(&str, &str)], key: &str) -> Vec<String> {
    let key = key.trim();
    if let Some((id, _)) = candidates.iter().find(|(id, _)| *id == key) {
        return vec![id.to_string()];
    }
    let key = key.to_lowercase();
    candidates
        .iter()
        .filter(|(_, name)| name.to_lowercase() == key)
        .map(|(id, _)| id.to_string())
        .collect()
}

fn one_match(key: &str, mut ids: Vec<String>) -> Result<String, String> {
    match ids.len() {
        0 => Err(format!("No tunnel named \"{}\"", key.trim())),
        1 => Ok(ids.remove(0)),
        _ => Err(format!(
            "More than one tunnel is named \"{}\"; use its id",
            key.trim()
        )),
    }
}

/// Everything the Tunnels view needs, owned in one place.
pub struct TunnelController {
    commands: Sender<Cmd>,
    events: Receiver<Ev>,
    pub(super) registry: TunnelRegistry,
    state: TunnelsState,
    host_id: Option<String>,
    /// Status of every tunnel that has been started this session.
    statuses: HashMap<String, (TunnelStatus, Option<String>)>,
    host_of: HashMap<String, String>,
    names: HashMap<String, String>,
    notices: Vec<String>,
    /// A tunnel just created from "Start tunnel": start it once listed.
    pending_start: Option<String>,
    /// Live numbers of the active tunnels of the machine on screen.
    stats: HashMap<String, TunnelStats>,
    last_sample: Option<Instant>,
    ticker: Ticker,
}

impl TunnelController {
    /// Start the persistence worker. `wake` runs on the worker thread after
    /// each answer (the app passes a redraw request).
    pub fn spawn(data_dir: PathBuf, wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let (ev_tx, ev_rx) = channel();
        let ticker = Ticker::new(wake.clone());
        let _ = std::thread::Builder::new()
            .name("terminus-tunnels".into())
            .spawn(move || worker(data_dir, cmd_rx, ev_tx, wake));
        Self {
            commands: cmd_tx,
            events: ev_rx,
            registry: TunnelRegistry::new(DEFAULT_SETTLE),
            state: TunnelsState::new(""),
            host_id: None,
            statuses: HashMap::new(),
            host_of: HashMap::new(),
            names: HashMap::new(),
            notices: Vec::new(),
            pending_start: None,
            stats: HashMap::new(),
            last_sample: None,
            ticker,
        }
    }

    #[cfg(test)]
    pub fn with_settle(mut self, settle: std::time::Duration) -> Self {
        self.registry.settle = settle;
        self
    }

    pub fn state(&self) -> &TunnelsState {
        &self.state
    }

    pub fn host_id(&self) -> Option<&str> {
        self.host_id.as_deref()
    }

    /// Show the tunnels of `host_id` (a stored host id). Running tunnels of
    /// other machines keep running.
    pub fn select_machine(&mut self, host_id: &str, label: &str) {
        if self.host_id.as_deref() != Some(host_id) {
            self.state = TunnelsState::new(label);
            self.host_id = Some(host_id.to_string());
        } else {
            self.state.host_label = label.to_string();
        }
        let _ = self.commands.send(Cmd::List {
            host_id: host_id.to_string(),
        });
    }

    /// The machine on screen has no SSH server (this computer, WSL): show
    /// no tunnels and persist nothing. Running tunnels keep running.
    pub fn clear_machine(&mut self, label: &str) {
        if self.host_id.is_some() || self.state.host_label != label {
            self.state = TunnelsState::new(label);
            self.host_id = None;
        }
    }

    /// Running tunnels of the machine on screen: the Tunnels tab badge.
    pub fn running_count(&self) -> usize {
        self.state.running_count()
    }

    #[cfg(test)]
    /// Running tunnels of any machine (sidebar badges).
    pub fn running_count_for(&self, host_id: &str) -> usize {
        self.statuses
            .iter()
            .filter(|(id, (st, _))| {
                *st == TunnelStatus::Running
                    && self.host_of.get(*id).map(String::as_str) == Some(host_id)
            })
            .count()
    }

    /// Persist a validated tunnel for the current machine.
    pub fn save(&mut self, draft: &TunnelDraft) {
        if let Some(host_id) = self.host_id.clone() {
            let _ = self.commands.send(Cmd::Save {
                host_id,
                draft: draft.clone(),
            });
        }
    }

    /// Id of the tunnel of the machine on screen designated by `key`: an
    /// exact id, else a name (case-insensitive). Unknown and ambiguous keys
    /// are errors.
    pub fn resolve_current(&self, key: &str) -> Result<String, String> {
        let candidates: Vec<(&str, &str)> = self
            .state
            .items
            .iter()
            .map(|t| (t.id.as_str(), t.name.as_str()))
            .collect();
        one_match(key, matching_ids(&candidates, key))
    }

    /// Start a tunnel of the machine on screen by id or name (what a command
    /// palette calls). Already running or starting is a no-op `Ok`. On
    /// failure the card turns Failed and `Err` carries a message fit for a
    /// toast (no separate notice is queued: the caller shows the `Err`).
    ///
    /// `spawn` builds the `ssh` command for the machine on screen, so only
    /// that machine's tunnels can be started.
    pub fn start_tunnel(&mut self, key: &str, spawn: SpawnFn) -> Result<(), String> {
        let id = self.resolve_current(key)?;
        if self.registry.is_active(&id) {
            return Ok(());
        }
        let Some(item) = self.state.items.iter().find(|t| t.id == id).cloned() else {
            return Err(format!("No tunnel \"{key}\""));
        };
        let result = self.launch(&item, spawn);
        self.apply_statuses();
        result
    }

    /// Stop a tunnel by id or name. Works for any machine; stopping one that
    /// is not running is a no-op `Ok`. `Err` for an unknown or ambiguous key.
    pub fn stop_tunnel(&mut self, key: &str) -> Result<(), String> {
        let candidates: Vec<(&str, &str)> = self
            .names
            .iter()
            .map(|(id, name)| (id.as_str(), name.as_str()))
            .collect();
        let mut ids = matching_ids(&candidates, key);
        if ids.len() > 1 {
            // The same name on several machines: the running one is meant.
            ids.retain(|id| self.registry.is_active(id));
            if ids.is_empty() {
                // Same name on several machines and none is running.
                return Ok(());
            }
        }
        let id = one_match(key, ids)?;
        self.registry.stop(&id);
        self.statuses.remove(&id);
        self.apply_statuses();
        Ok(())
    }

    /// Spawn the process of `item` and record Starting or Failed.
    fn launch(&mut self, item: &TunnelItem, spawn: SpawnFn) -> Result<(), String> {
        match spawn(item).and_then(|cmd| self.registry.start(&item.id, cmd)) {
            Ok(()) => {
                self.statuses
                    .insert(item.id.clone(), (TunnelStatus::Starting, None));
                Ok(())
            }
            Err(msg) => {
                self.statuses
                    .insert(item.id.clone(), (TunnelStatus::Failed, Some(msg.clone())));
                Err(msg)
            }
        }
    }

    /// Stop (if running) and soft-delete a tunnel.
    pub fn delete(&mut self, id: &str) {
        self.registry.stop(id);
        self.statuses.remove(id);
        self.names.remove(id);
        if let Some(host_id) = self.host_id.clone() {
            let _ = self.commands.send(Cmd::Delete {
                host_id,
                id: id.to_string(),
            });
        }
        self.state.items.retain(|t| t.id != id);
    }

    /// Start a stopped/failed tunnel, stop an active one.
    pub fn toggle(&mut self, id: &str, spawn: SpawnFn) {
        let Some(item) = self.state.items.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        if self.registry.is_active(id) {
            self.registry.stop(id);
            self.statuses.remove(id);
        } else if let Err(msg) = self.launch(&item, spawn) {
            self.notices.push(format!("{}: {msg}", item.name));
        }
        self.apply_statuses();
    }

    /// Start the tunnel created by the last "Start tunnel" once the worker
    /// has listed it. Call after [`tick`](Self::tick); true when it started.
    pub fn start_pending(&mut self, spawn: SpawnFn) -> bool {
        let Some(id) = self.pending_start.clone() else {
            return false;
        };
        if !self.state.items.iter().any(|t| t.id == id) {
            return false;
        }
        self.pending_start = None;
        self.toggle(&id, spawn);
        true
    }

    /// The machine was deleted: stop its tunnels.
    pub fn host_deleted(&mut self, host_id: &str) {
        let ids: Vec<String> = self
            .host_of
            .iter()
            .filter(|(_, h)| h.as_str() == host_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.registry.stop(&id);
            self.statuses.remove(&id);
            self.names.remove(&id);
        }
        if self.host_id.as_deref() == Some(host_id) {
            self.state.items.clear();
        }
    }

    /// App quit: stop every process.
    pub fn shutdown(&mut self) {
        self.registry.stop_all();
        self.statuses.clear();
        self.apply_statuses();
    }

    /// Error messages worth a toast, once each.
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    fn apply_statuses(&mut self) {
        for item in &mut self.state.items {
            match self.statuses.get(&item.id) {
                Some((st, err)) => {
                    item.status = *st;
                    item.error = err.clone();
                }
                None => {
                    item.status = TunnelStatus::Stopped;
                    item.error = None;
                }
            }
            item.stats = if item.status.is_active() {
                self.stats.get(&item.id).copied()
            } else {
                None
            };
        }
    }

    /// Update uptime (every call) and connection samples (about once a
    /// second) of the active tunnels on screen. True when a number changed.
    fn refresh_stats(&mut self) -> bool {
        let due = self.last_sample.is_none_or(|t| t.elapsed() >= SAMPLE_EVERY);
        let mut next = HashMap::new();
        for item in &self.state.items {
            let Some(up) = self.registry.uptime(&item.id) else {
                continue;
            };
            let connections = match self.stats.get(&item.id) {
                Some(prev) if !due => prev.connections,
                _ => self.registry.pid(&item.id).and_then(|pid| {
                    sample_connections(pid, item.kind, item.bind_port, item.dest_port)
                }),
            };
            next.insert(
                item.id.clone(),
                TunnelStats {
                    uptime_secs: up.as_secs(),
                    connections,
                },
            );
        }
        if due {
            self.last_sample = Some(Instant::now());
        }
        self.ticker.set_active(!next.is_empty());
        if next == self.stats {
            return false;
        }
        self.stats = next;
        true
    }

    /// Drain worker answers and process events. True when the view changed.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok(ev) = self.events.try_recv() {
            changed = true;
            match ev {
                Ev::Listed { host_id, items } => {
                    for t in &items {
                        self.host_of.insert(t.id.clone(), host_id.clone());
                        self.names.insert(t.id.clone(), t.name.clone());
                    }
                    if self.host_id.as_deref() == Some(host_id.as_str()) {
                        self.state.items = items;
                        self.apply_statuses();
                    }
                }
                Ev::Failed(msg) => self.notices.push(msg),
            }
        }
        for ev in self.registry.poll() {
            changed = true;
            match ev {
                TunnelEvent::Running(id) => {
                    self.statuses.insert(id, (TunnelStatus::Running, None));
                }
                TunnelEvent::Exited { id, message } => {
                    let name = self
                        .names
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| "Tunnel".into());
                    self.notices.push(format!("{name}: {message}"));
                    self.statuses
                        .insert(id, (TunnelStatus::Failed, Some(message)));
                }
            }
        }
        changed |= self.refresh_stats();
        if changed {
            self.apply_statuses();
        }
        changed
    }

    /// Probe used by the form: a local port is acceptable when nothing
    /// listens on it, or when it is the one the edited tunnel already holds,
    /// and no other tunnel of this machine is configured on it.
    fn port_probe(&self) -> impl Fn(u16) -> bool {
        let editing = self.state.form.as_ref().and_then(|f| f.id.clone());
        let own_active = editing
            .as_deref()
            .and_then(|id| self.state.items.iter().find(|t| t.id == id))
            .filter(|t| t.status.is_active())
            .map(|t| t.bind_port);
        let others: Vec<u16> = self
            .state
            .items
            .iter()
            .filter(|t| t.kind != TunnelKind::Remote && Some(&t.id) != editing.as_ref())
            .map(|t| t.bind_port)
            .collect();
        move |p| !others.contains(&p) && (own_active == Some(p) || local_port_free(p))
    }

    pub(super) fn run(&mut self, action: TunnelAction, spawn: SpawnFn) -> bool {
        match action {
            TunnelAction::None => false,
            TunnelAction::Redraw => true,
            TunnelAction::Save(mut draft) => {
                if draft.id.is_none() {
                    let id = Uuid::new_v4().to_string();
                    draft.id = Some(id.clone());
                    self.pending_start = Some(id);
                }
                self.save(&draft);
                true
            }
            TunnelAction::Toggle(id) => {
                self.toggle(&id, spawn);
                true
            }
            TunnelAction::Delete(id) => {
                self.delete(&id);
                true
            }
        }
    }

    /// Pointer press inside `content`; true when a repaint is needed.
    pub fn press(&mut self, content: Rect, x: f32, y: f32, spawn: SpawnFn) -> bool {
        let probe = self.port_probe();
        let action = self.state.press(content, x, y, &probe);
        self.run(action, spawn)
    }

    /// Key while the dialog is open; true when a repaint is needed.
    pub fn key(&mut self, key: FormKey, spawn: SpawnFn) -> bool {
        let probe = self.port_probe();
        let action = self.state.key(key, &probe);
        self.run(action, spawn)
    }

    pub fn hover(&mut self, content: Rect, x: f32, y: f32) -> bool {
        self.state.hover(content, x, y)
    }
}

impl Drop for TunnelController {
    fn drop(&mut self) {
        self.registry.stop_all();
    }
}
