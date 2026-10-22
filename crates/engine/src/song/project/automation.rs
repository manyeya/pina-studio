//! Automation: lanes of points, and every edit to them (points written, shapes, smoothing, moving with regions).

use super::*;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Automation {
    /// In dB.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volume: Vec<AutoPoint>,
    /// -1 to 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pan: Vec<AutoPoint>,
    /// 1 muted, 0 not: steps, never slopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mute: Vec<AutoPoint>,
    /// Send levels (dB), by the bus sent to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<SendAuto>,
    /// Plugin parameters over time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugins: Vec<PluginAuto>,
    /// The EQ's bands' frequencies, gains and widths over time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eq: Vec<EqAuto>,
    /// Its modulators' settings over time: a macro turned, an LFO's phase.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<ModAuto>,
    #[serde(default, skip_serializing_if = "AutoMode::is_read")]
    pub mode: AutoMode,
}

/// An EQ band's setting over time: the band by its place in the EQ; Hz, dB or Q.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EqAuto {
    pub band: u8,
    pub field: EqField,
    pub points: Vec<AutoPoint>,
}

/// A modulator's setting over time: the modulator by its place on the track.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ModAuto {
    pub modulator: u8,
    pub field: crate::dsp::modulate::ModParam,
    pub points: Vec<AutoPoint>,
}

/// A send's level over time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SendAuto {
    pub bus: u32,
    pub points: Vec<AutoPoint>,
}

/// A plugin parameter's automation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PluginAuto {
    pub plugin: Which,
    /// The parameter's id in the plugin.
    pub param: u32,
    /// Its name, for people and agents reading the project.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Values from 0 to 1.
    pub points: Vec<AutoPoint>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct AutoPoint {
    pub tick: u64,
    pub value: f32,
    /// How the line bends on to the next point: 0 straight; towards 1 it starts slowly and ends
    /// fast, towards -1 the other way round.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub curve: f32,
    /// Holds its value up to the next point, which it jumps to: a step.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub step: bool,
    /// Eases out of its value and into the next: an S, slow at both ends (`curve` aside).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ease: bool,
}

impl AutoPoint {
    pub fn new(tick: u64, value: f32) -> Self {
        AutoPoint {
            tick,
            value,
            ..Default::default()
        }
    }

    /// How far along its change the line on to the next point is, `t` of the way there.
    pub fn along(&self, t: f32) -> f32 {
        match self.ease {
            true => t * t * (3.0 - 2.0 * t),
            false => bend(t, self.curve),
        }
    }

    /// Whether the line on from it is anything but straight.
    pub fn shaped(&self) -> bool {
        self.step || self.ease || self.curve != 0.0
    }
}

/// A run of automation lifted out of a track, its ticks from where it was taken, to put down again
/// (copied points, or a region's automation going with it).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AutoClip {
    pub length: u64,
    pub lanes: Vec<(Param, Vec<AutoPoint>)>,
}

/// A shape drawn into automation, a cycle every so often.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoShape {
    Sine,
    Triangle,
    SawUp,
    SawDown,
    Square,
    Random,
    /// Up from low to high across the whole span, as a fade in; `RampDown` the other way.
    RampUp,
    RampDown,
}

impl AutoShape {
    pub const ALL: [AutoShape; 8] = [
        AutoShape::Sine,
        AutoShape::Triangle,
        AutoShape::SawUp,
        AutoShape::SawDown,
        AutoShape::Square,
        AutoShape::Random,
        AutoShape::RampUp,
        AutoShape::RampDown,
    ];

