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

mod consts;
mod drag;
mod drag_types;
mod groups;
mod hit;
mod layout;
mod rows;
mod scroll;
mod state;

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod scroll_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod test_support;

use std::collections::HashSet;

use crate::components::input::TextDraft;
use crate::geom::Rect;

pub use consts::*;
pub use drag_types::*;
pub use hit::PanelHit;
pub use rows::*;

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
