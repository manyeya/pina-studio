//! Modulation: a track's modulators as cards in the Modulation pane (an LFO, steps,
//! an envelope its notes set off, a follower of another track's level, random, a macro), each
//! moving as it plays. Drag a card's ↗ onto any knob or fader of its track (the header's fader
//! and pan, Smart Controls, the EQ, compressor and saturator, the Sampler's knobs, sends) to map
//! it there; that knob then wears a ring in the modulator's colour, its range, and a dot riding
//! where it is now. Click a card to pick it: dragging a knob it moves then sets how far, not the
//! knob. Each card lists where it goes, each route's depth dragged sideways. The engine side is
//! `pina_engine::dsp::modulate`.

use gpui::{
    Bounds, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent, PathBuilder, Pixels,
    Point, Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    dsp::modulate::{
        self, LfoShape, MAX_MODS, ModKind, ModParam, ModRate, ModRoute, ModTarget, Modulator,
        SamplerParam,
    },
    song::project::{Instrument, Param, Sampler, Which},
};

use crate::{
    editors::sampler::{Field, field_pos},
    mix::strip::{self, Channel},
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{CompField, Drag, SatField, Target, Workspace, pt},
};

/// The knobs a modulator can be dropped on (track, setting), where each was drawn; the steps'
/// editors (track, modulator), where.
pub type DropTargets = std::rc::Rc<std::cell::RefCell<Vec<(usize, ModTarget, Bounds<Pixels>)>>>;
pub type StepsAt =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<(usize, usize), Bounds<Pixels>>>>;

/// Each modulator's colour, by its place on its track.
pub const MOD_COLORS: [u32; MAX_MODS] = [VIOLET, AMBER, TEAL, PINK, SKY, MINT, CORAL, SLATE];

/// A modulator learning its target: the track's plugin parameters as they were when it began
/// (those already moved by a route or automation left out, as they move on their own), the
/// first to change being what it maps to.
pub struct Learn {
    pub track: usize,
    pub index: usize,
    base: Vec<(Which, u32, f64)>,
}

/// How far a parameter must move to count as turned, of its 0 to 1.
const TURNED: f64 = 0.004;

/// How many of a modulator's values a card keeps to draw its trace.
const TRACE: usize = 96;

/// A modulator's range on a knob: its colour, the knob positions it moves between, and where it
/// has it now.
pub struct Ring {
    pub color: u32,
    pub lo: f32,
    pub hi: f32,
    pub live: f32,
}

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

/// Rates to choose from: in time with the song, and free.
const SYNCED: [f32; 13] = [
    16.0,
    8.0,
    4.0,
    2.0,
    1.0,
    0.5,
    0.25,
    0.125,
    1.5,
    0.75,
    2.0 / 3.0,
    1.0 / 3.0,
    1.0 / 6.0,
];
const FREE: [f32; 7] = [0.1, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0];

/// A time in ms as a knob's position: 1 ms to 5 s, in proportion.
fn ms_pos(ms: f32) -> f32 {
    (ms.max(1.0).ln() / 5000f32.ln()).clamp(0.0, 1.0)
}

fn ms_of(pos: f32) -> f32 {
    5000f32.powf(pos.clamp(0.0, 1.0)).round()
}

