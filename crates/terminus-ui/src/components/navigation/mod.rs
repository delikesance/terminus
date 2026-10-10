//! Navigation: server rows, section/group headers, drop target, view tabs
//! and session pills (violet-ink design board `CNavigation`).
//!
//! Pure state, geometry and hit-testing in logical pixels; no GPU.

pub mod drop_target;
mod pill;
mod row;
pub mod section_header;
pub mod server_row;
pub mod session_pill;
mod tab;
pub mod view_tabs;

pub use pill::*;
pub use row::*;
pub use tab::*;

#[cfg(test)]
mod navigation_tests;
