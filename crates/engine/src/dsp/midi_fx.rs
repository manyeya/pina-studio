//! MIDI effects as they play: a track's notes changed, a block at a time, before its instrument
//! hears them. Allocation-free once made.

use crate::{
    music::Key,
    plugin::{Control, ControlKind, Note},
    song::project::{ArpOrder, MidiFx},
};

/// How many effects a track runs.
pub const MAX_MIDI_FX: usize = 8;
/// How many notes a block carries, how many can wait for later, how many keys an arpeggiator
/// holds, and how many pitches one note can become (a chord).
const NOTES: usize = 256;
const WAITING: usize = 256;
const HOLD: usize = 32;
const SPREAD: usize = 6;

/// Where the song is this block: its beat at the block's start (None: stopped), its tempo, and
/// the sample rate.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    pub beats: Option<f64>,
    pub tempo: f64,
    pub rate: f64,
}

#[derive(Clone, Copy, Default)]
struct Waiting {
    /// On the runner's own clock, in samples.
    at: u64,
    pitch: u8,
    velocity: f32,
}

/// One effect's memory.
struct State {
    /// What each pitch played became, while it sounds, for its note-off.
    became: [[u8; SPREAD]; 128],
    count: [u8; 128],
    /// Notes to come.
    waiting: [Waiting; WAITING],
    n_waiting: usize,
    /// The arpeggiator: what's held, in the order it went down, with its velocity; how many keys
    /// are down (with a latch, what's held stays when they're all up); its step and direction.
    held: [(u8, f32); HOLD],
    n_held: usize,
    down: usize,
    step: usize,
    dice: u64,
}

impl State {
    fn new(seed: u64) -> State {
        State {
            became: [[0; SPREAD]; 128],
            count: [0; 128],
            waiting: [Waiting::default(); WAITING],
            n_waiting: 0,
            held: [(0, 0.0); HOLD],
            n_held: 0,
            down: 0,
            step: 0,
            dice: seed | 1,
        }
    }

    fn roll(&mut self) -> f32 {
        let mut x = self.dice;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.dice = x;
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    fn wait(&mut self, w: Waiting) {
        if self.n_waiting < WAITING {
            self.waiting[self.n_waiting] = w;
            self.n_waiting += 1;
        }
    }
}

/// A block's notes, as one effect hands them to the next.
struct Notes {
    list: [Note; NOTES],
    n: usize,
}

impl Notes {
    fn push(&mut self, offset: u32, pitch: u8, velocity: f32) {
        if self.n < NOTES {
            self.list[self.n] = Note {
                offset,
                pitch,
                velocity,
            };
            self.n += 1;
        }
    }

    /// In time, a note-off before a note-on at the same sample.
    fn sort(&mut self) {
        self.list[..self.n].sort_by_key(|n| (n.offset, n.velocity > 0.0));
    }
}

/// A track's MIDI effects running: each one's memory, and the notes between them.
pub struct MidiFxRunner {
    states: Vec<State>,
    a: Box<Notes>,
    b: Box<Notes>,
    controls: Box<[Control; NOTES]>,
    /// Samples since it started, for what waits; and beats counted while the song's stopped.
    now: u64,
    free: f64,
}

impl Default for MidiFxRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// A note's pitch moved, kept to MIDI's.
fn moved(pitch: u8, by: i32) -> u8 {
    (pitch as i32 + by).clamp(0, 127) as u8
}

/// The nearest pitch in `key`'s scale (the lower, when two are as near).
fn in_scale(pitch: u8, key: Key) -> u8 {
    (0..12)
        .flat_map(|d| [moved(pitch, -d), moved(pitch, d)])
        .find(|&p| key.contains(p))
        .unwrap_or(pitch)
}

impl MidiFxRunner {
    pub fn new() -> Self {
        MidiFxRunner {
            states: (0..MAX_MIDI_FX as u64)
                .map(|k| State::new(0x9e37_79b9 + k * 7919))
                .collect(),
            a: Box::new(Notes {
                list: [Note::default(); NOTES],
                n: 0,
            }),
            b: Box::new(Notes {
                list: [Note::default(); NOTES],
                n: 0,
            }),
            controls: Box::new(
                [Control {
                    offset: 0,
                    kind: ControlKind::Aftertouch(0.0),
                }; NOTES],
            ),
            now: 0,
            free: 0.0,
        }
    }

