//! The garden: the seed, its genes round it, twelve branches and their flowers, drawn from the
//! plant's own genes every frame and swaying a little while anything's happening.
//!
//! Everything's worked out in the garden's own units, its radius 1 from the seed: `spine` is a
//! branch's line, the same one the pointer's found and dragged by, so what's grabbed is what's
//! drawn. A frame is a few dozen paths (each branch's stem, its leaves together, its flower
//! together) and some quads; glows are the GPU's blurred shadows, which cost next to nothing.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use gpui::{
    BoxShadow, Corners, Hsla, MouseMoveEvent, PathBuilder, hsla, linear_color_stop,
    linear_gradient, size,
};

use super::*;

/// The seed's orb, the ring its genes sit on, where branches start, a full-grown tip, and the
/// ring of notes: shares of the garden's radius.
pub(crate) const SEED_R: f32 = 0.075;
const GENOME_IN: f32 = 0.105;
const GENOME_OUT: f32 = 0.175;
pub(crate) const BASE: f32 = 0.2;
pub(crate) const TIP: f32 = 0.8;
const RING: f32 = 0.915;
/// How far a branch bends at its tip, radians, either way.
pub(crate) const BEND: f32 = 0.42;
/// The garden's side, in the window's units.
const SIDE: f32 = 600.0;

/// A branch's colour: round the colour wheel as its note is round the octave.
pub(crate) fn branch_hue(k: usize, light: f32, a: f32) -> Hsla {
    let h = (0.97 + k as f32 / BRANCHES as f32 * 0.83).fract();
    hsla(h, 0.78, light, a)
}

/// The same, as a plain colour for the house's widgets.
pub(crate) fn hue_u32(k: usize) -> u32 {
    let c: gpui::Rgba = branch_hue(k, 0.66, 1.0).into();
    let byte = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(c.r) << 16 | byte(c.g) << 8 | byte(c.b)
}

/// Which way branch `k` grows: C straight up, round clockwise.
pub(crate) fn angle(k: usize) -> f32 {
    -FRAC_PI_2 + k as f32 * TAU / BRANCHES as f32
}

fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Branch `k`'s line out from the seed, `n` + 1 points: as long as its reach, bent as it's
/// bent, wandering as its way through the genes does, swayed by `sway`.
pub(crate) fn spine(
    k: usize,
    path: u32,
    reach: f32,
    bend: f32,
    sway: f32,
    n: usize,
) -> Vec<(f32, f32)> {
    let theta = angle(k);
    let h = mix32(path ^ 0x6a09_e667);
    let f = 1.0 + 2.0 * unit(h);
    let phase = TAU * unit(mix32(h ^ 0x55));
    let wander = 0.08 + 0.12 * unit(mix32(h ^ 0x99));
    let reach = reach.clamp(0.0, 1.0);
    (0..=n)
        .map(|i| {
            let s = i as f32 / n as f32;
            let d = BASE + s * (TIP - BASE) * reach;
            let phi = bend * BEND * s.powf(1.3)
                + wander * (PI * f * s + phase).sin() * s * (1.0 - s) * 2.0
                + sway * s * s;
            let a = theta + phi;
            (d * a.cos(), d * a.sin())
        })
        .collect()
}

/// From the pointer (garden units) to a branch's reach and bend, as a drag sets them.
pub(crate) fn reach_and_bend(k: usize, x: f32, y: f32) -> (f32, f32) {
    let r = x.hypot(y);
    let reach = ((r - BASE) / (TIP - BASE)).clamp(0.0, 1.0);
    let mut off = y.atan2(x) - angle(k);
    while off > PI {
        off -= TAU;
    }
    while off < -PI {
        off += TAU;
    }
    (reach, (off / BEND).clamp(-1.0, 1.0))
}

/// The branch nearest the pointer (garden units): one whose flower it's on, else whose way it
/// points. None on the seed.
pub(crate) fn branch_at(b: &Bloom, shown: &[f32; BRANCHES], x: f32, y: f32) -> Option<usize> {
    let r = x.hypot(y);
    if r < GENOME_OUT {
        return None;
    }
    let tips = (0..BRANCHES).map(|k| {
        let s = spine(k, b.paths[k], shown[k], b.params[branch_bend(k)], 0.0, 1);
        let (tx, ty) = s[1];
        (k, (tx - x).hypot(ty - y))
    });
    if let Some((k, _)) = tips
        .filter(|(_, d)| *d < 0.06)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        return Some(k);
    }
    let a = y.atan2(x);
    (0..BRANCHES).min_by(|&i, &j| {
        let off = |k: usize| {
            let tip = angle(k) + b.params[branch_bend(k)] * BEND * 0.5;
            ((a - tip + PI).rem_euclid(TAU) - PI).abs()
        };
        off(i).total_cmp(&off(j))
    })
}

/// How much branch genes `g` stray from `seed`, 0 to 1.
fn strayed(g: &[f32; GENES], seed: &[f32; GENES]) -> f32 {
    let d: f32 = g.iter().zip(seed).map(|(a, b)| (a - b).abs()).sum();
    (d / GENES as f32 * 5.0).clamp(0.0, 1.0)
}

/// A flower as its genes draw it.
#[derive(Clone, Copy)]
struct Flower {
    petals: usize,
    long: f32,
    wide: f32,
    spin: f32,
    breath: f32,
}

