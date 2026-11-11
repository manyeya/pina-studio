//! Bloom's window: a garden. The seed sits in the middle with its genes round it; twelve branches
//! grow out, one for each note of the octave, each ending in a flower drawn from its genes (its
//! petals from its brightness and attack, its breathing from its LFO). Dragged out a branch
//! grows and its sound strays from the seed; dragged sideways it bends, another way at the same
//! distance. Playing lights the branches that sound, sap running up them.
//!
//! Its pages: the Garden, the Strand (every gene of the seed, or of a branch's own steering, as a
//! double helix and as knobs), Grow (a seed grown to sound like a recording) and Sounds.
//!
//! Every control is a parameter of the plant, through the same knobs as anywhere in the app: the
//! growth, each branch's length and bend and every gene of the seed can be automated and
//! modulated.

mod controls;
mod garden;
pub(crate) mod grow;
mod panels;
pub(crate) mod sounds;
mod strand;

pub(crate) use garden::{branch_hue, hue_u32};

/// Where branch `k`'s flower is at `reach`, the garden's units; and which way it grows.
#[cfg(test)]
pub(crate) fn garden_tip(b: &Bloom, k: usize, reach: f32) -> (f32, f32) {
    garden::spine(k, b.paths[k], reach, b.params[branch_bend(k)], 0.0, 1)[1]
}

#[cfg(test)]
pub(crate) fn garden_angle(k: usize) -> f32 {
    garden::angle(k)
}

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use crate::ui::scale::px;
use gpui::{
    Bounds, Context, Div, Entity, MouseButton, MouseDownEvent, Pixels, Subscription, div, point,
    prelude::*, rgb,
};
use pina_engine::{dsp::bloom::BLOOM_LIVE, song::project::*};

use crate::{
    ui::{
        menu::{Menu, MenuAction},
        text::TextInput,
        theme::*,
    },
    workspace::{Drag, Target, Workspace, pt},
};

pub const WINDOW_W: f32 = 1320.0;
pub const WINDOW_H: f32 = 860.0;
const MIN_SCALE: f32 = 0.7;
const HEAD_H: f32 = 48.0;
const CONTROLS_H: f32 = 120.0;
const KEYS_H: f32 = 88.0;
const LEFT_W: f32 = 236.0;
/// The greenhouse's floating cards.
const SEED_W: f32 = 272.0;
const SEED_H: f32 = 180.0;
const FAMILY_W: f32 = 340.0;
const FAMILY_H: f32 = 104.0;
const BRANCH_W: f32 = 296.0;
/// How long the garden keeps moving after anything last happened in it, seconds.
const AWAKE_S: f32 = 14.0;

/// Which page of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BloomPage {
    Garden,
    Strand,
    Grow,
    Sounds,
}

pub struct BloomView {
    pub track: usize,
    pub at: (f32, f32),
    pub scale: Option<f32>,
    pub page: BloomPage,
    /// What a new seed is planted as, and whether every kind is offered over the garden.
    pub kind: SeedKind,
    pub picking: bool,
    /// The branch under the pointer; one being picked as a partner to cross with.
    pub hover: Option<usize>,
    pub crossing: bool,
    /// The Strand page's part, whether its knobs steer the picked branch rather than the seed,
    /// and the gene under the pointer.
    pub group: GeneGroup,
    pub steer: bool,
    pub gene_hover: Option<usize>,
    pub garden: Rc<Cell<Bounds<Pixels>>>,
    pub keys: Rc<Cell<Bounds<Pixels>>>,
    /// Notes down from the window, the one under the pointer, the one a dragged branch plays.
    pub held: Vec<u8>,
    pub finger: Option<u8>,
    pub audition: Option<u8>,
    pub wheel: f32,
    pub bend: f32,
    /// What the engine says, and the garden as drawn: each branch's reach springing to where
    /// it is, its glow falling back gently.
    pub live: [f32; BLOOM_LIVE],
    pub shown: [f32; BRANCHES],
    pub speed: [f32; BRANCHES],
    pub glow: [f32; BRANCHES],
    pub born: Instant,
    last: Instant,
    /// When something last happened: the garden rests a while after.
    pub woke: Instant,
    /// Counts up, for each planting's luck.
    pub luck: u32,
    pub sounds: sounds::SoundsView,
    pub grow: grow::GrowView,
    typing_was: bool,
    _typed: Vec<Subscription>,
}

