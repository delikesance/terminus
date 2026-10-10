use super::*;

impl HostPanel {
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
    pub(super) fn new_group_form_slot_height(&self) -> f32 {
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

    /// Walk upward from `index` to find the enclosing group id, if any.
    pub(super) fn enclosing_group_id(&self, index: usize) -> Option<String> {
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

    /// Toggle whether a host's sessions are collapsed.
    pub fn toggle_host_collapsed(&mut self, host_id: &str) {
        if self.collapsed_hosts.contains(host_id) {
            self.collapsed_hosts.remove(host_id);
        } else {
            self.collapsed_hosts.insert(host_id.to_string());
        }
    }
}
