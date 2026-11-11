//! The garden's rails: on the left the seed (what to plant it as, planting, nudging, and its
//! family, every seed it grew from), on the right the picked branch (how far it's grown and bent,
//! what it changed, and what to do with it).

use super::*;

/// A strip of a genome's genes, each a bar its value tall.
fn genome_strip(genes: Vec<f32>, color: gpui::Hsla) -> impl IntoElement {
    crate::ui::scale::scaled_canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let n = genes.len().max(1);
            let w = f32::from(b.size.width) / n as f32;
            let h = f32::from(b.size.height);
            for (i, g) in genes.iter().enumerate() {
                let tall = (0.15 + 0.85 * g.clamp(0.0, 1.0)) * h;
                window.paint_quad(gpui::fill(
                    Bounds::new(
                        point(
                            b.origin.x + gpui::px(i as f32 * w),
                            b.origin.y + gpui::px(h - tall),
                        ),
                        gpui::size(gpui::px((w - 0.6).max(0.6)), gpui::px(tall)),
                    ),
                    color.opacity(0.35 + 0.65 * g),
                ));
            }
        },
    )
    .w_full()
    .h(px(14.))
    .flex_none()
}

impl Workspace {
    /// Every kind to plant, by family, over the garden: picked, it's what the next seed is.
    pub(crate) fn bloom_kind_picker(&mut self, t: usize, cx: &mut Context<Self>) -> Option<Div> {
        let v = self.bloom_view.as_ref().filter(|v| v.picking)?;
        let now = v.kind;
        let item = |k: SeedKind, hue: f32, cx: &mut Context<Self>| {
            let on = k == now;
            div()
                .px(px(8.))
                .py(px(5.))
                .rounded(px(7.))
                .flex()
                .flex_col()
                .cursor_pointer()
                .when(on, |d| d.bg(gpui::hsla(hue, 0.55, 0.5, 0.2)))
                .hover(|d| d.bg(rgb(RAISED)))
                .debug_selector(move || format!("bloom-kind-{}", k.name().to_lowercase()))
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                        .child(k.name()),
                )
                .child(small(k.about()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::Kind(k), cx);
                        // Twice: planted at once.
                        if e.click_count == 2 {
                            ws.bloom_op(t, BloomOp::Plant, cx);
                        }
                    }),
                )
        };
        let columns: Vec<Div> = SeedFamily::ALL
            .iter()
            .map(|&f| {
                let hue = f.hue();
                div()
                    .w(px(124.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .px(px(8.))
                            .pb(px(4.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .size(px(7.))
                                    .rounded_full()
                                    .bg(gpui::hsla(hue, 0.65, 0.6, 1.0)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(gpui::hsla(hue, 0.45, 0.75, 1.0))
                                    .whitespace_nowrap()
                                    .child(f.name()),
                            ),
                    )
                    .children(
                        f.kinds()
                            .iter()
                            .map(|&k| item(k, hue, cx))
                            .collect::<Vec<_>>(),
                    )
            })
            .collect();
        let anything = item(SeedKind::Any, 0.33, cx);
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::PickKind, cx);
                    }),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(296.))
                        .top(px(HEAD_H + 16.0))
                        .p(px(14.))
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .rounded(px(14.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE_STRONG))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(section_title("Plant a seed of…"))
                                .child(small("Double-click to plant at once")),
                        )
                        .child(div().w(px(150.)).child(anything))
                        .child(div().flex().gap(px(6.)).children(columns)),
                ),
        )
    }

    /// The seed card, floating over the garden's top left: the seed's genes, what the next one's
    /// planted as, planting and nudging.
    pub(crate) fn bloom_seed_card(&mut self, t: usize, b: &Bloom, cx: &mut Context<Self>) -> Div {
        let kind = self.bloom_view.as_ref().map_or(SeedKind::Any, |v| v.kind);
        let picking = self.bloom_view.as_ref().is_some_and(|v| v.picking);
        let hue = kind.family().map_or(0.33, |f| f.hue());
        let locked = (0..GENES).filter(|&g| b.locked(g)).count();
        let picker = div()
            .p(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(11.))
            .border_1()
            .border_color(gpui::hsla(hue, 0.5, 0.5, if picking { 0.9 } else { 0.45 }))
            .bg(gpui::hsla(hue, 0.5, 0.45, 0.1))
            .cursor_pointer()
            .hover(|d| d.bg(gpui::hsla(hue, 0.5, 0.45, 0.18)))
            .debug_selector(|| "bloom-kind-picker".into())
            .child(
                div()
                    .size(px(10.))
                    .flex_none()
                    .rounded_full()
                    .bg(gpui::hsla(hue, 0.65, 0.6, 1.0)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(kind.name()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(match kind.family() {
                                Some(f) => format!("{} · {}", f.name(), kind.about()),
                                None => kind.about().to_string(),
                            }),
                    ),
            )
            .child(chevron())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.bloom_op(t, BloomOp::PickKind, cx);
                }),
            );
        let plant = match kind {
            SeedKind::Any => "Plant a new seed".to_string(),
            k => format!("Plant a {} seed", k.name().to_lowercase()),
        };
        card()
            .p(px(14.))
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(caps("Seed"))
                    .child(small(match locked {
                        0 => "no genes locked".to_string(),
                        1 => "1 gene locked".to_string(),
                        n => format!("{n} genes locked"),
                    })),
            )
            .child(genome_strip(
                b.seed().to_vec(),
                gpui::hsla(0.12, 0.8, 0.62, 1.0),
            ))
            .child(picker)
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(
                        button(plant, true)
                            .flex_1()
                            .h(px(32.))
                            .bg(alpha(0x3ddc97, 0.18))
                            .border_color(rgb(0x3f8a62))
                            .debug_selector(|| "bloom-plant".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.bloom_op(t, BloomOp::Plant, cx);
                                }),
                            ),
                    )
                    .child(
                        button("Nudge", false)
                            .h(px(32.))
                            .debug_selector(|| "bloom-nudge".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.bloom_op(t, BloomOp::Mutate, cx);
                                }),
                            ),
                    ),
            )
    }

    /// The family along the garden's foot: every seed it grew from on one vine, oldest left, the
    /// seed now at the end. Clicked, one is the seed again.
    pub(crate) fn bloom_family_card(&mut self, t: usize, b: &Bloom, cx: &mut Context<Self>) -> Div {
        let n = b.ancestors.len();
        let seed = b.seed();
        let flowers: Vec<Div> = b
            .ancestors
            .iter()
            .enumerate()
            .map(|(k, a)| {
                let back = n - k;
                let name = match a.name.is_empty() {
                    true => format!(
                        "Gen {}",
                        (b.generation as usize + 1).saturating_sub(back).max(1)
                    ),
                    false => a.name.clone(),
                };
                let genes: [f32; GENES] =
                    std::array::from_fn(|i| a.genes.get(i).copied().unwrap_or(0.5));
                family_flower(genes, false, name)
                    .debug_selector(move || format!("bloom-ancestor-{k}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.bloom_op(t, BloomOp::GoBack(k), cx);
                        }),
                    )
            })
            .chain(std::iter::once(family_flower(seed, true, "Now".into())))
            .collect();
        card()
            .px(px(14.))
            .py(px(12.))
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(caps("Family"))
                    .child(small(match n {
                        0 => "the seeds it grows from will gather here".to_string(),
                        _ => "click one to go back to it".to_string(),
                    })),
            )
            .child(
                div()
                    .id("bloom-family")
                    .relative()
                    .flex_1()
                    .overflow_x_scroll()
                    .flex()
                    .items_end()
                    .gap(px(14.))
                    .child(
                        div()
                            .absolute()
                            .left(px(14.))
                            .right(px(14.))
                            .top(px(14.))
                            .h(px(2.))
                            .bg(rgb(0x2f5a3b)),
                    )
                    .children(flowers),
            )
    }

    /// The picked branch's card, floating over the garden's right: how far it's grown and bent,
    /// what it changed, and what to do with it.
    pub(crate) fn bloom_branch_card(&mut self, t: usize, b: &Bloom, cx: &mut Context<Self>) -> Div {
        let k = b.chosen as usize % BRANCHES;
        let hue = hue_u32(k);
        let crossing = self.bloom_view.as_ref().is_some_and(|v| v.crossing);
        let reach = b.reach(k, 0.0);
        let seed = b.seed();
        let genes = b.branch_genes(k, reach);
        let mut changed: Vec<usize> = (0..GENES)
            .filter(|&i| (genes[i] - seed[i]).abs() > 0.01)
            .collect();
        changed.sort_by(|&a, &c| {
            (genes[c] - seed[c])
                .abs()
                .total_cmp(&(genes[a] - seed[a]).abs())
        });
        let rows: Vec<Div> = changed
            .iter()
            .take(7)
            .map(|&i| {
                let (from, to) = (seed[i], genes[i]);
                let g = &GENE_TABLE[i];
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .whitespace_nowrap()
                                    .child(g.name),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .whitespace_nowrap()
                                    .child(format!("{} → {}", g.shape.say(from), g.shape.say(to))),
                            ),
                    )
                    .child(change_bar(from, to, k))
            })
            .collect();
        let act =
            |label: String, sel: &'static str, op: BloomOp, on: bool, cx: &mut Context<Self>| {
                button(label, on)
                    .h(px(30.))
                    .debug_selector(move || sel.into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.bloom_op(t, op.clone(), cx);
                        }),
                    )
            };
        let name = BRANCH_NAMES[k];
        let line = describe(&genes);
        let value = |i: usize| BLOOM_PARAMS[i].say(b.params[i]);
        card()
            .p(px(16.))
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().size(px(12.)).rounded_full().bg(rgb(hue)))
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(format!("Branch {name}")),
                    )
                    .child(div().flex_1())
                    .child(chip(format!("{:.0}% grown", reach * 100.0), hue)),
            )
            .child(small(line))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(labelled("Length", value(branch_length(k))))
                    .child(self.bloom_slider(t, branch_length(k), hue, cx))
                    .child(labelled("Bend", value(branch_bend(k))))
                    .child(self.bloom_slider(t, branch_bend(k), hue, cx)),
            )
            .child(div().h(px(1.)).bg(rgb(LINE)))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(caps("What it changed"))
                    .child(small(format!("{} genes", changed.len()))),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .when(changed.is_empty(), |d| {
                        d.child(hint("Still the seed: drag its flower out to grow it."))
                    })
                    .children(rows),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(
                                act(
                                    "Make it the seed".into(),
                                    "bloom-replant",
                                    BloomOp::Replant,
                                    false,
                                    cx,
                                )
                                .flex_1(),
                            )
                            .child(
                                act(
                                    "Clone to all".into(),
                                    "bloom-clone",
                                    BloomOp::Clone,
                                    false,
                                    cx,
                                )
                                .flex_1(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(
                                act(
                                    if crossing {
                                        "Pick a partner…".into()
                                    } else {
                                        "Cross with…".into()
                                    },
                                    "bloom-cross",
                                    BloomOp::Cross,
                                    crossing,
                                    cx,
                                )
                                .flex_1(),
                            )
                            .child(
                                act(
                                    "Cut back".into(),
                                    "bloom-cut",
                                    BloomOp::ResetBranch(k),
                                    false,
                                    cx,
                                )
                                .flex_1(),
                            ),
                    )
                    .when(b.keys == BloomKeys::Layer, |d| {
                        d.child(act(
                            match b.layered(k) {
                                true => format!("{name} is in the layer"),
                                false => format!("Put {name} in the layer"),
                            },
                            "bloom-layer",
                            BloomOp::Layer(k),
                            b.layered(k),
                            cx,
                        ))
                    }),
            )
    }
}

