//! The reverb's and delay's windows. The reverb's draws what it does to a click: the pre-delay,
//! its first reflections, then its tail falling, its lows, middle and highs each as long as
//! they ring; its styles along the top, its knobs under it. The delay's draws its echoes, each
//! side's on its own lane over the song's beats, as loud as its feedback and crossing leave
//! them; its styles, time in note values or ms, and knobs.

use pina_engine::{
    dsp::reverb::picture,
    song::project::{Delay, DelayStyle, Reverb, ReverbStyle},
};

use super::comp::side_row;
use super::*;
use crate::{
    mix::strip::{DelayField, NOTES, ReverbField, delay_text, nearest_note, reverb_text},
    workspace::Knob,
};

pub(super) const DISPLAY_H: f32 = 226.0;
/// The frequency picture's width, and the time picture's beside it (the window's, less its
/// margins and the gap).
pub(super) const FREQ_W: f32 = 330.0;
pub(super) const TIME_W: f32 = 980.0 - 28.0 - FREQ_W - 8.0;

/// A point on the reverb's or delay's pictures, to drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpaceHandle {
    Predelay,
    Decay,
    Bass,
    Damping,
    LowCut,
    HighCut,
    /// A delay side's time (0 left).
    Time(u8),
    Feedback,
    /// A tool's point, as its display numbers them.
    Tool(u8),
}
/// The display's floor, dB.
const DB_FLOOR: f32 = -60.0;
pub(super) const LOW: u32 = 0x60a5fa;
pub(super) const MIDDLE: u32 = 0x34d399;
pub(super) const HIGH: u32 = 0xfbbf24;

/// A knob on the stacked reverb or delay the window shows.
fn knob(ch: Channel, k: usize, knob: Knob) -> Target {
    Target::Stacked(ch, k, knob)
}