/// Something done in the window.
#[derive(Clone, Debug, PartialEq)]
pub enum BloomOp {
    Page(BloomPage),
    Kind(SeedKind),
    /// The kinds offered, or put away.
    PickKind,
    Plant,
    Mutate,
    Replant,
    Clone,
    Cross,
    CrossWith(usize),
    GoBack(usize),
    Pick(usize),
    Layer(usize),
    ResetBranch(usize),
    Keys(BloomKeys),
    Mono(bool),
    Quality(PolyQuality),
    Sync(bool),
    Voices(u8),
    Lock(usize),
    LockGroup(GeneGroup, bool),
    Group(GeneGroup),
    Steer(bool),
    /// A branch's steering of one gene taken away.
    Unsteer(usize),
    /// A parameter back to its default; given a lane on the track.
    Reset(usize),
    Automate(usize),
    Sound(sounds::SoundOp),
}

impl Workspace {
    pub(crate) fn bloom(&self, t: usize) -> Option<&Bloom> {
        match self.session.project.tracks.get(t)?.instrument.as_ref()? {
            Instrument::Bloom(b) => Some(b),
            _ => None,
        }
    }

    pub(crate) fn bloom_mut(&mut self, t: usize) -> Option<&mut Bloom> {
        match self
            .session
            .project
            .tracks
            .get_mut(t)?
            .instrument
            .as_mut()?
        {
            Instrument::Bloom(b) => Some(b),
            _ => None,
        }
    }

    pub(crate) fn bloom_scale(&self) -> f32 {
        let fit = ((self.viewport_w - 16.0) / WINDOW_W).min((self.viewport_h - 16.0) / WINDOW_H);
        let made = self
            .bloom_view
            .as_ref()
            .and_then(|v| v.scale)
            .unwrap_or(1.0);
        made.min(fit).max(MIN_SCALE)
    }

    pub fn open_bloom(&mut self, t: usize, cx: &mut Context<Self>) {
        if self.bloom(t).is_none() {
            return;
        }
        if self.poly_view.is_some() {
            self.close_poly(cx);
        }
        if self.bloom_view.as_ref().is_some_and(|v| v.track != t) {
            self.bloom_all_off();
        }
        let s = self.bloom_scale();
        let at = (
            ((self.viewport_w - WINDOW_W * s) / 2.0).max(0.0),
            ((self.viewport_h - WINDOW_H * s) / 2.0).max(8.0),
        );
        let typing_was = match &self.bloom_view {
            Some(v) => v.typing_was,
            None => self.typing.is_some(),
        };
        self.typing.get_or_insert(0);
        let kept = self
            .bloom_view
            .take()
            .map(|v| (v.at, v.scale, v.page, v.kind, v.group));
        let (at, scale, page, kind, group) = kept.unwrap_or((
            at,
            None,
            BloomPage::Garden,
            SeedKind::Any,
            GeneGroup::Motion,
        ));
        let (sounds, mut typed) = sounds::SoundsView::new(cx);
        let (grow, grow_subs) = grow::GrowView::new(cx);
        typed.extend(grow_subs);
        let shown = self.bloom(t).map_or([0.0; BRANCHES], |b| {
            std::array::from_fn(|k| b.reach(k, 0.0))
        });
        let now = Instant::now();
        self.bloom_view = Some(BloomView {
            track: t,
            at,
            scale,
            page,
            kind,
            picking: false,
            hover: None,
            crossing: false,
            group,
            steer: false,
            gene_hover: None,
            garden: Default::default(),
            keys: Default::default(),
            held: Vec::new(),
            finger: None,
            audition: None,
            wheel: 0.0,
            bend: 0.0,
            live: [0.0; BLOOM_LIVE],
            // A garden opening grows into view.
            shown: shown.map(|r| r * 0.15),
            speed: [0.0; BRANCHES],
            glow: [0.0; BRANCHES],
            born: now,
            last: now,
            woke: now,
            luck: (now.elapsed().as_nanos() as u32) ^ t as u32,
            sounds,
            grow,
            typing_was,
            _typed: typed,
        });
        cx.notify();
    }

