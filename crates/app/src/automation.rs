//! Automation, as Logic's and then some. Every control shows where its automation has it; moving
//! one records it (Touch, Latch, Write), moves the line up or down (Trim, or any time the song is
//! stopped), or takes over from it while held (Read). Under each track are its lanes, a
//! parameter each, to draw on: points to drag, lines to bend or step, shapes to draw in, a
//! marquee to select with, and points to copy and paste.

use gpui::{
    AnyElement, Bounds, Context, Div, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point,
    canvas, div, fill, linear_color_stop, linear_gradient, point, prelude::*, px, rgb, size,
};
use pina_engine::project::{
    AutoClip, AutoMode, AutoPoint, AutoShape, EqField, PPQ, Param, Project, SILENT_DB, Which,
    value_at,
};

use crate::{
    menu::{Menu, MenuAction},
    strip,
    theme::*,
    workspace::{Drag, Tool, Workspace},
};

/// Space kept free above and below a lane's line.
const PAD: f32 = 7.0;

/// How long a control let go takes to go back to its line (Touch, Trim): a sixteenth.
const RAMP: u64 = pina_engine::project::PPQ / 4;

/// How close (pixels) the pointer has to be to a line to take hold of it.
const NEAR: f32 = 6.0;

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
fn units(param: Param) -> u8 {
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
fn shifted(param: Param, value: f32, by: f32) -> f32 {
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
fn apart(param: Param, from: f32, to: f32) -> f32 {
    match param {
        Param::Volume | Param::Send(_) => strip::fader_pos(to) - strip::fader_pos(from),
        Param::Eq(..) | Param::Mod(..) => param.to_unit(to) - param.to_unit(from),
        _ => to - from,
    }
}

/// A control taken over from its automation, and until when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Live {
    /// While a hand has it, or while it records.
    Held,
    /// Until playback starts: moved while stopped, in "until you play" mode.
    UntilPlay,
}

/// A control moved over its automation by Trim, or while stopped: the line as it was, where the
/// control was taken hold of, and how far it has been moved since, when.
#[derive(Clone, Debug)]
pub struct Touch {
    snapshot: Vec<AutoPoint>,
    base: f32,
    /// Where a Trim pass began, while playing; None moves the whole line.
    from: Option<u64>,
    by: Vec<(u64, f32)>,
    held: bool,
}

/// What moving an automated control does when the song is stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stopped {
    /// Moves its whole line up or down, keeping its shape.
    #[default]
    Trim,
    /// Sets the line at the playhead: a point there.
    Point,
    /// Changes it until playback starts, when the automation takes over again.
    UntilPlay,
}

impl Stopped {
    pub const ALL: [Stopped; 3] = [Stopped::Trim, Stopped::Point, Stopped::UntilPlay];

    pub fn name(self) -> &'static str {
        match self {
            Stopped::Trim => "Moves the line",
            Stopped::Point => "Sets a point",
            Stopped::UntilPlay => "Until play",
        }
    }
}

/// Selected points: a lane's, by index.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Picked {
    pub track: u64,
    pub param: Param,
    pub points: Vec<usize>,
}

/// What an automation menu item does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AutoOp {
    /// The lanes: a parameter for one, another under them, one taken away.
    Lane(usize, Param),
    AddLane,
    RemoveLane(usize),
    Mode(AutoMode),
    /// The picked points' lines.
    Straight,
    SlowStart,
    FastStart,
    /// An S: eased out of the point and into the next.
    SCurve,
    Step,
    /// The selection's line (or the lane's) smoothed, over about so many ticks.
    Smooth(u64),
    Delete,
    Copy,
    /// Points copied, at a tick.
    Paste(u64),
    SelectAll,
    Thin,
    Clear,
    /// A shape from tick to tick, a cycle every so many.
    Shape(AutoShape, u64, u64, u64),
    /// A setting automated from now on: its lane shown, a point where it is if it has none.
    Automate(Param),
}

impl Workspace {
    // ---------------------------------------------------------------------------------------
    // Lanes

    /// How tall each automation lane is.
    pub fn auto_h(&self) -> f32 {
        self.settings.auto_height
    }

    /// The parameters a track's lanes show, top to bottom: those opened, else its first
    /// automated one, else its volume.
    pub fn auto_lanes(&self, track: usize) -> Vec<Param> {
        let t = &self.session.project.tracks[track];
        match self.auto_lanes.get(&t.id) {
            Some(lanes) if !lanes.is_empty() => lanes.clone(),
            _ => vec![t.automation.params().first().copied().unwrap_or_default()],
        }
    }

    /// The parameter a track's first lane shows.
    pub fn lane_param(&self, track: usize) -> Param {
        self.auto_lanes(track)[0]
    }

    /// How much of a track's row its lanes take, while they show.
    pub fn autos_h(&self, track: usize) -> f32 {
        match self.show_automation {
            true => self.auto_lanes(track).len() as f32 * self.auto_h(),
            false => 0.0,
        }
    }

    /// A lane's top, in the lanes.
    pub fn lane_top(&self, track: usize, k: usize) -> f32 {
        self.row_top(track) + self.lane_h(track) + k as f32 * self.auto_h()
    }

    /// Shows `param` in a track's `k`th lane (or a new one under them); if it showed in another,
    /// they swap.
    fn set_lane(&mut self, track: usize, k: usize, param: Param) {
        let mut lanes = self.auto_lanes(track);
        match lanes.iter().position(|&p| p == param) {
            Some(j) if k < lanes.len() => lanes.swap(j, k),
            Some(_) => {}
            None if k < lanes.len() => lanes[k] = param,
            None => lanes.push(param),
        }
        let id = self.session.project.tracks[track].id;
        self.auto_lanes.insert(id, lanes);
        self.show_automation = true;
    }

    /// Makes sure a parameter shows, as it records: in place of a first lane with nothing on
    /// it, else under the others.
    fn show_lane(&mut self, track: usize, param: Param) {
        let lanes = self.auto_lanes(track);
        if lanes.contains(&param) {
            return;
        }
        let t = &self.session.project.tracks[track];
        let empty = lanes.len() == 1 && t.automation.points(lanes[0]).is_empty();
        self.set_lane(track, if empty { 0 } else { lanes.len() }, param);
    }

    /// A lane more under a track's: the next automated parameter not showing, else the next of
    /// volume, pan, mute and its sends.
    fn add_lane(&mut self, track: usize) {
        let lanes = self.auto_lanes(track);
        let t = &self.session.project.tracks[track];
        let next = t
            .automation
            .params()
            .into_iter()
            .chain([Param::Volume, Param::Pan, Param::Mute])
            .chain(t.sends.iter().map(|s| Param::Send(s.bus)))
            .find(|p| !lanes.contains(p));
        let param = next.unwrap_or(Param::Volume);
        self.set_lane(track, lanes.len(), param);
    }

    fn remove_lane(&mut self, track: usize, k: usize) {
        let mut lanes = self.auto_lanes(track);
        if lanes.len() > 1 && k < lanes.len() {
            lanes.remove(k);
        }
        let id = self.session.project.tracks[track].id;
        self.auto_lanes.insert(id, lanes);
    }

    /// What a track's lanes can show: volume, pan, mute, its sends, and every parameter of its
    /// plugins, those with automation marked; picking one shows it in lane `k`.
    pub fn auto_params(&self, track: usize, k: usize) -> Vec<(String, MenuAction)> {
        let t = &self.session.project.tracks[track];
        let id = t.id;
        let mark = |p: Param| match t.automation.points(p).is_empty() {
            true => "",
            false => "● ",
        };
        let item = |p: Param, label: String| {
            (
                format!("{}{label}", mark(p)),
                MenuAction::Auto(id, AutoOp::Lane(k, p)),
            )
        };
        let mut items: Vec<(String, MenuAction)> = [Param::Volume, Param::Pan, Param::Mute]
            .into_iter()
            .map(|p| item(p, self.param_name(track, p)))
            .collect();
        for s in &t.sends {
            let p = Param::Send(s.bus);
            items.push(item(p, self.param_name(track, p)));
        }
        let plugins = std::iter::once(Which::Instrument)
            .chain((0..t.inserts.len()).map(Which::Insert))
            .filter(|&w| t.plugin(w).is_some());
        for which in plugins {
            let name = self.plugin_name(track, which);
            items.push((name.clone(), MenuAction::Nothing));
            for p in self.player.params(id, which) {
                let param = Param::Plugin(which, p.id);
                items.push(item(param, format!("{name}: {}", p.name)));
            }
        }
        // Its modulators' settings: a macro turned, an LFO's phase, an envelope's times.
        let settings: Vec<Param> = t
            .modulators
            .iter()
            .enumerate()
            .flat_map(|(k, m)| {
                pina_engine::modulate::ModParam::of(&m.kind)
                    .into_iter()
                    .map(move |s| Param::Mod(k as u8, s))
            })
            .collect();
        if !settings.is_empty() {
            items.push(("Modulators".into(), MenuAction::Nothing));
            items.extend(
                settings
                    .into_iter()
                    .map(|p| item(p, self.param_name(track, p))),
            );
        }
        // The EQ's bands: each one's frequency, gain (if it has one) and width.
        if let Some(eq) = t.eq.filter(|e| !e.bands.is_empty()) {
            items.push(("Channel EQ".into(), MenuAction::Nothing));
            for (b, band) in eq.bands.iter().enumerate() {
                let fields = [EqField::Freq, EqField::Gain, EqField::Q];
                for field in fields
                    .into_iter()
                    .filter(|&f| f != EqField::Gain || band.shape.has_gain())
                {
                    let p = Param::Eq(b as u8, field);
                    items.push(item(p, self.param_name(track, p)));
                }
            }
        }
        items
    }

