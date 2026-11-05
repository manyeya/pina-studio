//! The compressor's, saturator's and limiter's windows, as the Channel EQ's: over everything,
//! moved by their title, one open at a time. The compressor's shows what goes in, what comes out
//! and what it takes off as it plays, and its curve; the saturator's, its bands over the
//! channel's sound, and the band picked's style, curve and knobs; the limiter's, its levels and
//! loudness as it plays, and its styles and knobs.

mod comp;
mod limit;
mod sat;
mod space;
mod tools;

pub use space::SpaceHandle;

use std::{cell::Cell, collections::VecDeque, rc::Rc};

use gpui::{
    Animation, AnimationExt, Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent,
    PathBuilder, Pixels, Stateful, anchored, canvas, deferred, div, fill, point, prelude::*, px,
    rgb, size,
};
use pina_engine::{
    dsp::{
        eq::{LANE_LEN, spectrum_db},
        limit::LimitMeter,
    },
    song::project::{Comp, CompStyle, Fx, Limiter, SatStyle, Saturator, Unit},
};

use crate::{
    mix::{
        eq::{band_color, bar_button, dial, icon_button, value_line},
        strip::{self, Builtin, Channel},
    },
    ui::theme::*,
    workspace::{CompField, Drag, SatField, SatKnob, Target, Workspace, pt},
};

const HEAD_H: f32 = 48.0;
/// How many frames of levels the compressor's display keeps.
const HISTORY: usize = 720;
/// The saturator's display: 20 Hz to 20 kHz across, in this many columns.
const COLUMNS: usize = 240;

pub struct FxView {
    pub ch: Channel,
    pub which: Builtin,
    /// Which of the channel's: its own (None), or one stacked, by its place among them.
    pub stacked: Option<usize>,
    pub at: (f32, f32),
    /// How big it's been made: never smaller than it opens.
    pub size: (f32, f32),
    /// The compressor's (or limiter's) levels, a frame each, oldest first: in and out (dBFS),
    /// how far it turned down (dB), and (the limiter's) the short-term loudness (LUFS).
    pub history: VecDeque<[f32; 4]>,
    /// The limiter's loudness as last read, and its meters' bars (in, reduction, out, short-term
    /// loudness), falling back slowly.
    pub meter: LimitMeter,
    pub bars: [f32; 4],
    /// The gain reduction since last frame, as the engine had it (no falling away).
    pub reduction: f32,
    /// The saturator's or multiband's band picked.
    pub band: usize,
    /// The channel's sound's spectrum (dB) at `columns` (Hz), for the saturator's display.
    pub spectrum: Vec<f32>,
    pub columns: Vec<f64>,
    buf: Box<[f32; LANE_LEN]>,
    written: usize,
    /// Where the saturator's display was drawn, for dragging its splits; and a second display's
    /// (the reverb's and delay's frequency pictures).
    pub plot: Rc<Cell<Bounds<Pixels>>>,
    pub plot2: Rc<Cell<Bounds<Pixels>>>,
    /// What a stacked tool just played, each side, for its vectorscope, and how much had.
    pub scope: [Vec<f32>; 2],
    scope_seen: usize,
    /// What the tool has to say for itself.
    pub reading: [f32; 3],
}

impl FxView {
    fn new(ch: Channel, which: Builtin, stacked: Option<usize>, at: (f32, f32)) -> Self {
        FxView {
            ch,
            which,
            stacked,
            at,
            size: window_size(which),
            history: VecDeque::with_capacity(HISTORY),
            meter: LimitMeter::default(),
            bars: [-120.0, 0.0, -120.0, -120.0],
            reduction: 0.0,
            band: 0,
            spectrum: vec![-120.0; COLUMNS],
            columns: (0..COLUMNS)
                .map(|i| 20.0 * 1000f64.powf(i as f64 / (COLUMNS - 1) as f64))
                .collect(),
            buf: Box::new([0.0; LANE_LEN]),
            written: 0,
            plot: Rc::new(Cell::new(Bounds::default())),
            plot2: Rc::new(Cell::new(Bounds::default())),
            scope: [Vec::new(), Vec::new()],
            scope_seen: 0,
            reading: [0.0; 3],
        }
    }
}

/// A window's size.
fn window_size(which: Builtin) -> (f32, f32) {
    match which {
        Builtin::Sat => (920.0, 572.0),
        Builtin::Limit => (1130.0, 610.0),
        Builtin::Tool(_) => (980.0, 560.0),
        _ => (960.0, 500.0),
    }
}

/// Across the saturator's display: where a frequency is, 0 to 1, and back.
pub(crate) fn freq_x(hz: f32) -> f32 {
    (hz.max(20.0) / 20.0).log10() / 3.0
}

