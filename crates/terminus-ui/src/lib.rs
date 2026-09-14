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
pub mod chrome;
pub mod geom;
pub mod icons;
pub mod sftp_pane;
pub mod sidebar;
pub mod theme;

pub use activity_bar::{ActivityBarState, Section, SECTIONS};
pub use add_host::{
    AddHostForm, AddHostLayout, Field, FormInput, FormOutcome, HostFormValues,
};
pub use chrome::{Chrome, ChromeAction};
pub use geom::Rect;
pub use icons::{Icon, IconPlacement, Seg};
pub use sidebar::{HostItem, HostPanel, PanelHit};
pub use theme::ChromeTheme;

pub use terminus_bridge;
pub use terminus_core;

/// Placeholder version marker.
pub const UI_VERSION: &str = env!("CARGO_PKG_VERSION");