    pub fn name(self) -> &'static str {
        match self {
            AutoShape::Sine => "Sine",
            AutoShape::Triangle => "Triangle",
            AutoShape::SawUp => "Saw up",
            AutoShape::SawDown => "Saw down",
            AutoShape::Square => "Square",
            AutoShape::Random => "Random",
            AutoShape::RampUp => "Ramp up",
            AutoShape::RampDown => "Ramp down",
        }
    }

    /// Its points from `from` to `to`, a cycle every `every` ticks, between `low` and `high`.
    pub fn points(self, from: u64, to: u64, every: u64, low: f32, high: f32) -> Vec<AutoPoint> {
        let mid = (low + high) / 2.0;
        let every = every.max(1);
        let at = |tick: u64, value: f32, curve: f32, step: bool| AutoPoint {
            tick,
            value,
            curve,
            step,
            ease: false,
        };
        let mut out = Vec::new();
        match self {
            AutoShape::RampUp | AutoShape::RampDown => {
                let (a, b) = match self {
                    AutoShape::RampUp => (low, high),
                    _ => (high, low),
                };
                out.push(at(from, a, 0.0, false));
                out.push(at(to, b, 0.0, false));
                return out;
            }
            _ => {}
        }
        let mut start = from;
        let mut n = 0u64;
        while start < to {
            let q = |k: u64| (start + every * k / 4).min(to);
            match self {
                // A quarter cycle at a time, each bent as a sine's is.
                AutoShape::Sine => {
                    for (k, value, curve) in [
                        (0, mid, -0.3),
                        (1, high, 0.3),
                        (2, mid, -0.3),
                        (3, low, 0.3),
                    ] {
                        if q(k) < to || k == 0 {
                            out.push(at(q(k), value, curve, false));
                        }
                    }
                }
                AutoShape::Triangle => {
                    out.push(at(start, low, 0.0, false));
                    if q(2) < to {
                        out.push(at(q(2), high, 0.0, false));
                    }
                }
                AutoShape::SawUp | AutoShape::SawDown => {
                    let (a, b) = match self {
                        AutoShape::SawUp => (low, high),
                        _ => (high, low),
                    };
                    let end = (start + every).min(to);
                    out.push(at(start, a, 0.0, false));
                    out.push(at(end, b, 0.0, false));
                }
                AutoShape::Square => {
                    out.push(at(start, high, 0.0, true));
                    if q(2) < to {
                        out.push(at(q(2), low, 0.0, true));
                    }
                }
                AutoShape::Random => {
                    let r = crate::dsp::modulate::roll(n as i64, 7);
                    out.push(at(start, low + (high - low) * r, 0.0, true));
                }
                AutoShape::RampUp | AutoShape::RampDown => unreachable!(),
            }
            start += every;
            n += 1;
        }
        // It ends where the next cycle would start.
        let last = match self {
            AutoShape::Sine => mid,
            AutoShape::Triangle => low,
            AutoShape::SawUp | AutoShape::SawDown => return out,
            _ => out.last().map_or(mid, |p| p.value),
        };
        if out.last().is_some_and(|p| p.tick < to) {
            out.push(at(to, last, 0.0, false));
        }
        out
    }
}

impl Automation {
    pub fn is_empty(&self) -> bool {
        self.volume.is_empty()
            && self.pan.is_empty()
            && self.mute.is_empty()
            && self.sends.is_empty()
            && self.plugins.is_empty()
            && self.eq.is_empty()
            && self.mods.is_empty()
            && self.mode == AutoMode::Read
    }

    pub fn points(&self, param: Param) -> &[AutoPoint] {
        match param {
            Param::Volume => &self.volume,
            Param::Pan => &self.pan,
            Param::Mute => &self.mute,
            Param::Send(bus) => self
                .sends
                .iter()
                .find(|a| a.bus == bus)
                .map_or(&[], |a| &a.points),
            Param::Plugin(which, id) => self
                .plugins
                .iter()
                .find(|a| a.plugin == which && a.param == id)
                .map_or(&[], |a| &a.points),
            Param::Eq(band, field) => self
                .eq
                .iter()
                .find(|a| a.band == band && a.field == field)
                .map_or(&[], |a| &a.points),
            Param::Mod(k, field) => self
                .mods
                .iter()
                .find(|a| a.modulator == k && a.field == field)
                .map_or(&[], |a| &a.points),
        }
    }