fn field_of(p: SamplerParam) -> Field {
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

fn param_of(f: Field) -> Option<SamplerParam> {
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

/// The knob's angle (degrees, clockwise from three o'clock) for a position, as `knob` draws it.
fn angle(pos: f32) -> f32 {
    135.0 + 270.0 * pos.clamp(0.0, 1.0)
}

impl Workspace {
    /// The track and setting a knob turns, if modulators can move it.
    pub fn mod_key(&self, target: Target) -> Option<(usize, ModTarget)> {
        let track = |ch: Channel| match ch {
            Channel::Track(t) => Some(t),
            _ => None,
        };
        Some(match target {
            Target::Param { track: id, param } => {
                let t = self
                    .session
                    .project
                    .tracks
                    .iter()
                    .position(|t| t.id == id)?;
                (t, ModTarget::Plugin(Which::Instrument, param))
            }
            Target::LowCut(ch) => (track(ch)?, ModTarget::LowCut),
            // A quick knob moves its band once it has one.
            Target::Quick(ch, q) => {
                let eq = self.strip_dsp(ch).0?;
                (track(ch)?, ModTarget::EqGain(q.find(&eq)? as u8))
            }
            Target::Comp(ch, CompField::Threshold) => (track(ch)?, ModTarget::CompThreshold),
            Target::Comp(ch, CompField::Makeup) => (track(ch)?, ModTarget::CompMakeup),
            Target::Comp(..) => return None,
            Target::Sat(ch, SatField::Drive) => (track(ch)?, ModTarget::SatDrive),
            Target::Sat(ch, SatField::Tone) => (track(ch)?, ModTarget::SatTone),
            Target::Sat(ch, SatField::Mix) => (track(ch)?, ModTarget::SatMix),
            Target::Sat(..) => return None,
            Target::Sampler { track, field } => (track, ModTarget::Sampler(param_of(field)?)),
        })
    }

    /// Where a knob for `mt` on track `t` would sit, moved `by` in its units.
    pub fn mod_pos(&self, t: usize, mt: ModTarget, by: f32) -> f32 {
        let Some(track) = self.session.project.tracks.get(t) else {
            return 0.0;
        };
        let eq = track.eq.unwrap_or_default();
        let comp = track.comp.unwrap_or_default();
        let sat = track.sat.unwrap_or_default();
        match mt {
            ModTarget::Volume => strip::fader_pos(modulate::apply_volume(track.volume_db, by)),
            ModTarget::Pan => (modulate::apply_pan(track.pan, by) + 1.0) / 2.0,
            ModTarget::Send(i) => track.sends.get(i).map_or(0.0, |s| {
                strip::fader_pos(modulate::apply_volume(s.level_db, by))
            }),
            ModTarget::LowCut | ModTarget::EqGain(_) | ModTarget::EqFreq(_) => {
                let mut e = eq;
                modulate::apply_eq(&mut e, mt, by);
                match mt {
                    ModTarget::LowCut => strip::low_cut_pos(crate::mix::eq::low_cut(match by {
                        0.0 => &eq,
                        _ => &e,
                    })),
                    ModTarget::EqGain(i) => {
                        strip::eq_gain_pos(e.bands.get(i as usize).map_or(0.0, |b| b.gain_db))
                    }
                    ModTarget::EqFreq(i) => e.bands.get(i as usize).map_or(0.0, |b| {
                        ((b.freq / 10.0).ln() / 3000f32.ln()).clamp(0.0, 1.0)
                    }),
                    _ => 0.0,
                }
            }
            ModTarget::CompThreshold | ModTarget::CompMakeup => {
                let mut c = comp;
                modulate::apply_comp(&mut c, mt, by);
                let field = match mt {
                    ModTarget::CompThreshold => CompField::Threshold,
                    _ => CompField::Makeup,
                };
                strip::comp_pos(field, &c)
            }
            ModTarget::SatDrive | ModTarget::SatTone | ModTarget::SatMix => {
                let mut s = sat;
                modulate::apply_sat(&mut s, mt, by);
                let field = match mt {
                    ModTarget::SatDrive => SatField::Drive,
                    ModTarget::SatTone => SatField::Tone,
                    _ => SatField::Mix,
                };
                strip::sat_pos(field, &s)
            }
            ModTarget::Sampler(p) => {
                let Some(Instrument::Sampler(s)) = &track.instrument else {
                    return 0.0;
                };
                let mut voice = s.voice;
                modulate::apply_voice(&mut voice, p, by);
                field_pos(
                    &Sampler {
                        voice,
                        ..Default::default()
                    },
                    field_of(p),
                )
            }
            ModTarget::Plugin(which, id) => {
                (self.player.param_value(track.id, which, id) as f32 + by).clamp(0.0, 1.0)
            }
            // Drawn in the step sequencer's lane, not on a knob.
            ModTarget::Sound(..) => 0.0,
        }
    }

    /// The modulators moving `mt` on track `t`, as rings: their ranges and where they are now.
    pub fn mod_rings(&self, t: usize, mt: ModTarget) -> Vec<Ring> {
        let Some(track) = self.session.project.tracks.get(t) else {
            return Vec::new();
        };
        let values = self.mod_values.get(&track.id).copied().unwrap_or_default();
        track
            .modulators
            .iter()
            .enumerate()
            .flat_map(|(k, m)| {
                let bipolar = self
                    .session
                    .project
                    .mod_kind(t, k)
                    .is_some_and(|k| k.bipolar());
                let low = if bipolar { -1.0 } else { 0.0 };
                m.routes
                    .iter()
                    .filter(move |r| r.target == mt)
                    .map(move |r| (k, low, r.depth))
            })
            .map(|(k, low, depth)| {
                let (a, b) = (self.mod_pos(t, mt, depth * low), self.mod_pos(t, mt, depth));
                Ring {
                    color: MOD_COLORS[k % MAX_MODS],
                    lo: a.min(b),
                    hi: a.max(b),
                    live: self.mod_pos(t, mt, depth * values[k]),
                }
            })
            .collect()
    }

    /// A knob that modulators can move: `inner` (a `size` knob) with its rings round it, a drop
    /// target for a modulator's ↗, lit while one is dragged.
    pub fn modded(
        &self,
        key: Option<(usize, ModTarget)>,
        size: f32,
        inner: impl IntoElement,
    ) -> Div {
        let wrap = div().relative().size(px(size)).flex_none().child(inner);
        let Some((t, mt)) = key else {
            return wrap;
        };
        let rings = self.mod_rings(t, mt);
        let offering = matches!(self.drag, Some(Drag::Modulator { track, .. }) if track == t);
        let targets = self.mod_targets.clone();
        wrap.child(
            canvas(
                move |b, _, _| targets.borrow_mut().push((t, mt, b)),
                move |b, _, window, _| paint_rings(b, &rings, offering, window),
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }

    /// The same for a fader that slides sideways (the header's), `w` wide: its rings as bars
    /// over it.
    pub fn modded_fader(&self, t: usize, w: f32, h: f32) -> Div {
        let rings = self.mod_rings(t, ModTarget::Volume);
        let offering = matches!(self.drag, Some(Drag::Modulator { track, .. }) if track == t);
        let targets = self.mod_targets.clone();
        div().absolute().top_0().left_0().w(px(w)).h(px(h)).child(
            canvas(
                move |b, _, _| targets.borrow_mut().push((t, ModTarget::Volume, b)),
                move |b, _, window, _| {
                    let (x0, w) = (b.origin.x + px(7.), f32::from(b.size.width) - 14.0);
                    if offering {
                        window.paint_quad(
                            fill(b, alpha(ACCENT, 0.12))
                                .corner_radii(px(5.))
                                .border_widths(px(1.))
                                .border_color(alpha(ACCENT, 0.7)),
                        );
                    }
                    for (k, ring) in rings.iter().enumerate() {
                        let y = b.origin.y + px(1.0 + 2.5 * k as f32);
                        let (a, z) = (x0 + px(w * ring.lo), x0 + px(w * ring.hi));
                        window.paint_quad(fill(
                            Bounds::new(point(a, y), size(z - a + px(1.), px(2.))),
                            alpha(ring.color, 0.6),
                        ));
                        window.paint_quad(
                            fill(
                                Bounds::centered_at(
                                    point(x0 + px(w * ring.live), y + px(1.)),
                                    size(px(5.), px(5.)),
                                ),
                                rgb(ring.color),
                            )
                            .corner_radii(px(2.5)),
                        );
                    }
                },
            )
            .size_full(),
        )
    }

    /// A modulator's name on its track: LFO 1, Steps 2.
    pub fn mod_name(&self, t: usize, k: usize) -> String {
        let p = &self.session.project;
        let Some(m) = p.tracks.get(t).and_then(|t| t.modulators.get(k)) else {
            return String::new();
        };
        // One its stack shares by the stack's name, numbered among the stack's.
        let (list, at, from) = match m.kind {
            ModKind::Shared { modulator } => match p.stack_of(t) {
                Some(aux) if (modulator as usize) < aux.modulators.len() => (
                    &aux.modulators[..],
                    modulator as usize,
                    format!("{} · ", aux.name),
                ),
                _ => return "Shared".into(),
            },
            _ => (&p.tracks[t].modulators[..], k, String::new()),
        };
        let kind = list[at].kind.name();
        let n = list[..=at].iter().filter(|o| o.kind.name() == kind).count();
        format!("{from}{kind} {n}")
    }

    /// What a route moves, as people say it.
    pub fn target_name(&self, t: usize, mt: ModTarget) -> String {
        let track = self.session.project.tracks.get(t);
        match mt {
            ModTarget::Volume => "Volume".into(),
            ModTarget::Pan => "Pan".into(),
            ModTarget::Send(i) => {
                let aux = track
                    .and_then(|tr| tr.sends.get(i))
                    .and_then(|s| self.session.project.auxes.iter().find(|a| a.bus == s.bus));
                format!("Send to {}", aux.map_or("its aux", |a| a.name.as_str()))
            }
            ModTarget::LowCut => "Low cut".into(),
            ModTarget::EqGain(i) | ModTarget::EqFreq(i) => {
                let band = track
                    .and_then(|tr| tr.eq)
                    .and_then(|e| e.bands.get(i as usize).copied());
                let what = if matches!(mt, ModTarget::EqGain(_)) {
                    "gain"
                } else {
                    "frequency"
                };
                match band {
                    Some(b) => format!(
                        "EQ {} {what} ({} {})",
                        i + 1,
                        b.shape.name().to_lowercase(),
                        crate::mix::eq::hz_text(b.freq)
                    ),
                    None => format!("EQ band {} {what}", i + 1),
                }
            }
            ModTarget::CompThreshold => "Threshold".into(),
            ModTarget::CompMakeup => "Makeup".into(),
            ModTarget::SatDrive => "Drive".into(),
            ModTarget::SatTone => "Tone".into(),
            ModTarget::SatMix => "Saturation mix".into(),
            ModTarget::Sampler(p) => match p {
                SamplerParam::Cutoff => "Cutoff",
                SamplerParam::Resonance => "Resonance",
                SamplerParam::FilterEnv => "Filter envelope",
                SamplerParam::Transpose => "Transpose",
                SamplerParam::Volume => "Sampler level",
                SamplerParam::Vibrato => "Vibrato",
                SamplerParam::LfoFilter => "LFO to filter",
                SamplerParam::Tremolo => "Tremolo",
            }
            .into(),
            ModTarget::Plugin(which, id) => {
                let name = track
                    .and_then(|tr| {
                        let names = self.param_names(tr.id, which);
                        names.iter().find(|p| p.0 == id).map(|p| p.1.clone())
                    })
                    .unwrap_or_else(|| format!("Parameter {id}"));
                // An effect's, by its plugin: two can both have a Cutoff.
                match (which, track.and_then(|tr| tr.plugin(which))) {
                    (Which::Insert(_), Some(r)) => format!("{} · {name}", r.class),
                    _ => name,
                }
            }
            ModTarget::Sound(r, param) => {
                let sound = self
                    .sound_name(t, r)
                    .unwrap_or_else(|| format!("Sound {}", r + 1));
                let what = match param {
                    pina_engine::dsp::modulate::SoundParam::Velocity => "velocity",
                    pina_engine::dsp::modulate::SoundParam::Pitch => "pitch",
                    pina_engine::dsp::modulate::SoundParam::Chance => "chance",
                };
                format!("{sound} {what}")
            }
        }
    }

    /// A step pattern's sound on a track, by its row: the pattern shown, else the track's first.
    fn sound_name(&self, t: usize, r: usize) -> Option<String> {
        let track = self.session.project.tracks.get(t)?;
        let shown = self.pattern().filter(|at| at.0 == t).map(|at| at.1);
        let regions = shown.into_iter().chain(0..track.regions.len());
        let pitch = regions
            .filter_map(|i| match &track.regions.get(i)?.content {
                pina_engine::song::project::Content::Pattern { rows, .. } => {
                    rows.get(r).map(|row| row.pitch)
                }
                _ => None,
            })
            .next()?;
        Some(crate::editors::steps::row_name(
            track.instrument.as_ref(),
            self.session.project.song_key(),
            pitch,
        ))
    }

    /// Everything a modulator on track `t` can move, as a menu (the step pattern's sounds too).
    pub fn target_menu(&self, t: usize, k: usize) -> Vec<(String, MenuAction)> {
        let Some(track) = self.session.project.tracks.get(t) else {
            return Vec::new();
        };
        let ahead = !matches!(
            self.session.project.mod_kind(t, k),
            Some(ModKind::Envelope { .. } | ModKind::Follower { .. })
        );
        let mut items: Vec<(String, MenuAction)> = Vec::new();
        let group =
            |items: &mut Vec<(String, MenuAction)>, title: &str, targets: Vec<ModTarget>| {
                if targets.is_empty() {
                    return;
                }
                if !items.is_empty() {
                    items.push((String::new(), MenuAction::Nothing));
                }
                items.push((title.to_string(), MenuAction::Nothing));
                for mt in targets {
                    items.push((self.target_name(t, mt), MenuAction::Map(t, k, mt)));
                }
            };
        // The pattern's sounds first: what a step sequencer modulates.
        let rows = self
            .pattern()
            .filter(|at| at.0 == t)
            .and_then(|at| match &track.regions[at.1].content {
                pina_engine::song::project::Content::Pattern { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .unwrap_or(0);
        if ahead {
            use pina_engine::dsp::modulate::SoundParam;
            let sounds = (0..rows)
                .flat_map(|r| {
                    [SoundParam::Velocity, SoundParam::Pitch, SoundParam::Chance]
                        .map(|p| ModTarget::Sound(r, p))
                })
                .collect();
            group(&mut items, "Its pattern's sounds", sounds);
        }
        let mut mix = vec![ModTarget::Volume, ModTarget::Pan];
        mix.extend((0..track.sends.len()).map(ModTarget::Send));
        group(&mut items, "Mix", mix);
        group(
            &mut items,
            "Channel EQ, compressor, saturator",
            std::iter::once(ModTarget::LowCut)
                .chain(track.eq.iter().flat_map(|e| {
                    e.bands.iter().enumerate().flat_map(|(i, b)| {
                        let gain = b.shape.has_gain().then_some(ModTarget::EqGain(i as u8));
                        gain.into_iter().chain([ModTarget::EqFreq(i as u8)])
                    })
                }))
                .chain([
                    ModTarget::CompThreshold,
                    ModTarget::CompMakeup,
                    ModTarget::SatDrive,
                    ModTarget::SatTone,
                    ModTarget::SatMix,
                ])
                .collect(),
        );
        if matches!(track.instrument, Some(Instrument::Sampler(_))) {
            group(
                &mut items,
                "Sampler",
                SamplerParam::ALL.map(ModTarget::Sampler).to_vec(),
            );
        }
        // Every parameter of its plugins: the instrument's, then each insert's (type to find).
        for which in self.plugin_slots(t) {
            let names = self.param_names(track.id, which);
            if names.is_empty() {
                continue;
            }
            items.push((String::new(), MenuAction::Nothing));
            items.push((
                match which {
                    Which::Instrument => "Instrument".to_string(),
                    Which::Insert(i) => format!(
                        "Insert {}: {}",
                        i + 1,
                        track.inserts.get(i).map_or("", |r| r.class.as_str())
                    ),
                },
                MenuAction::Nothing,
            ));
            // Named from the one list (asking a plugin for one parameter's name asks it for
            // all); an effect's by its plugin too, as `target_name` says them.
            let class = match which {
                Which::Insert(i) => track.inserts.get(i).map(|r| r.class.as_str()),
                Which::Instrument => None,
            };
            items.extend(names.iter().map(|(id, name)| {
                let label = match class {
                    Some(c) => format!("{c} · {name}"),
                    None => name.clone(),
                };
                (label, MenuAction::Map(t, k, ModTarget::Plugin(which, *id)))
            }));
        }
        items
    }

    /// The plugins on track `t`: its instrument, if it's one, and its inserts.
    fn plugin_slots(&self, t: usize) -> Vec<Which> {
        let Some(track) = self.session.project.tracks.get(t) else {
            return Vec::new();
        };
        let instrument =
            matches!(track.instrument, Some(Instrument::Plugin(_))).then_some(Which::Instrument);
        instrument
            .into_iter()
            .chain((0..track.inserts.len()).map(Which::Insert))
            .collect()
    }

    /// Modulator `k` of track `t` learning its target (again: stops): the next parameter turned
    /// in one of the track's plugins' own windows. Opens the instrument's window if none is.
    pub fn learn(&mut self, t: usize, k: usize, cx: &mut Context<Self>) {
        if self
            .mod_learn
            .as_ref()
            .is_some_and(|l| (l.track, l.index) == (t, k))
        {
            self.mod_learn = None;
            self.status = Some("Stopped learning.".into());
            cx.notify();
            return;
        }
        let Some(track) = self.session.project.tracks.get(t) else {
            return;
        };
        let id = track.id;
        // What moves on its own (a route, automation) can't be told from a turn.
        let moved = |which: Which, param: u32| {
            track.modulators.iter().any(|m| {
                m.routes
                    .iter()
                    .any(|r| r.target == ModTarget::Plugin(which, param))
            }) || track
                .automation
                .plugins
                .iter()
                .any(|a| a.plugin == which && a.param == param)
        };
        let slots = self.plugin_slots(t);
        let base: Vec<(Which, u32, f64)> = slots
            .iter()
            .flat_map(|&which| {
                self.player
                    .params(id, which)
                    .into_iter()
                    .filter(move |p| !moved(which, p.id))
                    .map(move |p| (which, p.id, p.value))
            })
            .collect();
        if base.is_empty() {
            self.status = Some(
                "No plugin on this track to learn from: give it a plugin instrument or effect."
                    .into(),
            );
            cx.notify();
            return;
        }
        // Its window, to turn something in (not in tests: no windows on the screen).
        if let Some(&first) = slots.first().filter(|_| !cfg!(test)) {
            let _ = self.player.open_editor(id, first);
        }
        self.mod_learn = Some(Learn {
            track: t,
            index: k,
            base,
        });
        self.status = Some(format!(
            "{}: turn a control in the plugin's window to map it there (Learn again stops).",
            self.mod_name(t, k)
        ));
        cx.notify();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(120))
                    .await;
                match this.update(cx, |ws, cx| ws.learn_tick(cx)) {
                    Ok(false) => {}
                    _ => break,
                }
            }
        })
        .detach();
    }

    /// Whether learning's done: the first parameter turned since it began, mapped.
    fn learn_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(l) = &self.mod_learn else {
            return true;
        };
        let (t, k) = (l.track, l.index);
        let Some(id) = self
            .session
            .project
            .tracks
            .get(t)
            .filter(|tr| k < tr.modulators.len())
            .map(|tr| tr.id)
        else {
            self.mod_learn = None;
            return true;
        };
        let turned = l
            .base
            .iter()
            .find(|&&(which, param, was)| {
                (self.player.param_value(id, which, param) - was).abs() > TURNED
            })
            .map(|&(which, param, _)| ModTarget::Plugin(which, param));
        let Some(mt) = turned else {
            return false;
        };
        self.mod_learn = None;
        self.map_modulator(t, k, mt, cx);
        cx.notify();
        true
    }

    /// Edits a track's modulators as one undo step, and the engine hears.
    fn mods_edit(&mut self, t: usize, cx: &mut Context<Self>, f: impl FnOnce(&mut Vec<Modulator>)) {
        if t >= self.session.project.tracks.len() {
            return;
        }
        self.edit(cx, |ws| f(&mut ws.session.project.tracks[t].modulators));
    }

    pub fn add_modulator(&mut self, t: usize, kind: ModKind, cx: &mut Context<Self>) {
        let Some(track) = self.session.project.tracks.get(t) else {
            return;
        };
        if track.modulators.len() >= MAX_MODS {
            self.status = Some(format!("A track has {MAX_MODS} modulators at most."));
            cx.notify();
            return;
        }
        let k = track.modulators.len();
        self.mods_edit(t, cx, |mods| {
            mods.push(Modulator {
                kind,
                routes: Vec::new(),
            })
        });
        self.status = Some(format!(
            "{}: drag its ↗ onto a knob to have it move it",
            self.mod_name(t, k)
        ));
    }

    /// A modulator for every track in `t`'s stack: the stack's, each track routing it as it
    /// likes.
    pub fn share_modulator(&mut self, t: usize, kind: ModKind, cx: &mut Context<Self>) {
        let Some(aux) = self.session.project.stack_of(t) else {
            return;
        };
        if aux.modulators.len() >= MAX_MODS {
            self.status = Some(format!("A stack shares {MAX_MODS} modulators at most."));
            cx.notify();
            return;
        }
        let (bus, stack) = (aux.bus, aux.name.clone());
        self.edit(cx, |ws| {
            ws.session.project.add_shared(bus, kind);
        });
        let k = self.session.project.tracks[t]
            .modulators
            .len()
            .saturating_sub(1);
        self.status = Some(format!(
            "{}: every track in {stack} has it; drag its ↗ onto a knob of each to have it move it",
            self.mod_name(t, k)
        ));
    }

    pub fn remove_modulator(&mut self, t: usize, k: usize, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let Some(m) = p.tracks.get(t).and_then(|tr| tr.modulators.get(k)) else {
            return;
        };
        // One the stack shares goes from the stack and every track in it; else, this one.
        let shared = match m.kind {
            ModKind::Shared { modulator } if p.mod_kind(t, k).is_some() => {
                p.stack_of(t).map(|a| (a.bus, modulator as usize))
            }
            _ => None,
        };
        let gone: Vec<(usize, usize)> = match shared {
            Some((bus, s)) => p
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, tr)| tr.output == Some(bus))
                .filter_map(|(i, tr)| {
                    let at = tr.modulators.iter().position(
                        |m| matches!(m.kind, ModKind::Shared { modulator } if modulator as usize == s),
                    )?;
                    Some((i, at))
                })
                .collect(),
            None => vec![(t, k)],
        };
        let routes: Vec<(usize, ModRoute)> = gone
            .iter()
            .flat_map(|&(i, at)| {
                p.tracks[i].modulators[at]
                    .routes
                    .iter()
                    .map(move |r| (i, *r))
            })
            .collect();
        self.edit(cx, |ws| match shared {
            Some((bus, s)) => ws.session.project.remove_shared(bus, s),
            None => {
                let track = &mut ws.session.project.tracks[t];
                track.modulators.remove(k);
                // Its settings' automation goes with it; the ones after it move up a place.
                track.automation.modulator_removed(k);
            }
        });
        for (i, k) in gone {
            if let Some(id) = self.session.project.tracks.get(i).map(|tr| tr.id)
                && let Some(lanes) = self.auto_lanes.get_mut(&id)
            {
                lanes.retain(|p| !matches!(p, Param::Mod(m, _) if *m as usize == k));
                for p in lanes.iter_mut() {
                    if let Param::Mod(m, s) = *p
                        && m as usize > k
                    {
                        *p = Param::Mod(m - 1, s);
                    }
                }
            }
        }
        for (i, r) in routes {
            self.settle(i, r.target);
        }
        self.mod_edit = None;
    }

    /// A plugin parameter no longer moved: back where it's set.
    fn settle(&mut self, t: usize, mt: ModTarget) {
        if let (ModTarget::Plugin(which, id), Some(track)) =
            (mt, self.session.project.tracks.get(t))
        {
            let still = track
                .modulators
                .iter()
                .any(|m| m.routes.iter().any(|r| r.target == mt));
            if !still {
                let set = track.plugin(which).and_then(|r| r.params.get(&id).copied());
                if let Some(v) = set {
                    self.player.set_param(track.id, which, id, v);
                }
            }
        }
    }

    /// Modulator `k` of track `t` moving `mt`, by its usual depth (kept if it already does).
    pub fn map_modulator(&mut self, t: usize, k: usize, mt: ModTarget, cx: &mut Context<Self>) {
        let Some(track) = self.session.project.tracks.get(t) else {
            return;
        };
        // A pattern's steps are worked out ahead: an envelope or follower can't be known then.
        if let (ModTarget::Sound(..), Some(ModKind::Envelope { .. } | ModKind::Follower { .. })) =
            (mt, self.session.project.mod_kind(t, k))
        {
            self.status = Some(
                "A sound's steps are worked out ahead: an LFO, steps, random or a macro can move them, not an envelope or a follower.".into(),
            );
            cx.notify();
            return;
        }
        if track
            .modulators
            .get(k)
            .is_none_or(|m| m.routes.iter().any(|r| r.target == mt))
        {
            return;
        }
        // A plugin parameter moves about where it's set: kept in the song for the engine.
        let base = match mt {
            ModTarget::Plugin(which, id) => {
                Some((which, id, self.player.param_value(track.id, which, id)))
            }
            _ => None,
        };
        self.edit(cx, |ws| {
            let track = &mut ws.session.project.tracks[t];
            if let Some((which, id, v)) = base
                && let Some(r) = track.plugin_mut(which)
                && !r.params.contains_key(&id)
            {
                r.params.insert(id, v);
            }
            track.modulators[k].routes.push(ModRoute {
                target: mt,
                depth: modulate::default_depth(mt),
            });
        });
        self.status = Some(format!(
            "{} → {}: drag the route in its card, or pick the card and drag the knob, for how far",
            self.mod_name(t, k),
            self.target_name(t, mt)
        ));
    }

    pub fn unmap(&mut self, t: usize, k: usize, mt: ModTarget, cx: &mut Context<Self>) {
        self.mods_edit(t, cx, |mods| {
            if let Some(m) = mods.get_mut(k) {
                m.routes.retain(|r| r.target != mt);
            }
        });
        self.settle(t, mt);
    }

    /// A modulator dropped: onto the knob under the pointer, if it's one its track has.
    pub fn drop_modulator(&mut self, t: usize, k: usize, at: Point<f32>, cx: &mut Context<Self>) {
        let at = point(px(at.x), px(at.y));
        let hit = self
            .mod_targets
            .borrow()
            .iter()
            .rev()
            .find(|(_, _, b)| b.contains(&at))
            .map(|&(tt, mt, _)| (tt, mt));
        match hit {
            Some((tt, mt)) if tt == t => self.map_modulator(t, k, mt, cx),
            Some(_) => {
                self.status = Some("A modulator moves its own track's settings.".into());
                cx.notify();
            }
            None => {}
        }
    }

    /// A knob dragged while a modulator that moves it is picked: its depth instead.
    pub fn depth_drag(&self, drag: Drag) -> Drag {
        let Some((t, k)) = self.mod_edit else {
            return drag;
        };
        let (key, x, y) = match drag {
            Drag::Knob { target, y, .. } => (self.mod_key(target), 0.0, y),
            Drag::HFader { track, x, .. } => (Some((track, ModTarget::Volume)), x, 0.0),
            Drag::Pan {
                channel: Channel::Track(track),
                x,
                y,
                ..
            } => (Some((track, ModTarget::Pan)), x, y),
            _ => return drag,
        };
        let Some((tt, mt)) = key.filter(|&(tt, _)| tt == t) else {
            return drag;
        };
        let depth = self
            .session
            .project
            .tracks
            .get(tt)
            .and_then(|tr| tr.modulators.get(k))
            .and_then(|m| m.routes.iter().find(|r| r.target == mt))
            .map(|r| r.depth);
        match depth {
            Some(depth) => Drag::ModDepth {
                track: t,
                index: k,
                target: mt,
                x,
                y,
                depth,
            },
            None => drag,
        }
    }

    /// A route's depth dragged: up or right more, ⇧ finer; as far as its target goes either way.
    pub fn drag_depth(
        &mut self,
        (t, k, mt): (usize, usize, ModTarget),
        from: (f32, f32),
        depth: f32,
        p: Point<f32>,
    ) {
        let range = modulate::depth_range(mt);
        let (dx, dy) = (p.x - from.0, from.1 - p.y);
        let by = if dx.abs() > dy.abs() { dx } else { dy };
        let fine = if self.fine { 0.2 } else { 1.0 };
        let to = (depth + by / 160.0 * range * fine).clamp(-range, range);
        if let Some(r) = self
            .session
            .project
            .tracks
            .get_mut(t)
            .and_then(|tr| tr.modulators.get_mut(k))
            .and_then(|m| m.routes.iter_mut().find(|r| r.target == mt))
        {
            r.depth = to;
        }
        self.status = Some(format!(
            "{} → {}  {}",
            self.mod_name(t, k),
            self.target_name(t, mt),
            modulate::depth_text(mt, to)
        ));
    }

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
    fn mod_automated(&self, t: usize, k: usize, s: ModParam) -> Option<f32> {
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

    /// Each frame: the modulators' values, and a trace of each to draw.
    pub fn read_mods(&mut self) {
        let ids: Vec<(u64, usize)> = self
            .session
            .project
            .tracks
            .iter()
            .filter(|t| !t.modulators.is_empty())
            .map(|t| (t.id, t.modulators.len()))
            .collect();
        for (id, n) in ids {
            let values = self.player.modulators(id);
            self.mod_values.insert(id, values);
            for (k, &v) in values.iter().enumerate().take(n) {
                let trace = self.mod_trace.entry((id, k)).or_default();
                trace.push_back(v);
                while trace.len() > TRACE {
                    trace.pop_front();
                }
            }
        }
    }

    /// Where a modulator in time with the song is round its cycle: the playhead's, while it
    /// plays; the clock's, when stopped.
    fn mod_phase(&self, rate: ModRate) -> f64 {
        let tempo = self.session.project.tempo;
        match (rate, self.player.playing()) {
            (ModRate::Beats(b), true) => {
                let beats = self.playhead() as f64 / pina_engine::song::project::PPQ as f64;
                beats / b.max(1e-3) as f64
            }
            _ => self.mod_clock.elapsed().as_secs_f64() * rate.hz(tempo),
        }
    }

    /// The Modulation pane: the selected track's modulators as cards, and ones to add.
    pub fn modulation_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.track;
        let Some(track) = self.session.project.tracks.get(t) else {
            return div().flex_1().into_any_element();
        };
        let count = track.modulators.len();
        let cards: Vec<Div> = (0..count).map(|k| self.mod_card(t, k, cx)).collect();
        let add = |label: &'static str, kind: ModKind, cx: &mut Context<Self>| {
            div()
                .h(px(22.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(7.))
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(CARD))
                .text_size(px(11.))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .debug_selector(move || format!("add-mod-{label}"))
                .child(format!("+ {label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.add_modulator(t, kind.clone(), cx)),
                )
        };
        // A follower starts on the kick, or else the first other track.
        let tracks = &self.session.project.tracks;
        let source = tracks
            .iter()
            .find(|o| o.id != track.id && o.name.to_lowercase().contains("kick"))
            .or_else(|| tracks.iter().find(|o| o.id != track.id))
            .map_or(track.id, |o| o.id);
        let adders = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .flex_none()
            .child(
                div()
                    .mr(px(2.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child("Add"),
            )
            .child(add("LFO", ModKind::lfo(), cx))
            .child(add("Steps", ModKind::steps(), cx))
            .child(add("Envelope", ModKind::envelope(), cx))
            .child(add("Follower", ModKind::follower(source), cx))
            .child(add("Random", ModKind::random(), cx))
            .child(add("Macro", ModKind::mac(), cx))
            .when_some(self.shared_menu(t), |d, items| {
                d.child(
                    div()
                        .h(px(22.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .rounded(px(7.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(CARD))
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_2))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                        .debug_selector(|| "add-mod-shared".into())
                        .child("+ Shared ▾")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items: items.clone(),
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        ),
                )
            });
        let hint = match (count, self.mod_edit) {
            (0, _) => "Add a modulator, then drag its ↗ Map onto any knob or fader of this track: its volume and pan, Smart Controls, EQ, compressor, saturator, Sampler, a send".to_string(),
            (_, Some((tt, k))) if tt == t => format!(
                "{} picked: drag a knob it moves to set how far · click the card again to let go",
                self.mod_name(t, k)
            ),
            _ => "Drag a card's ↗ onto a knob · drag a route sideways for its depth · click a card, then drag its knobs for depth".to_string(),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .bg(rgb(LANE))
            .child(
                div()
                    .h(px(34.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(14.))
                    .border_b_1()
                    .border_color(rgb(LINE_SOFT))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(hint),
                    )
                    .child(adders),
            )
            .child(
                div()
                    .id("mod-cards")
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(10.))
                    .p(px(12.))
                    .overflow_x_scroll()
                    .children(cards),
            )
            .into_any_element()
    }

    fn mod_card(&self, t: usize, k: usize, cx: &mut Context<Self>) -> Div {
        let track = &self.session.project.tracks[t];
        // One its stack shares: the stack's, with this track's routes.
        let m = Modulator {
            kind: self
                .session
                .project
                .mod_kind(t, k)
                .unwrap_or(&track.modulators[k].kind)
                .clone(),
            routes: track.modulators[k].routes.clone(),
        };
        let color = MOD_COLORS[k % MAX_MODS];
        let picked = self.mod_edit == Some((t, k));
        let value = self.mod_values.get(&track.id).map_or(0.0, |v| v[k]);
        let name = self.mod_name(t, k);
        // One its stack shares: named as the stack's in its title, the stack on its picture.
        let stack = match track.modulators[k].kind {
            ModKind::Shared { .. } => self.session.project.stack_of(t).map(|a| a.name.clone()),
            _ => None,
        };
        let name = match &stack {
            Some(s) => name
                .strip_prefix(&format!("{s} · "))
                .unwrap_or(&name)
                .to_string(),
            None => name,
        };
        let menu = |label: String,
                    items: Vec<(String, MenuAction)>,
                    name: String,
                    cx: &mut Context<Self>| {
            div()
                .h(px(22.))
                .px(px(7.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .bg(rgb(FIELD))
                .font_family(MONO)
                .text_size(px(10.5))
                .text_color(rgb(TEXT_2))
                .whitespace_nowrap()
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)))
                .debug_selector(move || name.clone())
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.menu = Some(Menu {
                            at: e.position,
                            items: items.clone(),
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                )
        };
        let change = |c: ModChange| MenuAction::Mod(t, k, c);
        let rates = |now: ModRate| -> Vec<(String, MenuAction)> {
            let mut items: Vec<(String, MenuAction)> = SYNCED
                .iter()
                .map(|&b| {
                    let r = ModRate::Beats(b);
                    let tick = if now == r { "✓ " } else { "" };
                    (format!("{tick}{}", r.label()), change(ModChange::Rate(r)))
                })
                .collect();
            items.push((String::new(), MenuAction::Nothing));
            items.extend(FREE.iter().map(|&h| {
                let r = ModRate::Hz(h);
                let tick = if now == r { "✓ " } else { "" };
                (format!("{tick}{}", r.label()), change(ModChange::Rate(r)))
            }));
            items
        };
        let mini = |label: &'static str, field: ModField, text: String, cx: &mut Context<Self>| {
            let v = self.mod_field(t, k, field);
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(1.))
                .cursor_ns_resize()
                .debug_selector(move || format!("mod-{k}-{field:?}"))
                .child(div().size(px(24.)).child(knob(v, 24., color, false)))
                .child(
                    div()
                        .text_size(px(9.5))
                        .text_color(rgb(TEXT_3))
                        .child(label),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(9.5))
                        .text_color(rgb(TEXT_2))
                        .child(text),
                )
                // Right-click: automate it.
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let Some(s) = setting_of(field) else {
                            return;
                        };
                        let param = Param::Mod(k as u8, s);
                        let id = ws.session.project.tracks[t].id;
                        let has = !ws.session.project.tracks[t]
                            .automation
                            .points(param)
                            .is_empty();
                        let label = match has {
                            true => format!("Show {}'s automation", s.name()),
                            false => format!("Automate {}", s.name()),
                        };
                        ws.menu = Some(Menu {
                            at: e.position,
                            items: vec![(
                                label,
                                MenuAction::Auto(
                                    id,
                                    crate::arrange::automation::AutoOp::Automate(param),
                                ),
                            )],
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let y = pt(e.position).y;
                        ws.begin_drag(
                            Drag::ModValue {
                                track: t,
                                index: k,
                                field,
                                y,
                                value: v,
                            },
                            cx,
                        );
                    }),
                )
        };
        let ms = |v: f32| match v {
            v if v >= 1000.0 => format!("{:.1} s", v / 1000.0),
            v => format!("{v:.0} ms"),
        };
        // What it is, and how it's set.
        let controls: Div = match &m.kind {
            ModKind::Lfo {
                shape,
                rate,
                phase,
                unipolar,
            } => {
                let shapes = LfoShape::ALL
                    .iter()
                    .map(|&s| {
                        let tick = if s == *shape { "✓ " } else { "" };
                        (format!("{tick}{}", s.name()), change(ModChange::Shape(s)))
                    })
                    .collect();
                let uni = *unipolar;
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(menu(
                        shape.name().into(),
                        shapes,
                        format!("mod-{k}-shape"),
                        cx,
                    ))
                    .child(menu(
                        rate.label(),
                        rates(*rate),
                        format!("mod-{k}-rate"),
                        cx,
                    ))
                    .child(
                        div()
                            .h(px(22.))
                            .px(px(7.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .bg(if uni {
                                alpha(color, 0.25)
                            } else {
                                rgb(FIELD).into()
                            })
                            .text_size(px(10.5))
                            .text_color(rgb(if uni { TEXT } else { TEXT_3 }))
                            .cursor_pointer()
                            .child(if uni { "0 → 1" } else { "±" })
                            .debug_selector(move || format!("mod-{k}-uni"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.mod_change(t, k, ModChange::Unipolar(!uni), cx)
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(mini(
                        "Phase",
                        ModField::Phase,
                        format!("{:.0}°", phase * 360.0),
                        cx,
                    ))
            }
            ModKind::Steps {
                values,
                rate,
                glide,
            } => {
                let counts = [4usize, 8, 16, 32]
                    .map(|n| {
                        let tick = if n == values.len() { "✓ " } else { "" };
                        (format!("{tick}{n} steps"), change(ModChange::Count(n)))
                    })
                    .to_vec();
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(menu(
                        rate.label(),
                        rates(*rate),
                        format!("mod-{k}-rate"),
                        cx,
                    ))
                    .child(menu(
                        format!("{} steps", values.len()),
                        counts,
                        format!("mod-{k}-count"),
                        cx,
                    ))
                    .child(menu(
                        "Dice".into(),
                        vec![("Random values".into(), change(ModChange::Scramble))],
                        format!("mod-{k}-dice"),
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(mini(
                        "Glide",
                        ModField::Glide,
                        format!("{:.0}%", glide * 100.0),
                        cx,
                    ))
            }
            ModKind::Envelope {
                attack_ms,
                decay_ms,
                sustain,
                release_ms,
            } => div()
                .flex()
                .justify_between()
                .child(mini("Attack", ModField::Attack, ms(*attack_ms), cx))
                .child(mini("Decay", ModField::Decay, ms(*decay_ms), cx))
                .child(mini(
                    "Sustain",
                    ModField::Sustain,
                    format!("{:.0}%", sustain * 100.0),
                    cx,
                ))
                .child(mini("Release", ModField::Release, ms(*release_ms), cx)),
            ModKind::Follower {
                source,
                attack_ms,
                release_ms,
            } => {
                let sources: Vec<(String, MenuAction)> = self
                    .session
                    .project
                    .tracks
                    .iter()
                    .filter(|o| o.id != track.id)
                    .map(|o| {
                        let tick = if o.id == *source { "✓ " } else { "" };
                        (format!("{tick}{}", o.name), change(ModChange::Source(o.id)))
                    })
                    .collect();
                let heard = self
                    .session
                    .project
                    .tracks
                    .iter()
                    .find(|o| o.id == *source)
                    .map_or("a track".to_string(), |o| o.name.clone());
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .child("Hears"),
                    )
                    .child(menu(heard, sources, format!("mod-{k}-source"), cx))
                    .child(div().flex_1())
                    .child(mini("Attack", ModField::Attack, ms(*attack_ms), cx))
                    .child(mini("Release", ModField::Release, ms(*release_ms), cx))
            }
            ModKind::Random { rate, smooth } => {
                let on = *smooth;
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(menu(
                        rate.label(),
                        rates(*rate),
                        format!("mod-{k}-rate"),
                        cx,
                    ))
                    .child(
                        div()
                            .h(px(22.))
                            .px(px(7.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .bg(if on {
                                alpha(color, 0.25)
                            } else {
                                rgb(FIELD).into()
                            })
                            .text_size(px(10.5))
                            .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                            .cursor_pointer()
                            .child(if on { "Smooth" } else { "Stepped" })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.mod_change(t, k, ModChange::Smooth(!on), cx)
                                }),
                            ),
                    )
            }
            ModKind::Macro { value } => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child("Turn it, or automate it"),
                )
                .child(div().flex_1())
                .child(mini(
                    "Value",
                    ModField::Value,
                    format!("{:.0}%", value * 100.0),
                    cx,
                )),
            // The track left its stack, or the stack the modulator.
            ModKind::Shared { .. } => div()
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .child("Its stack no longer shares this: it moves nothing"),
        };
        // Where it goes: each route's depth dragged sideways (⇧ finer), × to take it off.
        let routes: Vec<Div> = m
            .routes
            .iter()
            .map(|r| {
                let (mt, depth) = (r.target, r.depth);
                let range = modulate::depth_range(mt);
                let share = (depth / range).clamp(-1.0, 1.0);
                div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(7.))
                    .rounded(px(6.))
                    .bg(rgb(0x141419))
                    .relative()
                    .overflow_hidden()
                    .cursor_ew_resize()
                    .debug_selector(move || format!("mod-{k}-route-{mt:?}"))
                    // How far, as a bar from the middle.
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(gpui::relative(0.5 + share.min(0.0) / 2.0))
                            .w(gpui::relative(share.abs() / 2.0))
                            .bg(alpha(color, 0.22)),
                    )
                    .child(div().text_color(rgb(color)).text_size(px(11.)).child("→"))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(self.target_name(t, mt)),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_2))
                            .child(modulate::depth_text(mt, depth)),
                    )
                    .child(
                        div()
                            .size(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .child("✕")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.unmap(t, k, mt, cx);
                                }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let p = pt(e.position);
                            ws.begin_drag(
                                Drag::ModDepth {
                                    track: t,
                                    index: k,
                                    target: mt,
                                    x: p.x,
                                    y: p.y,
                                    depth,
                                },
                                cx,
                            );
                        }),
                    )
            })
            .collect();
        let none = m.routes.is_empty();
        div()
            .w(px(248.))
            .flex_none()
            .h_full()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(12.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(if picked { color } else { LINE }))
            .when(picked, |d| {
                d.shadow(vec![gpui::BoxShadow {
                    color: alpha(color, 0.35),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(14.),
                    spread_radius: px(0.),
                    inset: false,
                }])
            })
            .debug_selector(move || format!("mod-card-{k}"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.mod_edit = match ws.mod_edit == Some((t, k)) {
                        true => None,
                        false => Some((t, k)),
                    };
                    cx.notify();
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(
                        div()
                            .flex_none()
                            .size(px(9.))
                            .rounded(px(5.))
                            .bg(rgb(color)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(div().flex_1())
                    // Its handle: drag it onto a knob.
                    .child(
                        div()
                            .h(px(22.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .rounded(px(6.))
                            .bg(alpha(color, 0.18))
                            .border_1()
                            .border_color(alpha(color, 0.5))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT))
                            .cursor(gpui::CursorStyle::ClosedHand)
                            .debug_selector(move || format!("mod-{k}-map"))
                            .child("↗")
                            .child(div().text_size(px(10.)).child("Map"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.mod_drag_at = Some(pt(e.position));
                                    ws.begin_drag(Drag::Modulator { track: t, index: k }, cx);
                                }),
                            ),
                    )
                    // Or learn it: whatever's turned next in a plugin's own window.
                    .child({
                        let learning = self
                            .mod_learn
                            .as_ref()
                            .is_some_and(|l| (l.track, l.index) == (t, k));
                        div()
                            .h(px(22.))
                            .px(px(7.))
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .bg(if learning {
                                alpha(color, 0.45)
                            } else {
                                rgb(FIELD).into()
                            })
                            .text_size(px(10.))
                            .text_color(rgb(if learning { TEXT } else { TEXT_2 }))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(RAISED)))
                            .debug_selector(move || format!("mod-{k}-learn"))
                            .child(if learning { "Learning…" } else { "Learn" })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.learn(t, k, cx);
                                }),
                            )
                    })
                    // Or pick where from a list.
                    .child({
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .debug_selector(move || format!("mod-{k}-to"))
                            .child("⋯")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    // Made when asked for: it names every parameter of the
                                    // track's plugins.
                                    let items = ws.target_menu(t, k);
                                    ws.menu = Some(Menu::search(e.position, items));
                                    cx.notify();
                                }),
                            )
                    })
                    .child(
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .debug_selector(move || format!("mod-{k}-remove"))
                            .child("✕")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.remove_modulator(t, k, cx);
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .relative()
                    .child(self.mod_visual(t, k, &m.kind, color, cx))
                    // Where it is now.
                    .child(
                        div()
                            .absolute()
                            .top(px(6.))
                            .right(px(8.))
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .child(format!("{value:+.2}")),
                    )
                    .when_some(stack, |d, s| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(6.))
                                .left(px(8.))
                                .px(px(6.))
                                .h(px(16.))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .bg(alpha(color, 0.18))
                                .text_size(px(9.5))
                                .text_color(rgb(TEXT_2))
                                .child(format!("Shared · {s}")),
                        )
                    }),
            )
            .child(controls.flex_none())
            .child(
                div()
                    .id(("mod-routes", k))
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .children(routes)
                    .when(none, |d| {
                        d.child(
                            div()
                                .h(px(24.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .border_1()
                                .border_dashed()
                                .border_color(rgb(LINE_STRONG))
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child("drag ↗ Map onto a knob"),
                        )
                    }),
            )
    }

    /// What a modulator moves to move an automation lane's parameter, if one can.
    pub fn mod_target_of(&self, t: usize, param: Param) -> Option<ModTarget> {
        use pina_engine::song::project::EqField;
        Some(match param {
            Param::Volume => ModTarget::Volume,
            Param::Pan => ModTarget::Pan,
            Param::Send(bus) => {
                let sends = &self.session.project.tracks.get(t)?.sends;
                ModTarget::Send(sends.iter().position(|s| s.bus == bus)?)
            }
            Param::Plugin(which, id) => ModTarget::Plugin(which, id),
            Param::Eq(band, EqField::Gain) => ModTarget::EqGain(band),
            Param::Eq(band, EqField::Freq) => ModTarget::EqFreq(band),
            _ => return None,
        })
    }

    /// An automation lane's menu of modulators: each of the track's, ticked if it moves `mt`
    /// (picking it maps it, or lets it go), and new ones that do.
    pub fn lane_mod_menu(
        &self,
        t: usize,
        mt: ModTarget,
        title: String,
    ) -> Vec<(String, MenuAction)> {
        let Some(track) = self.session.project.tracks.get(t) else {
            return Vec::new();
        };
        let mut items = vec![(format!("Modulate {title}"), MenuAction::Nothing)];
        for (k, m) in track.modulators.iter().enumerate() {
            let name = self.mod_name(t, k);
            items.push(match m.routes.iter().any(|r| r.target == mt) {
                true => (format!("✓ {name}"), MenuAction::Unmap(t, k, mt)),
                false => (name, MenuAction::Map(t, k, mt)),
            });
        }
        if track.modulators.len() < MAX_MODS {
            if !track.modulators.is_empty() {
                items.push((String::new(), MenuAction::Nothing));
            }
            for kind in [
                ModKind::lfo(),
                ModKind::steps(),
                ModKind::random(),
                ModKind::mac(),
                ModKind::envelope(),
            ] {
                items.push((
                    format!("New {}", kind.name()),
                    MenuAction::NewModFor(t, kind, mt),
                ));
            }
        }
        items
    }

    /// A new modulator on track `t`, moving `mt`.
    pub fn new_mod_for(&mut self, t: usize, kind: ModKind, mt: ModTarget, cx: &mut Context<Self>) {
        let Some(k) = self
            .session
            .project
            .tracks
            .get(t)
            .map(|tr| tr.modulators.len())
        else {
            return;
        };
        self.add_modulator(t, kind, cx);
        if self.session.project.tracks[t].modulators.len() > k {
            self.map_modulator(t, k, mt, cx);
        }
    }

    /// For a track in a stack: a modulator to share with every track in it, or one the stack
    /// shares that this track hasn't.
    pub fn shared_menu(&self, t: usize) -> Option<Vec<(String, MenuAction)>> {
        let p = &self.session.project;
        let aux = p.stack_of(t)?;
        let mut items = vec![(format!("Shared by {}", aux.name), MenuAction::Nothing)];
        // A follower starts on the kick, or else the first track outside the stack.
        let source = p
            .tracks
            .iter()
            .find(|o| o.name.to_lowercase().contains("kick"))
            .or_else(|| p.tracks.iter().find(|o| o.output != Some(aux.bus)))
            .map_or(p.tracks[t].id, |o| o.id);
        // An envelope hears notes, and a stack plays none of its own.
        for kind in [
            ModKind::lfo(),
            ModKind::steps(),
            ModKind::follower(source),
            ModKind::random(),
            ModKind::mac(),
        ] {
            items.push((
                format!("New {}", kind.name()),
                MenuAction::ShareMod(t, kind),
            ));
        }
        let has = |s: usize| {
            p.tracks[t]
                .modulators
                .iter()
                .any(|m| matches!(m.kind, ModKind::Shared { modulator } if modulator as usize == s))
        };
        let missing: Vec<usize> = (0..aux.modulators.len()).filter(|&s| !has(s)).collect();
        if !missing.is_empty() {
            items.push((String::new(), MenuAction::Nothing));
        }
        for s in missing {
            let kind = aux.modulators[s].kind.name();
            let n = aux.modulators[..=s]
                .iter()
                .filter(|o| o.kind.name() == kind)
                .count();
            items.push((
                format!("{} · {kind} {n}", aux.name),
                MenuAction::LinkMod(t, s as u8),
            ));
        }
        Some(items)
    }

    /// A card's picture of its modulator, moving: the wave and where it is, the steps (paint
    /// them), the envelope and its level, the follower's and random's recent values.
    fn mod_visual(
        &self,
        t: usize,
        k: usize,
        kind: &ModKind,
        color: u32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = self.session.project.tracks[t].id;
        let value = self.mod_values.get(&id).map_or(0.0, |v| v[k]);
        let trace: Vec<f32> = self
            .mod_trace
            .get(&(id, k))
            .map(|d| d.iter().copied().collect())
            .unwrap_or_default();
        let shape = match kind {
            ModKind::Lfo {
                shape,
                rate,
                phase,
                unipolar,
            } => {
                let at = (self.mod_phase(*rate) + *phase as f64).rem_euclid(1.0);
                Visual::Wave(*shape, *phase, at as f32, *unipolar)
            }
            ModKind::Steps { values, rate, .. } => {
                let n = values.len().max(1);
                let i = (self.mod_phase(*rate).floor() as i64).rem_euclid(n as i64) as usize;
                Visual::Steps(values.clone(), i)
            }
            ModKind::Envelope {
                attack_ms,
                decay_ms,
                sustain,
                release_ms,
            } => Visual::Envelope([*attack_ms, *decay_ms, *sustain, *release_ms], value),
            ModKind::Follower { .. } => Visual::Trace(trace, false),
            ModKind::Random { .. } => Visual::Trace(trace, true),
            ModKind::Macro { value } => Visual::Level(*value),
            ModKind::Shared { .. } => Visual::Level(0.0),
        };
        let steps = matches!(kind, ModKind::Steps { .. });
        let bounds = self.mod_steps.clone();
        div()
            .h(px(64.))
            .flex_none()
            .rounded(px(8.))
            .bg(rgb(0x0b0b0e))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .overflow_hidden()
            .relative()
            .debug_selector(move || format!("mod-{k}-visual"))
            .child(
                canvas(
                    move |b, _, _| {
                        if steps {
                            bounds.borrow_mut().insert((t, k), b);
                        }
                    },
                    move |b, _, window, _| paint_visual(b, &shape, color, window),
                )
                .absolute()
                .size_full(),
            )
            .when(steps, |d| {
                d.cursor_crosshair().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.begin_drag(Drag::ModSteps { track: t, index: k }, cx);
                        ws.paint_mod_step(t, k, pt(e.position));
                        ws.changed(cx);
                    }),
                )
            })
    }

    /// The modulator being dragged to a knob, under the pointer.
    pub fn mod_ghost(&self) -> Option<impl IntoElement> {
        let Some(Drag::Modulator { track, index }) = self.drag else {
            return None;
        };
        let at = self.mod_drag_at?;
        let color = MOD_COLORS[index % MAX_MODS];
        Some(
            div()
                .absolute()
                .left(px(at.x + 14.))
                .top(px(at.y + 10.))
                .flex()
                .items_center()
                .gap(px(7.))
                .px(px(10.))
                .py(px(6.))
                .rounded(px(9.))
                .bg(rgb(0x1f1f26))
                .border_1()
                .border_color(alpha(color, 0.6))
                .shadow_lg()
                .text_size(px(12.))
                .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(color)))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.mod_name(track, index)),
                )
                .child(div().text_color(rgb(TEXT_3)).child("drop it on a knob")),
        )
    }
}

