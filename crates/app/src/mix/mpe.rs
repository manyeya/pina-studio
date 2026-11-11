//! An Audio Unit or VST2 instrument's MPE, in a card: on or off, and the zone it hears (lower or
//! upper, how many note channels, how far each note's bend reaches). VST3 and CLAP instruments
//! hear each note's expression without it.

use gpui::{FontWeight, MouseButton, Pixels, Point, anchored, deferred, div, prelude::*, px, rgb};
use pina_engine::song::project::{Instrument, MpeZone};

use crate::{ui::theme::*, workspace::Workspace};

/// The MPE card open: its track, and where it opens.
pub struct MpeView {
    pub track: usize,
    pub at: Point<Pixels>,
}

/// The bend ranges instruments usually have.
const REACHES: [u8; 5] = [2, 12, 24, 48, 96];

impl Workspace {
    /// Changes a track's plugin instrument's MPE, as an edit.
    fn change_mpe(
        &mut self,
        t: usize,
        cx: &mut gpui::Context<Self>,
        f: impl FnOnce(&mut bool, &mut MpeZone),
    ) {
        self.edit(cx, |ws| {
            if let Some(Instrument::Plugin(r)) = &mut ws.session.project.tracks[t].instrument {
                f(&mut r.mpe, &mut r.mpe_zone);
            }
        });
    }

    /// Opens the MPE card of track `t`'s instrument at `at`.
    pub fn open_mpe(&mut self, t: usize, at: Point<Pixels>, cx: &mut gpui::Context<Self>) {
        self.mpe_view = Some(MpeView { track: t, at });
        cx.notify();
    }

    pub fn mpe_window(&mut self, cx: &mut gpui::Context<Self>) -> Option<impl IntoElement + use<>> {
        let v = self.mpe_view.as_ref()?;
        let (t, at) = (v.track, v.at);
        let Some(Instrument::Plugin(r)) = &self.session.project.tracks.get(t)?.instrument else {
            return None;
        };
        let (on, zone, name) = (r.mpe, r.mpe_zone, r.class.clone());
        let (manager, members) = zone.channels();
        let row = |label: &'static str| {
            div().flex().items_center().gap(px(8.)).child(
                div()
                    .w(px(96.))
                    .flex_none()
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .child(label),
            )
        };
        let choice = |label: String,
                      picked: bool,
                      cx: &mut gpui::Context<Self>,
                      f: fn(&mut MpeZone, u8),
                      v: u8| {
            let id = format!("mpe-{label}");
            seg(label, picked, 22.)
                .flex_1()
                .px(px(6.))
                .text_size(px(11.))
                .debug_selector(move || id.clone())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.change_mpe(t, cx, |_, z| f(z, v));
                    }),
                )
        };
        let step = |sign: &'static str, cx: &mut gpui::Context<Self>, by: i8| {
            button(sign, false)
                .w(px(24.))
                .h(px(22.))
                .debug_selector(move || format!("mpe-channels-{sign}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.change_mpe(t, cx, |_, z| {
                            z.channels = (z.channels as i8 + by).clamp(1, 15) as u8;
                        });
                    }),
                )
        };
        let card = div()
            .id("mpe-card")
            .w(px(360.))
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .rounded(px(12.))
            .bg(rgb(0x1c1c22))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                ws.mpe_view = None;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child("MPE"))
                            .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(name)),
                    )
                    .child(
                        switch(on)
                            .debug_selector(|| "mpe-on".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.change_mpe(t, cx, |on, _| *on ^= true);
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_2))
                    .child(match on {
                        true => format!(
                            "Each note on a channel of its own, {} to {}, with its own bend, slide and pressure; channel {} for what's everyone's.",
                            members.start() + 1,
                            members.end() + 1,
                            manager + 1
                        ),
                        false => "Off: every note on channel 1, so it hears no note's own bend, slide or pressure.".to_string(),
                    }),
            )
            .child(
                row("Zone").child(
                    tray()
                        .flex_1()
                        .p(px(2.))
                        .rounded(px(7.))
                        .child(choice("Lower".into(), !zone.upper, cx, |z, _| z.upper = false, 0))
                        .child(choice("Upper".into(), zone.upper, cx, |z, _| z.upper = true, 0)),
                ),
            )
            .child(
                row("Note channels")
                    .child(
                        div()
                            .w(px(40.))
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .bg(rgb(WELL))
                            .font_family(MONO)
                            .text_size(px(11.5))
                            .debug_selector(|| "mpe-channels".into())
                            .child(zone.channels.clamp(1, 15).to_string()),
                    )
                    .child(step("−", cx, -1))
                    .child(step("+", cx, 1)),
            )
            .child(
                row("Bend range").child(
                    tray().flex_1().p(px(2.)).rounded(px(7.)).children(
                        REACHES.map(|st| choice(format!("{st}"), zone.bend == st, cx, |z, v| z.bend = v, st)),
                    ),
                ),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child("Set the instrument's own MPE to match. Most use the lower zone and 48 semitones."),
            );
        Some(deferred(anchored().position(at).snap_to_window().child(card)).with_priority(2))
    }
}
