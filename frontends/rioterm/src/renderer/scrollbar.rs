// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::renderer::components::scroll::paint_thumb;
use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;
use terminus_ui::components::scroll::{ScrollHit, VScroll};

// Layout. Kept `pub` so other UI elements (command palette, future
// overlays) can render a scrollbar that matches the terminal's exactly
// without duplicating the numbers.
pub const SCROLLBAR_WIDTH: f32 = 6.0;
pub const SCROLLBAR_MARGIN: f32 = 2.0;
pub const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 20.0;
// Wider hit area for easier grabbing (terminal-only, no palette drag)
const SCROLLBAR_HIT_WIDTH: f32 = 14.0;

// Timing
pub const FADE_OUT_DELAY_MS: u128 = 2000;
pub const FADE_OUT_DURATION_MS: u128 = 300;

// Colors
pub const SCROLLBAR_COLOR: [f32; 4] = [0.6, 0.6, 0.6, 0.5];
pub const SCROLLBAR_DRAG_COLOR: [f32; 4] = [0.7, 0.7, 0.7, 0.7];

// Depth / order for the terminal-surface scrollbar (render on top of
// content but below overlays). Palette / other UIs pick their own
// values via `draw_thumb`'s parameters.
pub const TERMINAL_DEPTH: f32 = 0.0;
pub const TERMINAL_ORDER: u8 = 5;

/// Fade-in/out opacity for a scrollbar given the timestamp of the most
/// recent scroll event (`None` = never scrolled). `dragging` pins it to
/// fully opaque so a slow drag doesn't fade out under the user's cursor.
///
/// Matches the terminal scrollbar's envelope:
/// - 0.0 before any scroll ever happened
/// - 1.0 for the first `FADE_OUT_DELAY_MS` after a scroll
/// - linear fade over `FADE_OUT_DURATION_MS` back to 0.0
pub fn opacity_from_last_scroll(last_scroll: Option<Instant>, dragging: bool) -> f32 {
    if dragging {
        return 1.0;
    }
    let last_scroll = match last_scroll {
        Some(t) => t,
        None => return 0.0,
    };
    let elapsed = last_scroll.elapsed().as_millis();
    if elapsed < FADE_OUT_DELAY_MS {
        1.0
    } else {
        let fade_elapsed = elapsed - FADE_OUT_DELAY_MS;
        if fade_elapsed >= FADE_OUT_DURATION_MS {
            0.0
        } else {
            1.0 - (fade_elapsed as f32 / FADE_OUT_DURATION_MS as f32)
        }
    }
}

/// Paint a single scrollbar thumb — the one and only way rio renders a
/// scrollbar. Uses `SCROLLBAR_COLOR` (or `SCROLLBAR_DRAG_COLOR` if
/// `dragging`) modulated by `opacity`. `opacity <= 0.0` is a no-op so
/// callers can pipe the fade helper straight in.
///
/// `depth` + `order` let callers place the thumb above their own
/// background layers: the terminal uses `TERMINAL_DEPTH` /
/// `TERMINAL_ORDER` so the bar lives on top of the cell content, the
/// command palette uses a higher order so the bar isn't swallowed by
/// the palette's backdrop/bg rects.
pub fn draw_thumb(
    sugarloaf: &mut Sugarloaf,
    thumb: &terminus_ui::Rect,
    opacity: f32,
    dragging: bool,
    depth: f32,
    order: u8,
) {
    if opacity <= 0.0 {
        return;
    }
    let base = if dragging {
        SCROLLBAR_DRAG_COLOR
    } else {
        SCROLLBAR_COLOR
    };
    let color = [base[0], base[1], base[2], base[3] * opacity];
    paint_thumb(sugarloaf, thumb, color, 0.0, depth, order);
}

/// State for an active scrollbar drag operation.
#[derive(Clone, Copy)]
pub struct ScrollbarDragState {
    /// The rich_text_id of the panel being dragged
    pub rich_text_id: usize,
    /// Y offset within the thumb where the drag started (logical pixels)
    grab_offset_y: f32,
    scroll: VScroll,
}

/// Cached scroll state for a panel, updated each frame.
#[derive(Clone, Copy)]
pub struct PanelScrollState {
    pub rich_text_id: usize,
    pub panel_rect: [f32; 4],
    pub display_offset: usize,
    pub history_size: usize,
    pub screen_lines: usize,
}

pub struct Scrollbar {
    enabled: bool,
    /// Timestamp of last scroll activity per panel (keyed by rich_text_id)
    last_scroll_times: Vec<(usize, Instant)>,
    /// Active drag state
    pub drag_state: Option<ScrollbarDragState>,
    /// Cached per-panel scroll state, updated each render frame
    panel_states: Vec<PanelScrollState>,
}