    /// A parameter's points, making it a lane if it has none.
    pub fn points_mut(&mut self, param: Param) -> &mut Vec<AutoPoint> {
        match param {
            Param::Volume => &mut self.volume,
            Param::Pan => &mut self.pan,
            Param::Mute => &mut self.mute,
            Param::Send(bus) => {
                let i = match self.sends.iter().position(|a| a.bus == bus) {
                    Some(i) => i,
                    None => {
                        self.sends.push(SendAuto {
                            bus,
                            points: Vec::new(),
                        });
                        self.sends.len() - 1
                    }
                };
                &mut self.sends[i].points
            }
            Param::Plugin(plugin, param) => {
                let i = match self
                    .plugins
                    .iter()
                    .position(|a| a.plugin == plugin && a.param == param)
                {
                    Some(i) => i,
                    None => {
                        self.plugins.push(PluginAuto {
                            plugin,
                            param,
                            name: String::new(),
                            points: Vec::new(),
                        });
                        self.plugins.len() - 1
                    }
                };
                &mut self.plugins[i].points
            }
            Param::Eq(band, field) => {
                let i = match self
                    .eq
                    .iter()
                    .position(|a| a.band == band && a.field == field)
                {
                    Some(i) => i,
                    None => {
                        self.eq.push(EqAuto {
                            band,
                            field,
                            points: Vec::new(),
                        });
                        self.eq.len() - 1
                    }
                };
                &mut self.eq[i].points
            }
            Param::Mod(modulator, field) => {
                let i = match self
                    .mods
                    .iter()
                    .position(|a| a.modulator == modulator && a.field == field)
                {
                    Some(i) => i,
                    None => {
                        self.mods.push(ModAuto {
                            modulator,
                            field,
                            points: Vec::new(),
                        });
                        self.mods.len() - 1
                    }
                };
                &mut self.mods[i].points
            }
        }
    }

    /// A modulator taken off the track: its lanes go, and those of the ones after it follow
    /// it down a place.
    pub fn modulator_removed(&mut self, k: usize) {
        self.mods.retain(|a| a.modulator as usize != k);
        for a in &mut self.mods {
            if a.modulator as usize > k {
                a.modulator -= 1;
            }
        }
    }

    /// Names a plugin parameter's lane, so the project says what it is.
    pub fn name(&mut self, param: Param, name: &str) {
        if let Param::Plugin(which, id) = param
            && let Some(a) = self
                .plugins
                .iter_mut()
                .find(|a| a.plugin == which && a.param == id)
        {
            a.name = name.to_string();
        }
    }

    /// Drops plugin, send, EQ and modulator lanes left without points.
    pub fn tidy(&mut self) {
        self.plugins.retain(|a| !a.points.is_empty());
        self.sends.retain(|a| !a.points.is_empty());
        self.eq.retain(|a| !a.points.is_empty());
        self.mods.retain(|a| !a.points.is_empty());
    }

