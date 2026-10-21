//! The built-in instrument: a polyphonic sine with a short attack and release. Deterministic, so
//! tests can check exactly what the engine did.

use crate::plugin::Note;

const VOICES: usize = 32;
const ATTACK: f32 = 0.005;
const RELEASE: f32 = 0.05;
const GAIN: f32 = 0.25;

#[derive(Clone, Copy, Default)]
struct Voice {
    pitch: u8,
    velocity: f32,
    phase: f32,
    env: f32,
    held: bool,
}

pub struct Synth {
    rate: f32,
    voices: [Voice; VOICES],
}

impl Synth {
    pub fn new(rate: f64) -> Self {
        Synth {
            rate: rate as f32,
            voices: [Voice::default(); VOICES],
        }
    }

    /// Adds `frames` of output to `buf`, applying each note at its offset. `notes` must be sorted
    /// by offset.
    pub fn process(&mut self, buf: [&mut [f32]; 2], frames: usize, notes: &[Note]) {
        let [l, r] = buf;
        let (up, down) = (1.0 / (ATTACK * self.rate), 1.0 / (RELEASE * self.rate));
        let mut next = notes.iter().peekable();
        for i in 0..frames {
            while let Some(n) = next.next_if(|n| n.offset as usize <= i) {
                self.note(n);
            }
            let mut s = 0.0;
            for v in self.voices.iter_mut().filter(|v| v.held || v.env > 0.0) {
                v.env = if v.held {
                    (v.env + up).min(1.0)
                } else {
                    (v.env - down).max(0.0)
                };
                s += v.phase.sin() * v.env * v.velocity;
                let freq = 440.0 * 2f32.powf((v.pitch as f32 - 69.0) / 12.0);
                v.phase =
                    (v.phase + std::f32::consts::TAU * freq / self.rate) % std::f32::consts::TAU;
            }
            l[i] += s * GAIN;
            r[i] += s * GAIN;
        }
    }

    fn note(&mut self, n: &Note) {
        if n.velocity > 0.0 {
            // A free voice, else the quietest.
            let v = self
                .voices
                .iter_mut()
                .min_by(|a, b| (a.held, a.env).partial_cmp(&(b.held, b.env)).unwrap())
                .unwrap();
            *v = Voice {
                pitch: n.pitch,
                velocity: n.velocity,
                phase: 0.0,
                env: 0.0,
                held: true,
            };
        } else if let Some(v) = self
            .voices
            .iter_mut()
            .find(|v| v.held && v.pitch == n.pitch)
        {
            v.held = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_from_the_note_offset_and_releases() {
        let mut s = Synth::new(48_000.0);
        let (mut l, mut r) = (vec![0f32; 4800], vec![0f32; 4800]);
        let on = Note {
            offset: 100,
            pitch: 69,
            velocity: 1.0,
        };
        let off = Note {
            offset: 2000,
            pitch: 69,
            velocity: 0.0,
        };
        s.process([&mut l, &mut r], 4800, &[on, off]);
        assert!(l[..101].iter().all(|&x| x == 0.0));
        assert!(l[200..2000].iter().any(|&x| x.abs() > 0.1));
        // Release is 50 ms (2400 samples): silent after.
        assert!(l[4500..].iter().all(|&x| x == 0.0));
        assert_eq!(l, r);
    }
}
