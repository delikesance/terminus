//! Feedback: status dot, connection steps, progress bar and toast.
//!
//! Pure state and geometry (logical pixels); the painter in
//! `rioterm::renderer::components::feedback` walks these same functions.

mod progress;
mod status;
mod steps;
mod toast;

pub use progress::*;
pub use status::*;
pub use steps::*;
pub use toast::*;

#[cfg(test)]
mod feedback_tests;