    pub fn close_bloom(&mut self, cx: &mut Context<Self>) {
        self.bloom_all_off();
        if let Some(v) = self.bloom_view.take()
            && !v.typing_was
        {
            self.typing = None;
        }
        cx.notify();
    }

    /// Everything the window plays let go.
    pub(crate) fn bloom_all_off(&mut self) {
        let Some(v) = self.bloom_view.as_mut() else {
            return;
        };
        v.finger = None;
        let mut held = std::mem::take(&mut v.held);
        held.extend(v.audition.take());
        for pitch in held {
            self.play_note(pitch, 0);
        }
    }

    pub fn drag_bloom_window(
        &mut self,
        from: gpui::Point<f32>,
        start: (f32, f32),
        resize: bool,
        p: gpui::Point<f32>,
    ) {
        let (vw, vh) = (self.viewport_w, self.viewport_h);
        if resize {
            let s = ((start.0 + p.x - from.x) / WINDOW_W).max((start.1 + p.y - from.y) / WINDOW_H);
            let fit = ((vw - 16.0) / WINDOW_W).min((vh - 16.0) / WINDOW_H);
            if let Some(v) = self.bloom_view.as_mut() {
                v.scale = Some(s.clamp(MIN_SCALE, fit.max(MIN_SCALE)));
            }
            return;
        }
        let w = WINDOW_W * self.bloom_scale();
        if let Some(v) = self.bloom_view.as_mut() {
            v.at = (
                (start.0 + p.x - from.x).clamp(-w + 80.0, vw - 80.0),
                (start.1 + p.y - from.y).clamp(0.0, vh - HEAD_H),
            );
        }
    }

    /// The engine's news, and the garden moved on a frame: whether it's still moving, so the
    /// window keeps drawing.
    pub(crate) fn bloom_frame(&mut self) -> bool {
        let id = self
            .bloom_view
            .as_ref()
            .and_then(|v| self.session.project.tracks.get(v.track))
            .map(|t| t.id);
        if self.bloom_view.is_some() {
            self.player.watch_poly(id);
        }
        let Some(t) = self.bloom_view.as_ref().map(|v| v.track) else {
            return false;
        };
        let reach: [f32; BRANCHES] = match self.bloom(t) {
            Some(b) => {
                let extra =
                    self.bloom_view.as_ref().map_or(0.0, |v| v.wheel) * b.params[BLOOM_WHEEL];
                std::array::from_fn(|k| b.reach(k, extra))
            }
            None => return false,
        };
        let live = match cfg!(test) {
            true => None,
            false => Some(self.player.bloom_live()),
        };
        let dragging = matches!(
            self.drag,
            Some(Drag::BloomBranch { .. } | Drag::BloomKeys | Drag::Knob { .. })
        );
        let growing = self.bloom_view.as_ref().is_some_and(|v| v.grow.running());
        let Some(v) = self.bloom_view.as_mut() else {
            return false;
        };
        if let Some(live) = live {
            v.live = live;
        }
        let now = Instant::now();
        let dt = now.duration_since(v.last).as_secs_f32().clamp(0.0, 0.05);
        v.last = now;
        let mut moving = false;
        #[allow(clippy::needless_range_loop)] // each branch's spring, glow and reach together
        for k in 0..BRANCHES {
            // Where it's playing to, when a note's slide grows it further.
            let level = v.live[k];
            let to = match level > 1e-3 {
                true => reach[k].max(v.live[BRANCHES + k]),
                false => reach[k],
            };
            // A spring a little under critical: it overshoots a touch, as a stem does.
            let (stiff, damp) = (90.0, 15.0);
            let x = v.shown[k] - to;
            v.speed[k] += (-stiff * x - damp * v.speed[k]) * dt;
            v.shown[k] += v.speed[k] * dt;
            if x.abs() > 1e-4 || v.speed[k].abs() > 1e-4 {
                moving = true;
            } else {
                (v.shown[k], v.speed[k]) = (to, 0.0);
            }
            let fall = (v.glow[k] - dt * 2.5).max(0.0);
            v.glow[k] = level.min(1.0).max(fall);
            if v.glow[k] > 1e-3 {
                moving = true;
            }
        }
        if moving || dragging || growing || !v.held.is_empty() || v.live[2 * BRANCHES] > 0.0 {
            v.woke = now;
        }
        // Resting: the garden stops swaying, and costs nothing until something happens.
        cfg!(not(test)) && now.duration_since(v.woke).as_secs_f32() < AWAKE_S
    }

