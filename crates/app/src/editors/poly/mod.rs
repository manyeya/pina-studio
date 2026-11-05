//! The synth's window. Sound runs left to right: its sources, the one picked open in the middle,
//! the filters, and what comes out; what moves it sits under them, its keys along the bottom.
//!
//! It's deep in the rest of the app: every knob is the same knob as anywhere (its automation, its
//! modulators' rings and a modulator dropped on it all work), its sources can be dropped on an
//! effect's knob as well as its own, and its Effects page is the track's chain, not a second one.
//! Colour means one thing here: what's moving a knob. The sound itself is white.

mod browser;
mod controls;
mod effects;
mod filters;
mod head;
mod keys;
mod matrix;
mod modulation;
mod output;
mod sources;

pub(crate) use controls::PolyAmount;
pub(crate) use matrix::source_name;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui::{
    Bounds, Context, Div, Entity, MouseButton, MouseDownEvent, PathBuilder, Pixels, Subscription,
    anchored, canvas, deferred, div, point, prelude::*, px, rgb,
};
use pina_engine::{
    dsp::poly::{LIVE_SOURCES, POLY_LIVE},
    song::project::*,
};

use crate::{
    ui::{
        menu::{Menu, MenuAction},
        text::TextInput,
        theme::*,
    },
    workspace::{Drag, Target, Workspace, pt},
};

pub const WINDOW_W: f32 = 1380.0;
pub const WINDOW_H: f32 = 860.0;
/// The smallest it's made: its columns, and room for the wave between them.
const MIN_W: f32 = 1180.0;
const MIN_H: f32 = 720.0;
const HEAD_H: f32 = 48.0;
const MOD_H: f32 = 214.0;
const KEYS_H: f32 = 100.0;
const SOURCES_W: f32 = 256.0;
const FILTERS_W: f32 = 300.0;
const OUTPUT_W: f32 = 184.0;

/// What marks a drag on LFO `n`'s picture: this and `n`.
const DRAW_LFO: u8 = 10;
/// What marks a drag on the envelope's picture: its attack, decay and release points.
const ENV_POINTS: [u8; 3] = [2, 3, 4];

/// Where each long slider was drawn, by its parameter.
pub type Spots = Rc<RefCell<Vec<(usize, Bounds<Pixels>)>>>;

/// Which page of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolyPage {
    Synth,
    Matrix,
    Effects,
}

/// Which routes the matrix shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RouteKind {
    #[default]
    All,
    Env,
    Lfo,
    Macro,
    Playing,
}

impl RouteKind {
    fn has(self, s: PolySource) -> bool {
        match self {
            RouteKind::All => true,
            RouteKind::Env => matches!(s, PolySource::Env(_)),
            RouteKind::Lfo => matches!(s, PolySource::Lfo(_)),
            RouteKind::Macro => matches!(s, PolySource::Macro(_)),
            RouteKind::Playing => !matches!(
                s,
                PolySource::Env(_) | PolySource::Lfo(_) | PolySource::Macro(_)
            ),
        }
    }
}

