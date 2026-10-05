// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::renderer::scrollbar;
use rio_backend::sugarloaf::text::DrawOpts;
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

/// Actions that can be triggered from the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    TabCreate,
    TabClose,
    TabCloseUnfocused,
    SelectNextTab,
    SelectPrevTab,
    SplitRight,
    SplitDown,
    SelectNextSplit,
    SelectPrevSplit,
    ConfigEditor,
    WindowCreateNew,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    ToggleViMode,
    ToggleFullscreen,
    ToggleAppearanceTheme,
    Copy,
    Paste,
    SearchForward,
    SearchBackward,
    ClearHistory,
    CloseCurrentSplitOrTab,
    /// Browse the family names of every registered font. Does NOT
    /// execute a one-shot action — the palette stays open with the
    /// font list as its contents. Handled by `router`, not
    /// `Screen::execute_palette_action`.
    ListFonts,
    /// Browse stored SSH hosts. Same stay-open mode-switch pattern as
    /// [`Self::ListFonts`]. Enter on a host opens a session.
    ListHosts,
    /// Open the SFTP dual-pane: directly with one host, else via a picker.
    OpenSftp,
    /// Look for a new Terminus release now.
    CheckForUpdates,
    /// Download and install the release found by the last check.
    InstallUpdate,
    /// Relaunch into an update that is already installed.
    RestartToUpdate,
    Quit,
}

struct Command {
    title: &'static str,
    action: PaletteAction,
}

/// Every action the command catalog offers.
pub fn command_actions() -> impl Iterator<Item = PaletteAction> {
    COMMANDS.iter().map(|cmd| cmd.action)
}

impl PaletteAction {
    /// The key-binding action this command runs, whose shortcut the
    /// palette shows. `None` for palette-only commands.
    pub fn binding_action(self) -> Option<crate::bindings::Action> {
        use crate::bindings::Action;
        Some(match self {
            PaletteAction::TabCreate => Action::TabCreateNew,
            PaletteAction::TabClose => Action::TabCloseCurrent,
            PaletteAction::TabCloseUnfocused => Action::TabCloseUnfocused,
            PaletteAction::SelectNextTab => Action::SelectNextTab,
            PaletteAction::SelectPrevTab => Action::SelectPrevTab,
            PaletteAction::SplitRight => Action::SplitRight,
            PaletteAction::SplitDown => Action::SplitDown,
            PaletteAction::SelectNextSplit => Action::SelectNextSplit,
            PaletteAction::SelectPrevSplit => Action::SelectPrevSplit,
            PaletteAction::CloseCurrentSplitOrTab => Action::CloseCurrentSplitOrTab,
            PaletteAction::ConfigEditor => Action::ConfigEditor,
            PaletteAction::WindowCreateNew => Action::WindowCreateNew,
            PaletteAction::IncreaseFontSize => Action::IncreaseFontSize,
            PaletteAction::DecreaseFontSize => Action::DecreaseFontSize,
            PaletteAction::ResetFontSize => Action::ResetFontSize,
            PaletteAction::ToggleViMode => Action::ToggleViMode,
            PaletteAction::ToggleFullscreen => Action::ToggleFullscreen,
            PaletteAction::ToggleAppearanceTheme => Action::ToggleAppearanceTheme,
            PaletteAction::Copy => Action::Copy,
            PaletteAction::Paste => Action::Paste,
            PaletteAction::SearchForward => Action::SearchForward,
            PaletteAction::SearchBackward => Action::SearchBackward,
            PaletteAction::ClearHistory => Action::ClearHistory,
            PaletteAction::Quit => Action::Quit,
            _ => return None,
        })
    }
}

