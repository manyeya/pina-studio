//! The built-in sampler: samples on notes. A drum kit puts one on each note, played out to its end
//! however short the note; one sample can also play across a range of notes, repitched from its
//! root, as an 808 or a bass, and stop (gated) when the note does. Sounds in a choke group cut
//! each other off, as a closed hat cuts off an open one.

use std::{path::Path, sync::Arc};

use crate::{
    engine::{AudioData, load_wav},
    plugin::Note,
    project::Zone,
};

const VOICES: usize = 64;
/// How fast a choked sound fades: quick, but not a click.
const CHOKE_MS: f32 = 4.0;
/// A gated sound's release, unless its zone says otherwise.
const RELEASE_MS: f32 = 60.0;

struct Loaded {
    zone: Zone,
    data: Arc<AudioData>,
    /// Left and right gain: the zone's level and pan.
    gain: [f32; 2],
}

#[derive(Clone, Copy, Default)]
struct Voice {
    on: bool,
    zone: usize,
    note: u8,
    /// Where in the sample, and how far it moves a sample (faster for higher notes).
    pos: f64,
    step: f64,
    velocity: f32,
    /// 1 until it's let go; then falling by `fade` a sample.
    level: f32,
    fade: f32,
    held: bool,
    /// When it started, to take the oldest voice when all are busy.
    age: u64,
}

pub struct Sampler {
    rate: f32,
    zones: Vec<Loaded>,
    voices: [Voice; VOICES],
    started: u64,
}

