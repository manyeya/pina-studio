//! The mixer, in place of the arrangement: a strip for every track, aux and the master, labelled
//! down the left; and on the right the selected channel's chain, FL Studio style: its instrument,
//! its EQ to drag about, its compressor and saturator, its plugins, its sends.

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};
use tempo_engine::{
    dsp::sat_curve,
    player::Which,
    project::{Comp, Eq, Instrument, Saturator},
};

use crate::{
    menu::{Menu, MenuAction},
    strip::{self, Builtin, Channel, Layout, MIXER_INSERTS, MIXER_SENDS},
    theme::*,
    workspace::{Channels, CompField, Drag, SatField, Slider, Target, Workspace, pt},
};

/// The EQ graph's range: 20 Hz to 20 kHz across, ±18 dB up and down.
pub fn graph_x(freq: f32) -> f32 {
    (freq.max(20.0) / 20.0).log10() / 3.0
}

pub fn graph_y(db: f32) -> f32 {
    0.5 - db.clamp(-18.0, 18.0) / 36.0
}

impl Workspace {
    pub fn mixer_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let project = &self.session.project;
        let tracks = (0..project.tracks.len()).map(Channel::Track);
        let auxes = (0..project.auxes.len()).map(Channel::Aux);
        let channels: Vec<Channel> = match self.channels {
            Channels::All => tracks.chain(auxes).collect(),
            Channels::Tracks => tracks.collect(),
            Channels::Buses => auxes.collect(),
        };
        let layout = Layout::Mixer {
            rows: self.mixer_rows,
            fader: self.mixer_fader(),
        };
        let first_bus = channels.iter().position(|c| matches!(c, Channel::Aux(_)));
        let mut strips: Vec<Div> = Vec::new();
        for (i, ch) in channels.into_iter().enumerate() {
            let s = strip::strip(self, ch, layout, cx);
            strips.push(if Some(i) == first_bus && i > 0 {
                s.ml(px(10.))
            } else {
                s
            });
        }
        strips.push(strip::strip(self, Channel::Master, layout, cx).ml(px(10.)));

