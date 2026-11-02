//! The compressor's window: its style and Learn along the top; what goes in (grey), comes out
//! (its colour) and what it takes off (amber, hanging from the top) scrolling by, the threshold
//! across them; its curve, the level now a dot on it; the reduction's meter; and its knobs.

use pina_engine::{dsp::comp::curve, song::project::nearest_step};

use super::*;

const DISPLAY_H: f32 = 262.0;
/// The level display's floor, dB.
const FLOOR: f32 = -48.0;
/// The meter's depth, dB.
const METER_DB: f32 = 24.0;
const GR: u32 = METER_MID;

/// What each style is, in a line.
fn style_line(s: CompStyle) -> &'static str {
    match s {
        CompStyle::Clean => "Transparent, soft knee",
        CompStyle::Smooth => "Gentle, hears like the ear",
        CompStyle::Punchy => "Hears peaks, bites, adds colour",
        CompStyle::Bus => "Console glue, stepped controls",
    }
}

/// A style picked: a bus compressor's settings go to its steps; a smooth one lets go by itself.
fn set_style(ws: &mut Workspace, ch: Channel, style: CompStyle) {
    let Some(c) = ws.fx_unit_mut::<Comp>(ch) else {
        return;
    };
    c.style = style;
    match style {
        CompStyle::Bus => {
            c.ratio = nearest_step(c.ratio, &CompStyle::BUS_RATIOS);
            c.attack_ms = nearest_step(c.attack_ms, &CompStyle::BUS_ATTACKS);
            c.release_ms = nearest_step(c.release_ms, &CompStyle::BUS_RELEASES);
        }
        CompStyle::Smooth => c.auto_release = true,
        _ => {}
    }
}

