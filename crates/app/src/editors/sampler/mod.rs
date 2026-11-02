//! The Sampler editor, in the pane under the tracks: the selected track's built-in sampler.
//!
//! Down the left, its zones (drop samples there, or anywhere on it) and the picked zone's own
//! settings: its notes, root (heard from the sample, or set), tuning, level, pan, velocities and
//! their fade, loop crossfade. Along the top, how it reads its samples (Repitch, Stretch,
//! Grains), its character (clean, tape, vintage) and the zone's switches (one-shot or gated,
//! looping, reversed, choked, played as it's let go). Then the zone's sample, big: zoom with the
//! wheel, drag its start, end and loop (the loop's crossfade shown), and, in Stretch and Grains,
//! where the grains read and how far they scatter; every voice playing it a glowing playhead,
//! every grain a spark. Under it the keys, each zone over the notes it plays (click to hear).
//! Along the bottom, how it plays: the mode's own settings; the filter, its response drawn (drag
//! its point for cutoff and resonance); the amp envelope, drawn and dragged; and the modulation:
//! the LFOs, the modulation envelope, the matrix, and what plays with the sample (a pitch drop,
//! sub, noise, FM, ring, punch). Every change is an edit (undone like any) that the playing
//! sampler follows at once.

mod body;
mod controls;
mod edits;
mod empty;
mod fields;
mod graphs;
mod keys;
mod matrix;
pub(crate) mod modules;
mod wave;

pub(crate) use controls::pill;
pub use fields::*;
pub(crate) use graphs::response;
use graphs::{wave_graph, wave_icon};

use std::{
    cell::Cell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use gpui::{
    Bounds, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent, PathBuilder,
    PathPromptOptions, Pixels, Point, ScrollWheelEvent, Window, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};
use pina_engine::{
    audio::engine::read_audio,
    audio::peaks::Peaks,
    dsp::sampler::{Playing, cuts, note_of, onsets, pitch, slices},
    song::commands::{self, Command},
    song::project::{
        FilterMode, Instrument, LoopMode, ModDest, ModSource, Route, SampleCharacter, SampleMode,
        Sampler, SamplerVoice, Trigger, Wave as LfoWave, Zone,
    },
};

use crate::{
    editors::pianoroll::name_in,
    mix::eq::{dial, disc},
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Drag, Target, Workspace, pt},
};

/// A sample's waveform and hits, worked out once: its peaks, its loudness in `LOUD` steps (for
/// the body of the drawing), its length, and where its hits start.
pub struct Wave {
    peaks: Peaks,
    loud: Vec<f32>,
    len: usize,
    seconds: f32,
    onsets: Vec<(f32, f32)>,
}

/// How many steps a waveform's loudness is kept in.
const LOUD: usize = 4096;

/// The modulation card's pages.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ModTab {
    #[default]
    Lfo1,
    Lfo2,
    Env,
    Matrix,
    Layers,
}

#[derive(Default)]
pub struct SamplerUi {
    /// The zone picked, by index.
    pub zone: usize,
    /// Waveforms by file (relative to the song), None while one's being read.
    waves: HashMap<PathBuf, Option<Arc<Wave>>>,
    /// The part of the sample shown, as shares of it (all of it: none).
    pub view: Option<(f32, f32)>,
    pub tab: ModTab,
    /// The Envelopes page shows the modulation envelope (else the filter's).
    pub mod_env: bool,
    /// Where the voices were playing, last frame.
    pub playing: Vec<Playing>,
    /// A root being heard from its sample, by zone.
    pub hearing: Option<usize>,
    /// Where the graphs were drawn, for pressing and dragging their points.
    pub graphs: Rc<Graphs>,
    /// Playheads to show without a sound playing (for pictures of it).
    #[cfg(test)]
    pub pretend: Vec<Playing>,
}

