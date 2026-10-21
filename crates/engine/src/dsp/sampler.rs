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
mod tests {
    use super::*;
    use crate::audio::engine::{AudioData, MAX_BLOCK};

    fn sampler(zones: Vec<(Zone, Vec<f32>)>) -> Sampler {
        with_voice(zones, SamplerVoice::default())
    }

    fn with_voice(zones: Vec<(Zone, Vec<f32>)>, voice: SamplerVoice) -> Sampler {
        let zones = zones
            .into_iter()
            .map(|(zone, samples)| Loaded {
                p: Params::of(&zone),
                data: Arc::new(AudioData::mono(samples)),
            })
            .collect();
        Sampler::with(zones, voice, 48_000.0)
    }

    fn note(offset: u32, pitch: u8, velocity: f32) -> Note {
        Note {
            offset,
            pitch,
            velocity,
        }
    }

    fn run(s: &mut Sampler, notes: &[Note]) -> Vec<f32> {
        run_with(s, notes, &[])
    }

    fn run_with(s: &mut Sampler, notes: &[Note], controls: &[Control]) -> Vec<f32> {
        let (mut l, mut r) = (vec![0f32; MAX_BLOCK], vec![0f32; MAX_BLOCK]);
        s.process([&mut l, &mut r], MAX_BLOCK, notes, controls);
        l
    }

    /// The slope of a ramp read back: how fast it's being read.
    fn slope(out: &[f32], i: usize) -> f32 {
        (out[i + 1] - out[i]) * 48_000.0
    }

    fn peak(o: &[f32], a: usize, b: usize) -> f32 {
        o[a..b].iter().fold(0f32, |m, x| m.max(x.abs()))
    }

