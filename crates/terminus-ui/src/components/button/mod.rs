//! Buttons: five kinds x three sizes x five states, geometry and hit-testing.
//!
//! No fonts here: the caller measures the label and passes the width in.
//! The painter walks the same rects this module returns for hit-testing.

mod spec;
mod style;

pub use spec::*;
pub use style::*;

#[cfg(test)]
mod button_tests;