pub(crate) fn x_freq(x: f32) -> f32 {
    20.0 * 1000f32.powf(x.clamp(0.0, 1.0))
}

/// A level in dB on a display `h` tall, 0 dB at the top and `floor` at the bottom.
fn level_y(db: f32, floor: f32, h: f32) -> f32 {
    (db / floor).clamp(0.0, 1.0) * h
}

impl Workspace {
    /// A channel's compressor or saturator in its window (one added, if it had none).
    pub fn open_fx(&mut self, ch: Channel, which: Builtin, cx: &mut Context<Self>) {
        self.open_fx_at(ch, which, None, cx);
    }

    /// One of a channel's built-ins in its window: its own (`stacked` None, one added if it had
    /// none), or one stacked on it.
    pub fn open_fx_at(
        &mut self,
        ch: Channel,
        which: Builtin,
        stacked: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        if which == Builtin::Eq {
            return self.open_eq_at(ch, stacked, cx);
        }
        if let Some(k) = stacked {
            let kind = self.more(ch).get(k).map(|u| u.kind());
            if kind != Some(which.fx()) {
                return;
            }
        }
        let missing = stacked.is_none()
            && match which {
                Builtin::Comp => self.strip_dsp(ch).1.is_none(),
                Builtin::Limit => self.limiter(ch).is_none(),
                _ => self.sat(ch).is_none(),
            };
        if missing {
            self.edit(cx, |ws| match which {
                Builtin::Comp => drop(ws.strip_dsp_mut(ch).1.insert(Comp::default())),
                Builtin::Limit => drop(ws.limiter_mut(ch).insert(Limiter::default())),
                _ => drop(ws.sat_mut(ch).insert(Saturator::default())),
            });
        }
        self.channel = ch;
        if let Channel::Track(i) = ch {
            self.track = i;
        }
        let (w, h) = window_size(which);
        let at = match &self.fx_view {
            Some(v) => v.at,
            None => (
                ((self.viewport_w - w) / 2.0).max(0.0),
                ((self.viewport_h - h) / 2.0).max(CONTROL_BAR_H),
            ),
        };
        self.fx_view = Some(FxView::new(ch, which, stacked, at));
        self.player.watch_fx_more(stacked);
        cx.notify();
    }

    pub fn close_fx(&mut self, cx: &mut Context<Self>) {
        self.fx_view = None;
        self.player.watch_fx_more(None);
        cx.notify();
    }

    /// A knob's target in the window: on the built-in it shows, its channel's own or one
    /// stacked.
    fn fx_target(&self, t: Target) -> Target {
        use crate::workspace::Knob;
        match (self.fx_stacked(), t) {
            (Some(k), Target::Comp(ch, f)) => Target::Stacked(ch, k, Knob::Comp(f)),
            (Some(k), Target::Sat(ch, f)) => Target::Stacked(ch, k, Knob::Sat(f)),
            (Some(k), Target::Limit(ch, f)) => Target::Stacked(ch, k, Knob::Limit(f)),
            (_, t) => t,
        }
    }

    /// Whether the saturator's window wants every channel's sound kept.
    pub(crate) fn fx_hears_lanes(&self) -> bool {
        self.fx_view
            .as_ref()
            .is_some_and(|v| matches!(v.which, Builtin::Sat | Builtin::Tool(_)))
    }

