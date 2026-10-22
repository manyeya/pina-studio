//! The EQ window's display: the plot (the analyzer, each band's area, the curves, the dots,
//! their readouts and the panel of the bands picked), the frequency scale or piano under it,
//! and the analyzer's scale and output meter beside it.

mod dots;
mod plot;
mod scale;

pub(crate) use scale::*;

use gpui::{
    Bounds, Context, Div, MouseButton, MouseDownEvent, MouseMoveEvent, PathBuilder, Point,
    ScrollWheelEvent, Stateful, canvas, div, fill, linear_color_stop, linear_gradient, point,
    prelude::*, px, rgb, size,
};
use pina_engine::{
    dsp::eq::{Section, band_sections, response_db},
    song::project::{Eq, EqBand, MAX_BANDS, Placement},
};

use super::{
    CURVE, EqKnob, EqOp, Geo, RANGES, SCALE_H, SIDE_W, band_at, band_color, dot_db, hz_text,
    note_of, note_text, part_color, q_text,
};
use crate::{
    ui::menu::MenuAction,
    ui::theme::*,
    workspace::{Drag, Name, Workspace, pt},
};

/// A band as it's drawn: its curve (dB at each of the curve's points) and where its dot is.
pub(crate) struct Drawn {
    pub id: u16,
    pub index: usize,
    pub color: u32,
    pub band: EqBand,
    pub curve: Vec<f32>,
    /// Its curve at the end of its dynamic range, if it's dynamic.
    pub reach: Option<Vec<f32>>,
    pub dot: (f32, f32),
}

/// A line through points, dashed if asked.
fn line(
    points: &[Point<gpui::Pixels>],
    width: f32,
    dashed: bool,
) -> Option<gpui::Path<gpui::Pixels>> {
    let mut p = PathBuilder::stroke(px(width));
    let mut drawn = 0.0f32;
    p.move_to(*points.first()?);
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let step = f32::from((b.x - a.x).abs() + (b.y - a.y).abs());
        drawn += step;
        // Dashes of 6, gaps of 4.
        if dashed && drawn % 10.0 > 6.0 {
            p.move_to(b);
        } else {
            p.line_to(b);
        }
    }
    p.build().ok()
}

impl Workspace {
    /// Each band as it's drawn, at `freqs`: its curve (with its dynamic move, while playing) and
    /// where its dot sits.
    pub(super) fn eq_drawn(&self, eq: &Eq, geo: &Geo, freqs: &[f64]) -> Vec<Drawn> {
        let rate = self.session.project.sample_rate as f64;
        let playing = self.player.playing();
        let tap = self.player.eq_tap();
        let curve = |s: &[Section]| -> Vec<f32> {
            freqs
                .iter()
                .map(|&f| response_db(s, f, rate) as f32)
                .collect()
        };
        eq.bands
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let moved = if playing { tap.moved(i) } else { 0.0 };
                let mut heard = *b;
                if eq.scale > 0.0 {
                    heard.gain_db += moved / eq.scale;
                }
                let reach = b.dynamic.filter(|_| b.shape.has_gain()).map(|d| {
                    let far = EqBand {
                        gain_db: b.gain_db + d.range_db,
                        ..*b
                    };
                    curve(&band_sections(&far, eq.scale, rate))
                });
                let db = super::dot_db(&heard, eq.scale).clamp(-geo.range, geo.range);
                Drawn {
                    id: b.id,
                    index: i,
                    color: band_color(i),
                    band: *b,
                    curve: curve(&band_sections(&heard, eq.scale, rate)),
                    reach,
                    dot: (geo.x(b.freq as f64), geo.y(db)),
                }
            })
            .collect()
    }

    /// The display: the plot with its scale under it, and the analyzer's scale and meter.
    pub(super) fn eq_display_area(&mut self, eq: &Eq, cx: &mut Context<Self>) -> Div {
        let rate = self.session.project.sample_rate as f64;
        let Some(view) = self.eq_view.as_ref() else {
            return div();
        };
        let geo = view.geo(rate);
        let (dw, dh) = view.display_size();
        let n = (geo.w / 2.0) as usize;
        let freqs: Vec<f64> = (0..n)
            .map(|i| geo.freq(i as f32 / (n - 1) as f32 * geo.w))
            .collect();
        let bounds = view.display.clone();
        if view.list {
            let list = self.eq_list(dw, dh, cx);
            return div()
                .relative()
                .w(px(dw))
                .h(px(dh))
                .child(
                    canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .child(list);
        }
        let drawn = self.eq_drawn(eq, &geo, &freqs);
        let plot = self.eq_plot(eq, &geo, &freqs, &drawn, cx);
        let scale = self.eq_scale(&geo, &drawn, cx);
        let side = self.eq_side(&geo, dh, cx);
        div()
            .relative()
            .w(px(dw))
            .h(px(dh))
            .flex()
            .child(
                canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(
                div()
                    .w(px(geo.w))
                    .flex()
                    .flex_col()
                    .child(plot)
                    .child(scale),
            )
            .child(side)
    }
}
