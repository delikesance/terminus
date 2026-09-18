//! Sidebar: the host list, its scroll viewport and the add-host row.
//!
//! Geometry lives here so the painter and the mouse agree by
//! construction: both walk the same `*_rect` methods, and the wheel
//! clamp used by the hit-tests is the one the painter clips with.
//!
//! The list is a `Vec<Row>` rather than a `Vec<HostItem>` because the
//! panel mixes hosts with section labels and collapsible groups. A
//! section label is a row like any other, so the painter and the
//! hit-test still walk one sequence — the only difference is that its
//! height comes from [`SECTION_HEIGHT`].

use std::collections::HashSet;

use crate::geom::Rect;
use crate::icons::Icon;
use crate::os_icons::HostStatus;

/// Panel width, in logical pixels (`w-72` = 288).
pub const WIDTH: f32 = 288.0;
/// Title band ("SERVERS & HOSTS") — mock `p-4` (~16px) with text-xs.
pub const TITLE_HEIGHT: f32 = 44.0;
/// Search field band under the title — mock `p-3` band.
pub const SEARCH_BAND_HEIGHT: f32 = 52.0;
/// Combined header (title + search) for geometry that still expects one band.
pub const HEADER_HEIGHT: f32 = TITLE_HEIGHT + SEARCH_BAND_HEIGHT;
/// Painted height of one host card (compact two-line + soft pad).
pub const ITEM_HEIGHT: f32 = 56.0;
/// Vertical gap between floating host cards (breathing, not cramped).
pub const CARD_GAP: f32 = 10.0;
/// Vertical gap after a section label before the first row under it.
pub const SECTION_GAP: f32 = 10.0;
/// Height of a section label row ("Local" / "Hosts").
pub const SECTION_HEIGHT: f32 = 22.0;
/// Compact open-terminal row under a host.
pub const SESSION_HEIGHT: f32 = 28.0;
/// Gap between sibling session rows under the same host.
pub const SESSION_GAP: f32 = 6.0;
/// Extra air after the last session before the next host/group/section.
pub const SESSION_AFTER_GAP: f32 = 18.0;
/// Indent for session rows under a host (nest cue, not a second gutter).
pub const SESSION_INDENT: f32 = 16.0;
/// Session leaf corner radius — soft, not a capsule.
pub const SESSION_RADIUS: f32 = 6.0;
/// Reserved for callers that still key off a leading accent width.
pub const SESSION_ACCENT: f32 = 0.0;
/// Pad from the session leaf's left edge to the terminal icon.
pub const SESSION_CONTENT_PAD: f32 = 10.0;
/// Diameter of the luminous status dot on a host badge.
pub const STATUS_DOT: f32 = 6.0;
/// Side of the session close × hit box.
pub const SESSION_CLOSE_HIT: f32 = 20.0;
/// Side of the host "+" add-session hit box.
pub const HOST_ADD_HIT: f32 = 20.0;
/// Side of the expand/collapse chevron hit box.
pub const CHEVRON_HIT: f32 = 18.0;
/// Height of the dashed New Host CTA at the top of the list.
pub const CTA_HEIGHT: f32 = 60.0;
/// Breathing room above the New Host CTA (under the search band).
pub const CTA_TOP_GAP: f32 = 12.0;
/// Gap under the New Host CTA before the first section.
pub const CTA_GAP: f32 = 8.0;
/// Footer reserved height (mock has no footer).
pub const FOOTER_HEIGHT: f32 = 0.0;
/// Inline "New group" form height under the Hosts section header.
pub const NEW_GROUP_FORM_HEIGHT: f32 = 40.0;
/// Approximate width of the "+ New group" text button on the Hosts header.
pub const NEW_GROUP_BUTTON_WIDTH: f32 = 88.0;
/// Alias kept for callers that shared the dual-action header width.
pub const HOSTS_HEADER_ACTION_WIDTH: f32 = NEW_GROUP_BUTTON_WIDTH;
/// Sticky success notice height at the bottom of the panel.
pub const NOTICE_HEIGHT: f32 = 40.0;
/// Sticky error banner height (two lines of wrapped text).
pub const ERROR_BANNER_HEIGHT: f32 = 64.0;
/// Inset of the sticky notice/error card from the panel edges.
pub const NOTICE_MARGIN: f32 = 10.0;
/// Horizontal inset of cards inside the drawer.
pub const PAD_X: f32 = 10.0;
/// Inner padding inside a card (icon/text inset).
pub const CARD_PAD: f32 = 12.0;
/// Search field inset inside the search band.
pub const SEARCH_INSET_X: f32 = 12.0;
/// Search field height inside the search band.
pub const SEARCH_HEIGHT: f32 = 34.0;
/// Side of the OS glyph inside the badge tile.
pub const ICON_SIZE: f32 = 16.0;
/// Outer badge tile for the New Host CTA (`w-8 h-8`).
pub const BADGE_TILE: f32 = 32.0;
/// Outer badge tile for host cards (`p-1.5` around a 16px glyph ≈ 28).
pub const HOST_BADGE_TILE: f32 = 28.0;
/// Gap between that badge tile and the row's text.
pub const ICON_GAP: f32 = 10.0;
/// Side of the connecting-orbit slot on the trailing edge of a row.
pub const CONNECTING_SLOT: f32 = 18.0;
/// Rows advanced by one wheel notch.
pub const WHEEL_ROWS: f32 = 3.0;
/// Card corner radius — soft, not bubble.
pub const CARD_RADIUS: f32 = 8.0;
/// Left inset of the panel, i.e. the width of the rail beside it.
pub const ORIGIN_X: f32 = crate::activity_bar::WIDTH;

/// What a row opens, and therefore which glyph it carries.
///
/// The badge is the panel's whole visual taxonomy: it is what tells a
/// local shell apart from a WSL distro apart from an SSH host without
/// reading the subtitle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    /// The machine terminus runs on: a local shell.
    Local,
    /// A WSL distro of the Windows machine this one is nested in.
    Wsl,
    /// A stored SSH host.
    Ssh,
}

impl Badge {
    pub fn icon(self) -> Icon {
        match self {
            Badge::Local => Icon::Monitor,
            Badge::Wsl => Icon::SquareTerminal,
            Badge::Ssh => Icon::Server,
        }
    }
}

/// One host, as the list draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostItem {
    /// Echoed back by [`PanelHit::Item`]'s row, and what the frontend
    /// resolves a session from: `local`, `wsl:<distro>` or a host id.
    pub id: String,
    pub name: String,
    /// `user@host:port`, or the distro's state, already formatted.
    pub endpoint: String,
    pub badge: Badge,
    /// Whether the row came from the host store.
    ///
    /// The platform rows — this computer and the Windows distros — are
    /// always there because they are where terminus runs, so they are
    /// listed without being counted: the header's number has to agree with
    /// the `Hosts` section, which is the list the user edits.
    pub stored: bool,
    /// Stable OS key for brand glyphs (`nixos`, `ubuntu`, …).
    pub os_id: Option<String>,
    /// Live status dot (running WSL, active SSH tab, …).
    pub status: HostStatus,
    /// Indented under a collapsible group header.
    pub nested: bool,
    /// Open terminal sessions currently attached to this host.
    pub session_count: usize,
}

/// One open terminal session, as the list draws it under its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionItem {
    /// Index into the window's tab/`ContextManager` list.
    pub tab_index: usize,
    /// Parent host id (`local`, `wsl:…`, ssh id).
    pub host_id: String,
    pub title: String,
    /// Whether this is the focused terminal.
    pub active: bool,
    /// Whether the row may show a close affordance (not the pinned home).
    pub closable: bool,
}

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A section label. Not selectable, not clickable.
    Section(String),
    /// A collapsible host group from the store.
    Group {
        id: String,
        name: String,
        host_count: usize,
        /// Open sessions across all hosts in this group.
        session_count: usize,
        collapsed: bool,
    },
    Host(HostItem),
    /// Open terminal under a host (always follows its parent host row).
    Session(SessionItem),
}

impl Row {
    pub fn height(&self) -> f32 {
        match self {
            // Label row + breathing room before the first card under it.
            Row::Section(_) => SECTION_HEIGHT + SECTION_GAP,
            // Group headers share the host-card footprint (Apple HIG cards).
            Row::Group { .. } | Row::Host(_) => ITEM_HEIGHT + CARD_GAP,
            // Mid-sibling default; [`HostPanel::row_slot_height`] widens after
            // the last session under a host.
            Row::Session(_) => SESSION_HEIGHT + SESSION_GAP,
        }
    }

    /// Painted card height inside the row slot (excludes the trailing gap).
    pub fn card_height(&self) -> f32 {
        match self {
            Row::Section(_) => SECTION_HEIGHT,
            Row::Group { .. } | Row::Host(_) => ITEM_HEIGHT,
            Row::Session(_) => SESSION_HEIGHT,
        }
    }

    pub fn host(&self) -> Option<&HostItem> {
        match self {
            Row::Host(item) => Some(item),
            Row::Section(_) | Row::Group { .. } | Row::Session(_) => None,
        }
    }

    pub fn session(&self) -> Option<&SessionItem> {
        match self {
            Row::Session(item) => Some(item),
            _ => None,
        }
    }

    /// The section label, if this row is one.
    pub fn label(&self) -> Option<&str> {
        match self {
            Row::Section(label) => Some(label),
            Row::Group { .. } | Row::Host(_) | Row::Session(_) => None,
        }
    }

    /// Group metadata when this row is a collapsible group header.
    pub fn group(&self) -> Option<(&str, usize, bool)> {
        match self {
            Row::Group {
                name,
                host_count,
                collapsed,
                ..
            } => Some((name.as_str(), *host_count, *collapsed)),
            _ => None,
        }
    }
}

/// Where a dragged sidebar item would land on release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostDropTarget {
    /// Drop a host into this group id (membership).
    Group(String),
    /// Append at the end of the root Hosts list (and ungroup if needed).
    Ungroup,
    /// Insert among root items immediately before this host id.
    BeforeHost(String),
    /// Insert among root items immediately before this group id.
    BeforeGroup(String),
}

/// What is being dragged in the hosts panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostDragKind {
    Host,
    Group,
}

/// Lifecycle of a host/group drag.
#[derive(Debug, Clone, PartialEq)]
pub enum HostDragPhase {
    /// Pointer down, not yet past the move threshold.
    Armed,
    /// Ghost follows the cursor.
    Dragging,
    /// Ghost is tweening into the drop slot; persist on finish.
    Snapping {
        tween: crate::anim::RectTween,
        pending: HostDropTarget,
    },
}

/// In-progress drag of a stored host or a group.
#[derive(Debug, Clone, PartialEq)]
pub struct HostDrag {
    /// Host id or group id, depending on [`Self::kind`].
    pub host_id: String,
    pub host_name: String,
    pub endpoint: String,
    pub kind: HostDragKind,
    pub row_index: usize,
    pub press_x: f32,
    pub press_y: f32,
    pub current_x: f32,
    pub current_y: f32,
    /// Grab offset inside the source card (cursor − card origin).
    pub grab_dx: f32,
    pub grab_dy: f32,
    /// Source card geometry at press time.
    pub source_rect: crate::geom::Rect,
    /// Current phantom card rect (cursor-follow or snap tween).
    pub ghost_rect: crate::geom::Rect,
    pub phase: HostDragPhase,
    pub drop_target: Option<HostDropTarget>,
}

