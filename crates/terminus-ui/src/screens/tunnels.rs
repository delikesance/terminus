//! Tunnels view (stub): port forwards of the selected machine.
//!
//! The next wave adds the list, Start/Stop and "New tunnel"; the shell
//! already shows [`TunnelsState::running`] as the Tunnels tab badge (set
//! `Chrome::shell.tunnel_badge` from it).

use super::{ViewInput, ViewOutcome};
use crate::geom::Rect;

pub const TITLE: &str = "Tunnels";

/// What the Tunnels view asks the frontend to do (none yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelsAction {}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TunnelsState {
    pub machine_name: String,
    /// Running tunnels of the selected machine.
    pub running: u32,
}

impl TunnelsState {
    pub fn body(&self) -> String {
        if self.machine_name.is_empty() {
            "Forward ports between this computer and a server.".into()
        } else {
            format!(
                "Forward ports between this computer and {}.",
                self.machine_name
            )
        }
    }

    pub fn is_clickable(&self, _content: Rect, _x: f32, _y: f32) -> bool {
        false
    }

    pub fn handle(&mut self, _content: Rect, _input: &ViewInput) -> ViewOutcome {
        ViewOutcome::Ignored
    }
}
