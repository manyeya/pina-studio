//! The song's music: harmony, rhythm and the circle of fifths.

use super::*;

/// The key, and the chords the song uses, on the circle of fifths: majors round the outside,
/// their relative minors inside, each chord a dot as big as the time it's held; and the share of
/// each.
pub(crate) fn harmony(s: &Study, color: u32) -> Div {
    let mut held: Vec<(String, String, f32)> = Vec::new();
    for c in s.chords.iter().filter(|c| c.chord != "N") {
        let d = c.end - c.start;
        match held.iter_mut().find(|h| h.0 == c.chord) {
            Some(h) => h.2 += d,
            None => held.push((c.chord.clone(), c.numeral.clone(), d)),
        }
    }
    let total: f32 = held.iter().map(|h| h.2).sum::<f32>().max(1e-6);
    held.sort_by(|a, b| b.2.total_cmp(&a.2));
    // Shares by place on the wheel: (pitch class, minor) → share.
    let mut places: Vec<((u8, bool), f32)> = Vec::new();
    for (name, _, d) in &held {
        if let Some(c) = theory::Chord::parse(name) {
            let at = (c.root, c.quality.minor());
            match places.iter_mut().find(|p| p.0 == at) {
                Some(p) => p.1 += d / total,
                None => places.push((at, d / total)),
            }
        }
    }
    let key = Key::parse(&s.key.name);
    const SIZE: f32 = 236.0;
    let (r_out, r_in) = (94.0, 60.0);
    let pos = |k: usize, r: f32| {
        let a = (-90.0 + 30.0 * k as f32).to_radians();
        (SIZE / 2.0 + r * a.cos(), SIZE / 2.0 + r * a.sin())
    };
    let names = [
        "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
    ];
    let share_at = |pc: u8, minor: bool| {
        places
            .iter()
            .find(|p| p.0 == (pc, minor))
            .map_or(0.0, |p| p.1)
    };
    let mut marks = Vec::new();
    for k in 0..12usize {
        let major = ((k * 7) % 12) as u8;
        let minor = ((k * 7 + 9) % 12) as u8;
        for (pc, is_minor, r) in [(major, false, r_out), (minor, true, r_in)] {
            let (x, y) = pos(k, r);
            let used = share_at(pc, is_minor) > 0.0;
            let home = key.is_some_and(|k| k.root == pc && k.minor == is_minor);
            marks.push(
                div()
                    .absolute()
                    .left(px(x - 18.))
                    .top(px(y - 8.))
                    .w(px(36.))
                    .h(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(if is_minor { 9.5 } else { 10.5 }))
                    .font_weight(if home {
                        FontWeight::BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(rgb(match (home, used) {
                        (true, _) => TEXT,
                        (_, true) => TEXT_2,
                        _ => TEXT_4,
                    }))
                    .child(format!(
                        "{}{}",
                        names[pc as usize],
                        if is_minor { "m" } else { "" }
                    )),
            );
        }
    }
    let wheel_places = places.clone();
    let wheel = div()
        .relative()
        .w(px(SIZE))
        .h(px(SIZE))
        .flex_none()
        .child(
            canvas(
                |_, _, _| {},
                move |b, _, window, _| fifths(b, key, &wheel_places, color, r_out, r_in, window),
            )
            .absolute()
            .size_full(),
        )
        .children(marks);
    let changes = s
        .key
        .changes
        .iter()
        .filter_map(|(i, k)| s.sections.get(*i).map(|x| format!("{} in {k}", x.name)))
        .collect::<Vec<_>>();
    tile(
        "Harmony",
        "The key, and the chords it holds, on the circle of fifths",
    )
    .flex_1()
    .min_w(px(0.))
    .child(
        div().flex().gap(px(18.)).child(wheel).child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(22.))
                        .font_weight(FontWeight::BOLD)
                        .child(s.key.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .child(format!(
                            "Fit {:.2} · {}",
                            s.key.fit,
                            match s.key.tuning.abs() >= 5.0 {
                                true => format!("tuned {:+.0} cents", s.key.tuning),
                                false => "concert tuning".into(),
                            }
                        )),
                )
                .when(!changes.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(AMBER))
                            .child(format!("Moves: {}", changes.join(", "))),
                    )
                })
                .child(div().h(px(6.)))
                .children(held.iter().take(8).map(|(name, numeral, d)| {
                    let k = d / total;
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .w(px(56.))
                                .flex_none()
                                .font_family(MONO)
                                .text_size(px(12.5))
                                .whitespace_nowrap()
                                .child(name.clone()),
                        )
                        .child(
                            div()
                                .w(px(64.))
                                .flex_none()
                                .font_family(MONO)
                                .text_size(px(11.))
                                .whitespace_nowrap()
                                .text_color(rgb(tint(color, 0.3)))
                                .child(numeral.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .h(px(6.))
                                .rounded(px(3.))
                                .bg(rgb(WELL))
                                .child(
                                    div()
                                        .h_full()
                                        .w(gpui::relative(k.clamp(0.02, 1.0)))
                                        .rounded(px(3.))
                                        .bg(alpha(color, 0.75)),
                                ),
                        )
                        .child(
                            div()
                                .w(px(34.))
                                .flex()
                                .justify_end()
                                .font_family(MONO)
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child(format!("{:.0}%", k * 100.0)),
                        )
                })),
        ),
    )
}

