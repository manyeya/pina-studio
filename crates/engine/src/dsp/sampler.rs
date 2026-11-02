//! The built-in sampler: samples on notes. A drum kit puts one on each note, played out to its end
//! however short the note; one sample can also play across a range of notes, repitched from its
//! root, as an 808 or a bass, and stop (gated) when the note does. A zone plays a slice of its
//! sample (from `start` to `end`), forwards or backwards, looping while held (forward, its seam
//! crossfaded, or ping-pong); zones split by velocity are layers (blending over their velocity
//! fades), zones on layers of their own play together (stacked), and zones sharing notes,
//! velocities and a layer take turns (round robins). A zone can play as its note is let go
//! instead (a release sample). Sounds in a choke group cut each other off, as a closed hat cuts
//! off an open one.
//!
//! How it reads its samples: Repitch (faster is higher; read finely, from copies of the sample
//! halved in rate so what's played high doesn't alias), Stretch (overlapping grains keep each
//! note's pitch while the sample goes by at its own speed, or holds still) or Grains (a cloud of
//! them: size, density, spray, detune, reverse, spread). Its character: clean, tape (warm,
//! saturated, a little wow) or a vintage sampler's (its rate and bits, without smoothing).
//!
//! Every voice has an amp envelope, a filter (state-variable low, high, band pass or notch; a
//! four-pole ladder; a comb; vowel formants; with drive) with its own envelope and key
//! tracking, two LFOs, a modulation envelope and a matrix routing them (and velocity, key, the
//! mod wheel, pressure, a random value per note, the bend) to its settings; unison voices,
//! detuned and spread; and what's played with the sample: a pitch drop at its start, a sub
//! sine, noise, FM, ring modulation and punch. Mono mode plays one note at a time, legato
//! gliding between them. Pitch bend (two semitones either way) and each note's tuning
//! expression move what's playing, smoothly, so an 808 can glide. Its settings change as it
//! plays (`follow`) without reloading its samples or cutting what's sounding.

pub(crate) mod analysis;
pub(crate) mod filter;
pub(crate) mod modulation;
mod read;
mod tap;

pub use analysis::{cuts, note_of, onsets, pitch, slices};
pub use modulation::wave;
pub use tap::{Playing, SamplerTap};

use std::{path::Path, sync::Arc};

use crate::{
    audio::engine::load_wav,
    plugin::{Control, ControlKind, ExpressionKind, Note},
    song::project::{
        self, FilterMode, LoopMode, ModDest, ModSource, SampleCharacter, SampleMode, SamplerVoice,
        Trigger, Zone,
    },
};
use filter::{COMB, Coefs, Filter};
use modulation::{Env, Shape, Sources, Stage, dest, fall, random, routed, source_index};
use read::Mips;

const VOICES: usize = 64;
/// How fast a choked (or stolen) sound fades: quick, but not a click.
const CHOKE_MS: f32 = 4.0;
/// How far pitch bend goes, in semitones.
const BEND_RANGE: f32 = 2.0;
/// How quickly a voice's pitch follows a bend or tuning move: the time constant, in ms.
const GLIDE_MS: f32 = 8.0;
/// What moves a voice is worked out again every this many samples.
const EVERY: u32 = 16;
/// The most grains a voice plays at once.
const GRAINS: usize = 24;
/// The most layers (zones at once) a note plays.
const LAYERS: usize = 8;

/// A zone's settings, as numbers (copied into place as it plays, with no allocation).
#[derive(Clone, Copy, Debug)]
struct Params {
    note: u8,
    high: u8,
    root: u8,
    tune: f32,
    /// Left and right gain: the zone's level and pan.
    gain: [f32; 2],
    gate: bool,
    release_ms: Option<f32>,
    choke: Option<u8>,
    start: f32,
    end: f32,
    looping: LoopMode,
    loop_start: f32,
    loop_end: f32,
    loop_fade: f32,
    reverse: bool,
    velocities: (u8, u8),
    fade: u8,
    trigger: Trigger,
    layer: u8,
}

impl Params {
    fn of(z: &Zone) -> Params {
        let level = 10f32.powf(z.gain_db / 20.0);
        let pan = z.pan.clamp(-1.0, 1.0);
        let start = z.start.clamp(0.0, 1.0);
        let end = z.end.unwrap_or(1.0).clamp(start, 1.0);
        Params {
            note: z.note,
            high: z.high.unwrap_or(z.note),
            root: z.root.unwrap_or(z.note),
            tune: z.tune_cents / 100.0,
            gain: [level * (1.0 - pan).min(1.0), level * (1.0 + pan).min(1.0)],
            // A loop sustains while held: it stops (released) when its note does.
            gate: z.gate || z.loop_mode != LoopMode::Off,
            release_ms: z.release_ms,
            choke: z.choke,
            start,
            end,
            looping: z.loop_mode,
            loop_start: z.loop_start.unwrap_or(start).clamp(start, end),
            loop_end: z.loop_end.unwrap_or(end).clamp(start, end),
            loop_fade: z.loop_fade.clamp(0.0, 0.5),
            reverse: z.reverse,
            velocities: z.velocities.unwrap_or((1, 127)),
            fade: z.velocity_fade,
            trigger: z.trigger,
            layer: z.layer,
        }
    }

    /// How much it plays at a note and velocity: 0 not at all, 1 fully, between over its
    /// velocity fade (an equal-power curve, so blended layers keep their loudness).
    fn weight(&self, note: u8, velocity: u8) -> f32 {
        if !(self.note..=self.high).contains(&note) {
            return 0.0;
        }
        let (lo, hi) = (self.velocities.0 as f32, self.velocities.1 as f32);
        let (v, f) = (velocity.max(1) as f32, self.fade as f32 + 1.0);
        let x = if v < lo {
            1.0 - (lo - v) / f
        } else if v > hi {
            1.0 - (v - hi) / f
        } else {
            1.0
        };
        match x <= 0.0 {
            true => 0.0,
            false => (x.min(1.0) * std::f32::consts::FRAC_PI_2).sin(),
        }
    }

    /// Zones alike in notes and velocities, on one layer, take turns.
    fn alike(&self, o: &Params) -> bool {
        (
            self.note,
            self.high,
            self.velocities,
            self.trigger,
            self.layer,
        ) == (o.note, o.high, o.velocities, o.trigger, o.layer)
    }
}

