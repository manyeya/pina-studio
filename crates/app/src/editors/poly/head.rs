//! The title bar: the synth's name, its track, the sound's bar (stepped through, saved,
//! compared), a demo to hear it, the pages, the quality, and the way out. Dragged, the window
//! moves.

use super::*;

impl Workspace {
    pub(crate) fn poly_head(
        &mut self,
        t: usize,
        p: &Poly,
        page: PolyPage,
        cx: &mut Context<Self>,
    ) -> Div {
        let bar = self.sound_bar(t, p, cx);
        let key = if p.patch.is_empty() {
            "Init".to_string()
        } else {
            p.patch.clone()
        };
        let (part, about) = self
            .library()
            .find(&key)
            .map_or((String::new(), String::new()), |e| {
                (e.sound.part.clone(), e.sound.about.clone())
            });
        let track = &self.session.project.tracks[t];
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
            .gap(px(8.))
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
            .child(bar)
            .when(!part.is_empty(), |d| d.child(chip(part, TEXT_2)))
            .child(
                label(about)
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .text_ellipsis(),
            )
            .child(
                button(if demo { "Stop" } else { "Demo" }, demo)
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
                    .child(tab("Sounds", PolyPage::Sounds, cx))
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
