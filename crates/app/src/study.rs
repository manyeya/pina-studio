//! The Song Study page: a finished song, from its file, taken apart. Its identity and vitals over
//! its shape (a bar a stroke, coloured by section), its form (the same colour for the same
//! music), its harmony on the circle of fifths, its groove as steps, what plays in it, its mix and
//! dynamics, its lyrics with their rhymes and hooks, and the playbooks it's nearest. From it, a
//! playbook, a mix reference, a new song like it, or the agent's reading of what it means.

use std::path::PathBuf;

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, PathPromptOptions, Pixels, ScrollHandle,
    Stateful, Window, canvas, div, point, prelude::*, px, rgb,
};
use pina_engine::{
    music::Key,
    playbook::{self, Playbook},
    project::{Project, Reference},
    study::{self, Study},
    theory,
};

use crate::{
    genres::{BANDS, FLOOR, capital, genre_color, heading, label, role_color, swing_dots, tile},
    study_cli,
    theme::*,
    workspace::{View, Workspace},
};

#[derive(Default)]
pub struct Studies {
    pub scroll: ScrollHandle,
    /// Every song studied, the latest first.
    pub list: Vec<Study>,
    /// The study shown, by its file.
    pub open: Option<PathBuf>,
    /// Files being studied now.
    pub busy: Vec<PathBuf>,
    /// The section picked in the form.
    pub section: Option<usize>,
    /// The playbooks, for their mix targets.
    books: Vec<Playbook>,
}

/// A section's colour, by its letter: the same music, the same colour.
fn letter_color(label: &str) -> u32 {
    let palette = [SKY, CORAL, MINT, VIOLET, AMBER, PINK, TEAL, SLATE];
    let k = label
        .bytes()
        .next()
        .map_or(0, |b| b.wrapping_sub(b'A') as usize);
    palette[k % palette.len()]
}

/// A rhyme's colour, by its letter.
fn rhyme_color(c: char) -> u32 {
    let palette = [AMBER, SKY, PINK, MINT, VIOLET, CORAL, TEAL];
    palette[(c as u8).wrapping_sub(b'A') as usize % palette.len()]
}

fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The study's page colour: its loudest section's.
fn tint_of(s: &Study) -> u32 {
    s.sections
        .iter()
        .max_by(|a, b| a.energy.total_cmp(&b.energy))
        .map_or(ACCENT, |x| letter_color(&x.label))
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned())
}

impl Workspace {
    /// The Song Study page, on the latest study.
    pub fn open_study(&mut self, cx: &mut Context<Self>) {
        self.studies.list = study_cli::studied();
        self.studies.books = playbook::load().0;
        if self
            .studies
            .open
            .as_ref()
            .is_none_or(|f| !self.studies.list.iter().any(|s| s.file == *f))
        {
            self.studies.open = self.studies.list.first().map(|s| s.file.clone());
            self.studies.section = None;
        }
        self.view = View::Study;
        cx.notify();
    }

