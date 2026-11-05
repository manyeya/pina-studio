//! What the agent shows rather than says: it calls `present` with a kind of card and what goes on
//! it, and the panel draws that. A plan that ticks along as it works, a choice to make by clicking,
//! measurements, chords to hear, a song's shape to jump around, what to do next. Cards are kept
//! with the conversation; what's drawn is everything the agent sent, so an old one looks the same.

use pina_engine::{
    music::theory::Chord,
    song::midi_file::{MidiFile, Part},
    song::project::{Note, PPQ},
};
use serde_json::Value;

use super::*;

/// The chords being heard, where a browser file's index would be.
const CHORDS_HEARD: usize = usize::MAX - 1;

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_default()
}

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().map_or(&[], Vec::as_slice)
}

/// Whether a tool call is one of these.
pub(crate) fn is_card(name: &str) -> bool {
    name.strip_prefix("mcp__pina__").unwrap_or(name) == "present"
}

/// The frame every card sits in.
fn frame(title: &str, i: Icon) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .p(px(11.))
        .rounded(px(10.))
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(LINE))
        .when(!title.is_empty(), |d| {
            d.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(icon(i, 12.).flex_none().text_color(rgb(AGENT_TEXT)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.5))
                            .line_height(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(title.to_string()),
                    ),
            )
        })
}

/// A step of a plan's mark: to do, being done, done.
fn step_mark(state: &str, n: usize) -> gpui::AnyElement {
    match state {
        "done" => div()
            .size(px(15.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .bg(alpha(METER_LOW, 0.18))
            .child(icon(Icon::Check, 10.).text_color(rgb(METER_LOW)))
            .into_any_element(),
        "doing" => div()
            .size(px(15.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(AGENT)))
            .with_animation(
                ("plan-step", n),
                Animation::new(Duration::from_millis(1100))
                    .repeat()
                    .with_easing(gpui::pulsating_between(0.3, 1.0)),
                |d, t| d.opacity(t),
            )
            .into_any_element(),
        _ => div()
            .size(px(15.))
            .flex_none()
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .into_any_element(),
    }
}

/// What's planned, and how far it's got.
fn plan_card(n: usize, v: &Value) -> Div {
    let steps = list(v, "steps");
    let done = steps.iter().filter(|s| text(s, "state") == "done").count();
    let share = done as f32 / steps.len().max(1) as f32;
    let title = match text(v, "title") {
        "" => "The plan",
        t => t,
    };
    frame(title, Icon::Check)
        .debug_selector(move || format!("card-plan-{n}"))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .h(px(3.))
                        .rounded(px(2.))
                        .bg(rgb(LINE))
                        .overflow_hidden()
                        .child(
                            div()
                                .h_full()
                                .w(gpui::relative(share))
                                .rounded(px(2.))
                                .bg(rgb(METER_LOW)),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .font_family(MONO)
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .child(format!("{done} of {}", steps.len())),
                ),
        )
        .children(steps.iter().enumerate().map(|(k, s)| {
            let state = text(s, "state");
            div()
                .flex()
                .gap(px(8.))
                .child(step_mark(state, n * 100 + k))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .text_color(rgb(match state {
                            "done" => TEXT_3,
                            "doing" => TEXT,
                            _ => TEXT_2,
                        }))
                        .child(text(s, "text").to_string()),
                )
        }))
}

/// Measurements: a tile each, coloured by how it stands against its target.
fn metrics_card(n: usize, v: &Value) -> Div {
    let tiles = list(v, "metrics").iter().map(|m| {
        let color = match text(m, "state") {
            "good" => METER_LOW,
            "warn" => METER_MID,
            "bad" => METER_HIGH,
            _ => TEXT_4,
        };
        let target = text(m, "target");
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(px(8.))
            .rounded(px(7.))
            .bg(rgb(WELL))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded(px(3.))
                            .bg(rgb(color)),
                    )
                    .child(
                        div()
                            .min_w(px(0.))
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(text(m, "label").to_string()),
                    ),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(15.))
                    .line_height(px(20.))
                    .text_color(rgb(TEXT))
                    .whitespace_nowrap()
                    .child(text(m, "value").to_string()),
            )
            .when(!target.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_4))
                        .whitespace_nowrap()
                        .child(format!("aim {target}")),
                )
            })
    });
    let title = match text(v, "title") {
        "" => "Measured",
        t => t,
    };
    frame(title, Icon::Wave)
        .debug_selector(move || format!("card-metrics-{n}"))
        .child(div().grid().grid_cols(2).gap(px(6.)).children(tiles))
}

