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
