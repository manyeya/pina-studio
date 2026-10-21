//! A parameter's value: its own, where its automation has it now, as text, and on its lane's scale.

use super::*;

/// Where a value sits in a lane `h` tall, from its top: the fader's and sends' on the fader's
/// scale, pan with left at the top, mute low when muted, a plugin's with 1 at the top.
pub fn to_y(param: Param, value: f32, h: f32) -> f32 {
    let t = match param {
        Param::Volume | Param::Send(_) => 1.0 - strip::fader_pos(value),
        Param::Pan => (value.clamp(-1.0, 1.0) + 1.0) / 2.0,
        Param::Mute => value.clamp(0.0, 1.0),
        Param::Plugin(..) => 1.0 - value.clamp(0.0, 1.0),
        Param::Eq(..) | Param::Mod(..) => 1.0 - param.to_unit(value),
    };
    PAD + t * (h - 2.0 * PAD)
}

pub fn from_y(param: Param, y: f32, h: f32) -> f32 {
    let t = ((y - PAD) / (h - 2.0 * PAD)).clamp(0.0, 1.0);
    match param {
        Param::Volume | Param::Send(_) => strip::fader_db(1.0 - t),
        Param::Pan => t * 2.0 - 1.0,
        Param::Mute => (t > 0.5) as u8 as f32,
        Param::Plugin(..) => 1.0 - t,
        Param::Eq(..) | Param::Mod(..) => param.from_unit(1.0 - t),
    }
}

/// How each mode shows: Read green, Touch amber, Latch violet, Trim teal, Write red, Off grey.
pub fn mode_color(mode: AutoMode) -> u32 {
    match mode {
        AutoMode::Off => TEXT_3,
        AutoMode::Read => METER_LOW,
        AutoMode::Touch => AMBER,
        AutoMode::Latch => VIOLET,
        AutoMode::Trim => TEAL,
        AutoMode::Write => RECORD,
    }
}

/// What a mode does, for its menu.
pub fn mode_what(mode: AutoMode) -> &'static str {
    match mode {
        AutoMode::Off => "Ignored",
        AutoMode::Read => "Plays it",
        AutoMode::Touch => "Records while held",
        AutoMode::Latch => "Records until stop",
        AutoMode::Trim => "Moves the line while held",
        AutoMode::Write => "Records all, from play",
    }
}

/// A parameter's units, for whether points copied from one can go on another.
pub(crate) fn units(param: Param) -> u8 {
    match param {
        Param::Volume | Param::Send(_) => 0,
        Param::Pan => 1,
        Param::Mute => 2,
        Param::Plugin(..) => 3,
        Param::Eq(_, field) => 4 + field as u8,
        Param::Mod(_, setting) => 7 + setting as u8,
    }
}

/// A value moved by `by`: on the fader's scale for levels, so a trim sounds the same at any
/// level.
pub(crate) fn shifted(param: Param, value: f32, by: f32) -> f32 {
    match param {
        Param::Volume | Param::Send(_) => {
            strip::fader_db((strip::fader_pos(value) + by).clamp(0.0, 1.0))
        }
        Param::Pan => (value + by).clamp(-1.0, 1.0),
        Param::Mute => value,
        Param::Plugin(..) => (value + by).clamp(0.0, 1.0),
        // An EQ band's on its own scale: octaves for its frequency and width; a modulator's
        // times in octaves too.
        Param::Eq(..) | Param::Mod(..) => {
            param.from_unit((param.to_unit(value) + by).clamp(0.0, 1.0))
        }
    }
}

/// How far `from` is from `to`, as `shifted` moves it.
pub(crate) fn apart(param: Param, from: f32, to: f32) -> f32 {
    match param {
        Param::Volume | Param::Send(_) => strip::fader_pos(to) - strip::fader_pos(from),
        Param::Eq(..) | Param::Mod(..) => param.to_unit(to) - param.to_unit(from),
        _ => to - from,
    }
}

