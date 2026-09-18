// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// island.rs was originally retired from boo editor
// which is licensed under MIT license.

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

/// Right margin of the tab strip: caption buttons (Windows) + action
/// strip (+ / search), or a small gap + actions elsewhere.
#[inline]
fn island_margin_right() -> f32 {
    action_strip_width() + {
        #[cfg(target_os = "windows")]
        {
            crate::renderer::window_controls::MARGIN_RIGHT
        }
        #[cfg(not(target_os = "windows"))]
        {
            ISLAND_MARGIN_RIGHT
        }
    }
}

/// Left inset for the app logo (or macOS traffic lights).
#[inline]
fn island_margin_left() -> f32 {
    #[cfg(target_os = "macos")]
    {
        ISLAND_MARGIN_LEFT_MACOS
    }
    #[cfg(not(target_os = "macos"))]
    {
        LOGO_SLOT
    }
}

/// Width of the + / search action strip before window controls.
/// Apple HIG mock has no search/+ strip — only traffic lights / captions + tabs.
pub const ACTION_SLOT: f32 = 0.0;
const ACTION_STRIP_COUNT: f32 = 0.0;
/// App-logo hit/paint slot on the left of the title bar.
/// Non-macOS: small inset instead of a logo (mock has no logo).
pub const LOGO_SLOT: f32 = 12.0;
const TITLEBAR_ICON: f32 = 14.0;

#[inline]
pub fn action_strip_width() -> f32 {
    0.0
}

/// Title-bar chrome hit (excluding tabs / window controls).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleBarAction {
    Logo,
    NewTab,
    Search,
}

/// Hit-test logo / + / search in logical coordinates.
/// Apple HIG mock: no logo / search / + — always `None`.
pub fn title_bar_hit(
    _window_width_logical: f32,
    _x: f32,
    _y: f32,
) -> Option<TitleBarAction> {
    None
}

struct TabDrag {
    // Index of the dragged tab, follows the tab as it reorders.
    tab_index: usize,
    // Mouse x at press (unscaled), for the drag threshold.
    press_x: f32,
    // press_x − tab_left_x, keeps the grab point under the cursor.
    grab_offset: f32,
    // Latest unscaled mouse x.
    current_x: f32,
    // True once movement exceeded `DRAG_THRESHOLD`.
    started: bool,
}

fn fit_title_to_width<'a>(
    sugarloaf: &mut Sugarloaf,
    title: &'a str,
    max_width: f32,
) -> Cow<'a, str> {
    let attrs = Attributes::default();
    fit_title_with_widths(title, max_width, |c| {
        sugarloaf.char_advance(c, attrs, TITLE_FONT_SIZE)
    })
}

fn fit_title_with_widths<'a>(
    title: &'a str,
    max_width: f32,
    mut char_width: impl FnMut(char) -> f32,
) -> Cow<'a, str> {
    let suffix_width = char_width(TITLE_ELLIPSIS);

    // `truncate_ix` tracks the last byte offset at which the prefix so
    // far still has room for the suffix. Updated before adding the next
    // char's width so the moment we detect overflow we already know
    // where to cut.
    let mut accumulated: f32 = 0.0;
    let mut truncate_ix: usize = 0;
    for (ix, c) in title.char_indices() {
        if accumulated + suffix_width <= max_width {
            truncate_ix = ix;
        }
        accumulated += char_width(c);
        if accumulated > max_width {
            let mut out = String::with_capacity(truncate_ix + TITLE_ELLIPSIS.len_utf8());
            out.push_str(&title[..truncate_ix]);
            out.push(TITLE_ELLIPSIS);
            return Cow::Owned(out);
        }
    }
    Cow::Borrowed(title)
}

#[derive(Clone, PartialEq)]
pub struct TabStripLayout {
    pub left_margin: f32,
    pub right_margin: f32,
    /// Per-tab slot widths (content-hug, clamped). Empty when there are no tabs.
    pub widths: SmallVec<[f32; 12]>,
}

impl TabStripLayout {
    #[inline]
    pub fn len(&self) -> usize {
        self.widths.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.widths.is_empty()
    }

    /// Slot width for `index`, or 0 when out of range.
    #[inline]
    pub fn width_at(&self, index: usize) -> f32 {
        self.widths.get(index).copied().unwrap_or(0.0)
    }

    /// Left edge of tab `index` (logical px).
    #[inline]
    pub fn slot_x(&self, index: usize) -> f32 {
        self.left_margin + self.widths.iter().take(index).sum::<f32>()
    }

    /// Total width of all tab slots.
    #[inline]
    pub fn tabs_width(&self) -> f32 {
        self.widths.iter().sum()
    }
}

/// Status-dot diameter inside a pill (logical px).
const STATUS_DOT: f32 = 8.0;
const STATUS_GAP: f32 = 8.0;
/// Floor so a tiny label still reads as a pill (not a chip).
const MIN_TAB_WIDTH: f32 = 72.0;
/// Trailing room reserved for the hover × on closable tabs.
const CLOSE_RESERVE: f32 = CLOSE_MARGIN_RIGHT + CLOSE_HIT_HALF_WIDTH;

/// Natural slot width for measured title text (+ optional OS icon).
///
/// Layout: `[gap/2 | pad | dot | gap | icon? | title | pad | close? | gap/2]`
/// — the pill hugs its content; closable tabs keep a trailing × slot so
/// the hover affordance does not reflow the strip.
pub fn tab_slot_width_for_content(
    text_width: f32,
    has_icon: bool,
    closable: bool,
) -> f32 {
    let icon = if has_icon { TITLEBAR_ICON + 4.0 } else { 0.0 };
    let close = if closable { CLOSE_RESERVE } else { 0.0 };
    TAB_GAP
        + TAB_PADDING_X
        + STATUS_DOT
        + STATUS_GAP
        + icon
        + text_width.max(0.0)
        + TAB_PADDING_X
        + close
}

/// Build a left-aligned content-hug strip from per-tab natural widths.
///
/// When the sum overflows the available band, every slot is scaled down
/// proportionally (still left-aligned — never stretched to fill).
pub fn tab_strip_layout_from_widths(
    window_width: f32,
    scale_factor: f32,
    max_tab_width: f32,
    natural_widths: &[f32],
) -> TabStripLayout {
    let left_margin = island_margin_left();
    let right_margin = island_margin_right();
    let available = ((window_width / scale_factor) - right_margin - left_margin).max(0.0);
    let cap = max_tab_width.max(0.0);

    let mut widths: SmallVec<[f32; 12]> = natural_widths
        .iter()
        .map(|&w| {
            if cap > 0.0 {
                w.clamp(MIN_TAB_WIDTH.min(cap), cap.max(MIN_TAB_WIDTH))
            } else {
                w.max(0.0)
            }
        })
        .collect();

    let total: f32 = widths.iter().sum();
    if total > available && total > 0.0 && available > 0.0 {
        let scale = available / total;
        for w in &mut widths {
            *w *= scale;
        }
    } else if available <= 0.0 {
        for w in &mut widths {
            *w = 0.0;
        }
    }

    TabStripLayout {
        left_margin,
        right_margin,
        widths,
    }
}

/// Uniform fallback used by unit tests and before the first paint measures titles.
pub fn tab_strip_layout(
    window_width: f32,
    scale_factor: f32,
    num_tabs: usize,
    max_tab_width: f32,
) -> TabStripLayout {
    let n = num_tabs.max(0);
    let natural = tab_slot_width_for_content(48.0, false, true)
        .min(max_tab_width.max(MIN_TAB_WIDTH))
        .max(MIN_TAB_WIDTH);
    let widths = vec![natural; n];
    tab_strip_layout_from_widths(window_width, scale_factor, max_tab_width, &widths)
}

struct IslandFills {
    inactive: [f32; 4],
    active: [f32; 4],
    outline: Option<[f32; 4]>,
    close_hover: [f32; 4],
}

