//! The channel strip's knobs and sliders: their targets, values, and starting to turn one.

use super::*;

/// What a vertical drag turns: a knob, as a value from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    /// A parameter of a track's instrument plugin.
    Param {
        track: u64,
        param: u32,
    },
    /// A channel's EQ's first low cut.
    LowCut(Channel),
    /// A quick EQ knob: a band of the channel's EQ, found by what it is.
    Quick(Channel, crate::mix::eq::Quick),
    Comp(Channel, CompField),
    Sat(Channel, SatField),
    Limit(Channel, LimitField),
    /// A knob of one of a channel's stacked built-ins, by its place among them.
    Stacked(Channel, usize, Knob),
    /// One of a track's synth's parameters, by its place.
    Poly {
        track: usize,
        param: usize,
    },
    /// A setting of a track's built-in sampler (by track index).
    Sampler {
        track: usize,
        field: crate::editors::sampler::Field,
    },
}

/// A compressor's, saturator's or limiter's knob, on one stacked on a channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Knob {
    Comp(CompField),
    Sat(SatField),
    Limit(LimitField),
    Reverb(crate::mix::strip::ReverbField),
    Delay(crate::mix::strip::DelayField),
    /// Knob `i` of a tool's table (`mix::strip::knobs`).
    Param(usize),
}

impl Knob {
    /// Its place in its effect's table: what automation and modulators move it by.
    pub fn table(self) -> Option<usize> {
        use crate::mix::strip::DelayField as D;
        use pina_engine::song::project::SAT_BAND_KNOBS;
        Some(match self {
            Knob::Param(i) => i,
            Knob::Comp(f) => f as usize,
            Knob::Reverb(f) => f as usize,
            Knob::Limit(f) => f.param()? as usize,
            Knob::Sat(SatField::Drive) => 0,
            Knob::Sat(SatField::Tone) => 1,
            Knob::Sat(SatField::Mix) => 6 * SAT_BAND_KNOBS,
            Knob::Sat(SatField::Output) => 6 * SAT_BAND_KNOBS + 1,
            Knob::Sat(SatField::Band(b, k)) => b as usize * SAT_BAND_KNOBS + k as usize,
            Knob::Delay(f) => match f {
                D::Time(s) => s.min(1) as usize,
                D::Feedback => 2,
                D::Cross => 3,
                D::LowCut => 4,
                D::HighCut => 5,
                D::Drive => 6,
                D::Wow => 7,
                D::Flutter => 8,
                D::Diffusion => 9,
                D::Ducking => 10,
                D::Width => 11,
                D::Mix => 12,
                D::Output => 13,
            },
        })
    }
}

/// A saturator's knob: drive and tone for every band at once, its mix and output, or one
/// band's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SatField {
    Drive,
    Tone,
    Mix,
    Output,
    Band(u8, SatKnob),
}

/// A knob of a saturator's band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SatKnob {
    Drive,
    Tone,
    Dynamics,
    Feedback,
    FeedbackHz,
    Mix,
    Level,
}

/// A limiter's knob.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LimitField {
    Gain,
    Ceiling,
    Release,
    Lookahead,
    Attack,
    Link,
    Clip,
}

impl LimitField {
    /// The automation it writes, if it's one automation moves (not the lookahead or attack,
    /// whose moves would move the channel's delay; nor the link).
    pub fn param(self) -> Option<pina_engine::song::project::LimitParam> {
        use pina_engine::song::project::LimitParam;
        Some(match self {
            LimitField::Gain => LimitParam::Gain,
            LimitField::Ceiling => LimitParam::Ceiling,
            LimitField::Release => LimitParam::Release,
            LimitField::Clip => LimitParam::Clip,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompField {
    Threshold,
    Ratio,
    Attack,
    Release,
    Makeup,
    Mix,
    Sidechain,
    Spectral,
}

/// What a sideways drag sets: a slider, as a value from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slider {
    Strength,
    Swing,
    Zoom,
    Send {
        track: usize,
        index: usize,
    },
    /// How much one aspect of a sound counts in the sample browser's Similar list.
    Aspect(usize),
    /// A binaural track's sound: the way round, how high, how far.
    Azimuth(usize),
    Elevation(usize),
    Distance(usize),
}

impl Workspace {
    /// A channel's built-in EQ and compressor.
    pub fn strip_dsp(&self, ch: Channel) -> (Option<Eq>, Option<Comp>) {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => (p.tracks[i].eq, p.tracks[i].comp),
            Channel::Aux(i) => (p.auxes[i].eq, p.auxes[i].comp),
            Channel::Master => (p.master_eq, p.master_comp),
        }
    }

    pub fn strip_dsp_mut(&mut self, ch: Channel) -> (&mut Option<Eq>, &mut Option<Comp>) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => {
                let t = &mut p.tracks[i];
                (&mut t.eq, &mut t.comp)
            }
            Channel::Aux(i) => {
                let a = &mut p.auxes[i];
                (&mut a.eq, &mut a.comp)
            }
            Channel::Master => (&mut p.master_eq, &mut p.master_comp),
        }
    }