    /// The song stopped or jumped: what's waiting to end ends now (at the block's start), what's
    /// waiting to start doesn't, and arpeggiators let go of what they held.
    pub fn stop(&mut self) {
        for s in &mut self.states {
            for w in &mut s.waiting[..s.n_waiting] {
                if w.velocity == 0.0 {
                    w.at = 0;
                }
            }
            let n = s.n_waiting;
            let mut kept = 0;
            for k in 0..n {
                if s.waiting[k].velocity == 0.0 {
                    s.waiting[kept] = s.waiting[k];
                    kept += 1;
                }
            }
            s.n_waiting = kept;
            s.n_held = 0;
            s.down = 0;
        }
    }

    /// `notes` (in time) through `fx` in order, for a block of `frames`: what the instrument
    /// hears. `key` is the song's, for what follows it.
    pub fn run(
        &mut self,
        fx: &[MidiFx],
        key: Option<Key>,
        notes: &[Note],
        frames: usize,
        clock: Clock,
    ) -> &[Note] {
        self.notes(fx, key, notes, frames, clock);
        &self.a.list[..self.a.n]
    }

    /// Notes and controls through `fx`, as an instrument hears both.
    pub fn run_all(
        &mut self,
        fx: &[MidiFx],
        key: Option<Key>,
        (notes, controls): (&[Note], &[Control]),
        frames: usize,
        clock: Clock,
    ) -> (&[Note], &[Control]) {
        self.notes(fx, key, notes, frames, clock);
        let n = self.modify(fx, controls);
        (&self.a.list[..self.a.n], &self.controls[..n])
    }

    fn notes(
        &mut self,
        fx: &[MidiFx],
        key: Option<Key>,
        notes: &[Note],
        frames: usize,
        clock: Clock,
    ) {
        let n = notes.len().min(NOTES);
        self.a.list[..n].copy_from_slice(&notes[..n]);
        self.a.n = n;
        let spb = clock.rate * 60.0 / clock.tempo.max(1.0);
        let beats = clock.beats.unwrap_or(self.free);
        for (k, f) in fx.iter().take(MAX_MIDI_FX).enumerate() {
            self.b.n = 0;
            let (from, into, s) = (&self.a, &mut self.b, &mut self.states[k]);
            step(
                f,
                key,
                s,
                &from.list[..from.n],
                into,
                self.now,
                frames,
                beats,
                spb,
            );
            into.sort();
            std::mem::swap(&mut self.a, &mut self.b);
        }
        self.now += frames as u64;
        self.free = beats + frames as f64 / spb;
    }

    /// The controls through each Modifier: a controller sent as another, scaled, or dropped.
    pub fn controls(&mut self, fx: &[MidiFx], controls: &[Control]) -> &[Control] {
        let n = self.modify(fx, controls);
        &self.controls[..n]
    }