impl Workspace {
    /// A point on the reverb's or delay's pictures dragged to `p`: what it sets, set. `span` is
    /// the seconds across the time picture as the drag began.
    pub fn drag_space(&mut self, which: SpaceHandle, span: f32, p: gpui::Point<f32>) {
        let Some(v) = self.fx_view.as_ref() else {
            return;
        };
        let (ch, a, b) = (v.ch, v.plot.get(), v.plot2.get());
        let at = |b: Bounds<Pixels>| {
            (
                ((p.x - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0),
                ((p.y - f32::from(b.origin.y)) / f32::from(b.size.height).max(1.0)).clamp(0.0, 1.0),
            )
        };
        let (tx, ty) = at(a);
        let (fx, fy) = at(b);
        if let SpaceHandle::Tool(i) = which {
            return self.drag_tool(ch, i, (tx, ty), (fx, fy));
        }
        let bpm = self.session.project.tempo;
        let beats = |s: f32| s * bpm as f32 / 60.0;
        let hz = (x_freq(fx)).round();
        if let Some(r) = self.fx_unit_mut::<Reverb>(ch) {
            let (_, tail_from) = picture(r, bpm);
            match which {
                SpaceHandle::Predelay => {
                    let t = (tx * span).clamp(0.0, 0.5);
                    match r.predelay_beats {
                        Some(_) => r.predelay_beats = Some(NOTES[nearest_note(beats(t)).min(8)].0),
                        None => r.predelay_ms = (t * 1000.0).round(),
                    }
                }
                SpaceHandle::Decay => {
                    let reach = (-DB_FLOOR - 6.0) / -DB_FLOOR;
                    let d = ((tx * span - tail_from) / reach).clamp(0.1, 30.0);
                    r.decay_s = (d * 100.0).round() / 100.0;
                }
                SpaceHandle::Bass => {
                    let ratio = y_secs(fy) / r.decay_s.max(0.1);
                    r.bass = ((ratio.clamp(0.25, 4.0)) * 100.0).round() / 100.0;
                }
                SpaceHandle::Damping => r.damping_hz = hz.clamp(500.0, 20_000.0),
                SpaceHandle::LowCut => r.low_cut_hz = hz.clamp(20.0, 2000.0),
                SpaceHandle::HighCut => r.high_cut_hz = hz.clamp(1000.0, 20_000.0),
                _ => {}
            }
            return;
        }
        if let Some(d) = self.fx_unit_mut::<Delay>(ch) {
            match which {
                SpaceHandle::Time(side) => {
                    let s = (side as usize).min(1);
                    let t = (tx * span).clamp(0.001, 5.0);
                    match d.sync {
                        true => d.beats[s] = NOTES[nearest_note(beats(t))].0,
                        false => d.ms[s] = (t * 1000.0).round(),
                    }
                    // Ping-pong's sides go together.
                    if d.style == DelayStyle::PingPong {
                        (d.beats[1], d.ms[1]) = (d.beats[0], d.ms[0]);
                    }
                }
                SpaceHandle::Feedback => {
                    // How tall the second echo is drawn, back to a level.
                    let lane = 0.5;
                    let frac = match d.style == DelayStyle::PingPong {
                        true => (ty - lane) / (lane - 14.0 / DISPLAY_H),
                        false => (lane - ty) / (lane - 14.0 / DISPLAY_H),
                    }
                    .clamp(0.0, 1.0);
                    let level = 10f32.powf(-(1.0 - frac) * -DB_FLOOR / 20.0);
                    let share = match d.style == DelayStyle::PingPong {
                        true => d.cross.max(0.5),
                        false => (1.0 - d.cross).max(0.05),
                    };
                    d.feedback = ((level / share).clamp(0.0, 1.1) * 100.0).round() / 100.0;
                }
                SpaceHandle::LowCut => d.low_cut_hz = hz.clamp(20.0, 2000.0),
                SpaceHandle::HighCut => d.high_cut_hz = hz.clamp(500.0, 20_000.0),
                _ => {}
            }
        }
    }

    pub(super) fn reverb_window(
        &self,
        ch: Channel,
        r: Reverb,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let Some(k) = self.fx_stacked() else {
            return Vec::new();
        };
        let color = self.channel_look(ch).1;
        let on = !r.bypass;
        let style = choice_tray(
            "reverb-style",
            &ReverbStyle::ALL.map(|s| (s.name(), s)),
            r.style,
            26.,
            cx,
            move |ws, s| {
                if let Some(r) = ws.fx_unit_mut::<Reverb>(ch)
                    && r.style != s
                {
                    // Its kind as it usually is, kept as loud and as wet as it was.
                    let (mix, out) = (r.mix, r.output_db);
                    *r = Reverb {
                        mix,
                        output_db: out,
                        ..s.reverb()
                    };
                }
            },
        );
        let freeze = choice_tray(
            "reverb-freeze",
            &[("Freeze", true)],
            r.freeze,
            26.,
            cx,
            move |ws, _| {
                if let Some(r) = ws.fx_unit_mut::<Reverb>(ch) {
                    r.freeze = !r.freeze;
                }
            },
        );
        let head = self.fx_head(
            "Reverb",
            ch,
            Builtin::Tool(Fx::Reverb),
            on,
            style,
            freeze,
            cx,
        );
        let bpm = self.session.project.tempo;
        let view = self.fx_view.as_ref().expect("open");
        let display = div()
            .flex_none()
            .h(px(DISPLAY_H))
            .mx(px(14.))
            .mt(px(14.))
            .flex()
            .gap(px(8.))
            .child(reverb_display(&r, bpm, on, view.plot.clone(), cx))
            .child(reverb_freq(
                &r,
                (view.spectrum.clone(), view.columns.clone()),
                on,
                view.plot2.clone(),
                cx,
            ));
        let dial = |ws: &Self, f: ReverbField, name, size: f32, cx: &mut Context<Self>| {
            let dim = matches!(f, ReverbField::Shimmer) && r.shimmer == 0.0;
            ws.fx_dial(
                knob(ch, k, Knob::Reverb(f)),
                name,
                reverb_text(f, &r),
                size,
                if !on || dim { TEXT_4 } else { color },
                false,
                cx,
            )
        };
        let row = |items: Vec<Div>| div().flex().justify_between().children(items);
        let knobs = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(row(vec![
                dial(self, ReverbField::Predelay, "Pre-delay", 50., cx),
                dial(self, ReverbField::Size, "Size", 50., cx),
                dial(self, ReverbField::Decay, "Decay", 50., cx),
                dial(self, ReverbField::Early, "Early", 50., cx),
                dial(self, ReverbField::Diffusion, "Diffusion", 50., cx),
                dial(self, ReverbField::Modulation, "Modulation", 50., cx),
                dial(self, ReverbField::Mix, "Mix", 50., cx),
            ]))
            .child(row(vec![
                dial(self, ReverbField::Damping, "Damping", 42., cx),
                dial(self, ReverbField::Bass, "Bass", 42., cx),
                dial(self, ReverbField::LowCut, "Low cut", 42., cx),
                dial(self, ReverbField::HighCut, "High cut", 42., cx),
                dial(self, ReverbField::Width, "Width", 42., cx),
                dial(self, ReverbField::Ducking, "Ducking", 42., cx),
                dial(self, ReverbField::Shimmer, "Shimmer", 42., cx),
            ]));
        let synced = r.predelay_beats.is_some();
        let side = div()
            .w(px(214.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(side_row(
                "Pre-delay",
                choice_tray(
                    "reverb-sync",
                    &[("ms", false), ("Sync", true)],
                    synced,
                    22.,
                    cx,
                    move |ws, sync| {
                        let bpm = ws.session.project.tempo as f32;
                        if let Some(r) = ws.fx_unit_mut::<Reverb>(ch) {
                            match (sync, r.predelay_beats) {
                                (true, None) => {
                                    let beats = r.predelay_ms / 1000.0 * bpm / 60.0;
                                    r.predelay_beats = Some(NOTES[nearest_note(beats).min(8)].0);
                                }
                                (false, Some(b)) => {
                                    r.predelay_ms = (b * 60.0 / bpm * 1000.0).round();
                                    r.predelay_beats = None;
                                }
                                _ => {}
                            }
                        }
                    },
                ),
            ))
            .child(side_row(
                "Output",
                self.fx_pill(
                    knob(ch, k, Knob::Reverb(ReverbField::Output)),
                    reverb_text(ReverbField::Output, &r),
                    cx,
                ),
            ))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child(reverb_line(r.style)),
            );
        let controls = div()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(22.))
            .px(px(26.))
            .child(knobs)
            .child(div().w(px(1.)).h(px(140.)).bg(rgb(LINE)))
            .child(side);
        vec![
            head.into_any_element(),
            display.into_any_element(),
            controls.into_any_element(),
        ]
    }

    pub(super) fn delay_window(
        &self,
        ch: Channel,
        d: Delay,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let Some(k) = self.fx_stacked() else {
            return Vec::new();
        };
        let color = self.channel_look(ch).1;
        let on = !d.bypass;
        let style = choice_tray(
            "delay-style",
            &DelayStyle::ALL.map(|s| (s.name(), s)),
            d.style,
            26.,
            cx,
            move |ws, s| {
                if let Some(d) = ws.fx_unit_mut::<Delay>(ch)
                    && d.style != s
                {
                    let (mix, out) = (d.mix, d.output_db);
                    *d = Delay {
                        mix,
                        output_db: out,
                        ..s.delay()
                    };
                }
            },
        );
        let freeze = choice_tray(
            "delay-freeze",
            &[("Freeze", true)],
            d.freeze,
            26.,
            cx,
            move |ws, _| {
                if let Some(d) = ws.fx_unit_mut::<Delay>(ch) {
                    d.freeze = !d.freeze;
                }
            },
        );
        let head = self.fx_head("Delay", ch, Builtin::Tool(Fx::Delay), on, style, freeze, cx);
        let bpm = self.session.project.tempo;
        let view = self.fx_view.as_ref().expect("open");
        let display = div()
            .flex_none()
            .h(px(DISPLAY_H))
            .mx(px(14.))
            .mt(px(14.))
            .flex()
            .gap(px(8.))
            .child(delay_display(&d, bpm, on, view.plot.clone(), cx))
            .child(delay_freq(
                &d,
                (view.spectrum.clone(), view.columns.clone()),
                on,
                view.plot2.clone(),
                cx,
            ));
        let dial = |ws: &Self, f: DelayField, name, size: f32, cx: &mut Context<Self>| {
            ws.fx_dial(
                knob(ch, k, Knob::Delay(f)),
                name,
                delay_text(f, &d),
                size,
                if on { color } else { TEXT_4 },
                false,
                cx,
            )
        };
        let row = |items: Vec<Div>| div().flex().justify_between().children(items);
        let knobs = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(row(vec![
                dial(self, DelayField::Time(0), "Left", 50., cx),
                dial(self, DelayField::Time(1), "Right", 50., cx),
                dial(self, DelayField::Feedback, "Feedback", 50., cx),
                dial(self, DelayField::Cross, "Cross", 50., cx),
                dial(self, DelayField::Width, "Width", 50., cx),
                dial(self, DelayField::Ducking, "Ducking", 50., cx),
                dial(self, DelayField::Mix, "Mix", 50., cx),
            ]))
            .child(row(vec![
                dial(self, DelayField::LowCut, "Low cut", 42., cx),
                dial(self, DelayField::HighCut, "High cut", 42., cx),
                dial(self, DelayField::Drive, "Drive", 42., cx),
                dial(self, DelayField::Wow, "Wow", 42., cx),
                dial(self, DelayField::Flutter, "Flutter", 42., cx),
                dial(self, DelayField::Diffusion, "Diffusion", 42., cx),
                dial(self, DelayField::Output, "Output", 42., cx),
            ]));
        let side = div()
            .w(px(214.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(side_row(
                "Time",
                choice_tray(
                    "delay-sync",
                    &[("ms", false), ("Sync", true)],
                    d.sync,
                    22.,
                    cx,
                    move |ws, sync| {
                        let bpm = ws.session.project.tempo as f32;
                        if let Some(d) = ws.fx_unit_mut::<Delay>(ch)
                            && d.sync != sync
                        {
                            // Where it was, in the other's terms.
                            for s in 0..2 {
                                match sync {
                                    true => {
                                        let beats = d.ms[s] / 1000.0 * bpm / 60.0;
                                        d.beats[s] = NOTES[nearest_note(beats)].0;
                                    }
                                    false => d.ms[s] = (d.beats[s] * 60.0 / bpm * 1000.0).round(),
                                }
                            }
                            d.sync = sync;
                        }
                    },
                ),
            ))
            .child(side_row(
                "Right",
                choice_tray(
                    "delay-link",
                    &[("Own", false), ("As left", true)],
                    d.beats[0] == d.beats[1] && d.ms[0] == d.ms[1],
                    22.,
                    cx,
                    move |ws, link| {
                        if let Some(d) = ws.fx_unit_mut::<Delay>(ch)
                            && link
                        {
                            (d.beats[1], d.ms[1]) = (d.beats[0], d.ms[0]);
                        }
                    },
                ),
            ))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child(delay_line(d.style)),
            );
        let controls = div()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(22.))
            .px(px(26.))
            .child(knobs)
            .child(div().w(px(1.)).h(px(140.)).bg(rgb(LINE)))
            .child(side);
        vec![
            head.into_any_element(),
            display.into_any_element(),
            controls.into_any_element(),
        ]
    }
}

fn reverb_line(s: ReverbStyle) -> &'static str {
    match s {
        ReverbStyle::Room => "Close walls: early, many reflections",
        ReverbStyle::Chamber => "A live room: warm and even",
        ReverbStyle::Hall => "Wide and smooth, a long tail",
        ReverbStyle::Plate => "No walls: dense at once, bright",
        ReverbStyle::Ambience => "The walls, none of the tail",
        ReverbStyle::Cathedral => "Huge, slow to build, very long",
        ReverbStyle::Shimmer => "Its tail climbs an octave",
    }
}