struct Loaded {
    p: Params,
    mips: Mips,
}

impl Loaded {
    fn new(z: &Zone, channels: Vec<Vec<f32>>) -> Loaded {
        Loaded {
            p: Params::of(z),
            mips: Mips::new(channels),
        }
    }

    fn len(&self) -> f64 {
        self.mips.levels[0][0].len() as f64
    }
}

#[derive(Clone, Copy, Default)]
struct Grain {
    on: bool,
    pos: f64,
    step: f64,
    age: u32,
    len: u32,
    gain: [f32; 2],
}

/// A grain's window at `age` of `len`: smooth (Hann) at shape 0, flat-topped toward 1.
fn window(age: u32, len: u32, shape: f32) -> f32 {
    let t = age as f32 / len.max(1) as f32;
    let taper = 0.5 * (1.0 - 0.9 * shape.clamp(0.0, 1.0));
    let edge = t.min(1.0 - t);
    match edge < taper {
        true => 0.5 - 0.5 * (std::f32::consts::PI * edge / taper).cos(),
        false => 1.0,
    }
}

#[derive(Clone, Copy, Default)]
struct Voice {
    on: bool,
    zone: usize,
    note: u8,
    /// Where in the sample (frames), which way it's reading (1 or -1), and the frames it plays
    /// between, loops between, and crossfades its loop's seam over.
    pos: f64,
    dir: f64,
    bounds: (f64, f64),
    loops: (f64, f64),
    fade_len: f64,
    /// Repitch: how far it moves a sample (which way, too). Stretch and Grains: how fast each
    /// grain reads, and how fast the place they read from moves.
    step: f64,
    speed: f64,
    /// Plays once, as a release sample: not looping, not stopped by its note.
    once: bool,
    /// Semitones off its note: bends, tuning and glides, gliding to `target` at `glide`.
    shift: f32,
    target: f32,
    glide: f32,
    /// A unison voice's detune (semitones) and place (-1 to 1).
    detune: f32,
    pan: f32,
    /// Its level from velocity, layer and unison; and the velocity (0 to 1) and random value
    /// (-1 to 1) it was played with, for the matrix.
    gain: f32,
    velocity: f32,
    random: f32,
    amp: Env,
    fenv: Env,
    menv: Env,
    /// Its zone's own release (a factor a sample), if it has one, over the instrument's.
    release: Option<f32>,
    /// 1 until choked or stolen; then falling by `fade` a sample.
    level: f32,
    fade: f32,
    held: bool,
    /// When it started, to take the oldest voice when all are busy.
    age: u64,
    /// Samples since it started, and its LFOs' phases as it started them over (when they do).
    tick: u32,
    lfo_from: [f64; 2],
    /// The pitch drop's semitones now; punch's boost now.
    penv: f32,
    punch: f32,
    /// The sub's and the FM/ring oscillator's phases (cycles), and the note's frequency.
    phases: [f32; 2],
    note_hz: f32,
    noise_seed: u32,
    noise_tilt: [f32; 2],
    /// Moved by the matrix, worked out every few samples: FM, ring, sub and noise amounts.
    fm: f32,
    ring: f32,
    sub: f32,
    noise: f32,
    filter: Filter,
    coefs: Coefs,
    /// Each side's gain, ramping to where it's going over the next few samples.
    out: [f32; 2],
    out_step: [f32; 2],
    /// Its character's memory: the vintage sampler's held value and its clock, and smoothing.
    hold: [f32; 2],
    clock: f32,
    soft: [[f32; 2]; 2],
    /// Stretch and Grains: its grains, the place they read from (frames), the samples to the
    /// next, and their size, spacing, scatter (frames) and the place set (0 to 1).
    grains: [Grain; GRAINS],
    head: f64,
    wait: f32,
    grain_len: f32,
    interval: f32,
    spray: f32,
    position: f32,
    seed: u64,
}

/// What every voice reads from the instrument this stretch of a block.
struct Ctx {
    v: SamplerVoice,
    rate: f32,
    /// The LFOs' phases where this stretch starts, and how far they go a sample.
    base: [f64; 2],
    inc: [f64; 2],
    /// Samples since it started, at this stretch's start (the tape's wow).
    clock: u64,
    bend: f32,
    wheel: f32,
    pressure: f32,
    amp: Shape,
    filter: Shape,
    menv: Shape,
    gain: f32,
    /// How much the pitch drop keeps every few samples, and punch every sample.
    penv: f32,
    punch: f32,
    /// The tape's smoothing, the vintage clock a sample (in its ticks), its bits' steps and
    /// its output filter's smoothing.
    tape: f32,
    crush: f32,
    steps: f32,
    crush_soft: f32,
}

pub struct Sampler {
    rate: f32,
    zones: Vec<Loaded>,
    voice: SamplerVoice,
    amp: Shape,
    filter: Shape,
    menv: Shape,
    voices: Vec<Voice>,
    /// Each voice's comb delay lines, left then right, `COMB` long each.
    combs: Vec<f32>,
    started: u64,
    /// Pitch bend, in semitones; and each note's tuning (semitones) and volume (gain), by pitch.
    bend: f32,
    tuning: [f32; 128],
    gain: [f32; 128],
    /// The mod wheel, channel pressure and each note's (0 to 1); each note's last velocity,
    /// for its release samples.
    wheel: f32,
    pressure: f32,
    poly: [f32; 128],
    pressed: [f32; 128],
    /// The LFOs' phases (cycles since it began), the samples since then, and the song's tempo.
    lfo: [f64; 2],
    clock: u64,
    tempo: f32,
    /// Each note's next round robin.
    turns: [u16; 128],
    /// Mono: the notes held, the latest last.
    held: [u8; 16],
    holding: usize,
    /// The settings followed last (by address).
    following: usize,
    /// Its voice is moved by modulators.
    modulated: bool,
    tap: Arc<SamplerTap>,
}

