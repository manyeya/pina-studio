//! MIDI in the Agent panel: files given to the agent to study, and what it writes. Each is a card
//! to hear, stop, drag onto the tracks or give it again; what it wrote lives in the song's MIDI
//! folder, so the shelf is still there next time.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use gpui::{MouseDownEvent, PathPromptOptions, Point};
use pina_engine::{
    audio::engine::AudioData,
    music::Key,
    samples::midi,
    song::midi_file::{self, MidiFile},
};

use super::*;
use crate::{
    cli::midi::MIDI_DIR,
    sounds::browser::{outline, piano_roll, role_hue},
    workspace::{Drag, View, pt},
};

/// How many files can go with one message.
const MOST_GIVEN: usize = 32;

/// A MIDI file in the panel.
pub struct MidiCard {
    pub path: PathBuf,
    pub name: String,
    /// Its key, tempo, length and part, in a line.
    pub about: String,
    pub color: u32,
    pub file: Arc<MidiFile>,
    /// The agent wrote it.
    pub made: bool,
}

/// Which page of the panel shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatPage {
    #[default]
    Chat,
    Midi,
    History,
}

/// The path a `midi_write` answered with.
fn written(output: &str) -> Option<PathBuf> {
    let v: Value = serde_json::from_str(output.trim()).ok()?;
    v["file"].as_str().map(PathBuf::from)
}

impl Chat {
    /// Whether the tool call `id` is a `midi_write`.
    fn wrote_midi(&self, id: &str) -> bool {
        self.items.iter().rev().any(|i| {
            matches!(i, Item::Tool { id: x, name, .. } if x == id && name.ends_with("midi_write"))
        })
    }

    /// What went with the last question.
    pub(crate) fn last_given(&self) -> Vec<PathBuf> {
        let asked = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::You(_)))
            .unwrap_or(0);
        self.items[asked..]
            .iter()
            .filter_map(|i| match i {
                Item::Midi { path, given: true } => Some(path.clone()),
                _ => None,
            })
            .collect()
    }
}

impl Workspace {
    /// The panel's shelf, for the song that's open: what its agent has written.
    pub(crate) fn shelf_sync(&mut self) {
        if self.chat.shelf_of == self.session.dir {
            return;
        }
        self.chat.shelf_of = self.session.dir.clone();
        self.chat.shelf.clear();
        self.chat.attached.clear();
        self.chat.hearing = None;
        for path in midi::midi_files(&self.session.dir.join(MIDI_DIR)) {
            self.shelf_card(&path, true);
        }
    }

