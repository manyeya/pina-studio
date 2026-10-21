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
    /// A setting of a track's built-in sampler (by track index).
    Sampler {
        track: usize,
        field: crate::editors::sampler::Field,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SatField {
    Drive,
    Tone,
    Mix,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompField {
    Threshold,
    Ratio,
    Attack,
    Release,
    Makeup,
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
                crate::mix::strip::comp_pos(field, &self.strip_dsp(ch).1.unwrap_or_default())
            }
            Target::Sat(ch, field) => {
                crate::mix::strip::sat_pos(field, &self.sat(ch).unwrap_or_default())
            }
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
            Target::Sampler { track, field } => self.sampler_set(track, field, v),
            Target::Sat(ch, field) => {
                crate::mix::strip::sat_set(field, self.sat_mut(ch).get_or_insert_default(), v)
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
