//! A modulator card's settings: read, turned, automated, and chosen from its menus.

use super::*;

pub type StepsAt =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<(usize, usize), Bounds<Pixels>>>>;

/// What a card's knob or menu changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModField {
    Phase,
    Glide,
    Attack,
    Decay,
    Sustain,
    Release,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModChange {
    Shape(LfoShape),
    Rate(ModRate),
    Unipolar(bool),
    Count(usize),
    Source(u64),
    Smooth(bool),
    /// The steps' values at random.
    Scramble,
}

/// A time in ms as a knob's position: 1 ms to 5 s, in proportion.
pub(crate) fn ms_pos(ms: f32) -> f32 {
    (ms.max(1.0).ln() / 5000f32.ln()).clamp(0.0, 1.0)
}

pub(crate) fn ms_of(pos: f32) -> f32 {
    5000f32.powf(pos.clamp(0.0, 1.0)).round()
}

pub(crate) fn field_of(p: SamplerParam) -> Field {
    match p {
        SamplerParam::Cutoff => Field::Cutoff,
        SamplerParam::Resonance => Field::Resonance,
        SamplerParam::FilterEnv => Field::FilterEnv,
        SamplerParam::Transpose => Field::Transpose,
        SamplerParam::Volume => Field::Volume,
        SamplerParam::Vibrato => Field::Vibrato,
        SamplerParam::LfoFilter => Field::LfoFilter,
        SamplerParam::Tremolo => Field::Tremolo,
    }
}

pub(crate) fn param_of(f: Field) -> Option<SamplerParam> {
    SamplerParam::ALL.into_iter().find(|&p| field_of(p) == f)
}

/// The setting a card's knob turns, as automation names it.
pub fn setting_of(f: ModField) -> Option<ModParam> {
    Some(match f {
        ModField::Phase => ModParam::Phase,
        ModField::Glide => ModParam::Glide,
        ModField::Attack => ModParam::Attack,
        ModField::Decay => ModParam::Decay,
        ModField::Sustain => ModParam::Sustain,
        ModField::Release => ModParam::Release,
        ModField::Value => ModParam::Value,
    })
}

impl Workspace {
    /// A card's knob turned (0 to 1).
    pub fn set_mod_field(&mut self, t: usize, k: usize, field: ModField, v: f32) {
        let Some(kind) = self.session.project.mod_kind_mut(t, k) else {
            return;
        };
        let v = v.clamp(0.0, 1.0);
        match (kind, field) {
            (ModKind::Lfo { phase, .. }, ModField::Phase) => *phase = (v * 100.0).round() / 100.0,
            (ModKind::Steps { glide, .. }, ModField::Glide) => *glide = (v * 100.0).round() / 100.0,
            (ModKind::Envelope { attack_ms, .. }, ModField::Attack)
            | (ModKind::Follower { attack_ms, .. }, ModField::Attack) => *attack_ms = ms_of(v),
            (ModKind::Envelope { decay_ms, .. }, ModField::Decay) => *decay_ms = ms_of(v),
            (ModKind::Envelope { sustain, .. }, ModField::Sustain) => {
                *sustain = (v * 100.0).round() / 100.0
            }
            (ModKind::Envelope { release_ms, .. }, ModField::Release)
            | (ModKind::Follower { release_ms, .. }, ModField::Release) => *release_ms = ms_of(v),
            (ModKind::Macro { value }, ModField::Value) => *value = (v * 100.0).round() / 100.0,
            _ => {}
        }
    }

    pub fn mod_field(&self, t: usize, k: usize, field: ModField) -> f32 {
        let Some(kind) = self.session.project.mod_kind(t, k) else {
            return 0.0;
        };
        // Where its automation has it now, while that plays it.
        if let Some(s) = setting_of(field)
            && let Some(v) = self.mod_automated(t, k, s)
        {
            return match s {
                ModParam::Attack | ModParam::Decay | ModParam::Release => ms_pos(v),
                _ => v,
            };
        }
        match (kind, field) {
            (ModKind::Lfo { phase, .. }, ModField::Phase) => *phase,
            (ModKind::Steps { glide, .. }, ModField::Glide) => *glide,
            (ModKind::Envelope { attack_ms, .. }, ModField::Attack)
            | (ModKind::Follower { attack_ms, .. }, ModField::Attack) => ms_pos(*attack_ms),
            (ModKind::Envelope { decay_ms, .. }, ModField::Decay) => ms_pos(*decay_ms),
            (ModKind::Envelope { sustain, .. }, ModField::Sustain) => *sustain,
            (ModKind::Envelope { release_ms, .. }, ModField::Release)
            | (ModKind::Follower { release_ms, .. }, ModField::Release) => ms_pos(*release_ms),
            (ModKind::Macro { value }, ModField::Value) => *value,
            _ => 0.0,
        }
    }

