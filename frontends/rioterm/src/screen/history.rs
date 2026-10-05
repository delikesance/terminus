//! History capture and "Run again" for `Screen`.

use super::Screen;
use crate::hosts;

impl Screen<'_> {
    /// `RioEvent::CommandSubmitted` from the pane `route_id`: record it for
    /// the machine that pane belongs to.
    pub fn record_submitted_command(
        &mut self,
        route_id: usize,
        command: &str,
        cwd: Option<String>,
    ) {
        let row_id = self
            .context_manager
            .get_by_route_id(route_id)
            .and_then(|item| item.val.host_id.clone())
            .unwrap_or_else(|| hosts::LOCAL_ID.to_string());
        crate::history_worker::record(&row_id, command, cwd);
    }

    /// Run again: type `command` into the active session and press Enter.
    /// The one place the History view's `RunAgain` action is executed.
    #[allow(dead_code)]
    pub fn run_history_command(&mut self, command: &str) {
        // Strip control characters so a recorded line cannot inject escapes.
        let mut line: String = command.chars().filter(|c| !c.is_control()).collect();
        line.push('\r');
        self.ctx_mut()
            .current_mut()
            .messenger
            .send_write(line.into_bytes());
    }
}
