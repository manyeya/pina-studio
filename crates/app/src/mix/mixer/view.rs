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
                    // More strips than fit scroll sideways, by trackpad or by the wheel (scrolling
                    // only sideways, a wheel's turn moves along them). The strips' row is as wide
                    // as they are, so there's something to scroll.
                    // shortcut: a window too short for the faders' least height cuts the strips'
                    // feet off rather than scrolling down; scroll both ways if that bites.
                    .child(
                        div()
                            .id("mixer")
                            .flex_1()
                            .min_h(px(0.))
                            .flex()
                            .overflow_x_scroll()
                            .debug_selector(|| "mixer-strips".into())
                            .child(
                                div()
                                    .flex_none()
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

    /// A mixer strip's rows, top to bottom, as `strip` lays them out (8 apart, in 8 of padding
    /// and a border): what each is called down the left, and how tall it is. The fader takes
    /// what height the window leaves, so the whole strip shows.
    pub(crate) fn mixer_parts(&self) -> Vec<(&'static str, f32)> {
        let [eq, effects, sends] = self.mixer_rows;
        // A column of slots 22 tall, 3 apart.
        let slots = |n: usize| n as f32 * 25.0 - 3.0;
        let mut rows: Vec<(&'static str, f32)> = vec![("", 28.0)];
        if eq {
            rows.push(("EQ", 44.0));
        }
        rows.push(("Input", 24.0));
        if effects {
            rows.push(("Effects", slots(MIXER_INSERTS)));
        }
        if sends {
            rows.push(("Sends", slots(MIXER_SENDS)));
        }
        rows.extend([
            ("Output", 22.0),
            ("Mode", 18.0),
            ("Pan", 36.0),
            ("Level", 18.0),
            ("", 0.0),
            ("", 22.0),
            ("Name", 28.0),
        ]);
        let fixed: f32 = rows.iter().map(|r| r.1).sum();
        let gaps = 8.0 * (rows.len() - 1) as f32;
        // The window under the toolbar, the strips' row padded 12 above and below.
        let room = self.viewport_h - CONTROL_BAR_H - STATUS_H - 40.0 - 24.0;
        let fader = (room - fixed - gaps - 18.0).max(120.0);
        if let Some(r) = rows.iter_mut().find(|r| r.1 == 0.0) {
            r.1 = fader;
        }
        rows
    }

    /// The faders' height: what the window leaves the strips.
    pub(crate) fn mixer_fader(&self) -> f32 {
        self.mixer_parts()
            .iter()
            .rev()
            .nth(2)
            .map_or(120.0, |r| r.1)
    }

    /// What each row of the strips is, down the left, lined up with them.
    pub(crate) fn mixer_labels(&self) -> Div {
        let rows = self.mixer_parts();
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
