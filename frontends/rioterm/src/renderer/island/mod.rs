// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// island.rs was originally retired from boo editor
// which is licensed under MIT license.

// Most of this upstream Rio module is superseded by the Terminus chrome;
// what is left unused is kept to ease upstream merges.
#![allow(dead_code)]

use crate::context::ContextManager;
use crate::renderer::helpers::spring::Spring;
use rio_backend::event::{EventProxy, ProgressReport, ProgressState};
use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::{Attributes, Sugarloaf};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::time::Instant;

pub const ISLAND_HEIGHT: f32 = 44.0;
/// Thin top band after retiring the tab island (context label + captions).
pub const CONTEXT_BAR_HEIGHT: f32 = 32.0;
const PROGRESS_BAR_HEIGHT: f32 = 3.0;

const PROGRESS_BAR_TIMEOUT_SECS: u64 = 15;
const TITLE_FONT_SIZE: f32 = 12.0;

const TAB_PADDING_X: f32 = 14.0;
const TAB_GAP: f32 = 10.0;
const TAB_INSET_Y: f32 = 6.0;
/// Mock tab pills use `rounded-xl` (12px).
const TAB_RADIUS: f32 = 12.0;
const TITLE_ELLIPSIS: char = '…';
const DRAG_THRESHOLD: f32 = 4.0;
const DRAG_ANIMATION_LENGTH: f32 = 0.15;
const DRAG_MAX_DT: f32 = 0.05;
const ISLAND_MARGIN_RIGHT: f32 = 8.0;

/// Color picker constants
const PICKER_SWATCH_SIZE: f32 = 18.0;
const PICKER_SWATCH_GAP: f32 = 4.0;
const PICKER_PADDING: f32 = 6.0;
const PICKER_INPUT_HEIGHT: f32 = 26.0;
const PICKER_INPUT_FONT_SIZE: f32 = 12.0;
const PICKER_INPUT_MARGIN_TOP: f32 = 8.0;
const PICKER_TOP_PADDING: f32 = 4.0;
const PICKER_HEIGHT: f32 = PICKER_TOP_PADDING
    + PICKER_SWATCH_SIZE
    + PICKER_PADDING * 2.0
    + PICKER_INPUT_MARGIN_TOP
    + PICKER_INPUT_HEIGHT
    + PICKER_PADDING;
const PICKER_COLORS: [[f32; 4]; 6] = [
    // red
    [0.86, 0.26, 0.27, 1.0],
    // orange
    [0.90, 0.57, 0.22, 1.0],
    // yellow
    [0.85, 0.78, 0.25, 1.0],
    // green
    [0.34, 0.70, 0.38, 1.0],
    // blue
    [0.30, 0.55, 0.85, 1.0],
    // purple
    [0.68, 0.40, 0.80, 1.0],
];

/// Left margin on macOS to account for traffic light buttons
#[cfg(target_os = "macos")]
const ISLAND_MARGIN_LEFT_MACOS: f32 = 76.0;

const CLOSE_MARGIN_RIGHT: f32 = 16.0;
const CLOSE_ICON_SIZE: f32 = 12.0;
const CLOSE_MIN_ISLAND_WIDTH: f32 = 64.0;
const CLOSE_HOVER_HALF: f32 = 10.0;
const CLOSE_HOVER_CORNER_RADIUS: f32 = 5.0;
const CLOSE_HIT_HALF_WIDTH: f32 = 10.0;
const CLOSE_ALPHA_IDLE: f32 = 0.55;
const CLOSE_ALPHA_HOVER: f32 = 0.95;
const INACTIVE_CUSTOM_MUTE: f32 = 0.55;

