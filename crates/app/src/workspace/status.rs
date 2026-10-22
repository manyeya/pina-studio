//! The status bar along the bottom.

use super::*;

/// How long an agent's outline stays on what it touched.
pub const PRESENCE: Duration = Duration::from_secs(12);

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
                "Drag to move · ⌥-drag to copy · ⌥⇧-drag for an alias · drag an edge to trim · drag the top of its end to loop · ⌥-drag its end to time-stretch"
            }
        };
        let (text, color) = match &self.status {
            Some(s) => (s.clone(), TEXT_2),
            None => (hint.to_string(), TEXT_3),
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
                        .child(format!("Agent: {} · {}", a.what, a.ago())),
                )
            })
            .child(format!("CPU {:.0}%", (self.load * 100.0).min(999.0)))
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