impl HostDrag {
    pub fn is_group(&self) -> bool {
        matches!(self.kind, HostDragKind::Group)
    }

    /// True once the ghost is visible (dragging or snapping).
    pub fn ghost_visible(&self) -> bool {
        matches!(
            self.phase,
            HostDragPhase::Dragging | HostDragPhase::Snapping { .. }
        )
    }

    pub fn is_snapping(&self) -> bool {
        matches!(self.phase, HostDragPhase::Snapping { .. })
    }

    /// Legacy alias: past threshold or snapping.
    pub fn started(&self) -> bool {
        !matches!(self.phase, HostDragPhase::Armed)
    }
}

/// Pixels of movement before a press becomes a drag.
pub const HOST_DRAG_THRESHOLD: f32 = 5.0;

/// What a press inside the panel landed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelHit {
    /// The add-host control in the Hosts section header.
    AddHost,
    /// The new-group control beside the Hosts section label.
    NewGroup,
    /// Confirm the inline new-group form.
    NewGroupCreate,
    /// Cancel the inline new-group form.
    NewGroupCancel,
    /// The search field in the header.
    Search,
    /// The new-group name field (focus for typing).
    NewGroupField,
    /// A host row, by index into [`HostPanel::rows`].
    Item(usize),
    /// Expand/collapse chevron on a host that has sessions.
    ToggleHost(usize),
    /// "+" on a host row — open another session for that host.
    HostAddSession(usize),
    /// A session row (focus that terminal).
    Session(usize),
    /// Close × on a session row.
    CloseSession(usize),
    /// A group header row, by index into [`HostPanel::rows`].
    Group(usize),
    /// The panel's own background.
    Background,
}

/// Inline rename of a host or group from the context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameDraft {
    pub id: String,
    pub is_group: bool,
    pub name: String,
    /// Character index of the caret inside [`Self::name`].
    pub caret: usize,
    /// When set, selection spans `[min(anchor, caret), max(anchor, caret))`.
    pub sel_anchor: Option<usize>,
    pub focused: bool,
}

/// How a caret move should treat an existing selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameMoveKind {
    /// Collapse selection (if any) then move the caret.
    Collapse,
    /// Extend or start a selection (Shift held).
    Extend,
}

impl RenameDraft {
    /// Inclusive-exclusive character range of the selection, if any.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.sel_anchor?;
        let (a, b) = if anchor <= self.caret {
            (anchor, self.caret)
        } else {
            (self.caret, anchor)
        };
        (a < b).then_some((a, b))
    }

    pub fn clear_selection(&mut self) {
        self.sel_anchor = None;
    }

    pub fn select_all(&mut self) -> bool {
        let end = self.name.chars().count();
        if end == 0 {
            return false;
        }
        self.sel_anchor = Some(0);
        self.caret = end;
        true
    }

    /// Delete the selected range, if any. Returns whether anything changed.
    pub fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };
        let from = rename_char_byte(&self.name, start);
        let to = rename_char_byte(&self.name, end);
        self.name.replace_range(from..to, "");
        self.caret = start;
        self.sel_anchor = None;
        true
    }

    /// Insert printable text at the caret (replacing the selection).
    pub fn insert(&mut self, text: &str, max_bytes: usize) -> bool {
        if text.is_empty() || text.chars().any(char::is_control) {
            return false;
        }
        self.delete_selection();
        if self.name.len() + text.len() > max_bytes {
            return false;
        }
        let byte = rename_char_byte(&self.name, self.caret);
        self.name.insert_str(byte, text);
        self.caret += text.chars().count();
        self.sel_anchor = None;
        true
    }

    /// Delete before the caret, or the selection, or the previous word.
    pub fn backspace(&mut self, by_word: bool) -> bool {
        if self.delete_selection() {
            return true;
        }
        if by_word {
            let start = word_boundary_left(&self.name, self.caret);
            if start == self.caret {
                return false;
            }
            let from = rename_char_byte(&self.name, start);
            let to = rename_char_byte(&self.name, self.caret);
            self.name.replace_range(from..to, "");
            self.caret = start;
            return true;
        }
        if self.caret == 0 {
            return false;
        }
        let start = rename_char_byte(&self.name, self.caret - 1);
        let end = rename_char_byte(&self.name, self.caret);
        self.name.replace_range(start..end, "");
        self.caret -= 1;
        true
    }

    /// Delete after the caret, or the selection, or the next word.
    pub fn delete_forward(&mut self, by_word: bool) -> bool {
        if self.delete_selection() {
            return true;
        }
        let len = self.name.chars().count();
        if self.caret >= len {
            return false;
        }
        if by_word {
            let end = word_boundary_right(&self.name, self.caret);
            if end == self.caret {
                return false;
            }
            let from = rename_char_byte(&self.name, self.caret);
            let to = rename_char_byte(&self.name, end);
            self.name.replace_range(from..to, "");
            return true;
        }
        let start = rename_char_byte(&self.name, self.caret);
        let end = rename_char_byte(&self.name, self.caret + 1);
        self.name.replace_range(start..end, "");
        true
    }

    fn prepare_move(&mut self, kind: RenameMoveKind) {
        match kind {
            RenameMoveKind::Collapse => {
                if let Some((start, end)) = self.selection_range() {
                    // Bare arrow collapses to the edge in the move direction
                    // after the caller adjusts caret — clear here first.
                    let _ = (start, end);
                }
                self.sel_anchor = None;
            }
            RenameMoveKind::Extend => {
                if self.sel_anchor.is_none() {
                    self.sel_anchor = Some(self.caret);
                }
            }
        }
    }

    pub fn move_left(&mut self, kind: RenameMoveKind, by_word: bool) -> bool {
        if kind == RenameMoveKind::Collapse {
            if let Some((start, _)) = self.selection_range() {
                self.caret = start;
                self.sel_anchor = None;
                return true;
            }
        }
        self.prepare_move(kind);
        let next = if by_word {
            word_boundary_left(&self.name, self.caret)
        } else if self.caret == 0 {
            self.caret
        } else {
            self.caret - 1
        };
        if next == self.caret && kind != RenameMoveKind::Extend {
            return false;
        }
        let changed = next != self.caret || self.selection_range().is_some();
        self.caret = next;
        if kind == RenameMoveKind::Collapse {
            self.sel_anchor = None;
        }
        changed || kind == RenameMoveKind::Extend
    }

    pub fn move_right(&mut self, kind: RenameMoveKind, by_word: bool) -> bool {
        if kind == RenameMoveKind::Collapse {
            if let Some((_, end)) = self.selection_range() {
                self.caret = end;
                self.sel_anchor = None;
                return true;
            }
        }
        self.prepare_move(kind);
        let len = self.name.chars().count();
        let next = if by_word {
            word_boundary_right(&self.name, self.caret)
        } else if self.caret >= len {
            self.caret
        } else {
            self.caret + 1
        };
        let changed = next != self.caret;
        self.caret = next;
        if kind == RenameMoveKind::Collapse {
            self.sel_anchor = None;
        }
        changed || kind == RenameMoveKind::Extend
    }

    pub fn move_home(&mut self, kind: RenameMoveKind) -> bool {
        self.prepare_move(kind);
        if self.caret == 0 && kind == RenameMoveKind::Collapse {
            return false;
        }
        self.caret = 0;
        if kind == RenameMoveKind::Collapse {
            self.sel_anchor = None;
        }
        true
    }

    pub fn move_end(&mut self, kind: RenameMoveKind) -> bool {
        self.prepare_move(kind);
        let end = self.name.chars().count();
        if self.caret == end && kind == RenameMoveKind::Collapse {
            return false;
        }
        self.caret = end;
        if kind == RenameMoveKind::Collapse {
            self.sel_anchor = None;
        }
        true
    }

    /// Text before the caret, for painting the caret inline.
    pub fn prefix(&self) -> String {
        self.name.chars().take(self.caret).collect()
    }

    /// Text before the selection start (or caret if none).
    pub fn before_selection(&self) -> String {
        let start = self.selection_range().map(|(s, _)| s).unwrap_or(self.caret);
        self.name.chars().take(start).collect()
    }

    /// Selected text, empty when nothing is selected.
    pub fn selected_text(&self) -> String {
        let Some((start, end)) = self.selection_range() else {
            return String::new();
        };
        self.name.chars().skip(start).take(end - start).collect()
    }
}

fn rename_char_byte(value: &str, chars: usize) -> usize {
    value
        .char_indices()
        .nth(chars)
        .map(|(byte, _)| byte)
        .unwrap_or(value.len())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

fn word_boundary_left(value: &str, caret: usize) -> usize {
    let chars: Vec<char> = value.chars().collect();
    if caret == 0 || chars.is_empty() {
        return 0;
    }
    let mut i = caret.min(chars.len());
    // Skip trailing whitespace left of caret.
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let word = is_word_char(chars[i - 1]);
    while i > 0 && is_word_char(chars[i - 1]) == word && !chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i
}

fn word_boundary_right(value: &str, caret: usize) -> usize {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    if caret >= len {
        return len;
    }
    let mut i = caret;
    // Skip whitespace under/after caret.
    while i < len && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= len {
        return len;
    }
    let word = is_word_char(chars[i]);
    while i < len && is_word_char(chars[i]) == word && !chars[i].is_whitespace() {
        i += 1;
    }
    i
}

/// Sidebar state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostPanel {
    pub rows: Vec<Row>,
    /// Scroll offset in logical pixels, always within
    /// `0..=max_scroll`.
    pub scroll: f32,
    pub hover: Option<usize>,
    /// Whether the pointer is over the add-host header action.
    pub add_hover: bool,
    /// Filter text for the host list search field.
    pub filter: String,
    /// Whether the search field has keyboard focus.
    pub filter_focused: bool,
    /// Whether the pointer is over the new-group header button.
    pub new_group_hover: bool,
    /// Inline form under Hosts for naming a new group.
    pub new_group_drafting: bool,
    /// Draft name while [`Self::new_group_drafting`] is true.
    pub new_group_name: String,
    /// Whether the new-group name field has keyboard focus.
    pub new_group_focused: bool,
    /// Context-menu rename of a host or group row.
    pub rename: Option<RenameDraft>,
    /// Group ids whose child hosts are hidden in the list.
    pub collapsed_groups: HashSet<String>,
    /// Host ids whose open sessions are hidden under the host row.
    pub collapsed_hosts: HashSet<String>,
    /// Selected host row index (legacy highlight); prefer session selection.
    pub selected: Option<usize>,
    /// Active session tab index, when known.
    pub selected_session: Option<usize>,
    /// Host id whose session is currently starting (`wsl:…`, ssh id, …).
    pub connecting_id: Option<String>,
    /// Transient success message ("Added web-01"), cleared by the caller.
    pub notice: Option<String>,
    pub error: Option<String>,
    /// Armed / active host→group drag, when the pointer is down on a stored host.
    pub host_drag: Option<HostDrag>,
}

