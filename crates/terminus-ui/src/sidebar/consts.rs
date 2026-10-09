use crate::geom::Rect;

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
