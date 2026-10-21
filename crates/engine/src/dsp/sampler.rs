//! The built-in sampler: samples on notes. A drum kit puts one on each note, played out to its end
//! however short the note; one sample can also play across a range of notes, repitched from its
//! root, as an 808 or a bass, and stop (gated) when the note does. A zone plays a slice of its
//! sample (from `start` to `end`), forwards or backwards, looping while held (forward or
//! ping-pong); zones split by velocity are layers, and zones sharing notes and velocities take
//! turns (round robins). Sounds in a choke group cut each other off, as a closed hat cuts off an
//! open one.
//!
//! Every voice has an amp envelope (ADSR), a filter (low-, high- or band-pass, Andy Simper's
//! state-variable filter) with its own envelope and key tracking, and an LFO for vibrato, the
//! cutoff and tremolo. Mono mode plays one note at a time, legato gliding between them. Pitch
//! bend (two semitones either way) and each note's tuning expression move what's playing,
//! smoothly, so an 808 can glide. Its settings change as it plays (`follow`) without reloading
//! its samples or cutting what's sounding.

use std::{path::Path, sync::Arc};

use crate::{
    audio::engine::{AudioData, load_wav},
    plugin::{Control, ControlKind, ExpressionKind, Note},
    song::project::{self, FilterMode, LoopMode, SamplerVoice, Zone},
};

const VOICES: usize = 64;
/// How fast a choked (or stolen) sound fades: quick, but not a click.
const CHOKE_MS: f32 = 4.0;
/// How far pitch bend goes, in semitones.
const BEND_RANGE: f32 = 2.0;
/// How quickly a voice's pitch follows a bend or tuning move: the time constant, in ms.
const GLIDE_MS: f32 = 8.0;
/// Pitch and filter are worked out again every this many samples.
const EVERY: usize = 16;

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
    reverse: bool,
    velocities: (u8, u8),
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
            reverse: z.reverse,
            velocities: z.velocities.unwrap_or((1, 127)),
        }
    }

    fn takes(&self, note: u8, velocity: u8) -> bool {
        (self.note..=self.high).contains(&note)
            && (self.velocities.0..=self.velocities.1).contains(&velocity.max(1))
    }
}

struct Loaded {
    p: Params,
    data: Arc<AudioData>,
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    #[default]
    Done,
}

/// An envelope as it goes.
#[derive(Clone, Copy, Default)]
struct Env {
    stage: Stage,
    level: f32,
}

/// An envelope's shape at a sample rate: the attack's rise a sample, the decay's and release's
/// fall (as a factor a sample toward their goal), and the level held.
#[derive(Clone, Copy)]
struct Shape {
    rise: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

/// The factor a sample that falls 60 dB over `ms`.
fn fall(ms: f32, rate: f32) -> f32 {
    if ms <= 0.05 {
        0.0
    } else {
        (-6.9 / (ms * 0.001 * rate)).exp()
    }
}

impl Shape {
    fn new(attack_ms: f32, decay_ms: f32, sustain: f32, release_ms: f32, rate: f32) -> Shape {
        Shape {
            rise: if attack_ms <= 0.05 {
                1.0
            } else {
                1.0 / (attack_ms * 0.001 * rate)
            },
            decay: fall(decay_ms, rate),
            sustain: sustain.clamp(0.0, 1.0),
            release: fall(release_ms, rate),
        }
    }
}

impl Env {
    fn start(&mut self) {
        self.stage = Stage::Attack;
    }

    fn release(&mut self) {
        if self.stage != Stage::Done {
            self.stage = Stage::Release;
        }
    }

    fn next(&mut self, s: &Shape, release: f32) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.level += s.rise;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level = s.sustain + (self.level - s.sustain) * s.decay;
                if (self.level - s.sustain).abs() < 1e-4 {
                    self.level = s.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => self.level = s.sustain,
            Stage::Release => {
                self.level *= release;
                if self.level < 1e-3 {
                    self.level = 0.0;
                    self.stage = Stage::Done;
                }
            }
            Stage::Done => self.level = 0.0,
        }
        self.level
    }
}