fn delay_line(s: DelayStyle) -> &'static str {
    match s {
        DelayStyle::Digital => "Each echo as it went in",
        DelayStyle::Tape => "Wobbling, warming, saturating",
        DelayStyle::Analog => "Darker each time round",
        DelayStyle::PingPong => "Bouncing left and right",
        DelayStyle::Dub => "Long, dark, piling up",
        DelayStyle::Slapback => "One quick echo off a near wall",
    }
}

/// A label over a display, on its own dark ground so what's drawn under it doesn't cross it.
pub(super) fn chip_tag(x: f32, y: f32, text: String, color: u32) -> Div {
    tag(x, y, text, color)
        .px(px(4.))
        .py(px(1.))
        .rounded(px(4.))
        .bg(rgb(0x0b0b0e))
}

/// A level in dB down the display: 0 at the top, the floor at the bottom.
fn db_y(db: f32, h: f32) -> f32 {
    (db / DB_FLOOR).clamp(0.0, 1.0) * h
}

/// The reverb's answer to a click, drawn: pre-delay, reflections, and each band's fall.
fn reverb_display(
    r: &Reverb,
    bpm: f64,
    on: bool,
    plot: Rc<Cell<Bounds<Pixels>>>,
    cx: &mut Context<Workspace>,
) -> Div {
    let (early, tail_from) = picture(r, bpm);
    let pre = r.predelay_s(bpm);
    let decay = r.decay_s.clamp(0.1, 30.0);
    let span = (tail_from + decay * 1.15).max(0.4);
    let bands = [
        (decay * r.bass.clamp(0.25, 4.0), LOW, "Lows"),
        (decay, MIDDLE, "Middle"),
        (decay * 0.3, HIGH, "Highs"),
    ];
    let frozen = r.freeze;
    let dim = if on { 1.0 } else { 0.35 };
    let graph = canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            plot.set(b);
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let x = |t: f32| (t / span).clamp(0.0, 1.0) * w;
            let quad =
                |window: &mut gpui::Window, x: f32, y: f32, ww: f32, hh: f32, c: gpui::Hsla| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        c,
                    ));
                };
            // Seconds across, every 10 dB down.
            let step = match span {
                s if s > 12.0 => 2.0,
                s if s > 5.0 => 1.0,
                s if s > 2.0 => 0.5,
                _ => 0.1,
            };
            let mut t = step;
            while t < span {
                quad(window, x(t).round(), 0.0, 1.0, h, rgb(LINE_SOFT).into());
                t += step;
            }
            for db in [-10.0, -20.0, -30.0, -40.0, -50.0] {
                quad(
                    window,
                    0.0,
                    db_y(db, h).round(),
                    w,
                    1.0,
                    rgb(LINE_SOFT).into(),
                );
            }
            // The pre-delay, shaded.
            quad(window, 0.0, 0.0, x(pre), h, alpha(0xffffff, 0.025));
            // Each band's tail, the longest behind.
            let mut order = bands;
            order.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (rt, c, _) in order {
                let mut area = PathBuilder::fill();
                let at =
                    |t: f32, db: f32| point(b.origin.x + px(x(t)), b.origin.y + px(db_y(db, h)));
                area.move_to(at(tail_from, DB_FLOOR));
                let n = 48;
                for i in 0..=n {
                    let t = tail_from + (span - tail_from) * i as f32 / n as f32;
                    let db = match frozen {
                        true => -6.0,
                        false => -6.0 - 60.0 * (t - tail_from) / rt,
                    };
                    area.line_to(at(t, db.max(DB_FLOOR)));
                }
                area.line_to(at(span, DB_FLOOR));
                area.close();
                if let Ok(p) = area.build() {
                    window.paint_path(p, alpha(c, 0.16 * dim));
                }
                let mut line = PathBuilder::stroke(px(1.5));
                line.move_to(at(tail_from, -6.0));
                for i in 1..=n {
                    let t = tail_from + (span - tail_from) * i as f32 / n as f32;
                    let db = if frozen {
                        -6.0
                    } else {
                        -6.0 - 60.0 * (t - tail_from) / rt
                    };
                    line.line_to(at(t, db.max(DB_FLOOR)));
                }
                if let Ok(p) = line.build() {
                    window.paint_path(p, alpha(c, 0.85 * dim));
                }
            }
            // The first reflections: a line each, as loud as it is.
            for &(t, g) in &early {
                let db = 20.0 * g.max(1e-4).log10();
                let y = db_y(db, h);
                quad(
                    window,
                    x(t).round(),
                    y,
                    1.5,
                    h - y,
                    alpha(0xffffff, 0.55 * dim),
                );
            }
            // The click.
            quad(window, 0.0, 0.0, 2.0, h, alpha(0xffffff, 0.7 * dim));
        },
    )
    .absolute()
    .size_full();
    let label = |x: f32, y: f32, text: String, c: u32| tag(x, y, text, c);
    let w = TIME_W;
    let px_at = |t: f32| (t / span).clamp(0.0, 1.0) * w;
    // Where the middle's fall reaches the floor: dragged, its decay.
    let floor_at = tail_from + decay * (-DB_FLOOR - 6.0) / -DB_FLOOR;
    well()
        .h_full()
        .w(px(TIME_W))
        .flex_none()
        .debug_selector(|| "reverb-display".into())
        .child(graph)
        .child(handle(
            px_at(pre),
            DISPLAY_H - 18.0,
            0xffffff,
            "reverb-handle-predelay",
            SpaceHandle::Predelay,
            span,
            cx,
        ))
        .when(!frozen, |d| {
            d.child(handle(
                px_at(floor_at),
                DISPLAY_H - 18.0,
                MIDDLE,
                "reverb-handle-decay",
                SpaceHandle::Decay,
                span,
                cx,
            ))
        })
        .child(label(
            px_at(pre) + 6.0,
            8.0,
            match r.predelay_beats {
                Some(b) => format!("Pre-delay {}", crate::mix::strip::note_name(b)),
                None => format!("Pre-delay {:.0} ms", pre * 1000.0),
            },
            TEXT_3,
        ))
        .child(label(
            w - 150.0,
            8.0,
            match frozen {
                true => "Frozen".to_string(),
                false => format!("Falls 60 dB in {decay:.2} s"),
            },
            TEXT_2,
        ))
        .children(bands.iter().enumerate().map(|(i, &(rt, c, name))| {
            label(
                w - 150.0,
                26.0 + i as f32 * 15.0,
                format!("{name} {rt:.2} s"),
                c,
            )
        }))
}

