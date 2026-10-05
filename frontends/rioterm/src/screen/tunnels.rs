//! Glue between the Tunnels view and the host store: builds the `ssh`
//! command of a tunnel exactly like the interactive host tab does.

use super::Screen;
use crate::tunnel_worker::tunnel_ssh_args;
use std::process::Command;
use terminus_ui::views::tunnels::TunnelItem;

#[allow(dead_code)] // called once the shell plugs the Tunnels view in
impl Screen<'_> {
    /// `ssh -N -L/-R/-D …` for `item` on host `host_id`, using the host's
    /// user/port/identity/askpass from [`Screen::shell_for_row`] (the same
    /// builder as the terminal tab). Local and WSL rows have no SSH server.
    pub(super) fn tunnel_command(
        &self,
        host_id: &str,
        item: &TunnelItem,
    ) -> Result<Command, String> {
        let (shell, env) = self.shell_for_row(host_id)?;
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
}
