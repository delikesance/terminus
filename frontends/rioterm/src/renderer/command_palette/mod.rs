// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

// Most of this upstream Rio module is superseded by the Terminus chrome;
// what is left unused is kept to ease upstream merges.
#![allow(dead_code)]

use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;

/// Convert `[f32; 4]` colour to `[u8; 4]` for the `Text` API (the
/// vertex shader premultiplies, so pass non-premul RGBA).
#[inline]
fn color_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

// Layout
const PALETTE_WIDTH: f32 = 480.0;
const PALETTE_MARGIN_TOP: f32 = 80.0;
const PALETTE_PADDING: f32 = 4.0;

const INPUT_HEIGHT: f32 = 40.0;
const INPUT_FONT_SIZE: f32 = 14.0;
const INPUT_PADDING_X: f32 = 14.0;

const RESULT_ITEM_HEIGHT: f32 = 32.0;
const RESULT_FONT_SIZE: f32 = 13.0;
const SHORTCUT_FONT_SIZE: f32 = 11.0;
const MAX_VISIBLE_RESULTS: usize = 8;

// Copy icon (two overlapping page outlines with rounded corners,
// drawn by layering filled + cutout rounded rects). Sized to fit
// comfortably inside RESULT_ITEM_HEIGHT.
const COPY_ICON_PAGE_W: f32 = 10.0;
const COPY_ICON_PAGE_H: f32 = 12.0;
const COPY_ICON_OFFSET: f32 = 3.0;
const COPY_ICON_STROKE: f32 = 1.0;
const COPY_ICON_RADIUS: f32 = 2.0;
const COPY_ICON_W: f32 = COPY_ICON_PAGE_W + COPY_ICON_OFFSET; // 13
const COPY_ICON_H: f32 = COPY_ICON_PAGE_H + COPY_ICON_OFFSET; // 15

const SEPARATOR_HEIGHT: f32 = 1.0;
const RESULTS_MARGIN_TOP: f32 = 2.0;
const CARET_BLINK_MS: u128 = 500;

// Colors — Apple HIG flat dark (match terminus chrome theme)
const BACKDROP_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.60];
const BG_COLOR: [f32; 4] = [
    0x11 as f32 / 255.0,
    0x11 as f32 / 255.0,
    0x13 as f32 / 255.0,
    1.0,
];
const SELECTED_BG_COLOR: [f32; 4] = [0x0a as f32 / 255.0, 0x84 as f32 / 255.0, 1.0, 0.15];
const TEXT_COLOR: [f32; 4] = [
    0xf1 as f32 / 255.0,
    0xf5 as f32 / 255.0,
    0xf9 as f32 / 255.0,
    1.0,
];
const DIM_TEXT_COLOR: [f32; 4] = [
    0x94 as f32 / 255.0,
    0xa3 as f32 / 255.0,
    0xb8 as f32 / 255.0,
    1.0,
];
const SHORTCUT_TEXT_COLOR: [f32; 4] = [
    0x64 as f32 / 255.0,
    0x74 as f32 / 255.0,
    0x8b as f32 / 255.0,
    1.0,
];
const SEPARATOR_COLOR: [f32; 4] = [
    0x2f as f32 / 255.0,
    0x2f as f32 / 255.0,
    0x35 as f32 / 255.0,
    1.0,
];
const PALETTE_CORNER_RADIUS: f32 = 12.0;

// Depth / order
const DEPTH_BACKDROP: f32 = 0.0;
const DEPTH_BG: f32 = 0.1;
const DEPTH_ELEMENT: f32 = 0.2;
const ORDER: u8 = 20;

