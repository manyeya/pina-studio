//! The roll's tabs that write notes. Generate: one writer at a time (a rhythm spread evenly, named
//! when the world knows it; a melody grown from a seed in the key; one following a drawn shape),
//! what it writes over said, and a button to write it at the foot. Chords: the key's chords,
//! coloured by what they do, and those it borrows; or progressions by mood, each to hear first;
//! the chords picked kept at the foot with how they're voiced and played, a bass under them,
//! and a button to write them.

use pina_engine::music::{
    Key,
    generate::{self, Contour},
    harmony::{self, Performance, Voicing},
    theory::Chord,
};

use super::*;

/// What the Generate and Harmony tabs are set to, and the chords picked to write.
#[derive(Clone, Debug)]
pub struct Writing {
    /// A rhythm: hits of steps, turned on.
    pub pulses: u8,
    pub steps: u8,
    pub rotate: u8,
    /// A melody: its lowest and highest notes, how many steps play, its seed, its shape.
    pub low: u8,
    pub high: u8,
    pub density: u8,
    pub seed: u64,
    pub contour: Contour,
    /// Whether what's written takes the place of the region's notes.
    pub replace: bool,
    /// Which writer Generate shows, and whether Chords shows the key's chords or progressions.
    pub writer: Writer,
    pub progressions: bool,
    /// Chords: sevenths or triads, those picked in order, how they're voiced and played, a bass
    /// under them, how many beats each, and the playbook (by id) the progressions are narrowed to.
    pub sevenths: bool,
    pub chords: Vec<Chord>,
    pub voicing: Voicing,
    pub performance: Performance,
    pub bass: bool,
    pub beats: u8,
    pub book: Option<String>,
}

impl Default for Writing {
    fn default() -> Self {
        Writing {
            pulses: 3,
            steps: 8,
            rotate: 0,
            low: 60,
            high: 72,
            density: 70,
            seed: 1,
            contour: Contour::Arch,
            replace: true,
            writer: Writer::Rhythm,
            progressions: false,
            sevenths: false,
            chords: Vec::new(),
            voicing: Voicing::Led,
            performance: Performance::Block,
            bass: false,
            beats: 4,
            book: None,
        }
    }
}

/// Generate's writers.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Writer {
    Rhythm,
    Melody,
    Shape,
}

impl Writer {
    const ALL: [Writer; 3] = [Writer::Rhythm, Writer::Melody, Writer::Shape];

    fn name(self) -> &'static str {
        match self {
            Writer::Rhythm => "Rhythm",
            Writer::Melody => "Melody",
            Writer::Shape => "Shape",
        }
    }
}

/// Rhythms the world knows, as hits in steps: each spreads its hits as evenly as they go.
const RHYTHMS: [(&str, u8, u8); 6] = [
    ("Tresillo", 3, 8),
    ("Cinquillo", 5, 8),
    ("Bossa nova", 5, 16),
    ("Samba", 7, 16),
    ("Bell", 7, 12),
    ("Four on the floor", 4, 16),
];

/// A chord's colour by what it does in the key (its place among the key's seven): at rest,
/// leaning away, or pulling home; and borrowed ones.
const TONIC: u32 = 0x5b8def;
const SUBDOMINANT: u32 = 0x3ddc97;
const DOMINANT: u32 = 0xff9f43;
const BORROWED: u32 = 0xb388ff;

fn function(degree: usize) -> u32 {
    match degree {
        1 | 3 => SUBDOMINANT,
        4 | 6 => DOMINANT,
        _ => TONIC,
    }
}

/// A small label over a part of a tab.
fn part(name: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT_2))
        .child(name.into())
}

/// A quiet line saying what something does.
fn say(text: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(10.5))
        .line_height(px(14.))
        .text_color(rgb(TEXT_3))
        .child(text.into())
}

