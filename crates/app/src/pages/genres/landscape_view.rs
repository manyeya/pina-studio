//! Where genres sit among each other: the map.

use super::*;

/// A genre's song in miniature: a bar a section, as tall as it's loud.
pub(crate) fn ribbon(sections: &[Section], color: u32, w: f32, h: f32) -> Div {
    div()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .p(px(3.))
        .flex()
        .items_end()
        .gap(px(1.))
        .rounded(px(7.))
        .bg(alpha(color, 0.1))
        .children(sections.iter().map(|s| {
            div()
                .flex_grow(s.bars as f32)
                .flex_basis(px(0.))
                .h(gpui::relative(0.2 + 0.16 * s.energy as f32))
                .rounded(px(1.5))
                .bg(alpha(color, 0.35 + 0.13 * s.energy as f32))
        }))
}

/// The hero's art: the genre's song as a landscape, its sections' energy as hills, three
/// ridges deep, fading into the page.
pub(crate) fn landscape(b: Bounds<Pixels>, sections: &[Section], color: u32, window: &mut Window) {
    let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    // A soft wash of the genre's color behind it all.
    let mut wash = gpui::PathBuilder::fill();
    wash.move_to(point(px(x0), px(y0)));
    wash.line_to(point(px(x0 + w), px(y0)));
    wash.line_to(point(px(x0 + w), px(y0 + h)));
    wash.line_to(point(px(x0), px(y0 + h)));
    wash.close();
    if let Ok(p) = wash.build() {
        window.paint_path(p, alpha(color, 0.05));
    }
    if sections.is_empty() {
        return;
    }
    let total: u64 = sections.iter().map(|s| s.bars).sum::<u64>().max(1);
    // Energy along the song, eased from section to section.
    let level = |t: f32| -> f32 {
        let mut at = 0.0;
        let mut levels = Vec::with_capacity(sections.len());
        for s in sections {
            let span = s.bars as f32 / total as f32;
            levels.push((at, at + span, s.energy as f32 / 5.0));
            at += span;
        }
        let i = levels
            .iter()
            .position(|(a, b, _)| t >= *a && t <= *b)
            .unwrap_or(levels.len() - 1);
        let (a, b, e) = levels[i];
        let next = levels.get(i + 1).map_or(e, |l| l.2);
        let prev = if i > 0 { levels[i - 1].2 } else { e };
        let into = (t - a) / (b - a).max(1e-6);
        // Rise or fall over the edges of each section, flat across its middle.
        if into < 0.15 {
            let k = 0.5 - 0.5 * (std::f32::consts::PI * (0.5 + into / 0.3)).cos();
            prev + (e - prev) * k
        } else if into > 0.85 {
            let k = 0.5 - 0.5 * (std::f32::consts::PI * ((into - 0.85) / 0.3)).cos();
            e + (next - e) * k
        } else {
            e
        }
    };
    let ridge = |shift: f32, height: f32, wobble: f32, fade: f32, window: &mut Window| {
        let mut path = gpui::PathBuilder::fill();
        path.move_to(point(px(x0), px(y0 + h)));
        let n = 160;
        for k in 0..=n {
            let t = k as f32 / n as f32;
            let e = level((t + shift).clamp(0.0, 1.0));
            let ripple = (t * 23.0 + shift * 40.0).sin() * wobble + (t * 61.0).sin() * wobble * 0.4;
            let y = y0 + h - (0.12 + 0.78 * e) * h * height - ripple;
            path.line_to(point(px(x0 + t * w), px(y)));
        }
        path.line_to(point(px(x0 + w), px(y0 + h)));
        path.close();
        if let Ok(p) = path.build() {
            window.paint_path(p, alpha(color, fade));
        }
    };
    ridge(0.03, 0.95, 6.0, 0.07, window);
    ridge(-0.02, 0.8, 4.0, 0.12, window);
    ridge(0.0, 0.62, 2.0, 0.2, window);
    // The front ridge's edge, drawn.
    let mut edge = gpui::PathBuilder::stroke(px(1.5));
    let n = 160;
    for k in 0..=n {
        let t = k as f32 / n as f32;
        let e = level(t);
        let ripple = (t * 23.0).sin() * 2.0 + (t * 61.0).sin() * 0.8;
        let p = point(
            px(x0 + t * w),
            px(y0 + h - (0.12 + 0.78 * e) * h * 0.62 - ripple),
        );
        if k == 0 {
            edge.move_to(p);
        } else {
            edge.line_to(p);
        }
    }
    if let Ok(p) = edge.build() {
        window.paint_path(p, alpha(color, 0.75));
    }
    // Into the page: the bottom fades to its background.
    for k in 0..12 {
        let t = k as f32 / 12.0;
        let mut band = gpui::PathBuilder::fill();
        let top = y0 + h * (0.7 + 0.3 * t);
        band.move_to(point(px(x0), px(top)));
        band.line_to(point(px(x0 + w), px(top)));
        band.line_to(point(px(x0 + w), px(y0 + h)));
        band.line_to(point(px(x0), px(y0 + h)));
        band.close();
        if let Ok(p) = band.build() {
            window.paint_path(p, alpha(BG, 0.09));
        }
    }
}