impl Scrollbar {
    pub fn new(enabled: bool) -> Self {
        Scrollbar {
            enabled,
            last_scroll_times: Vec::new(),
            drag_state: None,
            panel_states: Vec::new(),
        }
    }

    /// Clear panel states before collecting new ones for this frame.
    pub fn clear_panel_states(&mut self) {
        self.panel_states.clear();
    }

    /// Add a panel's scroll state for this frame.
    pub fn push_panel_state(&mut self, state: PanelScrollState) {
        self.panel_states.push(state);
    }

    /// Get the cached panel states for rendering.
    pub fn panel_states(&self) -> &[PanelScrollState] {
        &self.panel_states
    }

    /// Notify the scrollbar that a scroll happened in the given panel.
    #[inline]
    pub fn notify_scroll(&mut self, rich_text_id: usize) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        if let Some(entry) = self
            .last_scroll_times
            .iter_mut()
            .find(|(id, _)| *id == rich_text_id)
        {
            entry.1 = now;
        } else {
            self.last_scroll_times.push((rich_text_id, now));
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_dragging(&self) -> bool {
        self.drag_state.is_some()
    }

    /// Compute the current opacity for a panel's scrollbar. Thin
    /// wrapper around the module-level `opacity_from_last_scroll`
    /// helper so the terminal and command palette share the same
    /// fade envelope.
    fn opacity_for(&self, rich_text_id: usize) -> f32 {
        let dragging = self
            .drag_state
            .is_some_and(|d| d.rich_text_id == rich_text_id);
        let last_scroll = self
            .last_scroll_times
            .iter()
            .find(|(id, _)| *id == rich_text_id)
            .map(|(_, t)| *t);
        opacity_from_last_scroll(last_scroll, dragging)
    }

    /// Scrollbar over a panel's right edge, in logical pixels. Offsets
    /// count lines from the oldest history line.
    fn geometry(
        panel_rect: [f32; 4],
        scale_factor: f32,
        history_size: usize,
        screen_lines: usize,
        grid_margin: (f32, f32),
    ) -> VScroll {
        let panel_x = (panel_rect[0] + grid_margin.0) / scale_factor;
        let panel_y = (panel_rect[1] + grid_margin.1) / scale_factor;
        let panel_width = panel_rect[2] / scale_factor;
        let panel_height = panel_rect[3] / scale_factor;
        VScroll {
            track: terminus_ui::Rect::new(
                panel_x + panel_width - SCROLLBAR_WIDTH - SCROLLBAR_MARGIN,
                panel_y + SCROLLBAR_MARGIN,
                SCROLLBAR_WIDTH,
                panel_height - SCROLLBAR_MARGIN * 2.0,
            ),
            content: (history_size + screen_lines) as f32,
            viewport: screen_lines as f32,
            min_thumb: SCROLLBAR_MIN_THUMB_HEIGHT,
        }
    }

    /// Test if a click at (mouse_x, mouse_y) in logical pixels hits the
    /// scrollbar of a given panel; the wider hit area eases grabbing.
    #[allow(clippy::too_many_arguments)]
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        panel_rect: [f32; 4],
        scale_factor: f32,
        display_offset: usize,
        history_size: usize,
        screen_lines: usize,
        grid_margin: (f32, f32),
    ) -> Option<(ScrollHit, VScroll)> {
        if !self.enabled || history_size == 0 {
            return None;
        }
        let scroll = Self::geometry(
            panel_rect,
            scale_factor,
            history_size,
            screen_lines,
            grid_margin,
        );
        let offset = (history_size - display_offset) as f32;
        let hit = scroll.hit(mouse_x, mouse_y, offset, SCROLLBAR_HIT_WIDTH)?;
        Some((hit, scroll))
    }

    /// Start a drag; a track press centres the thumb on the pointer.
    pub fn start_drag(&mut self, rich_text_id: usize, hit: ScrollHit, scroll: VScroll) {
        let grab_offset_y = match hit {
            ScrollHit::Thumb { grab } => grab,
            ScrollHit::Track => scroll.thumb_len() / 2.0,
        };
        self.drag_state = Some(ScrollbarDragState {
            rich_text_id,
            grab_offset_y,
            scroll,
        });
        self.notify_scroll(rich_text_id);
    }

    /// Update scroll position during drag. Returns the new display_offset.
    pub fn drag_update(&mut self, mouse_y: f32) -> Option<usize> {
        let state = self.drag_state?;
        let from_top = state.scroll.offset_at(mouse_y, state.grab_offset_y);
        let display_offset = (state.scroll.max_offset() - from_top).round() as usize;
        self.notify_scroll(state.rich_text_id);
        Some(display_offset)
    }