    /// Each frame the window's open: the compressor's or limiter's levels moved on (while
    /// there's sound), or the saturator's spectrum.
    pub fn fx_frame(&mut self) {
        let Some(v) = self.fx_view.as_ref() else {
            return;
        };
        let (ch, which) = (v.ch, v.which);
        let stacked = v.stacked;
        let gone = match ch {
            Channel::Track(i) => i >= self.session.project.tracks.len(),
            Channel::Aux(i) => i >= self.session.project.auxes.len(),
            Channel::Master => false,
        } || stacked
            .is_some_and(|k| self.more(ch).get(k).map(|u| u.kind()) != Some(which.fx()));
        if gone {
            self.fx_view = None;
            return;
        }
        let id = self.channel_id(ch);
        let playing = self.player.playing();
        let rate = self.session.project.sample_rate as f64;
        let db = |x: f32| (20.0 * x.max(1e-6).log10()).max(-90.0);
        if let (Builtin::Tool(_), Some(v)) = (which, self.fx_view.as_mut()) {
            let seen = self.player.scope(&mut v.scope, v.scope_seen);
            // Stopped: it dies away rather than sticking.
            if seen == v.scope_seen {
                v.scope.iter_mut().flatten().for_each(|x| *x *= 0.92);
            }
            v.scope_seen = seen;
        }
        // What's heard going in and coming out, for those that draw it.
        let levels = matches!(
            which,
            Builtin::Comp | Builtin::Tool(Fx::Shaper | Fx::Gate | Fx::DeEsser | Fx::Pitch)
        );
        if let Builtin::Tool(Fx::Multiband | Fx::Tune | Fx::TapeStop) = which {
            let r = self.player.tool_reading();
            if let Some(v) = self.fx_view.as_mut() {
                // The multiband's meters fall back as a needle does; the rest say what is.
                v.reading = match which {
                    Builtin::Tool(Fx::Multiband) => {
                        std::array::from_fn(|k| r[k].max(v.reading[k] * 0.9))
                    }
                    _ => r,
                };
                if which == Builtin::Tool(Fx::Tune) && playing {
                    if v.history.len() == HISTORY {
                        v.history.pop_front();
                    }
                    v.history.push_back([r[0], r[1], 0.0, 0.0]);
                }
            }
        }
        if levels {
            let [i, o] = self.player.comp_levels(id);
            let Some(v) = self.fx_view.as_mut() else {
                return;
            };
            if playing || i > 1e-4 {
                if v.history.len() == HISTORY {
                    v.history.pop_front();
                }
                v.history.push_back([db(i), db(o), v.reduction, 0.0]);
            }
            if which != Builtin::Tool(Fx::DeEsser) {
                return;
            }
        }
        if which == Builtin::Limit {
            let m = self.player.limit_meter(id);
            let Some(v) = self.fx_view.as_mut() else {
                return;
            };
            let short = match m.short.is_finite() {
                true => m.short.max(-90.0),
                false => -90.0,
            };
            if playing || m.input > 1e-4 {
                if v.history.len() == HISTORY {
                    v.history.pop_front();
                }
                v.history
                    .push_back([db(m.input), db(m.output), m.reduction, short]);
            }
            // Up at once, back down slowly, as a meter's needle.
            let now = [db(m.input), m.reduction, db(m.output), short];
            for (k, (b, n)) in v.bars.iter_mut().zip(now).enumerate() {
                let (fall, floor) = if k == 1 { (0.25, 0.0) } else { (0.5, -120.0) };
                *b = n.max(*b - fall).max(floor);
            }
            // Kept until the limiter's been heard.
            if m.short.is_finite() {
                v.meter = m;
            }
            return;
        }
        let Some(v) = self.fx_view.as_mut() else {
            return;
        };
        let written = self.player.lane_sound(id, &mut v.buf);
        let fresh = written != v.written && written > 0;
        v.written = written;
        let now = match fresh {
            true => spectrum_db(&v.buf[..], rate, &v.columns),
            false => vec![-120.0; COLUMNS],
        };
        for (s, n) in v.spectrum.iter_mut().zip(now) {
            // Up quickly, down slowly, as the EQ's analyzer.
            *s = if n > *s {
                *s + (n - *s) * 0.6
            } else {
                (*s - 0.8).max(n)
            };
        }
    }

    /// The window moved by its title, or made bigger by its corner.
    pub fn drag_fx_window(
        &mut self,
        from: gpui::Point<f32>,
        start: (f32, f32),
        resize: bool,
        p: gpui::Point<f32>,
    ) {
        let (vw, vh) = (self.viewport_w, self.viewport_h);
        let Some(v) = self.fx_view.as_mut() else {
            return;
        };
        let (dx, dy) = (p.x - from.x, p.y - from.y);
        if resize {
            let (w, h) = window_size(v.which);
            v.size = (
                (start.0 + dx).clamp(w, vw.max(w)),
                (start.1 + dy).clamp(h, vh.max(h)),
            );
            return;
        }
        v.at = (
            (start.0 + dx).clamp(-v.size.0 + 80.0, vw - 80.0),
            (start.1 + dy).clamp(0.0, vh - HEAD_H),
        );
    }

