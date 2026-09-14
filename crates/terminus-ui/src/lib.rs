//! Terminus UI: renderer modules (ActivityBar, Sidebar, SFTP pane) layered on
//! top of Rio's Sugarloaf renderer.
//!
//! Scaffold placeholder — see `milestone.md` (renderers wired in later steps).

pub mod activity_bar;
pub mod sftp_pane;
pub mod sidebar;
pub mod theme;

pub use terminus_bridge;
pub use terminus_core;

/// Placeholder version marker until the UI renderers land.
pub const UI_VERSION: &str = env!("CARGO_PKG_VERSION");
