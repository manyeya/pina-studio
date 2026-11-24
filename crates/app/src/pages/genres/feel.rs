//! A genre's feel: tempo, swing and key, its beat (the person's, step by step) and its
//! progressions (theirs, as numerals). Pina Studio has no grooves or progressions of its own.

use super::*;

/// Swing as eight steps: every second one late by it.
pub(crate) fn swing_dots(swing: f32, color: u32) -> Div {
    let late = (swing - 50.0) / 50.0;
    div()
        .relative()
        .w(px(168.))
        .h(px(18.))
        .children((0..8).map(move |k| {
            let x = k as f32 * 21.0 + if k % 2 == 1 { late * 21.0 } else { 0.0 };
            let on = k % 2 == 1;
            div()
                .absolute()
                .left(px(x))
                .top(px(if on { 5. } else { 3. }))
                .size(px(if on { 8. } else { 12. }))
                .rounded(px(6.))
                .bg(if on {
                    rgb(color).into()
                } else {
                    alpha(color, 0.35)
                })
        }))
}

impl Workspace {
    /// Tempo, swing, key, groove and harmony.
    pub(crate) fn genre_feel(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
        let stepper = |label: String,
                       sel: &'static str,
                       cx: &mut Context<Self>,
                       f: fn(&mut Playbook, f64)| {
            let step = |sign: f64, cx: &mut Context<Self>| {
                div()
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(rgb(TEXT_3))
                    .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                    .child(
                        icon(if sign < 0.0 { Icon::Minus } else { Icon::Plus }, 12.)
                            .text_color(rgb(TEXT_2)),
                    )
                    .debug_selector(move || {
                        format!("{sel}-{}", if sign < 0.0 { "down" } else { "up" })
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.edit_genre(false, cx, |p| f(p, sign))),
                    )
            };
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .child(step(-1.0, cx))
                .child(
                    div()
                        .min_w(px(34.))
                        .flex()
                        .justify_center()
                        .font_family(MONO)
                        .text_size(px(20.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(label),
                )
                .child(step(1.0, cx))
        };
        let tempo = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(stepper(
                format!("{:.0}", p.tempo.0),
                "genre-slowest",
                cx,
                |p, d| p.tempo.0 = (p.tempo.0 + d * 2.0).clamp(20.0, p.tempo.1),
            ))
            .child(
                div()
                    .px(px(4.))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_4))
                    .child("to"),
            )
            .child(stepper(
                format!("{:.0}", p.tempo.1),
                "genre-fastest",
                cx,
                |p, d| p.tempo.1 = (p.tempo.1 + d * 2.0).clamp(p.tempo.0, 300.0),
            ));
        let swing = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(stepper(
                format!("{:.0}", p.swing),
                "genre-swing",
                cx,
                |p, d| p.swing = (p.swing + d as f32).clamp(50.0, 75.0),
            ))
            .child(swing_dots(p.swing, color));
        let key = div()
            .flex()
            .gap(px(4.))
            .children([(false, "Major"), (true, "Minor")].map(|(minor, label)| {
                seg(label, p.minor == minor, 28.)
                    .debug_selector(move || format!("genre-{}", label.to_lowercase()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit_genre(false, cx, |p| p.minor = minor)
                        }),
                    )
            }));
        let fields = self.genres.fields.as_ref();
        let harmony = fields.map(|f| f.harmony.clone());
        let groove = fields.map(|f| f.groove.clone());
        let key_for = Key {
            root: 9,
            minor: p.minor,
        };
        let rows: Vec<Div> = fields
            .map(|f| f.progressions.clone())
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                let chords = p
                    .progressions
                    .get(i)
                    .and_then(|n| playbook::progression_chords(n, p.minor))
                    .map(|bars| {
                        bars.iter()
                            .map(|bar| {
                                bar.iter()
                                    .map(|c| c.name_in(Some(&key_for)))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .collect::<Vec<_>>()
                            .join("  ·  ")
                    });
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(field_box(Some(f), "genre-progression"))
                    .when_some(chords, |d, c| {
                        d.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(11.))
                                .text_color(rgb(tint(color, 0.45)))
                                .child(c),
                        )
                    })
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading("Feel", "What every song in it shares"))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(
                        tile("Tempo", "BPM, slowest to fastest")
                            .flex_1()
                            .child(tempo),
                    )
                    .child(
                        tile("Swing", "50 is straight; the late steps lean")
                            .flex_1()
                            .child(swing),
                    )
                    .child(
                        tile("Key", "What its songs are mostly in")
                            .flex_1()
                            .child(key),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(
                        tile("Beat", "Click a step: hit, accent, soft, ghost, off")
                            .flex_1()
                            .child(self.beat_grid(p, color, cx))
                            .child(field_box(groove, "genre-groove")),
                    )
                    .child(
                        tile("Progressions", "Yours, as numerals a bar at a time")
                            .flex_1()
                            .child(field_box(harmony, "genre-harmony"))
                            .children(rows)
                            .child(div().text_size(px(11.)).text_color(rgb(TEXT_4)).child(
                                format!(
                                    "Chords shown in {}. Clear one to take it out. The Studio Agent \
writes from these, and with none writes a few of its own.",
                                    key_for.name()
                                ),
                            )),
                    ),
            )
    }

    /// The book's beat, a row a drum, each step clicked through hit, accent, soft, ghost and off.
    fn beat_grid(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
        let blank = ".".repeat(16);
        let b = p.beat.clone().unwrap_or(playbook::Beat {
            kick: blank.clone(),
            snare: blank.clone(),
            hats: blank,
        });
        let lines = [
            (0usize, "kick", b.kick),
            (1, "snare", b.snare),
            (2, "hats", b.hats),
        ];
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .children(lines.into_iter().map(|(row, name, steps)| {
                let steps: Vec<char> = steps
                    .chars()
                    .chain(std::iter::repeat('.'))
                    .take(16)
                    .collect();
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(
                        div()
                            .w(px(38.))
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_4))
                            .child(name),
                    )
                    .children(steps.into_iter().enumerate().map(|(k, c)| {
                        let level = match c {
                            'X' => 1.0,
                            'x' => 0.78,
                            'o' => 0.55,
                            'g' => 0.3,
                            _ => 0.0,
                        };
                        div()
                            .w(px(14.))
                            .h(px(14.))
                            .rounded(px(3.))
                            .when(k % 4 == 0, |d| d.ml(px(2.)))
                            .cursor_pointer()
                            .bg(match level > 0.0 {
                                false => rgb(WELL).into(),
                                true => alpha(color, level),
                            })
                            .hover(|d| d.border_1().border_color(rgb(LINE_STRONG)))
                            .debug_selector(move || format!("beat-{name}-{k}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.edit_genre(false, cx, |p| step_beat(p, row, k))
                                }),
                            )
                    }))
            }))
    }
}

/// One step of a book's beat moved on: off, hit, accent, soft, ghost, off. A book with no beat
/// gets one, empty but for this step.
pub(crate) fn step_beat(p: &mut Playbook, row: usize, k: usize) {
    let blank = ".".repeat(16);
    let b = p.beat.get_or_insert(playbook::Beat {
        kick: blank.clone(),
        snare: blank.clone(),
        hats: blank,
    });
    let line = match row {
        0 => &mut b.kick,
        1 => &mut b.snare,
        _ => &mut b.hats,
    };
    let mut steps: Vec<char> = line
        .chars()
        .chain(std::iter::repeat('.'))
        .take(16)
        .collect();
    steps[k] = match steps[k] {
        '.' => 'x',
        'x' => 'X',
        'X' => 'o',
        'o' => 'g',
        _ => '.',
    };
    *line = steps.into_iter().collect();
}