        let filter = |label: &'static str, which: Channels, on: bool, cx: &mut Context<Self>| {
            seg(label, on, 24.).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.channels = which;
                    cx.notify();
                }),
            )
        };
        let show = |label: &'static str, row: usize, on: bool, cx: &mut Context<Self>| {
            button(label, on).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.mixer_rows[row] ^= true;
                    cx.notify();
                }),
            )
        };
        let toolbar = div()
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Mixer"),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(filter(
                        "All",
                        Channels::All,
                        self.channels == Channels::All,
                        cx,
                    ))
                    .child(filter(
                        "Tracks",
                        Channels::Tracks,
                        self.channels == Channels::Tracks,
                        cx,
                    ))
                    .child(filter(
                        "Buses",
                        Channels::Buses,
                        self.channels == Channels::Buses,
                        cx,
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child("Show"),
            )
            .child(show("EQ", 0, self.mixer_rows[0], cx))
            .child(show("Effects", 1, self.mixer_rows[1], cx))
            .child(show("Sends", 2, self.mixer_rows[2], cx));

        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(rgb(LANE))
                    .child(toolbar)
                    .child(
                        div()
                            .id("mixer")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_scroll()
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .p(px(12.))
                                    .pl(px(16.))
                                    .child(self.mixer_labels())
                                    .children(strips),
                            ),
                    ),
            )
            .child(self.channel_panel(cx))
    }

    /// The faders take what height the window leaves the strips, and the rows it doesn't show.
    fn mixer_fader(&self) -> f32 {
        let [eq, effects, sends] = self.mixer_rows;
        // The rest of a strip, gaps and padding included, with every row showing.
        let rest = 464.0;
        let freed = (!eq as u8 as f32) * 52.0
            + (!effects as u8 as f32) * (MIXER_INSERTS as f32 * 25.0 + 5.0)
            + (!sends as u8 as f32) * (MIXER_SENDS as f32 * 25.0 + 5.0);
        let room = self.viewport_h - CONTROL_BAR_H - STATUS_H - 40.0 - 24.0;
        (room - rest + freed).max(120.0)
    }

    /// What each row of the strips is, down the left, lined up with them.
    fn mixer_labels(&self) -> Div {
        let [eq, effects, sends] = self.mixer_rows;
        // Heights of the rows above each label, as the strips lay them out (8 between).
        let mut rows: Vec<(&'static str, f32)> = vec![("", 28.0)];
        if eq {
            rows.push(("EQ", 44.0));
        }
        rows.push(("Input", 24.0));
        if effects {
            rows.push(("Effects", MIXER_INSERTS as f32 * 25.0 - 3.0));
        }
        if sends {
            rows.push(("Sends", MIXER_SENDS as f32 * 25.0 - 3.0));
        }
        rows.extend([
            ("Output", 22.0),
            ("Pan", 36.0),
            ("Level", 18.0),
            ("", self.mixer_fader()),
            ("", 22.0),
            ("Name", 28.0),
        ]);
        div()
            .w(px(56.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .py(px(9.))
            .children(rows.into_iter().map(|(label, h)| {
                div()
                    .h(px(h))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .text_size(px(10.5))
                    .line_height(px(22.))
                    .text_color(rgb(TEXT_3))
                    .child(label)
            }))
    }

    /// The selected channel's chain, top to bottom as it plays.
    fn channel_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let ch = self.channel;
        let project = &self.session.project;
        let (name, color) = self.channel_look(ch);
        let (glyph, sub) = match ch {
            Channel::Track(i) => {
                let t = &project.tracks[i];
                let to = t
                    .output
                    .and_then(|b| project.auxes.iter().find(|a| a.bus == b))
                    .map_or("Stereo Out".to_string(), |a| a.name.clone());
                (
                    crate::looks::glyph(self.shown(i).look.icon, 18.),
                    format!("Channel {} · to {to}", i + 1),
                )
            }
            Channel::Aux(i) => (
                icon(Icon::Bus, 18.),
                format!("Bus {} · to Stereo Out", project.auxes[i].bus),
            ),
            Channel::Master => (icon(Icon::Output, 18.), "Output 1–2".to_string()),
        };
        let header = div()
            .flex_none()
            .h(px(44.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .size(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .bg(alpha(color, 0.14))
                    .text_color(rgb(color))
                    .child(glyph.text_color(rgb(color))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(sub)),
            );

        let instrument = match ch {
            Channel::Track(i) => {
                let t = &project.tracks[i];
                let look_icon = self.shown(i).look.icon;
                let (title, sub, opens) = match &t.instrument {
                    None => (
                        "Audio input".to_string(),
                        "Recorded and imported audio".to_string(),
                        false,
                    ),
                    Some(Instrument::Synth) => {
                        ("Sine Synth".into(), "Instrument · built in".into(), false)
                    }
                    Some(Instrument::Drums) => {
                        ("Drum Kit".into(), "Instrument · built in".into(), false)
                    }
                    Some(Instrument::Sampler(s)) => (
                        crate::tracks::sampler_name(s),
                        format!("Sampler · built in · {} samples", s.zones.len()),
                        false,
                    ),
                    Some(Instrument::Plugin(r)) => {
                        (r.class.clone(), "Instrument · VST3".into(), true)
                    }
                };
                Some(
                    card()
                        .h(px(56.))
                        .flex_row()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .size(px(34.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(8.))
                                .bg(rgb(FIELD))
                                .text_color(rgb(TEXT_2))
                                .child(
                                    match opens {
                                        true => icon(Icon::Plug, 16.),
                                        false => crate::looks::glyph(look_icon, 16.),
                                    }
                                    .text_color(rgb(TEXT_2)),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(title),
                                )
                                .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(sub)),
                        )
                        .child(
                            button(if opens { "Open" } else { "Change" }, true).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    if opens {
                                        ws.open_editor(ch, Which::Instrument, cx);
                                    } else {
                                        ws.menu =
                                            Some(ws.plugin_menu(ch, Which::Instrument, e.position));
                                        cx.notify();
                                    }
                                }),
                            ),
                        ),
                )
            }
            _ => None,
        };

        let (eq, comp) = self.strip_dsp(ch);
        let sat = self.sat(ch);
        let inserts = self.inserts(ch).to_vec();
        let used = inserts.len()
            + eq.is_some() as usize
            + comp.is_some() as usize
            + sat.is_some() as usize;
        let mut chain: Vec<Div> = Vec::new();
        if let Some(eq) = eq {
            chain.push(self.eq_card(ch, eq, cx));
        }
        if let Some(comp) = comp {
            chain.push(self.comp_card(ch, comp, cx));
        }
        if let Some(sat) = sat {
            chain.push(self.sat_card(ch, sat, cx));
        }
        for (n, r) in inserts.iter().enumerate() {
            chain.push(
                card()
                    .h(px(44.))
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(icon(Icon::Plug, 15.).text_color(rgb(TEXT_3)))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(r.class.clone()),
                    )
                    .child(button("Open", false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.open_editor(ch, Which::Insert(n), cx)),
                    ))
                    .child(icon_seg(Icon::Close, false, 26.).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.choose(
                                MenuAction::Insert {
                                    channel: ch,
                                    index: n,
                                    plugin: None,
                                },
                                cx,
                            )
                        }),
                    )),
            );
        }
        let add = Some({
            let index = inserts.len();
            div()
                .flex_none()
                .h(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .rounded(px(10.))
                .border_1()
                .border_dashed()
                .border_color(rgb(0x2e2e36))
                .text_color(rgb(TEXT_3))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .hover(|s| s.border_color(rgb(LINE_STRONG)).text_color(rgb(TEXT_2)))
                .child(icon(Icon::Plus, 13.).text_color(rgb(TEXT_3)))
                .child("Add effect")
                .debug_selector(|| "add-effect".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        ws.menu = Some(ws.plugin_menu(ch, Which::Insert(index), e.position));
                        cx.notify();
                    }),
                )
        });

        let sends = match ch {
            Channel::Track(t) => project.tracks[t]
                .sends
                .iter()
                .enumerate()
                .map(|(index, s)| {
                    let bus = project
                        .auxes
                        .iter()
                        .find(|a| a.bus == s.bus)
                        .map_or(format!("Bus {}", s.bus), |a| a.name.clone());
                    let slider = Slider::Send { track: t, index };
                    let pre = s.pre;
                    self.slider_row(bus, format!("{} dB", db_text(s.level_db)), slider, cx)
                        .child(
                            seg(if pre { "Pre" } else { "Post" }, pre, 22.)
                                .w(px(40.))
                                .justify_center()
                                .debug_selector(move || format!("send-pre-{index}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        let action = MenuAction::SendPre {
                                            track: t,
                                            index,
                                            pre: !pre,
                                        };
                                        ws.choose(action, cx);
                                    }),
                                ),
                        )
                })
                .collect(),
            _ => Vec::new(),
        };

        div()
            .w(px(PANEL_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .id("channel-panel")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(header)
                    .children(instrument)
                    .child(
                        div()
                            .h(px(16.))
                            .mt(px(4.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(section_title("Effects"))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{used} in the chain")),
                            ),
                    )
                    .children(chain)
                    .children(add)
                    .when(!sends.is_empty(), |d| {
                        d.child(
                            div()
                                .mt(px(4.))
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex()
                                        .justify_between()
                                        .child(section_title("Sends"))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(rgb(TEXT_3))
                                                .child("Before or after the fader"),
                                        ),
                                )
                                .children(sends),
                        )
                    }),
            )
    }

    fn eq_card(&self, ch: Channel, eq: Eq, cx: &mut Context<Self>) -> Div {
        let rate = self.session.project.sample_rate as f64;
        let curve = strip::eq_curve(&eq, rate, 160);
        let color = self.channel_look(ch).1;
        let on = !eq.bypass;
        let graph = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let line = |window: &mut gpui::Window, x: f32, y: f32, ww: f32, hh: f32, c: u32| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        rgb(c),
                    ));
                };
                for f in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
                    line(window, (graph_x(f) * w).round(), 0.0, 1.0, h, LINE_SOFT);
                }
                for db in [12.0, 6.0, -6.0, -12.0] {
                    line(window, 0.0, (graph_y(db) * h).round(), w, 1.0, LINE_SOFT);
                }
                line(window, 0.0, (h / 2.0).round(), w, 1.0, RAISED);
                let x = |i: usize| b.origin.x + px(i as f32 * w / (curve.len() - 1) as f32);
                let y = |g: f32| b.origin.y + px((graph_y(g) * h).clamp(1.0, h - 1.0));
                let mut area = gpui::PathBuilder::fill();
                area.move_to(point(b.origin.x, b.origin.y + px(h / 2.0)));
                for (i, &g) in curve.iter().enumerate() {
                    area.line_to(point(x(i), y(g)));
                }
                area.line_to(point(b.origin.x + b.size.width, b.origin.y + px(h / 2.0)));
                area.close();
                if let Ok(p) = area.build() {
                    window.paint_path(p, alpha(color, if on { 0.14 } else { 0.05 }));
                }
                let mut stroke = gpui::PathBuilder::stroke(px(2.));
                stroke.move_to(point(x(0), y(curve[0])));
                for (i, &g) in curve.iter().enumerate().skip(1) {
                    stroke.line_to(point(x(i), y(g)));
                }
                if let Ok(p) = stroke.build() {
                    window.paint_path(
                        p,
                        if on {
                            rgb(color).into()
                        } else {
                            alpha(TEXT_3, 1.0)
                        },
                    );
                }
            },
        )
        .size_full();
        // A dot where each band is; the graph opens the EQ's window to work on them.
        let width = PANEL_W - 32.0 - 22.0;
        let height = 120.0;
        let dots = eq
            .bands
            .iter()
            .enumerate()
            .map(|(i, band)| {
                let db = match band.shape.has_gain() {
                    true => band.gain(eq.scale),
                    false => 0.0,
                };
                div()
                    .absolute()
                    .left(px(graph_x(band.freq) * width - 6.0))
                    .top(px(graph_y(db) * height - 6.0))
                    .size(px(12.))
                    .rounded(px(6.))
                    .when(!band.off, |d| d.bg(rgb(crate::eq::band_color(i))))
                    .when(band.off, |d| {
                        d.border_1()
                            .border_color(alpha(crate::eq::band_color(i), 0.6))
                    })
                    .debug_selector(move || format!("eq-card-band-{i}"))
            })
            .collect::<Vec<_>>();
        let labels = [
            (50.0, "50"),
            (100.0, "100"),
            (500.0, "500"),
            (1000.0, "1k"),
            (5000.0, "5k"),
            (10000.0, "10k"),
        ]
        .map(|(f, label)| {
            div()
                .absolute()
                .left(px(graph_x(f) * width - 12.0))
                .w(px(24.))
                .flex()
                .justify_center()
                .font_family(MONO)
                .text_size(px(9.))
                .text_color(rgb(TEXT_3))
                .child(label)
        });
        let n = eq.bands.len();
        let readout = match n {
            0 => "No bands yet · click to open the EQ".to_string(),
            _ => {
                let shapes: Vec<String> = eq
                    .bands
                    .iter()
                    .take(4)
                    .map(|b| match b.shape.has_gain() {
                        true => format!("{} {}", strip::hz(b.freq), db_text(b.gain_db)),
                        false => format!("{} {}", b.shape.name(), strip::hz(b.freq)),
                    })
                    .collect();
                let more = if n > 4 {
                    format!(" · +{}", n - 4)
                } else {
                    String::new()
                };
                format!("{}{more}", shapes.join(" · "))
            }
        };
        card()
            .gap(px(10.))
            .child(self.card_header("Channel EQ", ch, Builtin::Eq, on, cx))
            .child(
                div()
                    .relative()
                    .h(px(height))
                    .rounded(px(8.))
                    .bg(rgb(WELL))
                    .cursor_pointer()
                    .debug_selector(|| "eq-card".into())
                    .child(graph)
                    .children(dots)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.open_eq(ch, cx);
                        }),
                    ),
            )
            .child(div().relative().h(px(10.)).mt(px(-4.)).children(labels))
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(readout),
            )
    }

    fn comp_card(&self, ch: Channel, comp: Comp, cx: &mut Context<Self>) -> Div {
        let id = self.channel_id(ch);
        let reduction = self.reductions.get(&id).copied().unwrap_or(0.0);
        let color = self.channel_look(ch).1;
        let on = !comp.bypass;
        let knobs = [
            (CompField::Threshold, "Threshold"),
            (CompField::Ratio, "Ratio"),
            (CompField::Attack, "Attack"),
            (CompField::Release, "Release"),
            (CompField::Makeup, "Makeup"),
        ]
        .map(|(field, label)| {
            let target = Target::Comp(ch, field);
            let value = strip::comp_pos(field, &comp);
            div()
                .w(px(58.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .cursor_ns_resize()
                .debug_selector(move || format!("comp-{field:?}"))
                .child(self.modded(self.mod_key(target), 36., knob(value, 36., color, false)))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .child(strip::comp_text(field, &comp)),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(label),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let y = pt(e.position).y;
                        ws.begin_drag(Drag::Knob { target, y, value }, cx);
                    }),
                )
        });
        let shown = (reduction / 20.0).clamp(0.0, 1.0);
        card()
            .gap(px(10.))
            .child(self.card_header("Compressor", ch, Builtin::Comp, on, cx))
            .child(
                div()
                    .h(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .child("Gain reduction"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(6.))
                            .rounded(px(3.))
                            .bg(rgb(WELL))
                            .flex()
                            .justify_end()
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(shown))
                                    .rounded(px(3.))
                                    .bg(rgb(METER_MID)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(52.))
                            .flex()
                            .justify_end()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(if reduction > 0.05 { METER_MID } else { TEXT_3 }))
                            .child(format!("{} dB", db_text(-reduction))),
                    ),
            )
            .child(div().flex().justify_between().children(knobs))
            .child(
                // What it listens to: the peaks, or an RMS average (gentler, more like the ear).
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child("Detector"),
                    )
                    .child(tray().p(px(2.)).rounded(px(7.)).children(
                        [("Peak", false), ("RMS", true)].map(|(label, rms)| {
                            seg(label, comp.rms == rms, 22.)
                                .debug_selector(move || format!("comp-detector-{label}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        ws.edit(cx, |ws| {
                                            if let Some(c) = ws.strip_dsp_mut(ch).1 {
                                                c.rms = rms;
                                            }
                                        })
                                    }),
                                )
                        }),
                    )),
            )
    }

    /// The saturator: its curve (how hard it rounds off what comes in), and its knobs.
    fn sat_card(&self, ch: Channel, sat: Saturator, cx: &mut Context<Self>) -> Div {
        let color = self.channel_look(ch).1;
        let on = !sat.bypass;
        let curve = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let line = |window: &mut gpui::Window, x: f32, y: f32, ww: f32, hh: f32| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        rgb(LINE_SOFT),
                    ));
                };
                line(window, 0.0, (h / 2.0).round(), w, 1.0);
                line(window, (w / 2.0).round(), 0.0, 1.0, h);
                // In from -1 to 1 across, out from -1 to 1 up: straight is clean.
                let at = |x: f32, y: f32| {
                    point(
                        b.origin.x + px((x + 1.0) / 2.0 * w),
                        b.origin.y + px((1.0 - (y.clamp(-1.0, 1.0) + 1.0) / 2.0) * h),
                    )
                };
                let mut clean = gpui::PathBuilder::stroke(px(1.));
                clean.move_to(at(-1.0, -1.0));
                clean.line_to(at(1.0, 1.0));
                if let Ok(p) = clean.build() {
                    window.paint_path(p, alpha(TEXT_4, 1.0));
                }
                let mut path = gpui::PathBuilder::stroke(px(2.));
                for i in 0..=64 {
                    let x = i as f32 / 32.0 - 1.0;
                    let y = sat_curve(&sat, x);
                    match i {
                        0 => path.move_to(at(x, y)),
                        _ => path.line_to(at(x, y)),
                    }
                }
                if let Ok(p) = path.build() {
                    let c = if on {
                        rgb(color).into()
                    } else {
                        alpha(TEXT_3, 1.0)
                    };
                    window.paint_path(p, c);
                }
            },
        )
        .size_full();
        let knobs = [
            (SatField::Drive, "Drive", false),
            (SatField::Tone, "Tone", true),
            (SatField::Mix, "Mix", false),
            (SatField::Output, "Output", true),
        ]
        .map(|(field, label, bipolar)| {
            let target = Target::Sat(ch, field);
            let value = strip::sat_pos(field, &sat);
            let shown = if bipolar { value * 2.0 - 1.0 } else { value };
            div()
                .w(px(60.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .cursor_ns_resize()
                .debug_selector(move || format!("sat-{field:?}"))
                .child(self.modded(self.mod_key(target), 36., knob(shown, 36., color, bipolar)))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .child(strip::sat_text(field, &sat)),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(label),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let y = pt(e.position).y;
                        ws.begin_drag(Drag::Knob { target, y, value }, cx);
                    }),
                )
        });
        card()
            .gap(px(10.))
            .child(self.card_header("Saturator", ch, Builtin::Sat, on, cx))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(
                        div()
                            .size(px(64.))
                            .flex_none()
                            .rounded(px(8.))
                            .bg(rgb(WELL))
                            .p(px(6.))
                            .child(curve),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap(px(3.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(format!(
                                "{} into a soft clip",
                                strip::sat_text(SatField::Drive, &sat)
                            ))
                            .child(format!(
                                "{} · {} wet",
                                strip::sat_text(SatField::Tone, &sat),
                                strip::sat_text(SatField::Mix, &sat)
                            )),
                    ),
            )
            .child(div().flex().justify_between().children(knobs))
    }

    /// A built-in's title row: on/off, name, and taking it out.
    fn card_header(
        &self,
        title: &'static str,
        ch: Channel,
        b: Builtin,
        on: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(26.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                switch(on)
                    .debug_selector(move || format!("bypass-{b:?}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit(cx, |ws| match b {
                                Builtin::Eq => {
                                    if let Some(e) = ws.strip_dsp_mut(ch).0 {
                                        e.bypass ^= true
                                    }
                                }
                                Builtin::Comp => {
                                    if let Some(c) = ws.strip_dsp_mut(ch).1 {
                                        c.bypass ^= true
                                    }
                                }
                                Builtin::Sat => {
                                    if let Some(s) = ws.sat_mut(ch) {
                                        s.bypass ^= true
                                    }
                                }
                            })
                        }),
                    ),
            )
            .child(div().font_size_title().child(title))
            .child(div().flex_1())
            .child(icon_seg(Icon::More, false, 26.).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.menu = Some(Menu {
                        at: e.position,
                        items: vec![(
                            "Remove".into(),
                            MenuAction::RemoveBuiltin {
                                channel: ch,
                                builtin: b,
                            },
                        )],
                        ..Default::default()
                    });
                    cx.notify();
                }),
            ))
    }
}

