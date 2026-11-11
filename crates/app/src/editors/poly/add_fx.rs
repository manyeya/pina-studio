//! Adding an effect to the synth's rack: every one it offers, by what it does, each drawn and
//! said in a line, searched by name or by what it sounds like. Pointed at, it's heard in the
//! sound as it plays (the keys held, the demo, the song) without being put in; clicked, it goes
//! in last and opens on the stage.

use super::*;
use pina_engine::song::project::rack_about;

/// The browser's lists, down its left: everything, each kind, and those added lately.
const CATS: [&str; 6] = ["All", "Drive", "Motion", "Space", "Shape", "Recent"];

/// How many the Recent list keeps.
const RECENT: usize = 8;

/// The offers for `cat` with every word of `query` in their name, kind or line, grouped by
/// kind, in the order the rack offers them (Recent: newest first).
pub(crate) fn offers(
    cat: &str,
    query: &str,
    recent: &[String],
) -> Vec<(&'static str, Vec<&'static str>)> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let fits = |group: &str, name: &str| {
        let all = format!("{name} {group} {}", rack_about(name)).to_lowercase();
        words.iter().all(|w| all.contains(w))
    };
    if cat == "Recent" {
        let names: Vec<&'static str> = recent
            .iter()
            .filter_map(|r| {
                RACK_OFFERS
                    .iter()
                    .find_map(|(g, names)| names.iter().find(|n| **n == r).map(|n| (*g, *n)))
            })
            .filter(|(g, n)| fits(g, n))
            .map(|(_, n)| n)
            .collect();
        return match names.is_empty() {
            true => Vec::new(),
            false => vec![("Recent", names)],
        };
    }
    RACK_OFFERS
        .iter()
        .filter(|(g, _)| cat == "All" || *g == cat)
        .map(|(g, names)| {
            (
                *g,
                names
                    .iter()
                    .copied()
                    .filter(|n| fits(g, n))
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, names)| !names.is_empty())
        .collect()
}

impl Workspace {
    /// The browser opened (or closed), its search cleared.
    pub(crate) fn browse_effects(&mut self, open: bool, cx: &mut Context<Self>) {
        if let Some(v) = self.poly_view.as_mut() {
            v.adding = open;
            v.fx_cat = "All";
        }
        if let Some(input) = self.poly_view.as_ref().and_then(|v| v.fx_find.clone()) {
            input.update(cx, |i, cx| i.set_text("", cx));
        }
        self.hear_offer(None, cx);
        cx.notify();
    }