impl Workspace {
    /// A control's own value: where its fader, knob or button is set.
    pub fn own_value(&self, track: usize, param: Param) -> f32 {
        let t = &self.session.project.tracks[track];
        match param {
            Param::Volume => t.volume_db,
            Param::Pan => t.pan,
            Param::Mute => t.mute as u8 as f32,
            Param::Send(bus) => t
                .sends
                .iter()
                .find(|s| s.bus == bus)
                .map_or(SILENT_DB, |s| s.level_db),
            Param::Plugin(which, id) => t
                .plugin(which)
                .and_then(|r| r.params.get(&id).copied())
                .unwrap_or_else(|| self.player.param_value(t.id, which, id))
                as f32,
            Param::Eq(band, field) => {
                let b =
                    t.eq.and_then(|e| e.bands.get(band as usize).copied())
                        .unwrap_or_default();
                match field {
                    EqField::Freq => b.freq,
                    EqField::Gain => b.gain_db,
                    EqField::Q => b.q,
                }
            }
            Param::Mod(k, setting) => self
                .session
                .project
                .mod_kind(track, k as usize)
                .and_then(|kind| setting.get(kind))
                .unwrap_or(0.0),
        }
    }

    pub(crate) fn set_own(&mut self, track: usize, param: Param, value: f32) {
        let t = &mut self.session.project.tracks[track];
        match param {
            Param::Volume => t.volume_db = value,
            Param::Pan => t.pan = value,
            Param::Mute => t.mute = value > 0.5,
            Param::Send(bus) => {
                if let Some(s) = t.sends.iter_mut().find(|s| s.bus == bus) {
                    s.level_db = value;
                }
            }
            Param::Plugin(which, id) => {
                if let Some(r) = t.plugin_mut(which) {
                    r.params.insert(id, value as f64);
                }
            }
            Param::Eq(band, field) => {
                if let Some(b) = t.eq.as_mut().and_then(|e| e.bands.get_mut(band as usize)) {
                    match field {
                        EqField::Freq => b.freq = value,
                        EqField::Gain => b.gain_db = value,
                        EqField::Q => b.q = value,
                    }
                }
            }
            Param::Mod(k, setting) => {
                if let Some(kind) = self.session.project.mod_kind_mut(track, k as usize) {
                    setting.set(kind, value);
                }
            }
        }
    }

    /// A track parameter's value at the playhead, while its automation plays it (and no hand
    /// has taken over from it).
    pub fn automated(&self, track: usize, param: Param) -> Option<f32> {
        let t = self.session.project.tracks.get(track)?;
        if t.automation.mode == AutoMode::Off || self.taken.contains_key(&(t.id, param)) {
            return None;
        }
        value_at(t.automation.points(param), self.playhead() as f64)
    }

    /// What a control shows: where its automation has it, else its own value.
    pub fn now_value(&self, track: usize, param: Param) -> f32 {
        self.automated(track, param)
            .unwrap_or_else(|| self.own_value(track, param))
    }

    /// Whether a track sounds muted now: by its button, or its automation.
    pub fn muted_now(&self, track: usize) -> bool {
        self.now_value(track, Param::Mute) > 0.5
    }

    /// The colour a control is marked with while its automation plays it: its mode's.
    pub fn auto_mark(&self, track: usize, param: Param) -> Option<u32> {
        let t = self.session.project.tracks.get(track)?;
        let a = &t.automation;
        (a.mode != AutoMode::Off && !a.points(param).is_empty()).then(|| mode_color(a.mode))
    }

    /// A value as its control shows it: dB, L/C/R, muted or not, or as its plugin says.
    pub fn value_text(&self, track: usize, param: Param, value: f32) -> String {
        match param {
            Param::Volume | Param::Send(_) => format!("{} dB", db_text(value)),
            Param::Pan => strip::pan_label(value),
            Param::Mute => if value > 0.5 { "Muted" } else { "On" }.into(),
            Param::Eq(_, EqField::Freq) => crate::mix::eq::hz_text(value),
            Param::Eq(_, EqField::Gain) => format!("{} dB", db_text(value)),
            Param::Eq(_, EqField::Q) => format!("Q {value:.2}"),
            Param::Mod(_, setting) => {
                use pina_engine::dsp::modulate::ModParam;
                match setting {
                    ModParam::Attack | ModParam::Decay | ModParam::Release if value >= 1000.0 => {
                        format!("{:.1} s", value / 1000.0)
                    }
                    ModParam::Attack | ModParam::Decay | ModParam::Release => {
                        format!("{value:.0} ms")
                    }
                    ModParam::Phase => format!("{:.0}°", value * 360.0),
                    _ => format!("{:.0}%", value * 100.0),
                }
            }
            Param::Plugin(which, id) => {
                let id_t = self.session.project.tracks[track].id;
                let text = self.player.param_text(id_t, which, id, value as f64);
                match text.is_empty() {
                    true => format!("{:.0}%", value * 100.0),
                    false => text,
                }
            }
        }
    }
}