/// A point on a display to drag: a ring at `x`, `y` (its middle), over the picture.
pub(super) fn handle(
    x: f32,
    y: f32,
    color: u32,
    sel: &'static str,
    which: SpaceHandle,
    span: f32,
    cx: &mut Context<Workspace>,
) -> Div {
    div()
        .absolute()
        .left(px(x - 7.0))
        .top(px(y - 7.0))
        .size(px(14.))
        .rounded(px(7.))
        .bg(rgb(0x0b0b0e))
        .border_2()
        .border_color(rgb(color))
        .shadow_md()
        .cursor_pointer()
        .hover(|d| d.bg(rgb(color)))
        .debug_selector(move || sel.into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.begin_drag(
                    Drag::Space {
                        handle: which,
                        span,
                    },
                    cx,
                );
            }),
        )
}

/// How long `r` rings at `hz`, seconds: the lows as long as its bass says, the highs past its
/// damping a good deal shorter, the middle its decay; blended between.
fn decay_at(r: &Reverb, hz: f32) -> f32 {
    let low = 1.0 / (1.0 + (hz / 250.0).powi(2));
    let x = (hz / r.damping_hz.max(1.0)).powi(2);
    let high = x / (1.0 + x);
    let mid = (1.0 - low - high).max(0.0);
    r.decay_s * (low * r.bass + high * 0.3 + mid)
}