fn flower_of(g: &[f32; GENES], t: f32, k: usize) -> Flower {
    let lfo = GENE_TABLE[9].shape.value(g[9]).clamp(0.05, 3.0);
    let moves = (g[12] + g[13] + g[11]).min(1.0);
    Flower {
        petals: 5 + (g[17] * 12.0) as usize % 4,
        long: 0.03 + 0.032 * g[32],
        wide: 0.3 + 0.45 * g[0].sqrt(),
        spin: t * (0.05 + 0.25 * g[9]) + k as f32 * 0.7,
        breath: 1.0 + 0.09 * moves * (t * TAU * lfo * 0.5).sin(),
    }
}

/// Everything a frame draws, worked out before it's painted.
struct Scene {
    t: f32,
    branches: Vec<Drawn>,
    seed: [f32; GENES],
    /// The picked branch's genes, against the seed's.
    picked: [f32; GENES],
    chosen: usize,
    locks: u64,
    /// How far the plant's growth and the wheel grow everything, 0 to 1.
    extra: f32,
    /// How much is sounding, 0 to 1.
    sounding: f32,
    crossing: bool,
    awake: bool,
}

struct Drawn {
    k: usize,
    spine: Vec<(f32, f32)>,
    /// Where its length alone would put its tip, if the growth takes it further.
    base_tip: Option<(f32, f32)>,
    reach: f32,
    leaves: Vec<(usize, f32, f32)>,
    flower: Flower,
    glow: f32,
    hover: bool,
    layered: bool,
}