impl Sampler {
    /// Loads the zones' samples from `dir` at `rate`. A sample that can't be read is an error.
    pub fn load(s: &project::Sampler, dir: &Path, rate: f64) -> Result<Sampler, String> {
        let zones = s
            .zones
            .iter()
            .map(|z| Ok(Loaded::new(z, load_wav(&dir.join(&z.file), rate)?.channels)))
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Sampler::with(zones, s.voice, rate as f32))
    }

    fn with(zones: Vec<Loaded>, voice: SamplerVoice, rate: f32) -> Sampler {
        let mut s = Sampler {
            rate,
            zones,
            voice,
            amp: Shape::new(0.0, 0.0, 1.0, 60.0, rate),
            filter: Shape::new(0.0, 400.0, 0.0, 200.0, rate),
            menv: Shape::new(0.0, 500.0, 0.0, 300.0, rate),
            voices: vec![Voice::default(); VOICES],
            combs: vec![0.0; VOICES * 2 * COMB],
            started: 0,
            bend: 0.0,
            tuning: [0.0; 128],
            gain: [1.0; 128],
            wheel: 0.0,
            pressure: 0.0,
            poly: [0.0; 128],
            pressed: [1.0; 128],
            lfo: [0.0; 2],
            clock: 0,
            tempo: 120.0,
            turns: [0; 128],
            held: [0; 16],
            holding: 0,
            following: 0,
            modulated: false,
            tap: Arc::new(SamplerTap::default()),
        };
        s.shape();
        s
    }

    /// Where its voices are playing, for the editor.
    pub fn tap(&self) -> Arc<SamplerTap> {
        self.tap.clone()
    }

    /// The song's tempo, which LFOs in time with it follow.
    pub fn set_tempo(&mut self, bpm: f32) {
        self.tempo = bpm.max(1.0);
    }

    fn shape(&mut self) {
        let v = &self.voice;
        self.amp = Shape::new(v.attack_ms, v.decay_ms, v.sustain, v.release_ms, self.rate);
        self.filter = Shape::new(
            v.filter_attack_ms,
            v.filter_decay_ms,
            v.filter_sustain,
            v.filter_release_ms,
            self.rate,
        );
        let m = v.mod_env;
        self.menv = Shape::new(m.attack_ms, m.decay_ms, m.sustain, m.release_ms, self.rate);
    }

    /// Takes on changed settings, its samples the same (the same zones, in order): what's
    /// playing plays on. Allocation-free, for the audio thread; `at` tells one set of settings
    /// from another.
    pub fn follow(&mut self, s: &project::Sampler, at: usize) {
        if at == self.following || s.zones.len() != self.zones.len() {
            return;
        }
        self.following = at;
        for (l, z) in self.zones.iter_mut().zip(&s.zones) {
            l.p = Params::of(z);
        }
        if s.voice != self.voice {
            self.voice = s.voice;
            self.shape();
        }
    }

    /// Its voice as its settings have it moved by modulators, this block; back as they have it
    /// when nothing moves it. (Envelope times aren't modulated: they'd need reshaping.)
    pub fn modulate(&mut self, base: &project::SamplerVoice, by: &crate::dsp::modulate::Offsets) {
        if by.voice {
            let mut v = *base;
            by.voice(&mut v);
            self.voice = v;
            self.modulated = true;
        } else if std::mem::take(&mut self.modulated) {
            self.voice = *base;
        }
    }

    /// An LFO's step a sample: a cycle `beats` long at the tempo, else `hz` a second.
    fn lfo_inc(&self, hz: f32, beats: f32) -> f64 {
        let hz = match beats > 0.0 {
            true => self.tempo / 60.0 / beats,
            false => hz.max(0.0),
        };
        hz as f64 / self.rate as f64
    }

    fn ctx(&self) -> Ctx {
        let v = self.voice;
        let rate = self.rate;
        let crush_hz = (v.crush_khz * 1000.0).clamp(2_000.0, rate);
        let smooth = |hz: f32| 1.0 - (-std::f32::consts::TAU * hz.min(rate * 0.45) / rate).exp();
        Ctx {
            v,
            rate,
            base: self.lfo,
            inc: [
                self.lfo_inc(v.lfo_hz, v.lfo_beats),
                self.lfo_inc(v.lfo2.hz, v.lfo2.beats),
            ],
            clock: self.clock,
            bend: self.bend,
            wheel: self.wheel,
            pressure: self.pressure,
            amp: self.amp,
            filter: self.filter,
            menv: self.menv,
            gain: 10f32.powf(v.gain_db / 20.0),
            penv: fall(v.pitch_decay_ms.max(1.0), rate).powi(EVERY as i32),
            punch: fall(40.0, rate),
            tape: smooth(20_000.0 - 14_000.0 * v.color.clamp(0.0, 1.0)),
            crush: crush_hz / rate,
            steps: (1u32 << (v.bits.clamp(2, 16) - 1)) as f32,
            crush_soft: smooth(0.42 * crush_hz),
        }
    }

    /// Adds `frames` of output to `buf`, applying each note and control at its offset. Both
    /// must be sorted by offset. Allocation-free.
    pub fn process(
        &mut self,
        buf: [&mut [f32]; 2],
        frames: usize,
        notes: &[Note],
        controls: &[Control],
    ) {
        let [l, r] = buf;
        let mut next = notes.iter().peekable();
        let mut moves = controls.iter().peekable();
        let mut at = 0;
        while at < frames {
            while let Some(c) = moves.next_if(|c| c.offset as usize <= at) {
                self.control(c.kind);
            }
            while let Some(n) = next.next_if(|n| n.offset as usize <= at) {
                self.note(n);
            }
            // On to the next note or control (or the block's end).
            let end = [
                next.peek().map(|n| n.offset as usize),
                moves.peek().map(|c| c.offset as usize),
            ]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(frames)
            .clamp(at + 1, frames);
            self.render(&mut l[at..end], &mut r[at..end]);
            at = end;
        }
        self.publish();
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        let cx = self.ctx();
        let Sampler {
            voices,
            zones,
            combs,
            gain,
            poly,
            ..
        } = self;
        for (i, vc) in voices.iter_mut().enumerate().filter(|(_, v)| v.on) {
            let comb = &mut combs[i * 2 * COMB..(i + 1) * 2 * COMB];
            let note = vc.note as usize;
            let z = &zones[vc.zone];
            render_voice(
                vc,
                z,
                comb,
                gain[note],
                poly[note],
                &cx,
                &mut l[..n],
                &mut r[..n],
            );
        }
        for k in 0..2 {
            self.lfo[k] += cx.inc[k] * n as f64;
        }
        self.clock += n as u64;
    }

    /// Tells the editor where the voices are: each one's place, and in Stretch and Grains its
    /// grains'.
    fn publish(&self) {
        let mut k = 0;
        let mut put = |p: Playing| {
            if k < tap::SLOTS {
                self.tap.write(k, Some(p));
                k += 1;
            }
        };
        for vc in self.voices.iter().filter(|v| v.on) {
            let z = &self.zones[vc.zone];
            let len = z.len().max(1.0);
            let level = vc.amp.level * vc.level;
            let head = match self.voice.mode {
                SampleMode::Repitch => vc.pos,
                _ => vc.head,
            };
            put(Playing {
                zone: vc.zone,
                at: (head / len) as f32,
                level,
                grain: false,
            });
            if self.voice.mode != SampleMode::Repitch {
                // A grain run on past the sample's end reads silence: none to show.
                for g in vc
                    .grains
                    .iter()
                    .filter(|g| g.on && g.pos >= 0.0 && g.pos < len)
                {
                    put(Playing {
                        zone: vc.zone,
                        at: (g.pos / len) as f32,
                        level: level * window(g.age, g.len, self.voice.grain_shape),
                        grain: true,
                    });
                }
            }
        }
        self.tap.write(k, None);
    }

    /// The share of the way to its target a glide of `ms` goes a sample.
    fn coef(&self, ms: f32) -> f32 {
        if ms <= 0.05 {
            1.0
        } else {
            1.0 - (-1.0 / (ms * 0.001 * self.rate)).exp()
        }
    }

    /// A bend, the mod wheel, pressure, or a note's tuning or volume, from here on.
    fn control(&mut self, kind: ControlKind) {
        match kind {
            ControlKind::PitchBend(b) => self.bend = b.clamp(-1.0, 1.0) * BEND_RANGE,
            ControlKind::Cc { number: 1, value } => {
                self.wheel = value.clamp(0.0, 1.0);
                return;
            }
            ControlKind::Aftertouch(v) => {
                self.pressure = v.clamp(0.0, 1.0);
                return;
            }
            ControlKind::PolyPressure { pitch, value } => {
                self.poly[pitch as usize & 127] = value.clamp(0.0, 1.0);
                return;
            }
            ControlKind::Expression { pitch, kind, value } => match kind {
                ExpressionKind::Tuning => {
                    self.tuning[pitch as usize & 127] = value.clamp(-120.0, 120.0)
                }
                ExpressionKind::Volume => self.gain[pitch as usize & 127] = value.clamp(0.0, 4.0),
                _ => return,
            },
            _ => return,
        }
        let (bend, tuning, glide) = (self.bend, self.tuning, self.coef(GLIDE_MS));
        for v in self.voices.iter_mut().filter(|v| v.on) {
            v.target = bend + tuning[v.note as usize];
            v.glide = glide;
        }
    }

    /// A voice's note let go: a gated sound into its release; a one-shot plays on.
    fn release_voice(&mut self, i: usize) {
        let gate = self.zones[self.voices[i].zone].p.gate;
        let v = &mut self.voices[i];
        v.held = false;
        if gate && !v.once {
            v.amp.release();
            v.fenv.release();
            v.menv.release();
        }
    }

    /// The zones a note plays at a velocity, as `trigger` has them, each with how much: one of
    /// each set of zones alike (the next in turn, as round robins), up to `LAYERS`.
    fn pick(
        &mut self,
        pitch: u8,
        velocity: u8,
        trigger: Trigger,
    ) -> ([(usize, f32); LAYERS], usize) {
        let mut out = [(0, 0.0); LAYERS];
        let mut n = 0;
        let turn = self.turns[pitch as usize] as usize;
        let zones = &self.zones;
        for (i, z) in zones.iter().enumerate() {
            if z.p.trigger != trigger || n == LAYERS {
                continue;
            }
            let w = z.p.weight(pitch, velocity);
            // The first of its set stands for the set.
            if w <= 0.0 || zones[..i].iter().any(|y| y.p.alike(&z.p)) {
                continue;
            }
            let set = zones.iter().enumerate().filter(|(_, y)| y.p.alike(&z.p));
            let members = set.clone().count();
            let chosen = set.map(|(k, _)| k).nth(turn % members).unwrap_or(i);
            out[n] = (chosen, w);
            n += 1;
        }
        if n > 0 {
            self.turns[pitch as usize] = self.turns[pitch as usize].wrapping_add(1);
        }
        (out, n)
    }

    fn note(&mut self, n: &Note) {
        let pitch = n.pitch & 127;
        // Choked: another pad in its kit's choke group played; what's sounding falls quiet.
        if n.velocity < 0.0 {
            let fade = 1.0 / (CHOKE_MS * 0.001 * self.rate);
            for v in self.voices.iter_mut().filter(|v| v.on) {
                v.fade = v.fade.max(fade);
            }
            return;
        }
        if n.velocity <= 0.0 {
            if self.voice.mono {
                self.let_go_mono(pitch);
            } else {
                for i in 0..VOICES {
                    if self.voices[i].on && self.voices[i].held && self.voices[i].note == pitch {
                        self.release_voice(i);
                    }
                }
            }
            // Its release samples, at the velocity it was played with.
            let velocity = self.pressed[pitch as usize];
            self.start(pitch, velocity, Trigger::Release, None);
            return;
        }
        self.pressed[pitch as usize] = n.velocity.clamp(0.0, 1.0);
        // Mono: one note's voices. Legato over a held note glides them to the new one without
        // starting over; otherwise what's sounding fades as the new note starts, gliding from
        // the last.
        let mut from = None;
        if self.voice.mono {
            let sounding = (0..VOICES).find(|&i| {
                let v = &self.voices[i];
                v.on && !v.once && v.amp.stage != Stage::Release
            });
            let was = self.top();
            self.hold(pitch);
            if let Some(i) = sounding {
                if self.voice.legato && self.voices[i].held {
                    for k in 0..VOICES {
                        let v = &self.voices[k];
                        if v.on && v.held && !v.once {
                            self.glide_to(k, pitch);
                        }
                    }
                    return;
                }
                let fade = 1.0 / (CHOKE_MS * 0.001 * self.rate);
                for v in self.voices.iter_mut().filter(|v| v.on) {
                    v.fade = v.fade.max(fade);
                }
                from = Some(self.voices[i].note as f32 + self.voices[i].shift - self.bend);
            } else if let Some(w) = was {
                from = Some(w as f32);
            }
        }
        // A new note starts where the bend is, but untuned: its own tuning starts with it.
        self.tuning[pitch as usize] = 0.0;
        self.gain[pitch as usize] = 1.0;
        self.start(pitch, n.velocity, Trigger::Attack, from);
    }

    /// Starts what a note plays (its layers, each in unison) as `trigger` has them.
    fn start(&mut self, pitch: u8, velocity: f32, trigger: Trigger, from: Option<f32>) {
        let v127 = (velocity.clamp(0.0, 1.0) * 127.0).round() as u8;
        let (picks, n) = self.pick(pitch, v127, trigger);
        // Their choke groups fall quiet.
        for &(zone, _) in &picks[..n] {
            if let Some(group) = self.zones[zone].p.choke {
                let fade = 1.0 / (CHOKE_MS * 0.001 * self.rate);
                for v in self.voices.iter_mut().filter(|v| v.on) {
                    if self.zones[v.zone].p.choke == Some(group) {
                        v.fade = v.fade.max(fade);
                    }
                }
            }
        }
        let unison = self.voice.unison.clamp(1, 8) as usize;
        let share = 1.0 / (unison as f32).sqrt();
        for &(zone, weight) in &picks[..n] {
            for k in 0..unison {
                // Spread evenly from one side (and one detune) to the other.
                let x = match unison {
                    1 => 0.0,
                    _ => k as f32 / (unison - 1) as f32 * 2.0 - 1.0,
                };
                let start = Start {
                    zone,
                    pitch,
                    velocity,
                    weight: weight * share,
                    detune: x * self.voice.detune_cents / 200.0,
                    pan: x * self.voice.spread.clamp(0.0, 1.0),
                    from,
                    once: trigger == Trigger::Release,
                };
                self.start_voice(start);
            }
        }
    }

    fn start_voice(&mut self, st: Start) {
        let p = self.zones[st.zone].p;
        let v = self.voice;
        let pitch = st.pitch;
        // A free voice, else the oldest.
        let i = match self.voices.iter().position(|v| !v.on) {
            Some(i) => i,
            None => (0..VOICES).min_by_key(|&i| self.voices[i].age).unwrap(),
        };
        self.started += 1;
        let len = self.zones[st.zone].len();
        let bounds = (
            (p.start as f64 * len).floor(),
            (p.end as f64 * len).ceil().min(len),
        );
        let loops = (
            (p.loop_start as f64 * len).floor(),
            (p.loop_end as f64 * len).ceil().min(len),
        );
        // The crossfade takes what's before the loop: no more than there is, nor half of it.
        let fade_len = (p.loop_fade as f64 * (loops.1 - loops.0))
            .min(loops.0 - bounds.0)
            .min((loops.1 - loops.0) / 2.0)
            .max(0.0);
        let random_value = random(self.started.wrapping_mul(7919) ^ pitch as u64);
        // Where it starts: as the matrix moves it, from what's known as the note starts.
        let mut s: Sources = [0.0; 12];
        s[source_index(ModSource::Velocity)] = st.velocity;
        s[source_index(ModSource::Key)] = (pitch as f32 - 60.0) / 60.0;
        s[source_index(ModSource::Wheel)] = self.wheel;
        s[source_index(ModSource::Pressure)] = self.pressure.max(self.poly[pitch as usize]);
        s[source_index(ModSource::Random)] = random_value;
        s[source_index(ModSource::Bend)] = self.bend / BEND_RANGE;
        let m = routed(&v.matrix, &s);
        let start = m[dest(ModDest::Start)] as f64;
        let backwards = p.reverse != (v.speed < 0.0);
        let span = bounds.1 - bounds.0;
        let last = (bounds.1 - 2.0).max(bounds.0);
        let pos = match backwards {
            true => (bounds.1 - 2.0 - start * span).clamp(bounds.0, last),
            false => (bounds.0 + start * span).clamp(bounds.0, last),
        };
        let place = ((v.position + m[dest(ModDest::Position)]) as f64 + start).clamp(0.0, 1.0);
        let head = match backwards {
            true => bounds.1 - 1.0 - place * span,
            false => bounds.0 + place * span,
        };
        let glide_ms = match v.glide_ms > 0.0 {
            true => v.glide_ms,
            false => GLIDE_MS,
        };
        // Gliding in from the last note played, mono with glide on.
        let shift = match st.from {
            Some(f) if v.glide_ms > 0.0 => self.bend + (f - pitch as f32),
            _ => self.bend,
        };
        // Softer notes are quieter, more than in proportion, as a drum is (as much as the
        // instrument's velocity says).
        let curve = 1.5 * v.velocity.clamp(0.0, 1.0);
        let gain = st.velocity.clamp(0.0, 1.0).powf(curve) * st.weight;
        // A retriggered LFO starts from nought with its note.
        let from_nought = |on: bool, phase: f64| if on { -phase } else { 0.0 };
        let lfo_from = [
            from_nought(v.lfo_retrigger, self.lfo[0]),
            from_nought(v.lfo2.retrigger, self.lfo[1]),
        ];
        self.voices[i] = Voice {
            on: true,
            zone: st.zone,
            note: pitch,
            pos,
            dir: if p.reverse { -1.0 } else { 1.0 },
            bounds,
            loops,
            fade_len,
            step: 1.0,
            once: st.once,
            shift,
            target: self.bend,
            glide: self.coef(glide_ms),
            detune: st.detune,
            pan: st.pan,
            gain,
            velocity: st.velocity,
            random: random_value,
            amp: Env::started(),
            fenv: Env::started(),
            menv: Env::started(),
            release: p.release_ms.map(|ms| fall(ms.max(0.1), self.rate)),
            level: 1.0,
            held: !st.once,
            age: self.started,
            lfo_from,
            penv: v.pitch_env,
            punch: 1.5 * v.punch.clamp(0.0, 1.0),
            noise_seed: self.started as u32 | 1,
            head,
            seed: self.started.wrapping_mul(0x2545_F491_4F6C_DD1D),
            ..Voice::default()
        };
        // Its comb starts silent.
        self.combs[i * 2 * COMB..(i + 1) * 2 * COMB].fill(0.0);
    }

    fn top(&self) -> Option<u8> {
        self.holding.checked_sub(1).map(|i| self.held[i])
    }

    fn hold(&mut self, pitch: u8) {
        self.unhold(pitch);
        if self.holding == self.held.len() {
            self.held.copy_within(1.., 0);
            self.holding -= 1;
        }
        self.held[self.holding] = pitch;
        self.holding += 1;
    }

    fn unhold(&mut self, pitch: u8) {
        if let Some(i) = self.held[..self.holding].iter().position(|&p| p == pitch) {
            self.held.copy_within(i + 1..self.holding, i);
            self.holding -= 1;
        }
    }

    /// A held mono voice moved to another note, gliding there, without starting over.
    fn glide_to(&mut self, i: usize, pitch: u8) {
        let ms = match self.voice.glide_ms > 0.0 {
            true => self.voice.glide_ms,
            false => GLIDE_MS,
        };
        let glide = self.coef(ms);
        let v = &mut self.voices[i];
        // Where it sounds now, as an offset from the new note; gliding to none.
        v.shift += v.note as f32 - pitch as f32;
        v.note = pitch;
        v.glide = glide;
    }

    /// Mono: a note let go. Back to the note held under it, if there is one; else released.
    fn let_go_mono(&mut self, pitch: u8) {
        self.unhold(pitch);
        let under = self.top();
        for i in 0..VOICES {
            let v = self.voices[i];
            if !(v.on && v.held && v.note == pitch && !v.once) {
                continue;
            }
            match under {
                Some(u) if self.zones[v.zone].p.weight(u, 100) > 0.0 => self.glide_to(i, u),
                _ => self.release_voice(i),
            }
        }
    }
}