    /// Asks for a song's file, and studies it.
    pub fn pick_study(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Study".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            this.update(cx, |ws, cx| {
                if let Some(p) = paths.into_iter().next() {
                    ws.study_file(p, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Studies a song's file in the background (its lyrics, year and genre looked up online), and
    /// shows it.
    pub fn study_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.view != View::Study {
            self.open_study(cx);
        }
        if self.studies.busy.contains(&path) {
            return;
        }
        self.studies.busy.push(path.clone());
        self.status = Some(format!("Listening to {}…", file_name(&path)));
        cx.notify();
        let job = {
            let path = path.clone();
            // Tests stay off the network.
            cx.background_spawn(async move { study_cli::study(&path, !cfg!(test)) })
        };
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |ws, cx| {
                ws.studies.busy.retain(|p| *p != path);
                match result {
                    Ok(s) => {
                        ws.status = Some(format!("Studied {}", file_name(&s.file)));
                        ws.studies.list.retain(|x| x.file != s.file);
                        ws.studies.open = Some(s.file.clone());
                        ws.studies.list.insert(0, s);
                        ws.studies.section = None;
                        ws.studies.scroll.set_offset(point(px(0.), px(0.)));
                    }
                    Err(e) => ws.status = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn show_study(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        if self.studies.open.as_ref() != Some(&file) {
            self.studies.open = Some(file);
            self.studies.section = None;
            self.studies.scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }

    fn forget_study(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        study_cli::forget(&file);
        self.studies.list.retain(|s| s.file != file);
        if self.studies.open.as_ref() == Some(&file) {
            self.studies.open = self.studies.list.first().map(|s| s.file.clone());
            self.studies.section = None;
        }
        cx.notify();
    }

    fn opened_study(&self) -> Option<Study> {
        let f = self.studies.open.as_ref()?;
        self.studies.list.iter().find(|s| s.file == *f).cloned()
    }

    /// The study as a playbook of the person's, saved.
    fn save_study_playbook(&mut self, s: &Study) -> Result<Playbook, String> {
        let taken: Vec<String> = playbook::load().0.into_iter().map(|p| p.id).collect();
        let p = study::to_playbook(s, &taken);
        playbook::save(&p)?;
        Ok(p)
    }

    fn study_to_playbook(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.opened_study() else { return };
        match self.save_study_playbook(&s) {
            Ok(p) => {
                self.status = Some(format!("Saved the playbook {}", p.name));
                self.open_genres(Some(&p.id), cx);
            }
            Err(e) => {
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    /// The study's mix as what this song's critic and mix tools aim at.
    fn study_as_reference(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.opened_study() else { return };
        let r = reference(&s);
        self.edit(cx, |ws| ws.session.project.reference = Some(r));
        self.status = Some(format!(
            "{} is this song's mix reference",
            file_name(&s.file)
        ));
    }

    /// A new song like the study's: its tempo, key and meter, its playbook as the genre, its mix
    /// as the reference. Opened, for the agent to write with.
    fn song_like_study(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.opened_study() else { return };
        let made = (|| -> Result<PathBuf, String> {
            let book = self.save_study_playbook(&s)?;
            let title = match s.identity.title.is_empty() {
                true => "Untitled".to_string(),
                false => format!(
                    "Like {}",
                    s.identity
                        .title
                        .split(['(', '['])
                        .next()
                        .unwrap_or("")
                        .trim()
                ),
            };
            let [beats, unit] = s.tempo.time_signature;
            let dir = crate::projects::create(
                &crate::projects::root(),
                &title,
                (s.tempo.bpm as f64).round().clamp(20.0, 999.0),
                Key::parse(&s.key.name),
                [beats as u64, unit as u64],
                crate::projects::Template::Empty,
            )?;
            let mut p = Project::load(&dir)?;
            p.genre = Some(book.id);
            p.reference = Some(reference(&s));
            p.save(&dir)?;
            Ok(dir)
        })();
        match made {
            Ok(dir) => self.open_song(dir, window, cx),
            Err(e) => {
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    /// The agent reads the song: its theme and message, its imagery, why it works.
    fn ask_about_study(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.opened_study() else { return };
        self.view = View::Arrange;
        self.show_browser = false;
        self.show_agent = true;
        self.ask_agent(
            format!(
                "Study the song at {} with the study tool, then tell me what it's about: its theme and \
                 core message, its metaphors and imagery, how its rhymes and scansion carry the hook, \
                 and what in its structure, harmony, groove and mix makes it work. Read it, don't change \
                 this song.",
                s.file.display()
            ),
            cx,
        );
    }

    pub fn study_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let rail = self.study_rail(cx);
        let main = match self.opened_study() {
            Some(s) => self.study_sheet(&s, cx).into_any_element(),
            None => self.study_empty(cx).into_any_element(),
        };
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .bg(rgb(BG))
            .child(rail)
            .child(main)
    }

    /// Where a song is dropped or picked: dashed, lit while a file's dragged over the window.
    fn study_drop(&self, big: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let dragging = self.file_drag.is_some();
        div()
            .id(if big { "study-drop-big" } else { "study-drop" })
            .debug_selector(move || {
                if big {
                    "study-drop-big".into()
                } else {
                    "study-drop".into()
                }
            })
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(if big { 10. } else { 4. }))
            .when(big, |d| d.w(px(460.)).h(px(200.)))
            .when(!big, |d| d.h(px(78.)))
            .rounded(px(14.))
            .border_1()
            .border_dashed()
            .border_color(if dragging {
                rgb(AGENT).into()
            } else {
                alpha(0xffffff, 0.16)
            })
            .bg(if dragging {
                alpha(AGENT, 0.08)
            } else {
                alpha(0xffffff, 0.02)
            })
            .cursor_pointer()
            .hover(|d| {
                d.bg(alpha(0xffffff, 0.05))
                    .border_color(alpha(0xffffff, 0.3))
            })
            .child(
                icon(Icon::Wave, if big { 26. } else { 16. }).text_color(rgb(if dragging {
                    AGENT
                } else {
                    TEXT_2
                })),
            )
            .child(
                div()
                    .text_size(px(if big { 15. } else { 12.5 }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .child(if dragging {
                        "Drop to study it"
                    } else {
                        "Study a song"
                    }),
            )
            .child(
                div()
                    .text_size(px(if big { 12.5 } else { 11. }))
                    .text_color(rgb(TEXT_3))
                    .child("Drop a WAV, AIFF, FLAC or MP3, or click"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| ws.pick_study(cx)),
            )
    }

    /// The songs studied, down the left.
    fn study_rail(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let open = self.studies.open.clone();
        let busy: Vec<_> = self
            .studies
            .busy
            .iter()
            .map(|p| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(9.))
                    .bg(alpha(AGENT, 0.06))
                    .child(div().size(px(7.)).rounded(px(4.)).bg(rgb(AGENT)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(rgb(TEXT))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(file_name(p)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(AGENT_TEXT))
                                    .child("Listening…"),
                            ),
                    )
            })
            .collect();
        let rows: Vec<_> = self
            .studies
            .list
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let on = open.as_ref() == Some(&s.file);
                let title = match s.identity.title.is_empty() {
                    true => file_name(&s.file),
                    false => s.identity.title.clone(),
                };
                let file = s.file.clone();
                let gone = s.file.clone();
                div()
                    .id(("study", i))
                    .group("study-row")
                    .debug_selector(move || format!("study-{i}"))
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgb(RAISED)))
                    .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
                    .child(barcode(s, 40., 26.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_4))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(format!("{:.0} BPM · {}", s.tempo.bpm, s.key.name)),
                            ),
                    )
                    .child(
                        div()
                            .id(("forget", i))
                            .debug_selector(move || format!("study-forget-{i}"))
                            .invisible()
                            .group_hover("study-row", |d| d.visible())
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .hover(|d| d.bg(alpha(0xffffff, 0.08)))
                            .child(icon(Icon::Close, 11.).text_color(rgb(TEXT_3)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.forget_study(gone.clone(), cx);
                                }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.show_study(file.clone(), cx)),
                    )
            })
            .collect();
        div()
            .id("study-rail")
            .w(px(256.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(12.))
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .overflow_y_scroll()
            .child(
                div()
                    .px(px(6.))
                    .pt(px(6.))
                    .pb(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Song Study"),
                    )
                    .child(div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child(
                        "Any song, taken apart: its form, harmony, groove, sound and words.",
                    )),
            )
            .child(self.study_drop(false, cx))
            .child(div().h(px(10.)))
            .children(busy)
            .when(!self.studies.list.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(6.))
                        .pt(px(6.))
                        .pb(px(4.))
                        .child(label("Studied")),
                )
            })
            .children(rows)
    }

    /// Nothing studied yet: what a study finds, and where to drop a song.
    fn study_empty(&mut self, cx: &mut Context<Self>) -> Div {
        let pillar = |title: &'static str, items: &'static str, color: u32| {
            div()
                .w(px(200.))
                .p(px(14.))
                .rounded(px(12.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .w(px(22.))
                        .h(px(3.))
                        .mb(px(4.))
                        .rounded(px(2.))
                        .bg(rgb(color)),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .child(items),
                )
        };
        let busy = !self.studies.busy.is_empty();
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(28.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(28.))
                            .font_weight(FontWeight::BOLD)
                            .child(if busy { "Listening…" } else { "Take a song apart" }),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(TEXT_3))
                            .child(match busy {
                                true => "Its beats, chords, sections and sound, then its lyrics online.",
                                false => "Everything a song is made of, measured from its file. Its audio never leaves this Mac.",
                            }),
                    ),
            )
            .when(!busy, |d| d.child(self.study_drop(true, cx)))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(pillar("Identity", "Title, artist, year and genre, from its tags and online", SKY))
                    .child(pillar("Form", "Intro, verses, choruses, bridge: where each starts, and how hard it hits", CORAL))
                    .child(pillar("Music", "Tempo, key, chords, groove, swing, instruments, mix and dynamics", MINT))
                    .child(pillar("Words", "Lyrics by section, rhyme schemes, syllables, hooks, and what they mean", VIOLET)),
            )
    }

    fn study_sheet(&mut self, s: &Study, cx: &mut Context<Self>) -> Stateful<Div> {
        let color = tint_of(s);
        div()
            .id("study-sheet")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.studies.scroll)
            .child(self.study_hero(s, color, cx))
            .child(
                div()
                    .px(px(36.))
                    .pb(px(56.))
                    .flex()
                    .flex_col()
                    .gap(px(34.))
                    .child(self.study_form(s, cx))
                    .child(
                        div()
                            .flex()
                            .gap(px(16.))
                            .child(harmony(s, color))
                            .child(rhythm(s, color)),
                    )
                    .child(instruments(s))
                    .child(self.study_mix(s, color))
                    .child(self.study_words(s, color, cx))
                    .child(self.study_genres(s, cx)),
            )
    }

    /// The song's name and vitals over its shape.
    fn study_hero(&mut self, s: &Study, color: u32, cx: &mut Context<Self>) -> Div {
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

    /// The form: a block a section, as wide as it's long, the same colour for the same music;
    /// under it, each one's chords; the one picked, opened up.
    fn study_form(&mut self, s: &Study, cx: &mut Context<Self>) -> Div {
        let picked = self
            .studies
            .section
            .or_else(|| s.sections.iter().position(|x| x.name.starts_with("Chorus")))
            .unwrap_or(0)
            .min(s.sections.len().saturating_sub(1));
        let blocks = s.sections.iter().enumerate().map(|(i, x)| {
            let c = letter_color(&x.label);
            let on = i == picked;
            div()
                .id(("section", i))
                .debug_selector(move || format!("study-section-{i}"))
                .flex_grow(x.bars.max(1) as f32)
                .flex_basis(px(0.))
                .min_w(px(0.))
                .h(px(64.))
                .px(px(8.))
                .py(px(7.))
                .flex()
                .flex_col()
                .justify_between()
                .rounded(px(8.))
                .cursor_pointer()
                .bg(alpha(c, if on { 0.3 } else { 0.08 + 0.14 * x.energy }))
                .border_t_2()
                .border_color(rgb(c))
                .when(on, |d| d.border_1().border_color(rgb(c)))
                .hover(|d| d.bg(alpha(c, 0.26)))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(x.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(format!("{} bars", x.bars)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.studies.section = Some(i);
                        cx.notify();
                    }),
                )
        });
        let chords = s.sections.iter().map(|x| {
            div()
                .flex_grow(x.bars.max(1) as f32)
                .flex_basis(px(0.))
                .min_w(px(0.))
                .px(px(2.))
                .font_family(MONO)
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(x.chords.clone())
        });
        let detail = s.sections.get(picked).map(|x| section_detail(s, picked, x));
        // The form as letters: "A B C B D C".
        let letters: Vec<&str> = s.sections.iter().map(|x| x.label.as_str()).collect();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading(
                "Structure",
                "Where each section starts and how hard it hits; one colour, one music",
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .children(letters.iter().map(|l| {
                        div()
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .bg(alpha(letter_color(l), 0.18))
                            .text_size(px(11.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(letter_color(l)))
                            .child(l.to_string())
                    }))
                    .child(
                        div()
                            .pl(px(6.))
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child(form_note(s)),
                    ),
            )
            .child(div().flex().gap(px(3.)).children(blocks))
            .child(div().flex().gap(px(3.)).children(chords))
            .children(detail)
    }

    /// Mix and dynamics: the numbers, the spectrum (against its nearest genre's target) and
    /// each section's loudness.
    fn study_mix(&mut self, s: &Study, color: u32) -> Div {
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

    /// The words: lines by section, each with its rhyme's letter and syllables; the hooks, the
    /// voice, the words it keeps coming back to; and the agent, for what they mean.
    fn study_words(&mut self, s: &Study, color: u32, cx: &mut Context<Self>) -> Div {
        let ask = div()
            .h(px(32.))
            .px(px(13.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(9.))
            .bg(alpha(AGENT, 0.12))
            .border_1()
            .border_color(alpha(AGENT, 0.35))
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(AGENT_TEXT))
            .cursor_pointer()
            .hover(|d| d.bg(alpha(AGENT, 0.2)))
            .debug_selector(|| "study-ask".into())
            .child(icon(Icon::Agent, 13.).text_color(rgb(AGENT)))
            .child("Ask the agent what it means")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| ws.ask_about_study(cx)),
            );
        let meaning = tile(
            "Theme & meaning",
            "Its core message, its metaphors and imagery, and why it works: read by the agent from the lyrics and this study",
        )
        .child(div().flex().child(ask));
        let Some(l) = s.lyrics.as_ref() else {
            let who = match s.identity.title.is_empty() {
                true => "this file".to_string(),
                false => format!("“{}”", s.identity.title),
            };
            return div()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(heading("Lyrics & poetic devices", "Rhyme schemes, scansion and hooks"))
                .child(
                    div()
                        .flex()
                        .gap(px(16.))
                        .child(
                            tile("No lyrics found", "Looked up on LRCLIB by the title and artist in the file's tags")
                                .flex_1()
                                .child(div().text_size(px(12.5)).text_color(rgb(TEXT_2)).child(format!(
                                    "None were found for {who}. If its file name or tags spell the title and artist, \
                                     studying it again looks once more; an instrumental has none."
                                ))),
                        )
                        .child(meaning.flex_1()),
                );
        };
        let hooks = l.hooks.clone();
        // Lines grouped by section, in order.
        let mut groups: Vec<(Option<usize>, Vec<&study::words::Line>)> = Vec::new();
        for line in &l.lines {
            match groups.last_mut() {
                Some((sec, lines)) if *sec == line.section => lines.push(line),
                _ => groups.push((line.section, vec![line])),
            }
        }
        let scheme_of = |i: usize| {
            l.schemes
                .iter()
                .find(|(k, _)| *k == i)
                .map(|(_, v)| v.clone())
        };
        let lines = groups.into_iter().map(|(sec, lines)| {
            let section = sec.and_then(|i| s.sections.get(i));
            let c = section.map_or(SLATE, |x| letter_color(&x.label));
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .pb(px(2.))
                        .child(div().w(px(3.)).h(px(12.)).rounded(px(2.)).bg(rgb(c)))
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(tint(c, 0.3)))
                                .child(
                                    section.map_or("Lyrics".to_string(), |x| x.name.to_uppercase()),
                                ),
                        )
                        .when_some(sec.and_then(scheme_of), |d, sch| {
                            d.child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(sch),
                            )
                        }),
                )
                .children(lines.into_iter().map(|line| {
                    let hook = hooks.contains(&line.text);
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .size(px(18.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.))
                                .bg(alpha(rhyme_color(line.rhyme), 0.16))
                                .font_family(MONO)
                                .text_size(px(10.5))
                                .text_color(rgb(rhyme_color(line.rhyme)))
                                .child(line.rhyme.to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(13.))
                                .text_color(rgb(if hook { tint(color, 0.35) } else { TEXT_2 }))
                                .when(hook, |d| d.font_weight(FontWeight::SEMIBOLD))
                                .child(line.text.clone()),
                        )
                        .child(
                            div()
                                .w(px(26.))
                                .flex_none()
                                .flex()
                                .justify_end()
                                .font_family(MONO)
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_4))
                                .child(line.syllables.to_string()),
                        )
                }))
        });
        let top = l.words.first().map_or(1, |w| w.1).max(1);
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(heading(
                "Lyrics & poetic devices",
                "Each line's rhyme (same letter, same sound) and syllables; its hooks lit",
            ))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .p(px(18.))
                            .rounded(px(14.))
                            .bg(rgb(CARD))
                            .border_1()
                            .border_color(rgb(LINE))
                            .flex()
                            .flex_col()
                            .gap(px(18.))
                            .children(lines)
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(format!("Lyrics: {}", l.source)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(320.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .child(meaning)
                            .child(
                                tile("Voice", "Who sings, and to whom")
                                    .child(div().text_size(px(14.)).child(capital(&l.voice))),
                            )
                            .when(!l.hooks.is_empty(), |d| {
                                d.child(tile("Hooks", "Lines sung three times or more").children(
                                    l.hooks.iter().take(4).map(|h| {
                                        div()
                                            .pl(px(10.))
                                            .border_l_2()
                                            .border_color(rgb(color))
                                            .text_size(px(13.))
                                            .text_color(rgb(TEXT))
                                            .child(format!("“{h}”"))
                                    }),
                                ))
                            })
                            .when(!l.words.is_empty(), |d| {
                                d.child(tile("Its words", "What it keeps coming back to").child(
                                    div().flex().flex_wrap().gap(px(6.)).children(
                                        l.words.iter().take(14).map(|(w, n)| {
                                            let k = *n as f32 / top as f32;
                                            div()
                                                .px(px(8.))
                                                .py(px(3.))
                                                .rounded(px(7.))
                                                .bg(alpha(color, 0.06 + 0.18 * k))
                                                .text_size(px(11.0 + 5.0 * k))
                                                .text_color(rgb(if k > 0.5 {
                                                    TEXT
                                                } else {
                                                    TEXT_2
                                                }))
                                                .child(format!("{w} {n}"))
                                        }),
                                    ),
                                ))
                            }),
                    ),
            )
    }

    /// The playbooks it's nearest, and why.
    fn study_genres(&mut self, s: &Study, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(heading(
                "Nearest playbooks",
                "By tempo, feel, mode, groove and spectrum",
            ))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .children(s.genres.iter().enumerate().map(|(i, g)| {
                        let c = genre_color(&g.id);
                        let id = g.id.clone();
                        div()
                            .id(("study-genre", i))
                            .debug_selector(move || format!("study-genre-{i}"))
                            .flex_1()
                            .min_w(px(0.))
                            .p(px(14.))
                            .rounded(px(14.))
                            .bg(rgb(CARD))
                            .border_1()
                            .border_color(rgb(LINE))
                            .cursor_pointer()
                            .hover(|d| d.border_color(alpha(c, 0.6)))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(c)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(13.5))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(g.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .font_family(MONO)
                                            .text_size(px(12.))
                                            .text_color(rgb(tint(c, 0.3)))
                                            .child(format!("{:.0}%", g.score * 100.0)),
                                    ),
                            )
                            .child(
                                div().h(px(4.)).rounded(px(2.)).bg(rgb(WELL)).child(
                                    div()
                                        .h_full()
                                        .w(gpui::relative(g.score.clamp(0.0, 1.0)))
                                        .rounded(px(2.))
                                        .bg(rgb(c)),
                                ),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(g.why.clone()),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.open_genres(Some(&id), cx)),
                            )
                    })),
            )
    }
}