#[derive(Clone, Copy, Default)]
struct Voice {
    on: bool,
    zone: usize,
    note: u8,
    /// Where in the sample (frames), which way it's reading (1 or -1), and the frames it plays
    /// between and loops between.
    pos: f64,
    dir: f64,
    bounds: (f64, f64),
    loops: (f64, f64),
    /// How far it moves a sample at its pitch, worked out every few samples.
    step: f64,
    /// Semitones off its note: bends, tuning and glides, gliding to `target` at `glide`.
    shift: f32,
    target: f32,
    glide: f32,
    velocity: f32,
    amp: Env,
    fenv: Env,
    /// Its zone's own release (a factor a sample), if it has one, over the instrument's.
    release: Option<f32>,
    /// 1 until choked or stolen; then falling by `fade` a sample.
    level: f32,
    fade: f32,
    held: bool,
    /// When it started, to take the oldest voice when all are busy.
    age: u64,
    /// The filter's state (each channel's two integrators) and coefficients.
    ic: [[f32; 2]; 2],
    coef: [f32; 4],
}

pub struct Sampler {
    rate: f32,
    zones: Vec<Loaded>,
    voice: SamplerVoice,
    amp: Shape,
    filter: Shape,
    voices: [Voice; VOICES],
    started: u64,
    /// Pitch bend, in semitones; and each note's tuning (semitones) and volume (gain), by pitch.
    bend: f32,
    tuning: [f32; 128],
    gain: [f32; 128],
    lfo: f32,
    /// Each note's next round robin.
    turns: [u16; 128],
    /// Mono: the notes held, the latest last.
    held: [u8; 16],
    holding: usize,
    /// The settings followed last (by address).
    following: usize,
    /// Its voice is moved by modulators.
    modulated: bool,
    tick: usize,
}