/// A voice about to start: its zone, note, velocity (0 to 1), its layer's and unison's share of
/// the level, unison's detune and place, the note it glides from, and whether it's a release
/// sample.
struct Start {
    zone: usize,
    pitch: u8,
    velocity: f32,
    weight: f32,
    detune: f32,
    pan: f32,
    from: Option<f32>,
    once: bool,
}

/// Every few samples: where the voice's pitch, filter, level and grains are now, the matrix
/// moving them.
fn control(vc: &mut Voice, z: &Loaded, note_gain: f32, poly: f32, cx: &Ctx, i: usize) {
    let v = &cx.v;
    let p = &z.p;
    let phase = |k: usize| cx.base[k] + cx.inc[k] * i as f64 + vc.lfo_from[k];
    let lfo1 = wave(v.lfo_wave, phase(0), 1);
    let lfo2 = wave(v.lfo2.wave, phase(1), 2);
    let mut s: Sources = [0.0; 12];
    s[source_index(ModSource::Lfo1)] = lfo1;
    s[source_index(ModSource::Lfo2)] = lfo2;
    s[source_index(ModSource::ModEnv)] = vc.menv.level;
    s[source_index(ModSource::AmpEnv)] = vc.amp.level;
    s[source_index(ModSource::FilterEnv)] = vc.fenv.level;
    s[source_index(ModSource::Velocity)] = vc.velocity;
    s[source_index(ModSource::Key)] = (vc.note as f32 - 60.0) / 60.0;
    s[source_index(ModSource::Wheel)] = cx.wheel;
    s[source_index(ModSource::Pressure)] = cx.pressure.max(poly);
    s[source_index(ModSource::Random)] = vc.random;
    s[source_index(ModSource::Bend)] = cx.bend / BEND_RANGE;
    let m = routed(&v.matrix, &s);
    let at = |d: ModDest| m[dest(d)];
    // Pitch: the note against the zone's root, its tuning and the instrument's, what's gliding,
    // unison's detune, vibrato, the pitch drop, the matrix, and tape's wow and flutter.
    let wow = match v.character {
        SampleCharacter::Tape => {
            let t = (cx.clock + i as u64) as f32 / cx.rate;
            v.color.clamp(0.0, 1.0)
                * (0.05 * (std::f32::consts::TAU * 0.6 * t).sin()
                    + 0.015 * (std::f32::consts::TAU * 6.5 * t).sin())
        }
        _ => 0.0,
    };
    let played =
        vc.note as f32 + v.transpose + vc.shift + vc.detune + vc.penv + at(ModDest::Pitch) * 24.0;
    let semis = played - p.root as f32 + p.tune + v.vibrato_cents / 100.0 * lfo1 + wow;
    let ratio = 2f64.powf(semis as f64 / 12.0);
    let speed = (v.speed + at(ModDest::Speed) * 2.0) as f64;
    match v.mode {
        SampleMode::Repitch => vc.step = ratio * speed * vc.dir,
        _ => {
            vc.step = ratio * vc.dir;
            vc.speed = speed * vc.dir;
        }
    }
    vc.note_hz = 440.0 * 2f32.powf((played - 69.0) / 12.0);
    vc.penv *= cx.penv;
    if v.filter != FilterMode::Off {
        let octaves = v.filter_env * vc.fenv.level
            + v.lfo_filter * lfo1
            + v.key_track * (vc.note as f32 - 60.0) / 12.0
            + at(ModDest::Cutoff) * 6.0;
        let hz = v.cutoff_hz * 2f32.powf(octaves);
        let reso = v.resonance + at(ModDest::Resonance);
        let drive = v.drive + at(ModDest::Drive);
        vc.coefs = filter::coefs(v.filter, hz, reso, drive, cx.rate);
    }
    // Level and place: tremolo, the matrix, the note's own volume, velocity, layer and unison.
    let tremolo = 1.0 - v.tremolo.clamp(0.0, 1.0) * (0.5 + 0.5 * lfo1);
    let level =
        tremolo * (1.0 + at(ModDest::Level)).clamp(0.0, 2.0) * note_gain * vc.gain * cx.gain;
    let pan = (vc.pan + at(ModDest::Pan)).clamp(-1.0, 1.0);
    let to = [
        level * p.gain[0] * (1.0 - pan).min(1.0),
        level * p.gain[1] * (1.0 + pan).min(1.0),
    ];
    if vc.tick == 0 {
        vc.out = to;
    }
    vc.out_step = [
        (to[0] - vc.out[0]) / EVERY as f32,
        (to[1] - vc.out[1]) / EVERY as f32,
    ];
    vc.fm = (v.fm + at(ModDest::Fm)).clamp(0.0, 1.0);
    vc.ring = v.ring.clamp(0.0, 1.0);
    vc.sub = (v.sub + at(ModDest::Sub)).clamp(0.0, 1.0);
    vc.noise = (v.noise + at(ModDest::Noise)).clamp(0.0, 1.0);
    // Grains: size, spacing (Stretch overlaps four), scatter and place.
    let size = (v.grain_ms * 8f32.powf(at(ModDest::Size))).clamp(5.0, 2_000.0);
    vc.grain_len = size * 0.001 * cx.rate;
    vc.interval = match v.mode {
        SampleMode::Stretch => vc.grain_len / 4.0,
        _ => cx.rate / (v.density * 8f32.powf(at(ModDest::Density))).clamp(0.5, 500.0),
    };
    vc.spray = (v.spray + at(ModDest::Spray)).clamp(0.0, 1.0) * cx.rate;
    vc.position = (v.position + at(ModDest::Position)).clamp(0.0, 1.0);
}

