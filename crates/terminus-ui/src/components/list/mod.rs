//! Lists: rows, cards, tables and their hit-testing.
//!
//! Three row families from the design board:
//!
//! * the **list card** (tunnel, snippet, SSH key, status row): optional
//!   leading status dot, title + mono detail, right meta, then 0-2 action
//!   slots laid out right to left;
//! * the **file row** (SFTP browser): icon, name, size and date columns,
//!   with default / hover / selected / drop-target / renaming states;
//! * the **history row**: mono command, mono cwd, relative time, one
//!   trailing action slot, hairline divider below.
//!
//! Everything is in logical pixels. Painters walk the `*_layout` functions
//! and the mouse uses the `*_hit` functions, so they cannot disagree.

mod card;
mod file_row;
mod history_row;

pub use card::*;
pub use file_row::*;
pub use history_row::*;

#[cfg(test)]
mod list_tests;