    /// Something happened: the garden wakes.
    pub(crate) fn wake_bloom(&mut self) {
        if let Some(v) = self.bloom_view.as_mut() {
            v.woke = Instant::now();
        }
    }

    /// The time the garden's moving to, seconds: fixed in tests so pictures repeat.
    pub(crate) fn bloom_time(&self) -> f32 {
        match cfg!(test) {
            true => 3.25,
            false => self
                .bloom_view
                .as_ref()
                .map_or(0.0, |v| v.born.elapsed().as_secs_f32()),
        }
    }

    pub fn bloom_op(&mut self, t: usize, op: BloomOp, cx: &mut Context<Self>) {
        self.wake_bloom();
        // What only the window remembers.
        if let Some(v) = self.bloom_view.as_mut() {
            match op {
                BloomOp::Page(p) => {
                    v.page = p;
                    return cx.notify();
                }
                BloomOp::Kind(k) => {
                    (v.kind, v.picking) = (k, false);
                    return cx.notify();
                }
                BloomOp::PickKind => {
                    v.picking = !v.picking;
                    return cx.notify();
                }
                BloomOp::Group(g) => {
                    v.group = g;
                    return cx.notify();
                }
                BloomOp::Steer(s) => {
                    v.steer = s;
                    return cx.notify();
                }
                BloomOp::Cross => {
                    v.crossing = !v.crossing;
                    self.status = v.crossing.then(|| {
                        "Pick the branch to cross it with: their child becomes the seed".into()
                    });
                    return cx.notify();
                }
                _ => {}
            }
        }
        match op {
            BloomOp::Sound(s) => return self.bloom_sound_op(t, s, cx),
            BloomOp::Automate(i) => {
                self.show_lane(t, Param::Poly(i as u16));
                self.status = Some(format!(
                    "{} has a lane under its track now",
                    BLOOM_PARAMS[i].name
                ));
                return cx.notify();
            }
            _ => {}
        }
        let luck = self.bloom_view.as_mut().map_or(7, |v| {
            v.luck = v.luck.wrapping_add(0x9e37_79b9);
            v.luck
        });
        let kind = self.bloom_view.as_ref().map_or(SeedKind::Any, |v| v.kind);
        if let BloomOp::CrossWith(_) = op
            && let Some(v) = self.bloom_view.as_mut()
        {
            v.crossing = false;
            self.status = None;
        }
        // A new seed: its branches sprout from nothing, as a seed's do.
        let sprouts = matches!(
            op,
            BloomOp::Plant | BloomOp::Replant | BloomOp::CrossWith(_) | BloomOp::GoBack(_)
        );
        self.edit(cx, |ws| {
            let Some(b) = ws.bloom_mut(t) else { return };
            match op {
                BloomOp::Plant => b.plant(kind, luck),
                BloomOp::Mutate => {
                    let seed = b.seed();
                    let nudged: Vec<f32> = seed
                        .iter()
                        .enumerate()
                        .map(|(i, g)| match b.locked(i) || GENE_TABLE[i].plant {
                            true => *g,
                            false => {
                                let h = mix32(luck ^ mix32(i as u32));
                                let r = (h >> 8) as f32 / (1u32 << 24) as f32;
                                reflect(g + 0.08 * GENE_TABLE[i].weight * (2.0 * r - 1.0))
                            }
                        })
                        .collect();
                    b.set_seed(&nudged);
                }
                BloomOp::Replant => b.replant(luck),
                BloomOp::Clone => b.clone_chosen(),
                BloomOp::CrossWith(other) => {
                    let mate = b.branch_genes(other, b.reach(other, 0.0));
                    b.cross(&mate, luck);
                }
                BloomOp::GoBack(k) => b.go_back(k),
                BloomOp::Pick(k) => b.chosen = (k % BRANCHES) as u8,
                BloomOp::Layer(k) => b.layer ^= 1 << (k % BRANCHES),
                BloomOp::ResetBranch(k) => {
                    b.params[branch_length(k)] = 0.0;
                    b.params[branch_bend(k)] = 0.0;
                    b.edits.retain(|e| e.branch as usize != k);
                }
                BloomOp::Keys(k) => b.keys = k,
                BloomOp::Mono(m) => b.voicing.mono = m,
                BloomOp::Quality(q) => b.quality = q,
                BloomOp::Sync(s) => b.sync = s,
                BloomOp::Voices(n) => b.voicing.polyphony = n.clamp(1, 32),
                BloomOp::Lock(g) => b.locks ^= 1 << g,
                BloomOp::LockGroup(g, on) => {
                    for i in g.genes() {
                        match on {
                            true => b.locks |= 1 << i,
                            false => b.locks &= !(1 << i),
                        }
                    }
                }
                BloomOp::Unsteer(g) => {
                    let c = b.chosen;
                    b.edits.retain(|e| !(e.branch == c && e.gene as usize == g));
                }
                BloomOp::Reset(i) => b.params[i] = BLOOM_PARAMS[i].default,
                _ => {}
            }
        });
        if sprouts && let Some(v) = self.bloom_view.as_mut() {
            for k in 0..BRANCHES {
                // Staggered, round the plant.
                v.shown[k] = 0.0;
                v.speed[k] = -0.6 * k as f32;
            }
        }
    }