/// A voice's part of a stretch of a block, added to `l` and `r`.
#[allow(clippy::too_many_arguments)]
fn render_voice(
    vc: &mut Voice,
    z: &Loaded,
    comb: &mut [f32],
    note_gain: f32,
    poly: f32,
    cx: &Ctx,
    l: &mut [f32],
    r: &mut [f32],
) {
    let v = &cx.v;
    let (comb_l, comb_r) = comb.split_at_mut(COMB);
    for i in 0..l.len() {
        if vc.tick.is_multiple_of(EVERY) {
            control(vc, z, note_gain, poly, cx, i);
        }
        vc.tick = vc.tick.wrapping_add(1);
        let e = vc.amp.next(&cx.amp, vc.release.unwrap_or(cx.amp.release));
        if vc.amp.stage == Stage::Done || vc.level <= 0.0 {
            vc.on = false;
            return;
        }
        vc.fenv.next(&cx.filter, cx.filter.release);
        vc.menv.next(&cx.menv, cx.menv.release);
        // The oscillator FM and ring modulation ride on.
        let osc = match vc.fm > 0.0 || vc.ring > 0.0 {
            true => {
                let hz = vc.note_hz * v.osc_ratio.clamp(0.25, 16.0);
                vc.phases[1] = (vc.phases[1] + hz / cx.rate).fract();
                (vc.phases[1] * std::f32::consts::TAU).sin()
            }
            false => 0.0,
        };
        let fm = match vc.fm > 0.0 {
            true => (vc.fm * 2.0 * osc).exp2() as f64,
            false => 1.0,
        };
        let read = match v.mode {
            SampleMode::Repitch => read_repitch(vc, z, v, fm),
            _ => read_grains(vc, z, v, fm),
        };
        let Some([mut x, mut y]) = read else {
            vc.on = false;
            return;
        };
        character(vc, cx, &mut x, &mut y);
        if vc.sub > 0.0 {
            vc.phases[0] = (vc.phases[0] + vc.note_hz * 0.5 / cx.rate).fract();
            let s = (vc.phases[0] * std::f32::consts::TAU).sin() * vc.sub * 0.5;
            x += s;
            y += s;
        }
        if vc.noise > 0.0 {
            let color = v.noise_color.clamp(-1.0, 1.0);
            for (k, side) in [&mut x, &mut y].into_iter().enumerate() {
                vc.noise_seed ^= vc.noise_seed << 13;
                vc.noise_seed ^= vc.noise_seed >> 17;
                vc.noise_seed ^= vc.noise_seed << 5;
                let white = vc.noise_seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
                // Darker: toward its low part; brighter: toward what's above that.
                let low = &mut vc.noise_tilt[k];
                *low += (white - *low) * 0.08;
                let shaped = match color < 0.0 {
                    true => *low * 2.5,
                    false => (white - *low) * 1.3,
                };
                *side += (white + (shaped - white) * color.abs()) * vc.noise * 0.25;
            }
        }
        if vc.ring > 0.0 {
            let m = 1.0 - vc.ring + vc.ring * osc;
            x *= m;
            y *= m;
        }
        if v.filter != FilterMode::Off {
            x = vc.filter.run(&vc.coefs, 0, x, comb_l);
            y = vc.filter.run(&vc.coefs, 1, y, comb_r);
        }
        let g = e * vc.level * (1.0 + vc.punch);
        vc.punch *= cx.punch;
        l[i] += x * g * vc.out[0];
        r[i] += y * g * vc.out[1];
        vc.out[0] += vc.out_step[0];
        vc.out[1] += vc.out_step[1];
        if vc.shift != vc.target {
            vc.shift += (vc.target - vc.shift) * vc.glide;
            if (vc.target - vc.shift).abs() < 1e-4 {
                vc.shift = vc.target;
            }
        }
        if vc.fade > 0.0 {
            vc.level = (vc.level - vc.fade).max(0.0);
        }
    }
}