const COMMANDS: &[Command] = &[
    Command {
        title: "New Tab",
        action: PaletteAction::TabCreate,
    },
    Command {
        title: "Close Tab",
        action: PaletteAction::TabClose,
    },
    Command {
        title: "Close Other Tabs",
        action: PaletteAction::TabCloseUnfocused,
    },
    Command {
        title: "Next Tab",
        action: PaletteAction::SelectNextTab,
    },
    Command {
        title: "Previous Tab",
        action: PaletteAction::SelectPrevTab,
    },
    Command {
        title: "Split Right",
        action: PaletteAction::SplitRight,
    },
    Command {
        title: "Split Down",
        action: PaletteAction::SplitDown,
    },
    Command {
        title: "Next Split",
        action: PaletteAction::SelectNextSplit,
    },
    Command {
        title: "Previous Split",
        action: PaletteAction::SelectPrevSplit,
    },
    Command {
        title: "Close Split or Tab",
        action: PaletteAction::CloseCurrentSplitOrTab,
    },
    Command {
        title: "Settings",
        action: PaletteAction::ConfigEditor,
    },
    Command {
        title: "New Window",
        action: PaletteAction::WindowCreateNew,
    },
    Command {
        title: "Increase Font Size",
        action: PaletteAction::IncreaseFontSize,
    },
    Command {
        title: "Decrease Font Size",
        action: PaletteAction::DecreaseFontSize,
    },
    Command {
        title: "Reset Font Size",
        action: PaletteAction::ResetFontSize,
    },
    Command {
        title: "Toggle Vi Mode",
        action: PaletteAction::ToggleViMode,
    },
    Command {
        title: "Toggle Fullscreen",
        action: PaletteAction::ToggleFullscreen,
    },
    Command {
        title: "Toggle Appearance Theme",
        action: PaletteAction::ToggleAppearanceTheme,
    },
    Command {
        title: "Copy",
        action: PaletteAction::Copy,
    },
    Command {
        title: "Paste",
        action: PaletteAction::Paste,
    },
    Command {
        title: "Search Forward",
        action: PaletteAction::SearchForward,
    },
    Command {
        title: "Search Backward",
        action: PaletteAction::SearchBackward,
    },
    Command {
        title: "Clear History",
        action: PaletteAction::ClearHistory,
    },
    Command {
        title: "List Fonts",
        action: PaletteAction::ListFonts,
    },
    Command {
        title: "Open Host…",
        action: PaletteAction::ListHosts,
    },
    Command {
        title: "Open SFTP",
        action: PaletteAction::OpenSftp,
    },
    Command {
        title: "Check for Updates",
        action: PaletteAction::CheckForUpdates,
    },
    Command {
        title: "Install Update",
        action: PaletteAction::InstallUpdate,
    },
    Command {
        title: "Restart to Update",
        action: PaletteAction::RestartToUpdate,
    },
    Command {
        title: "Quit",
        action: PaletteAction::Quit,
    },
];

/// What the palette is currently browsing and filtering over.
///
/// `Commands` is the default — fuzzy-matches against the static
/// `COMMANDS` list (and matching hosts when the query is non-empty)
/// and dispatches a `PaletteAction` / opens a host on Enter.
///
/// `Fonts` is entered via the `ListFonts` command. The palette stays
/// open, its content is replaced with the owned list of font family
/// names, and Enter closes the palette (no font-switching action yet).
/// The list is owned so the filter pass doesn't keep a borrow on the
/// sugarloaf FontLibrary.
///
/// `Hosts` is entered via `ListHosts` ("Open Host…"). Same stay-open
/// pattern as Fonts; Enter opens an SSH session for the selected host.
enum PaletteMode {
    Commands,
    Fonts(Vec<String>),
    Hosts(Vec<HostPaletteItem>),
}

/// What confirming a host in the palette's hosts list does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPick {
    /// Open (or focus) a shell session.
    Session,
    /// Open the SFTP dual pane for it.
    Sftp,
}

/// One stored SSH host as the palette needs it (no secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPaletteItem {
    pub id: String,
    /// Display name from the store.
    pub title: String,
    /// `user@host[:port]` for secondary matching / hint.
    pub subtitle: String,
}

/// One row in the filtered result list. Variants carry exactly the
/// data the render pass needs — no `&'static Command` vs `&str`
/// lifetime mixing.
enum PaletteRow<'a> {
    Command {
        title: &'a str,
        shortcut: &'a str,
        action: PaletteAction,
    },
    Font {
        family: &'a str,
    },
    Host {
        id: &'a str,
        title: &'a str,
        subtitle: &'a str,
    },
}

impl<'a> PaletteRow<'a> {
    fn title(&self) -> &'a str {
        match *self {
            PaletteRow::Command { title, .. } => title,
            PaletteRow::Font { family } => family,
            PaletteRow::Host { title, .. } => title,
        }
    }

    fn shortcut(&self) -> &'a str {
        match *self {
            PaletteRow::Command { shortcut, .. } => shortcut,
            PaletteRow::Font { .. } | PaletteRow::Host { .. } => "",
        }
    }

    fn action(&self) -> Option<PaletteAction> {
        match *self {
            PaletteRow::Command { action, .. } => Some(action),
            PaletteRow::Font { .. } | PaletteRow::Host { .. } => None,
        }
    }
}

/// Paint a rounded-rect outline via the shared chrome [`paint_surface_stroke`].
#[allow(clippy::too_many_arguments)]
fn stroke_rounded_rect(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    stroke: f32,
    radius: f32,
    stroke_color: [f32; 4],
    fill_color: [f32; 4],
    depth: f32,
    order: u8,
) {
    crate::renderer::chrome::paint_surface_stroke(
        sugarloaf,
        &terminus_ui::Rect::new(x, y, width, height),
        fill_color,
        Some(stroke_color),
        radius,
        stroke,
        depth,
        order,
        false,
    );
}

