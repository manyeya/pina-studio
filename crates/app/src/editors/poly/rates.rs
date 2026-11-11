//! An LFO's rate, picked: in time with the song, any length from a sixty-fourth to 32 bars,
//! straight, triplet or dotted; or free, in Hz up to audio rate; or any length typed.

use super::*;
use crate::ui::theme::tray;

/// Every length on offer, in beats (a quarter note each), short to long.
pub(crate) const LENGTHS: [(&str, f32); 12] = [
    ("1/64", 0.0625),
    ("1/32", 0.125),
    ("1/16", 0.25),
    ("1/8", 0.5),
    ("1/4", 1.0),
    ("1/2", 2.0),
    ("1 bar", 4.0),
    ("2 bars", 8.0),
    ("4 bars", 16.0),
    ("8 bars", 32.0),
    ("16 bars", 64.0),
    ("32 bars", 128.0),
];

/// How each length can be felt: as written, in threes, or half again.
pub(crate) const FEELS: [(&str, f32, &str); 3] = [
    ("Straight", 1.0, ""),
    ("Triplet", 2.0 / 3.0, "T"),
    ("Dotted", 1.5, "D"),
];

/// A length the grid has, by its name: "1/16 T", "2 bars D".
fn grid_name(length: usize, feel: usize) -> String {
    let (name, _) = LENGTHS[length];
    let mark = FEELS[feel].2;
    match (mark.is_empty(), name.split_once(' ')) {
        (true, _) => name.to_string(),
        // "2 bars" felt in threes is "2 T": the bars are in its column.
        (false, Some((n, _))) => format!("{n} {mark}"),
        (false, None) => format!("{name} {mark}"),
    }
}

/// Where `beats` is on the grid, if it is.
fn on_grid(beats: f32) -> Option<(usize, usize)> {
    (0..FEELS.len())
        .flat_map(|f| (0..LENGTHS.len()).map(move |l| (l, f)))
        .find(|&(l, f)| (LENGTHS[l].1 * FEELS[f].1 - beats).abs() < 1e-4)
}

/// A length as the button says it: its name on the grid, else in beats; free, none.
pub(crate) fn length_name(beats: f32) -> String {
    match (beats > 0.0, on_grid(beats)) {
        (false, _) => "Free".into(),
        (true, Some((l, f))) => {
            let name = LENGTHS[l].0;
            match FEELS[f].2 {
                "" => name.to_string(),
                mark => format!("{name} {mark}"),
            }
        }
        (true, None) => format!("{} beats", trim(beats)),
    }
}

fn trim(x: f32) -> String {
    let s = format!("{x:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A rate typed: a length ("1/16t", "3/16", "1/8 dotted", "2 bars", "1.5 beats", "0.75") or a
/// speed ("4 hz").
#[derive(Debug, PartialEq)]
pub(crate) enum Typed {
    Beats(f32),
    Hz(f32),
}

pub(crate) fn parse_rate(text: &str) -> Option<Typed> {
    let t = text.trim().to_lowercase();
    let (num, rest) = t.split_at(
        t.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '/'))
            .unwrap_or(t.len()),
    );
    let rest = rest.trim();
    let value = match num.split_once('/') {
        // A fraction of a whole note: 1/16 is a quarter of a beat.
        Some((a, b)) => 4.0 * a.parse::<f32>().ok()? / b.parse::<f32>().ok()?,
        None => num.parse::<f32>().ok()?,
    };
    if !(value.is_finite() && value > 0.0) {
        return None;
    }
    let fraction = num.contains('/');
    let feel = match rest {
        r if r.starts_with('t') && !r.starts_with("tr") || r.starts_with("trip") => 2.0 / 3.0,
        r if r.starts_with('d') => 1.5,
        _ => 1.0,
    };
    Some(match rest {
        r if r.starts_with("hz") => Typed::Hz(value.clamp(0.01, 200.0)),
        r if r.starts_with("bar") => Typed::Beats(value * 4.0),
        r if r.starts_with("beat") || r.is_empty() && !fraction => Typed::Beats(value),
        _ => Typed::Beats(value * feel),
    })
}