fn island_fills(bg: [f32; 4]) -> IslandFills {
    let luminance = 0.2126 * bg[0] + 0.7152 * bg[1] + 0.0722 * bg[2];
    if luminance > 0.5 {
        IslandFills {
            inactive: [0.0, 0.0, 0.0, 0.06],
            active: [1.0, 1.0, 1.0, 0.92],
            outline: Some([0.0, 0.0, 0.0, 0.14]),
            close_hover: [0.0, 0.0, 0.0, 0.09],
        }
    } else {
        // Dark strip: inactive pills stay readable (soft fill + hairline);
        // active is the elevated card.
        IslandFills {
            inactive: [
                0x1c as f32 / 255.0,
                0x1c as f32 / 255.0,
                0x20 as f32 / 255.0,
                1.0,
            ],
            active: [
                0x2a as f32 / 255.0,
                0x2a as f32 / 255.0,
                0x30 as f32 / 255.0,
                1.0,
            ],
            outline: Some([
                0x3a as f32 / 255.0,
                0x3a as f32 / 255.0,
                0x42 as f32 / 255.0,
                1.0,
            ]),
            close_hover: [1.0, 1.0, 1.0, 0.12],
        }
    }
}

#[inline]
fn over(dst: [f32; 4], src: [f32; 4]) -> [f32; 4] {
    let a = src[3];
    [
        src[0] * a + dst[0] * (1.0 - a),
        src[1] * a + dst[1] * (1.0 - a),
        src[2] * a + dst[2] * (1.0 - a),
        dst[3],
    ]
}

#[allow(clippy::too_many_arguments)]
fn draw_island(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    fill: [f32; 4],
    outline: Option<[f32; 4]>,
    punch: Option<[f32; 4]>,
    order: u8,
) {
    let card = terminus_ui::Rect::new(x, y, w, h);
    match outline {
        Some(ring) => {
            if let Some(bg) = punch {
                // Punch under the fill so translucent fills keep a solid backing.
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &card,
                    bg,
                    Some(ring),
                    radius,
                    1.0,
                    0.05,
                    order,
                    false,
                );
                let inner = terminus_ui::Rect::new(
                    x + 1.0,
                    y + 1.0,
                    (w - 2.0).max(0.0),
                    (h - 2.0).max(0.0),
                );
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &inner,
                    fill,
                    None,
                    (radius - 1.0).clamp(0.0, inner.width.min(inner.height) / 2.0),
                    1.0,
                    0.05,
                    order,
                    false,
                );
            } else {
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &card,
                    fill,
                    Some(ring),
                    radius,
                    1.0,
                    0.05,
                    order,
                    false,
                );
            }
        }
        None => {
            crate::renderer::chrome::paint_surface_stroke(
                sugarloaf, &card, fill, None, radius, 1.0, 0.05, order, false,
            );
        }
    }
}

#[inline]
fn island_rect(slot_x: f32, tab_width: f32) -> (f32, f32, f32, f32, f32) {
    let x = slot_x + TAB_GAP / 2.0;
    let w = (tab_width - TAB_GAP).max(0.0);
    let y = TAB_INSET_Y;
    let h = ISLAND_HEIGHT - TAB_INSET_Y * 2.0;
    let radius = TAB_RADIUS.min(w / 2.0).min(h / 2.0);
    (x, y, w, h, radius)
}

#[inline]
fn close_button_center(island_x: f32, island_w: f32) -> Option<f32> {
    (island_w >= CLOSE_MIN_ISLAND_WIDTH)
        .then_some(island_x + island_w - CLOSE_MARGIN_RIGHT)
}

#[inline]
fn close_button_center_x(layout: &TabStripLayout, tab_index: usize) -> Option<f32> {
    let slot_x = layout.slot_x(tab_index);
    let (ix, _, iw, _, _) = island_rect(slot_x, layout.width_at(tab_index));
    close_button_center(ix, iw)
}

#[inline]
pub fn close_button_hit(
    layout: &TabStripLayout,
    tab_index: usize,
    x_unscaled: f32,
) -> bool {
    close_button_center_x(layout, tab_index)
        .is_some_and(|cx| (x_unscaled - cx).abs() <= CLOSE_HIT_HALF_WIDTH)
}

/// Which tab slot contains `x_unscaled`, if any.
pub fn tab_index_at(
    layout: &TabStripLayout,
    x_unscaled: f32,
    num_tabs: usize,
) -> Option<usize> {
    if num_tabs == 0 || layout.is_empty() {
        return None;
    }
    let x_in = x_unscaled - layout.left_margin;
    if x_in < 0.0 || x_in >= layout.tabs_width() {
        return None;
    }
    let mut cursor = 0.0;
    for (i, &w) in layout.widths.iter().enumerate().take(num_tabs) {
        if x_in < cursor + w {
            return Some(i);
        }
        cursor += w;
    }
    None
}