/// Paint a "copy" icon (two overlapping rounded page outlines)
/// anchored at `(x, y)`. Drawn from rects only — no font glyph
/// dependency — so it renders consistently regardless of what the
/// user's font stack can produce for ⎘ / 📋 / similar.
///
/// `row_fill_color` is the background behind the icon (palette BG when
/// the row is idle, selection highlight when hovered/selected); it's
/// used to cut out the page interiors so the outlines read as a
/// proper border rather than two solid blobs. Back page painted
/// slightly below the front via depth so the front's cutout
/// correctly hides the overlapping portion of the back's stroke.
#[allow(clippy::too_many_arguments)]
fn draw_copy_icon(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    stroke_color: [f32; 4],
    row_fill_color: [f32; 4],
    depth: f32,
    order: u8,
) {
    // Back page (upper-left).
    stroke_rounded_rect(
        sugarloaf,
        x,
        y,
        COPY_ICON_PAGE_W,
        COPY_ICON_PAGE_H,
        COPY_ICON_STROKE,
        COPY_ICON_RADIUS,
        stroke_color,
        row_fill_color,
        depth,
        order,
    );
    // Front page (offset down-right), painted above the back so its
    // cutout hides the back's overlapping interior.
    stroke_rounded_rect(
        sugarloaf,
        x + COPY_ICON_OFFSET,
        y + COPY_ICON_OFFSET,
        COPY_ICON_PAGE_W,
        COPY_ICON_PAGE_H,
        COPY_ICON_STROKE,
        COPY_ICON_RADIUS,
        stroke_color,
        row_fill_color,
        depth + 0.01,
        order,
    );
}

/// Fuzzy match: checks if all query chars appear in order in the target.
/// Returns a score (higher = better match), or None if no match.
fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let target_lower: Vec<char> = target.to_lowercase().chars().collect();

    if query_lower.is_empty() {
        return Some(0);
    }

    let mut qi = 0;
    let mut score: i32 = 0;
    let mut prev_match = false;
    let mut first_match_pos = None;

    for (ti, &tc) in target_lower.iter().enumerate() {
        if qi < query_lower.len() && tc == query_lower[qi] {
            if first_match_pos.is_none() {
                first_match_pos = Some(ti);
            }
            // Consecutive match bonus
            if prev_match {
                score += 5;
            }
            // Word boundary bonus (start of string or after space/punctuation)
            if ti == 0 || !target_lower[ti - 1].is_alphanumeric() {
                score += 10;
            }
            prev_match = true;
            qi += 1;
        } else {
            prev_match = false;
        }
    }

    if qi < query_lower.len() {
        return None; // Not all query chars matched
    }

    // Bonus for matching near the start
    if let Some(pos) = first_match_pos {
        score += (20_i32).saturating_sub(pos as i32);
    }

    Some(score)
}

/// Best fuzzy score across a host's searchable fields (empty query → 0).
fn host_fuzzy_score(query: &str, host: &HostPaletteItem) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    [
        fuzzy_score(query, &host.title),
        fuzzy_score(query, &host.subtitle),
        fuzzy_score(query, &host.id),
    ]
    .into_iter()
    .flatten()
    .max()
}