pub struct Island {
    /// Retained for config wire-up; Tab mode always paints pills (plan: ignore
    /// hide-if-single for visibility).
    #[allow(dead_code)]
    pub hide_if_single: bool,
    /// Cap on tab width in logical px (`navigation.max-tab-width`).
    pub max_tab_width: f32,
    pub inactive_text_color: [f32; 4],
    pub active_text_color: [f32; 4],
    /// Current progress bar state
    progress_state: Option<ProgressState>,
    /// Current progress value (0-100)
    progress_value: Option<u8>,
    /// When the *current* state began. Reset only when transitioning into a
    /// new state, so the indeterminate animation phase is not yanked back to
    /// zero by repeated identical OSC 9;4 reports (issue #1509).
    progress_started_at: Option<Instant>,
    /// Last time we saw an OSC 9;4 report — bumped on every report, used by
    /// the stale-bar dismissal timer. Decoupled from `progress_started_at`
    /// for the same reason.
    progress_last_seen: Option<Instant>,
    /// Progress bar color
    pub progress_bar_color: [f32; 4],
    /// Progress bar error color
    pub progress_bar_error_color: [f32; 4],
    /// Which tab has the color picker open (None = closed)
    color_picker_tab: Option<usize>,
    /// Current rename input text while picker is open
    rename_input: terminus_ui::TextDraft,
    /// Caret blink timer
    rename_caret_time: Instant,
    /// In-progress tab drag (reorder by dragging)
    drag: Option<TabDrag>,
    /// Per-tab x-offset springs: displaced tabs sliding into their slot
    /// and the released tab settling after a drag. Keyed by tab index.
    slide_springs: FxHashMap<usize, Spring>,
    /// Timestamp of the last spring advance, for per-frame dt.
    last_anim_frame: Instant,
    /// Cursor is over a tab's close button — draws the hover backdrop.
    /// Updated on every cursor move by `Screen`.
    close_hover: bool,
    /// Which tab the pointer is over (close affordance + hover fill).
    hovered_tab: Option<usize>,
    /// Last painted strip geometry — hit-tests reuse measured content widths.
    layout_cache: TabStripLayout,
    /// Cursor is over a Windows caption button (min/max/close).
    #[cfg(target_os = "windows")]
    window_control_hover: Option<crate::renderer::window_controls::WindowControl>,
}

impl Island {
    pub fn new(
        inactive_text_color: [f32; 4],
        active_text_color: [f32; 4],
        hide_if_single: bool,
        max_tab_width: f32,
    ) -> Self {
        Self {
            hide_if_single,
            max_tab_width,
            inactive_text_color,
            active_text_color,
            progress_state: None,
            progress_value: None,
            progress_started_at: None,
            progress_last_seen: None,
            // Default progress bar color (blue-ish)
            progress_bar_color: [0x0a as f32 / 255.0, 0x84 as f32 / 255.0, 1.0, 1.0],
            // Default error color (red-ish)
            progress_bar_error_color: [1.0, 0.3, 0.3, 1.0],
            color_picker_tab: None,
            rename_input: terminus_ui::TextDraft::default(),
            rename_caret_time: Instant::now(),
            drag: None,
            slide_springs: FxHashMap::default(),
            last_anim_frame: Instant::now(),
            close_hover: false,
            hovered_tab: None,
            layout_cache: TabStripLayout {
                left_margin: island_margin_left(),
                right_margin: island_margin_right(),
                widths: SmallVec::new(),
            },
            #[cfg(target_os = "windows")]
            window_control_hover: None,
        }
    }

    /// Content-hug layout from the last paint (for hit-tests / drag).
    #[inline]
    pub fn cached_layout(&self) -> &TabStripLayout {
        &self.layout_cache
    }

    /// Update hover chrome for the tab strip. Returns true when paint must refresh.
    pub fn set_tab_hover(&mut self, tab: Option<usize>, on_close: bool) -> bool {
        let changed = self.hovered_tab != tab || self.close_hover != on_close;
        self.hovered_tab = tab;
        self.close_hover = on_close;
        changed
    }

    /// Set whether the cursor hovers a tab's close button.
    /// Returns true when the state changed (the caller redraws).
    pub fn set_close_hover(&mut self, hover: bool) -> bool {
        self.set_tab_hover(self.hovered_tab, hover)
    }

    /// Set which Windows caption button is hovered. Returns true when
    /// the state changed (the caller redraws).
    #[cfg(target_os = "windows")]
    pub fn set_window_control_hover(
        &mut self,
        hover: Option<crate::renderer::window_controls::WindowControl>,
    ) -> bool {
        let changed = self.window_control_hover != hover;
        self.window_control_hover = hover;
        changed
    }

    pub fn update_colors(
        &mut self,
        inactive_text_color: [f32; 4],
        active_text_color: [f32; 4],
    ) {
        self.inactive_text_color = inactive_text_color;
        self.active_text_color = active_text_color;
    }

    /// Get the height of the island
    #[inline]
    pub fn height(&self) -> f32 {
        ISLAND_HEIGHT
    }
}

mod drag;
mod draw;
mod layout;
mod picker;
mod picker_render;
mod progress;
mod render;
use self::drag::*;
pub use self::draw::*;
pub use self::layout::*;
#[cfg(test)]
mod island_tests;
