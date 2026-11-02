//! The song's mix: its instruments, barcode and sound DNA.

use super::*;

/// What plays in it: each part heard, how sure, and in which sections.
pub(crate) fn instruments(s: &Study) -> Div {
    let rows = s.instruments.iter().map(|e| {
        let c = role_color(&e.role);
        div()
            .flex()
            .items_center()
            .gap(px(14.))
            .py(px(10.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(div().size(px(10.)).flex_none().rounded(px(5.)).bg(rgb(c)))
            .child(
                div()
                    .w(px(260.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(e.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child(e.about.clone()),
                    ),
            )
            .child(div().flex_1().min_w(px(0.)).flex().gap(px(2.)).children(
                s.sections.iter().enumerate().map(|(i, x)| {
                    let on = e.sections.contains(&i);
                    div()
                        .flex_grow(x.bars.max(1) as f32)
                        .flex_basis(px(0.))
                        .min_w(px(0.))
                        .h(px(10.))
                        .rounded(px(3.))
                        .bg(if on { alpha(c, 0.75) } else { rgb(WELL).into() })
                }),
            ))
            .child(
                div()
                    .w(px(90.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .h(px(4.))
                            .rounded(px(2.))
                            .bg(rgb(WELL))
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(e.confidence.clamp(0.0, 1.0)))
                                    .rounded(px(2.))
                                    .bg(rgb(c)),
                            ),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .child(format!("{:.0}%", e.confidence * 100.0)),
                    ),
            )
    });
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(heading(
            "Instrumentation",
            "What's heard, where in the song, and how sure the ear is",
        ))
        .child(
            div()
                .px(px(18.))
                .py(px(6.))
                .rounded(px(14.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .pt(px(8.))
                        .child(div().w(px(10.)))
                        .child(div().w(px(260.)).child(label("Part")))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .gap(px(2.))
                                .children(s.sections.iter().map(|x| {
                                    div()
                                        .flex_grow(x.bars.max(1) as f32)
                                        .flex_basis(px(0.))
                                        .min_w(px(0.))
                                        .text_size(px(9.5))
                                        .text_color(rgb(letter_color(&x.label)))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(x.name.clone())
                                })),
                        )
                        .child(div().w(px(90.)).child(label("Sure"))),
                )
                .children(rows),
        )
}

/// A study in miniature: its bars' energy, coloured by section.
pub(crate) fn barcode(s: &Study, w: f32, h: f32) -> Div {
    let n = s.energy.len().max(1);
    let step = n.div_ceil(24);
    let bars: Vec<(f32, u32)> = (0..n)
        .step_by(step)
        .map(|b| {
            let e = s.energy.get(b).copied().unwrap_or(0.0);
            let c = s
                .sections
                .iter()
                .find(|x| b >= x.bar && b < x.bar + x.bars)
                .map_or(SLATE, |x| letter_color(&x.label));
            (e, c)
        })
        .collect();
    div()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .p(px(3.))
        .flex()
        .items_center()
        .gap(px(0.5))
        .rounded(px(7.))
        .bg(alpha(tint_of(s), 0.1))
        .children(bars.into_iter().map(|(e, c)| {
            div()
                .flex_1()
                .h(gpui::relative(0.15 + 0.85 * e))
                .rounded(px(1.))
                .bg(alpha(c, 0.45 + 0.5 * e))
        }))
}

impl Workspace {
    /// Mix and dynamics: the numbers, the spectrum (against its nearest genre's target) and
    /// each section's loudness.
    pub(crate) fn study_mix(&mut self, s: &Study, color: u32) -> Div {
        let m = &s.mix.overall;
        let sp = &m.spectrum_db;
        let bands = [sp.sub, sp.bass, sp.low_mid, sp.mid, sp.presence, sp.air];
        let target = s
            .genres
            .first()
            .and_then(|g| self.studies.books.iter().find(|b| b.id == g.id))
            .map(|b| (b.name.clone(), b.spectrum));
        let numbers = [
            ("Integrated", format!("{:.1}", m.lufs), "LUFS"),
            ("Loudest 3 s", format!("{:.1}", m.loudest_lufs), "LUFS"),
            (
                "Peak",
                format!("{:.1}", m.peak_db[0].max(m.peak_db[1])),
                "dBFS",
            ),
            ("Crest", format!("{:.1}", m.crest_db), "dB"),
            ("Range", format!("{:.1}", s.mix.range_db), "dB"),
            ("Width", format!("{:.1}", m.stereo.width_db), "dB sides"),
        ];
        let (lo, hi) = s
            .sections
            .iter()
            .map(|x| x.lufs)
            .filter(|v| *v > -70.0)
            .fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v), h.max(v)));
        let spread = (hi - lo).max(3.0);
        let target_name = target.as_ref().map(|t| t.0.clone());
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(heading(
                "Mix & dynamics",
                "How loud, how wide, where its energy sits, and how it moves",
            ))
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .child(
                        tile("Loudness", "Measured as streaming services do (BS.1770)")
                            .w(px(300.))
                            .flex_none()
                            .child(div().flex().flex_wrap().gap_y(px(14.)).children(
                                numbers.into_iter().map(|(name, v, unit)| {
                                    div()
                                        .w(px(132.))
                                        .flex()
                                        .flex_col()
                                        .gap(px(1.))
                                        .child(
                                            div()
                                                .text_size(px(10.5))
                                                .text_color(rgb(TEXT_3))
                                                .child(name),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_end()
                                                .gap(px(4.))
                                                .child(
                                                    div()
                                                        .font_family(MONO)
                                                        .text_size(px(20.))
                                                        .child(v),
                                                )
                                                .child(
                                                    div()
                                                        .pb(px(3.))
                                                        .text_size(px(10.5))
                                                        .text_color(rgb(TEXT_4))
                                                        .child(unit),
                                                ),
                                        )
                                }),
                            ))
                            .when(m.clipped > 0, |d| {
                                d.child(
                                    div().text_size(px(11.5)).text_color(rgb(METER_HIGH)).child(
                                        format!("{} samples at or past full scale", m.clipped),
                                    ),
                                )
                            }),
                    )
                    .child(
                        tile(
                            "Spectrum",
                            "Each band's share of the energy, dB of the total",
                        )
                        .flex_1()
                        .min_w(px(0.))
                        .child(div().flex().gap(px(10.)).children((0..6).map(|i| {
                            let v = bands[i];
                            let level = ((v - FLOOR) / -FLOOR).clamp(0.0, 1.0);
                            let aim = target
                                .as_ref()
                                .map(|t| ((t.1[i] - FLOOR) / -FLOOR).clamp(0.0, 1.0));
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex()
                                        .justify_center()
                                        .font_family(MONO)
                                        .text_size(px(11.5))
                                        .child(format!("{v:.1}")),
                                )
                                .child(
                                    div()
                                        .h(px(120.))
                                        .relative()
                                        .rounded(px(8.))
                                        .bg(rgb(WELL))
                                        .child(
                                            div()
                                                .absolute()
                                                .bottom_0()
                                                .left_0()
                                                .right_0()
                                                .h(gpui::relative(level))
                                                .rounded(px(8.))
                                                .bg(alpha(color, 0.16 + 0.3 * level))
                                                .border_t_2()
                                                .border_color(rgb(color)),
                                        )
                                        .when_some(aim, |d, a| {
                                            d.child(
                                                div()
                                                    .absolute()
                                                    .left(px(4.))
                                                    .right(px(4.))
                                                    .bottom(gpui::relative(a))
                                                    .h(px(2.))
                                                    .rounded(px(1.))
                                                    .bg(alpha(0xffffff, 0.7)),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .justify_center()
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_2))
                                        .child(BANDS[i].0),
                                )
                        })))
                        .when_some(target_name, |d, name| {
                            d.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child(div().w(px(14.)).h(px(2.)).bg(alpha(0xffffff, 0.7)))
                                    .child(format!("{name}'s target")),
                            )
                        }),
                    ),
            )
            .child(
                tile(
                    "Dynamics",
                    "Each section's loudness: how the song builds and drops",
                )
                .child(div().flex().items_end().gap(px(3.)).h(px(96.)).children(
                    s.sections.iter().map(|x| {
                        let k = ((x.lufs - (hi - spread)) / spread).clamp(0.04, 1.0);
                        let c = letter_color(&x.label);
                        div()
                            .flex_grow(x.bars.max(1) as f32)
                            .flex_basis(px(0.))
                            .min_w(px(0.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .justify_end()
                            .gap(px(3.))
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.))
                                    .text_color(rgb(TEXT_3))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(format!("{:.1}", x.lufs)),
                            )
                            .child(
                                div()
                                    .h(gpui::relative(0.8 * k))
                                    .rounded_t(px(5.))
                                    .bg(alpha(c, 0.25 + 0.4 * k))
                                    .border_t_2()
                                    .border_color(rgb(c)),
                            )
                    }),
                )),
            )
    }
}
