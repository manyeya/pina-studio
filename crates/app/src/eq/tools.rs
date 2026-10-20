//! The EQ window's tools, as Pro-Q's: EQ Match (this track's sound made like another's, or a song's:
//! bands fitted to the difference), EQ Sketch (a curve drawn, bands made along it), and
//! Spectrum Grab's peaks.

use std::path::PathBuf;

use gpui::{
    Context, Div, FontWeight, MouseButton, MouseDownEvent, PathPromptOptions, div, prelude::*, px,
    rgb,
};
use pina_engine::{
    eq::{average_spectrum, fit_bands, smooth_third, spectrum_db},
    project::{EqBand, Shape},
};

use super::{EqOp, Geo};
use crate::{
    menu::MenuAction,
    strip::Channel,
    theme::*,
    workspace::{Drag, Workspace},
};

/// The frequencies EQ Match compares at: 240 an even step apart in octaves, 20 Hz to 20 kHz.
fn match_freqs() -> Vec<f64> {
    (0..240)
        .map(|i| 20.0 * 1000f64.powf(i as f64 / 239.0))
        .collect()
}

/// What EQ Match makes this track sound like.
#[derive(Clone, Debug, PartialEq)]
pub enum MatchSource {
    None,
    /// Another track, by id, heard as it plays.
    Track(u64),
    /// A song's file, measured all through.
    File(PathBuf),
}

/// EQ Match: what it's heard of this track and the reference (power summed at each of its
/// frequencies, and how many times), how many bands to fit, and those it has.
pub struct Match {
    pub source: MatchSource,
    freqs: Vec<f64>,
    input: (Vec<f64>, usize),
    reference: (Vec<f64>, usize),
    pub count: usize,
    pub proposed: Vec<EqBand>,
    /// The difference it fitted (dB at each frequency), to draw.
    pub difference: Vec<f32>,
}

/// Enough heard to match: a few seconds of each, as the frames come (about 60 a second).
const ENOUGH: usize = 180;

impl Match {
    pub fn new(source: MatchSource) -> Self {
        let freqs = match_freqs();
        let n = freqs.len();
        Match {
            source,
            freqs,
            input: (vec![0.0; n], 0),
            reference: (vec![0.0; n], 0),
            count: 6,
            proposed: Vec::new(),
            difference: Vec::new(),
        }
    }

    /// A frame's worth heard: this track going in, and the reference track if that's what it's
    /// matched to.
    pub fn learn(&mut self, input: &[f32], reference: Option<&[f32]>, rate: f64) {
        let len = 4096.min(input.len());
        let add = |into: &mut (Vec<f64>, usize), s: &[f32], freqs: &[f64]| {
            let db = spectrum_db(&s[s.len() - len..], rate, freqs);
            // Silence teaches nothing.
            if db.iter().all(|&d| d < -100.0) {
                return;
            }
            for (p, d) in into.0.iter_mut().zip(db) {
                *p += 10f64.powf(d as f64 / 10.0);
            }
            into.1 += 1;
        };
        let freqs = self.freqs.clone();
        add(&mut self.input, input, &freqs);
        if let (MatchSource::Track(_), Some(r)) = (&self.source, reference) {
            add(&mut self.reference, r, &freqs);
        }
    }

    /// A song's file as the reference: its average spectrum.
    pub fn reference_file(&mut self, path: PathBuf) -> Result<(), String> {
        let (channels, rate) = pina_engine::engine::read_audio(&path)?;
        let n = channels.first().map_or(0, |c| c.len());
        let mono: Vec<f32> = (0..n)
            .map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / channels.len() as f32)
            .collect();
        let db = average_spectrum(&mono, rate, &self.freqs, 4096);
        self.reference = (db.iter().map(|&d| 10f64.powf(d as f64 / 10.0)).collect(), 1);
        self.source = MatchSource::File(path);
        Ok(())
    }

    /// How far it's got hearing each, 0 to 1.
    pub fn heard(&self) -> (f32, f32) {
        let share = |n: usize| (n as f32 / ENOUGH as f32).min(1.0);
        let reference = match self.source {
            MatchSource::File(_) => 1.0,
            _ => share(self.reference.1),
        };
        (share(self.input.1), reference)
    }

    pub fn ready(&self) -> bool {
        let (a, b) = self.heard();
        a >= 1.0 && b >= 1.0
    }

    /// The bands that make this track sound like the reference: the difference between them,
    /// smoothed a third of an octave and levelled (loudness isn't the EQ's to match), fitted.
    pub fn run(&mut self, rate: f64) {
        let mean = |(sum, n): &(Vec<f64>, usize)| -> Vec<f32> {
            sum.iter()
                .map(|p| (10.0 * (p / (*n).max(1) as f64).max(1e-14).log10()) as f32)
                .collect()
        };
        let (input, reference) = (mean(&self.input), mean(&self.reference));
        let raw: Vec<f32> = reference.iter().zip(&input).map(|(r, i)| r - i).collect();
        let smooth = smooth_third(&raw, &self.freqs);
        let middle: Vec<f32> = self
            .freqs
            .iter()
            .zip(&smooth)
            .filter(|(f, _)| (60.0..=12_000.0).contains(*f))
            .map(|(_, d)| *d)
            .collect();
        let level = middle.iter().sum::<f32>() / middle.len().max(1) as f32;
        self.difference = smooth
            .iter()
            .map(|d| (d - level).clamp(-15.0, 15.0))
            .collect();
        self.proposed = fit_bands(&self.difference, &self.freqs, self.count, rate);
    }

    pub fn freqs(&self) -> &[f64] {
        &self.freqs
    }

    #[cfg(test)]
    pub fn pretend(&mut self, input: &[f32], reference: &[f32]) {
        let power = |db: &[f32]| db.iter().map(|&d| 10f64.powf(d as f64 / 10.0)).collect();
        self.input = (power(input), ENOUGH);
        self.reference = (power(reference), ENOUGH);
    }
}

