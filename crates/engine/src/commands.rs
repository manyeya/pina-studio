//! Edits as data: what an agent or a script sends tempo, through the CLI, a running app's socket,
//! or MCP. In the app each is one undoable step, the same as the edit made by hand.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::project::{
    AUDIO_DIR, AutoPoint, AuxSend, Cell, Content, Instrument, Note, PPQ, PatternRow, PluginRef,
    Project, Region, Track,
};

/// What `tempo describe` prints: every command, for people and agents.
pub const DESCRIBE: &str = include_str!("commands.md");

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SetTempo {
        bpm: f64,
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
    },
    /// Replaces a pattern region's rows.
    SetPattern {
        track: usize,
        region: usize,
        rows: Vec<PatternRow>,
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
    },
    SetSend {
        track: usize,
        send: usize,
        level_db: f32,
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
    /// Replaces a track's volume (dB) or pan (-1 to 1) automation; no points turns it off.
    SetAutomation {
        track: usize,
        param: AutoParam,
        points: Vec<AutoPoint>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelRef {
    Track(usize),
    Aux(usize),
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AutoParam {
    Volume,
    Pan,
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
/// of what it made.
pub fn apply(p: &mut Project, dir: &Path, cmd: Command) -> Result<Value, String> {
    match cmd {
        Command::SetTempo { bpm } => {
            if !(bpm.is_finite() && (20.0..=999.0).contains(&bpm)) {
                return Err("bpm must be between 20 and 999".into());
            }
            p.tempo = bpm;
        }
        Command::SetMaster { volume_db } => p.master_volume_db = finite("volume_db", volume_db)?,
        Command::AddTrack {
            name,
            instrument,
            index,
        } => {
            let index = index.unwrap_or(p.tracks.len()).min(p.tracks.len());
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
            let t = track(p, t)?;
            if t.regions
                .iter()
                .any(|r| matches!(r.content, Content::Audio { .. }))
            {
                return Err("an audio track with audio regions can't take an instrument".into());
            }
            t.instrument = Some(instrument);
        }
        Command::SetInsert {
            channel,
            index,
            plugin,
        } => {
            let list = match channel {
                ChannelRef::Track(i) => &mut track(p, i)?.inserts,
                ChannelRef::Aux(i) => &mut aux(p, i)?.inserts,
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
            };
            tr.regions.push(Region {
                start,
                length: length.max(1),
                content,
            });
            return Ok(json!({ "region": [t, tr.regions.len() - 1] }));
        }
        Command::SetPattern {
            track: t,
            region: r,
            rows: new,
        } => {
            region_at(p, t, r)?;
            match &mut p.tracks[t].regions[r].content {
                Content::Pattern { rows, .. } => *rows = new,
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
            if wav.spec().sample_rate != p.sample_rate {
                return Err(format!(
                    "{}: {} Hz, but the project is {} Hz",
                    file.display(),
                    wav.spec().sample_rate,
                    p.sample_rate
                ));
            }
            let frames = wav.duration() as u64;
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
        Command::AddSend {
            track: t,
            bus,
            level_db,
        } => {
            track(p, t)?;
            let bus = match bus {
                Some(b) if p.auxes.iter().any(|a| a.bus == b) => b,
                Some(b) => return Err(format!("no aux on bus {b}")),
                None => p.add_aux(),
            };
            let level_db = finite("level_db", level_db)?;
            track(p, t)?.sends.push(AuxSend { bus, level_db });
            return Ok(json!({ "bus": bus }));
        }
        Command::SetSend {
            track: t,
            send,
            level_db,
        } => {
            let level_db = finite("level_db", level_db)?;
            let s = track(p, t)?
                .sends
                .get_mut(send)
                .ok_or(format!("track {t} has no send {send}"))?;
            s.level_db = level_db;
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
            points,
        } => {
            let mut points = points;
            points.sort_by_key(|p| p.tick);
            let a = &mut track(p, t)?.automation;
            match param {
                AutoParam::Volume => a.volume = points,
                AutoParam::Pan => a.pan = points,
            }
        }
    }
    Ok(json!({}))
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
    let to = PathBuf::from(AUDIO_DIR).join(name);
    std::fs::create_dir_all(dir.join(AUDIO_DIR)).map_err(|e| e.to_string())?;
    if !dir.join(&to).exists() {
        std::fs::copy(file, dir.join(&to)).map_err(|e| format!("{shown}: {e}"))?;
    }
    Ok(to)
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