    /// End the drag operation.
    pub fn end_drag(&mut self) {
        if let Some(state) = self.drag_state.take() {
            self.notify_scroll(state.rich_text_id);
        }
    }

    /// Render a scrollbar for a given panel.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        sugarloaf: &mut Sugarloaf,
        panel_rect: [f32; 4],
        scale_factor: f32,
        display_offset: usize,
        history_size: usize,
        screen_lines: usize,
        rich_text_id: usize,
        grid_margin: (f32, f32),
    ) {
        if !self.enabled || history_size == 0 {
            return;
        }

        let opacity = self.opacity_for(rich_text_id);
        if opacity <= 0.0 {
            return;
        }

        let Some(thumb) = Self::geometry(
            panel_rect,
            scale_factor,
            history_size,
            screen_lines,
            grid_margin,
        )
        .thumb((history_size - display_offset) as f32) else {
            return;
        };

        let is_dragging = self
            .drag_state
            .is_some_and(|d| d.rich_text_id == rich_text_id);

        draw_thumb(
            sugarloaf,
            &thumb,
            opacity,
            is_dragging,
            TERMINAL_DEPTH,
            TERMINAL_ORDER,
        );
    }

    /// Test hook: direct access to the per-panel last-scroll timestamp
    /// so tests can seed / read it without reaching through
    /// `notify_scroll` + clock manipulation.
    #[cfg(test)]
    fn last_scroll_for(&self, rich_text_id: usize) -> Option<Instant> {
        self.last_scroll_times
            .iter()
            .find(|(id, _)| *id == rich_text_id)
            .map(|(_, t)| *t)
    }

    /// Returns true if a scrollbar needs per-frame redraws right now:
    /// while dragging or while a fade-out is in progress. Returns false
    /// during the fully-visible delay before the fade starts — callers
    /// must pair this with `next_wake_in()` to schedule the redraw that
    /// kicks off the fade.
    pub fn needs_redraw(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        if self.drag_state.is_some() {
            return true;
        }
        let deadline = FADE_OUT_DELAY_MS + FADE_OUT_DURATION_MS;
        self.last_scroll_times
            .retain(|(_, t)| t.elapsed().as_millis() < deadline);
        self.last_scroll_times
            .iter()
            .any(|(_, t)| t.elapsed().as_millis() >= FADE_OUT_DELAY_MS)
    }

    pub fn next_wake_in(&self) -> Option<std::time::Duration> {
        if !self.enabled || self.drag_state.is_some() {
            return None;
        }
        self.last_scroll_times
            .iter()
            .filter_map(|(_, t)| {
                let elapsed = t.elapsed().as_millis();
                if elapsed < FADE_OUT_DELAY_MS {
                    Some(std::time::Duration::from_millis(
                        (FADE_OUT_DELAY_MS - elapsed) as u64,
                    ))
                } else {
                    None
                }
            })
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_zero_when_never_scrolled() {
        assert_eq!(opacity_from_last_scroll(None, false), 0.0);
    }

    #[test]
    fn opacity_one_while_dragging_regardless_of_last_scroll() {
        // Dragging pins the thumb at full alpha so it doesn't fade
        // out from under the cursor during a slow drag.
        assert_eq!(opacity_from_last_scroll(None, true), 1.0);
        let old = Instant::now() - std::time::Duration::from_secs(10);
        assert_eq!(opacity_from_last_scroll(Some(old), true), 1.0);
    }

    #[test]
    fn opacity_one_inside_visibility_window() {
        // A scroll that just happened is fully visible.
        let now = Instant::now();
        assert_eq!(opacity_from_last_scroll(Some(now), false), 1.0);
    }

    #[test]
    fn opacity_zero_after_full_fade() {
        // Past FADE_OUT_DELAY + FADE_OUT_DURATION, the thumb is gone.
        let deep_past = Instant::now()
            - std::time::Duration::from_millis(
                (FADE_OUT_DELAY_MS + FADE_OUT_DURATION_MS + 50) as u64,
            );
        assert_eq!(opacity_from_last_scroll(Some(deep_past), false), 0.0);
    }

    #[test]
    fn notify_scroll_stores_timestamp_per_panel() {
        let mut bar = Scrollbar::new(true);
        assert!(bar.last_scroll_for(7).is_none());
        bar.notify_scroll(7);
        assert!(bar.last_scroll_for(7).is_some());
        assert!(bar.last_scroll_for(99).is_none());
    }

    #[test]
    fn notify_scroll_noop_when_disabled() {
        let mut bar = Scrollbar::new(false);
        bar.notify_scroll(1);
        assert!(bar.last_scroll_for(1).is_none());
    }
}
