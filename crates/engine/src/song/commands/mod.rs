//! Edits as data: what an agent or a script sends Pina Studio, through the CLI, a running app's socket,
//! or MCP. In the app each is one undoable step, the same as the edit made by hand.

mod automation;
mod files;
mod mixing;
mod modulation;
mod plugins;
mod regions;
mod song;
mod tracks;
mod writing;

pub use automation::*;
pub(crate) use files::*;
pub use mixing::*;
pub(crate) use modulation::*;
pub use plugins::*;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    dsp::modulate::{ModKind, ModParam},
    music::arrange::{self, Arrangement, Transition},
    music::compose::{self, BassPart, ChordPart, DrumPart, MelodyPart},
    music::{Key, NoteEdit},
    plugin::{PluginApi, PluginParam, value_from_text},
    song::project::{
        AUDIO_DIR, AutoMode, AutoPoint, AutoShape, AuxSend, BEAT_UNITS, Cell, Comp, Content,
        ControlPoint, Cycle, Dynamic, Eq, Flex, Input, Instrument, Marker, Note, PPQ,
        PaletteDevice, PaletteSound, PanLaw, PanMode, Param, PatternRow, Place, PluginRef, Preset,
        PresetSource, Project, Region, Saturator, Track, Which,
    },
};

/// What `pina describe` prints: every command, for people and agents.
pub const DESCRIBE: &str = include_str!("commands.md");

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SetTempo {
        bpm: f64,
    },
    /// Beats to a bar and the note that's a beat.
    SetTimeSignature {
        beats: u64,
        unit: u64,
    },
    SetMaster {
        #[serde(default)]
        volume_db: Option<f32>,
        /// How loud the middle is against the sides, for every track's pan.
        #[serde(default)]
        pan_law: Option<PanLaw>,
    },
    AddTrack {
        name: String,
        /// None makes an audio track.
        #[serde(default)]
        instrument: Option<Instrument>,
        /// Where to put it; the end by default.
        #[serde(default)]
        index: Option<usize>,
    },
    DeleteTrack {
        track: usize,
    },
    SetTrack {
        track: usize,
        #[serde(default)]
        name: Option<String>,
        /// "#d97757", or "auto": its family's, by what it plays.
        #[serde(default)]
        color: Option<String>,
        /// An icon's id (`look::ICONS`), or "auto": what it plays says.
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        volume_db: Option<f32>,
        #[serde(default)]
        pan: Option<f32>,
        /// The pan modes: balance, stereo (each side placed, `spread` apart) or binaural.
        #[serde(default)]
        pan_mode: Option<PanMode>,
        #[serde(default)]
        spread: Option<f32>,
        /// Binaural: degrees above (or below) the head, and metres from it.
        #[serde(default)]
        elevation: Option<f32>,
        #[serde(default)]
        distance: Option<f32>,
        #[serde(default)]
        mute: Option<bool>,
        #[serde(default)]
        solo: Option<bool>,
    },
    SetInstrument {
        track: usize,
        instrument: Instrument,
    },
    /// Puts a plugin in an insert slot (an index past the last adds one), or takes it out.
    SetInsert {
        channel: ChannelRef,
        index: usize,
        #[serde(default)]
        plugin: Option<PluginRef>,
    },
    AddMidiRegion {
        track: usize,
        start: u64,
        length: u64,
        #[serde(default)]
        notes: Vec<Note>,
        #[serde(default)]
        controls: Vec<ControlPoint>,
    },
    /// Replaces a MIDI region's controller, pitch bend, pressure and expression moves.
    SetControls {
        track: usize,
        region: usize,
        controls: Vec<ControlPoint>,
    },
    SetNotes {
        track: usize,
        region: usize,
        notes: Vec<Note>,
    },
    /// A step pattern region, its rows of steps looping over its length.
    AddPatternRegion {
        track: usize,
        start: u64,
        length: u64,
        /// Ticks per step: a sixteenth by default.
        #[serde(default)]
        step: Option<u64>,
        rows: Vec<PatternRow>,
        /// 50 (straight) to 75.
        #[serde(default)]
        swing: Option<f32>,
    },
    /// Replaces a pattern region's rows, or sets its swing.
    SetPattern {
        track: usize,
        region: usize,
        #[serde(default)]
        rows: Option<Vec<PatternRow>>,
        #[serde(default)]
        swing: Option<f32>,
    },
    /// A WAV file, copied into the project's `Audio Files` if it's elsewhere.
    AddAudioRegion {
        track: usize,
        file: PathBuf,
        start: u64,
        #[serde(default)]
        offset: u64,
        /// The rest of the file by default.
        #[serde(default)]
        length: Option<u64>,
    },
    MoveRegion {
        track: usize,
        region: usize,
        start: u64,
        #[serde(default)]
        to_track: Option<usize>,
    },
    TrimRegion {
        track: usize,
        region: usize,
        #[serde(default)]
        start: Option<u64>,
        #[serde(default)]
        end: Option<u64>,
    },
    SplitRegion {
        track: usize,
        region: usize,
        at: u64,
    },
    DeleteRegion {
        track: usize,
        region: usize,
    },
    /// A region's name and playback settings (any of them): MIDI played transposed, louder or
    /// quantized, its notes kept; looping; and audio's flex mode and whether it follows the
    /// song's tempo.
    SetRegion {
        track: usize,
        region: usize,
        #[serde(default)]
        name: Option<String>,
        #[serde(default, rename = "loop")]
        looped: Option<bool>,
        /// Ticks from its start to where its loops stop; 0 runs them to the next region.
        #[serde(default)]
        loop_length: Option<u64>,
        /// An audio region's fades, in ticks.
        #[serde(default)]
        fade_in: Option<u64>,
        #[serde(default)]
        fade_out: Option<u64>,
        #[serde(default)]
        transpose: Option<i8>,
        #[serde(default)]
        velocity: Option<i8>,
        /// Ticks; 0 turns it off.
        #[serde(default)]
        quantize: Option<u64>,
        #[serde(default)]
        flex: Option<Flex>,
        #[serde(default)]
        follow: Option<bool>,
        #[serde(default)]
        formants: Option<bool>,
        /// Warp markers: [a frame of the file, where it plays in the stretched audio].
        #[serde(default)]
        warps: Option<Vec<[u64; 2]>>,
    },
    /// An alias of a MIDI or pattern region: a copy linked to it, so editing either edits both.
    AddAlias {
        track: usize,
        region: usize,
        start: u64,
        #[serde(default)]
        to_track: Option<usize>,
    },
    /// Makes an alias a region of its own.
    Unlink {
        track: usize,
        region: usize,
    },
    /// Puts a loop in a track's Live Loops cell for a scene, or empties the cell.
    SetCell {
        track: usize,
        scene: usize,
        #[serde(default)]
        region: Option<Region>,
    },
    /// Flex: stretches an audio region to `length` ticks, keeping its pitch.
    FlexRegion {
        track: usize,
        region: usize,
        length: u64,
    },
    /// Comps a take folder: take `take` plays from `from` to `to`, ticks from the region's start.
    Comp {
        track: usize,
        region: usize,
        from: u64,
        to: u64,
        take: usize,
    },
    /// Replaces a take folder with what its comp plays.
    Flatten {
        track: usize,
        region: usize,
    },
    /// Sends a track to a bus; without one, to a new aux.
    AddSend {
        track: usize,
        #[serde(default)]
        bus: Option<u32>,
        #[serde(default)]
        level_db: f32,
        #[serde(default)]
        pre: bool,
    },
    SetSend {
        track: usize,
        send: usize,
        #[serde(default)]
        level_db: Option<f32>,
        #[serde(default)]
        pre: Option<bool>,
    },
    /// What an audio track records: input channels from `channel` (0 is the first), one or two.
    SetInput {
        track: usize,
        #[serde(default)]
        input: Option<Input>,
    },
    /// Where a track plays: into an aux or stack's bus, or (without one) the master.
    SetOutput {
        track: usize,
        #[serde(default)]
        bus: Option<u32>,
    },
    /// Makes a summing stack of these tracks: a channel they all play into, the tracks moved
    /// together under the first of them. Flattening one undoes it.
    AddStack {
        tracks: Vec<usize>,
        #[serde(default)]
        name: Option<String>,
    },
    FlattenStack {
        aux: usize,
    },
    RemoveSend {
        track: usize,
        send: usize,
    },
    SetAux {
        aux: usize,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        volume_db: Option<f32>,
        #[serde(default)]
        pan: Option<f32>,
        #[serde(default)]
        mute: Option<bool>,
    },
    /// Replaces a track's volume (dB), pan (-1 to 1), mute (1 or 0), send level (dB) or plugin
    /// parameter (0 to 1) automation; no points takes it out.
    SetAutomation {
        track: usize,
        param: AutoParam,
        /// For a send: which of the track's sends.
        #[serde(default)]
        send: Option<usize>,
        /// For a plugin parameter: which plugin, and the parameter's id.
        #[serde(default)]
        plugin: Option<Which>,
        #[serde(default)]
        id: Option<ParamRef>,
        #[serde(default)]
        name: Option<String>,
        /// For a modulator's setting: which modulator, and which of its settings.
        #[serde(default)]
        modulator: Option<usize>,
        #[serde(default)]
        setting: Option<ModParam>,
        points: Vec<PointIn>,
    },
    /// The sounds the person picked for the song (each file with what it's for), and whether
    /// it's those and no others. Leave out what you don't change; `sounds` replaces them all.
    SetPalette {
        #[serde(default)]
        sounds: Option<Vec<PaletteSound>>,
        #[serde(default)]
        devices: Option<Vec<PaletteDevice>>,
        #[serde(default)]
        only: Option<bool>,
    },
    /// Smooths a track's automation from `from` to `to` (ticks; all of it without), blurred
    /// over about `width` ticks (an eighth without).
    SmoothAutomation {
        track: usize,
        param: AutoParam,
        #[serde(default)]
        send: Option<usize>,
        #[serde(default)]
        plugin: Option<Which>,
        #[serde(default)]
        id: Option<u32>,
        #[serde(default)]
        modulator: Option<usize>,
        #[serde(default)]
        setting: Option<ModParam>,
        #[serde(default)]
        from: Option<u64>,
        #[serde(default)]
        to: Option<u64>,
        #[serde(default)]
        width: Option<u64>,
    },
    /// Draws a shape into a track's automation from `from` to `to` (ticks), a cycle every
    /// `every` ticks, between `low` and `high`, over what was there.
    ShapeAutomation {
        track: usize,
        param: AutoParam,
        #[serde(default)]
        send: Option<usize>,
        #[serde(default)]
        plugin: Option<Which>,
        #[serde(default)]
        id: Option<u32>,
        #[serde(default)]
        modulator: Option<usize>,
        #[serde(default)]
        setting: Option<ModParam>,
        shape: AutoShape,
        from: u64,
        to: u64,
        #[serde(default)]
        every: Option<u64>,
        low: f32,
        high: f32,
    },
    /// How a track follows its automation: off, read, touch or latch.
    SetAutomationMode {
        track: usize,
        mode: AutoMode,
    },
    /// Turns a plugin's parameter (0 to 1), by id; without `value`, Pina Studio stops setting it.
    SetPluginParam {
        channel: ChannelRef,
        plugin: Which,
        param: ParamRef,
        #[serde(default)]
        value: Option<f64>,
        /// The value as the plugin shows it ("850 Hz", "Sine"), instead of `value`.
        #[serde(default)]
        text: Option<String>,
    },
    /// Loads a preset into a plugin: by name, or `{"program": n}` (and `"list"`) or
    /// `{"file": path}`; without `preset`, its settings go back to before.
    SetPreset {
        channel: ChannelRef,
        plugin: Which,
        #[serde(default)]
        preset: Option<PresetChoice>,
    },
    /// What an insert with a sidechain input hears there: a track's output, or (without
    /// `from`) nothing.
    SetSidechain {
        channel: ChannelRef,
        insert: usize,
        #[serde(default)]
        from: Option<usize>,
    },
    /// Names a place in the song; answers with the marker's index, in time order.
    AddMarker {
        tick: u64,
        name: String,
    },
    SetMarker {
        marker: usize,
        #[serde(default)]
        tick: Option<u64>,
        #[serde(default)]
        name: Option<String>,
    },
    DeleteMarker {
        marker: usize,
    },
    /// The song's key, like "A minor"; without one, Pina Studio works it out from the notes. With
    /// `move_notes`, the music moves into the new key.
    SetKey {
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        move_notes: bool,
    },
    /// Loops playback from `start` to `end` (ticks); `on: false` keeps the range without looping.
    SetCycle {
        start: u64,
        end: u64,
        #[serde(default = "yes")]
        on: bool,
    },
    /// A channel's built-in EQ (its bands, gain scale and output); without `eq`, takes it out.
    /// Only the fields given change (see `set_effect`): `bands` replaces them all. The old EQ's
    /// names (`low_cut`, `low_mid`, `high_mid`, `high_shelf`) still work, on its first low cut,
    /// bells and high shelf.
    SetEq {
        channel: ChannelRef,
        #[serde(default)]
        eq: Option<Value>,
    },
    /// One band of a channel's EQ, by its place: only what's given changes. A place one past the
    /// last adds a band (a 1 kHz bell unless it says); `remove` takes the band out. The channel
    /// gets an EQ if it hasn't one.
    SetEqBand {
        channel: ChannelRef,
        band: usize,
        #[serde(default)]
        shape: Option<crate::song::project::Shape>,
        #[serde(default)]
        freq: Option<f32>,
        #[serde(default)]
        gain_db: Option<f32>,
        #[serde(default)]
        q: Option<f32>,
        #[serde(default)]
        slope: Option<f32>,
        #[serde(default)]
        placement: Option<crate::song::project::Placement>,
        /// `false` makes it still; an object (any of range_db, threshold_db, attack_ms,
        /// release_ms) makes it move with the sound.
        #[serde(default)]
        dynamic: Option<Value>,
        #[serde(default)]
        off: Option<bool>,
        #[serde(default)]
        gain_q: Option<bool>,
        #[serde(default)]
        remove: bool,
    },
    /// A channel's built-in compressor, after its EQ; without `comp`, takes it out.
    SetComp {
        channel: ChannelRef,
        #[serde(default)]
        comp: Option<Value>,
    },
    /// A channel's built-in saturator, after its compressor; without `sat`, takes it out.
    SetSaturator {
        channel: ChannelRef,
        #[serde(default)]
        sat: Option<Value>,
    },
    /// Quantizes, humanizes, makes legato, chops, arpeggiates, transposes or changes the
    /// velocity of a MIDI region's notes (or one of them, with `note`).
    EditNotes {
        track: usize,
        region: usize,
        edit: NoteEdit,
        #[serde(default)]
        note: Option<usize>,
    },
    /// The song's chords, as symbols or numerals in its key, bars split by "|" and chords in a
    /// bar sharing it: "Am9 | Fmaj9 | C | G" or "i9 | VImaj9 | III | VII". From `start` (ticks,
    /// 0 by default) for `bars` bars (one pass by default), repeating; without `chords`, clears.
    SetChords {
        #[serde(default)]
        chords: Option<String>,
        #[serde(default)]
        start: Option<u64>,
        #[serde(default)]
        bars: Option<u64>,
    },
    /// Plays the song's chords on an instrument track, voiced from chord to chord.
    WriteChords(ChordPart),
    /// A bassline on the song's chords; `kick` style plays on the kick, as an 808 does.
    WriteBass(BassPart),
    /// A melody, riff, arpeggio or counter-line on the song's chords, in its key.
    WriteMelody(MelodyPart),
    /// A genre's groove as a step pattern, with fills and its swing.
    WriteDrums(DrumPart),
    /// What the song is: a playbook's id ("lofi", "trap"); without one, none.
    SetGenre {
        #[serde(default)]
        genre: Option<String>,
    },
    /// Makes a loop into a song: sections laid out one after another, each with its parts,
    /// marked, the chords carried along.
    Arrange(Arrangement),
    /// Copies bars of some tracks (all by default) to another place, replacing what's there.
    CopyBars {
        from: u64,
        bars: u64,
        to: u64,
        #[serde(default)]
        tracks: Option<Vec<usize>>,
    },
    /// A riser, reverse cymbal, impact, downlifter, drum fill, gap or a whole build into a bar.
    Transition(Transition),
    /// A finished song the mix aims at (the `reference` tool measures one and sets it); without
    /// one, the genre's targets.
    SetReference {
        #[serde(default)]
        reference: Option<crate::song::project::Reference>,
    },
    /// A modulator on a track (eight at most): an LFO, steps, an envelope, a follower, random, a
    /// macro.
    AddModulator {
        track: usize,
        modulator: crate::dsp::modulate::Modulator,
    },
    /// A modulator replaced (its routes kept unless given), or taken off.
    SetModulator {
        track: usize,
        index: usize,
        modulator: crate::dsp::modulate::Modulator,
    },
    RemoveModulator {
        track: usize,
        index: usize,
    },
    /// A modulator a stack shares: every track in it gets a `shared` modulator playing it, to
    /// route as it likes; or one taken off the stack, and off every track in it.
    ShareModulator {
        aux: usize,
        modulator: crate::dsp::modulate::Modulator,
    },
    UnshareModulator {
        aux: usize,
        index: usize,
    },
    /// A modulator moving a setting by `depth` in its units; 0 takes the route off.
    Modulate {
        track: usize,
        modulator: usize,
        target: crate::dsp::modulate::ModTarget,
        depth: f32,
    },
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelRef {
    Track(usize),
    Aux(usize),
    Master,
}

