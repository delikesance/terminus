//! Screen sessions: connecting.

use super::super::Screen;
use crate::hosts;
use terminus_ui::sidebar::Badge;

use super::probe::terminal_has_printable_output;

const BACKGROUND_CONNECT_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

impl Screen<'_> {
    pub(in crate::screen) fn begin_session_connecting(&mut self, id: &str) {
        self.demote_connecting_modal();
        self.chrome.panel.begin_connecting(id);
        let now = std::time::Instant::now();
        self.connecting_started = Some(now);
        self.connecting_step_at = Some(now);
        self.connecting_success_at = None;

        let host = self
            .chrome
            .panel
            .rows
            .iter()
            .filter_map(terminus_ui::sidebar::Row::host)
            .find(|item| item.id == id);

        let (title, endpoint, kind) = match host {
            Some(item) if item.badge == Badge::Wsl => (
                item.name.clone(),
                // Distro rows already read "WSL · running · default".
                if item.endpoint.starts_with("WSL") {
                    item.endpoint.clone()
                } else {
                    format!("WSL · {}", item.endpoint)
                },
                terminus_ui::ConnectKind::Wsl,
            ),
            Some(item) => (
                item.name.clone(),
                format!("SSH {}", item.endpoint),
                terminus_ui::ConnectKind::Ssh,
            ),
            None if id.starts_with(hosts::WSL_PREFIX) => (
                id.trim_start_matches(hosts::WSL_PREFIX).to_string(),
                "WSL".to_string(),
                terminus_ui::ConnectKind::Wsl,
            ),
            None => (
                id.to_string(),
                format!("SSH {id}"),
                terminus_ui::ConnectKind::Ssh,
            ),
        };

        self.chrome.connection = Some(match kind {
            terminus_ui::ConnectKind::Wsl => {
                terminus_ui::ConnectionSequence::start_wsl(id, title, endpoint)
            }
            terminus_ui::ConnectKind::Ssh => {
                terminus_ui::ConnectionSequence::start_ssh(id, title, endpoint)
            }
        });
    }

    /// A new connection takes the modal: the one it replaces keeps its
    /// sidebar indicator until its session speaks.
    pub(in crate::screen) fn demote_connecting_modal(&mut self) {
        let Some(host) = self.chrome.connection.as_ref().map(|c| c.host_id.clone())
        else {
            return;
        };
        let started = self
            .connecting_started
            .unwrap_or_else(std::time::Instant::now);
        self.background_connecting.push((host, started));
    }

    /// Retire the indicators of demoted connections whose session printed,
    /// vanished or took too long.
    pub(in crate::screen) fn retire_background_connecting(&mut self) {
        let pending = std::mem::take(&mut self.background_connecting);
        for (host, started) in pending {
            let printed = self.context_manager.contexts_mut().iter().any(|tab| {
                tab.contexts().values().any(|item| {
                    let ctx = item.context();
                    ctx.host_id.as_deref() == Some(host.as_str())
                        && terminal_has_printable_output(ctx)
                })
            });
            let present = self
                .context_manager
                .contexts_mut()
                .iter()
                .any(|tab| tab.live_host_ids().contains(&host));
            if printed || !present || started.elapsed() >= BACKGROUND_CONNECT_LIMIT {
                self.chrome.panel.end_connecting(&host);
            } else {
                self.background_connecting.push((host, started));
            }
        }
    }

    pub(in crate::screen) fn end_session_connecting(&mut self) {
        if let Some(conn) = self.chrome.connection.as_ref() {
            self.chrome.panel.end_connecting(&conn.host_id);
        }
        self.chrome.connection = None;
        self.connecting_started = None;
        self.connecting_step_at = None;
        self.connecting_success_at = None;
    }

    /// Public dismiss from the connection modal's Close button.
    pub fn force_end_connecting(&mut self) {
        self.end_session_connecting();
    }

    /// Advance / retire the connection modal for this frame.
    ///
    /// Returns whether the chrome still needs continuous redraws.
    pub(in crate::screen) fn tick_session_connecting(&mut self) -> bool {
        self.retire_background_connecting();
        if self.chrome.connection.is_none() {
            self.connecting_started = None;
            return !self.background_connecting.is_empty();
        }
        let Some(started) = self.connecting_started else {
            return false;
        };

        let elapsed = started.elapsed();
        const STEP_MS: std::time::Duration = std::time::Duration::from_millis(850);
        const SUCCESS_HOLD: std::time::Duration = std::time::Duration::from_millis(1200);

        // User closed the modal.
        if self.chrome.connection.is_none() {
            return false;
        }

        let give_up = self
            .chrome
            .connection
            .as_ref()
            .map(terminus_ui::ConnectionSequence::give_up_after)
            .unwrap_or_default();
        if elapsed >= give_up {
            self.end_session_connecting();
            return false;
        }

        // Hold the success frame, then dismiss.
        if let Some(ok_at) = self.connecting_success_at {
            if ok_at.elapsed() >= SUCCESS_HOLD {
                self.end_session_connecting();
                return false;
            }
            return true;
        }

        let id = self
            .chrome
            .connection
            .as_ref()
            .map(|c| c.host_id.clone())
            .unwrap_or_default();

        let ctx = self.context_manager.current();
        if ctx.host_id.as_deref() != Some(id.as_str()) && elapsed.as_millis() > 200 {
            self.end_session_connecting();
            return false;
        }

        // Advance steps on a timer so the line fills like the mock.
        let step_at = self.connecting_step_at.unwrap_or(started);
        if step_at.elapsed() >= STEP_MS {
            if let Some(conn) = self.chrome.connection.as_mut() {
                if conn.advance() {
                    self.connecting_step_at = Some(std::time::Instant::now());
                }
            }
        }

        // Ready when the PTY has spoken, or (WSL shells, slow to paint)
        // the last step has been held for a beat.
        let on_last = self
            .chrome
            .connection
            .as_ref()
            .is_some_and(|c| c.step + 1 >= terminus_ui::STEP_COUNT);
        let printed = terminal_has_printable_output(ctx);
        let held = on_last && step_at.elapsed() >= STEP_MS;
        let ready = self
            .chrome
            .connection
            .as_ref()
            .is_some_and(|c| c.is_ready(printed, held));

        if ready {
            if let Some(conn) = self.chrome.connection.as_mut() {
                conn.mark_success();
            }
            self.chrome.panel.end_connecting(&id);
            self.connecting_success_at = Some(std::time::Instant::now());
        }

        true
    }

    /// Looping `0..1` phase for the active-node pulse.
    pub(in crate::screen) fn connecting_phase(&self) -> Option<f32> {
        let started = self.connecting_started?;
        self.chrome.connection.as_ref()?;
        Some(terminus_ui::loading_phase(started.elapsed().as_secs_f32()))
    }
}
