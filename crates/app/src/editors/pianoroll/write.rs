//! The roll's tabs that write notes: Generate (a rhythm spread evenly, a melody grown from a
//! seed in the key, one following a shape) and Harmony (the key's chords and those it borrows,
//! progressions by mood, each voiced and played as picked, a bass under it, written in).

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
    /// Chords: sevenths or triads, those picked in order, how they're voiced and played, a bass
    /// under them, how many beats each, and the mood the progressions are narrowed to.
    pub sevenths: bool,
    pub chords: Vec<Chord>,
    pub voicing: Voicing,
    pub performance: Performance,
    pub bass: bool,
    pub beats: u8,
    pub mood: Option<&'static str>,
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
            sevenths: false,
            chords: Vec::new(),
            voicing: Voicing::Led,
            performance: Performance::Block,
            bass: false,
            beats: 4,
            mood: None,
        }
    }
}

/// A small heading over a part of a tab.
fn part(name: &'static str) -> Div {
    div()
        .pt(px(4.))
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(name.to_uppercase())
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
        f(&mut self.roll_writing);
        cx.notify();
    }

    /// A number with a step down and up.
    fn writing_stepper(
        &self,
        name: &'static str,
        value: String,
        step: impl Fn(&mut Writing, i32) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let button = |sign: &'static str, by: i32, cx: &mut Context<Self>| {
            let step = step.clone();
            div()
                .size(px(22.))
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
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .w(px(58.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(name),
            )
            .child(
                div()
                    .w(px(54.))
                    .h(px(22.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(rgb(WELL))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .child(value),
            )
            .child(button("−", -1, cx))
            .child(button("+", 1, cx))
    }

    /// A button that writes.
    fn write_button(
        &self,
        label: &'static str,
        cx: &mut Context<Self>,
        notes: impl Fn(&Workspace) -> Vec<Note> + 'static,
    ) -> Div {
        button(label, true)
            .h(px(26.))
            .text_size(px(11.5))
            .debug_selector(move || format!("write-{label}"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    let written = notes(ws);
                    ws.write_notes(written, cx);
                }),
            )
    }

    /// Rhythms, melodies grown from a seed, and melodies following a shape.
    pub(crate) fn generate_tab(&self, cx: &mut Context<Self>) -> Div {
        let w = &self.roll_writing;
        let key = self.session.project.song_key();
        let low_high = |n: u8| self.note_name(n);
        let hits = generate::euclid(w.pulses, w.steps, w.rotate);
        let pattern = div().flex().gap(px(2.)).children(hits.iter().map(|&h| {
            div().size(px(10.)).rounded(px(2.)).bg(if h {
                rgb(ACCENT).into()
            } else {
                alpha(TEXT, 0.12)
            })
        }));
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child("Writes over the region, a grid step apart"),
                    )
                    .child(
                        pick(if w.replace { "Replace" } else { "Add" }, w.replace)
                            .debug_selector(|| "write-replace".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.set_writing(cx, |w| w.replace ^= true)
                                }),
                            ),
                    ),
            )
            .child(part("Rhythm"))
            .child(pattern)
            .child(self.writing_stepper(
                "Hits",
                w.pulses.to_string(),
                |w, by| w.pulses = (w.pulses as i32 + by).clamp(1, w.steps as i32) as u8,
                cx,
            ))
            .child(self.writing_stepper(
                "Of",
                w.steps.to_string(),
                |w, by| {
                    w.steps = (w.steps as i32 + by).clamp(2, 32) as u8;
                    w.pulses = w.pulses.min(w.steps);
                },
                cx,
            ))
            .child(self.writing_stepper(
                "Turned",
                w.rotate.to_string(),
                |w, by| w.rotate = (w.rotate as i32 + by).rem_euclid(w.steps.max(1) as i32) as u8,
                cx,
            ))
            .child(self.write_button("Write rhythm", cx, |ws| {
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
            }))
            .child(part("Melody"))
            .child(self.writing_stepper(
                "Lowest",
                low_high(w.low),
                |w, by| w.low = (w.low as i32 + by).clamp(0, w.high as i32) as u8,
                cx,
            ))
            .child(self.writing_stepper(
                "Highest",
                low_high(w.high),
                |w, by| w.high = (w.high as i32 + by).clamp(w.low as i32, 127) as u8,
                cx,
            ))
            .child(self.writing_stepper(
                "Plays",
                format!("{}%", w.density),
                |w, by| w.density = (w.density as i32 + by * 10).clamp(10, 100) as u8,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child(self.write_button("Grow a melody", cx, move |ws| {
                        let w = &ws.roll_writing;
                        generate::seeded(
                            key,
                            (w.low, w.high),
                            ws.grid(),
                            ws.span(),
                            w.density,
                            w.seed,
                        )
                    }))
                    .child(
                        button("Another", false)
                            .h(px(26.))
                            .text_size(px(11.5))
                            .debug_selector(|| "write-another".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.roll_writing.seed = fresh_seed();
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
                            ),
                    ),
            )
            .child(row(Contour::ALL.map(|c| {
                pick(c.name(), w.contour == c)
                    .debug_selector(move || format!("write-shape-{}", c.name()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.set_writing(cx, |w| w.contour = c)),
                    )
            })))
            .child(self.write_button("Write the shape", cx, move |ws| {
                let w = &ws.roll_writing;
                generate::shaped(w.contour, key, (w.low, w.high), ws.grid(), ws.span())
            }))
    }

    /// The key's chords and those it borrows, progressions by mood, the chords picked, and how
    /// they're voiced and played.
    pub(crate) fn harmony_tab(&self, cx: &mut Context<Self>) -> Div {
        let w = &self.roll_writing;
        let Some(key) = self.session.project.song_key() else {
            return div()
                .text_size(px(11.))
                .text_color(rgb(TEXT_3))
                .child("Set the song's key in the control bar, and its chords show here.");
        };
        let spelled = |c: &Chord| c.name_in(Some(&key));
        // Borrowed chords are spelled in the key they're borrowed from: E♭, not D♯, in C.
        let parallel = Key {
            minor: !key.minor,
            ..key
        };
        let chord_chip =
            |numeral: String, c: Chord, id: String, spell: Key, cx: &mut Context<Self>| {
                div()
                    .w(px(54.))
                    .h(px(40.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(rgb(FIELD))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(RAISED)))
                    .debug_selector(move || id.clone())
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
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
            };
        let own: Vec<Div> = harmony::in_key(key, w.sevenths)
            .into_iter()
            .enumerate()
            .map(|(i, (n, c))| chord_chip(n, c, format!("harmony-chord-{i}"), key, cx))
            .collect();
        let lent: Vec<Div> = harmony::borrowed(key)
            .into_iter()
            .enumerate()
            .map(|(i, (n, c))| chord_chip(n, c, format!("harmony-borrowed-{i}"), parallel, cx))
            .collect();
        let moods: Vec<Div> = std::iter::once(None)
            .chain(harmony::moods().into_iter().take(14).map(Some))
            .map(|m| {
                let label = m.map_or("All".to_string(), |m| m.replace('_', " "));
                pick(label, w.mood == m)
                    .debug_selector(move || format!("harmony-mood-{}", m.unwrap_or("all")))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.set_writing(cx, |w| w.mood = m)),
                    )
            })
            .collect();
        let progressions: Vec<Div> = harmony::progressions(key.minor, w.mood)
            .into_iter()
            .take(12)
            .enumerate()
            .map(|(i, p)| {
                let chords = harmony::progression(p, key);
                let names: Vec<String> = chords.iter().map(|(c, _)| spelled(c)).collect();
                div()
                    .px(px(8.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .bg(rgb(FIELD))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(RAISED)))
                    .debug_selector(move || format!("harmony-progression-{i}"))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .child(names.join("  ")),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .child(p.numerals.replace('|', "·")),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.roll_writing.chords = chords.iter().map(|c| c.0).collect();
                            cx.notify();
                        }),
                    )
            })
            .collect();
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
        let none = picked.is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(part("To write"))
            .child(match none {
                true => div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_4))
                    .child("Click chords (they play) or a progression"),
                false => row(picked),
            })
            .child(row(Voicing::ALL.map(|v| {
                pick(v.name(), w.voicing == v)
                    .debug_selector(move || format!("harmony-voicing-{}", v.name()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.set_writing(cx, |w| w.voicing = v)),
                    )
            })))
            .child(row(Performance::ALL.map(|p| {
                pick(p.name(), w.performance == p)
                    .debug_selector(move || format!("harmony-play-{}", p.name()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.set_writing(cx, |w| w.performance = p)),
                    )
            })))
            .child(row(vec![
                pick(if w.bass { "Bass under" } else { "No bass" }, w.bass)
                    .debug_selector(|| "harmony-bass".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.set_writing(cx, |w| w.bass ^= true)),
                    ),
                pick(format!("{} beats each", w.beats), false)
                    .debug_selector(|| "harmony-beats".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.set_writing(cx, |w| {
                                w.beats = match w.beats {
                                    2 => 4,
                                    4 => 8,
                                    _ => 2,
                                }
                            })
                        }),
                    ),
            ]))
            .child(self.write_button("Write the chords", cx, |ws| {
                let w = &ws.roll_writing;
                let voiced = harmony::voice(&w.chords, w.voicing, 60);
                let each = PPQ * w.beats as u64;
                let spans = vec![each; voiced.len()];
                harmony::perform(&voiced, &spans, w.performance, PPQ, w.bass)
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(part("In key"))
                    .child(
                        pick(if w.sevenths { "Sevenths" } else { "Triads" }, w.sevenths)
                            .debug_selector(|| "harmony-sevenths".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.set_writing(cx, |w| w.sevenths ^= true)
                                }),
                            ),
                    ),
            )
            .child(row(own))
            .child(part("Borrowed"))
            .child(row(lent))
            .child(part("Progressions"))
            .child(row(moods))
            .child(div().flex().flex_col().gap(px(3.)).children(progressions))
    }
}