    #[test]
    fn a_note_glides_to_its_tuning() {
        // A long ramp read at the root's speed, then tuned up an octave: twice as fast, once the
        // glide (a few milliseconds) settles.
        let ramp: Vec<f32> = (0..240_000).map(|i| i as f32 / 48_000.0).collect();
        let sub = Zone {
            note: 24,
            high: Some(48),
            root: Some(36),
            gate: true,
            ..Zone::default()
        };
        let mut s = sampler(vec![(sub, ramp)]);
        run(&mut s, &[note(0, 36, 1.0)]);
        let up = Control {
            offset: 0,
            kind: ControlKind::Expression {
                pitch: 36,
                kind: ExpressionKind::Tuning,
                value: 12.0,
            },
        };
        let first = run_with(&mut s, &[], &[up]);
        // Going faster within the block, and not yet twice as fast.
        assert!(
            slope(&first, 40) > 1.0 && slope(&first, 40) < 1.6,
            "{}",
            slope(&first, 40)
        );
        let settle = |s: &mut Sampler| (0..8).map(|_| run_with(s, &[], &[])).last().unwrap();
        let later = settle(&mut s);
        assert!(
            (slope(&later, 100) - 2.0).abs() < 0.01,
            "{}",
            slope(&later, 100)
        );
        // Pitch bend all the way down: two semitones under that.
        let down = Control {
            offset: 0,
            kind: ControlKind::PitchBend(-1.0),
        };
        run_with(&mut s, &[], &[down]);
        let bent = settle(&mut s);
        let want = 2.0 * 2f32.powf(-2.0 / 12.0);
        assert!(
            (slope(&bent, 100) - want).abs() < 0.01,
            "{}",
            slope(&bent, 100)
        );
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
        assert!(out[0] > 0.0 && out[60] == 0.0, "{} {}", out[0], out[60]);
        // Tuned up an octave, its root reads twice as fast too.
        s.zones[0].p.tune = 12.0;
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

    #[test]
    fn a_slice_plays_its_part_forwards_or_backwards() {
        let ramp: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        let slice = Zone {
            note: 36,
            start: 0.5,
            end: Some(0.6),
            ..Zone::default()
        };
        let mut s = sampler(vec![(slice.clone(), ramp.clone())]);
        let out = run(&mut s, &[note(0, 36, 1.0)]);
        assert!(
            (out[0] - 0.5).abs() < 1e-3 && (out[50] - 0.55).abs() < 1e-3,
            "{} {}",
            out[0],
            out[50]
        );
        assert_eq!(out[120], 0.0); // past the slice's end
        let back = Zone {
            reverse: true,
            ..slice
        };
        let mut s = sampler(vec![(back, ramp)]);
        let out = run(&mut s, &[note(0, 36, 1.0)]);
        assert!(
            out[0] > 0.59 && out[50] < out[0] && out[50] > 0.5,
            "{} {}",
            out[0],
            out[50]
        );
    }

    #[test]
    fn a_loop_holds_while_the_note_does_and_releases_after() {
        let ramp: Vec<f32> = (0..400).map(|i| 0.1 + i as f32 / 1000.0).collect();
        let pad = Zone {
            note: 60,
            loop_mode: LoopMode::Forward,
            loop_start: Some(0.5),
            ..Zone::default()
        };
        let voice = SamplerVoice {
            release_ms: 2.0,
            ..SamplerVoice::default()
        };
        let mut s = with_voice(vec![(pad, ramp)], voice);
        // Well past the 400-sample sample, still sounding, in its loop's half.
        let out = run(&mut s, &[note(0, 60, 1.0)]);
        assert!(out[MAX_BLOCK - 1] > 0.29, "{}", out[MAX_BLOCK - 1]);
        let out = run(&mut s, &[note(0, 60, 0.0)]);
        assert!(out[0] > 0.0 && out[300] == 0.0, "{} {}", out[0], out[300]);
    }

    #[test]
    fn layers_by_velocity_and_round_robins_take_turns() {
        let flat = SamplerVoice {
            velocity: 0.0,
            ..SamplerVoice::default()
        };
        let soft = Zone {
            note: 38,
            velocities: Some((1, 63)),
            ..Zone::default()
        };
        let hard = Zone {
            note: 38,
            velocities: Some((64, 127)),
            ..Zone::default()
        };
        let mut s = with_voice(vec![(soft, vec![0.2; 100]), (hard, vec![0.8; 100])], flat);
        assert!((run(&mut s, &[note(0, 38, 0.3)])[10] - 0.2).abs() < 1e-6);
        assert!((run(&mut s, &[note(0, 38, 0.9)])[10] - 0.8).abs() < 1e-6);
        // Two snares on the same note and velocities: one, then the other.
        let a = Zone {
            note: 38,
            ..Zone::default()
        };
        let mut s = with_voice(vec![(a.clone(), vec![0.3; 100]), (a, vec![0.6; 100])], flat);
        let first = run(&mut s, &[note(0, 38, 1.0)])[10];
        let second = run(&mut s, &[note(0, 38, 1.0)])[10];
        assert!(
            (first - 0.3).abs() < 1e-6 && (second - 0.6).abs() < 1e-6,
            "{first} {second}"
        );
    }

    #[test]
    fn the_envelope_rises_and_the_filter_darkens() {
        // A tone near 6.9 kHz.
        let tone: Vec<f32> = (0..48_000).map(|i| ((i as f32) * 0.9).sin()).collect();
        let zone = Zone {
            note: 60,
            ..Zone::default()
        };
        let slow = SamplerVoice {
            attack_ms: 10.0,
            ..SamplerVoice::default()
        };
        let mut s = with_voice(vec![(zone.clone(), tone.clone())], slow);
        let out = run(&mut s, &[note(0, 60, 1.0)]);
        assert!(
            peak(&out, 0, 40) < 0.2 && peak(&out, 400, 512) > 0.5,
            "{} {}",
            peak(&out, 0, 40),
            peak(&out, 400, 512)
        );
        let dark = SamplerVoice {
            filter: FilterMode::LowPass,
            cutoff_hz: 500.0,
            resonance: 0.0,
            ..SamplerVoice::default()
        };
        let mut s = with_voice(vec![(zone, tone)], dark);
        let out = run(&mut s, &[note(0, 60, 1.0)]);
        assert!(peak(&out, 200, 512) < 0.1, "{}", peak(&out, 200, 512));
    }

    #[test]
    fn mono_legato_glides_without_starting_over() {
        let ramp: Vec<f32> = (0..96_000).map(|i| i as f32 / 48_000.0).collect();
        let bass = Zone {
            note: 24,
            high: Some(72),
            root: Some(36),
            gate: true,
            ..Zone::default()
        };
        let voice = SamplerVoice {
            mono: true,
            legato: true,
            glide_ms: 1.0,
            ..SamplerVoice::default()
        };
        let mut s = with_voice(vec![(bass, ramp)], voice);
        run(&mut s, &[note(0, 36, 1.0)]);
        let before = run(&mut s, &[]);
        // An octave up over it: the same voice, not back at the sample's start, reading twice
        // as fast once it's glided.
        let out = run(&mut s, &[note(0, 48, 1.0)]);
        assert!(
            out[0] > before[MAX_BLOCK - 1] - 0.01,
            "{} {}",
            out[0],
            before[MAX_BLOCK - 1]
        );
        let later = (0..4).map(|_| run(&mut s, &[])).last().unwrap();
        assert!(
            (slope(&later, 100) - 2.0).abs() < 0.02,
            "{}",
            slope(&later, 100)
        );
        assert_eq!(s.voices.iter().filter(|v| v.on).count(), 1);
    }

    #[test]
    fn a_loop_is_sliced_where_its_hits_start() {
        // Four clicks a quarter of a second apart, each a decaying burst.
        let rate = 48_000.0;
        let mut x = vec![0f32; 48_000];
        for k in 0..4 {
            for i in 0..2000 {
                let n = k * 12_000 + i;
                x[n] = (i as f32 * 0.7).sin() * (-(i as f32) / 300.0).exp();
            }
        }
        let found = onsets(&x, rate);
        let cut = cuts(&found, 4);
        assert_eq!(cut.len(), 4, "{found:?}");
        for (k, c) in cut.iter().enumerate() {
            assert!((c - k as f32 * 0.25).abs() < 0.01, "{cut:?}");
        }
        let zones = slices(Path::new("loop.wav"), &cut, 36);
        assert_eq!(
            zones.iter().map(|z| z.note).collect::<Vec<_>>(),
            [36, 37, 38, 39]
        );
        assert_eq!(zones[1].end, Some(cut[2]));
        assert_eq!(zones[3].end, None);
    }

    #[test]
    fn changed_settings_are_followed_without_a_reload() {
        let zone = Zone {
            note: 36,
            ..Zone::default()
        };
        let mut s = sampler(vec![(zone.clone(), vec![0.5; 1000])]);
        let louder = project::Sampler {
            zones: vec![Zone {
                gain_db: 6.0,
                ..zone
            }],
            voice: SamplerVoice::default(),
        };
        s.follow(&louder, 1);
        let out = run(&mut s, &[note(0, 36, 1.0)]);
        assert!(
            (out[10] - 0.5 * 10f32.powf(6.0 / 20.0)).abs() < 1e-4,
            "{}",
            out[10]
        );
    }
}