/// Seconds a cycle takes, and what that is in a bar of 4/4, at `bpm`.
fn describe(beats: f32, bpm: f64) -> String {
    let hz = bpm as f32 / 60.0 / beats;
    let per_bar = 4.0 / beats;
    let bar = match per_bar >= 1.0 {
        true => format!("{} cycles a bar", trim(per_bar)),
        false => format!("a cycle every {} bars", trim(1.0 / per_bar)),
    };
    format!("{bar}: {hz:.2} Hz at {bpm:.0} BPM")
}

impl Workspace {
    /// The LFO's rate button: what it's set to; pressed, the picker opens under it.
    pub(crate) fn lfo_rate_button(
        &self,
        t: usize,
        l: usize,
        p: &Poly,
        cx: &mut Context<Self>,
    ) -> Div {
        // Over its button, wherever that is; pushed back on screen at the window's edge.
        let beats = p.lfos[l].beats;
        let open = self
            .poly_view
            .as_ref()
            .is_some_and(|v| v.rate_open == Some(l));
        let label = match beats > 0.0 {
            true => length_name(beats),
            false => {
                let hz = POLY_PARAMS[lfo_param(l, LfoField::Rate)]
                    .scale
                    .value(self.poly_at(t, lfo_param(l, LfoField::Rate)));
                format!("{hz:.2} Hz")
            }
        };
        dropdown(label, 28.)
            .font_family(MONO)
            .bg(rgb(FIELD))
            .border_color(rgb(if open { ACCENT } else { FIELD }))
            .debug_selector(|| "poly-lfo-sync".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(v) = ws.poly_view.as_mut() {
                        v.rate_open = match v.rate_open == Some(l) {
                            true => None,
                            false => Some(l),
                        };
                    }
                    cx.notify();
                }),
            )
            .map(|button| {
                // Over its button: the LFOs are at the foot of the window.
                div()
                    .flex()
                    .flex_col()
                    .when(open, |d| d.child(self.lfo_rate_panel(t, l, p, cx)))
                    .child(button)
            })
    }

    /// The picker: in time or free; every length, straight, triplet or dotted; the speed in Hz;
    /// any length typed.
    pub(crate) fn lfo_rate_panel(
        &self,
        t: usize,
        l: usize,
        p: &Poly,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let beats = p.lfos[l].beats;
        let bpm = self.session.project.tempo;
        let rate = lfo_param(l, LfoField::Rate);
        let hz = POLY_PARAMS[rate].scale.value(self.poly_at(t, rate));
        let synced = beats > 0.0;
        let set_beats = move |ws: &mut Workspace, b: f32, cx: &mut Context<Workspace>| {
            ws.poly_op(t, PolyOp::LfoBeats(l, b), cx);
        };
        // In time from free, or free from in time: as fast as it was.
        let mode = |name: &'static str, on: bool, to_free: bool, cx: &mut Context<Self>| {
            seg(name, on, 28.)
                .px(px(14.))
                .debug_selector(move || {
                    format!("poly-rate-{}", name.to_lowercase().replace(' ', "-"))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        let Some(lfo) = ws.poly(t).map(|p| p.lfos[l].clone()) else {
                            return;
                        };
                        let bpm = ws.session.project.tempo as f32;
                        match (to_free, lfo.beats > 0.0) {
                            (true, true) => ws.edit(cx, |ws| {
                                if let Some(p) = ws.poly_mut(t) {
                                    p.params[rate] = (bpm / 60.0 / lfo.beats).clamp(0.01, 200.0);
                                    p.lfos[l].beats = 0.0;
                                }
                            }),
                            (false, false) => {
                                let hz = POLY_PARAMS[rate].scale.value(ws.poly_at(t, rate));
                                let want = bpm / 60.0 / hz.max(0.01);
                                // The nearest on the grid, as heard (by ratio, not difference).
                                let near = (0..FEELS.len())
                                    .flat_map(|f| {
                                        (0..LENGTHS.len()).map(move |k| LENGTHS[k].1 * FEELS[f].1)
                                    })
                                    .min_by(|a, b| {
                                        (a / want).ln().abs().total_cmp(&(b / want).ln().abs())
                                    })
                                    .unwrap_or(1.0);
                                set_beats(ws, near, cx);
                            }
                            _ => {}
                        }
                    }),
                )
        };
        let heads = div()
            .flex()
            .gap(px(4.))
            .child(div().w(px(70.)).flex_none())
            .children(LENGTHS.iter().map(|(name, _)| {
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_center()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(*name)
            }));
        let picked = synced.then(|| on_grid(beats)).flatten();
        let rows: Vec<Div> = (0..FEELS.len())
            .map(|f| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .w(px(70.))
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(rgb(TEXT_2))
                            .child(FEELS[f].0),
                    )
                    .children((0..LENGTHS.len()).map(|k| {
                        let on = picked == Some((k, f));
                        let b = LENGTHS[k].1 * FEELS[f].1;
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .font_family(MONO)
                            .text_size(px(10.5))
                            .whitespace_nowrap()
                            .bg(rgb(if on { ACCENT } else { SURFACE }))
                            .border_1()
                            .border_color(rgb(if on { ACCENT } else { LINE }))
                            .text_color(rgb(if on { 0xffffff } else { TEXT_2 }))
                            .when(on, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                            .cursor_pointer()
                            .hover(|d| {
                                d.bg(rgb(if on { ACCENT } else { RAISED }))
                                    .text_color(rgb(TEXT))
                            })
                            .debug_selector(move || format!("poly-rate-{k}-{f}"))
                            .child(grid_name(k, f))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    set_beats(ws, b, cx);
                                }),
                            )
                    }))
            })
            .collect();
        let naming = self.naming.as_ref().and_then(|n| match n.what {
            crate::workspace::Name::LfoBeats(tt, ll) if tt == t && ll == l => Some(n.input.clone()),
            _ => None,
        });
        let typed = match naming {
            Some(input) => div().w(px(150.)).child(input),
            None => div()
                .w(px(150.))
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .bg(rgb(WELL))
                .border_1()
                .border_color(rgb(LINE))
                .font_family(MONO)
                .text_size(px(12.))
                .cursor_text()
                .debug_selector(|| "poly-rate-typed".into())
                .child(match synced {
                    true => format!("{} beats", trim(beats)),
                    false => format!("{hz:.2} Hz"),
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, window, cx| {
                        cx.stop_propagation();
                        ws.start_naming(crate::workspace::Name::LfoBeats(t, l), window, cx);
                    }),
                ),
        };
        let panel = div()
            .id("poly-rate-panel")
            .occlude()
            .w(px(780.))
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .rounded(px(14.))
            .bg(rgb(0x1f1f26))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .text_color(rgb(TEXT))
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                if ws.naming.is_none()
                    && let Some(v) = ws.poly_view.as_mut()
                {
                    v.rate_open = None;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        tray()
                            .p(px(2.))
                            .rounded(px(8.))
                            .child(mode("In time", synced, false, cx))
                            .child(mode("Free", !synced, true, cx)),
                    )
                    .child(label(match synced {
                        true => format!(
                            "one cycle every {}: {}",
                            length_name(beats),
                            describe(beats, bpm)
                        ),
                        false => format!("{hz:.2} Hz, whatever the tempo"),
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .when(!synced, |d| d.opacity(0.55))
                    .child(heads)
                    .children(rows),
            )
            .child(div().h(px(1.)).bg(rgb(LINE)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .when(synced, |d| d.opacity(0.55))
                    .child(
                        div()
                            .w(px(70.))
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(rgb(TEXT_2))
                            .child("Free"),
                    )
                    .child(div().flex_1().child(self.poly_slider(t, rate, 28.0, cx)))
                    .child(
                        div()
                            .w(px(84.))
                            .font_family(MONO)
                            .text_size(px(12.))
                            .child(format!("{hz:.2} Hz")),
                    ),
            )
            .child(
                // Under the slider, where each speed is on it (it's even in ratios, not Hz).
                div().pl(px(84.)).pr(px(98.)).child(
                    div()
                        .relative()
                        .h(px(14.))
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .children(
                            [
                                (0.01, "0.01 Hz"),
                                (0.1, "0.1"),
                                (1.0, "1"),
                                (10.0, "10"),
                                (200.0, "200 Hz"),
                            ]
                            .map(|(hz, s): (f32, &str)| {
                                let at = (hz / 0.01).log10() / (200f32 / 0.01).log10();
                                div()
                                    .absolute()
                                    .left(gpui::relative(at))
                                    .ml(px(if at > 0.9 {
                                        -52.0
                                    } else if at > 0.0 {
                                        -8.0
                                    } else {
                                        0.0
                                    }))
                                    .whitespace_nowrap()
                                    .child(s)
                            }),
                        ),
                ),
            )
            .child(
                div()
                    .pl(px(84.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(label("Or type any length or speed:"))
                    .child(typed)
                    .child(label("3/16, 1/8 t, 2 bars, 0.75 beats, 4 hz")),
            );
        deferred(
            anchored()
                .anchor(gpui::Anchor::BottomLeft)
                .snap_to_window_with_margin(px(8.))
                .child(panel.mb(px(6.))),
        )
        .with_priority(2)
    }
}

impl Workspace {
    /// The free speed an LFO's rate parameter is at, in Hz.
    pub(crate) fn poly_hz(&self, t: usize, rate: usize) -> f32 {
        POLY_PARAMS[rate].scale.value(self.poly_at(t, rate))
    }

    /// A length or speed typed for LFO `l`: in time at that length, or free at that speed.
    pub(crate) fn lfo_rate_typed(
        &mut self,
        t: usize,
        l: usize,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        match parse_rate(text) {
            Some(Typed::Beats(b)) => self.poly_op(t, PolyOp::LfoBeats(l, b.clamp(0.01, 512.0)), cx),
            Some(Typed::Hz(hz)) => {
                let rate = lfo_param(l, LfoField::Rate);
                self.edit(cx, |ws| {
                    if let Some(p) = ws.poly_mut(t) {
                        p.lfos[l].beats = 0.0;
                        p.params[rate] = hz;
                    }
                });
            }
            None => {
                self.status = Some(format!(
                    "\"{text}\" isn't a length or a speed: try 3/16, 1/8 t, 2 bars or 4 hz"
                ))
            }
        }
        cx.notify();
    }
}

/// An LFO's free rate, by its place among the synth's parameters.
pub(crate) fn lfo_rate_param(l: usize) -> usize {
    lfo_param(l, LfoField::Rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_typed_as_musicians_write_them() {
        assert_eq!(parse_rate("1/16"), Some(Typed::Beats(0.25)));
        assert_eq!(parse_rate("1/16t"), Some(Typed::Beats(0.25 * 2.0 / 3.0)));
        assert_eq!(parse_rate("1/8 dotted"), Some(Typed::Beats(0.75)));
        assert_eq!(parse_rate("3/16"), Some(Typed::Beats(0.75)));
        assert_eq!(parse_rate("2 bars"), Some(Typed::Beats(8.0)));
        assert_eq!(parse_rate("1.5 beats"), Some(Typed::Beats(1.5)));
        assert_eq!(parse_rate("0.75"), Some(Typed::Beats(0.75)));
        assert_eq!(parse_rate("4 Hz"), Some(Typed::Hz(4.0)));
        assert_eq!(parse_rate("1/4 triplet"), Some(Typed::Beats(2.0 / 3.0)));
        assert_eq!(parse_rate("fast"), None);
        assert_eq!(parse_rate("0"), None);
        // Every length on the grid reads back by its name.
        assert_eq!(length_name(0.25 * 2.0 / 3.0), "1/16 T");
        assert_eq!(length_name(12.0), "2 bars D");
        assert_eq!(length_name(0.0), "Free");
        assert_eq!(length_name(0.3), "0.3 beats");
    }
}