    /// `path`'s card, made if it hasn't one: its place on the shelf, unless it can't be read.
    fn shelf_card(&mut self, path: &Path, made: bool) -> Option<usize> {
        if let Some(i) = self.chat.shelf.iter().position(|c| c.path == path) {
            return Some(i);
        }
        let mut file = midi_file::open(path).ok()?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        // What the agent wrote says itself which parts are drums; its name doesn't get a say.
        let study = midi::study(&file, if made { "" } else { &name });
        if !made {
            midi::mark_drums(&mut file, &study);
        }
        let key = study.key.as_deref().and_then(Key::parse);
        let about = [
            key.map(|k| k.short()),
            study.tempo.map(|t| format!("{t:.0} BPM")),
            Some(match study.bars {
                1 => "1 bar".to_string(),
                n => format!("{n} bars"),
            }),
            (!study.role.is_empty()).then(|| study.role.clone()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        self.chat.shelf.push(MidiCard {
            path: path.to_path_buf(),
            name,
            about,
            color: role_hue(&study.role),
            file: Arc::new(file),
            made,
        });
        Some(self.chat.shelf.len() - 1)
    }

    /// Files (or the MIDI in folders) to go with the next message.
    pub fn attach_midi(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        self.shelf_sync();
        let mut unread = 0;
        for path in paths {
            let files = match path.is_dir() {
                true => midi::midi_files(path),
                false => vec![path.clone()],
            };
            for file in files {
                if self.chat.attached.len() >= MOST_GIVEN {
                    self.status = Some(format!("{MOST_GIVEN} MIDI files is the most at once."));
                    cx.notify();
                    return;
                }
                match self.shelf_card(&file, false) {
                    Some(_) if !self.chat.attached.contains(&file) => {
                        self.chat.attached.push(file);
                    }
                    Some(_) => {}
                    None => unread += 1,
                }
            }
        }
        let n = self.chat.attached.len();
        self.status = Some(match (unread, n) {
            (0, 1) => "1 MIDI file ready for the Studio Agent.".to_string(),
            (0, n) => format!("{n} MIDI files ready for the Studio Agent."),
            (k, _) => format!("{k} couldn't be read as MIDI."),
        });
        cx.notify();
    }

    /// One file given to the agent, or taken back.
    pub fn give_midi(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match self.chat.attached.iter().position(|p| *p == path) {
            Some(i) => {
                self.chat.attached.remove(i);
                cx.notify();
            }
            None => self.attach_midi(&[path], cx),
        }
    }

    /// Asks which MIDI files to give the agent.
    pub(crate) fn pick_midi_files(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Give to the Studio Agent".into()),
        });
        cx.spawn(async move |ws, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                ws.update(cx, |ws, cx| ws.attach_midi(&paths, cx)).ok();
            }
        })
        .detach();
    }

    /// What's been given, as the agent's told it, and the files: they go with this message.
    pub(crate) fn midi_note(&mut self) -> Option<(String, Vec<PathBuf>)> {
        let given = std::mem::take(&mut self.chat.attached);
        if given.is_empty() {
            return None;
        }
        let list: Vec<String> = given.iter().map(|p| format!("- {}", p.display())).collect();
        Some((
            format!(
                "(MIDI files given to you:\n{}\nRead one with midi_study, notes true; several \
                 with midi_group. Asked for something like them, write it with midi_write, `like` \
                 set to these.)",
                list.join("\n")
            ),
            given,
        ))
    }

    /// What a tool answered: a `midi_write`'s file goes on the shelf, and in the conversation.
    pub(crate) fn heard_midi(&mut self, e: &Event) -> Option<PathBuf> {
        let Event::ToolDone {
            id,
            ok: true,
            output,
        } = e
        else {
            return None;
        };
        if !self.chat.wrote_midi(id) {
            return None;
        }
        let path = written(output)?;
        self.shelf_sync();
        self.shelf_card(&path, true)?;
        Some(path)
    }

    /// Hears a card, at the song's tempo, in time with it while it plays.
    pub(crate) fn audition_card(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(card) = self.chat.shelf.get(i) else {
            return;
        };
        let (path, file) = (card.path.clone(), card.file.clone());
        let tempo = self.session.project.tempo;
        let key = (path, tempo.to_bits(), 0f32.to_bits());
        self.browser.wanted = None;
        self.browser.midi_wanted = None;
        self.chat.wanted = Some(i);
        if let Some(data) = self.browser.heard.get(&key).cloned() {
            return self.play_card(i, data, cx);
        }
        let rate = self.player.rate();
        let dir = self.session.dir.clone();
        let made = cx.background_spawn(async move {
            let [l, r] = midi_file::render(&file, tempo, 0, rate, &dir)?;
            Ok::<_, String>(Arc::new(AudioData::new(vec![l, r])))
        });
        cx.spawn(async move |ws, cx| {
            let made = made.await;
            ws.update(cx, |ws, cx| match made {
                Ok(data) => {
                    if ws.browser.heard.len() >= 64 {
                        ws.browser.heard.clear();
                    }
                    ws.browser.heard.insert(key, data.clone());
                    if ws.chat.wanted == Some(i) {
                        ws.play_card(i, data, cx);
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

    fn play_card(&mut self, i: usize, data: Arc<AudioData>, cx: &mut Context<Self>) {
        let seconds = data.channels()[0].len() as f32 / self.player.rate() as f32;
        match self.player.audition_synced(data, 0.0) {
            Ok(()) => {
                self.browser.audition = None;
                self.browser.midi_playing = None;
                self.browser.looping = self.player.playing();
                self.chat.hearing = Some((i, Instant::now(), seconds));
            }
            Err(e) => self.status = Some(e),
        }
        cx.notify();
    }

    /// Whether card `i` is being heard.
    pub(crate) fn card_heard(&self, i: usize) -> bool {
        self.chat.hearing.is_some_and(|(k, since, seconds)| {
            k == i && (self.browser.looping || since.elapsed().as_secs_f32() < seconds)
        })
    }

    /// Whether `at` (a window point) is over the Agent panel.
    pub(crate) fn over_agent(&self, at: Point<f32>) -> bool {
        self.view == View::Arrange
            && self.show_agent
            && !self.show_browser
            && at.x >= self.viewport_w - self.panel_width()
    }

    /// Whether MIDI is being carried over the panel, so it says what letting go will do.
    fn offering_midi(&self) -> bool {
        match self.drag {
            Some(Drag::Midi {
                at, moved: true, ..
            }) => self.over_agent(at),
            _ => self.file_drag.as_ref().is_some_and(|(at, paths)| {
                self.over_agent(*at) && paths.iter().any(|p| p.is_dir() || midi_file::is_midi(p))
            }),
        }
    }

    /// Over the panel while MIDI's carried across it.
    pub(crate) fn midi_drop_hint(&self) -> Option<Div> {
        self.offering_midi().then(|| {
            div()
                .absolute()
                .top(px(6.))
                .left(px(6.))
                .right(px(6.))
                .bottom(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(12.))
                .bg(alpha(AGENT, 0.12))
                .border_2()
                .border_dashed()
                .border_color(alpha(AGENT, 0.7))
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(AGENT_TEXT))
                .debug_selector(|| "agent-midi-drop".into())
                .child("Give this to the Studio Agent")
        })
    }

    /// A card to hear, drag and add.
    pub(crate) fn midi_card(&self, i: usize, cx: &mut Context<Self>) -> Option<Div> {
        let card = self.chat.shelf.get(i)?;
        let playing = self.card_heard(i);
        let (color, path) = (card.color, card.path.clone());
        let given = self.chat.attached.contains(&path);
        let round = |sel: String| {
            div()
                .size(px(26.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(13.))
                .cursor_pointer()
                .debug_selector(move || sel.clone())
        };
        let play = round(format!("chat-midi-play-{i}"))
            .bg(rgb(if playing { ACCENT } else { TEXT }))
            .child(
                icon(if playing { Icon::Stop } else { Icon::Play }, 12.)
                    .text_color(rgb(if playing { 0xffffff } else { BG })),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    match playing {
                        true => ws.stop_audition(cx),
                        false => ws.audition_card(i, cx),
                    }
                }),
            );
        let small = |label: &'static str, sel: String| {
            div()
                .h(px(22.))
                .px(px(8.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .text_size(px(11.))
                .text_color(rgb(TEXT_2))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .debug_selector(move || sel.clone())
                .child(label)
        };
        let add = small("Add", format!("chat-midi-add-{i}")).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.place_card(i, None, cx);
            }),
        );
        let give = small(
            if given { "Given" } else { "Give" },
            format!("chat-midi-give-{i}"),
        )
        .when(given, |d| {
            d.border_color(rgb(AGENT)).text_color(rgb(AGENT_TEXT))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.give_midi(path.clone(), cx);
            }),
        );
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(9.))
                .p(px(8.))
                .rounded(px(10.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(alpha(color, if playing { 0.8 } else { 0.3 }))
                .cursor_grab()
                .debug_selector(move || format!("chat-midi-{i}"))
                .child(play)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .h(px(26.))
                                .rounded(px(4.))
                                .bg(alpha(color, 0.08))
                                .overflow_hidden()
                                .child(piano_roll(Some(outline(&card.file)), color)),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(card.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_4))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(card.about.clone()),
                        ),
                )
                .child(div().flex().flex_col().gap(px(4.)).child(add).child(give))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        let at = pt(e.position);
                        ws.begin_drag(
                            Drag::ChatMidi {
                                card: i,
                                from: at,
                                at,
                                moved: false,
                            },
                            cx,
                        );
                    }),
                ),
        )
    }

    /// A card into the song: where it's dropped, else at the bar the playhead's in.
    pub(crate) fn place_card(&mut self, i: usize, at: Option<Point<f32>>, cx: &mut Context<Self>) {
        let Some(card) = self.chat.shelf.get(i) else {
            return;
        };
        let (file, name) = (card.file.clone(), card.name.clone());
        self.put_midi(&file, &name, 0, at, cx);
    }

    pub(crate) fn drag_card(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        if let Some(Drag::ChatMidi {
            from, at, moved, ..
        }) = &mut self.drag
        {
            *at = p;
            *moved |= (p.x - from.x).hypot(p.y - from.y) > 4.0;
        }
        cx.notify();
    }

    /// A card let go of over the tracks: it goes where it's dropped.
    pub(crate) fn drop_card(&mut self, drag: Option<Drag>, cx: &mut Context<Self>) {
        if let Some(Drag::ChatMidi {
            card,
            at,
            moved: true,
            ..
        }) = drag
            && self.drop_target(at).is_some()
        {
            self.place_card(card, Some(at), cx);
        }
    }

    /// The card being dragged, by the pointer, saying where it would go.
    pub fn card_ghost(&self) -> Option<impl IntoElement + use<>> {
        let Some(Drag::ChatMidi {
            card,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let c = self.chat.shelf.get(card)?;
        let hint = self.midi_hint(&c.file, at);
        Some(crate::sounds::browser::midi_ghost_at(
            at,
            &c.file,
            c.name.clone(),
            c.color,
            hint,
        ))
    }

    /// Chat, or the shelf of MIDI.
    pub(crate) fn agent_tabs(&self, cx: &mut Context<Self>) -> Div {
        let page = self.chat.page;
        let tab = |name: &'static str, to: ChatPage, sel: &'static str, cx: &mut Context<Self>| {
            seg(name, page == to, 26.)
                .flex_1()
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.chat.page = to;
                        cx.notify();
                    }),
                )
        };
        // Filling their tray evenly, as the Browser's tabs do.
        let count = |n: usize| {
            (n > 0).then(|| {
                div()
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(n.to_string())
            })
        };
        div().flex_none().px(px(10.)).py(px(8.)).child(
            tray()
                .rounded(px(9.))
                .child(tab("Chat", ChatPage::Chat, "agent-tab-chat", cx))
                .child(
                    tab("MIDI", ChatPage::Midi, "agent-tab-midi", cx)
                        .children(count(self.chat.shelf.len())),
                )
                .child(
                    tab("History", ChatPage::History, "agent-tab-history", cx)
                        .children(count(self.chat.past.len())),
                ),
        )
    }

    /// The shelf: all the MIDI in this conversation's reach, what it wrote first.
    pub(crate) fn agent_midi_page(&mut self, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let name = self.chat.kind.def().name;
        let of = |ws: &Self, made: bool, cx: &mut Context<Self>| -> Vec<Div> {
            (0..ws.chat.shelf.len())
                .rev()
                .filter(|&i| ws.chat.shelf[i].made == made)
                .filter_map(|i| ws.midi_card(i, cx))
                .collect()
        };
        let (wrote, given) = (of(self, true, cx), of(self, false, cx));
        let heading = |text: String| {
            div()
                .pt(px(4.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_3))
                .child(text)
        };
        let empty = wrote.is_empty() && given.is_empty();
        div()
            .id("agent-midi")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(7.))
            .p(px(12.))
            .when(empty, |d| {
                d.child(
                    div()
                        .text_size(px(12.5))
                        .line_height(px(18.))
                        .text_color(rgb(TEXT_3))
                        .child(format!(
                            "Give {name} MIDI to study, or ask it to write some: drop files or a \
                             folder on this panel, or use the + below. What it writes lands here \
                             to hear and drag onto the tracks."
                        )),
                )
            })
            .when(!wrote.is_empty(), |d| {
                d.child(heading(format!("Written by {name}")))
                    .children(wrote)
            })
            .when(!given.is_empty(), |d| {
                d.child(heading("Given to it".into())).children(given)
            })
    }

    /// What's going with the next message, and what to do with it in a click.
    pub(crate) fn midi_attached(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.chat.attached.is_empty() {
            return None;
        }
        let many = self.chat.attached.len() > 1;
        let (them, these) = if many {
            ("them", "these")
        } else {
            ("it", "this")
        };
        let chips = self.chat.attached.iter().enumerate().map(|(i, path)| {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            div()
                .max_w(px(180.))
                .h(px(24.))
                .pl(px(8.))
                .pr(px(3.))
                .flex()
                .items_center()
                .gap(px(5.))
                .rounded(px(7.))
                .bg(rgb(RAISED))
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .text_size(px(11.5))
                .text_color(rgb(TEXT_2))
                .child(
                    div()
                        .min_w(px(0.))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(name),
                )
                .child(
                    div()
                        .size(px(17.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(LINE_STRONG)))
                        .debug_selector(move || format!("agent-given-off-{i}"))
                        .child(icon(Icon::Close, 9.).text_color(rgb(TEXT_3)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                if i < ws.chat.attached.len() {
                                    ws.chat.attached.remove(i);
                                }
                                cx.notify();
                            }),
                        ),
                )
        });
        let ask = |label: String, sel: &'static str, text: String, cx: &mut Context<Self>| {
            div()
                .h(px(24.))
                .px(px(9.))
                .flex()
                .items_center()
                .rounded(px(7.))
                .bg(alpha(AGENT, 0.14))
                .border_1()
                .border_color(alpha(AGENT, 0.4))
                .text_size(px(11.5))
                .text_color(rgb(AGENT_TEXT))
                .cursor_pointer()
                .hover(|d| d.bg(alpha(AGENT, 0.24)))
                .debug_selector(move || sel.into())
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.ask_agent(text.clone(), cx)),
                )
        };
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .px(px(10.))
                .pt(px(10.))
                .child(div().flex().flex_wrap().gap(px(4.)).children(chips))
                .when(!self.chat.busy, |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(5.))
                            .child(ask(
                                format!("Analyse {them}"),
                                "agent-midi-analyse",
                                format!("Study {these} and tell me what makes {them} work."),
                                cx,
                            ))
                            .child(ask(
                                format!("Write one like {them}"),
                                "agent-midi-similar",
                                format!(
                                    "Study {these}, then write me a new MIDI like {them} for this \
                                     song. Keep what makes {them} work without copying the notes."
                                ),
                                cx,
                            )),
                    )
                }),
        )
    }

    /// The + by the message: MIDI to give it.
    pub(crate) fn midi_attach_button(&self, cx: &mut Context<Self>) -> Div {
        div()
            .size(px(26.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(RAISED)))
            .debug_selector(|| "agent-attach".into())
            .child(icon(Icon::Plus, 12.).text_color(rgb(TEXT_2)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| ws.pick_midi_files(cx)),
            )
    }
}