impl Sampler {
    /// Loads the zones' samples from `dir` at `rate`. A sample that can't be read is an error.
    pub fn load(zones: &[Zone], dir: &Path, rate: f64) -> Result<Sampler, String> {
        let zones = zones
            .iter()
            .map(|z| {
                let data = Arc::new(load_wav(&dir.join(&z.file), rate)?);
                let level = 10f32.powf(z.gain_db / 20.0);
                let pan = z.pan.clamp(-1.0, 1.0);
                let gain = [level * (1.0 - pan).min(1.0), level * (1.0 + pan).min(1.0)];
                Ok(Loaded {
                    zone: z.clone(),
                    data,
                    gain,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Sampler {
            rate: rate as f32,
            zones,
            voices: [Voice::default(); VOICES],
            started: 0,
        })
    }

    /// Adds `frames` of output to `buf`, applying each note at its offset. `notes` must be sorted
    /// by offset. Allocation-free.
    pub fn process(&mut self, buf: [&mut [f32]; 2], frames: usize, notes: &[Note]) {
        let [l, r] = buf;
        let mut next = notes.iter().peekable();
        for i in 0..frames {
            while let Some(n) = next.next_if(|n| n.offset as usize <= i) {
                self.note(n);
            }
            let (mut sl, mut sr) = (0.0, 0.0);
            for v in self.voices.iter_mut().filter(|v| v.on) {
                let z = &self.zones[v.zone];
                let channels = &z.data.channels;
                let len = channels[0].len();
                let at = v.pos as usize;
                if at + 1 >= len || v.level <= 0.0 {
                    v.on = false;
                    continue;
                }
                let t = (v.pos - at as f64) as f32;
                let read = |c: usize| {
                    let ch = &channels[c.min(channels.len() - 1)];
                    ch[at] + (ch[at + 1] - ch[at]) * t
                };
                let g = v.velocity * v.level;
                sl += read(0) * g * z.gain[0];
                sr += read(1) * g * z.gain[1];
                v.pos += v.step;
                if v.fade > 0.0 {
                    v.level = (v.level - v.fade).max(0.0);
                }
            }
            l[i] += sl;
            r[i] += sr;
        }
    }

    fn note(&mut self, n: &Note) {
        if n.velocity <= 0.0 {
            // A gated sound stops when its note does; a one-shot plays on.
            let release = |z: &Zone| z.release_ms.unwrap_or(RELEASE_MS);
            for v in self
                .voices
                .iter_mut()
                .filter(|v| v.on && v.held && v.note == n.pitch)
            {
                v.held = false;
                let zone = &self.zones[v.zone].zone;
                if zone.gate {
                    v.fade = 1.0 / (release(zone).max(1.0) * 0.001 * self.rate);
                }
            }
            return;
        }
        let Some(index) = self.zones.iter().position(|z| z.zone.covers(n.pitch)) else {
            return;
        };
        let zone = &self.zones[index].zone;
        // Its choke group falls quiet.
        if let Some(group) = zone.choke {
            let fade = 1.0 / (CHOKE_MS * 0.001 * self.rate);
            for v in self.voices.iter_mut().filter(|v| v.on) {
                if self.zones[v.zone].zone.choke == Some(group) {
                    v.fade = v.fade.max(fade);
                }
            }
        }
        let root = zone.root.unwrap_or(zone.note);
        let semitones = n.pitch as f64 - root as f64 + zone.tune_cents as f64 / 100.0;
        let step = 2f64.powf(semitones / 12.0);
        // A free voice, else the oldest.
        let v = match self.voices.iter().position(|v| !v.on) {
            Some(i) => i,
            None => (0..VOICES).min_by_key(|&i| self.voices[i].age).unwrap(),
        };
        self.started += 1;
        self.voices[v] = Voice {
            on: true,
            zone: index,
            note: n.pitch,
            pos: 0.0,
            step,
            // Softer notes are quieter, more than in proportion, as a drum is.
            velocity: n.velocity.clamp(0.0, 1.0).powf(1.5),
            level: 1.0,
            fade: 0.0,
            held: true,
            age: self.started,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{AudioData, MAX_BLOCK};

    fn sampler(zones: Vec<(Zone, Vec<f32>)>) -> Sampler {
        let zones = zones
            .into_iter()
            .map(|(zone, samples)| Loaded {
                zone,
                data: Arc::new(AudioData::mono(samples)),
                gain: [1.0, 1.0],
            })
            .collect();
        Sampler {
            rate: 48_000.0,
            zones,
            voices: [Voice::default(); VOICES],
            started: 0,
        }
    }

    fn note(offset: u32, pitch: u8, velocity: f32) -> Note {
        Note {
            offset,
            pitch,
            velocity,
        }
    }

    fn run(s: &mut Sampler, notes: &[Note]) -> Vec<f32> {
        let (mut l, mut r) = (vec![0f32; MAX_BLOCK], vec![0f32; MAX_BLOCK]);
        s.process([&mut l, &mut r], MAX_BLOCK, notes);
        l
    }

    #[test]
    fn a_hit_plays_out_however_short_its_note() {
        let kick = Zone {
            note: 36,
            ..Zone::default()
        };
        let mut s = sampler(vec![(kick, vec![0.5; 300])]);
        let out = run(&mut s, &[note(10, 36, 1.0), note(20, 36, 0.0)]);
        assert_eq!(out[9], 0.0);
        assert!((out[10] - 0.5).abs() < 1e-6 && (out[200] - 0.5).abs() < 1e-6);
        assert_eq!(out[400], 0.0); // past its end
        // Another note, or a quieter one: nothing, and less.
        assert_eq!(run(&mut s, &[note(0, 37, 1.0)])[10], 0.0);
        let soft = run(&mut s, &[note(0, 36, 0.5)])[10];
        assert!(soft < 0.25 && soft > 0.1, "{soft}");
    }

    #[test]
    fn a_range_repitches_and_a_gated_sound_stops_with_its_note() {
        // A ramp, so how fast it's read shows.
        let ramp: Vec<f32> = (0..4800).map(|i| i as f32 / 4800.0).collect();
        let bass = Zone {
            note: 24,
            high: Some(48),
            root: Some(36),
            gate: true,
            release_ms: Some(1.0),
            ..Zone::default()
        };
        let mut s = sampler(vec![(bass, ramp)]);
        // An octave up reads twice as fast.
        let out = run(&mut s, &[note(0, 48, 1.0)]);
        assert!((out[100] - 200.0 / 4800.0).abs() < 1e-4, "{}", out[100]);
        // Let go: gone within its millisecond.
        let out = run(&mut s, &[note(0, 48, 0.0)]);
        assert!(out[0] > 0.0 && out[60] == 0.0);
        // Tuned up an octave, its root reads twice as fast too.
        s.zones[0].zone.tune_cents = 1200.0;
        let out = run(&mut s, &[note(0, 36, 1.0)]);
        assert!((out[100] - 200.0 / 4800.0).abs() < 1e-4, "{}", out[100]);
    }

    #[test]
    fn a_closed_hat_chokes_the_open_one() {
        let open = Zone {
            note: 46,
            choke: Some(1),
            ..Zone::default()
        };
        let closed = Zone {
            note: 42,
            choke: Some(1),
            ..Zone::default()
        };
        let mut s = sampler(vec![(open, vec![0.5; 48_000]), (closed, vec![0.25; 20])]);
        let out = run(&mut s, &[note(0, 46, 1.0), note(100, 42, 1.0)]);
        assert!((out[50] - 0.5).abs() < 1e-6);
        // The open hat is fading out under the closed one, and both are gone a little later.
        assert!(out[150] < 0.5);
        assert_eq!(out[400], 0.0);
    }
}
