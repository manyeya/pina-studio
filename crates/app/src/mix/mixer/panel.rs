//! The selected channel's chain beside the strips, and the cards and sliders it's made of.

use super::*;

/// A raised card in the side panel.
pub(crate) fn card() -> Div {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .p(px(10.))
        .rounded(px(10.))
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(LINE))
}

pub(crate) trait Title {
    fn font_size_title(self) -> Self;
}

impl Title for Div {
    fn font_size_title(self) -> Self {
        self.text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD)
    }
}

impl Workspace {
    /// The selected channel's chain, top to bottom as it plays.
    pub(crate) fn channel_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let ch = self.channel;
        let project = &self.session.project;
        let (name, color) = self.channel_look(ch);
        let (glyph, sub) = match ch {
            Channel::Track(i) => {
                let t = &project.tracks[i];
                let to = t
                    .output
                    .and_then(|b| project.auxes.iter().find(|a| a.bus == b))
                    .map_or("Stereo Out".to_string(), |a| a.name.clone());
                (
                    crate::ui::looks::glyph(self.shown(i).look.icon, 18.),
                    format!("Channel {} · to {to}", i + 1),
                )
            }
            Channel::Aux(i) => (
                icon(Icon::Bus, 18.),
                format!("Bus {} · to Stereo Out", project.auxes[i].bus),
            ),
            Channel::Master => (icon(Icon::Output, 18.), "Output 1–2".to_string()),
        };
        let header = div()
            .flex_none()
            .h(px(44.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .size(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .bg(alpha(color, 0.14))
                    .text_color(rgb(color))
                    .child(glyph.text_color(rgb(color))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(sub)),
            );

        let instrument = match ch {
            Channel::Track(i) => {
                let t = &project.tracks[i];
                let look_icon = self.shown(i).look.icon;
                let (title, sub, opens) = match &t.instrument {
                    None => (
                        "Audio input".to_string(),
                        "Recorded and imported audio".to_string(),
                        false,
                    ),
                    Some(Instrument::Synth) => {
                        ("Sine Synth".into(), "Instrument · built in".into(), false)
                    }
                    Some(Instrument::Drums) => {
                        ("Drum Kit".into(), "Instrument · built in".into(), false)
                    }
                    Some(Instrument::Sampler(s)) => (
                        crate::arrange::tracks::sampler_name(s),
                        format!("Sampler · built in · {} samples", s.zones.len()),
                        false,
                    ),
                    Some(Instrument::Plugin(r)) => {
                        (r.class.clone(), "Instrument · VST3".into(), true)
                    }
                };
                Some(
                    card()
                        .h(px(56.))
                        .flex_row()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .size(px(34.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(8.))
                                .bg(rgb(FIELD))
                                .text_color(rgb(TEXT_2))
                                .child(
                                    match opens {
                                        true => icon(Icon::Plug, 16.),
                                        false => crate::ui::looks::glyph(look_icon, 16.),
                                    }
                                    .text_color(rgb(TEXT_2)),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(title),
                                )
                                .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(sub)),
                        )
                        .child(
                            button(if opens { "Open" } else { "Change" }, true).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    if opens {
                                        ws.open_editor(ch, Which::Instrument, cx);
                                    } else {
                                        ws.menu =
                                            Some(ws.plugin_menu(ch, Which::Instrument, e.position));
                                        cx.notify();
                                    }
                                }),
                            ),
                        ),
                )
            }
            _ => None,
        };

        let (eq, comp) = self.strip_dsp(ch);
        let sat = self.sat(ch);
        let inserts = self.inserts(ch).to_vec();
        let used = inserts.len()
            + eq.is_some() as usize
            + comp.is_some() as usize
            + sat.is_some() as usize;
        let mut chain: Vec<Div> = Vec::new();
        if let Some(eq) = eq {
            chain.push(self.eq_card(ch, eq, cx));
        }
        if let Some(comp) = comp {
            chain.push(self.comp_card(ch, comp, cx));
        }
        if let Some(sat) = sat {
            chain.push(self.sat_card(ch, sat, cx));
        }
        for (n, r) in inserts.iter().enumerate() {
            chain.push(
                card()
                    .h(px(44.))
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(icon(Icon::Plug, 15.).text_color(rgb(TEXT_3)))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(r.class.clone()),
                    )
                    .child(button("Open", false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.open_editor(ch, Which::Insert(n), cx)),
                    ))
                    .child(icon_seg(Icon::Close, false, 26.).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.choose(
                                MenuAction::Insert {
                                    channel: ch,
                                    index: n,
                                    plugin: None,
                                },
                                cx,
                            )
                        }),
                    )),
            );
        }
        let add = Some({
            let index = inserts.len();
            div()
                .flex_none()
                .h(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .rounded(px(10.))
                .border_1()
                .border_dashed()
                .border_color(rgb(0x2e2e36))
                .text_color(rgb(TEXT_3))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .hover(|s| s.border_color(rgb(LINE_STRONG)).text_color(rgb(TEXT_2)))
                .child(icon(Icon::Plus, 13.).text_color(rgb(TEXT_3)))
                .child("Add effect")
                .debug_selector(|| "add-effect".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        ws.menu = Some(ws.plugin_menu(ch, Which::Insert(index), e.position));
                        cx.notify();
                    }),
                )
        });

        let sends = match ch {
            Channel::Track(t) => project.tracks[t]
                .sends
                .iter()
                .enumerate()
                .map(|(index, s)| {
                    let bus = project
                        .auxes
                        .iter()
                        .find(|a| a.bus == s.bus)
                        .map_or(format!("Bus {}", s.bus), |a| a.name.clone());
                    let slider = Slider::Send { track: t, index };
                    let pre = s.pre;
                    self.slider_row(bus, format!("{} dB", db_text(s.level_db)), slider, cx)
                        .child(
                            seg(if pre { "Pre" } else { "Post" }, pre, 22.)
                                .w(px(40.))
                                .justify_center()
                                .debug_selector(move || format!("send-pre-{index}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        let action = MenuAction::SendPre {
                                            track: t,
                                            index,
                                            pre: !pre,
                                        };
                                        ws.choose(action, cx);
                                    }),
                                ),
                        )
                })
                .collect(),
            _ => Vec::new(),
        };

        div()
            .w(px(PANEL_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .id("channel-panel")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(header)
                    .children(instrument)
                    .child(
                        div()
                            .h(px(16.))
                            .mt(px(4.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(section_title("Effects"))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{used} in the chain")),
                            ),
                    )
                    .children(chain)
                    .children(add)
                    .when(!sends.is_empty(), |d| {
                        d.child(
                            div()
                                .mt(px(4.))
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex()
                                        .justify_between()
                                        .child(section_title("Sends"))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(rgb(TEXT_3))
                                                .child("Before or after the fader"),
                                        ),
                                )
                                .children(sends),
                        )
                    }),
            )
    }

    /// A built-in's title row: on/off, name, and taking it out.
    pub(crate) fn card_header(
        &self,
        title: &'static str,
        ch: Channel,
        b: Builtin,
        on: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(26.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                switch(on)
                    .debug_selector(move || format!("bypass-{b:?}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit(cx, |ws| match b {
                                Builtin::Eq => {
                                    if let Some(e) = ws.strip_dsp_mut(ch).0 {
                                        e.bypass ^= true
                                    }
                                }
                                Builtin::Comp => {
                                    if let Some(c) = ws.strip_dsp_mut(ch).1 {
                                        c.bypass ^= true
                                    }
                                }
                                Builtin::Sat => {
                                    if let Some(s) = ws.sat_mut(ch) {
                                        s.bypass ^= true
                                    }
                                }
                            })
                        }),
                    ),
            )
            .child(div().font_size_title().child(title))
            .child(div().flex_1())
            .child(icon_seg(Icon::More, false, 26.).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.menu = Some(Menu {
                        at: e.position,
                        items: vec![(
                            "Remove".into(),
                            MenuAction::RemoveBuiltin {
                                channel: ch,
                                builtin: b,
                            },
                        )],
                        ..Default::default()
                    });
                    cx.notify();
                }),
            ))
    }

    /// A labelled horizontal slider, dragged sideways.
    pub fn slider_row(
        &self,
        label: String,
        value: String,
        slider: Slider,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(28.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .w(px(56.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_color(rgb(TEXT_2))
                    .child(label),
            )
            .child(self.slider_bar(slider, cx).flex_1())
            .child(
                div()
                    .w(px(64.))
                    .flex()
                    .justify_end()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(value),
            )
    }

    /// A slider's bar: filled to its value, with a thumb. Click or drag anywhere on it.
    pub fn slider_bar(&self, slider: Slider, cx: &mut Context<Self>) -> Div {
        let v = self.slider(slider).clamp(0.0, 1.0);
        let bounds = self.slider_bounds.clone();
        div()
            .h(px(16.))
            .relative()
            .flex()
            .items_center()
            .cursor_ew_resize()
            .child(
                div()
                    .w_full()
                    .h(px(4.))
                    .rounded(px(2.))
                    .bg(rgb(RAISED))
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(v))
                            .rounded(px(2.))
                            .bg(rgb(TEXT_3)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(gpui::relative(v))
                    .ml(px(-6.))
                    .top(px(2.))
                    .size(px(12.))
                    .rounded(px(6.))
                    .bg(rgb(TEXT))
                    .border_2()
                    .border_color(rgb(SURFACE)),
            )
            .child(
                canvas(
                    move |b, _, _| {
                        bounds.borrow_mut().insert(slider, b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.start_slider(slider, pt(e.position), cx);
                }),
            )
    }
}
