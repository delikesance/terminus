//! Glue between the Tunnels view and the host store: builds the `ssh`
//! command of a tunnel exactly like the interactive host tab does.

use super::Screen;
use crate::tunnel_worker::tunnel_ssh_args;
use std::process::Command;
use terminus_ui::views::tunnels::TunnelItem;

impl Screen<'_> {
    /// `ssh -N -L/-R/-D …` for `item` on host `host_id`, using the host's
    /// user/port/identity/askpass from [`Screen::shell_for_row`] (the same
    /// builder as the terminal tab). Local and WSL rows have no SSH server.
    pub(super) fn tunnel_command(
        &self,
        host_id: &str,
        item: &TunnelItem,
    ) -> Result<Command, String> {
        let (shell, env) = self.plain_shell_for_row(host_id)?;
        let shell = shell
            .filter(|s| s.program.as_deref() == Some("ssh"))
            .ok_or_else(|| "Tunnels need an SSH server".to_string())?;
        let env = env.unwrap_or_default();
        let args = tunnel_ssh_args(&shell.args, !env.is_empty(), item);
        let mut cmd = Command::new("ssh");
        cmd.args(args);
        cmd.envs(env);
        Ok(cmd)
    }

    /// Start the tunnel `key` (id or name) of the machine on screen; a
    /// running one is left alone. For the command palette; `Err` is a
    /// message for a toast.
    pub(crate) fn start_tunnel(&mut self, key: &str) -> Result<(), String> {
        let mut ctl = self
            .tunnels
            .take()
            .ok_or_else(|| "Tunnels are not available".to_string())?;
        let host = ctl.host_id().map(str::to_string);
        let result = {
            let this = &*self;
            let mut spawn = |item: &TunnelItem| match &host {
                Some(h) => this.tunnel_command(h, item),
                None => Err("Tunnels need an SSH server".to_string()),
            };
            ctl.start_tunnel(key, &mut spawn)
        };
        self.tunnels = Some(ctl);
        result
    }

    /// Stop the tunnel `key` (id or name) of any machine; stopping a
    /// stopped one is fine. For the command palette.
    pub(crate) fn stop_tunnel(&mut self, key: &str) -> Result<(), String> {
        self.tunnels
            .as_mut()
            .ok_or_else(|| "Tunnels are not available".to_string())?
            .stop_tunnel(key)
    }
}
