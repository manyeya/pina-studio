//! A genre's feel: swing and groove.

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

/// A groove's bar: its kick, snare (or clap) and hats as steps.
pub(crate) fn groove_grid(id: &str, color: u32) -> Div {
    let g = theory::groove(id);
    let (steps, rows) = theory::drums(g, 1, 0, 1);
    let fine = steps as usize / 16;
    let pick = |notes: &[u8]| {
        rows.iter()
            .filter(|(n, _)| notes.contains(n))
            .map(|(_, cells)| cells.clone())
            .reduce(|a, b| a.iter().zip(&b).map(|(x, y)| (*x).max(*y)).collect())
            .unwrap_or_default()
    };
    let lines = [
        ("kick", pick(&[36])),
        ("snare", pick(&[38, 39, 37])),
        ("hats", pick(&[42, 46, 70])),
    ];
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .children(lines.into_iter().map(move |(name, cells)| {
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
                .children((0..16).map(move |k| {
                    let v = cells.get(k * fine.max(1)).copied().unwrap_or(0);
                    div()
                        .w(px(12.))
                        .h(px(12.))
                        .rounded(px(3.))
                        .when(k % 4 == 0, |d| d.ml(px(2.)))
                        .bg(match v {
                            0 => rgb(WELL).into(),
                            v => alpha(color, 0.25 + 0.75 * v as f32 / 127.0),
                        })
                }))
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
        let grooves: Vec<_> = theory::GROOVES
            .iter()
            .map(|g| {
                let on = g.id == p.groove;
                let id = g.id;
                div()
                    .px(px(8.))
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .rounded(px(7.))
                    .text_size(px(11.5))
                    .cursor_pointer()
                    .border_1()
                    .when(on, |d| {
                        d.bg(alpha(color, 0.18))
                            .border_color(alpha(color, 0.6))
                            .text_color(rgb(TEXT))
                    })
                    .when(!on, |d| {
                        d.border_color(rgb(LINE))
                            .text_color(rgb(TEXT_3))
                            .hover(|d| d.text_color(rgb(TEXT_2)).bg(rgb(OFF)))
                    })
                    .child(id.replace('_', " "))
                    .debug_selector(move || format!("groove-{id}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit_genre(false, cx, |p| p.groove = id.to_string())
                        }),
                    )
            })
            .collect();
        let key_for = Key {
            root: 9,
            minor: p.minor,
        };
        let found = theory::suggest(&key_for, &p.harmony, 0);
        let chords = found
            .numerals
            .split('|')
            .map(|bar| {
                bar.split_whitespace()
                    .filter_map(|c| theory::numeral(c, &key_for).map(|c| c.name_in(Some(&key_for))))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>();
        let harmony = self.genres.fields.as_ref().map(|f| f.harmony.clone());
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
                        tile("Groove", "The drums write_drums plays for it")
                            .flex_1()
                            .child(groove_grid(&p.groove, color))
                            .child(div().flex().flex_wrap().gap(px(5.)).children(grooves)),
                    )
                    .child(
                        tile("Harmony", "Words its chords are found by")
                            .flex_1()
                            .child(field_box(harmony, "genre-harmony"))
                            .child(div().flex().flex_wrap().gap(px(6.)).children(
                                chords.into_iter().map(|c| {
                                    div()
                                        .px(px(9.))
                                        .h(px(26.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(7.))
                                        .bg(alpha(color, 0.1))
                                        .border_1()
                                        .border_color(alpha(color, 0.25))
                                        .font_family(MONO)
                                        .text_size(px(12.))
                                        .text_color(rgb(tint(color, 0.45)))
                                        .child(c)
                                }),
                            ))
                            .child(div().text_size(px(11.)).text_color(rgb(TEXT_4)).child(
                                format!("In {}, the first progression they find", key_for.name()),
                            )),
                    ),
            )
    }
}