/// A progression as a MIDI file, a bar a chord in the middle of the keyboard.
fn chords_file(names: &[String]) -> MidiFile {
    let bar = PPQ * 4;
    let notes = names.iter().enumerate().flat_map(|(i, name)| {
        let chord = Chord::parse(name);
        let classes = chord.map_or(Vec::new(), |c| c.classes());
        let root = chord.map_or(0, |c| c.root);
        classes.into_iter().map(move |pc| {
            // Up from the root, so the chord stands the right way up.
            let up = (pc as i32 - root as i32).rem_euclid(12) as u8;
            Note {
                start: (i as u64 * bar) as i64,
                length: bar,
                pitch: 48 + root % 12 + up,
                velocity: 90,
                ..Default::default()
            }
        })
    });
    MidiFile {
        parts: vec![Part {
            name: "Chords".into(),
            channel: 0,
            program: None,
            notes: notes.collect(),
            controls: Vec::new(),
        }],
        time_signature: Some([4, 4]),
        ..Default::default()
    }
}

impl Workspace {
    /// A card the agent sent, if it's one it can draw.
    pub(crate) fn card(
        &self,
        n: usize,
        v: &Value,
        items: &[Item],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        match v["kind"].as_str()? {
            "plan" => Some(plan_card(n, v)),
            "metrics" => Some(metrics_card(n, v)),
            "choices" => Some(self.choices_card(n, v, items, cx)),
            "chords" => Some(self.chords_card(n, v, cx)),
            "sections" => Some(self.sections_card(n, v, cx)),
            "next" => self.next_card(n, v, items, cx),
            _ => None,
        }
    }

    /// A question to answer by clicking: what's picked goes as the next thing said.
    fn choices_card(&self, n: usize, v: &Value, items: &[Item], cx: &mut Context<Self>) -> Div {
        let answered = items[n..].iter().find_map(|i| match i {
            Item::You(t) => Some(t.clone()),
            _ => None,
        });
        let open = answered.is_none() && !self.chat.busy;
        let options = list(v, "options").iter().enumerate().map(|(k, o)| {
            let label = text(o, "label").to_string();
            let ask = match text(o, "ask") {
                "" => label.clone(),
                a => a.to_string(),
            };
            let picked = answered.as_ref() == Some(&ask);
            let detail = text(o, "detail").to_string();
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(9.))
                .py(px(7.))
                .rounded(px(8.))
                .border_1()
                .border_color(if picked {
                    alpha(ACCENT, 0.6)
                } else {
                    rgb(LINE_STRONG).into()
                })
                .when(picked, |d| d.bg(alpha(ACCENT, 0.12)))
                .when(!open && !picked, |d| d.opacity(0.45))
                .when(open, |d| {
                    d.cursor_pointer()
                        .hover(|d| d.bg(rgb(RAISED)).border_color(rgb(TEXT_4)))
                })
                .debug_selector(move || format!("choice-{n}-{k}"))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .line_height(px(17.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(TEXT))
                                .child(label),
                        )
                        .when(!detail.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_size(px(11.))
                                    .line_height(px(15.))
                                    .text_color(rgb(TEXT_3))
                                    .child(detail),
                            )
                        }),
                )
                .child(match picked {
                    true => icon(Icon::Check, 12.)
                        .flex_none()
                        .text_color(rgb(ACCENT_TEXT)),
                    false => icon(Icon::Forward, 11.).flex_none().text_color(rgb(TEXT_4)),
                })
                .when(open, |d| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.ask_agent(ask.clone(), cx);
                        }),
                    )
                })
        });
        let title = match text(v, "title") {
            "" => "Which way?",
            t => t,
        };
        frame(title, Icon::Agent)
            .debug_selector(move || format!("card-choices-{n}"))
            .children(options)
    }

    /// Chords as chips, a numeral under each, with a way to hear them.
    fn chords_card(&self, n: usize, v: &Value, cx: &mut Context<Self>) -> Div {
        let chips = list(v, "chords");
        let names: Vec<String> = chips
            .iter()
            .map(|c| c.as_str().unwrap_or(text(c, "name")).to_string())
            .collect();
        let heard = self.midi_heard(CHORDS_HEARD);
        let title = match (text(v, "title"), text(v, "key")) {
            ("", "") => "Chords".to_string(),
            ("", k) => format!("Chords in {k}"),
            (t, _) => t.to_string(),
        };
        let row = chips.iter().enumerate().map(|(k, c)| {
            let numeral = text(c, "numeral").to_string();
            div()
                .min_w(px(40.))
                .px(px(9.))
                .py(px(5.))
                .flex()
                .flex_col()
                .items_center()
                .rounded(px(7.))
                .bg(rgb(WELL))
                .border_1()
                .border_color(rgb(LINE_SOFT))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(12.5))
                        .text_color(rgb(TEXT))
                        .child(names[k].clone()),
                )
                .when(!numeral.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(9.5))
                            .text_color(rgb(TEXT_4))
                            .child(numeral),
                    )
                })
        });
        let file = chords_file(&names);
        let name = std::path::PathBuf::from(format!("chords:{}", names.join(" ")));
        frame(&title, Icon::Keys)
            .debug_selector(move || format!("card-chords-{n}"))
            .child(div().flex().flex_wrap().gap(px(4.)).children(row))
            .child(
                div().flex().child(
                    div()
                        .h(px(24.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .rounded(px(6.))
                        .text_size(px(11.5))
                        .text_color(rgb(if heard { TEXT } else { TEXT_2 }))
                        .bg(rgb(if heard { RAISED } else { WELL }))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(RAISED)))
                        .debug_selector(move || format!("chords-hear-{n}"))
                        .child(
                            icon(if heard { Icon::Stop } else { Icon::Play }, 10.)
                                .text_color(rgb(TEXT_2)),
                        )
                        .child(if heard { "Stop" } else { "Hear them" })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                match heard {
                                    true => ws.stop_audition(cx),
                                    false => {
                                        let f = std::sync::Arc::new(file.clone());
                                        ws.hear_midi(CHORDS_HEARD, name.clone(), vec![(f, 0)], cx);
                                    }
                                }
                            }),
                        ),
                ),
            )
    }

    /// The song's shape: each section as wide as it's long, as tall as it's strong. A click goes
    /// there.
    fn sections_card(&self, n: usize, v: &Value, cx: &mut Context<Self>) -> Div {
        const TALL: f32 = 44.0;
        let parts = list(v, "sections");
        let mut start = 1;
        let blocks = parts.iter().enumerate().map(|(k, s)| {
            let bars = s["bars"].as_u64().unwrap_or(4).max(1);
            let energy = s["energy"].as_f64().unwrap_or(0.5).clamp(0.1, 1.0) as f32;
            let at = s["bar"].as_u64().unwrap_or(start);
            start = at + bars;
            div()
                .flex_grow(bars as f32)
                .flex_basis(px(0.))
                .min_w(px(14.))
                .h(px(TALL))
                .flex()
                .items_end()
                .cursor_pointer()
                .hover(|d| d.opacity(0.75))
                .debug_selector(move || format!("section-{n}-{k}"))
                .child(
                    div()
                        .w_full()
                        .h(px(TALL * energy))
                        .rounded_t(px(3.))
                        .bg(alpha(AGENT, 0.2 + 0.5 * energy)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        let bar = ws.session.project.bar();
                        ws.locate_tick(at.saturating_sub(1) * bar, cx);
                    }),
                )
        });
        let names = parts
            .iter()
            .map(|s| format!("{} {}", text(s, "name"), s["bars"].as_u64().unwrap_or(4)))
            .collect::<Vec<_>>()
            .join(" · ");
        let title = match text(v, "title") {
            "" => "The song's shape",
            t => t,
        };
        frame(title, Icon::Grid)
            .debug_selector(move || format!("card-sections-{n}"))
            .child(div().flex().items_end().gap(px(2.)).children(blocks))
            .child(
                div()
                    .text_size(px(10.5))
                    .line_height(px(15.))
                    .text_color(rgb(TEXT_3))
                    .child(names),
            )
    }

    /// What to do next: only while nothing's been asked since.
    fn next_card(
        &self,
        n: usize,
        v: &Value,
        items: &[Item],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let later = items[n..].iter().any(|i| matches!(i, Item::You(_)));
        if later || self.chat.busy {
            return None;
        }
        let chips = list(v, "options").iter().enumerate().map(|(k, o)| {
            let label = o.as_str().unwrap_or(text(o, "label")).to_string();
            let ask = match text(o, "ask") {
                "" => label.clone(),
                a => a.to_string(),
            };
            div()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(13.))
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .text_size(px(11.5))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .debug_selector(move || format!("next-{n}-{k}"))
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.ask_agent(ask.clone(), cx);
                    }),
                )
        });
        Some(div().flex().flex_wrap().gap(px(5.)).children(chips))
    }
}