impl Workspace {
    pub(crate) fn bloom_garden(&mut self, t: usize, b: &Bloom, cx: &mut Context<Self>) -> Div {
        let Some(v) = self.bloom_view.as_ref() else {
            return div();
        };
        let time = self.bloom_time();
        let awake = cfg!(test) || v.woke.elapsed().as_secs_f32() < AWAKE_S;
        // A sway that dies away as the garden comes to rest.
        let calm = match awake {
            true => 1.0,
            false => 0.0,
        };
        let seed = b.seed();
        let chosen = b.chosen as usize % BRANCHES;
        let extra = (b.params[BLOOM_GROWTH] + v.wheel * b.params[BLOOM_WHEEL]).clamp(0.0, 1.0);
        let branches: Vec<Drawn> = (0..BRANCHES)
            .map(|k| {
                let reach = v.shown[k].clamp(0.0, 1.2);
                let bend = b.params[branch_bend(k)];
                let sway = calm * 0.035 * (time * 0.55 + k as f32 * 1.7).sin()
                    + calm * 0.012 * (time * 1.9 + k as f32 * 0.4).sin();
                let spine = spine(k, b.paths[k], reach.min(1.0), bend, sway * reach, 28);
                let len = b.params[branch_length(k)];
                let base_tip = (extra > 0.01 && len < 0.99).then(|| {
                    let s = super::garden::spine(k, b.paths[k], len, bend, sway * len, 1);
                    s[1]
                });
                let tip = b.branch_genes(k, reach.min(1.0));
                let mut leaves = Vec::new();
                for (n, s) in [0.3f32, 0.47, 0.63, 0.78].iter().enumerate() {
                    let along = s * reach.min(1.0) * (TIP - BASE);
                    if along < 0.045 {
                        continue;
                    }
                    let g = b.branch_genes(k, s * reach.min(1.0));
                    let i = ((*s * 28.0) as usize).min(27);
                    let side = if (n + k) % 2 == 0 { 1.0 } else { -1.0 };
                    leaves.push((i, side, strayed(&g, &seed)));
                }
                Drawn {
                    k,
                    spine,
                    base_tip,
                    reach,
                    leaves,
                    flower: flower_of(&tip, time, k),
                    glow: v.glow[k],
                    hover: v.hover == Some(k),
                    layered: b.keys == BloomKeys::Layer && b.layered(k),
                }
            })
            .collect();
        let sounding = v.glow.iter().fold(0f32, |m, g| m.max(*g));
        let scene = Scene {
            t: time,
            picked: b.branch_genes(chosen, v.shown[chosen].min(1.0)),
            branches,
            seed,
            chosen,
            locks: b.locks,
            extra,
            sounding,
            crossing: v.crossing,
            awake,
        };
        let bounds = v.garden.clone();
        let canvas = crate::ui::scale::scaled_canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| paint_garden(window, b, &scene),
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        // The notes round the ring, and the picked one's name and growth.
        let labels = (0..BRANCHES).map(|k| {
            let a = angle(k);
            let (x, y) = (0.5 + 0.5 * RING * a.cos(), 0.5 + 0.5 * RING * a.sin());
            let on = k == chosen;
            div()
                .absolute()
                .left(px(x * SIDE - 16.0))
                .top(px(y * SIDE - 9.0))
                .w(px(32.))
                .h(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .font_family(MONO)
                .text_size(px(if on { 12. } else { 10.5 }))
                .font_weight(if on {
                    gpui::FontWeight::BOLD
                } else {
                    gpui::FontWeight::MEDIUM
                })
                .text_color(if on {
                    branch_hue(k, 0.78, 1.0)
                } else {
                    branch_hue(k, 0.7, 0.55)
                })
                .child(BRANCH_NAMES[k])
        });
        let empty = (0..BRANCHES).all(|k| b.params[branch_length(k)] < 0.01) && extra < 0.01;
        let hover_tip = v.hover.map(|k| {
            let s = spine(
                k,
                b.paths[k],
                v.shown[k].min(1.0),
                b.params[branch_bend(k)],
                0.0,
                1,
            );
            let (x, y) = s[1];
            let grown = b.params[branch_length(k)];
            (k, 0.5 + 0.5 * x, 0.5 + 0.5 * y, grown)
        });
        let garden = v.garden.clone();
        let shown = v.shown;
        let held = b.clone();
        let at = move |p: gpui::Point<Pixels>| -> (f32, f32) {
            let g = garden.get();
            let half = f32::from(g.size.width).max(1.0) / 2.0;
            (
                (f32::from(p.x) - f32::from(g.origin.x) - half) / half,
                (f32::from(p.y) - f32::from(g.origin.y) - half) / half,
            )
        };
        let square = div()
            .relative()
            .w(px(SIDE))
            .h(px(SIDE))
            .flex_none()
            .debug_selector(|| "bloom-garden".into())
            .child(canvas)
            .children(labels)
            .children(hover_tip.map(|(k, x, y, grown)| {
                let below = y > 0.5;
                div()
                    .absolute()
                    .left(px(x * SIDE - 60.0))
                    .top(px(y * SIDE + if below { 18.0 } else { -40.0 }))
                    .w(px(120.))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(6.))
                            .bg(alpha(0x0b0d0c, 0.86))
                            .border_1()
                            .border_color(branch_hue(k, 0.6, 0.5))
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_2))
                            .whitespace_nowrap()
                            .child(format!("{} · {:.0}% grown", BRANCH_NAMES[k], grown * 100.0)),
                    )
            }))
            .when(empty, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(SIDE * 0.5 + 64.0))
                        .flex()
                        .justify_center()
                        .child(hint("Drag out from the seed to grow a branch")),
                )
            })
            .on_mouse_move({
                let at = at.clone();
                let held = held.clone();
                cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
                    let (x, y) = at(e.position);
                    let now = match x.hypot(y) <= 1.0 {
                        true => branch_at(&held, &shown, x, y),
                        false => None,
                    };
                    if let Some(v) = ws.bloom_view.as_mut()
                        && v.hover != now
                    {
                        v.hover = now;
                        cx.notify();
                    }
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let at = at.clone();
                    move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let (x, y) = at(e.position);
                        ws.bloom_press(t, x, y, e, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let (x, y) = at(e.position);
                    let Some(b) = ws.bloom(t) else { return };
                    let shown = ws.bloom_view.as_ref().map_or([0.0; BRANCHES], |v| v.shown);
                    match branch_at(b, &shown, x, y) {
                        Some(k) => ws.bloom_branch_menu(t, k, e.position, cx),
                        None => ws.bloom_seed_menu(t, e.position, cx),
                    }
                }),
            );
        div()
            .relative()
            .overflow_hidden()
            .bg(gpui::rgb(0x090c0b))
            .flex()
            .items_center()
            .justify_center()
            .child(square)
    }

    /// The pointer down on the garden at (x, y), its units: a branch taken hold of (and heard),
    /// crossed with, or replanted; the seed planted anew.
    fn bloom_press(
        &mut self,
        t: usize,
        x: f32,
        y: f32,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(b) = self.bloom(t).cloned() else {
            return;
        };
        let b = &b;
        let shown = self
            .bloom_view
            .as_ref()
            .map_or([0.0; BRANCHES], |v| v.shown);
        let crossing = self.bloom_view.as_ref().is_some_and(|v| v.crossing);
        let Some(k) = branch_at(b, &shown, x, y) else {
            if x.hypot(y) < GENOME_OUT {
                if e.click_count == 2 || e.modifiers.platform {
                    self.bloom_op(t, BloomOp::Plant, cx);
                } else if e.modifiers.alt {
                    self.bloom_op(t, BloomOp::Clone, cx);
                } else {
                    self.status = Some(
                        "Double-click the seed to plant a new one, ⌥-click to clone the picked \
                         branch to them all"
                            .into(),
                    );
                    cx.notify();
                }
            }
            return;
        };
        if crossing {
            return self.bloom_op(t, BloomOp::CrossWith(k), cx);
        }
        if e.click_count == 2 {
            self.bloom_op(t, BloomOp::Pick(k), cx);
            return self.bloom_op(t, BloomOp::Replant, cx);
        }
        if b.chosen as usize != k {
            self.bloom_op(t, BloomOp::Pick(k), cx);
        }
        let on_tip = {
            let s = spine(k, b.paths[k], shown[k], b.params[branch_bend(k)], 0.0, 1);
            (s[1].0 - x).hypot(s[1].1 - y) < 0.06
        };
        self.begin_drag(
            Drag::BloomBranch {
                track: t,
                branch: k,
            },
            cx,
        );
        // Pressed off its flower, it grows to where it was pressed.
        if !on_tip {
            self.drag_bloom_branch(t, k, x, y);
        }
        let note = self.bloom(t).map(|b| self.bloom_note_for(b, k));
        if let Some((pitch, velocity)) = note {
            self.play_note(pitch, velocity);
            if let Some(v) = self.bloom_view.as_mut() {
                v.audition = Some(pitch);
            }
        }
        cx.notify();
    }

    /// A branch dragged to (x, y), garden units: its length and bend set, and written as
    /// automation if the track's writing.
    pub(crate) fn drag_bloom_branch(&mut self, t: usize, k: usize, x: f32, y: f32) {
        let (reach, bend) = reach_and_bend(k, x, y);
        let wheel = self.bloom_view.as_ref().map_or(0.0, |v| v.wheel);
        let Some(b) = self.bloom_mut(t) else { return };
        let extra = (b.params[BLOOM_GROWTH] + wheel * b.params[BLOOM_WHEEL]).clamp(0.0, 1.0);
        // What's dragged is where the tip goes: the length that, with the growth, reaches it.
        let len = match extra < 0.999 {
            true => ((reach - extra) / (1.0 - extra)).clamp(0.0, 1.0),
            false => reach,
        };
        b.params[branch_length(k)] = len;
        b.params[branch_bend(k)] = bend;
        self.moved(t, Param::Poly(branch_length(k) as u16), len);
        self.moved(t, Param::Poly(branch_bend(k) as u16), bend);
    }

    /// A finger off a branch: what it played let go.
    pub(crate) fn bloom_let_go(&mut self) {
        if let Some(p) = self.bloom_view.as_mut().and_then(|v| v.audition.take()) {
            self.play_note(p, 0);
        }
    }

    fn bloom_branch_menu(
        &mut self,
        t: usize,
        k: usize,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(b) = self.bloom(t) else { return };
        let name = BRANCH_NAMES[k];
        let mut items = vec![
            (
                format!("Make {name} the seed"),
                MenuAction::Bloom(t, BloomOp::Replant),
            ),
            (
                format!("Clone {name} to every branch"),
                MenuAction::Bloom(t, BloomOp::Clone),
            ),
            (
                format!("Cross {name} with another branch…"),
                MenuAction::Bloom(t, BloomOp::Cross),
            ),
            (
                format!("Cut {name} back to the seed"),
                MenuAction::Bloom(t, BloomOp::ResetBranch(k)),
            ),
        ];
        if b.keys == BloomKeys::Layer {
            let label = match b.layered(k) {
                true => format!("Take {name} out of the layer"),
                false => format!("Put {name} in the layer"),
            };
            items.push((label, MenuAction::Bloom(t, BloomOp::Layer(k))));
        }
        items.push((
            format!("Automate {name}'s length"),
            MenuAction::Bloom(t, BloomOp::Automate(branch_length(k))),
        ));
        // Whatever's done, it's to this one.
        self.bloom_op(t, BloomOp::Pick(k), cx);
        self.bloom_menu(at, items, cx);
    }

    fn bloom_seed_menu(&mut self, t: usize, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let kind = self.bloom_view.as_ref().map_or(SeedKind::Any, |v| v.kind);
        let items = vec![
            (
                format!("Plant a new {} seed", kind.name().to_lowercase()),
                MenuAction::Bloom(t, BloomOp::Plant),
            ),
            (
                "Nudge the seed a little".into(),
                MenuAction::Bloom(t, BloomOp::Mutate),
            ),
            (
                "Clone the picked branch to them all".into(),
                MenuAction::Bloom(t, BloomOp::Clone),
            ),
        ];
        self.bloom_menu(at, items, cx);
    }
}

