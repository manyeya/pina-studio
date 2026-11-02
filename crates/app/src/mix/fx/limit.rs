//! The limiter's window. Along the top: the listening aids, the loudness Learn aims at, and
//! Learn. Then the display: what goes in (grey) and comes out (its colour) scrolling by, what it
//! takes off (red, hanging from the top), the short-term loudness (white), the ceiling and the
//! target across. Beside it, its meters: in, reduction, out and loudness, and the integrated
//! loudness with its short-term and momentary readings, the true peak and the peak-to-loudness
//! ratio. Under them: its ten styles, its gain and ceiling, its timing knobs, and its switches.

use pina_engine::{
    dsp::limit::learn::TARGETS,
    song::project::{LimitStyle, Limiter},
};

use super::comp::{legend, side_row};
use super::*;
use crate::{
    ui::menu::{Menu, MenuAction},
    workspace::LimitField,
};

const DISPLAY_H: f32 = 300.0;
/// The display's floor, dB: a limiter's work is all near the top.
const FLOOR: f32 = -24.0;
/// The reduction meter's depth, dB.
const GR_DB: f32 = 12.0;
const GR: u32 = METER_HIGH;
/// The loudness, and the loudness aimed at.
const LOUDNESS: u32 = AMBER;

/// A level as text: "−13.9", or a dash for nothing yet.
fn lufs_text(v: f32) -> String {
    match v.is_finite() && v > -70.0 {
        true => format!("{:.1}", v).replace('-', "−"),
        false => "—".into(),
    }
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-6).log10()
}

/// A style picked: Safe never clips, so its clipper goes.
fn set_style(ws: &mut Workspace, ch: Channel, style: LimitStyle) {
    if let Some(l) = ws.fx_unit_mut::<Limiter>(ch) {
        l.style = style;
        if style == LimitStyle::Safe {
            (l.clip, l.true_peak) = (0.0, true);
        }
    }
}

impl Workspace {
    /// What a channel's limiter is drawn in: its colour, or (the master's being white, as its
    /// loudness line is) the accent.
    pub(crate) fn limit_color(&self, ch: Channel) -> u32 {
        match ch {
            Channel::Master => ACCENT,
            _ => self.channel_look(ch).1,
        }
    }

