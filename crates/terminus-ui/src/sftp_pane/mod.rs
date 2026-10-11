//! Dual-pane SFTP browser state, geometry, and hit-testing (paint-free).
//!
//! Layout lives inside the leaf grid `layout_rect`:
//! toolbar (Close + optional name edit) → Left | Right lists → status footer.
//!
//! Either side may be local FS or a remote host (`SftpBackend`).

/// Row height for file entries.
pub const ROW_HEIGHT: f32 = 30.0;
/// Breadcrumb / header strip height.
pub const HEADER_HEIGHT: f32 = 34.0;
/// Top toolbar (close / name edit) height.
pub const TOOLBAR_HEIGHT: f32 = 40.0;
/// Shared status footer height.
pub const FOOTER_HEIGHT: f32 = 28.0;
/// Gap between left and right panes.
pub const PANE_GAP: f32 = 6.0;
/// Horizontal padding inside a pane.
pub const PANE_PAD: f32 = 8.0;
/// Toolbar / header icon button size.
pub const BTN_SIZE: f32 = 28.0;
/// Gap between toolbar buttons.
pub const BTN_GAP: f32 = 6.0;
/// Pointer travel before a row press becomes a drag.
pub const SFTP_DRAG_THRESHOLD: f32 = 5.0;

mod filter;
mod layout;
mod pane_state;
mod paths;
mod transfer;
mod types;

pub use layout::*;
pub use pane_state::*;
pub use paths::*;
pub use transfer::*;
pub use types::*;

#[cfg(test)]
mod filter_tests;
#[cfg(test)]
mod pane_tests;