/// The house mark: a small flower.
pub(crate) fn emblem(size_px: f32) -> Div {
    let canvas = crate::ui::scale::scaled_canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let r = f32::from(b.size.width) / 2.0;
            let mut p = PathBuilder::fill();
            for i in 0..6 {
                let a = i as f32 * TAU / 6.0 - FRAC_PI_2;
                petal(
                    &mut p,
                    (f32::from(c.x), f32::from(c.y)),
                    a,
                    r * 0.95,
                    r * 0.42,
                );
            }
            if let Ok(path) = p.build() {
                window.paint_path(
                    path,
                    linear_gradient(
                        135.0,
                        linear_color_stop(branch_hue(0, 0.7, 1.0), 0.0),
                        linear_color_stop(branch_hue(5, 0.62, 1.0), 1.0),
                    ),
                );
            }
            dot(
                window,
                (f32::from(c.x), f32::from(c.y)),
                r * 0.26,
                hsla(0.12, 0.9, 0.85, 1.0),
            );
        },
    )
    .size_full();
    div().size(px(size_px)).flex_none().child(canvas)
}

/// A sound's portrait, for its card: its flower as its genes draw it (petals from its
/// wavetable, their length from its brightness, their width from its attack), on a stem, its
/// genes in a ring round it, in its family's `hue`.
pub(crate) fn paint_portrait(
    window: &mut gpui::Window,
    b: Bounds<Pixels>,
    c: (f32, f32),
    g: &[f32; GENES],
    hue: f32,
) {
    let unit = crate::ui::scale::scale();
    let h = f32::from(b.size.height);
    let bottom = f32::from(b.origin.y) + h;
    glow(window, c, h * 0.16, h * 0.55, hsla(hue, 0.6, 0.42, 0.35));
    // The stem, leaning as the sound's balance does.
    let base = (c.0 + (g[22] - 0.5) * 30.0 * unit, bottom + 2.0);
    let bend = (g[41] - 0.5) * 50.0 * unit;
    let mut stem = PathBuilder::stroke(gpui::px(2.2 * unit));
    let pt = |(x, y): (f32, f32)| gpui::point(gpui::px(x), gpui::px(y));
    stem.move_to(pt(base));
    stem.curve_to(
        pt(c),
        pt(((base.0 + c.0) / 2.0 + bend, (base.1 + c.1) / 2.0)),
    );
    if let Ok(p) = stem.build() {
        window.paint_path(p, hsla(hue, 0.4, 0.4, 0.9));
    }
    let mid = ((base.0 + c.0) / 2.0 + bend * 0.5, (base.1 + c.1) / 2.0);
    let mut leaves = PathBuilder::fill();
    petal(&mut leaves, mid, -0.6 - FRAC_PI_2 * 0.5, h * 0.16, 0.38);
    petal(&mut leaves, mid, PI + 0.6 + FRAC_PI_2 * 0.5, h * 0.13, 0.38);
    if let Ok(p) = leaves.build() {
        window.paint_path(p, hsla(hue, 0.4, 0.34, 0.85));
    }
    // Its genes round it.
    let ring = h * 0.36;
    for (i, v) in g.iter().enumerate() {
        let a = -FRAC_PI_2 + (i as f32 + 0.5) * TAU / GENES as f32;
        let r = ring + v * 7.0 * unit;
        dot(
            window,
            (c.0 + r * a.cos(), c.1 + r * a.sin()),
            1.1 * unit,
            hsla(hue, 0.5, 0.7, 0.25 + 0.6 * v),
        );
    }
    let f = flower_of(g, 0.0, 0);
    let long = h * (0.16 + 0.12 * g[32]);
    let mut petals = PathBuilder::fill();
    for i in 0..f.petals {
        let a = g[9] * TAU + i as f32 * TAU / f.petals as f32;
        petal(&mut petals, c, a, long, f.wide);
    }
    if let Ok(p) = petals.build() {
        window.paint_path(
            p,
            linear_gradient(
                180.0,
                linear_color_stop(hsla(hue, 0.7, 0.72, 0.95), 0.0),
                linear_color_stop(hsla(hue, 0.65, 0.5, 0.95), 1.0),
            ),
        );
    }
    dot(window, c, long * 0.26, hsla(0.13, 0.85, 0.72, 1.0));
}

