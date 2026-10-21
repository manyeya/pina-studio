//! Edits as data: what an agent or a script sends Pina Studio, through the CLI, a running app's socket,
//! or MCP. In the app each is one undoable step, the same as the edit made by hand.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    arrange::{self, Arrangement, Transition},
    compose::{self, BassPart, ChordPart, DrumPart, MelodyPart},
    modulate::ModParam,
    music::{Key, NoteEdit},
    plugin::{PluginApi, PluginParam, value_from_text},
    project::{
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
        /// Logic's pan modes: balance, stereo (each side placed, `spread` apart) or binaural.
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
    /// Makes a summing stack of these tracks: a channel they all play into.
    AddStack {
        tracks: Vec<usize>,
        #[serde(default)]
        name: Option<String>,
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
        shape: Option<crate::project::Shape>,
        #[serde(default)]
        freq: Option<f32>,
        #[serde(default)]
        gain_db: Option<f32>,
        #[serde(default)]
        q: Option<f32>,
        #[serde(default)]
        slope: Option<f32>,
        #[serde(default)]
        placement: Option<crate::project::Placement>,
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
        reference: Option<crate::project::Reference>,
    },
    /// A modulator on a track (eight at most): an LFO, steps, an envelope, a follower, random, a
    /// macro.
    AddModulator {
        track: usize,
        modulator: crate::modulate::Modulator,
    },
    /// A modulator replaced (its routes kept unless given), or taken off.
    SetModulator {
        track: usize,
        index: usize,
        modulator: crate::modulate::Modulator,
    },
    RemoveModulator {
        track: usize,
        index: usize,
    },
    /// A modulator moving a setting by `depth` in its units; 0 takes the route off.
    Modulate {
        track: usize,
        modulator: usize,
        target: crate::modulate::ModTarget,
        depth: f32,
    },
}

fn yes() -> bool {
    true
}

fn modulator_at(
    p: &mut Project,
    t: usize,
    index: usize,
) -> Result<&mut crate::modulate::Modulator, String> {
    let track = track(p, t)?;
    let n = track.modulators.len();
    track
        .modulators
        .get_mut(index)
        .ok_or_else(|| format!("no modulator {index}: track {t} has {n}"))
}

/// A plugin parameter, by its id or its name.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum ParamRef {
    Id(u32),
    Name(String),
}

/// An automation point: its value, or its value as the plugin shows it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PointIn {
    pub tick: u64,
    #[serde(default)]
    pub value: Option<f32>,
    #[serde(default)]
    pub text: Option<String>,
    /// How the line bends on to the next point, -1 to 1.
    #[serde(default)]
    pub curve: Option<f32>,
    /// Holds its value up to the next point.
    #[serde(default)]
    pub step: Option<bool>,
    /// Eases out and into the next point: an S.
    #[serde(default)]
    pub ease: Option<bool>,
}

/// A preset, by name or where it comes from.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum PresetChoice {
    Name(String),
    Found(Preset),
    From(PresetSource),
}

/// What `resolve` asks of the plugins a command names: one, loaded.
pub trait PluginSource {
    fn plugin(
        &mut self,
        project: &Project,
        channel: ChannelRef,
        which: Which,
    ) -> Result<&dyn PluginApi, String>;
}

/// Puts what a command says in words into what Pina Studio keeps: a parameter's name into its id, a
/// value as the plugin shows it ("850 Hz") into its value, a preset's name into the preset.
/// Asks `plugins` for a plugin only when the command needs one.
pub fn resolve(
    p: &Project,
    cmd: Command,
    plugins: &mut dyn PluginSource,
) -> Result<Command, String> {
    Ok(match cmd {
        Command::SetPluginParam {
            channel,
            plugin,
            param,
            value,
            text,
        } if matches!(param, ParamRef::Name(_)) || text.is_some() => {
            let found = plugins.plugin(p, channel, plugin)?;
            let info = find_param(&found.params(), &param)?;
            let value = match text {
                Some(t) => Some(value_from_text(found, &info, &t)?),
                None => value,
            };
            Command::SetPluginParam {
                channel,
                plugin,
                param: ParamRef::Id(info.id),
                value,
                text: None,
            }
        }
        Command::SetAutomation {
            track,
            param,
            send,
            plugin: Some(which),
            id: Some(id),
            name,
            modulator,
            setting,
            points,
        } if matches!(id, ParamRef::Name(_)) || points.iter().any(|p| p.text.is_some()) => {
            let found = plugins.plugin(p, ChannelRef::Track(track), which)?;
            let info = find_param(&found.params(), &id)?;
            let points = points
                .into_iter()
                .map(|pt| {
                    Ok(PointIn {
                        value: match &pt.text {
                            Some(t) => Some(value_from_text(found, &info, t)? as f32),
                            None => pt.value,
                        },
                        text: None,
                        ..pt
                    })
                })
                .collect::<Result<_, String>>()?;
            Command::SetAutomation {
                track,
                param,
                send,
                plugin: Some(which),
                id: Some(ParamRef::Id(info.id)),
                name: name.or(Some(info.name)),
                modulator,
                setting,
                points,
            }
        }
        Command::SetPreset {
            channel,
            plugin,
            preset: Some(choice),
        } if !matches!(choice, PresetChoice::Found(_)) => {
            let found = plugins.plugin(p, channel, plugin)?;
            let presets = found.presets();
            let preset = match choice {
                PresetChoice::Name(name) => find_named(&presets, &name, |p| &p.name, "preset")?,
                PresetChoice::From(source) => presets
                    .into_iter()
                    .find(|p| p.source == source)
                    .unwrap_or_else(|| Preset {
                        name: match &source {
                            PresetSource::File { file } => file
                                .file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            PresetSource::Program { program, .. } => format!("Program {program}"),
                        },
                        source,
                    }),
                PresetChoice::Found(p) => p,
            };
            Command::SetPreset {
                channel,
                plugin,
                preset: Some(PresetChoice::Found(preset)),
            }
        }
        Command::SetSidechain {
            channel,
            insert,
            from: Some(from),
        } => {
            let found = plugins.plugin(p, channel, Which::Insert(insert))?;
            if !found.sidechain() {
                return Err(format!("{} has no sidechain input", found.name()));
            }
            Command::SetSidechain {
                channel,
                insert,
                from: Some(from),
            }
        }
        other => other,
    })
}

/// The plugin on a channel, if there's one there.
pub fn plugin_ref(p: &Project, channel: ChannelRef, which: Which) -> Option<&PluginRef> {
    match (channel, which) {
        (ChannelRef::Track(i), w) => p.tracks.get(i)?.plugin(w),
        (ChannelRef::Aux(i), Which::Insert(n)) => p.auxes.get(i)?.inserts.get(n),
        (ChannelRef::Master, Which::Insert(n)) => p.master_inserts.get(n),
        _ => None,
    }
}

/// A parameter by id, or by name: the same name ignoring case, else the only one whose name has
/// it in.
pub fn find_param(params: &[PluginParam], which: &ParamRef) -> Result<PluginParam, String> {
    match which {
        ParamRef::Id(id) => params
            .iter()
            .find(|p| p.id == *id)
            .cloned()
            .ok_or_else(|| format!("no parameter {id}")),
        ParamRef::Name(name) => find_named(params, name, |p| &p.name, "parameter"),
    }
}

