//! Adapter between the command palette and the tunnels controller.
//!
//! The palette's `>forward` rows list and start/stop tunnels only through
//! this file, on top of `Screen::{start_tunnel, stop_tunnel}` (idempotent,
//! by id) and `TunnelController::state`. If that API changes, only this
//! file changes.
//!
//! Surface (all on `Screen`):
//! - [`Screen::palette_tunnel_items`]: snapshot of the on-screen machine's tunnels.
//! - [`Screen::palette_tunnel_start`] / [`Screen::palette_tunnel_stop`]:
//!   `Ok(notice)` / `Err(message)` for the panel.

use super::Screen;
use crate::renderer::command_palette::{TunnelOp, TunnelPaletteItem};
use terminus_ui::views::tunnels::{TunnelItem, TunnelStatus};

/// Toast for a start/stop of tunnel `id`, decided from the live list before
/// the call (the controller's start/stop are idempotent and say nothing
/// about whether they did anything). An unknown id falls back to the id.
pub(super) fn notice(items: &[TunnelItem], id: &str, op: TunnelOp) -> String {
    let found = items.iter().find(|t| t.id == id);
    let name = found.map_or(id, |t| t.name.as_str());
    let active = found.is_some_and(|t| t.status.is_active());
    match (op, active) {
        (TunnelOp::Start, true) => format!("{name} is already running"),
        (TunnelOp::Start, false) => format!("Starting tunnel {name}\u{2026}"),
        (TunnelOp::Stop, false) if found.is_some() => format!("{name} is not running"),
        (TunnelOp::Stop, _) => format!("Stopped tunnel {name}"),
        (TunnelOp::Show, _) => String::new(),
    }
}

/// The palette's view of one tunnel.
pub(super) fn palette_item(item: &TunnelItem) -> TunnelPaletteItem {
    let state = match item.status {
        TunnelStatus::Stopped => "Stopped",
        TunnelStatus::Starting => "Starting",
        TunnelStatus::Running => "Running",
        TunnelStatus::Failed => "Failed",
    };
    TunnelPaletteItem {
        id: item.id.clone(),
        name: item.name.clone(),
        hint: format!("{} \u{b7} {state}", item.route()),
        active: item.status.is_active(),
    }
}

impl Screen<'_> {
    /// Tunnels of the machine on screen (empty without an SSH machine).
    pub(super) fn palette_tunnel_items(&self) -> Vec<TunnelPaletteItem> {
        self.tunnels
            .as_ref()
            .map(|c| c.state().items.iter().map(palette_item).collect())
            .unwrap_or_default()
    }

    /// Start tunnel `id` of the machine on screen (idempotent).
    pub(super) fn palette_tunnel_start(&mut self, id: &str) -> Result<String, String> {
        let said = self.palette_tunnel_notice(id, TunnelOp::Start);
        self.start_tunnel(id).map(|()| said)
    }

    /// Stop tunnel `id` (idempotent).
    pub(super) fn palette_tunnel_stop(&mut self, id: &str) -> Result<String, String> {
        let said = self.palette_tunnel_notice(id, TunnelOp::Stop);
        self.stop_tunnel(id).map(|()| said)
    }

    fn palette_tunnel_notice(&self, id: &str, op: TunnelOp) -> String {
        let items = self.tunnels.as_ref().map(|c| c.state().items.as_slice());
        notice(items.unwrap_or_default(), id, op)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terminus_ui::views::tunnels::TunnelKind;

    fn tunnel(id: &str, name: &str, status: TunnelStatus) -> TunnelItem {
        TunnelItem {
            id: id.into(),
            name: name.into(),
            kind: TunnelKind::Local,
            bind_host: "127.0.0.1".into(),
            bind_port: 8080,
            dest_host: "db".into(),
            dest_port: 5432,
            status,
            error: None,
            stats: None,
        }
    }

    #[test]
    fn start_notice_depends_on_the_live_state() {
        for status in [TunnelStatus::Stopped, TunnelStatus::Failed] {
            let items = [tunnel("a", "db", status)];
            assert_eq!(
                notice(&items, "a", TunnelOp::Start),
                "Starting tunnel db\u{2026}"
            );
        }
        for status in [TunnelStatus::Starting, TunnelStatus::Running] {
            let items = [tunnel("a", "db", status)];
            assert_eq!(
                notice(&items, "a", TunnelOp::Start),
                "db is already running"
            );
        }
    }

    #[test]
    fn stop_notice_depends_on_the_live_state() {
        let running = [tunnel("a", "db", TunnelStatus::Running)];
        assert_eq!(notice(&running, "a", TunnelOp::Stop), "Stopped tunnel db");
        let stopped = [tunnel("a", "db", TunnelStatus::Stopped)];
        assert_eq!(notice(&stopped, "a", TunnelOp::Stop), "db is not running");
    }

    #[test]
    fn an_unknown_id_is_named_by_the_id() {
        let items = [tunnel("a", "db", TunnelStatus::Stopped)];
        assert_eq!(
            notice(&items, "zz", TunnelOp::Start),
            "Starting tunnel zz\u{2026}"
        );
        assert_eq!(notice(&items, "zz", TunnelOp::Stop), "Stopped tunnel zz");
    }

    #[test]
    fn show_has_nothing_to_say() {
        let items = [tunnel("a", "db", TunnelStatus::Stopped)];
        assert_eq!(notice(&items, "a", TunnelOp::Show), "");
    }

    #[test]
    fn palette_item_carries_name_route_state_and_activity() {
        let item = palette_item(&tunnel("a", "db", TunnelStatus::Running));
        assert_eq!(item.id, "a");
        assert_eq!(item.name, "db");
        assert_eq!(item.hint, "localhost:8080 \u{2192} db:5432 \u{b7} Running");
        assert!(item.active);
        assert!(!palette_item(&tunnel("b", "x", TunnelStatus::Failed)).active);
    }
}
