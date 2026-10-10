use super::*;

impl Chrome {
    /// Update an armed host drag while the primary button is held.
    ///
    /// Returns whether the chrome needs a repaint.
    pub fn handle_drag_move(&mut self, window_height: f32, x: f32, y: f32) -> bool {
        let Some(drag) = self.panel.host_drag.as_mut() else {
            return false;
        };
        // Ignore pointer while the ghost is snapping home.
        if matches!(drag.phase, crate::sidebar::HostDragPhase::Snapping { .. }) {
            return true;
        }
        drag.current_x = x;
        drag.current_y = y;
        let dx = x - drag.press_x;
        let dy = y - drag.press_y;
        if matches!(drag.phase, crate::sidebar::HostDragPhase::Armed)
            && dx * dx + dy * dy >= crate::sidebar::HOST_DRAG_THRESHOLD.powi(2)
        {
            drag.phase = crate::sidebar::HostDragPhase::Dragging;
        }
        if matches!(drag.phase, crate::sidebar::HostDragPhase::Dragging) {
            let w = drag.source_rect.width;
            let h = drag.source_rect.height;
            drag.ghost_rect =
                crate::geom::Rect::new(x - drag.grab_dx, y - drag.grab_dy, w, h);
        }
        let dragging = matches!(drag.phase, crate::sidebar::HostDragPhase::Dragging);
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        let target = if dragging {
            self.panel.drop_target_at(origin_y, height, x, y)
        } else {
            None
        };
        if let Some(drag) = self.panel.host_drag.as_mut() {
            drag.drop_target = target;
        }
        let _ = self.handle_hover(window_height, x, y);
        true
    }

    /// Finish a host press/drag on primary-button release.
    pub fn handle_release(&mut self, window_height: f32, x: f32, y: f32) -> ChromeAction {
        let Some(drag) = self.panel.host_drag.as_ref() else {
            return ChromeAction::Ignored;
        };
        if matches!(drag.phase, crate::sidebar::HostDragPhase::Armed) {
            let id = drag.host_id.clone();
            let is_group = drag.is_group();
            self.panel.host_drag = None;
            return if is_group {
                ChromeAction::ToggleGroup(id)
            } else {
                ChromeAction::OpenHost(id)
            };
        }
        // Dragging → apply drop immediately (no snap tween).
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        let cached = self
            .panel
            .host_drag
            .as_ref()
            .and_then(|d| d.drop_target.clone());
        let host_id = self
            .panel
            .host_drag
            .as_ref()
            .map(|d| d.host_id.clone())
            .unwrap_or_default();
        let is_group = self
            .panel
            .host_drag
            .as_ref()
            .is_some_and(crate::sidebar::HostDrag::is_group);
        let target = self.panel.drop_target_at(origin_y, height, x, y).or(cached);
        let Some(target) = target else {
            self.panel.host_drag = None;
            return ChromeAction::Consumed;
        };
        if let crate::sidebar::HostDropTarget::Group(ref group_id) = target {
            self.panel.collapsed_groups.remove(group_id);
        }
        self.panel.host_drag = None;
        Self::action_from_drop(host_id, is_group, target)
    }

    pub(super) fn action_from_drop(
        host_id: String,
        is_group: bool,
        pending: crate::sidebar::HostDropTarget,
    ) -> ChromeAction {
        match pending {
            crate::sidebar::HostDropTarget::Group(group_id) => {
                ChromeAction::SetHostGroup {
                    host_id,
                    group_id: Some(group_id),
                }
            }
            crate::sidebar::HostDropTarget::Ungroup => {
                if is_group {
                    ChromeAction::ReorderGroup {
                        group_id: host_id,
                        before_group_id: None,
                        before_host_id: None,
                    }
                } else {
                    ChromeAction::ReorderHost {
                        host_id,
                        before_host_id: None,
                        before_group_id: None,
                    }
                }
            }
            crate::sidebar::HostDropTarget::BeforeHost(before_host_id) => {
                if is_group {
                    ChromeAction::ReorderGroup {
                        group_id: host_id,
                        before_group_id: None,
                        before_host_id: Some(before_host_id),
                    }
                } else {
                    ChromeAction::ReorderHost {
                        host_id,
                        before_host_id: Some(before_host_id),
                        before_group_id: None,
                    }
                }
            }
            crate::sidebar::HostDropTarget::BeforeGroup(before_group_id) => {
                if is_group {
                    ChromeAction::ReorderGroup {
                        group_id: host_id,
                        before_group_id: Some(before_group_id),
                        before_host_id: None,
                    }
                } else {
                    ChromeAction::ReorderHost {
                        host_id,
                        before_host_id: None,
                        before_group_id: Some(before_group_id),
                    }
                }
            }
        }
    }

    /// Per-frame drag tick: while a host is dragged near the top or
    /// bottom edge of the machine list, scroll the list and retarget the
    /// drop under the (still) pointer. Never produces an action.
    pub fn tick_host_drag(&mut self, dt: f32) -> Option<ChromeAction> {
        let drag = self.panel.host_drag.as_ref()?;
        if !matches!(drag.phase, crate::sidebar::HostDragPhase::Dragging) {
            return None;
        }
        let (x, y) = (drag.current_x, drag.current_y);
        // The frontend keeps the window size current every frame.
        let window_height = self.shell.window.1;
        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        let speed = self.panel.drag_autoscroll_speed(origin_y, height, y);
        if speed == 0.0 {
            return None;
        }
        let before = self.panel.scroll;
        self.panel.scroll_by(speed * dt, origin_y, height);
        if self.panel.scroll != before {
            let target = self.panel.drop_target_at(origin_y, height, x, y);
            if let Some(drag) = self.panel.host_drag.as_mut() {
                drag.drop_target = target;
            }
        }
        None
    }

    /// Whether the chrome needs continuous frames.
    pub fn needs_animation_frames(&self) -> bool {
        self.connection.is_some()
            || self
                .panel
                .host_drag
                .as_ref()
                .is_some_and(|d| d.started() && !d.is_snapping())
    }
}