/// Bands from a curve drawn left to right (frequency, dB): each stretch it leaves 0 dB and comes
/// back, a bell at its height as wide as it went; at either end, a shelf, or a cut if it dives.
pub fn sketch_bands(points: &[(f64, f32)], range: f32) -> Vec<EqBand> {
    const NEAR: f32 = 1.0;
    let mut out = Vec::new();
    let n = points.len();
    let mut i = 0;
    while i < n {
        if points[i].1.abs() <= NEAR {
            i += 1;
            continue;
        }
        let a = i;
        while i < n && points[i].1.abs() > NEAR && points[i].1.signum() == points[a].1.signum() {
            i += 1;
        }
        let b = i - 1;
        let k = (a..=b)
            .max_by(|&x, &y| points[x].1.abs().total_cmp(&points[y].1.abs()))
            .unwrap_or(a);
        let (f, g) = points[k];
        let gain = (g.clamp(-30.0, 30.0) * 2.0).round() / 2.0;
        // Half its height, either side: where a shelf is set.
        let half = |from: usize, to: usize| {
            let range: Box<dyn Iterator<Item = usize>> = if from < to {
                Box::new(from..=to)
            } else {
                Box::new((to..=from).rev())
            };
            range
                .map(|j| points[j])
                .find(|p| p.1.abs() <= g.abs() / 2.0)
                .map_or(points[to].0, |p| p.0)
        };
        let band = match (a == 0, b == n - 1) {
            (true, _) if g < -range * 0.7 => EqBand::new(Shape::LowCut, half(k, b) as f32, 0.0),
            (true, _) => EqBand::new(Shape::LowShelf, half(k, b) as f32, gain),
            (_, true) if g < -range * 0.7 => EqBand::new(Shape::HighCut, half(k, a) as f32, 0.0),
            (_, true) => EqBand::new(Shape::HighShelf, half(k, a) as f32, gain),
            _ => {
                let octaves = (points[b].0 / points[a].0).log2().max(0.15);
                let r = 2f64.powf(octaves);
                let mut bell = EqBand::new(Shape::Bell, f as f32, gain);
                bell.q = ((r.sqrt() / (r - 1.0)).clamp(0.3, 12.0) * 100.0).round() as f32 / 100.0;
                bell
            }
        };
        out.push(EqBand {
            freq: band.freq.round().clamp(10.0, 30_000.0),
            ..band
        });
    }
    out
}

/// The peaks of a spectrum worth a band (dB at each of `freqs`): standing 4 dB over what's
/// round them, the loudest six (none within a fortieth of the spectrum of a louder one), low to
/// high.
pub fn peaks(spec: &[f32], floor: f32) -> Vec<usize> {
    let n = spec.len();
    let k = (n / 60).max(2);
    let mut found: Vec<usize> = (k..n.saturating_sub(k))
        .filter(|&i| {
            let v = spec[i];
            v > floor + 10.0
                && (i - k..=i + k).all(|j| spec[j] <= v)
                && (i.saturating_sub(3 * k)..(i + 3 * k).min(n))
                    .map(|j| spec[j])
                    .fold(f32::MAX, f32::min)
                    < v - 4.0
        })
        .collect();
    found.sort_by(|&a, &b| spec[b].total_cmp(&spec[a]));
    let apart = (n / 40).max(1);
    let mut kept: Vec<usize> = Vec::new();
    for i in found {
        if kept.len() < 6 && kept.iter().all(|&k| k.abs_diff(i) >= apart) {
            kept.push(i);
        }
    }
    kept.sort();
    kept
}