fn draw_close_button(
    sugarloaf: &mut Sugarloaf,
    cx: f32,
    color: [f32; 4],
    hover: bool,
    scale_factor: f32,
) {
    use crate::renderer::chrome;
    use terminus_ui::icons::{Icon, IconPlacement};

    let alpha = if hover {
        CLOSE_ALPHA_HOVER
    } else {
        CLOSE_ALPHA_IDLE
    };
    let color = [color[0], color[1], color[2], color[3] * alpha];
    let ix = cx - CLOSE_ICON_SIZE / 2.0;
    let iy = (ISLAND_HEIGHT - CLOSE_ICON_SIZE) / 2.0;
    // Lucide mask (same path as window-control ×) — AA via tiny-skia,
    // not two diagonal `line` strokes that stair-step and clip.
    chrome::draw_icon(
        sugarloaf,
        Icon::X,
        IconPlacement::new(ix, iy, CLOSE_ICON_SIZE),
        color,
        scale_factor,
    );
}

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
    rename_input: String,
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
            rename_input: String::new(),
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

    /// Arm a tab drag at mouse press. The drag only `started`s once the
    /// pointer moves past `DRAG_THRESHOLD`.
    pub fn start_drag(&mut self, tab_index: usize, grab_offset: f32, x: f32) {
        self.drag = Some(TabDrag {
            tab_index,
            press_x: x,
            grab_offset,
            current_x: x,
            started: false,
        });
    }

    /// Feed a mouse move into the armed drag. Returns `true` once the
    /// drag is active (threshold exceeded).
    pub fn update_drag(&mut self, x: f32) -> bool {
        match self.drag.as_mut() {
            Some(drag) => {
                drag.current_x = x;
                if !drag.started && (x - drag.press_x).abs() > DRAG_THRESHOLD {
                    drag.started = true;
                }
                drag.started
            }
            None => false,
        }
    }

    /// Whether a drag is armed or active.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Index of the dragged tab, if a drag is active.
    pub fn drag_index(&self) -> Option<usize> {
        self.drag
            .as_ref()
            .filter(|d| d.started)
            .map(|d| d.tab_index)
    }

    /// Left edge of the floating (dragged) tab, clamped to the tabs
    /// region (`left_margin..left_margin + tabs_width`) — the empty
    /// chrome beyond the last slot is not a valid drop area.
    fn drag_floating_left(&self, layout: &TabStripLayout) -> Option<f32> {
        let drag = self.drag.as_ref().filter(|d| d.started)?;
        let left = drag.current_x - drag.grab_offset;
        let drag_w = layout.width_at(drag.tab_index);
        // `.max(0.0)` keeps the clamp range valid (min ≤ max) even if a
        // pathologically narrow window makes tabs_width < tab_width.
        let max_left = layout.left_margin + (layout.tabs_width() - drag_w).max(0.0);
        Some(left.clamp(layout.left_margin, max_left))
    }

    /// Center x of the floating tab — the reference point that decides
    /// which slot the drag targets.
    pub fn drag_center(&self, layout: &TabStripLayout) -> Option<f32> {
        let drag = self.drag.as_ref().filter(|d| d.started)?;
        let left = self.drag_floating_left(layout)?;
        Some(left + layout.width_at(drag.tab_index) / 2.0)
    }

    /// Finish a drag: seed a settle spring from the floating position
    /// into the slot so the tab slides into place.
    pub fn end_drag(&mut self, layout: &TabStripLayout) {
        if let (Some(floating_left), Some(drag)) = (
            self.drag_floating_left(layout),
            self.drag.as_ref().filter(|d| d.started),
        ) {
            let slot_x = layout.slot_x(drag.tab_index);
            let offset = floating_left - slot_x;
            if offset.abs() > 0.01 {
                let spring = self
                    .slide_springs
                    .entry(drag.tab_index)
                    .or_insert_with(Spring::new);
                spring.position = offset;
            }
        }
        self.drag = None;
    }

    /// Drop an armed/active drag without any settle animation.
    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }

    /// New index of tab `i` after the tab at `from` rotated to `to`.
    fn remap_index(i: usize, from: usize, to: usize) -> usize {
        if i == from {
            to
        } else if from < to && i > from && i <= to {
            i - 1
        } else if to < from && i >= to && i < from {
            i + 1
        } else {
            i
        }
    }

    /// Re-key all per-tab-index state after the tab at `from` moved to
    /// `to` (rotate semantics, matching
    /// `ContextManager::move_current_tab_to`), then seed slide springs
    /// on the displaced tabs so they animate into their new slot.
    pub fn remap_tab_move(&mut self, from: usize, to: usize, tab_width: f32) {
        if from == to {
            return;
        }

        self.slide_springs = self
            .slide_springs
            .drain()
            .map(|(i, v)| (Self::remap_index(i, from, to), v))
            .collect();
        if let Some(picker) = self.color_picker_tab {
            self.color_picker_tab = Some(Self::remap_index(picker, from, to));
        }
        if let Some(ref mut drag) = self.drag {
            drag.tab_index = Self::remap_index(drag.tab_index, from, to);
        }

        // Displaced tabs shifted one slot away from `from` toward `to`'s
        // side; seed (or accumulate into) a spring so each one starts at
        // its old x and slides to the new slot. The moved tab itself ends
        // at `to`, which both ranges exclude — while dragging it floats,
        // and on a keyboard move it jumps (no old position to animate
        // from that wouldn't fight the selection change).
        let (range, delta) = if from < to {
            // Tabs at from+1..=to moved left by one: now at from..to.
            (from..to, tab_width)
        } else {
            // Tabs at to..from moved right by one: now at to+1..=from.
            (to + 1..from + 1, -tab_width)
        };
        for i in range {
            let spring = self.slide_springs.entry(i).or_insert_with(Spring::new);
            spring.position += delta;
        }
    }

    /// Re-key per-tab state after tabs `a` and `b` swapped places —
    /// `ContextManager::move_current_to_prev/next` semantics, which swap
    /// (including the wrap-around end-to-end case) instead of rotating.
    /// Adjacent swaps get slide springs; wrap-around jumps don't (a
    /// full-bar slide reads as glitch, not motion).
    pub fn remap_tab_swap(&mut self, a: usize, b: usize, tab_width: f32) {
        if a == b {
            return;
        }

        let swap_key = |i: usize| {
            if i == a {
                b
            } else if i == b {
                a
            } else {
                i
            }
        };
        self.slide_springs = self
            .slide_springs
            .drain()
            .map(|(i, v)| (swap_key(i), v))
            .collect();
        if let Some(picker) = self.color_picker_tab {
            self.color_picker_tab = Some(swap_key(picker));
        }

        if a.abs_diff(b) == 1 {
            let delta = (b as f32 - a as f32) * tab_width;
            let spring = self.slide_springs.entry(a).or_insert_with(Spring::new);
            spring.position += delta;
            let spring = self.slide_springs.entry(b).or_insert_with(Spring::new);
            spring.position -= delta;
        }
    }

    /// Check if the progress bar should be auto-dismissed due to timeout.
    /// Uses `progress_last_seen` (heartbeat), not `progress_started_at`, so
    /// a long-running TUI that keeps reporting stays visible.
    fn check_progress_timeout(&mut self) {
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
    fn render_progress_bar(
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

    /// Get the height of the island
    #[inline]
    pub fn height(&self) -> f32 {
        ISLAND_HEIGHT
    }

    /// Render tabs using equal-width layout
    #[inline]
    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        dimensions: (f32, f32, f32),
        context_manager: &ContextManager<EventProxy>,
        bg_color: [f32; 4],
        #[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
        window_maximized: bool,
    ) {
        let (window_width, _window_height, scale_factor) = dimensions;
        let num_tabs = context_manager.len();
        let current_tab_index = context_manager.current_index();
        let logical_w = window_width / scale_factor;

        // Apple HIG title bar strip (#111113) + bottom hairline (#2f2f35).
        // Strip under the pills (order 0). Pills / close / dots sit above
        // it — painting pills at 0 left them invisible under this rect,
        // so hover lift and × never showed (only the floating drag tab
        // at order 11 did).
        let strip = [
            0x11 as f32 / 255.0,
            0x11 as f32 / 255.0,
            0x13 as f32 / 255.0,
            1.0,
        ];
        crate::renderer::chrome::paint_title_strip(sugarloaf, logical_w, ISLAND_HEIGHT);

        // Immediate-mode: no cached ids to hide. If we early-return
        // without drawing, the tabs just don't appear this frame.

        // A lone tab cannot be reordered. A drag can only start with two
        // or more tabs, but one can outlive the second tab (its shell
        // exits mid-drag) — drop the drag so we don't float a phantom.
        if num_tabs == 1 {
            self.drag = None;
            self.slide_springs.clear();
        }

        // A reorder that didn't come from this drag (tab closed via
        // shell exit, keyboard move) breaks the drag.tab_index ==
        // current_index invariant — drop the drag instead of floating
        // a phantom tab over the wrong slot.
        if self
            .drag
            .as_ref()
            .is_some_and(|d| d.tab_index != current_tab_index)
        {
            self.drag = None;
        }

        // Advance the slide springs (drag-reorder animation) by this
        // frame's dt; settled springs drop out of the map.
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_anim_frame)
            .as_secs_f32()
            .min(DRAG_MAX_DT);
        self.last_anim_frame = now;
        self.slide_springs
            .retain(|_, s| s.update(dt, DRAG_ANIMATION_LENGTH));

        // Measure each tab's content, then hug — left-aligned pills.
        let measure_opts = DrawOpts {
            font_size: TITLE_FONT_SIZE,
            ..DrawOpts::default()
        };
        let mut natural: SmallVec<[f32; 12]> = SmallVec::with_capacity(num_tabs);
        for tab_index in 0..num_tabs {
            let raw_title = self.get_title_for_tab(context_manager, tab_index);
            let text_w = if raw_title.is_empty() {
                0.0
            } else {
                sugarloaf.text_mut().measure(&raw_title, &measure_opts)
            };
            let has_icon = terminus_ui::OsGlyph::from_hint(
                context_manager.tab_os_id(tab_index),
                &raw_title,
            )
            .has_mark();
            let closable = !context_manager.is_pinned(tab_index);
            natural.push(tab_slot_width_for_content(text_w, has_icon, closable));
        }
        let layout = tab_strip_layout_from_widths(
            window_width,
            scale_factor,
            self.max_tab_width,
            &natural,
        );
        self.layout_cache = layout.clone();
        let left_margin = layout.left_margin;

        // Starting from left edge (with margin on macOS for traffic lights)
        let mut x_position = left_margin;

        // Active drag: the dragged tab is skipped in the slot loop and
        // drawn floating (after the loop, on a higher layer) instead.
        let drag_index = self.drag_index();
        let floating_left = self.drag_floating_left(&layout);

        // Adaptive island fills from Apple HIG strip (not terminal bg).
        let fills = island_fills(strip);

        // Render each tab
        for tab_index in 0..num_tabs {
            let tab_width = layout.width_at(tab_index);
            // The dragged tab floats — drawn after the loop instead.
            if Some(tab_index) == drag_index {
                x_position += tab_width;
                continue;
            }

            let is_active = tab_index == current_tab_index;

            // Slot position plus any slide-spring offset (tab still
            // animating into its slot after a reorder).
            let tab_x = x_position
                + self
                    .slide_springs
                    .get(&tab_index)
                    .map_or(0.0, |s| s.position);

            // Get title for this tab, then truncate with a trailing
            // ellipsis so overflowing titles can't bleed into the next
            // tab or past the left edge (issue #1508).
            let raw_title = self.get_title_for_tab(context_manager, tab_index);
            if raw_title.is_empty() {
                x_position += tab_width;
                continue;
            }

            let closable = !context_manager.is_pinned(tab_index);
            let close_budget = if closable { CLOSE_RESERVE } else { 0.0 };
            let glyph = terminus_ui::OsGlyph::from_hint(
                context_manager.tab_os_id(tab_index),
                &raw_title,
            );
            let icon_slot = if glyph.has_mark() {
                TITLEBAR_ICON + 4.0
            } else {
                0.0
            };
            let max_text_width = (tab_width
                - TAB_GAP
                - TAB_PADDING_X * 2.0
                - close_budget
                - STATUS_DOT
                - STATUS_GAP
                - icon_slot)
                .max(0.0);
            let title = fit_title_to_width(sugarloaf, &raw_title, max_text_width);

            let text_color = if is_active {
                self.active_text_color
            } else {
                self.inactive_text_color
            };

            let title_opts = DrawOpts {
                font_size: TITLE_FONT_SIZE,
                color: color_u8(text_color),
                ..DrawOpts::default()
            };

            // UI text always paints in a final pass above every rect,
            // so the floating tab's opaque background can't occlude
            // titles passing underneath it — skip a title once the
            // floating tab intrudes past the slot's text padding.
            let drag_w = drag_index.map(|i| layout.width_at(i)).unwrap_or(tab_width);
            let hidden_by_drag = floating_left.is_some_and(|fl| {
                let overlap = (tab_x + tab_width).min(fl + drag_w) - tab_x.max(fl);
                overlap > TAB_PADDING_X
            });

            // Pill fill first (above strip), then chrome / label on top.
            let (ix, iy, iw, ih, radius) = island_rect(tab_x, tab_width);
            let fill = match context_manager.custom_color(tab_index) {
                Some(mut custom) => {
                    if !is_active {
                        custom[3] *= INACTIVE_CUSTOM_MUTE;
                    }
                    if self.hovered_tab == Some(tab_index) {
                        custom[0] = (custom[0] + 0.05).min(1.0);
                        custom[1] = (custom[1] + 0.05).min(1.0);
                        custom[2] = (custom[2] + 0.05).min(1.0);
                    }
                    custom
                }
                None => {
                    let hovered = self.hovered_tab == Some(tab_index);
                    if is_active {
                        if hovered {
                            [
                                (fills.active[0] + 0.05).min(1.0),
                                (fills.active[1] + 0.05).min(1.0),
                                (fills.active[2] + 0.05).min(1.0),
                                fills.active[3],
                            ]
                        } else {
                            fills.active
                        }
                    } else if hovered {
                        [
                            (fills.inactive[0] + 0.08).min(1.0),
                            (fills.inactive[1] + 0.08).min(1.0),
                            (fills.inactive[2] + 0.08).min(1.0),
                            fills.inactive[3],
                        ]
                    } else {
                        fills.inactive
                    }
                }
            };
            // Above the strip (0); below chrome rail (4+) and floating drag (11).
            draw_island(
                sugarloaf,
                ix,
                iy,
                iw,
                ih,
                radius,
                fill,
                fills.outline,
                Some(strip),
                2,
            );

            // Close × only on hover of a closable (non-pinned) tab.
            let show_close = self.hovered_tab == Some(tab_index)
                && !context_manager.is_pinned(tab_index);
            if show_close {
                if let Some(cx) = close_button_center(ix, iw) {
                    if self.close_hover {
                        crate::renderer::chrome::paint_surface_stroke(
                            sugarloaf,
                            &terminus_ui::Rect::new(
                                cx - CLOSE_HOVER_HALF,
                                ISLAND_HEIGHT / 2.0 - CLOSE_HOVER_HALF,
                                CLOSE_HOVER_HALF * 2.0,
                                CLOSE_HOVER_HALF * 2.0,
                            ),
                            fills.close_hover,
                            None,
                            CLOSE_HOVER_CORNER_RADIUS,
                            1.0,
                            0.05,
                            3,
                            false,
                        );
                    }
                    draw_close_button(
                        sugarloaf,
                        cx,
                        if is_active {
                            self.active_text_color
                        } else {
                            self.inactive_text_color
                        },
                        self.close_hover,
                        scale_factor,
                    );
                }
            }

            if !hidden_by_drag {
                let group_x = tab_x + TAB_GAP / 2.0 + TAB_PADDING_X;
                let text_y = (ISLAND_HEIGHT / 2.0) - (TITLE_FONT_SIZE / 2.);
                let dot_y = (ISLAND_HEIGHT - STATUS_DOT) / 2.0;
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &terminus_ui::Rect::new(group_x, dot_y, STATUS_DOT, STATUS_DOT),
                    [0.20, 0.83, 0.60, 1.0],
                    None,
                    STATUS_DOT / 2.0,
                    1.0,
                    0.06,
                    5,
                    false,
                );
                let icon_x = group_x + STATUS_DOT + STATUS_GAP;
                let text_x = icon_x + icon_slot;
                if glyph.has_mark() {
                    use crate::renderer::chrome;
                    use terminus_ui::icons::IconPlacement;
                    let iy = (ISLAND_HEIGHT - TITLEBAR_ICON) / 2.0;
                    chrome::draw_os_glyph(
                        sugarloaf,
                        glyph,
                        IconPlacement::new(icon_x, iy, TITLEBAR_ICON),
                        glyph.color(),
                        scale_factor,
                    );
                }
                sugarloaf
                    .text_mut()
                    .draw(text_x, text_y, &title, &title_opts);
            }

            x_position += tab_width;
        }

        // Draw the floating (dragged) tab above the slot tabs.
        if let (Some(drag_idx), Some(floating_x)) = (drag_index, floating_left) {
            let tab_width = layout.width_at(drag_idx);
            let (ix, iy, iw, ih, radius) = island_rect(floating_x, tab_width);

            // Soft elevation: a slightly inflated dark halo behind the
            // lifted island so it reads as floating over the strip.
            crate::renderer::chrome::paint_surface_stroke(
                sugarloaf,
                &terminus_ui::Rect::new(ix - 2.0, iy - 1.0, iw + 4.0, ih + 3.0),
                [0.0, 0.0, 0.0, 0.18],
                None,
                radius + 2.0,
                1.0,
                0.05,
                11,
                false,
            );

            let fill = match context_manager.custom_color(drag_idx) {
                Some(mut custom) => {
                    custom[3] = 1.0;
                    custom
                }
                None => fills.active,
            };
            draw_island(
                sugarloaf,
                ix,
                iy,
                iw,
                ih,
                radius,
                fill,
                fills.outline,
                None,
                11,
            );

            if let Some(cx) = close_button_center(ix, iw) {
                draw_close_button(
                    sugarloaf,
                    cx,
                    self.active_text_color,
                    false,
                    scale_factor,
                );
            }

            let raw_title = self.get_title_for_tab(context_manager, drag_idx);
            if !raw_title.is_empty() {
                let max_text_width = (tab_width
                    - TAB_GAP
                    - TAB_PADDING_X
                    - CLOSE_MARGIN_RIGHT
                    - CLOSE_HIT_HALF_WIDTH)
                    .max(0.0);
                let title = fit_title_to_width(sugarloaf, &raw_title, max_text_width);
                let title_opts = DrawOpts {
                    font_size: TITLE_FONT_SIZE,
                    color: color_u8(self.active_text_color),
                    ..DrawOpts::default()
                };
                let ui = sugarloaf.text_mut();
                let text_y = (ISLAND_HEIGHT / 2.0) - (TITLE_FONT_SIZE / 2.);
                let text_x = floating_x + TAB_GAP / 2.0 + TAB_PADDING_X;
                ui.draw(text_x, text_y, &title, &title_opts);
            }
        }

        // Render color picker if open
        if let Some(picker_tab) = self.color_picker_tab {
            if picker_tab < num_tabs {
                let picker_tab_x = layout.slot_x(picker_tab);
                let tab_width = layout.width_at(picker_tab);
                let selected = context_manager.custom_color(picker_tab);
                self.render_color_picker(sugarloaf, picker_tab_x, tab_width, selected);
            }
        }

        // Render the progress bar below the island
        self.render_progress_bar(sugarloaf, window_width, scale_factor, ISLAND_HEIGHT);

        let logical_w = window_width / scale_factor;
        render_title_bar_chrome(
            sugarloaf,
            logical_w,
            scale_factor,
            self.active_text_color,
        );

        #[cfg(target_os = "windows")]
        {
            crate::renderer::window_controls::render(
                sugarloaf,
                logical_w,
                scale_factor,
                window_maximized,
                self.window_control_hover,
                self.active_text_color,
            );
        }
    }

    /// Toggle the color picker for a given tab index
    pub fn toggle_color_picker(
        &mut self,
        tab_index: usize,
        current_title: &str,
        context_manager: &mut ContextManager<EventProxy>,
    ) {
        if self.color_picker_tab == Some(tab_index) {
            self.apply_rename(context_manager);
            self.color_picker_tab = None;
        } else {
            self.color_picker_tab = Some(tab_index);
            // Initialize rename input with custom title or current displayed title
            self.rename_input = context_manager
                .custom_title(tab_index)
                .map(str::to_string)
                .unwrap_or_else(|| current_title.to_string());
            self.rename_caret_time = Instant::now();
        }
    }

    /// Close the color picker, applying any pending rename
    pub fn close_color_picker(
        &mut self,
        context_manager: &mut ContextManager<EventProxy>,
    ) {
        if self.color_picker_tab.is_some() {
            self.apply_rename(context_manager);
        }
        self.color_picker_tab = None;
    }

    /// Dismiss the picker WITHOUT committing a pending rename. Used when the
    /// tab set changes underneath it (e.g. a tab close), where the anchored
    /// index may no longer point at the same tab.
    pub fn dismiss_color_picker(&mut self) {
        self.color_picker_tab = None;
    }

    /// Apply the rename input as a custom title for the current picker tab
    fn apply_rename(&mut self, context_manager: &mut ContextManager<EventProxy>) {
        if let Some(tab) = self.color_picker_tab {
            let trimmed = self.rename_input.trim().to_string();
            let title = (!trimmed.is_empty()).then_some(trimmed);
            context_manager.set_custom_title(tab, title);
        }
    }

    /// Handle keyboard input while the color picker (with rename field) is open.
    /// Returns true if input was consumed.
    /// Handle one key event for the rename input. The caller
    /// (`has_key_wait`'s `Modal::IslandRename` arm) already verified
    /// the picker is open via `active_modal` and consumes the event
    /// unconditionally, so there is nothing to return.
    pub fn handle_rename_input(
        &mut self,
        key_event: &rio_window::event::KeyEvent,
        context_manager: &mut ContextManager<EventProxy>,
    ) {
        use rio_window::event::ElementState;
        use rio_window::keyboard::{Key, NamedKey};

        if key_event.state != ElementState::Pressed {
            return; // consume release events too
        }

        match &key_event.logical_key {
            Key::Named(NamedKey::Escape) => {
                // Cancel — discard input, close picker
                self.color_picker_tab = None;
            }
            Key::Named(NamedKey::Enter) => {
                // Confirm — apply rename and close
                self.apply_rename(context_manager);
                self.color_picker_tab = None;
            }
            Key::Named(NamedKey::Backspace) => {
                self.rename_input.pop();
                self.rename_caret_time = Instant::now();
            }
            _ => {
                if let Some(text) = key_event.text.as_ref() {
                    self.append_rename_text(text.as_str());
                }
            }
        }
    }

    /// Append committed or typed text to the rename input, applying
    /// the shared overlay input policy (`is_printable_text`) so the
    /// key path and the IME commit path can never drift. Returns
    /// whether text was actually appended (same contract as
    /// `CommandPalette::append_query`).
    pub fn append_rename_text(&mut self, text: &str) -> bool {
        if self.color_picker_tab.is_none() || !crate::renderer::is_printable_text(text) {
            return false;
        }
        self.rename_input.push_str(text);
        self.rename_caret_time = Instant::now();
        true
    }

    /// Check if a click hits a color swatch in the picker.
    /// Returns true if the click was consumed.
    pub fn handle_color_picker_click(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        scale_factor: f32,
        window_width: f32,
        num_tabs: usize,
        context_manager: &mut ContextManager<EventProxy>,
    ) -> bool {
        let picker_tab = match self.color_picker_tab {
            Some(t) => t,
            None => return false,
        };

        let mouse_x_unscaled = mouse_x / scale_factor;
        let mouse_y_unscaled = mouse_y / scale_factor;

        // Prefer the content-hug cache from the last paint so the picker
        // stays under the same pill the user clicked.
        let layout = if self.layout_cache.len() == num_tabs {
            self.layout_cache.clone()
        } else {
            tab_strip_layout(window_width, scale_factor, num_tabs, self.max_tab_width)
        };
        let tab_x = layout.slot_x(picker_tab);
        let tab_width = layout.width_at(picker_tab);

        // Picker is rendered just below the island
        let picker_y = ISLAND_HEIGHT;

        // Check if click is within picker vertical range
        if mouse_y_unscaled < picker_y || mouse_y_unscaled > picker_y + PICKER_HEIGHT {
            // Click outside picker — apply rename and close
            self.apply_rename(context_manager);
            self.color_picker_tab = None;
            return false;
        }

        // Total picker width — N color swatches + 1 reset swatch
        let slot_count = PICKER_COLORS.len() + 1;
        let total_swatches_width = slot_count as f32 * PICKER_SWATCH_SIZE
            + (slot_count - 1) as f32 * PICKER_SWATCH_GAP;
        let picker_start_x = tab_x + (tab_width - total_swatches_width) / 2.0;

        // Check each swatch
        let swatch_y = picker_y + PICKER_PADDING + PICKER_TOP_PADDING;
        let swatch_y_end = swatch_y + PICKER_SWATCH_SIZE;
        for (i, color) in PICKER_COLORS.iter().enumerate() {
            let swatch_x =
                picker_start_x + i as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
            if mouse_x_unscaled >= swatch_x
                && mouse_x_unscaled <= swatch_x + PICKER_SWATCH_SIZE
                && mouse_y_unscaled >= swatch_y
                && mouse_y_unscaled <= swatch_y_end
            {
                context_manager.set_custom_color(picker_tab, Some(*color));
                self.apply_rename(context_manager);
                self.color_picker_tab = None;
                return true;
            }
        }

        // Reset swatch — clears any custom color for this tab
        let reset_x = picker_start_x
            + PICKER_COLORS.len() as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
        if mouse_x_unscaled >= reset_x
            && mouse_x_unscaled <= reset_x + PICKER_SWATCH_SIZE
            && mouse_y_unscaled >= swatch_y
            && mouse_y_unscaled <= swatch_y_end
        {
            context_manager.set_custom_color(picker_tab, None);
            self.apply_rename(context_manager);
            self.color_picker_tab = None;
            return true;
        }

        // Clicked in picker area but not on a swatch
        true
    }

    /// Render the color picker dropdown below a tab
    fn render_color_picker(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        tab_x: f32,
        tab_width: f32,
        selected_color: Option<[f32; 4]>,
    ) {
        let padding = PICKER_PADDING;
        let bg_y = ISLAND_HEIGHT;

        // Compute total swatches width to derive the consistent inner content width
        // N color swatches + 1 reset swatch
        let slot_count = PICKER_COLORS.len() + 1;
        let total_swatches_width = slot_count as f32 * PICKER_SWATCH_SIZE
            + (slot_count - 1) as f32 * PICKER_SWATCH_GAP;
        let inner_width = total_swatches_width;
        let bg_width = inner_width + padding * 2.0;
        let bg_x = tab_x + (tab_width - bg_width) / 2.0;
        let content_x = bg_x + padding;

        // Background
        crate::renderer::chrome::paint_surface_stroke(
            sugarloaf,
            &terminus_ui::Rect::new(bg_x, bg_y, bg_width, PICKER_HEIGHT),
            [0.15, 0.15, 0.15, 1.0],
            None,
            4.0,
            1.0,
            0.0,
            10,
            false,
        );

        // Swatches — aligned to content_x
        let swatch_y = bg_y + padding + PICKER_TOP_PADDING;
        for (i, color) in PICKER_COLORS.iter().enumerate() {
            let sx = content_x + i as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
            let is_selected = selected_color == Some(*color);

            if is_selected {
                let border = 2.0;
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &terminus_ui::Rect::new(
                        sx - border,
                        swatch_y - border,
                        PICKER_SWATCH_SIZE + border * 2.0,
                        PICKER_SWATCH_SIZE + border * 2.0,
                    ),
                    *color,
                    Some([1.0, 1.0, 1.0, 1.0]),
                    4.0,
                    border,
                    0.0,
                    10,
                    false,
                );
            } else {
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &terminus_ui::Rect::new(
                        sx,
                        swatch_y,
                        PICKER_SWATCH_SIZE,
                        PICKER_SWATCH_SIZE,
                    ),
                    *color,
                    None,
                    3.0,
                    1.0,
                    0.0,
                    10,
                    false,
                );
            }
        }

        // Reset swatch — neutral box with a diagonal slash, selected when no color is set
        let reset_x = content_x
            + PICKER_COLORS.len() as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
        let reset_selected = selected_color.is_none();
        let reset_fill = [0.22, 0.22, 0.22, 1.0];
        if reset_selected {
            let border = 2.0;
            crate::renderer::chrome::paint_surface_stroke(
                sugarloaf,
                &terminus_ui::Rect::new(
                    reset_x - border,
                    swatch_y - border,
                    PICKER_SWATCH_SIZE + border * 2.0,
                    PICKER_SWATCH_SIZE + border * 2.0,
                ),
                reset_fill,
                Some([1.0, 1.0, 1.0, 1.0]),
                4.0,
                border,
                0.0,
                10,
                false,
            );
        } else {
            crate::renderer::chrome::paint_surface_stroke(
                sugarloaf,
                &terminus_ui::Rect::new(
                    reset_x,
                    swatch_y,
                    PICKER_SWATCH_SIZE,
                    PICKER_SWATCH_SIZE,
                ),
                reset_fill,
                None,
                3.0,
                1.0,
                0.0,
                10,
                false,
            );
        }
        let slash_inset = 3.0;
        crate::renderer::chrome::paint_line(
            sugarloaf,
            reset_x + slash_inset,
            swatch_y + PICKER_SWATCH_SIZE - slash_inset,
            reset_x + PICKER_SWATCH_SIZE - slash_inset,
            swatch_y + slash_inset,
            1.5,
            [0.86, 0.26, 0.27, 1.0],
            0.0,
            10,
        );

        // Rename text input — same left/right edge as swatches
        let input_y = swatch_y + PICKER_SWATCH_SIZE + PICKER_INPUT_MARGIN_TOP;
        let input_x = content_x;
        let input_width = inner_width;

        // Input background
        crate::renderer::chrome::paint_surface_stroke(
            sugarloaf,
            &terminus_ui::Rect::new(input_x, input_y, input_width, PICKER_INPUT_HEIGHT),
            [0.10, 0.10, 0.10, 1.0],
            None,
            3.0,
            1.0,
            0.0,
            10,
            false,
        );

        let text_inset = 6.0;
        let text_x = input_x + text_inset;
        let max_text_width = input_width - text_inset * 2.0;
        let text_y = input_y + (PICKER_INPUT_HEIGHT - PICKER_INPUT_FONT_SIZE) / 2.0;

        let text_color = if self.rename_input.is_empty() {
            [0.45, 0.45, 0.45, 1.0]
        } else {
            [0.93, 0.93, 0.93, 1.0]
        };
        let rename_opts = DrawOpts {
            font_size: PICKER_INPUT_FONT_SIZE,
            color: color_u8(text_color),
            ..DrawOpts::default()
        };

        // Determine visible text: trim from the front if it overflows.
        let display_text: String = if self.rename_input.is_empty() {
            "Tab title...".to_string()
        } else {
            let input = self.rename_input.as_str();
            let chars: Vec<char> = input.chars().collect();
            let ui = sugarloaf.text_mut();
            let mut start = 0;
            let full_width = ui.measure(input, &rename_opts);
            if full_width > max_text_width {
                let mut lo = 0;
                let mut hi = chars.len();
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    let substr: String = chars[mid..].iter().collect();
                    let w = ui.measure(&substr, &rename_opts);
                    if w > max_text_width {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                start = lo;
            }
            chars[start..].iter().collect()
        };

        let rendered_width =
            sugarloaf
                .text_mut()
                .draw(text_x, text_y, &display_text, &rename_opts);
        let rendered_width = if self.rename_input.is_empty() {
            0.0
        } else {
            rendered_width
        };

        // Blinking caret
        let elapsed = self.rename_caret_time.elapsed().as_millis();
        let show_caret = (elapsed / 500).is_multiple_of(2);
        if show_caret {
            let caret_x = text_x + rendered_width;
            if caret_x <= input_x + input_width {
                crate::renderer::chrome::paint_caret(
                    sugarloaf,
                    caret_x,
                    input_y + 4.0,
                    PICKER_INPUT_HEIGHT - 8.0,
                    [0.93, 0.93, 0.93, 1.0],
                    0.0,
                    10,
                );
            }
        }
    }

    /// Whether the color picker is currently open
    pub fn is_color_picker_open(&self) -> bool {
        self.color_picker_tab.is_some()
    }

    /// Get the title text for a specific tab index
    fn get_title_for_tab(
        &self,
        context_manager: &ContextManager<EventProxy>,
        tab_index: usize,
    ) -> String {
        // Custom user-set title takes priority
        if let Some(custom) = context_manager.custom_title(tab_index) {
            return custom.to_string();
        }

        if let Some(context_title) = context_manager.title(tab_index) {
            if !context_title.content.is_empty() && !context_title.content.contains("{{")
            {
                return context_title.content.clone();
            }

            // Fallback to program name if title is empty
            if let Some(ref extra) = context_title.extra {
                if !extra.program.is_empty() {
                    return extra.program.clone();
                }
            }
        }

        // Default fallback - show tab number
        String::from("~")
    }
}