impl Workspace {
    pub(super) fn comp_window(
        &self,
        ch: Channel,
        c: Comp,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let color = self.channel_look(ch).1;
        let on = !c.bypass;
        let learning = self.learning == Some(ch);
        let style = choice_tray(
            "comp-style",
            &CompStyle::ALL.map(|s| (s.name(), s)),
            c.style,
            26.,
            cx,
            move |ws, s| set_style(ws, ch, s),
        );
        let learn = bar_button(
            if learning { "Listening…" } else { "Learn" },
            learning,
            AGENT,
        )
        .debug_selector(|| "comp-learn".into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                if ws.learning.is_none() {
                    ws.learn_comp_at(ch, ws.fx_stacked(), cx);
                }
            }),
        );
        let learn = match learning {
            true => breathing(learn, "comp-learning"),
            false => learn.into_any_element(),
        };
        let head = self.fx_head("Compressor", ch, Builtin::Comp, on, style, learn, cx);
        let display = div()
            .flex_none()
            .h(px(DISPLAY_H))
            .mx(px(14.))
            .mt(px(14.))
            .flex()
            .gap(px(10.))
            .child(self.comp_history(&c, color))
            .child(self.comp_curve(&c, color))
            .child(self.comp_meter(ch));
        let dial = |ws: &Self, field, name, cx: &mut Context<Self>| {
            let dim = field == CompField::Release && c.auto_release;
            ws.fx_dial(
                Target::Comp(ch, field),
                name,
                strip::comp_text(field, &c),
                58.,
                if !on || dim { TEXT_4 } else { color },
                false,
                cx,
            )
        };
        let knobs = div()
            .flex_1()
            .flex()
            .justify_between()
            .child(dial(self, CompField::Threshold, "Threshold", cx))
            .child(dial(self, CompField::Ratio, "Ratio", cx))
            .child(dial(self, CompField::Attack, "Attack", cx))
            .child(dial(self, CompField::Release, "Release", cx))
            .child(dial(self, CompField::Makeup, "Makeup", cx))
            .child(dial(self, CompField::Spectral, "Spectral", cx))
            .child(dial(self, CompField::Mix, "Mix", cx));
        let side = div()
            .w(px(232.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(side_row(
                "Release",
                choice_tray(
                    "comp-release",
                    &[("Set", false), ("Auto", true)],
                    c.auto_release,
                    22.,
                    cx,
                    move |ws, auto| {
                        if let Some(c) = ws.fx_unit_mut::<Comp>(ch) {
                            c.auto_release = auto;
                        }
                    },
                ),
            ))
            .child(side_row(
                "Sidechain HP",
                self.fx_pill(
                    Target::Comp(ch, CompField::Sidechain),
                    strip::comp_text(CompField::Sidechain, &c),
                    cx,
                ),
            ))
            .child(match c.style {
                CompStyle::Clean => side_row(
                    "Detector",
                    choice_tray(
                        "comp-detector",
                        &[("Peak", false), ("RMS", true)],
                        c.rms,
                        22.,
                        cx,
                        move |ws, rms| {
                            if let Some(c) = ws.fx_unit_mut::<Comp>(ch) {
                                c.rms = rms;
                            }
                        },
                    ),
                ),
                _ => div().h(px(28.)),
            })
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child(style_line(c.style)),
            );
        let controls = div()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(22.))
            .px(px(26.))
            .child(knobs)
            .child(div().w(px(1.)).h(px(120.)).bg(rgb(LINE)))
            .child(side);
        vec![
            head.into_any_element(),
            display.into_any_element(),
            controls.into_any_element(),
        ]
    }

    /// What goes in, comes out and is taken off, scrolling by, newest at the right; the
    /// threshold across.
    fn comp_history(&self, c: &Comp, color: u32) -> Div {
        let view = self.fx_view.as_ref().expect("open");
        let history: Vec<[f32; 4]> = view.history.iter().copied().collect();
        let empty = history.is_empty();
        let threshold = c.threshold_db;
        let on = !c.bypass;
        let graph = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                for db in [-6.0, -12.0, -18.0, -24.0, -30.0, -36.0, -42.0] {
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
                    // In and out, filled up from the floor.
                    for (k, tint) in [(0, alpha(0xffffff, 0.09)), (1, alpha(color, 0.07))] {
                        let mut area = PathBuilder::fill();
                        area.move_to(point(x(0), bottom));
                        for (i, l) in history.iter().enumerate() {
                            area.line_to(point(x(i), y(l[k])));
                        }
                        area.line_to(point(x(n - 1), bottom));
                        area.close();
                        if let Ok(p) = area.build() {
                            window.paint_path(p, tint);
                        }
                    }
                    for (k, width, tint) in [
                        (0, 1.0, alpha(0xffffff, 0.22)),
                        (
                            1,
                            1.5,
                            if on {
                                rgb(color).into()
                            } else {
                                alpha(TEXT_3, 1.0)
                            },
                        ),
                    ] {
                        let mut edge = PathBuilder::stroke(px(width));
                        edge.move_to(point(x(0), y(history[0][k])));
                        for (i, l) in history.iter().enumerate().skip(1) {
                            edge.line_to(point(x(i), y(l[k])));
                        }
                        if let Ok(p) = edge.build() {
                            window.paint_path(p, tint);
                        }
                    }
                    // What it takes off, hanging from the top on the same scale.
                    let gr = |g: f32| b.origin.y + px(level_y(-g, FLOOR, h));
                    let mut area = PathBuilder::fill();
                    area.move_to(point(x(0), b.origin.y));
                    for (i, l) in history.iter().enumerate() {
                        area.line_to(point(x(i), gr(l[2])));
                    }
                    area.line_to(point(x(n - 1), b.origin.y));
                    area.close();
                    if let Ok(p) = area.build() {
                        window.paint_path(p, alpha(GR, 0.18));
                    }
                    let mut line = PathBuilder::stroke(px(2.));
                    line.move_to(point(x(0), gr(history[0][2])));
                    for (i, l) in history.iter().enumerate().skip(1) {
                        line.line_to(point(x(i), gr(l[2])));
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, rgb(GR));
                    }
                }
                dashed(
                    window,
                    b,
                    level_y(threshold, FLOOR, h),
                    w,
                    alpha(TEXT_2, 0.55),
                );
            },
        )
        .absolute()
        .size_full();
        let h = DISPLAY_H - 2.0;
        let scale = [-6.0, -12.0, -18.0, -24.0, -30.0, -36.0, -42.0].map(|db: f32| {
            div()
                .absolute()
                .right(px(6.))
                .top(px(level_y(db, FLOOR, h) - 12.0))
                .font_family(MONO)
                .text_size(px(9.5))
                .text_color(rgb(TEXT_4))
                .child(format!("{db:.0}"))
        });
        let ty = (level_y(threshold, FLOOR, h) - 15.0).max(3.0);
        well()
            .flex_1()
            .h_full()
            .debug_selector(|| "comp-history".into())
            .child(graph)
            .children(scale)
            .child(tag(
                10.,
                ty,
                format!("Threshold {} dB", db_text(threshold)),
                TEXT_2,
            ))
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
                    .child(legend(GR, "Reduction")),
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

    /// Its curve: what each level comes out at, 1:1 faint under it; the level now a dot.
    fn comp_curve(&self, c: &Comp, color: u32) -> Div {
        let view = self.fx_view.as_ref().expect("open");
        let now = view.history.back().copied();
        let c = *c;
        let graph = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let s = f32::from(b.size.width);
                let at = |i: f32, o: f32| {
                    point(
                        b.origin.x + px(s - level_y(i, FLOOR, s)),
                        b.origin.y + px(level_y(o, FLOOR, s)),
                    )
                };
                for db in [-12.0, -24.0, -36.0] {
                    let k = level_y(db, FLOOR, s).round();
                    window.paint_quad(fill(
                        Bounds::new(point(b.origin.x, b.origin.y + px(k)), size(px(s), px(1.))),
                        rgb(LINE_SOFT),
                    ));
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(s - k), b.origin.y),
                            size(px(1.), px(s)),
                        ),
                        rgb(LINE_SOFT),
                    ));
                }
                let mut unity = PathBuilder::stroke(px(1.));
                unity.move_to(at(FLOOR, FLOOR));
                unity.line_to(at(0.0, 0.0));
                if let Ok(p) = unity.build() {
                    window.paint_path(p, alpha(0xffffff, 0.12));
                }
                let steps = 120;
                let level = |k: usize| FLOOR + (0.0 - FLOOR) * k as f32 / steps as f32;
                let mut area = PathBuilder::fill();
                area.move_to(point(b.origin.x, b.origin.y + b.size.height));
                for k in 0..=steps {
                    area.line_to(at(level(k), curve(&c, level(k))));
                }
                area.line_to(point(b.origin.x + b.size.width, b.origin.y + b.size.height));
                area.close();
                if let Ok(p) = area.build() {
                    window.paint_path(p, alpha(color, if c.bypass { 0.03 } else { 0.1 }));
                }
                let mut line = PathBuilder::stroke(px(2.));
                line.move_to(at(level(0), curve(&c, level(0))));
                for k in 1..=steps {
                    line.line_to(at(level(k), curve(&c, level(k))));
                }
                if let Ok(p) = line.build() {
                    window.paint_path(
                        p,
                        if c.bypass {
                            alpha(TEXT_3, 1.0)
                        } else {
                            rgb(color).into()
                        },
                    );
                }
                // Where it starts working.
                let t = c.threshold_db.clamp(FLOOR, 0.0);
                if let Some(p) = crate::mix::eq::disc(at(t, curve(&c, t)), 3.5) {
                    window.paint_path(p, rgb(TEXT_2));
                }
                // The level now, where it's come out.
                if let Some([i, _, g, _]) = now.filter(|l| l[0] > FLOOR) {
                    let dot = at(i, i - g + c.makeup_db);
                    if let Some(p) = crate::mix::eq::disc(dot, 9.0) {
                        window.paint_path(p, alpha(color, 0.28));
                    }
                    if let Some(p) = crate::mix::eq::disc(dot, 4.5) {
                        window.paint_path(p, rgb(TEXT));
                    }
                }
            },
        )
        .absolute()
        .size_full();
        well()
            .size(px(DISPLAY_H))
            .flex_none()
            .debug_selector(|| "comp-curve".into())
            .child(graph)
            .child(tag(8., 6., "OUT", TEXT_4))
            .child(
                div()
                    .absolute()
                    .right(px(8.))
                    .bottom(px(6.))
                    .font_family(MONO)
                    .text_size(px(9.5))
                    .text_color(rgb(TEXT_4))
                    .child("IN"),
            )
            .child(
                div()
                    .absolute()
                    .right(px(8.))
                    .top(px(6.))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_2))
                    .child(format!("{} : 1", strip::trim(c.ratio))),
            )
    }

    /// How far it's turning down now: a bar hanging from the top, and the number.
    fn comp_meter(&self, ch: Channel) -> Div {
        let gr = self
            .reductions
            .get(&self.channel_id(ch))
            .copied()
            .unwrap_or(0.0);
        let shown = (gr / METER_DB).clamp(0.0, 1.0);
        let ticks = [3.0, 6.0, 12.0, 18.0].map(|db: f32| {
            div()
                .absolute()
                .left(px(18.))
                .top(px(db / METER_DB * (DISPLAY_H - 46.0) - 6.0))
                .font_family(MONO)
                .text_size(px(9.))
                .text_color(rgb(TEXT_4))
                .child(format!("{db:.0}"))
        });
        div()
            .w(px(48.))
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex_1()
                    .child(
                        div()
                            .absolute()
                            .left(px(4.))
                            .top_0()
                            .bottom_0()
                            .w(px(10.))
                            .rounded(px(3.))
                            .bg(rgb(0x0b0b0e))
                            .child(
                                div()
                                    .w_full()
                                    .h(gpui::relative(shown))
                                    .rounded(px(3.))
                                    .bg(rgb(GR)),
                            ),
                    )
                    .children(ticks),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(if gr > 0.05 { GR } else { TEXT_3 }))
                    .child(format!("{:.1}", -gr.max(0.0))),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_4))
                    .child("GR dB"),
            )
    }

    /// A value to drag up and down in a pill (⌘-click puts it back).
    pub(super) fn fx_pill(&self, target: Target, text: String, cx: &mut Context<Self>) -> Div {
        let target = self.fx_target(target);
        let value = self.target(target);
        div()
            .h(px(26.))
            .min_w(px(78.))
            .px(px(9.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .bg(rgb(FIELD))
            .cursor_ns_resize()
            .hover(|s| s.bg(rgb(RAISED)))
            .font_family(MONO)
            .text_size(px(11.5))
            .whitespace_nowrap()
            .debug_selector(move || format!("fx-pill-{target:?}"))
            .child(text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let y = pt(e.position).y;
                    ws.begin_drag(Drag::Knob { target, y, value }, cx);
                }),
            )
    }
}

/// A setting's name and its control, side by side.
pub(super) fn side_row(name: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.))
        .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(name))
        .child(control)
}

/// A swatch and what it stands for.
pub(super) fn legend(color: u32, name: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(5.))
        .text_color(rgb(TEXT_3))
        .child(div().size(px(7.)).rounded(px(2.)).bg(rgb(color)))
        .child(name)
}