    /// The compressor's or saturator's window, if one's open.
    pub fn fx_window(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let v = self.fx_view.as_ref()?;
        let (ch, which, at, stacked, (w, h)) = (v.ch, v.which, v.at, v.stacked, v.size);
        let body = match (which, stacked) {
            (Builtin::Comp, _) => self.comp_window(ch, self.built_in(ch, stacked)?, cx),
            (Builtin::Limit, None) => self.limit_window(ch, self.limiter_heard(ch)?, cx),
            (Builtin::Limit, _) => self.limit_window(ch, self.built_in(ch, stacked)?, cx),
            (Builtin::Tool(Fx::Reverb), _) => {
                self.reverb_window(ch, self.built_in(ch, stacked)?, cx)
            }
            (Builtin::Tool(Fx::Delay), _) => self.delay_window(ch, self.built_in(ch, stacked)?, cx),
            (Builtin::Tool(_), _) => self.tool_window(ch, self.tool_heard(ch, stacked?)?, cx),
            _ => self.sat_window(ch, self.built_in(ch, stacked)?, cx),
        };
        let grip = resize_grip()
            .debug_selector(|| "fx-resize".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let Some(start) = ws.fx_view.as_ref().map(|v| v.size) else {
                        return;
                    };
                    ws.begin_drag(
                        Drag::FxWindow {
                            from: pt(e.position),
                            start,
                            resize: true,
                        },
                        cx,
                    );
                }),
            );
        let frame = div()
            .id("fx-window")
            .occlude()
            .relative()
            .w(px(w))
            .h(px(h))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .overflow_hidden()
            .bg(rgb(0x101115))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .children(body)
            .child(grip);
        Some(
            deferred(
                anchored()
                    .position(point(px(at.0), px(at.1)))
                    .snap_to_window()
                    .child(frame),
            )
            .with_priority(1),
        )
    }

    /// Along the top: on/off, the name, whose it is, then `middle` and `right`, and close. Drag
    /// it to move the window.
    #[allow(clippy::too_many_arguments)]
    fn fx_head(
        &self,
        title: &'static str,
        ch: Channel,
        which: Builtin,
        on: bool,
        middle: impl IntoElement,
        right: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (name, color) = self.channel_look(ch);
        div()
            .id("fx-head")
            .h(px(HEAD_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .bg(rgb(0x14151a))
            .border_b_1()
            .border_color(rgb(LINE))
            .cursor(gpui::CursorStyle::OpenHand)
            .debug_selector(|| "fx-title".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    let Some(start) = ws.fx_view.as_ref().map(|v| v.at) else {
                        return;
                    };
                    ws.begin_drag(
                        Drag::FxWindow {
                            from: pt(e.position),
                            start,
                            resize: false,
                        },
                        cx,
                    );
                }),
            )
            .child(switch(on).debug_selector(|| "fx-on".into()).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.edit(cx, |ws| match (ws.fx_stacked(), which) {
                        (Some(k), _) => {
                            if let Some(u) = ws.chain_mut(ch).more.get_mut(k) {
                                *u.bypass_mut() ^= true;
                            }
                        }
                        (None, Builtin::Comp) => {
                            if let Some(c) = ws.fx_unit_mut::<Comp>(ch) {
                                c.bypass ^= true;
                            }
                        }
                        (None, Builtin::Limit) => {
                            if let Some(l) = ws.fx_unit_mut::<Limiter>(ch) {
                                l.bypass ^= true;
                            }
                        }
                        (None, _) => {
                            if let Some(s) = ws.fx_unit_mut::<Saturator>(ch) {
                                s.bypass ^= true;
                            }
                        }
                    });
                }),
            ))
            .child(
                div()
                    .text_size(px(13.5))
                    .font_weight(FontWeight::BOLD)
                    .child(match self.fx_stacked().and_then(|k| self.stacked(ch, k)) {
                        Some((_, name)) => name,
                        None => title.to_string(),
                    }),
            )
            .child(
                div()
                    .h(px(26.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(0x1c1d23))
                    .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(color)))
                    .child(
                        div()
                            .max_w(px(160.))
                            .text_size(px(11.5))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    ),
            )
            .child(middle)
            .child(div().flex_1())
            .child(right)
            .child(icon_button(Icon::Close, "fx-close").on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.close_fx(cx);
                }),
            ))
    }

    /// A knob of the window: its name over it, the dial, its value under it. Drag it up and
    /// down; ⌘-click puts it back to how it comes.
    #[allow(clippy::too_many_arguments)]
    fn fx_dial(
        &self,
        target: Target,
        name: &'static str,
        text: String,
        dial_size: f32,
        color: u32,
        bipolar: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let target = self.fx_target(target);
        let value = self.target(target);
        let shown = if bipolar { value * 2.0 - 1.0 } else { value };
        let reset = match target {
            Target::Comp(_, f) => strip::comp_pos(f, &Comp::default()),
            Target::Sat(_, SatField::Band(_, k)) => {
                strip::sat_pos(SatField::Band(0, k), &Saturator::default())
            }
            Target::Sat(_, f) => strip::sat_pos(f, &Saturator::default()),
            Target::Limit(_, f) => strip::limit_pos(f, &Limiter::default()),
            Target::Stacked(_, _, knob) => match knob {
                crate::workspace::Knob::Comp(f) => strip::comp_pos(f, &Comp::default()),
                crate::workspace::Knob::Sat(SatField::Band(_, k)) => {
                    strip::sat_pos(SatField::Band(0, k), &Saturator::default())
                }
                crate::workspace::Knob::Sat(f) => strip::sat_pos(f, &Saturator::default()),
                crate::workspace::Knob::Limit(f) => strip::limit_pos(f, &Limiter::default()),
                crate::workspace::Knob::Reverb(f) => {
                    let r = self
                        .built_in::<pina_engine::song::project::Reverb>(
                            self.fx_view.as_ref().map_or(Channel::Master, |v| v.ch),
                            self.fx_stacked(),
                        )
                        .unwrap_or_default();
                    // Back to how its style usually is.
                    strip::reverb_pos(f, &r.style.reverb())
                }
                crate::workspace::Knob::Param(i) => self
                    .fx_view
                    .as_ref()
                    .and_then(|v| self.more(v.ch).get(v.stacked?))
                    .and_then(|u| {
                        let fresh = strip::fresh_tool(u.kind(), false)?;
                        Some((strip::knobs(u.kind()).get(i)?.pos)(&fresh))
                    })
                    .unwrap_or(value),
                crate::workspace::Knob::Delay(f) => {
                    let d = self
                        .built_in::<pina_engine::song::project::Delay>(
                            self.fx_view.as_ref().map_or(Channel::Master, |v| v.ch),
                            self.fx_stacked(),
                        )
                        .unwrap_or_default();
                    strip::delay_pos(f, &d.style.delay())
                }
            },
            _ => value,
        };
        let sel = name.to_lowercase().replace(' ', "-");
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .child(name),
            )
            .child(
                div()
                    .cursor_ns_resize()
                    .debug_selector(move || format!("fx-knob-{sel}"))
                    .child(self.modded(
                        self.mod_key(target),
                        dial_size,
                        dial(
                            shown,
                            dial_size,
                            (dial_size / 9.0).clamp(4.0, 7.0),
                            color,
                            bipolar,
                        ),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.modifiers.platform {
                                ws.edit(cx, |ws| ws.set_target(target, reset));
                                return;
                            }
                            let y = pt(e.position).y;
                            ws.begin_drag(Drag::Knob { target, y, value }, cx);
                        }),
                    ),
            )
            .child(value_line(
                text,
                if dial_size >= 56.0 { 16.0 } else { 13.0 },
                None,
            ))
    }
}