/// The study's mix, as a song's reference.
fn reference(s: &Study) -> Reference {
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

fn steadiness(c: f32) -> &'static str {
    match c {
        c if c > 0.6 => "steady",
        c if c > 0.35 => "fairly steady",
        _ => "loose",
    }
}

fn key_note(s: &Study) -> String {
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

/// The form in a sentence: how it's built.
fn form_note(s: &Study) -> String {
    let chorus = s
        .sections
        .iter()
        .filter(|x| x.name.starts_with("Chorus"))
        .count();
    let verse = s
        .sections
        .iter()
        .filter(|x| x.name.starts_with("Verse"))
        .count();
    let first = s
        .sections
        .iter()
        .find(|x| x.name.starts_with("Chorus"))
        .map(|x| x.start);
    let mut out = match (verse, chorus) {
        (0, 0) => "No section comes back the same: built as a groove that moves".to_string(),
        (v, c) => format!(
            "{v} verse{}, {c} chorus{}",
            if v == 1 { "" } else { "s" },
            if c == 1 { "" } else { "es" }
        ),
    };
    if let Some(t) = first {
        out.push_str(&format!(" · first chorus at {}", clock(t)));
    }
    if s.sections.iter().any(|x| x.name == "Bridge") {
        out.push_str(" · a bridge");
    }
    out
}

/// The section picked, opened up: where it is, how loud, its chords as numerals, what plays in
/// it, and its first lines.
fn section_detail(s: &Study, i: usize, x: &study::SectionInfo) -> Div {
    let c = letter_color(&x.label);
    let heard: Vec<String> = s
        .instruments
        .iter()
        .filter(|e| e.sections.contains(&i))
        .map(|e| e.name.clone())
        .collect();
    let lines: Vec<String> = s
        .lyrics
        .as_ref()
        .map(|l| {
            l.lines
                .iter()
                .filter(|l| l.section == Some(i))
                .take(4)
                .map(|l| l.text.clone())
                .collect()
        })
        .unwrap_or_default();
    let fact = |name: &'static str, value: String| {
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
            .child(div().font_family(MONO).text_size(px(15.)).child(value))
    };
    div()
        .mt(px(4.))
        .p(px(18.))
        .rounded(px(14.))
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(LINE))
        .border_l_4()
        .border_color(rgb(c))
        .flex()
        .gap(px(28.))
        .debug_selector(|| "study-section-detail".into())
        .child(
            div()
                .w(px(180.))
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(20.))
                        .font_weight(FontWeight::BOLD)
                        .child(x.name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(18.))
                        .child(fact("Bars", format!("{}–{}", x.bar + 1, x.bar + x.bars)))
                        .child(fact("At", clock(x.start))),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(18.))
                        .child(fact("LUFS", format!("{:.1}", x.lufs)))
                        .child(fact("Energy", format!("{:.0}%", x.energy * 100.0))),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(label("Goes round"))
                        .child(
                            div()
                                .font_family(MONO)
                                .text_size(px(17.))
                                .child(x.chords.clone()),
                        )
                        .child(
                            div()
                                .font_family(MONO)
                                .text_size(px(12.5))
                                .text_color(rgb(tint(c, 0.3)))
                                .child(x.numerals.clone()),
                        ),
                )
                .when(!heard.is_empty(), |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.))
                            .children(heard.into_iter().map(|h| {
                                div()
                                    .px(px(8.))
                                    .py(px(3.))
                                    .rounded(px(7.))
                                    .bg(rgb(RAISED))
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .child(h)
                            })),
                    )
                })
                .when(!lines.is_empty(), |d| {
                    d.child(
                        div()
                            .pl(px(12.))
                            .border_l_2()
                            .border_color(alpha(c, 0.5))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .children(lines.into_iter().map(|l| {
                                div().text_size(px(12.5)).text_color(rgb(TEXT_2)).child(l)
                            })),
                    )
                }),
        )
}