    fn modify(&mut self, fx: &[MidiFx], controls: &[Control]) -> usize {
        let mut n = 0;
        for c in controls.iter().take(NOTES) {
            let mut kind = Some(c.kind);
            for f in fx {
                if let (
                    MidiFx::Modifier { from, to, scale },
                    Some(ControlKind::Cc { number, value }),
                ) = (f, kind)
                    && number == *from
                {
                    kind = (*to > 0).then_some(ControlKind::Cc {
                        number: *to,
                        value: (value * *scale as f32 / 100.0).clamp(0.0, 1.0),
                    });
                }
            }
            if let Some(kind) = kind {
                self.controls[n] = Control {
                    offset: c.offset,
                    kind,
                };
                n += 1;
            }
        }
        n
    }
}

/// One effect over a block's notes, into `out`.
#[allow(clippy::too_many_arguments)]
fn step(
    f: &MidiFx,
    key: Option<Key>,
    s: &mut State,
    notes: &[Note],
    out: &mut Notes,
    now: u64,
    frames: usize,
    beats: f64,
    spb: f64,
) {
    match *f {
        MidiFx::Arpeggiator {
            rate,
            order,
            octaves,
            gate,
            latch,
        } => arpeggiate(
            s,
            notes,
            out,
            (now, frames, beats, spb),
            (
                rate.max(1.0 / 64.0) as f64,
                order,
                octaves.clamp(1, 4),
                gate,
                latch,
            ),
        ),
        MidiFx::NoteRepeater {
            count,
            rate,
            decay,
            transpose,
        } => {
            let every = (rate.max(1.0 / 64.0) as f64 * spb).max(1.0);
            for n in notes {
                out.push(n.offset, n.pitch, n.velocity);
                if n.velocity <= 0.0 {
                    continue;
                }
                let mut v = n.velocity;
                for k in 1..=count.min(32) as u64 {
                    v *= decay.clamp(0.0, 1.0);
                    if v < 1.0 / 127.0 {
                        break;
                    }
                    let at = now + n.offset as u64 + (k as f64 * every) as u64;
                    let pitch = moved(n.pitch, transpose as i32 * k as i32);
                    s.wait(Waiting {
                        at,
                        pitch,
                        velocity: v,
                    });
                    s.wait(Waiting {
                        at: at + (every * 0.5) as u64,
                        pitch,
                        velocity: 0.0,
                    });
                }
            }
        }
        // The rest change each note as it comes: its note-off ends what it became.
        _ => {
            for n in notes {
                let p = n.pitch as usize & 127;
                if n.velocity < 0.0 {
                    out.push(n.offset, n.pitch, n.velocity);
                    continue;
                }
                // A note-off (or the same key again): what it became ends.
                for &q in &s.became[p][..s.count[p] as usize] {
                    out.push(n.offset, q, 0.0);
                }
                s.count[p] = 0;
                if n.velocity == 0.0 {
                    continue;
                }
                let mut made = [(0u8, 0f32); SPREAD];
                let mut m = 0;
                let mut add = |q: u8, v: f32| {
                    if m < SPREAD {
                        made[m] = (q, v);
                        m += 1;
                    }
                };
                match *f {
                    MidiFx::ChordTrigger { shape } => {
                        for q in shape.pitches(n.pitch, key) {
                            add(q, n.velocity);
                        }
                    }
                    MidiFx::ScaleQuantizer { root, minor } => {
                        let k = root.map(|root| Key { root, minor }).or(key);
                        add(k.map_or(n.pitch, |k| in_scale(n.pitch, k)), n.velocity);
                    }
                    MidiFx::Transposer { semitones } => {
                        add(moved(n.pitch, semitones as i32), n.velocity)
                    }
                    MidiFx::Velocity {
                        scale,
                        add: by,
                        min,
                        max,
                    } => {
                        let v = n.velocity * 127.0 * scale as f32 / 100.0 + by as f32;
                        let (lo, hi) =
                            (min.clamp(1, 127) as f32, max.clamp(min.max(1), 127) as f32);
                        add(n.pitch, v.clamp(lo, hi) / 127.0);
                    }
                    MidiFx::Randomizer {
                        pitch,
                        velocity,
                        chance,
                    } => {
                        if s.roll() * 100.0 < chance as f32 {
                            let p = (s.roll() * 2.0 - 1.0) * pitch as f32;
                            let v = (s.roll() * 2.0 - 1.0) * velocity as f32 / 127.0;
                            add(
                                moved(n.pitch, p.round() as i32),
                                (n.velocity + v).clamp(1.0 / 127.0, 1.0),
                            );
                        }
                    }
                    _ => add(n.pitch, n.velocity),
                }
                for &(q, v) in &made[..m] {
                    out.push(n.offset, q, v);
                    s.became[p][s.count[p] as usize] = q;
                    s.count[p] += 1;
                }
            }
        }
    }
    // What was waiting and comes due this block.
    let end = now + frames as u64;
    let mut kept = 0;
    for k in 0..s.n_waiting {
        let w = s.waiting[k];
        if w.at < end {
            out.push(w.at.saturating_sub(now) as u32, w.pitch, w.velocity);
        } else {
            s.waiting[kept] = w;
            kept += 1;
        }
    }
    s.n_waiting = kept;
}

/// The arpeggiator: keys going down and up change what's held; each step on the beat plays
/// the next of it (or all of it), its note-off `gate` of a step later.
fn arpeggiate(
    s: &mut State,
    notes: &[Note],
    out: &mut Notes,
    (now, frames, beats, spb): (u64, usize, f64, f64),
    (rate, order, octaves, gate, latch): (f64, ArpOrder, u8, f32, bool),
) {
    // Steps this block, where each falls; and the keys as they went before each.
    let mut steps = [0u32; 64];
    let mut n_steps = 0;
    let mut b = (beats / rate).ceil() * rate;
    let end = beats + frames as f64 / spb;
    while b < end && n_steps < 64 {
        steps[n_steps] = ((b - beats) * spb)
            .round()
            .min(frames as f64 - 1.0)
            .max(0.0) as u32;
        n_steps += 1;
        b += rate;
    }
    let mut keys = notes.iter().peekable();
    for &at in &steps[..n_steps] {
        while let Some(n) = keys.next_if(|n| n.offset <= at) {
            hold(s, n, latch);
        }
        play_step(s, out, now, at, rate * spb, (order, octaves, gate));
    }
    for n in keys {
        hold(s, n, latch);
    }
}

/// A key down or up, as the arpeggiator holds it.
fn hold(s: &mut State, n: &Note, latch: bool) {
    if n.velocity < 0.0 {
        return;
    }
    let at = s.held[..s.n_held].iter().position(|h| h.0 == n.pitch);
    if n.velocity > 0.0 {
        // With a latch, the first key down after all were up starts what's held afresh.
        if latch && s.down == 0 {
            s.n_held = 0;
            s.step = 0;
        }
        s.down += 1;
        if at.is_none() && s.n_held < HOLD {
            s.held[s.n_held] = (n.pitch, n.velocity);
            s.n_held += 1;
        }
    } else {
        s.down = s.down.saturating_sub(1);
        if let (Some(i), false) = (at, latch) {
            s.held.copy_within(i + 1..s.n_held, i);
            s.n_held -= 1;
        }
    }
}

/// One step: the next of what's held (up the octaves), or all of it, from `at` for `gate` of a
/// step `len` samples long.
fn play_step(
    s: &mut State,
    out: &mut Notes,
    now: u64,
    at: u32,
    len: f64,
    (order, octaves, gate): (ArpOrder, u8, f32),
) {
    let n = s.n_held;
    if n == 0 {
        return;
    }
    let mut sorted = [(0u8, 0f32); HOLD];
    sorted[..n].copy_from_slice(&s.held[..n]);
    if order != ArpOrder::Played {
        sorted[..n].sort_by_key(|h| h.0);
    }
    let all = n * octaves as usize;
    let nth = |i: usize| {
        let (h, o) = (sorted[i % n], i / n);
        (moved(h.0, 12 * o as i32), h.1)
    };
    let off = now + at as u64 + (len * gate.clamp(0.05, 1.0) as f64).max(1.0) as u64;
    let mut sound = |s: &mut State, (pitch, velocity): (u8, f32)| {
        out.push(at, pitch, velocity);
        s.wait(Waiting {
            at: off,
            pitch,
            velocity: 0.0,
        });
    };
    match order {
        ArpOrder::Chord => {
            for i in 0..n {
                sound(s, nth(i));
            }
        }
        ArpOrder::Random => {
            let i = (s.roll() * all as f32) as usize % all;
            sound(s, nth(i));
        }
        _ => {
            let i = match order {
                ArpOrder::Down => all - 1 - s.step % all,
                ArpOrder::UpDown if all > 1 => {
                    let k = s.step % (2 * all - 2);
                    if k < all { k } else { 2 * all - 2 - k }
                }
                _ => s.step % all,
            };
            sound(s, nth(i));
        }
    }
    s.step += 1;
}

#[cfg(test)]
mod tests;
