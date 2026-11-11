//! The Strand: every gene of the seed down a double helix, turning slowly, each rung its gene
//! (a bead where its value is, the picked branch's beside it in the branch's colour), its name
//! and value beside it, and a lock that keeps it from ever straying. Its three parts open as
//! knobs on the right: the seed's genes (automatable, like any knob), or, steering, what the
//! picked branch grows each gene to at its tip.

use std::f32::consts::PI;

use gpui::{PathBuilder, hsla};

use super::*;

const ROW_H: f32 = 11.6;
const GROUP_GAP: f32 = 10.0;
const NAMES_W: f32 = 132.0;
const HELIX_W: f32 = 200.0;
const VALUES_W: f32 = 176.0;
const TINTS: [f32; 3] = [0.09, 0.52, 0.8];

fn row_y(i: usize) -> f32 {
    14.0 + i as f32 * ROW_H + (i / 16) as f32 * GROUP_GAP
}

impl Workspace {
    pub(crate) fn bloom_strand_page(&mut self, t: usize, b: &Bloom, cx: &mut Context<Self>) -> Div {
        let Some(v) = self.bloom_view.as_ref() else {
            return div();
        };
        let (group, steer, hover) = (v.group, v.steer, v.gene_hover);
        let k = b.chosen as usize % BRANCHES;
        let seed = b.seed();
        let branch = b.branch_genes(k, b.reach(k, 0.0));
        let time = self.bloom_time();
        let (helix_seed, helix_branch, locks) = (seed, branch, b.locks);
        let picked_hue = k;
        let helix = crate::ui::scale::scaled_canvas(
            |_, _, _| {},
            move |bb, _, window, _| {
                paint_helix(
                    window,
                    bb,
                    time,
                    &helix_seed,
                    &helix_branch,
                    locks,
                    picked_hue,
                    group,
                )
            },
        )
        .absolute()
        .top_0()
        .left(px(NAMES_W))
        .w(px(HELIX_W))
        .h_full();
        let rows = (0..GENES).map(|i| {
            let g = &GENE_TABLE[i];
            let locked = b.locked(i);
            let on = hover == Some(i);
            let in_group = g.group == group;
            let steered = b
                .edits
                .iter()
                .any(|e| e.branch as usize == k && e.gene as usize == i);
            let y = row_y(i);
            let (s, br) = (seed[i], branch[i]);
            div()
                .absolute()
                .left_0()
                .top(px(y - ROW_H / 2.0))
                .w(px(NAMES_W + HELIX_W + VALUES_W))
                .h(px(ROW_H))
                .flex()
                .items_center()
                .rounded(px(3.))
                .when(on, |d| d.bg(alpha(0xffffff, 0.05)))
                .debug_selector(move || format!("bloom-gene-{}", GENE_TABLE[i].id))
                .child(
                    div()
                        .w(px(NAMES_W - 8.0))
                        .flex()
                        .justify_end()
                        .text_size(px(10.))
                        .text_color(rgb(match (in_group, on) {
                            (_, true) => TEXT,
                            (true, _) => TEXT_2,
                            _ => TEXT_4,
                        }))
                        .whitespace_nowrap()
                        .child(g.name),
                )
                .child(div().w(px(HELIX_W + 8.0)))
                .child(
                    crate::ui::scale::scaled_canvas(
                        |_, _, _| {},
                        move |bb, _, window, _| {
                            let w = f32::from(bb.size.width);
                            let mid = bb.origin.y + bb.size.height / 2.0;
                            let x = |v: f32| bb.origin.x + gpui::px(w * v.clamp(0.0, 1.0));
                            window.paint_quad(gpui::fill(
                                Bounds::new(
                                    point(bb.origin.x, mid - gpui::px(1.)),
                                    gpui::size(bb.size.width, gpui::px(2.)),
                                ),
                                rgb(LINE),
                            ));
                            window.paint_quad(
                                gpui::fill(
                                    Bounds::new(
                                        point(bb.origin.x, mid - gpui::px(1.5)),
                                        gpui::size(gpui::px(w * s), gpui::px(3.)),
                                    ),
                                    hsla(TINTS[i / 16], 0.5, 0.6, if locked { 0.35 } else { 0.8 }),
                                )
                                .corner_radii(gpui::px(1.5)),
                            );
                            if (br - s).abs() > 0.01 {
                                window.paint_quad(
                                    gpui::fill(
                                        Bounds::centered_at(
                                            point(x(br), mid),
                                            gpui::size(gpui::px(6.), gpui::px(6.)),
                                        ),
                                        branch_hue(k, 0.7, 1.0),
                                    )
                                    .corner_radii(gpui::px(3.)),
                                );
                            }
                        },
                    )
                    .w(px(VALUES_W - 52.0))
                    .h(px(ROW_H)),
                )
                .child(
                    div()
                        .w(px(18.))
                        .flex()
                        .justify_center()
                        .text_size(px(9.))
                        .text_color(if steered {
                            branch_hue(k, 0.7, 1.0)
                        } else {
                            gpui::rgb(TEXT_4).into()
                        })
                        .child(if steered { "◆" } else { "" }),
                )
                .child(
                    div()
                        .w(px(26.))
                        .h(px(ROW_H))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(3.))
                        .cursor_pointer()
                        .text_size(px(9.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgb(if locked { AMBER } else { TEXT_4 }))
                        .hover(|d| d.bg(rgb(RAISED)))
                        .debug_selector(move || format!("bloom-lock-{}", GENE_TABLE[i].id))
                        .child(if locked { "LOCK" } else { "·" })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.bloom_op(t, BloomOp::Lock(i), cx);
                            }),
                        ),
                )
                .on_mouse_move(cx.listener(move |ws, _: &gpui::MouseMoveEvent, _, cx| {
                    if let Some(v) = ws.bloom_view.as_mut()
                        && v.gene_hover != Some(i)
                    {
                        v.gene_hover = Some(i);
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::Group(GENE_TABLE[i].group), cx);
                    }),
                )
        });
        let strand = div()
            .relative()
            .flex_none()
            .w(px(NAMES_W + HELIX_W + VALUES_W + 16.0))
            .h_full()
            .bg(gpui::rgb(0x090c0b))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(helix)
            .children(rows);
        let tabs = GeneGroup::ALL.map(|g| {
            seg(g.name(), g == group, 28.)
                .debug_selector(move || format!("bloom-group-{}", g.genes().start))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::Group(g), cx);
                    }),
                )
        });
        let steer_seg = |name: String, on: bool, to: bool, cx: &mut Context<Self>| {
            seg(name, on, 26.)
                .debug_selector(move || format!("bloom-steer-{to}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::Steer(to), cx);
                    }),
                )
        };
        let all_locked = group.genes().all(|i| b.locked(i));
        let knobs = group
            .genes()
            .map(|i| {
                let dial = match steer {
                    false => self.bloom_knob(
                        t,
                        gene_param(i),
                        Some(GENE_TABLE[i].name.into()),
                        40.0,
                        hue_tint(i),
                        cx,
                    ),
                    true => self.bloom_steer_knob(t, k, i, &branch, cx),
                };
                div()
                    .w(px(112.))
                    .py(px(8.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .rounded(px(10.))
                    .when(hover == Some(i), |d| d.bg(rgb(CARD)))
                    .on_mouse_move(cx.listener(move |ws, _: &gpui::MouseMoveEvent, _, cx| {
                        if let Some(v) = ws.bloom_view.as_mut()
                            && v.gene_hover != Some(i)
                        {
                            v.gene_hover = Some(i);
                            cx.notify();
                        }
                    }))
                    .child(dial)
                    .when(b.locked(i), |d| d.child(small("locked")))
            })
            .collect::<Vec<_>>();
        let about = hover.map_or_else(
            || {
                "Point at a gene to hear what it does. Locked genes never stray as branches grow."
                    .to_string()
            },
            |i| {
                let g = &GENE_TABLE[i];
                format!(
                    "{}: {} Seed {}, branch {} {}.",
                    g.name,
                    g.about,
                    g.shape.say(seed[i]),
                    BRANCH_NAMES[k],
                    g.shape.say(branch[i])
                )
            },
        );
        let name = BRANCH_NAMES[k];
        let editor = div()
            .flex_1()
            .min_w(px(0.))
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(tray().p(px(2.)).rounded(px(8.)).children(tabs))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        tray()
                            .p(px(2.))
                            .rounded(px(7.))
                            .child(steer_seg("The seed's genes".into(), !steer, false, cx))
                            .child(steer_seg(format!("Steer branch {name}"), steer, true, cx)),
                    )
                    .child(
                        button(if all_locked { "Unlock all" } else { "Lock all" }, all_locked)
                            .debug_selector(|| "bloom-lock-group".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.bloom_op(t, BloomOp::LockGroup(group, !all_locked), cx);
                                }),
                            ),
                    ),
            )
            .child(hint(match steer {
                false => "The seed: every branch grows from these. Automate any of them.".to_string(),
                true => format!(
                    "What branch {name} grows each gene to at its tip, whatever its way. Right-click a knob to let it wander again."
                ),
            }))
            .child(div().flex().flex_wrap().gap(px(8.)).children(knobs))
            .child(div().flex_1())
            .child(
                div()
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_2))
                    .child(about),
            );
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .child(strand)
            .child(editor)
    }

    /// A knob that steers branch `k`'s gene `i`: where it grows to at the tip.
    fn bloom_steer_knob(
        &self,
        t: usize,
        k: usize,
        i: usize,
        now: &[f32; GENES],
        cx: &mut Context<Self>,
    ) -> Div {
        let b = self.bloom(t);
        let tip = b
            .and_then(|b| {
                b.edits
                    .iter()
                    .find(|e| e.branch as usize == k && e.gene as usize == i)
            })
            .map(|e| e.tip);
        let value = tip.unwrap_or(now[i]);
        let g = &GENE_TABLE[i];
        let hue = hue_u32(k);
        div()
            .flex_none()
            .min_w(px(54.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .child(
                div()
                    .rounded_full()
                    .cursor_ns_resize()
                    .debug_selector(move || format!("bloom-steer-knob-{}", GENE_TABLE[i].id))
                    .child(knob(
                        value,
                        40.0,
                        if tip.is_some() { hue } else { TEXT_4 },
                        false,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.begin_drag(
                                Drag::BloomSteer {
                                    track: t,
                                    branch: k,
                                    gene: i,
                                    y: pt(e.position).y,
                                    value,
                                },
                                cx,
                            );
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.bloom_op(t, BloomOp::Unsteer(i), cx);
                        }),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(g.name),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .font_family(MONO)
                    .text_color(rgb(if tip.is_some() { TEXT } else { TEXT_3 }))
                    .child(g.shape.say(value)),
            )
    }

    /// A steering knob dragged: the branch's tip for that gene set.
    pub(crate) fn drag_bloom_steer(&mut self, t: usize, k: usize, i: usize, value: f32) {
        let Some(b) = self.bloom_mut(t) else { return };
        let tip = value.clamp(0.0, 1.0);
        match b
            .edits
            .iter_mut()
            .find(|e| e.branch as usize == k && e.gene as usize == i)
        {
            Some(e) => e.tip = tip,
            None => b.edits.push(BranchEdit {
                branch: k as u8,
                gene: i as u8,
                tip,
            }),
        }
    }
}

fn hue_tint(i: usize) -> u32 {
    match i / 16 {
        0 => AMBER,
        1 => TEAL,
        _ => VIOLET,
    }
}

/// The double helix, turning, each rung a gene.
#[allow(clippy::too_many_arguments)]
fn paint_helix(
    window: &mut gpui::Window,
    b: Bounds<Pixels>,
    t: f32,
    seed: &[f32; GENES],
    branch: &[f32; GENES],
    locks: u64,
    k: usize,
    group: GeneGroup,
) {
    let unit = crate::ui::scale::scale();
    let cx0 = f32::from(b.origin.x) + f32::from(b.size.width) / 2.0;
    let top = f32::from(b.origin.y);
    let amp = f32::from(b.size.width) * 0.36;
    let turn = t * 0.45;
    let pt = |x: f32, y: f32| gpui::point(gpui::px(x), gpui::px(y));
    let at = |i: f32| {
        let y = top + row_y(0) * unit + (i * ROW_H + (i / 16.0).floor() * GROUP_GAP) * unit;
        let phi = i * 0.36 + turn;
        (y, phi.sin(), phi.cos())
    };
    // The two backbones, as lines through every rung's ends and between.
    for side in [1.0f32, -1.0] {
        let mut p = PathBuilder::stroke(gpui::px(2.0 * unit));
        let steps = (GENES - 1) * 4;
        for s in 0..=steps {
            let i = s as f32 / 4.0;
            let (y, sn, _) = at(i);
            let q = pt(cx0 + side * amp * sn, y);
            if s == 0 {
                p.move_to(q);
            } else {
                p.line_to(q);
            }
        }
        if let Ok(path) = p.build() {
            let h = if side > 0.0 { 0.33 } else { 0.12 };
            window.paint_path(path, hsla(h, 0.45, 0.55, 0.55));
        }
    }
    for i in 0..GENES {
        let (y, sn, cs) = at(i as f32);
        let (x1, x2) = (cx0 + amp * sn, cx0 - amp * sn);
        let front = 0.55 + 0.45 * cs;
        let lit = GENE_TABLE[i].group == group;
        let tint = TINTS[i / 16];
        let base = if lit { 0.75 } else { 0.35 };
        window.paint_quad(gpui::fill(
            Bounds::new(
                pt(x1.min(x2), y - 0.6 * unit),
                gpui::size(gpui::px((x1 - x2).abs()), gpui::px(1.2 * unit)),
            ),
            hsla(tint, 0.4, 0.55, 0.25 * front * base),
        ));
        let bead = |v: f32| x2 + (x1 - x2) * v.clamp(0.0, 1.0);
        let r = (2.3 + 1.2 * front) * unit;
        let locked = locks >> i & 1 == 1;
        let c = match locked {
            true => hsla(0.0, 0.0, 0.6, front),
            false => hsla(tint, 0.6, 0.66, front * base + 0.2),
        };
        window.paint_quad(
            gpui::fill(
                Bounds::centered_at(
                    pt(bead(seed[i]), y),
                    gpui::size(gpui::px(2.0 * r), gpui::px(2.0 * r)),
                ),
                c,
            )
            .corner_radii(gpui::px(r)),
        );
        if (branch[i] - seed[i]).abs() > 0.01 {
            let r = r * 0.8;
            window.paint_quad(
                gpui::fill(
                    Bounds::centered_at(
                        pt(bead(branch[i]), y),
                        gpui::size(gpui::px(2.0 * r), gpui::px(2.0 * r)),
                    ),
                    branch_hue(k, 0.7, front),
                )
                .corner_radii(gpui::px(r)),
            );
        }
    }
    // A slow shimmer running down it, so it's plainly alive.
    let f = (t * 0.12).fract();
    let (y, _, _) = at(f * (GENES as f32 - 1.0));
    window.paint_quad(gpui::fill(
        Bounds::new(
            pt(cx0 - amp, y - 6.0 * unit),
            gpui::size(gpui::px(2.0 * amp), gpui::px(12.0 * unit)),
        ),
        hsla(0.3, 0.6, 0.7, 0.04 * (PI * f).sin()),
    ));
}
