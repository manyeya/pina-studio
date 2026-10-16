//! Edits as data: what an agent or a script sends tempo, through the CLI, a running app's socket,
//! or MCP. In the app each is one undoable step, the same as the edit made by hand.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    music::{Key, NoteEdit},
    project::{
        AUDIO_DIR, AutoMode, AutoPoint, AuxSend, BEAT_UNITS, Cell, Comp, Content, Cycle, Eq, Input,
        Instrument, Marker, Note, PPQ, Param, PatternRow, PluginRef, Project, Region, Saturator,
        Track, Which,
    },
};

/// What `tempo describe` prints: every command, for people and agents.
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
        volume_db: f32,
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
        #[serde(default)]
        volume_db: Option<f32>,
        #[serde(default)]
        pan: Option<f32>,
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
    /// quantized, its notes kept; and looping.
    SetRegion {
        track: usize,
        region: usize,
        #[serde(default)]
        name: Option<String>,
        #[serde(default, rename = "loop")]
        looped: Option<bool>,
        #[serde(default)]
        transpose: Option<i8>,
        #[serde(default)]
        velocity: Option<i8>,
        /// Ticks; 0 turns it off.
        #[serde(default)]
        quantize: Option<u64>,
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
    /// Replaces a track's volume (dB), pan (-1 to 1) or plugin parameter (0 to 1) automation; no
    /// points takes it out.
    SetAutomation {
        track: usize,
        param: AutoParam,
        /// For a plugin parameter: which plugin, and the parameter's id.
        #[serde(default)]
        plugin: Option<Which>,
        #[serde(default)]
        id: Option<u32>,
        #[serde(default)]
        name: Option<String>,
        points: Vec<AutoPoint>,
    },
    /// How a track follows its automation: off, read, touch or latch.
    SetAutomationMode {
        track: usize,
        mode: AutoMode,
    },
    /// Turns a plugin's parameter (0 to 1), by id; without `value`, tempo stops setting it.
    SetPluginParam {
        channel: ChannelRef,
        plugin: Which,
        param: u32,
        #[serde(default)]
        value: Option<f64>,
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
    /// The song's key, like "A minor"; without one, tempo works it out from the notes. With
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
    /// A channel's built-in EQ (low cut, two bells, high shelf); without `eq`, takes it out.
    /// Only the fields given change (see `set_effect`).
    SetEq {
        channel: ChannelRef,
        #[serde(default)]
        eq: Option<Value>,
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

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AutoParam {
    Volume,
    Pan,
    Plugin,
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
            p.tempo = bpm;
        }
        Command::SetMaster { volume_db } => p.master_volume_db = finite("volume_db", volume_db)?,
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
            volume_db,
            pan,
            mute,
            solo,
        } => {
            let t = track(p, t)?;
            if let Some(v) = name {
                t.name = v;
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
        } => {
            let tr = track(p, t)?;
            if tr.instrument.is_none() {
                return Err(format!(
                    "track {t} is an audio track: MIDI goes on instrument tracks"
                ));
            }
            tr.regions.push(Region {
                start,
                length: length.max(1),
                content: Content::Midi { notes },
                ..Default::default()
            });
            return Ok(json!({ "region": [t, tr.regions.len() - 1] }));
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
            let wav = hound::WavReader::open(dir.join(&file))
                .map_err(|e| format!("{}: {e}", file.display()))?;
            // Played at the project's rate, converted if it was recorded at another.
            let frames =
                wav.duration() as u64 * p.sample_rate as u64 / wav.spec().sample_rate as u64;
            let length = length.unwrap_or_else(|| p.ticks(frames.saturating_sub(offset)).max(1));
            let content = Content::Audio {
                file,
                offset,
                stretch: 1.0,
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
            transpose,
            velocity,
            quantize,
        } => {
            region_at(p, t, r)?;
            let region = &mut p.tracks[t].regions[r];
            if let Some(v) = name {
                region.name = v;
            }
            if let Some(v) = looped {
                region.looped = v;
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
            plugin,
            id,
            name,
            points,
        } => {
            let mut points = points;
            points.sort_by_key(|p| p.tick);
            let param = match param {
                AutoParam::Volume => Param::Volume,
                AutoParam::Pan => Param::Pan,
                AutoParam::Plugin => {
                    let (Some(plugin), Some(id)) = (plugin, id) else {
                        return Err("a plugin parameter needs `plugin` and `id`".into());
                    };
                    if track(p, t)?.plugin(plugin).is_none() {
                        return Err(format!("track {t} has no plugin there"));
                    }
                    for pt in &mut points {
                        pt.value = finite("value", pt.value)?.clamp(0.0, 1.0);
                    }
                    Param::Plugin(plugin, id)
                }
            };
            let a = &mut track(p, t)?.automation;
            *a.points_mut(param) = points;
            if let Some(name) = name {
                a.name(param, &name);
            }
            a.tidy();
        }
        Command::SetAutomationMode { track: t, mode } => track(p, t)?.automation.mode = mode,
        Command::SetPluginParam {
            channel,
            plugin,
            param,
            value,
        } => {
            let r = match channel {
                ChannelRef::Track(i) => track(p, i)?.plugin_mut(plugin),
                ChannelRef::Aux(i) => match plugin {
                    Which::Insert(n) => aux(p, i)?.inserts.get_mut(n),
                    Which::Instrument => None,
                },
                ChannelRef::Master => match plugin {
                    Which::Insert(n) => p.master_inserts.get_mut(n),
                    Which::Instrument => None,
                },
            }
            .ok_or("no plugin there")?;
            match value {
                Some(v) if v.is_finite() => drop(r.params.insert(param, v.clamp(0.0, 1.0))),
                Some(_) => return Err("value must be a number from 0 to 1".into()),
                None => drop(r.params.remove(&param)),
            }
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
        Command::SetEq { channel, eq } => set_effect(strip(p, channel)?.eq, eq)?,
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
            edit.apply(notes, note);
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
    let current = slot.clone().unwrap_or_default();
    let mut value = serde_json::to_value(current).map_err(|e| e.to_string())?;
    overlay(&mut value, changes);
    *slot = Some(serde_json::from_value(value).map_err(|e| e.to_string())?);
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
        let eq = p.tracks[0].eq.unwrap();
        assert_eq!(eq.low_cut, 150.0);
        // The shelf keeps its frequency and Q.
        assert_eq!((eq.high_shelf.freq, eq.high_shelf.gain_db), (10_000.0, 2.5));
        assert_eq!(eq.high_shelf.q, Eq::default().high_shelf.q);
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
        let base = std::env::temp_dir().join(format!("tempo-test-cmd-{}", std::process::id()));
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
        std::fs::remove_dir_all(base).unwrap();
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
            "add_marker",
            "set_marker",
            "delete_marker",
            "set_key",
            "set_cycle",
            "set_eq",
            "set_comp",
            "set_saturator",
            "edit_notes",
            "set_region",
            "add_alias",
            "unlink",
            "set_input",
            "set_automation_mode",
            "set_plugin_param",
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
