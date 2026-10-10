//! Session probes: terminal output sampling and exit-status decoding.

use crate::context;
use rio_backend::event::EventProxy;

/// The session's visible lines, top to bottom, trailing blanks trimmed.
pub(super) fn printable_lines(ctx: &context::Context<EventProxy>) -> Vec<String> {
    use crate::crosswords::pos::{Column, Line};
    let terminal = ctx.terminal.lock();
    let lines = terminal.screen_lines().min(16);
    let cols = terminal.columns().min(200);
    (0..lines)
        .map(|row| {
            let line = Line(row as i32);
            let text: String = (0..cols)
                .map(|col| match terminal.grid[line][Column(col)].c() {
                    '\0' => ' ',
                    c => c,
                })
                .collect();
            text.trim_end().to_string()
        })
        .collect()
}

/// The last `count` screen lines down to the cursor, top to bottom: where
/// a long-running session's final words (ssh's disconnect message) are,
/// unlike [`printable_lines`], which reads a fresh session from the top.
pub(super) fn lines_up_to_cursor(
    ctx: &context::Context<EventProxy>,
    count: usize,
) -> Vec<String> {
    use crate::crosswords::pos::{Column, Line};
    let terminal = ctx.terminal.lock();
    let cursor = terminal.grid.cursor.pos.row.0.max(0);
    let first = (cursor + 1 - count as i32).max(0);
    let cols = terminal.columns().min(200);
    (first..=cursor)
        .map(|row| {
            let line = Line(row);
            let text: String = (0..cols)
                .map(|col| match terminal.grid[line][Column(col)].c() {
                    '\0' => ' ',
                    c => c,
                })
                .collect();
            text.trim_end().to_string()
        })
        .collect()
}

/// The exit code in a `ChildExited` status: Unix reports the raw wait
/// status, Windows the code itself.
pub(super) fn exit_code(status: Option<i32>) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.and_then(|raw| std::process::ExitStatus::from_raw(raw).code())
    }
    #[cfg(not(unix))]
    {
        status
    }
}

/// The signal that killed the process in a `ChildExited` status, if one did.
pub(super) fn exit_signal(status: Option<i32>) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.and_then(|raw| std::process::ExitStatus::from_raw(raw).signal())
    }
    #[cfg(not(unix))]
    {
        // Windows reports an exit code only.
        let _ = status;
        None
    }
}

/// How the session's process ended, from its `ChildExited` status.
pub(super) fn session_end(status: Option<i32>) -> terminus_ui::lost_session::SessionEnd {
    terminus_ui::lost_session::classify_exit(exit_code(status), exit_signal(status))
}

/// A pane's `layout_rect` (physical pixels, relative to the grid root) as
/// the chrome's logical rect: offset by the grid margin, then unscaled the
/// way `apply_taffy_layout` positions the pane.
pub(super) fn pane_chrome_rect(
    layout_rect: [f32; 4],
    margin_left: f32,
    margin_top: f32,
    scale: f32,
) -> terminus_ui::geom::Rect {
    terminus_ui::geom::Rect {
        x: (layout_rect[0] + margin_left) / scale,
        y: (layout_rect[1] + margin_top) / scale,
        width: layout_rect[2] / scale,
        height: layout_rect[3] / scale,
    }
}

/// True when the session's grid already shows something other than blank
/// cells — the cue that the connecting overlay can retire.
pub(super) fn terminal_has_printable_output(ctx: &context::Context<EventProxy>) -> bool {
    use crate::crosswords::pos::{Column, Line};
    let terminal = ctx.terminal.lock();
    let lines = terminal.screen_lines().min(16);
    let cols = terminal.columns().min(120);
    for row in 0..lines {
        let line = Line(row as i32);
        for col in 0..cols {
            let c = terminal.grid[line][Column(col)].c();
            if !c.is_whitespace() && c != '\0' {
                return true;
            }
        }
    }
    false
}
