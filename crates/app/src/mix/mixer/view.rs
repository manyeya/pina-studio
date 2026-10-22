//! The mixer page: every channel's strip, faders and labels.

use super::*;

impl Workspace {
    pub fn mixer_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let project = &self.session.project;
        let tracks = (0..project.tracks.len()).map(Channel::Track);
        let auxes = (0..project.auxes.len()).map(Channel::Aux);
        let channels: Vec<Channel> = match self.channels {
            Channels::All => tracks.chain(auxes).collect(),
            Channels::Tracks => tracks.collect(),
            Channels::Buses => auxes.collect(),
        };
        let layout = Layout::Mixer {
            rows: self.mixer_rows,
            fader: self.mixer_fader(),
        };
        let first_bus = channels.iter().position(|c| matches!(c, Channel::Aux(_)));
        let mut strips: Vec<Div> = Vec::new();
        for (i, ch) in channels.into_iter().enumerate() {
            let s = strip::strip(self, ch, layout, cx);
            strips.push(if Some(i) == first_bus && i > 0 {
                s.ml(px(10.))
            } else {
                s
            });
        }
        strips.push(strip::strip(self, Channel::Master, layout, cx).ml(px(10.)));

        let filter = |label: &'static str, which: Channels, on: bool, cx: &mut Context<Self>| {
            seg(label, on, 24.).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.channels = which;
                    cx.notify();
                }),
            )
        };
        let show = |label: &'static str, row: usize, on: bool, cx: &mut Context<Self>| {
            button(label, on).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.mixer_rows[row] ^= true;
                    cx.notify();
                }),
            )
        };
        let toolbar = div()
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Mixer"),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(filter(
                        "All",
                        Channels::All,
                        self.channels == Channels::All,
                        cx,
                    ))
                    .child(filter(
                        "Tracks",
                        Channels::Tracks,
                        self.channels == Channels::Tracks,
                        cx,
                    ))
                    .child(filter(
                        "Buses",
                        Channels::Buses,
                        self.channels == Channels::Buses,
                        cx,
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child("Show"),
            )
            .child(show("EQ", 0, self.mixer_rows[0], cx))
            .child(show("Effects", 1, self.mixer_rows[1], cx))
            .child(show("Sends", 2, self.mixer_rows[2], cx));

        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(rgb(LANE))
                    .child(toolbar)
                    .child(
                        div()
                            .id("mixer")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_scroll()
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .p(px(12.))
                                    .pl(px(16.))
                                    .child(self.mixer_labels())
                                    .children(strips),
                            ),
                    ),
            )
            .child(self.channel_panel(cx))
    }

    /// The faders take what height the window leaves the strips, and the rows it doesn't show.
    pub(crate) fn mixer_fader(&self) -> f32 {
        let [eq, effects, sends] = self.mixer_rows;
        // The rest of a strip, gaps and padding included, with every row showing.
        let rest = 464.0;
        let freed = (!eq as u8 as f32) * 52.0
            + (!effects as u8 as f32) * (MIXER_INSERTS as f32 * 25.0 + 5.0)
            + (!sends as u8 as f32) * (MIXER_SENDS as f32 * 25.0 + 5.0);
        let room = self.viewport_h - CONTROL_BAR_H - STATUS_H - 40.0 - 24.0;
        (room - rest + freed).max(120.0)
    }

    /// What each row of the strips is, down the left, lined up with them.
    pub(crate) fn mixer_labels(&self) -> Div {
        let [eq, effects, sends] = self.mixer_rows;
        // Heights of the rows above each label, as the strips lay them out (8 between).
        let mut rows: Vec<(&'static str, f32)> = vec![("", 28.0)];
        if eq {
            rows.push(("EQ", 44.0));
        }
        rows.push(("Input", 24.0));
        if effects {
            rows.push(("Effects", MIXER_INSERTS as f32 * 25.0 - 3.0));
        }
        if sends {
            rows.push(("Sends", MIXER_SENDS as f32 * 25.0 - 3.0));
        }
        rows.extend([
            ("Output", 22.0),
            ("Pan", 36.0),
            ("Level", 18.0),
            ("", self.mixer_fader()),
            ("", 22.0),
            ("Name", 28.0),
        ]);
        div()
            .w(px(56.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .py(px(9.))
            .children(rows.into_iter().map(|(label, h)| {
                div()
                    .h(px(h))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .text_size(px(10.5))
                    .line_height(px(22.))
                    .text_color(rgb(TEXT_3))
                    .child(label)
            }))
    }
}