impl Workspace {
    /// The song map: sections as blocks, as long as they are and as tall as they're loud; under
    /// them, which part plays in which, a cell each, clicked on and off.
    pub(crate) fn genre_map(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
        let picked = self.genres.section;
        let roles: Vec<String> = p.parts.iter().map(|x| x.role.clone()).collect();
        let blocks: Vec<_> = p
            .sections
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let on = picked == Some(i);
                let level = s.energy as f32 / 5.0;
                div()
                    .id(("section", i))
                    .flex_grow(s.bars as f32)
                    .flex_basis(px(0.))
                    .min_w(px(58.))
                    .h(px(92.))
                    .relative()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .cursor_pointer()
                    .debug_selector(move || format!("section-{i}"))
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .right_0()
                            .h(gpui::relative(0.18 + 0.82 * level))
                            .rounded_t(px(8.))
                            .rounded_b(px(3.))
                            .bg(alpha(color, 0.10 + 0.32 * level))
                            .border_1()
                            .border_color(if on {
                                rgb(color).into()
                            } else {
                                alpha(color, 0.25)
                            }),
                    )
                    .child(
                        div()
                            .relative()
                            .px(px(8.))
                            .pb(px(7.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(s.name.clone()),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.))
                                    .text_color(rgb(tint(color, 0.4)))
                                    .child(format!("{} bars", s.bars)),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.genres.section = if ws.genres.section == Some(i) {
                                None
                            } else {
                                Some(i)
                            };
                            ws.genre_fields(cx);
                            cx.notify();
                        }),
                    )
            })
            .collect();
        // The grid: a row a part, a cell a section.
        let grid: Vec<_> = roles
            .iter()
            .map(|role| {
                let rc = role_color(role);
                let cells: Vec<_> = p
                    .sections
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let plays = s.parts.contains(role);
                        let role = role.clone();
                        let sel = format!("cell-{role}-{i}");
                        div()
                            .id(gpui::SharedString::from(sel.clone()))
                            .flex_grow(s.bars as f32)
                            .flex_basis(px(0.))
                            .min_w(px(58.))
                            .h(px(20.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .when(plays, |d| d.bg(alpha(rc, 0.72)).hover(|d| d.bg(rgb(rc))))
                            .when(!plays, |d| {
                                d.border_1()
                                    .border_color(rgb(LINE))
                                    .hover(|d| d.bg(alpha(rc, 0.18)).border_color(alpha(rc, 0.5)))
                            })
                            .debug_selector(move || sel.clone())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    let role = role.clone();
                                    ws.edit_genre(false, cx, move |p| {
                                        if let Some(s) = p.sections.get_mut(i) {
                                            match s.parts.iter().position(|x| *x == role) {
                                                Some(k) => drop(s.parts.remove(k)),
                                                None => s.parts.push(role),
                                            }
                                        }
                                    })
                                }),
                            )
                    })
                    .collect();
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .w(px(84.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(rc)))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(TEXT_2))
                                    .child(role.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .gap(px(4.))
                            .children(cells),
                    )
            })
            .collect();
        let controls = picked
            .and_then(|i| p.sections.get(i).map(|s| (i, s.clone())))
            .map(|(i, s)| self.section_controls(i, &s, p.sections.len(), color, cx));
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                heading(
                    "Song map",
                    "Its sections, how hard each plays, and who plays in it",
                )
                .child(
                    div()
                        .id("add-section")
                        .h(px(26.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_2))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(OFF)))
                        .child(icon(Icon::Plus, 12.).text_color(rgb(TEXT_2)))
                        .child("Section")
                        .debug_selector(|| "add-section".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                let roles: Vec<String> = ws
                                    .genres
                                    .draft
                                    .as_ref()
                                    .map(|p| p.parts.iter().map(|x| x.role.clone()).collect())
                                    .unwrap_or_default();
                                ws.edit_genre(true, cx, |p| {
                                    p.sections.push(Section {
                                        name: format!("Section {}", p.sections.len() + 1),
                                        bars: 8,
                                        parts: roles,
                                        energy: 3,
                                    })
                                });
                                ws.genres.section =
                                    ws.genres.draft.as_ref().map(|p| p.sections.len() - 1);
                                ws.genre_fields(cx);
                            }),
                        ),
                ),
            )
            .child(
                div()
                    .p(px(14.))
                    .rounded(px(14.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap(px(4.))
                            .child(div().w(px(84.)).flex_none())
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .items_end()
                                    .gap(px(4.))
                                    .children(blocks),
                            ),
                    )
                    .child(div().h(px(6.)))
                    .children(grid)
                    .children(controls),
            )
    }
}
