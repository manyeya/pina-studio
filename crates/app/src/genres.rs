//! The Playbooks page: every genre tempo's agents and critic know, to read and to change. A genre
//! shows as its song's shape (a landscape of its sections' energy), its feel, its song map (which
//! part plays in which section, toggled in a grid), its parts, its mix target and its notes;
//! every change is saved to the person's playbook file as it's made.

use gpui::{
    Bounds, Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels,
    ScrollHandle, Stateful, Subscription, Window, canvas, div, point, prelude::*, px, rgb,
};
use tempo_engine::{
    music::Key,
    playbook::{self, Origin, Part, Playbook, Section},
    theory,
};

use crate::{
    text::{TextEvent, TextInput},
    theme::*,
    workspace::{View, Workspace},
};

/// The parts a genre can have, in the order they're shown.
const ROLES: [&str; 11] = [
    "drums", "perc", "bass", "chords", "pad", "lead", "counter", "arp", "vocals", "fx", "texture",
];

/// What the spectrum's six bands are called.
pub(crate) const BANDS: [(&str, &str); 6] = [
    ("Sub", "< 60"),
    ("Bass", "60–250"),
    ("Low mid", "250–1k"),
    ("Mid", "1–4k"),
    ("Presence", "4–8k"),
    ("Air", "> 8k"),
];

/// The floor of the spectrum editor, dB of the total.
pub(crate) const FLOOR: f32 = -32.0;

/// A genre's color: tempo's have their own, the person's one from the palette by name.
pub fn genre_color(id: &str) -> u32 {
    match id {
        "lofi" => 0xffb86b,
        "boom_bap" => 0xff7a59,
        "trap" => 0xa78bfa,
        "drill" => 0x7dd3fc,
        "rnb" => 0xf472b6,
        "neo_soul" => 0xfb923c,
        "pop" => 0x38bdf8,
        "house" => 0x2dd4bf,
        "deep_house" => 0x5eead4,
        "techno" => 0xcbd5e1,
        "afrobeats" => 0xfacc15,
        "amapiano" => 0x3ddc97,
        "dnb" => 0x60a5fa,
        "reggaeton" => 0xf87171,
        "edm" => 0xc084fc,
        "cinematic" => 0xe8c27a,
        "ambient" => 0x93c5fd,
        other => {
            let palette = [CORAL, AMBER, MINT, SKY, VIOLET, PINK, TEAL];
            let h = other
                .bytes()
                .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
            palette[h as usize % palette.len()]
        }
    }
}

/// A part's color, the same everywhere on the page.
pub(crate) fn role_color(role: &str) -> u32 {
    match role {
        "drums" => CORAL,
        "perc" => AMBER,
        "bass" => VIOLET,
        "chords" => TEAL,
        "pad" => SKY,
        "lead" => PINK,
        "counter" => 0xfb923c,
        "arp" => MINT,
        "vocals" => 0xf9a8d4,
        "fx" => SLATE,
        _ => 0x8b8b96,
    }
}

/// A text a genre has, that a field edits.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Text {
    Name,
    Harmony,
    Sound(usize),
    Write(usize),
    Note(usize),
    Section(usize),
}

/// The fields for the genre being edited.
struct Fields {
    name: Entity<TextInput>,
    harmony: Entity<TextInput>,
    sounds: Vec<Entity<TextInput>>,
    writes: Vec<Entity<TextInput>>,
    notes: Vec<Entity<TextInput>>,
    section: Option<Entity<TextInput>>,
    _subs: Vec<Subscription>,
}

#[derive(Default)]
pub struct Genres {
    pub scroll: ScrollHandle,
    pub list: Vec<Playbook>,
    pub problems: Vec<String>,
    /// The genre being edited, as it's being changed.
    pub draft: Option<Playbook>,
    /// The section picked in the song map.
    pub section: Option<usize>,
    fields: Option<Fields>,
}

impl Workspace {
    /// The Playbooks page, on `id` (else the song's genre, else the first).
    pub fn open_genres(&mut self, id: Option<&str>, cx: &mut Context<Self>) {
        let (list, problems) = playbook::load();
        self.genres.list = list;
        self.genres.problems = problems;
        self.view = View::Playbooks;
        let want = id
            .map(str::to_string)
            .or(self.genres.draft.as_ref().map(|d| d.id.clone()))
            .or(self.session.project.genre.clone());
        let id = want
            .filter(|w| self.genres.list.iter().any(|p| p.id == *w))
            .or(self.genres.list.first().map(|p| p.id.clone()));
        if let Some(id) = id {
            self.select_genre(&id, cx);
        }
        cx.notify();
    }

    fn select_genre(&mut self, id: &str, cx: &mut Context<Self>) {
        let other = self.genres.draft.as_ref().is_none_or(|d| d.id != id);
        self.genres.draft = self.genres.list.iter().find(|p| p.id == id).cloned();
        self.genres.section = None;
        if other {
            self.genres.scroll.set_offset(point(px(0.), px(0.)));
        }
        self.genre_fields(cx);
        cx.notify();
    }

