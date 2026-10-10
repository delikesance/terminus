//! Overlays: dialog shell, wizard stepper, context menu and command palette.
//!
//! Pure geometry, state and key semantics in logical pixels; the painter in
//! `frontends/rioterm/src/renderer/components/overlay.rs` walks the same
//! rect functions the pointer hit-tests. Specs come from the design board
//! `COverlays` (violet ink).
//!
//! [`Menu`] is the single context menu (disabled rows, separators): a press
//! returns Item / Consume / Dismiss, height derives from the entries, width
//! grows with the longest label, `clamped` keeps it on screen.

mod dialog;
mod menu;
mod palette;
mod stepper;

pub use dialog::*;
pub use menu::*;
pub use palette::*;
pub use stepper::*;

#[cfg(test)]
mod dialog_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod palette_tests;
