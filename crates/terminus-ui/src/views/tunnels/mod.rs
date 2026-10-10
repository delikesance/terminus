//! Tunnels view: state, validation, geometry and hit-testing.
//!
//! Pure logic in logical pixels, no GPU and no I/O. Everything is computed
//! from a `content: Rect` handed in by the shell (the area under the machine
//! header), so the view never assumes where the header or sidebar are. The
//! painter (`frontends/rioterm/src/renderer/views/tunnels.rs`) walks the same
//! `*_layout` functions the pointer hit-tests, so a control cannot be drawn
//! where it cannot be clicked.
//!
//! Widths that depend on measured text (buttons, the segmented control) come
//! from [`Metrics`]; the painter measures once per frame and stores them in
//! [`TunnelsState::metrics`], the defaults are only good guesses for the first
//! frame and for tests.

mod form;
mod layout;
mod model;
mod state;

pub use form::*;
pub use layout::*;
pub use model::*;
pub use state::*;

#[cfg(test)]
mod form_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod test_support;