impl Workspace {
    pub(super) fn match_op(&mut self, op: EqOp, cx: &mut Context<Self>) {
        let rate = self.session.project.sample_rate as f64;
        let Some(v) = self.eq_view.as_mut() else {
            return;
        };
        match op {
            EqOp::Match(track) => {
                let source = track.map_or(MatchSource::None, MatchSource::Track);
                v.matching = Some(Match::new(source));
                v.list = false;
                if track.is_some() {
                    v.ext = track;
                }
            }
            EqOp::MatchTrack(id) => {
                v.matching = Some(Match::new(MatchSource::Track(id)));
                v.ext = Some(id);
            }
            EqOp::MatchFile => {
                let picked = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Match".into()),
                });
                cx.spawn(async move |this, cx| {
                    let Ok(Ok(Some(paths))) = picked.await else {
                        return;
                    };
                    this.update(cx, |ws, cx| {
                        if let Some(p) = paths.into_iter().next() {
                            ws.match_file(p, cx);
                        }
                    })
                    .ok();
                })
                .detach();
            }
            EqOp::MatchRun => {
                if let Some(m) = v.matching.as_mut() {
                    m.run(rate);
                }
            }
            EqOp::MatchBands(by) => {
                if let Some(m) = v.matching.as_mut() {
                    m.count = (m.count as i32 + by as i32).clamp(1, 16) as usize;
                    if !m.proposed.is_empty() {
                        m.run(rate);
                    }
                }
            }
            EqOp::MatchFinish => {
                let bands = v.matching.take().map(|m| m.proposed).unwrap_or_default();
                let n = bands.len();
                let mut made = Vec::new();
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.eq_mut() {
                        for b in &bands {
                            if let Some(i) = eq.bands.push(*b) {
                                made.push(eq.bands[i].id);
                            }
                        }
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked = made;
                }
                self.status = Some(format!(
                    "EQ Match added {n} band{}",
                    if n == 1 { "" } else { "s" }
                ));
            }
            EqOp::MatchCancel => v.matching = None,
            _ => {}
        }
        cx.notify();
    }

    /// A song's file as EQ Match's reference.
    pub fn match_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(m) = self.eq_view.as_mut().and_then(|v| v.matching.as_mut()) else {
            return;
        };
        if let Err(e) = m.reference_file(path) {
            self.status = Some(format!("Couldn't read it to match: {e}"));
        }
        cx.notify();
    }

    /// EQ Match, over the display: what it's heard of each, the reference to pick, how many
    /// bands, and Match and Finish.
    pub(super) fn match_panel(&self, g: &Geo, cx: &mut Context<Self>) -> Option<Div> {
        let m = self.eq_view.as_ref()?.matching.as_ref()?;
        let (a, b) = m.heard();
        let reference = match &m.source {
            MatchSource::None => "Pick a reference".to_string(),
            MatchSource::Track(id) => self
                .session
                .project
                .tracks
                .iter()
                .find(|t| t.id == *id)
                .map_or("A track".into(), |t| t.name.clone()),
            MatchSource::File(p) => p
                .file_name()
                .map_or("A file".into(), |n| n.to_string_lossy().into_owned()),
        };
        let bar = |share: f32, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .w(px(74.))
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child(label),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .h(px(6.))
                        .rounded(px(3.))
                        .bg(rgb(0x0c0d10))
                        .child(
                            div()
                                .absolute()
                                .left(px(0.))
                                .top(px(0.))
                                .h(px(6.))
                                .w(gpui::relative(share))
                                .rounded(px(3.))
                                .bg(rgb(if share >= 1.0 { MINT } else { ACCENT })),
                        ),
                )
                .child(
                    div()
                        .w(px(64.))
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(match share >= 1.0 {
                            true => "heard".to_string(),
                            false => format!("{:.0}%", share * 100.0),
                        }),
                )
        };
        let button =
            |label: &'static str, sel: &'static str, on: bool, op: EqOp, cx: &mut Context<Self>| {
                div()
                    .h(px(26.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .when(on, |d| {
                        d.bg(rgb(ACCENT))
                            .text_color(rgb(0xffffff))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(ACCENT_TEXT)))
                    })
                    .when(!on, |d| d.bg(rgb(FIELD)).text_color(rgb(TEXT_4)))
                    .debug_selector(move || sel.into())
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            if on {
                                ws.eq_op(op, cx);
                            }
                        }),
                    )
            };
        let matched = !m.proposed.is_empty();
        let w = 440.0f32.min(g.w - 24.0);
        Some(
            div()
                .absolute()
                .left(px((g.w - w) / 2.0))
                .top(px(14.))
                .w(px(w))
                .p(px(14.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .rounded(px(12.))
                .bg(alpha(0x17181e, 0.96))
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .shadow_lg()
                .debug_selector(|| "eq-match".into())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::BOLD)
                                .child("EQ Match"),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .size(px(22.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(RAISED)))
                                .debug_selector(|| "eq-match-close".into())
                                .child(icon(Icon::Close, 10.).text_color(rgb(TEXT_3)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.eq_op(EqOp::MatchCancel, cx);
                                    }),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .w(px(74.))
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .child("Like"),
                        )
                        .child(
                            dropdown(reference, 26.)
                                .debug_selector(|| "eq-match-track".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        let own = ws.eq_view.as_ref().and_then(|v| match v.ch {
                                            Channel::Track(i) => {
                                                ws.session.project.tracks.get(i).map(|t| t.id)
                                            }
                                            _ => None,
                                        });
                                        let items = ws
                                            .session
                                            .project
                                            .tracks
                                            .iter()
                                            .filter(|t| Some(t.id) != own)
                                            .map(|t| (t.name.clone(), MenuAction::Eq(EqOp::MatchTrack(t.id))))
                                            .collect();
                                        ws.eq_menu(e.position, items, cx);
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .h(px(26.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .rounded(px(6.))
                                .cursor_pointer()
                                .bg(rgb(FIELD))
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_2))
                                .hover(|s| s.bg(rgb(RAISED)))
                                .debug_selector(|| "eq-match-file".into())
                                .child("A song's file…")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.eq_op(EqOp::MatchFile, cx);
                                    }),
                                ),
                        ),
                )
                .child(bar(a, "This track"))
                .child(bar(b, "Reference"))
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_4))
                        .child(match (m.ready(), matched) {
                            (_, true) => "The white line is the difference; the bands fitted to it show as it would sound. More bands catch smaller differences.",
                            (true, false) => "Heard enough of both: match them.",
                            (false, false) => "Play the song: it listens to this track and the reference for a few seconds.",
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(button("Match", "eq-match-run", m.ready(), EqOp::MatchRun, cx))
                        .child(div().flex_1())
                        .when(matched, |el| {
                            el.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child("Bands"),
                            )
                            .child(
                                tray()
                                    .rounded(px(7.))
                                    .child(
                                        seg("−", false, 22.)
                                            .debug_selector(|| "eq-match-fewer".into())
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, _, cx| {
                                                    cx.stop_propagation();
                                                    ws.eq_op(EqOp::MatchBands(-1), cx);
                                                }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(22.))
                                            .flex()
                                            .justify_center()
                                            .font_family(MONO)
                                            .text_size(px(11.))
                                            .child(format!("{}", m.proposed.len())),
                                    )
                                    .child(
                                        seg("+", false, 22.)
                                            .debug_selector(|| "eq-match-more".into())
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, _, cx| {
                                                    cx.stop_propagation();
                                                    ws.eq_op(EqOp::MatchBands(1), cx);
                                                }),
                                            ),
                                    ),
                            )
                            .child(button("Finish", "eq-match-finish", true, EqOp::MatchFinish, cx))
                        }),
                ),
        )
    }

    /// EQ Sketch let go: the bands along the curve drawn.
    pub fn finish_sketch(&mut self, cx: &mut Context<Self>) {
        let Some(geo) = self.eq_geo() else { return };
        let Some(v) = self.eq_view.as_mut() else {
            return;
        };
        let stroke = std::mem::take(&mut v.stroke);
        v.sketching = false;
        let points: Vec<(f64, f32)> = stroke
            .iter()
            .map(|&(x, y)| (geo.freq(x), geo.db(y)))
            .collect();
        let bands = sketch_bands(&points, geo.range);
        if bands.is_empty() {
            cx.notify();
            return;
        }
        let mut made = Vec::new();
        self.edit(cx, |ws| {
            if let Some(eq) = ws.eq_mut() {
                for b in &bands {
                    if let Some(i) = eq.bands.push(*b) {
                        made.push(eq.bands[i].id);
                    }
                }
            }
        });
        if let Some(v) = self.eq_view.as_mut() {
            v.picked = made;
        }
        cx.notify();
    }

    /// A stroke of EQ Sketch drawn on to a point: going back, it draws that stretch again.
    pub fn drag_eq_sketch(&mut self, p: gpui::Point<f32>) {
        let Some(v) = self.eq_view.as_mut() else {
            return;
        };
        let o = v.display.get().origin;
        let (x, y) = (p.x - f32::from(o.x), p.y - f32::from(o.y));
        v.stroke.retain(|&(sx, _)| sx < x);
        v.stroke.push((x, y));
    }

    /// Spectrum Grab: a peak pressed, a bell made there (as wide as the peak) to pull.
    pub(super) fn grab_peak(&mut self, x: f32, e: &MouseDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(geo) = self.eq_geo() else {
            return false;
        };
        let Some(v) = self.eq_view.as_ref() else {
            return false;
        };
        let Some(held) = v.grabbing.clone() else {
            return false;
        };
        let floor = -(v.analysis.range as f32);
        let columns = v.columns.clone();
        let near = peaks(&held, floor).into_iter().min_by(|&a, &b| {
            (geo.x(columns[a]) - x)
                .abs()
                .total_cmp(&(geo.x(columns[b]) - x).abs())
        });
        let Some(i) = near.filter(|&i| (geo.x(columns[i]) - x).abs() < 30.0) else {
            return false;
        };
        // As wide as the peak stands 3 dB over its sides.
        let (mut lo, mut hi) = (i, i);
        while lo > 0 && held[lo] > held[i] - 3.0 {
            lo -= 1;
        }
        while hi + 1 < held.len() && held[hi] > held[i] - 3.0 {
            hi += 1;
        }
        let r = 2f64.powf((columns[hi] / columns[lo]).log2().max(0.1));
        let mut band = EqBand::new(Shape::Bell, columns[i].round() as f32, 0.0);
        band.q = ((r.sqrt() / (r - 1.0)).clamp(0.5, 12.0) * 100.0).round() as f32 / 100.0;
        let Some(id) = self.make_band(band, false, cx) else {
            return false;
        };
        self.hold_picked();
        self.drag = Some(Drag::EqNode {
            id,
            from: crate::workspace::pt(e.position),
            q: false,
            alt: false,
            axis: 0,
            moved: true,
            deselect: false,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sketch_makes_shelves_bells_and_cuts() {
        // Up on the left, a dip in the middle, a dive at the right.
        let points: Vec<(f64, f32)> = (0..200)
            .map(|i| {
                let f = 20.0 * 1000f64.powf(i as f64 / 199.0);
                let o = (f / 1_000.0).log2() as f32;
                let db = if f < 120.0 {
                    4.0
                } else if (0.0..1.5).contains(&o) {
                    -5.0
                } else if f > 15_000.0 {
                    -20.0
                } else {
                    0.0
                };
                (f, db)
            })
            .collect();
        let bands = sketch_bands(&points, 12.0);
        let shapes: Vec<Shape> = bands.iter().map(|b| b.shape).collect();
        assert_eq!(
            shapes,
            [Shape::LowShelf, Shape::Bell, Shape::HighCut],
            "{bands:?}"
        );
        assert_eq!(bands[0].gain_db, 4.0);
        let bell = bands[1];
        assert!(
            bell.gain_db == -5.0 && (1_000.0..2_900.0).contains(&bell.freq),
            "{bell:?}"
        );
    }

    #[test]
    fn eq_match_fits_what_would_make_one_like_the_other() {
        let mut m = Match::new(MatchSource::None);
        let freqs = m.freqs().to_vec();
        // The reference is this track with 6 dB more round 3 kHz.
        let input: Vec<f32> = freqs.iter().map(|_| -40.0).collect();
        let reference: Vec<f32> = freqs
            .iter()
            .map(|&f| -40.0 + 6.0 * (-((f / 3_000.0).log2().powi(2)) / 0.5).exp() as f32)
            .collect();
        m.pretend(&input, &reference);
        assert!(m.ready());
        m.run(48_000.0);
        let b = m.proposed[0];
        assert_eq!(b.shape, Shape::Bell);
        assert!(
            (b.freq / 3_000.0 - 1.0).abs() < 0.1 && b.gain_db > 3.5,
            "{b:?}"
        );
        // Peaks: a spectrum with two, found.
        let spec: Vec<f32> = freqs
            .iter()
            .map(|&f| {
                let bump = |c: f64| 12.0 * (-((f / c).log2().powi(2)) / 0.01).exp() as f32;
                -50.0 + bump(200.0) + bump(4_000.0)
            })
            .collect();
        let found: Vec<f64> = peaks(&spec, -90.0).iter().map(|&i| freqs[i]).collect();
        assert_eq!(found.len(), 2, "{found:?}");
    }
}
