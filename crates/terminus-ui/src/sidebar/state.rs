use super::*;

impl HostPanel {
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

    /// The header's count label ("3 hosts").
    pub fn count_label(&self) -> String {
        match self.host_count() {
            0 => "empty".to_string(),
            1 => "1 host".to_string(),
            n => format!("{n} hosts"),
        }
    }
}