    /// `name` (None: nothing) heard in track `t`'s sound as though it were in the rack, last.
    fn hear_offer(&mut self, offer: Option<(usize, &'static str)>, cx: &mut Context<Self>) {
        let unit = offer.and_then(|(t, name)| Some((t, rack_effect(name)?)));
        if unit.map(|u| (u.0, u.1)) == self.rack_preview {
            return;
        }
        self.rack_preview = unit;
        if let Some(v) = self.poly_view.as_mut() {
            v.hearing = offer.map(|o| o.1);
        }
        let project = self.engine_project().into_owned();
        self.status = self.player.sync(&project).err();
        cx.notify();
    }

    /// An offer picked: in, last; on the stage; remembered as added lately.
    fn add_offer(&mut self, t: usize, name: &'static str, cx: &mut Context<Self>) {
        self.browse_effects(false, cx);
        self.poly_op(t, PolyOp::AddFx(name), cx);
        let recent = &mut self.settings.recent_effects;
        recent.retain(|r| r != name);
        recent.insert(0, name.to_string());
        recent.truncate(RECENT);
        self.settings.save();
    }

    /// The browser, over the synth: search, the kinds down the left, every offer drawn.
    pub(crate) fn effects_browser(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let Some(v) = self.poly_view.as_ref() else {
            return div();
        };
        let recent = &self.settings.recent_effects;
        let shown = offers(v.fx_cat, &v.fx_query, recent);
        let count = |cat: &str| -> usize {
            offers(cat, &v.fx_query, recent)
                .iter()
                .map(|(_, n)| n.len())
                .sum()
        };
        let after = p.fx.last().map(rack_name);
        let cats = CATS.map(|cat| {
            let on = v.fx_cat == cat;
            div()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(7.))
                .text_size(px(13.))
                .cursor_pointer()
                .bg(rgb(if on { RAISED } else { 0x17171c }))
                .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                .when(on, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                .hover(|d| d.bg(rgb(RAISED)))
                .debug_selector(move || format!("poly-add-cat-{}", cat.to_lowercase()))
                .child(div().flex_1().child(cat))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child(count(cat).to_string()),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(v) = ws.poly_view.as_mut() {
                            v.fx_cat = cat;
                        }
                        cx.notify();
                    }),
                )
        });
        let hearing = v.hearing;
        let sections: Vec<Div> = shown
            .into_iter()
            .map(|(group, names)| {
                let cards: Vec<Div> = names
                    .into_iter()
                    .filter_map(|name| Some((name, rack_effect(name)?)))
                    .map(|(name, u)| {
                        let heard = hearing == Some(name);
                        div()
                            .id(gpui::SharedString::from(format!("poly-offer-{name}")))
                            .p(px(8.))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .rounded(px(10.))
                            .bg(rgb(if heard { RAISED } else { 0x1f1f26 }))
                            .border_1()
                            .border_color(rgb(if heard { ACCENT } else { LINE }))
                            .cursor_pointer()
                            .debug_selector(move || format!("poly-offer-{name}"))
                            .child(
                                picture()
                                    .h(px(44.))
                                    .px(px(4.))
                                    .flex()
                                    .items_center()
                                    .child(effects::mini_canvas(effects::mini(&u), TEXT))
                                    .when(heard, |d| {
                                        d.child(
                                            div()
                                                .absolute()
                                                .top(px(5.))
                                                .right(px(5.))
                                                .h(px(20.))
                                                .px(px(7.))
                                                .flex()
                                                .items_center()
                                                .rounded(px(10.))
                                                .bg(rgb(ACCENT))
                                                .text_size(px(10.5))
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(rgb(0xffffff))
                                                .child("Hearing it"),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(rack_name(&u)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .line_height(px(15.))
                                    .text_color(rgb(TEXT_3))
                                    .child(rack_about(name)),
                            )
                            .on_hover(cx.listener(move |ws, over: &bool, _, cx| match *over {
                                true => ws.hear_offer(Some((t, name)), cx),
                                false => {
                                    if ws.poly_view.as_ref().and_then(|v| v.hearing) == Some(name) {
                                        ws.hear_offer(None, cx);
                                    }
                                }
                            }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.add_offer(t, name, cx);
                                }),
                            )
                            .map(|d| div().child(d))
                    })
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_2))
                            .child(group),
                    )
                    .child(div().grid().grid_cols(4).gap(px(10.)).children(cards))
            })
            .collect();
        let none = sections.is_empty();
        let demo = v.demo.is_some();
        let panel = div()
            .id("poly-add-panel")
            .occlude()
            .w(px(980.))
            .h(px(660.))
            .flex()
            .flex_col()
            .rounded(px(16.))
            .overflow_hidden()
            .bg(rgb(0x17171c))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(18.))
                    .pt(px(16.))
                    .pb(px(12.))
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Add an effect"),
                    )
                    .child(label(match after {
                        Some(a) => format!("goes in after {a} · {} of {RACK}", p.fx.len() + 1),
                        None => format!("goes in first, after its voices · 1 of {RACK}"),
                    }))
                    .child(div().flex_1())
                    .child(
                        button(if demo { "Stop the demo" } else { "Play a demo to hear them" }, demo)
                            .h(px(28.))
                            .px(px(10.))
                            .text_size(px(12.))
                            .debug_selector(|| "poly-add-demo".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.poly_demo(cx);
                                }),
                            ),
                    )
                    .child(
                        crate::mix::eq::icon_button(Icon::Close, "poly-add-close").on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.browse_effects(false, cx);
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .px(px(18.))
                    .pb(px(12.))
                    .child(crate::pages::genres::field_box(v.fx_find.clone(), "poly-add-search")),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .w(px(190.))
                            .flex_none()
                            .p(px(10.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .border_r_1()
                            .border_color(rgb(LINE))
                            .children(cats)
                            .child(div().flex_1())
                            .child(
                                div()
                                    .p(px(8.))
                                    .text_size(px(11.5))
                                    .line_height(px(16.))
                                    .text_color(rgb(TEXT_3))
                                    .child("Point at one to hear it in the sound as it plays: hold keys, play the demo or the song. Click to add it."),
                            ),
                    )
                    .child(
                        div()
                            .id("poly-add-list")
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_y_scroll()
                            .px(px(16.))
                            .py(px(14.))
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .children(sections)
                            .when(none, |d| {
                                d.child(label(match v.fx_cat {
                                    "Recent" if v.fx_query.is_empty() => {
                                        "Nothing added lately".to_string()
                                    }
                                    _ => format!("Nothing that sounds like \"{}\"", v.fx_query),
                                }))
                            }),
                    ),
            );
        // Over the whole synth, dimmed: a click outside closes it.
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.45))
            .debug_selector(|| "poly-add-backdrop".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.browse_effects(false, cx);
                }),
            )
            .child(panel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_are_found_by_what_they_sound_like() {
        let names = |cat: &str, q: &str| -> Vec<&str> {
            offers(cat, q, &[])
                .into_iter()
                .flat_map(|(_, n)| n)
                .collect()
        };
        assert_eq!(names("All", "").len(), 37);
        assert_eq!(names("Drive", "").len(), 8);
        assert!(names("All", "warm").contains(&"Tape"));
        assert_eq!(names("All", "octave"), ["Rectify", "Shimmer"]);
        assert_eq!(names("Space", "bounc"), ["Ping-Pong Delay"]);
        assert!(names("All", "nothing like this").is_empty());
        let recent = vec!["Hall".to_string(), "Tube".to_string(), "Gone".to_string()];
        assert_eq!(names("Recent", ""), Vec::<&str>::new());
        assert_eq!(
            offers("Recent", "", &recent),
            vec![("Recent", vec!["Hall", "Tube"])]
        );
        // Every offer says what it does.
        for (_, names) in RACK_OFFERS {
            for n in names {
                assert!(!rack_about(n).is_empty(), "{n}");
            }
        }
    }
}