    /// Fields for the draft's texts, each saving what's typed into it.
    fn genre_fields(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.genres.draft.clone() else {
            self.genres.fields = None;
            return;
        };
        let mut subs = Vec::new();
        let mut field = |ws: &mut Self, text: &str, hint: &str, what: Text| {
            let f = cx.new(|cx| match what {
                Text::Name => TextInput::form(text, hint, cx).typed(36., 44., FontWeight::BOLD),
                _ => TextInput::form(text, hint, cx),
            });
            subs.push(cx.subscribe(&f, move |ws, _, e: &TextEvent, cx| {
                if let TextEvent::Confirm(t) = e {
                    ws.set_genre_text(what, t.clone(), cx);
                }
            }));
            let _ = ws;
            f
        };
        let name = field(self, &p.name, "Name", Text::Name);
        let harmony = field(
            self,
            &p.harmony,
            "Words for its chords: dark, soulful…",
            Text::Harmony,
        );
        let sounds = (0..p.parts.len())
            .map(|i| {
                field(
                    self,
                    &p.parts[i].sound,
                    "Sounds to look for",
                    Text::Sound(i),
                )
            })
            .collect();
        let writes = (0..p.parts.len())
            .map(|i| field(self, &p.parts[i].write, "How to write it", Text::Write(i)))
            .collect();
        let notes = (0..p.notes.len())
            .map(|i| field(self, &p.notes[i], "A note for the agent", Text::Note(i)))
            .collect();
        let section = self
            .genres
            .section
            .and_then(|s| p.sections.get(s))
            .map(|s| s.name.clone());
        let section = section.map(|name| {
            let i = self.genres.section.unwrap();
            field(self, &name, "Section", Text::Section(i))
        });
        self.genres.fields = Some(Fields {
            name,
            harmony,
            sounds,
            writes,
            notes,
            section,
            _subs: subs,
        });
    }

    fn set_genre_text(&mut self, what: Text, t: String, cx: &mut Context<Self>) {
        self.edit_genre(false, cx, move |p| match what {
            Text::Name if !t.is_empty() => p.name = t,
            Text::Name => {}
            Text::Harmony => p.harmony = t,
            Text::Sound(i) => {
                if let Some(x) = p.parts.get_mut(i) {
                    x.sound = t
                }
            }
            Text::Write(i) => {
                if let Some(x) = p.parts.get_mut(i) {
                    x.write = t
                }
            }
            Text::Note(i) if t.is_empty() && i < p.notes.len() => drop(p.notes.remove(i)),
            Text::Note(i) => {
                if let Some(x) = p.notes.get_mut(i) {
                    *x = t
                }
            }
            Text::Section(i) if !t.is_empty() => {
                if let Some(s) = p.sections.get_mut(i) {
                    s.name = t
                }
            }
            Text::Section(_) => {}
        });
    }

    /// Changes the genre being edited and saves it; `shape` when parts, sections or notes come
    /// or go, so the fields are made again.
    fn edit_genre(&mut self, shape: bool, cx: &mut Context<Self>, f: impl FnOnce(&mut Playbook)) {
        let Some(p) = self.genres.draft.as_mut() else {
            return;
        };
        let before = (p.parts.len(), p.notes.len(), p.sections.len());
        f(p);
        let shape = shape || before != (p.parts.len(), p.notes.len(), p.sections.len());
        let saved = p.clone();
        if let Some(s) = self.genres.section
            && s >= saved.sections.len()
        {
            self.genres.section = saved.sections.len().checked_sub(1);
        }
        self.save_genre(&saved, cx);
        if shape {
            self.genre_fields(cx);
        }
        cx.notify();
    }

    fn save_genre(&mut self, p: &Playbook, cx: &mut Context<Self>) {
        match playbook::save(p) {
            Ok(_) => {
                match self.genres.list.iter_mut().find(|x| x.id == p.id) {
                    Some(x) => *x = p.clone(),
                    None => self.genres.list.push(p.clone()),
                }
                self.status = Some(format!("Saved {}", p.name));
            }
            Err(e) => self.status = Some(format!("{}: {e}", p.name)),
        }
        cx.notify();
    }

    /// A copy of the genre being edited, as a new genre of the person's.
    fn duplicate_genre(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.genres.draft.clone() else {
            return;
        };
        let taken: Vec<String> = self.genres.list.iter().map(|p| p.id.clone()).collect();
        let name = format!("{} 2", p.name);
        let copy = Playbook {
            id: playbook::new_id(&name, &taken),
            name,
            ..p
        };
        self.save_genre(&copy, cx);
        self.select_genre(&copy.id.clone(), cx);
    }