/// Seconds of decay up the frequency picture: 0.05 at the bottom, 20 at the top.
fn secs_y(t: f32, h: f32) -> f32 {
    (1.0 - (t.max(0.05) / 0.05).ln() / 400f32.ln()).clamp(0.0, 1.0) * h
}

fn y_secs(y: f32) -> f32 {
    0.05 * 400f32.powf(1.0 - y.clamp(0.0, 1.0))
}

/// The tail's spectrum as it plays, drawn faint across a picture `w` by `h` at `b`.
pub(super) fn spectrum_fill(
    window: &mut gpui::Window,
    b: Bounds<Pixels>,
    (spectrum, columns): &(Vec<f32>, Vec<f64>),
    dim: f32,
) {
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    if spectrum.is_empty() {
        return;
    }
    let mut area = PathBuilder::fill();
    area.move_to(point(b.origin.x, b.origin.y + px(h)));
    for (db, hz) in spectrum.iter().zip(columns) {
        let x = freq_x(*hz as f32) * w;
        let y = ((-db) / 80.0).clamp(0.0, 1.0) * h;
        area.line_to(point(b.origin.x + px(x), b.origin.y + px(y)));
    }
    area.line_to(point(b.origin.x + px(w), b.origin.y + px(h)));
    area.close();
    if let Ok(p) = area.build() {
        window.paint_path(p, alpha(0xffffff, 0.06 * dim));
    }
}