/// What a command that names a plugin's parameter, value or preset in words says when it hasn't
/// been through `resolve` (which the CLI and the app do).
const UNRESOLVED: &str = "send it through `pina do` or the app, which load it";

fn track(p: &mut Project, i: usize) -> Result<&mut Track, String> {
    let n = p.tracks.len();
    p.tracks
        .get_mut(i)
        .ok_or_else(|| format!("no track {i}: there are {n}"))
}

fn region_at(p: &Project, t: usize, r: usize) -> Result<(), String> {
    let n = p
        .tracks
        .get(t)
        .ok_or_else(|| format!("no track {t}: there are {}", p.tracks.len()))?
        .regions
        .len();
    if r < n {
        Ok(())
    } else {
        Err(format!("track {t} has no region {r}: it has {n}"))
    }
}

fn finite(name: &str, v: f32) -> Result<f32, String> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(format!("{name} must be a number"))
    }
}

/// Applies a command to the project in `dir`. Returns what an agent needs next, like the index
/// of what it made. Aliases of a region it changes change with it.
pub fn apply(p: &mut Project, dir: &Path, cmd: Command) -> Result<Value, String> {
    let mut links = p.link_contents();
    let result = run(p, dir, cmd);
    p.sync_links(&mut links);
    result
}

fn run(p: &mut Project, dir: &Path, cmd: Command) -> Result<Value, String> {
    match &cmd {
        Command::SetTempo { .. }
        | Command::SetMaster { .. }
        | Command::SetTimeSignature { .. }
        | Command::SetPalette { .. }
        | Command::AddMarker { .. }
        | Command::SetMarker { .. }
        | Command::DeleteMarker { .. }
        | Command::SetKey { .. }
        | Command::SetCycle { .. }
        | Command::SetReference { .. }
        | Command::SetGenre { .. } => song::run(p, dir, cmd),
        Command::AddTrack { .. }
        | Command::DeleteTrack { .. }
        | Command::SetTrack { .. }
        | Command::SetInstrument { .. }
        | Command::SetInsert { .. }
        | Command::SetInput { .. }
        | Command::SetOutput { .. }
        | Command::AddStack { .. }
        | Command::FlattenStack { .. } => tracks::run(p, dir, cmd),
        Command::AddMidiRegion { .. }
        | Command::SetControls { .. }
        | Command::SetNotes { .. }
        | Command::AddPatternRegion { .. }
        | Command::SetPattern { .. }
        | Command::AddAudioRegion { .. }
        | Command::MoveRegion { .. }
        | Command::TrimRegion { .. }
        | Command::SplitRegion { .. }
        | Command::SetCell { .. }
        | Command::FlexRegion { .. }
        | Command::Comp { .. }
        | Command::Flatten { .. }
        | Command::DeleteRegion { .. }
        | Command::SetRegion { .. }
        | Command::AddAlias { .. }
        | Command::Unlink { .. }
        | Command::EditNotes { .. } => regions::run(p, dir, cmd),
        Command::AddSend { .. }
        | Command::SetSend { .. }
        | Command::RemoveSend { .. }
        | Command::SetAux { .. }
        | Command::SetEq { .. }
        | Command::SetEqBand { .. }
        | Command::SetComp { .. }
        | Command::SetSaturator { .. } => mixing::run(p, dir, cmd),
        Command::SetAutomation { .. }
        | Command::SmoothAutomation { .. }
        | Command::ShapeAutomation { .. }
        | Command::SetAutomationMode { .. } => automation::run(p, dir, cmd),
        Command::SetPluginParam { .. }
        | Command::SetPreset { .. }
        | Command::SetSidechain { .. } => plugins::run(p, dir, cmd),
        Command::SetChords { .. }
        | Command::WriteChords(..)
        | Command::WriteBass(..)
        | Command::WriteMelody(..)
        | Command::WriteDrums(..)
        | Command::Arrange(..)
        | Command::CopyBars { .. }
        | Command::Transition(..) => writing::run(p, dir, cmd),
        Command::AddModulator { .. }
        | Command::SetModulator { .. }
        | Command::ShareModulator { .. }
        | Command::UnshareModulator { .. }
        | Command::RemoveModulator { .. }
        | Command::Modulate { .. } => modulation::run(p, dir, cmd),
    }
}

#[cfg(test)]
mod tests;