/// A small flower for the family strip: its petals from `g`, in branch `k`'s colour, ringed when
/// it's the seed now.
pub(crate) fn paint_mini(
    window: &mut gpui::Window,
    b: Bounds<Pixels>,
    g: &[f32; GENES],
    k: usize,
    now: bool,
) {
    let unit = crate::ui::scale::scale();
    let c = (
        f32::from(b.origin.x) + f32::from(b.size.width) / 2.0,
        f32::from(b.origin.y) + f32::from(b.size.height) / 2.0,
    );
    let r = f32::from(b.size.width) / 2.0;
    window.paint_quad(
        gpui::fill(
            Bounds::centered_at(
                gpui::point(gpui::px(c.0), gpui::px(c.1)),
                size(gpui::px(2.0 * r), gpui::px(2.0 * r)),
            ),
            match now {
                true => hsla(0.36, 0.25, 0.14, 1.0),
                false => hsla(0.4, 0.12, 0.09, 1.0),
            },
        )
        .corner_radii(gpui::px(r))
        .border_widths(gpui::px(if now { 1.5 * unit } else { 1.0 * unit }))
        .border_color(match now {
            true => hsla(0.38, 0.5, 0.66, 1.0),
            false => hsla(0.4, 0.15, 0.25, 1.0),
        }),
    );
    let f = flower_of(g, 0.0, k);
    let mut petals = PathBuilder::fill();
    for i in 0..f.petals {
        let a = i as f32 * TAU / f.petals as f32 - FRAC_PI_2;
        petal(&mut petals, c, a, r * 0.72, f.wide);
    }
    if let Ok(p) = petals.build() {
        window.paint_path(p, branch_hue(k, 0.64, 1.0));
    }
    dot(window, c, r * 0.2, hsla(0.13, 0.85, 0.75, 1.0));
}

/// A find on the Grow target: a small flower drawn from its genes (petals from its wavetable,
/// their width from its attack, turned by its LFO), `r` across, in the strand's `hue`.
pub(crate) fn paint_find(
    window: &mut gpui::Window,
    c: (f32, f32),
    r: f32,
    g: &[f32; GENES],
    hue: f32,
    alpha: f32,
) {
    let f = flower_of(g, 0.0, 0);
    let mut petals = PathBuilder::fill();
    for i in 0..f.petals {
        let a = -FRAC_PI_2 + g[9] * TAU + i as f32 * TAU / f.petals as f32;
        petal(&mut petals, c, a, r, f.wide.max(0.42));
    }
    if let Ok(p) = petals.build() {
        window.paint_path(
            p,
            linear_gradient(
                180.0,
                linear_color_stop(hsla(hue, 0.75, 0.74, alpha), 0.0),
                linear_color_stop(hsla(hue, 0.7, 0.5, alpha), 1.0),
            ),
        );
    }
    dot(window, c, r * 0.3, hsla(0.13, 0.85, 0.74, alpha));
}

/// A petal or a leaf from `at`, pointing `a` (radians), `long` and `wide` (its width over its
/// length), added to `p`.
fn petal(p: &mut PathBuilder, at: (f32, f32), a: f32, long: f32, wide: f32) {
    let (dx, dy) = (a.cos(), a.sin());
    let (nx, ny) = (-dy, dx);
    let pt = |x: f32, y: f32| gpui::point(gpui::px(x), gpui::px(y));
    let tip = (at.0 + dx * long, at.1 + dy * long);
    let w = long * wide;
    let mid = (at.0 + dx * long * 0.45, at.1 + dy * long * 0.45);
    p.move_to(pt(at.0, at.1));
    p.curve_to(pt(tip.0, tip.1), pt(mid.0 + nx * w, mid.1 + ny * w));
    p.curve_to(pt(at.0, at.1), pt(mid.0 - nx * w, mid.1 - ny * w));
    p.close();
}