pub struct PolyView {
    pub track: usize,
    pub at: (f32, f32),
    /// How big it's been made, if it has.
    pub size: Option<(f32, f32)>,
    pub page: PolyPage,
    /// The source open in the middle: an oscillator (0 to 2), the sub (3), the noise (4).
    pub picked: usize,
    /// The filter whose settings show.
    pub filter: usize,
    /// The modulation source in hand, and the one pointed at: what it moves lights up.
    pub source: PolySource,
    pub pointed: Option<PolySource>,
    /// Where a source's cord starts while it's dragged to a knob.
    pub cord: Option<gpui::Point<f32>>,
    /// Where the curve, the shape and the keys were drawn, for the pointer; the long sliders.
    pub curve: Rc<Cell<Bounds<Pixels>>>,
    pub plot: Rc<Cell<Bounds<Pixels>>>,
    pub keys: Rc<Cell<Bounds<Pixels>>>,
    pub sliders: Spots,
    /// Notes down from the window (its keys, typing, the demo), and the one under the pointer.
    pub held: Vec<u8>,
    pub finger: Option<u8>,
    /// The wheels, as held.
    pub wheel: f32,
    pub bend: f32,
    /// The sounds' drawer, its search and the part it shows; the tables' list and its search.
    pub browsing: bool,
    pub part: &'static str,
    pub find: Option<Entity<TextInput>>,
    pub query: String,
    pub tables: Option<usize>,
    pub table_find: Option<Entity<TextInput>>,
    pub table_query: String,
    pub routes: RouteKind,
    /// The demo playing, by its turn: a new turn stops the old.
    pub demo: Option<u64>,
    turns: u64,
    /// Musical typing was on before the window turned it on.
    typing_was: bool,
    /// What the engine says: each source now, what came out, and that as a spectrum (dB).
    pub live: [f32; POLY_LIVE],
    pub scope: Vec<f32>,
    heard: usize,
    pub spectrum: Vec<f32>,
    _typed: Vec<Subscription>,
}

