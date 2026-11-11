//! A note's own expression, as an MPE controller plays it: its pitch, slide and pressure, each a
//! curve of points over the note. Kept on the note, so it goes wherever the note goes: moved,
//! copied, transposed, split.

use serde::{Deserialize, Serialize};

use super::{
    Content, ControlPoint, ControlValue, ExpressionKind, Note, Project, sampler::is_zero_f32,
};
use crate::plugin::ControlKind;

/// How far a note's pitch bends either way, in semitones: MPE's usual reach.
pub const BEND_REACH: f32 = 48.0;

/// The three things a note's expression moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dim {
    /// Semitones off the note's own pitch.
    Pitch,
    /// The finger sliding up the key (CC 74), 0 to 1.
    Slide,
    /// How hard it's pressed, 0 to 1.
    Press,
}

impl Dim {
    pub const ALL: [Dim; 3] = [Dim::Pitch, Dim::Slide, Dim::Press];

    pub fn name(self) -> &'static str {
        match self {
            Dim::Pitch => "Pitch",
            Dim::Slide => "Slide",
            Dim::Press => "Press",
        }
    }

    /// The lowest and highest it goes.
    pub fn range(self) -> (f32, f32) {
        match self {
            Dim::Pitch => (-BEND_REACH, BEND_REACH),
            _ => (0.0, 1.0),
        }
    }

    /// Where it sits untouched.
    pub fn rest(self) -> f32 {
        match self {
            Dim::Slide => 0.5,
            _ => 0.0,
        }
    }

    /// How far a point may be left out of a line before the line looks different.
    pub fn tolerance(self) -> f32 {
        match self {
            Dim::Pitch => 0.03,
            _ => 0.004,
        }
    }

    /// The move it makes for the note at `pitch`.
    pub fn control(self, pitch: u8, value: f32) -> ControlKind {
        match self {
            Dim::Pitch => ControlKind::Expression {
                pitch,
                kind: ExpressionKind::Tuning,
                value,
            },
            Dim::Slide => ControlKind::Expression {
                pitch,
                kind: ExpressionKind::Brightness,
                value,
            },
            Dim::Press => ControlKind::PolyPressure { pitch, value },
        }
    }

    /// Of a region's control moves, a note's own of this kind: its pitch and value.
    fn of(v: &ControlValue) -> Option<(u8, Dim, f32)> {
        Some(match *v {
            ControlValue::Expression {
                pitch,
                kind: ExpressionKind::Tuning,
                value,
            } => (pitch, Dim::Pitch, value.clamp(-BEND_REACH, BEND_REACH)),
            ControlValue::Expression {
                pitch,
                kind: ExpressionKind::Brightness,
                value,
            } => (pitch, Dim::Slide, value.clamp(0.0, 1.0)),
            ControlValue::PolyPressure { pitch, value } => {
                (pitch, Dim::Press, value.min(127) as f32 / 127.0)
            }
            _ => return None,
        })
    }
}

/// A point of a note's curve: `at` ticks into the note, its value, and how the line on to the
/// next bows: 0 straight, toward 1 slow then fast, toward -1 fast then slow.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Bend {
    pub at: u64,
    pub value: f32,
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub curve: f32,
}

impl Bend {
    pub fn new(at: u64, value: f32) -> Bend {
        Bend {
            at,
            value,
            curve: 0.0,
        }
    }
}

/// How far up a bowed line is, `f` of the way along it.
pub fn shape(f: f32, curve: f32) -> f32 {
    let k = curve.clamp(-1.0, 1.0) * 6.0;
    let f = f.clamp(0.0, 1.0);
    match k.abs() < 1e-3 {
        true => f,
        false => ((k * f).exp() - 1.0) / (k.exp() - 1.0),
    }
}

/// A curve's value `at`: the first point's before it, the last's after, between two along the
/// line from one to the next. None with no points.
pub fn curve_at(points: &[Bend], at: u64) -> Option<f32> {
    let next = points.partition_point(|b| b.at <= at);
    Some(
        match (next.checked_sub(1).map(|k| points[k]), points.get(next)) {
            (None, Some(b)) => b.value,
            (Some(a), None) => a.value,
            (Some(a), Some(b)) => {
                let f = (at - a.at) as f32 / (b.at - a.at).max(1) as f32;
                a.value + (b.value - a.value) * shape(f, a.curve)
            }
            (None, None) => return None,
        },
    )
}