fn dot(window: &mut gpui::Window, c: (f32, f32), r: f32, color: Hsla) {
    window.paint_quad(
        gpui::fill(
            Bounds::centered_at(
                gpui::point(gpui::px(c.0), gpui::px(c.1)),
                size(gpui::px(2.0 * r), gpui::px(2.0 * r)),
            ),
            color,
        )
        .corner_radii(gpui::px(r)),
    );
}

fn glow(window: &mut gpui::Window, c: (f32, f32), r: f32, blur: f32, color: Hsla) {
    let bounds = Bounds::centered_at(
        gpui::point(gpui::px(c.0), gpui::px(c.1)),
        size(gpui::px(2.0 * r), gpui::px(2.0 * r)),
    );
    window.paint_drop_shadows(
        bounds,
        Corners::all(gpui::px(r)),
        &[BoxShadow {
            color,
            offset: gpui::point(gpui::px(0.), gpui::px(0.)),
            blur_radius: gpui::px(blur),
            spread_radius: gpui::px(r * 0.3),
            inset: false,
        }],
    );
}

/// A CSS gradient's angle (0 up, clockwise, degrees) for a direction on screen.
fn css_angle(dx: f32, dy: f32) -> f32 {
    dx.atan2(-dy).to_degrees().rem_euclid(360.0)
}

