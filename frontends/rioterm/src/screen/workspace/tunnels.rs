//! Tunnels view (`terminus_ui::views::tunnels`, run by
//! `crate::tunnel_worker::TunnelController`): follows the selected
//! machine, routes input, starts `ssh -N` through
//! [`Screen::tunnel_command`], and surfaces failures.

use super::super::Screen;
use crate::tunnel_worker::{SpawnFn, TunnelController};
use terminus_ui::screens::{text_edit, ViewInput, ViewKey, ViewOutcome};
use terminus_ui::shell::MachineInfo;
use terminus_ui::views::tunnels::{FormKey, TunnelItem, TunnelStatus};

impl Screen<'_> {
    /// Run `f` with the controller and a spawner that builds the `ssh`
    /// command of a tunnel for the controller's machine.
    fn with_tunnels<R>(
        &mut self,
        f: impl FnOnce(&mut TunnelController, SpawnFn) -> R,
    ) -> Option<R> {
        let mut ctl = self.tunnels.take()?;
        let host = ctl.host_id().map(str::to_string);
        let out = {
            let this = &*self;
            let mut spawn = |item: &TunnelItem| match &host {
                Some(h) => this.tunnel_command(h, item),
                None => Err("Tunnels need an SSH server".to_string()),
            };
            f(&mut ctl, &mut spawn)
        };
        self.tunnels = Some(ctl);
        Some(out)
    }

    /// Follow the selected machine, process worker answers and tunnel
    /// exits, start a just-created tunnel, and publish the tab badge and
    /// failures. Cheap; runs every frame.
    pub(super) fn sync_tunnels(&mut self, machine: &MachineInfo, has_ssh: bool) {
        let Some(ctl) = self.tunnels.as_mut() else {
            return;
        };
        if has_ssh {
            if ctl.host_id() != Some(machine.id.as_str()) {
                ctl.select_machine(&machine.id, &machine.name);
            }
        } else {
            ctl.clear_machine(&machine.name);
        }
        let starting = self.tunnels.as_ref().is_some_and(|c| {
            c.state()
                .items
                .iter()
                .any(|t| t.status == TunnelStatus::Starting)
        });
        if starting {
            self.wake_after_settle();
        }
        let Some(ctl) = self.tunnels.as_mut() else {
            return;
        };
        let notices = ctl.take_notices();
        self.chrome.shell.tunnel_badge = ctl.running_count() as u32;
        if let Some(last) = notices.into_iter().last() {
            self.chrome.panel.error = Some(last);
        }
    }

    /// Worker answers and process events, and the start of a tunnel just
    /// created from the form. Called at the top of each frame; true when
    /// the view changed (the frame must repaint).
    pub(crate) fn tick_tunnels(&mut self) -> bool {
        let changed = self.tunnels.as_mut().is_some_and(|c| c.tick());
        let started = self
            .with_tunnels(|ctl, spawn| ctl.start_pending(spawn))
            .unwrap_or(false);
        changed || started
    }

    /// A starting tunnel turns Running (or Failed) after the settle delay,
    /// with nothing else waking an idle window: ping it once then.
    fn wake_after_settle(&mut self) {
        let Some(wake) = self.sftp_wake.clone() else {
            return;
        };
        let now = std::time::Instant::now();
        if self.tunnel_wake_at.is_some_and(|at| now < at) {
            return;
        }
        let delay =
            crate::tunnel_worker::DEFAULT_SETTLE + std::time::Duration::from_millis(150);
        self.tunnel_wake_at = Some(now + delay);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            wake();
        });
    }

    /// Route one input to the Tunnels view.
    pub(super) fn tunnels_view_input(&mut self, input: &ViewInput) -> ViewOutcome {
        let content = self.chrome.shell.content_rect();
        let redraw = |changed: bool| {
            if changed {
                ViewOutcome::Redraw
            } else {
                ViewOutcome::Consumed
            }
        };
        let no_ssh = self.tunnels.as_ref().is_some_and(|c| c.host_id().is_none());
        match input {
            ViewInput::Press { .. } if no_ssh => {
                self.chrome.panel.error = Some(
                    "Tunnels need an SSH server. Pick one in the sidebar.".to_string(),
                );
                ViewOutcome::Redraw
            }
            ViewInput::Press { x, y, .. } => {
                let (x, y) = (*x, *y);
                redraw(
                    self.with_tunnels(|ctl, spawn| ctl.press(content, x, y, spawn))
                        .unwrap_or(false),
                )
            }
            ViewInput::Move { x, y, .. } => redraw(
                self.tunnels
                    .as_mut()
                    .is_some_and(|c| c.hover(content, *x, *y)),
            ),
            ViewInput::Wheel { .. }
            | ViewInput::Release { .. }
            | ViewInput::ContextPress { .. } => ViewOutcome::Consumed,
            ViewInput::Key { key, mods } => {
                let form_open = self
                    .tunnels
                    .as_ref()
                    .is_some_and(|c| c.state().form.is_some());
                if !form_open {
                    // Esc goes back to the terminal (shell fallback).
                    return ViewOutcome::Ignored;
                }
                let keys: Vec<FormKey> = match key {
                    ViewKey::Text(t) if !mods.ctrl && !mods.logo => {
                        t.chars().map(FormKey::Char).collect()
                    }
                    ViewKey::Tab if mods.shift => vec![FormKey::BackTab],
                    ViewKey::Tab => vec![FormKey::Tab],
                    ViewKey::Enter => vec![FormKey::Enter],
                    ViewKey::Escape => vec![FormKey::Escape],
                    other => text_edit(other, *mods)
                        .map(FormKey::Edit)
                        .into_iter()
                        .collect(),
                };
                let mut changed = false;
                for k in keys {
                    changed |= self
                        .with_tunnels(|ctl, spawn| ctl.key(k, spawn))
                        .unwrap_or(false);
                }
                redraw(changed)
            }
        }
    }
}