/// Fewer points drawing nearly the same line: each kept only if leaving it out would move the
/// line by more than `tolerance` (Ramer-Douglas-Peucker, measured up and down). Curves kept.
pub fn thin(points: &[Bend], tolerance: f32) -> Vec<Bend> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    let (first, last) = (0, points.len() - 1);
    keep[first] = true;
    keep[last] = true;
    let mut spans = vec![(first, last)];
    while let Some((a, z)) = spans.pop() {
        let (pa, pz) = (points[a], points[z]);
        let line = |b: &Bend| {
            let f = (b.at - pa.at) as f32 / (pz.at - pa.at).max(1) as f32;
            pa.value + (pz.value - pa.value) * shape(f, pa.curve)
        };
        let far = (a + 1..z)
            .map(|k| (k, (points[k].value - line(&points[k])).abs()))
            .max_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((k, d)) = far
            && d > tolerance
        {
            keep[k] = true;
            spans.push((a, k));
            spans.push((k, z));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(b, k)| k.then_some(*b))
        .collect()
}

/// A note's own pitch, slide and pressure over its length: a curve of points each, empty when it
/// has none.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct NoteExpression {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pitch: Vec<Bend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slide: Vec<Bend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub press: Vec<Bend>,
}

impl NoteExpression {
    pub fn is_empty(&self) -> bool {
        self.pitch.is_empty() && self.slide.is_empty() && self.press.is_empty()
    }

    pub fn get(&self, d: Dim) -> &[Bend] {
        match d {
            Dim::Pitch => &self.pitch,
            Dim::Slide => &self.slide,
            Dim::Press => &self.press,
        }
    }

    pub fn get_mut(&mut self, d: Dim) -> &mut Vec<Bend> {
        match d {
            Dim::Pitch => &mut self.pitch,
            Dim::Slide => &mut self.slide,
            Dim::Press => &mut self.press,
        }
    }

    /// Its value `at` ticks into the note, if it has that curve.
    pub fn value_at(&self, d: Dim, at: u64) -> Option<f32> {
        curve_at(self.get(d), at)
    }

    /// Cut `at` ticks in: what's after goes to what it returns (from 0, starting where the
    /// line was), what's before stays (ending there).
    pub fn split(&mut self, at: u64) -> NoteExpression {
        let mut after = NoteExpression::default();
        for d in Dim::ALL {
            let Some(there) = self.value_at(d, at) else {
                continue;
            };
            let points = self.get_mut(d);
            let cut = points.partition_point(|b| b.at < at);
            let mut tail: Vec<Bend> = points.split_off(cut);
            if tail.first().is_none_or(|b| b.at > at) {
                tail.insert(0, Bend::new(at, there));
            }
            for b in &mut tail {
                b.at -= at;
            }
            points.push(Bend::new(at, there));
            *after.get_mut(d) = tail;
        }
        after
    }

    /// Every point's time times `by`: the note stretched, or squeezed.
    pub fn stretch(&mut self, by: f64) {
        for d in Dim::ALL {
            for b in self.get_mut(d) {
                b.at = (b.at as f64 * by).round().max(0.0) as u64;
            }
        }
    }

    /// The note's moves as it plays, for the note at `pitch` lasting `length`: each curve's
    /// value as it begins, at each point, and along a line that changes every `step` ticks.
    pub fn moves(&self, pitch: u8, length: u64, step: u64) -> Vec<(u64, ControlKind)> {
        let step = step.max(1);
        let mut out = Vec::new();
        for d in Dim::ALL {
            let points = self.get(d);
            let Some(first) = curve_at(points, 0) else {
                continue;
            };
            out.push((0, d.control(pitch, first)));
            for w in points.windows(2) {
                let (a, b) = (w[0], w[1]);
                if a.value != b.value {
                    let mut t = a.at + step;
                    while t < b.at && t < length {
                        let f = (t - a.at) as f32 / (b.at - a.at) as f32;
                        let v = a.value + (b.value - a.value) * shape(f, a.curve);
                        out.push((t, d.control(pitch, v)));
                        t += step;
                    }
                }
                if b.at > 0 && b.at < length {
                    out.push((b.at, d.control(pitch, b.value)));
                }
            }
        }
        out.sort_by_key(|m| m.0);
        out
    }
}