    /// How big the window is on screen.
    pub(crate) fn bloom_size(&self) -> (f32, f32) {
        let s = self.bloom_scale();
        (WINDOW_W * s, WINDOW_H * s)
    }

    /// The window, as its own view draws it: the parts that only change when the plant does
    /// (its title, rails, controls and keys) are views of their own, kept as they were drawn;
    /// what moves (the garden, the strand, the strands growing) is drawn again each frame. A
    /// frame of the garden draws nothing else in the app.
    fn bloom_window_body(
        &mut self,
        parts: &[Entity<BloomPart>; 6],
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(v) = self.bloom_view.as_ref() else {
            return gpui::Empty.into_any_element();
        };
        let (t, page) = (v.track, v.page);
        let Some(b) = self.bloom(t).cloned() else {
            return gpui::Empty.into_any_element();
        };
        if self.bloom_frame() {
            window.request_animation_frame();
        }
        // While the song plays, its automation turns the knobs: the parts follow it.
        if self.player.playing() {
            for p in parts {
                p.update(cx, |_, cx| cx.notify());
            }
        }
        // What this window's knobs said they were for modulators last frame goes, once each.
        {
            let mut seen = std::collections::HashSet::new();
            self.mod_targets
                .borrow_mut()
                .retain(|(t, mt, b)| seen.insert((*t, format!("{mt:?}"), format!("{b:?}"))));
        }
        let s = self.bloom_scale();
        crate::ui::scale::with_scale(s, || {
            let part = |k: usize, style: gpui::StyleRefinement| {
                gpui::AnyView::from(parts[k].clone()).cached(style)
            };
            // The greenhouse: the garden edge to edge, the seed, its family and the picked
            // branch floating over it.
            let body = match page {
                BloomPage::Garden => div()
                    .flex_1()
                    .min_h(px(0.))
                    .relative()
                    .overflow_hidden()
                    .bg(gpui::rgb(0x090c0b))
                    .child(
                        self.bloom_garden(t, &b, cx)
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                    )
                    .child(part(
                        1,
                        gpui::StyleRefinement::default()
                            .absolute()
                            .left(px(16.))
                            .top(px(16.))
                            .w(px(SEED_W))
                            .h(px(SEED_H)),
                    ))
                    .child(part(
                        2,
                        gpui::StyleRefinement::default()
                            .absolute()
                            .left(px(16.))
                            .bottom(px(16.))
                            .w(px(FAMILY_W))
                            .h(px(FAMILY_H)),
                    ))
                    .child(part(
                        3,
                        gpui::StyleRefinement::default()
                            .absolute()
                            .right(px(16.))
                            .top(px(16.))
                            .bottom(px(16.))
                            .w(px(BRANCH_W)),
                    )),
                BloomPage::Strand => self.bloom_strand_page(t, &b, cx),
                BloomPage::Grow => self.bloom_grow_page(t, &b, cx),
                BloomPage::Sounds => self.bloom_sounds_page(t, &b, cx),
            };
            // Growing wants the room; the keys stay, to play what's tried.
            let dock = (page != BloomPage::Grow).then(|| {
                part(
                    4,
                    gpui::StyleRefinement::default()
                        .flex_none()
                        .w_full()
                        .h(px(CONTROLS_H)),
                )
            });
            let dialog = self.bloom_save_dialog(t, cx);
            let picker = self.bloom_kind_picker(t, cx);
            let frame = div()
                .id("bloom-window")
                .occlude()
                .relative()
                .w(px(WINDOW_W))
                .h(px(WINDOW_H))
                .flex()
                .flex_col()
                .rounded(px(14.))
                .overflow_hidden()
                .bg(rgb(SURFACE))
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .shadow_lg()
                .font_family(SANS)
                .text_color(rgb(TEXT))
                .text_size(px(12.))
                .on_mouse_move(cx.listener(|ws, _: &gpui::MouseMoveEvent, _, _| ws.wake_bloom()))
                .child(part(
                    0,
                    gpui::StyleRefinement::default()
                        .flex_none()
                        .w_full()
                        .h(px(HEAD_H)),
                ))
                .child(body)
                .children(dock)
                .child(part(
                    5,
                    gpui::StyleRefinement::default()
                        .flex_none()
                        .w_full()
                        .h(px(KEYS_H)),
                ))
                .children(picker)
                .children(dialog)
                .child(
                    resize_grip()
                        .debug_selector(|| "bloom-resize".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let s = ws.bloom_scale();
                                ws.begin_drag(
                                    Drag::BloomWindow {
                                        from: pt(e.position),
                                        start: (WINDOW_W * s, WINDOW_H * s),
                                        resize: true,
                                    },
                                    cx,
                                );
                            }),
                        ),
                );
            crate::ui::scale::scaled(s, frame).into_any_element()
        })
    }

    /// One of the window's parts that's kept as drawn, drawn.
    fn bloom_part(&mut self, part: Part, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(v) = self.bloom_view.as_ref() else {
            return gpui::Empty.into_any_element();
        };
        let (t, page) = (v.track, v.page);
        let Some(b) = self.bloom(t).cloned() else {
            return gpui::Empty.into_any_element();
        };
        let s = self.bloom_scale();
        crate::ui::scale::with_scale(s, || {
            let d = match part {
                Part::Head => self.bloom_head(t, &b, page, cx),
                Part::Seed => self.bloom_seed_card(t, &b, cx),
                Part::Family => self.bloom_family_card(t, &b, cx),
                Part::Branch => self.bloom_branch_card(t, &b, cx),
                Part::Controls => self.bloom_controls(t, &b, cx),
                Part::Keys => self.bloom_keys_row(t, &b, cx),
            };
            crate::ui::scale::scaled(s, d.size_full()).into_any_element()
        })
    }

    fn bloom_head(&mut self, t: usize, b: &Bloom, page: BloomPage, cx: &mut Context<Self>) -> Div {
        let track = self.session.project.tracks[t].name.clone();
        let at = self.bloom_view.as_ref().map_or((0.0, 0.0), |v| v.at);
        let tab = |name: &'static str, to: BloomPage, cx: &mut Context<Self>| {
            seg(name, page == to, 28.)
                .debug_selector(move || format!("bloom-page-{}", name.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::Page(to), cx);
                    }),
                )
        };
        let sound = match b.sound.is_empty() {
            true => format!("{} seed", b.kind.name()),
            false => b.sound.clone(),
        };
        let quality = |name: &'static str, q: PolyQuality, cx: &mut Context<Self>| {
            seg(name, b.quality == q, 24.)
                .debug_selector(move || format!("bloom-quality-{}", name.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::Quality(q), cx);
                    }),
                )
        };
        div()
            .flex_none()
            .h(px(HEAD_H))
            .pl(px(16.))
            .pr(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_b_1()
            .border_color(rgb(LINE))
            .cursor_grab()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.begin_drag(
                        Drag::BloomWindow {
                            from: pt(e.position),
                            start: at,
                            resize: false,
                        },
                        cx,
                    );
                }),
            )
            .child(garden::emblem(18.0))
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(BLOOM_NAME.to_uppercase()),
            )
            .child(chip(track, TEXT_3))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(TEXT))
                    .whitespace_nowrap()
                    .child(sound),
            )
            .child(chip(
                format!("Generation {}", b.generation + 1),
                hue_u32(b.chosen as usize),
            ))
            .child(div().flex_1())
            .child(
                button("Save", false)
                    .debug_selector(|| "bloom-save".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, window, cx| {
                            cx.stop_propagation();
                            ws.bloom_start_save(t, window, cx);
                        }),
                    ),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(tab("Garden", BloomPage::Garden, cx))
                    .child(tab("Strand", BloomPage::Strand, cx))
                    .child(tab("Grow", BloomPage::Grow, cx))
                    .child(tab("Sounds", BloomPage::Sounds, cx)),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(7.))
                    .child(quality("Draft", PolyQuality::Draft, cx))
                    .child(quality("Good", PolyQuality::Good, cx))
                    .child(quality("Best", PolyQuality::Best, cx)),
            )
            .child(
                div()
                    .id("bloom-close")
                    .debug_selector(|| "bloom-close".into())
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(CARD)))
                    .child(icon(Icon::Close, 14.).text_color(rgb(TEXT_2)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.close_bloom(cx);
                        }),
                    ),
            )
    }

    /// A menu of choices, as the window's.
    pub(crate) fn bloom_menu(
        &mut self,
        at: gpui::Point<Pixels>,
        items: Vec<(String, MenuAction)>,
        cx: &mut Context<Self>,
    ) {
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// The note that plays branch `k` as the keys are set, and how hard.
    pub(crate) fn bloom_note_for(&self, b: &Bloom, k: usize) -> (u8, u8) {
        match b.keys {
            BloomKeys::Octave => (60 + k as u8, 100),
            BloomKeys::Spread => ((24 + (k * 85 + 42) / 12) as u8, 100),
            BloomKeys::Velocity => (60, ((k as f32 + 0.5) / 12.0 * 127.0) as u8),
            _ => (60, 100),
        }
    }
}

