//! The status bar along the bottom.

use super::*;
use gpui::{Div, Stateful};

/// How long an agent's outline stays on what it touched.
pub const PRESENCE: Duration = Duration::from_secs(12);

/// Hover help: what a control does, in the status bar while the pointer's over it.
pub trait Help {
    fn help(self, text: &'static str, cx: &mut Context<Workspace>) -> Stateful<Div>;
}

impl Help for Div {
    fn help(self, text: &'static str, cx: &mut Context<Workspace>) -> Stateful<Div> {
        self.id(text)
            .on_hover(cx.listener(move |ws, over: &bool, _, cx| {
                if *over {
                    ws.help = Some(text);
                } else if ws.help == Some(text) {
                    ws.help = None;
                }
                cx.notify();
            }))
    }
}

impl Workspace {
    /// The agent's most recent edit, while its outline still shows.
    pub fn agent_presence(&self) -> Option<&AgentEdit> {
        self.agent.last().filter(|a| a.at.elapsed() < PRESENCE)
    }

    pub(crate) fn status_bar(&self) -> impl IntoElement {
        let hint = match (self.view, self.tool, self.bottom) {
            (View::Songs, ..) => {
                "Click a song to open it · ⌘-click to pick several, Delete to move them to the Trash · right-click for more"
            }
            (View::Mixer, ..) => {
                "Click a slot to open it · right-click a send for pre or post · drag a fader, ⌥-click to reset · double-click a name to rename"
            }
            (View::Samples, ..) if self.browser.view == crate::sounds::browser::Results::Map => {
                "Hover a dot to hear it · click to pick it · drag a box to keep what's in it · ⌘-scroll to zoom"
            }
            (View::Plugins, ..) => {
                "Click a plugin to see it · double-click or ↵ puts it on the selected track · drag one out onto a track · F for the compact browser"
            }
            (View::Samples, ..) => {
                "Click a sound to hear it · ↑ ↓ for the next · drag one out to put it on a track · F for the compact browser"
            }
            // Held: what it does to regions, as the grips on their ends show.
            (View::Arrange, _, None) if self.alt => match cfg!(windows) {
                true => {
                    "Alt held: drag a region's end to time-stretch it · drag a region to copy it"
                }
                false => {
                    "⌥ held: drag a region's end to time-stretch it · drag a region to copy it"
                }
            },
            (_, _, None) if self.show_browser => {
                "Click a sound to hear it · ↑ ↓ for the next · drag it onto a track, or double-click to add it · F hides it"
            }
            (_, t, _) if t != Tool::Pointer => t.hint(),
            (_, _, Some(Bottom::PianoRoll)) => {
                "Double-click writes · drag a band to pick · ⌥-drag copies · ↑↓ ⇧↑↓ ←→ move · ⌘C ⌘V ⌘D · Q quantize · right-click for more"
            }
            _ if self.show_automation => {
                "Click for a point · drag to select · drag a line or its ◇ to move or bend it · ⌥-click deletes · ⌘C ⌘V · right-click for shapes · the pencil draws"
            }
            _ => {
                "Drag to move · ⌥-drag to copy · ⌥⇧-drag for an alias · drag an edge to trim · drag the top of its end to loop · ⌥-drag its end to time-stretch · ⇧T stretches audio to the tempo"
            }
        };
        // How to work the arrangement, only over the arrangement.
        let hint = if self.view == View::Arrange { hint } else { "" };
        let (text, color) = match (self.help, &self.status) {
            (Some(h), _) => (h.to_string(), TEXT_2),
            (None, Some(s)) => (s.clone(), TEXT_2),
            (None, None) => (hint.to_string(), TEXT_3),
        };
        let rate = self.player.rate();
        let buffer = self.player.buffer();
        // 48 kHz, 44.1 kHz.
        let khz = format!("{:.1}", rate / 1000.0);
        let khz = khz.trim_end_matches(".0");
        let format = if buffer > 0 {
            format!(
                "{khz} kHz · {buffer} smp · {:.1} ms",
                buffer as f64 / rate * 1000.0
            )
        } else {
            format!("{khz} kHz")
        };
        // Red for a few seconds after a buffer came too late, which is a dropout.
        let late = self
            .load_late
            .is_some_and(|t| t.elapsed() < Duration::from_secs(3));
        let agent = self
            .agent
            .last()
            .filter(|a| a.at.elapsed() < Duration::from_secs(60));
        div()
            .h(px(STATUS_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(12.))
            .bg(rgb(BG))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .text_size(px(11.))
            .text_color(rgb(TEXT_3))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_color(rgb(color))
                    .child(text),
            )
            .when_some(agent, |d, a| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_color(rgb(AGENT_TEXT))
                        .whitespace_nowrap()
                        .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(AGENT)))
                        .child(format!("Studio Agent: {} · {}", a.what, a.ago())),
                )
            })
            .child(
                div()
                    .font_family(MONO)
                    .debug_selector(|| "status-cpu".to_string())
                    .when(late, |d| d.text_color(rgb(METER_HIGH)))
                    .child(format!("CPU {:.0}%", (self.load * 100.0).min(999.0))),
            )
            .child(div().font_family(MONO).child(format))
            .when_some(self.typing, |d, octave| {
                // Musical typing is on: letters are notes, so say so, and where they are.
                let low = (60 + octave as i32 * 12).clamp(0, 127) as u8;
                let high = low.saturating_add(17).min(127);
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_color(rgb(ACCENT_TEXT))
                        .whitespace_nowrap()
                        .child(icon(Icon::Keys, 12.).text_color(rgb(ACCENT_TEXT)))
                        .child(format!(
                            "Musical Typing · {}–{} · Z X octave · ⌘K off",
                            self.note_name(low),
                            self.note_name(high)
                        )),
                )
            })
            .child(match (self.session.edited, self.session.on_disk) {
                (true, _) => "Edited",
                (false, true) => "Saved",
                (false, false) => "Not saved",
            })
    }
}