#[inline]
fn color_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

fn render_title_bar_chrome(
    _sugarloaf: &mut Sugarloaf,
    _window_width_logical: f32,
    _scale_factor: f32,
    _icon_color: [f32; 4],
) {
    // Apple HIG mock: title bar is strip + tabs (+ Windows captions) only.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_geometry_invariants() {
        const {
            assert!(TAB_INSET_Y * 2.0 < ISLAND_HEIGHT);
            assert!(CLOSE_MARGIN_RIGHT + CLOSE_HIT_HALF_WIDTH < CLOSE_MIN_ISLAND_WIDTH);
            assert!(CLOSE_HOVER_HALF * 2.0 <= ISLAND_HEIGHT - TAB_INSET_Y * 2.0);
        }
    }

    /// Lone tab hugs its content — not a full-strip or fixed natural bar.
    #[test]
    fn single_tab_uses_content_hug_width() {
        let natural = tab_slot_width_for_content(80.0, false, false);
        let layout = tab_strip_layout_from_widths(1600.0, 2.0, 240.0, &[natural]);
        assert_eq!(layout.width_at(0), natural.max(MIN_TAB_WIDTH));
        assert_eq!(layout.tabs_width(), layout.width_at(0));
        let logical_w = 800.0;
        assert!(layout.left_margin + layout.tabs_width() < logical_w / 2.0);
    }

    #[test]
    fn content_hug_keeps_tabs_different_widths() {
        let short = tab_slot_width_for_content(40.0, false, false);
        let long = tab_slot_width_for_content(120.0, true, true);
        let layout = tab_strip_layout_from_widths(3000.0, 2.0, 240.0, &[short, long]);
        assert!(layout.width_at(0) < layout.width_at(1));
        assert_eq!(layout.tabs_width(), layout.width_at(0) + layout.width_at(1));
    }

    #[test]
    fn island_rect_insets_slot_and_clamps_radius() {
        // Slot at x=100, width 180 → island inset by half the gap on
        // each side and TAB_INSET_Y vertically.
        let (x, y, w, h, radius) = island_rect(100.0, 180.0);
        assert_eq!(x, 100.0 + TAB_GAP / 2.0);
        assert_eq!(y, TAB_INSET_Y);
        assert_eq!(w, 180.0 - TAB_GAP);
        assert_eq!(h, ISLAND_HEIGHT - TAB_INSET_Y * 2.0);
        assert_eq!(radius, TAB_RADIUS);

        let (_, _, w, h, radius) = island_rect(0.0, 4.0);
        assert_eq!(w, 0.0);
        assert_eq!(radius, 0.0);
        assert!(radius <= h / 2.0);
    }

    #[test]
    fn island_fills_adapt_to_background_luminance() {
        let dark = island_fills([0.06, 0.05, 0.06, 1.0]);
        let light = island_fills([0.98, 0.98, 0.97, 1.0]);
        // Dark themes: inactive pills are solid (readable); active is brighter.
        assert!(dark.inactive[3] >= 0.9);
        assert!(dark.active[0] > dark.inactive[0]);
        assert_eq!(light.inactive[0], 0.0);
        // On light themes the active island must read as the brighter,
        // elevated card: a strong white overlay against the recessed
        // black-tinted inactive fill.
        assert_eq!(light.active[0], 1.0);
        assert!(light.active[3] >= 0.8);
        // Both themes keep a hairline so tabs read as separate pills.
        assert!(light.outline.is_some());
        assert!(dark.outline.is_some());
    }

    #[test]
    fn over_composites_source_over_destination() {
        let dst = [0.2, 0.4, 0.6, 1.0];
        let out = over(dst, [1.0, 1.0, 1.0, 0.25]);
        assert!((out[0] - 0.4).abs() < 1e-6);
        assert!((out[1] - 0.55).abs() < 1e-6);
        assert!((out[2] - 0.7).abs() < 1e-6);
        assert_eq!(out[3], 1.0);
        // Zero-alpha source is a no-op; full-alpha replaces.
        assert_eq!(over(dst, [0.9, 0.1, 0.3, 0.0]), dst);
        assert_eq!(over(dst, [0.9, 0.1, 0.3, 1.0]), [0.9, 0.1, 0.3, 1.0]);
    }

    #[test]
    fn test_island_initialization() {
        let inactive_color = [0.5, 0.5, 0.5, 1.0];
        let active_color = [0.9, 0.9, 0.9, 1.0];

        let island = Island::new(inactive_color, active_color, true, 240.0);

        assert_eq!(island.inactive_text_color, inactive_color);
        assert_eq!(island.active_text_color, active_color);
        assert!(island.hide_if_single);
    }

    #[test]
    fn test_island_height() {
        let island =
            Island::new([0.8, 0.8, 0.8, 1.0], [1.0, 1.0, 1.0, 1.0], false, 240.0);
        assert_eq!(island.height(), ISLAND_HEIGHT);
    }

    fn test_island() -> Island {
        Island::new([0.5, 0.5, 0.5, 1.0], [0.9, 0.9, 0.9, 1.0], false, 240.0)
    }

    #[test]
    fn progress_first_report_seeds_started_and_seen() {
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Indeterminate,
            progress: None,
        });
        assert!(island.progress_started_at.is_some());
        assert!(island.progress_last_seen.is_some());
        assert_eq!(island.progress_state, Some(ProgressState::Indeterminate));
    }

    #[test]
    fn progress_repeated_same_state_keeps_started_at_stable() {
        // Issue #1509: a TUI that heartbeats `OSC 9;4;3` (or any same-state
        // report) must NOT restart the indeterminate animation phase, or the
        // pulsing block snaps back to the left edge on every report.
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Indeterminate,
            progress: None,
        });
        let first_started = island.progress_started_at.unwrap();
        let first_seen = island.progress_last_seen.unwrap();

        // Sleep so a subsequent Instant::now() is observably later — the
        // started_at field must stay equal while last_seen advances.
        std::thread::sleep(std::time::Duration::from_millis(15));
        island.set_progress_report(ProgressReport {
            state: ProgressState::Indeterminate,
            progress: None,
        });

        assert_eq!(
            island.progress_started_at,
            Some(first_started),
            "started_at must not move on a same-state heartbeat"
        );
        assert!(
            island.progress_last_seen.unwrap() > first_seen,
            "last_seen must advance on every report"
        );
    }

    #[test]
    fn progress_state_transition_resets_started_at() {
        // Set → Indeterminate is a real state change, so the animation
        // anchor should be reseated. (Set has no animation, but the
        // started_at field still becomes meaningful as soon as we hit
        // Indeterminate.)
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Set,
            progress: Some(50),
        });
        let first = island.progress_started_at.unwrap();

        std::thread::sleep(std::time::Duration::from_millis(15));
        island.set_progress_report(ProgressReport {
            state: ProgressState::Indeterminate,
            progress: None,
        });

        assert!(
            island.progress_started_at.unwrap() > first,
            "transitioning into a new state must move started_at forward"
        );
        assert_eq!(island.progress_state, Some(ProgressState::Indeterminate));
    }

    #[test]
    fn progress_set_value_change_does_not_reseat_started_at() {
        // Same `Set` state with a different percentage is still the same
        // state — only the value updates. started_at stays put; the bar
        // just redraws at the new fraction.
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Set,
            progress: Some(20),
        });
        let first = island.progress_started_at.unwrap();

        std::thread::sleep(std::time::Duration::from_millis(15));
        island.set_progress_report(ProgressReport {
            state: ProgressState::Set,
            progress: Some(60),
        });

        assert_eq!(island.progress_started_at, Some(first));
        assert_eq!(island.progress_value, Some(60));
    }

    /// Each char = 1.0 wide, including the ellipsis. Easy arithmetic.
    fn fixed_unit_width(_c: char) -> f32 {
        1.0
    }

    fn rendered_width(s: &str, char_width: impl FnMut(char) -> f32) -> f32 {
        s.chars().map(char_width).sum()
    }

    #[test]
    fn title_fits_is_returned_unchanged() {
        assert_eq!(
            fit_title_with_widths("hello", 10.0, fixed_unit_width),
            "hello"
        );
        assert_eq!(fit_title_with_widths("hi", 2.0, fixed_unit_width), "hi");
    }

    #[test]
    fn title_that_fits_borrows_without_allocating() {
        // Confirms the zero-allocation "no truncation" hot path: when the
        // full title fits, the returned Cow must stay Borrowed so the
        // render loop doesn't allocate a new String every frame.
        let out = fit_title_with_widths("ok", 10.0, fixed_unit_width);
        assert!(
            matches!(out, Cow::Borrowed(_)),
            "expected borrowed, got {out:?}"
        );
    }

    #[test]
    fn title_zero_budget_returns_ellipsis() {
        // Historically this was short-circuited to return the full title;
        // now it falls through the loop and returns "…" consistently with
        // tiny-but-positive budgets.
        assert_eq!(fit_title_with_widths("abc", 0.0, fixed_unit_width), "…");
    }

    #[test]
    fn title_overflow_gets_ellipsized_and_fits_budget() {
        // "hello world" budgeted at 5 → best we can do without exceeding
        // is "hell" (4) + "…" (1) = 5. Anything more overflows.
        let out = fit_title_with_widths("hello world", 5.0, fixed_unit_width);
        assert_eq!(out, "hell…");
        assert!(
            rendered_width(&out, fixed_unit_width) <= 5.0,
            "truncated width {} must be ≤ budget 5",
            rendered_width(&out, fixed_unit_width)
        );
    }

    #[test]
    fn title_respects_budget_with_wide_chars() {
        // Mixed widths: 'W' = 2.0, others (including ellipsis) = 1.0.
        // Title "WxWxW", budget 4.0. Walk:
        // ix=0 W: before add, 0+1(suffix) ≤ 4 → truncate_ix=0; accum→2
        // ix=1 x: 2+1 ≤ 4 → truncate_ix=1; accum→3
        // ix=2 W: 3+1 ≤ 4 → truncate_ix=2; accum→5; 5>4 → cut.
        // Output: title[..2] + "…" = "Wx…", width 2+1+1 = 4 ≤ 4 ✓
        let widths = |c: char| if c == 'W' { 2.0 } else { 1.0 };
        let out = fit_title_with_widths("WxWxW", 4.0, widths);
        assert_eq!(out, "Wx…");
        assert!(rendered_width(&out, widths) <= 4.0);
    }

    #[test]
    fn title_truncation_preserves_utf8_boundaries() {
        // Each emoji/char = 2.0 wide; ellipsis = 2.0.
        // Title "🎟🎟🎟" = 6.0. Budget 4.0 → one emoji + "…" = 4.0 ≤ 4 ✓.
        // Crucial: the byte index we cut at must be on a UTF-8 boundary.
        let w = |_c: char| 2.0;
        let out = fit_title_with_widths("🎟🎟🎟", 4.0, w);
        assert_eq!(out, "🎟…");
        assert!(out.chars().count() == 2, "{out:?} should be 2 graphemes");
    }

    #[test]
    fn title_budget_smaller_than_ellipsis_still_returns_ellipsis() {
        // Budget 0.5 < ellipsis_width 1.0: first char overflows, prefix is
        // empty, we return just "…" so the user at least sees *something*
        // indicating truncation rather than a blank tab label.
        let out = fit_title_with_widths("abc", 0.5, fixed_unit_width);
        assert_eq!(out, "…");
    }

    #[test]
    fn title_empty_input_returned_as_is() {
        assert_eq!(fit_title_with_widths("", 10.0, fixed_unit_width), "");
    }

    #[test]
    fn title_exact_fit_not_truncated() {
        // Title "abcd" = 4.0, budget 4.0 → fits exactly, no truncation.
        assert_eq!(fit_title_with_widths("abcd", 4.0, fixed_unit_width), "abcd");
    }

    #[test]
    fn tab_strip_layout_geometry() {
        // Overflow: four equal naturals compress proportionally to fill.
        let natural = 168.0;
        let layout = tab_strip_layout_from_widths(1000.0, 2.0, 240.0, &[natural; 4]);
        let left = island_margin_left();
        let right = island_margin_right();
        assert_eq!(layout.left_margin, left);
        assert_eq!(layout.right_margin, right);
        let expected = (500.0 - right - left) / 4.0;
        assert!((layout.width_at(0) - expected).abs() < 0.01);
        assert!((layout.tabs_width() - expected * 4.0).abs() < 0.01);
        assert!(tab_strip_layout(1000.0, 2.0, 0, 240.0).is_empty());
    }

    #[test]
    fn tab_strip_layout_caps_slot_width() {
        let wide = tab_slot_width_for_content(200.0, true, true);
        let layout = tab_strip_layout_from_widths(3000.0, 2.0, 240.0, &[wide, wide]);
        assert_eq!(layout.width_at(0), 240.0);
        assert_eq!(layout.tabs_width(), 480.0);
        assert!(layout.left_margin + layout.tabs_width() < 1500.0);

        let layout = tab_strip_layout_from_widths(10.0, 2.0, 240.0, &[wide; 4]);
        assert_eq!(layout.width_at(0), 0.0);
        assert_eq!(layout.tabs_width(), 0.0);
    }

    #[test]
    fn remap_tab_move_forward_rotates_indices() {
        // Move tab 1 → 3: tabs 2 and 3 shift left by one.
        assert_eq!(Island::remap_index(1, 1, 3), 3);
        assert_eq!(Island::remap_index(2, 1, 3), 1);
        assert_eq!(Island::remap_index(3, 1, 3), 2);
        assert_eq!(Island::remap_index(0, 1, 3), 0);
        assert_eq!(Island::remap_index(4, 1, 3), 4);
    }

    #[test]
    fn remap_tab_move_backward_rotates_indices() {
        // Move tab 3 → 0: tabs 0, 1, 2 shift right by one.
        assert_eq!(Island::remap_index(3, 3, 0), 0);
        assert_eq!(Island::remap_index(0, 3, 0), 1);
        assert_eq!(Island::remap_index(1, 3, 0), 2);
        assert_eq!(Island::remap_index(2, 3, 0), 3);
        assert_eq!(Island::remap_index(4, 3, 0), 4);
    }

    #[test]
    fn remap_tab_move_carries_picker_and_springs() {
        let mut island = test_island();
        island.color_picker_tab = Some(3);

        // Tab 1 → 3 (rotate): the open picker shifts 3 → 2. Per-tab colors
        // and titles now live on the tab in ContextManager (see
        // context::test::test_custom_color_* / test_custom_title_*), so they
        // no longer need remapping here.
        island.remap_tab_move(1, 3, 100.0);
        assert_eq!(island.color_picker_tab, Some(2));

        // Displaced tabs (now at 1 and 2) got slide springs of +width.
        assert_eq!(island.slide_springs.len(), 2);
        assert_eq!(island.slide_springs.get(&1).unwrap().position, 100.0);
        assert_eq!(island.slide_springs.get(&2).unwrap().position, 100.0);
    }

    #[test]
    fn drag_threshold_gates_start() {
        let mut island = test_island();
        island.start_drag(0, 10.0, 50.0);
        assert!(island.is_dragging());
        assert_eq!(island.drag_index(), None, "not started below threshold");
        assert!(!island.update_drag(52.0));
        assert!(island.update_drag(58.0), "8px exceeds threshold");
        assert_eq!(island.drag_index(), Some(0));
        island.cancel_drag();
        assert!(!island.is_dragging());
    }

    fn test_layout() -> TabStripLayout {
        TabStripLayout {
            left_margin: 0.0,
            right_margin: ISLAND_MARGIN_RIGHT,
            widths: smallvec::smallvec![100.0, 100.0, 100.0, 100.0],
        }
    }

    #[test]
    fn close_button_anchors_to_island_right_edge() {
        // Full-width slot: slot 1 spans 180..360, island 183..354, so
        // the button centers at 354 - CLOSE_MARGIN_RIGHT.
        let layout = TabStripLayout {
            left_margin: 0.0,
            right_margin: ISLAND_MARGIN_RIGHT,
            widths: smallvec::smallvec![180.0, 180.0],
        };
        let cx = close_button_center_x(&layout, 1).unwrap();
        assert_eq!(
            cx,
            180.0 + TAB_GAP / 2.0 + (180.0 - TAB_GAP) - CLOSE_MARGIN_RIGHT
        );
        // The whole forgiving hit box stays inside the island.
        assert!(cx + CLOSE_HIT_HALF_WIDTH <= 360.0 - TAB_GAP / 2.0);

        // Narrow islands (many tabs) drop the button — no hit box, so
        // rendering and click handling agree via the shared helper.
        let narrow = TabStripLayout {
            left_margin: 0.0,
            right_margin: ISLAND_MARGIN_RIGHT,
            widths: smallvec::smallvec![60.0; 10],
        };
        assert_eq!(close_button_center_x(&narrow, 3), None);
    }

    #[test]
    fn close_hit_box_clears_the_title_budget() {
        // Left-padded title on slot 0 ends at island_x + TAB_PADDING_X +
        // max_text; the close hit box must start at or after that point.
        let layout = TabStripLayout {
            left_margin: 0.0,
            right_margin: ISLAND_MARGIN_RIGHT,
            widths: smallvec::smallvec![180.0, 180.0],
        };
        let cx = close_button_center_x(&layout, 0).unwrap();
        let island_x = TAB_GAP / 2.0;
        let max_text = (layout.width_at(0)
            - TAB_GAP
            - TAB_PADDING_X
            - CLOSE_MARGIN_RIGHT
            - CLOSE_HIT_HALF_WIDTH)
            .max(0.0);
        let title_max_right = island_x + TAB_PADDING_X + max_text;
        assert!(cx - CLOSE_HIT_HALF_WIDTH >= title_max_right);
    }

    #[test]
    fn drag_center_clamps_to_strip() {
        let mut island = test_island();
        // Tab 0 grabbed 10px from its left edge, tabs region spans
        // 0..400 with 100-wide slots.
        island.start_drag(0, 10.0, 50.0);
        island.update_drag(200.0); // started
        let center = island.drag_center(&test_layout()).unwrap();
        assert_eq!(center, 190.0 + 50.0);

        // Dragged far right: floating left clamps to 300, center 350.
        island.update_drag(1000.0);
        assert_eq!(island.drag_center(&test_layout()), Some(350.0));

        // Far left: clamps to 0, center 50.
        island.update_drag(-500.0);
        assert_eq!(island.drag_center(&test_layout()), Some(50.0));
    }

    #[test]
    fn end_drag_seeds_settle_spring() {
        let mut island = test_island();
        island.start_drag(2, 0.0, 200.0);
        island.update_drag(250.0); // floating left = 250, slot x = 200
        island.end_drag(&test_layout());
        assert!(!island.is_dragging());
        let spring = island.slide_springs.get(&2).unwrap();
        assert_eq!(spring.position, 50.0);
    }

    #[test]
    fn progress_remove_clears_all_progress_state() {
        let mut island = test_island();
        island.set_progress_report(ProgressReport {
            state: ProgressState::Set,
            progress: Some(50),
        });
        island.set_progress_report(ProgressReport {
            state: ProgressState::Remove,
            progress: None,
        });
        assert!(island.progress_state.is_none());
        assert!(island.progress_value.is_none());
        assert!(island.progress_started_at.is_none());
        assert!(island.progress_last_seen.is_none());
    }
}