/// Frequencies across a picture: their lines, and their names along the foot.
pub(super) fn freq_grid(window: &mut gpui::Window, b: Bounds<Pixels>) {
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    for f in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
        window.paint_quad(fill(
            Bounds::new(
                point(b.origin.x + px((freq_x(f) * w).round()), b.origin.y),
                size(px(1.), px(h)),
            ),
            rgb(LINE_SOFT),
        ));
    }
}

/// How long the reverb rings across the frequencies, its lows' and highs' points to drag, its
/// cuts shaded at the edges (dragged by their own points), over what it's playing now.
fn reverb_freq(
    r: &Reverb,
    heard: (Vec<f32>, Vec<f64>),
    on: bool,
    plot: Rc<Cell<Bounds<Pixels>>>,
    cx: &mut Context<Workspace>,
) -> Div {
    let r = *r;
    let dim = if on { 1.0 } else { 0.35 };
    let graph = canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            plot.set(b);
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            freq_grid(window, b);
            spectrum_fill(window, b, &heard, dim);
            // Its cuts, shaded.
            let (lo, hi) = (freq_x(r.low_cut_hz) * w, freq_x(r.high_cut_hz) * w);
            for (x, ww) in [(0.0, lo), (hi, w - hi)] {
                window.paint_quad(fill(
                    Bounds::new(
                        point(b.origin.x + px(x), b.origin.y),
                        size(px(ww.max(0.0)), px(h)),
                    ),
                    alpha(0x000000, 0.45),
                ));
            }
            for x in [lo, hi] {
                window.paint_quad(fill(
                    Bounds::new(
                        point(b.origin.x + px(x.round()), b.origin.y),
                        size(px(1.), px(h)),
                    ),
                    alpha(0xffffff, 0.3 * dim),
                ));
            }
            // Its decay at each frequency: filled under, the line over.
            let n = 96;
            let at = |i: usize| {
                let x = i as f32 / (n - 1) as f32;
                let t = if r.freeze {
                    20.0
                } else {
                    decay_at(&r, x_freq(x))
                };
                point(b.origin.x + px(x * w), b.origin.y + px(secs_y(t, h)))
            };
            let mut area = PathBuilder::fill();
            area.move_to(point(b.origin.x, b.origin.y + px(h)));
            for i in 0..n {
                area.line_to(at(i));
            }
            area.line_to(point(b.origin.x + px(w), b.origin.y + px(h)));
            area.close();
            if let Ok(p) = area.build() {
                window.paint_path(p, alpha(MIDDLE, 0.14 * dim));
            }
            let mut line = PathBuilder::stroke(px(1.8));
            line.move_to(at(0));
            for i in 1..n {
                line.line_to(at(i));
            }
            if let Ok(p) = line.build() {
                window.paint_path(p, alpha(MIDDLE, 0.95 * dim));
            }
        },
    )
    .absolute()
    .size_full();
    let w = FREQ_W;
    let x = |hz: f32| freq_x(hz) * w;
    let y = |t: f32| secs_y(t, DISPLAY_H);
    well()
        .h_full()
        .w(px(FREQ_W))
        .flex_none()
        .debug_selector(|| "reverb-freq".into())
        .child(graph)
        .child(tag(8.0, 8.0, "Decay across the spectrum", TEXT_3))
        .child(handle(
            x(60.0),
            y(decay_at(&r, 60.0)),
            LOW,
            "reverb-handle-bass",
            SpaceHandle::Bass,
            0.0,
            cx,
        ))
        .child(handle(
            x(r.damping_hz),
            y(decay_at(&r, r.damping_hz)),
            HIGH,
            "reverb-handle-damping",
            SpaceHandle::Damping,
            0.0,
            cx,
        ))
        .child(handle(
            x(r.low_cut_hz),
            DISPLAY_H - 18.0,
            0xffffff,
            "reverb-handle-low-cut",
            SpaceHandle::LowCut,
            0.0,
            cx,
        ))
        .child(handle(
            x(r.high_cut_hz),
            DISPLAY_H - 18.0,
            0xffffff,
            "reverb-handle-high-cut",
            SpaceHandle::HighCut,
            0.0,
            cx,
        ))
        .child(tag(
            8.0,
            DISPLAY_H - 34.0,
            format!(
                "Lows ×{:.2} · darker past {}",
                r.bass,
                crate::mix::strip::reverb_text(ReverbField::Damping, &r)
            ),
            TEXT_3,
        ))
}

