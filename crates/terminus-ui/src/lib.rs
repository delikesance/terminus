//! Terminus UI: the chrome's state, layout and hit-testing.
//!
//! This crate is deliberately paint-free. It answers three questions
//! and nothing else:
//!
//! * what does the chrome look like — every rectangle, in logical
//!   pixels, from [`geom::Rect`] and the per-section modules;
//! * what is under the pointer — [`chrome::Chrome::handle_press`],
//!   `handle_hover` and `handle_wheel`, which walk the same rectangles;
//! * what does the keyboard do — [`add_host::AddHostForm`], a plain
//!   text sink with a caret and no I/O.
//!
//! The painters live in the frontend (`rioterm`) and read this state;
//! the host database lives there too. Keeping the geometry and the
//! focus model here means they can be tested without a window, a GPU or
//! a database.

pub mod activity_bar;
pub mod add_host;
pub mod anim;
pub mod chrome;
pub mod connection;
pub mod geom;
pub mod icons;
pub mod loading;
pub mod os_icons;
pub mod overlap;
pub mod settings;
pub mod sftp_pane;
pub mod sidebar;
pub mod snippets;
pub mod theme;

pub use activity_bar::{ActivityBarState, RailAction, RailHit, Section, TOP_SECTIONS, SECTIONS};
pub use add_host::{
    AddHostForm, AddHostLayout, Field, FormInput, FormOutcome, HostFormValues,
};
pub use chrome::{Chrome, ChromeAction};
pub use connection::{
    ConnectKind, ConnectionHit, ConnectionSequence, NodeVisual, STEP_COUNT,
};
pub use geom::Rect;
pub use icons::{Cmd, Icon, IconPlacement};
pub use loading::{breath_ring, orbit_dots, phase as loading_phase, shimmer_bar, OrbitDot};
pub use os_icons::{HostStatus, OsGlyph};
pub use overlap::{assert_no_overlaps, assert_panel_no_overlaps, find_overlaps, rects_overlap};
pub use settings::{SettingsHit, SettingsModal, SettingsTab, SshKeyItem};
pub use anim::{lerp, lerp_rect, Ease, RectTween, Tween, SNAP_DURATION};
pub use sidebar::{
    Badge, HostDrag, HostDragPhase, HostDropTarget, HostItem, HostPanel, PanelHit, Row,
    HOST_DRAG_THRESHOLD,
};
pub use snippets::{SnippetHit, SnippetItem, SnippetsPanel};
pub use theme::ChromeTheme;

pub use terminus_bridge;
pub use terminus_core;

/// Placeholder version marker.
pub const UI_VERSION: &str = env!("CARGO_PKG_VERSION");