/// Command palette UI component (Raycast-style)
pub struct CommandPalette {
    enabled: bool,
    /// Search text; edits go through [`TextDraft`] like every other field.
    pub query: terminus_ui::TextDraft,
    pub selected_index: usize,
    scroll_offset: usize,
    pub has_adaptive_theme: bool,
    /// Which list the palette is showing (commands, fonts, or hosts).
    mode: PaletteMode,
    /// Snapshot of stored hosts for Commands-mode inline matching.
    /// Refreshed when the palette opens / when entering Hosts mode.
    hosts_cache: Vec<HostPaletteItem>,
    /// What confirming a host in the hosts list does.
    host_pick: HostPick,
    /// Snapshot of the on-screen machine's tunnels for `>forward`.
    tunnels_cache: Vec<TunnelPaletteItem>,
    /// Timestamp for caret blinking
    caret_blink_start: Instant,
    /// Timestamp of the last event that actually changed `scroll_offset`.
    /// Drives the scrollbar fade-in/fade-out, sharing the terminal
    /// scrollbar's 2 s delay + 300 ms fade envelope via
    /// `scrollbar::opacity_from_last_scroll`. `None` while the palette
    /// has never scrolled since it opened — scrollbar stays hidden.
    last_scroll_time: Option<Instant>,
    /// Shortcut labels from the window's live key bindings.
    shortcuts: Vec<(PaletteAction, String)>,
    /// Geometry of the last painted frame: hit-testing walks exactly this.
    last_layout: Option<terminus_ui::components::overlay::PaletteLayout>,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            enabled: false,
            query: terminus_ui::TextDraft::default(),
            selected_index: 0,
            scroll_offset: 0,
            has_adaptive_theme: false,
            mode: PaletteMode::Commands,
            hosts_cache: Vec::new(),
            host_pick: HostPick::Session,
            tunnels_cache: Vec::new(),
            caret_blink_start: Instant::now(),
            last_scroll_time: None,
            shortcuts: Vec::new(),
            last_layout: None,
        }
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.query.clear();
            self.selected_index = 0;
            self.scroll_offset = 0;
            self.caret_blink_start = Instant::now();
            // Clear scrollbar history so reopening the palette never
            // flashes a leftover scrollbar from the previous session.
            self.last_scroll_time = None;
            // Always re-open into Commands mode — a stale Fonts/Hosts list
            // from a previous session would be misleading and surprising.
            self.mode = PaletteMode::Commands;
            self.host_pick = HostPick::Session;
        }
    }

    /// Refresh the host snapshot used by Commands-mode inline matching.
    pub fn set_hosts(&mut self, hosts: Vec<HostPaletteItem>) {
        self.hosts_cache = hosts;
    }

    /// Refresh the tunnel snapshot used by `>forward`.
    pub fn set_tunnels(&mut self, tunnels: Vec<TunnelPaletteItem>) {
        self.tunnels_cache = tunnels;
    }

    /// How the query reads. `>` is a literal character in the fonts and
    /// hosts lists, so only the commands list has operations.
    fn op_query(&self) -> terminus_ui::palette_query::Query {
        match self.mode {
            PaletteMode::Commands => terminus_ui::palette_query::parse(&self.query.value),
            _ => terminus_ui::palette_query::Query::Plain,
        }
    }

    /// Swap the palette into font-browsing mode with the given family
    /// list. Clears the query so the full list is visible, keeps the
    /// palette open. Called by the router after the user picks the
    /// `List Fonts` command.
    pub fn enter_fonts_mode(&mut self, fonts: Vec<String>) {
        self.mode = PaletteMode::Fonts(fonts);
        self.query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.caret_blink_start = Instant::now();
        self.last_scroll_time = None;
    }

    /// Swap into host-browsing mode (same stay-open pattern as fonts).
    pub fn enter_hosts_mode(&mut self, hosts: Vec<HostPaletteItem>) {
        self.enter_hosts_mode_for(hosts, HostPick::Session);
    }

    /// Host list whose confirmation does `pick` (session or SFTP).
    pub fn enter_hosts_mode_for(&mut self, hosts: Vec<HostPaletteItem>, pick: HostPick) {
        self.host_pick = pick;
        self.enter_hosts_list(hosts);
    }

    /// What confirming a host does in the current hosts list.
    pub fn host_pick(&self) -> HostPick {
        use terminus_ui::palette_query::{PaletteOp, Query};
        match self.op_query() {
            Query::Op(PaletteOp::Sftp { .. }) => HostPick::Sftp,
            _ => self.host_pick,
        }
    }

    fn enter_hosts_list(&mut self, hosts: Vec<HostPaletteItem>) {
        self.hosts_cache = hosts.clone();
        self.mode = PaletteMode::Hosts(hosts);
        self.query.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.caret_blink_start = Instant::now();
        self.last_scroll_time = None;
    }

    /// Append committed or typed text to the query, applying the
    /// shared overlay input policy (`is_printable_text`) so the key
    /// path and the IME commit path can never drift. Returns whether
    /// anything was appended.
    pub fn append_query(&mut self, text: &str) -> bool {
        if !crate::renderer::is_printable_text(text) {
            return false;
        }
        if !self.query.insert(text, usize::MAX, false) {
            return false;
        }
        self.reset_results();
        true
    }

    pub fn set_query(&mut self, query: String) {
        self.query = terminus_ui::TextDraft::new(query);
        self.reset_results();
    }

    /// Shared text editing (Backspace, Delete, caret, selection).
    pub fn edit_query(&mut self, edit: terminus_ui::TextEdit) -> bool {
        if !self.query.apply(edit) {
            return false;
        }
        self.reset_results();
        true
    }

    fn reset_results(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.caret_blink_start = Instant::now();
        // Typing reshapes the list entirely — drop any scrollbar
        // fade state so the next scroll starts with a clean timer.
        self.last_scroll_time = None;
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
                self.last_scroll_time = Some(Instant::now());
            }
        }
    }

    pub fn move_selection_down(&mut self) {
        let count = self.filtered_rows().len();
        if self.selected_index < count.saturating_sub(1) {
            self.selected_index += 1;
            if self.selected_index >= self.scroll_offset + MAX_VISIBLE_RESULTS {
                self.scroll_offset = self.selected_index - MAX_VISIBLE_RESULTS + 1;
                self.last_scroll_time = Some(Instant::now());
            }
        }
    }

    pub fn get_selected_action(&self) -> Option<PaletteAction> {
        self.filtered_rows()
            .get(self.selected_index)
            .and_then(|(_, row)| row.action())
    }

    /// Selected family name if (and only if) the palette is in fonts
    /// mode and the selection points at a valid row. Owned `String`
    /// so the caller can mutate the palette state (`set_enabled`) in
    /// the same statement without fighting the borrow checker.
    pub fn get_selected_font(&self) -> Option<String> {
        self.filtered_rows()
            .get(self.selected_index)
            .and_then(|(_, row)| match row {
                PaletteRow::Font { family } => Some((*family).to_owned()),
                PaletteRow::Command { .. }
                | PaletteRow::Host { .. }
                | PaletteRow::Tunnel { .. }
                | PaletteRow::OpHint { .. } => None,
            })
    }

    /// Selected host id when the current row is a host (Commands or Hosts mode).
    pub fn get_selected_host_id(&self) -> Option<String> {
        self.filtered_rows()
            .get(self.selected_index)
            .and_then(|(_, row)| match row {
                PaletteRow::Host { id, .. } => Some((*id).to_owned()),
                PaletteRow::Command { .. }
                | PaletteRow::Font { .. }
                | PaletteRow::Tunnel { .. }
                | PaletteRow::OpHint { .. } => None,
            })
    }

    /// Selected tunnel id and what to do with it (`>forward ...` rows).
    pub fn get_selected_tunnel(&self) -> Option<(String, TunnelOp)> {
        self.filtered_rows()
            .get(self.selected_index)
            .and_then(|(_, row)| match row {
                PaletteRow::Tunnel { id, op, .. } => Some(((*id).to_owned(), *op)),
                _ => None,
            })
    }

    /// Confirm a completion row (`>`, `>sf`): the query becomes the
    /// operation's prefix and the palette stays open. False when the
    /// selection is not a completion.
    pub fn complete_selected_op(&mut self) -> bool {
        let complete = self.filtered_rows().get(self.selected_index).and_then(
            |(_, row)| match row {
                PaletteRow::OpHint { complete, .. } => Some(*complete),
                _ => None,
            },
        );
        match complete {
            Some(text) => {
                self.set_query(text.to_string());
                true
            }
            None => false,
        }
    }

    /// Shortcut labels to show, from the window's live key bindings.
    pub fn set_shortcuts(&mut self, shortcuts: Vec<(PaletteAction, String)>) {
        self.shortcuts = shortcuts;
    }

    fn shortcut_for(&self, action: PaletteAction) -> &str {
        self.shortcuts
            .iter()
            .find(|(a, _)| *a == action)
            .map_or("", |(_, label)| label.as_str())
    }
}

mod action;
mod draw;
mod filter;
mod hit;
mod rows;
pub use self::action::*;
pub use self::rows::*;
#[cfg(test)]
mod palette_ops_tests;
#[cfg(test)]
mod palette_tests;