fn paint_garden(window: &mut gpui::Window, b: Bounds<Pixels>, s: &Scene) {
    let half = f32::from(b.size.width) / 2.0;
    let (cx0, cy0) = (f32::from(b.origin.x) + half, f32::from(b.origin.y) + half);
    let at = |(x, y): (f32, f32)| (cx0 + x * half, cy0 + y * half);
    let pt = |(x, y): (f32, f32)| gpui::point(gpui::px(x), gpui::px(y));
    let unit = crate::ui::scale::scale();
    let chosen_hue = |l: f32, a: f32| branch_hue(s.chosen, l, a);

    // The ground: a glow of the picked branch's colour under the seed, growth rings, spokes.
    glow(
        window,
        (cx0, cy0),
        half * 0.36,
        half * 0.7,
        chosen_hue(0.35, 0.18 + 0.12 * s.sounding),
    );
    let mut rings = PathBuilder::stroke(gpui::px(1.0 * unit));
    for r in [0.32f32, 0.48, 0.64, TIP] {
        circle_path(&mut rings, (cx0, cy0), r * half, 72);
    }
    if let Ok(p) = rings.build() {
        window.paint_path(p, hsla(0.42, 0.25, 0.5, 0.06));
    }
    let mut spokes = PathBuilder::stroke(gpui::px(1.0 * unit));
    for k in 0..BRANCHES {
        let a = angle(k);
        spokes.move_to(pt(at((BASE * a.cos(), BASE * a.sin()))));
        spokes.line_to(pt(at((RING * 0.9 * a.cos(), RING * 0.9 * a.sin()))));
    }
    if let Ok(p) = spokes.build() {
        window.paint_path(p, hsla(0.42, 0.2, 0.5, 0.035));
    }
    // The ring of notes: the picked one's arc lit.
    let mut ring = PathBuilder::stroke(gpui::px(1.0 * unit));
    circle_path(&mut ring, (cx0, cy0), RING * half - 14.0 * unit, 120);
    if let Ok(p) = ring.build() {
        window.paint_path(p, hsla(0.0, 0.0, 1.0, 0.05));
    }
    let a0 = angle(s.chosen);
    let mut arc = PathBuilder::stroke(gpui::px(2.5 * unit));
    for i in 0..=16 {
        let a = a0 - 0.22 + 0.44 * i as f32 / 16.0;
        let r = RING * half - 14.0 * unit;
        let p = pt((cx0 + r * a.cos(), cy0 + r * a.sin()));
        if i == 0 {
            arc.move_to(p);
        } else {
            arc.line_to(p);
        }
    }
    if let Ok(p) = arc.build() {
        window.paint_path(p, chosen_hue(0.62, 0.8));
    }
    // How far the growth and the wheel take everything.
    if s.extra > 0.01 {
        let r = (BASE + s.extra * (TIP - BASE)) * half;
        let mut g = PathBuilder::stroke(gpui::px(1.2 * unit))
            .dash_array(&[gpui::px(3.0 * unit), gpui::px(5.0 * unit)]);
        circle_path(&mut g, (cx0, cy0), r, 96);
        if let Ok(p) = g.build() {
            window.paint_path(p, hsla(0.3, 0.6, 0.7, 0.28));
        }
    }

    // Pollen drifting out from the seed.
    if s.awake {
        for i in 0..42u32 {
            let h = mix32(i ^ 0x2545_f491);
            let speed = 0.025 + 0.05 * unit_of(h);
            let life = (s.t * speed + unit_of(mix32(h ^ 1))).fract();
            let a = TAU * unit_of(mix32(h ^ 2)) + 0.4 * (s.t * 0.1 + i as f32).sin();
            let r = BASE * 0.8 + life * (RING - BASE * 0.8);
            let fade = (PI * life).sin() * (0.18 + 0.5 * s.sounding);
            let size = (1.0 + 1.6 * unit_of(mix32(h ^ 3))) * unit;
            dot(
                window,
                at((r * a.cos(), r * a.sin())),
                size,
                hsla(0.14, 0.5, 0.85, fade),
            );
        }
    }

    // The branches, furthest-reaching last so their flowers sit on top.
    let mut order: Vec<&Drawn> = s.branches.iter().collect();
    order.sort_by(|a, b| {
        let key = |d: &Drawn| (d.k == s.chosen, d.hover, (d.reach * 1000.0) as i32);
        key(a).cmp(&key(b))
    });
    for d in order {
        if d.reach < 0.004 {
            // A bud on the seed, where it'll grow from.
            let a = angle(d.k);
            dot(
                window,
                at((BASE * a.cos(), BASE * a.sin())),
                2.5 * unit,
                branch_hue(d.k, 0.6, 0.5),
            );
            continue;
        }
        let picked = d.k == s.chosen;
        let lift = if d.hover || picked { 0.1 } else { 0.0 };
        let pts: Vec<(f32, f32)> = d.spine.iter().map(|&p| at(p)).collect();
        // The stem, tapering, darker at its root.
        let thick = (2.2 + 3.2 * d.reach.min(1.0)) * unit;
        let n = pts.len();
        let mut left = Vec::with_capacity(n);
        let mut right = Vec::with_capacity(n);
        for i in 0..n {
            let (a, c) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
            let (dx, dy) = (c.0 - a.0, c.1 - a.1);
            let l = dx.hypot(dy).max(1e-3);
            let (nx, ny) = (-dy / l, dx / l);
            let w = thick * (1.0 - 0.7 * i as f32 / (n - 1) as f32) * 0.5;
            left.push((pts[i].0 + nx * w, pts[i].1 + ny * w));
            right.push((pts[i].0 - nx * w, pts[i].1 - ny * w));
        }
        let mut stem = PathBuilder::fill();
        stem.move_to(pt(left[0]));
        for &p in &left[1..] {
            stem.line_to(pt(p));
        }
        for &p in right.iter().rev() {
            stem.line_to(pt(p));
        }
        stem.close();
        let (dx, dy) = (pts[n - 1].0 - pts[0].0, pts[n - 1].1 - pts[0].1);
        if let Ok(p) = stem.build() {
            window.paint_path(
                p,
                linear_gradient(
                    css_angle(dx, dy),
                    linear_color_stop(branch_hue(d.k, 0.28 + lift, 0.85), 0.0),
                    linear_color_stop(branch_hue(d.k, 0.55 + lift, 0.95), 1.0),
                ),
            );
        }
        // Leaves: bigger where the branch has strayed further from the seed.
        let mut leaves = PathBuilder::fill();
        for &(i, side, change) in &d.leaves {
            let (a, c) = (pts[i], pts[(i + 1).min(n - 1)]);
            let dir = (c.1 - a.1).atan2(c.0 - a.0) + side * 0.95;
            let long = (0.028 + 0.05 * change) * half * (0.7 + 0.3 * d.reach.min(1.0));
            petal(&mut leaves, a, dir, long, 0.36);
        }
        if let Ok(p) = leaves.build() {
            window.paint_path(p, branch_hue(d.k, 0.42 + lift, 0.75));
        }
        // Sap running up it while it plays.
        if d.glow > 0.01 {
            let mut lit = PathBuilder::stroke(gpui::px(1.6 * unit));
            lit.move_to(pt(pts[0]));
            for &p in &pts[1..] {
                lit.line_to(pt(p));
            }
            if let Ok(p) = lit.build() {
                window.paint_path(p, branch_hue(d.k, 0.82, 0.55 * d.glow));
            }
            for j in 0..4 {
                let f = (s.t * 0.9 + j as f32 / 4.0).fract();
                let p = pts[((f * (n - 1) as f32) as usize).min(n - 1)];
                dot(
                    window,
                    p,
                    2.2 * unit,
                    branch_hue(d.k, 0.9, d.glow * (1.0 - f * 0.6)),
                );
            }
        }
        // Its length alone, where the growth takes it further: a small mark.
        if let Some(bt) = d.base_tip {
            dot(window, at(bt), 2.0 * unit, branch_hue(d.k, 0.75, 0.6));
        }
        // The flower.
        let tip = pts[n - 1];
        let f = d.flower;
        let swell = f.breath * (1.0 + 0.32 * d.glow) * (0.65 + 0.35 * d.reach.min(1.0));
        let long = f.long * half * swell;
        if d.glow > 0.01 || picked {
            let strength = (d.glow * 0.85).max(if picked { 0.25 } else { 0.0 });
            glow(
                window,
                tip,
                long * 0.55,
                long * (1.4 + d.glow),
                branch_hue(d.k, 0.6, strength),
            );
        }
        let mut petals = PathBuilder::fill();
        for i in 0..f.petals {
            let a = f.spin + i as f32 * TAU / f.petals as f32;
            petal(&mut petals, tip, a, long, f.wide);
        }
        if let Ok(p) = petals.build() {
            window.paint_path(
                p,
                linear_gradient(
                    css_angle(dx, dy),
                    linear_color_stop(branch_hue(d.k, 0.5 + lift, 0.92), 0.0),
                    linear_color_stop(branch_hue(d.k, 0.78 + lift * 0.5, 0.95), 1.0),
                ),
            );
        }
        let heart = long * 0.26;
        dot(
            window,
            tip,
            heart,
            hsla(0.13, 0.85, 0.72 + 0.2 * d.glow, 1.0),
        );
        if d.layered {
            let mut o = PathBuilder::stroke(gpui::px(1.2 * unit));
            circle_path(&mut o, tip, long * 1.25, 32);
            if let Ok(p) = o.build() {
                window.paint_path(p, branch_hue(d.k, 0.8, 0.6));
            }
        }
        if picked || d.hover || s.crossing {
            let mut o = PathBuilder::stroke(gpui::px(1.5 * unit));
            circle_path(&mut o, tip, long * 1.08 + 3.0 * unit, 40);
            if let Ok(p) = o.build() {
                let a = if picked { 0.9 } else { 0.45 };
                window.paint_path(p, hsla(0.0, 0.0, 1.0, a));
            }
        }
    }

    // The seed: its genes round it (the picked branch's beside them, lines where they've moved),
    // and the orb, breathing with what plays.
    let ring_at = |i: usize, g: f32| {
        let a = -FRAC_PI_2 + (i as f32 + 0.5) * TAU / GENES as f32;
        let r = GENOME_IN + g.clamp(0.0, 1.0) * (GENOME_OUT - GENOME_IN);
        at((r * a.cos(), r * a.sin()))
    };
    let tints = [0.09f32, 0.52, 0.8];
    let mut moved = PathBuilder::stroke(gpui::px(1.2 * unit));
    let mut any = false;
    for i in 0..GENES {
        if (s.picked[i] - s.seed[i]).abs() > 0.03 {
            moved.move_to(pt(ring_at(i, s.seed[i])));
            moved.line_to(pt(ring_at(i, s.picked[i])));
            any = true;
        }
    }
    if any && let Ok(p) = moved.build() {
        window.paint_path(p, chosen_hue(0.7, 0.7));
    }
    let mut track_ring = PathBuilder::stroke(gpui::px(1.0 * unit));
    circle_path(&mut track_ring, (cx0, cy0), GENOME_IN * half, 64);
    circle_path(&mut track_ring, (cx0, cy0), GENOME_OUT * half, 64);
    if let Ok(p) = track_ring.build() {
        window.paint_path(p, hsla(0.0, 0.0, 1.0, 0.05));
    }
    for i in 0..GENES {
        let group = i / 16;
        let locked = s.locks >> i & 1 == 1;
        let c = match locked {
            true => hsla(0.0, 0.0, 0.55, 0.9),
            false => hsla(tints[group], 0.55, 0.72, 0.9),
        };
        dot(window, ring_at(i, s.seed[i]), 1.7 * unit, c);
        if (s.picked[i] - s.seed[i]).abs() > 0.03 {
            dot(
                window,
                ring_at(i, s.picked[i]),
                2.1 * unit,
                chosen_hue(0.72, 1.0),
            );
        }
    }
    let breathe =
        1.0 + 0.05 * (s.t * 1.3).sin() * if s.awake { 1.0 } else { 0.0 } + 0.12 * s.sounding;
    let r = SEED_R * half * breathe;
    glow(
        window,
        (cx0, cy0),
        r * 0.8,
        r * (1.6 + 1.5 * s.sounding),
        hsla(0.12, 0.9, 0.6, 0.35 + 0.4 * s.sounding),
    );
    window.paint_quad(
        gpui::fill(
            Bounds::centered_at(pt((cx0, cy0)), size(gpui::px(2.0 * r), gpui::px(2.0 * r))),
            linear_gradient(
                160.0,
                linear_color_stop(hsla(0.13, 0.9, 0.78, 1.0), 0.0),
                linear_color_stop(hsla(0.06, 0.75, 0.42, 1.0), 1.0),
            ),
        )
        .corner_radii(gpui::px(r)),
    );
    // A seed's crease.
    let mut crease = PathBuilder::stroke(gpui::px(1.4 * unit));
    crease.move_to(pt((cx0 - r * 0.15, cy0 - r * 0.62)));
    crease.curve_to(
        pt((cx0 - r * 0.1, cy0 + r * 0.62)),
        pt((cx0 + r * 0.35, cy0)),
    );
    if let Ok(p) = crease.build() {
        window.paint_path(p, hsla(0.07, 0.6, 0.3, 0.55));
    }
}