/// The parts of the window kept as drawn until the plant changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Head,
    Seed,
    Family,
    Branch,
    Controls,
    Keys,
}

/// Bloom's window, a view of its own: its frames (the garden swaying, sap rising, strands
/// growing) draw it alone, not the app under it. Drawn again whenever the workspace changes.
pub struct BloomWindow {
    ws: gpui::WeakEntity<Workspace>,
    parts: [Entity<BloomPart>; 6],
    _changes: Subscription,
}

impl BloomWindow {
    pub fn new(ws: &Entity<Workspace>, cx: &mut Context<Self>) -> BloomWindow {
        let parts = [
            Part::Head,
            Part::Seed,
            Part::Family,
            Part::Branch,
            Part::Controls,
            Part::Keys,
        ]
        .map(|part| cx.new(|cx| BloomPart::new(ws, part, cx)));
        BloomWindow {
            ws: ws.downgrade(),
            parts,
            _changes: cx.observe(ws, |_, _, cx| cx.notify()),
        }
    }
}

impl gpui::Render for BloomWindow {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let parts = self.parts.clone();
        self.ws
            .update(cx, |ws, cx| ws.bloom_window_body(&parts, window, cx))
            .unwrap_or_else(|_| gpui::Empty.into_any_element())
    }
}