/// What comes round each time in the delay: its cuts' curve to drag, over what's playing now.
fn delay_freq(
    d: &Delay,
    heard: (Vec<f32>, Vec<f64>),
    on: bool,
    plot: Rc<Cell<Bounds<Pixels>>>,
    cx: &mut Context<Workspace>,
) -> Div {
    let d = *d;
    let dim = if on { 1.0 } else { 0.35 };
    let graph = canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            plot.set(b);
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            freq_grid(window, b);
            spectrum_fill(window, b, &heard, dim);
            // Each time round: a 12 dB high-pass and low-pass, drawn as each echo keeps it
            // (three times round, darker each).
            let n = 96;
            for (round, a) in [(1.0f32, 0.95f32), (2.0, 0.5), (3.0, 0.28)] {
                let mut line = PathBuilder::stroke(px(1.6));
                for i in 0..n {
                    let x = i as f32 / (n - 1) as f32;
                    let f = x_freq(x);
                    let hp = (f / d.low_cut_hz).powi(2) / (1.0 + (f / d.low_cut_hz).powi(2));
                    let lp = 1.0 / (1.0 + (f / d.high_cut_hz).powi(2));
                    let db = 10.0 * (hp * lp).max(1e-9).log10() * round;
                    let p = point(
                        b.origin.x + px(x * w),
                        b.origin.y + px(((-db) / 36.0).clamp(0.0, 1.0) * (h - 30.0) + 24.0),
                    );
                    match i {
                        0 => line.move_to(p),
                        _ => line.line_to(p),
                    }
                }
                if let Ok(p) = line.build() {
                    window.paint_path(p, alpha(MIDDLE, a * dim));
                }
            }
        },
    )
    .absolute()
    .size_full();
    let x = |hz: f32| freq_x(hz) * FREQ_W;
    well()
        .h_full()
        .w(px(FREQ_W))
        .flex_none()
        .debug_selector(|| "delay-freq".into())
        .child(graph)
        .child(tag(8.0, 8.0, "Each time round", TEXT_3))
        .child(handle(
            x(d.low_cut_hz),
            40.0,
            0xffffff,
            "delay-handle-low-cut",
            SpaceHandle::LowCut,
            0.0,
            cx,
        ))
        .child(handle(
            x(d.high_cut_hz),
            40.0,
            0xffffff,
            "delay-handle-high-cut",
            SpaceHandle::HighCut,
            0.0,
            cx,
        ))
}

