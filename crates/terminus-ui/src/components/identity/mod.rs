//! Identity: OS tiles and the machine / workspace header.
//!
//! Pure geometry and colour rules (no sugarloaf): the painter in
//! `rioterm::renderer::components::identity` walks the rects below.

mod gallery;
mod header;
mod tile;

pub use gallery::*;
pub use header::*;
pub use tile::*;

#[cfg(test)]
mod identity_tests;