    /// Every parameter with points, faders first.
    pub fn params(&self) -> Vec<Param> {
        let mut out = Vec::new();
        if !self.volume.is_empty() {
            out.push(Param::Volume);
        }
        if !self.pan.is_empty() {
            out.push(Param::Pan);
        }
        if !self.mute.is_empty() {
            out.push(Param::Mute);
        }
        out.extend(
            self.sends
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| Param::Send(a.bus)),
        );
        out.extend(
            self.plugins
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| Param::Plugin(a.plugin, a.param)),
        );
        out.extend(
            self.eq
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| Param::Eq(a.band, a.field)),
        );
        out.extend(
            self.mods
                .iter()
                .filter(|a| !a.points.is_empty())
                .map(|a| Param::Mod(a.modulator, a.field)),
        );
        out
    }

    /// Records a control's value at `tick` while the song plays, replacing the line since
    /// `after`: the last tick this pass recorded, or None on its first, which keeps the line as it
    /// was up to here.
    pub fn write(&mut self, param: Param, after: Option<u64>, tick: u64, value: f32) {
        let points = self.points_mut(param);
        let from = match after {
            Some(a) => a.min(tick),
            None => {
                let before = tick.saturating_sub(1);
                if let Some(old) = value_at(points, tick as f64)
                    && tick > 0
                    && !points.iter().any(|p| p.tick == before)
                {
                    let i = points.partition_point(|p| p.tick < before);
                    points.insert(
                        i,
                        AutoPoint {
                            step: param.steps(),
                            ..AutoPoint::new(before, old)
                        },
                    );
                }
                before
            }
        };
        points.retain(|p| p.tick <= from || p.tick > tick);
        let n = points.partition_point(|p| p.tick <= from);
        // Holding still needs no points in between: the last one moves along.
        if after.is_some() && n >= 2 && points[n - 1].value == value && points[n - 2].value == value
        {
            points[n - 1].tick = tick;
        } else {
            points.insert(
                n,
                AutoPoint {
                    step: param.steps(),
                    ..AutoPoint::new(tick, value)
                },
            );
        }
    }

    /// Plugin lanes after an insert is taken out of slot `i`: its own go, later ones move up.
    pub(crate) fn insert_removed(&mut self, i: usize) {
        self.plugins.retain(|a| a.plugin != Which::Insert(i));
        for a in &mut self.plugins {
            if let Which::Insert(j) = &mut a.plugin
                && *j > i
            {
                *j -= 1;
            }
        }
    }

    /// Drops a plugin's lanes, when another plugin takes its place.
    pub(crate) fn forget(&mut self, which: Which) {
        self.plugins.retain(|a| a.plugin != which);
    }

    /// Adds a point in time order and returns its index.
    pub fn add(&mut self, param: Param, point: AutoPoint) -> usize {
        let points = self.points_mut(param);
        let i = points.partition_point(|p| p.tick <= point.tick);
        points.insert(i, point);
        i
    }

    /// Moves a point, but not past its neighbours.
    pub fn set(&mut self, param: Param, i: usize, tick: u64, value: f32) {
        let points = self.points_mut(param);
        let lo = if i > 0 { points[i - 1].tick } else { 0 };
        let hi = points.get(i + 1).map_or(u64::MAX, |p| p.tick);
        points[i] = AutoPoint {
            tick: tick.clamp(lo, hi),
            value,
            ..points[i]
        };
    }

    /// Puts `new` in place of the line from `from` to `to`, keeping it as it was either side
    /// (where it had points): it jumps into the new and back out.
    pub fn replace(&mut self, param: Param, from: u64, to: u64, new: Vec<AutoPoint>) {
        let points = self.points_mut(param);
        let had = !points.is_empty();
        let before = value_at(points, from as f64 - 0.5);
        let after = value_at(points, to as f64);
        let stepped = |i: usize, points: &Vec<AutoPoint>| i > 0 && points[i - 1].step;
        let i = points.partition_point(|p| p.tick < from);
        let j = points.partition_point(|p| p.tick <= to);
        // A step that ran into the span still holds up to it.
        let held = stepped(i, points);
        let mut put: Vec<AutoPoint> = Vec::new();
        if had && from > 0 {
            put.push(AutoPoint {
                step: held || param.steps(),
                ..AutoPoint::new(from, before.unwrap_or(0.0))
            });
        }
        put.extend(
            new.into_iter()
                .filter(|p| (from..=to).contains(&p.tick))
                .map(|p| AutoPoint {
                    step: p.step || param.steps(),
                    ..p
                }),
        );
        if had && let Some(after) = after {
            put.push(AutoPoint {
                step: param.steps() || points.get(j.saturating_sub(1)).is_some_and(|p| p.step),
                ..AutoPoint::new(to, after)
            });
        }
        points.splice(i..j, put);
        self.tidy();
    }

    /// Draws a shape from `from` to `to`, a cycle every `every` ticks, over what was there.
    #[allow(clippy::too_many_arguments)]
    pub fn shape(
        &mut self,
        param: Param,
        shape: AutoShape,
        from: u64,
        to: u64,
        every: u64,
        low: f32,
        high: f32,
    ) {
        let (lo, hi) = param.range();
        let (low, high) = (low.clamp(lo, hi), high.clamp(lo, hi));
        self.replace(param, from, to, shape.points(from, to, every, low, high));
    }

    /// Moves the whole line up or down by `by` (dB for the fader and sends), each point within
    /// the parameter's range.
    pub fn offset(&mut self, param: Param, by: f32) {
        let (lo, hi) = param.range();
        for p in self.points_mut(param) {
            p.value = (p.value + by).clamp(lo, hi);
        }
    }

    /// Takes out points that change nothing: each within `tolerance` (a share of the
    /// parameter's range) of the straight line its neighbours would draw without it. Steps and
    /// curves stay.
    pub fn thin(&mut self, param: Param, tolerance: f32) {
        let (lo, hi) = param.range();
        // The fader's range is mostly silence: judge it by the top 60 dB.
        let span = match param {
            Param::Volume | Param::Send(_) => 66.0,
            // On an octave scale a straight line in Hz isn't one: keep every point.
            p if p.logarithmic() => 0.0,
            _ => hi - lo,
        };
        let tol = tolerance * span;
        let points = self.points_mut(param);
        let n = points.len();
        if n < 3 {
            return;
        }
        let mut keep = vec![true; n];
        // Douglas-Peucker over each run of straight segments.
        let fixed = |i: usize| {
            i == 0
                || i == n - 1
                || points[i].shaped()
                || points[i - 1].shaped()
                || points[i].tick == points[i - 1].tick
                || points[i + 1].tick == points[i].tick
        };
        let mut a = 0;
        for b in 1..n {
            if fixed(b) {
                simplify(points, a, b, tol, &mut keep);
                a = b;
            }
        }
        let mut k = 0;
        points.retain(|_| {
            k += 1;
            keep[k - 1]
        });
    }

    /// Lifts out every lane's points from `from` to `to` (as they sound: with the line's values
    /// at either end), their ticks from `from`.
    pub fn take(&self, from: u64, to: u64) -> AutoClip {
        let mut lanes = Vec::new();
        for param in self.params() {
            let points = self.points(param);
            let mut out: Vec<AutoPoint> = Vec::new();
            if let Some(v) = value_at(points, from as f64) {
                out.push(AutoPoint {
                    step: param.steps()
                        || points
                            .iter()
                            .rev()
                            .find(|p| p.tick <= from)
                            .is_some_and(|p| p.step),
                    ..AutoPoint::new(0, v)
                });
            }
            out.extend(
                points
                    .iter()
                    .filter(|p| p.tick > from && p.tick < to)
                    .map(|p| AutoPoint {
                        tick: p.tick - from,
                        ..*p
                    }),
            );
            if let Some(v) = value_at(points, to as f64 - 0.5) {
                out.push(AutoPoint::new(to - from, v));
            }
            lanes.push((param, out));
        }
        AutoClip {
            length: to.saturating_sub(from),
            lanes,
        }
    }

    /// Puts a lifted run down at `at`, over what was there for its length, lane by lane.
    pub fn put(&mut self, at: u64, clip: &AutoClip) {
        for (param, points) in &clip.lanes {
            let shifted = points
                .iter()
                .map(|p| AutoPoint {
                    tick: p.tick + at,
                    ..*p
                })
                .collect();
            self.replace(*param, at, at + clip.length, shifted);
        }
    }

    /// Smooths the line from `from` to `to`: blurred over about `width` ticks either side (on
    /// the fader's scale for levels), its ends left where they were so it joins the line either
    /// side, then as few points as keep its shape. Steps and bends inside go smooth too; mute,
    /// all steps, stays as it is.
    pub fn smooth(&mut self, param: Param, from: u64, to: u64, width: u64) {
        let points = self.points(param);
        if param.steps() || to <= from || points.len() < 2 {
            return;
        }
        let width = width.max(1) as f64;
        let n = (((to - from) as f64 / (width / 4.0).clamp(10.0, PPQ as f64 / 16.0)).ceil()
            as usize)
            .max(2);
        let step = (to - from) as f64 / n as f64;
        let reach = (3.0 * width / step).ceil() as i64;
        // The line as it is, sampled out past both ends, as a share of its range.
        let line: Vec<f32> = (-reach..=n as i64 + reach)
            .map(|k| {
                let t = (from as f64 + k as f64 * step).max(0.0);
                param.to_unit(value_at(points, t).unwrap_or(0.0))
            })
            .collect();
        let kernel: Vec<f32> = (-reach..=reach)
            .map(|j| (-(j as f64 * step / width).powi(2) / 2.0).exp() as f32)
            .collect();
        let total: f32 = kernel.iter().sum();
        let mut blurred: Vec<f32> = (0..=n)
            .map(|k| {
                let around = &line[k..k + kernel.len()];
                around.iter().zip(&kernel).map(|(v, g)| v * g).sum::<f32>() / total
            })
            .collect();
        // Its ends pinned where they were.
        let r = reach as usize;
        let (d0, d1) = (line[r] - blurred[0], line[r + n] - blurred[n]);
        for (k, v) in blurred.iter_mut().enumerate() {
            let u = k as f32 / n as f32;
            *v = (*v + d0 * (1.0 - u) + d1 * u).clamp(0.0, 1.0);
        }
        let sampled: Vec<AutoPoint> = blurred
            .iter()
            .enumerate()
            .map(|(k, &u)| AutoPoint::new(from + (k as f64 * step).round() as u64, u))
            .collect();
        let mut keep = vec![true; sampled.len()];
        simplify(&sampled, 0, sampled.len() - 1, 0.004, &mut keep);
        let ends = (value_at(points, from as f64), value_at(points, to as f64));
        let mut smooth: Vec<AutoPoint> = sampled
            .iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(p, _)| AutoPoint::new(p.tick, param.from_unit(p.value)))
            .collect();
        // Exactly where they were, not as near as a round trip through the fader's scale.
        if let (Some(a), Some(b)) = ends {
            let last = smooth.len() - 1;
            (smooth[0].value, smooth[last].value) = (a, b);
        }
        let points = self.points_mut(param);
        let i = points.partition_point(|p| p.tick < from);
        let j = points.partition_point(|p| p.tick <= to);
        points.splice(i..j, smooth);
    }

    /// Takes out every lane's points from `from` to `to`, the line either side kept as it was.
    pub fn erase(&mut self, from: u64, to: u64) {
        for param in self.params() {
            let points = self.points(param);
            let (a, b) = (
                value_at(points, from as f64 - 0.5),
                value_at(points, to as f64),
            );
            let mut put = Vec::new();
            if let Some(a) = a.filter(|_| from > 0) {
                put.push(AutoPoint::new(from, a));
            }
            if let Some(b) = b {
                put.push(AutoPoint::new(to, b));
            }
            let points = self.points_mut(param);
            points.retain(|p| p.tick < from || p.tick > to);
            let i = points.partition_point(|p| p.tick < from);
            points.splice(i..i, put);
        }
        self.tidy();
    }
}