/// Repitch: the sample read at the voice's place, which moves on by its step (times `fm`),
/// round its loop while it has one; None once it's played out.
fn read_repitch(vc: &mut Voice, z: &Loaded, v: &SamplerVoice, fm: f64) -> Option<[f32; 2]> {
    let p = &z.p;
    let (a, b) = vc.loops;
    if p.looping != LoopMode::Off && !vc.once && b - a > 2.0 {
        match p.looping {
            LoopMode::PingPong => {
                if vc.step > 0.0 && vc.pos >= b - 1.0 {
                    vc.dir = -vc.dir;
                    vc.step = -vc.step;
                    vc.pos = (2.0 * (b - 1.0) - vc.pos).max(a);
                } else if vc.step < 0.0 && vc.pos <= a {
                    vc.dir = -vc.dir;
                    vc.step = -vc.step;
                    vc.pos = (2.0 * a - vc.pos).min(b - 1.0);
                }
            }
            _ => {
                if vc.step > 0.0 && vc.pos >= b - 1.0 {
                    vc.pos -= b - 1.0 - a;
                } else if vc.step < 0.0 && vc.pos <= a {
                    vc.pos += b - 1.0 - a;
                }
            }
        }
    }
    let (lo, hi) = vc.bounds;
    if vc.pos < lo || vc.pos >= hi - 1.0 {
        return None;
    }
    let step = vc.step * fm;
    let vintage = v.character == SampleCharacter::Vintage;
    let level = z.mips.level(step);
    let read = |pos: f64| match vintage {
        true => [z.mips.nearest(0, pos), z.mips.nearest(1, pos)],
        false => [z.mips.read(level, 0, pos), z.mips.read(level, 1, pos)],
    };
    let mut s = read(vc.pos);
    // Nearing a forward loop's end, what comes before its start fades in: at the end, it's
    // all that, and the jump back to the start is seamless.
    if p.looping == LoopMode::Forward && !vc.once && vc.fade_len > 1.0 && vc.step > 0.0 {
        let from = b - 1.0 - vc.fade_len;
        if vc.pos > from {
            let w = (((vc.pos - from) / vc.fade_len) as f32).clamp(0.0, 1.0);
            let other = read(vc.pos - (b - 1.0 - a));
            let (keep, take) = (
                (w * std::f32::consts::FRAC_PI_2).cos(),
                (w * std::f32::consts::FRAC_PI_2).sin(),
            );
            s = [s[0] * keep + other[0] * take, s[1] * keep + other[1] * take];
        }
    }
    vc.pos += step;
    Some(s)
}

