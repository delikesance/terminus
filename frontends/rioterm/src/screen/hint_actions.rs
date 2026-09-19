//! `Screen` hint actions surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::context;
use crate::context::renderable::Cursor;
use crate::crosswords::grid::Dimensions;
use crate::crosswords::pos::{Column, Pos};
use crate::mouse::Mouse;
use crate::renderer::Renderer;
use core::fmt::Debug;
use rio_backend::clipboard::{Clipboard, ClipboardType};
use rio_backend::config::Shell;
use rio_backend::crosswords::pos::Line;
use rio_backend::event::EventProxy;
use rio_window::keyboard::ModifiersState;
use rio_window::window::CursorIcon;
use std::ffi::OsStr;

impl Screen<'_> {
    #[inline]
    /// Update hint highlighting based on mouse position and modifiers
    pub fn update_highlighted_hints(&mut self) -> bool {
        // Check if any hint configuration has matching modifiers
        let should_highlight = self.hints_config.iter().any(|hint_config| {
            hint_config.mouse.enabled && self.modifiers_match(&hint_config.mouse.mods)
        });

        let had_highlight = self
            .context_manager
            .current()
            .renderable_content
            .highlighted_hint
            .is_some();

        if !should_highlight {
            return self.clear_highlighted_hint();
        }

        let mods = self.modifiers.state();

        // Mouse events arrive per pixel; when the last probe of this
        // viewport cell with these modifiers found nothing, there is
        // nothing new to learn until one of them changes. The cell is
        // pure geometry, so an unchanged probe skips without even
        // taking the terminal lock. While a highlight is shown the
        // probe always reruns, so text changing under the underline
        // still refreshes it. Wheel scrolling resets the probe in
        // `Self::scroll`; content sliding under a stationary cursor
        // without one is stale until the mouse crosses a cell.
        let viewport_point = self.mouse_position(0);
        if !had_highlight && self.last_hint_probe == Some((viewport_point, mods)) {
            return false;
        }

        let terminal = self.context_manager.current().terminal.lock();
        let display_offset = terminal.display_offset();
        let mouse_point =
            Pos::new(viewport_point.row - display_offset, viewport_point.col);

        // Find hint at mouse position
        let highlighted_hint = self.find_hint_at_point(&terminal, mouse_point, mods);
        drop(terminal);
        self.last_hint_probe = Some((viewport_point, mods));

        let current = self.context_manager.current_mut();

        if let Some(hint_match) = highlighted_hint {
            // Reprobes run on every mouse event while a highlight is
            // shown (so text changing under it refreshes); when the
            // match is the same one already displayed there is nothing
            // to redraw, and re-marking full damage per pixel would
            // rebuild the grid for the whole hover.
            let unchanged = current
                .renderable_content
                .highlighted_hint
                .as_ref()
                .is_some_and(|shown| {
                    shown.start == hint_match.start
                        && shown.end == hint_match.end
                        && shown.text == hint_match.text
                });
            if unchanged {
                return false;
            }

            // Mark the hint range as damaged so it gets re-rendered.
            //
            // Two damage signals are required:
            // * Terminal-side: `update_selection_damage` marks the affected
            // lines so the partial render path knows what to redraw.
            // * Renderer-side: `pending_update.set_terminal_damage(Full)`
            // ensures the render loop doesn't early-exit on
            // `!pending_update.is_dirty()`
            {
                let mut terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();

                let hint_range = rio_backend::selection::SelectionRange::new(
                    hint_match.start,
                    hint_match.end,
                    false,
                );
                terminal.update_selection_damage(Some(hint_range), display_offset);
            }

            current
                .renderable_content
                .pending_update
                .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
            current.renderable_content.highlighted_hint = Some(hint_match);
            true
        } else {
            if current.renderable_content.highlighted_hint.is_some() {
                let mut terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();
                terminal.update_selection_damage(None, display_offset);
            }

            // Force a render so the previously-highlighted line clears.
            if had_highlight {
                current
                    .renderable_content
                    .pending_update
                    .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
            }
            current.renderable_content.highlighted_hint = None;
            had_highlight
        }
    }

    /// Drop any hint highlight, clearing its damage so the line
    /// repaints. Returns whether a highlight existed. Also forgets the
    /// last probed cell: clears run on context switches, where a stale
    /// probe could suppress the first probe of the new panel.
    pub fn clear_highlighted_hint(&mut self) -> bool {
        self.last_hint_probe = None;
        let current = self.context_manager.current_mut();
        let had_highlight = current.renderable_content.highlighted_hint.is_some();

        if had_highlight {
            let mut terminal = current.terminal.lock();
            let display_offset = terminal.display_offset();
            terminal.update_selection_damage(None, display_offset);
        }

        current.renderable_content.highlighted_hint = None;
        had_highlight
    }

    /// Check if current modifiers match the required modifiers
    pub(super) fn modifiers_match(&self, required_mods: &[String]) -> bool {
        if required_mods.is_empty() {
            return true;
        }

        let current_mods = self.modifiers.state();

        for required_mod in required_mods {
            let matches = match required_mod.as_str() {
                "Shift" => current_mods.shift_key(),
                "Control" | "Ctrl" => current_mods.control_key(),
                "Alt" => current_mods.alt_key(),
                "Super" | "Cmd" | "Command" => current_mods.super_key(),
                _ => false,
            };

            if !matches {
                return false;
            }
        }

        true
    }

    /// Find hint at the specified point
    pub(super) fn find_hint_at_point(
        &self,
        terminal: &rio_backend::crosswords::Crosswords<EventProxy>,
        point: rio_backend::crosswords::pos::Pos,
        _modifiers: rio_window::keyboard::ModifiersState,
    ) -> Option<crate::hints::HintMatch> {
        // The logical line under the point is rule-independent:
        // extracted lazily on the first regex rule, then shared across
        // the remaining rules.
        let mut logical_line: Option<Option<crate::hints::LogicalLine>> = None;

        // Check each enabled hint configuration
        for hint_config in &self.hints_config {
            // Check if mouse highlighting is enabled for this hint
            if !hint_config.mouse.enabled {
                continue;
            }

            // Check if current modifiers match the required modifiers for this hint
            if !self.modifiers_match(&hint_config.mouse.mods) {
                continue;
            }

            // Check hyperlinks if enabled
            if hint_config.hyperlinks {
                if let Some(hyperlink_match) =
                    self.find_hyperlink_at_point(terminal, point)
                {
                    return Some(hyperlink_match);
                }
            }

            // Check regex patterns if specified
            if let Some(regex_pattern) = &hint_config.regex {
                if let Some(regex) = self.compiled_hint_regex(regex_pattern) {
                    let line = logical_line.get_or_insert_with(|| {
                        crate::hints::LogicalLine::extract(terminal, point)
                    });
                    if let Some(m) = line.as_ref().and_then(|line| {
                        line.match_at(
                            terminal,
                            point,
                            &regex,
                            hint_config.post_processing,
                        )
                    }) {
                        return Some(crate::hints::HintMatch {
                            text: m.text,
                            start: m.start,
                            end: m.end,
                            hint: hint_config.clone(),
                        });
                    }
                }
            }
        }

        None
    }

    /// Find hyperlink at the specified point
    pub(super) fn find_hyperlink_at_point(
        &self,
        terminal: &rio_backend::crosswords::Crosswords<EventProxy>,
        point: rio_backend::crosswords::pos::Pos,
    ) -> Option<crate::hints::HintMatch> {
        let grid = &terminal.grid;

        // Check if the point is within grid bounds
        if point.row >= grid.total_lines() as i32 || point.col.0 >= grid.columns() {
            return None;
        }

        // Look up the cell's hyperlink via the per-grid extras table.
        // Cells in the same OSC 8 span share an `extras_id`, so we
        // walk left/right comparing the hyperlink itself (extras slots
        // are interned by content, so a cell with combining marks has
        // a different id while belonging to the same link) to find the
        // span boundaries.
        let hyperlink = terminal.cell_hyperlink(point.row, point.col)?;

        let mut start_col = point.col;
        let mut end_col = point.col;

        while start_col > rio_backend::crosswords::pos::Column(0) {
            let prev_col = start_col - 1;
            if terminal.cell_hyperlink(point.row, prev_col).as_ref() == Some(&hyperlink) {
                start_col = prev_col;
            } else {
                break;
            }
        }
        while end_col < grid.columns() - 1 {
            let next_col = end_col + 1;
            if terminal.cell_hyperlink(point.row, next_col).as_ref() == Some(&hyperlink) {
                end_col = next_col;
            } else {
                break;
            }
        }

        // Build a synthetic hint config so the rest of the hint
        // pipeline (highlighting, click action) treats this just like
        // a regex/url match.
        let hint_config = std::rc::Rc::new(rio_backend::config::hints::Hint {
            regex: None,
            hyperlinks: true,
            post_processing: true,
            persist: false,
            action: rio_backend::config::hints::HintAction::Action {
                action: rio_backend::config::hints::HintInternalAction::Open,
            },
            mouse: rio_backend::config::hints::HintMouse::default(),
            binding: None,
        });

        let mut uri = hyperlink.uri().to_string();
        if hint_config.post_processing {
            uri = post_process_hyperlink_uri(&uri);
        }

        Some(crate::hints::HintMatch {
            text: uri,
            start: rio_backend::crosswords::pos::Pos::new(point.row, start_col),
            end: rio_backend::crosswords::pos::Pos::new(point.row, end_col),
            hint: hint_config,
        })
    }

    /// Compiled regex for a hint pattern, from the cache when possible.
    /// A pattern that fails to compile is cached as absent implicitly:
    /// the failed compile repeats, but invalid patterns are a config
    /// error and rare.
    pub(super) fn compiled_hint_regex(
        &self,
        pattern: &str,
    ) -> Option<std::rc::Rc<onig::Regex>> {
        if let Some(regex) = self.hint_regex_cache.borrow().get(pattern) {
            return Some(regex.clone());
        }
        let regex = std::rc::Rc::new(onig::Regex::new(pattern).ok()?);
        self.hint_regex_cache
            .borrow_mut()
            .insert(pattern.to_string(), regex.clone());
        Some(regex)
    }

    /// Whether a hint (regex match or OSC 8 link) is currently highlighted
    /// under the mouse. Only ever true while the hint's mods are held, so
    /// it doubles as "the user is following a link right now".
    #[inline]
    pub fn has_highlighted_hint(&self) -> bool {
        self.highlighted_hint().is_some()
    }

    pub fn highlighted_hint(&self) -> Option<&crate::hints::HintMatch> {
        self.context_manager
            .current()
            .renderable_content
            .highlighted_hint
            .as_ref()
    }

    /// Cursor icon for the current mouse position: a pointer over a
    /// highlighted hint, otherwise the icon the terminal mode calls for.
    #[inline]
    pub fn mouse_cursor_icon(&self) -> CursorIcon {
        if self.has_highlighted_hint() {
            CursorIcon::Pointer
        } else if !self.modifiers.state().shift_key() && self.mouse_mode() {
            CursorIcon::Default
        } else {
            CursorIcon::Text
        }
    }

    /// Execute a hint latched at press time. The latched match is the
    /// payload, not the release-time highlight: the modifier can
    /// change mid-click and swap which hint config the same span
    /// resolves to, and the action that runs must be the one the
    /// press landed on.
    #[inline]
    pub fn open_latched_hint(
        &mut self,
        latched: crate::hints::HintMatch,
        clipboard: &mut Clipboard,
    ) {
        // Clear with damage recorded: an action that steals no focus
        // (Copy) would otherwise leave the underline painted until
        // unrelated output touches those rows.
        self.clear_highlighted_hint();
        self.execute_hint_action(&latched, clipboard);
    }

    /// Hand `target` to the platform's default handler.
    ///
    /// `target` comes from terminal output, so it is attacker-controlled and
    /// must never reach a shell: `cmd /c start` would treat `&` in a URL as a
    /// command separator, and on Unix a launcher gets it as a single argv
    /// entry rather than a command line.
    pub(super) fn open_with_default_handler(&self, target: &str) {
        #[cfg(not(any(target_os = "macos", windows)))]
        self.exec("xdg-open", [target]);

        #[cfg(target_os = "macos")]
        self.exec("open", [target]);

        #[cfg(windows)]
        shell_execute_open(target);
    }

    pub fn exec<I, S>(&self, program: &str, args: I)
    where
        I: IntoIterator<Item = S> + Debug + Copy,
        S: AsRef<OsStr>,
    {
        #[cfg(unix)]
        {
            let main_fd = *self.ctx().current().main_fd;
            let shell_pid = &self.ctx().current().shell_pid;
            match teletypewriter::spawn_daemon(program, args, main_fd, *shell_pid) {
                Ok(_) => tracing::debug!("Launched {} with args {:?}", program, args),
                Err(_) => {
                    tracing::warn!("Unable to launch {} with args {:?}", program, args)
                }
            }
        }

        #[cfg(windows)]
        {
            match teletypewriter::spawn_daemon(program, args) {
                Ok(_) => tracing::debug!("Launched {} with args {:?}", program, args),
                Err(_) => {
                    tracing::warn!("Unable to launch {} with args {:?}", program, args)
                }
            }
        }
    }

    pub(super) fn stop_hint_mode_if_active(&mut self) {
        if self.hint_state.is_active() {
            self.hint_state.stop();
            self.update_hint_state();
        }
    }

    /// Process a new character for keyboard hints
    #[allow(dead_code)]
    pub fn hint_input(&mut self, c: char, clipboard: &mut Clipboard) {
        let terminal = self.context_manager.current().terminal.lock();
        if let Some(hint_match) = self.hint_state.keyboard_input(&*terminal, c) {
            drop(terminal);
            self.execute_hint_action(&hint_match, clipboard);
            // Stop hint mode and update state with proper damage tracking
            self.hint_state.stop();
            self.update_hint_state();
        } else {
            drop(terminal);
            self.update_hint_state();
        }
        self.mark_dirty();
    }

    /// Start hint mode with the given hint configuration
    pub fn start_hint_mode(
        &mut self,
        hint: std::rc::Rc<rio_backend::config::hints::Hint>,
    ) {
        self.hint_state.start(hint);
        let terminal = self.context_manager.current().terminal.lock();
        self.hint_state.update_matches(&*terminal);
        drop(terminal);

        // Update hint state and trigger damage tracking
        self.update_hint_state();

        self.mark_dirty();
    }

    /// What a hint should hand to a launcher: the match text, or the path it
    /// resolves to against the terminal's OSC 7 CWD when it names one that
    /// exists. URLs and non-existent paths come back unchanged.
    pub(super) fn hint_open_target(
        &self,
        hint_match: &crate::hints::HintMatch,
    ) -> String {
        // Cloned so the terminal lock is released before resolving, which
        // goes to the filesystem.
        let cwd = self
            .context_manager
            .current()
            .terminal
            .lock()
            .current_directory
            .clone();
        match crate::hints::resolve_path_for_opening(&hint_match.text, cwd.as_deref()) {
            Some(resolved) => resolved.to_string_lossy().into_owned(),
            None => hint_match.text.clone(),
        }
    }

    /// Execute the action for a selected hint
    pub(super) fn execute_hint_action(
        &mut self,
        hint_match: &crate::hints::HintMatch,
        clipboard: &mut Clipboard,
    ) {
        use rio_backend::config::hints::{HintAction, HintCommand, HintInternalAction};

        match &hint_match.hint.action {
            HintAction::Action { action } => match action {
                HintInternalAction::Copy => {
                    clipboard.set(ClipboardType::Clipboard, hint_match.text.clone());
                }
                HintInternalAction::Paste => {
                    self.paste(&hint_match.text, true);
                }
                HintInternalAction::Select => {
                    // Set selection to the hint match
                    let selection = rio_backend::selection::SelectionRange::new(
                        hint_match.start,
                        hint_match.end,
                        false, // not a block selection
                    );
                    self.context_manager
                        .current_mut()
                        .set_selection(Some(selection));
                    self.mark_dirty();
                }
                HintInternalAction::MoveViModeCursor => {
                    // Move vi mode cursor to hint position
                    let mut terminal = self.context_manager.current().terminal.lock();
                    terminal.vi_mode_cursor.pos = hint_match.start;
                    drop(terminal);
                    self.mark_dirty();
                }
                HintInternalAction::Open => {
                    let target = self.hint_open_target(hint_match);
                    self.open_with_default_handler(&target);
                }
            },
            HintAction::Command { command } => {
                let arg_text = self.hint_open_target(hint_match);

                match command {
                    HintCommand::Simple(program) => {
                        self.exec(program, [&arg_text]);
                    }
                    HintCommand::WithArgs { program, args } => {
                        let mut all_args = args.clone();
                        all_args.push(arg_text);
                        self.exec(program, &all_args);
                    }
                }
            }
        }
    }

    /// Update hint state and trigger appropriate damage tracking
    pub fn update_hint_state(&mut self) {
        use rio_backend::event::TerminalDamage;

        if self.hint_state.is_active() {
            // Update hint labels
            self.update_hint_labels();

            // Update hint matches in renderable content
            let matches: Vec<rio_backend::crosswords::search::Match> = self
                .hint_state
                .matches()
                .iter()
                .map(|hint_match| hint_match.start..=hint_match.end)
                .collect();
            self.context_manager
                .current_mut()
                .renderable_content
                .hint_matches = Some(matches);

            // Hint state changed (search input, label visibility,
            // match selection). The visualization changes per-cell —
            // hint highlights, label glyphs — without touching cell
            // content, so we mark each affected line dirty on the
            // live grid. The next snapshot picks them up via the
            // per-row dirty walk and sets `visible_rows[y].dirty` so
            // GPU emit re-emits those rows. Coarse fallback when we
            // can't compute affected lines: `Full`.
            {
                let current = self.context_manager.current_mut();
                let hint_labels = current.renderable_content.hint_labels.clone();
                let hint_matches = current.renderable_content.hint_matches.clone();
                let mut terminal = current.terminal.lock();
                let display_offset = terminal.display_offset();
                let screen_lines = terminal.screen_lines();

                let visible_grid_line = |line: i32| -> bool {
                    let viewport_row = line + display_offset as i32;
                    viewport_row >= 0 && (viewport_row as usize) < screen_lines
                };
                let mut dirty_lines: Vec<i32> = Vec::new();
                for label in hint_labels.iter().flatten() {
                    let line = label.position.row.0;
                    if visible_grid_line(line) {
                        dirty_lines.push(line);
                    }
                }
                if let Some(hint_matches) = &hint_matches {
                    for hint_match in hint_matches {
                        for line in hint_match.start().row.0..=hint_match.end().row.0 {
                            if visible_grid_line(line) {
                                dirty_lines.push(line);
                            }
                        }
                    }
                }
                let any = !dirty_lines.is_empty();
                for line in dirty_lines {
                    terminal.grid[rio_backend::crosswords::pos::Line(line)].dirty = true;
                }
                drop(terminal);

                current
                    .renderable_content
                    .pending_update
                    .set_terminal_damage(if any {
                        TerminalDamage::Partial
                    } else {
                        TerminalDamage::Full
                    });
            }
        } else if !self.search_active() {
            // Clear hint state only if search is not active,
            // since search also uses hint_matches for highlighting
            self.context_manager
                .current_mut()
                .renderable_content
                .hint_matches = None;
            self.context_manager
                .current_mut()
                .renderable_content
                .hint_labels = None;
            // Force full damage to clear all hint highlights
            let current = self.context_manager.current_mut();
            current
                .renderable_content
                .pending_update
                .set_terminal_damage(TerminalDamage::Full);
        }
    }

    pub(super) fn update_hint_labels(&mut self) {
        use crate::context::renderable::HintLabel;

        let hint_labels = if self.hint_state.is_active() {
            let matches = self.hint_state.matches();
            let visible_labels = self.hint_state.visible_labels();

            let mut labels = Vec::new();
            for (match_index, remaining_label) in visible_labels {
                if let Some(hint_match) = matches.get(match_index) {
                    // Create labels for each character in the hint label
                    for (char_index, &label_char) in remaining_label.iter().enumerate() {
                        let position = rio_backend::crosswords::pos::Pos::new(
                            hint_match.start.row,
                            hint_match.start.col + char_index,
                        );

                        labels.push(HintLabel {
                            position,
                            label: label_char,
                            is_first: char_index == 0, // First character gets different styling
                        });
                    }
                }
            }
            Some(labels)
        } else {
            None
        };

        self.context_manager
            .current_mut()
            .renderable_content
            .hint_labels = hint_labels;
    }
}