/// A part of Bloom's window kept as it was drawn until the workspace changes.
pub struct BloomPart {
    ws: gpui::WeakEntity<Workspace>,
    part: Part,
    _changes: Subscription,
}

impl BloomPart {
    fn new(ws: &Entity<Workspace>, part: Part, cx: &mut Context<Self>) -> BloomPart {
        BloomPart {
            ws: ws.downgrade(),
            part,
            _changes: cx.observe(ws, |_, _, cx| cx.notify()),
        }
    }
}

impl gpui::Render for BloomPart {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let part = self.part;
        self.ws
            .update(cx, |ws, cx| ws.bloom_part(part, cx))
            .unwrap_or_else(|_| gpui::Empty.into_any_element())
    }
}

/// A note's name with its octave: C4 is middle C.
pub(crate) fn note_name(n: u8) -> String {
    format!("{}{}", BRANCH_NAMES[n as usize % 12], n as i32 / 12 - 1)
}

/// Just under the window's controls: what's pointed at, said.
pub(crate) fn hint(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .text_color(rgb(TEXT_3))
        .child(s.into())
}

pub(crate) fn small(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(10.5))
        .text_color(rgb(TEXT_4))
        .whitespace_nowrap()
        .child(s.into())
}

/// How long ago, roughly, for a sound's line.
pub(crate) fn ago(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86_400 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86_400),
    }
}