    /// tempo's own back, for a genre the person changed; or the person's genre gone.
    fn reset_genre(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.genres.draft.clone() else {
            return;
        };
        if let Err(e) = playbook::remove(&p.id) {
            self.status = Some(e);
            cx.notify();
            return;
        }
        let gone = playbook::origin(&p) == Origin::Yours;
        self.status = Some(match gone {
            true => format!("Deleted {}", p.name),
            false => format!("{} is tempo's again", p.name),
        });
        let next = match gone {
            true => None,
            false => Some(p.id.as_str()),
        };
        self.genres.draft = None;
        self.open_genres(next, cx);
    }

    /// The song plays this genre: what its agents and critic aim at.
    fn use_genre(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.genres.draft.clone() else {
            return;
        };
        let tempo = self.session.project.tempo;
        self.edit(cx, |ws| ws.session.project.genre = Some(p.id.clone()));
        self.status = Some(match tempo < p.tempo.0 || tempo > p.tempo.1 {
            true => format!(
                "This song is {} now. Its {tempo:.0} BPM is outside {}'s {:.0}–{:.0}.",
                p.name, p.name, p.tempo.0, p.tempo.1
            ),
            false => format!(
                "This song is {} now: its agents and critic follow it.",
                p.name
            ),
        });
    }

