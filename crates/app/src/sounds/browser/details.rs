//! The details of the picked sample.

use super::*;

impl Workspace {
    /// The editor bar's detail: how many sounds, or how the scan is going.
    pub fn samples_detail(&self) -> String {
        let b = &self.browser;
        match &b.scan {
            Some(p) if p[1].load(Ordering::Relaxed) > 0 => "Analysing…".into(),
            _ if !b.loaded => "Loading…".into(),
            _ => format!("{} sounds", b.library.samples.len()),
        }
    }

    /// The right column: the selected sample, what it is, and what sounds like it.
    pub(crate) fn details(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = &self.browser;
        let column = div()
            .id("sample-details")
            .w(px(DETAILS_W))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE));
        let Some((index, s)) = b.selected.and_then(|i| Some((i, b.sample(i)?))) else {
            return column.child(
                div()
                    .size_full()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .text_center()
                    .child(icon(Icon::Wave, 22.).text_color(rgb(TEXT_4)))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_2))
                            .child("Pick a sound"),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child("Its details show here: what it is, its note or key, tempo and character, and what sounds like it."),
                    ),
            );
        };
        let a = &s.analysis;
        let color = category_color(a.category);
        let heard = b
            .audition
            .filter(|x| x.0 == index)
            .map(|(_, since, secs)| (since.elapsed().as_secs_f32() / secs.max(0.01)).min(1.0));
        let points = a.waveform.clone();
        let big = canvas(
            |_, _, _| {},
            move |bnd, _, window, _| paint_bars(&points, bnd, color, heard, window),
        )
        .size_full();
        let playing = heard.is_some();
        let semis = self.sample_transpose(index);
        // The facts: what it is, its note or key, tempo, length, level and shape.
        let mut facts = vec![fact(
            "Category",
            format!("{} · {:.0}%", a.category.name(), a.confidence * 100.0),
        )];
        if let Some(k) = &a.key {
            facts.push(fact("Key", k.name.clone()));
        } else if let Some(p) = &a.pitch {
            facts.push(fact("Note", note_text(p)));
        }
        if let Some(bpm) = a.bpm {
            facts.push(fact(
                "Tempo",
                match a.bars {
                    Some(bars) => format!(
                        "{bpm:.0} BPM · {bars:.0} bar{}",
                        if bars == 1.0 { "" } else { "s" }
                    ),
                    None => format!("{bpm:.0} BPM"),
                },
            ));
        }
        facts.push(fact("Length", seconds_text(a.seconds)));
        if (-70.0..0.0).contains(&a.loudness) && a.loudness > -69.9 {
            facts.push(fact("Loudness", format!("{} LUFS", db_text(a.loudness))));
        }
        facts.push(fact("Peak", format!("{} dB", db_text(a.peak_db))));
        if a.envelope.attack_ms > 0.0 || a.envelope.decay_ms > 0.0 {
            facts.push(fact(
                "Attack · decay",
                format!(
                    "{:.0} · {:.0} ms",
                    a.envelope.attack_ms, a.envelope.decay_ms
                ),
            ));
        }
        if a.spectrum.centroid_hz > 0.0 {
            let hz = a.spectrum.centroid_hz;
            facts.push(fact(
                "Brightness",
                match hz >= 1000.0 {
                    true => format!("{:.1} kHz", hz / 1000.0),
                    false => format!("{hz:.0} Hz"),
                },
            ));
        }
        facts.push(fact(
            "Stereo",
            match a.channels {
                1 => "Mono".to_string(),
                _ if a.width < 0.05 => "Mono (2 ch)".to_string(),
                _ => format!("Wide {:.0}%", a.width * 100.0),
            },
        ));
        // Where its energy is, band by band.
        let bands = a.spectrum.bands_db;
        let has_bands = bands.iter().any(|&d| d != 0.0);
        let spectrum = div().flex().gap(px(6.)).items_end().children(
            ["Sub", "Bass", "Low", "Mid", "Pres", "Air"]
                .into_iter()
                .zip(bands)
                .map(|(name, db)| {
                    let v = ((db + 48.0) / 48.0).clamp(0.04, 1.0);
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.))
                        .child(
                            div()
                                .w_full()
                                .h(px(34.))
                                .flex()
                                .flex_col()
                                .justify_end()
                                .child(
                                    div()
                                        .w_full()
                                        .h(relative(v))
                                        .rounded(px(2.))
                                        .bg(alpha(color, 0.75)),
                                ),
                        )
                        .child(div().text_size(px(9.5)).text_color(rgb(TEXT_3)).child(name))
                }),
        );
        let reasons = b.hit(index).map(|h| h.reasons.clone()).unwrap_or_default();
        let similar: Vec<Div> = b
            .similar
            .iter()
            .enumerate()
            .filter_map(|(n, h)| {
                let s = b.sample(h.index)?;
                let c = category_color(s.analysis.category);
                let i = h.index;
                Some(
                    div()
                        .h(px(30.))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(OFF)))
                        .debug_selector(move || format!("similar-{n}"))
                        .child(div().w(px(44.)).h(px(16.)).flex_none().child(wave(
                            s.analysis.waveform.clone(),
                            c,
                            0.8,
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_2))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        )
                        .child(
                            div()
                                .font_family(MONO)
                                .text_size(px(10.))
                                .text_color(rgb(TEXT_3))
                                .child(format!("{:.0}%", h.score.clamp(0.0, 1.0) * 100.0)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                ws.select_sample(i, true, cx);
                                let at = pt(e.position);
                                ws.begin_drag(
                                    Drag::Sample {
                                        index: i,
                                        from: at,
                                        at,
                                        moved: false,
                                    },
                                    cx,
                                );
                            }),
                        ),
                )
            })
            .collect();
        let sliders: Vec<Div> =
            (0..ASPECTS.len())
                .map(|i| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child(ASPECTS[i])
                                .child(div().font_family(MONO).child(format!(
                                    "{:.0}",
                                    *aspect(&mut { b.aspects }, i) * 100.0
                                ))),
                        )
                        .child(
                            self.slider_bar(Slider::Aspect(i), cx)
                                .debug_selector(move || format!("aspect-{i}")),
                        )
                })
                .collect();
        let into = match a.kind {
            Kind::Loop => "As audio, in time with the song",
            Kind::OneShot => "On a sampler, in tune",
        };
        column.child(
            div()
                .p(px(14.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(match s.folder.is_empty() {
                                    true => s
                                        .path
                                        .parent()
                                        .map_or(String::new(), |p| p.display().to_string()),
                                    false => s.folder.clone(),
                                }),
                        ),
                )
                .child(
                    div()
                        .h(px(64.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .size(px(36.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(18.))
                                .bg(rgb(if playing { ACCENT } else { TEXT }))
                                .cursor_pointer()
                                .debug_selector(|| "audition".into())
                                .child(
                                    icon(if playing { Icon::Stop } else { Icon::Play }, 16.)
                                        .text_color(rgb(if playing { 0xffffff } else { BG })),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| match playing {
                                        true => ws.stop_audition(cx),
                                        false => ws.audition_sample(index, cx),
                                    }),
                                ),
                        )
                        .child(div().flex_1().h_full().child(big)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .flex_wrap()
                        .child(chip(a.category.name(), color))
                        .child(chip(kind_name(a.kind), TEXT_3))
                        .when(semis.abs() >= 0.05, |d| {
                            d.child(chip(format!("Plays {} st", semis_text(semis)), ACCENT_TEXT))
                        })
                        .children(a.traits.iter().map(|t| chip(t.clone(), TEXT_2))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            primary("Add to song")
                                .w_full()
                                .child(icon(Icon::Plus, 14.).text_color(rgb(0xffffff)))
                                .flex_row_reverse()
                                .debug_selector(|| "add-to-song".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.add_sample(index, cx)),
                                ),
                        )
                        .child(
                            self.give_button(s.path.clone(), 30., cx)
                                .w_full()
                                .justify_center(),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .text_center()
                                .child(format!("{into} · or drag it onto a track")),
                        ),
                )
                .when(!reasons.is_empty(), |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(heading("Why it's here"))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(4.))
                                    .children(reasons.into_iter().map(|r| chip(r, ACCENT_TEXT))),
                            ),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(heading("Sound"))
                        .child(
                            div()
                                .grid()
                                .grid_cols(2)
                                .gap_x(px(12.))
                                .gap_y(px(10.))
                                .children(facts),
                        ),
                )
                .when(has_bands, |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(heading("Spectrum"))
                            .child(spectrum),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(heading("Similar"))
                        .child(
                            div()
                                .grid()
                                .grid_cols(2)
                                .gap_x(px(14.))
                                .gap_y(px(6.))
                                .children(sliders),
                        )
                        .child(match similar.is_empty() {
                            true => div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_4))
                                .child("Nothing else sounds like it yet."),
                            false => div().mt(px(4.)).flex().flex_col().children(similar),
                        }),
                ),
        )
    }
}
