//! Sidebar machine list: sections ("This computer", one per group,
//! "Servers"), machine rows, the optional filter field, the scroll
//! viewport, drag & drop and inline rename.
//!
//! Layout follows the violet-ink shell (`App.dc.html`): the list runs
//! between the command bar and the Add server / Settings buttons owned by
//! [`crate::shell::sidebar`]; rows are Navigation server rows (44px, 2px
//! apart) and section / group headers are 12px labels.
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
use crate::text_field::TextDraft;

/// Sidebar width, in logical pixels.
pub const WIDTH: f32 = crate::shell::layout::SIDEBAR_WIDTH;
/// Title band font size.
pub const TITLE_FONT_SIZE: f32 = 12.0;
/// Row title font size.
pub const ROW_TITLE_FONT_SIZE: f32 = 13.0;
/// Secondary labels / endpoints font size.
pub const ROW_SUB_FONT_SIZE: f32 = 11.0;
/// Section labels font size.
pub const SECTION_LABEL_FONT_SIZE: f32 = 10.0;
/// Add-host label font size.
pub const ADD_LABEL_FONT_SIZE: f32 = 12.0;
/// Title band ("SERVERS & HOSTS") — mock `p-4` (~16px) with text-xs.
pub const TITLE_HEIGHT: f32 = 44.0;
/// Search field band under the title — mock `p-3` band.
pub const SEARCH_BAND_HEIGHT: f32 = 52.0;
/// Fixed chrome above the list: brand + command bar (see
/// [`crate::shell::sidebar::list_top`]).
pub const HEADER_HEIGHT: f32 = 116.0;
/// Filter field band, shown only while filtering.
pub const FILTER_BAND_HEIGHT: f32 = SEARCH_HEIGHT + 8.0;
/// Machine row height (Navigation server row).
pub const ITEM_HEIGHT: f32 = crate::components::navigation::server_row::HEIGHT;
/// Gap between machine rows.
pub const CARD_GAP: f32 = 2.0;
/// Space above a section / group header that is not the first row.
pub const SECTION_GAP: f32 = 16.0;
/// Section / group header label box: 15px line + 6px under it.
pub const SECTION_HEIGHT: f32 = 21.0;
/// Rendered section names.
pub const LOCAL_SECTION: &str = "This computer";
pub const SERVERS_SECTION: &str = "Servers";

/// Top of the 12px label inside a header card rect.
pub fn section_label_y(card: Rect) -> f32 {
    card.y + (SECTION_HEIGHT - 6.0 - 12.0) * 0.5
}

/// Painted separator line Y position below a host card.
pub fn host_item_separator_y(card_rect: Rect) -> f32 {
    card_rect.y + ITEM_HEIGHT - 1.0
}

/// Host badge rect.
pub fn host_badge_rect(card_rect: Rect) -> Rect {
    Rect::new(
        card_rect.x + CARD_PAD,
        card_rect.y + (ITEM_HEIGHT - HOST_BADGE_TILE) / 2.0,
        HOST_BADGE_TILE,
        HOST_BADGE_TILE,
    )
}
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
/// Footer reserved height (the Add server / Settings block lives in the
/// shell; the list stops above it).
pub const FOOTER_HEIGHT: f32 = 0.0;
/// Inline "New group" form height under the Hosts section header.
pub const NEW_GROUP_FORM_HEIGHT: f32 = 40.0;
/// Approximate width of the "New group" text action on the Servers header.
pub const NEW_GROUP_BUTTON_WIDTH: f32 = 72.0;
/// Alias kept for callers that shared the dual-action header width.
pub const HOSTS_HEADER_ACTION_WIDTH: f32 = NEW_GROUP_BUTTON_WIDTH;
/// Gap above the empty-list hint.
/// Machine-list scroll thumb: shortest length, width, inset from the
/// sidebar's right edge (inside its 12 px padding).
pub const SCROLL_THUMB_MIN: f32 = 24.0;
pub const SCROLL_THUMB_WIDTH: f32 = 3.0;
pub const SCROLL_THUMB_INSET: f32 = 4.0;
/// Host drag auto-scroll: edge band height and top speed (px/s).
pub const DRAG_AUTOSCROLL_ZONE: f32 = 36.0;
pub const DRAG_AUTOSCROLL_SPEED: f32 = 420.0;
pub const EMPTY_HINT_GAP: f32 = 6.0;
/// Empty-list hint height: a title and up to two wrapped lines.
pub const EMPTY_HINT_HEIGHT: f32 = 56.0;

/// Guidance shown in place of a list with nothing in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyHint {
    pub title: &'static str,
    pub body: String,
}

