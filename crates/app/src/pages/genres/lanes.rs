//! A playbook as lanes: its sections on one timeline, who plays where (a click changes it), how
//! hard each plays, and the open song under it; what it says to do as moves; and every editor
//! it has, docked below.

use std::{sync::Arc, time::Instant};

use gpui::{SharedString, Stateful, relative};
use pina_engine::{
    audio::engine::AudioData,
    music::playbook::moves::{self, Kind, Move},
    song::midi_file::{self, MidiFile},
};

use super::*;
use crate::pages::study::{act, lane, origin, rect};

/// How wide a lane's name is, as on the study's lanes.
const NAME_W: f32 = 92.;

/// What's open under the lanes.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum BookTab {
    #[default]
    Moves,
    Sound,
    Feel,
    Parts,
    Mix,
    Notes,
}

/// Which books the rail shows.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Shelf {
    #[default]
    All,
    Fits,
    Minor,
    Major,
}

/// What's being heard: which book or chord, since when, and how long it is.
pub struct Heard {
    pub what: String,
    since: Instant,
    seconds: f32,
    data: Arc<AudioData>,
    /// A book goes round; a chord plays once.
    round: bool,
}

/// A section's colour: the same kind of section, the same colour.
fn section_color(name: &str, book: u32) -> u32 {
    match name.split(' ').next().unwrap_or(name) {
        "Chorus" => CORAL,
        "Verse" => SKY,
        "Pre-chorus" => AMBER,
        "Bridge" => VIOLET,
        "Interlude" => TEAL,
        "Intro" | "Outro" => SLATE,
        _ => book,
    }
}