    /// A setting's automation where it plays now; for one the stack shares, the first track in
    /// it with a lane for it (as the engine plays it).
    pub(crate) fn mod_automated(&self, t: usize, k: usize, s: ModParam) -> Option<f32> {
        let p = &self.session.project;
        let own = self.automated(t, Param::Mod(k as u8, s));
        let (Some(ModKind::Shared { modulator }), Some(bus)) = (
            p.tracks.get(t)?.modulators.get(k).map(|m| &m.kind),
            p.tracks[t].output,
        ) else {
            return own;
        };
        p.tracks
            .iter()
            .enumerate()
            .filter(|(_, tr)| tr.output == Some(bus))
            .flat_map(|(i, tr)| {
                let links = tr.modulators.iter().enumerate();
                links
                    .filter(|(_, m)| {
                        m.kind
                            == ModKind::Shared {
                                modulator: *modulator,
                            }
                    })
                    .map(move |(j, _)| (i, j))
            })
            .find_map(|(i, j)| self.automated(i, Param::Mod(j as u8, s)))
    }

    /// A card's menu choice.
    pub fn mod_change(&mut self, t: usize, k: usize, change: ModChange, cx: &mut Context<Self>) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(7, |d| d.as_nanos() as u64);
        if t >= self.session.project.tracks.len() {
            return;
        }
        self.edit(cx, |ws| {
            let Some(kind) = ws.session.project.mod_kind_mut(t, k) else {
                return;
            };
            match (kind, change) {
                (ModKind::Lfo { shape, .. }, ModChange::Shape(s)) => *shape = s,
                (ModKind::Lfo { rate, .. }, ModChange::Rate(r))
                | (ModKind::Steps { rate, .. }, ModChange::Rate(r))
                | (ModKind::Random { rate, .. }, ModChange::Rate(r)) => *rate = r,
                (ModKind::Lfo { unipolar, .. }, ModChange::Unipolar(u)) => *unipolar = u,
                (ModKind::Steps { values, .. }, ModChange::Count(n)) => {
                    let had = values.len().max(1);
                    *values = (0..n)
                        .map(|i| values.get(i % had).copied().unwrap_or(0.0))
                        .collect();
                }
                (ModKind::Steps { values, .. }, ModChange::Scramble) => {
                    let mut s = seed | 1;
                    for v in values.iter_mut() {
                        s ^= s << 13;
                        s ^= s >> 7;
                        s ^= s << 17;
                        *v = ((s % 1000) as f32 / 1000.0 * 20.0).round() / 20.0;
                    }
                }
                (ModKind::Follower { source, .. }, ModChange::Source(id)) => *source = id,
                (ModKind::Random { smooth, .. }, ModChange::Smooth(on)) => *smooth = on,
                _ => {}
            }
        });
    }

    /// The steps' value at the pointer, while painting them.
    pub fn paint_mod_step(&mut self, t: usize, k: usize, p: Point<f32>) {
        let Some(b) = self.mod_steps.borrow().get(&(t, k)).copied() else {
            return;
        };
        let (x, y) = (p.x - f32::from(b.origin.x), p.y - f32::from(b.origin.y));
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if let Some(ModKind::Steps { values, .. }) = self.session.project.mod_kind_mut(t, k) {
            let n = values.len();
            if n == 0 || w <= 0.0 {
                return;
            }
            let i = ((x / w * n as f32).floor() as isize).clamp(0, n as isize - 1) as usize;
            values[i] = ((1.0 - y / h).clamp(0.0, 1.0) * 20.0).round() / 20.0;
        }
    }
}