    // ---------------------------------------------------------------------------------------
    // Values

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
            Param::Mod(k, setting) => t
                .modulators
                .get(k as usize)
                .and_then(|m| setting.get(&m.kind))
                .unwrap_or(0.0),
        }
    }

    fn set_own(&mut self, track: usize, param: Param, value: f32) {
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
                if let Some(m) = t.modulators.get_mut(k as usize) {
                    setting.set(&mut m.kind, value);
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
            Param::Eq(_, EqField::Freq) => crate::eq::hz_text(value),
            Param::Eq(_, EqField::Gain) => format!("{} dB", db_text(value)),
            Param::Eq(_, EqField::Q) => format!("Q {value:.2}"),
            Param::Mod(_, setting) => {
                use pina_engine::modulate::ModParam;
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

    /// The project the engine plays: controls a hand has taken over follow it, not their
    /// automation.
    pub fn without_live(&self, project: &mut Project) {
        for &(id, param) in self.taken.keys() {
            if let Some(t) = project.tracks.iter_mut().find(|t| t.id == id) {
                t.automation.points_mut(param).clear();
                t.automation.tidy();
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Controls moved by hand

    /// A control on a track has been moved by hand to `value` (and set there): what that does
    /// to its automation, by the track's mode, playing or not.
    pub fn moved(&mut self, track: usize, param: Param, value: f32) {
        let Some(t) = self.session.project.tracks.get(track) else {
            return;
        };
        let (key, mode) = ((t.id, param), t.automation.mode);
        let has = !t.automation.points(param).is_empty();
        if mode == AutoMode::Off {
            return;
        }
        if self.player.playing() {
            match mode {
                AutoMode::Read if has => drop(self.taken.insert(key, Live::Held)),
                AutoMode::Read => {}
                AutoMode::Trim if has && !param.steps() => self.trim(track, param, value, true),
                _ => {
                    self.record_auto(track, param, value);
                    self.taken.insert(key, Live::Held);
                }
            }
            return;
        }
        if !has {
            return;
        }
        match self.settings.stopped {
            _ if param.steps() => self.point_at_playhead(track, param, value),
            Stopped::Trim => self.trim(track, param, value, false),
            Stopped::Point => self.point_at_playhead(track, param, value),
            Stopped::UntilPlay => drop(self.taken.insert(key, Live::UntilPlay)),
        }
    }

    /// Moved and let go at once: a value typed, a reset, a button, a swipe.
    pub fn moved_once(&mut self, track: usize, param: Param, value: f32) {
        self.moved(track, param, value);
        self.let_go(track, param);
    }

    /// Moves the line by how far the control has come since it was taken hold of: all of it
    /// when stopped; while playing (Trim), from where it was taken hold of, as long as it's held.
    fn trim(&mut self, track: usize, param: Param, value: f32, playing: bool) {
        let tick = self.playhead();
        let t = &self.session.project.tracks[track];
        let key = (t.id, param);
        let points = t.automation.points(param);
        let touch = self.touches.entry(key).or_insert_with(|| Touch {
            snapshot: points.to_vec(),
            base: value_at(points, tick as f64).unwrap_or(value),
            from: playing.then_some(tick),
            by: Vec::new(),
            held: true,
        });
        let by = apart(param, touch.base, value);
        let at = if playing { tick } else { 0 };
        match touch.by.last_mut() {
            Some(last) if last.0 >= at => last.1 = by,
            _ => touch.by.push((at, by)),
        }
        touch.held = true;
        self.retouch(track, param);
        self.written.insert(key);
    }

    /// A touched lane, drawn again from its snapshot and how far it's been moved.
    fn retouch(&mut self, track: usize, param: Param) {
        let key = (self.session.project.tracks[track].id, param);
        let Some(touch) = self.touches.get(&key) else {
            return;
        };
        let snap = &touch.snapshot;
        let last = touch.by.last().map_or(0.0, |b| b.1);
        let by_at = |tick: u64| -> f32 {
            let i = touch.by.partition_point(|b| b.0 <= tick);
            match (i.checked_sub(1).map(|i| touch.by[i]), touch.by.get(i)) {
                (Some(a), Some(b)) => {
                    let f = (tick - a.0) as f32 / (b.0 - a.0).max(1) as f32;
                    a.1 + (b.1 - a.1) * f
                }
                (Some(a), None) => a.1,
                (None, Some(b)) => b.1,
                (None, None) => 0.0,
            }
        };
        let points: Vec<AutoPoint> = match touch.from {
            None => snap
                .iter()
                .map(|p| AutoPoint {
                    value: shifted(param, p.value, last),
                    ..*p
                })
                .collect(),
            Some(from) => {
                let end = match touch.held {
                    true => u64::MAX,
                    false => touch.by.last().map_or(from, |b| b.0),
                };
                let mut ticks: Vec<u64> = snap
                    .iter()
                    .map(|p| p.tick)
                    .filter(|&t| t >= from && t <= end)
                    .chain(touch.by.iter().map(|b| b.0))
                    .chain([from])
                    .chain((end != u64::MAX).then_some(end))
                    .collect();
                ticks.sort_unstable();
                ticks.dedup();
                let mut out: Vec<AutoPoint> =
                    snap.iter().filter(|p| p.tick < from).copied().collect();
                // As it was up to where it was taken hold of.
                if from > 0
                    && let Some(v) = value_at(snap, (from - 1) as f64)
                {
                    out.push(AutoPoint::new(from - 1, v));
                }
                for (n, &t) in ticks.iter().enumerate() {
                    let own = snap.iter().position(|p| p.tick == t);
                    // A point of the line keeps its shape on to the next, if that's next here.
                    let shape = own
                        .filter(|&j| snap.get(j + 1).map(|p| p.tick) == ticks.get(n + 1).copied());
                    let v = value_at(snap, t as f64).unwrap_or(touch.base);
                    out.push(AutoPoint {
                        tick: t,
                        value: shifted(param, v, by_at(t)),
                        curve: shape.map_or(0.0, |j| snap[j].curve),
                        step: shape.is_some_and(|j| snap[j].step),
                        ease: shape.is_some_and(|j| snap[j].ease),
                    });
                }
                // Let go, it ramps back to the line as it was.
                let back = end.saturating_add(RAMP);
                if end != u64::MAX
                    && let Some(v) = value_at(snap, back as f64)
                {
                    out.push(AutoPoint::new(back, v));
                }
                out.extend(snap.iter().filter(|p| p.tick > back).copied());
                out
            }
        };
        let a = &mut self.session.project.tracks[track].automation;
        *a.points_mut(param) = points;
    }

    /// The line set at the playhead: the point there moved, or one put there.
    fn point_at_playhead(&mut self, track: usize, param: Param, value: f32) {
        let tick = self.playhead();
        let a = &mut self.session.project.tracks[track].automation;
        let points = a.points_mut(param);
        match points.iter().position(|p| p.tick == tick) {
            Some(i) => points[i].value = value,
            None => {
                a.add(
                    param,
                    AutoPoint {
                        step: param.steps(),
                        ..AutoPoint::new(tick, value)
                    },
                );
            }
        }
    }

    /// While the song plays, a control moved on a track whose mode records records its value
    /// at the playhead.
    pub fn record_auto(&mut self, track: usize, param: Param, value: f32) {
        if !self.player.playing() {
            return;
        }
        let name = matches!(param, Param::Plugin(..)).then(|| self.param_name(track, param));
        let tick = self.playhead();
        let Some(t) = self.session.project.tracks.get_mut(track) else {
            return;
        };
        if !t.automation.mode.writes() {
            return;
        }
        let key = (t.id, param);
        let after = self.writing.get(&key).map(|w| w.0);
        // The line as it was, to ramp back to when a Touch is let go.
        if after.is_none() {
            self.recorded
                .insert(key, t.automation.points(param).to_vec());
        }
        t.automation.write(param, after, tick, value);
        if let Some(name) = name {
            t.automation.name(param, &name);
        }
        self.writing.insert(key, (tick, value, true));
        self.written.insert(key);
        self.show_lane(track, param);
    }

    /// Lets go of a control: Read hands back to the automation; Touch and Trim stop recording;
    /// Latch and Write carry on with its last value until playback stops.
    pub fn let_go(&mut self, track: usize, param: Param) {
        let Some(t) = self.session.project.tracks.get(track) else {
            return;
        };
        let key = (t.id, param);
        let carries = matches!(t.automation.mode, AutoMode::Latch | AutoMode::Write);
        if let Some(touch) = self.touches.get_mut(&key) {
            if touch.from.is_some() {
                let tick = self.session.project.ticks(self.player.position());
                let last = touch.by.last().map_or(0.0, |b| b.1);
                if touch.by.last().is_none_or(|b| b.0 < tick) {
                    touch.by.push((tick, last));
                }
            }
            touch.held = false;
            self.retouch(track, param);
            self.touches.remove(&key);
        }
        match (carries, self.writing.get_mut(&key)) {
            (true, Some(w)) => w.2 = false,
            (_, _) => {
                // Touch let go: back to the line as it was, over a moment.
                if let (Some(w), Some(was)) =
                    (self.writing.remove(&key), self.recorded.remove(&key))
                {
                    let back = w.0 + RAMP;
                    if let Some(v) = value_at(&was, back as f64) {
                        let points = self.session.project.tracks[track]
                            .automation
                            .points_mut(param);
                        points.retain(|p| p.tick <= w.0 || p.tick > back);
                        let i = points.partition_point(|p| p.tick <= w.0);
                        points.insert(
                            i,
                            AutoPoint {
                                step: param.steps(),
                                ..AutoPoint::new(back, v)
                            },
                        );
                    }
                }
                if self.taken.get(&key) == Some(&Live::Held) {
                    self.taken.remove(&key);
                }
            }
        }
    }

    /// Lets go of every control held (a mouse let go of anywhere).
    pub fn let_go_all(&mut self) {
        let held: Vec<(u64, Param)> = self
            .touches
            .keys()
            .copied()
            .chain(
                self.taken
                    .iter()
                    .filter(|(k, l)| **l == Live::Held && !self.writing.contains_key(k))
                    .map(|(k, _)| *k),
            )
            .chain(self.writing.iter().filter(|(_, w)| w.2).map(|(k, _)| *k))
            .collect();
        for (id, param) in held {
            if let Some(t) = self.session.project.tracks.iter().position(|t| t.id == id) {
                self.let_go(t, param);
            }
        }
    }

    /// Each frame: playback starting or stopping begins or ends a pass; while it plays, held
    /// and latched controls keep writing as the playhead moves, and Trim keeps moving the line.
    pub fn automation_frame(&mut self, cx: &mut Context<Self>) {
        // A control moved without a drag (a click, a swipe, a key) has been let go.
        let held = !self.touches.is_empty()
            || self
                .taken
                .iter()
                .any(|(k, l)| *l == Live::Held && !self.writing.contains_key(k));
        if self.drag.is_none() && held {
            self.let_go_all();
            self.changed(cx);
        }
        let playing = self.player.playing();
        if playing != self.was_playing {
            self.was_playing = playing;
            match playing {
                true => self.pass_starts(cx),
                false => self.pass_ends(cx),
            }
        }
        if !playing || (self.writing.is_empty() && self.touches.is_empty()) {
            return;
        }
        let tick = self.playhead();
        let mut wrote = false;
        for (&(id, param), w) in self.writing.iter_mut() {
            let Some(t) = self.session.project.tracks.iter_mut().find(|t| t.id == id) else {
                continue;
            };
            if tick > w.0 {
                t.automation.write(param, Some(w.0), tick, w.1);
                w.0 = tick;
                wrote = true;
            }
        }
        let trims: Vec<(u64, Param)> = self
            .touches
            .iter()
            .filter(|(_, t)| t.held && t.from.is_some())
            .map(|(k, _)| *k)
            .collect();
        for (id, param) in trims {
            let Some(track) = self.session.project.tracks.iter().position(|t| t.id == id) else {
                continue;
            };
            if let Some(touch) = self.touches.get_mut(&(id, param)) {
                let last = touch.by.last().map_or(0.0, |b| b.1);
                if touch.by.last().is_none_or(|b| b.0 < tick) {
                    touch.by.push((tick, last));
                }
            }
            self.retouch(track, param);
            wrote = true;
        }
        if wrote {
            self.changed(cx);
        }
    }

    /// Playback starts: what was changed until play goes back to its automation, and Write
    /// starts writing every automated control on its tracks.
    fn pass_starts(&mut self, cx: &mut Context<Self>) {
        self.taken.retain(|_, l| *l != Live::UntilPlay);
        let tick = self.playhead();
        let writes: Vec<usize> = (0..self.session.project.tracks.len())
            .filter(|&t| self.session.project.tracks[t].automation.mode == AutoMode::Write)
            .collect();
        if !writes.is_empty() {
            self.session.checkpoint();
        }
        for track in writes {
            let t = &self.session.project.tracks[track];
            let id = t.id;
            for param in t.automation.params() {
                let points = self.session.project.tracks[track].automation.points(param);
                let value =
                    value_at(points, tick as f64).unwrap_or_else(|| self.own_value(track, param));
                self.set_own(track, param, value);
                self.writing.insert((id, param), (tick, value, false));
                self.taken.insert((id, param), Live::Held);
                self.written.insert((id, param));
            }
        }
        self.changed(cx);
    }

    /// Playback stops: recording does; what was recorded is thinned of points that change
    /// nothing, and Write turns to Latch, as Logic's does after a pass.
    fn pass_ends(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<(u64, Param)> = self.touches.keys().copied().collect();
        for (id, param) in ids {
            if let Some(t) = self.session.project.tracks.iter().position(|t| t.id == id) {
                self.let_go(t, param);
            }
        }
        self.writing.clear();
        self.recorded.clear();
        self.taken.retain(|_, l| *l != Live::Held);
        let thin = self.settings.thin_recorded;
        let latch = self.settings.write_to_latch;
        for (id, param) in std::mem::take(&mut self.written) {
            if let Some(t) = self.session.project.tracks.iter_mut().find(|t| t.id == id) {
                if thin {
                    t.automation.thin(param, 0.004);
                }
                if latch && t.automation.mode == AutoMode::Write {
                    t.automation.mode = AutoMode::Latch;
                }
            }
        }
        self.changed(cx);
    }

    // ---------------------------------------------------------------------------------------
    // Editing lanes

    /// A lane's parameter, by its track and place.
    fn lane_at(&self, track: usize, k: usize) -> Param {
        self.auto_lanes(track).get(k).copied().unwrap_or_default()
    }

    /// The lane under a window y on a track's row, if one is.
    fn lane_y(&self, track: usize, k: usize, y: f32) -> f32 {
        y - f32::from(self.lanes_bounds.get().origin.y) - self.lane_top(track, k)
    }

    /// A value at a window y in a lane.
    pub fn auto_value(&self, track: usize, k: usize, y: f32) -> f32 {
        from_y(
            self.lane_at(track, k),
            self.lane_y(track, k, y),
            self.auto_h(),
        )
    }

    /// A tick at a window x, on the grid unless ⌘ is held or the grid is off.
    fn auto_tick(&self, x: f32, free: bool) -> u64 {
        let tick = self.tick_at(x);
        match free || !self.settings.snap_automation {
            true => tick.round().max(0.0) as u64,
            false => self.snap(tick),
        }
    }

    pub fn picked_points(&self, track: usize, param: Param) -> Vec<usize> {
        let id = self.session.project.tracks[track].id;
        match &self.auto_picked {
            Some(p) if p.track == id && p.param == param => p.points.clone(),
            _ => Vec::new(),
        }
    }

    fn pick_points(&mut self, track: usize, param: Param, points: Vec<usize>) {
        let id = self.session.project.tracks[track].id;
        self.auto_picked = Some(Picked {
            track: id,
            param,
            points,
        });
        self.auto_focus = Some((id, param));
    }

    /// The pointer pressed on a lane, not on a point: the pencil draws, the eraser and a drag
    /// select (the eraser takes out what it selects), a press on the line grabs it, and a click
    /// puts a point.
    pub fn lane_press(
        &mut self,
        track: usize,
        k: usize,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        let p = (f32::from(e.position.x), f32::from(e.position.y));
        self.select_track(track);
        self.region = None;
        let id = self.session.project.tracks[track].id;
        self.auto_focus = Some((id, param));
        if self.tool == Tool::Pencil {
            self.session.checkpoint();
            let tick = self.auto_tick(p.0, e.modifiers.platform);
            let value = self.auto_value(track, k, p.1);
            self.draw_to(track, param, None, tick, value);
            self.drag = Some(Drag::AutoDraw {
                track,
                param,
                last: (tick, value),
            });
            self.changed(cx);
            return;
        }
        // On the line between two points: up or down together.
        let points = self.session.project.tracks[track].automation.points(param);
        if self.tool != Tool::Eraser && !points.is_empty() && !e.modifiers.shift {
            let tick = self.tick_at(p.0);
            let y = self.lane_y(track, k, p.1);
            let line = value_at(points, tick).map(|v| to_y(param, v, self.auto_h()));
            if line.is_some_and(|l| (l - y).abs() < NEAR) {
                let i = points.partition_point(|q| (q.tick as f64) <= tick);
                let ends: Vec<usize> = match (i.checked_sub(1), i < points.len()) {
                    (Some(a), true) => vec![a, i],
                    (Some(a), false) => vec![a],
                    (None, _) => vec![0],
                };
                self.auto_grab = points.to_vec();
                self.pick_points(track, param, ends);
                self.begin_drag(
                    Drag::AutoLine {
                        track,
                        param,
                        k,
                        y: p.1,
                        moved: false,
                    },
                    cx,
                );
                return;
            }
        }
        if !e.modifiers.shift {
            self.auto_picked = None;
        }
        self.drag = Some(Drag::AutoMarquee {
            track,
            k,
            from: p,
            to: p,
            erase: self.tool == Tool::Eraser,
            free: e.modifiers.platform,
        });
        cx.notify();
    }

    /// A point pressed: ⌥, the eraser or a double-click takes it out; ⇧ adds it to the
    /// selection or takes it off; else it's dragged, with the others selected.
    pub fn point_press(
        &mut self,
        track: usize,
        k: usize,
        index: usize,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        self.select_track(track);
        self.region = None;
        if e.modifiers.alt || self.tool == Tool::Eraser || e.click_count == 2 {
            let mut gone = self.picked_points(track, param);
            if !gone.contains(&index) {
                gone = vec![index];
            }
            self.delete_points(track, param, gone, cx);
            return;
        }
        let mut picked = self.picked_points(track, param);
        if e.modifiers.shift {
            match picked.iter().position(|&i| i == index) {
                Some(j) => drop(picked.remove(j)),
                None => picked.push(index),
            }
            self.pick_points(track, param, picked);
            cx.notify();
            return;
        }
        if !picked.contains(&index) {
            picked = vec![index];
        }
        self.pick_points(track, param, picked);
        self.auto_grab = self.session.project.tracks[track]
            .automation
            .points(param)
            .to_vec();
        self.begin_drag(
            Drag::Auto {
                track,
                param,
                k,
                index,
                moved: false,
            },
            cx,
        );
    }

    /// Selected points dragged: the one held to the pointer (on the grid unless ⌘), the others
    /// as far, none past a point not selected.
    pub fn drag_points(
        &mut self,
        track: usize,
        param: Param,
        k: usize,
        index: usize,
        p: Point<f32>,
    ) {
        let picked = self.picked_points(track, param);
        let grab = &self.auto_grab;
        let Some(held) = grab.get(index).copied() else {
            return;
        };
        let h = self.auto_h();
        let free = self.fine;
        let tick = self.auto_tick(p.x, free) as i64;
        let mut dt = tick - held.tick as i64;
        // None past a neighbour that isn't moving.
        for &i in &picked {
            let t = grab[i].tick as i64;
            let prev = (0..i)
                .rev()
                .find(|j| !picked.contains(j))
                .map(|j| grab[j].tick as i64);
            let next = (i + 1..grab.len())
                .find(|j| !picked.contains(j))
                .map(|j| grab[j].tick as i64);
            dt = dt.max(prev.unwrap_or(0) - t).max(-t);
            if let Some(n) = next {
                dt = dt.min(n - t);
            }
        }
        let dy = self.lane_y(track, k, p.y) - to_y(param, held.value, h);
        let mut points = grab.clone();
        for &i in &picked {
            let q = grab[i];
            points[i].tick = (q.tick as i64 + dt).max(0) as u64;
            points[i].value = from_y(param, to_y(param, q.value, h) + dy, h);
        }
        // The one held goes exactly where the pointer is.
        points[index].value = from_y(param, self.lane_y(track, k, p.y), h);
        *self.session.project.tracks[track]
            .automation
            .points_mut(param) = points;
    }

    /// The line between two points dragged up or down: both ends with it.
    pub fn drag_line(&mut self, track: usize, param: Param, k: usize, y: f32, p: Point<f32>) {
        let _ = k;
        let h = self.auto_h();
        let dy = (p.y - y) * if self.fine { 0.25 } else { 1.0 };
        let mut points = self.auto_grab.clone();
        for i in self.picked_points(track, param) {
            if let Some(q) = points.get_mut(i) {
                q.value = from_y(param, to_y(param, q.value, h) + dy, h);
            }
        }
        *self.session.project.tracks[track]
            .automation
            .points_mut(param) = points;
    }

    /// A line's bend dragged from its middle: pulled towards the end it rises to, it gets
    /// there sooner.
    pub fn drag_bend(
        &mut self,
        track: usize,
        param: Param,
        index: usize,
        y: f32,
        curve: f32,
        p: Point<f32>,
    ) {
        let h = self.auto_h();
        let points = self.session.project.tracks[track]
            .automation
            .points_mut(param);
        let (Some(a), Some(b)) = (points.get(index).copied(), points.get(index + 1).copied())
        else {
            return;
        };
        let (ya, yb) = (to_y(param, a.value, h), to_y(param, b.value, h));
        let way = if ya > yb { 1.0 } else { -1.0 };
        points[index].curve = (curve + (p.y - y) / 50.0 * way).clamp(-1.0, 1.0);
        points[index].ease = false;
    }

    /// The pencil drawn from its last point to here: what was between goes.
    fn draw_to(&mut self, track: usize, param: Param, last: Option<u64>, tick: u64, value: f32) {
        let a = &mut self.session.project.tracks[track].automation;
        let points = a.points_mut(param);
        let (lo, hi) = match last {
            Some(l) => (l.min(tick), l.max(tick)),
            None => (tick, tick),
        };
        points.retain(|p| p.tick == last.unwrap_or(u64::MAX) || p.tick < lo || p.tick > hi);
        let i = points.partition_point(|p| p.tick < tick);
        points.insert(
            i,
            AutoPoint {
                step: param.steps(),
                ..AutoPoint::new(tick, value)
            },
        );
    }

    pub fn drag_draw(&mut self, track: usize, param: Param, last: (u64, f32), p: Point<f32>) {
        let k = self
            .auto_lanes(track)
            .iter()
            .position(|&q| q == param)
            .unwrap_or(0);
        let tick = self.auto_tick(p.x, false);
        let value = self.auto_value(track, k, p.y);
        if tick == last.0 {
            let points = self.session.project.tracks[track]
                .automation
                .points_mut(param);
            if let Some(q) = points.iter_mut().find(|q| q.tick == tick) {
                q.value = value;
            }
        } else {
            self.draw_to(track, param, Some(last.0), tick, value);
        }
        if let Some(Drag::AutoDraw { last, .. }) = &mut self.drag {
            *last = (tick, value);
        }
    }

    /// The pencil let go: what it drew, thinned.
    pub fn draw_done(&mut self, track: usize, param: Param) {
        if self.settings.thin_recorded {
            self.session.project.tracks[track]
                .automation
                .thin(param, 0.004);
        }
    }

    /// A marquee let go: the points inside selected (or erased); a click without dragging puts
    /// a point, on the line if it was near.
    #[allow(clippy::too_many_arguments)]
    pub fn marquee_done(
        &mut self,
        track: usize,
        k: usize,
        from: (f32, f32),
        to: (f32, f32),
        erase: bool,
        free: bool,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        let h = self.auto_h();
        if (to.0 - from.0).abs() < 4.0 && (to.1 - from.1).abs() < 4.0 {
            if erase {
                return;
            }
            let tick = self.auto_tick(from.0, free);
            let y = self.lane_y(track, k, from.1);
            let points = self.session.project.tracks[track].automation.points(param);
            let line = value_at(points, tick as f64);
            let value = match line {
                Some(v) if (to_y(param, v, h) - y).abs() < NEAR * 1.5 => v,
                _ => from_y(param, y, h),
            };
            let name = self.param_name(track, param);
            let mut index = 0;
            self.edit(cx, |ws| {
                let a = &mut ws.session.project.tracks[track].automation;
                index = a.add(
                    param,
                    AutoPoint {
                        step: param.steps(),
                        ..AutoPoint::new(tick, value)
                    },
                );
                a.name(param, &name);
            });
            self.pick_points(track, param, vec![index]);
            return;
        }
        let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
        let (y0, y1) = (
            self.lane_y(track, k, from.1.min(to.1)),
            self.lane_y(track, k, from.1.max(to.1)),
        );
        let (t0, t1) = (self.tick_at(x0), self.tick_at(x1));
        let points = self.session.project.tracks[track].automation.points(param);
        let inside: Vec<usize> = points
            .iter()
            .enumerate()
            .filter(|(_, q)| {
                let y = to_y(param, q.value, h);
                (q.tick as f64) >= t0 && (q.tick as f64) <= t1 && y >= y0 - 4.0 && y <= y1 + 4.0
            })
            .map(|(i, _)| i)
            .collect();
        if erase {
            self.delete_points(track, param, inside, cx);
            return;
        }
        let mut picked = self.picked_points(track, param);
        for i in inside {
            if !picked.contains(&i) {
                picked.push(i);
            }
        }
        self.pick_points(track, param, picked);
        cx.notify();
    }

    fn delete_points(
        &mut self,
        track: usize,
        param: Param,
        mut gone: Vec<usize>,
        cx: &mut Context<Self>,
    ) {
        gone.sort_unstable();
        gone.dedup();
        self.edit(cx, |ws| {
            let a = &mut ws.session.project.tracks[track].automation;
            let points = a.points_mut(param);
            for &i in gone.iter().rev() {
                if i < points.len() {
                    points.remove(i);
                }
            }
            a.tidy();
            ws.auto_picked = None;
        });
    }

    /// Delete with points selected takes them out.
    pub fn delete_picked(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(p) = self.auto_picked.clone().filter(|p| !p.points.is_empty()) else {
            return false;
        };
        let Some(track) = self
            .session
            .project
            .tracks
            .iter()
            .position(|t| t.id == p.track)
        else {
            return false;
        };
        self.delete_points(track, p.param, p.points, cx);
        true
    }

    /// ⌘C with points selected copies them.
    pub fn copy_picked(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(p) = self.auto_picked.clone().filter(|p| !p.points.is_empty()) else {
            return false;
        };
        let Some(t) = self.session.project.tracks.iter().find(|t| t.id == p.track) else {
            return false;
        };
        let points = t.automation.points(p.param);
        let chosen: Vec<AutoPoint> = p
            .points
            .iter()
            .filter_map(|&i| points.get(i).copied())
            .collect();
        let (Some(first), Some(last)) = (
            chosen.iter().map(|q| q.tick).min(),
            chosen.iter().map(|q| q.tick).max(),
        ) else {
            return false;
        };
        let mut lane: Vec<AutoPoint> = chosen
            .iter()
            .map(|q| AutoPoint {
                tick: q.tick - first,
                ..*q
            })
            .collect();
        lane.sort_by_key(|q| q.tick);
        self.auto_clip = Some(AutoClip {
            length: last - first,
            lanes: vec![(p.param, lane)],
        });
        self.status = Some(format!("{} points copied", chosen.len()));
        cx.notify();
        true
    }

    /// ⌘V after points were copied: them, at the playhead, on the lane last worked on.
    pub fn paste_points(&mut self, cx: &mut Context<Self>) -> bool {
        if self.auto_clip.is_none() || self.auto_focus.is_none() {
            return false;
        }
        let tick = self.playhead();
        self.paste_points_at(tick, cx)
    }

    fn paste_points_at(&mut self, tick: u64, cx: &mut Context<Self>) -> bool {
        let (Some(clip), Some((id, param))) = (self.auto_clip.clone(), self.auto_focus) else {
            return false;
        };
        let Some(track) = self.session.project.tracks.iter().position(|t| t.id == id) else {
            return false;
        };
        let Some((from, points)) = clip.lanes.first().cloned() else {
            return false;
        };
        if units(from) != units(param) {
            self.status = Some(format!(
                "{} points don't go on {}.",
                self.param_name(track, from),
                self.param_name(track, param)
            ));
            cx.notify();
            return true;
        }
        let one = AutoClip {
            length: clip.length,
            lanes: vec![(param, points)],
        };
        self.edit(cx, |ws| {
            ws.session.project.tracks[track].automation.put(tick, &one);
        });
        let a = &self.session.project.tracks[track].automation;
        let picked = a
            .points(param)
            .iter()
            .enumerate()
            .filter(|(_, q)| q.tick >= tick && q.tick <= tick + clip.length)
            .map(|(i, _)| i)
            .collect();
        self.pick_points(track, param, picked);
        true
    }

    /// An automation menu's item, on a track.
    pub fn auto_op(&mut self, id: u64, op: AutoOp, cx: &mut Context<Self>) {
        let Some(track) = self.session.project.tracks.iter().position(|t| t.id == id) else {
            return;
        };
        let focus = self.auto_focus.filter(|f| f.0 == id).map(|f| f.1);
        let param = focus.unwrap_or_else(|| self.lane_param(track));
        let picked = self.picked_points(track, param);
        let reshape = |ws: &mut Workspace, f: &dyn Fn(&mut AutoPoint)| {
            let points = ws.session.project.tracks[track]
                .automation
                .points_mut(param);
            for &i in &picked {
                if let Some(q) = points.get_mut(i) {
                    f(q);
                }
            }
        };
        match op {
            AutoOp::Lane(k, p) => {
                self.set_lane(track, k, p);
                self.auto_focus = Some((id, p));
            }
            AutoOp::AddLane => self.add_lane(track),
            AutoOp::Automate(param) => {
                if self.session.project.tracks[track]
                    .automation
                    .points(param)
                    .is_empty()
                {
                    let value = self.own_value(track, param);
                    self.edit(cx, |ws| {
                        ws.session.project.tracks[track]
                            .automation
                            .points_mut(param)
                            .push(AutoPoint::new(0, value));
                    });
                }
                self.select_track(track);
                self.show_lane(track, param);
                self.show_automation = true;
            }
            AutoOp::RemoveLane(k) => self.remove_lane(track, k),
            AutoOp::Mode(mode) => self.edit(cx, |ws| {
                ws.session.project.tracks[track].automation.mode = mode;
                ws.writing.retain(|k, _| k.0 != id);
                ws.recorded.retain(|k, _| k.0 != id);
                ws.taken.retain(|k, _| k.0 != id);
            }),
            AutoOp::Straight => self.edit(cx, |ws| {
                reshape(ws, &|q| (q.curve, q.step, q.ease) = (0.0, false, false))
            }),
            AutoOp::SlowStart => self.edit(cx, |ws| {
                reshape(ws, &|q| (q.curve, q.step, q.ease) = (0.6, false, false))
            }),
            AutoOp::FastStart => self.edit(cx, |ws| {
                reshape(ws, &|q| (q.curve, q.step, q.ease) = (-0.6, false, false))
            }),
            AutoOp::SCurve => self.edit(cx, |ws| {
                reshape(ws, &|q| (q.curve, q.step, q.ease) = (0.0, false, true))
            }),
            AutoOp::Step => self.edit(cx, |ws| reshape(ws, &|q| (q.step, q.ease) = (true, false))),
            AutoOp::Smooth(width) => {
                // The selection's span; one point, from the one before it to the one after; or
                // all of it.
                let points = self.session.project.tracks[track].automation.points(param);
                let ticks: Vec<u64> = picked
                    .iter()
                    .filter_map(|&i| points.get(i).map(|q| q.tick))
                    .collect();
                let (from, to) = match (ticks.iter().min(), ticks.iter().max()) {
                    (Some(&a), Some(&b)) if b > a => (a, b),
                    (Some(_), _) => {
                        let i = picked[0];
                        let last = points.len().saturating_sub(1);
                        (
                            points[i.saturating_sub(1)].tick,
                            points[(i + 1).min(last)].tick,
                        )
                    }
                    _ => match (points.first(), points.last()) {
                        (Some(a), Some(b)) => (a.tick, b.tick),
                        _ => (0, 0),
                    },
                };
                if param.steps() || to <= from {
                    self.status = Some(match param.steps() {
                        true => "Mute is all steps: nothing to smooth.".into(),
                        false => "Nothing there to smooth.".into(),
                    });
                } else {
                    self.edit(cx, |ws| {
                        let a = &mut ws.session.project.tracks[track].automation;
                        a.smooth(param, from, to, width);
                        let n = a.points(param).len();
                        ws.status = Some(format!("Smoothed: {n} points"));
                        // What was smoothed stays selected, to smooth again.
                        let picked = a
                            .points(param)
                            .iter()
                            .enumerate()
                            .filter(|(_, q)| q.tick >= from && q.tick <= to)
                            .map(|(i, _)| i)
                            .collect();
                        ws.auto_picked = Some(Picked {
                            track: id,
                            param,
                            points: picked,
                        });
                    });
                }
            }
            AutoOp::Delete => self.delete_points(track, param, picked, cx),
            AutoOp::Copy => drop(self.copy_picked(cx)),
            AutoOp::Paste(tick) => drop(self.paste_points_at(tick, cx)),
            AutoOp::SelectAll => {
                let n = self.session.project.tracks[track]
                    .automation
                    .points(param)
                    .len();
                self.pick_points(track, param, (0..n).collect());
            }
            AutoOp::Thin => self.edit(cx, |ws| {
                let a = &mut ws.session.project.tracks[track].automation;
                let before = a.points(param).len();
                a.thin(param, 0.01);
                let after = a.points(param).len();
                ws.status = Some(format!("{} points taken out", before - after));
                ws.auto_picked = None;
            }),
            AutoOp::Clear => self.edit(cx, |ws| {
                let a = &mut ws.session.project.tracks[track].automation;
                a.points_mut(param).clear();
                a.tidy();
                ws.auto_picked = None;
            }),
            AutoOp::Shape(shape, from, to, every) => {
                let (low, high) = self.shape_range(track, param, shape, from);
                let name = self.param_name(track, param);
                self.edit(cx, |ws| {
                    let a = &mut ws.session.project.tracks[track].automation;
                    a.shape(param, shape, from, to, every, low, high);
                    a.name(param, &name);
                    ws.auto_picked = None;
                });
            }
        }
        cx.notify();
    }

    /// Where a shape swings, by what it moves: a level from where it is down 12 dB (a ramp
    /// from silence), a pan most of the way across, a plugin's most of its range.
    fn shape_range(&self, track: usize, param: Param, shape: AutoShape, at: u64) -> (f32, f32) {
        let ramp = matches!(shape, AutoShape::RampUp | AutoShape::RampDown);
        match param {
            Param::Volume | Param::Send(_) => {
                let top = self
                    .automated(track, param)
                    .and_then(|_| {
                        let t = &self.session.project.tracks[track];
                        value_at(t.automation.points(param), at as f64)
                    })
                    .unwrap_or_else(|| self.own_value(track, param))
                    .clamp(-30.0, 6.0);
                (if ramp { SILENT_DB } else { top - 12.0 }, top)
            }
            Param::Pan => match ramp {
                true => (-1.0, 1.0),
                false => (-0.8, 0.8),
            },
            Param::Mute => (0.0, 1.0),
            Param::Plugin(..) => match ramp {
                true => (0.0, 1.0),
                false => (0.1, 0.9),
            },
            // A sweep: from 200 Hz to 8 kHz; a gain either way; a width from wide to narrow.
            Param::Eq(_, EqField::Freq) => (200.0, 8_000.0),
            Param::Eq(_, EqField::Gain) => match ramp {
                true => (0.0, 6.0),
                false => (-6.0, 6.0),
            },
            Param::Eq(_, EqField::Q) => (0.5, 4.0),
            Param::Mod(..) => match ramp {
                true => (param.from_unit(0.0), param.from_unit(1.0)),
                false => (param.from_unit(0.1), param.from_unit(0.9)),
            },
        }
    }

    /// Right-click on a lane: shapes to draw over the selection, the cycle or the bar
    /// clicked, a cycle every grid step; and the lane's own items.
    pub fn auto_lane_menu(
        &mut self,
        track: usize,
        k: usize,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        let id = self.session.project.tracks[track].id;
        self.auto_focus = Some((id, param));
        let tick = self.tick_at(f32::from(at.x)).max(0.0) as u64;
        let p = &self.session.project;
        let points = p.tracks[track].automation.points(param);
        let picked: Vec<u64> = self
            .picked_points(track, param)
            .iter()
            .filter_map(|&i| points.get(i).map(|q| q.tick))
            .collect();
        let bar = p.bar();
        let (from, to, over) = match (picked.iter().min(), picked.iter().max(), p.cycle) {
            (Some(&a), Some(&b), _) if b > a => (a, b, "the selection".to_string()),
            (_, _, Some(c)) if !c.off && c.start <= tick && tick < c.end => {
                (c.start, c.end, "the cycle".to_string())
            }
            _ => {
                let start = tick / bar * bar;
                (start, start + bar, format!("bar {}", start / bar + 1))
            }
        };
        let every = match self.grid() {
            1 => p.beat(),
            g => g,
        };
        let rate = match every {
            e if e == bar => "bar".to_string(),
            PPQ_BEAT => "beat".to_string(),
            e if e % PPQ_BEAT == 0 => format!("{} beats", e / PPQ_BEAT),
            e => format!("1/{}", PPQ_BEAT * 4 / e.max(1)),
        };
        let mut items: Vec<(String, MenuAction)> = vec![(
            format!("Shapes over {over}, a cycle every {rate}"),
            MenuAction::Nothing,
        )];
        let shapes = match param.steps() {
            true => vec![AutoShape::Square, AutoShape::Random],
            false => AutoShape::ALL.to_vec(),
        };
        for s in shapes {
            items.push((
                s.name().to_string(),
                MenuAction::Auto(id, AutoOp::Shape(s, from, to, every)),
            ));
        }
        items.push((String::new(), MenuAction::Nothing));
        if self.auto_clip.is_some() {
            let snapped = self.snap(tick as f64);
            items.push((
                "Paste\t⌘V".into(),
                MenuAction::Auto(id, AutoOp::Paste(snapped)),
            ));
        }
        if !param.steps() {
            let what = match picked.len() > 1 {
                true => "Selection",
                false => "Lane",
            };
            items.push((
                format!("Smooth {what}"),
                MenuAction::Auto(id, AutoOp::Smooth(PPQ / 2)),
            ));
            items.push((
                format!("Smooth {what} More"),
                MenuAction::Auto(id, AutoOp::Smooth(PPQ * 2)),
            ));
        }
        items.push(("Select All".into(), MenuAction::Auto(id, AutoOp::SelectAll)));
        items.push(("Thin Points".into(), MenuAction::Auto(id, AutoOp::Thin)));
        items.push(("Clear Lane".into(), MenuAction::Auto(id, AutoOp::Clear)));
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// Right-click on a point: its line's shape (and the selection's), or out it goes.
    pub fn point_menu(
        &mut self,
        track: usize,
        k: usize,
        index: usize,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        let id = self.session.project.tracks[track].id;
        let mut picked = self.picked_points(track, param);
        if !picked.contains(&index) {
            picked = vec![index];
        }
        self.pick_points(track, param, picked);
        let q = self.session.project.tracks[track].automation.points(param)[index];
        let tick = |on: bool| if on { "✓ " } else { "" };
        let mut items = Vec::new();
        if !param.steps() {
            items.extend([
                (
                    format!("{}Straight", tick(!q.shaped())),
                    MenuAction::Auto(id, AutoOp::Straight),
                ),
                (
                    format!("{}S-Curve", tick(!q.step && q.ease)),
                    MenuAction::Auto(id, AutoOp::SCurve),
                ),
                (
                    format!(
                        "{}Curve: Slow Start",
                        tick(!q.step && !q.ease && q.curve > 0.0)
                    ),
                    MenuAction::Auto(id, AutoOp::SlowStart),
                ),
                (
                    format!(
                        "{}Curve: Fast Start",
                        tick(!q.step && !q.ease && q.curve < 0.0)
                    ),
                    MenuAction::Auto(id, AutoOp::FastStart),
                ),
                (
                    format!("{}Step", tick(q.step)),
                    MenuAction::Auto(id, AutoOp::Step),
                ),
                (String::new(), MenuAction::Nothing),
                (
                    "Smooth".into(),
                    MenuAction::Auto(id, AutoOp::Smooth(PPQ / 2)),
                ),
                (
                    "Smooth More".into(),
                    MenuAction::Auto(id, AutoOp::Smooth(PPQ * 2)),
                ),
                (String::new(), MenuAction::Nothing),
            ]);
        }
        items.extend([
            ("Copy\t⌘C".into(), MenuAction::Auto(id, AutoOp::Copy)),
            ("Delete\t⌫".into(), MenuAction::Auto(id, AutoOp::Delete)),
        ]);
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// The modes, for a track's lane or strip.
    pub fn mode_menu(&mut self, track: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        let t = &self.session.project.tracks[track];
        let (id, now) = (t.id, t.automation.mode);
        let items = AutoMode::ALL
            .iter()
            .map(|&m| {
                let tick = if m == now { "✓ " } else { "" };
                (
                    format!("{tick}{}\t{}", m.name(), mode_what(m)),
                    MenuAction::Auto(id, AutoOp::Mode(m)),
                )
            })
            .collect();
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// A region moved (or copied) with "Automation moves with regions": the automation under
    /// it goes with it.
    pub fn carry_automation(&mut self, from: (usize, u64, u64), to: (usize, u64), copy: bool) {
        let (t0, start, end) = from;
        let (t1, at) = to;
        if (t0 == t1 && start == at) || end <= start {
            return;
        }
        let clip = self.session.project.tracks[t0].automation.take(start, end);
        if clip.lanes.is_empty() {
            return;
        }
        if !copy {
            self.session.project.tracks[t0].automation.erase(start, end);
        }
        self.session.project.tracks[t1].automation.put(at, &clip);
    }

    // ---------------------------------------------------------------------------------------
    // Views

    /// A track's lane headers, under its header: what each shows (pick another), its value now,
    /// the track's mode, and lanes to add or take away.
    pub fn auto_headers(&self, i: usize, cx: &mut Context<Self>) -> Vec<Div> {
        let t = &self.session.project.tracks[i];
        let (id, mode) = (t.id, t.automation.mode);
        let color = self.color(i);
        let h = self.auto_h();
        let lanes = self.auto_lanes(i);
        let n = lanes.len();
        let mut out = Vec::new();
        for (k, &param) in lanes.iter().enumerate() {
            let top = self.lane_top(i, k);
            let has = !t.automation.points(param).is_empty();
            let value = self.now_value(i, param);
            let lit = has && mode != AutoMode::Off;
            let (label, plugin) = match param {
                Param::Plugin(Which::Instrument, _) => (self.param_name(i, param), None),
                Param::Plugin(which, _) => {
                    (self.param_name(i, param), Some(self.plugin_name(i, which)))
                }
                _ => (self.param_name(i, param), None),
            };
            // The scale, at the lane's right edge: its top, middle and bottom.
            let marks: Vec<(f32, &str)> = match param {
                Param::Volume | Param::Send(_) => {
                    vec![(6.0, "+6"), (0.0, "0"), (-12.0, "−12"), (-30.0, "−30")]
                }
                Param::Pan => vec![(-1.0, "L"), (0.0, "C"), (1.0, "R")],
                Param::Mute => vec![(0.0, "play"), (1.0, "mute")],
                Param::Plugin(..) => vec![(1.0, "100"), (0.5, "50"), (0.0, "0")],
                Param::Eq(_, EqField::Freq) => {
                    vec![(10_000.0, "10k"), (1_000.0, "1k"), (100.0, "100")]
                }
                Param::Eq(_, EqField::Gain) => vec![(12.0, "+12"), (0.0, "0"), (-12.0, "−12")],
                Param::Eq(_, EqField::Q) => vec![(10.0, "10"), (1.0, "1"), (0.1, "0.1")],
                Param::Mod(_, s) if s.range().1 > 1.0 => {
                    vec![(5_000.0, "5s"), (100.0, "100ms"), (2.0, "2ms")]
                }
                Param::Mod(..) => vec![(1.0, "100"), (0.5, "50"), (0.0, "0")],
            };
            let scale = marks.into_iter().map(|(v, s)| {
                div()
                    .absolute()
                    .right(px(4.))
                    .top(px((to_y(param, v, h) - 5.0).clamp(0.0, h - 11.0)))
                    .text_size(px(8.5))
                    .font_family(MONO)
                    .text_color(rgb(TEXT_3))
                    .child(s)
            });
            let readout = div()
                .h(px(18.))
                .min_w(px(52.))
                .px(px(5.))
                .flex_none()
                .flex()
                .items_center()
                .justify_end()
                .rounded(px(4.))
                .bg(rgb(0x0b0b0e))
                .border_1()
                .border_color(rgb(0x22222a))
                .font_family(MONO)
                .text_size(px(10.))
                .text_color(rgb(if lit { mode_color(mode) } else { TEXT_2 }))
                .whitespace_nowrap()
                .debug_selector(move || format!("auto-value-{i}-{k}"))
                .child(self.value_text(i, param, value));
            let icon_button = |i_: Icon, name: String| {
                div()
                    .size(px(18.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .text_color(rgb(TEXT_3))
                    .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                    .cursor_pointer()
                    .debug_selector(move || name.clone())
                    .child(icon(i_, 11.).text_color(rgb(TEXT_2)))
            };
            let top_row = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    dropdown(label, 20.)
                        .min_w(px(0.))
                        .max_w(px(150.))
                        .flex_shrink_1()
                        .overflow_hidden()
                        .when(k == 0, |d| {
                            d.debug_selector(move || format!("auto-param-{i}"))
                        })
                        .when(k > 0, |d| {
                            d.debug_selector(move || format!("auto-param-{i}-{k}"))
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.menu = Some(Menu::search(e.position, ws.auto_params(i, k)));
                                cx.notify();
                            }),
                        ),
                )
                .children(plugin.map(|name| {
                    div()
                        .min_w(px(0.))
                        .flex_shrink_1()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .text_ellipsis()
                        .child(name)
                }))
                .child(div().flex_1())
                .child(readout);
            let bottom_row = div()
                .flex()
                .items_center()
                .gap(px(4.))
                .when(k == 0, |d| {
                    d.child(
                        chip(mode.name(), mode_color(mode))
                            .cursor_pointer()
                            .debug_selector(move || format!("auto-mode-{i}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.mode_menu(i, e.position, cx);
                                }),
                            ),
                    )
                })
                .when(k > 0, |d| {
                    d.child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .child(format!("{} points", t.automation.points(param).len())),
                    )
                })
                .child(div().flex_1())
                .when(k == n - 1, |d| {
                    d.child(
                        icon_button(Icon::Plus, format!("auto-add-{i}")).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.auto_op(id, AutoOp::AddLane, cx);
                            }),
                        ),
                    )
                })
                .when(n > 1, |d| {
                    d.child(
                        icon_button(Icon::Close, format!("auto-remove-{i}-{k}")).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.auto_op(id, AutoOp::RemoveLane(k), cx);
                            }),
                        ),
                    )
                });
            out.push(
                div()
                    .absolute()
                    .top(px(top))
                    .left_0()
                    .w_full()
                    .h(px(h))
                    .bg(rgb(0x141419))
                    .border_b_1()
                    .border_color(rgb(LINE_SOFT))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px(3.))
                            .bg(alpha(color, if lit { 0.9 } else { 0.35 })),
                    )
                    .children(scale)
                    .child(
                        div()
                            .absolute()
                            .left(px(12.))
                            .right(px(30.))
                            .top(px(6.))
                            .bottom(px(5.))
                            .flex()
                            .flex_col()
                            .justify_between()
                            .child(top_row)
                            .when(h >= 44.0, |d| d.child(bottom_row)),
                    ),
            );
        }
        out
    }

    /// Every track's lanes, in the lanes area: a press area under each, the line painted over
    /// it (straight, bent or stepped, the area under it shaded, the playhead's value lit), and
    /// its points and bend handles to drag.
    pub fn auto_views(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let h = self.auto_h();
        let playhead = self.playhead();
        let mut out = Vec::new();
        for (t, track) in self.session.project.tracks.iter().enumerate() {
            let color = self.color(t);
            let off = track.automation.mode == AutoMode::Off;
            for (k, param) in self.auto_lanes(t).into_iter().enumerate() {
                let top = self.lane_top(t, k);
                let points: Vec<AutoPoint> = track.automation.points(param).to_vec();
                let level = self.own_value(t, param);
                let picked = self.picked_points(t, param);
                let live = self.taken.contains_key(&(track.id, param));
                let marquee = match self.drag {
                    Some(Drag::AutoMarquee {
                        track: mt,
                        k: mk,
                        from,
                        to,
                        erase,
                        ..
                    }) if mt == t && mk == k => Some((from, to, erase)),
                    _ => None,
                };
                let group = format!("auto-{t}-{k}");
                // The press area.
                out.push(
                    div()
                        .id(gpui::ElementId::Name(format!("auto-lane-{t}-{k}").into()))
                        .group(group.clone())
                        .absolute()
                        .top(px(top))
                        .left_0()
                        .w_full()
                        .h(px(h))
                        .cursor(match self.tool {
                            Tool::Pencil => gpui::CursorStyle::Crosshair,
                            Tool::Eraser => gpui::CursorStyle::OperationNotAllowed,
                            _ => gpui::CursorStyle::Arrow,
                        })
                        .when(k == 0, |d| {
                            d.debug_selector(move || format!("auto-lane-{t}"))
                        })
                        .when(k > 0, |d| {
                            d.debug_selector(move || format!("auto-lane-{t}-{k}"))
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.lane_press(t, k, e, cx);
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.select_track(t);
                                ws.auto_lane_menu(t, k, e.position, cx);
                            }),
                        )
                        .into_any_element(),
                );
                // The line.
                let shown = points.clone();
                let lit_at = value_at(&points, playhead as f64)
                    .filter(|_| !off && !live)
                    .map(|v| (playhead, v));
                let picked_ticks: Vec<usize> = picked.clone();
                let lanes_left = f32::from(self.lanes_bounds.get().origin.x);
                let _ = lanes_left;
                out.push(
                    canvas(
                        |_, _, _| {},
                        move |b, _, window, _| {
                            let y = |v: f32| b.origin.y + px(top + to_y(param, v, h));
                            let x = |tick: u64| b.origin.x + px(tick as f32 * ppt - sx);
                            let right = b.origin.x + b.size.width;
                            let bottom = b.origin.y + px(top + h - 1.0);
                            // Guides: the middle (pan), 0 dB, or half.
                            let guide = match param {
                                Param::Volume | Param::Send(_) => Some(0.0),
                                Param::Pan => Some(0.0),
                                Param::Plugin(..) => Some(0.5),
                                Param::Eq(_, EqField::Gain) => Some(0.0),
                                Param::Mute | Param::Eq(..) | Param::Mod(..) => None,
                            };
                            if let Some(g) = guide {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(b.origin.x, y(g)),
                                        size(b.size.width, px(1.)),
                                    ),
                                    alpha(0xffffff, 0.045),
                                ));
                            }
                            let faint = if off || live { 0.35 } else { 1.0 };
                            // The line as a list of window points, bends sampled.
                            let mut line: Vec<Point<Pixels>> = Vec::new();
                            match (shown.first(), shown.last()) {
                                (Some(f), Some(_)) => {
                                    line.push(point(b.origin.x.min(x(f.tick)), y(f.value)));
                                    for (n, p) in shown.iter().enumerate() {
                                        line.push(point(x(p.tick), y(p.value)));
                                        let Some(q) = shown.get(n + 1) else { continue };
                                        if p.step {
                                            line.push(point(x(q.tick), y(p.value)));
                                        } else if p.curve != 0.0 || p.ease {
                                            let w = f32::from(x(q.tick) - x(p.tick));
                                            let steps = (w / 3.0).clamp(6.0, 64.0) as usize;
                                            for s in 1..steps {
                                                let f = s as f32 / steps as f32;
                                                let v = p.value + (q.value - p.value) * p.along(f);
                                                let tx =
                                                    p.tick as f32 + (q.tick - p.tick) as f32 * f;
                                                line.push(point(
                                                    b.origin.x + px(tx * ppt - sx),
                                                    y(v),
                                                ));
                                            }
                                        }
                                    }
                                    let l = shown.last().unwrap();
                                    line.push(point(right.max(x(l.tick)), y(l.value)));
                                }
                                _ => {
                                    line.push(point(b.origin.x, y(level)));
                                    line.push(point(right, y(level)));
                                }
                            }
                            // The area under it, darkening down.
                            let mut area = PathBuilder::fill();
                            area.move_to(point(line[0].x, bottom));
                            for p in &line {
                                area.line_to(*p);
                            }
                            area.line_to(point(line[line.len() - 1].x, bottom));
                            area.close();
                            let strength = if shown.is_empty() { 0.04 } else { 0.2 } * faint;
                            if let Ok(p) = area.build() {
                                window.paint_path(
                                    p,
                                    linear_gradient(
                                        180.,
                                        linear_color_stop(alpha(color, strength), 0.),
                                        linear_color_stop(alpha(color, strength * 0.15), 1.),
                                    ),
                                );
                            }
                            let mut path =
                                PathBuilder::stroke(px(if shown.is_empty() { 1. } else { 1.6 }));
                            path.move_to(line[0]);
                            for p in &line[1..] {
                                path.line_to(*p);
                            }
                            if let Ok(p) = path.build() {
                                let ink = if shown.is_empty() { 0.3 } else { faint };
                                window.paint_path(p, alpha(color, ink));
                            }
                            // Where the playhead is on it.
                            if let Some((tick, v)) = lit_at {
                                let c = point(x(tick), y(v));
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            c - point(px(5.), px(5.)),
                                            size(px(10.), px(10.)),
                                        ),
                                        alpha(color, 0.25),
                                    )
                                    .corner_radii(px(5.)),
                                );
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            c - point(px(2.5), px(2.5)),
                                            size(px(5.), px(5.)),
                                        ),
                                        rgb(0xffffff),
                                    )
                                    .corner_radii(px(2.5)),
                                );
                            }
                            // A marquee being dragged.
                            if let Some((from, to, erase)) = marquee {
                                let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
                                let (y0, y1) = (from.1.min(to.1), from.1.max(to.1));
                                let tint = if erase { RECORD } else { ACCENT };
                                let r = Bounds::new(
                                    point(px(x0), px(y0)),
                                    size(px(x1 - x0), px(y1 - y0)),
                                );
                                window.paint_quad(
                                    fill(r, alpha(tint, 0.12))
                                        .border_widths(px(1.))
                                        .border_color(alpha(tint, 0.7)),
                                );
                            }
                            let _ = &picked_ticks;
                        },
                    )
                    .absolute()
                    .size_full()
                    .into_any_element(),
                );
                // Bend handles, halfway along each line wide enough, while the lane is hovered.
                for (n, w) in points.windows(2).enumerate() {
                    let (a, b) = (w[0], w[1]);
                    let (xa, xb) = (a.tick as f32 * ppt - sx, b.tick as f32 * ppt - sx);
                    if a.step || xb - xa < 28.0 || (a.value - b.value).abs() < 1e-6 || param.steps()
                    {
                        continue;
                    }
                    let mid = a.value + (b.value - a.value) * a.along(0.5);
                    let (cx_, cy) = ((xa + xb) / 2.0, top + to_y(param, mid, h));
                    let bent = a.curve != 0.0 || a.ease;
                    let curve = a.curve;
                    out.push(
                        div()
                            .absolute()
                            .left(px(cx_ - 5.))
                            .top(px(cy - 5.))
                            .size(px(10.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(if bent { 0.8 } else { 0.0 })
                            .group_hover(group.clone(), |s| s.opacity(1.0))
                            .cursor_ns_resize()
                            .debug_selector(move || format!("auto-bend-{t}-{k}-{n}"))
                            .child(
                                div()
                                    .size(px(6.))
                                    .rounded(px(1.))
                                    .bg(rgb(0x0b0b0e))
                                    .border_1()
                                    .border_color(alpha(color, 0.9)),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.select_track(t);
                                    if e.click_count == 2 {
                                        ws.edit(cx, |ws| {
                                            let pts = ws.session.project.tracks[t]
                                                .automation
                                                .points_mut(param);
                                            if let Some(q) = pts.get_mut(n) {
                                                q.curve = 0.0;
                                            }
                                        });
                                        return;
                                    }
                                    ws.begin_drag(
                                        Drag::AutoBend {
                                            track: t,
                                            param,
                                            index: n,
                                            y: f32::from(e.position.y),
                                            curve,
                                        },
                                        cx,
                                    );
                                }),
                            )
                            .into_any_element(),
                    );
                }
                // The points.
                for (n, q) in points.iter().enumerate() {
                    let px_ = q.tick as f32 * ppt - sx;
                    if px_ < -10.0 || px_ > 6000.0 {
                        continue;
                    }
                    let on = picked.contains(&n);
                    let py = top + to_y(param, q.value, h);
                    out.push(
                        div()
                            .absolute()
                            .left(px(px_ - 6.))
                            .top(px(py - 6.))
                            .size(px(12.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_move()
                            .debug_selector(move || format!("auto-point-{t}-{k}-{n}"))
                            .child(
                                div()
                                    .size(px(if on { 9. } else { 8. }))
                                    .rounded(px(5.))
                                    .bg(rgb(if on { 0xffffff } else { 0x0b0b0e }))
                                    .border_2()
                                    .border_color(rgb(color))
                                    .when(on, |d| {
                                        d.shadow(vec![gpui::BoxShadow {
                                            color: alpha(color, 0.7),
                                            offset: point(px(0.), px(0.)),
                                            blur_radius: px(6.),
                                            spread_radius: px(0.),
                                            inset: false,
                                        }])
                                    }),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.point_press(t, k, n, e, cx);
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    ws.select_track(t);
                                    ws.point_menu(t, k, n, e.position, cx);
                                }),
                            )
                            .into_any_element(),
                    );
                }
            }
        }
        // What a point being dragged is at, beside it.
        if let Some((t, param, index)) = match self.drag {
            Some(Drag::Auto {
                track,
                param,
                index,
                moved: true,
                ..
            }) => Some((track, param, index)),
            Some(Drag::AutoLine {
                track,
                param,
                moved: true,
                ..
            }) => self
                .picked_points(track, param)
                .first()
                .map(|&i| (track, param, i)),
            Some(Drag::AutoDraw { track, param, last }) => self.session.project.tracks[track]
                .automation
                .points(param)
                .iter()
                .position(|q| q.tick == last.0)
                .map(|i| (track, param, i)),
            _ => None,
        } && let Some(q) = self.session.project.tracks[t]
            .automation
            .points(param)
            .get(index)
        {
            let k = self
                .auto_lanes(t)
                .iter()
                .position(|&p| p == param)
                .unwrap_or(0);
            let x = q.tick as f32 * ppt - sx;
            let y = self.lane_top(t, k) + to_y(param, q.value, h);
            let [bar, beat, div_, _] = self.session.project.bbt(q.tick);
            let when = format!("{}.{}.{}", bar + 1, beat + 1, div_ + 1);
            out.push(
                div()
                    .absolute()
                    .left(px(x + 10.))
                    .top(px((y - 24.).max(self.lane_top(t, k))))
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(rgb(0x0b0b0e))
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT))
                    .whitespace_nowrap()
                    .child(format!("{}  ·  {when}", self.value_text(t, param, q.value)))
                    .into_any_element(),
            );
        }
        out
    }
}

/// Ticks in a beat, for naming a grid step.
const PPQ_BEAT: u64 = pina_engine::project::PPQ;