    pub(super) fn limit_window(
        &self,
        ch: Channel,
        l: Limiter,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let color = self.limit_color(ch);
        let on = !l.bypass;
        let learning = self.learning == Some(ch);
        let target = l.target_lufs.unwrap_or(-14.0);
        let aid = |name: &'static str, lit: bool, tint: u32, cx: &mut Context<Self>| {
            bar_button(name, lit, tint)
                .debug_selector(move || format!("limit-{}", name.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(l) = ws.fx_unit_mut::<Limiter>(ch) {
                                match name {
                                    "Unity" => l.unity ^= true,
                                    _ => l.delta ^= true,
                                }
                            }
                        });
                    }),
                )
        };
        let aids = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .pl(px(6.))
            .child(div().w(px(1.)).h(px(18.)).bg(rgb(LINE)).mr(px(6.)))
            .child(aid("Unity", l.unity, SKY, cx))
            .child(aid("Delta", l.delta, CORAL, cx));
        let name = TARGETS
            .iter()
            .find(|t| (t.1 - target).abs() < 0.05)
            .map_or("Custom", |t| t.0);
        let aim = div()
            .h(px(26.))
            .pl(px(9.))
            .pr(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .bg(rgb(0x1c1d23))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(RAISED)))
            .debug_selector(|| "limit-target".into())
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT))
                    .child(format!("{} LUFS", lufs_text(target))),
            )
            .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(name))
            .child(icon(Icon::Chevron, 11.).text_color(rgb(TEXT_3)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let items = TARGETS
                        .iter()
                        .map(|&(name, lufs)| {
                            (
                                format!("{name}  {} LUFS", lufs_text(lufs)),
                                MenuAction::LearnLimit { channel: ch, lufs },
                            )
                        })
                        .collect();
                    ws.menu = Some(Menu {
                        at: e.position,
                        items,
                        ..Default::default()
                    });
                    cx.notify();
                }),
            );
        let learn = bar_button(
            if learning { "Listening…" } else { "Learn" },
            learning,
            AGENT,
        )
        .debug_selector(|| "limit-learn".into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                if ws.learning.is_none() {
                    ws.learn_limit(ch, target, cx);
                }
            }),
        );
        let learn = match learning {
            true => breathing(learn, "limit-learning"),
            false => learn.into_any_element(),
        };
        let right = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(aim)
            .child(learn);
        let head = self.fx_head("Limiter", ch, Builtin::Limit, on, aids, right, cx);
        let display = div()
            .flex_none()
            .h(px(DISPLAY_H))
            .mx(px(14.))
            .mt(px(14.))
            .flex()
            .gap(px(10.))
            .child(self.limit_history(&l, color))
            .child(self.limit_meters(ch, &l, color, cx));
        vec![
            head.into_any_element(),
            display.into_any_element(),
            self.limit_controls(ch, &l, color, cx).into_any_element(),
        ]
    }

    /// In, out and what's taken off scrolling by, newest at the right; the short-term loudness
    /// over them, the ceiling and the target across.
    fn limit_history(&self, l: &Limiter, color: u32) -> Div {
        let view = self.fx_view.as_ref().expect("open");
        let history: Vec<[f32; 4]> = view.history.iter().copied().collect();
        let empty = history.is_empty();
        let (ceiling, target, on) = (l.ceiling_db, l.target_lufs, !l.bypass);
        let lines = [-3.0, -6.0, -9.0, -12.0, -15.0, -18.0, -21.0];
        let graph = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                for db in lines {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x, b.origin.y + px(level_y(db, FLOOR, h).round())),
                            size(b.size.width, px(1.)),
                        ),
                        rgb(LINE_SOFT),
                    ));
                }
                let n = history.len();
                if n > 1 {
                    let step = w / (HISTORY - 1) as f32;
                    let x = |k: usize| b.origin.x + px(w - (n - 1 - k) as f32 * step);
                    let y = |db: f32| b.origin.y + px(level_y(db, FLOOR, h));
                    let bottom = b.origin.y + b.size.height;
                    let out = match on {
                        true => color,
                        false => TEXT_3,
                    };
                    // In faint, out over it brightest at the top: what's between them is what
                    // was shaved off.
                    for k in 0..2 {
                        let mut area = PathBuilder::fill();
                        area.move_to(point(x(0), bottom));
                        for (i, v) in history.iter().enumerate() {
                            area.line_to(point(x(i), y(v[k])));
                        }
                        area.line_to(point(x(n - 1), bottom));
                        area.close();
                        if let Ok(p) = area.build() {
                            match k {
                                0 => window.paint_path(p, alpha(0xffffff, 0.09)),
                                _ => window.paint_path(
                                    p,
                                    gpui::linear_gradient(
                                        180.,
                                        gpui::linear_color_stop(alpha(out, 0.34), 0.),
                                        gpui::linear_color_stop(alpha(out, 0.04), 1.),
                                    ),
                                ),
                            }
                        }
                    }
                    let mut edge = |k: usize, width: f32, tint: gpui::Hsla| {
                        let mut path = PathBuilder::stroke(px(width));
                        path.move_to(point(x(0), y(history[0][k])));
                        for (i, v) in history.iter().enumerate().skip(1) {
                            path.line_to(point(x(i), y(v[k])));
                        }
                        if let Ok(p) = path.build() {
                            window.paint_path(p, tint);
                        }
                    };
                    edge(0, 1.0, alpha(0xffffff, 0.22));
                    edge(1, 1.25, alpha(out, 0.9));
                    // The short-term loudness, on the same scale.
                    edge(3, 2.0, alpha(LOUDNESS, 0.95));
                    // What it takes off, hanging from the top.
                    let gr = |g: f32| b.origin.y + px(level_y(-g, FLOOR, h));
                    let mut area = PathBuilder::fill();
                    area.move_to(point(x(0), b.origin.y));
                    for (i, v) in history.iter().enumerate() {
                        area.line_to(point(x(i), gr(v[2])));
                    }
                    area.line_to(point(x(n - 1), b.origin.y));
                    area.close();
                    if let Ok(p) = area.build() {
                        window.paint_path(p, alpha(GR, 0.22));
                    }
                    let mut line = PathBuilder::stroke(px(2.));
                    line.move_to(point(x(0), gr(history[0][2])));
                    for (i, v) in history.iter().enumerate().skip(1) {
                        line.line_to(point(x(i), gr(v[2])));
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, rgb(GR));
                    }
                }
                dashed(window, b, level_y(ceiling, FLOOR, h), w, alpha(TEXT_2, 0.6));
                if let Some(t) = target {
                    dashed(window, b, level_y(t, FLOOR, h), w, alpha(LOUDNESS, 0.6));
                }
            },
        )
        .absolute()
        .size_full();
        let h = DISPLAY_H - 2.0;
        let scale = lines.map(|db: f32| {
            div()
                .absolute()
                .right(px(6.))
                .top(px(level_y(db, FLOOR, h) - 12.0))
                .font_family(MONO)
                .text_size(px(9.5))
                .text_color(rgb(TEXT_4))
                .child(format!("{db:.0}"))
        });
        well()
            .flex_1()
            .h_full()
            .debug_selector(|| "limit-history".into())
            .child(graph)
            .children(scale)
            .child(
                tag(
                    0.,
                    (level_y(ceiling, FLOOR, h) + 4.0).max(3.0),
                    format!(
                        "Ceiling {} dB{}",
                        db_text(ceiling),
                        if l.true_peak { "TP" } else { "" }
                    ),
                    TEXT_2,
                )
                .left_auto()
                .right(px(30.))
                .px(px(4.))
                .rounded(px(3.))
                .bg(alpha(0x0b0b0e, 0.85)),
            )
            .children(target.map(|t| {
                tag(
                    10.,
                    (level_y(t, FLOOR, h) + 4.0).max(3.0),
                    format!("Target {} LUFS", lufs_text(t)),
                    LOUDNESS,
                )
                .px(px(4.))
                .rounded(px(3.))
                .bg(alpha(0x0b0b0e, 0.85))
            }))
            .child(
                div()
                    .absolute()
                    .left(px(10.))
                    .bottom(px(8.))
                    .flex()
                    .gap(px(12.))
                    .text_size(px(10.5))
                    .child(legend(0x8a8a95, "In"))
                    .child(legend(color, "Out"))
                    .child(legend(GR, "Reduction"))
                    .child(legend(LOUDNESS, "Loudness")),
            )
            .when(empty, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_4))
                        .child("Play to see what it does"),
                )
            })
    }

    /// Its meters: in, reduction, out and short-term loudness as bars; the loudness read out.
    fn limit_meters(&self, ch: Channel, l: &Limiter, color: u32, cx: &mut Context<Self>) -> Div {
        let view = self.fx_view.as_ref().expect("open");
        let [input, gr, output, short] = view.bars;
        let m = view.meter;
        let to = |db: f32| ((db - FLOOR) / -FLOOR).clamp(0.0, 1.0);
        // A bar: filled up from the bottom (or down from the top), a mark across at `mark` (from
        // the top), its value and name under it.
        let bar = |name: &'static str,
                   fill: f32,
                   tint: u32,
                   down: bool,
                   mark: Option<f32>,
                   value: String| {
            let body = div()
                .relative()
                .w(px(12.))
                .flex_1()
                .rounded(px(3.))
                .bg(rgb(0x17181d))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .when(down, |d| d.top_0())
                        .when(!down, |d| d.bottom_0())
                        .h(gpui::relative(fill))
                        .bg(rgb(tint)),
                )
                .children(mark.map(|m| {
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(gpui::relative(m))
                        .h(px(2.))
                        .bg(rgb(TEXT))
                }));
            div()
                .w(px(36.))
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(5.))
                .child(body)
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(9.5))
                        .whitespace_nowrap()
                        .text_color(rgb(TEXT_2))
                        .child(value),
                )
                .child(div().text_size(px(9.5)).text_color(rgb(TEXT_4)).child(name))
        };
        let peak = |db: f32| match db > -90.0 {
            true => format!("{:.1}", db).replace('-', "−"),
            false => "—".into(),
        };
        let bars = div()
            .h_full()
            .flex()
            .gap(px(4.))
            .child(bar("IN", to(input), 0x6a6a75, false, None, peak(input)))
            .child(bar(
                "GR",
                (gr / GR_DB).clamp(0.0, 1.0),
                GR,
                true,
                None,
                format!("{:.1}", -gr.max(0.0)).replace('-', "−"),
            ))
            .child(bar(
                "OUT",
                to(output),
                color,
                false,
                Some(1.0 - to(l.ceiling_db)),
                peak(output),
            ))
            .child(bar(
                "LUFS",
                to(short),
                LOUDNESS,
                false,
                l.target_lufs.map(|t| 1.0 - to(t)),
                lufs_text(short),
            ));
        let tp = db(m.true_peak);
        let over = tp > l.ceiling_db + 0.05;
        let row = |name: &'static str, value: String, tint: u32| {
            div()
                .flex()
                .justify_between()
                .items_baseline()
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(name),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.5))
                        .text_color(rgb(tint))
                        .child(value),
                )
        };
        let integrated = m.integrated;
        let heard = integrated.is_finite() && integrated > -70.0;
        let off = l.target_lufs.filter(|_| heard).map(|t| integrated - t);
        let read = div()
            .flex_1()
            .h_full()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .rounded(px(10.))
            .bg(rgb(0x0b0b0e))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .text_size(px(9.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_4))
                    .child("INTEGRATED"),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(5.))
                    .debug_selector(|| "limit-integrated".into())
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(30.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(if heard { TEXT } else { TEXT_4 }))
                            .child(lufs_text(integrated)),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child("LUFS"),
                    ),
            )
            .child(match off {
                Some(o) if o.abs() < 0.5 => chip("On target", MINT),
                Some(o) => chip(
                    format!(
                        "{:.1} {} target",
                        o.abs(),
                        if o > 0.0 { "over" } else { "under" }
                    ),
                    if o > 0.0 { CORAL } else { SKY },
                ),
                None => div().h(px(18.)),
            })
            .child(div().h(px(1.)).my(px(4.)).bg(rgb(LINE_SOFT)))
            .child(row("Short-term", lufs_text(m.short), TEXT_2))
            .child(row("Momentary", lufs_text(m.momentary), TEXT_2))
            .child(row(
                "True peak",
                match m.true_peak > 0.0 {
                    true => format!("{} dBTP", lufs_text(tp)),
                    false => "—".into(),
                },
                if over { METER_HIGH } else { TEXT_2 },
            ))
            .child(row(
                "PLR",
                match heard && m.true_peak > 0.0 {
                    true => format!("{:.1} dB", tp - integrated),
                    false => "—".into(),
                },
                TEXT_2,
            ))
            .child(div().flex_1())
            .child(
                bar_button("Reset", false, TEXT)
                    .justify_center()
                    .debug_selector(|| "limit-reset".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            let id = ws.channel_id(ch);
                            ws.player.reset_loudness(id);
                            if let Some(v) = ws.fx_view.as_mut() {
                                v.meter = Default::default();
                            }
                            cx.notify();
                        }),
                    ),
            );
        div()
            .w(px(340.))
            .flex_none()
            .h_full()
            .flex()
            .gap(px(10.))
            .child(
                div()
                    .h_full()
                    .px(px(8.))
                    .py(px(10.))
                    .rounded(px(10.))
                    .bg(rgb(0x0b0b0e))
                    .border_1()
                    .border_color(rgb(LINE_SOFT))
                    .child(bars),
            )
            .child(read)
    }

    /// Its styles, gain and ceiling, timing, and switches.
    fn limit_controls(&self, ch: Channel, l: &Limiter, color: u32, cx: &mut Context<Self>) -> Div {
        let on = !l.bypass;
        let tint = |dim: bool| if !on || dim { TEXT_4 } else { color };
        let styles = div()
            .w(px(262.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(section_title("Style"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .children(LimitStyle::ALL.map(|s| {
                        let lit = s == l.style;
                        div()
                            .w(px(129.))
                            .h(px(26.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.))
                            .border_1()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .cursor_pointer()
                            .when(lit, |d| {
                                d.bg(alpha(color, 0.16))
                                    .border_color(alpha(color, 0.55))
                                    .text_color(rgb(color))
                            })
                            .when(!lit, |d| {
                                d.bg(rgb(BG))
                                    .border_color(rgb(LINE_SOFT))
                                    .text_color(rgb(TEXT_3))
                                    .hover(|d| d.bg(rgb(OFF)).text_color(rgb(TEXT_2)))
                            })
                            .debug_selector(move || {
                                format!("limit-style-{}", s.name().to_lowercase())
                            })
                            .child(s.name())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.edit(cx, |ws| set_style(ws, ch, s));
                                }),
                            )
                    })),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child(l.style.about()),
            );
        let knob = |ws: &Self, field, name, size: f32, dim: bool, cx: &mut Context<Self>| {
            ws.fx_dial(
                Target::Limit(ch, field),
                name,
                strip::limit_text(field, l),
                size,
                tint(dim),
                false,
                cx,
            )
        };
        let safe = l.style == LimitStyle::Safe;
        let big = div()
            .flex()
            .gap(px(26.))
            .items_center()
            .child(knob(self, LimitField::Gain, "Gain", 76., false, cx))
            .child(knob(self, LimitField::Ceiling, "Ceiling", 76., false, cx));
        let small = div()
            .w(px(258.))
            .flex()
            .flex_wrap()
            .justify_center()
            .gap_x(px(18.))
            .gap_y(px(10.))
            .child(knob(
                self,
                LimitField::Lookahead,
                "Lookahead",
                44.,
                false,
                cx,
            ))
            .child(knob(self, LimitField::Attack, "Attack", 44., safe, cx))
            .child(knob(self, LimitField::Release, "Release", 44., false, cx))
            .child(knob(self, LimitField::Link, "Link", 44., false, cx))
            .child(knob(self, LimitField::Clip, "Clip", 44., safe, cx));
        let side = div()
            .w(px(212.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(side_row(
                "Release",
                choice_tray(
                    "limit-release",
                    &[("Set", false), ("Auto", true)],
                    l.auto_release,
                    22.,
                    cx,
                    move |ws, auto| {
                        if let Some(l) = ws.fx_unit_mut::<Limiter>(ch) {
                            l.auto_release = auto;
                        }
                    },
                ),
            ))
            .child(side_row(
                "True peak",
                choice_tray(
                    "limit-tp",
                    &[("Off", false), ("On", true)],
                    l.true_peak || safe,
                    22.,
                    cx,
                    move |ws, tp| {
                        if let Some(l) = ws.fx_unit_mut::<Limiter>(ch) {
                            l.true_peak = tp;
                        }
                    },
                ),
            ))
            .child(side_row(
                "Oversample",
                choice_tray(
                    "limit-os",
                    &[("1×", 1u8), ("2×", 2), ("4×", 4), ("8×", 8)],
                    l.oversample.max(1),
                    22.,
                    cx,
                    move |ws, os| {
                        if let Some(l) = ws.fx_unit_mut::<Limiter>(ch) {
                            l.oversample = os;
                        }
                    },
                ),
            ))
            .child(side_row(
                "DC filter",
                choice_tray(
                    "limit-dc",
                    &[("Off", false), ("On", true)],
                    l.dc,
                    22.,
                    cx,
                    move |ws, dc| {
                        if let Some(l) = ws.fx_unit_mut::<Limiter>(ch) {
                            l.dc = dc;
                        }
                    },
                ),
            ));
        let rule = || div().w(px(1.)).h(px(150.)).bg(rgb(LINE));
        div()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(24.))
            .px(px(22.))
            .child(styles)
            .child(rule())
            .child(big)
            .child(rule())
            .child(small)
            .child(rule())
            .child(side)
    }
}
