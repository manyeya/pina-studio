//! The study's top: the song's identity and vitals.

use super::*;

/// The study's mix, as a song's reference.
pub(crate) fn reference(s: &Study) -> Reference {
    let a = &s.mix.overall;
    let sp = &a.spectrum_db;
    Reference {
        file: s.file.clone(),
        lufs: a.lufs,
        crest_db: a.crest_db,
        spectrum: [sp.sub, sp.bass, sp.low_mid, sp.mid, sp.presence, sp.air],
        width_db: a.stereo.width_db,
        key: s.key.name.clone(),
    }
}

pub(crate) fn steadiness(c: f32) -> &'static str {
    match c {
        c if c > 0.6 => "steady",
        c if c > 0.35 => "fairly steady",
        _ => "loose",
    }
}

pub(crate) fn key_note(s: &Study) -> String {
    let clear = match s.key.fit {
        f if f > 0.7 => "clear",
        f if f > 0.5 => "fair",
        _ => "ambiguous",
    };
    match s.key.tuning.abs() >= 5.0 {
        true => format!("{clear} · {:+.0}¢ off A440", s.key.tuning),
        false => clear.to_string(),
    }
}

impl Workspace {
    /// The song's name and vitals over its shape.
    pub(crate) fn study_hero(&mut self, s: &Study, color: u32, cx: &mut Context<Self>) -> Div {
        let id = &s.identity;
        let title = match id.title.is_empty() {
            true => file_name(&s.file),
            false => id.title.clone(),
        };
        let byline: Vec<String> = [
            id.artist.clone(),
            id.album.clone(),
            id.year.clone(),
            id.genre.clone(),
        ]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect();
        let action = |label: &'static str, sel: &'static str, primary: bool| {
            div()
                .h(px(32.))
                .px(px(13.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(9.))
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .debug_selector(move || sel.into())
                .when(primary, |d| {
                    d.bg(rgb(TEXT))
                        .text_color(rgb(BG))
                        .hover(|d| d.bg(rgb(0xffffff)))
                })
                .when(!primary, |d| {
                    d.bg(alpha(0x000000, 0.35))
                        .border_1()
                        .border_color(alpha(0xffffff, 0.12))
                        .text_color(rgb(TEXT_2))
                        .hover(|d| d.bg(alpha(0xffffff, 0.08)).text_color(rgb(TEXT)))
                })
                .child(label)
        };
        let energy = s.energy.clone();
        let spans: Vec<(usize, usize, u32)> = s
            .sections
            .iter()
            .map(|x| (x.bar, x.bar + x.bars, letter_color(&x.label)))
            .collect();
        let m = &s.mix.overall;
        let swing = match s.groove.swing < 52.0 {
            true => "Straight".to_string(),
            false => format!("{:.0}%", s.groove.swing),
        };
        let vitals = [
            (
                "Tempo",
                format!("{:.0}", s.tempo.bpm),
                format!("BPM · {}", steadiness(s.tempo.confidence)),
            ),
            ("Key", s.key.name.clone(), key_note(s)),
            (
                "Meter",
                format!(
                    "{}/{}",
                    s.tempo.time_signature[0], s.tempo.time_signature[1]
                ),
                format!("{} bars", s.tempo.bars.len()),
            ),
            (
                "Length",
                clock(id.seconds),
                format!("{} sections", s.sections.len()),
            ),
            (
                "Loudness",
                format!("{:.1}", m.lufs),
                format!("LUFS · crest {:.0} dB", m.crest_db),
            ),
            ("Swing", swing, format!("on the {}", s.groove.swing_on)),
        ];
        div()
            .relative()
            .h(px(392.))
            .overflow_hidden()
            .mb(px(30.))
            .child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| dna(b, &energy, &spans, color, window),
                )
                .absolute()
                .size_full(),
            )
            // The sections' names along the shape's foot.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(6.))
                    .flex()
                    .children(s.sections.iter().map(|x| {
                        div()
                            .flex_grow(x.bars.max(1) as f32)
                            .flex_basis(px(0.))
                            .min_w(px(0.))
                            .pl(px(6.))
                            .border_l_1()
                            .border_color(alpha(letter_color(&x.label), 0.5))
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(alpha(tint(letter_color(&x.label), 0.35), 0.95))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(x.name.clone())
                    })),
            )
            .child(
                div()
                    .absolute()
                    .left(px(36.))
                    .right(px(36.))
                    .top(px(28.))
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(16.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .text_size(px(10.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(tint(color, 0.3)))
                                            .child("SONG STUDY")
                                            .child(div().text_color(rgb(TEXT_4)).child("·"))
                                            .child(
                                                div()
                                                    .font_family(MONO)
                                                    .font_weight(FontWeight::NORMAL)
                                                    .text_color(rgb(TEXT_3))
                                                    .whitespace_nowrap()
                                                    .overflow_hidden()
                                                    .text_ellipsis()
                                                    .child(file_name(&s.file)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(36.))
                                            .font_weight(FontWeight::BOLD)
                                            .line_height(px(42.))
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .debug_selector(|| "study-title".into())
                                            .child(title),
                                    )
                                    .when(!byline.is_empty(), |d| {
                                        d.child(
                                            div()
                                                .text_size(px(13.5))
                                                .text_color(rgb(TEXT_2))
                                                .child(byline.join("  ·  ")),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .justify_end()
                                    .max_w(px(470.))
                                    .gap(px(8.))
                                    .child(
                                        action("Start a song like this", "study-start", true)
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, window, cx| {
                                                    ws.song_like_study(window, cx)
                                                }),
                                            ),
                                    )
                                    .child(
                                        action("Save as playbook", "study-playbook", false)
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, _, cx| {
                                                    ws.study_to_playbook(cx)
                                                }),
                                            ),
                                    )
                                    .child(
                                        action("Mix reference", "study-reference", false)
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|ws, _, _, cx| {
                                                    ws.study_as_reference(cx)
                                                }),
                                            ),
                                    ),
                            ),
                    )
                    .child(div().flex().gap(px(30.)).children(vitals.into_iter().map(
                        |(name, value, note)| {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(TEXT_3))
                                        .child(name.to_uppercase()),
                                )
                                .child(
                                    div()
                                        .font_family(MONO)
                                        .text_size(px(24.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(TEXT))
                                        .child(value),
                                )
                                .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(note))
                        },
                    ))),
            )
    }
}