    /// A knob's value now, from 0 to 1.
    pub fn target(&self, target: Target) -> f32 {
        use crate::mix::strip::{eq_gain_pos, low_cut_pos};
        match target {
            Target::Param { track, param } => {
                self.player
                    .param_value(track, pina_engine::audio::player::Which::Instrument, param)
                    as f32
            }
            Target::LowCut(ch) => low_cut_pos(
                self.strip_dsp(ch)
                    .0
                    .map_or(0.0, |e| crate::mix::eq::low_cut(&e)),
            ),
            Target::Quick(ch, q) => eq_gain_pos(self.strip_dsp(ch).0.map_or(0.0, |e| q.gain(&e))),
            Target::Comp(ch, field) => {
                crate::mix::strip::comp_pos(field, &self.built_in(ch, None).unwrap_or_default())
            }
            Target::Sat(ch, field) => {
                crate::mix::strip::sat_pos(field, &self.built_in(ch, None).unwrap_or_default())
            }
            Target::Limit(ch, field) => {
                crate::mix::strip::limit_pos(field, &self.limiter(ch).unwrap_or_default())
            }
            Target::Stacked(ch, k, knob) => {
                use crate::mix::strip::{comp_pos, limit_pos, sat_pos};
                let at = Some(k);
                match knob {
                    Knob::Comp(f) => comp_pos(f, &self.built_in(ch, at).unwrap_or_default()),
                    Knob::Sat(f) => sat_pos(f, &self.built_in(ch, at).unwrap_or_default()),
                    Knob::Limit(f) => limit_pos(f, &self.built_in(ch, at).unwrap_or_default()),
                    Knob::Reverb(f) => {
                        crate::mix::strip::reverb_pos(f, &self.built_in(ch, at).unwrap_or_default())
                    }
                    Knob::Delay(f) => {
                        crate::mix::strip::delay_pos(f, &self.built_in(ch, at).unwrap_or_default())
                    }
                    Knob::Param(i) => self.tool_heard(ch, k).map_or(0.0, |u| {
                        crate::mix::strip::knobs(u.kind())
                            .get(i)
                            .map_or(0.0, |p| (p.pos)(&u))
                    }),
                }
            }
            Target::Poly { track, param } => self.now_value(track, Param::Poly(param as u16)),
            Target::Sampler { track, field } => self.sampler_pos(track, field),
        }
    }

    /// Turns a knob to `v` (0 to 1); a channel's EQ or compressor appears if it had none.
    pub fn set_target(&mut self, target: Target, v: f32) {
        use crate::mix::strip::{comp_set, eq_gain_db, low_cut_hz};
        match target {
            // In the project, so it's undone like any edit; the player passes it on.
            Target::Param { track, param } => {
                let tracks = &mut self.session.project.tracks;
                if let Some(r) = tracks
                    .iter_mut()
                    .find(|t| t.id == track)
                    .and_then(|t| t.plugin_mut(Which::Instrument))
                {
                    r.params.insert(param, v as f64);
                }
            }
            Target::LowCut(ch) => crate::mix::eq::set_low_cut(
                self.strip_dsp_mut(ch).0.get_or_insert_default(),
                low_cut_hz(v),
            ),
            Target::Quick(ch, q) => {
                let eq = self.strip_dsp_mut(ch).0.get_or_insert_default();
                if let Some(i) = q.band(eq) {
                    eq.bands[i].gain_db = eq_gain_db(v);
                }
            }
            Target::Comp(ch, field) => {
                comp_set(field, self.strip_dsp_mut(ch).1.get_or_insert_default(), v)
            }
            Target::Poly { track, param } => {
                let q = &pina_engine::song::project::POLY_PARAMS[param];
                let value = match q.reads {
                    pina_engine::song::project::Reads::Whole => q.scale.value(v).round(),
                    _ => q.scale.value(v),
                };
                if let Some(p) = self.poly_mut(track) {
                    p.params[param] = value;
                }
            }
            Target::Sampler { track, field } => self.sampler_set(track, field, v),
            Target::Sat(ch, field) => {
                crate::mix::strip::sat_set(field, self.sat_mut(ch).get_or_insert_default(), v)
            }
            Target::Limit(ch, field) => {
                crate::mix::strip::limit_set(field, self.limiter_mut(ch).get_or_insert_default(), v)
            }
            Target::Stacked(ch, k, knob) => {
                use crate::mix::strip::{limit_set, sat_set};
                let at = Some(k);
                match knob {
                    Knob::Comp(f) => drop(self.unit_mut(ch, at).map(|c| comp_set(f, c, v))),
                    Knob::Sat(f) => drop(self.unit_mut(ch, at).map(|s| sat_set(f, s, v))),
                    Knob::Limit(f) => drop(self.unit_mut(ch, at).map(|l| limit_set(f, l, v))),
                    Knob::Reverb(f) => drop(
                        self.unit_mut(ch, at)
                            .map(|r| crate::mix::strip::reverb_set(f, r, v)),
                    ),
                    Knob::Delay(f) => drop(
                        self.unit_mut(ch, at)
                            .map(|d| crate::mix::strip::delay_set(f, d, v)),
                    ),
                    Knob::Param(i) => {
                        if let Some(u) = self.chain_mut(ch).more.get_mut(k)
                            && let Some(p) = crate::mix::strip::knobs(u.kind()).get(i)
                        {
                            (p.set)(u, v);
                        }
                    }
                }
            }
        }
    }