    pub fn genres_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let rail = self.genre_rail(cx);
        let main = match self.genres.draft.clone() {
            Some(p) => self.genre_sheet(&p, cx).into_any_element(),
            None => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_3))
                .child("No playbooks")
                .into_any_element(),
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

    /// The genres, down the left.
    fn genre_rail(&mut self, cx: &mut Context<Self>) -> Div {
        let selected = self.genres.draft.as_ref().map(|p| p.id.clone());
        let song = self.session.project.genre.clone();
        let rows: Vec<_> = self
            .genres
            .list
            .iter()
            .map(|p| {
                let on = selected.as_deref() == Some(p.id.as_str());
                let color = genre_color(&p.id);
                let id = p.id.clone();
                let mark = match playbook::origin(p) {
                    Origin::Tempo => None,
                    Origin::Edited => Some("Edited"),
                    Origin::Yours => Some("Yours"),
                };
                let here = song.as_deref() == Some(p.id.as_str());
                div()
                    .id(gpui::SharedString::from(format!("genre-{}", p.id)))
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
                    .debug_selector({
                        let id = p.id.clone();
                        move || format!("genre-{id}")
                    })
                    .child(ribbon(&p.sections, color, 40., 26.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .child(p.name.clone()),
                                    )
                                    .when(here, |d| {
                                        d.child(div().size(px(6.)).rounded(px(3.)).bg(rgb(AGENT)))
                                    }),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!(
                                        "{:.0}–{:.0} · {}",
                                        p.tempo.0,
                                        p.tempo.1,
                                        if p.minor { "minor" } else { "major" }
                                    )),
                            ),
                    )
                    .when_some(mark, |d, m| {
                        d.child(
                            div()
                                .px(px(6.))
                                .h(px(17.))
                                .flex()
                                .items_center()
                                .rounded(px(5.))
                                .text_size(px(10.))
                                .font_weight(FontWeight::MEDIUM)
                                .bg(alpha(color, 0.14))
                                .text_color(rgb(tint(color, 0.35)))
                                .child(m),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.select_genre(&id, cx)),
                    )
            })
            .collect();
        let problems = self.genres.problems.clone();
        div()
            .w(px(272.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(14.))
                    .h(px(52.))
                    .child(
                        div()
                            .id("genres-back")
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .text_color(rgb(TEXT_3))
                            .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .child(icon(Icon::Back, 15.).text_color(rgb(TEXT_2)))
                            .debug_selector(|| "genres-back".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.view = View::Arrange;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Playbooks"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{} genres", self.genres.list.len())),
                            ),
                    ),
            )
            .child(
                div()
                    .id("genre-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows),
            )
            .when(!problems.is_empty(), |d| {
                d.child(
                    div()
                        .m(px(10.))
                        .p(px(9.))
                        .rounded(px(8.))
                        .bg(alpha(RECORD, 0.1))
                        .border_1()
                        .border_color(alpha(RECORD, 0.3))
                        .text_size(px(11.))
                        .line_height(px(15.))
                        .text_color(rgb(TEXT_2))
                        .child(format!("Can't read: {}", problems.join("; "))),
                )
            })
            .child(
                div()
                    .id("genres-folder")
                    .flex_none()
                    .m(px(10.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(OFF)))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_2))
                            .child("Show the files in Finder"),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_4))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child("~/Library/Application Support/tempo/playbooks"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            if let Err(e) = crate::produce::edit_playbooks(None) {
                                ws.status = Some(e);
                                cx.notify();
                            }
                        }),
                    ),
            )
    }

    /// One genre, top to bottom.
    fn genre_sheet(&mut self, p: &Playbook, cx: &mut Context<Self>) -> Stateful<Div> {
        let color = genre_color(&p.id);
        div()
            .id("genre-sheet")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.genres.scroll)
            .child(self.genre_hero(p, color, cx))
            .child(
                div()
                    .px(px(36.))
                    .pb(px(48.))
                    .flex()
                    .flex_col()
                    .gap(px(30.))
                    .child(self.genre_feel(p, color, cx))
                    .child(self.genre_map(p, color, cx))
                    .child(self.genre_parts(p, cx))
                    .child(self.genre_mix(p, color, cx))
                    .child(self.genre_notes(p, cx)),
            )
    }

    /// The genre's name over its song's shape.
    fn genre_hero(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
        let origin = playbook::origin(p);
        let bars: u64 = p.sections.iter().map(|s| s.bars).sum();
        let here = self.session.project.genre.as_deref() == Some(p.id.as_str());
        let name = self.genres.fields.as_ref().map(|f| f.name.clone());
        let sections = p.sections.clone();
        let action = |label: &'static str, primary: bool| {
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
        div()
            .relative()
            .h(px(236.))
            .overflow_hidden()
            .mb(px(26.))
            .child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| landscape(b, &sections, color, window),
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left(px(36.))
                    .right(px(36.))
                    .top(px(30.))
                    .flex()
                    .items_start()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .text_size(px(10.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(tint(color, 0.3)))
                                    .child("PLAYBOOK")
                                    .child(div().text_color(rgb(TEXT_4)).child("·"))
                                    .child(
                                        div()
                                            .font_family(MONO)
                                            .font_weight(FontWeight::NORMAL)
                                            .text_color(rgb(TEXT_3))
                                            .child(p.id.clone()),
                                    )
                                    .child(div().text_color(rgb(TEXT_4)).child("·"))
                                    .child(div().text_color(rgb(TEXT_3)).child(match origin {
                                        Origin::Tempo => "tempo's",
                                        Origin::Edited => "tempo's, edited",
                                        Origin::Yours => "yours",
                                    })),
                            )
                            .child(
                                div()
                                    .h(px(46.))
                                    .debug_selector(|| "genre-name".into())
                                    .when_some(name, |d, f| d.child(f)),
                            )
                            .child(div().text_size(px(13.)).text_color(rgb(TEXT_2)).child(
                                format!(
                                    "{bars} bars · {} sections · {} parts · {:.1} LUFS master",
                                    p.sections.len(),
                                    p.parts.len(),
                                    p.lufs
                                ),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                action(
                                    if here {
                                        "This song's genre"
                                    } else {
                                        "Use in this song"
                                    },
                                    !here,
                                )
                                .when(here, |d| d.opacity(0.8))
                                .debug_selector(|| "genre-use".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.use_genre(cx)),
                                ),
                            )
                            .child(
                                action("Duplicate", false)
                                    .debug_selector(|| "genre-duplicate".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| ws.duplicate_genre(cx)),
                                    ),
                            )
                            .when(origin != Origin::Tempo, |d| {
                                d.child(
                                    action(
                                        match origin {
                                            Origin::Yours => "Delete",
                                            _ => "Reset to tempo's",
                                        },
                                        false,
                                    )
                                    .debug_selector(|| "genre-reset".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| ws.reset_genre(cx)),
                                    ),
                                )
                            }),
                    ),
            )
    }

    /// Tempo, swing, key, groove and harmony.
    fn genre_feel(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
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

    /// The song map: sections as blocks, as long as they are and as tall as they're loud; under
    /// them, which part plays in which, a cell each, clicked on and off.
    fn genre_map(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
        let picked = self.genres.section;
        let roles: Vec<String> = p.parts.iter().map(|x| x.role.clone()).collect();
        let blocks: Vec<_> = p
            .sections
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let on = picked == Some(i);
                let level = s.energy as f32 / 5.0;
                div()
                    .id(("section", i))
                    .flex_grow(s.bars as f32)
                    .flex_basis(px(0.))
                    .min_w(px(58.))
                    .h(px(92.))
                    .relative()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .cursor_pointer()
                    .debug_selector(move || format!("section-{i}"))
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .right_0()
                            .h(gpui::relative(0.18 + 0.82 * level))
                            .rounded_t(px(8.))
                            .rounded_b(px(3.))
                            .bg(alpha(color, 0.10 + 0.32 * level))
                            .border_1()
                            .border_color(if on {
                                rgb(color).into()
                            } else {
                                alpha(color, 0.25)
                            }),
                    )
                    .child(
                        div()
                            .relative()
                            .px(px(8.))
                            .pb(px(7.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(s.name.clone()),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.))
                                    .text_color(rgb(tint(color, 0.4)))
                                    .child(format!("{} bars", s.bars)),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.genres.section = if ws.genres.section == Some(i) {
                                None
                            } else {
                                Some(i)
                            };
                            ws.genre_fields(cx);
                            cx.notify();
                        }),
                    )
            })
            .collect();
        // The grid: a row a part, a cell a section.
        let grid: Vec<_> = roles
            .iter()
            .map(|role| {
                let rc = role_color(role);
                let cells: Vec<_> = p
                    .sections
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let plays = s.parts.contains(role);
                        let role = role.clone();
                        let sel = format!("cell-{role}-{i}");
                        div()
                            .id(gpui::SharedString::from(sel.clone()))
                            .flex_grow(s.bars as f32)
                            .flex_basis(px(0.))
                            .min_w(px(58.))
                            .h(px(20.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .when(plays, |d| d.bg(alpha(rc, 0.72)).hover(|d| d.bg(rgb(rc))))
                            .when(!plays, |d| {
                                d.border_1()
                                    .border_color(rgb(LINE))
                                    .hover(|d| d.bg(alpha(rc, 0.18)).border_color(alpha(rc, 0.5)))
                            })
                            .debug_selector(move || sel.clone())
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
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .w(px(84.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(rc)))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(TEXT_2))
                                    .child(role.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .gap(px(4.))
                            .children(cells),
                    )
            })
            .collect();
        let controls = picked
            .and_then(|i| p.sections.get(i).map(|s| (i, s.clone())))
            .map(|(i, s)| self.section_controls(i, &s, p.sections.len(), color, cx));
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                heading(
                    "Song map",
                    "Its sections, how hard each plays, and who plays in it",
                )
                .child(
                    div()
                        .id("add-section")
                        .h(px(26.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_2))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(OFF)))
                        .child(icon(Icon::Plus, 12.).text_color(rgb(TEXT_2)))
                        .child("Section")
                        .debug_selector(|| "add-section".into())
                        .on_mouse_down(
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
                                    })
                                });
                                ws.genres.section =
                                    ws.genres.draft.as_ref().map(|p| p.sections.len() - 1);
                                ws.genre_fields(cx);
                            }),
                        ),
                ),
            )
            .child(
                div()
                    .p(px(14.))
                    .rounded(px(14.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap(px(4.))
                            .child(div().w(px(84.)).flex_none())
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .items_end()
                                    .gap(px(4.))
                                    .children(blocks),
                            ),
                    )
                    .child(div().h(px(6.)))
                    .children(grid)
                    .children(controls),
            )
    }

    /// The picked section's name, length and energy, where it goes, and taking it out.
    fn section_controls(
        &mut self,
        i: usize,
        s: &Section,
        count: usize,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let name = self.genres.fields.as_ref().and_then(|f| f.section.clone());
        let small = |label: Icon,
                     sel: &'static str,
                     cx: &mut Context<Self>,
                     f: Box<dyn Fn(&mut Playbook)>| {
            let f = std::rc::Rc::new(f);
            div()
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .border_1()
                .border_color(rgb(LINE))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .child(icon(label, 12.).text_color(rgb(TEXT_2)))
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let f = f.clone();
                        ws.edit_genre(true, cx, move |p| f(p))
                    }),
                )
        };
        let energy: Vec<_> = (1..=5u8)
            .map(|e| {
                let on = e <= s.energy;
                div()
                    .w(px(14.))
                    .h(px(6.0 + e as f32 * 3.0))
                    .rounded(px(3.))
                    .cursor_pointer()
                    .bg(if on {
                        rgb(color).into()
                    } else {
                        alpha(color, 0.15)
                    })
                    .debug_selector(move || format!("energy-{e}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.edit_genre(false, cx, move |p| {
                                if let Some(s) = p.sections.get_mut(i) {
                                    s.energy = e
                                }
                            })
                        }),
                    )
            })
            .collect();
        div()
            .mt(px(8.))
            .pt(px(12.))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .flex()
            .items_center()
            .gap(px(14.))
            .child(div().w(px(180.)).child(field_box(name, "section-name")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(small(
                        Icon::Minus,
                        "bars-down",
                        cx,
                        Box::new(move |p| {
                            if let Some(s) = p.sections.get_mut(i) {
                                s.bars = s.bars.saturating_sub(4).max(4)
                            }
                        }),
                    ))
                    .child(
                        div()
                            .min_w(px(58.))
                            .flex()
                            .justify_center()
                            .font_family(MONO)
                            .text_size(px(12.5))
                            .child(format!("{} bars", s.bars)),
                    )
                    .child(small(
                        Icon::Plus,
                        "bars-up",
                        cx,
                        Box::new(move |p| {
                            if let Some(s) = p.sections.get_mut(i) {
                                s.bars = (s.bars + 4).min(128)
                            }
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(4.))
                    .child(
                        div()
                            .mr(px(4.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child("Energy"),
                    )
                    .children(energy),
            )
            .child(div().flex_1())
            .when(i > 0, |d| {
                d.child(small(
                    Icon::Back,
                    "section-left",
                    cx,
                    Box::new(move |p| {
                        p.sections.swap(i, i - 1);
                    }),
                ))
            })
            .when(i + 1 < count, |d| {
                d.child(small(
                    Icon::Forward,
                    "section-right",
                    cx,
                    Box::new(move |p| {
                        p.sections.swap(i, i + 1);
                    }),
                ))
            })
            .child(small(
                Icon::Close,
                "section-remove",
                cx,
                Box::new(move |p| {
                    if i < p.sections.len() {
                        p.sections.remove(i);
                    }
                }),
            ))
    }

    /// Each part: what it sounds like and how it's written.
    fn genre_parts(&mut self, p: &Playbook, cx: &mut Context<Self>) -> Div {
        let fields = self.genres.fields.as_ref();
        let cards: Vec<_> = p
            .parts
            .iter()
            .enumerate()
            .map(|(i, part)| {
                let rc = role_color(&part.role);
                let sound = fields.and_then(|f| f.sounds.get(i).cloned());
                let write = fields.and_then(|f| f.writes.get(i).cloned());
                div()
                    .w(px(330.))
                    .flex_grow(1.0)
                    .p(px(14.))
                    .rounded(px(14.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().w(px(4.)).h(px(18.)).rounded(px(2.)).bg(rgb(rc)))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(capital(&part.role)),
                            )
                            .child(
                                div()
                                    .id(("part-remove", i))
                                    .h(px(24.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .rounded(px(7.))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .cursor_pointer()
                                    .hover(|d| {
                                        d.bg(alpha(RECORD, 0.14)).border_color(alpha(RECORD, 0.5))
                                    })
                                    .child(icon(Icon::Close, 10.).text_color(rgb(TEXT_2)))
                                    .child("Remove")
                                    .debug_selector(move || format!("part-remove-{i}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            ws.edit_genre(true, cx, move |p| {
                                                if i < p.parts.len() {
                                                    let role = p.parts.remove(i).role;
                                                    for s in &mut p.sections {
                                                        s.parts.retain(|x| *x != role);
                                                    }
                                                }
                                            })
                                        }),
                                    ),
                            ),
                    )
                    .child(label("Sounds"))
                    .child(
                        field_box(sound, "part-sound")
                            .debug_selector(move || format!("part-sound-{i}")),
                    )
                    .child(label("How to write it"))
                    .child(
                        field_box(write, "part-write")
                            .debug_selector(move || format!("part-write-{i}")),
                    )
            })
            .collect();
        let missing: Vec<&str> = ROLES
            .iter()
            .copied()
            .filter(|r| !p.parts.iter().any(|x| x.role == *r))
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading(
                "Parts",
                "Who plays, the sounds to look for, how to write them",
            ))
            .child(div().flex().flex_wrap().gap(px(12.)).children(cards))
            .when(!missing.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_3))
                                .child("Add"),
                        )
                        .children(missing.into_iter().map(|role| {
                            let rc = role_color(role);
                            div()
                                .h(px(24.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .rounded(px(12.))
                                .border_1()
                                .border_color(rgb(LINE))
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_2))
                                .cursor_pointer()
                                .hover(|d| d.bg(alpha(rc, 0.14)).border_color(alpha(rc, 0.5)))
                                .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(rc)))
                                .child(role)
                                .debug_selector(move || format!("add-part-{role}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        ws.edit_genre(true, cx, |p| {
                                            p.parts.push(Part {
                                                role: role.into(),
                                                sound: String::new(),
                                                write: String::new(),
                                            })
                                        })
                                    }),
                                )
                        })),
                )
            })
    }

    /// Where its mixes sit: loudness, and the balance of six bands, dragged.
    fn genre_mix(&mut self, p: &Playbook, color: u32, cx: &mut Context<Self>) -> Div {
        let lufs = p.lufs;
        let bands: Vec<_> = (0..6)
            .map(|i| {
                let v = p.spectrum[i];
                let level = ((v - FLOOR) / -FLOOR).clamp(0.0, 1.0);
                let set = move |ws: &mut Workspace,
                                y: f32,
                                b: Bounds<Pixels>,
                                save: bool,
                                cx: &mut Context<Workspace>| {
                    let top = f32::from(b.origin.y);
                    let h = f32::from(b.size.height).max(1.0);
                    let at = (1.0 - ((y - top) / h).clamp(0.0, 1.0)) * -FLOOR + FLOOR;
                    let v = (at * 2.0).round() / 2.0;
                    match save {
                        true => ws.edit_genre(false, cx, move |p| p.spectrum[i] = v),
                        false => {
                            if let Some(p) = ws.genres.draft.as_mut() {
                                p.spectrum[i] = v;
                            }
                            cx.notify();
                        }
                    }
                };
                let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
                let (b1, b2, b3) = (bounds.clone(), bounds.clone(), bounds.clone());
                div()
                    .id(("band", i))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(12.))
                            .text_color(rgb(TEXT))
                            .flex()
                            .justify_center()
                            .child(format!("{v:.1}")),
                    )
                    .child(
                        div()
                            .id(("band-col", i))
                            .h(px(150.))
                            .relative()
                            .rounded(px(9.))
                            .bg(rgb(WELL))
                            .cursor_ns_resize()
                            .debug_selector(move || format!("band-{i}"))
                            .child(
                                canvas(move |b, _, _| b1.set(b), |_, _, _, _| {})
                                    .absolute()
                                    .size_full(),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .bottom_0()
                                    .left_0()
                                    .right_0()
                                    .h(gpui::relative(level))
                                    .rounded(px(9.))
                                    .bg(alpha(color, 0.16 + 0.3 * level))
                                    .border_t_2()
                                    .border_color(rgb(color)),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    set(ws, f32::from(e.position.y), b2.get(), true, cx)
                                }),
                            )
                            .on_mouse_move(cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
                                if e.pressed_button == Some(MouseButton::Left) {
                                    set(ws, f32::from(e.position.y), b3.get(), false, cx)
                                }
                            }))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    if let Some(p) = ws.genres.draft.clone() {
                                        ws.save_genre(&p, cx);
                                    }
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .child(BANDS[i].0),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(9.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(BANDS[i].1),
                            ),
                    )
            })
            .collect();
        let step = |sign: f32, cx: &mut Context<Self>| {
            div()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(LINE))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)))
                .child(
                    icon(if sign < 0.0 { Icon::Minus } else { Icon::Plus }, 13.)
                        .text_color(rgb(TEXT_2)),
                )
                .debug_selector(move || format!("lufs-{}", if sign < 0.0 { "down" } else { "up" }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.edit_genre(false, cx, move |p| {
                            p.lufs = (p.lufs + sign * 0.5).clamp(-24.0, -4.0)
                        })
                    }),
                )
        };
        // Loudness, on a scale from quiet (-20) to loud (-6).
        let fill = ((lufs + 20.0) / 14.0).clamp(0.0, 1.0);
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(heading(
                "Mix target",
                "What a finished master sounds like: the critic and master aim here",
            ))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(
                        tile(
                            "Spectrum",
                            "Each band's share of the energy, dB of the total: drag a band",
                        )
                        .flex_1()
                        .child(div().flex().gap(px(10.)).children(bands)),
                    )
                    .child(
                        tile("Loudness", "Integrated LUFS of the master")
                            .w(px(220.))
                            .flex_none()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(step(-1.0, cx))
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .justify_center()
                                            .font_family(MONO)
                                            .text_size(px(30.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(format!("{lufs:.1}")),
                                    )
                                    .child(step(1.0, cx)),
                            )
                            .child(
                                div().h(px(8.)).rounded(px(4.)).bg(rgb(WELL)).child(
                                    div()
                                        .h_full()
                                        .w(gpui::relative(fill))
                                        .rounded(px(4.))
                                        .bg(rgb(color)),
                                ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .font_family(MONO)
                                    .text_size(px(10.))
                                    .text_color(rgb(TEXT_4))
                                    .child("quiet −20")
                                    .child("−6 loud"),
                            ),
                    ),
            )
    }

    /// What the agent should know about it, a line each.
    fn genre_notes(&mut self, p: &Playbook, cx: &mut Context<Self>) -> Div {
        let fields = self.genres.fields.as_ref();
        let notes: Vec<_> = (0..p.notes.len())
            .map(|i| {
                let f = fields.and_then(|f| f.notes.get(i).cloned());
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .w(px(22.))
                            .flex_none()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_4))
                            .child(format!("{:02}", i + 1)),
                    )
                    .child(div().flex_1().min_w(px(0.)).child(field_box(f, "note")))
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                heading(
                    "Notes for the agent",
                    "What makes it sound like itself, and what sounds amateur",
                )
                .child(
                    div()
                        .id("add-note")
                        .h(px(26.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(rgb(LINE))
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_2))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(OFF)))
                        .child(icon(Icon::Plus, 12.).text_color(rgb(TEXT_2)))
                        .child("Note")
                        .debug_selector(|| "add-note".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.edit_genre(true, cx, |p| p.notes.push(String::new()))
                            }),
                        ),
                ),
            )
            .child(
                div()
                    .p(px(14.))
                    .rounded(px(14.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(notes)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_4))
                            .child("Clear a note to take it out."),
                    ),
            )
    }
}