/// What a card draws.
enum Visual {
    Wave(LfoShape, f32, f32, bool),
    Steps(Vec<f32>, usize),
    Envelope([f32; 4], f32),
    Trace(Vec<f32>, bool),
    Level(f32),
}

fn paint_visual(b: Bounds<Pixels>, v: &Visual, color: u32, window: &mut Window) {
    let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let at = |x: f32, y: f32| point(px(x0 + x), px(y0 + y));
    // -1 to 1 (or 0 to 1) to a height, with room round it.
    let y_of = |v: f32, bipolar: bool| {
        let u = if bipolar { (v + 1.0) / 2.0 } else { v };
        6.0 + (1.0 - u.clamp(0.0, 1.0)) * (h - 12.0)
    };
    let line = |pts: &[(f32, f32)], width: f32, ink: Hsla, window: &mut Window| {
        let mut p = PathBuilder::stroke(px(width));
        for (i, &(x, y)) in pts.iter().enumerate() {
            match i {
                0 => p.move_to(at(x, y)),
                _ => p.line_to(at(x, y)),
            }
        }
        if let Ok(p) = p.build() {
            window.paint_path(p, ink);
        }
    };
    let area = |pts: &[(f32, f32)], base: f32, ink: Hsla, window: &mut Window| {
        if pts.len() < 2 {
            return;
        }
        let mut p = PathBuilder::fill();
        p.move_to(at(pts[0].0, base));
        for &(x, y) in pts {
            p.line_to(at(x, y));
        }
        p.line_to(at(pts[pts.len() - 1].0, base));
        p.close();
        if let Ok(p) = p.build() {
            window.paint_path(p, ink);
        }
    };
    let dot = |x: f32, y: f32, window: &mut Window| {
        window.paint_quad(
            fill(
                Bounds::centered_at(at(x, y), size(px(8.), px(8.))),
                rgb(0xffffff),
            )
            .corner_radii(px(4.)),
        );
        window.paint_quad(
            fill(
                Bounds::centered_at(at(x, y), size(px(5.), px(5.))),
                rgb(color),
            )
            .corner_radii(px(2.5)),
        );
    };
    // The middle, faint.
    window.paint_quad(fill(
        Bounds::new(at(0.0, h / 2.0), size(px(w), px(1.))),
        alpha(0xffffff, 0.05),
    ));
    match v {
        Visual::Wave(shape, phase, now, unipolar) => {
            // Two cycles across, as it plays; the dot where it is in the first.
            let n = 96;
            let pts: Vec<(f32, f32)> = (0..=n)
                .map(|i| {
                    let x = i as f32 / n as f32;
                    let s = shape.at((x * 2.0 + phase) as f64);
                    let s = if *unipolar { 0.5 + 0.5 * s } else { s };
                    (x * w, y_of(s, !unipolar))
                })
                .collect();
            area(&pts, y_of(0.0, !unipolar), alpha(color, 0.12), window);
            line(&pts, 2.0, rgb(color).into(), window);
            let x = ((*now - phase).rem_euclid(1.0)) / 2.0;
            let s = shape.at((x * 2.0 + phase) as f64);
            let s = if *unipolar { 0.5 + 0.5 * s } else { s };
            window.paint_quad(fill(
                Bounds::new(at(x * w, 0.0), size(px(1.), px(h))),
                alpha(color, 0.35),
            ));
            dot(x * w, y_of(s, !unipolar), window);
        }
        Visual::Steps(values, now) => {
            let n = values.len().max(1);
            let cell = w / n as f32;
            for (i, &v) in values.iter().enumerate() {
                let top = y_of(v, false);
                let lit = i == *now;
                window.paint_quad(
                    fill(
                        Bounds::new(
                            at(i as f32 * cell + 1.0, top),
                            size(px(cell - 2.0), px(h - 6.0 - top)),
                        ),
                        if lit {
                            rgb(color).into()
                        } else {
                            alpha(color, 0.45)
                        },
                    )
                    .corner_radii(px(2.)),
                );
            }
        }
        Visual::Envelope([a, d, s, r], level) => {
            // Its shape, each stage as wide as it's long (on a log scale), and its level now.
            let span = |ms: f32| (ms.max(1.0)).ln() + 1.0;
            let total = span(*a) + span(*d) + span(1000.0) * 0.6 + span(*r);
            let k = w / total;
            let xa = span(*a) * k;
            let xd = xa + span(*d) * k;
            let xs = xd + span(1000.0) * 0.6 * k;
            let pts = [
                (0.0, y_of(0.0, false)),
                (xa, y_of(1.0, false)),
                (xd, y_of(*s, false)),
                (xs, y_of(*s, false)),
                (w, y_of(0.0, false)),
            ];
            area(&pts, y_of(0.0, false), alpha(color, 0.12), window);
            line(&pts, 2.0, rgb(color).into(), window);
            let y = y_of(*level, false);
            window.paint_quad(fill(
                Bounds::new(at(0.0, y), size(px(w), px(1.))),
                alpha(color, 0.5),
            ));
            dot(w - 8.0, y, window);
        }
        Visual::Trace(values, bipolar) => {
            let n = TRACE.max(2);
            let pts: Vec<(f32, f32)> = values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let x = w - (values.len() - 1 - i) as f32 * (w / (n - 1) as f32);
                    (x, y_of(v, *bipolar))
                })
                .collect();
            area(&pts, y_of(0.0, *bipolar), alpha(color, 0.18), window);
            line(&pts, 2.0, rgb(color).into(), window);
            if let Some(&(x, y)) = pts.last() {
                dot(x - 4.0, y, window);
            }
        }
        Visual::Level(v) => {
            let wv = w * v.clamp(0.0, 1.0);
            window.paint_quad(
                fill(
                    Bounds::new(at(0.0, h / 2.0 - 6.0), size(px(wv), px(12.))),
                    alpha(color, 0.6),
                )
                .corner_radii(px(3.)),
            );
            dot(wv.max(4.0), h / 2.0, window);
        }
    }
}