/// The delay's echoes: when each lands and how loud, the left above, the right below.
fn delay_display(
    d: &Delay,
    bpm: f64,
    on: bool,
    plot: Rc<Cell<Bounds<Pixels>>>,
    cx: &mut Context<Workspace>,
) -> Div {
    let times = d.times_s(bpm);
    let beat = (60.0 / bpm.max(1.0)) as f32;
    // Its echoes, followed round: each spawns one on its side and one across.
    let fb = if d.freeze {
        1.0
    } else {
        d.feedback.clamp(0.0, 1.1).min(1.0)
    };
    let cross = match d.style {
        DelayStyle::PingPong => d.cross.max(0.5),
        _ => d.cross,
    };
    let span = (times[0].max(times[1]) * 8.0).clamp(1.0, 8.0);
    let mut echoes: Vec<(f32, usize, f32)> = Vec::new();
    let mut queue: Vec<(f32, usize, f32)> = match d.style {
        DelayStyle::PingPong => vec![(times[0], 0, 1.0)],
        _ => vec![(times[0], 0, 1.0), (times[1], 1, 1.0)],
    };
    while let Some((t, s, a)) = queue.pop() {
        if t > span || a < 0.001 || echoes.len() > 96 {
            continue;
        }
        echoes.push((t, s, a));
        let o = 1 - s;
        queue.push((t + times[s], s, a * fb * (1.0 - cross)));
        queue.push((t + times[o], o, a * fb * cross));
    }
    let synced = d.sync;
    let dim = if on { 1.0 } else { 0.35 };
    let count = echoes.len();
    let graph = canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            plot.set(b);
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let lane = h / 2.0;
            let x = |t: f32| (t / span).clamp(0.0, 1.0) * w;
            let quad =
                |window: &mut gpui::Window, x: f32, y: f32, ww: f32, hh: f32, c: gpui::Hsla| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        c,
                    ));
                };
            // The beats under it (bars brighter), or every 100 ms.
            let step = if synced { beat } else { 0.1 };
            let mut k = 1;
            while k as f32 * step < span {
                let bar = synced && k % 4 == 0;
                let c = if bar { LINE } else { LINE_SOFT };
                quad(
                    window,
                    x(k as f32 * step).round(),
                    0.0,
                    1.0,
                    h,
                    rgb(c).into(),
                );
                k += 1;
            }
            quad(window, 0.0, lane.round(), w, 1.0, rgb(LINE).into());
            for &(t, s, a) in &echoes {
                let tall = (1.0 - (20.0 * a.log10()) / DB_FLOOR).clamp(0.0, 1.0) * (lane - 14.0);
                let (top, c) = match s {
                    0 => (lane - tall - 2.0, MIDDLE),
                    _ => (lane + 2.0, LOW),
                };
                quad(window, x(t) - 1.5, top, 3.0, tall, alpha(c, 0.85 * dim));
            }
            quad(window, 0.0, 0.0, 2.0, h, alpha(0xffffff, 0.7 * dim));
        },
    )
    .absolute()
    .size_full();
    let text = |s: usize| match d.sync {
        true => format!(
            "{} · {:.0} ms",
            crate::mix::strip::note_name(d.beats[s]),
            times[s] * 1000.0
        ),
        false => format!("{:.0} ms", times[s] * 1000.0),
    };
    // The first echo each side (its time) and the next on the left (the feedback), to drag.
    let x_at = |t: f32| (t / span).clamp(0.0, 1.0) * TIME_W;
    let lane = DISPLAY_H / 2.0;
    let tall =
        |a: f32| (1.0 - (20.0 * a.max(1e-4).log10()) / DB_FLOOR).clamp(0.0, 1.0) * (lane - 14.0);
    let ping = d.style == DelayStyle::PingPong;
    let second = match ping {
        // Bouncing, the second is the right's.
        true => (times[0] + times[1], lane + 2.0 + tall(fb * cross)),
        false => (2.0 * times[0], lane - 2.0 - tall(fb * (1.0 - cross))),
    };
    well()
        .h_full()
        .w(px(TIME_W))
        .flex_none()
        .debug_selector(|| "delay-display".into())
        .child(graph)
        .child(handle(
            x_at(times[0]),
            lane - 2.0 - tall(1.0),
            MIDDLE,
            "delay-handle-left",
            SpaceHandle::Time(0),
            span,
            cx,
        ))
        .when(!ping, |el| {
            el.child(handle(
                x_at(times[1]),
                lane + 2.0 + tall(1.0),
                LOW,
                "delay-handle-right",
                SpaceHandle::Time(1),
                span,
                cx,
            ))
        })
        .when(!d.freeze, |el| {
            el.child(handle(
                x_at(second.0),
                second.1,
                0xffffff,
                "delay-handle-feedback",
                SpaceHandle::Feedback,
                span,
                cx,
            ))
        })
        .child(chip_tag(
            TIME_W - 190.0,
            DISPLAY_H / 2.0 - 26.0,
            format!("Left {}", text(0)),
            MIDDLE,
        ))
        .child(chip_tag(
            TIME_W - 190.0,
            DISPLAY_H - 26.0,
            format!("Right {}", text(1)),
            LOW,
        ))
        .child(tag(
            TIME_W - 190.0,
            8.0,
            match d.freeze {
                true => "Frozen: going round".to_string(),
                false => format!("{count} echoes over -60 dB"),
            },
            TEXT_3,
        ))
}