/// The groove: kick, snare and hats as the steps they play most bars, its swing and how
/// syncopated it is.
pub(crate) fn rhythm(s: &Study, color: u32) -> Div {
    let g = &s.groove;
    let row = |name: &'static str, steps: &str| {
        let cells: Vec<f32> = steps
            .chars()
            .map(|c| match c {
                'X' => 1.0,
                'x' => 0.75,
                'o' => 0.5,
                'g' => 0.3,
                _ => 0.0,
            })
            .collect();
        div()
            .flex()
            .items_center()
            .gap(px(3.))
            .child(
                div()
                    .w(px(42.))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(name),
            )
            .children(cells.into_iter().enumerate().map(move |(k, v)| {
                div()
                    .w(px(16.))
                    .h(px(16.))
                    .rounded(px(4.))
                    .when(k % 4 == 0, |d| d.ml(px(3.)))
                    .bg(if v == 0.0 {
                        rgb(WELL).into()
                    } else {
                        alpha(color, 0.2 + 0.8 * v)
                    })
            }))
    };
    let nearest = capital(&g.nearest.replace('_', " "));
    let stat = |name: &'static str, value: String, note: String| {
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
            .child(div().font_family(MONO).text_size(px(17.)).child(value))
            .child(div().text_size(px(11.)).text_color(rgb(TEXT_4)).child(note))
    };
    tile(
        "Rhythm & groove",
        "What its drums play most bars, sixteen steps to the bar",
    )
    .flex_1()
    .min_w(px(0.))
    .child(
        div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .child(row("kick", &g.kick))
            .child(row("snare", &g.snare))
            .child(row("hats", &g.hats)),
    )
    .child(
        div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(swing_dots(g.swing, color))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .child(match g.swing < 52.0 {
                        true => format!("Straight {}", g.swing_on),
                        false => format!("Swung {:.0}% on the {}", g.swing, g.swing_on),
                    }),
            ),
    )
    .child(
        div()
            .flex()
            .gap(px(26.))
            .child(stat(
                "Syncopation",
                format!("{:.0}%", g.syncopation * 100.0),
                "hits off the 8ths".into(),
            ))
            .child(stat(
                "Nearest groove",
                nearest,
                format!("{:.0}% alike", g.likeness * 100.0),
            ))
            .child(stat(
                "Tempo",
                format!("{:.1}", s.tempo.bpm),
                format!("{} beat", steadiness(s.tempo.confidence)),
            )),
    )
}