/// A section's heading: its title, what it's for, and room for a button on the right.
pub(crate) fn heading(title: &'static str, about: &'static str) -> Div {
    div().flex().items_end().gap(px(12.)).child(
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(17.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .child(about),
            ),
    )
}

/// A card holding one setting.
pub(crate) fn tile(title: &'static str, about: &'static str) -> Div {
    div()
        .min_w(px(200.))
        .p(px(14.))
        .rounded(px(14.))
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(LINE))
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_3))
                        .child(title.to_uppercase()),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_4))
                        .child(about),
                ),
        )
}

pub(crate) fn label(text: &'static str) -> Div {
    div()
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(text.to_uppercase())
}

/// A text field's box.
fn field_box(f: Option<Entity<TextInput>>, sel: &'static str) -> Div {
    div()
        .min_h(px(32.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(8.))
        .bg(rgb(FIELD))
        .border_1()
        .border_color(rgb(LINE_STRONG))
        .text_size(px(12.5))
        .debug_selector(move || sel.into())
        .when_some(f, |d, f| d.child(f))
}

pub(crate) fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// A genre's song in miniature: a bar a section, as tall as it's loud.
fn ribbon(sections: &[Section], color: u32, w: f32, h: f32) -> Div {
    div()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .p(px(3.))
        .flex()
        .items_end()
        .gap(px(1.))
        .rounded(px(7.))
        .bg(alpha(color, 0.1))
        .children(sections.iter().map(|s| {
            div()
                .flex_grow(s.bars as f32)
                .flex_basis(px(0.))
                .h(gpui::relative(0.2 + 0.16 * s.energy as f32))
                .rounded(px(1.5))
                .bg(alpha(color, 0.35 + 0.13 * s.energy as f32))
        }))
}

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
fn groove_grid(id: &str, color: u32) -> Div {
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

/// The hero's art: the genre's song as a landscape, its sections' energy as hills, three
/// ridges deep, fading into the page.
fn landscape(b: Bounds<Pixels>, sections: &[Section], color: u32, window: &mut Window) {
    let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    // A soft wash of the genre's color behind it all.
    let mut wash = gpui::PathBuilder::fill();
    wash.move_to(point(px(x0), px(y0)));
    wash.line_to(point(px(x0 + w), px(y0)));
    wash.line_to(point(px(x0 + w), px(y0 + h)));
    wash.line_to(point(px(x0), px(y0 + h)));
    wash.close();
    if let Ok(p) = wash.build() {
        window.paint_path(p, alpha(color, 0.05));
    }
    if sections.is_empty() {
        return;
    }
    let total: u64 = sections.iter().map(|s| s.bars).sum::<u64>().max(1);
    // Energy along the song, eased from section to section.
    let level = |t: f32| -> f32 {
        let mut at = 0.0;
        let mut levels = Vec::with_capacity(sections.len());
        for s in sections {
            let span = s.bars as f32 / total as f32;
            levels.push((at, at + span, s.energy as f32 / 5.0));
            at += span;
        }
        let i = levels
            .iter()
            .position(|(a, b, _)| t >= *a && t <= *b)
            .unwrap_or(levels.len() - 1);
        let (a, b, e) = levels[i];
        let next = levels.get(i + 1).map_or(e, |l| l.2);
        let prev = if i > 0 { levels[i - 1].2 } else { e };
        let into = (t - a) / (b - a).max(1e-6);
        // Rise or fall over the edges of each section, flat across its middle.
        if into < 0.15 {
            let k = 0.5 - 0.5 * (std::f32::consts::PI * (0.5 + into / 0.3)).cos();
            prev + (e - prev) * k
        } else if into > 0.85 {
            let k = 0.5 - 0.5 * (std::f32::consts::PI * ((into - 0.85) / 0.3)).cos();
            e + (next - e) * k
        } else {
            e
        }
    };
    let ridge = |shift: f32, height: f32, wobble: f32, fade: f32, window: &mut Window| {
        let mut path = gpui::PathBuilder::fill();
        path.move_to(point(px(x0), px(y0 + h)));
        let n = 160;
        for k in 0..=n {
            let t = k as f32 / n as f32;
            let e = level((t + shift).clamp(0.0, 1.0));
            let ripple = (t * 23.0 + shift * 40.0).sin() * wobble + (t * 61.0).sin() * wobble * 0.4;
            let y = y0 + h - (0.12 + 0.78 * e) * h * height - ripple;
            path.line_to(point(px(x0 + t * w), px(y)));
        }
        path.line_to(point(px(x0 + w), px(y0 + h)));
        path.close();
        if let Ok(p) = path.build() {
            window.paint_path(p, alpha(color, fade));
        }
    };
    ridge(0.03, 0.95, 6.0, 0.07, window);
    ridge(-0.02, 0.8, 4.0, 0.12, window);
    ridge(0.0, 0.62, 2.0, 0.2, window);
    // The front ridge's edge, drawn.
    let mut edge = gpui::PathBuilder::stroke(px(1.5));
    let n = 160;
    for k in 0..=n {
        let t = k as f32 / n as f32;
        let e = level(t);
        let ripple = (t * 23.0).sin() * 2.0 + (t * 61.0).sin() * 0.8;
        let p = point(
            px(x0 + t * w),
            px(y0 + h - (0.12 + 0.78 * e) * h * 0.62 - ripple),
        );
        if k == 0 {
            edge.move_to(p);
        } else {
            edge.line_to(p);
        }
    }
    if let Ok(p) = edge.build() {
        window.paint_path(p, alpha(color, 0.75));
    }
    // Into the page: the bottom fades to its background.
    for k in 0..12 {
        let t = k as f32 / 12.0;
        let mut band = gpui::PathBuilder::fill();
        let top = y0 + h * (0.7 + 0.3 * t);
        band.move_to(point(px(x0), px(top)));
        band.line_to(point(px(x0 + w), px(top)));
        band.line_to(point(px(x0 + w), px(y0 + h)));
        band.line_to(point(px(x0), px(y0 + h)));
        band.close();
        if let Ok(p) = band.build() {
            window.paint_path(p, alpha(BG, 0.09));
        }
    }
}
