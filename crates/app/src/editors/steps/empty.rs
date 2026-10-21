//! What the step sequencer offers when there's no pattern yet.

use super::*;

/// A pattern in miniature: up to five rows of sixteen steps, in fours.
pub(crate) fn mini_grid(rows: &[Vec<u8>], color: u32) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .children(rows.iter().map(|cells| {
            div().flex().gap(px(2.)).children((0..16).map(|k| {
                let v = cells.get(k).copied().unwrap_or(0);
                div()
                    .size(px(12.))
                    .rounded(px(2.5))
                    .when(k % 4 == 0 && k > 0, |d| d.ml(px(4.)))
                    .bg(match v {
                        0 if (k / 4) % 2 == 0 => rgb(0x22222a).into(),
                        0 => rgb(0x1a1a20).into(),
                        v => alpha(color, 0.45 + 0.55 * v as f32 / 127.0),
                    })
            }))
        }))
}

impl Workspace {
    /// No pattern to show: one to make, or the track's MIDI to make one of, each shown as what it
    /// would be.
    pub(crate) fn steps_none(&mut self, cx: &mut Context<Self>) -> Div {
        let p = &self.session.project;
        let track = p.tracks.get(self.track);
        let midi = self
            .region
            .filter(|&at| {
                at.0 == self.track && matches!(p.region(at).content, Content::Midi { .. })
            })
            .or_else(|| {
                let t = track?;
                t.regions
                    .iter()
                    .position(|r| matches!(r.content, Content::Midi { .. }))
                    .map(|r| (self.track, r))
            });
        let instrument = track.and_then(|t| t.instrument.clone());
        let name = track.map_or(String::new(), |t| t.name.clone());
        let color = if track.is_some() {
            self.color(self.track)
        } else {
            ACCENT
        };
        let heading = |title: String, about: &'static str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(rgb(TEXT_3))
                        .child(about),
                )
        };
        let Some(instrument) = instrument else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(LANE))
                .child(heading(
                    "Steps need an instrument".into(),
                    "Select a drum kit, sampler or synth track to sequence it in steps.",
                ));
        };
        // What a new pattern starts as: its rows, empty.
        let fresh = match (&instrument, kit_sounds(Some(&instrument))) {
            (Instrument::Drums, _) => STARTING_ROWS.len(),
            (_, Some(sounds)) => sounds.len().min(6),
            _ => 4,
        };
        let empty = vec![vec![0u8; 16]; fresh.min(5)];
        // What the MIDI becomes: its busiest rows, the first bar.
        let converted = midi.map(|at| {
            let r = p.region(at);
            let notes = r
                .content
                .notes(r.length)
                .map(|n| n.to_vec())
                .unwrap_or_default();
            let mut rows: Vec<Vec<u8>> = to_pattern(&notes, PPQ / 4, 16)
                .into_iter()
                .map(|r| r.cells)
                .collect();
            rows.sort_by_key(|c| std::cmp::Reverse(c.iter().filter(|&&v| v > 0).count()));
            rows.truncate(5);
            let label = if r.name.is_empty() {
                name.clone()
            } else {
                r.name.clone()
            };
            (at, rows, notes.len(), label)
        });
        let tile = |title: &'static str, hint: Option<&'static str>, caption: String, grid: Div| {
            div()
                .w(px(300.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .p(px(14.))
                .rounded(px(14.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .cursor_pointer()
                .hover(|d| d.border_color(alpha(color, 0.7)).bg(rgb(0x1e1e24)))
                .child(
                    div()
                        .h(px(104.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(10.))
                        .bg(rgb(WELL))
                        .child(grid),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .children(hint.map(|h| {
                            div()
                                .px(px(6.))
                                .py(px(2.))
                                .rounded(px(5.))
                                .bg(rgb(RAISED))
                                .font_family(MONO)
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_2))
                                .child(h)
                        })),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .child(caption),
                )
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(22.))
            .bg(rgb(LANE))
            .child(heading(
                format!("{name} has no pattern yet"),
                match converted {
                    Some(_) => "Start one, or turn the MIDI already on this track into steps.",
                    None => "Steps are the quickest way to write a beat or a riff.",
                },
            ))
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .gap(px(16.))
                    .child(
                        tile(
                            "New pattern",
                            Some("⌥⌘P"),
                            "One bar of sixteen steps, at the playhead.".into(),
                            mini_grid(&empty, color),
                        )
                        .debug_selector(|| "steps-new".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.new_pattern(cx)),
                        ),
                    )
                    .when_some(converted, |d, (at, rows, count, label)| {
                        d.child(
                            tile(
                                "Convert to steps",
                                None,
                                format!("{label}'s {count} notes, each on its nearest sixteenth."),
                                mini_grid(&rows, color),
                            )
                            .debug_selector(|| "steps-convert".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.midi_to_pattern(at, cx)),
                            ),
                        )
                    }),
            )
    }
}
