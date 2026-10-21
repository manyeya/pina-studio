//! The roll's control bar.

use super::*;

/// A note's length as a fraction of a bar of 4/4: "1/16", "3/8", "1/1".
pub(crate) fn length_name(ticks: u64) -> String {
    let whole = PPQ * 4;
    let gcd = |mut a: u64, mut b: u64| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    let g = gcd(ticks, whole).max(1);
    match whole / g {
        d if d <= 64 => format!("{}/{d}", ticks / g),
        _ => format!("{ticks} ticks"),
    }
}

/// A different seed each time, so humanizing twice isn't the same nudge twice.
pub(crate) fn fresh_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

/// A toggle with an icon, tinted with the accent while on.
pub(crate) fn chip_button(i: Icon, label: String, on: bool) -> Div {
    chip(on)
        .child(icon(i, 13.).text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 })))
        .child(label)
}

/// A toggle with words alone.
pub(crate) fn chip_text(label: &str, on: bool) -> Div {
    chip(on).child(label.to_string())
}

pub(crate) fn chip(on: bool) -> Div {
    div()
        .h(px(26.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(7.))
        .cursor_pointer()
        .whitespace_nowrap()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .bg(if on {
            alpha(ACCENT, 0.16)
        } else {
            rgb(0x23232a).into()
        })
        .text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 }))
        .hover(|s| s.bg(rgb(RAISED)))
}

impl Workspace {
    /// The piano roll's own controls in the editor's bar: scale, writing in key, chord stamps,
    /// ghost notes, colour by velocity, quantize, fit and zoom.
    pub fn roll_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let key = self.session.project.song_key();
        let scale_label = key.map_or("Scale".to_string(), |k| k.short());
        let ghost = self
            .roll
            .ghost
            .and_then(|g| self.session.project.tracks.iter().find(|t| t.id == g))
            .map_or("Ghost".to_string(), |t| t.name.clone());
        let at = self.roll_region();
        let grid = self.roll.grid;
        let stamp = self.roll.stamp;
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .min_w(px(0.))
            .overflow_hidden()
            .child(
                chip_button(Icon::Scale, scale_label, self.roll.scale)
                    .debug_selector(|| "roll-scale".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.roll.scale ^= true;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                chip_text("In key", self.roll.in_key)
                    .debug_selector(|| "roll-in-key".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.roll.in_key ^= true;
                            if ws.roll.in_key && ws.session.project.song_key().is_none() {
                                ws.status = Some(
                                    "The song has no key yet: set one in the control bar.".into(),
                                );
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                chip_text(
                    if stamp == Stamp::Note {
                        "Chords"
                    } else {
                        stamp.name()
                    },
                    stamp != Stamp::Note,
                )
                .debug_selector(|| "roll-stamp".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        let items = Stamp::ALL
                            .iter()
                            .map(|&s| (s.name().to_string(), MenuAction::Stamp(s)))
                            .collect();
                        ws.menu = Some(Menu {
                            at: e.position,
                            items,
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                ),
            )
            .child(
                chip_button(Icon::Ghost, ghost, self.roll.ghost.is_some())
                    .debug_selector(|| "roll-ghost".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            let mut items = vec![("Off".to_string(), MenuAction::Ghost(None))];
                            let shown = ws
                                .roll_region()
                                .map(|at| ws.session.project.tracks[at.0].id);
                            items.extend(
                                ws.session
                                    .project
                                    .tracks
                                    .iter()
                                    .filter(|t| t.instrument.is_some() && Some(t.id) != shown)
                                    .map(|t| (t.name.clone(), MenuAction::Ghost(Some(t.id)))),
                            );
                            ws.menu = Some(Menu {
                                at: e.position,
                                items,
                                ..Default::default()
                            });
                            cx.notify();
                        }),
                    ),
            )
            .child(
                chip(self.roll.by_velocity)
                    // A swatch of the colours velocities take.
                    .child(
                        div().flex().rounded(px(2.)).overflow_hidden().children(
                            [20u8, 64, 96, 127]
                                .map(|v| div().w(px(3.)).h(px(10.)).bg(rgb(velocity_color(v)))),
                        ),
                    )
                    .child("Velocity")
                    .debug_selector(|| "roll-colours".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.roll.by_velocity ^= true;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                button(
                    format!(
                        "Q {}",
                        crate::workspace::inspector::grid_name(grid).trim_end_matches(" note")
                    ),
                    false,
                )
                .debug_selector(|| "roll-quantize".into())
                .when(at.is_none(), |d| d.opacity(0.4).cursor_default())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.roll_op(RollOp::Quantize, cx)),
                ),
            )
            .child(
                button("Fit", false)
                    .debug_selector(|| "roll-fit".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.fit_roll();
                            cx.notify();
                        }),
                    ),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(rgb(LINE)))
            .child(
                div()
                    .w(px(84.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(self.slider_bar(Slider::Zoom, cx).flex_1()),
            )
    }
}
