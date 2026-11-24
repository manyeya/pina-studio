//! Music theory to read and check by: chords from symbols or roman numerals and laid out in
//! time, and the drums of a kit. There are no progressions or grooves here: those are the
//! person's, in their playbooks, or the agent's own.

mod chords;
mod drums;
mod spans;

pub use chords::*;
pub use drums::*;
pub use spans::*;

use serde::{Deserialize, Serialize};

use crate::music::Key;

#[cfg(test)]
mod tests;