impl Chat {
    /// A card sent again under the same `id` takes the first's place, so a plan ticks along where
    /// it is rather than repeating down the page.
    pub(crate) fn update_card(&mut self, id: &str) {
        let Some(at) = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::Tool { id: x, name, .. } if x == id && is_card(name)))
        else {
            return;
        };
        let Item::Tool { input, .. } = &self.items[at] else {
            return;
        };
        let (key, input) = (input["id"].clone(), input.clone());
        if !key.is_string() {
            return;
        }
        let first = self.items[..at].iter_mut().find_map(|i| match i {
            Item::Tool { name, input, .. } if is_card(name) && input["id"] == key => Some(input),
            _ => None,
        });
        if let Some(first) = first {
            *first = input;
            self.items.remove(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_stand_on_their_roots() {
        let file = chords_file(&["Am".to_string(), "F".to_string()]);
        let notes = &file.parts[0].notes;
        let first: Vec<u8> = notes
            .iter()
            .filter(|n| n.start == 0)
            .map(|n| n.pitch)
            .collect();
        // A C E, from the root up.
        assert_eq!(first, [57, 60, 64]);
        assert_eq!(file.end(), PPQ * 8);
    }

    #[test]
    fn a_chord_that_cant_be_read_is_silence_not_a_crash() {
        let file = chords_file(&["not a chord".to_string()]);
        assert!(file.parts[0].notes.is_empty());
    }
}
