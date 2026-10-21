//! The built-in drum kit: synthesized one-shots on the General MIDI drum notes, so pattern
//! regions have something to play without plugins. Deterministic, like the synth.

use crate::plugin::Note;

const VOICES: usize = 24;
const GAIN: f32 = 0.3;

/// The General MIDI names of the notes the kit plays, for the step sequencer.
pub const SOUNDS: &[(u8, &str)] = &[
    (36, "Kick"),
    (38, "Snare"),
    (37, "Side Stick"),
    (39, "Clap"),
    (42, "Closed Hat"),
    (46, "Open Hat"),
    (41, "Low Tom"),
    (45, "Mid Tom"),
    (48, "High Tom"),
    (49, "Crash"),
];

pub fn name(pitch: u8) -> String {
    SOUNDS
        .iter()
        .find(|s| s.0 == pitch)
        .map_or(format!("Note {pitch}"), |s| s.1.to_string())
}

#[derive(Clone, Copy, Default)]
struct Voice {
    pitch: u8,
    velocity: f32,
    /// Samples since it was struck; None when silent.
    t: Option<u32>,
    phase: f32,
    /// One-pole high-pass state, for the noisy sounds.
    last_in: f32,
    last_out: f32,
}

pub struct Drums {
    rate: f32,
    voices: [Voice; VOICES],
    next: usize,
    seed: u32,
}

impl Drums {
    pub fn new(rate: f64) -> Self {
        Drums {
            rate: rate as f32,
            voices: [Voice::default(); VOICES],
            next: 0,
            seed: 0x1234_5678,
        }
    }

    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    /// Adds `frames` of the kit to `buf`, striking each note on at its offset. Note offs are
    /// ignored: drums ring out.
    pub fn process(&mut self, buf: [&mut [f32]; 2], frames: usize, notes: &[Note]) {
        let [l, r] = buf;
        let mut next = notes.iter().filter(|n| n.velocity > 0.0).peekable();
        for i in 0..frames {
            while let Some(n) = next.next_if(|n| n.offset as usize <= i) {
                self.voices[self.next] = Voice {
                    pitch: n.pitch,
                    velocity: n.velocity,
                    t: Some(0),
                    ..Voice::default()
                };
                self.next = (self.next + 1) % VOICES;
            }
            let mut s = 0.0;
            for v in 0..VOICES {
                let Some(t) = self.voices[v].t else { continue };
                let noise = self.noise();
                let voice = &mut self.voices[v];
                let secs = t as f32 / self.rate;
                let (out, done) = sound(voice, secs, noise, self.rate);
                s += out * voice.velocity;
                voice.t = if done { None } else { Some(t + 1) };
            }
            l[i] += s * GAIN;
            r[i] += s * GAIN;
        }
    }
}

/// One sample of a voice `secs` after it was struck, and whether it has finished.
fn sound(v: &mut Voice, secs: f32, noise: f32, rate: f32) -> (f32, bool) {
    let tau = std::f32::consts::TAU;
    let mut sine = |freq: f32| {
        v.phase = (v.phase + tau * freq / rate) % tau;
        v.phase.sin()
    };
    let decay = |time: f32| (-secs / time).exp();
    let (out, length) = match v.pitch {
        35 | 36 => (sine(50.0 + 110.0 * decay(0.04)) * decay(0.3), 1.2),
        38 | 40 => (
            sine(190.0) * decay(0.08) * 0.5 + noise * decay(0.14) * 0.6,
            0.6,
        ),
        37 => (sine(1700.0) * decay(0.01), 0.1),
        39 => {
            // Three quick bursts, then the tail.
            let burst = [0.0, 0.011, 0.022]
                .iter()
                .any(|&b| (b..b + 0.006).contains(&secs));
            (noise * if burst { 0.9 } else { decay(0.1) * 0.5 }, 0.6)
        }
        42 | 44 => (noise * decay(0.035), 0.3),
        46 => (noise * decay(0.25), 1.2),
        49 | 57 => (noise * decay(0.9) * 0.6, 3.5),
        41 | 43 | 45 | 47 | 48 | 50 => {
            let base = 60.0 + (v.pitch as f32 - 41.0) * 12.0;
            (sine(base * (1.0 + 0.5 * decay(0.05))) * decay(0.25), 1.0)
        }
        p => (
            sine(440.0 * 2f32.powf((p as f32 - 69.0) / 12.0)) * decay(0.15),
            0.8,
        ),
    };
    // Noisy sounds get a high-pass, so hats hiss rather than rumble.
    let out = if matches!(v.pitch, 42 | 44 | 46 | 49 | 57) {
        let hp = 0.95 * (v.last_out + out - v.last_in);
        v.last_in = out;
        v.last_out = hp;
        hp
    } else {
        out
    };
    (out, secs > length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_hit_sounds_and_ends() {
        for &(pitch, _) in SOUNDS {
            let mut d = Drums::new(48_000.0);
            let n = 48_000 * 4;
            let (mut l, mut r) = (vec![0f32; n], vec![0f32; n]);
            let hit = Note {
                offset: 10,
                pitch,
                velocity: 1.0,
            };
            d.process([&mut l, &mut r], n, &[hit]);
            let loudest = l.iter().fold(0f32, |m, x| m.max(x.abs()));
            assert!(loudest > 0.05, "{} is silent", name(pitch));
            assert_eq!(l[..10], [0.0; 10]);
            assert_eq!(*l.last().unwrap(), 0.0, "{} rings forever", name(pitch));
        }
    }
}