fn find_named<T: Clone>(
    items: &[T],
    name: &str,
    of: impl Fn(&T) -> &String,
    what: &str,
) -> Result<T, String> {
    let lower = name.trim().to_lowercase();
    if let Some(t) = items.iter().find(|t| of(t).to_lowercase() == lower) {
        return Ok(t.clone());
    }
    let near: Vec<&T> = items
        .iter()
        .filter(|t| of(t).to_lowercase().contains(&lower))
        .collect();
    match near[..] {
        [one] => Ok(one.clone()),
        [] => Err(format!("no {what} called {name:?}")),
        _ => Err(format!(
            "{} {what}s have {name:?} in their names: {}",
            near.len(),
            near.iter()
                .take(8)
                .map(|t| format!("{:?}", of(t)))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelRef {
    Track(usize),
    Aux(usize),
    Master,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AutoParam {
    Volume,
    Pan,
    Mute,
    Send,
    Plugin,
    /// An EQ band's frequency (Hz), gain (dB) or width (Q): the band's place as `id`.
    EqFreq,
    EqGain,
    EqQ,
    /// One of a modulator's settings: the modulator's place as `modulator`, the setting as
    /// `setting`.
    Modulator,
}

/// An EQ band's setting, as automation names it: its place in the track's EQ.
fn eq_param(
    p: &mut Project,
    t: usize,
    param: AutoParam,
    band: Option<u32>,
) -> Result<Param, String> {
    let field = match param {
        AutoParam::EqFreq => crate::project::EqField::Freq,
        AutoParam::EqGain => crate::project::EqField::Gain,
        _ => crate::project::EqField::Q,
    };
    let Some(band) = band else {
        return Err("an EQ band's automation needs `id`: the band's place in the EQ".into());
    };
    let n = track(p, t)?.eq.map_or(0, |e| e.bands.len());
    if band as usize >= n {
        return Err(format!("track {t}'s EQ has no band {band}: it has {n}"));
    }
    Ok(Param::Eq(band as u8, field))
}

/// The parameter a command's automation is of.
#[allow(clippy::too_many_arguments)]
fn auto_param(
    p: &mut Project,
    t: usize,
    param: AutoParam,
    send: Option<usize>,
    plugin: Option<Which>,
    id: Option<u32>,
    modulator: Option<usize>,
    setting: Option<ModParam>,
) -> Result<Param, String> {
    Ok(match param {
        AutoParam::Volume => Param::Volume,
        AutoParam::Pan => Param::Pan,
        AutoParam::Mute => Param::Mute,
        AutoParam::Send => send_param(p, t, send)?,
        AutoParam::Plugin => {
            let (Some(plugin), Some(id)) = (plugin, id) else {
                return Err("a plugin parameter needs `plugin` and `id`".into());
            };
            if track(p, t)?.plugin(plugin).is_none() {
                return Err(format!("track {t} has no plugin there"));
            }
            Param::Plugin(plugin, id)
        }
        AutoParam::EqFreq | AutoParam::EqGain | AutoParam::EqQ => eq_param(p, t, param, id)?,
        AutoParam::Modulator => mod_param(p, t, modulator, setting)?,
    })
}

/// A modulator's setting, as automation names it: the modulator's place, and a setting it has.
fn mod_param(
    p: &mut Project,
    t: usize,
    modulator: Option<usize>,
    setting: Option<ModParam>,
) -> Result<Param, String> {
    let (Some(k), Some(setting)) = (modulator, setting) else {
        return Err(
            "a modulator's automation needs `modulator` (its place) and `setting` (value, phase, glide, attack, decay, sustain or release)"
                .into(),
        );
    };
    let m = modulator_at(p, t, k)?;
    if setting.get(&m.kind).is_none() {
        let has: Vec<&str> = ModParam::of(&m.kind)
            .into_iter()
            .map(ModParam::name)
            .collect();
        return Err(format!(
            "track {t}'s modulator {k} ({}) has no {}: it has {}",
            m.kind.name(),
            setting.name().to_lowercase(),
            match has.is_empty() {
                true => "none to automate".to_string(),
                false => has.join(", ").to_lowercase(),
            }
        ));
    }
    Ok(Param::Mod(k as u8, setting))
}

/// A track's send, as its automation names it: by the bus it sends to.
fn send_param(p: &mut Project, t: usize, send: Option<usize>) -> Result<Param, String> {
    let Some(i) = send else {
        return Err("a send's automation needs `send`: which of the track's sends".into());
    };
    let bus = track(p, t)?
        .sends
        .get(i)
        .ok_or_else(|| format!("track {t} has no send {i}"))?
        .bus;
    Ok(Param::Send(bus))
}

/// What a command that names a plugin's parameter, value or preset in words says when it hasn't
/// been through `resolve` (which the CLI and the app do).
const UNRESOLVED: &str = "send it through `pina do` or the app, which load it";

/// The plugin on a channel: a track's instrument or insert, an aux's or the master's insert.
fn plugin_mut(
    p: &mut Project,
    channel: ChannelRef,
    which: Which,
) -> Result<&mut PluginRef, String> {
    match channel {
        ChannelRef::Track(i) => track(p, i)?.plugin_mut(which),
        ChannelRef::Aux(i) => match which {
            Which::Insert(n) => aux(p, i)?.inserts.get_mut(n),
            Which::Instrument => None,
        },
        ChannelRef::Master => match which {
            Which::Insert(n) => p.master_inserts.get_mut(n),
            Which::Instrument => None,
        },
    }
    .ok_or_else(|| "no plugin there".to_string())
}

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

fn aux(p: &mut Project, i: usize) -> Result<&mut crate::project::Aux, String> {
    let n = p.auxes.len();
    p.auxes
        .get_mut(i)
        .ok_or_else(|| format!("no aux {i}: there are {n}"))
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
    match cmd {
        Command::SetTempo { bpm } => {
            if !(bpm.is_finite() && (20.0..=999.0).contains(&bpm)) {
                return Err("bpm must be between 20 and 999".into());
            }
            p.set_tempo(bpm);
        }
        Command::SetMaster { volume_db, pan_law } => {
            if let Some(v) = volume_db {
                p.master_volume_db = finite("volume_db", v)?;
            }
            if let Some(law) = pan_law {
                p.pan_law = law;
            }
        }
        Command::SetTimeSignature { beats, unit } => {
            if !(1..=32).contains(&beats) || !BEAT_UNITS.contains(&unit) {
                return Err("a time signature is 1 to 32 beats of a 1, 2, 4, 8 or 16".into());
            }
            p.time_signature = [beats, unit];
        }
        Command::AddTrack {
            name,
            instrument,
            index,
        } => {
            let index = index.unwrap_or(p.tracks.len()).min(p.tracks.len());
            let mut instrument = instrument;
            if let Some(i) = &mut instrument {
                import_instrument(dir, i)?;
            }
            p.add_track(
                index,
                Track {
                    name,
                    instrument,
                    ..Default::default()
                },
            );
            return Ok(json!({ "track": index }));
        }
        Command::DeleteTrack { track: t } => {
            track(p, t)?;
            p.tracks.remove(t);
        }
        Command::SetTrack {
            track: t,
            name,
            color,
            icon,
            volume_db,
            pan,
            pan_mode,
            spread,
            elevation,
            distance,
            mute,
            solo,
        } => {
            let t = track(p, t)?;
            if let Some(v) = pan_mode {
                t.pan_mode = v;
            }
            if let Some(v) = spread {
                t.spread = Some(finite("spread", v)?.clamp(-1.0, 1.0));
            }
            if let Some(v) = elevation {
                t.elevation = Some(finite("elevation", v)?.clamp(-90.0, 90.0));
            }
            if let Some(v) = distance {
                t.distance = Some(finite("distance", v)?.clamp(Place::NEAREST, Place::FARTHEST));
            }
            if let Some(v) = name {
                t.name = v;
            }
            if let Some(v) = color {
                t.color = match v.as_str() {
                    "auto" => None,
                    c if crate::look::parse_color(c).is_some() => Some(c.to_lowercase()),
                    c => return Err(format!("color: {c:?} isn't \"#rrggbb\" or \"auto\"")),
                };
            }
            if let Some(v) = icon {
                t.icon = match v.as_str() {
                    "auto" => None,
                    i if crate::look::icon(i).is_some() => Some(i.to_string()),
                    i => {
                        let ids: Vec<&str> = crate::look::ICONS.iter().map(|i| i.id).collect();
                        return Err(format!("icon: no icon {i:?}; there are {}", ids.join(", ")));
                    }
                };
            }
            if let Some(v) = volume_db {
                t.volume_db = finite("volume_db", v)?;
            }
            if let Some(v) = pan {
                t.pan = finite("pan", v)?.clamp(-1.0, 1.0);
            }
            if let Some(v) = mute {
                t.mute = v;
            }
            if let Some(v) = solo {
                t.solo = v;
            }
        }
        Command::SetInstrument {
            track: t,
            instrument,
        } => {
            let mut instrument = instrument;
            import_instrument(dir, &mut instrument)?;
            let t = track(p, t)?;
            if t.regions
                .iter()
                .any(|r| matches!(r.content, Content::Audio { .. }))
            {
                return Err("an audio track with audio regions can't take an instrument".into());
            }
            t.set_instrument(instrument);
        }
        Command::SetInsert {
            channel,
            index,
            plugin,
        } => {
            let list = match channel {
                ChannelRef::Track(i) => {
                    return track(p, i)?.set_insert(index, plugin).map(|_| json!({}));
                }
                ChannelRef::Aux(i) => &mut aux(p, i)?.inserts,
                ChannelRef::Master => &mut p.master_inserts,
            };
            match plugin {
                Some(plugin) if index < list.len() => list[index] = plugin,
                Some(plugin) => list.push(plugin),
                None if index < list.len() => drop(list.remove(index)),
                None => return Err(format!("no insert {index} to remove")),
            }
        }
        Command::AddMidiRegion {
            track: t,
            start,
            length,
            notes,
            mut controls,
        } => {
            let tr = track(p, t)?;
            if tr.instrument.is_none() {
                return Err(format!(
                    "track {t} is an audio track: MIDI goes on instrument tracks"
                ));
            }
            controls.sort_by_key(|c| c.start);
            tr.regions.push(Region {
                start,
                length: length.max(1),
                content: Content::Midi { notes, controls },
                ..Default::default()
            });
            return Ok(json!({ "region": [t, tr.regions.len() - 1] }));
        }
        Command::SetControls {
            track: t,
            region: r,
            mut controls,
        } => {
            region_at(p, t, r)?;
            controls.sort_by_key(|c| c.start);
            let only = "only MIDI regions have controller moves: that's audio, a pattern or takes";
            *p.controls_mut((t, r)).ok_or(only)? = controls;
        }
        Command::SetNotes {
            track: t,
            region: r,
            notes,
        } => {
            region_at(p, t, r)?;
            let only = "only MIDI regions have notes: that's audio, or a take folder";
            *p.notes_mut((t, r)).ok_or(only)? = notes;
        }
        Command::AddPatternRegion {
            track: t,
            start,
            length,
            step,
            rows,
            swing,
        } => {
            let tr = track(p, t)?;
            if tr.instrument.is_none() {
                return Err(format!(
                    "track {t} is an audio track: patterns go on instrument tracks"
                ));
            }
            let content = Content::Pattern {
                step: step.unwrap_or(PPQ / 4).max(1),
                rows,
                swing: finite("swing", swing.unwrap_or(50.0))?.clamp(50.0, 75.0),
            };
            tr.regions.push(Region {
                start,
                length: length.max(1),
                content,
                ..Default::default()
            });
            return Ok(json!({ "region": [t, tr.regions.len() - 1] }));
        }
        Command::SetPattern {
            track: t,
            region: r,
            rows: new,
            swing: new_swing,
        } => {
            region_at(p, t, r)?;
            match &mut p.tracks[t].regions[r].content {
                Content::Pattern { rows, swing, .. } => {
                    if let Some(new) = new {
                        *rows = new;
                    }
                    if let Some(v) = new_swing {
                        *swing = finite("swing", v)?.clamp(50.0, 75.0);
                    }
                }
                _ => return Err("that isn't a pattern region".into()),
            }
        }
        Command::AddAudioRegion {
            track: t,
            file,
            start,
            offset,
            length,
        } => {
            if track(p, t)?.instrument.is_some() {
                return Err(format!(
                    "track {t} is an instrument track: audio goes on audio tracks"
                ));
            }
            let file = import(dir, &file)?;
            // How long it is: a WAV's header says; AIFF, FLAC and MP3 are read.
            let (frames, from) = match hound::WavReader::open(dir.join(&file)) {
                Ok(wav) => (wav.duration() as u64, wav.spec().sample_rate as f64),
                Err(_) => {
                    let (sides, rate) = crate::engine::read_audio(&dir.join(&file))?;
                    (sides[0].len() as u64, rate)
                }
            };
            // Played at the project's rate, converted if it was recorded at another.
            let frames = (frames as f64 * p.sample_rate as f64 / from) as u64;
            let length = length.unwrap_or_else(|| p.ticks(frames.saturating_sub(offset)).max(1));
            let content = Content::Audio {
                file,
                offset,
                stretch: 1.0,
                flex: Flex::default(),
                follow: false,
                formants: false,
                warps: Vec::new(),
            };
            let regions = &mut track(p, t)?.regions;
            regions.push(Region {
                start,
                length,
                content,
                ..Default::default()
            });
            return Ok(json!({ "region": [t, regions.len() - 1] }));
        }
        Command::MoveRegion {
            track: t,
            region: r,
            start,
            to_track,
        } => {
            region_at(p, t, r)?;
            let to = to_track.unwrap_or(t);
            track(p, to)?;
            let midi = matches!(p.region((t, r)).content, Content::Midi { .. });
            if p.tracks[to].instrument.is_some() != midi {
                return Err(format!("track {to} can't play that region"));
            }
            let at = p.move_region((t, r), to, start);
            return Ok(json!({ "region": [at.0, at.1] }));
        }
        Command::TrimRegion {
            track: t,
            region: r,
            start,
            end,
        } => {
            region_at(p, t, r)?;
            if let Some(end) = end {
                p.trim_end((t, r), end);
            }
            if let Some(start) = start {
                p.trim_start((t, r), start);
            }
        }
        Command::SplitRegion {
            track: t,
            region: r,
            at,
        } => {
            region_at(p, t, r)?;
            let second = p
                .split_region((t, r), at)
                .ok_or("that tick isn't inside the region")?;
            return Ok(json!({ "region": [second.0, second.1] }));
        }
        Command::SetCell {
            track: t,
            scene,
            region,
        } => {
            let tr = track(p, t)?;
            if let Some(r) = &region
                && r.content.is_midi() != tr.instrument.is_some()
            {
                return Err(format!("track {t} can't play that loop"));
            }
            tr.cells.retain(|c| c.scene != scene);
            if let Some(region) = region {
                tr.cells.push(Cell {
                    scene,
                    region: Region { start: 0, ..region },
                });
            }
        }
        Command::FlexRegion {
            track: t,
            region: r,
            length,
        } => {
            region_at(p, t, r)?;
            p.flex((t, r), length)?;
        }
        Command::Comp {
            track: t,
            region: r,
            from,
            to,
            take,
        } => {
            region_at(p, t, r)?;
            p.comp((t, r), from, to, take)?;
        }
        Command::Flatten {
            track: t,
            region: r,
        } => {
            region_at(p, t, r)?;
            let regions: Vec<[usize; 2]> =
                p.flatten((t, r)).into_iter().map(|(a, b)| [a, b]).collect();
            return Ok(json!({ "regions": regions }));
        }
        Command::DeleteRegion {
            track: t,
            region: r,
        } => {
            region_at(p, t, r)?;
            p.tracks[t].regions.remove(r);
        }
        Command::SetRegion {
            track: t,
            region: r,
            name,
            looped,
            loop_length,
            fade_in,
            fade_out,
            transpose,
            velocity,
            quantize,
            flex,
            follow,
            formants,
            warps,
        } => {
            region_at(p, t, r)?;
            let region = &mut p.tracks[t].regions[r];
            if flex.is_some() || follow.is_some() || formants.is_some() || warps.is_some() {
                let Content::Audio {
                    flex: f,
                    follow: w,
                    formants: k,
                    warps: m,
                    ..
                } = &mut region.content
                else {
                    return Err("flex, follow, formants and warps are for audio regions".into());
                };
                *f = flex.unwrap_or(*f);
                *w = follow.unwrap_or(*w);
                *k = formants.unwrap_or(*k);
                if let Some(mut v) = warps {
                    v.sort();
                    *m = v;
                }
            }
            if let Some(v) = name {
                region.name = v;
            }
            if let Some(v) = looped {
                region.looped = v;
            }
            if let Some(v) = loop_length {
                region.loop_length = (v > 0).then_some(v);
                region.looped |= v > 0;
            }
            if let Some(v) = fade_in {
                region.fade_in = v.min(region.length);
            }
            if let Some(v) = fade_out {
                region.fade_out = v.min(region.length);
            }
            if let Some(v) = transpose {
                region.transpose = v.clamp(-96, 96);
            }
            if let Some(v) = velocity {
                region.velocity = v.clamp(-99, 99);
            }
            if let Some(v) = quantize {
                region.quantize = (v > 0).then_some(v);
            }
        }
        Command::AddAlias {
            track: t,
            region: r,
            start,
            to_track,
        } => {
            region_at(p, t, r)?;
            let to = to_track.unwrap_or(t);
            track(p, to)?;
            let at = p.alias((t, r), to, start)?;
            return Ok(json!({ "region": [at.0, at.1] }));
        }
        Command::Unlink {
            track: t,
            region: r,
        } => {
            region_at(p, t, r)?;
            p.unlink((t, r));
        }
        Command::AddSend {
            track: t,
            bus,
            level_db,
            pre,
        } => {
            track(p, t)?;
            let bus = match bus {
                Some(b) if p.auxes.iter().any(|a| a.bus == b) => b,
                Some(b) => return Err(format!("no aux on bus {b}")),
                None => p.add_aux(),
            };
            let level_db = finite("level_db", level_db)?;
            track(p, t)?.sends.push(AuxSend { bus, level_db, pre });
            return Ok(json!({ "bus": bus }));
        }
        Command::SetSend {
            track: t,
            send,
            level_db,
            pre,
        } => {
            let s = track(p, t)?
                .sends
                .get_mut(send)
                .ok_or(format!("track {t} has no send {send}"))?;
            if let Some(v) = level_db {
                s.level_db = finite("level_db", v)?;
            }
            if let Some(v) = pre {
                s.pre = v;
            }
        }
        Command::SetInput { track: t, input } => {
            let tr = track(p, t)?;
            if tr.instrument.is_some() {
                return Err(format!(
                    "track {t} is an instrument track: inputs are for audio tracks"
                ));
            }
            tr.input = input;
        }
        Command::SetOutput { track: t, bus } => {
            if let Some(b) = bus
                && !p.auxes.iter().any(|a| a.bus == b)
            {
                return Err(format!("no aux on bus {b}"));
            }
            track(p, t)?.output = bus;
        }
        Command::AddStack { tracks, name } => {
            if tracks.is_empty() {
                return Err("a stack needs tracks".into());
            }
            for &t in &tracks {
                track(p, t)?;
            }
            let bus = p.add_stack(&tracks);
            if let Some(name) = name {
                p.auxes.last_mut().unwrap().name = name;
            }
            return Ok(json!({ "bus": bus }));
        }
        Command::RemoveSend { track: t, send } => {
            let sends = &mut track(p, t)?.sends;
            if send >= sends.len() {
                return Err(format!("track {t} has no send {send}"));
            }
            sends.remove(send);
        }
        Command::SetAux {
            aux: a,
            name,
            volume_db,
            pan,
            mute,
        } => {
            let a = aux(p, a)?;
            if let Some(v) = name {
                a.name = v;
            }
            if let Some(v) = volume_db {
                a.volume_db = finite("volume_db", v)?;
            }
            if let Some(v) = pan {
                a.pan = finite("pan", v)?.clamp(-1.0, 1.0);
            }
            if let Some(v) = mute {
                a.mute = v;
            }
        }
        Command::SetAutomation {
            track: t,
            param,
            send,
            plugin,
            id,
            name,
            modulator,
            setting,
            points,
        } => {
            let mut points = points
                .into_iter()
                .map(|pt| match (pt.value, &pt.text) {
                    (Some(value), _) => Ok(AutoPoint {
                        curve: finite("curve", pt.curve.unwrap_or(0.0))?.clamp(-1.0, 1.0),
                        step: pt.step.unwrap_or(false),
                        ease: pt.ease.unwrap_or(false),
                        ..AutoPoint::new(pt.tick, finite("value", value)?)
                    }),
                    (None, Some(t)) => {
                        Err(format!("{t:?} needs the plugin to be read: {UNRESOLVED}"))
                    }
                    (None, None) => Err("each point needs a `value` (or a `text`)".to_string()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            points.sort_by_key(|p| p.tick);
            let param = match param {
                AutoParam::Volume => Param::Volume,
                AutoParam::Pan => Param::Pan,
                AutoParam::Mute => Param::Mute,
                AutoParam::Send => send_param(p, t, send)?,
                AutoParam::Plugin => {
                    let (Some(plugin), Some(id)) = (plugin, id) else {
                        return Err("a plugin parameter needs `plugin` and `id`".into());
                    };
                    let ParamRef::Id(id) = id else {
                        return Err(format!(
                            "a parameter by name needs the plugin: {UNRESOLVED}"
                        ));
                    };
                    if track(p, t)?.plugin(plugin).is_none() {
                        return Err(format!("track {t} has no plugin there"));
                    }
                    for pt in &mut points {
                        pt.value = finite("value", pt.value)?.clamp(0.0, 1.0);
                    }
                    Param::Plugin(plugin, id)
                }
                AutoParam::EqFreq | AutoParam::EqGain | AutoParam::EqQ => {
                    let band = match id {
                        Some(ParamRef::Id(n)) => Some(n),
                        Some(_) => return Err("an EQ band's `id` is its place: a number".into()),
                        None => None,
                    };
                    eq_param(p, t, param, band)?
                }
                AutoParam::Modulator => mod_param(p, t, modulator, setting)?,
            };
            let (lo, hi) = param.range();
            for pt in &mut points {
                pt.value = pt.value.clamp(lo, hi);
                pt.step |= param.steps();
            }
            let a = &mut track(p, t)?.automation;
            *a.points_mut(param) = points;
            if let Some(name) = name {
                a.name(param, &name);
            }
            a.tidy();
        }
        Command::SetPalette {
            sounds,
            devices,
            only,
        } => {
            if let Some(sounds) = sounds {
                let mut seen = std::collections::HashSet::new();
                for s in &sounds {
                    if !seen.insert(&s.path) {
                        return Err(format!("{} is in the palette twice", s.path.display()));
                    }
                }
                p.palette.sounds = sounds;
            }
            if let Some(devices) = devices {
                for (i, d) in devices.iter().enumerate() {
                    if devices[..i].iter().any(|e| e.same(d)) {
                        return Err(format!("{} is in the palette twice", d.name));
                    }
                }
                p.palette.devices = devices;
            }
            if let Some(only) = only {
                p.palette.only = only;
            }
        }
        Command::SmoothAutomation {
            track: t,
            param,
            send,
            plugin,
            id,
            modulator,
            setting,
            from,
            to,
            width,
        } => {
            let param = auto_param(p, t, param, send, plugin, id, modulator, setting)?;
            let a = &mut track(p, t)?.automation;
            let points = a.points(param);
            let (Some(first), Some(last)) = (points.first(), points.last()) else {
                return Err(format!("track {t} has no automation there to smooth"));
            };
            let (from, to) = (from.unwrap_or(first.tick), to.unwrap_or(last.tick));
            if to <= from {
                return Err("`to` must be after `from`".into());
            }
            a.smooth(param, from, to, width.unwrap_or(PPQ / 2));
        }
        Command::ShapeAutomation {
            track: t,
            param,
            send,
            plugin,
            id,
            modulator,
            setting,
            shape,
            from,
            to,
            every,
            low,
            high,
        } => {
            let param = auto_param(p, t, param, send, plugin, id, modulator, setting)?;
            if to <= from {
                return Err("`to` must be after `from`".into());
            }
            let every = every.unwrap_or(PPQ).max(1);
            let (low, high) = (finite("low", low)?, finite("high", high)?);
            track(p, t)?
                .automation
                .shape(param, shape, from, to, every, low, high);
        }
        Command::SetAutomationMode { track: t, mode } => track(p, t)?.automation.mode = mode,
        Command::SetPluginParam {
            channel,
            plugin,
            param,
            value,
            text,
        } => {
            let ParamRef::Id(param) = param else {
                return Err(format!(
                    "a parameter by name needs the plugin: {UNRESOLVED}"
                ));
            };
            if text.is_some() {
                return Err(format!("a value as text needs the plugin: {UNRESOLVED}"));
            }
            let r = plugin_mut(p, channel, plugin)?;
            match value {
                Some(v) if v.is_finite() => drop(r.params.insert(param, v.clamp(0.0, 1.0))),
                Some(_) => return Err("value must be a number from 0 to 1".into()),
                None => drop(r.params.remove(&param)),
            }
        }
        Command::SetPreset {
            channel,
            plugin,
            preset,
        } => {
            let preset = match preset {
                Some(PresetChoice::Found(preset)) => Some(preset),
                Some(_) => {
                    return Err(format!(
                        "a preset needs the plugin to be found: {UNRESOLVED}"
                    ));
                }
                None => None,
            };
            let r = plugin_mut(p, channel, plugin)?;
            // A preset is a whole new sound: what was set over the old one goes, and the old
            // settings with it.
            if preset.is_some() {
                r.params.clear();
                r.state = None;
            }
            r.preset = preset;
        }
        Command::SetSidechain {
            channel,
            insert,
            from,
        } => {
            let id = match from {
                Some(t) => Some(track(p, t)?.id),
                None => None,
            };
            plugin_mut(p, channel, Which::Insert(insert))?.sidechain = id;
        }
        Command::AddMarker { tick, name } => {
            let at = p.markers.partition_point(|m| m.tick <= tick);
            p.markers.insert(at, Marker { tick, name });
            return Ok(json!({ "marker": at }));
        }
        Command::SetMarker { marker, tick, name } => {
            let n = p.markers.len();
            let m = p
                .markers
                .get_mut(marker)
                .ok_or_else(|| format!("no marker {marker}: there are {n}"))?;
            if let Some(name) = name {
                m.name = name;
            }
            if let Some(tick) = tick {
                m.tick = tick;
                p.markers.sort_by_key(|m| m.tick);
            }
        }
        Command::DeleteMarker { marker } => {
            if marker >= p.markers.len() {
                return Err(format!("no marker {marker}: there are {}", p.markers.len()));
            }
            p.markers.remove(marker);
        }
        Command::SetKey { key, move_notes } => {
            let parsed = match key {
                None => None,
                Some(k) => Some(Key::parse(&k).ok_or_else(|| {
                    format!("{k:?} isn't a key: try \"A minor\" or \"Eb major\"")
                })?),
            };
            p.set_key(parsed, move_notes);
        }
        Command::SetCycle { start, end, on } => {
            if end <= start {
                return Err("a cycle's end must be after its start".into());
            }
            p.cycle = Some(Cycle {
                start,
                end,
                off: !on,
            });
        }
        Command::SetEq { channel, eq } => {
            // Loading a song takes the nearest; asked for, it's a mistake.
            let resolution = eq.as_ref().and_then(|e| e.get("resolution")?.as_f64());
            if let Some(r) = resolution.filter(|r| !(0.0..=3.0).contains(r)) {
                return Err(format!("resolution {r}: it's 0 (low) to 3 (very high)"));
            }
            // Its sidechain is a track, numbered as commands number them: its id here.
            let source = match eq.as_ref().and_then(|e| e.get("sidechain")) {
                Some(Value::Null) => Some(None),
                Some(v) => {
                    let t = v
                        .as_u64()
                        .ok_or("sidechain is a track's number (from 0), or null for none")?;
                    Some(Some(track(p, t as usize)?.id))
                }
                None => None,
            };
            let slot = strip(p, channel)?.eq;
            let mut new = *slot;
            set_effect(&mut new, eq)?;
            if let (Some(e), Some(source)) = (&mut new, source) {
                e.sidechain = source;
            }
            if let Some(e) = &new {
                check_eq(e)?;
            }
            *slot = new;
        }
        Command::SetEqBand {
            channel,
            band: i,
            shape,
            freq,
            gain_db,
            q,
            slope,
            placement,
            dynamic,
            off,
            gain_q,
            remove,
        } => {
            let slot = strip(p, channel)?.eq;
            let mut eq = slot.unwrap_or_default();
            let n = eq.bands.len();
            if i > n || (remove && i == n) {
                return Err(format!("no band {i}: the EQ has {n} (band {n} adds one)"));
            }
            if remove {
                eq.bands.remove(i);
            } else {
                let mut b = eq.bands.get(i).copied().unwrap_or_default();
                if let Some(shape) = shape {
                    // A new shape starts with its own width, unless one's given.
                    if shape != b.shape && q.is_none() {
                        b.q = shape.default_q();
                    }
                    b.shape = shape;
                }
                b.freq = freq.unwrap_or(b.freq);
                b.gain_db = gain_db.unwrap_or(b.gain_db);
                b.q = q.unwrap_or(b.q);
                b.slope = slope.unwrap_or(b.slope);
                b.placement = placement.unwrap_or(b.placement);
                b.off = off.unwrap_or(b.off);
                b.gain_q = gain_q.unwrap_or(b.gain_q);
                match dynamic {
                    None => {}
                    Some(Value::Bool(false)) | Some(Value::Null) => b.dynamic = None,
                    Some(changes) => {
                        let d = b.dynamic.unwrap_or_default();
                        let mut v = serde_json::to_value(d).map_err(|e| e.to_string())?;
                        overlay(&mut v, changes);
                        let d = serde_json::from_value::<Dynamic>(v).map_err(|e| e.to_string())?;
                        b.dynamic = Some(d);
                    }
                }
                match i == n {
                    true => {
                        eq.bands.push(b).ok_or("an EQ has at most 24 bands")?;
                    }
                    false => eq.bands[i] = b,
                }
            }
            check_eq(&eq)?;
            *slot = Some(eq);
        }
        Command::SetComp { channel, comp } => set_effect(strip(p, channel)?.comp, comp)?,
        Command::SetSaturator { channel, sat } => set_effect(strip(p, channel)?.sat, sat)?,
        Command::EditNotes {
            track: t,
            region: r,
            edit,
            note,
        } => {
            region_at(p, t, r)?;
            let only = "only MIDI regions have notes: that's audio, a pattern or a take folder";
            let notes = p.notes_mut((t, r)).ok_or(only)?;
            if note.is_some_and(|i| i >= notes.len()) {
                return Err(format!("that region has {} notes", notes.len()));
            }
            edit.apply(notes, note.as_slice());
        }
        Command::SetChords {
            chords,
            start,
            bars,
        } => return compose::set_chords(p, chords.as_deref(), start, bars),
        Command::WriteChords(w) => return compose::write_chords(p, w),
        Command::WriteBass(w) => return compose::write_bass(p, w),
        Command::WriteMelody(w) => return compose::write_melody(p, w),
        Command::WriteDrums(w) => return compose::write_drums(p, w),
        Command::Arrange(a) => return arrange::arrange(p, a),
        Command::CopyBars {
            from,
            bars,
            to,
            tracks,
        } => return arrange::copy_bars(p, from, bars, to, tracks),
        Command::Transition(t) => return arrange::transition(p, dir, t),
        Command::SetReference { reference } => p.reference = reference,
        Command::AddModulator {
            track: t,
            modulator,
        } => {
            let t = track(p, t)?;
            if t.modulators.len() >= crate::modulate::MAX_MODS {
                return Err(format!(
                    "a track has {} modulators at most",
                    crate::modulate::MAX_MODS
                ));
            }
            t.modulators.push(modulator);
        }
        Command::SetModulator {
            track: t,
            index,
            modulator,
        } => {
            let m = modulator_at(p, t, index)?;
            let routes = std::mem::take(&mut m.routes);
            *m = modulator;
            if m.routes.is_empty() {
                m.routes = routes;
            }
        }
        Command::RemoveModulator { track: t, index } => {
            modulator_at(p, t, index)?;
            p.tracks[t].modulators.remove(index);
            p.tracks[t].automation.modulator_removed(index);
        }
        Command::Modulate {
            track: t,
            modulator,
            target,
            depth,
        } => {
            let range = crate::modulate::depth_range(target);
            let depth = finite("depth", depth)?.clamp(-range, range);
            let m = modulator_at(p, t, modulator)?;
            m.routes.retain(|r| r.target != target);
            if depth != 0.0 {
                m.routes.push(crate::modulate::ModRoute { target, depth });
            }
        }
        Command::SetGenre { genre } => {
            p.genre = match genre {
                None => None,
                Some(g) => Some(
                    crate::playbook::find(&g)
                        .ok_or(format!(
                            "no playbook for {g:?}; there are {}",
                            crate::playbook::ids().join(", ")
                        ))?
                        .id
                        .to_string(),
                ),
            };
        }
    }
    Ok(json!({}))
}

/// Sets a built-in effect from the fields given (a band's too), the rest staying as they were,
/// or as the effect's defaults if it wasn't there. `None` takes it out.
fn set_effect<T>(slot: &mut Option<T>, changes: Option<Value>) -> Result<(), String>
where
    T: Serialize + serde::de::DeserializeOwned + Default + Clone,
{
    let Some(changes) = changes else {
        *slot = None;
        return Ok(());
    };
    let current = slot.clone().unwrap_or_default();
    let mut value = serde_json::to_value(current).map_err(|e| e.to_string())?;
    overlay(&mut value, changes);
    *slot = Some(serde_json::from_value(value).map_err(|e| e.to_string())?);
    Ok(())
}

/// JSON changes laid over JSON: objects merge, anything else replaces.
fn overlay(base: &mut Value, changes: Value) {
    match (base, changes) {
        (Value::Object(base), Value::Object(changes)) => {
            for (k, v) in changes {
                overlay(base.entry(k).or_insert(Value::Null), v);
            }
        }
        (base, changes) => *base = changes,
    }
}

/// An EQ's bands in their ranges: each says which is out and what it can be.
fn check_eq(eq: &Eq) -> Result<(), String> {
    for (i, b) in eq.bands.iter().enumerate() {
        let fail = |what: String| Err(format!("band {i}: {what}"));
        if !(10.0..=30_000.0).contains(&b.freq) {
            return fail(format!("freq {} Hz: it's 10 to 30000", b.freq));
        }
        if !(-30.0..=30.0).contains(&b.gain_db) {
            return fail(format!("gain_db {}: it's -30 to 30", b.gain_db));
        }
        if !(0.025..=40.0).contains(&b.q) {
            return fail(format!("q {}: it's 0.025 to 40", b.q));
        }
        if b.shape.slope_range().is_some() && b.shape.fit_slope(b.slope) != b.slope {
            let (lo, hi) = b.shape.slope_range().unwrap_or_default();
            let takes = match b.shape {
                crate::project::Shape::LowCut | crate::project::Shape::HighCut => {
                    format!("{lo} to {hi}, or 144 (a brickwall)")
                }
                crate::project::Shape::Notch | crate::project::Shape::AllPass => {
                    format!("{lo} to {hi} in twelves")
                }
                _ => format!("{lo} to {hi}"),
            };
            return fail(format!(
                "slope {} dB an octave: a {} takes {takes}",
                b.slope,
                b.shape.name().to_lowercase()
            ));
        }
        if let Some(d) = b.dynamic
            && (!(-30.0..=30.0).contains(&d.range_db)
                || d.threshold_db.is_some_and(|t| !(-80.0..=0.0).contains(&t))
                || !(0.0..=1.0).contains(&d.density))
        {
            return fail(
                "dynamic: range_db is -30 to 30, threshold_db -80 to 0, density 0 to 1".into(),
            );
        }
    }
    if !(0.0..=2.0).contains(&eq.scale) {
        return Err(format!("scale {}: it's 0 to 2", eq.scale));
    }

    Ok(())
}

/// A channel's built-in effects.
pub struct Strip<'a> {
    pub eq: &'a mut Option<Eq>,
    pub comp: &'a mut Option<Comp>,
    pub sat: &'a mut Option<Saturator>,
}

/// A channel's built-in EQ, compressor and saturator.
pub fn strip(p: &mut Project, channel: ChannelRef) -> Result<Strip<'_>, String> {
    Ok(match channel {
        ChannelRef::Track(i) => {
            let t = track(p, i)?;
            Strip {
                eq: &mut t.eq,
                comp: &mut t.comp,
                sat: &mut t.sat,
            }
        }
        ChannelRef::Aux(i) => {
            let a = aux(p, i)?;
            Strip {
                eq: &mut a.eq,
                comp: &mut a.comp,
                sat: &mut a.sat,
            }
        }
        ChannelRef::Master => Strip {
            eq: &mut p.master_eq,
            comp: &mut p.master_comp,
            sat: &mut p.master_sat,
        },
    })
}

/// Brings a WAV file into the project: one already under `dir` is used where it is, one elsewhere
/// is copied into `Audio Files`. Returns its path relative to `dir`.
fn import(dir: &Path, file: &Path) -> Result<PathBuf, String> {
    let shown = file.display();
    if file.is_relative() && dir.join(file).exists() {
        return Ok(file.to_path_buf());
    }
    let name = file
        .file_name()
        .ok_or_else(|| format!("{shown}: not a file"))?;
    std::fs::create_dir_all(dir.join(AUDIO_DIR)).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(file).map_err(|e| format!("{shown}: {e}"))?;
    // The same name from elsewhere, with other audio, gets a number: Kick.wav, Kick 2.wav.
    let stem = Path::new(name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let ext = Path::new(name)
        .extension()
        .unwrap_or_default()
        .to_string_lossy();
    for n in 1.. {
        let to = match n {
            1 => PathBuf::from(AUDIO_DIR).join(name),
            n => PathBuf::from(AUDIO_DIR).join(format!("{stem} {n}.{ext}")),
        };
        match std::fs::read(dir.join(&to)) {
            Ok(there) if there == bytes => return Ok(to),
            Ok(_) => continue,
            Err(_) => {
                std::fs::write(dir.join(&to), &bytes).map_err(|e| format!("{shown}: {e}"))?;
                return Ok(to);
            }
        }
    }
    unreachable!()
}

/// Brings a sampler's samples into the project, as `import` does audio.
fn import_instrument(dir: &Path, instrument: &mut Instrument) -> Result<(), String> {
    if let Instrument::Sampler(s) = instrument {
        for z in &mut s.zones {
            z.file = import(dir, &z.file)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_drum_pattern() {
        let mut p = Project::new(48_000);
        run(
            &mut p,
            r#"{"op": "add_track", "name": "Drums", "instrument": "drums"}"#,
        )
        .unwrap();
        let r = run(
            &mut p,
            r#"{"op": "add_pattern_region", "track": 0, "start": 0, "length": 3840,
                "rows": [{"pitch": 36, "cells": [100, 0, 0, 0, 100, 0, 0, 0]}]}"#,
        );
        assert_eq!(r, Ok(json!({"region": [0, 0]})));
        let set = r#"{"op": "set_pattern", "track": 0, "region": 0, "rows": [{"pitch": 38, "cells": [0, 0, 0, 0, 90]}]}"#;
        run(&mut p, set).unwrap();
        let notes = p.region((0, 0)).content.notes(3840).unwrap();
        // Five-step loop, a snare on step 5: sixteenths 4, 9 and 14 in a bar of 16.
        let starts: Vec<i64> = notes.iter().map(|n| n.start).collect();
        assert_eq!(starts, [960, 2160, 3360]);
    }

    fn run(p: &mut Project, json: &str) -> Result<Value, String> {
        let cmd: Command = serde_json::from_str(json).map_err(|e| e.to_string())?;
        apply(p, Path::new("/nonexistent"), cmd)
    }

    #[test]
    fn an_agent_can_build_a_song() {
        let mut p = Project::new(48_000);
        run(&mut p, r#"{"op": "set_tempo", "bpm": 96}"#).unwrap();
        let t = run(
            &mut p,
            r#"{"op": "add_track", "name": "Keys", "instrument": "synth"}"#,
        )
        .unwrap();
        assert_eq!(t, json!({"track": 0}));
        let r = run(
            &mut p,
            r#"{"op": "add_midi_region", "track": 0, "start": 0, "length": 3840,
                "notes": [{"start": 0, "length": 960, "pitch": 60, "velocity": 100}]}"#,
        )
        .unwrap();
        assert_eq!(r, json!({"region": [0, 0]}));
        let half = run(
            &mut p,
            r#"{"op": "split_region", "track": 0, "region": 0, "at": 1920}"#,
        )
        .unwrap();
        assert_eq!(half, json!({"region": [0, 1]}));
        let bus = run(&mut p, r#"{"op": "add_send", "track": 0, "level_db": -6}"#).unwrap();
        assert_eq!(bus, json!({"bus": 1}));
        run(&mut p, r#"{"op": "set_aux", "aux": 0, "name": "Verb"}"#).unwrap();
        run(
            &mut p,
            r#"{"op": "set_automation", "track": 0, "param": "volume",
                "points": [{"tick": 960, "value": -6}, {"tick": 0, "value": 0}]}"#,
        )
        .unwrap();
        assert_eq!(p.tempo, 96.0);
        assert_eq!(p.tracks[0].regions.len(), 2);
        assert_eq!(p.auxes[0].name, "Verb");
        assert_eq!(p.tracks[0].automation.volume[0].tick, 0);
        // Bent and stepped points; a send's level and the mute, by send and as steps.
        run(
            &mut p,
            r#"{"op": "set_automation", "track": 0, "param": "pan",
                "points": [{"tick": 0, "value": -1, "curve": 0.5}, {"tick": 960, "value": 1, "step": true}]}"#,
        )
        .unwrap();
        let pan = &p.tracks[0].automation.pan;
        assert_eq!((pan[0].curve, pan[1].step), (0.5, true));
        run(
            &mut p,
            r#"{"op": "set_automation", "track": 0, "param": "send", "send": 0,
                "points": [{"tick": 0, "value": -20}]}"#,
        )
        .unwrap();
        assert_eq!(p.tracks[0].automation.sends[0].bus, 1);
        let no_send = run(
            &mut p,
            r#"{"op": "set_automation", "track": 0, "param": "send", "send": 3, "points": []}"#,
        );
        assert!(no_send.unwrap_err().contains("no send 3"));
        run(
            &mut p,
            r#"{"op": "shape_automation", "track": 0, "param": "mute", "shape": "square",
                "from": 0, "to": 3840, "every": 960, "low": 0, "high": 1}"#,
        )
        .unwrap();
        let mute = &p.tracks[0].automation.mute;
        assert!(mute.len() >= 8 && mute.iter().all(|p| p.step));
        // An S, then smoothed: a gentle hill made of more points.
        run(
            &mut p,
            r#"{"op": "set_automation", "track": 0, "param": "volume",
                "points": [{"tick": 0, "value": -40, "ease": true}, {"tick": 1920, "value": 0}, {"tick": 3840, "value": -40}]}"#,
        )
        .unwrap();
        assert!(p.tracks[0].automation.volume[0].ease);
        run(
            &mut p,
            r#"{"op": "smooth_automation", "track": 0, "param": "volume", "width": 960}"#,
        )
        .unwrap();
        assert!(p.tracks[0].automation.volume.len() > 3);
        let backwards = run(
            &mut p,
            r#"{"op": "smooth_automation", "track": 0, "param": "volume", "from": 9, "to": 3}"#,
        );
        assert!(backwards.unwrap_err().contains("after"));
        // The person's palette: sounds with what they're for, these only; then only changed.
        run(
            &mut p,
            r#"{"op": "set_palette", "sounds": [{"path": "/s/kick.wav", "role": "kick"},
                {"path": "/s/keys.wav"}], "only": true}"#,
        )
        .unwrap();
        assert_eq!(p.palette.sounds.len(), 2);
        assert_eq!(p.palette.sounds[0].role.as_deref(), Some("kick"));
        assert!(p.palette.only && p.palette.has(Path::new("/s/keys.wav")));
        run(&mut p, r#"{"op": "set_palette", "only": false}"#).unwrap();
        assert_eq!((p.palette.sounds.len(), p.palette.only), (2, false));
        let twice = run(
            &mut p,
            r#"{"op": "set_palette", "sounds": [{"path": "/s/a.wav"}, {"path": "/s/a.wav"}]}"#,
        );
        assert!(twice.unwrap_err().contains("twice"));
        run(
            &mut p,
            r#"{"op": "set_palette", "devices": [{"name": "Sine Synth", "instrument": true,
                "own": "synth", "role": "pad"}, {"name": "Room", "plugin": {"class": "Room",
                "path": "/p/Room.vst3"}}]}"#,
        )
        .unwrap();
        assert_eq!(p.palette.devices.len(), 2);
        assert_eq!(p.palette.sounds.len(), 2);
        assert!(p.palette.has_device(&p.palette.devices[1].clone()));
        let twice = run(
            &mut p,
            r#"{"op": "set_palette", "devices": [{"name": "A", "own": "eq"}, {"name": "B", "own": "eq"}]}"#,
        );
        assert!(twice.unwrap_err().contains("twice"));
        run(
            &mut p,
            r#"{"op": "set_automation_mode", "track": 0, "mode": "trim"}"#,
        )
        .unwrap();
        assert_eq!(p.tracks[0].automation.mode, AutoMode::Trim);
    }

    /// A plugin that only shows things: a cutoff from 0 to 1000 Hz, a three-way wave, two
    /// presets. It reads no text itself, so `resolve` reads its display.
    struct Shown;

    impl PluginApi for Shown {
        fn name(&self) -> &str {
            "Shown"
        }
        fn params(&self) -> Vec<PluginParam> {
            let param = |id, name: &str, steps| PluginParam {
                id,
                name: name.into(),
                steps,
                ..Default::default()
            };
            vec![param(3, "Filter Cutoff", 0), param(9, "Wave", 2)]
        }
        fn set_param(&self, _: u32, _: f64) {}
        fn show_param(&self, _: u32, _: f64) {}
        fn param_value(&self, _: u32) -> f64 {
            0.0
        }
        fn param_text(&self, id: u32, v: f64) -> String {
            match id {
                3 => format!("{:.1} Hz", v * 1000.0),
                _ => ["Saw", "Square", "Sine"][(v * 2.0).round() as usize].into(),
            }
        }
        fn param_from_text(&self, _: u32, _: &str) -> Option<f64> {
            None
        }
        fn state(&self) -> Option<Vec<u8>> {
            None
        }
        fn set_state(&self, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
        fn presets(&self) -> Vec<Preset> {
            let program = |n| PresetSource::Program {
                list: 0,
                program: n,
            };
            vec![
                Preset {
                    name: "Warm Pad".into(),
                    source: program(0),
                },
                Preset {
                    name: "Sub Bass".into(),
                    source: program(1),
                },
            ]
        }
        fn load_preset(&self, _: &Preset) -> Result<(), String> {
            Ok(())
        }
        fn save_preset(&self, _: &str) -> Result<PathBuf, String> {
            Err("no".into())
        }
        fn open_editor(&self, _: objc2::MainThreadMarker) -> Result<crate::plugin::Editor, String> {
            Err("no".into())
        }
    }

    impl PluginSource for Shown {
        fn plugin(
            &mut self,
            _: &Project,
            _: ChannelRef,
            _: Which,
        ) -> Result<&dyn PluginApi, String> {
            Ok(self)
        }
    }

    #[test]
    fn parameters_and_presets_are_named_in_words() {
        let mut p = Project::new(48_000);
        run(
            &mut p,
            r#"{"op": "add_track", "name": "Pad", "instrument": "synth"}"#,
        )
        .unwrap();
        p.tracks[0].instrument = Some(Instrument::Plugin(PluginRef::default()));
        let said = |p: &mut Project, json: &str| {
            let cmd: Command = serde_json::from_str(json).unwrap();
            let cmd = resolve(p, cmd, &mut Shown)?;
            apply(p, Path::new("."), cmd)
        };
        let ch = r#""channel": {"track": 0}, "plugin": "instrument""#;
        let set = |what: &str| format!(r#"{{"op": "set_plugin_param", {ch}, {what}}}"#);
        said(&mut p, &set(r#""param": "cutoff", "text": "250 Hz""#)).unwrap();
        said(&mut p, &set(r#""param": "Wave", "text": "square""#)).unwrap();
        let r = p.tracks[0].plugin(Which::Instrument).unwrap().clone();
        assert!((r.params.get(&3).unwrap() - 0.25).abs() < 1e-3);
        assert_eq!(r.params.get(&9), Some(&0.5));
        // Set in that order, and kept in it.
        assert_eq!(
            r.params.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [3, 9]
        );
        let wrong = said(&mut p, &set(r#""param": "Wave", "text": "Triangle""#));
        assert!(wrong.unwrap_err().contains("Saw, Square, Sine"));
        let far = said(&mut p, &set(r#""param": "cutoff", "text": "5 kHz""#));
        assert!(far.unwrap_err().contains("out of its range"));
        // A preset by part of its name: it replaces what was set.
        let preset = format!(r#"{{"op": "set_preset", {ch}, "preset": "sub"}}"#);
        said(&mut p, &preset).unwrap();
        let r = p.tracks[0].plugin(Which::Instrument).unwrap();
        assert_eq!(r.preset.as_ref().unwrap().name, "Sub Bass");
        assert!(r.params.is_empty());
        // Automation points by text, the parameter by name.
        let auto = r#"{"op": "set_automation", "track": 0, "param": "plugin", "plugin": "instrument",
            "id": "Filter Cutoff", "points": [{"tick": 0, "text": "100 Hz"}, {"tick": 960, "value": 1}]}"#;
        said(&mut p, auto).unwrap();
        let lane = &p.tracks[0].automation.plugins[0];
        assert_eq!((lane.param, lane.name.as_str()), (3, "Filter Cutoff"));
        assert!((lane.points[0].value - 0.1).abs() < 1e-3);
        // Unresolved, a name can't be applied.
        let raw: Command = serde_json::from_str(&set(r#""param": "Wave", "value": 1"#)).unwrap();
        assert!(apply(&mut p, Path::new("."), raw).is_err());
    }

    #[test]
    fn effects_change_only_what_is_given() {
        let mut p = Project::new(48_000);
        run(&mut p, r#"{"op": "add_track", "name": "Keys"}"#).unwrap();
        let ch = r#""channel": {"track": 0}"#;
        run(
            &mut p,
            &format!(r#"{{"op": "set_eq", {ch}, "eq": {{"low_cut": 150}}}}"#),
        )
        .unwrap();
        let shelf = r#"{"high_shelf": {"gain_db": 2.5}}"#;
        run(
            &mut p,
            &format!(r#"{{"op": "set_eq", {ch}, "eq": {shelf}}}"#),
        )
        .unwrap();
        // The old EQ's names: a low cut and a shelf, the shelf at its frequency and Q.
        let eq = p.tracks[0].eq.unwrap();
        let shapes: Vec<_> = eq
            .bands
            .iter()
            .map(|b| (b.shape, b.freq, b.gain_db))
            .collect();
        assert_eq!(
            shapes,
            [
                (crate::project::Shape::LowCut, 150.0, 0.0),
                (crate::project::Shape::HighShelf, 10_000.0, 2.5)
            ]
        );
        assert_eq!(eq.bands[1].q, std::f32::consts::FRAC_1_SQRT_2);
        // Bands by place: one changed, one added, a dynamic one, one taken out.
        let band = |b: &str| format!(r#"{{"op": "set_eq_band", {ch}, {b}}}"#);
        run(&mut p, &band(r#""band": 0, "slope": 24"#)).unwrap();
        run(
            &mut p,
            &band(r#""band": 2, "freq": 300, "gain_db": -3, "q": 1.5, "placement": "mid""#),
        )
        .unwrap();
        run(
            &mut p,
            &band(r#""band": 2, "dynamic": {"range_db": -4, "threshold_db": -24}"#),
        )
        .unwrap();
        let eq = p.tracks[0].eq.unwrap();
        assert_eq!(eq.bands[0].slope, 24.0);
        let b = eq.bands[2];
        assert_eq!((b.freq, b.gain_db, b.q), (300.0, -3.0, 1.5));
        assert_eq!(b.placement, crate::project::Placement::Mid);
        assert_eq!(b.dynamic.unwrap().range_db, -4.0);
        // Its attack and release its own, by its frequency.
        assert_eq!(b.dynamic.unwrap().attack_ms, None);
        assert_eq!(b.dynamic.unwrap().threshold_db, Some(-24.0));
        let wrong = run(&mut p, &band(r#""band": 0, "slope": 120"#)).unwrap_err();
        assert!(wrong.contains("band 0: slope 120"), "{wrong}");
        // Fractional slopes, on any shape: a bell steeper.
        run(&mut p, &band(r#""band": 0, "slope": 20.5"#)).unwrap();
        run(&mut p, &band(r#""band": 2, "slope": 36"#)).unwrap();
        let eq = p.tracks[0].eq.unwrap();
        assert_eq!((eq.bands[0].slope, eq.bands[2].slope), (20.5, 36.0));
        run(&mut p, &band(r#""band": 0, "slope": 24"#)).unwrap();
        // Spectral, and the EQ in linear phase at high resolution.
        run(
            &mut p,
            &band(r#""band": 2, "dynamic": {"spectral": true, "density": 0.8, "tilt": false}"#),
        )
        .unwrap();
        let d = p.tracks[0].eq.unwrap().bands[2].dynamic.unwrap();
        assert!(
            d.spectral && !d.tilt && d.density == 0.8 && d.range_db == -4.0,
            "{d:?}"
        );
        let set = |eq: &str| format!(r#"{{"op": "set_eq", {ch}, "eq": {eq}}}"#);
        run(&mut p, &set(r#"{"processing": "linear", "resolution": 2}"#)).unwrap();
        let eq = p.tracks[0].eq.unwrap();
        assert_eq!(
            (eq.processing, eq.resolution, eq.bands.len()),
            (crate::project::Processing::Linear, 2, 3)
        );
        let wrong = run(&mut p, &set(r#"{"resolution": 7}"#)).unwrap_err();
        assert!(wrong.contains("resolution 7"), "{wrong}");
        let wrong = run(&mut p, &band(r#""band": 2, "dynamic": {"density": 2}"#)).unwrap_err();
        assert!(wrong.contains("density"), "{wrong}");
        run(&mut p, &set(r#"{"processing": "zero_latency"}"#)).unwrap();
        assert!(
            run(&mut p, &band(r#""band": 5"#))
                .unwrap_err()
                .contains("no band 5")
        );
        run(&mut p, &band(r#""band": 1, "remove": true"#)).unwrap();
        assert_eq!(p.tracks[0].eq.unwrap().bands.len(), 2);
        // The whole list at once.
        run(
            &mut p,
            &format!(
                r#"{{"op": "set_eq", {ch}, "eq": {{"bands": [{{"shape": "high_cut", "freq": 9000, "slope": 48}}], "output_db": -2}}}}"#
            ),
        )
        .unwrap();
        let eq = p.tracks[0].eq.unwrap();
        assert_eq!(
            (eq.bands.len(), eq.bands[0].slope, eq.output_db),
            (1, 48.0, -2.0)
        );
        // A cut written without a width has a cut's: no bump at its corner.
        assert_eq!(eq.bands[0].q, std::f32::consts::FRAC_1_SQRT_2);
        let comp = r#"{"threshold_db": -9, "ratio": 2, "rms": true}"#;
        run(
            &mut p,
            &format!(r#"{{"op": "set_comp", {ch}, "comp": {comp}}}"#),
        )
        .unwrap();
        run(
            &mut p,
            &format!(r#"{{"op": "set_comp", {ch}, "comp": {{"makeup_db": 6}}}}"#),
        )
        .unwrap();
        let c = p.tracks[0].comp.unwrap();
        assert_eq!(
            (c.threshold_db, c.ratio, c.makeup_db, c.rms),
            (-9.0, 2.0, 6.0, true)
        );
        let sat = r#"{"drive_db": 7, "mix": 0.45}"#;
        run(
            &mut p,
            &format!(r#"{{"op": "set_saturator", {ch}, "sat": {sat}}}"#),
        )
        .unwrap();
        run(
            &mut p,
            &format!(r#"{{"op": "set_saturator", {ch}, "sat": {{"tone": 0.1}}}}"#),
        )
        .unwrap();
        let s = p.tracks[0].sat.unwrap();
        assert_eq!((s.drive_db, s.mix, s.tone), (7.0, 0.45, 0.1));
        // Bypassed and back, and taken out.
        run(
            &mut p,
            &format!(r#"{{"op": "set_comp", {ch}, "comp": {{"bypass": true}}}}"#),
        )
        .unwrap();
        assert!(p.tracks[0].comp.unwrap().bypass);
        assert_eq!(p.tracks[0].comp.unwrap().ratio, 2.0);
        run(&mut p, &format!(r#"{{"op": "set_comp", {ch}}}"#)).unwrap();
        assert!(p.tracks[0].comp.is_none());
        assert!(
            run(
                &mut p,
                &format!(r#"{{"op": "set_eq", {ch}, "eq": {{"low_cut": "x"}}}}"#)
            )
            .is_err()
        );
    }

    #[test]
    fn mistakes_say_what_went_wrong() {
        let mut p = Project::new(48_000);
        assert_eq!(
            run(&mut p, r#"{"op": "delete_track", "track": 2}"#),
            Err("no track 2: there are 0".into())
        );
        run(&mut p, r#"{"op": "add_track", "name": "Vox"}"#).unwrap(); // an audio track
        let midi = run(
            &mut p,
            r#"{"op": "add_midi_region", "track": 0, "start": 0, "length": 960}"#,
        );
        assert!(midi.unwrap_err().contains("audio track"));
        // A misspelt field is refused, not ignored.
        let typo = run(&mut p, r#"{"op": "set_track", "track": 0, "volume": -6}"#);
        assert!(typo.unwrap_err().contains("volume"));
        assert!(run(&mut p, r#"{"op": "set_tempo", "bpm": 0}"#).is_err());
        assert!(run(&mut p, r#"{"op": "add_send", "track": 0, "bus": 9}"#).is_err());
    }

    #[test]
    fn audio_from_elsewhere_is_copied_in() {
        let base = std::env::temp_dir().join(format!("pina-test-cmd-{}", std::process::id()));
        let (dir, outside) = (base.join("song"), base.join("take.wav"));
        std::fs::create_dir_all(&dir).unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&outside, spec).unwrap();
        (0..24_000).for_each(|_| w.write_sample(0i16).unwrap());
        w.finalize().unwrap();

        let mut p = Project::new(48_000);
        run(&mut p, r#"{"op": "add_track", "name": "Vox"}"#).unwrap();
        let cmd = Command::AddAudioRegion {
            track: 0,
            file: outside,
            start: PPQ,
            offset: 0,
            length: None,
        };
        apply(&mut p, &dir, cmd).unwrap();
        let r = p.region((0, 0));
        assert_eq!((r.start, r.length), (PPQ, PPQ)); // half a second at 120 bpm is a beat
        assert!(dir.join("Audio Files/take.wav").exists());
        // Sliced, and following the tempo: at 60 it plays twice as long, still a beat.
        run(
            &mut p,
            r#"{"op": "set_region", "track": 0, "region": 0, "flex": "slicing", "follow": true}"#,
        )
        .unwrap();
        run(&mut p, r#"{"op": "set_tempo", "bpm": 60}"#).unwrap();
        let r = p.region((0, 0));
        assert!(matches!(
            r.content,
            Content::Audio {
                flex: Flex::Slicing,
                follow: true,
                stretch: 2.0,
                ..
            }
        ));
        assert_eq!(r.length, PPQ);
        // MIDI has no flex.
        run(
            &mut p,
            r#"{"op": "add_track", "name": "Keys", "instrument": "synth"}"#,
        )
        .unwrap();
        run(
            &mut p,
            r#"{"op": "add_midi_region", "track": 1, "start": 0, "length": 960, "notes": []}"#,
        )
        .unwrap();
        let err = run(
            &mut p,
            r#"{"op": "set_region", "track": 1, "region": 0, "flex": "speed"}"#,
        );
        assert!(err.unwrap_err().contains("audio"));
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn a_modulators_settings_are_automated_and_go_with_it() {
        use crate::modulate::ModParam;
        let mut p = Project::new(48_000);
        run(
            &mut p,
            r#"{"op": "add_track", "name": "Pad", "instrument": "synth"}"#,
        )
        .unwrap();
        for m in [
            r#"{"type": "lfo", "shape": "sine", "rate": {"beats": 1}}"#,
            r#"{"type": "macro"}"#,
        ] {
            run(
                &mut p,
                &format!(r#"{{"op": "add_modulator", "track": 0, "modulator": {m}}}"#),
            )
            .unwrap();
        }
        run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "modulator", "modulator": 1, "setting": "value", "points": [{"tick": 0, "value": 0}, {"tick": 3840, "value": 2}]}"#).unwrap();
        let lane = p.tracks[0]
            .automation
            .points(Param::Mod(1, ModParam::Value));
        // Kept to its range: a macro's value is 0 to 1.
        assert_eq!(lane.iter().map(|x| x.value).collect::<Vec<_>>(), [0.0, 1.0]);
        // A setting it hasn't: said, with what it has.
        let e = run(&mut p, r#"{"op": "set_automation", "track": 0, "param": "modulator", "modulator": 1, "setting": "attack", "points": []}"#).unwrap_err();
        assert!(e.contains("has value"), "{e}");
        run(&mut p, r#"{"op": "shape_automation", "track": 0, "param": "modulator", "modulator": 0, "setting": "phase", "shape": "ramp_up", "from": 0, "to": 3840, "low": 0, "high": 1}"#).unwrap();
        assert!(
            !p.tracks[0]
                .automation
                .points(Param::Mod(0, ModParam::Phase))
                .is_empty()
        );
        // The LFO taken off: its lane goes, and the macro's moves up a place with it.
        run(
            &mut p,
            r#"{"op": "remove_modulator", "track": 0, "index": 0}"#,
        )
        .unwrap();
        assert!(
            p.tracks[0]
                .automation
                .points(Param::Mod(0, ModParam::Phase))
                .is_empty()
        );
        assert_eq!(
            p.tracks[0]
                .automation
                .points(Param::Mod(0, ModParam::Value))
                .len(),
            2
        );
    }

    #[test]
    fn modulators_are_added_routed_and_removed() {
        use crate::modulate::{ModKind, ModTarget, SamplerParam};
        let mut p = Project::new(48_000);
        run(
            &mut p,
            r#"{"op": "add_track", "name": "Bass", "instrument": "synth"}"#,
        )
        .unwrap();
        run(
            &mut p,
            r#"{"op": "add_modulator", "track": 0, "modulator": {"type": "lfo", "shape": "square", "rate": {"beats": 0.5}}}"#,
        )
        .unwrap();
        run(&mut p, r#"{"op": "modulate", "track": 0, "modulator": 0, "target": {"sampler": "cutoff"}, "depth": 2}"#).unwrap();
        run(
            &mut p,
            r#"{"op": "modulate", "track": 0, "modulator": 0, "target": "pan", "depth": 5}"#,
        )
        .unwrap();
        let m = &p.tracks[0].modulators[0];
        assert!(matches!(m.kind, ModKind::Lfo { .. }));
        let routes: Vec<_> = m.routes.iter().map(|r| (r.target, r.depth)).collect();
        assert_eq!(
            routes,
            [
                (ModTarget::Sampler(SamplerParam::Cutoff), 2.0),
                (ModTarget::Pan, 2.0)
            ]
        );
        run(
            &mut p,
            r#"{"op": "modulate", "track": 0, "modulator": 0, "target": "pan", "depth": 0}"#,
        )
        .unwrap();
        assert_eq!(p.tracks[0].modulators[0].routes.len(), 1);
        let err = run(
            &mut p,
            r#"{"op": "remove_modulator", "track": 0, "index": 3}"#,
        );
        assert!(err.unwrap_err().contains("no modulator 3"));
        run(
            &mut p,
            r#"{"op": "remove_modulator", "track": 0, "index": 0}"#,
        )
        .unwrap();
        assert!(p.tracks[0].modulators.is_empty());
    }

    #[test]
    fn pan_modes_spread_and_the_pan_law() {
        let mut p = Project::new(48_000);
        run(
            &mut p,
            r#"{"op": "add_track", "name": "Pad", "instrument": "synth"}"#,
        )
        .unwrap();
        run(
            &mut p,
            r#"{"op": "set_track", "track": 0, "pan_mode": "stereo", "spread": 0.4}"#,
        )
        .unwrap();
        run(
            &mut p,
            r#"{"op": "set_master", "pan_law": "minus3_compensated"}"#,
        )
        .unwrap();
        let t = &p.tracks[0];
        assert_eq!((t.pan_mode, t.spread), (PanMode::Stereo, Some(0.4)));
        assert_eq!(
            (p.pan_law, p.master_volume_db),
            (PanLaw::Minus3Compensated, 0.0)
        );
    }

    #[test]
    fn every_command_is_described() {
        let samples = [
            "set_tempo",
            "set_time_signature",
            "set_master",
            "add_track",
            "delete_track",
            "set_track",
            "set_instrument",
            "set_insert",
            "add_midi_region",
            "set_notes",
            "add_pattern_region",
            "set_pattern",
            "add_audio_region",
            "move_region",
            "trim_region",
            "split_region",
            "delete_region",
            "flex_region",
            "set_cell",
            "comp",
            "flatten",
            "add_send",
            "set_send",
            "set_output",
            "add_stack",
            "remove_send",
            "set_aux",
            "set_automation",
            "shape_automation",
            "smooth_automation",
            "set_palette",
            "add_marker",
            "set_marker",
            "delete_marker",
            "set_key",
            "set_cycle",
            "set_eq",
            "set_eq_band",
            "set_comp",
            "set_saturator",
            "edit_notes",
            "set_region",
            "add_alias",
            "unlink",
            "set_input",
            "set_automation_mode",
            "set_plugin_param",
            "set_controls",
            "set_preset",
            "set_sidechain",
            "set_chords",
            "write_chords",
            "write_bass",
            "write_melody",
            "write_drums",
            "set_genre",
            "arrange",
            "copy_bars",
            "transition",
            "set_reference",
            "add_modulator",
            "set_modulator",
            "remove_modulator",
            "modulate",
        ];
        for op in samples {
            assert!(
                DESCRIBE.contains(&format!("`{op}`")),
                "{op} isn't in commands.md"
            );
        }
        // And the list above is complete: an unknown op names every real one.
        let err = serde_json::from_str::<Command>(r#"{"op": "nope"}"#)
            .unwrap_err()
            .to_string();
        for op in samples {
            assert!(err.contains(op), "{op} isn't a command");
        }
        assert_eq!(err.matches('`').count() / 2, samples.len() + 1); // + the unknown one
    }
}
