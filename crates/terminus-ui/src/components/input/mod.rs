//! Inputs: text field, search, select and textarea geometry, plus the shared
//! text-editing model ([`TextDraft`]) and its paint state ([`FieldPaint`]).

mod draft;
mod layout;
mod paint;

pub use draft::*;
pub use layout::*;
pub use paint::*;

#[cfg(test)]
mod draft_tests;
#[cfg(test)]
mod layout_tests;