/// Something picked in the window.
#[derive(Clone, Debug, PartialEq)]
pub enum PolyOp {
    Patch(&'static str),
    Table(usize, &'static str),
    Warp(usize, usize, Warp),
    Filter(usize, PolyFilter),
    LfoWave(usize, LfoWave),
    LfoBeats(usize, f32),
    RouteSource(usize, PolySource),
    RouteBy(usize, Option<PolySource>),
    RouteDest(usize, PolyDest),
    AddRoute,
    RemoveRoute(usize),
    ArpMode(ArpMode),
    ArpBeats(f32),
    /// A parameter back to its default; given a lane on the track to automate it.
    Reset(usize),
    Automate(usize),
    /// The eight macros every patch has now, given to a synth made before.
    GiveMacros,
}

/// A source's colour, wherever it shows: its chip, its rings, its routes.
pub fn source_color(s: PolySource) -> u32 {
    match s {
        PolySource::Env(_) => AMBER,
        PolySource::Lfo(_) => SKY,
        PolySource::Macro(_) => VIOLET,
        _ => PINK,
    }
}

/// A source's short name, on its chip.
pub fn source_short(s: PolySource) -> String {
    match s {
        PolySource::Env(i) => format!("E{}", i + 1),
        PolySource::Lfo(i) => format!("L{}", i + 1),
        PolySource::Macro(i) => format!("M{}", i + 1),
        PolySource::Velocity => "Vel".into(),
        PolySource::Note => "Note".into(),
        PolySource::Wheel => "Wheel".into(),
        PolySource::Pressure => "Press".into(),
        PolySource::Bend => "Bend".into(),
        PolySource::Random => "Rand".into(),
        PolySource::Slide => "Slide".into(),
    }
}

impl Workspace {
    /// The synth on track `t`.
    pub(crate) fn poly(&self, t: usize) -> Option<&Poly> {
        match self.session.project.tracks.get(t)?.instrument.as_ref()? {
            Instrument::Poly(p) => Some(p),
            _ => None,
        }
    }

    pub(crate) fn poly_mut(&mut self, t: usize) -> Option<&mut Poly> {
        match self
            .session
            .project
            .tracks
            .get_mut(t)?
            .instrument
            .as_mut()?
        {
            Instrument::Poly(p) => Some(p),
            _ => None,
        }
    }

    /// The window's size: as it's been made, else as large as it opens (or the app's window
    /// less a margin), never under what its columns need.
    pub(crate) fn poly_size(&self) -> (f32, f32) {
        let (w, h) = self
            .poly_view
            .as_ref()
            .and_then(|v| v.size)
            .unwrap_or((WINDOW_W, WINDOW_H));
        (
            w.min(self.viewport_w - 16.0).max(MIN_W),
            h.min(self.viewport_h - 16.0).max(MIN_H),
        )
    }

    pub fn open_poly(&mut self, t: usize, cx: &mut Context<Self>) {
        if self.poly(t).is_none() {
            return;
        }
        if self.poly_view.as_ref().is_some_and(|v| v.track != t) {
            self.poly_all_off();
        }
        let (w, h) = self.poly_size();
        let at = (
            ((self.viewport_w - w) / 2.0).max(0.0),
            ((self.viewport_h - h) / 2.0).max(8.0),
        );
        let typing_was = match &self.poly_view {
            Some(v) => v.typing_was,
            None => self.typing.is_some(),
        };
        // A synth window plays from the computer's keys as well as its own.
        self.typing.get_or_insert(0);
        let held = self
            .poly_view
            .take()
            .map(|v| (v.at, v.size, v.page, v.picked, v.filter, v.source));
        let (at, size, page, picked, filter, source) =
            held.unwrap_or((at, None, PolyPage::Synth, 0, 0, PolySource::Env(1)));
        let find = cx.new(|cx| TextInput::search("Search sounds", cx));
        let table_find = cx.new(|cx| TextInput::search("Search tables", cx));
        let typed = vec![
            cx.observe(&find, |ws, input, cx| {
                let text = input.read(cx).text().trim().to_lowercase();
                if let Some(v) = ws.poly_view.as_mut().filter(|v| v.query != text) {
                    v.query = text;
                    cx.notify();
                }
            }),
            cx.observe(&table_find, |ws, input, cx| {
                let text = input.read(cx).text().trim().to_lowercase();
                if let Some(v) = ws.poly_view.as_mut().filter(|v| v.table_query != text) {
                    v.table_query = text;
                    cx.notify();
                }
            }),
        ];
        self.poly_view = Some(PolyView {
            track: t,
            at,
            size,
            page,
            picked,
            filter,
            source,
            pointed: None,
            cord: None,
            curve: Default::default(),
            plot: Default::default(),
            keys: Default::default(),
            sliders: Default::default(),
            held: Vec::new(),
            finger: None,
            wheel: 0.0,
            bend: 0.0,
            browsing: false,
            part: "all",
            find: Some(find),
            query: String::new(),
            tables: None,
            table_find: Some(table_find),
            table_query: String::new(),
            routes: RouteKind::All,
            demo: None,
            turns: 0,
            typing_was,
            live: [0.0; POLY_LIVE],
            scope: Vec::new(),
            heard: 0,
            spectrum: Vec::new(),
            _typed: typed,
        });
        cx.notify();
    }

    /// Closes the window: what it was playing let go, typing as it was before.
    pub fn close_poly(&mut self, cx: &mut Context<Self>) {
        self.poly_all_off();
        if let Some(v) = self.poly_view.take()
            && !v.typing_was
        {
            self.typing = None;
        }
        cx.notify();
    }

    /// The window moved by its title, or resized by its corner.
    pub fn drag_poly_window(
        &mut self,
        from: gpui::Point<f32>,
        start: (f32, f32),
        resize: bool,
        p: gpui::Point<f32>,
    ) {
        let (w, _) = self.poly_size();
        let (vw, vh) = (self.viewport_w, self.viewport_h);
        if resize {
            if let Some(v) = self.poly_view.as_mut() {
                v.size = Some((
                    (start.0 + p.x - from.x).clamp(MIN_W, (vw - 16.0).max(MIN_W)),
                    (start.1 + p.y - from.y).clamp(MIN_H, (vh - 16.0).max(MIN_H)),
                ));
            }
            return;
        }
        if let Some(v) = self.poly_view.as_mut() {
            v.at = (
                (start.0 + p.x - from.x).clamp(-w + 80.0, vw - 80.0),
                (start.1 + p.y - from.y).clamp(0.0, vh - HEAD_H),
            );
        }
    }

    /// The engine's news of the synth whose window is open: its sources, what it played and the
    /// spectrum of that. Whether anything's moving, so the window keeps drawing.
    pub(crate) fn poly_frame(&mut self) -> bool {
        let id = self
            .poly_view
            .as_ref()
            .and_then(|v| self.session.project.tracks.get(v.track))
            .map(|t| t.id);
        self.player.watch_poly(id);
        let rate = self.session.project.sample_rate as f64;
        let Some(v) = self.poly_view.as_mut() else {
            return false;
        };
        // Tests draw what they're given.
        if !cfg!(test) {
            v.live = self.player.poly_live();
            let seen = self.player.poly_scope(&mut v.scope, v.heard);
            if seen != v.heard && v.page == PolyPage::Synth {
                v.spectrum = spectrum_of(&v.scope, rate, &v.spectrum);
            }
            v.heard = seen;
        }
        let loud = v.scope.iter().any(|s| s.abs() > 1e-4);
        v.live[POLY_LIVE - 1] > 0.0 || loud || v.demo.is_some() || v.wheel != 0.0
    }

    /// Where a source is now (0 to 1, -1 for the two-way ones): a macro where it's turned, the
    /// rest as the engine last said, if this track's window is open.
    pub(crate) fn poly_now(&self, t: usize, s: PolySource) -> Option<f32> {
        if let PolySource::Macro(m) = s {
            return Some(self.poly_at(t, macro_param(m as usize)));
        }
        let v = self.poly_view.as_ref().filter(|v| v.track == t)?;
        LIVE_SOURCES.iter().position(|x| *x == s).map(|k| v.live[k])
    }

    /// A parameter's knob position as it plays.
    pub(crate) fn poly_at(&self, t: usize, i: usize) -> f32 {
        self.target(Target::Poly { track: t, param: i })
    }

    /// A parameter's knob position with what's moving it now.
    pub(crate) fn poly_moving(&self, t: usize, i: usize) -> f32 {
        let base = self.poly_at(t, i);
        let Some(p) = self.poly(t) else { return base };
        let moved: f32 = p
            .routes
            .iter()
            .filter(|r| r.dest == PolyDest::Param(i as u16))
            .map(|r| r.depth * self.poly_now(t, r.source).unwrap_or(0.0))
            .sum();
        (base + moved).clamp(0.0, 1.0)
    }

    /// A choice made in the window.
    pub fn poly_op(&mut self, t: usize, op: PolyOp, cx: &mut Context<Self>) {
        if let PolyOp::Automate(i) = op {
            self.show_lane(t, Param::Poly(i as u16));
            self.status = Some(format!(
                "{} has a lane under its track now",
                POLY_PARAMS[i].name
            ));
            cx.notify();
            return;
        }
        self.edit(cx, |ws| {
            let Some(p) = ws.poly_mut(t) else { return };
            match op {
                PolyOp::Patch(name) => {
                    if let Some(new) = poly_patch(name) {
                        *p = new;
                    }
                }
                PolyOp::Table(o, name) => p.tables[o] = name.to_string(),
                PolyOp::Warp(o, k, w) => p.warps[o][k] = w,
                PolyOp::Filter(i, kind) => p.filters[i] = kind,
                PolyOp::LfoWave(i, w) => p.lfos[i].wave = w,
                PolyOp::LfoBeats(i, b) => p.lfos[i].beats = b,
                PolyOp::RouteSource(i, s) => {
                    if let Some(r) = p.routes.get_mut(i) {
                        r.source = s;
                    }
                }
                PolyOp::RouteBy(i, s) => {
                    if let Some(r) = p.routes.get_mut(i) {
                        r.by = s;
                    }
                }
                PolyOp::RouteDest(i, d) => {
                    if let Some(r) = p.routes.get_mut(i) {
                        r.dest = d;
                    }
                }
                PolyOp::AddRoute if p.routes.len() < MAX_POLY_ROUTES => p.routes.push(PolyRoute {
                    source: PolySource::Env(1),
                    dest: PolyDest::Param(filter_param(0, FilterField::Cutoff) as u16),
                    depth: 0.3,
                    by: None,
                }),
                PolyOp::AddRoute => {}
                PolyOp::RemoveRoute(i) => {
                    if i < p.routes.len() {
                        p.routes.remove(i);
                    }
                }
                PolyOp::ArpMode(m) => p.arp.mode = m,
                PolyOp::ArpBeats(b) => p.arp.beats = b,
                PolyOp::Reset(i) => p.params[i] = POLY_PARAMS[i].default,
                PolyOp::GiveMacros => give_macros(p),
                PolyOp::Automate(_) => {}
            }
        });
    }

    /// A source dropped on a knob: routed to it, or to an effect's on the track. Once: dropped on
    /// a knob it already moves, it's left as it was.
    pub fn drop_poly_source(
        &mut self,
        t: usize,
        source: PolySource,
        at: gpui::Point<f32>,
        cx: &mut Context<Self>,
    ) {
        use pina_engine::dsp::modulate::ModTarget;
        let at = point(px(at.x), px(at.y));
        let hit = self
            .mod_targets
            .borrow()
            .iter()
            .rev()
            .find(|(tt, _, b)| *tt == t && b.contains(&at))
            .map(|&(_, mt, _)| mt);
        let dest = match hit {
            Some(ModTarget::Poly(i)) => PolyDest::Param(i),
            Some(ModTarget::Tool(k, i)) => PolyDest::Effect(k, i),
            _ => return,
        };
        if let Some(v) = self.poly_view.as_mut() {
            v.source = source;
        }
        self.edit(cx, |ws| {
            if let Some(p) = ws.poly_mut(t)
                && p.routes.len() < MAX_POLY_ROUTES
                && !p
                    .routes
                    .iter()
                    .any(|r| r.source == source && r.dest == dest)
            {
                p.routes.push(PolyRoute {
                    source,
                    dest,
                    depth: 0.3,
                    by: None,
                });
            }
        });
    }

    /// The pointer on a picture: filter `id`'s curve (0 or 1), the envelope's points, an LFO's
    /// wave drawn.
    pub fn drag_poly_point(&mut self, id: u8, p: gpui::Point<f32>) {
        let Some(v) = self.poly_view.as_ref() else {
            return;
        };
        let (t, source) = (v.track, v.source);
        let on = |b: Bounds<Pixels>| {
            (
                ((p.x - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0),
                ((p.y - f32::from(b.origin.y)) / f32::from(b.size.height).max(1.0)).clamp(0.0, 1.0),
            )
        };
        let (curve, shaped) = (on(v.curve.get()), on(v.plot.get()));
        if id >= DRAW_LFO {
            return self.draw_lfo(
                t,
                (id - DRAW_LFO) as usize,
                shaped.0,
                1.0 - (1.0 - shaped.1 - 0.04) / 0.92,
            );
        }
        let Some(q) = self.poly_mut(t) else { return };
        if (id as usize) < FILTERS {
            let (x, y) = curve;
            let i = id as usize;
            q.params[filter_param(i, FilterField::Cutoff)] = POLY_PARAMS
                [filter_param(i, FilterField::Cutoff)]
            .scale
            .value(x);
            q.params[filter_param(i, FilterField::Resonance)] = 1.0 - y;
            return;
        }
        let PolySource::Env(e) = source else { return };
        let e = e as usize;
        let (x, y) = shaped;
        let time = |f| POLY_PARAMS[env_param(e, f)].scale;
        match id {
            2 => {
                q.params[env_param(e, EnvField::Attack)] =
                    time(EnvField::Attack).value((x * 3.0).min(1.0));
            }
            3 => {
                q.params[env_param(e, EnvField::Decay)] =
                    time(EnvField::Decay).value(((x - 1.0 / 3.0) * 3.0).clamp(0.0, 1.0));
                // The picture keeps a margin top and bottom, so its points can be reached.
                q.params[env_param(e, EnvField::Sustain)] =
                    (1.0 - (y - 0.08) / 0.84).clamp(0.0, 1.0);
            }
            _ => {
                q.params[env_param(e, EnvField::Release)] =
                    time(EnvField::Release).value(((x - 2.0 / 3.0) * 3.0).clamp(0.0, 1.0));
            }
        }
    }

    pub fn poly_window(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let v = self.poly_view.as_ref()?;
        v.sliders.borrow_mut().clear();
        let (t, at, page, browsing, tables) = (v.track, v.at, v.page, v.browsing, v.tables);
        let cord = v.cord.zip(self.mod_drag_at).map(|(from, to)| {
            let hue = match self.drag {
                Some(Drag::PolySource { source, .. }) => source_color(source),
                _ => TEXT,
            };
            (from, to, hue)
        });
        let poly = self.poly(t)?.clone();
        let (w, h) = self.poly_size();
        let head = self.poly_head(t, &poly, page, cx);
        let body = match page {
            PolyPage::Synth => self.poly_synth_page(t, &poly, cx),
            PolyPage::Matrix => self.poly_matrix_page(t, &poly, cx),
            PolyPage::Effects => self.poly_effects_page(t, &poly, cx),
        };
        let keys = self.poly_keys_row(t, &poly, cx);
        let drawer = browsing.then(|| self.poly_sounds(t, &poly, cx));
        let list = tables.map(|o| self.poly_tables(t, &poly, o, cx));
        let frame = div()
            .id("poly-window")
            .occlude()
            .relative()
            .w(px(w))
            .h(px(h))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .overflow_hidden()
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(head)
            .child(body)
            .child(keys)
            .children(drawer)
            .children(list)
            .child(
                resize_grip()
                    .debug_selector(|| "poly-resize".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let start = ws.poly_size();
                            ws.begin_drag(
                                Drag::PolyWindow {
                                    from: pt(e.position),
                                    start,
                                    resize: true,
                                },
                                cx,
                            );
                        }),
                    ),
            )
            .children(cord.map(|(from, to, hue)| {
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| paint_cord(window, from, to, hue),
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
            }));
        Some(
            deferred(
                anchored()
                    .position(point(px(at.0), px(at.1)))
                    .snap_to_window()
                    .child(frame),
            )
            .with_priority(0),
        )
    }

    fn poly_synth_page(&mut self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let column = |d: Div| d.flex_none().h_full().border_r_1().border_color(rgb(LINE));
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(column(self.poly_sources(t, p, cx)).w(px(SOURCES_W)))
                    .child(
                        self.poly_editor(t, p, cx)
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .border_r_1()
                            .border_color(rgb(LINE)),
                    )
                    .child(column(self.poly_filters(t, p, cx)).w(px(FILTERS_W)))
                    .child(
                        self.poly_output(t, p, cx)
                            .flex_none()
                            .h_full()
                            .w(px(OUTPUT_W)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .h(px(MOD_H))
                    .flex()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(self.poly_modulation(t, p, cx).flex_1().min_w(px(0.)))
                    .child(
                        self.poly_macros(t, p, cx)
                            .flex_none()
                            .w(px(FILTERS_W + OUTPUT_W))
                            .border_l_1()
                            .border_color(rgb(LINE)),
                    ),
            )
    }

    /// Everything the window plays let go: its keys, typing, the demo.
    pub(crate) fn poly_all_off(&mut self) {
        let Some(v) = self.poly_view.as_mut() else {
            return;
        };
        v.demo = None;
        v.finger = None;
        let held = std::mem::take(&mut v.held);
        for pitch in held {
            self.play_note(pitch, 0);
        }
    }
}

/// A sound's spectrum for the filters' picture: dB at 96 places from 20 Hz to 20 kHz, falling
/// back gently from what it was.
fn spectrum_of(scope: &[f32], rate: f64, was: &[f32]) -> Vec<f32> {
    const PLACES: usize = 96;
    if scope.len() < 2048 {
        return vec![-120.0; PLACES];
    }
    let freqs: Vec<f64> = (0..PLACES)
        .map(|k| 20.0 * 1000f64.powf(k as f64 / (PLACES - 1) as f64))
        .collect();
    let now = pina_engine::dsp::eq::spectrum_db(&scope[scope.len() - 2048..], rate, &freqs);
    now.iter()
        .enumerate()
        .map(|(k, &db)| db.max(was.get(k).copied().unwrap_or(-120.0) - 1.5))
        .collect()
}

/// The cord from a source to the pointer, sagging as a cable does.
fn paint_cord(window: &mut gpui::Window, from: gpui::Point<f32>, to: gpui::Point<f32>, hue: u32) {
    let sag = (to.x - from.x).abs() * 0.2 + 40.0;
    let mid = ((from.x + to.x) / 2.0, from.y.max(to.y) + sag);
    let mut p = PathBuilder::stroke(px(2.));
    for k in 0..=32 {
        let s = k as f32 / 32.0;
        let (a, b, c) = ((1.0 - s) * (1.0 - s), 2.0 * s * (1.0 - s), s * s);
        let at = point(
            px(a * from.x + b * mid.0 + c * to.x),
            px(a * from.y + b * mid.1 + c * to.y),
        );
        match k {
            0 => p.move_to(at),
            _ => p.line_to(at),
        }
    }
    if let Ok(path) = p.build() {
        window.paint_path(path, rgb(hue));
    }
    window.paint_quad(
        gpui::fill(
            Bounds::centered_at(point(px(to.x), px(to.y)), gpui::size(px(9.), px(9.))),
            rgb(hue),
        )
        .corner_radii(px(4.5)),
    );
}

pub(crate) fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// A dark screen a picture is drawn on.
pub(crate) fn picture() -> Div {
    div()
        .relative()
        .overflow_hidden()
        .rounded(px(6.))
        .bg(rgb(WELL))
}

/// A small grey label.
pub(crate) fn label(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .text_color(rgb(TEXT_3))
        .whitespace_nowrap()
        .child(s.into())
}

/// A part's title.
pub(crate) fn title(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(13.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .child(s.into())
}

/// A line through `points` on `b`, x and y each 0 to 1 from the top left.
pub(crate) fn polyline(
    window: &mut gpui::Window,
    b: Bounds<Pixels>,
    points: impl Iterator<Item = (f32, f32)>,
    width: f32,
    color: gpui::Hsla,
) {
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let mut p = PathBuilder::stroke(px(width));
    let mut first = true;
    for (x, y) in points {
        let at = point(b.origin.x + px(x * w), b.origin.y + px(y * h));
        if first {
            p.move_to(at);
            first = false;
        } else {
            p.line_to(at);
        }
    }
    if let Ok(path) = p.build() {
        window.paint_path(path, color);
    }
}

/// The area between `points` and the level `floor` (0 to 1 down), filled.
pub(crate) fn filled(
    window: &mut gpui::Window,
    b: Bounds<Pixels>,
    points: &[(f32, f32)],
    floor: f32,
    color: gpui::Hsla,
) {
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let at = |x: f32, y: f32| point(b.origin.x + px(x * w), b.origin.y + px(y * h));
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return;
    };
    let mut p = PathBuilder::fill();
    p.move_to(at(first.0, floor));
    for &(x, y) in points {
        p.line_to(at(x, y));
    }
    p.line_to(at(last.0, floor));
    p.close();
    if let Ok(path) = p.build() {
        window.paint_path(path, color);
    }
}

/// A round handle on a picture, at (x, y) in pixels from `b`'s top left.
pub(crate) fn handle(window: &mut gpui::Window, b: Bounds<Pixels>, x: f32, y: f32, color: u32) {
    let c = point(b.origin.x + px(x), b.origin.y + px(y));
    window.paint_quad(
        gpui::fill(
            Bounds::centered_at(c, gpui::size(px(11.), px(11.))),
            rgb(WELL),
        )
        .corner_radii(px(5.5))
        .border_widths(px(2.))
        .border_color(rgb(color)),
    );
}
