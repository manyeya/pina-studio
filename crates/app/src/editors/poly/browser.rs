//! The sounds: every patch to search, narrow to a part or those liked, pick or hear.

use super::*;

/// The parts a patch can be for, as the drawer lists them.
const PARTS: [(&str, &str); 8] = [
    ("all", "All"),
    ("lead", "Lead"),
    ("bass", "Bass"),
    ("pad", "Pad"),
    ("keys", "Keys"),
    ("pluck", "Pluck"),
    ("fx", "FX"),
    ("liked", "Liked"),
];

/// The patches to show, by their place in `POLY_PATCHES`: those for `part` (or liked, or all)
/// with every word of `query` in their name, part or description.
pub(crate) fn sounds(query: &str, part: &str, liked: &[String]) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    POLY_PATCHES
        .iter()
        .enumerate()
        .filter(|(_, (name, of, _))| match part {
            "all" => true,
            "liked" => liked.iter().any(|l| l == name),
            p => *of == p,
        })
        .filter(|(_, (name, of, about))| {
            let all = format!("{name} {of} {about}").to_lowercase();
            words.iter().all(|w| all.contains(w))
        })
        .map(|(i, _)| i)
        .collect()
}

impl Workspace {
    /// The drawer over the sources: search, parts, and the list.
    pub(crate) fn poly_sounds(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let Some(v) = self.poly_view.as_ref() else {
            return div();
        };
        let liked = &self.settings.liked_sounds;
        let shown = sounds(&v.query, v.part, liked);
        let count = |part: &str| sounds("", part, liked).len();
        let parts = div()
            .flex()
            .flex_wrap()
            .gap(px(4.))
            .children(PARTS.map(|(id, name)| {
                let on = v.part == id;
                div()
                    .h(px(24.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .bg(rgb(if on { RAISED } else { WELL }))
                    .text_size(px(11.))
                    .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                    .debug_selector(move || format!("poly-part-{id}"))
                    .child(name)
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .child(count(id).to_string()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(v) = ws.poly_view.as_mut() {
                                v.part = id;
                            }
                            cx.notify();
                        }),
                    )
            }));
        let rows = shown.iter().map(|&k| {
            let (name, _, about) = POLY_PATCHES[k];
            let on = p.patch == name;
            let liked = self.settings.liked_sounds.iter().any(|l| l == name);
            let small = |i: Icon, lit: bool, sel: String| {
                div()
                    .size(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .text_color(rgb(if lit { TEXT } else { TEXT_3 }))
                    .hover(|d| d.bg(rgb(FIELD)).text_color(rgb(TEXT)))
                    .debug_selector(move || sel.clone())
                    .child(icon(i, 12.).text_color(rgb(if lit { TEXT } else { TEXT_2 })))
            };
            div()
                .flex()
                .items_center()
                .rounded(px(6.))
                .pr(px(4.))
                .bg(rgb(if on { RAISED } else { SURFACE }))
                .hover(|d| d.bg(rgb(CARD)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .px(px(8.))
                        .py(px(6.))
                        .cursor_pointer()
                        .debug_selector(move || format!("poly-sound-{name}"))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(name),
                        )
                        .child(label(about).overflow_hidden().text_ellipsis())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.poly_op(t, PolyOp::Patch(name), cx);
                            }),
                        ),
                )
                .child(
                    small(
                        if liked {
                            Icon::HeartFilled
                        } else {
                            Icon::Heart
                        },
                        liked,
                        format!("poly-like-{name}"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            let list = &mut ws.settings.liked_sounds;
                            match list.iter().position(|l| l == name) {
                                Some(i) => {
                                    list.remove(i);
                                }
                                None => list.push(name.to_string()),
                            }
                            ws.settings.save();
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    small(Icon::Play, false, format!("poly-hear-{name}")).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.poly_op(t, PolyOp::Patch(name), cx);
                            if ws.poly_view.as_ref().is_some_and(|v| v.demo.is_none()) {
                                ws.poly_demo(cx);
                            }
                        }),
                    ),
                )
        });
        let none = shown.is_empty();
        let close = cx.listener(|ws, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            if let Some(v) = ws.poly_view.as_mut() {
                v.browsing = false;
            }
            cx.notify();
        });
        div()
            .absolute()
            .top(px(HEAD_H))
            .bottom(px(KEYS_H))
            .left_0()
            .w(px(SOURCES_W + 24.0))
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .debug_selector(|| "poly-sounds".into())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(title("Sounds"))
                    .child(label(format!("{} shown", shown.len())))
                    .child(div().flex_1())
                    .child(
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .text_color(rgb(TEXT_3))
                            .hover(|d| d.bg(rgb(FIELD)))
                            .debug_selector(|| "poly-sounds-close".into())
                            .child(icon(Icon::Close, 11.).text_color(rgb(TEXT_2)))
                            .on_mouse_down(MouseButton::Left, close),
                    ),
            )
            .child(crate::pages::genres::field_box(
                v.find.clone(),
                "poly-sound-search",
            ))
            .child(parts)
            .child(
                div()
                    .id("poly-sound-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows)
                    .when(none, |d| d.child(label("No sound matches.").p(px(8.)))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_narrow_by_part_words_and_likes() {
        assert_eq!(sounds("", "all", &[]).len(), POLY_PATCHES.len());
        let bass = sounds("", "bass", &[]);
        assert!(!bass.is_empty() && bass.iter().all(|&i| POLY_PATCHES[i].1 == "bass"));
        let reese = sounds("detuned saws", "all", &[]);
        assert!(
            reese.iter().any(|&i| POLY_PATCHES[i].0 == "Reese"),
            "{reese:?}"
        );
        assert!(sounds("growling", "pad", &[]).is_empty());
        assert_eq!(sounds("growling", "bass", &[]).len(), 1);
        let liked = vec!["Acid".to_string()];
        assert_eq!(sounds("", "liked", &liked).len(), 1);
    }
}
