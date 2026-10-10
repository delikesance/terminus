use super::*;

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

    pub(super) fn notice_height(&self) -> f32 {
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
    pub(super) fn content_top(&self, origin_y: f32) -> f32 {
        origin_y + HEADER_HEIGHT + self.search_band_rect(origin_y).height
    }

    /// Row 0's unscrolled top edge.
    pub(super) fn rows_top(&self, origin_y: f32) -> f32 {
        self.content_top(origin_y)
    }

    /// Scrolled offset of row `index` from row 0's top edge (visible rows only).
    pub(super) fn offset_of(&self, index: usize) -> f32 {
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
}