/// Keeps the points between `a` and `b` that bend the line more than `tol` (Douglas-Peucker).
pub(crate) fn simplify(points: &[AutoPoint], a: usize, b: usize, tol: f32, keep: &mut [bool]) {
    if b <= a + 1 {
        return;
    }
    let (pa, pb) = (points[a], points[b]);
    let line = |t: u64| {
        let f = (t - pa.tick) as f32 / (pb.tick - pa.tick).max(1) as f32;
        pa.value + (pb.value - pa.value) * f
    };
    let (far, off) = (a + 1..b)
        .map(|i| (i, (points[i].value - line(points[i].tick)).abs()))
        .fold((a, 0.0), |m, x| if x.1 > m.1 { x } else { m });
    if off > tol {
        simplify(points, a, far, tol, keep);
        simplify(points, far, b, tol, keep);
    } else {
        keep[a + 1..b].iter_mut().for_each(|k| *k = false);
    }
}

/// How far along a segment's change a bent line is, `t` of the way along it.
pub fn bend(t: f32, curve: f32) -> f32 {
    if curve.abs() < 1e-3 {
        return t;
    }
    let k = curve.clamp(-1.0, 1.0) * 6.0;
    ((k * t).exp() - 1.0) / (k.exp() - 1.0)
}

/// A parameter's value at a tick: lines (straight, bent or eased by the point they start from)
/// between points, steps after a stepped one, level before the first and after the last. None
/// without points.
pub fn value_at(points: &[AutoPoint], tick: f64) -> Option<f32> {
    let i = points.partition_point(|p| (p.tick as f64) <= tick);
    match (i.checked_sub(1).map(|i| points[i]), points.get(i)) {
        (None, None) => None,
        (Some(a), None) => Some(a.value),
        (None, Some(b)) => Some(b.value),
        (Some(a), Some(_)) if a.step => Some(a.value),
        (Some(a), Some(b)) => {
            let t = ((tick - a.tick as f64) / (b.tick - a.tick) as f64) as f32;
            Some(a.value + (b.value - a.value) * a.along(t))
        }
    }
}
