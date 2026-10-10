use super::*;

impl HostPanel {
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
    pub(super) fn root_reorder_cards(
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
    pub(super) fn root_insert_target_at(
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

    pub(super) fn point_in_hosts_section(
        &self,
        origin_y: f32,
        height: f32,
        y: f32,
    ) -> bool {
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
}