/// Open `target` with whatever Windows has registered for it, without a
/// shell in the middle. `ShellExecuteW` takes the target as one string
/// rather than a command line, so metacharacters in it stay data.
#[cfg(windows)]
pub(super) fn shell_execute_open(target: &str) {
    use std::os::windows::ffi::OsStrExt;
    let wide_target: Vec<u16> = std::ffi::OsStr::new(target)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let operation: Vec<u16> = "open\0".encode_utf16().collect();
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            wide_target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };

    // A return at or below 32 is an error code rather than an instance
    // handle. Worth logging, because the symptom of failing here is a click
    // that appears to do nothing at all.
    let code = result as isize;
    if code <= 32 {
        tracing::warn!("ShellExecuteW could not open {target}: code {code}");
    }
}

/// Apply post-processing to hyperlink URIs to remove trailing delimiters and handle uneven brackets.
pub(super) fn post_process_hyperlink_uri(uri: &str) -> String {
    let chars: Vec<char> = uri.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let mut end_idx = chars.len() - 1;
    let mut open_parents = 0;
    let mut open_brackets = 0;

    // First pass: handle uneven brackets/parentheses
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' => open_parents += 1,
            '[' => open_brackets += 1,
            ')' => {
                if open_parents == 0 {
                    // Unmatched closing parenthesis, truncate here
                    end_idx = i.saturating_sub(1);
                    break;
                } else {
                    open_parents -= 1;
                }
            }
            ']' => {
                if open_brackets == 0 {
                    // Unmatched closing bracket, truncate here
                    end_idx = i.saturating_sub(1);
                    break;
                } else {
                    open_brackets -= 1;
                }
            }
            _ => (),
        }
    }

    // Second pass: remove trailing delimiters
    while end_idx > 0 {
        match chars[end_idx] {
            '.' | ',' | ':' | ';' | '?' | '!' | '(' | '[' | '\'' => {
                end_idx = end_idx.saturating_sub(1);
            }
            _ => break,
        }
    }

    chars.into_iter().take(end_idx + 1).collect()
}