/// A knob's rings: each modulator's range just outside its arc, a step further out each, and a
/// dot where it has it now; a halo while a modulator is offered to it.
fn paint_rings(b: Bounds<Pixels>, rings: &[Ring], offering: bool, window: &mut Window) {
    let c = b.center();
    let base = f32::from(b.size.width.min(b.size.height)) / 2.0;
    let at = |deg: f32, r: f32| {
        let a = deg.to_radians();
        point(c.x + px(r * a.cos()), c.y + px(r * a.sin()))
    };
    if offering {
        window.paint_quad(
            fill(
                Bounds::centered_at(c, size(px(base * 2.0 + 8.0), px(base * 2.0 + 8.0))),
                alpha(ACCENT, 0.12),
            )
            .corner_radii(px(base + 4.0))
            .border_widths(px(1.))
            .border_color(alpha(ACCENT, 0.8)),
        );
    }
    for (k, ring) in rings.iter().enumerate() {
        let r = base + 3.0 + 3.0 * k as f32;
        let (from, to) = (angle(ring.lo), angle(ring.hi));
        if to - from > 0.5 {
            let mut p = PathBuilder::stroke(px(2.));
            let steps = ((to - from) / 6.0).ceil().max(2.0) as usize;
            for i in 0..=steps {
                let d = from + (to - from) * i as f32 / steps as f32;
                match i {
                    0 => p.move_to(at(d, r)),
                    _ => p.line_to(at(d, r)),
                }
            }
            if let Ok(p) = p.build() {
                window.paint_path(p, alpha(ring.color, 0.75));
            }
        }
        window.paint_quad(
            fill(
                Bounds::centered_at(at(angle(ring.live), r), size(px(5.), px(5.))),
                rgb(ring.color),
            )
            .corner_radii(px(2.5)),
        );
    }
}
