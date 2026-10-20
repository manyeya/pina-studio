//! The editor under the tracks: a bar naming what's being edited, with a tab for each editor
//! (piano roll, step sequencer, score, takes, Live Loops) and that editor's own controls; and the
//! editor itself, its left column lined up with the track headers so its grid meets the lanes.
//! Drag its top edge to make it taller or shorter; double-click the edge for its usual height.

use gpui::{Context, FontWeight, MouseButton, MouseDownEvent, div, prelude::*, px, rgb};
use pina_engine::project::Content;

use crate::{
    theme::*,
    workspace::{Bottom, Drag, Workspace, pt},
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
        // What's being edited: the selected region (the piano roll's, which may be the selected
        // track's own), or the whole song for Live Loops.
        let shown = match pane {
            Bottom::PianoRoll => self.roll_region(),
            _ => self.region,
        };
        let (title, detail, color) = match (pane, shown) {
            (Bottom::Sampler, _) => match self.sampler_track() {
                Some(t) => {
                    let zones = match &self.session.project.tracks[t].instrument {
                        Some(pina_engine::project::Instrument::Sampler(s)) => s.zones.len(),
                        _ => 0,
                    };
                    (
                        self.session.project.tracks[t].name.clone(),
                        format!("{zones} zones"),
                        self.color(t),
                    )
                }
                None => ("Sampler".to_string(), String::new(), TEXT_4),
            },
            (Bottom::Modulation, _) => match self.session.project.tracks.get(self.track) {
                Some(t) => (
                    t.name.clone(),
                    match t.modulators.len() {
                        1 => "1 modulator".to_string(),
                        n => format!("{n} modulators"),
                    },
                    self.color(self.track),
                ),
                None => ("Modulation".to_string(), String::new(), TEXT_4),
            },
            (Bottom::Loops, _) => (
                "Live Loops".to_string(),
                format!("{} tracks", self.session.project.tracks.len()),
                ACCENT,
            ),
            (_, Some(at)) => {
                let t = &self.session.project.tracks[at.0];
                let r = &t.regions[at.1];
                let detail = match &r.content {
                    Content::Midi { notes, .. } => format!("{} notes", notes.len()),
                    Content::Pattern { rows, .. } => format!("{} sounds", rows.len()),
                    Content::Takes { takes, .. } => format!("{} takes", takes.len()),
                    Content::Audio { .. } => "Audio".into(),
                };
                (t.name.clone(), detail, self.color(at.0))
            }
            (Bottom::PianoRoll, None) => match self.session.project.tracks.get(self.track) {
                Some(t) if t.instrument.is_some() => (
                    t.name.clone(),
                    "double-click to write a note".to_string(),
                    self.color(self.track),
                ),
                _ => ("No region".to_string(), String::new(), TEXT_4),
            },
            (_, None) => ("No region".to_string(), String::new(), TEXT_4),
        };
        let controls = match pane {
            Bottom::PianoRoll => Some(self.roll_controls(cx)),
            Bottom::Steps => self.steps_controls(cx),
            Bottom::Sampler => self.sampler_controls(cx),
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
                    .w(px(self.header_w))
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
                            // Those Settings shows, and the one open.
                            .children(
                                Bottom::ALL
                                    .into_iter()
                                    .filter(|&b| b == pane || self.settings.shows(b))
                                    .map(|b| tab(b.name(), b, cx))
                                    .collect::<Vec<_>>(),
                            ),
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
            Bottom::Sampler => self.sampler_body(cx).into_any_element(),
            Bottom::Modulation => self.modulation_body(cx).into_any_element(),
            Bottom::Score => self.score_body(cx).into_any_element(),
            Bottom::Takes => self.takes_body(cx).into_any_element(),
            Bottom::Loops => self.loops_body(cx).into_any_element(),
        };
        let height = self.editor_height();
        let resizing = matches!(self.drag, Some(Drag::Editor { .. }));
        // The top edge: a strip to grab, lit while it's held or under the pointer.
        let edge = div()
            .id("editor-edge")
            .absolute()
            .top(px(-4.))
            .left_0()
            .w_full()
            .h(px(8.))
            .flex()
            .items_center()
            .cursor_row_resize()
            .child(
                div()
                    .w_full()
                    .h(px(2.))
                    .when(resizing, |d| d.bg(rgb(ACCENT))),
            )
            .hover(|s| s.bg(alpha(ACCENT, 0.35)))
            .debug_selector(|| "editor-edge".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if e.click_count == 2 {
                        ws.editor_h = EDITOR_H;
                        cx.notify();
                        return;
                    }
                    let y = pt(e.position).y;
                    ws.begin_drag(Drag::Editor { y, height }, cx);
                }),
            );
        div()
            .h(px(height))
            .flex_none()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(LANE))
            .child(bar)
            .child(body)
            .child(edge)
    }
}