/// A row of choices: a tray of buttons, the one `now` lit.
fn choice_tray<T: Copy + PartialEq + 'static>(
    sel: &'static str,
    options: &[(&'static str, T)],
    now: T,
    h: f32,
    cx: &mut Context<Workspace>,
    pick: impl Fn(&mut Workspace, T) + Copy + 'static,
) -> Div {
    tray()
        .p(px(2.))
        .rounded(px(8.))
        .children(options.iter().map(|&(name, v)| {
            seg(name, v == now, h)
                .debug_selector(move || format!("{sel}-{}", name.to_lowercase().replace(' ', "-")))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| pick(ws, v));
                    }),
                )
        }))
}

/// A dashed line across, `w` long at `y`.
fn dashed(window: &mut gpui::Window, b: Bounds<Pixels>, y: f32, w: f32, color: gpui::Hsla) {
    let mut x = 0.0;
    while x < w {
        window.paint_quad(fill(
            Bounds::new(
                point(b.origin.x + px(x), b.origin.y + px(y.round())),
                size(px(4f32.min(w - x)), px(1.)),
            ),
            color,
        ));
        x += 7.0;
    }
}

/// A small label laid over a display, at `x`, `y`.
fn tag(x: f32, y: f32, text: impl Into<gpui::SharedString>, color: u32) -> Div {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .font_family(MONO)
        .text_size(px(9.5))
        .text_color(rgb(color))
        .whitespace_nowrap()
        .child(text.into())
}

/// A display's well: dark, rounded, its contents laid over each other.
fn well() -> Div {
    div()
        .relative()
        .overflow_hidden()
        .rounded(px(10.))
        .bg(rgb(0x0b0b0e))
        .border_1()
        .border_color(rgb(LINE_SOFT))
}

/// A thing breathing while it waits.
fn breathing(el: Div, id: &'static str) -> gpui::AnyElement {
    el.with_animation(
        id,
        Animation::new(std::time::Duration::from_millis(1100))
            .repeat()
            .with_easing(gpui::pulsating_between(0.45, 1.0)),
        |d, t| d.opacity(t),
    )
    .into_any_element()
}
