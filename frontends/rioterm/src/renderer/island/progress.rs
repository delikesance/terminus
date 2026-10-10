use super::*;

impl Island {
    /// Update the progress bar state from an OSC 9;4 report.
    ///
    /// `progress_last_seen` is bumped on every (non-Remove) report so the
    /// stale-bar dismissal timer keeps the bar alive while the TUI is
    /// actively reporting. `progress_started_at` is reset only when the
    /// state actually transitions, so a TUI sending the same `OSC 9;4;3`
    /// every 100 ms (issue #1509) doesn't yank the indeterminate animation
    /// phase back to zero on every report.
    pub fn set_progress_report(&mut self, report: ProgressReport) {
        match report.state {
            ProgressState::Remove => {
                self.progress_state = None;
                self.progress_value = None;
                self.progress_started_at = None;
                self.progress_last_seen = None;
            }
            new_state => {
                let now = Instant::now();
                self.progress_last_seen = Some(now);

                let transitioning = self.progress_state != Some(new_state);
                self.progress_state = Some(new_state);
                self.progress_value = report.progress;
                if transitioning {
                    self.progress_started_at = Some(now);
                }
            }
        }
    }

    /// Check if the island needs continuous rendering (for animations)
    pub fn needs_redraw(&self) -> bool {
        // A held drag doesn't need continuous frames: the floating tab
        // only moves on CursorMoved (which requests its own redraws);
        // only the slide springs animate between input events.
        matches!(self.progress_state, Some(ProgressState::Indeterminate))
            || !self.slide_springs.is_empty()
    }

    /// Check if the progress bar should be auto-dismissed due to timeout.
    /// Uses `progress_last_seen` (heartbeat), not `progress_started_at`, so
    /// a long-running TUI that keeps reporting stays visible.
    pub(super) fn check_progress_timeout(&mut self) {
        if let Some(last_seen) = self.progress_last_seen {
            if last_seen.elapsed().as_secs() >= PROGRESS_BAR_TIMEOUT_SECS {
                self.progress_state = None;
                self.progress_value = None;
                self.progress_started_at = None;
                self.progress_last_seen = None;
            }
        }
    }

    /// Render the progress bar below the tab strip, or at the top when hidden.
    pub(super) fn render_progress_bar(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        window_width: f32,
        scale_factor: f32,
        y_position: f32,
    ) {
        // Check for timeout first
        self.check_progress_timeout();

        let state = match self.progress_state {
            Some(s) => s,
            None => return, // No progress bar to render
        };

        let width = window_width / scale_factor;

        // Determine color based on state
        let color = match state {
            ProgressState::Error => self.progress_bar_error_color,
            _ => self.progress_bar_color,
        };

        match state {
            ProgressState::Remove => {
                // Should not reach here, but just in case
            }
            ProgressState::Set | ProgressState::Error | ProgressState::Pause => {
                // Render progress bar with specific percentage
                let progress = self.progress_value.unwrap_or(0) as f32 / 100.0;
                let bar_width = width * progress;

                if bar_width > 0.0 {
                    crate::renderer::chrome::paint_flat(
                        sugarloaf,
                        &terminus_ui::Rect::new(
                            0.0,
                            y_position,
                            bar_width,
                            PROGRESS_BAR_HEIGHT,
                        ),
                        color,
                        0.0, // Same depth as other rects
                        0,
                    );
                }
            }
            ProgressState::Indeterminate => {
                // For indeterminate, show a pulsing/moving indicator.
                // Phase is anchored to `progress_started_at` (set only on
                // state transition) — using `progress_last_seen` here would
                // freeze the bar at position 0 for any TUI that heartbeats
                // its OSC 9;4;3 faster than `cycle_ms`. (Issue #1509.)
                let elapsed = self
                    .progress_started_at
                    .map(|t| t.elapsed().as_millis() as f32)
                    .unwrap_or(0.0);

                // Move the bar from left to right over 2 seconds, then repeat
                let cycle_ms = 2000.0;
                let position = (elapsed % cycle_ms) / cycle_ms;
                let bar_fraction = 0.2; // 20% of width
                let bar_width = width * bar_fraction;
                let x_pos = position * (width - bar_width);

                crate::renderer::chrome::paint_flat(
                    sugarloaf,
                    &terminus_ui::Rect::new(
                        x_pos,
                        y_position,
                        bar_width,
                        PROGRESS_BAR_HEIGHT,
                    ),
                    color,
                    0.0,
                    0,
                );
            }
        }
    }
}