/// Sticky success notice height at the bottom of the panel.
pub const NOTICE_HEIGHT: f32 = 40.0;
/// Extra notice height per wrapped line beyond the first.
pub const NOTICE_LINE_STEP: f32 = 15.0;
/// Characters that fit on one notice line (monospace hint text).
pub const NOTICE_LINE_CHARS: usize = 36;
/// Longest a notice grows; the painter elides past this.
pub const NOTICE_MAX_LINES: usize = 3;
/// Sticky error banner height (two lines of wrapped text).
pub const ERROR_BANNER_HEIGHT: f32 = 64.0;
/// Inset of the sticky notice/error card from the panel edges.
pub const NOTICE_MARGIN: f32 = 10.0;
/// Horizontal inset of rows inside the sidebar (`aside` padding).
pub const PAD_X: f32 = crate::shell::sidebar::PAD_X;
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
/// Left edge of the panel (the sidebar starts at the window edge).
pub const ORIGIN_X: f32 = 0.0;

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
            // Gap above + label box (the first visible header drops the gap,
            // see [`HostPanel::row_slot_height`]).
            Row::Section(_) | Row::Group { .. } => SECTION_GAP + SECTION_HEIGHT,
            Row::Host(_) => ITEM_HEIGHT + CARD_GAP,
            // Mid-sibling default; [`HostPanel::row_slot_height`] widens after
            // the last session under a host.
            Row::Session(_) => SESSION_HEIGHT + SESSION_GAP,
        }
    }

    /// Painted card height inside the row slot (excludes the trailing gap).
    pub fn card_height(&self) -> f32 {
        match self {
            Row::Section(_) | Row::Group { .. } => SECTION_HEIGHT,
            Row::Host(_) => ITEM_HEIGHT,
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

/// Inline rename of a host or group from the context menu. The text is the
/// shared [`TextDraft`], so it edits exactly like every other field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameDraft {
    pub id: String,
    pub is_group: bool,
    pub text: TextDraft,
    pub focused: bool,
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
    pub filter: TextDraft,
    /// Whether the search field has keyboard focus.
    pub filter_focused: bool,
    /// Whether the pointer is over the new-group header button.
    pub new_group_hover: bool,
    /// Inline form under Hosts for naming a new group.
    pub new_group_drafting: bool,
    /// Draft name while [`Self::new_group_drafting`] is true.
    pub new_group_name: TextDraft,
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
    /// Host ids whose sessions are currently starting (`wsl:…`, ssh id, …).
    pub connecting_ids: Vec<String>,
    /// Transient success message ("Added web-01"), cleared by the caller.
    pub notice: Option<String>,
    pub error: Option<String>,
    /// Armed / active host→group drag, when the pointer is down on a stored host.
    pub host_drag: Option<HostDrag>,
    /// Legacy per-row "+" / chevron controls (off: sessions live in the
    /// pills row). Kept for the session-row model and its tests.
    pub row_controls: bool,
}

impl HostPanel {
    /// The panel's own box.
    pub fn rect(&self, origin_y: f32, height: f32) -> Rect {
        Rect::new(ORIGIN_X, origin_y, WIDTH, height)
    }

    /// Fixed chrome above the list (brand + command bar, painted by the
    /// shell).
    pub fn header_rect(&self, origin_y: f32) -> Rect {
        Rect::new(ORIGIN_X, origin_y, WIDTH, HEADER_HEIGHT)
    }

    /// Whether the filter field is on screen: while focused or non-empty.
    pub fn filter_visible(&self) -> bool {
        self.filter_focused || !self.filter.value.is_empty()
    }

    /// Title-only band ("SERVERS & HOSTS").
    pub fn title_rect(&self, origin_y: f32) -> Rect {
        self.header_rect(origin_y)
    }

    /// Filter band at the top of the list (zero height when hidden).
    pub fn search_band_rect(&self, origin_y: f32) -> Rect {
        let h = if self.filter_visible() {
            FILTER_BAND_HEIGHT
        } else {
            0.0
        };
        Rect::new(ORIGIN_X, origin_y + HEADER_HEIGHT, WIDTH, h)
    }

    /// The filter field (empty rect when hidden).
    pub fn search_rect(&self, origin_y: f32) -> Rect {
        if !self.filter_visible() {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        }
        let band = self.search_band_rect(origin_y);
        Rect::new(
            band.x + PAD_X,
            band.y,
            band.width - 2.0 * PAD_X,
            SEARCH_HEIGHT,
        )
    }

    /// Height reserved for a sticky notice/error card, including margin.
    pub fn notice_reserve(&self) -> f32 {
        if self.error.is_some() {
            ERROR_BANNER_HEIGHT + NOTICE_MARGIN
        } else if self.notice.is_some() {
            self.notice_height() + NOTICE_MARGIN
        } else {
            0.0
        }
    }

    /// Lines the success notice wraps to (1..=NOTICE_MAX_LINES).
    pub fn notice_lines(&self) -> usize {
        let chars = self.notice.as_deref().map_or(0, |n| n.chars().count());
        chars.div_ceil(NOTICE_LINE_CHARS).clamp(1, NOTICE_MAX_LINES)
    }

    fn notice_height(&self) -> f32 {
        NOTICE_HEIGHT + (self.notice_lines() - 1) as f32 * NOTICE_LINE_STEP
    }

    /// Sticky notice/error card anchored to the bottom of the panel.
    pub fn notice_rect(&self, origin_y: f32, height: f32) -> Option<Rect> {
        let message = self.error.as_ref().or(self.notice.as_ref())?;
        debug_assert!(!message.is_empty());
        let banner_h = if self.error.is_some() {
            ERROR_BANNER_HEIGHT
        } else {
            self.notice_height()
        };
        // Never climb into the sticky header when the panel is shorter than
        // header + notice (tiny windows / tests).
        let y = (self.list_bottom(origin_y, height) - banner_h)
            .max(self.content_top(origin_y));
        Some(Rect::new(
            ORIGIN_X + PAD_X,
            y,
            WIDTH - 2.0 * PAD_X,
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

    /// Bottom of the scroll viewport, above the Add server / Settings block.
    pub fn list_bottom(&self, origin_y: f32, height: f32) -> f32 {
        crate::shell::sidebar::list_bottom(origin_y + height)
            .max(self.content_top(origin_y))
    }

    /// Everything under the list: notice card (when any) and the shell's
    /// bottom buttons.
    pub fn footer_rect(&self, origin_y: f32, height: f32) -> Rect {
        let top = (self.list_bottom(origin_y, height) - self.notice_reserve())
            .max(self.content_top(origin_y));
        Rect::new(ORIGIN_X, top, WIDTH, (origin_y + height - top).max(0.0))
    }

    /// The "Add server" button (bottom of the sidebar).
    pub fn add_button_rect(&self, origin_y: f32, height: f32) -> Rect {
        crate::shell::sidebar::add_server_rect(origin_y + height)
    }

    /// Index of the `Servers` section row (ungrouped stored hosts), if
    /// present. `Hosts` is accepted for older row sets.
    pub fn hosts_section_index(&self) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row.label(), Some(SERVERS_SECTION) | Some("Hosts")))
    }

    /// First row of the user's stored machines: the first group header,
    /// or the Servers section when there is no group before it.
    pub fn stored_area_index(&self) -> Option<usize> {
        let first_group = self
            .rows
            .iter()
            .position(|row| matches!(row, Row::Group { .. }));
        match (first_group, self.hosts_section_index()) {
            (Some(g), Some(h)) => Some(g.min(h)),
            (g, h) => g.or(h),
        }
    }

    /// Retired dual-header "+ New host" — use [`Self::add_button_rect`].
    pub fn new_host_button_rect(&self, _origin_y: f32, _height: f32) -> Rect {
        Rect::new(0.0, 0.0, 0.0, 0.0)
    }

    /// "+ New group" control on the right of the Hosts section label.
    pub fn new_group_button_rect(&self, origin_y: f32, height: f32) -> Rect {
        let Some(index) = self.hosts_section_index() else {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        };
        // Scrolled out of the viewport: not drawn, so not clickable.
        if !self.visible_row_indices().contains(&index)
            || !self.row_painted(origin_y, height, index)
        {
            return Rect::new(0.0, 0.0, 0.0, 0.0);
        }
        let header = self.card_rect(origin_y, index);
        let w = NEW_GROUP_BUTTON_WIDTH;
        Rect::new(header.right() - w, header.y, w, SECTION_HEIGHT)
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
        let section = self.card_rect(origin_y, index);
        Some(Rect::new(
            ORIGIN_X + PAD_X,
            section.bottom() + 2.0,
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
        let mut text = TextDraft::new(current_name);
        // Whole name selected: typing replaces it, arrows keep it.
        text.select_all();
        self.rename = Some(RenameDraft {
            id,
            is_group,
            text,
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

    /// Whether this host id is one of those currently connecting.
    pub fn is_connecting(&self, id: &str) -> bool {
        self.connecting_ids
            .iter()
            .any(|connecting| connecting == id)
    }

    /// Whether any host is connecting.
    pub fn any_connecting(&self) -> bool {
        !self.connecting_ids.is_empty()
    }

    /// Start the connecting indicator for `id`, next to any already running.
    pub fn begin_connecting(&mut self, id: impl Into<String>) {
        let id = id.into();
        if !self.is_connecting(&id) {
            self.connecting_ids.push(id);
        }
        self.error = None;
    }

    /// Drop the connecting indicator of `id`.
    pub fn end_connecting(&mut self, id: &str) {
        self.connecting_ids.retain(|connecting| connecting != id);
    }

    /// Drop every connecting indicator.
    pub fn end_all_connecting(&mut self) {
        self.connecting_ids.clear();
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
        // Headers sit at the bottom of their slot, under the top gap.
        if matches!(
            self.rows.get(index),
            Some(Row::Section(_) | Row::Group { .. })
        ) {
            return Rect::new(slot.x, slot.bottom() - h, slot.width, h);
        }
        let (nest_left, nest_right) = match self.rows.get(index) {
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
        // Sessions moved to the pills row; rows carry no "+" any more.
        if !self.row_controls {
            return None;
        }
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
        if !self.row_controls {
            return None;
        }
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
        let _ = has_children;
        let pad_bottom = 0.0;
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
                    if let Some(Row::Group { id, host_count, .. }) = self.rows.get(index)
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
    fn root_reorder_cards(
        &self,
        origin_y: f32,
    ) -> Vec<(crate::geom::Rect, HostDropTarget)> {
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
                    self.card_rect(origin_y, hosts_idx).bottom() + 4.0
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
        let Some(first) = self.stored_area_index() else {
            return false;
        };
        let body = self.body_rect(origin_y, height);
        if y < body.y || y > body.bottom() {
            return false;
        }
        let hosts_top = self.item_rect(origin_y, first).y;
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
            Row::Section(_) | Row::Group { .. } => {
                let first = self.visible_row_indices().first() == Some(&index);
                if first {
                    SECTION_HEIGHT
                } else {
                    SECTION_GAP + SECTION_HEIGHT
                }
            }
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
            _ => row.height(),
        }
    }

    /// Unscrolled top of the scrollable content (below the sticky header).
    fn content_top(&self, origin_y: f32) -> f32 {
        origin_y + HEADER_HEIGHT + self.search_band_rect(origin_y).height
    }

    /// Row 0's unscrolled top edge.
    fn rows_top(&self, origin_y: f32) -> f32 {
        self.content_top(origin_y)
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
        let filter_lower = self.filter.value.trim().to_ascii_lowercase();
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
                            // A group header starts its own section.
                            Row::Section(_) | Row::Group { .. } => break,
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

    /// Row of the stored SSH host just saved: its name and endpoint when
    /// both match (names and endpoints can each repeat), else its endpoint.
    pub fn find_saved_host(&self, name: &str, endpoint: &str) -> Option<usize> {
        let ssh = |row: &Row| {
            row.host()
                .filter(|item| item.badge == Badge::Ssh && item.endpoint == endpoint)
                .map(|item| item.name == name)
        };
        self.rows
            .iter()
            .position(|row| ssh(row) == Some(true))
            .or_else(|| self.rows.iter().position(|row| ssh(row).is_some()))
    }

    /// What to say when the list shows no host of the user's: none saved
    /// yet, or none matching the filter.
    pub fn empty_hint(&self) -> Option<EmptyHint> {
        let filter = self.filter.value.trim();
        if !filter.is_empty() {
            let any_host = self.visible_row_indices().into_iter().any(|i| {
                self.rows
                    .get(i)
                    .is_some_and(|row| matches!(row, Row::Host(_) | Row::Group { .. }))
            });
            return (!any_host).then(|| EmptyHint {
                title: "No matches",
                body: format!(
                    "Nothing matches \u{201c}{filter}\u{201d}. Press Esc to clear."
                ),
            });
        }
        (self.host_count() == 0).then(|| EmptyHint {
            title: "No saved hosts yet",
            body: "Click Add server below and paste user@host to connect.".to_string(),
        })
    }

    /// Where [`Self::empty_hint`] goes: under the last visible row.
    pub fn empty_hint_rect(&self, origin_y: f32, height: f32) -> Option<Rect> {
        self.empty_hint()?;
        let body = self.body_rect(origin_y, height);
        Some(Rect::new(
            body.x + PAD_X,
            body.y + self.content_height() - self.scroll + EMPTY_HINT_GAP,
            body.width - 2.0 * PAD_X,
            EMPTY_HINT_HEIGHT,
        ))
    }

    /// Esc in the filter: clear it first, leave the field on the next one,
    /// so hosts are never left hidden behind a filter nobody sees.
    pub fn escape_filter(&mut self) {
        if self.filter.value.is_empty() {
            self.filter_focused = false;
        } else {
            self.filter.clear();
            self.scroll = 0.0;
        }
    }

    /// Total height of the visible rows, ignoring the viewport.
    pub fn content_height(&self) -> f32 {
        let hosts_idx = self.hosts_section_index();
        let form_extra = self.new_group_form_slot_height();
        self.visible_row_indices()
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
            .map(|row| match row {
                Row::Host(host) if host.stored => 1,
                // A collapsed group lists no host rows, only their number.
                Row::Group {
                    host_count,
                    collapsed: true,
                    ..
                } => *host_count,
                _ => 0,
            })
            .sum()
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

    /// Whether row `index` is drawn: its card lies wholly inside the
    /// scroll viewport. UI text cannot be clipped, so a row the viewport
    /// cuts is skipped — and, to keep hit-testing in sync with the
    /// pixels, its visible sliver is not clickable either.
    pub fn row_painted(&self, origin_y: f32, height: f32, index: usize) -> bool {
        let body = self.body_rect(origin_y, height);
        let card = self.card_rect(origin_y, index);
        card.y >= body.y - 0.5 && card.bottom() <= body.bottom() + 0.5
    }

    /// The scroll affordance: a thin thumb on the sidebar's right edge,
    /// sized and placed like a scrollbar over the viewport. `None` while
    /// the whole list fits.
    pub fn scroll_thumb(&self, origin_y: f32, height: f32) -> Option<Rect> {
        let max = self.max_scroll(origin_y, height);
        if max <= 0.5 {
            return None;
        }
        let body = self.body_rect(origin_y, height);
        let content = self.content_height().max(1.0);
        let thumb_h = (body.height * body.height / content)
            .clamp(SCROLL_THUMB_MIN.min(body.height), body.height);
        let progress = (self.scroll / max).clamp(0.0, 1.0);
        Some(Rect::new(
            ORIGIN_X + WIDTH - SCROLL_THUMB_INSET - SCROLL_THUMB_WIDTH,
            body.y + (body.height - thumb_h) * progress,
            SCROLL_THUMB_WIDTH,
            thumb_h,
        ))
    }

    /// Scroll the least needed for row `index` to be fully on screen.
    /// Returns whether the scroll changed.
    pub fn reveal_row(&mut self, index: usize, origin_y: f32, height: f32) -> bool {
        if index >= self.rows.len() || self.row_painted(origin_y, height, index) {
            return false;
        }
        let body = self.body_rect(origin_y, height);
        let card = self.card_rect(origin_y, index);
        let before = self.scroll;
        if card.y < body.y {
            self.scroll -= body.y - card.y;
        } else if card.bottom() > body.bottom() {
            self.scroll += card.bottom() - body.bottom();
        }
        self.clamp_scroll(origin_y, height);
        (self.scroll - before).abs() > f32::EPSILON
    }

    /// Auto-scroll speed (px/s, negative = up) for a host dragged at
    /// pointer `y`: zero away from the viewport's edges, faster nearer
    /// them, and zero when there is nothing to scroll.
    pub fn drag_autoscroll_speed(&self, origin_y: f32, height: f32, y: f32) -> f32 {
        if self.max_scroll(origin_y, height) <= 0.0 {
            return 0.0;
        }
        let body = self.body_rect(origin_y, height);
        let zone = DRAG_AUTOSCROLL_ZONE.min(body.height / 3.0);
        if zone <= 0.0 {
            return 0.0;
        }
        let into_top = body.y + zone - y;
        let into_bottom = y - (body.bottom() - zone);
        if into_bottom > 0.0 {
            DRAG_AUTOSCROLL_SPEED * (into_bottom / zone).min(1.0)
        } else if into_top > 0.0 {
            -DRAG_AUTOSCROLL_SPEED * (into_top / zone).min(1.0)
        } else {
            0.0
        }
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
        self.rows = rows;
        self.selected = keep.as_deref().and_then(|id| self.row_of_host(id));
        self.selected_session = keep_session.filter(|&tab| {
            self.rows
                .iter()
                .any(|r| r.session().is_some_and(|s| s.tab_index == tab))
        });
        self.hover = None;
        let mut connecting = std::mem::take(&mut self.connecting_ids);
        connecting.retain(|id| self.row_of_host(id).is_some());
        self.connecting_ids = connecting;
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

    /// The visible row whose painted card holds `(x, y)`, inside the
    /// scroll viewport.
    fn row_at(&self, origin_y: f32, height: f32, x: f32, y: f32) -> Option<usize> {
        let body = self.body_rect(origin_y, height);
        if !body.contains(x, y) {
            return None;
        }
        self.visible_row_indices().into_iter().find(|&index| {
            self.card_rect(origin_y, index).contains(x, y)
                && self.row_painted(origin_y, height, index)
        })
    }

    /// Which host row is under `(x, y)`, if any.
    fn item_at(&self, origin_y: f32, height: f32, x: f32, y: f32) -> Option<usize> {
        self.row_at(origin_y, height, x, y)
            .filter(|&i| self.rows[i].host().is_some())
    }

    fn session_at(&self, origin_y: f32, height: f32, x: f32, y: f32) -> Option<usize> {
        self.row_at(origin_y, height, x, y)
            .filter(|&i| self.rows[i].session().is_some())
    }

    /// Which group header row is under `(x, y)`, if any.
    fn group_at(&self, origin_y: f32, height: f32, x: f32, y: f32) -> Option<usize> {
        self.row_at(origin_y, height, x, y)
            .filter(|&i| self.rows[i].group().is_some())
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
    use crate::text_field::TextMoveKind;

    #[test]
    fn section_label_and_geometry_helpers_are_consistent() {
        let (_oy, _) = tall();
        let rect = Rect::new(0.0, 100.0, 100.0, SECTION_HEIGHT);
        // 12px label centred in the 15px line above the 6px gap.
        assert_eq!(section_label_y(rect), 100.0 + 1.5);

        let card = Rect::new(0.0, 100.0, 100.0, ITEM_HEIGHT);
        assert_eq!(host_item_separator_y(card), 100.0 + 44.0 - 1.0);

        let badge = host_badge_rect(card);
        assert_eq!(badge.y, 100.0 + (44.0 - 28.0) / 2.0);
    }

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
    fn layout_constants_match_the_violet_ink_shell() {
        assert_eq!(WIDTH, 260.0);
        assert_eq!(ORIGIN_X, 0.0);
        assert_eq!(PAD_X, 12.0);
        assert_eq!(ITEM_HEIGHT, 44.0);
        assert_eq!(CARD_GAP, 2.0);
        assert_eq!(SECTION_GAP, 16.0);
        assert_eq!(SECTION_HEIGHT, 21.0);
        assert_eq!(HEADER_HEIGHT, crate::shell::sidebar::list_top());
        assert_eq!(HOST_BADGE_TILE, 28.0);
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
            panel.hit_test(
                oy,
                h,
                ORIGIN_X + PAD_X + 10.0,
                panel.item_rect(oy, 0).y + 2.0
            ),
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
        let session = panel.card_rect(oy, 1);
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
            (banner.bottom() - panel.list_bottom(oy, h)).abs() < 0.01,
            "banner should hug the bottom of the list"
        );
        assert!((banner.height - ERROR_BANNER_HEIGHT).abs() < 0.01);
        assert!(panel.body_rect(oy, h).bottom() <= banner.y + 0.01);

        let row = panel.item_rect(oy, 0);
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + PAD_X + 4.0, row.y + 2.0),
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
    fn a_long_notice_grows_to_fit_up_to_three_lines() {
        let mut panel = panel(3);
        panel.notice = Some("Added web-01".to_string());
        assert_eq!(panel.notice_lines(), 1);
        let short = panel.notice_rect(0.0, 800.0).expect("notice");
        assert!((short.height - NOTICE_HEIGHT).abs() < 0.01);

        panel.notice = Some("Terminus 9.9.9 is installed. Restart to update".to_string());
        assert_eq!(panel.notice_lines(), 2);
        let two = panel.notice_rect(0.0, 800.0).expect("notice");
        assert!((two.height - (NOTICE_HEIGHT + NOTICE_LINE_STEP)).abs() < 0.01);
        assert!(
            (two.bottom() - short.bottom()).abs() < 0.01,
            "stays anchored"
        );
        assert!(panel.body_rect(0.0, 800.0).bottom() <= two.y + 0.01);

        panel.notice = Some("x ".repeat(200));
        assert_eq!(panel.notice_lines(), 3, "capped");
    }

    #[test]
    fn an_empty_list_says_how_to_add_a_host() {
        let empty = panel(0);
        let hint = empty.empty_hint().expect("hint");
        assert_eq!(hint.title, "No saved hosts yet");
        assert!(hint.body.contains("Add server"), "{}", hint.body);
        assert!(panel(2).empty_hint().is_none());
    }

    #[test]
    fn hosts_in_a_collapsed_group_still_count() {
        // Collapsed groups carry their hosts in `host_count`, not as rows.
        let mut panel = HostPanel::default();
        panel.set_rows(vec![
            Row::Section("Hosts".into()),
            Row::Group {
                id: "g1".into(),
                name: "prod".into(),
                host_count: 3,
                session_count: 0,
                collapsed: true,
            },
        ]);
        assert_eq!(panel.host_count(), 3);
        assert_eq!(panel.count_label(), "3 hosts");
        assert!(panel.empty_hint().is_none(), "not 'No saved hosts yet'");
        // Filtering on the group's name shows the group: not "No matches".
        panel.filter = TextDraft::new("prod");
        assert!(panel.empty_hint().is_none());
    }

    #[test]
    fn a_filter_with_no_match_says_so_and_how_to_clear_it() {
        let mut panel = panel(2);
        panel.filter = TextDraft::new("zzz");
        let hint = panel.empty_hint().expect("hint");
        assert_eq!(hint.title, "No matches");
        assert!(
            hint.body.contains("zzz") && hint.body.contains("Esc"),
            "{}",
            hint.body
        );
        panel.filter = TextDraft::new("host-1");
        assert!(panel.empty_hint().is_none(), "a match hides the hint");
    }

    #[test]
    fn the_hint_sits_below_the_list() {
        let (oy, h) = tall();
        let panel = panel(0);
        let rect = panel.empty_hint_rect(oy, h).expect("rect");
        let body = panel.body_rect(oy, h);
        assert!(rect.y >= body.y + panel.content_height() - 0.01);
        assert!(rect.x >= body.x && rect.right() <= body.right() + 0.01);
    }

    #[test]
    fn escape_clears_the_filter_before_leaving_it() {
        let mut panel = panel(2);
        panel.filter = TextDraft::new("zzz");
        panel.filter_focused = true;
        panel.escape_filter();
        assert_eq!(panel.filter.value, "");
        assert!(panel.filter_focused, "first Esc only clears");
        panel.escape_filter();
        assert!(!panel.filter_focused);
    }

    #[test]
    fn a_saved_host_is_found_by_name_and_endpoint() {
        let mut hosts = items(2);
        // Same endpoint, different names (e.g. one per key).
        hosts[1].endpoint = hosts[0].endpoint.clone();
        let panel = {
            let mut p = HostPanel::default();
            p.set_items(hosts);
            p
        };
        let second = panel
            .find_saved_host("host-1", "deploy@host-0:2222")
            .expect("row");
        assert_eq!(panel.rows[second].host().unwrap().id, "id-1");
        let first = panel
            .find_saved_host("renamed", "deploy@host-0:2222")
            .expect("row");
        assert_eq!(
            panel.rows[first].host().unwrap().id,
            "id-0",
            "endpoint fallback"
        );
        assert_eq!(panel.find_saved_host("x", "nobody@nowhere"), None);
    }

    #[test]
    fn renaming_starts_with_the_old_name_selected() {
        let mut panel = panel(1);
        panel.begin_rename("id-0".into(), false, "old-name");
        let draft = panel.rename.as_mut().unwrap();
        assert_eq!(draft.text.selection_range(), Some((0, 8)));
        draft.text.insert("web", 64, false);
        assert_eq!(draft.text.value, "web", "typing replaces the old name");
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

        // First header drops its top gap.
        assert_eq!(
            panel.content_height(),
            SECTION_HEIGHT
                + (SECTION_GAP + SECTION_HEIGHT)
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
            SECTION_HEIGHT,
            "first header"
        );
        assert_eq!(
            panel.item_rect(oy, 3).height,
            SECTION_GAP + SECTION_HEIGHT,
            "later headers carry the gap above them"
        );
        assert_eq!(
            panel.item_rect(oy, 4).y,
            panel.item_rect(oy, 3).bottom(),
            "the first stored host sits right under the Hosts label"
        );

        // A press on a label (left side) is background, not a host.
        let label = panel.card_rect(oy, 3);
        assert_eq!(
            panel.hit_test(oy, h, ORIGIN_X + PAD_X + 10.0, label.y + label.height / 2.0),
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
        // Grouped hosts line up with every other row (no indent).
        assert_eq!(nested.x, tray.x);
        assert_eq!(nested.right(), tray.right());
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
        // A collapsed group is just its header label; the next row
        // follows straight under it, like rows under a section label.
        assert!(
            gap_after_closed.abs() < 0.5,
            "closed-group→next gap={gap_after_closed}"
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
        assert!(!panel.any_connecting());
    }

    #[test]
    fn several_hosts_can_connect_at_once() {
        let mut panel = HostPanel::default();
        panel.set_rows(grouped());
        panel.begin_connecting("wsl:Ubuntu-24.04");
        panel.begin_connecting("local");
        panel.begin_connecting("local");
        assert!(panel.is_connecting("wsl:Ubuntu-24.04") && panel.is_connecting("local"));

        panel.end_connecting("local");
        assert!(panel.is_connecting("wsl:Ubuntu-24.04") && !panel.is_connecting("local"));

        panel.end_all_connecting();
        assert!(!panel.any_connecting());
    }

    #[test]
    fn rename_draft_moves_caret_and_inserts_spaces() {
        let mut panel = HostPanel::default();
        panel.begin_rename("h1".into(), false, "web");
        let draft = panel.rename.as_mut().unwrap();
        assert_eq!(draft.text.caret, 3);
        // End drops the initial select-all, caret stays at the end.
        draft.text.move_end(TextMoveKind::Collapse);
        assert_eq!(draft.text.selection_range(), None);
        assert!(draft.text.move_left(TextMoveKind::Collapse, false));
        assert_eq!(draft.text.caret, 2);
        assert!(draft.text.insert(" ", 64, false));
        assert_eq!(draft.text.value, "we b");
        assert_eq!(draft.text.caret, 3);
        assert!(draft.text.insert("01", 64, false));
        assert_eq!(draft.text.value, "we 01b");
        assert_eq!(draft.text.prefix(), "we 01");
        assert!(draft.text.move_home(TextMoveKind::Collapse));
        assert_eq!(draft.text.caret, 0);
        assert!(draft.text.move_end(TextMoveKind::Collapse));
        assert_eq!(draft.text.caret, draft.text.value.chars().count());
        assert!(draft.text.backspace(false));
        assert_eq!(draft.text.value, "we 01");
    }

    #[test]
    fn rename_draft_shift_selects_and_ctrl_skips_words() {
        let mut panel = HostPanel::default();
        panel.begin_rename("h1".into(), false, "main-server box");
        let draft = panel.rename.as_mut().unwrap();
        draft.text.move_home(TextMoveKind::Collapse);
        assert!(draft.text.move_right(TextMoveKind::Extend, false));
        assert!(draft.text.move_right(TextMoveKind::Extend, false));
        assert!(draft.text.move_right(TextMoveKind::Extend, false));
        assert_eq!(draft.text.selection_range(), Some((0, 3)));
        assert!(draft.text.move_right(TextMoveKind::Collapse, true));
        // Collapse to end of selection then word-jump would need two steps;
        // after collapse-by-right with selection, caret is at 3.
        assert_eq!(draft.text.caret, 3);
        assert!(draft.text.selection_range().is_none());
        assert!(draft.text.move_right(TextMoveKind::Collapse, true));
        assert_eq!(draft.text.caret, 4); // after "main" (shared word rules)
        assert!(draft.text.select_all());
        assert_eq!(
            draft.text.selection_range(),
            Some((0, draft.text.value.chars().count()))
        );
        assert!(draft.text.insert("foo bar", 64, false));
        assert_eq!(draft.text.value, "foo bar");
        draft.text.move_end(TextMoveKind::Collapse);
        assert!(draft.text.backspace(true));
        assert_eq!(draft.text.value, "foo ");
    }

    // ---- intent slice --------------------------------------------------

    fn panel_with_sessions() -> HostPanel {
        let mut p = HostPanel::default();
        p.set_rows(vec![
            Row::Host(HostItem {
                id: "host-a".into(),
                name: "Host A".into(),
                endpoint: "a@host-a".into(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Active,
                nested: false,
                session_count: 2,
            }),
            Row::Session(SessionItem {
                tab_index: 0,
                host_id: "host-a".into(),
                title: "shell".into(),
                active: true,
                closable: false,
            }),
            Row::Session(SessionItem {
                tab_index: 1,
                host_id: "host-a".into(),
                title: "vim".into(),
                active: false,
                closable: true,
            }),
        ]);
        p.row_controls = true;
        p
    }

    fn panel_with_group() -> HostPanel {
        let mut p = HostPanel::default();
        p.set_rows(vec![
            Row::Group {
                id: "grp-1".into(),
                name: "Production".into(),
                host_count: 1,
                session_count: 0,
                collapsed: false,
            },
            Row::Host(HostItem {
                id: "nested-host".into(),
                name: "web-01".into(),
                endpoint: "deploy@web-01".into(),
                badge: Badge::Ssh,
                stored: true,
                os_id: None,
                status: HostStatus::Idle,
                nested: true,
                session_count: 0,
            }),
        ]);
        p
    }

    #[test]
    fn toggle_host_expansion_toggles_collapsed_hosts() {
        let mut panel = panel_with_sessions();
        assert!(!panel.collapsed_hosts.contains("host-a"));
        assert_eq!(panel.visible_row_indices(), vec![0, 1, 2]);

        // First toggle: collapses session rows.
        panel.toggle_host_collapsed("host-a");
        assert!(panel.collapsed_hosts.contains("host-a"));
        assert_eq!(panel.visible_row_indices(), vec![0], "sessions hidden");

        // Second toggle: expands them back.
        panel.toggle_host_collapsed("host-a");
        assert!(!panel.collapsed_hosts.contains("host-a"));
        assert_eq!(
            panel.visible_row_indices(),
            vec![0, 1, 2],
            "sessions restored"
        );
    }

    #[test]
    fn toggle_group_collapses_and_expands() {
        let mut panel = panel_with_group();
        // Group visible with its nested child: indices 0 (group) and 1 (nested host).
        assert_eq!(panel.visible_row_indices(), vec![0, 1]);

        // Collapse: nested host disappears.
        panel.collapsed_groups.insert("grp-1".into());
        assert_eq!(panel.visible_row_indices(), vec![0], "nested host hidden");

        // Expand: nested host is back.
        panel.collapsed_groups.remove("grp-1");
        assert_eq!(
            panel.visible_row_indices(),
            vec![0, 1],
            "nested host restored"
        );
    }

    fn host(id: &str, nested: bool, stored: bool) -> Row {
        Row::Host(HostItem {
            id: id.into(),
            name: id.into(),
            endpoint: format!("root@{id}"),
            badge: if stored { Badge::Ssh } else { Badge::Local },
            stored,
            os_id: None,
            status: HostStatus::Idle,
            nested,
            session_count: 0,
        })
    }

    fn shell_rows() -> Vec<Row> {
        vec![
            Row::Section(LOCAL_SECTION.into()),
            host("local", false, false),
            Row::Group {
                id: "g1".into(),
                name: "jeremy".into(),
                host_count: 1,
                session_count: 0,
                collapsed: false,
            },
            host("prod", true, true),
            Row::Section(SERVERS_SECTION.into()),
            host("h2", false, true),
        ]
    }

    #[test]
    fn groups_above_servers_are_part_of_the_stored_area() {
        let mut panel = HostPanel::default();
        panel.set_rows(shell_rows());
        assert_eq!(panel.hosts_section_index(), Some(4));
        assert_eq!(panel.stored_area_index(), Some(2));
        let (oy, h) = tall();
        let h2 = panel.card_rect(oy, 5);
        panel.host_drag = Some(HostDrag {
            host_id: "h2".into(),
            host_name: "h2".into(),
            endpoint: String::new(),
            kind: HostDragKind::Host,
            row_index: 5,
            press_x: h2.x,
            press_y: h2.y,
            current_x: h2.x,
            current_y: h2.y,
            grab_dx: 0.0,
            grab_dy: 0.0,
            source_rect: h2,
            ghost_rect: h2,
            phase: HostDragPhase::Dragging,
            drop_target: None,
        });
        // Onto the grouped host: join the group.
        let prod = panel.card_rect(oy, 3);
        assert_eq!(
            panel.drop_target_at(oy, h, prod.x + 10.0, prod.y + 10.0),
            Some(HostDropTarget::Group("g1".into()))
        );
        // Onto the top of the group header: reorder before it.
        let g = panel.card_rect(oy, 2);
        assert_eq!(
            panel.drop_target_at(oy, h, g.x + 10.0, g.y + 2.0),
            Some(HostDropTarget::BeforeGroup("g1".into()))
        );
        // The local machines above are not a drop zone.
        let local = panel.card_rect(oy, 1);
        assert_eq!(
            panel.drop_target_at(oy, h, local.x + 10.0, local.y + 10.0),
            None
        );
    }

    #[test]
    fn headers_and_rows_follow_the_shell_rhythm() {
        let mut panel = HostPanel::default();
        panel.set_rows(shell_rows());
        let oy = 0.0;
        let first = panel.card_rect(oy, 0);
        assert_eq!(first.y, crate::shell::sidebar::list_top());
        assert_eq!(first.height, SECTION_HEIGHT);
        let local = panel.card_rect(oy, 1);
        assert_eq!(local.y, first.bottom());
        assert_eq!((local.x, local.width, local.height), (12.0, 236.0, 44.0));
        let group = panel.card_rect(oy, 2);
        assert_eq!(group.y, local.bottom() + CARD_GAP + SECTION_GAP);
        // The New group action rides the Servers header.
        let action = panel.new_group_button_rect(oy, 900.0);
        let servers = panel.card_rect(oy, 4);
        assert_eq!(action.right(), servers.right());
        assert_eq!(action.y, servers.y);
    }

    #[test]
    fn the_filter_field_only_takes_room_while_filtering() {
        let mut panel = HostPanel::default();
        panel.set_rows(shell_rows());
        let (oy, h) = tall();
        let y0 = panel.card_rect(oy, 0).y;
        assert_eq!(panel.search_rect(oy).width, 0.0);
        panel.filter_focused = true;
        let field = panel.search_rect(oy);
        assert!(field.width > 0.0);
        assert_eq!(panel.card_rect(oy, 0).y, y0 + FILTER_BAND_HEIGHT);
        assert_eq!(
            panel.hit_test(oy, h, field.x + 5.0, field.y + 5.0),
            Some(PanelHit::Search)
        );
        panel.filter_focused = false;
        panel.filter = TextDraft::new("prod");
        assert!(panel.filter_visible(), "a live filter stays visible");
        panel.escape_filter();
        assert!(!panel.filter_visible());
    }

    #[test]
    fn rows_have_no_per_row_session_controls() {
        let mut panel = HostPanel::default();
        let mut rows = shell_rows();
        if let Row::Host(h) = &mut rows[5] {
            h.session_count = 2;
        }
        panel.set_rows(rows);
        assert!(panel.host_add_session_rect(0.0, 5).is_none());
        assert!(panel.host_chevron_rect(0.0, 5).is_none());
    }

    #[test]
    fn filtering_hides_a_section_whose_own_rows_do_not_match() {
        let mut panel = HostPanel::default();
        panel.set_rows(shell_rows());
        panel.filter = TextDraft::new("prod");
        // "This computer" (local only) is hidden; the group with prod stays.
        assert_eq!(panel.visible_row_indices(), vec![2, 3]);
    }

    // ---- Polish 4: overflowing list (many hosts / small window) ----

    /// 922x630: the default window clamped to a 1024x700 screen.
    const SMALL_H: f32 = 630.0;

    #[test]
    fn a_row_cut_by_the_viewport_is_neither_painted_nor_hit() {
        let panel = panel(30);
        let (oy, h) = (0.0, SMALL_H);
        let body = panel.body_rect(oy, h);
        let cut = panel
            .visible_row_indices()
            .into_iter()
            .find(|&i| {
                let c = panel.card_rect(oy, i);
                c.y < body.bottom() && c.bottom() > body.bottom()
            })
            .expect("30 rows overflow a 630 px window");
        assert!(!panel.row_painted(oy, h, cut));
        let card = panel.card_rect(oy, cut);
        assert_eq!(
            panel.hit_test(oy, h, card.x + 10.0, card.y + 2.0),
            Some(PanelHit::Background),
            "the visible sliver of an unpainted row must not be clickable"
        );
        assert!(panel.row_painted(oy, h, 0));
    }

    #[test]
    fn the_scroll_thumb_shows_only_when_the_list_overflows() {
        let (oy, h) = (0.0, SMALL_H);
        assert_eq!(panel(3).scroll_thumb(oy, h), None);

        let mut panel = panel(30);
        let body = panel.body_rect(oy, h);
        let top = panel.scroll_thumb(oy, h).expect("overflowing list");
        assert!((top.y - body.y).abs() < 0.01, "at the top when unscrolled");
        assert!(top.height < body.height && top.height >= SCROLL_THUMB_MIN);
        assert!(top.right() <= ORIGIN_X + WIDTH && top.x > ORIGIN_X + WIDTH - PAD_X);
        // Thumb share of the track = viewport share of the content.
        let share = body.height / panel.content_height();
        assert!((top.height - body.height * share).abs() < 0.5);

        panel.scroll = panel.max_scroll(oy, h);
        let bottom = panel.scroll_thumb(oy, h).unwrap();
        assert!((bottom.bottom() - body.bottom()).abs() < 0.01, "at the end");
    }

    #[test]
    fn reveal_scrolls_a_hidden_row_fully_into_view_and_leaves_visible_ones() {
        let (oy, h) = (0.0, SMALL_H);
        let mut panel = panel(30);
        let body = panel.body_rect(oy, h);

        assert!(!panel.reveal_row(2, oy, h), "already visible: no scroll");
        assert_eq!(panel.scroll, 0.0);

        assert!(panel.reveal_row(25, oy, h));
        let card = panel.card_rect(oy, 25);
        assert!(panel.row_painted(oy, h, 25));
        assert!(
            (card.bottom() - body.bottom()).abs() < 0.01,
            "lands at the bottom"
        );

        assert!(panel.reveal_row(0, oy, h));
        assert_eq!(panel.scroll, 0.0);

        // The last row of all: never scrolls past the clamp.
        assert!(panel.reveal_row(29, oy, h));
        // (the slot's trailing card gap may stay below the fold)
        assert!(panel.max_scroll(oy, h) - panel.scroll <= CARD_GAP + 0.01);
        assert!(panel.row_painted(oy, h, 29));
    }

    #[test]
    fn a_drag_near_the_list_edges_asks_for_auto_scroll() {
        let (oy, h) = (0.0, SMALL_H);
        let panel = panel(30);
        let body = panel.body_rect(oy, h);
        assert!(panel.drag_autoscroll_speed(oy, h, body.bottom() - 4.0) > 0.0);
        assert!(panel.drag_autoscroll_speed(oy, h, body.y + 4.0) < 0.0);
        assert_eq!(
            panel.drag_autoscroll_speed(oy, h, body.y + body.height / 2.0),
            0.0
        );
        // Nearer the edge scrolls faster.
        assert!(
            panel.drag_autoscroll_speed(oy, h, body.bottom() - 2.0)
                > panel.drag_autoscroll_speed(oy, h, body.bottom() - 20.0)
        );
        // Nothing to scroll: no auto-scroll.
        assert_eq!(
            panel_fit().drag_autoscroll_speed(oy, h, body.bottom() - 4.0),
            0.0
        );
    }

    fn panel_fit() -> HostPanel {
        panel(2)
    }
}