/// Stretch and Grains: the grains sounding (new ones started as they're due) at their places;
/// the place they're read from moving on at the voice's speed, or held (freeze). None once
/// it's played out and its grains are done.
fn read_grains(vc: &mut Voice, z: &Loaded, v: &SamplerVoice, fm: f64) -> Option<[f32; 2]> {
    let (lo, hi) = vc.bounds;
    let (a, b) = vc.loops;
    let looping = z.p.looping != LoopMode::Off && !vc.once && b - a > 2.0;
    if v.freeze {
        vc.head = lo + vc.position as f64 * (hi - lo);
    } else {
        vc.head += vc.speed;
    }
    if looping {
        let n = b - a;
        if vc.head >= b {
            vc.head = a + (vc.head - a).rem_euclid(n);
        } else if vc.head < a && vc.speed < 0.0 {
            vc.head = b - (a - vc.head).rem_euclid(n);
        }
    }
    let inside = vc.head >= lo && vc.head < hi;
    if !inside && vc.grains.iter().all(|g| !g.on) {
        return None;
    }
    vc.wait -= 1.0;
    if vc.wait <= 0.0 && inside {
        let mut next = || {
            vc.seed = vc.seed.wrapping_add(1);
            random(vc.seed)
        };
        let len = vc.grain_len.max(8.0);
        let mut step = vc.step;
        let (pos, gain, wait) = match v.mode {
            // Stretch: each grain from the place itself; four overlap, so each is half as loud.
            SampleMode::Stretch => (vc.head, [0.5, 0.5], vc.interval),
            _ => {
                let wait = vc.interval * (1.0 + 0.5 * next());
                let pos = (vc.head + (vc.spray * next()) as f64).clamp(lo, (hi - 1.0).max(lo));
                step *= 2f64.powf((v.grain_pitch * next() / 1200.0) as f64);
                // Some grains backwards: from their end to their start, over the same sound.
                let backwards = next() * 0.5 + 0.5 < v.grain_reverse;
                let pos = match backwards {
                    true => {
                        step = -step;
                        pos + len as f64 * step.abs()
                    }
                    false => pos,
                };
                let pan = v.grain_spread.clamp(0.0, 1.0) * next();
                // As many overlapping as there are, they come down to keep the level.
                let g = 1.0 / (len / vc.interval.max(1.0)).max(1.0).sqrt();
                let gain = [g * (1.0 - pan).min(1.0), g * (1.0 + pan).min(1.0)];
                (pos, gain, wait)
            }
        };
        vc.wait += wait.max(1.0);
        if let Some(slot) = vc.grains.iter_mut().find(|g| !g.on) {
            *slot = Grain {
                on: true,
                pos,
                step,
                age: 0,
                len: len as u32,
                gain,
            };
        }
    }
    let mut s = [0f32; 2];
    for g in vc.grains.iter_mut().filter(|g| g.on) {
        let w = window(g.age, g.len, v.grain_shape);
        let step = g.step * fm;
        let level = z.mips.level(step);
        for (c, out) in s.iter_mut().enumerate() {
            *out += z.mips.read(level, c, g.pos) * w * g.gain[c];
        }
        g.pos += step;
        g.age += 1;
        if g.age >= g.len {
            g.on = false;
        }
    }
    Some(s)
}