/// Command palette UI component (Raycast-style)
pub struct CommandPalette {
    enabled: bool,
    pub query: String,
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
            query: String::new(),
            selected_index: 0,
            scroll_offset: 0,
            has_adaptive_theme: false,
            mode: PaletteMode::Commands,
            hosts_cache: Vec::new(),
            host_pick: HostPick::Session,
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
        self.host_pick
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
        let query = format!("{}{}", self.query, text);
        self.set_query(query);
        true
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
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
                PaletteRow::Command { .. } | PaletteRow::Host { .. } => None,
            })
    }

    /// Selected host id when the current row is a host (Commands or Hosts mode).
    pub fn get_selected_host_id(&self) -> Option<String> {
        self.filtered_rows()
            .get(self.selected_index)
            .and_then(|(_, row)| match row {
                PaletteRow::Host { id, .. } => Some((*id).to_owned()),
                PaletteRow::Command { .. } | PaletteRow::Font { .. } => None,
            })
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

    /// Filtered list of rows for the current mode. Modes share the same
    /// fuzzy-score + sort pipeline so typing behaves identically.
    fn filtered_rows(&self) -> Vec<(i32, PaletteRow<'_>)> {
        let mut results: Vec<(i32, PaletteRow<'_>)> = match &self.mode {
            PaletteMode::Commands => {
                let has_adaptive = self.has_adaptive_theme;
                let mut rows: Vec<(i32, PaletteRow<'_>)> = COMMANDS
                    .iter()
                    .filter(|cmd| {
                        if cmd.action == PaletteAction::ToggleAppearanceTheme {
                            return has_adaptive;
                        }
                        true
                    })
                    .filter_map(|cmd| {
                        let score = fuzzy_score(&self.query, cmd.title)?;
                        Some((
                            score,
                            PaletteRow::Command {
                                title: cmd.title,
                                shortcut: self.shortcut_for(cmd.action),
                                action: cmd.action,
                            },
                        ))
                    })
                    .collect();
                // Inline host jump: when the user is already typing, also
                // surface matching hosts so Ctrl+Shift+P → "prod" → Enter works
                // without entering Hosts mode first. Empty query keeps the
                // catalog command-only to avoid dumping a long host list.
                if !self.query.is_empty() {
                    for host in &self.hosts_cache {
                        if let Some(score) = host_fuzzy_score(&self.query, host) {
                            rows.push((
                                score,
                                PaletteRow::Host {
                                    id: host.id.as_str(),
                                    title: host.title.as_str(),
                                    subtitle: host.subtitle.as_str(),
                                },
                            ));
                        }
                    }
                }
                rows
            }
            PaletteMode::Fonts(fonts) => fonts
                .iter()
                .filter_map(|family| {
                    let score = fuzzy_score(&self.query, family)?;
                    Some((score, PaletteRow::Font { family }))
                })
                .collect(),
            PaletteMode::Hosts(hosts) => hosts
                .iter()
                .filter_map(|host| {
                    let score = host_fuzzy_score(&self.query, host)?;
                    Some((
                        score,
                        PaletteRow::Host {
                            id: host.id.as_str(),
                            title: host.title.as_str(),
                            subtitle: host.subtitle.as_str(),
                        },
                    ))
                })
                .collect(),
        };

        results.sort_by_key(|r| std::cmp::Reverse(r.0));
        // Servers list after commands so each group is contiguous (the
        // overlay palette draws one header per group). Stable: scores keep
        // their order inside a group.
        results.sort_by_key(|(_, row)| matches!(row, PaletteRow::Host { .. }));
        results
    }

    /// Group header a row belongs under.
    fn group_of(row: &PaletteRow<'_>) -> &'static str {
        match row {
            PaletteRow::Command { .. } => "Commands",
            PaletteRow::Font { .. } => "Fonts",
            PaletteRow::Host { .. } => "Servers",
        }
    }

    fn placeholder(&self) -> &'static str {
        match self.mode {
            PaletteMode::Commands => "Search servers and commands",
            PaletteMode::Fonts(_) => "Type a font name",
            PaletteMode::Hosts(_) if self.host_pick == HostPick::Sftp => {
                "Choose a server to browse"
            }
            PaletteMode::Hosts(_) => "Type a server name",
        }
    }

    /// Rows as the overlay palette shows them (group, label, right hint).
    fn row_specs(&self) -> Vec<terminus_ui::palette_view::RowSpec> {
        use terminus_ui::palette_view::RowSpec;
        self.filtered_rows()
            .iter()
            .map(|(_, row)| {
                let hint = match row {
                    PaletteRow::Command { shortcut, .. } => (*shortcut).to_string(),
                    PaletteRow::Host { subtitle, .. } => (*subtitle).to_string(),
                    PaletteRow::Font { .. } => "Copy".to_string(),
                };
                RowSpec::new(Self::group_of(row), row.title(), hint)
            })
            .collect()
    }

    /// The overlay palette for the current scroll window.
    fn view(&self) -> terminus_ui::components::overlay::Palette {
        terminus_ui::palette_view::visible_palette(
            &self.query,
            self.placeholder(),
            &self.row_specs(),
            self.scroll_offset,
            self.selected_index,
            MAX_VISIBLE_RESULTS,
        )
    }

    /// Query to offer "Add server" for: nothing matched but something was typed.
    pub fn add_server_query(&self) -> Option<String> {
        let q = self.query.trim();
        (!q.is_empty() && self.filtered_rows().is_empty()).then(|| q.to_string())
    }

    /// Hit-test a click against the last painted frame (logical pixels).
    /// `Err(())`: outside the panel (close). `Ok(Some(i))`: result row `i`.
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        _window_width: f32,
        _scale_factor: f32,
    ) -> Result<Option<usize>, ()> {
        use terminus_ui::components::overlay::PaletteHit;
        let Some(layout) = self.last_layout.as_ref() else {
            return Ok(None);
        };
        match layout.hit_test(mouse_x, mouse_y) {
            PaletteHit::Outside => Err(()),
            PaletteHit::Item(rel) => {
                let index = self.scroll_offset + rel;
                Ok((index < self.filtered_rows().len()).then_some(index))
            }
            PaletteHit::AddServer | PaletteHit::Inside => Ok(None),
        }
    }

    /// Whether the click landed on the empty-state "Add server" line.
    pub fn add_server_hit(&self, mouse_x: f32, mouse_y: f32) -> bool {
        use terminus_ui::components::overlay::PaletteHit;
        self.add_server_query().is_some()
            && self
                .last_layout
                .as_ref()
                .is_some_and(|l| l.hit_test(mouse_x, mouse_y) == PaletteHit::AddServer)
    }

    /// Update selection based on mouse position. Returns true if selection changed.
    pub fn hover(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> bool {
        if let Ok(Some(index)) =
            self.hit_test(mouse_x, mouse_y, window_width, scale_factor)
        {
            if self.selected_index != index {
                self.selected_index = index;
                return true;
            }
        }
        false
    }

    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        theme: &terminus_ui::theme::ChromeTheme,
        dimensions: (f32, f32, f32),
    ) {
        use crate::renderer::components::overlay::paint_palette;
        use crate::renderer::ui_text::{measure_ui_text, UiWeight};
        use terminus_ui::components::overlay as ov;

        if !self.enabled {
            // Immediate mode: not drawing == not visible.
            self.last_layout = None;
            return;
        }

        let (window_width, window_height, scale_factor) = dimensions;
        let window = (window_width / scale_factor, window_height / scale_factor);
        let palette = self.view();
        let layout = palette.layout(window);

        sugarloaf.begin_overlay();
        crate::renderer::chrome::paint_flat(
            sugarloaf,
            &terminus_ui::Rect::new(0.0, 0.0, window.0, window.1),
            ov::SCRIM,
            0.08,
            30,
        );
        paint_palette(sugarloaf, theme, &palette, &layout);

        let elapsed_ms = self.caret_blink_start.elapsed().as_millis();
        if (elapsed_ms / CARET_BLINK_MS).is_multiple_of(2) {
            let q = &layout.query;
            let text_x = q.x + ov::PALETTE_QUERY_PAD_X + ov::PALETTE_QUERY_ICON + 12.0;
            let w = if self.query.is_empty() {
                0.0
            } else {
                measure_ui_text(sugarloaf, &self.query, 19.0, UiWeight::Regular)
            };
            let h = (19.0_f32 * 1.25).round();
            crate::renderer::chrome::paint_flat(
                sugarloaf,
                &terminus_ui::Rect::new(
                    text_x + w,
                    q.y + (q.height - h) / 2.0,
                    1.5,
                    h,
                ),
                theme.accent,
                0.16,
                30,
            );
        }
        sugarloaf.end_overlay();
        self.last_layout = Some(layout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_enabled_resets_state() {
        let mut palette = CommandPalette::new();
        palette.set_query("test".to_string());
        palette.selected_index = 3;
        palette.scroll_offset = 2;

        palette.set_enabled(true);

        assert!(palette.query.is_empty());
        assert_eq!(palette.selected_index, 0);
        assert_eq!(palette.scroll_offset, 0);
    }

    #[test]
    fn test_filtered_commands_empty_query() {
        let palette = CommandPalette::new();
        let filtered = palette.filtered_rows();
        // ToggleAppearanceTheme is hidden when has_adaptive_theme is false
        assert_eq!(filtered.len(), COMMANDS.len() - 1);
    }

    #[test]
    fn test_filtered_commands_by_title() {
        let mut palette = CommandPalette::new();
        palette.query = "split".to_string();
        let filtered = palette.filtered_rows();
        assert!(filtered.len() >= 2);
        for (_, row) in &filtered {
            assert!(row.title().to_lowercase().contains("split"));
        }
    }

    #[test]
    fn test_filtered_commands_case_insensitive() {
        let mut palette = CommandPalette::new();
        palette.query = "QUIT".to_string();
        let filtered = palette.filtered_rows();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.title(), "Quit");
    }

    #[test]
    fn test_fuzzy_matching() {
        let mut palette = CommandPalette::new();
        palette.query = "nt".to_string(); // Should match "New Tab", "Next Tab", etc.
        let filtered = palette.filtered_rows();
        assert!(!filtered.is_empty());
    }

    #[test]
    fn test_set_query_resets_selection_and_scroll() {
        let mut palette = CommandPalette::new();
        palette.selected_index = 5;
        palette.scroll_offset = 3;
        palette.set_query("test".to_string());
        assert_eq!(palette.selected_index, 0);
        assert_eq!(palette.scroll_offset, 0);
    }

    #[test]
    fn test_move_selection_down() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        assert_eq!(palette.selected_index, 0);
        palette.move_selection_down();
        assert_eq!(palette.selected_index, 1);
        palette.move_selection_down();
        assert_eq!(palette.selected_index, 2);
    }

    #[test]
    fn test_move_selection_down_boundary() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        let count = palette.filtered_rows().len();
        palette.selected_index = count - 1;
        palette.move_selection_down();
        assert_eq!(palette.selected_index, count - 1);
    }

    #[test]
    fn test_move_selection_up() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        palette.selected_index = 3;
        palette.move_selection_up();
        assert_eq!(palette.selected_index, 2);
    }

    #[test]
    fn test_move_selection_up_boundary() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        palette.move_selection_up();
        assert_eq!(palette.selected_index, 0);
    }

    #[test]
    fn test_get_selected_action() {
        let palette = CommandPalette::new();
        let action = palette.get_selected_action();
        assert!(action.is_some());
        // First command is "New Tab"
        assert_eq!(action.unwrap(), PaletteAction::TabCreate);
    }

    #[test]
    fn test_get_selected_action_with_filter() {
        let mut palette = CommandPalette::new();
        palette.set_query("quit".to_string());
        let action = palette.get_selected_action();
        assert_eq!(action, Some(PaletteAction::Quit));
    }

    #[test]
    fn test_scroll_offset_on_move_down() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        for _ in 0..MAX_VISIBLE_RESULTS {
            palette.move_selection_down();
        }
        assert!(palette.scroll_offset > 0);
    }

    #[test]
    fn test_hit_test_walks_the_painted_layout() {
        let mut palette = CommandPalette::new();
        // Nothing painted yet: nothing to hit.
        assert_eq!(palette.hit_test(0.0, 0.0, 1200.0, 1.0), Ok(None));
        palette.set_enabled(true);
        let layout = palette.view().layout((1440.0, 900.0));
        let second = layout
            .rows
            .iter()
            .filter_map(|r| match r {
                terminus_ui::components::overlay::PaletteRow::Item { rect, index } => {
                    Some((*rect, *index))
                }
                _ => None,
            })
            .nth(1)
            .unwrap();
        palette.last_layout = Some(layout);
        assert!(palette.hit_test(0.0, 0.0, 1440.0, 1.0).is_err(), "scrim closes");
        let (rect, _) = second;
        assert_eq!(palette.hit_test(rect.x + 4.0, rect.y + 4.0, 1440.0, 1.0), Ok(Some(1)));
        // Scrolled by 3: the same row is absolute index 4.
        palette.scroll_offset = 3;
        assert_eq!(palette.hit_test(rect.x + 4.0, rect.y + 4.0, 1440.0, 1.0), Ok(Some(4)));
    }

    #[test]
    fn test_fuzzy_score_basic() {
        assert!(fuzzy_score("nt", "New Tab").is_some());
        assert!(fuzzy_score("xyz", "New Tab").is_none());
        assert!(fuzzy_score("", "New Tab").is_some());
    }

    #[test]
    fn test_fuzzy_score_ordering() {
        // "New Tab" should score higher than "Next Tab" for "net" because of word boundary
        let score_new = fuzzy_score("net", "New Tab").unwrap_or(-100);
        let score_next = fuzzy_score("net", "Next Tab").unwrap_or(-100);
        // Both should match
        assert!(score_new > -100);
        assert!(score_next > -100);
    }

    #[test]
    fn enter_fonts_mode_switches_to_font_list() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        palette.set_query("ab".to_string());
        palette.selected_index = 2;

        let fonts = vec![
            "JetBrains Mono".to_string(),
            "Fira Code".to_string(),
            "Cascadia Code".to_string(),
        ];
        palette.enter_fonts_mode(fonts);

        // Query cleared, selection reset, full list visible.
        assert!(palette.query.is_empty());
        assert_eq!(palette.selected_index, 0);
        assert_eq!(palette.filtered_rows().len(), 3);
        // Every row is a Font row, so no executable action.
        assert!(palette.get_selected_action().is_none());
    }

    #[test]
    fn fonts_mode_filters_by_fuzzy_score() {
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec![
            "JetBrains Mono".to_string(),
            "Fira Code".to_string(),
            "Cascadia Code".to_string(),
        ]);
        palette.set_query("cas".to_string());
        let filtered = palette.filtered_rows();
        assert!(filtered.iter().any(|(_, r)| r.title() == "Cascadia Code"));
        assert!(filtered.iter().all(|(_, r)| {
            r.title().to_lowercase().contains('c')
                && r.title().to_lowercase().contains('a')
                && r.title().to_lowercase().contains('s')
        }));
    }

    #[test]
    fn fonts_mode_row_has_no_shortcut_column() {
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec!["Fira Code".to_string()]);
        let filtered = palette.filtered_rows();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.shortcut(), "");
    }

    #[test]
    fn set_enabled_resets_fonts_mode_to_commands() {
        // Re-opening the palette with the keyboard must drop any stale
        // font list — reopening otherwise would land the user on fonts
        // they saw yesterday, which is surprising.
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec!["Fira Code".to_string()]);
        palette.enabled = true;
        palette.set_enabled(false);
        palette.set_enabled(true);
        assert!(matches!(palette.mode, PaletteMode::Commands));
        // Commands list is back (non-empty modulo adaptive-theme filter).
        assert!(!palette.filtered_rows().is_empty());
    }

    #[test]
    fn get_selected_font_returns_family_in_fonts_mode() {
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec![
            "JetBrains Mono".to_string(),
            "Fira Code".to_string(),
        ]);
        // First row (sorted alphabetically by fuzzy_score tie-break:
        // both score 0 with empty query, so first-inserted wins).
        let selected = palette.get_selected_font();
        assert!(selected.is_some());
        // The returned name must be one of the inputs, irrespective
        // of fuzzy-sort ordering.
        let s = selected.unwrap();
        assert!(s == "JetBrains Mono" || s == "Fira Code");
    }

    #[test]
    fn get_selected_font_none_in_commands_mode() {
        let palette = CommandPalette::new();
        // Default mode is Commands; no font to copy.
        assert!(palette.get_selected_font().is_none());
    }

    #[test]
    fn get_selected_font_none_when_empty_filter() {
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec!["Fira Code".to_string()]);
        palette.set_query("zzzz".to_string());
        // Query doesn't match anything → no selected font.
        assert!(palette.get_selected_font().is_none());
    }

    fn sample_hosts() -> Vec<HostPaletteItem> {
        vec![
            HostPaletteItem {
                id: "h1".into(),
                title: "Production".into(),
                subtitle: "root@prod.example".into(),
            },
            HostPaletteItem {
                id: "h2".into(),
                title: "Staging".into(),
                subtitle: "deploy@staging.example:2222".into(),
            },
        ]
    }

    #[test]
    fn sftp_picker_lists_hosts_and_remembers_why() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        palette.enter_hosts_mode_for(sample_hosts(), HostPick::Sftp);
        assert_eq!(palette.host_pick(), HostPick::Sftp);
        assert_eq!(palette.filtered_rows().len(), 2);
        palette.set_query("stag".to_string());
        assert_eq!(palette.get_selected_host_id().as_deref(), Some("h2"));
        // The plain "Open Host…" picker still opens sessions.
        palette.enter_hosts_mode(sample_hosts());
        assert_eq!(palette.host_pick(), HostPick::Session);
    }

    #[test]
    fn enter_hosts_mode_lists_all_hosts() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        palette.enter_hosts_mode(sample_hosts());
        assert!(palette.query.is_empty());
        assert_eq!(palette.filtered_rows().len(), 2);
        assert!(palette.get_selected_action().is_none());
        assert!(palette.get_selected_host_id().is_some());
    }

    #[test]
    fn hosts_mode_filters_by_name_or_endpoint() {
        let mut palette = CommandPalette::new();
        palette.enter_hosts_mode(sample_hosts());
        palette.set_query("stag".to_string());
        let filtered = palette.filtered_rows();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.title(), "Staging");
        assert_eq!(palette.get_selected_host_id().as_deref(), Some("h2"));
    }

    #[test]
    fn commands_mode_surfaces_hosts_only_when_querying() {
        let mut palette = CommandPalette::new();
        palette.set_hosts(sample_hosts());
        // Empty query: commands only (no host dump).
        let empty = palette.filtered_rows();
        assert!(empty
            .iter()
            .all(|(_, r)| !matches!(r, PaletteRow::Host { .. })));
        assert!(empty.iter().any(|(_, r)| r.title() == "Open Host…"));

        palette.set_query("prod".to_string());
        let mixed = palette.filtered_rows();
        assert!(
            mixed.iter().any(|(_, r)| matches!(
                r,
                PaletteRow::Host {
                    title: "Production",
                    ..
                }
            )),
            "expected Production host in mixed results: {:?}",
            mixed.iter().map(|(_, r)| r.title()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn set_enabled_resets_hosts_mode_to_commands() {
        let mut palette = CommandPalette::new();
        palette.enter_hosts_mode(sample_hosts());
        palette.enabled = true;
        palette.set_enabled(false);
        palette.set_enabled(true);
        assert!(matches!(palette.mode, PaletteMode::Commands));
    }

    // Scrollbar geometry + fade math live in `renderer::scrollbar` and
    // are tested there. The tests below cover the palette's own contract:
    // the scrollbar only surfaces after the user actually scrolls, and
    // resets when the list reshapes.

    #[test]
    fn scrollbar_hidden_until_first_scroll() {
        // Long list, palette just opened — no scroll event has happened,
        // so the fade timer is `None` and the scrollbar stays invisible
        // despite the list being taller than the visible window.
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode((0..50).map(|i| format!("Family {i:02}")).collect());
        assert!(palette.last_scroll_time.is_none());
    }

    #[test]
    fn scrollbar_triggered_only_when_offset_actually_changes() {
        // The first few `move_selection_down` calls don't change
        // `scroll_offset` (selection walks within the visible window).
        // Only when selection crosses the window boundary does
        // `scroll_offset` bump, and only then does the scrollbar wake.
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode((0..50).map(|i| format!("Family {i:02}")).collect());
        for _ in 0..MAX_VISIBLE_RESULTS {
            palette.move_selection_down();
        }
        // At this point selection has just crossed into scroll territory.
        assert!(palette.last_scroll_time.is_some());
    }

    #[test]
    fn scrollbar_timer_reset_on_query_change() {
        // Typing re-filters the list, which can shrink it below the
        // visible window. Any stale scrollbar timer must clear so a
        // leftover thumb doesn't linger over the new short list.
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode((0..50).map(|i| format!("Family {i:02}")).collect());
        for _ in 0..MAX_VISIBLE_RESULTS {
            palette.move_selection_down();
        }
        assert!(palette.last_scroll_time.is_some());
        palette.set_query("Family 00".to_string());
        assert!(palette.last_scroll_time.is_none());
    }

    #[test]
    fn scrollbar_timer_reset_on_palette_reopen() {
        // Closing and re-opening the palette must drop any lingering
        // scrollbar state so the user doesn't see a fading thumb on a
        // fresh palette.
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode((0..50).map(|i| format!("Family {i:02}")).collect());
        for _ in 0..MAX_VISIBLE_RESULTS {
            palette.move_selection_down();
        }
        palette.set_enabled(false);
        palette.set_enabled(true);
        assert!(palette.last_scroll_time.is_none());
    }

    #[test]
    fn list_fonts_command_is_present_and_actionable() {
        // Confirms `List Fonts` shows up in the command list and
        // reports the correct action when selected.
        let mut palette = CommandPalette::new();
        palette.set_query("list fonts".to_string());
        let filtered = palette.filtered_rows();
        assert!(!filtered.is_empty());
        assert_eq!(filtered[0].1.title(), "List Fonts");
        palette.selected_index = 0;
        assert_eq!(
            palette.get_selected_action(),
            Some(PaletteAction::ListFonts)
        );
    }

    #[test]
    fn sftp_action_listed() {
        let mut palette = CommandPalette::new();
        palette.set_query("sftp".to_string());
        let filtered = palette.filtered_rows();
        let found = filtered.iter().any(|(_, row)| {
            matches!(row.action(), Some(PaletteAction::OpenSftp))
                || row.title().to_lowercase().contains("sftp")
        });
        assert!(found, "SFTP: Palette >sftp — not implemented yet");
    }

    #[test]
    fn shortcuts_shown_are_the_live_bindings() {
        let mut palette = CommandPalette::new();
        palette.set_shortcuts(vec![(PaletteAction::TabCreate, "Ctrl+Shift+T".into())]);
        palette.set_query("new tab".to_string());
        let rows = palette.filtered_rows();
        assert_eq!(rows[0].1.title(), "New Tab");
        assert_eq!(rows[0].1.shortcut(), "Ctrl+Shift+T");
        palette.set_query("split right".to_string());
        assert_eq!(
            palette.filtered_rows()[0].1.shortcut(),
            "",
            "unbound: no guess"
        );
    }

    #[test]
    fn commands_map_to_their_key_binding_actions() {
        use crate::bindings::Action;
        assert_eq!(
            PaletteAction::TabCreate.binding_action(),
            Some(Action::TabCreateNew)
        );
        assert_eq!(PaletteAction::Copy.binding_action(), Some(Action::Copy));
        assert_eq!(
            PaletteAction::CloseCurrentSplitOrTab.binding_action(),
            Some(Action::CloseCurrentSplitOrTab)
        );
        assert_eq!(PaletteAction::ListHosts.binding_action(), None);
    }

    #[test]
    fn update_commands_are_listed() {
        for (query, action) in [
            ("check for updates", PaletteAction::CheckForUpdates),
            ("install update", PaletteAction::InstallUpdate),
            ("restart to update", PaletteAction::RestartToUpdate),
        ] {
            let mut palette = CommandPalette::new();
            palette.set_query(query.to_string());
            assert_eq!(palette.get_selected_action(), Some(action), "{query}");
        }
    }

    fn host(id: &str, title: &str) -> HostPaletteItem {
        HostPaletteItem {
            id: id.into(),
            title: title.into(),
            subtitle: format!("root@{title}"),
        }
    }

    #[test]
    fn servers_list_after_commands_so_each_group_is_contiguous() {
        let mut palette = CommandPalette::new();
        palette.set_hosts(vec![host("1", "tab-server")]);
        palette.query = "tab".to_string();
        let specs = palette.row_specs();
        let first_server = specs.iter().position(|r| r.group == "Servers").unwrap();
        assert!(first_server > 0);
        assert!(specs[..first_server].iter().all(|r| r.group == "Commands"));
        assert!(specs[first_server..].iter().all(|r| r.group == "Servers"));
        assert_eq!(specs[first_server].hint, "root@tab-server");
    }

    #[test]
    fn view_windows_the_rows_and_names_the_placeholder() {
        let mut palette = CommandPalette::new();
        palette.set_enabled(true);
        for _ in 0..(MAX_VISIBLE_RESULTS + 2) {
            palette.move_selection_down();
        }
        let view = palette.view();
        assert_eq!(view.item_count(), MAX_VISIBLE_RESULTS);
        assert_eq!(view.selected, MAX_VISIBLE_RESULTS - 1);
        assert_eq!(view.placeholder.as_deref(), Some("Search servers and commands"));
    }

    #[test]
    fn nothing_matching_offers_add_server_with_the_query() {
        let mut palette = CommandPalette::new();
        assert_eq!(palette.add_server_query(), None, "empty query never offers it");
        palette.set_query("zzzqqq".to_string());
        assert_eq!(palette.add_server_query().as_deref(), Some("zzzqqq"));
        palette.set_query("quit".to_string());
        assert_eq!(palette.add_server_query(), None, "there are matches");
    }

    #[test]
    fn font_rows_hint_copy() {
        let mut palette = CommandPalette::new();
        palette.enter_fonts_mode(vec!["Fira Code".into()]);
        let specs = palette.row_specs();
        assert_eq!((specs[0].group, specs[0].hint.as_str()), ("Fonts", "Copy"));
    }
}
