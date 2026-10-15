//! The editor under the tracks: a bar naming what's being edited, with a tab for each editor
//! (piano roll, step sequencer, score, takes, Live Loops) and that editor's own controls; and the
//! editor itself, its left column lined up with the track headers so its grid meets the lanes.

use gpui::{Context, FontWeight, MouseButton, div, prelude::*, px, rgb};
use tempo_engine::project::Content;

use crate::{
    theme::*,
    workspace::{Bottom, Workspace},
};

impl Workspace {
    pub fn editor(&mut self, pane: Bottom, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = |label: &'static str, b: Bottom, cx: &mut Context<Self>| {
            seg(label, pane == b, 24.)
                .debug_selector(move || format!("tab-{b:?}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.show_bottom(b, cx)),
                )
        };
        // What's being edited: the selected region, or the whole song for Live Loops.
        let (title, detail, color) = match (pane, self.region) {
            (Bottom::Loops, _) => (
                "Live Loops".to_string(),
                format!("{} tracks", self.session.project.tracks.len()),
                ACCENT,
            ),
            (_, Some(at)) => {
                let t = &self.session.project.tracks[at.0];
                let r = &t.regions[at.1];
                let detail = match &r.content {
                    Content::Midi { notes } => format!("{} notes", notes.len()),
                    Content::Pattern { rows, .. } => format!("{} sounds", rows.len()),
                    Content::Takes { takes, .. } => format!("{} takes", takes.len()),
                    Content::Audio { .. } => "Audio".into(),
                };
                (t.name.clone(), detail, self.color(at.0))
            }
            (_, None) => ("No region".to_string(), String::new(), TEXT_4),
        };
        let controls = match pane {
            Bottom::PianoRoll => Some(self.roll_controls(cx)),
            _ => None,
        };
        let bar = div()
            .h(px(EDITOR_BAR_H))
            .flex_none()
            .flex()
            .items_center()
            .bg(rgb(SURFACE))
            .border_t_1()
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .w(px(HEADER_W))
                    .flex_none()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(12.))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .size(px(10.))
                            .flex_none()
                            .rounded(px(3.))
                            .bg(rgb(color)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(title),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .child(detail),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(10.))
                    .child(
                        tray()
                            .p(px(2.))
                            .rounded(px(8.))
                            .child(tab("Piano Roll", Bottom::PianoRoll, cx))
                            .child(tab("Step Sequencer", Bottom::Steps, cx))
                            .child(tab("Score", Bottom::Score, cx))
                            .child(tab("Takes", Bottom::Takes, cx))
                            .child(tab("Live Loops", Bottom::Loops, cx)),
                    )
                    .child(div().flex_1())
                    .children(controls)
                    .child(
                        icon_seg(Icon::Close, false, 26.)
                            .debug_selector(|| "close-editor".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.bottom = None;
                                    cx.notify();
                                }),
                            ),
                    ),
            );
        let body = match pane {
            Bottom::PianoRoll => self.roll_body(cx).into_any_element(),
            Bottom::Steps => self.steps_body(cx).into_any_element(),
            Bottom::Score => self.score_body(cx).into_any_element(),
            Bottom::Takes => self.takes_body(cx).into_any_element(),
            Bottom::Loops => self.loops_body(cx).into_any_element(),
        };
        div()
            .h(px(EDITOR_H))
            .flex_none()
            .flex()
            .flex_col()
            .bg(rgb(LANE))
            .child(bar)
            .child(body)
    }
}