/// A row's chip, picked or not.
fn pick(text: impl Into<gpui::SharedString>, on: bool) -> Div {
    div()
        .h(px(22.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .text_size(px(11.))
        .cursor_pointer()
        .bg(if on {
            alpha(ACCENT, 0.2)
        } else {
            rgb(FIELD).into()
        })
        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
        .when(!on, |d| d.hover(|s| s.bg(rgb(RAISED))))
        .child(text.into())
}

fn row(children: impl IntoIterator<Item = Div>) -> Div {
    div().flex().flex_wrap().gap(px(4.)).children(children)
}

impl Workspace {
    /// Notes written into the shown region from its start (in place of what's there, if set to),
    /// the region grown to hold them: one edit.
    pub(crate) fn write_notes(&mut self, written: Vec<Note>, cx: &mut Context<Self>) {
        let Some(at) = self.roll_region() else {
            self.status =
                Some("Pick a MIDI region to write into, or draw one with the pencil.".into());
            return cx.notify();
        };
        let replace = self.roll_writing.replace;
        let n = written.len();
        self.edit(cx, |ws| {
            if let Some(notes) = ws.session.project.notes_mut(at) {
                if replace {
                    notes.clear();
                }
                let first = notes.len();
                notes.extend(written);
                ws.picked = (first..notes.len()).collect();
            }
            let end = ws
                .session
                .project
                .region(at)
                .content
                .notes(0)
                .map_or(0, |list| {
                    list.iter()
                        .map(|n| n.start + n.length as i64)
                        .max()
                        .unwrap_or(0)
                });
            let bar = ws.session.project.bar().max(1);
            let r = &mut ws.session.project.tracks[at.0].regions[at.1];
            if end as u64 > r.length {
                r.length = (end as u64).div_ceil(bar) * bar;
            }
        });
        self.status = Some(format!("Wrote {n} notes"));
        cx.notify();
    }

    /// How long the shown region is (a bar with none).
    fn span(&self) -> u64 {
        self.roll_region()
            .map_or(self.session.project.bar(), |at| {
                self.session.project.region(at).length
            })
            .max(1)
    }

    /// A chord heard on the shown region's track, held a moment.
    pub(crate) fn audition_chord(&mut self, pitches: Vec<u8>, cx: &mut Context<Self>) {
        let Some(at) = self.roll_region() else { return };
        let track = self.session.project.tracks[at.0].id;
        for &p in &pitches {
            self.player.preview(track, p, 96);
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(700))
                .await;
            let _ = this.update(cx, |ws, _| {
                for &p in &pitches {
                    ws.player.preview(track, p, 0);
                }
            });
        })
        .detach();
    }

    /// Changes what the tabs are set to.
    fn set_writing(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Writing)) {
        let was = self.roll_writing.progressions;
        f(&mut self.roll_writing);
        // Progressions opened: the playbooks as they are now (the agent or a hand may have changed them).
        if self.roll_writing.progressions && !was {
            self.genres.list = pina_engine::music::playbook::load().0;
        }
        cx.notify();
    }

    /// A number named over it, with a step down and up: three fit across the panel.
    fn small_stepper(
        &self,
        name: &'static str,
        value: String,
        step: impl Fn(&mut Writing, i32) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let button = |sign: &'static str, by: i32, cx: &mut Context<Self>| {
            let step = step.clone();
            div()
                .size(px(20.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .bg(rgb(FIELD))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(RAISED)))
                .debug_selector(move || format!("write-{name}-{sign}"))
                .child(sign)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.set_writing(cx, |w| step(w, by));
                    }),
                )
        };
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(name),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h(px(22.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .bg(rgb(WELL))
                            .font_family(MONO)
                            .text_size(px(11.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(value),
                    )
                    .child(button("−", -1, cx))
                    .child(button("+", 1, cx)),
            )
    }

    /// The tab's own tabs: one of a few, picked.
    fn subtabs<T: Copy + PartialEq + 'static>(
        &self,
        items: Vec<(T, &'static str)>,
        now: T,
        set: fn(&mut Writing, T),
        cx: &mut Context<Self>,
    ) -> Div {
        tray()
            .p(px(2.))
            .rounded(px(7.))
            .children(items.into_iter().map(|(v, label)| {
                seg(label, v == now, 22.)
                    .flex_1()
                    .min_w(px(0.))
                    .px(px(4.))
                    .text_size(px(11.))
                    .debug_selector(move || format!("write-tab-{label}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.set_writing(cx, |w| set(w, v))),
                    )
            }))
    }

    /// The button that writes, wide and plain at the foot of a tab; greyed out with nothing
    /// to write.
    fn write_button(
        &self,
        label: String,
        id: &'static str,
        ready: bool,
        cx: &mut Context<Self>,
        notes: impl Fn(&Workspace) -> Vec<Note> + 'static,
    ) -> Div {
        button(label, true)
            .flex_1()
            .min_w(px(0.))
            .h(px(28.))
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .debug_selector(move || format!("write-{id}"))
            .when(!ready, |d| d.opacity(0.4).cursor_default())
            .when(ready, |d| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        let written = notes(ws);
                        ws.write_notes(written, cx);
                    }),
                )
            })
    }

    /// Replace the region's notes, or add to them.
    fn replace_or_add(&self, cx: &mut Context<Self>) -> Div {
        let replace = self.roll_writing.replace;
        tray().flex_none().p(px(2.)).rounded(px(7.)).children(
            [("Replace", true), ("Add", false)].map(|(label, r)| {
                seg(label, replace == r, 24.)
                    .px(px(8.))
                    .text_size(px(11.))
                    .debug_selector(move || format!("write-{}", label.to_lowercase()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.set_writing(cx, |w| w.replace = r)),
                    )
            }),
        )
    }

    /// Where what's written goes: the region's bars, and how far apart.
    fn over_what(&self) -> String {
        let bar = self.session.project.bar().max(1);
        let bars = self.span().div_ceil(bar).max(1);
        let grid = length_name(self.grid());
        match bars {
            1 => format!("Over its bar, a {grid} apart"),
            n => format!("Over its {n} bars, a {grid} apart"),
        }
    }

    /// A shape a melody can follow, drawn.
    fn contour_tile(&self, c: Contour, cx: &mut Context<Self>) -> Div {
        let on = self.roll_writing.contour == c;
        let glyph = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let mut path = gpui::PathBuilder::stroke(px(1.75));
                for k in 0..=24 {
                    let t = k as f32 / 24.0;
                    let at = point(px(x0 + t * w), px(y0 + h - c.at(t as f64) as f32 * h));
                    match k {
                        0 => path.move_to(at),
                        _ => path.line_to(at),
                    }
                }
                if let Ok(p) = path.build() {
                    window.paint_path(p, if on { rgb(ACCENT) } else { rgb(TEXT_3) });
                }
            },
        )
        .w(px(30.))
        .h(px(14.));
        div()
            .flex_1()
            .min_w(px(0.))
            .h(px(46.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .rounded(px(7.))
            .bg(if on {
                alpha(ACCENT, 0.16)
            } else {
                rgb(FIELD).into()
            })
            .when(on, |d| d.border_1().border_color(alpha(ACCENT, 0.6)))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(RAISED)))
            .debug_selector(move || format!("write-shape-{}", c.name()))
            .child(glyph)
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                    .child(c.name()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| ws.set_writing(cx, |w| w.contour = c)),
            )
    }

    /// The lowest and highest notes a melody goes between.
    fn range_steppers(&self, cx: &mut Context<Self>) -> Div {
        let w = &self.roll_writing;
        div()
            .flex()
            .gap(px(8.))
            .child(self.small_stepper(
                "Lowest",
                self.note_name(w.low),
                |w, by| w.low = (w.low as i32 + by).clamp(0, w.high as i32) as u8,
                cx,
            ))
            .child(self.small_stepper(
                "Highest",
                self.note_name(w.high),
                |w, by| w.high = (w.high as i32 + by).clamp(w.low as i32, 127) as u8,
                cx,
            ))
    }

    /// One writer at a time, its settings, and what it writes over; at the foot, replace or add,
    /// and the button that writes.
    pub(crate) fn generate_tab(&self, cx: &mut Context<Self>) -> (Div, Option<Div>) {
        let w = &self.roll_writing;
        let key = self.session.project.song_key();
        let scale = key.map_or("every pitch: the song has no key".to_string(), |k| {
            format!("the {} scale", k.name())
        });
        let body = match w.writer {
            Writer::Rhythm => {
                let hits = generate::euclid(w.pulses, w.steps, w.rotate);
                let pattern = div()
                    .flex()
                    .gap(px(2.))
                    .children(hits.iter().enumerate().map(|(k, &h)| {
                        div().flex_1().min_w(px(0.)).h(px(22.)).rounded(px(3.)).bg(
                            match (h, k % 4 == 0) {
                                (true, _) => rgb(ACCENT).into(),
                                (false, true) => alpha(TEXT, 0.14),
                                (false, false) => alpha(TEXT, 0.07),
                            },
                        )
                    }));
                let known = RHYTHMS
                    .iter()
                    .find(|r| (r.1, r.2) == (w.pulses, w.steps))
                    .map(|r| r.0);
                let pitch = self
                    .roll_region()
                    .and_then(|at| self.picked_notes(at).first().map(|p| p.1.pitch));
                let named = RHYTHMS.map(|(name, hits, of)| {
                    pick(name, known == Some(name))
                        .debug_selector(move || format!("write-rhythm-{name}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.set_writing(cx, |w| {
                                    (w.pulses, w.steps, w.rotate) = (hits, of, 0)
                                })
                            }),
                        )
                });
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(pattern)
                    .child(say(match known {
                        Some(name) => format!("{} hits in {} steps: {name}", w.pulses, w.steps),
                        None => format!(
                            "{} hits in {} steps, spread as evenly as they go",
                            w.pulses, w.steps
                        ),
                    }))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(self.small_stepper(
                                "Hits",
                                w.pulses.to_string(),
                                |w, by| {
                                    w.pulses = (w.pulses as i32 + by).clamp(1, w.steps as i32) as u8
                                },
                                cx,
                            ))
                            .child(self.small_stepper(
                                "Steps",
                                w.steps.to_string(),
                                |w, by| {
                                    w.steps = (w.steps as i32 + by).clamp(2, 32) as u8;
                                    w.pulses = w.pulses.min(w.steps);
                                },
                                cx,
                            ))
                            .child(self.small_stepper(
                                "Turn",
                                w.rotate.to_string(),
                                |w, by| {
                                    w.rotate = (w.rotate as i32 + by)
                                        .rem_euclid(w.steps.max(1) as i32)
                                        as u8
                                },
                                cx,
                            )),
                    )
                    .child(part("Known rhythms"))
                    .child(row(named))
                    .child(say(match pitch {
                        Some(p) => format!("On {}, the picked note's pitch.", self.note_name(p)),
                        None => "On C3. Pick a note to write it on that pitch.".to_string(),
                    }))
            }
            Writer::Melody => div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(say(format!(
                    "Steps and now and then leaps up and down {scale}, a seed deciding which."
                )))
                .child(self.range_steppers(cx))
                .child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(self.small_stepper(
                            "Plays",
                            format!("{}%", w.density),
                            |w, by| w.density = (w.density as i32 + by * 10).clamp(10, 100) as u8,
                            cx,
                        ))
                        .child(self.small_stepper(
                            "Seed",
                            w.seed.to_string(),
                            |w, by| w.seed = (w.seed as i64 + by as i64).max(1) as u64,
                            cx,
                        )),
                ),
            Writer::Shape => div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(say(format!(
                    "A note each step, as high on {scale} as the shape is there."
                )))
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .children(Contour::ALL.map(|c| self.contour_tile(c, cx))),
                )
                .child(self.range_steppers(cx)),
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(self.subtabs(
                Writer::ALL.map(|v| (v, v.name())).to_vec(),
                w.writer,
                |w, v| w.writer = v,
                cx,
            ))
            .child(body);
        let ready = self.roll_region().is_some();
        let write = match w.writer {
            Writer::Rhythm => {
                self.write_button("Write rhythm".into(), "Write rhythm", ready, cx, |ws| {
                    let w = &ws.roll_writing;
                    // On the picked note's pitch, else middle C.
                    let pitch = ws
                        .roll_region()
                        .and_then(|at| ws.picked_notes(at).first().map(|p| p.1.pitch))
                        .unwrap_or(60);
                    generate::rhythm(
                        (w.pulses, w.steps, w.rotate),
                        ws.grid(),
                        ws.span(),
                        pitch,
                        100,
                    )
                })
            }
            Writer::Melody => self.write_button(
                "Grow a melody".into(),
                "Grow a melody",
                ready,
                cx,
                move |ws| {
                    let w = &ws.roll_writing;
                    generate::seeded(
                        key,
                        (w.low, w.high),
                        ws.grid(),
                        ws.span(),
                        w.density,
                        w.seed,
                    )
                },
            ),
            Writer::Shape => self.write_button(
                "Write the shape".into(),
                "Write the shape",
                ready,
                cx,
                move |ws| {
                    let w = &ws.roll_writing;
                    generate::shaped(w.contour, key, (w.low, w.high), ws.grid(), ws.span())
                },
            ),
        };
        let another = (w.writer == Writer::Melody).then(|| {
            button("Another", false)
                .flex_none()
                .h(px(28.))
                .px(px(10.))
                .text_size(px(11.5))
                .debug_selector(|| "write-another".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.roll_writing.seed = fresh_seed() % 100_000 + 1;
                        let w = &ws.roll_writing;
                        let notes = generate::seeded(
                            key,
                            (w.low, w.high),
                            ws.grid(),
                            ws.span(),
                            w.density,
                            w.seed,
                        );
                        ws.write_notes(notes, cx);
                    }),
                )
        });
        let foot = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(say(self.over_what()))
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(self.replace_or_add(cx))
                    .child(write)
                    .children(another),
            );
        (body, Some(foot))
    }

    /// A menu of a setting's choices, by the button that opens it.
    fn choices_menu(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        items: Vec<(String, WritingSet)>,
        cx: &mut Context<Self>,
    ) {
        self.menu = Some(Menu {
            at,
            items: items
                .into_iter()
                .map(|(label, set)| (label, MenuAction::Writing(set)))
                .collect(),
            ..Default::default()
        });
        cx.notify();
    }

    /// A Chords setting picked from its menu.
    pub(crate) fn set_writing_to(&mut self, set: WritingSet, cx: &mut Context<Self>) {
        self.set_writing(cx, |w| match set {
            WritingSet::Voicing(v) => w.voicing = v,
            WritingSet::Performance(p) => w.performance = p,
            WritingSet::Beats(b) => w.beats = b,
            WritingSet::Book(b) => w.book = b,
        });
    }

    /// A progression heard, a chord after another.
    fn audition_progression(&mut self, chords: Vec<Chord>, cx: &mut Context<Self>) {
        let Some(at) = self.roll_region() else { return };
        let track = self.session.project.tracks[at.0].id;
        let voiced = harmony::voice(&chords, self.roll_writing.voicing, 60);
        cx.spawn(async move |this, cx| {
            for pitches in voiced {
                let _ = this.update(cx, |ws, _| {
                    for &p in &pitches {
                        ws.player.preview(track, p, 92);
                    }
                });
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(520))
                    .await;
                let _ = this.update(cx, |ws, _| {
                    for &p in &pitches {
                        ws.player.preview(track, p, 0);
                    }
                });
            }
        })
        .detach();
    }

    /// A chord to click: heard, and added to those to write; coloured by what it does.
    fn chord_tile(
        &self,
        numeral: String,
        c: Chord,
        spell: Key,
        ink: u32,
        id: String,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .relative()
            .h(px(44.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .overflow_hidden()
            .bg(alpha(ink, 0.07))
            .border_1()
            .border_color(alpha(ink, 0.18))
            .cursor_pointer()
            .hover(|s| s.bg(alpha(ink, 0.16)))
            .debug_selector(move || id.clone())
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(2.))
                    .bg(rgb(ink)),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .child(c.name_in(Some(&spell))),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_3))
                    .child(numeral),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    let voiced = harmony::voice(&[c], ws.roll_writing.voicing, 60);
                    ws.roll_writing.chords.push(c);
                    ws.audition_chord(voiced.concat(), cx);
                    cx.notify();
                }),
            )
    }

    /// The key's chords and those it borrows, or progressions by mood; at the foot, the chords
    /// picked, how they're voiced and played, and the button that writes them.
    pub(crate) fn chords_tab(&self, cx: &mut Context<Self>) -> (Div, Option<Div>) {
        let w = &self.roll_writing;
        let Some(key) = self.session.project.song_key() else {
            return (
                say(
                    "Set the song's key in the control bar (the key there), and its chords show here.",
                ),
                None,
            );
        };
        let spelled = |c: &Chord| c.name_in(Some(&key));
        // Borrowed chords are spelled in the key they're borrowed from: E♭, not D♯, in C.
        let parallel = Key {
            minor: !key.minor,
            ..key
        };
        let body = match w.progressions {
            false => {
                let own: Vec<Div> = harmony::in_key(key, w.sevenths)
                    .into_iter()
                    .enumerate()
                    .map(|(i, (n, c))| {
                        self.chord_tile(n, c, key, function(i), format!("harmony-chord-{i}"), cx)
                    })
                    .collect();
                let lent: Vec<Div> = harmony::borrowed(key)
                    .into_iter()
                    .enumerate()
                    .map(|(i, (n, c))| {
                        self.chord_tile(
                            n,
                            c,
                            parallel,
                            BORROWED,
                            format!("harmony-borrowed-{i}"),
                            cx,
                        )
                    })
                    .collect();
                let legend = |ink: u32, name: &'static str| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(ink)))
                        .child(div().text_size(px(10.)).text_color(rgb(TEXT_3)).child(name))
                };
                let sevenths = tray().p(px(2.)).rounded(px(6.)).children(
                    [("Triads", false), ("7ths", true)].map(|(label, s)| {
                        seg(label, w.sevenths == s, 20.)
                            .px(px(7.))
                            .text_size(px(10.5))
                            .debug_selector(move || match s {
                                true => "harmony-sevenths".to_string(),
                                false => "harmony-triads".to_string(),
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.set_writing(cx, |w| w.sevenths = s)
                                }),
                            )
                    }),
                );
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(part(format!("In {}", key.name())))
                            .child(sevenths),
                    )
                    .child(div().grid().grid_cols(4).gap(px(4.)).children(own))
                    .child(part(format!("Borrowed from {}", parallel.name())))
                    .child(div().grid().grid_cols(4).gap(px(4.)).children(lent))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_x(px(10.))
                            .gap_y(px(4.))
                            .child(legend(TONIC, "Rest"))
                            .child(legend(SUBDOMINANT, "Lean away"))
                            .child(legend(DOMINANT, "Pull home"))
                            .child(legend(BORROWED, "Borrowed")),
                    )
                    .child(say("A click plays a chord and adds it to yours."))
            }
            true => {
                // The person's own progressions: Pina Studio keeps none.
                let books: Vec<(String, String, Vec<String>)> = self
                    .genres
                    .list
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone(), p.all_progressions()))
                    .filter(|b| !b.2.is_empty())
                    .collect();
                let shown = w
                    .book
                    .as_ref()
                    .and_then(|id| books.iter().find(|b| &b.0 == id))
                    .map_or("All playbooks".to_string(), |b| b.1.clone());
                let menu: Vec<(String, String)> =
                    books.iter().map(|b| (b.0.clone(), b.1.clone())).collect();
                let moods = button(format!("{shown}  ▾"), false)
                    .h(px(24.))
                    .px(px(10.))
                    .text_size(px(11.))
                    .debug_selector(|| "harmony-books".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let now = ws.roll_writing.book.clone();
                            let tick = |on: bool| if on { "✓ " } else { "" };
                            let items = std::iter::once((
                                format!("{}All playbooks", tick(now.is_none())),
                                WritingSet::Book(None),
                            ))
                            .chain(menu.iter().map(|(id, name)| {
                                (
                                    format!("{}{name}", tick(now.as_deref() == Some(id))),
                                    WritingSet::Book(Some(id.clone())),
                                )
                            }))
                            .collect();
                            ws.choices_menu(e.position, items, cx);
                        }),
                    );
                let picked: Vec<(String, String)> = books
                    .iter()
                    .filter(|b| w.book.as_ref().is_none_or(|id| &b.0 == id))
                    .flat_map(|b| b.2.iter().map(|n| (n.clone(), b.1.clone())))
                    .collect();
                let empty = picked.is_empty();
                let list: Vec<Div> = picked
                    .into_iter()
                    .enumerate()
                    .map(|(i, (numerals, book))| {
                        let chords: Vec<Chord> = harmony::progression(&numerals, key)
                            .into_iter()
                            .map(|c| c.0)
                            .collect();
                        let names: Vec<String> = chords.iter().map(&spelled).collect();
                        let heard = chords.clone();
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(7.))
                            .bg(rgb(FIELD))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(RAISED)))
                            .debug_selector(move || format!("harmony-progression-{i}"))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(names.join("  ·  ")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(rgb(TEXT_3))
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(format!(
                                                "{}  ·  {book}",
                                                numerals.replace('|', "·")
                                            )),
                                    ),
                            )
                            .child(
                                icon_seg(Icon::Play, false, 22.)
                                    .flex_none()
                                    .debug_selector(move || format!("harmony-hear-{i}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.audition_progression(heard.clone(), cx);
                                        }),
                                    ),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.roll_writing.chords = chords.clone();
                                    cx.notify();
                                }),
                            )
                    })
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(part("By playbook"))
                            .when(!books.is_empty(), |d| d.child(moods)),
                    )
                    .child(say(match empty {
                        true => {
                            "Your playbooks have no progressions yet. Add them on a playbook's \
Feel tab, or ask the Studio Agent for a few."
                        }
                        false => "A click makes it yours; ▶ plays it first.",
                    }))
                    .child(div().flex().flex_col().gap(px(4.)).children(list))
            }
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(self.subtabs(
                vec![(false, "Chords"), (true, "Progressions")],
                w.progressions,
                |w, v| w.progressions = v,
                cx,
            ))
            .child(body);
        // At the foot: the chords picked, how they're played, and the button that writes them.
        let picked: Vec<Div> = w
            .chords
            .iter()
            .enumerate()
            .map(|(i, c)| {
                pick(format!("{}  ×", spelled(c)), true)
                    .debug_selector(move || format!("harmony-picked-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.set_writing(cx, |w| {
                                w.chords.remove(i);
                            })
                        }),
                    )
            })
            .collect();
        let menu_button = |label: String,
                           id: &'static str,
                           items: Vec<(String, WritingSet)>,
                           cx: &mut Context<Self>| {
            button(format!("{label} ▾"), false)
                .flex_auto()
                .h(px(24.))
                .px(px(6.))
                .text_size(px(11.))
                .whitespace_nowrap()
                .overflow_hidden()
                .debug_selector(move || format!("harmony-{id}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.choices_menu(e.position, items.clone(), cx);
                    }),
                )
        };
        let tick = |on: bool| if on { "✓ " } else { "" };
        let voicings = Voicing::ALL
            .iter()
            .map(|&v| {
                (
                    format!("{}{}", tick(w.voicing == v), v.name()),
                    WritingSet::Voicing(v),
                )
            })
            .collect();
        let plays = Performance::ALL
            .iter()
            .map(|&p| {
                (
                    format!("{}{}", tick(w.performance == p), p.name()),
                    WritingSet::Performance(p),
                )
            })
            .collect();
        let lengths = [2u8, 4, 8, 16]
            .map(|b| {
                (
                    format!("{}{b} beats each", tick(w.beats == b)),
                    WritingSet::Beats(b),
                )
            })
            .to_vec();
        let count = w.chords.len();
        let foot = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            // Your chords, on one line with what they are; Clear at its end.
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(4.))
                    .child(part("Your chords").mr(px(2.)))
                    .when(count == 0, |d| {
                        d.child(say("click chords or a progression"))
                    })
                    .children(picked)
                    .when(count > 0, |d| {
                        d.child(div().flex_1()).child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .cursor_pointer()
                                .hover(|s| s.text_color(rgb(TEXT)))
                                .debug_selector(|| "harmony-clear".into())
                                .child("Clear")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        ws.set_writing(cx, |w| w.chords.clear())
                                    }),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .child(menu_button(
                        w.voicing.name().into(),
                        "voicing",
                        voicings,
                        cx,
                    ))
                    .child(menu_button(w.performance.name().into(), "play", plays, cx))
                    .child(menu_button(
                        format!("{} beats", w.beats),
                        "beats",
                        lengths,
                        cx,
                    ))
                    .child(
                        pick("Bass", w.bass)
                            .h(px(24.))
                            .flex_none()
                            .debug_selector(|| "harmony-bass".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.set_writing(cx, |w| w.bass ^= true)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(self.replace_or_add(cx))
                    .child(self.write_button(
                        match count {
                            0 | 1 => "Write the chord".into(),
                            n => format!("Write {n} chords"),
                        },
                        "Write the chords",
                        count > 0 && self.roll_region().is_some(),
                        cx,
                        |ws| {
                            let w = &ws.roll_writing;
                            let voiced = harmony::voice(&w.chords, w.voicing, 60);
                            let each = PPQ * w.beats as u64;
                            let spans = vec![each; voiced.len()];
                            harmony::perform(&voiced, &spans, w.performance, PPQ, w.bass)
                        },
                    )),
            );
        (body, Some(foot))
    }
}

/// A Chords setting picked from its menu.
#[derive(Clone, PartialEq, Debug)]
pub enum WritingSet {
    Voicing(Voicing),
    Performance(Performance),
    Beats(u8),
    Book(Option<String>),
}