/// Moves the notes' own expression from among a region's controls onto the notes (as songs kept
/// it before, and as it still comes from a recording, a file or an agent): each pitch, slide and
/// pressure move to the note of its pitch sounding then (the latest begun), each curve thinned
/// to the points that shape it. Moves no note sounds for stay.
pub fn adopt_expression(notes: &mut [Note], controls: &mut Vec<ControlPoint>) {
    if !controls.iter().any(|c| Dim::of(&c.kind).is_some()) {
        return;
    }
    let mut touched = vec![false; notes.len()];
    controls.retain(|c| {
        let Some((pitch, d, value)) = Dim::of(&c.kind) else {
            return true;
        };
        let owner = notes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                n.pitch == pitch && n.start <= c.start && c.start < n.start + n.length as i64
            })
            .max_by_key(|(_, n)| n.start)
            .map(|(i, _)| i);
        let Some(i) = owner else {
            return true;
        };
        let at = (c.start - notes[i].start) as u64;
        notes[i].expression.get_mut(d).push(Bend::new(at, value));
        touched[i] = true;
        false
    });
    for (n, _) in notes.iter_mut().zip(touched).filter(|(_, t)| *t) {
        for d in Dim::ALL {
            let points = n.expression.get_mut(d);
            points.sort_by_key(|b| b.at);
            // Of two at one time, the later said.
            let mut once: Vec<Bend> = Vec::with_capacity(points.len());
            for b in points.drain(..) {
                match once.last_mut() {
                    Some(last) if last.at == b.at => *last = b,
                    _ => once.push(b),
                }
            }
            *points = thin(&once, d.tolerance());
        }
    }
}

/// A shape of expression written onto a note in one go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gesture {
    /// Up into the note from below.
    Scoop,
    /// Dropping away at its end.
    Fall,
    /// Bent up, and held there.
    BendUp,
    /// Bent down, and held there.
    BendDown,
    /// A vibrato that comes in after the note starts.
    Vibrato,
    /// Sliding into the next note's pitch at its end.
    Glide,
    /// Slide rising over the note (a filter opening).
    Open,
    /// Slide falling over the note.
    Close,
    /// Slide wobbling.
    Wobble,
    /// Pressure growing, then easing.
    Swell,
    /// Pressure fading away.
    Fade,
    /// Pressed hard, then let ease.
    Accent,
    /// Pressure pulsing.
    Tremolo,
}

