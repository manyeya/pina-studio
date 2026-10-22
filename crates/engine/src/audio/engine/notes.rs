//! Notes as they play: held, released, gathered for a block from the song, a loop cell and live playing.

use super::*;

/// The notes an instrument is holding, counted by pitch: the same pitch can be held twice (two
/// notes overlapping), and each needs its own note-off, or one voice rings on.
#[derive(Clone, Copy)]
pub(crate) struct Held([u8; 128]);

impl Held {
    pub(crate) fn new() -> Self {
        Held([0; 128])
    }

    /// Whether any note is held.
    pub(crate) fn any(&self) -> bool {
        self.0.iter().any(|&c| c > 0)
    }

    pub(crate) fn apply(&mut self, pitch: u8, on: bool) {
        let count = &mut self.0[pitch as usize];
        *count = if on {
            count.saturating_add(1)
        } else {
            count.saturating_sub(1)
        };
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
        for (pitch, count) in self.0.iter_mut().enumerate() {
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
        held.apply(e.pitch, e.velocity > 0.0);
        notes[*n] = Note {
            offset: (e.at - from) as u32 + into as u32,
            pitch: e.pitch,
            velocity: e.velocity,
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