impl Sampler {
    /// Loads the zones' samples from `dir` at `rate`. A sample that can't be read is an error.
    pub fn load(s: &project::Sampler, dir: &Path, rate: f64) -> Result<Sampler, String> {
        let zones = s
            .zones
            .iter()
            .map(|z| {
                Ok(Loaded {
                    p: Params::of(z),
                    data: Arc::new(load_wav(&dir.join(&z.file), rate)?),
                })
            })
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
            voices: [Voice::default(); VOICES],
            started: 0,
            bend: 0.0,
            tuning: [0.0; 128],
            gain: [1.0; 128],
            lfo: 0.0,
            turns: [0; 128],
            held: [0; 16],
            holding: 0,
            following: 0,
            modulated: false,
            tick: 0,
        };
        s.shape();
        s
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
        let v = self.voice;
        let gain = 10f32.powf(v.gain_db / 20.0);
        let lfo_step = v.lfo_hz.max(0.0) / self.rate;
        let (amp, fshape) = (self.amp, self.filter);
        for i in 0..frames {
            while let Some(c) = moves.next_if(|c| c.offset as usize <= i) {
                self.control(c.kind);
            }
            while let Some(n) = next.next_if(|n| n.offset as usize <= i) {
                self.note(n);
            }
            let wave = (self.lfo * std::f32::consts::TAU).sin();
            self.lfo = (self.lfo + lfo_step).fract();
            let update = self.tick.is_multiple_of(EVERY);
            self.tick = self.tick.wrapping_add(1);
            let tremolo = 1.0 - v.tremolo.clamp(0.0, 1.0) * (0.5 + 0.5 * wave);
            let (mut sl, mut sr) = (0.0, 0.0);
            for voice in self.voices.iter_mut().filter(|x| x.on) {
                let z = &self.zones[voice.zone];
                let p = z.p;
                let channels = &z.data.channels;
                // Moving on: round its loop while it has one, else stopping at the end.
                let (a, b) = voice.loops;
                if p.looping != LoopMode::Off && b - a > 2.0 {
                    match p.looping {
                        LoopMode::PingPong => {
                            if voice.dir > 0.0 && voice.pos >= b - 1.0 {
                                voice.dir = -1.0;
                                voice.step = -voice.step.abs();
                                voice.pos = (2.0 * (b - 1.0) - voice.pos).max(a);
                            } else if voice.dir < 0.0 && voice.pos <= a {
                                voice.dir = 1.0;
                                voice.step = voice.step.abs();
                                voice.pos = (2.0 * a - voice.pos).min(b - 1.0);
                            }
                        }
                        _ => {
                            if voice.dir > 0.0 && voice.pos >= b - 1.0 {
                                voice.pos -= b - 1.0 - a;
                            } else if voice.dir < 0.0 && voice.pos <= a {
                                voice.pos += b - 1.0 - a;
                            }
                        }
                    }
                }
                let (lo, hi) = voice.bounds;
                if voice.pos < lo || voice.pos >= hi - 1.0 || voice.level <= 0.0 {
                    voice.on = false;
                    continue;
                }
                let e = voice.amp.next(&amp, voice.release.unwrap_or(amp.release));
                if voice.amp.stage == Stage::Done {
                    voice.on = false;
                    continue;
                }
                let fe = voice.fenv.next(&fshape, fshape.release);
                if update {
                    // Pitch: the note against the zone's root, its tuning and the instrument's,
                    // what's gliding, and vibrato.
                    let semis = voice.note as f32 - p.root as f32
                        + p.tune
                        + v.transpose
                        + voice.shift
                        + v.vibrato_cents / 100.0 * wave;
                    voice.step = 2f64.powf(semis as f64 / 12.0) * voice.dir;
                    if v.filter != FilterMode::Off {
                        let octaves = v.filter_env * fe
                            + v.lfo_filter * wave
                            + v.key_track * (voice.note as f32 - 60.0) / 12.0;
                        let fc = (v.cutoff_hz * 2f32.powf(octaves)).clamp(20.0, self.rate * 0.45);
                        let g = (std::f32::consts::PI * fc / self.rate).tan();
                        let k = 2.0 * (1.0 - 0.97 * v.resonance.clamp(0.0, 1.0));
                        let a1 = 1.0 / (1.0 + g * (g + k));
                        voice.coef = [a1, g * a1, g * g * a1, k];
                    }
                }
                let at = voice.pos as usize;
                let t = (voice.pos - at as f64) as f32;
                let read = |c: usize| {
                    let ch = &channels[c.min(channels.len() - 1)];
                    ch[at] + (ch[at + 1] - ch[at]) * t
                };
                let g = voice.velocity
                    * voice.level
                    * e
                    * tremolo
                    * self.gain[voice.note as usize]
                    * gain;
                let (mut x, mut y) = (read(0) * g * p.gain[0], read(1) * g * p.gain[1]);
                if v.filter != FilterMode::Off {
                    let [a1, a2, a3, k] = voice.coef;
                    for (s, ic) in [&mut x, &mut y].into_iter().zip(voice.ic.iter_mut()) {
                        let v3 = *s - ic[1];
                        let v1 = a1 * ic[0] + a2 * v3;
                        let v2 = ic[1] + a2 * ic[0] + a3 * v3;
                        ic[0] = 2.0 * v1 - ic[0];
                        ic[1] = 2.0 * v2 - ic[1];
                        *s = match v.filter {
                            FilterMode::LowPass => v2,
                            FilterMode::HighPass => *s - k * v1 - v2,
                            FilterMode::BandPass => v1,
                            FilterMode::Off => *s,
                        };
                    }
                }
                sl += x;
                sr += y;
                if voice.shift != voice.target {
                    voice.shift += (voice.target - voice.shift) * voice.glide;
                    if (voice.target - voice.shift).abs() < 1e-4 {
                        voice.shift = voice.target;
                    }
                }
                voice.pos += voice.step;
                if voice.fade > 0.0 {
                    voice.level = (voice.level - voice.fade).max(0.0);
                }
            }
            l[i] += sl;
            r[i] += sr;
        }
    }

    /// The share of the way to its target a glide of `ms` goes a sample.
    fn coef(&self, ms: f32) -> f32 {
        if ms <= 0.05 {
            1.0
        } else {
            1.0 - (-1.0 / (ms * 0.001 * self.rate)).exp()
        }
    }

    /// A bend, or a note's tuning or volume, from here on.
    fn control(&mut self, kind: ControlKind) {
        match kind {
            ControlKind::PitchBend(b) => self.bend = b.clamp(-1.0, 1.0) * BEND_RANGE,
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
        if gate {
            v.amp.release();
            v.fenv.release();
        }
    }

    fn note(&mut self, n: &Note) {
        let pitch = n.pitch & 127;
        if n.velocity <= 0.0 {
            if self.voice.mono {
                self.let_go_mono(pitch);
                return;
            }
            for i in 0..VOICES {
                if self.voices[i].on && self.voices[i].held && self.voices[i].note == pitch {
                    self.release_voice(i);
                }
            }
            return;
        }
        let velocity = (n.velocity.clamp(0.0, 1.0) * 127.0).round() as u8;
        // The zones that take it; of several, the next in turn (round robin).
        let count = self
            .zones
            .iter()
            .filter(|z| z.p.takes(pitch, velocity))
            .count();
        if count == 0 {
            return;
        }
        let turn = self.turns[pitch as usize] as usize % count;
        self.turns[pitch as usize] = self.turns[pitch as usize].wrapping_add(1);
        let Some(index) = self
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.p.takes(pitch, velocity))
            .nth(turn)
            .map(|(i, _)| i)
        else {
            return;
        };
        let p = self.zones[index].p;
        // Its choke group falls quiet.
        if let Some(group) = p.choke {
            let fade = 1.0 / (CHOKE_MS * 0.001 * self.rate);
            for v in self.voices.iter_mut().filter(|v| v.on) {
                if self.zones[v.zone].p.choke == Some(group) {
                    v.fade = v.fade.max(fade);
                }
            }
        }
        // Softer notes are quieter, more than in proportion, as a drum is (as much as the
        // instrument's velocity says).
        let gain = n
            .velocity
            .clamp(0.0, 1.0)
            .powf(1.5 * self.voice.velocity.clamp(0.0, 1.0));
        // Mono: one voice. Legato over a held note glides it to the new one without starting
        // over; otherwise what's sounding fades as the new note starts, gliding from the last.
        let mut from = None;
        if self.voice.mono {
            let sounding = (0..VOICES)
                .find(|&i| self.voices[i].on && self.voices[i].amp.stage != Stage::Release);
            let was = self.top();
            self.hold(pitch);
            if let Some(i) = sounding {
                if self.voice.legato && self.voices[i].held && self.voices[i].zone == index {
                    self.glide_to(i, pitch);
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
        // A free voice, else the oldest.
        let v = match self.voices.iter().position(|v| !v.on) {
            Some(i) => i,
            None => (0..VOICES).min_by_key(|&i| self.voices[i].age).unwrap(),
        };
        self.started += 1;
        // A new note starts where the bend is, but untuned: its own tuning starts with it.
        self.tuning[pitch as usize] = 0.0;
        self.gain[pitch as usize] = 1.0;
        let len = self.zones[index].data.channels[0].len() as f64;
        let bounds = (
            (p.start as f64 * len).floor(),
            (p.end as f64 * len).ceil().min(len),
        );
        let loops = (
            (p.loop_start as f64 * len).floor(),
            (p.loop_end as f64 * len).ceil().min(len),
        );
        let (pos, dir) = match p.reverse {
            true => ((bounds.1 - 2.0).max(bounds.0), -1.0),
            false => (bounds.0, 1.0),
        };
        let glide_ms = match self.voice.glide_ms > 0.0 {
            true => self.voice.glide_ms,
            false => GLIDE_MS,
        };
        // Gliding in from the last note played, mono with glide on.
        let shift = match from {
            Some(f) if self.voice.glide_ms > 0.0 => self.bend + (f - pitch as f32),
            _ => self.bend,
        };
        let mut amp = Env::default();
        amp.start();
        let mut fenv = Env::default();
        fenv.start();
        self.voices[v] = Voice {
            on: true,
            zone: index,
            note: pitch,
            pos,
            dir,
            bounds,
            loops,
            step: dir,
            shift,
            target: self.bend,
            glide: self.coef(glide_ms),
            velocity: gain,
            amp,
            fenv,
            release: p.release_ms.map(|ms| fall(ms.max(0.1), self.rate)),
            level: 1.0,
            fade: 0.0,
            held: true,
            age: self.started,
            ic: [[0.0; 2]; 2],
            coef: [1.0, 0.0, 0.0, 2.0],
        };
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
        let Some(i) = (0..VOICES)
            .find(|&i| self.voices[i].on && self.voices[i].held && self.voices[i].note == pitch)
        else {
            return;
        };
        match self.top() {
            Some(under) if self.zones[self.voices[i].zone].p.takes(under, 100) => {
                self.glide_to(i, under)
            }
            _ => self.release_voice(i),
        }
    }
}

/// Where a sound's hits start, as shares of its length, each with how hard it starts: rises in
/// the energy of its edges (the difference between samples, so a hat's tick counts as much as a
/// kick's thud), each the biggest within 60 ms, over the sound's typical rise.
pub fn onsets(mono: &[f32], rate: f32) -> Vec<(f32, f32)> {
    const SIZE: usize = 1024;
    const HOP: usize = 256;
    let n = mono.len();
    if n < SIZE * 2 {
        return vec![(0.0, 1.0)];
    }
    let energy: Vec<f32> = (0..(n - SIZE) / HOP)
        .map(|f| {
            let a = f * HOP;
            let e: f32 = mono[a + 1..a + SIZE]
                .iter()
                .zip(&mono[a..a + SIZE - 1])
                .map(|(x, y)| (x - y) * (x - y))
                .sum();
            (e + 1e-9).ln()
        })
        .collect();
    let flux: Vec<f32> = std::iter::once(0.0)
        .chain(energy.windows(2).map(|w| (w[1] - w[0]).max(0.0)))
        .collect();
    let mean = flux.iter().sum::<f32>() / flux.len().max(1) as f32;
    let sd =
        (flux.iter().map(|f| (f - mean).powi(2)).sum::<f32>() / flux.len().max(1) as f32).sqrt();
    let gap = ((0.06 * rate) as usize / HOP).max(1);
    let mut out: Vec<(f32, f32)> = vec![(0.0, flux.iter().copied().fold(0.0, f32::max).max(1.0))];
    for i in 1..flux.len() {
        let (a, b) = (i.saturating_sub(gap), (i + gap + 1).min(flux.len()));
        if flux[i] > mean + sd && flux[a..b].iter().all(|&v| v <= flux[i]) {
            // The new energy came in at the frame's end; the hit starts where the sound first
            // rises toward its peak there (a few samples before, to keep its attack whole).
            let centre = i * HOP + SIZE - HOP;
            let (a, b) = (centre.saturating_sub(HOP), (centre + HOP).min(n));
            let peak = mono[a..b].iter().fold(0f32, |m, x| m.max(x.abs()));
            let rise = mono[a..b]
                .iter()
                .position(|x| x.abs() > 0.2 * peak)
                .unwrap_or(0);
            let at = (a + rise).saturating_sub(16) as f32 / n as f32;
            if at - out.last().map_or(-1.0, |o| o.0) > 0.06 * rate / n as f32 {
                out.push((at, flux[i]));
            }
        }
    }
    out
}

/// The strongest `most` of a sound's onsets, in order: where to cut it into slices (its start
/// always one).
pub fn cuts(onsets: &[(f32, f32)], most: usize) -> Vec<f32> {
    let mut by_strength: Vec<(f32, f32)> = onsets.iter().copied().skip(1).collect();
    by_strength.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut cuts: Vec<f32> = std::iter::once(0.0)
        .chain(
            by_strength
                .into_iter()
                .take(most.saturating_sub(1))
                .map(|o| o.0),
        )
        .collect();
    cuts.sort_by(f32::total_cmp);
    cuts
}

/// A sound cut into one-shot zones at `cuts` (shares of its length), on notes upward from
/// `first`, as a slicer does.
pub fn slices(file: &Path, cuts: &[f32], first: u8) -> Vec<Zone> {
    cuts.iter()
        .enumerate()
        .filter(|(k, _)| first as usize + k <= 127)
        .map(|(k, &start)| Zone {
            file: file.to_path_buf(),
            name: format!("Slice {}", k + 1),
            note: first + k as u8,
            start,
            end: cuts.get(k + 1).copied().filter(|&e| e < 1.0),
            ..Zone::default()
        })
        .collect()
}

#[cfg(test)]
mod tests;
