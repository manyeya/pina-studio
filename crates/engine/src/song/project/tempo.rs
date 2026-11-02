//! The song's tempo over time: what it starts at, and where it changes. Ticks to seconds and back
//! go through it, so a change moves everything after it in time but not in bars and beats.

use serde::{Deserialize, Serialize};

use super::{PPQ, Project};

/// The slowest and fastest a tempo can be.
pub const MIN_BPM: f64 = 20.0;
pub const MAX_BPM: f64 = 999.0;

/// From `tick` on, `bpm` quarter notes a minute, until the next change.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct TempoChange {
    pub tick: u64,
    pub bpm: f64,
}

impl Project {
    /// The tempo at a tick.
    pub fn tempo_at(&self, tick: u64) -> f64 {
        self.tempos
            .iter()
            .take_while(|c| c.tick <= tick)
            .last()
            .map_or(self.tempo, |c| c.bpm)
    }

    /// Seconds from the start to a tick; before the start, at the first tempo.
    pub fn seconds_at(&self, tick: f64) -> f64 {
        let (mut seconds, mut at, mut bpm) = (0.0, 0.0, self.tempo);
        if tick > 0.0 {
            for c in &self.tempos {
                let t = c.tick as f64;
                if t >= tick {
                    break;
                }
                seconds += (t - at) / PPQ as f64 * 60.0 / bpm;
                (at, bpm) = (t, c.bpm);
            }
        }
        seconds + (tick - at) / PPQ as f64 * 60.0 / bpm
    }

    /// The tick (fractional) so many seconds from the start.
    pub fn tick_at_seconds(&self, seconds: f64) -> f64 {
        let (mut elapsed, mut at, mut bpm) = (0.0, 0.0, self.tempo);
        if seconds > 0.0 {
            for c in &self.tempos {
                let t = c.tick as f64;
                let end = elapsed + (t - at) / PPQ as f64 * 60.0 / bpm;
                if end > seconds {
                    break;
                }
                (elapsed, at, bpm) = (end, t, c.bpm);
            }
        }
        at + (seconds - elapsed) * bpm / 60.0 * PPQ as f64
    }

    /// A tick's sample at `rate`.
    pub fn samples(&self, ticks: u64, rate: f64) -> u64 {
        (self.seconds_at(ticks as f64) * rate).round() as u64
    }

    /// A sample at the project's rate as a tick, rounded down.
    pub fn ticks(&self, samples: u64) -> u64 {
        (self.tick_at_seconds(samples as f64 / self.sample_rate as f64) + 1e-6).max(0.0) as u64
    }

    /// How many samples at `rate` from one tick to another.
    pub fn span(&self, from: u64, to: u64, rate: f64) -> u64 {
        self.samples(to, rate)
            .saturating_sub(self.samples(from, rate))
    }

    /// Sets the tempo from `tick` on (the song's own at 0), as far as the next change.
    pub fn set_tempo_at(&mut self, tick: u64, bpm: f64) {
        let bpm = bpm.clamp(MIN_BPM, MAX_BPM);
        match tick {
            0 => self.tempo = bpm,
            _ => match self.tempos.iter_mut().find(|c| c.tick == tick) {
                Some(c) => c.bpm = bpm,
                None => self.tempos.push(TempoChange { tick, bpm }),
            },
        }
        self.tidy_tempos();
    }

    /// Takes the tempo change at `tick` out: the tempo before it carries on.
    pub fn delete_tempo(&mut self, tick: u64) {
        self.tempos.retain(|c| c.tick != tick);
    }

    /// Moves a tempo change to another tick.
    pub fn move_tempo(&mut self, from: u64, to: u64) {
        let Some(at) = self.tempos.iter().position(|c| c.tick == from) else {
            return;
        };
        let bpm = self.tempos.remove(at).bpm;
        self.set_tempo_at(to, bpm);
    }