/// The key, and the chords the song uses, on the circle of fifths: majors round the outside,
/// their relative minors inside, each chord a dot as big as the time it's held; and the share of
/// each.
fn harmony(s: &Study, color: u32) -> Div {
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
fn rhythm(s: &Study, color: u32) -> Div {
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

/// What plays in it: each part heard, how sure, and in which sections.
fn instruments(s: &Study) -> Div {
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
fn barcode(s: &Study, w: f32, h: f32) -> Div {
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

/// The hero's art: the song as a strand, a stroke a bar, as tall as the bar is loud, mirrored
/// about its middle, each section in its colour, under a soft envelope; fading into the page.
fn dna(
    b: Bounds<Pixels>,
    energy: &[f32],
    spans: &[(usize, usize, u32)],
    color: u32,
    window: &mut Window,
) {
    let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let rect = |x: f32, y: f32, rw: f32, rh: f32| {
        let mut p = gpui::PathBuilder::fill();
        p.move_to(point(px(x), px(y)));
        p.line_to(point(px(x + rw), px(y)));
        p.line_to(point(px(x + rw), px(y + rh)));
        p.line_to(point(px(x), px(y + rh)));
        p.close();
        p.build().ok()
    };
    if let Some(p) = rect(x0, y0, w, h) {
        window.paint_path(p, alpha(color, 0.045));
    }
    let n = energy.len();
    if n == 0 {
        return;
    }
    // The strand runs along the hero's foot, under the title and vitals.
    let mid = y0 + h - 82.0;
    let amp = 46.0;
    let colour_of = |bar: usize| {
        spans
            .iter()
            .find(|s| bar >= s.0 && bar < s.1)
            .map_or(SLATE, |s| s.2)
    };
    let slot = w / n as f32;
    // The envelope: a smooth band through the bars' tops, per section.
    for &(a, z, c) in spans {
        let (a, z) = (a.min(n), z.min(n));
        if z <= a {
            continue;
        }
        let mut p = gpui::PathBuilder::fill();
        let top = |bar: usize| mid - (0.18 + 0.82 * energy[bar]) * amp * 1.2;
        p.move_to(point(px(x0 + a as f32 * slot), px(mid)));
        for bar in a..z {
            p.line_to(point(px(x0 + (bar as f32 + 0.5) * slot), px(top(bar))));
        }
        p.line_to(point(px(x0 + z as f32 * slot), px(mid)));
        for bar in (a..z).rev() {
            p.line_to(point(
                px(x0 + (bar as f32 + 0.5) * slot),
                px(2.0 * mid - top(bar)),
            ));
        }
        p.close();
        if let Ok(p) = p.build() {
            window.paint_path(p, alpha(c, 0.1));
        }
    }
    // A stroke a bar.
    let thick = (slot * 0.55).clamp(1.0, 6.0);
    for (bar, &e) in energy.iter().enumerate() {
        let c = colour_of(bar);
        let half = (0.12 + 0.88 * e) * amp;
        let x = x0 + (bar as f32 + 0.5) * slot - thick / 2.0;
        if let Some(p) = rect(x, mid - half, thick, half * 2.0) {
            window.paint_path(p, alpha(c, 0.22 + 0.5 * e));
        }
    }
    // The section lines, down to their names.
    for &(a, _, c) in spans.iter().skip(1) {
        if let Some(p) = rect(
            x0 + a as f32 * slot,
            mid - amp * 1.3,
            1.0,
            h - (mid - amp * 1.3 - y0),
        ) {
            window.paint_path(p, alpha(c, 0.4));
        }
    }
}

/// The circle of fifths: its rings, the key's neighbourhood lit, a dot a chord.
fn fifths(
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
