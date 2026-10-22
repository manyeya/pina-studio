//! The Browser: everything that can go on a track, found in one search. Pina Studio's own instruments
//! and effects and every plugin installed, one entry each however many formats it comes in, in
//! the categories its plugin says it belongs to (VST3's subcategories, CLAP's features, an Audio
//! Unit's type, VST2's category; its name where those are silent); its preset files; the song's
//! samples; and the songs themselves. Favourites and what's used lately are remembered.

mod cats;
mod devices;
mod kept;
mod rows;
mod using;
mod view;

pub use cats::*;
pub use devices::*;
pub use kept::*;

use std::{collections::HashMap, path::PathBuf, sync::OnceLock};

use gpui::{
    Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, Subscription, Svg, div,
    prelude::*, px, rgb, svg,
};
use pina_engine::{
    plugin::scan::PluginInfo,
    song::project::{Format, Instrument, Preset, Which},
};
use serde::{Deserialize, Serialize};

use crate::{
    mix::strip::{Builtin, Channel},
    ui::looks::{glyph, hsl},
    ui::menu::MenuAction,
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::{Drag, Workspace, pt},
};

pub const EFFECT_CATS: [Cat; 10] = [
    Cat::Eq,
    Cat::Dynamics,
    Cat::Reverb,
    Cat::Delay,
    Cat::Distortion,
    Cat::Modulation,
    Cat::Filter,
    Cat::Pitch,
    Cat::Space,
    Cat::Utility,
];
pub const INSTRUMENT_CATS: [Cat; 5] = [
    Cat::Synth,
    Cat::Sampler,
    Cat::Drums,
    Cat::Keys,
    Cat::Orchestral,
];

/// Pina Studio's own devices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Own {
    Synth,
    Drums,
    Sampler,
    Eq,
    Comp,
    Sat,
}

/// Something that can go on a track.
#[derive(Clone, Debug)]
pub struct Device {
    /// What favourites and recents keep: "own:eq", "plugin:surge synth team/surge xt".
    pub key: String,
    pub name: String,
    pub vendor: String,
    pub cat: Cat,
    pub own: Option<Own>,
    /// Its plugin in each format it's installed in, the one used first.
    pub formats: Vec<PluginInfo>,
    /// Lowercase words a search looks through.
    hay: String,
}

impl Device {
    pub fn instrument(&self) -> bool {
        self.cat.instrument()
    }

    /// What the formats are called, the one used first.
    pub fn badges(&self) -> Vec<&'static str> {
        self.formats
            .iter()
            .map(|p| match p.format {
                Format::Clap => "CLAP",
                Format::Vst3 => "VST3",
                Format::Au => "AU",
                Format::Vst2 => "VST2",
            })
            .collect()
    }
}

/// Where a device goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Track(usize),
    NewTrack,
}

/// What the Browser shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Instruments,
    Effects,
    /// The sample library.
    Sounds,
}

/// A line of results: a device, one of its presets, a sample, a song.
#[derive(Clone, Debug)]
pub enum Row {
    Heading(String),
    Device(usize),
    Preset(usize, Preset),
    Sample(usize),
    Song(crate::pages::projects::Song),
}

#[cfg(test)]
mod tests;