    /// Replaces the tempo between two ticks with `changes` (each at a tick in that stretch), the
    /// tempo after `to` as it was.
    pub fn replace_tempos(&mut self, from: u64, to: u64, changes: &[TempoChange]) {
        let after = self.tempo_at(to);
        self.tempos.retain(|c| c.tick < from || c.tick > to);
        if from == 0
            && let Some(first) = changes.first().filter(|c| c.tick == 0)
        {
            self.tempo = first.bpm.clamp(MIN_BPM, MAX_BPM);
        }
        self.tempos.extend(
            changes
                .iter()
                .filter(|c| c.tick > 0 && (from..to).contains(&c.tick))
                .map(|c| TempoChange {
                    tick: c.tick,
                    bpm: c.bpm.clamp(MIN_BPM, MAX_BPM),
                }),
        );
        if to > 0 && !self.tempos.iter().any(|c| c.tick == to) {
            self.tempos.push(TempoChange {
                tick: to,
                bpm: after,
            });
        }
        self.tidy_tempos();
    }

    /// In order, one change to a tick, none at the start and none that keeps the tempo it was.
    fn tidy_tempos(&mut self) {
        self.tempos.sort_by_key(|c| c.tick);
        self.tempos.dedup_by(|b, a| {
            if a.tick == b.tick {
                a.bpm = b.bpm;
                true
            } else {
                false
            }
        });
        let mut was = self.tempo;
        self.tempos.retain(|c| {
            let keep = c.tick > 0 && (c.bpm - was).abs() > 1e-9;
            if keep {
                was = c.bpm;
            }
            keep
        });
    }
}

/// One stretch of steady tempo, in samples: where it starts, its quarter note there, its tempo.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    sample: f64,
    beat: f64,
    bpm: f64,
}

/// The tempo map in samples at one rate, for the audio thread: the beat (in quarter notes) and
/// tempo at any sample, and the sample of any beat.
#[derive(Clone, Debug, PartialEq)]
pub struct TempoMap {
    spans: Vec<Span>,
    rate: f64,
}

impl TempoMap {
    pub fn new(p: &Project, rate: f64) -> Self {
        let mut spans = vec![Span {
            sample: 0.0,
            beat: 0.0,
            bpm: p.tempo,
        }];
        for c in &p.tempos {
            spans.push(Span {
                sample: p.seconds_at(c.tick as f64) * rate,
                beat: c.tick as f64 / PPQ as f64,
                bpm: c.bpm,
            });
        }
        TempoMap { spans, rate }
    }

    /// One tempo throughout.
    pub fn steady(bpm: f64, rate: f64) -> Self {
        TempoMap {
            spans: vec![Span {
                sample: 0.0,
                beat: 0.0,
                bpm,
            }],
            rate,
        }
    }

    fn span_at(&self, sample: f64) -> &Span {
        let k = self.spans.partition_point(|s| s.sample <= sample);
        &self.spans[k.saturating_sub(1)]
    }

    /// The quarter note playing at a sample.
    pub fn beats(&self, sample: u64) -> f64 {
        let s = self.span_at(sample as f64);
        s.beat + (sample as f64 - s.sample) / self.rate * s.bpm / 60.0
    }

    /// The tempo at a sample.
    pub fn tempo(&self, sample: u64) -> f64 {
        self.span_at(sample as f64).bpm
    }

    /// The sample a quarter note plays at.
    pub fn sample(&self, beats: f64) -> f64 {
        let k = self.spans.partition_point(|s| s.beat <= beats);
        let s = &self.spans[k.saturating_sub(1)];
        s.sample + (beats - s.beat) * 60.0 / s.bpm * self.rate
    }
}

/// How the song's tempo treats audio recorded or brought in: kept as it is (audio follows it),
/// adapted to the audio, or adapted only while nothing else in the song keeps time.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TempoMode {
    Keep,
    Adapt,
    #[default]
    Auto,
}

impl TempoMode {
    pub const ALL: [TempoMode; 3] = [TempoMode::Keep, TempoMode::Adapt, TempoMode::Auto];

    pub fn name(self) -> &'static str {
        match self {
            TempoMode::Keep => "Keep",
            TempoMode::Adapt => "Adapt",
            TempoMode::Auto => "Auto",
        }
    }

    pub fn is_default(&self) -> bool {
        *self == TempoMode::Auto
    }
}