impl HostPanel {
    /// The panel's own box.
    pub fn rect(&self, origin_y: f32, height: f32) -> Rect {
        Rect::new(ORIGIN_X, origin_y, WIDTH, height)
    }

    pub fn header_rect(&self, origin_y: f32) -> Rect {
        Rect::new(ORIGIN_X, origin_y, WIDTH, TITLE_HEIGHT)
    }

    /// Title-only band ("SERVERS & HOSTS").
    pub fn title_rect(&self, origin_y: f32) -> Rect {
        self.header_rect(origin_y)
    }

    /// Search band under the title.
    pub fn search_band_rect(&self, origin_y: f32) -> Rect {
        Rect::new(ORIGIN_X, origin_y + TITLE_HEIGHT, WIDTH, SEARCH_BAND_HEIGHT)
    }

    /// The search field inset inside the search band.
    pub fn search_rect(&self, origin_y: f32) -> Rect {
        let band = self.search_band_rect(origin_y);
        Rect::new(
            band.x + SEARCH_INSET_X,
            band.y + (SEARCH_BAND_HEIGHT - SEARCH_HEIGHT) / 2.0,
            band.width - 2.0 * SEARCH_INSET_X,
            SEARCH_HEIGHT,
        )
    }

    /// Height reserved for a sticky notice/error card, including margin.
    pub fn notice_reserve(&self) -> f32 {
        if self.error.is_some() {
            ERROR_BANNER_HEIGHT + NOTICE_MARGIN
        } else if self.notice.is_some() {
            NOTICE_HEIGHT + NOTICE_MARGIN
        } else {
            0.0
        }
    }

    /// Sticky notice/error card anchored to the bottom of the panel.
    pub fn notice_rect(&self, origin_y: f32, height: f32) -> Option<Rect> {
        let message = self.error.as_ref().or(self.notice.as_ref())?;
        debug_assert!(!message.is_empty());
        let banner_h = if self.error.is_some() {
            ERROR_BANNER_HEIGHT
        } else {
            NOTICE_HEIGHT
        };
        // Never climb into the sticky header when the panel is shorter than
        // header + notice (tiny windows / tests).
        let y = (origin_y + height - NOTICE_MARGIN - banner_h).max(self.content_top(origin_y));
        Some(Rect::new(
            ORIGIN_X + NOTICE_MARGIN,
            y,
            WIDTH - 2.0 * NOTICE_MARGIN,
            banner_h,
        ))
    }

    /// The scroll viewport: everything between the header and the sticky
    /// notice/error card (or the bottom of the panel when none is showing).
    pub fn body_rect(&self, origin_y: f32, height: f32) -> Rect {
        let top = self.content_top(origin_y);
        let bottom = self.footer_rect(origin_y, height).y.max(top);
        Rect::new(ORIGIN_X, top, WIDTH, bottom - top)
    }

    pub fn footer_rect(&self, origin_y: f32, height: f32) -> Rect {
        let reserve = FOOTER_HEIGHT + self.notice_reserve();
        let top = (origin_y + height - reserve).max(self.content_top(origin_y));
        Rect::new(ORIGIN_X, top, WIDTH, (origin_y + height - top).max(0.0))
    }

    /// Dashed New Host CTA at the top of the scrollable content.
    pub fn add_button_rect(&self, origin_y: f32, _height: f32) -> Rect {
        Rect::new(
            ORIGIN_X + PAD_X,
            self.content_top(origin_y) + CTA_TOP_GAP - self.scroll,
            WIDTH - 2.0 * PAD_X,
            CTA_HEIGHT,
        )
    }

