//! The title bar: the synth's name, its track, the patch (stepped through or picked from the
//! sounds), a demo to hear it, the pages, the quality, and the way out. Dragged, the window moves.

use super::*;
use gpui::Focusable;

impl Workspace {
    pub(crate) fn poly_head(
        &self,
        t: usize,
        p: &Poly,
        page: PolyPage,
        cx: &mut Context<Self>,
    ) -> Div {
        let track = &self.session.project.tracks[t];
        let here = POLY_PATCHES.iter().position(|q| q.0 == p.patch);
        let (part, about) = here.map_or(("", ""), |k| (POLY_PATCHES[k].1, POLY_PATCHES[k].2));
        let patch = match p.patch.is_empty() {
            true => "Init".to_string(),
            false => p.patch.clone(),
        };
        let step = |by: isize, cx: &mut Context<Self>| {
            let n = POLY_PATCHES.len() as isize;
            let to = POLY_PATCHES[(here.unwrap_or(0) as isize + by).rem_euclid(n) as usize].0;
            div()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_color(rgb(TEXT_3))
                .hover(|d| d.bg(rgb(CARD)).text_color(rgb(TEXT)))
                .debug_selector(move || {
                    if by < 0 {
                        "poly-patch-back"
                    } else {
                        "poly-patch-next"
                    }
                    .into()
                })
                .child(
                    icon(if by < 0 { Icon::Back } else { Icon::Forward }, 14.)
                        .text_color(rgb(TEXT_2)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.poly_op(t, PolyOp::Patch(to), cx);
                    }),
                )
        };
        let browsing = self.poly_view.as_ref().is_some_and(|v| v.browsing);
        let demo = self.poly_view.as_ref().is_some_and(|v| v.demo.is_some());
        let tab = |name: &'static str, to: PolyPage, cx: &mut Context<Self>| {
            seg(name, page == to, 28.)
                .debug_selector(move || format!("poly-page-{}", name.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.poly_view.as_mut() {
                            v.page = to;
                        }
                        cx.notify();
                    }),
                )
        };
        let at = self.poly_view.as_ref().map_or((0.0, 0.0), |v| v.at);
        div()
            .flex_none()
            .h(px(HEAD_H))
            .pl(px(16.))
            .pr(px(10.))
            .flex()
            .items_center()
            .gap(px(12.))
            .border_b_1()
            .border_color(rgb(LINE))
            .cursor_grab()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.begin_drag(
                        Drag::PolyWindow {
                            from: pt(e.position),
                            start: at,
                            resize: false,
                        },
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(POLY_NAME.to_uppercase()),
            )
            .child(chip(track.name.clone(), TEXT_3))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(step(-1, cx))
                    .child(
                        div()
                            .h(px(32.))
                            .min_w(px(160.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .bg(rgb(if browsing { RAISED } else { WELL }))
                            .hover(|d| d.bg(rgb(CARD)))
                            .cursor_pointer()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .debug_selector(|| "poly-patch".into())
                            .child(patch)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, window, cx| {
                                    cx.stop_propagation();
                                    if let Some(v) = ws.poly_view.as_mut() {
                                        v.browsing ^= true;
                                        if v.browsing
                                            && let Some(f) = v.find.clone()
                                        {
                                            window.focus(&f.read(cx).focus_handle(cx), cx);
                                        }
                                    }
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(step(1, cx)),
            )
            .when(!part.is_empty(), |d| {
                d.child(chip(capitalized(part), TEXT_2))
            })
            .child(
                label(about)
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .text_ellipsis(),
            )
            .child(
                button(if demo { "Stop the demo" } else { "Play a demo" }, demo)
                    .debug_selector(|| "poly-demo".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.poly_demo(cx);
                        }),
                    ),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(tab("Synth", PolyPage::Synth, cx))
                    .child(tab("Matrix", PolyPage::Matrix, cx))
                    .child(tab("Effects", PolyPage::Effects, cx)),
            )
            .child(self.poly_choices(
                t,
                "poly-quality",
                p.quality,
                &[
                    ("Draft", PolyQuality::Draft),
                    ("Good", PolyQuality::Good),
                    ("Best", PolyQuality::Best),
                ],
                |p, q| p.quality = q,
                cx,
            ))
            .child(
                div()
                    .id("poly-close")
                    .debug_selector(|| "poly-close".into())
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(rgb(TEXT_3))
                    .hover(|d| d.bg(rgb(CARD)).text_color(rgb(TEXT)))
                    .child(icon(Icon::Close, 14.).text_color(rgb(TEXT_2)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.close_poly(cx);
                        }),
                    ),
            )
    }
}