/// A card floating over the garden: dark glass, a soft edge, a shadow under it.
fn card() -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .rounded(px(16.))
        .bg(gpui::hsla(0.4, 0.06, 0.09, 0.92))
        .border_1()
        .border_color(gpui::hsla(0.0, 0.0, 1.0, 0.08))
        .shadow_lg()
        .overflow_hidden()
}

/// A small heading in capitals.
fn caps(s: &'static str) -> Div {
    div()
        .text_size(px(10.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(s.to_uppercase())
}

fn chevron() -> impl IntoElement {
    crate::ui::scale::scaled_canvas(
        |_, _, _| {},
        |b, _, window, _| {
            let c = b.center();
            let s = f32::from(b.size.width) / 2.0;
            let mut p = gpui::PathBuilder::stroke(gpui::px(1.5 * crate::ui::scale::scale()));
            p.move_to(point(c.x - gpui::px(s * 0.5), c.y - gpui::px(s * 0.25)));
            p.line_to(point(c.x, c.y + gpui::px(s * 0.25)));
            p.line_to(point(c.x + gpui::px(s * 0.5), c.y - gpui::px(s * 0.25)));
            if let Ok(path) = p.build() {
                window.paint_path(path, rgb(TEXT_3));
            }
        },
    )
    .size(px(12.))
    .flex_none()
}

/// A name and its value either end of a line, over a slider.
fn labelled(name: &'static str, value: String) -> Div {
    div()
        .flex()
        .justify_between()
        .text_size(px(11.))
        .text_color(rgb(TEXT_3))
        .child(name)
        .child(div().font_family(MONO).text_color(rgb(TEXT_2)).child(value))
}

/// A gene's move: where the seed has it (a tick), where the branch has it (a dot), the way between
/// in the branch's colour.
fn change_bar(from: f32, to: f32, k: usize) -> impl IntoElement {
    crate::ui::scale::scaled_canvas(
        |_, _, _| {},
        move |bb, _, window, _| {
            let w = f32::from(bb.size.width);
            let mid = bb.origin.y + bb.size.height / 2.0;
            let x = |v: f32| bb.origin.x + gpui::px(w * v.clamp(0.0, 1.0));
            window.paint_quad(
                gpui::fill(
                    Bounds::new(
                        point(bb.origin.x, mid - gpui::px(1.5)),
                        gpui::size(bb.size.width, gpui::px(3.)),
                    ),
                    rgb(LINE_STRONG),
                )
                .corner_radii(gpui::px(1.5)),
            );
            let (lo, hi) = (from.min(to), from.max(to));
            window.paint_quad(
                gpui::fill(
                    Bounds::new(
                        point(x(lo), mid - gpui::px(2.)),
                        gpui::size(gpui::px((w * (hi - lo)).max(1.0)), gpui::px(4.)),
                    ),
                    branch_hue(k, 0.6, 0.9),
                )
                .corner_radii(gpui::px(2.)),
            );
            window.paint_quad(gpui::fill(
                Bounds::centered_at(point(x(from), mid), gpui::size(gpui::px(2.), gpui::px(9.))),
                rgb(TEXT_3),
            ));
            window.paint_quad(
                gpui::fill(
                    Bounds::centered_at(point(x(to), mid), gpui::size(gpui::px(8.), gpui::px(8.))),
                    branch_hue(k, 0.75, 1.0),
                )
                .corner_radii(gpui::px(4.)),
            );
        },
    )
    .w_full()
    .h(px(9.))
}

/// One of the family: a small flower drawn from its genes, its name under it.
fn family_flower(genes: [f32; GENES], now: bool, name: String) -> Div {
    let hue = ((genes
        .iter()
        .enumerate()
        .map(|(i, g)| g * (i as f32 + 1.0))
        .sum::<f32>()
        * 7.3) as usize)
        % BRANCHES;
    let flower = crate::ui::scale::scaled_canvas(
        |_, _, _| {},
        move |b, _, window, _| super::garden::paint_mini(window, b, &genes, hue, now),
    )
    .size(px(30.));
    div()
        .relative()
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(5.))
        .cursor_pointer()
        .child(flower)
        .child(
            div()
                .max_w(px(70.))
                .text_size(px(10.))
                .font_weight(if now {
                    gpui::FontWeight::SEMIBOLD
                } else {
                    gpui::FontWeight::MEDIUM
                })
                .text_color(rgb(if now { TEXT } else { TEXT_3 }))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(name),
        )
}
