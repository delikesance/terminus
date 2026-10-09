use super::*;
use crate::components::scroll::VScroll;

impl HostPanel {
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
        let body = self.body_rect(origin_y, height);
        VScroll {
            track: Rect::new(
                ORIGIN_X + WIDTH - SCROLL_THUMB_INSET - SCROLL_THUMB_WIDTH,
                body.y,
                SCROLL_THUMB_WIDTH,
                body.height,
            ),
            content: self.content_height(),
            viewport: body.height,
            min_thumb: SCROLL_THUMB_MIN,
        }
        .thumb(self.scroll)
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

    /// Scroll by whole wheel notches.
    pub fn scroll_rows(&mut self, rows: f32, origin_y: f32, height: f32) {
        self.scroll_by(rows * (ITEM_HEIGHT + CARD_GAP), origin_y, height);
    }
}