    pub fn slider(&self, slider: Slider) -> f32 {
        match slider {
            Slider::Strength => self.roll.strength,
            Slider::Swing => (self.roll.swing - 50.0) / 25.0,
            Slider::Zoom => (self.px_per_beat / 4.0).ln() / 200f32.ln(),
            Slider::Send { track, index } => {
                let bus = self.session.project.tracks[track].sends[index].bus;
                strip::fader_pos(self.now_value(track, Param::Send(bus)))
            }
            Slider::Aspect(i) => {
                *crate::sounds::browser::aspect(&mut self.browser.aspects.clone(), i)
            }
            Slider::Azimuth(t) | Slider::Elevation(t) | Slider::Distance(t) => {
                let Some(place) = self.place_of(t) else {
                    return 0.0;
                };
                match slider {
                    Slider::Azimuth(_) => (place.azimuth / 180.0 + 1.0) / 2.0,
                    Slider::Elevation(_) => (place.elevation + 90.0) / 180.0,
                    _ => (place.distance - Place::NEAREST) / (Place::FARTHEST - Place::NEAREST),
                }
            }
        }
    }

    pub fn set_slider(&mut self, slider: Slider, v: f32) {
        match slider {
            Slider::Strength => self.roll.strength = (v * 100.0).round() / 100.0,
            Slider::Swing => self.roll.swing = (50.0 + v * 25.0).round(),
            Slider::Zoom => {
                let width = f32::from(self.lanes_bounds.get().size.width);
                self.zoom_around(4.0 * 200f32.powf(v), width / 2.0)
            }
            Slider::Send { track, index } => {
                self.session.project.tracks[track].sends[index].level_db = strip::fader_db(v)
            }
            Slider::Aspect(i) => {
                *crate::sounds::browser::aspect(&mut self.browser.aspects, i) =
                    (v * 20.0).round() / 20.0;
                self.find_similar();
            }
            Slider::Azimuth(t) | Slider::Elevation(t) | Slider::Distance(t) => {
                let Some(mut place) = self.place_of(t) else {
                    return;
                };
                match slider {
                    Slider::Azimuth(_) => place.azimuth = ((v * 2.0 - 1.0) * 180.0).round(),
                    Slider::Elevation(_) => place.elevation = (v * 180.0 - 90.0).round(),
                    _ => {
                        let d = Place::NEAREST + v * (Place::FARTHEST - Place::NEAREST);
                        place.distance = (d * 20.0).round() / 20.0;
                    }
                }
                self.set_place(t, place);
            }
        }
    }

    /// Starts dragging a slider, setting it to where it was clicked.
    pub fn start_slider(&mut self, slider: Slider, p: Point<f32>, cx: &mut Context<Self>) {
        let Some(b) = self.slider_bounds.borrow().get(&slider).copied() else {
            return;
        };
        let (left, width) = (f32::from(b.origin.x), f32::from(b.size.width));
        self.begin_drag(
            Drag::Slider {
                slider,
                left,
                width,
            },
            cx,
        );
        self.drag_to(p, cx);
    }
}
