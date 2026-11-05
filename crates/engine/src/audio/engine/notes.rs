//! Notes as they play: held, released, gathered for a block from the song, a loop cell and live playing.

use super::*;

/// The notes an instrument is holding, counted by pitch: the same pitch can be held twice (two
/// notes overlapping), and each needs its own note-off, or one voice rings on. Also the notes
/// whose chance didn't come up, whose note-offs are let pass; and the dice that decided.
#[derive(Clone, Copy)]
pub(crate) struct Held {
    on: [u8; 128],
    skipped: [u8; 128],
    dice: u64,
}

impl Held {
    pub(crate) fn new() -> Self {
        // Each its own dice, so one track's chances don't fall as another's do.
        static SEED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let seed = SEED.fetch_add(0x9e37_79b9_7f4a_7c15, std::sync::atomic::Ordering::Relaxed);
        Held {
            on: [0; 128],
            skipped: [0; 128],
            dice: seed | 1,
        }
    }

    /// Whether any note is held.
    pub(crate) fn any(&self) -> bool {
        self.on.iter().any(|&c| c > 0)
    }

    pub(crate) fn apply(&mut self, pitch: u8, on: bool) {
        let count = &mut self.on[pitch as usize];
        *count = if on {
            count.saturating_add(1)
        } else {
            count.saturating_sub(1)
        };
    }

    /// A roll of the dice, 0 to 1.
    fn roll(&mut self) -> f32 {
        let mut x = self.dice;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.dice = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    /// An event as it plays: None for a note whose chance didn't come up (and its note-off),
    /// else its velocity, strayed as far as it may.
    // shortcut: a skipped note's note-off is the next one of its pitch, so of two overlapping
    // notes of one pitch the played one can end where the skipped one would have; key offs to
    // their notes if that's ever heard.
    pub(crate) fn play(&mut self, e: &Event) -> Option<f32> {
        let p = e.pitch as usize & 127;
        if e.velocity == 0.0 {
            if self.skipped[p] > 0 {
                self.skipped[p] -= 1;
                return None;
            }
            return Some(0.0);
        }
        if e.velocity > 0.0 && e.chance < 100 && self.roll() * 100.0 >= e.chance as f32 {
            self.skipped[p] = self.skipped[p].saturating_add(1);
            return None;
        }
        Some(match e.spread {
            0 => e.velocity,
            s => {
                let by = (self.roll() * 2.0 - 1.0) * s as f32 / 127.0;
                (e.velocity + by).clamp(1.0 / 127.0, 1.0)
            }
        })
    }

    /// Lets go of every note held, at `offset` in the block, unless `keep` says it's still on;
    /// adds the note-offs to `notes`.
    pub(crate) fn release(
        &mut self,
        offset: u32,
        keep: impl Fn(u8) -> bool,
        notes: &mut [Note; BLOCK_NOTES],
        n: &mut usize,
    ) {
        self.skipped = [0; 128];
        for (pitch, count) in self.on.iter_mut().enumerate() {
            if *count == 0 || keep(pitch as u8) {
                continue;
            }
            for _ in 0..std::mem::take(count) {
                if *n < BLOCK_NOTES {
                    notes[*n] = Note {
                        offset,
                        pitch: pitch as u8,
                        velocity: 0.0,
                    };
                    *n += 1;
                }
            }
        }
    }
}

/// Whether a pitch is sounding at a sample in some events (sorted by time): its last event before
/// then turned it on. Looks back so far, then says no.
pub(crate) fn sounding(events: &[Event], at: u64, pitch: u8) -> bool {
    const LOOK_BACK: usize = 4096;
    let end = events.partition_point(|e| e.at < at);
    events[end.saturating_sub(LOOK_BACK)..end]
        .iter()
        .rev()
        .find(|e| e.pitch == pitch)
        .is_some_and(|e| e.velocity > 0.0)
}

/// Adds what `events` and `clips` play from `from` to `to` (samples) into a block, `into`
/// samples into it.
#[allow(clippy::too_many_arguments)]
/// Where a block's notes and controller moves are gathered.
pub(crate) struct Gathered {
    pub(crate) notes: [Note; BLOCK_NOTES],
    pub(crate) n: usize,
    pub(crate) controls: [Control; BLOCK_CONTROLS],
    pub(crate) m: usize,
}

impl Gathered {
    pub(crate) fn new() -> Self {
        Gathered {
            notes: [Note::default(); BLOCK_NOTES],
            n: 0,
            controls: [Control {
                offset: 0,
                kind: ControlKind::Aftertouch(0.0),
            }; BLOCK_CONTROLS],
            m: 0,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn gather(
    events: &[Event],
    controls: &[ControlEvent],
    clips: &[Clip],
    (from, to): (u64, u64),
    into: usize,
    g: &mut Gathered,
    held: &mut Held,
    [l, r]: [&mut [f32]; 2],
) {
    let first = controls.partition_point(|c| c.at < from);
    for c in controls[first..].iter().take_while(|c| c.at < to) {
        if g.m == BLOCK_CONTROLS {
            break;
        }
        g.controls[g.m] = Control {
            offset: (c.at - from) as u32 + into as u32,
            kind: c.kind,
        };
        g.m += 1;
    }
    let (notes, n) = (&mut g.notes, &mut g.n);
    let first = events.partition_point(|e| e.at < from);
    for e in events[first..].iter().take_while(|e| e.at < to) {
        if *n == BLOCK_NOTES {
            break;
        }
        let Some(velocity) = held.play(e) else {
            continue;
        };
        held.apply(e.pitch, velocity > 0.0);
        notes[*n] = Note {
            offset: (e.at - from) as u32 + into as u32,
            pitch: e.pitch,
            velocity,
        };
        *n += 1;
    }
    // ponytail: walks the clips from the first every block; index them by time if a track gets
    // thousands of regions.
    for c in clips.iter().take_while(|c| c.start < to) {
        let (a, b) = (from.max(c.start), to.min(c.start + c.len));
        for (ch, dst) in [&mut *l, &mut *r].into_iter().enumerate() {
            let src = &c.data.channels[ch.min(c.data.channels.len() - 1)];
            let fades = c.fades[1] > 0 || c.fades[3] > 0;
            for t in a..b {
                let i = (c.offset + t - c.start) as usize;
                let v = src.get(i).copied().unwrap_or(0.0);
                dst[(t - from) as usize + into] += if fades { v * faded(c, t) } else { v };
            }
        }
    }
}

/// A note from a MIDI keyboard. Velocity 0 is a note off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveNote {
    pub pitch: u8,
    pub velocity: u8,
}

/// A live note as the engine played it, at the playhead, for recording.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Played {
    pub at: u64,
    pub pitch: u8,
    pub velocity: u8,
}
