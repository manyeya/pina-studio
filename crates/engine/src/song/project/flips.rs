//! A flip made real: a Sampler a chop a zone (its words in the vocal, pitched and reversed as the
//! flip has them, one at a time and stopping with their notes), and a looping MIDI region playing
//! them where the flip puts them.

use std::path::Path;

use super::*;
use crate::lyrics::flips::Flip;

/// The bars a flip plays for, looping, when it's put in the song.
const FLIP_PLAYS: u64 = 8;

impl Project {
    /// `flip` on a Sampler track of its own after the track of the region at `at`, its chops
    /// from `file` (`seconds` long: the region's own, or its vocals stem), its region at `tick`
    /// looping for 8 bars (or its length, if longer). Its track; none if it plays nothing.
    pub fn flip_words(
        &mut self,
        at: At,
        file: &Path,
        seconds: f32,
        flip: &Flip,
        tick: u64,
    ) -> Option<usize> {
        if flip.hits.is_empty() || seconds <= 0.0 || at.0 >= self.tracks.len() {
            return None;
        }
        let most = (127 - FIRST_SLICE) as usize + 1;
        let mut zones: Vec<Zone> = Vec::new();
        let mut made: Vec<(u32, u32, bool, i8)> = Vec::new();
        let mut notes = Vec::new();
        for h in &flip.hits {
            let key = (h.from.to_bits(), h.to.to_bits(), h.reverse, h.pitch);
            // A chop played again is the same zone: one a sound, as on an MPC's pads.
            let k = match made.iter().position(|m| *m == key) {
                Some(k) => k,
                None if zones.len() < most => {
                    let note = FIRST_SLICE + zones.len() as u8;
                    made.push(key);
                    zones.push(Zone {
                        file: file.to_path_buf(),
                        name: match h.words.is_empty() {
                            true => format!("Chop {}", zones.len() + 1),
                            false => h.words.clone(),
                        },
                        note,
                        // Its own pitch, on top of the flip's: its note against a root moved by it.
                        root: (h.pitch != 0)
                            .then(|| (note as i32 - h.pitch as i32).clamp(0, 127) as u8),
                        start: (h.from / seconds).clamp(0.0, 1.0),
                        end: Some((h.to / seconds).clamp(0.0, 1.0)),
                        reverse: h.reverse,
                        gate: true,
                        release_ms: Some(25.0),
                        choke: Some(CHOP_CHOKE),
                        ..Zone::default()
                    });
                    zones.len() - 1
                }
                None => continue,
            };
            notes.push(Note {
                start: (h.beat as f64 * PPQ as f64).round() as i64,
                length: ((h.beats as f64 * PPQ as f64).round() as u64).max(1),
                pitch: FIRST_SLICE + k as u8,
                velocity: 120,
                ..Default::default()
            });
        }
        let bar = self.bar().max(1);
        let length = flip.bars.max(1) as u64 * bar;
        let midi = Region {
            name: flip.name.clone(),
            start: tick,
            length,
            looped: true,
            loop_length: Some((FLIP_PLAYS * bar).max(length)),
            content: Content::Midi {
                notes,
                controls: Vec::new(),
            },
            ..Default::default()
        };
        let track = Track {
            name: flip.name.clone(),
            instrument: Some(Instrument::Sampler(Sampler {
                zones,
                voice: SamplerVoice {
                    // Up is faster, as a record sped up: the flip's pitch on all of it.
                    transpose: flip.pitch as f32,
                    ..Default::default()
                },
            })),
            output: self.tracks[at.0].output,
            regions: vec![midi],
            ..Default::default()
        };
        let index = at.0 + 1;
        self.add_track(index, track);
        Some(index)
    }
}

impl Project {
    /// Slices of the audio region at `at`'s file (or `file`, a stem of it, `seconds` long like
    /// it): `(from, to, name)` seconds into it. On a Sampler track of their own after its track,
    /// called `name`: a zone a slice (one-shots, cutting each other off, as chops on an MPC), and
    /// a MIDI region playing each where it plays in the region now, from the bar at `tick` (as
    /// far into the bar as the first was). At most 92; the track.
    pub fn slices_to_sampler(
        &mut self,
        at: At,
        file: &Path,
        seconds: f32,
        slices: &[(f32, f32, String)],
        name: &str,
        tick: u64,
    ) -> Option<usize> {
        let rate = self.sample_rate as f64;
        let r = self.region(at).clone();
        let t = Timing::of(&r.content, rate)?;
        let tick_of = |s: f32| {
            self.audio_tick(&t, r.start as f64, s as f64 * rate)
                .round()
                .max(0.0) as u64
        };
        let most = (127 - FIRST_SLICE) as usize + 1;
        let slices: Vec<&(f32, f32, String)> = slices.iter().take(most).collect();
        let first = tick_of(slices.first()?.0);
        let bar = self.bar().max(1);
        let into = first - first / bar * bar;
        let share = |s: f32| (s / seconds.max(1e-6)).clamp(0.0, 1.0);
        let zones = slices
            .iter()
            .enumerate()
            .map(|(k, (a, b, n))| Zone {
                file: file.to_path_buf(),
                name: n.clone(),
                note: FIRST_SLICE + k as u8,
                start: share(*a),
                end: Some(share(*b)),
                gain_db: r.gain_db,
                choke: Some(CHOP_CHOKE),
                ..Zone::default()
            })
            .collect();
        let notes: Vec<Note> = slices
            .iter()
            .enumerate()
            .map(|(k, (a, b, _))| Note {
                start: (tick_of(*a) - first + into) as i64,
                length: tick_of(*b).saturating_sub(tick_of(*a)).max(1),
                pitch: FIRST_SLICE + k as u8,
                velocity: 127,
                ..Default::default()
            })
            .collect();
        let end = notes
            .iter()
            .map(|n| n.start as u64 + n.length)
            .max()
            .unwrap_or(bar);
        let midi = Region {
            name: name.into(),
            start: tick,
            length: end.div_ceil(bar).max(1) * bar,
            content: Content::Midi {
                notes,
                controls: Vec::new(),
            },
            ..Default::default()
        };
        let track = Track {
            name: name.into(),
            instrument: Some(Instrument::Sampler(Sampler {
                zones,
                voice: SamplerVoice::default(),
            })),
            output: self.tracks[at.0].output,
            regions: vec![midi],
            ..Default::default()
        };
        let index = at.0 + 1;
        self.add_track(index, track);
        Some(index)
    }
}