impl Gesture {
    pub const ALL: [Gesture; 13] = [
        Gesture::Scoop,
        Gesture::Fall,
        Gesture::BendUp,
        Gesture::BendDown,
        Gesture::Vibrato,
        Gesture::Glide,
        Gesture::Open,
        Gesture::Close,
        Gesture::Wobble,
        Gesture::Swell,
        Gesture::Fade,
        Gesture::Accent,
        Gesture::Tremolo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Gesture::Scoop => "Scoop",
            Gesture::Fall => "Fall",
            Gesture::BendUp => "Bend up",
            Gesture::BendDown => "Bend down",
            Gesture::Vibrato => "Vibrato",
            Gesture::Glide => "Glide",
            Gesture::Open => "Open",
            Gesture::Close => "Close",
            Gesture::Wobble => "Wobble",
            Gesture::Swell => "Swell",
            Gesture::Fade => "Fade",
            Gesture::Accent => "Accent",
            Gesture::Tremolo => "Tremolo",
        }
    }

    /// What it moves.
    pub fn dim(self) -> Dim {
        match self {
            Gesture::Scoop
            | Gesture::Fall
            | Gesture::BendUp
            | Gesture::BendDown
            | Gesture::Vibrato
            | Gesture::Glide => Dim::Pitch,
            Gesture::Open | Gesture::Close | Gesture::Wobble => Dim::Slide,
            _ => Dim::Press,
        }
    }

    /// Its points on a note `length` ticks long (`beat` ticks a beat), at `amount` (1 as it
    /// usually goes), waving `rate` times a beat; a glide goes `to` semitones (the next note's
    /// pitch from this one's), and without one there's nothing to glide to.
    pub fn points(
        self,
        length: u64,
        beat: u64,
        amount: f32,
        rate: f32,
        to: Option<f32>,
    ) -> Vec<Bend> {
        let l = length.max(1);
        let a = amount.max(0.0);
        let at = |f: f32| (l as f32 * f).round() as u64;
        let curved = |at: u64, value: f32, curve: f32| Bend { at, value, curve };
        let (lo, hi) = (
            |v: f32| v.clamp(0.0, 1.0),
            |v: f32| v.clamp(-BEND_REACH, BEND_REACH),
        );
        // A wave about `mid`, `swing` either way, from `from` (coming in over as long again): a
        // point a quarter of a turn, each line bowed as a sine goes, quick from a crossing and
        // slow from a peak.
        let wave = |mid: f32, swing: f32, from: u64, keep: fn(f32) -> f32| {
            let every = (beat as f32 / rate.max(0.1) / 4.0).max(1.0);
            let mut out = vec![Bend::new(0, keep(mid))];
            let mut k = 0;
            loop {
                let t = from as f32 + k as f32 * every;
                if t > l as f32 {
                    break;
                }
                let grow = ((t - from as f32) / from.max(1) as f32).min(1.0);
                let s = [0.0, 1.0, 0.0, -1.0][k % 4];
                let bow = if k % 2 == 0 { -0.3 } else { 0.3 };
                out.push(curved(t as u64, keep(mid + s * swing * grow), bow));
                k += 1;
            }
            out
        };
        let short = |part: f32, most: u64| at(part).min(most).max(1);
        match self {
            Gesture::Scoop => vec![
                curved(0, hi(-2.0 * a), -0.5),
                Bend::new(short(0.25, beat / 4), 0.0),
            ],
            Gesture::Fall => vec![
                curved(l - short(0.35, beat / 2), 0.0, 0.6),
                Bend::new(l, hi(-5.0 * a)),
            ],
            Gesture::BendUp | Gesture::BendDown => {
                let sign = if self == Gesture::BendUp { 1.0 } else { -1.0 };
                vec![
                    curved(short(0.15, beat / 4), 0.0, -0.3),
                    Bend::new(at(0.5), hi(sign * 2.0 * a)),
                ]
            }
            Gesture::Vibrato => wave(0.0, 0.4 * a, short(0.25, beat / 2), hi),
            Gesture::Glide => match to {
                Some(to) if to != 0.0 => vec![curved(at(0.55), 0.0, 0.4), Bend::new(l, hi(to))],
                _ => Vec::new(),
            },
            Gesture::Open => vec![
                Bend::new(0, lo(0.5 - 0.4 * a)),
                Bend::new(l, lo(0.5 + 0.4 * a)),
            ],
            Gesture::Close => vec![
                Bend::new(0, lo(0.5 + 0.4 * a)),
                Bend::new(l, lo(0.5 - 0.4 * a)),
            ],
            Gesture::Wobble => wave(0.5, 0.35 * a, 1, lo),
            Gesture::Swell => vec![
                Bend::new(0, lo(0.1)),
                curved(at(0.7), lo(0.9 * a), 0.0),
                Bend::new(l, lo(0.5 * a)),
            ],
            Gesture::Fade => vec![curved(0, lo(0.9 * a), -0.4), Bend::new(l, 0.0)],
            Gesture::Accent => vec![
                curved(0, lo(a), -0.5),
                Bend::new(short(0.3, beat / 4), lo(0.35 * a)),
            ],
            Gesture::Tremolo => wave(lo(0.5 * a), 0.4 * a, 1, lo),
        }
    }
}

impl Content {
    /// Its notes' own expression moved onto them from among its controls (and its takes').
    pub fn adopt_expression(&mut self) {
        match self {
            Content::Midi { notes, controls } => adopt_expression(notes, controls),
            Content::Takes { takes, .. } => {
                for t in takes {
                    t.content.adopt_expression();
                }
            }
            _ => {}
        }
    }
}

impl Project {
    /// Every region's notes' own expression moved onto them: songs made before notes kept it.
    pub(crate) fn adopt_expression(&mut self) {
        for t in &mut self.tracks {
            let cells = t.cells.iter_mut().map(|c| &mut c.region);
            for r in t.regions.iter_mut().chain(cells) {
                r.content.adopt_expression();
            }
        }
    }
}

#[cfg(test)]
mod tests;