/// The circle of fifths: its rings, the key's neighbourhood lit, a dot a chord.
pub(crate) fn fifths(
    b: Bounds<Pixels>,
    key: Option<Key>,
    places: &[((u8, bool), f32)],
    color: u32,
    r_out: f32,
    r_in: f32,
    window: &mut Window,
) {
    let (cx0, cy0) = (
        f32::from(b.origin.x) + f32::from(b.size.width) / 2.0,
        f32::from(b.origin.y) + f32::from(b.size.height) / 2.0,
    );
    let at = |k: f32, r: f32| {
        let a = (-90.0 + 30.0 * k).to_radians();
        point(px(cx0 + r * a.cos()), px(cy0 + r * a.sin()))
    };
    let ring = |r: f32, width: f32, fill: gpui::Hsla, window: &mut Window| {
        let mut p = gpui::PathBuilder::stroke(px(width));
        for i in 0..=72 {
            let q = at(i as f32 / 6.0, r);
            if i == 0 {
                p.move_to(q);
            } else {
                p.line_to(q);
            }
        }
        if let Ok(p) = p.build() {
            window.paint_path(p, fill);
        }
    };
    let disc = |k: f32, r: f32, size: f32, fill: gpui::Hsla, window: &mut Window| {
        let a = (-90.0 + 30.0 * k).to_radians();
        let (x, y) = (cx0 + r * a.cos(), cy0 + r * a.sin());
        let mut p = gpui::PathBuilder::fill();
        for i in 0..=24 {
            let t = i as f32 / 24.0 * std::f32::consts::TAU;
            let q = point(px(x + size * t.cos()), px(y + size * t.sin()));
            if i == 0 {
                p.move_to(q);
            } else {
                p.line_to(q);
            }
        }
        p.close();
        if let Ok(p) = p.build() {
            window.paint_path(p, fill);
        }
    };
    // The key's neighbourhood: its own place and a fifth either side, on both rings.
    if let Some(key) = key {
        let k = (0..12).find(|&k| match key.minor {
            false => (k * 7) % 12 == key.root as usize,
            true => (k * 7 + 9) % 12 == key.root as usize,
        });
        if let Some(k) = k {
            let mut p = gpui::PathBuilder::fill();
            let (from, to) = (k as f32 - 1.5, k as f32 + 1.5);
            let steps = 18;
            p.move_to(at(from, r_in - 22.0));
            for i in 0..=steps {
                p.line_to(at(
                    from + (to - from) * i as f32 / steps as f32,
                    r_out + 20.0,
                ));
            }
            for i in (0..=steps).rev() {
                p.line_to(at(
                    from + (to - from) * i as f32 / steps as f32,
                    r_in - 22.0,
                ));
            }
            p.close();
            if let Ok(p) = p.build() {
                window.paint_path(p, alpha(color, 0.1));
            }
        }
    }
    ring(r_out, 1.0, alpha(0xffffff, 0.07), window);
    ring(r_in, 1.0, alpha(0xffffff, 0.07), window);
    let top = places.iter().map(|p| p.1).fold(1e-6, f32::max);
    for &((pc, minor), share) in places {
        let k = (0..12).find(|&k| match minor {
            false => (k * 7) % 12 == pc as usize,
            true => (k * 7 + 9) % 12 == pc as usize,
        });
        let Some(k) = k else { continue };
        let size = 5.0 + 11.0 * (share / top).sqrt();
        let home = key.is_some_and(|key| key.root == pc && key.minor == minor);
        if home {
            disc(
                k as f32,
                if minor { r_in } else { r_out },
                size + 3.0,
                alpha(color, 0.9),
                window,
            );
            disc(
                k as f32,
                if minor { r_in } else { r_out },
                size + 1.5,
                rgb(CARD).into(),
                window,
            );
        }
        disc(
            k as f32,
            if minor { r_in } else { r_out },
            size,
            alpha(color, 0.25 + 0.45 * share / top),
            window,
        );
    }
}