fn unit_of(h: u32) -> f32 {
    unit(h)
}

fn circle_path(p: &mut PathBuilder, c: (f32, f32), r: f32, n: usize) {
    for i in 0..=n {
        let a = i as f32 * TAU / n as f32;
        let q = gpui::point(gpui::px(c.0 + r * a.cos()), gpui::px(c.1 + r * a.sin()));
        if i == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_is_found_and_dragged_where_it_is_drawn() {
        let mut b = Bloom::default();
        for k in 0..BRANCHES {
            b.params[branch_length(k)] = 0.6;
            b.params[branch_bend(k)] = 0.3;
        }
        let shown = [0.6; BRANCHES];
        for k in 0..BRANCHES {
            let s = spine(k, b.paths[k], 0.6, 0.3, 0.0, 1);
            let (x, y) = s[1];
            assert_eq!(branch_at(&b, &shown, x, y), Some(k), "branch {k}");
            // Dragged to where its tip is, it's where it was.
            let (reach, bend) = reach_and_bend(k, x, y);
            assert!(
                (reach - 0.6).abs() < 1e-3 && (bend - 0.3).abs() < 1e-3,
                "{k}: {reach} {bend}"
            );
        }
        assert_eq!(branch_at(&b, &shown, 0.0, 0.0), None, "the seed");
        // C points straight up.
        let (x, y) = spine(0, 1, 1.0, 0.0, 0.0, 1)[1];
        assert!(x.abs() < 1e-4 && y < -0.7);
    }

    #[test]
    fn each_branch_has_its_own_colour() {
        let all: Vec<u32> = (0..BRANCHES).map(hue_u32).collect();
        let mut seen = all.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), BRANCHES);
    }
}