impl Workspace {
    /// Plays `file`, rendered on real instruments once and kept by `what`.
    fn hear_notes(&mut self, what: String, file: MidiFile, round: bool, cx: &mut Context<Self>) {
        self.genres.wanted = Some(what.clone());
        let tempo = file.tempo.unwrap_or(120.0);
        if let Some(data) = self.genres.rendered.get(&what).cloned() {
            self.play_rendered(what, data, round, cx);
            return;
        }
        let rate = self.player.rate();
        let dir = self.session.dir.clone();
        let made = cx.background_spawn(async move {
            let [l, r] = midi_file::render_all(&[(&file, 0)], tempo, rate, &dir)?;
            Ok::<_, String>(Arc::new(AudioData::new(vec![l, r])))
        });
        cx.spawn(async move |ws, cx| {
            let made = made.await;
            ws.update(cx, |ws, cx| match made {
                Ok(data) => {
                    ws.genres.rendered.insert(what.clone(), data.clone());
                    if ws.genres.wanted.as_ref() == Some(&what) {
                        ws.play_rendered(what, data, round, cx);
                    }
                }
                Err(e) => {
                    ws.status = Some(e);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn play_rendered(
        &mut self,
        what: String,
        data: Arc<AudioData>,
        round: bool,
        cx: &mut Context<Self>,
    ) {
        let seconds = data.channels()[0].len() as f32 / self.player.rate() as f32;
        match self.player.audition_from(data.clone(), 0.0, 0.0) {
            Ok(()) => {
                self.genres.heard = Some(Heard {
                    what,
                    since: Instant::now(),
                    seconds,
                    data,
                    round,
                })
            }
            Err(e) => self.status = Some(e),
        }
        cx.notify();
    }

    pub(crate) fn stop_book(&mut self, cx: &mut Context<Self>) {
        self.genres.wanted = None;
        self.genres.heard = None;
        self.player.stop_audition();
        cx.notify();
    }

    fn book_keep_going(&mut self, cx: &mut Context<Self>) {
        let Some(h) = &self.genres.heard else {
            return;
        };
        if h.since.elapsed().as_secs_f32() >= h.seconds {
            match h.round {
                true => {
                    let (what, data) = (h.what.clone(), h.data.clone());
                    self.play_rendered(what, data, true, cx);
                }
                false => self.genres.heard = None,
            }
        }
    }

    /// The step of the bar and the bar of the book being heard, while it is.
    fn book_at(&self, p: &Playbook) -> Option<(usize, usize)> {
        let h = self.genres.heard.as_ref().filter(|h| h.what == p.id)?;
        let tempo = (p.tempo.0 + p.tempo.1) / 2.0;
        let steps = h.since.elapsed().as_secs_f64() * tempo / 15.0;
        Some((steps as usize % 16, steps as usize / 16))
    }

    pub(crate) fn genre_sheet(&mut self, p: &Playbook, cx: &mut Context<Self>) -> Div {
        self.book_keep_going(cx);
        let color = genre_color(&p.id);
        let moves = moves::moves(p);
        let head = self.genre_head(p, cx);
        let lanes = self.genre_lanes(p, moves.get(self.genres.picked), color, cx);
        let dock = self.genre_dock(p, &moves, color, cx);
        let side = self.genre_side(p, color, cx);
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .debug_selector(|| "genre-sheet".into())
            .child(head)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(lanes)
                            .child(dock),
                    )
                    .child(side),
            )
    }

    fn genre_head(&mut self, p: &Playbook, cx: &mut Context<Self>) -> Div {
        let playing = self.genres.heard.as_ref().is_some_and(|h| h.what == p.id);
        let here = self.session.project.genre.as_deref() == Some(p.id.as_str());
        let source = p.source.as_ref().and_then(|f| f.file_name()).map(|f| {
            let name = f.to_string_lossy();
            format!("From {}", name.rsplit_once('.').map_or(&*name, |(s, _)| s))
        });
        let bars: u64 = p.sections.iter().map(|s| s.bars).sum();
        let facts: [(&'static str, String, &'static str); 4] = [
            ("Tempo", format!("{:.0}-{:.0}", p.tempo.0, p.tempo.1), "BPM"),
            (
                "Mode",
                if p.minor { "Minor" } else { "Major" }.to_string(),
                "",
            ),
            ("Swing", format!("{:.0}", p.swing), ""),
            ("Loudness", format!("{}", p.lufs), "LUFS"),
        ];
        let id = p.id.clone();
        let hearing = moves::hearing(p);
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(14.))
            .py(px(10.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .id("genre-play")
                    .debug_selector(|| "genre-play".into())
                    .size(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(18.))
                    .bg(rgb(TEXT))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(0xffffff)))
                    .child(
                        icon(if playing { Icon::Stop } else { Icon::Play }, 14.)
                            .text_color(rgb(0x101014)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| match playing {
                            true => ws.stop_book(cx),
                            false => ws.hear_notes(id.clone(), hearing.clone(), true, cx),
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(120.))
                    .max_w(px(280.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .debug_selector(|| "genre-title".into())
                            .text_size(px(15.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(p.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(match source {
                                Some(s) => format!("{s} · {bars} bars"),
                                None => format!("{bars} bars"),
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap(px(2.))
                    .p(px(3.))
                    .rounded(px(9.))
                    .bg(rgb(WELL))
                    .border_1()
                    .border_color(rgb(LINE))
                    .children(facts.into_iter().map(|(name, value, unit)| {
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .px(px(11.))
                            .py(px(4.))
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap(px(3.))
                                    .font_family(MONO)
                                    .whitespace_nowrap()
                                    .child(
                                        div()
                                            .text_size(px(14.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(value),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.5))
                                            .text_color(rgb(TEXT_3))
                                            .child(unit),
                                    ),
                            )
                            .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(name))
                    })),
            )
            .child(div().flex_1())
            .child(
                act(
                    if here {
                        "This song's playbook"
                    } else {
                        "Use for this song"
                    },
                    "genre-use",
                    !here,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.use_genre(cx)),
                ),
            )
    }

    /// Its sections, who plays in them, how hard, and the open song under them.
    fn genre_lanes(
        &mut self,
        p: &Playbook,
        chosen: Option<&Move>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let picked = self.genres.section;
        let song = &self.session.project;
        let bar = song.bar().max(1);
        let book_bars: u64 = p.sections.iter().map(|s| s.bars).sum();
        let song_bars = song.end().div_ceil(bar);
        let total = book_bars.max(song_bars).max(1) as f32;
        let mut at = 0u64;
        let spans: Vec<(f32, f32)> = p
            .sections
            .iter()
            .map(|s| {
                let span = (at as f32 / total, s.bars as f32 / total);
                at += s.bars;
                span
            })
            .collect();
        let every = match total as u64 {
            0..=48 => 4,
            49..=128 => 8,
            _ => 16,
        };
        let (ruler, track) = lane("Bars", Some(24.), None);
        let ticks = (0..total as u64).step_by(every).map(|b| {
            div()
                .absolute()
                .left(relative(b as f32 / total))
                .top_0()
                .bottom_0()
                .pl(px(5.))
                .border_l_1()
                .border_color(rgb(LINE))
                .font_family(MONO)
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .flex()
                .items_center()
                .child((b + 1).to_string())
        });
        let mut lanes = vec![ruler.child(track.children(ticks))];
        // The sections; a click picks one.
        let (row, track) = lane("Form", None, None);
        let blocks = p.sections.iter().enumerate().map(|(i, s)| {
            let (left, width) = spans[i];
            let on = picked == Some(i);
            let color = section_color(&s.name, color);
            div()
                .id(("section", i))
                .debug_selector(move || format!("section-{i}"))
                .absolute()
                .left(relative(left))
                .w(relative(width))
                .top(px(4.))
                .bottom(px(4.))
                .px(px(6.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .overflow_hidden()
                .whitespace_nowrap()
                .bg(alpha(color, 0.3))
                .text_size(px(11.5))
                .text_color(rgb(TEXT))
                .cursor_pointer()
                .hover(|d| d.bg(alpha(color, 0.46)))
                .when(on, |d| d.border_1().border_color(rgb(ACCENT)))
                .child(if width > 0.07 {
                    s.name.clone()
                } else if width > 0.025 {
                    s.name.chars().take(1).collect()
                } else {
                    String::new()
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.genres.section = (ws.genres.section != Some(i)).then_some(i);
                        ws.genre_fields(cx);
                        cx.notify();
                    }),
                )
        });
        lanes.push(row.child(track.children(blocks.collect::<Vec<_>>())));
        // Who plays where: a click changes it, and saves.
        for part in &p.parts {
            let role = part.role.clone();
            let rc = role_color(&role);
            let (row, track) = lane(capital(&role), None, Some(rc));
            let cells: Vec<_> = p
                .sections
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let plays = s.parts.contains(&role);
                    let role = role.clone();
                    let sel = format!("cell-{role}-{i}");
                    let (left, width) = spans[i];
                    div()
                        .id(SharedString::from(sel.clone()))
                        .debug_selector(move || sel.clone())
                        .absolute()
                        .left(relative(left))
                        .w(relative(width))
                        .top_0()
                        .bottom_0()
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .hover(|d| d.bg(alpha(rc, 0.1)))
                        .child(
                            div()
                                .w_full()
                                .h(px(10.))
                                .rounded(px(2.))
                                .border_r_1()
                                .border_color(rgb(BG))
                                .when(plays, |d| d.bg(alpha(rc, 0.6))),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                let role = role.clone();
                                ws.edit_genre(false, cx, move |p| {
                                    if let Some(s) = p.sections.get_mut(i) {
                                        match s.parts.iter().position(|x| *x == role) {
                                            Some(k) => drop(s.parts.remove(k)),
                                            None => s.parts.push(role),
                                        }
                                    }
                                })
                            }),
                        )
                })
                .collect();
            lanes.push(row.child(track.children(cells)));
        }
        // How hard: as measured, in a book from a song; else as it's set.
        let (row, track) = lane("Loudness", None, None);
        let levels: Vec<(f32, f32, f32)> = p
            .sections
            .iter()
            .zip(&spans)
            .map(|(s, (left, width))| {
                let level = match s.lufs {
                    Some(l) => ((l + 26.0) / 16.0).clamp(0.04, 1.0),
                    None => s.energy as f32 / 5.0,
                };
                (*left, *width, level)
            })
            .collect();
        let paint = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (x, y, w, h) = origin(b);
                for (left, width, level) in &levels {
                    let tall = level * (h - 4.0);
                    let fill = alpha(ACCENT, 0.45);
                    rect(
                        window,
                        x + left * w,
                        y + h - tall,
                        width * w - 1.0,
                        tall,
                        fill,
                    );
                }
            },
        )
        .absolute()
        .size_full();
        lanes.push(row.child(track.child(paint)));
        // The song that's open, section by section, where it has sections.
        let (row, track) = lane("This song", Some(30.), None);
        let marks: Vec<(String, f32, f32)> = song
            .markers
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let next = song.markers.get(i + 1).map_or(song.end(), |n| n.tick);
                let from = m.tick as f32 / bar as f32;
                (
                    m.name.clone(),
                    from / total,
                    (next - m.tick) as f32 / bar as f32 / total,
                )
            })
            .collect();
        let empty = marks.is_empty();
        let whole = (song_bars as f32 / total).max(0.0);
        lanes.push(
            row.child(
                track
                    .children(marks.into_iter().map(|(name, left, width)| {
                        div()
                            .absolute()
                            .left(relative(left))
                            .w(relative(width))
                            .top(px(4.))
                            .bottom(px(4.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .rounded(px(3.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .bg(rgb(FIELD))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_2))
                            .child(if width > 0.06 { name } else { String::new() })
                    }))
                    .when(empty && song_bars > 0, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left_0()
                                .w(relative(whole))
                                .top(px(4.))
                                .bottom(px(4.))
                                .px(px(6.))
                                .flex()
                                .items_center()
                                .rounded(px(3.))
                                .bg(rgb(FIELD))
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .child(format!("{song_bars} bars, no sections yet")),
                        )
                    }),
            ),
        );
        // The bars a picked move is about, lit across every lane.
        let lit = chosen
            .filter(|_| self.genres.tab == BookTab::Moves)
            .and_then(|m| m.at);
        div()
            .id("genre-lanes")
            .debug_selector(|| "genre-lanes".into())
            .relative()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(BG))
            .children(lanes)
            .when_some(lit, |d, (from, bars)| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(NAME_W))
                        .right_0()
                        .child(
                            div()
                                .debug_selector(|| "genre-lit".into())
                                .absolute()
                                .left(relative(from as f32 / total))
                                .w(relative(bars as f32 / total))
                                .top_0()
                                .bottom_0()
                                .bg(alpha(AGENT, 0.1))
                                .border_l_1()
                                .border_r_1()
                                .border_color(alpha(AGENT, 0.6)),
                        ),
                )
            })
    }

    fn genre_dock(
        &mut self,
        p: &Playbook,
        moves: &[Move],
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let open = self.genres.tab;
        let tabs = [
            (BookTab::Moves, "Moves"),
            (BookTab::Sound, "Chords and groove"),
            (BookTab::Feel, "Feel"),
            (BookTab::Parts, "Parts"),
            (BookTab::Mix, "Mix target"),
            (BookTab::Notes, "Notes"),
        ];
        let body = match open {
            BookTab::Moves => self.book_moves(moves, cx),
            BookTab::Sound => self.book_sound(p, color, cx),
            BookTab::Feel => self.genre_feel(p, color, cx),
            BookTab::Parts => self.genre_parts(p, cx),
            BookTab::Mix => self.genre_mix(p, color, cx),
            BookTab::Notes => self.genre_notes(p, cx),
        };
        div()
            .flex_none()
            .h(relative(0.4))
            .min_h(px(260.))
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_t_1()
            .border_color(rgb(LINE_STRONG))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(px(2.))
                    .px(px(8.))
                    .pt(px(4.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .children(tabs.map(|(tab, name)| {
                        let on = tab == open;
                        div()
                            .id(name)
                            .debug_selector(move || {
                                format!(
                                    "genre-tab-{}",
                                    name.split(' ').next().unwrap_or(name).to_lowercase()
                                )
                            })
                            .px(px(10.))
                            .pt(px(4.))
                            .pb(px(6.))
                            .text_size(px(12.))
                            .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                            .cursor_pointer()
                            .hover(|d| d.text_color(rgb(TEXT)))
                            .border_b_2()
                            .border_color(if on { rgb(TEXT).into() } else { alpha(0, 0.0) })
                            .child(name)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.genres.tab = tab;
                                    cx.notify();
                                }),
                            )
                    })),
            )
            .child(
                div()
                    .id("genre-dock")
                    .debug_selector(|| "genre-dock".into())
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(14.))
                    .child(body),
            )
    }

    /// What the book says to do, a move at a time; each one asked of the Studio Agent in a click.
    fn book_moves(&mut self, moves: &[Move], cx: &mut Context<Self>) -> Div {
        let picked = self.genres.picked.min(moves.len().saturating_sub(1));
        let song = self
            .session
            .dir
            .file_name()
            .map_or("this song".into(), |n| n.to_string_lossy().into_owned());
        let list = Kind::ALL.iter().flat_map(|kind| {
            let rows: Vec<_> = moves
                .iter()
                .enumerate()
                .filter(|(_, m)| m.kind == *kind)
                .map(|(i, m)| {
                    let on = i == picked;
                    div()
                        .id(("move", i))
                        .debug_selector(move || format!("move-{i}"))
                        .flex()
                        .gap(px(8.))
                        .px(px(8.))
                        .py(px(5.))
                        .rounded(px(6.))
                        .text_size(px(12.5))
                        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                        .cursor_pointer()
                        .when(on, |d| d.bg(rgb(FIELD)))
                        .when(!on, |d| d.hover(|d| d.bg(rgb(CARD))))
                        .child(
                            div()
                                .w(px(14.))
                                .flex_none()
                                .font_family(MONO)
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .child((i + 1).to_string()),
                        )
                        .child(m.title.clone())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.genres.picked = i;
                                cx.notify();
                            }),
                        )
                        .into_any_element()
                })
                .collect();
            let head = (!rows.is_empty()).then(|| {
                div()
                    .px(px(8.))
                    .pt(px(6.))
                    .pb(px(2.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(kind.name())
                    .into_any_element()
            });
            head.into_iter().chain(rows).collect::<Vec<_>>()
        });
        let list: Vec<_> = list.collect();
        let m = moves.get(picked).cloned();
        div()
            .flex()
            .gap(px(20.))
            .child(
                div()
                    .w(px(330.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .children(list),
            )
            .when_some(m, |d, m| {
                let ask = format!(
                    "Using my {} playbook: {}.",
                    self.genres
                        .draft
                        .as_ref()
                        .map_or(String::new(), |p| p.name.clone()),
                    m.ask
                );
                d.child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(
                            div()
                                .text_size(px(20.))
                                .line_height(px(25.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(m.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(13.5))
                                .line_height(px(19.))
                                .text_color(rgb(TEXT_2))
                                .child(m.why.clone()),
                        )
                        .child(
                            div()
                                .p(px(10.))
                                .rounded(px(8.))
                                .border_1()
                                .border_color(rgb(LINE))
                                .bg(rgb(CARD))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_3))
                                        .child("The Studio Agent will be asked"),
                                )
                                .child(div().text_size(px(13.)).child(m.ask.clone())),
                        )
                        .child(
                            div().flex().child(
                                div()
                                    .id("move-do")
                                    .debug_selector(|| "move-do".into())
                                    .h(px(30.))
                                    .px(px(12.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(8.))
                                    .bg(rgb(AGENT))
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x14180a))
                                    .cursor_pointer()
                                    .child(format!("Do this in {song}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            ws.view = View::Arrange;
                                            ws.show_browser = false;
                                            ws.show_agent = true;
                                            ws.ask_agent(ask.clone(), cx);
                                        }),
                                    ),
                            ),
                        ),
                )
            })
    }

    /// The song's own chords, each a click to hear, and its drums, lit as they're heard.
    fn book_sound(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
        let at = self.book_at(p);
        let tempo = (p.tempo.0 + p.tempo.1) / 2.0;
        let pads: Vec<_> = p
            .chords
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let on = at.is_some_and(|(_, bar)| bar % p.chords.len() == i);
                let name = c.clone();
                div()
                    .id(("pad", i))
                    .debug_selector(move || format!("chord-pad-{i}"))
                    .flex_1()
                    .min_w(px(0.))
                    .h(px(58.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(2.))
                    .rounded(px(7.))
                    .border_1()
                    .border_color(rgb(if on { TEXT } else { LINE }))
                    .bg(rgb(CARD))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(FIELD)))
                    .child(
                        div()
                            .max_w_full()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .font_family(MONO)
                            .text_size(px(if c.len() > 4 { 11. } else { 13. }))
                            .font_weight(FontWeight::MEDIUM)
                            .child(c.clone()),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(ACCENT_TEXT))
                            .child(p.numerals.get(i).cloned().unwrap_or_default()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            if let Some(f) = moves::chord_alone(&name, tempo) {
                                ws.hear_notes(format!("chord {name}"), f, false, cx);
                            }
                        }),
                    )
            })
            .collect();
        let steps = at.map(|a| a.0);
        let rows: Vec<_> = moves::beat_rows(p)
            .into_iter()
            .map(|(pitch, row)| {
                let name = match pitch {
                    35 | 36 => "Kick",
                    37..=40 => "Snare",
                    42 | 44 | 46 => "Hats",
                    _ => "Perc",
                };
                let cells = row.chars().enumerate().map(|(i, c)| {
                    let level = match c {
                        'X' => 1.0,
                        'x' => 0.78,
                        'o' => 0.55,
                        'g' => 0.3,
                        _ => 0.0,
                    };
                    div()
                        .flex_1()
                        .h(px(22.))
                        .rounded(px(3.))
                        .bg(rgb(if i % 4 == 0 { RAISED } else { FIELD }))
                        .when(level > 0.0, |d| d.bg(alpha(color, level)))
                        .when(steps == Some(i), |d| d.border_1().border_color(rgb(TEXT)))
                });
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(
                        div()
                            .w(px(52.))
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(name),
                    )
                    .children(cells)
            })
            .collect();
        div()
            .flex()
            .gap(px(20.))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child(
                        match p.chords.is_empty() {
                            true => format!(
                                "This book only says \"{}\". Save it again from its study to keep the song's own chords.",
                                p.harmony
                            ),
                            false => "Chords, the song's own".to_string(),
                        },
                    ))
                    .child(div().flex().gap(px(6.)).children(pads)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(div().text_size(px(11.5)).text_color(rgb(TEXT_3)).child(
                        match p.beat.is_some() {
                            true => "Groove, the song's own, 16 steps",
                            false => "Groove, 16 steps",
                        },
                    ))
                    .children(rows),
            )
    }

    /// The book's name, where it came from, the open song against it, and what's picked.
    fn genre_side(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Stateful<Div> {
        let name = self.genres.fields.as_ref().map(|f| f.name.clone());
        let song = &self.session.project;
        let tempo = song.tempo;
        let fits = tempo >= p.tempo.0 && tempo <= p.tempo.1;
        let minor = song.song_key().map(|k| k.minor);
        let mode_fits = minor.is_none_or(|m| m == p.minor);
        let checks: Vec<(&'static str, String, bool, Option<String>)> = vec![
            (
                "Tempo",
                format!("{tempo:.0} / {:.0}-{:.0}", p.tempo.0, p.tempo.1),
                fits,
                (!fits).then(|| {
                    format!(
                        "Set my song's tempo to {:.0} BPM",
                        (p.tempo.0 + p.tempo.1) / 2.0
                    )
                }),
            ),
            (
                "Mode",
                format!(
                    "{} / {}",
                    match minor {
                        Some(true) => "Minor",
                        Some(false) => "Major",
                        None => "No key yet",
                    },
                    if p.minor { "Minor" } else { "Major" }
                ),
                mode_fits,
                (!mode_fits).then(|| {
                    format!(
                        "Move my song to a {} key",
                        if p.minor { "minor" } else { "major" }
                    )
                }),
            ),
        ];
        let source = p.source.clone();
        let controls = self
            .genres
            .section
            .and_then(|i| p.sections.get(i).map(|s| (i, s.clone())));
        let count = p.sections.len();
        let picked = controls.map(|(i, s)| self.section_controls(i, &s, count, color, cx));
        div()
            .id("genre-side")
            .w(px(272.))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(14.))
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(field_box(name, "genre-name").flex_none())
            .when_some(source, |d, file| {
                d.child(
                    div()
                        .id("genre-source")
                        .debug_selector(|| "genre-source".into())
                        .text_size(px(12.))
                        .text_color(rgb(ACCENT_TEXT))
                        .cursor_pointer()
                        .child("Open the study it came from")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.open_study(cx);
                                ws.show_study(file.clone(), cx);
                            }),
                        ),
                )
            })
            .child(
                div()
                    .pt(px(4.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child("This song against it"),
            )
            .children(
                checks
                    .into_iter()
                    .enumerate()
                    .map(|(i, (what, values, ok, fix))| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .text_size(px(12.5))
                                    .child(what)
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(if ok { MINT } else { AMBER }))
                                            .child(if ok { "Fits" } else { "Off" }),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child(values),
                            )
                            .when_some(fix, |d, fix| {
                                d.child(
                                    div()
                                        .id(("check", i))
                                        .text_size(px(11.5))
                                        .text_color(rgb(ACCENT_TEXT))
                                        .cursor_pointer()
                                        .child(fix.clone())
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |ws, _, _, cx| {
                                                ws.view = View::Arrange;
                                                ws.show_browser = false;
                                                ws.show_agent = true;
                                                ws.ask_agent(fix.clone(), cx);
                                            }),
                                        ),
                                )
                            })
                    }),
            )
            .children(picked)
            .child(
                div()
                    .mt_auto()
                    .pt(px(8.))
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .child(act("Add section", "add-section", false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            let roles: Vec<String> = ws
                                .genres
                                .draft
                                .as_ref()
                                .map(|p| p.parts.iter().map(|x| x.role.clone()).collect())
                                .unwrap_or_default();
                            ws.edit_genre(true, cx, |p| {
                                p.sections.push(Section {
                                    name: format!("Section {}", p.sections.len() + 1),
                                    bars: 8,
                                    parts: roles,
                                    energy: 3,
                                    lufs: None,
                                })
                            });
                            ws.genres.section =
                                ws.genres.draft.as_ref().map(|p| p.sections.len() - 1);
                            ws.genre_fields(cx);
                        }),
                    ))
                    .child(act("Duplicate", "genre-duplicate", false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.duplicate_genre(cx)),
                    ))
                    .child(act("Delete", "genre-delete", false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.delete_genre(cx)),
                    )),
            )
    }
}
