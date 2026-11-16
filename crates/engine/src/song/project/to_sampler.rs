//! An audio region made playable: a Sampler track of its own, holding the part of the file the
//! region plays (whole, across the keys, or cut at its hits into slices up from C1), and a MIDI
//! region playing it back where it was.

use super::*;

/// The note a whole sound plays as recorded on.
pub const SAMPLER_ROOT: u8 = 60;
/// The first slice's note (C1); the rest go up a note each.
pub const FIRST_SLICE: u8 = 36;

/// The choke group a sampler's chops share, so they play one at a time.
pub const CHOP_CHOKE: u8 = 16;

impl Project {
    /// The audio region at `at` as a new Sampler track, just after its track, named after the
    /// region: the part it plays, its gain and (whole) its transpose and fades kept; with
    /// `cuts` (frames of its file), sliced there into one-shots up from `FIRST_SLICE`. A MIDI
    /// region in its place plays it as it was, and the audio region is muted. `frames` is the
    /// file's length (as `cuts` count it). Returns the new track's index.
    pub fn sampler_from_audio(
        &mut self,
        at: At,
        frames: u64,
        cuts: Option<&[u64]>,
    ) -> Option<usize> {
        let rate = self.sample_rate as f64;
        let r = self.region(at).clone();
        let Content::Audio { file, .. } = &r.content else {
            return None;
        };
        let t = Timing::of(&r.content, rate)?;
        let start = r.start as f64;
        let total = frames.max(1) as f64;
        let from = self.audio_frame(&t, start, start).clamp(0.0, total);
        let to = self
            .audio_frame(&t, start, (r.start + r.length) as f64)
            .clamp(from + 1.0, total);
        let share = |f: f64| (f / total).clamp(0.0, 1.0) as f32;
        let end = |f: f64| (f < total - 0.5).then(|| share(f));
        // Where a frame plays, in ticks from the region's start.
        let tick = |f: f64| (self.audio_tick(&t, start, f) - start).round().max(0.0) as u64;
        let mut voice = SamplerVoice {
            transpose: r.transpose as f32,
            ..Default::default()
        };
        // Stretched or following the tempo: played at the speed the region plays it, whole or
        // sliced (else each slice outlasts, or falls short of, its place).
        let heard = self.seconds_at((r.start + r.length) as f64) - self.seconds_at(start);
        let speed = (to - from) / rate / heard.max(1e-6);
        if (speed - 1.0).abs() > 0.01 {
            voice.mode = SampleMode::Stretch;
            voice.speed = speed as f32;
        }
        let (zones, notes): (Vec<Zone>, Vec<Note>) = match cuts {
            // Whole: one zone over every key, the region's note on its root; it stops with the
            // note, so the MIDI region's one note plays it for as long as the region did.
            None => {
                voice.attack_ms = (self.seconds_at((r.start + r.fade_in) as f64)
                    - self.seconds_at(start)) as f32
                    * 1000.0;
                let fade_out = r.fade_out.min(r.length);
                voice.release_ms = match fade_out {
                    0 => voice.release_ms,
                    f => {
                        let end = (r.start + r.length) as f64;
                        ((self.seconds_at(end) - self.seconds_at(end - f as f64)) * 1000.0) as f32
                    }
                };
                let zone = Zone {
                    file: file.clone(),
                    name: r.name.clone(),
                    note: 0,
                    high: Some(127),
                    root: Some(SAMPLER_ROOT),
                    start: share(from),
                    end: end(to),
                    gain_db: r.gain_db,
                    gate: true,
                    ..Zone::default()
                };
                let note = Note {
                    start: 0,
                    length: r.length,
                    pitch: SAMPLER_ROOT,
                    velocity: 127,
                    ..Default::default()
                };
                (vec![zone], vec![note])
            }
            // Sliced: a one-shot a hit, on its own note, played where the hit was.
            Some(cuts) => {
                let most = (127 - FIRST_SLICE) as usize + 1;
                let mut at: Vec<f64> = std::iter::once(from)
                    .chain(
                        cuts.iter()
                            .map(|&c| c as f64)
                            .filter(|&c| c > from + 1.0 && c < to - 1.0),
                    )
                    .collect();
                at.sort_by(f64::total_cmp);
                at.dedup();
                at.truncate(most);
                let next = |k: usize| at.get(k + 1).copied().unwrap_or(to);
                let zones = (0..at.len())
                    .map(|k| Zone {
                        file: file.clone(),
                        name: format!("Slice {}", k + 1),
                        note: FIRST_SLICE + k as u8,
                        start: share(at[k]),
                        end: end(next(k)),
                        gain_db: r.gain_db,
                        // One at a time, as chops on an MPC: each cuts off the one before.
                        choke: Some(CHOP_CHOKE),
                        ..Zone::default()
                    })
                    .collect();
                let notes = (0..at.len())
                    .map(|k| Note {
                        start: tick(at[k]) as i64,
                        length: tick(next(k)).saturating_sub(tick(at[k])).max(1),
                        pitch: FIRST_SLICE + k as u8,
                        velocity: 127,
                        ..Default::default()
                    })
                    .collect();
                (zones, notes)
            }
        };
        let name = match r.name.is_empty() {
            true => format!("{} Sampler", self.tracks[at.0].name),
            false => r.name.clone(),
        };
        let midi = Region {
            name: name.clone(),
            start: r.start,
            length: r.length,
            // Its loops play on as the audio's did: to where they were dragged, or the next region
            // on its track (on a track of its own, none would stop them).
            looped: r.looped,
            loop_length: match r.looped {
                true => (r.loop_length.is_some()
                    || self.tracks[at.0].regions.iter().any(|o| o.start > r.start))
                .then(|| self.loop_end(at) - r.start),
                false => r.loop_length,
            },
            content: Content::Midi {
                notes,
                controls: Vec::new(),
            },
            ..Default::default()
        };
        let track = Track {
            name,
            instrument: Some(Instrument::Sampler(Sampler { zones, voice })),
            output: self.tracks[at.0].output,
            regions: vec![midi],
            ..Default::default()
        };
        let index = at.0 + 1;
        self.add_track(index, track);
        self.tracks[at.0].regions[at.1].mute = true;
        Some(index)
    }
}