/// A raised card in the side panel.
fn card() -> Div {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .p(px(10.))
        .rounded(px(10.))
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(LINE))
}

trait Title {
    fn font_size_title(self) -> Self;
}

impl Title for Div {
    fn font_size_title(self) -> Self {
        self.text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD)
    }
}

impl Workspace {
    /// A labelled horizontal slider, dragged sideways.
    pub fn slider_row(
        &self,
        label: String,
        value: String,
        slider: Slider,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(28.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .w(px(56.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_color(rgb(TEXT_2))
                    .child(label),
            )
            .child(self.slider_bar(slider, cx).flex_1())
            .child(
                div()
                    .w(px(64.))
                    .flex()
                    .justify_end()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(value),
            )
    }

    /// A slider's bar: filled to its value, with a thumb. Click or drag anywhere on it.
    pub fn slider_bar(&self, slider: Slider, cx: &mut Context<Self>) -> Div {
        let v = self.slider(slider).clamp(0.0, 1.0);
        let bounds = self.slider_bounds.clone();
        div()
            .h(px(16.))
            .relative()
            .flex()
            .items_center()
            .cursor_ew_resize()
            .child(
                div()
                    .w_full()
                    .h(px(4.))
                    .rounded(px(2.))
                    .bg(rgb(RAISED))
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(v))
                            .rounded(px(2.))
                            .bg(rgb(TEXT_3)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(gpui::relative(v))
                    .ml(px(-6.))
                    .top(px(2.))
                    .size(px(12.))
                    .rounded(px(6.))
                    .bg(rgb(TEXT))
                    .border_2()
                    .border_color(rgb(SURFACE)),
            )
            .child(
                canvas(
                    move |b, _, _| {
                        bounds.borrow_mut().insert(slider, b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.start_slider(slider, pt(e.position), cx);
                }),
            )
    }
}