    /// Index of the `Hosts` section row, if present.
    pub fn hosts_section_index(&self) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.label() == Some("Hosts"))
    }

    /// Retired dual-header "+ New host" — use [`Self::add_button_rect`].
    pub fn new_host_button_rect(&self, _origin_y: f32, _height: f32) -> Rect {
        Rect::new(0.0, 0.0, 0.0, 0.0)
    }

    /// "+ New group" control on the right of the Hosts section label.
    pub fn new_group_button_rect(&self, origin_y: f32, _height: f32) -> Rect {
        let Some(index) = self.hosts_section_index() else {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        };
        if !self.visible_row_indices().contains(&index) {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        }
        let row = self.item_rect(origin_y, index);
        let w = NEW_GROUP_BUTTON_WIDTH;
        let h = 22.0;
        Rect::new(row.right() - w, row.y + (SECTION_HEIGHT - h) * 0.5, w, h)
    }

    /// Inline new-group form under the Hosts section (only when drafting).
    pub fn new_group_form_rect(&self, origin_y: f32) -> Option<Rect> {
        if !self.new_group_drafting {
            return None;
        }
        let index = self.hosts_section_index()?;
        if !self.visible_row_indices().contains(&index) {
            return None;
        }
        let section = self.item_rect(origin_y, index);
        Some(Rect::new(
            ORIGIN_X + PAD_X,
            section.y + SECTION_HEIGHT + 2.0,
            WIDTH - 2.0 * PAD_X,
            NEW_GROUP_FORM_HEIGHT,
        ))
    }

    /// Name field inside the new-group form.
    pub fn new_group_field_rect(&self, origin_y: f32) -> Option<Rect> {
        let form = self.new_group_form_rect(origin_y)?;
        // Leave room for Create (~56) + Cancel (~52) + gaps.
        let trailing = 56.0 + 4.0 + 52.0 + 4.0;
        Some(Rect::new(
            form.x + 4.0,
            form.y + 4.0,
            (form.width - 8.0 - trailing).max(40.0),
            form.height - 8.0,
        ))
    }

    /// Create button inside the new-group form.
    pub fn new_group_create_rect(&self, origin_y: f32) -> Option<Rect> {
        let form = self.new_group_form_rect(origin_y)?;
        let field = self.new_group_field_rect(origin_y)?;
        Some(Rect::new(
            field.right() + 4.0,
            form.y + 4.0,
            56.0,
            form.height - 8.0,
        ))
    }

    /// Cancel button inside the new-group form.
    pub fn new_group_cancel_rect(&self, origin_y: f32) -> Option<Rect> {
        let form = self.new_group_form_rect(origin_y)?;
        let create = self.new_group_create_rect(origin_y)?;
        Some(Rect::new(
            create.right() + 4.0,
            form.y + 4.0,
            52.0,
            form.height - 8.0,
        ))
    }

    /// Extra scroll height inserted under the Hosts section for the form.
    fn new_group_form_slot_height(&self) -> f32 {
        if self.new_group_drafting {
            NEW_GROUP_FORM_HEIGHT + CARD_GAP + 2.0
        } else {
            0.0
        }
    }

    /// Open the inline new-group form and focus the name field.
    pub fn open_new_group_form(&mut self) {
        self.new_group_drafting = true;
        self.new_group_focused = true;
        self.filter_focused = false;
    }

    /// Close the inline new-group form and clear the draft.
    pub fn close_new_group_form(&mut self) {
        self.new_group_drafting = false;
        self.new_group_focused = false;
        self.new_group_name.clear();
    }

    /// Start renaming a stored host or group (context menu).
    pub fn begin_rename(&mut self, id: String, is_group: bool, current_name: &str) {
        self.filter_focused = false;
        self.new_group_focused = false;
        let name = current_name.to_string();
        let caret = name.chars().count();
        self.rename = Some(RenameDraft {
            id,
            is_group,
            name,
            caret,
            sel_anchor: None,
            focused: true,
        });
    }

    pub fn cancel_rename(&mut self) {
        self.rename = None;
    }

    pub fn is_renaming(&self, id: &str) -> bool {
        self.rename.as_ref().is_some_and(|r| r.id == id)
    }

    /// Toggle the inline new-group form.
    pub fn toggle_new_group_form(&mut self) {
        if self.new_group_drafting {
            self.close_new_group_form();
        } else {
            self.open_new_group_form();
        }
    }

    /// Whether this host id is the one currently connecting.
    pub fn is_connecting(&self, id: &str) -> bool {
        self.connecting_id.as_deref() == Some(id)
    }

    /// Start (or replace) the connecting indicator for `id`.
    pub fn begin_connecting(&mut self, id: impl Into<String>) {
        self.connecting_id = Some(id.into());
        self.error = None;
    }

    /// Drop the connecting indicator.
    pub fn end_connecting(&mut self) {
        self.connecting_id = None;
    }

    /// Centre of the orbit indicator on the trailing edge of a host row.
    ///
    /// Kept as geometry here so the painter and any future hit-test agree.
    pub fn connecting_center(&self, origin_y: f32, index: usize) -> Option<(f32, f32)> {
        let host = self.rows.get(index)?.host()?;
        if !self.is_connecting(&host.id) {
            return None;
        }
        let row = self.card_rect(origin_y, index);
        let cx = row.right() - CARD_PAD - CONNECTING_SLOT * 0.5;
        let cy = row.y + ITEM_HEIGHT * 0.5;
        Some((cx, cy))
    }

    /// Track under the subtitle where the shimmer sweeps while connecting.
    pub fn connecting_shimmer_track(
        &self,
        origin_y: f32,
        index: usize,
    ) -> Option<(f32, f32, f32)> {
        let host = self.rows.get(index)?.host()?;
        if !self.is_connecting(&host.id) {
            return None;
        }
        let row = self.card_rect(origin_y, index);
        let text_x = row.x + self.host_leading_inset(index) + HOST_BADGE_TILE + ICON_GAP;
        let right = row.right() - CARD_PAD - CONNECTING_SLOT - 6.0;
        let width = (right - text_x).max(0.0);
        Some((text_x, row.y + 48.0, width))
    }

    /// A row slot, in viewport coordinates (already scrolled).
    ///
    /// Host/group slots include [`CARD_GAP`] below the painted card.
    pub fn item_rect(&self, origin_y: f32, index: usize) -> Rect {
        let height = self.row_slot_height(index);
        let card_inset = PAD_X;
        Rect::new(
            ORIGIN_X + card_inset,
            self.rows_top(origin_y) + self.offset_of(index) - self.scroll,
            WIDTH - 2.0 * card_inset,
            height,
        )
    }

    /// Painted card box inside a host/group/session slot (excludes the gap).
    ///
    /// Nested hosts/sessions inside a group tray are inset by [`CARD_PAD`] on
    /// **both** sides so the child card does not flush against the tray edge.
    pub fn card_rect(&self, origin_y: f32, index: usize) -> Rect {
        let slot = self.item_rect(origin_y, index);
        let h = self
            .rows
            .get(index)
            .map(Row::card_height)
            .unwrap_or(ITEM_HEIGHT);
        let (nest_left, nest_right) = match self.rows.get(index) {
            Some(Row::Host(host)) if host.nested => (CARD_PAD, CARD_PAD),
            Some(Row::Session(session)) => {
                let parent_nested = self
                    .rows
                    .iter()
                    .find_map(|r| r.host().filter(|h| h.id == session.host_id))
                    .is_some_and(|h| h.nested);
                let left = SESSION_INDENT + if parent_nested { CARD_PAD } else { 0.0 };
                let right = if parent_nested { CARD_PAD } else { 0.0 };
                (left, right)
            }
            _ => (0.0, 0.0),
        };
        Rect::new(
            slot.x + nest_left,
            slot.y,
            (slot.width - nest_left - nest_right).max(0.0),
            h.min(slot.height),
        )
    }

    /// Close × hit box on a session row.
    pub fn session_close_rect(&self, origin_y: f32, index: usize) -> Option<Rect> {
        let _ = self.rows.get(index)?.session()?;
        let card = self.card_rect(origin_y, index);
        let s = SESSION_CLOSE_HIT;
        Some(Rect::new(
            card.right() - CARD_PAD - s,
            card.y + (card.height - s) * 0.5,
            s,
            s,
        ))
    }

    /// "+" add-session hit box on a host row (left of trailing chevron).
    pub fn host_add_session_rect(&self, origin_y: f32, index: usize) -> Option<Rect> {
        let host = self.rows.get(index)?.host()?;
        let card = self.card_rect(origin_y, index);
        let s = HOST_ADD_HIT;
        let chevron_slot = if host.session_count > 0 {
            CHEVRON_HIT + 4.0
        } else {
            0.0
        };
        Some(Rect::new(
            card.right() - CARD_PAD - chevron_slot - s,
            card.y + (card.height - s) * 0.5,
            s,
            s,
        ))
    }

    /// Chevron hit box on a host that has sessions (trailing twistie).
    pub fn host_chevron_rect(&self, origin_y: f32, index: usize) -> Option<Rect> {
        let host = self.rows.get(index)?.host()?;
        if host.session_count == 0 {
            return None;
        }
        let card = self.card_rect(origin_y, index);
        let s = CHEVRON_HIT;
        Some(Rect::new(
            card.right() - CARD_PAD - s,
            card.y + (card.height - s) * 0.5,
            s,
            s,
        ))
    }

    /// Leading content inset before the OS badge (stable; no twistie column).
    pub fn host_leading_inset(&self, _index: usize) -> f32 {
        CARD_PAD
    }

    /// Outer tray that wraps a group header and its visible nested hosts/sessions.
    ///
    /// Paint-only geometry: hit-tests still use per-row [`card_rect`]s. When the
    /// group is collapsed the tray is just the header card.
    pub fn group_tray_rect(&self, origin_y: f32, group_index: usize) -> Option<Rect> {
        let Row::Group { .. } = self.rows.get(group_index)? else {
            return None;
        };
        let header = self.card_rect(origin_y, group_index);
        let visible = self.visible_row_indices();
        let pos = visible.iter().position(|&i| i == group_index)?;

        let mut bottom = header.bottom();
        let mut has_children = false;
        for &idx in visible.iter().skip(pos + 1) {
            match self.rows.get(idx) {
                Some(Row::Host(host)) if host.nested => {
                    bottom = self.card_rect(origin_y, idx).bottom();
                    has_children = true;
                }
                Some(Row::Session(_)) => {
                    bottom = self.card_rect(origin_y, idx).bottom();
                    has_children = true;
                }
                _ => break,
            }
        }
        // Inner pad under the last child — same as the side gutters ([`CARD_PAD`]).
        // The last nested row's slot also reserves [`CARD_GAP`] *below* this pad so
        // the gap from tray edge to the next root card matches collapsed groups.
        let pad_bottom = if has_children { CARD_PAD } else { 0.0 };
        Some(Rect::new(
            header.x,
            header.y,
            header.width,
            (bottom - header.y + pad_bottom).max(header.height),
        ))
    }

    /// Indices of nested hosts belonging to a group (visible only).
    pub fn group_nested_host_indices(&self, group_index: usize) -> Vec<usize> {
        let visible = self.visible_row_indices();
        let Some(pos) = visible.iter().position(|&i| i == group_index) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &idx in visible.iter().skip(pos + 1) {
            match self.rows.get(idx) {
                Some(Row::Host(host)) if host.nested => out.push(idx),
                Some(Row::Session(_)) => {}
                _ => break,
            }
        }
        out
    }

    /// Resolve a drop target under `(x, y)` while dragging a host or group.
    ///
    /// Root reordering uses the pointer's Y among root cards (top half =
    /// insert before, past the last card = append) so a insertion bar can
    /// guide the drop. Membership into a group still wins when the pointer
    /// is on a nested host, an empty group, or a session under a group.
    pub fn drop_target_at(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<HostDropTarget> {
        let dragging_group = self.host_drag.as_ref().is_some_and(HostDrag::is_group);
        let drag_id = self.host_drag.as_ref().map(|d| d.host_id.as_str());

        // Membership takes priority over root reordering.
        if !dragging_group {
            match self.hit_test(origin_y, height, x, y) {
                Some(PanelHit::Item(index)) => {
                    if let Some(group_id) = self.enclosing_group_id(index) {
                        return Some(HostDropTarget::Group(group_id));
                    }
                }
                Some(PanelHit::Session(index)) => {
                    let host_id = self.rows.get(index)?.session()?.host_id.clone();
                    let host_idx = self.row_of_host(&host_id)?;
                    if let Some(group_id) = self.enclosing_group_id(host_idx) {
                        return Some(HostDropTarget::Group(group_id));
                    }
                }
                Some(PanelHit::Group(index)) => {
                    if let Some(Row::Group {
                        id,
                        host_count,
                        ..
                    }) = self.rows.get(index)
                    {
                        if *host_count == 0 {
                            let card = self.card_rect(origin_y, index);
                            // Middle band of an empty group = join; edges = reorder.
                            let rel = (y - card.y) / card.height.max(1.0);
                            if (0.25..0.75).contains(&rel) {
                                return Some(HostDropTarget::Group(id.clone()));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if !self.point_in_hosts_section(origin_y, height, y) {
            return None;
        }

        self.root_insert_target_at(origin_y, y, drag_id)
    }

    /// Root cards (ungrouped stored hosts + groups) in paint order.
    fn root_reorder_cards(&self, origin_y: f32) -> Vec<(crate::geom::Rect, HostDropTarget)> {
        let mut out = Vec::new();
        for (index, row) in self.rows.iter().enumerate() {
            match row {
                Row::Host(host) if host.stored && !host.nested => {
                    out.push((
                        self.card_rect(origin_y, index),
                        HostDropTarget::BeforeHost(host.id.clone()),
                    ));
                }
                Row::Group { id, .. } => {
                    out.push((
                        self.card_rect(origin_y, index),
                        HostDropTarget::BeforeGroup(id.clone()),
                    ));
                }
                _ => {}
            }
        }
        out
    }

    /// Pick insert-before / append from pointer Y among root cards.
    fn root_insert_target_at(
        &self,
        origin_y: f32,
        y: f32,
        drag_id: Option<&str>,
    ) -> Option<HostDropTarget> {
        let cards = self.root_reorder_cards(origin_y);
        if cards.is_empty() {
            return Some(HostDropTarget::Ungroup);
        }

        for (rect, target) in &cards {
            let skips_self = match target {
                HostDropTarget::BeforeHost(id) | HostDropTarget::BeforeGroup(id) => {
                    Some(id.as_str()) == drag_id
                }
                _ => false,
            };
            if skips_self {
                continue;
            }
            let mid = rect.y + rect.height * 0.5;
            if y < mid {
                return Some(target.clone());
            }
        }
        Some(HostDropTarget::Ungroup)
    }

    /// Thin insertion bar for root reorder targets (not membership).
    pub fn insertion_bar_rect(
        &self,
        origin_y: f32,
        target: &HostDropTarget,
    ) -> Option<crate::geom::Rect> {
        const BAR_H: f32 = 3.0;
        let width = WIDTH - 2.0 * PAD_X;
        let x = ORIGIN_X + PAD_X;
        match target {
            HostDropTarget::BeforeHost(host_id) => {
                let index = self.row_of_host(host_id)?;
                let card = self.card_rect(origin_y, index);
                Some(crate::geom::Rect::new(
                    x,
                    card.y - BAR_H * 0.5 - 1.0,
                    width,
                    BAR_H,
                ))
            }
            HostDropTarget::BeforeGroup(group_id) => {
                let index = self.rows.iter().position(
                    |row| matches!(row, Row::Group { id, .. } if id == group_id),
                )?;
                let card = self.card_rect(origin_y, index);
                Some(crate::geom::Rect::new(
                    x,
                    card.y - BAR_H * 0.5 - 1.0,
                    width,
                    BAR_H,
                ))
            }
            HostDropTarget::Ungroup => {
                let cards = self.root_reorder_cards(origin_y);
                let y = if let Some((last, _)) = cards.last() {
                    last.bottom() + CARD_GAP * 0.5 - BAR_H * 0.5
                } else {
                    let hosts_idx = self.hosts_section_index()?;
                    self.item_rect(origin_y, hosts_idx).bottom() + 4.0
                };
                Some(crate::geom::Rect::new(x, y, width, BAR_H))
            }
            HostDropTarget::Group(_) => None,
        }
    }

    /// Walk upward from `index` to find the enclosing group id, if any.
    fn enclosing_group_id(&self, index: usize) -> Option<String> {
        for i in (0..=index).rev() {
            match self.rows.get(i)? {
                Row::Group { id, .. } => return Some(id.clone()),
                Row::Section(_) => return None,
                Row::Host(host) if !host.nested => return None,
                _ => {}
            }
        }
        None
    }

    fn point_in_hosts_section(&self, origin_y: f32, height: f32, y: f32) -> bool {
        let Some(hosts_idx) = self.hosts_section_index() else {
            return false;
        };
        let body = self.body_rect(origin_y, height);
        if y < body.y || y > body.bottom() {
            return false;
        }
        let hosts_top = self.item_rect(origin_y, hosts_idx).y;
        y >= hosts_top
    }

    /// Clear any in-progress host drag.
    pub fn clear_host_drag(&mut self) {
        self.host_drag = None;
    }

    /// Destination rect for a snap into `target` (group tray, insertion slot).
    pub fn drop_slot_rect(
        &self,
        origin_y: f32,
        target: &HostDropTarget,
    ) -> Option<crate::geom::Rect> {
        match target {
            HostDropTarget::Group(group_id) => {
                let group_index = self.rows.iter().position(
                    |row| matches!(row, Row::Group { id, .. } if id == group_id),
                )?;
                let tray = self.group_tray_rect(origin_y, group_index)?;
                let header = self.card_rect(origin_y, group_index);
                let nest = CARD_PAD;
                // Append after the last nested host/session — not under the header.
                let nested = self.group_nested_host_indices(group_index);
                let y = if let Some(&last_host) = nested.last() {
                    let mut bottom = self.card_rect(origin_y, last_host).bottom();
                    let visible = self.visible_row_indices();
                    if let Some(pos) = visible.iter().position(|&i| i == last_host) {
                        for &idx in visible.iter().skip(pos + 1) {
                            match self.rows.get(idx) {
                                Some(Row::Session(_)) => {
                                    bottom = self.card_rect(origin_y, idx).bottom();
                                }
                                _ => break,
                            }
                        }
                    }
                    bottom + 4.0
                } else {
                    header.bottom() + 4.0
                };
                let y = y.min((tray.bottom() - ITEM_HEIGHT).max(header.bottom()));
                Some(crate::geom::Rect::new(
                    tray.x + nest,
                    y,
                    (tray.width - 2.0 * nest).max(40.0),
                    ITEM_HEIGHT,
                ))
            }
            HostDropTarget::Ungroup
            | HostDropTarget::BeforeHost(_)
            | HostDropTarget::BeforeGroup(_) => {
                let bar = self.insertion_bar_rect(origin_y, target)?;
                Some(crate::geom::Rect::new(
                    bar.x,
                    bar.y - ITEM_HEIGHT * 0.5 + bar.height * 0.5,
                    bar.width,
                    ITEM_HEIGHT,
                ))
            }
        }
    }

    /// Slot height for a visible row, including post-session breathing room.
    pub fn row_slot_height(&self, index: usize) -> f32 {
        let Some(row) = self.rows.get(index) else {
            return ITEM_HEIGHT + CARD_GAP;
        };
        match row {
            Row::Session(session) => {
                let visible = self.visible_row_indices();
                let pos = visible.iter().position(|&i| i == index);
                let next = pos.and_then(|p| visible.get(p + 1).copied());
                let same_host_next = next
                    .and_then(|n| self.rows.get(n))
                    .and_then(Row::session)
                    .is_some_and(|s| s.host_id == session.host_id);
                let last_in_group = match next {
                    None => true,
                    Some(n) => {
                        !matches!(
                            self.rows.get(n),
                            Some(Row::Host(h)) if h.nested
                        ) && !matches!(self.rows.get(n), Some(Row::Session(_)))
                    }
                };
                let gap = if same_host_next {
                    SESSION_GAP
                } else if last_in_group
                    && self
                        .rows
                        .iter()
                        .find_map(|r| r.host().filter(|h| h.id == session.host_id))
                        .is_some_and(|h| h.nested)
                {
                    // Nested host's last session: tray pad + root CARD_GAP.
                    CARD_PAD + CARD_GAP
                } else {
                    SESSION_AFTER_GAP
                };
                SESSION_HEIGHT + gap
            }
            Row::Host(host) if host.nested => {
                let visible = self.visible_row_indices();
                let pos = visible.iter().position(|&i| i == index);
                let next = pos.and_then(|p| visible.get(p + 1).copied());
                let next_is_nested_continuation = next.is_some_and(|n| {
                    matches!(
                        self.rows.get(n),
                        Some(Row::Host(h)) if h.nested
                    ) || matches!(self.rows.get(n), Some(Row::Session(_)))
                });
                if next_is_nested_continuation {
                    ITEM_HEIGHT + CARD_GAP
                } else {
                    // Last nested host in the tray: inner CARD_PAD + outer CARD_GAP
                    // so tray→next-root spacing matches collapsed group→group.
                    ITEM_HEIGHT + CARD_PAD + CARD_GAP
                }
            }
            _ => row.height(),
        }
    }

    /// Unscrolled top of the scrollable content (below the sticky header).
    fn content_top(&self, origin_y: f32) -> f32 {
        origin_y + HEADER_HEIGHT
    }

    /// Row 0's unscrolled top edge: below the New Host CTA.
    fn rows_top(&self, origin_y: f32) -> f32 {
        self.content_top(origin_y) + CTA_TOP_GAP + CTA_HEIGHT + CTA_GAP
    }

    /// Scrolled offset of row `index` from row 0's top edge (visible rows only).
    fn offset_of(&self, index: usize) -> f32 {
        let hosts_idx = self.hosts_section_index();
        let form_extra = self.new_group_form_slot_height();
        self.visible_row_indices()
            .into_iter()
            .take_while(|visible| *visible < index)
            .map(|visible| {
                let mut h = self.row_slot_height(visible);
                if hosts_idx == Some(visible) {
                    h += form_extra;
                }
                h
            })
            .sum()
    }

    /// Row indices that should be painted and hit-tested.
    pub fn visible_row_indices(&self) -> Vec<usize> {
        let filter_lower = self.filter.trim().to_ascii_lowercase();
        let filtering = !filter_lower.is_empty();

        let host_visible = |host: &HostItem| -> bool {
            !filtering
                || host.name.to_ascii_lowercase().contains(&filter_lower)
                || host.endpoint.to_ascii_lowercase().contains(&filter_lower)
        };
        let session_visible = |session: &SessionItem| -> bool {
            !filtering || session.title.to_ascii_lowercase().contains(&filter_lower)
        };

        let mut out = Vec::new();
        let mut i = 0;
        while i < self.rows.len() {
            match &self.rows[i] {
                Row::Section(_) => {
                    let mut any = false;
                    let mut j = i + 1;
                    while j < self.rows.len() {
                        match &self.rows[j] {
                            Row::Section(_) => break,
                            Row::Group { .. } => {
                                any = true;
                                break;
                            }
                            Row::Host(host) => {
                                if host_visible(host) {
                                    any = true;
                                    break;
                                }
                            }
                            Row::Session(session) => {
                                if session_visible(session) {
                                    any = true;
                                    break;
                                }
                            }
                        }
                        j += 1;
                    }
                    if !filtering || any {
                        out.push(i);
                    }
                    i += 1;
                }
                Row::Group {
                    id,
                    name,
                    collapsed,
                    ..
                } => {
                    let collapsed = *collapsed || self.collapsed_groups.contains(id);
                    let mut j = i + 1;
                    let mut child_indices = Vec::new();
                    while j < self.rows.len() {
                        match &self.rows[j] {
                            Row::Host(host) if host.nested => {
                                child_indices.push(j);
                                j += 1;
                                // Sessions belonging to this nested host.
                                while j < self.rows.len() {
                                    match &self.rows[j] {
                                        Row::Session(_) => {
                                            child_indices.push(j);
                                            j += 1;
                                        }
                                        _ => break,
                                    }
                                }
                            }
                            Row::Session(_) => {
                                // Orphan session under group — skip.
                                j += 1;
                            }
                            _ => break,
                        }
                    }
                    let any_visible_child =
                        child_indices.iter().any(|&idx| match &self.rows[idx] {
                            Row::Host(host) => host_visible(host),
                            Row::Session(session) => session_visible(session),
                            _ => false,
                        });
                    let name_matches =
                        filtering && name.to_ascii_lowercase().contains(&filter_lower);
                    if !filtering || any_visible_child || name_matches {
                        out.push(i);
                    }
                    if !collapsed {
                        for idx in child_indices {
                            match &self.rows[idx] {
                                Row::Host(host) => {
                                    if host_visible(host) || name_matches {
                                        out.push(idx);
                                    }
                                }
                                Row::Session(session) => {
                                    let parent_collapsed =
                                        self.collapsed_hosts.contains(&session.host_id);
                                    if !parent_collapsed
                                        && (session_visible(session) || name_matches)
                                    {
                                        out.push(idx);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    i = j;
                }
                Row::Host(host) => {
                    let show_host = host_visible(host);
                    if show_host {
                        out.push(i);
                    }
                    let host_id = host.id.clone();
                    let host_collapsed = self.collapsed_hosts.contains(&host_id);
                    i += 1;
                    while i < self.rows.len() {
                        match &self.rows[i] {
                            Row::Session(session) if session.host_id == host_id => {
                                if show_host
                                    && !host_collapsed
                                    && (session_visible(session) || !filtering)
                                {
                                    out.push(i);
                                }
                                i += 1;
                            }
                            _ => break,
                        }
                    }
                }
                Row::Session(_) => {
                    // Handled while walking the parent host.
                    i += 1;
                }
            }
        }
        out
    }

    /// Total height of the visible rows, ignoring the viewport.
    pub fn content_height(&self) -> f32 {
        let hosts_idx = self.hosts_section_index();
        let form_extra = self.new_group_form_slot_height();
        CTA_TOP_GAP
            + CTA_HEIGHT
            + CTA_GAP
            + self
                .visible_row_indices()
                .into_iter()
                .map(|index| {
                    let mut h = self.row_slot_height(index);
                    if hosts_idx == Some(index) {
                        h += form_extra;
                    }
                    h
                })
                .sum::<f32>()
    }

    /// How many stored hosts the list holds: what the `Hosts` section shows
    /// and what the header counts. Section labels and the platform rows are
    /// not hosts of the user's.
    pub fn host_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.host().is_some_and(|host| host.stored))
            .count()
    }

    /// Largest legal scroll offset for this viewport.
    pub fn max_scroll(&self, origin_y: f32, height: f32) -> f32 {
        (self.content_height() - self.body_rect(origin_y, height).height).max(0.0)
    }

    /// Pull `scroll` back inside the viewport.
    ///
    /// Called after every list or window change: without it a shrink
    /// would leave the list scrolled past its own content, showing an
    /// empty panel.
    pub fn clamp_scroll(&mut self, origin_y: f32, height: f32) {
        let max = self.max_scroll(origin_y, height);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// Scroll by `delta` logical pixels, clamped.
    pub fn scroll_by(&mut self, delta: f32, origin_y: f32, height: f32) {
        self.scroll += delta;
        self.clamp_scroll(origin_y, height);
    }

    /// Scroll by whole wheel notches.
    pub fn scroll_rows(&mut self, rows: f32, origin_y: f32, height: f32) {
        self.scroll_by(rows * (ITEM_HEIGHT + CARD_GAP), origin_y, height);
    }

    /// Replace the list, keeping the selected host selected.
    ///
    /// Selection is an index, so a refresh that reorders or inserts
    /// rows would silently move the highlight onto a different host
    /// unless it is re-resolved by id. The connecting indicator is
    /// likewise re-checked: a distro that vanished mid-start drops it.
    pub fn set_rows(&mut self, rows: Vec<Row>) {
        let keep = self.selected_id().map(str::to_string);
        let keep_session = self.selected_session;
        let connecting = self.connecting_id.clone();
        self.rows = rows;
        self.selected = keep.as_deref().and_then(|id| self.row_of_host(id));
        self.selected_session = keep_session.filter(|&tab| {
            self.rows
                .iter()
                .any(|r| r.session().is_some_and(|s| s.tab_index == tab))
        });
        self.hover = None;
        if let Some(id) = connecting {
            if self.row_of_host(&id).is_none() {
                self.connecting_id = None;
            } else {
                self.connecting_id = Some(id);
            }
        }
        #[cfg(debug_assertions)]
        crate::overlap::assert_panel_no_overlaps(self, 0.0, 2000.0);
    }

    /// Replace a flat host list, i.e. one with no section labels.
    pub fn set_items(&mut self, items: Vec<HostItem>) {
        self.set_rows(items.into_iter().map(Row::Host).collect());
    }

    /// Row index of the host with this id.
    pub fn row_of_host(&self, id: &str) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.host().is_some_and(|host| host.id == id))
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected_item().map(|item| item.id.as_str())
    }

    pub fn selected_item(&self) -> Option<&HostItem> {
        self.selected
            .and_then(|row| self.rows.get(row))
            .and_then(Row::host)
    }

    /// Select a host by id. Returns whether it was found.
    pub fn select_id(&mut self, id: &str) -> bool {
        match self.row_of_host(id) {
            Some(row) => {
                self.selected = Some(row);
                true
            }
            None => false,
        }
    }

    /// Follow the active session: select `id`, or drop the highlight when no
    /// row carries it.
    pub fn follow_host(&mut self, id: &str) {
        if !self.select_id(id) {
            self.selected = None;
        }
    }

    /// Select the open session with this tab index.
    pub fn select_session(&mut self, tab_index: usize) -> bool {
        let Some(row) = self
            .rows
            .iter()
            .position(|r| r.session().is_some_and(|s| s.tab_index == tab_index))
        else {
            return false;
        };
        self.selected_session = Some(tab_index);
        if let Some(host_id) = self.rows[row].session().map(|s| s.host_id.clone()) {
            self.selected = self.row_of_host(&host_id);
            self.collapsed_hosts.remove(&host_id);
        }
        true
    }

    /// Follow the active terminal: highlight its session row (and host).
    pub fn follow_session(&mut self, tab_index: usize, host_id: &str) {
        if !self.select_session(tab_index) {
            self.selected_session = Some(tab_index);
            let _ = self.select_id(host_id);
        }
    }

    /// Toggle whether a host's sessions are collapsed.
    pub fn toggle_host_collapsed(&mut self, host_id: &str) {
        if self.collapsed_hosts.contains(host_id) {
            self.collapsed_hosts.remove(host_id);
        } else {
            self.collapsed_hosts.insert(host_id.to_string());
        }
    }

    /// Which host row is under `(x, y)`, if any.
    fn item_at(&self, origin_y: f32, height: f32, x: f32, y: f32) -> Option<usize> {
        let body = self.body_rect(origin_y, height);
        if !body.contains(x, y) {
            return None;
        }
        let mut top = self.rows_top(origin_y) - self.scroll;
        for index in self.visible_row_indices() {
            let row = &self.rows[index];
            let slot_h = self.row_slot_height(index);
            let card_h = row.card_height();
            if y >= top && y < top + card_h {
                return row.host().map(|_| index);
            }
            top += slot_h;
        }
        None
    }

    fn session_at(&self, origin_y: f32, height: f32, x: f32, y: f32) -> Option<usize> {
        let body = self.body_rect(origin_y, height);
        if !body.contains(x, y) {
            return None;
        }
        let mut top = self.rows_top(origin_y) - self.scroll;
        for index in self.visible_row_indices() {
            let row = &self.rows[index];
            let slot_h = self.row_slot_height(index);
            let card_h = row.card_height();
            if y >= top && y < top + card_h {
                return row.session().map(|_| index);
            }
            top += slot_h;
        }
        None
    }

    /// Which group header row is under `(x, y)`, if any.
    fn group_at(&self, origin_y: f32, height: f32, x: f32, y: f32) -> Option<usize> {
        let body = self.body_rect(origin_y, height);
        if !body.contains(x, y) {
            return None;
        }
        let mut top = self.rows_top(origin_y) - self.scroll;
        for index in self.visible_row_indices() {
            let row = &self.rows[index];
            let slot_h = self.row_slot_height(index);
            let card_h = row.card_height();
            if y >= top && y < top + card_h {
                if row.group().is_some() {
                    return Some(index);
                }
                return None;
            }
            top += slot_h;
        }
        None
    }

    /// What is under `(x, y)`.
    pub fn hit_test(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<PanelHit> {
        if !self.rect(origin_y, height).contains(x, y) {
            return None;
        }
        if self.search_rect(origin_y).contains(x, y) {
            return Some(PanelHit::Search);
        }
        if let Some(rect) = self.new_group_create_rect(origin_y) {
            if rect.contains(x, y) {
                return Some(PanelHit::NewGroupCreate);
            }
        }
        if let Some(rect) = self.new_group_cancel_rect(origin_y) {
            if rect.contains(x, y) {
                return Some(PanelHit::NewGroupCancel);
            }
        }
        if let Some(rect) = self.new_group_field_rect(origin_y) {
            if rect.contains(x, y) {
                return Some(PanelHit::NewGroupField);
            }
        }
        if self.add_button_rect(origin_y, height).contains(x, y) {
            return Some(PanelHit::AddHost);
        }
        if self.new_group_button_rect(origin_y, height).contains(x, y) {
            return Some(PanelHit::NewGroup);
        }
        if let Some(index) = self.session_at(origin_y, height, x, y) {
            if let Some(close) = self.session_close_rect(origin_y, index) {
                if close.contains(x, y) {
                    return Some(PanelHit::CloseSession(index));
                }
            }
            return Some(PanelHit::Session(index));
        }
        if let Some(index) = self.group_at(origin_y, height, x, y) {
            return Some(PanelHit::Group(index));
        }
        if let Some(index) = self.item_at(origin_y, height, x, y) {
            if let Some(chevron) = self.host_chevron_rect(origin_y, index) {
                if chevron.contains(x, y) {
                    return Some(PanelHit::ToggleHost(index));
                }
            }
            if let Some(add) = self.host_add_session_rect(origin_y, index) {
                if add.contains(x, y) {
                    return Some(PanelHit::HostAddSession(index));
                }
            }
            return Some(PanelHit::Item(index));
        }
        Some(PanelHit::Background)
    }

    pub fn set_hover(&mut self, hit: Option<PanelHit>) -> bool {
        let (item, add, new_group) = match hit {
            Some(PanelHit::Item(index))
            | Some(PanelHit::ToggleHost(index))
            | Some(PanelHit::HostAddSession(index))
            | Some(PanelHit::Session(index))
            | Some(PanelHit::CloseSession(index)) => (Some(index), false, false),
            Some(PanelHit::AddHost) => (None, true, false),
            Some(PanelHit::NewGroup) => (None, false, true),
            _ => (None, false, false),
        };
        if item == self.hover
            && add == self.add_hover
            && new_group == self.new_group_hover
        {
            return false;
        }
        self.hover = item;
        self.add_hover = add;
        self.new_group_hover = new_group;
        true
    }

    /// Which row the pointer is over, if any.
    ///
    /// Returns the hit rather than an index so the add-host row can be
    /// highlighted too — it is not an item, and reporting `None` for it
    /// would leave the button with no hover feedback.
    pub fn hover_at(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<PanelHit> {
        match self.hit_test(origin_y, height, x, y) {
            Some(PanelHit::Background) | None => None,
            hit => hit,
        }
    }

    /// The header's count label ("3 hosts").
    pub fn count_label(&self) -> String {
        match self.host_count() {
            0 => "empty".to_string(),
            1 => "1 host".to_string(),
            n => format!("{n} hosts"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(n: usize) -> Vec<HostItem> {
        (0..n)
            .map(|i| HostItem {
                id: format!("id-{i}"),
                name: format!("host-{i}"),
                endpoint: format!("deploy@host-{i}:2222"),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Idle,
                nested: false,
                session_count: 0,
            })
            .collect()
    }

    fn panel(n: usize) -> HostPanel {
        let mut panel = HostPanel::default();
        panel.set_items(items(n));
        panel
    }

    /// A viewport tall enough that N rows fit without scrolling.
    fn tall() -> (f32, f32) {
        (0.0, 1000.0)
    }

    #[test]
    fn rows_are_hit_at_their_painted_position() {
        let panel = panel(3);
        let (oy, h) = tall();
        for index in 0..3 {
            let rect = panel.item_rect(oy, index);
            let (x, y) = (rect.x + 10.0, rect.y + rect.height / 2.0);
            assert_eq!(panel.hit_test(oy, h, x, y), Some(PanelHit::Item(index)));
        }
    }

    #[test]
    fn the_add_button_is_its_own_target() {
        let panel = panel(2);
        let (oy, h) = tall();
        let button = panel.add_button_rect(oy, h);
        let (x, y) = (
            button.x + button.width / 2.0,
            button.y + button.height / 2.0,
        );
        assert_eq!(panel.hit_test(oy, h, x, y), Some(PanelHit::AddHost));

        // The footer strip beside the button is not the button.
        let beside = Rect::new(panel.footer_rect(oy, h).x, y, 4.0, 1.0);
        assert_eq!(
            panel.hit_test(oy, h, beside.x + 1.0, y),
            Some(PanelHit::Background)
        );
    }

    #[test]
    fn mock_layout_constants_match_apple_hig_mock() {
        assert_eq!(WIDTH, 288.0); // w-72
        assert_eq!(ORIGIN_X, 64.0); // rail w-16
        assert_eq!(CARD_GAP, 10.0);
        assert_eq!(CTA_GAP, 8.0);
        assert_eq!(CTA_TOP_GAP, 12.0);
        assert_eq!(PAD_X, 10.0);
        assert_eq!(CARD_PAD, 12.0);
        assert_eq!(CARD_RADIUS, 8.0);
        assert_eq!(BADGE_TILE, 32.0); // w-8 h-8 (New Host CTA)
        assert_eq!(HOST_BADGE_TILE, 28.0); // p-1.5 + 16px glyph
        assert_eq!(ITEM_HEIGHT, 56.0);
        assert!(CTA_HEIGHT >= 56.0);
        assert_eq!(SESSION_HEIGHT, 28.0);
        assert_eq!(SESSION_GAP, 6.0);
        assert_eq!(SESSION_AFTER_GAP, 18.0);
        assert_eq!(SESSION_INDENT, 16.0);
        assert_eq!(SESSION_RADIUS, 6.0);
        assert_eq!(SESSION_ACCENT, 0.0);
        assert_eq!(SESSION_CONTENT_PAD, 10.0);
        assert_eq!(SECTION_GAP, 10.0);
        assert_eq!(STATUS_DOT, 6.0);
        assert_eq!(SECTION_HEIGHT, 22.0);
    }

    #[test]
    fn the_panel_claims_nothing_left_of_itself() {
        let panel = panel(3);
        let (oy, h) = tall();
        let row = panel.item_rect(oy, 0);
        assert_eq!(panel.hit_test(oy, h, ORIGIN_X - 1.0, row.y + 1.0), None);
        assert_eq!(panel.hit_test(oy, h, ORIGIN_X + WIDTH, row.y + 1.0), None);
    }

    #[test]
    fn scrolling_moves_the_hit_targets_with_the_pixels() {
        let mut panel = panel(20);
        let (oy, h) = (0.0, 400.0);
        let body = panel.body_rect(oy, h);
        assert_eq!(
            panel.max_scroll(oy, h),
            panel.content_height() - body.height
        );

        let y0 = {
            let mut unscrolled = panel.clone();
            unscrolled.scroll = 0.0;
            unscrolled.item_rect(oy, 0).y
        };

        panel.scroll_rows(WHEEL_ROWS, oy, h);
        assert_eq!(panel.scroll, WHEEL_ROWS * (ITEM_HEIGHT + CARD_GAP));

        // The pixel that held row 0 before the scroll now holds row 3.
        assert!(panel.item_rect(oy, 0).bottom() < y0 + 2.0);
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + PAD_X + 10.0, y0 + 2.0),
            Some(PanelHit::Item(3))
        );
    }

    #[test]
    fn scroll_is_clamped_to_the_content() {
        let mut panel = panel(3);
        let (oy, h) = tall();
        panel.scroll_rows(100.0, oy, h);
        assert_eq!(panel.scroll, 0.0);

        let (oy, h) = (0.0, 200.0);
        panel.scroll_rows(100.0, oy, h);
        assert_eq!(panel.scroll, panel.max_scroll(oy, h));
    }

    #[test]
    fn losing_hosts_pulls_the_list_back_into_view() {
        let mut panel = panel(20);
        let (oy, h) = (0.0, 400.0);
        panel.scroll_rows(20.0, oy, h);
        let scrolled = panel.scroll;
        assert!(scrolled > 0.0);

        // Every host but two is gone, so there is no longer anything to
        // scroll to — without the clamp the panel would sit empty.
        panel.set_items(items(2));
        panel.clamp_scroll(oy, h);
        assert_eq!(panel.scroll, 0.0);
        assert_eq!(panel.max_scroll(oy, h), 0.0);
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + 10.0, panel.item_rect(oy, 0).y + 2.0),
            Some(PanelHit::Item(0))
        );
    }

    #[test]
    fn a_shrinking_window_keeps_the_scroll_legal() {
        let mut panel = panel(20);
        let (oy, h) = (0.0, 400.0);
        panel.scroll_rows(20.0, oy, h);
        assert_eq!(panel.scroll, panel.max_scroll(oy, h));

        // A shorter window means a smaller viewport and therefore a
        // larger maximum, so the offset stays legal — but it must stay
        // inside the new bounds, which is what the clamp guarantees
        // before every paint.
        let short = 200.0;
        panel.clamp_scroll(oy, short);
        let max = panel.max_scroll(oy, short);
        assert!(panel.scroll <= max, "{} > {max}", panel.scroll);
        assert!(panel.scroll > 0.0);

        // And a viewport taller than the content pins the list back to
        // the top instead of leaving it scrolled into empty space.
        panel.clamp_scroll(oy, 4000.0);
        assert_eq!(panel.scroll, 0.0);
    }

    #[test]
    fn refreshing_keeps_the_selected_host_selected() {
        let mut panel = panel(3);
        panel.selected = Some(1);
        assert_eq!(panel.selected_id(), Some("id-1"));

        // A new host is prepended: index 1 is no longer the same host.
        let mut next = items(3);
        next.insert(
            0,
            HostItem {
                id: "new".to_string(),
                name: "new".to_string(),
                endpoint: "root@new".to_string(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Idle,
                nested: false,
                session_count: 0,
            },
        );
        panel.set_items(next);
        assert_eq!(panel.selected_id(), Some("id-1"));
        assert_eq!(panel.selected, Some(2));
        assert_eq!(panel.hover, None);
    }

    #[test]
    fn a_deleted_selection_is_dropped_rather_than_moved() {
        let mut panel = panel(3);
        panel.selected = Some(2);
        panel.set_items(items(1));
        assert_eq!(panel.selected, None);
        assert_eq!(panel.selected_id(), None);
    }

    #[test]
    fn last_session_gets_extra_gap_before_next_host() {
        let mut panel = HostPanel::default();
        panel.set_rows(vec![
            Row::Host(HostItem {
                id: "a".into(),
                name: "A".into(),
                endpoint: "a".into(),
                badge: Badge::Local,
                stored: false,
                os_id: None,
                status: HostStatus::Active,
                nested: false,
                session_count: 1,
            }),
            Row::Session(SessionItem {
                tab_index: 0,
                host_id: "a".into(),
                title: "shell".into(),
                active: true,
                closable: false,
            }),
            Row::Host(HostItem {
                id: "b".into(),
                name: "B".into(),
                endpoint: "b".into(),
                badge: Badge::Wsl,
                stored: false,
                os_id: None,
                status: HostStatus::Idle,
                nested: false,
                session_count: 0,
            }),
        ]);
        assert_eq!(panel.row_slot_height(1), SESSION_HEIGHT + SESSION_AFTER_GAP);
        let oy = 0.0;
        let session_bottom = panel.card_rect(oy, 1).bottom();
        let next_top = panel.card_rect(oy, 2).y;
        assert!(
            next_top - session_bottom >= SESSION_AFTER_GAP - 0.5,
            "gap {} < SESSION_AFTER_GAP",
            next_top - session_bottom
        );

        let (oy, h) = tall();
        // Breathing gap after the session is not a hit target.
        let gap_y = session_bottom + SESSION_AFTER_GAP * 0.5;
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + PAD_X + 10.0, gap_y),
            Some(PanelHit::Background)
        );
        let next = panel.card_rect(oy, 2);
        assert_eq!(
            panel.hit_test(oy, h, next.x + 10.0, next.y + next.height / 2.0),
            Some(PanelHit::Item(2))
        );
    }

    #[test]
    fn sessions_nest_under_hosts_and_collapse_with_them() {
        let mut panel = HostPanel::default();
        panel.set_rows(vec![
            Row::Host(HostItem {
                id: "local".to_string(),
                name: "This computer".to_string(),
                endpoint: "nixos".to_string(),
                badge: Badge::Local,
                stored: false,
                os_id: None,
                status: HostStatus::Idle,
                nested: false,
                session_count: 2,
            }),
            Row::Session(SessionItem {
                tab_index: 0,
                host_id: "local".to_string(),
                title: "This computer".to_string(),
                active: true,
                closable: false,
            }),
            Row::Session(SessionItem {
                tab_index: 1,
                host_id: "local".to_string(),
                title: "shell".to_string(),
                active: false,
                closable: true,
            }),
        ]);
        let (oy, h) = tall();
        assert_eq!(panel.visible_row_indices(), vec![0, 1, 2]);
        let session = panel.item_rect(oy, 1);
        assert_eq!(
            panel.hit_test(oy, h, session.x + 10.0, session.y + 4.0),
            Some(PanelHit::Session(1))
        );
        let close = panel.session_close_rect(oy, 2).expect("closable");
        assert_eq!(
            panel.hit_test(oy, h, close.x + 2.0, close.y + 2.0),
            Some(PanelHit::CloseSession(2))
        );
        panel.toggle_host_collapsed("local");
        assert_eq!(panel.visible_row_indices(), vec![0]);
        assert!(panel.select_session(1));
        assert_eq!(panel.selected_session, Some(1));
        assert_eq!(panel.visible_row_indices(), vec![0, 1, 2]);
    }

    #[test]
    fn following_a_vanished_host_clears_the_highlight() {
        let mut panel = panel(3);
        panel.follow_host("id-2");
        assert_eq!(panel.selected_id(), Some("id-2"));

        // The host behind the highlight is gone: the highlight goes with it
        // instead of staying on whichever row now sits at that index.
        panel.follow_host("id-9");
        assert_eq!(panel.selected, None);
        assert_eq!(panel.selected_id(), None);
    }

    #[test]
    fn the_error_banner_sits_at_the_bottom_without_pushing_rows() {
        let mut panel = panel(2);
        let (oy, h) = tall();
        let without = panel.item_rect(oy, 0).y;

        panel.error = Some("'x' is not a valid port".to_string());
        let with = panel.item_rect(oy, 0).y;
        assert_eq!(with, without, "sticky footer must not shift rows");

        let banner = panel.notice_rect(oy, h).expect("error banner");
        assert!(
            (banner.bottom() - (oy + h - NOTICE_MARGIN)).abs() < 0.01,
            "banner should hug the panel bottom"
        );
        assert!((banner.height - ERROR_BANNER_HEIGHT).abs() < 0.01);
        assert!(panel.body_rect(oy, h).bottom() <= banner.y + 0.01);

        let row = panel.item_rect(oy, 0);
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + 4.0, row.y + 2.0),
            Some(PanelHit::Item(0))
        );
    }

    #[test]
    fn the_notice_never_overlaps_the_add_row() {
        // A wide range of window heights: the sticky notice is anchored to the
        // bottom and the body must never extend under it.
        for height in [120.0, 200.0, 400.0, 1000.0] {
            let mut panel = panel(30);
            panel.notice = Some("Added web-01".to_string());
            let body = panel.body_rect(0.0, height);
            let footer = panel.footer_rect(0.0, height);
            assert!(body.bottom() <= footer.y, "height {height}");
            assert!(body.height >= 0.0, "height {height}");
            if let Some(notice) = panel.notice_rect(0.0, height) {
                assert!(body.bottom() <= notice.y + 0.01, "height {height}");
            }
        }
    }

    #[test]
    fn count_label_is_pluralised() {
        let mut panel = panel(0);
        assert_eq!(panel.count_label(), "empty");
        panel.set_items(items(1));
        assert_eq!(panel.count_label(), "1 host");
        panel.set_items(items(7));
        assert_eq!(panel.count_label(), "7 hosts");
    }

    /// Local section, platform rows, then a Hosts section.
    fn grouped() -> Vec<Row> {
        vec![
            Row::Section("Local".to_string()),
            Row::Host(HostItem {
                id: "local".to_string(),
                name: "This computer".to_string(),
                endpoint: "nixos@NixOS".to_string(),
                badge: Badge::Local,
                stored: false,
                os_id: Some("nixos".to_string()),
                status: HostStatus::Idle,
                nested: false,
                session_count: 0,
            }),
            Row::Host(HostItem {
                id: "wsl:Ubuntu-24.04".to_string(),
                name: "Ubuntu 24.04 LTS".to_string(),
                endpoint: "windows".to_string(),
                badge: Badge::Wsl,
                stored: false,
                os_id: None,
                status: HostStatus::Running,
                nested: false,
                session_count: 0,
            }),
            Row::Section("Hosts".to_string()),
            Row::Host(HostItem {
                id: "9c1e".to_string(),
                name: "web-01".to_string(),
                endpoint: "deploy@web-01:2222".to_string(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Idle,
                nested: false,
                session_count: 0,
            }),
            Row::Host(HostItem {
                id: "77ab".to_string(),
                name: "db-01".to_string(),
                endpoint: "deploy@db-01:22".to_string(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Active,
                nested: false,
                session_count: 0,
            }),
        ]
    }

    #[test]
    fn sections_take_their_own_height_and_are_not_targets() {
        let mut panel = HostPanel::default();
        panel.set_rows(grouped());
        let (oy, h) = tall();

        assert_eq!(
            panel.content_height(),
            CTA_TOP_GAP
                + CTA_HEIGHT
                + CTA_GAP
                + 2.0 * (SECTION_HEIGHT + SECTION_GAP)
                + 4.0 * (ITEM_HEIGHT + CARD_GAP)
        );
        // Local + two platform hosts + Hosts + two stored hosts.
        assert_eq!(panel.rows.len(), 6);
        assert_eq!(panel.host_count(), 2);
        assert_eq!(panel.count_label(), "2 hosts");

        // Rows stack in order: Local, local, wsl, Hosts, then stored hosts.
        assert_eq!(panel.item_rect(oy, 1).y, panel.item_rect(oy, 0).bottom());
        assert_eq!(
            panel.item_rect(oy, 0).height,
            SECTION_HEIGHT + SECTION_GAP,
            "a label row includes breathing room before the next card"
        );
        assert_eq!(
            panel.item_rect(oy, 4).y,
            panel.item_rect(oy, 3).bottom(),
            "the first stored host sits right under the Hosts label"
        );

        // A press on a label (left side) is background, not a host.
        let label = panel.item_rect(oy, 3);
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + 10.0, label.y + label.height / 2.0),
            Some(PanelHit::Background)
        );
        // The New group control sits on the Hosts header's trailing edge.
        let new_group = panel.new_group_button_rect(oy, h);
        assert_eq!(
            panel.hit_test(oy, h, new_group.x + 4.0, new_group.y + 4.0),
            Some(PanelHit::NewGroup)
        );
    }

    #[test]
    fn collapsing_a_group_hides_only_its_nested_hosts() {
        let mut panel = HostPanel::default();
        panel.set_rows(vec![
            Row::Section("Hosts".to_string()),
            Row::Group {
                id: "g1".to_string(),
                name: "jeremy".to_string(),
                host_count: 1,
                session_count: 0,
                collapsed: true,
            },
            // Ungrouped host that follows the collapsed group — must stay.
            Row::Host(HostItem {
                id: "solo".to_string(),
                name: "solo".to_string(),
                endpoint: "root@solo".to_string(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Idle,
                nested: false,
                session_count: 0,
            }),
        ]);
        let visible = panel.visible_row_indices();
        assert_eq!(visible, vec![0, 1, 2], "ungrouped hosts survive a collapse");

        panel.set_rows(vec![
            Row::Section("Hosts".to_string()),
            Row::Group {
                id: "g1".to_string(),
                name: "jeremy".to_string(),
                host_count: 1,
                session_count: 0,
                collapsed: false,
            },
            Row::Host(HostItem {
                id: "nested".to_string(),
                name: "nested".to_string(),
                endpoint: "root@n".to_string(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Idle,
                nested: true,
                session_count: 0,
            }),
            Row::Host(HostItem {
                id: "solo".to_string(),
                name: "solo".to_string(),
                endpoint: "root@solo".to_string(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Idle,
                nested: false,
                session_count: 0,
            }),
        ]);
        assert_eq!(panel.visible_row_indices(), vec![0, 1, 2, 3]);
        panel.collapsed_groups.insert("g1".to_string());
        assert_eq!(
            panel.visible_row_indices(),
            vec![0, 1, 3],
            "only the nested child disappears"
        );
    }

    #[test]
    fn group_tray_spans_header_and_nested_hosts() {
        let mut panel = HostPanel::default();
        panel.set_rows(vec![
            Row::Group {
                id: "g1".to_string(),
                name: "jeremy".to_string(),
                host_count: 1,
                session_count: 0,
                collapsed: false,
            },
            Row::Host(HostItem {
                id: "nested".to_string(),
                name: "jerem prod".to_string(),
                endpoint: "ubuntu@1".to_string(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Idle,
                nested: true,
                session_count: 0,
            }),
            Row::Host(HostItem {
                id: "solo".to_string(),
                name: "solo".to_string(),
                endpoint: "root@solo".to_string(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Idle,
                nested: false,
                session_count: 0,
            }),
        ]);
        let (oy, _) = tall();
        let tray = panel.group_tray_rect(oy, 0).expect("tray");
        let header = panel.card_rect(oy, 0);
        let nested = panel.card_rect(oy, 1);
        let solo = panel.card_rect(oy, 2);
        assert!((tray.y - header.y).abs() < 0.5);
        assert!(tray.bottom() >= nested.bottom() - 0.5);
        assert!(
            tray.bottom() < solo.y,
            "tray must not swallow the next ungrouped host"
        );
        // Nested host must keep CARD_PAD gutter on both sides of the tray.
        assert!(
            (nested.x - tray.x - CARD_PAD).abs() < 0.5,
            "left nest pad: nested.x={} tray.x={}",
            nested.x,
            tray.x
        );
        assert!(
            (tray.right() - nested.right() - CARD_PAD).abs() < 0.5,
            "right nest pad: tray.right={} nested.right={}",
            tray.right(),
            nested.right()
        );
        assert_eq!(panel.group_nested_host_indices(0), vec![1]);

        // Tray bottom + CARD_GAP must land on the next root card (same as
        // collapsed→collapsed spacing).
        let gap_after_open = solo.y - tray.bottom();
        assert!(
            (gap_after_open - CARD_GAP).abs() < 0.5,
            "open-group→next gap={gap_after_open}, want CARD_GAP={CARD_GAP}"
        );

        panel.collapsed_groups.insert("g1".to_string());
        let collapsed = panel.group_tray_rect(oy, 0).expect("collapsed tray");
        assert!((collapsed.height - header.height).abs() < 1.0);
        let solo_after = panel.card_rect(oy, 2);
        // solo is still index 2 in rows but may not be visible... wait when
        // collapsed, nested is hidden so solo is still at index 2 in rows,
        // visible indices change offset. card_rect uses offset_of which uses
        // visible rows — solo should move up.
        let gap_after_closed = solo_after.y - collapsed.bottom();
        assert!(
            (gap_after_closed - CARD_GAP).abs() < 0.5,
            "closed-group→next gap={gap_after_closed}, want CARD_GAP={CARD_GAP}"
        );
    }

    #[test]
    fn selection_survives_a_regrouping() {
        let mut panel = HostPanel::default();
        panel.set_rows(grouped());
        assert!(panel.select_id("wsl:Ubuntu-24.04"));
        assert_eq!(panel.selected, Some(2));
        assert_eq!(
            panel.selected_item().map(|item| item.name.as_str()),
            Some("Ubuntu 24.04 LTS")
        );

        // Same hosts, no sections any more.
        panel.set_rows(vec![Row::Host(HostItem {
            id: "wsl:Ubuntu-24.04".to_string(),
            name: "Ubuntu 24.04 LTS".to_string(),
            endpoint: "windows".to_string(),
            badge: Badge::Wsl,
            stored: false,
            os_id: None,
            status: HostStatus::Running,
            nested: false,
            session_count: 0,
        })]);
        assert_eq!(panel.selected, Some(0));

        // A host that is gone is not left selected.
        panel.set_rows(vec![]);
        assert_eq!(panel.selected, None);
    }

    #[test]
    fn new_group_form_expands_content_and_accepts_hits() {
        let mut panel = HostPanel::default();
        panel.set_rows(grouped());
        let (oy, h) = tall();
        let before = panel.content_height();
        panel.open_new_group_form();
        assert!(panel.new_group_drafting);
        assert!(panel.new_group_focused);
        assert!(panel.content_height() > before);

        let field = panel.new_group_field_rect(oy).expect("field");
        assert_eq!(
            panel.hit_test(oy, h, field.x + 4.0, field.y + 4.0),
            Some(PanelHit::NewGroupField)
        );
        let create = panel.new_group_create_rect(oy).expect("create");
        assert_eq!(
            panel.hit_test(oy, h, create.x + 4.0, create.y + 4.0),
            Some(PanelHit::NewGroupCreate)
        );
        panel.close_new_group_form();
        assert!(!panel.new_group_drafting);
        assert_eq!(panel.content_height(), before);
    }

    #[test]
    fn connecting_indicator_tracks_the_host_id() {
        let mut panel = HostPanel::default();
        panel.set_rows(grouped());
        panel.begin_connecting("wsl:Ubuntu-24.04");
        assert!(panel.is_connecting("wsl:Ubuntu-24.04"));
        assert!(!panel.is_connecting("local"));

        let index = panel.row_of_host("wsl:Ubuntu-24.04").unwrap();
        assert!(panel.connecting_center(0.0, index).is_some());
        assert!(panel.connecting_shimmer_track(0.0, index).is_some());

        // Survives a regrouping that keeps the host.
        panel.set_rows(grouped());
        assert!(panel.is_connecting("wsl:Ubuntu-24.04"));

        // Drops when the host vanishes.
        panel.set_rows(vec![]);
        assert_eq!(panel.connecting_id, None);
    }

    #[test]
    fn rename_draft_moves_caret_and_inserts_spaces() {
        let mut panel = HostPanel::default();
        panel.begin_rename("h1".into(), false, "web");
        let draft = panel.rename.as_mut().unwrap();
        assert_eq!(draft.caret, 3);
        assert!(draft.move_left(RenameMoveKind::Collapse, false));
        assert_eq!(draft.caret, 2);
        assert!(draft.insert(" ", 64));
        assert_eq!(draft.name, "we b");
        assert_eq!(draft.caret, 3);
        assert!(draft.insert("01", 64));
        assert_eq!(draft.name, "we 01b");
        assert_eq!(draft.prefix(), "we 01");
        assert!(draft.move_home(RenameMoveKind::Collapse));
        assert_eq!(draft.caret, 0);
        assert!(draft.move_end(RenameMoveKind::Collapse));
        assert_eq!(draft.caret, draft.name.chars().count());
        assert!(draft.backspace(false));
        assert_eq!(draft.name, "we 01");
    }

    #[test]
    fn rename_draft_shift_selects_and_ctrl_skips_words() {
        let mut panel = HostPanel::default();
        panel.begin_rename("h1".into(), false, "main-server box");
        let draft = panel.rename.as_mut().unwrap();
        draft.move_home(RenameMoveKind::Collapse);
        assert!(draft.move_right(RenameMoveKind::Extend, false));
        assert!(draft.move_right(RenameMoveKind::Extend, false));
        assert!(draft.move_right(RenameMoveKind::Extend, false));
        assert_eq!(draft.selection_range(), Some((0, 3)));
        assert_eq!(draft.selected_text(), "mai");
        assert!(draft.move_right(RenameMoveKind::Collapse, true));
        // Collapse to end of selection then word-jump would need two steps;
        // after collapse-by-right with selection, caret is at 3.
        assert_eq!(draft.caret, 3);
        assert!(draft.selection_range().is_none());
        assert!(draft.move_right(RenameMoveKind::Collapse, true));
        assert_eq!(draft.caret, 11); // after "main-server"
        assert!(draft.select_all());
        assert_eq!(draft.selection_range(), Some((0, draft.name.chars().count())));
        assert!(draft.insert("foo bar", 64));
        assert_eq!(draft.name, "foo bar");
        draft.move_end(RenameMoveKind::Collapse);
        assert!(draft.backspace(true));
        assert_eq!(draft.name, "foo ");
    }
}
