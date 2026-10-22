//! Mixing by measurement. Each track's own sound is rendered and measured first (its spectrum,
//! loudness, width, its lowest note), and its channel set from that: a low cut under its lowest
//! note, mud and harshness cut only as far as it has them, air where it lacks it, its low end
//! kept mono. Parts make room for each other: the bass ducks at the kick's own frequency as it
//! hits, the chords and pads dip their presence while the lead plays (dynamic EQ bands hearing
//! the other track), and in genres that pump, the music ducks under the kick. Compression is set
//! from each part's level, not a fixed threshold. A reverb (Dragonfly's, if it's here) with its
//! pre-delay and decay set from the tempo and genre, a delay in time with the song, the drums
//! glued on a bus of their own, then each part's level against the drums.

mod master;
mod measure;
mod settings;
mod space;
mod stages;

pub use master::*;
pub(crate) use measure::*;
pub(crate) use settings::*;
pub(crate) use space::*;
pub use stages::*;

use std::path::Path;

use pina_engine::{
    music::analysis,
    music::arrange::role,
    plugin::scan,
    song::commands::{AutoParam, Command, PointIn},
    song::project::{Content, Format, PluginRef, Project},
};
use serde_json::{Value, json};

use crate::cli;

/// An edit given as JSON, as an agent would send it.
fn run(dir: &Path, v: Value) -> Result<Value, String> {
    let c: Command = serde_json::from_value(v).map_err(|e| e.to_string())?;
    cli::edit_with(dir, c)
}

#[cfg(test)]
mod tests;