/// What reading does to the sound: tape's saturation and softened top, or the vintage
/// sampler's held, stepped values (and its output filter).
fn character(vc: &mut Voice, cx: &Ctx, x: &mut f32, y: &mut f32) {
    let v = &cx.v;
    match v.character {
        SampleCharacter::Clean => {}
        SampleCharacter::Tape => {
            let color = v.color.clamp(0.0, 1.0);
            let (d, bias) = (1.0 + 3.0 * color, 0.08 * color);
            // Unity for quiet sounds, rounding off (a little lopsided) as it gets loud.
            let k = d * (1.0 - (d * bias).tanh().powi(2));
            for (c, s) in [x, y].into_iter().enumerate() {
                let sat = ((d * (*s + bias)).tanh() - (d * bias).tanh()) / k;
                vc.soft[c][0] += (sat - vc.soft[c][0]) * cx.tape;
                *s = vc.soft[c][0];
            }
        }
        SampleCharacter::Vintage => {
            vc.clock += cx.crush;
            if vc.clock >= 1.0 {
                vc.clock -= vc.clock.floor();
                vc.hold = [
                    (*x * cx.steps).round() / cx.steps,
                    (*y * cx.steps).round() / cx.steps,
                ];
            }
            for (c, s) in [x, y].into_iter().enumerate() {
                *s = vc.hold[c];
                if v.crush_filter {
                    let f = &mut vc.soft[c];
                    f[0] += (*s - f[0]) * cx.crush_soft;
                    f[1] += (f[0] - f[1]) * cx.crush_soft;
                    *s = f[1];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