/// Where each graph was last drawn.
#[derive(Default)]
pub struct Graphs {
    pub amp: Cell<Bounds<Pixels>>,
    pub filter_env: Cell<Bounds<Pixels>>,
    pub mod_env: Cell<Bounds<Pixels>>,
    pub curve: Cell<Bounds<Pixels>>,
}

impl Wave {
    /// Its loudness at `n` points along it, 0 to 1: its outline, for a little picture.
    pub(crate) fn outline(&self, n: usize) -> Vec<f32> {
        let most = self.loud.iter().fold(1e-6f32, |m, &v| m.max(v));
        (0..n)
            .map(|i| {
                let a = i * self.loud.len() / n.max(1);
                let b = ((i + 1) * self.loud.len() / n.max(1)).max(a + 1);
                let part = &self.loud[a.min(self.loud.len())..b.min(self.loud.len())];
                part.iter().fold(0f32, |m, &v| m.max(v)) / most
            })
            .collect()
    }
}

impl Graphs {
    pub fn of(&self, env: EnvKind) -> &Cell<Bounds<Pixels>> {
        match env {
            EnvKind::Amp => &self.amp,
            EnvKind::Filter => &self.filter_env,
            EnvKind::Mod => &self.mod_env,
        }
    }
}

/// A sampler setting a knob turns: the instrument's, or a zone's (by index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    Attack,
    Decay,
    /// How long a one-shot rings, as a drum's length: all of it at the top, else dying away
    /// over the decay (its sustain none).
    Length,
    Sustain,
    Release,
    Cutoff,
    Resonance,
    Drive,
    FilterEnv,
    FilterAttack,
    FilterDecay,
    FilterSustain,
    FilterRelease,
    KeyTrack,
    Velocity,
    Glide,
    LfoRate,
    Vibrato,
    LfoFilter,
    Tremolo,
    Lfo2Rate,
    ModAttack,
    ModDecay,
    ModSustain,
    ModRelease,
    Transpose,
    Volume,
    Speed,
    Position,
    GrainSize,
    Density,
    Spray,
    GrainPitch,
    GrainReverse,
    GrainSpread,
    GrainShape,
    Unison,
    Detune,
    Spread,
    PitchEnv,
    PitchDecay,
    Sub,
    Noise,
    NoiseColor,
    Fm,
    Ring,
    OscRatio,
    Punch,
    Color,
    CrushKhz,
    Bits,
    Route(usize),
    Note(usize),
    High(usize),
    Root(usize),
    Tune(usize),
    Gain(usize),
    Pan(usize),
    VelLow(usize),
    VelHigh(usize),
    VelFade(usize),
    LoopFade(usize),
}

/// How a zone's sample is cut into slices across the keys.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slicing {
    /// At its strongest hits, at most this many.
    Hits(usize),
    /// Into this many equal parts.
    Equal(usize),
}

/// Which mark on the waveform is dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Start,
    End,
    LoopStart,
    LoopEnd,
    /// Where Stretch and Grains read.
    Position,
}

/// A sampler setting picked from a menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SamplerOp {
    /// A matrix route's source or destination (none: the route taken out).
    Route {
        track: usize,
        slot: usize,
        source: Option<ModSource>,
        dest: Option<ModDest>,
    },
    Filter {
        track: usize,
        mode: FilterMode,
    },
    /// A vintage sampler's rate, bits and output filter at once.
    Vintage {
        track: usize,
        khz: f32,
        bits: u8,
        filter: bool,
    },
}

/// Which envelope a graph shows: the amp's, the filter's or the modulation one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnvKind {
    Amp,
    Filter,
    Mod,
}

/// A zone's colour, by its place.
fn zone_color(i: usize) -> u32 {
    [CORAL, AMBER, MINT, SKY, VIOLET, PINK, TEAL, 0xfb923c][i % 8]
}

fn zone_name(z: &Zone) -> String {
    match z.name.is_empty() {
        true => z
            .file
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
        false => z.name.clone(),
    }
}

#[cfg(test)]
mod tests;
