use super::*;

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

impl HostPanel {
    /// The visible row whose painted card holds `(x, y)`, inside the
    /// scroll viewport.
    pub(super) fn row_at(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<usize> {
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
    pub(super) fn item_at(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<usize> {
        self.row_at(origin_y, height, x, y)
            .filter(|&i| self.rows[i].host().is_some())
    }

    pub(super) fn session_at(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<usize> {
        self.row_at(origin_y, height, x, y)
            .filter(|&i| self.rows[i].session().is_some())
    }

    /// Which group header row is under `(x, y)`, if any.
    pub(super) fn group_at(
        &self,
        origin_y: f32,
        height: f32,
        x: f32,
        y: f32,
    ) -> Option<usize> {
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
}
