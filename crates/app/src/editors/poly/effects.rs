//! The Effects page: the track's own chain, as the mixer shows it, each knob one a synth source
//! can be dropped on.

use super::*;
use crate::{
    mix::{
        eq::{dial, value_line},
        strip::{self, Builtin, Channel},
    },
    workspace::Knob,
};

impl Workspace {
    pub(crate) fn poly_effects_page(&mut self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let ch = Channel::Track(t);
        let track = &self.session.project.tracks[t];
        let (comp, sat, eq, limit) = (
            track.comp.is_some(),
            track.sat.is_some(),
            track.eq.is_some(),
            track.limit.is_some(),
        );
        let more = track.more.clone();
        let own = |name: &'static str, b: Builtin, cx: &mut Context<Self>| {
            let open = cx.listener(move |ws: &mut Workspace, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                match b {
                    Builtin::Eq => ws.open_eq(ch, cx),
                    b => ws.open_fx(ch, b, cx),
                }
            });
            effect_card(name.to_string())
                .child(label("The channel's own"))
                .child(button("Open", false).on_mouse_down(MouseButton::Left, open))
        };
        let mut cards = Vec::new();
        if eq {
            cards.push(own("Channel EQ", Builtin::Eq, cx));
        }
        if comp {
            cards.push(own("Compressor", Builtin::Comp, cx));
        }
        if sat {
            cards.push(own("Saturator", Builtin::Sat, cx));
        }
        if limit {
            cards.push(own("Limiter", Builtin::Limit, cx));
        }
        for (k, u) in more.iter().enumerate() {
            let Some((b, name)) = self.stacked(ch, k) else {
                continue;
            };
            let table = strip::knobs(u.kind());
            let on = !u.bypassed();
            let row = div().flex().flex_wrap().gap(px(10.)).children(
                (0..table.len())
                    .filter(|&i| knob_shows(u, i))
                    .map(|i| self.effect_knob(ch, k, i, table[i].name, cx)),
            );
            let power = button(if on { "On" } else { "Off" }, on)
                .debug_selector(move || format!("poly-effect-power-{k}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(u) = ws.chain_mut(ch).more.get_mut(k) {
                                let off = u.bypass_mut();
                                *off = !*off;
                            }
                        });
                    }),
                );
            let open = cx.listener(move |ws: &mut Workspace, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.open_fx_at(ch, b, Some(k), cx);
            });
            cards.push(
                effect_card(name)
                    .when(!on, |d| d.opacity(0.55))
                    .child(row)
                    .child(div().flex().gap(px(6.)).child(power).child(
                        button("Open its window", false).on_mouse_down(MouseButton::Left, open),
                    )),
            );
        }
        let none = cards.is_empty();
        let add = self.poly_menu(
            "poly-add-effect".into(),
            "Add an effect".into(),
            move |_| {
                let tools = [
                    Fx::Reverb,
                    Fx::Delay,
                    Fx::Filter,
                    Fx::Motion,
                    Fx::Shaper,
                    Fx::Gate,
                    Fx::DeEsser,
                    Fx::Stereo,
                    Fx::Multiband,
                    Fx::Pitch,
                    Fx::TapeStop,
                    Fx::Tune,
                ];
                [Builtin::Eq, Builtin::Comp, Builtin::Sat, Builtin::Limit]
                    .into_iter()
                    .chain(tools.map(Builtin::Tool))
                    .map(|builtin| {
                        (
                            builtin.name().to_string(),
                            MenuAction::AddBuiltin {
                                channel: ch,
                                builtin,
                            },
                        )
                    })
                    .collect()
            },
            false,
            cx,
        );
        let chips = |list: Vec<PolySource>, cx: &mut Context<Self>| {
            div()
                .flex()
                .gap(px(3.))
                .children(list.into_iter().map(|s| self.source_chip(t, p, s, cx)))
        };
        let sources = div()
            .w(px(SOURCES_W + 160.0))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(label("Drag a source onto an effect's knob"))
            .child(chips((0..ENVS as u8).map(PolySource::Env).collect(), cx))
            .child(chips((0..LFOS as u8).map(PolySource::Lfo).collect(), cx))
            .child(chips(
                (0..MACROS as u8).map(PolySource::Macro).collect(),
                cx,
            ));
        div().flex_1().min_h(px(0.)).child(
            div()
                .id("poly-effects")
                .size_full()
                .overflow_y_scroll()
                .px(px(20.))
                .py(px(14.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(title("This track's effects"))
                        .child(label("The same chain the mixer shows, not a second one"))
                        .child(div().flex_1())
                        .child(div().w(px(180.)).child(add)),
                )
                .child(div().flex().flex_wrap().gap(px(12.)).children(cards))
                .when(none, |d| {
                    d.child(label(
                        "No effects on this track yet. Add one, and its knobs show here.",
                    ))
                })
                .child(sources),
        )
    }

    /// A stacked effect's knob, on the Effects page.
    fn effect_knob(
        &self,
        ch: Channel,
        k: usize,
        i: usize,
        name: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        let target = Target::Stacked(ch, k, Knob::Param(i));
        let value = self.target(target);
        let u = self.tool_heard(ch, k);
        let table = u.map(|u| strip::knobs(u.kind()));
        let bipolar = table.and_then(|t| t.get(i)).is_some_and(|p| p.bipolar);
        let text = match (u, table.and_then(|t| t.get(i))) {
            (Some(u), Some(p)) => (p.say)(&u),
            _ => String::new(),
        };
        let shown = if bipolar { value * 2.0 - 1.0 } else { value };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .child(
                div()
                    .cursor_ns_resize()
                    .child(self.modded(
                        self.mod_key(target),
                        34.0,
                        dial(shown, 34.0, 3.5, TEXT, bipolar),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let y = pt(e.position).y;
                            ws.begin_drag(Drag::Knob { target, y, value }, cx);
                        }),
                    ),
            )
            .child(label(name))
            .child(value_line(text, 10.5, None))
    }
}

fn effect_card(name: String) -> Div {
    div()
        .flex_1()
        .min_w(px(250.))
        .p(px(14.))
        .flex()
        .flex_col()
        .gap(px(12.))
        .rounded(px(8.))
        .bg(rgb(WELL))
        .child(title(name))
}
