//! Songs: every project in `~/Music/tempo` (and those opened from elsewhere), each with a cover
//! drawn from its arrangement; to open in this window, start from a template, duplicate, rename,
//! show in the Finder, or move to the Trash. `tempo new` and `tempo projects` do the same from a
//! terminal.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use gpui::{
    Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, Subscription, Window, div,
    prelude::*, px, rgb,
};
use serde::Serialize;
use tempo_engine::{
    look::{self, Family},
    music::Key,
    project::{FILE, Instrument, Project, Track},
    session::Session,
};

use crate::{
    looks::{family_shade, glyph},
    menu::{Menu, MenuAction},
    text::{TextEvent, TextInput},
    theme::*,
    workspace::{View, Workspace},
};

/// A song, as its card shows it.
#[derive(Clone, Debug, Serialize)]
pub struct Song {
    pub dir: PathBuf,
    pub name: String,
    #[serde(skip)]
    pub modified: SystemTime,
    pub tempo: f64,
    pub key: Option<String>,
    pub signature: [u64; 2],
    pub tracks: usize,
    pub seconds: f64,
    /// Its arrangement, small: each track's colour, and where its regions are (0 to 1 along).
    #[serde(skip)]
    pub lanes: Vec<(u32, Vec<(f32, f32)>)>,
}

/// Where songs are kept: `TEMPO_SONGS`, else `~/Music/tempo`.
pub fn root() -> PathBuf {
    match std::env::var_os("TEMPO_SONGS") {
        Some(d) => d.into(),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Music/tempo"),
    }
}

fn recent_file() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/Application Support/tempo/recent.json")
}

/// The songs opened lately, newest first.
pub fn recent() -> Vec<PathBuf> {
    std::fs::read(recent_file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Remembers a song as just opened.
pub fn opened(dir: &Path) {
    if cfg!(test) {
        return;
    }
    let mut list = recent();
    list.retain(|d| d != dir);
    list.insert(0, dir.to_path_buf());
    list.truncate(20);
    let file = recent_file();
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let _ = std::fs::write(file, serde_json::to_vec_pretty(&list).unwrap());
}

/// What a song is, read from its folder.
pub fn summary(dir: &Path) -> Option<Song> {
    let p = Project::load(dir).ok()?;
    let modified = std::fs::metadata(dir.join(FILE))
        .and_then(|m| m.modified())
        .ok()?;
    let end = p.end().max(p.bar()) as f32;
    let none = |_: &Path| None;
    let mut seen: Vec<(Family, usize)> = Vec::new();
    let lanes = p
        .tracks
        .iter()
        .map(|t| {
            let l = look::look(t, &none);
            let n = match seen.iter_mut().find(|(f, _)| *f == l.family) {
                Some((_, n)) => {
                    *n += 1;
                    *n
                }
                None => {
                    seen.push((l.family, 0));
                    0
                }
            };
            let color = t
                .color
                .as_deref()
                .and_then(look::parse_color)
                .unwrap_or_else(|| family_shade(l.family, n));
            let spans = t
                .regions
                .iter()
                .map(|r| {
                    (
                        r.start as f32 / end,
                        ((r.start + r.length) as f32 / end).min(1.0),
                    )
                })
                .collect();
            (color, spans)
        })
        .collect();
    Some(Song {
        dir: dir.to_path_buf(),
        name: dir.file_name()?.to_string_lossy().into_owned(),
        modified,
        tempo: p.tempo,
        key: p.key.clone(),
        signature: p.time_signature,
        tracks: p.tracks.len(),
        seconds: p.end() as f64 / tempo_engine::project::PPQ as f64 * 60.0 / p.tempo,
        lanes,
    })
}

/// Every song: in `root` (its folders that hold a project), and those opened from elsewhere;
/// newest first.
pub fn list(root: &Path, elsewhere: &[PathBuf]) -> Vec<Song> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|d| d.join(FILE).is_file())
        .collect();
    for d in elsewhere {
        if d.join(FILE).is_file() && !dirs.contains(d) {
            dirs.push(d.clone());
        }
    }
    let mut songs: Vec<Song> = dirs.iter().filter_map(|d| summary(d)).collect();
    songs.sort_by_key(|s| std::cmp::Reverse(s.modified));
    songs
}

/// A name no song in `root` has: `base`, else "base 2", "base 3"…
pub fn unique_name(root: &Path, base: &str) -> String {
    let base = base.trim();
    let base = if base.is_empty() { "Untitled" } else { base };
    (1..)
        .map(|n| match n {
            1 => base.to_string(),
            n => format!("{base} {n}"),
        })
        .find(|name| !root.join(name).exists())
        .unwrap()
}

/// What a new song starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    Empty,
    Beat,
    Band,
    LoFi,
    Electronic,
}

pub const TEMPLATES: [Template; 5] = [
    Template::Empty,
    Template::Beat,
    Template::Band,
    Template::LoFi,
    Template::Electronic,
];

impl Template {
    pub fn name(self) -> &'static str {
        match self {
            Template::Empty => "Empty",
            Template::Beat => "Beat",
            Template::Band => "Band",
            Template::LoFi => "Lo-fi",
            Template::Electronic => "Electronic",
        }
    }

    pub fn from_name(s: &str) -> Option<Template> {
        TEMPLATES.into_iter().find(|t| {
            t.name()
                .replace('-', "")
                .eq_ignore_ascii_case(&s.replace('-', ""))
        })
    }

    /// Its tracks: their names (which say how they look) and instruments.
    pub fn tracks(self) -> Vec<(&'static str, Option<Instrument>)> {
        let synth = || Some(Instrument::Synth);
        let drums = || Some(Instrument::Drums);
        match self {
            Template::Empty => vec![],
            Template::Beat => vec![
                ("Drums", drums()),
                ("808", synth()),
                ("Keys", synth()),
                ("Lead", synth()),
                ("Vocals", None),
            ],
            Template::Band => vec![
                ("Drums", drums()),
                ("Bass", synth()),
                ("Keys", synth()),
                ("Guitar", None),
                ("Vocals", None),
            ],
            Template::LoFi => vec![
                ("Vinyl", None),
                ("Drums", drums()),
                ("Keys", synth()),
                ("Bass", synth()),
            ],
            Template::Electronic => vec![
                ("Drums", drums()),
                ("Bass", synth()),
                ("Lead", synth()),
                ("Pad", synth()),
                ("FX", None),
            ],
        }
    }

    /// What it suits, for its card.
    pub fn about(self) -> &'static str {
        match self {
            Template::Empty => "Nothing yet",
            Template::Beat => "Drums, 808, keys, lead, vocals",
            Template::Band => "Drums, bass, keys, guitar, vocals",
            Template::LoFi => "Vinyl, drums, keys, bass",
            Template::Electronic => "Drums, bass, lead, pad, FX",
        }
    }
}

/// Starts a song in `root`: its name made unique, its tempo, key and time, and a template's
/// tracks. Answers its folder.
pub fn create(
    root: &Path,
    name: &str,
    tempo: f64,
    key: Option<Key>,
    signature: [u64; 2],
    template: Template,
) -> Result<PathBuf, String> {
    if !(20.0..=999.0).contains(&tempo) {
        return Err(format!("a tempo of {tempo} BPM is out of 20 to 999"));
    }
    let dir = root.join(unique_name(root, name));
    let mut p = Project::new(48_000);
    p.tempo = tempo;
    p.key = key.map(|k| k.name());
    p.time_signature = signature;
    for (name, instrument) in template.tracks() {
        p.tracks.push(Track {
            name: name.into(),
            instrument,
            ..Default::default()
        });
    }
    p.assign_ids();
    p.save(&dir)?;
    Ok(dir)
}

/// A copy of a song beside it, "<name> copy": cloned, so it takes no space until it changes.
pub fn duplicate(dir: &Path) -> Result<PathBuf, String> {
    let parent = dir.parent().ok_or("no folder above it")?;
    let name = dir.file_name().ok_or("no name")?.to_string_lossy();
    let to = parent.join(unique_name(parent, &format!("{name} copy")));
    let status = std::process::Command::new("cp")
        .arg("-cR")
        .arg(dir)
        .arg(&to)
        .status()
        .map_err(|e| e.to_string())?;
    // Unsaved edits kept from the original stay with it.
    let _ = std::fs::remove_dir_all(to.join(tempo_engine::project::RECOVERY_DIR));
    match status.success() {
        true => Ok(to),
        false => Err(format!("couldn't copy {}", dir.display())),
    }
}

/// Renames a song's folder (its name is its folder's).
pub fn rename(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return Err(format!("\u{201c}{name}\u{201d} can't name a song"));
    }
    let to = dir.parent().ok_or("no folder above it")?.join(name);
    if to == dir {
        return Ok(to);
    }
    if to.exists() {
        return Err(format!("there's a song called {name} already"));
    }
    std::fs::rename(dir, &to).map_err(|e| e.to_string())?;
    Ok(to)
}

/// Moves a song to the Trash, where it can be taken back from.
pub fn trash(dir: &Path) -> Result<(), String> {
    let trash = PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".Trash");
    let name = dir
        .file_name()
        .ok_or("no name")?
        .to_string_lossy()
        .into_owned();
    let to = (1..)
        .map(|n| match n {
            1 => trash.join(&name),
            n => trash.join(format!("{name} {n}")),
        })
        .find(|p| !p.exists())
        .unwrap();
    std::fs::rename(dir, to).map_err(|e| e.to_string())
}

/// Shows a song's folder in the Finder.
pub fn reveal(dir: &Path) {
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(dir)
        .spawn();
}

/// "Today 08:23", "Yesterday", "12 Oct".
fn when(t: SystemTime) -> String {
    let ago = t.elapsed().map_or(0, |d| d.as_secs());
    match ago {
        0..60 => "Just now".into(),
        60..3_600 => format!("{} min ago", ago / 60),
        3_600..86_400 => format!("{} h ago", ago / 3_600),
        86_400..172_800 => "Yesterday".into(),
        _ => format!("{} days ago", ago / 86_400),
    }
}

fn length(seconds: f64) -> String {
    let s = seconds.round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The new-song sheet's choices.
pub struct NewSong {
    pub name: Entity<TextInput>,
    pub tempo: Entity<TextInput>,
    pub key: Option<Key>,
    pub signature: [u64; 2],
    pub template: Template,
    _subs: Vec<Subscription>,
}

/// A song being renamed on its card.
pub struct Renaming {
    pub dir: PathBuf,
    pub field: Entity<TextInput>,
    _done: Subscription,
}

impl Workspace {
    /// Looks for songs again, in the background.
    pub fn refresh_songs(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) && std::env::var_os("TEMPO_TEST_SONGS").is_none() {
            return;
        }
        let elsewhere = recent();
        let job = cx.background_spawn(async move { list(&root(), &elsewhere) });
        cx.spawn(async move |this, cx| {
            let songs = job.await;
            this.update(cx, |ws, cx| {
                ws.songs = songs;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// ⌘O: the Songs page, or the tracks again.
    pub fn toggle_songs(&mut self, cx: &mut Context<Self>) {
        self.view = match self.view {
            View::Songs => View::Arrange,
            _ => {
                self.refresh_songs(cx);
                View::Songs
            }
        };
        cx.notify();
    }

    /// Asks for a song to open in this window, once it can (after the next frame has a window).
    pub fn ask_to_open(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        self.opening = Some(dir);
        cx.notify();
    }

    /// Opens a song in this window, asking first whether to save the one open if it changed.
    pub fn open_song(&mut self, dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if dir == self.session.dir {
            self.view = View::Arrange;
            cx.notify();
            return;
        }
        if !self.session.edited {
            self.switch_to(dir, window, cx);
            return;
        }
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            "Save changes to this song before opening another?",
            None,
            &["Save", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else { return };
            this.update_in(cx, |ws, window, cx| match choice {
                0 if ws.save(cx) => ws.switch_to(dir, window, cx),
                1 => {
                    ws.session.forget_kept();
                    ws.switch_to(dir, window, cx)
                }
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    /// This window on another song: its session, engine, socket and agent; the view of the
    /// tracks from the top.
    pub fn switch_to(&mut self, dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let session = match Session::open(&dir) {
            Ok(s) => s,
            Err(e) => {
                self.status = Some(e);
                cx.notify();
                return;
            }
        };
        let player = match cfg!(test) {
            true => tempo_engine::player::Player::silent(&session.project, &dir),
            false => match tempo_engine::player::Player::start(&session.project, &dir) {
                Ok(p) => p,
                Err(e) => {
                    self.status = Some(e);
                    cx.notify();
                    return;
                }
            },
        };
        let old = std::mem::replace(&mut self.session, session);
        let _ = std::fs::remove_file(crate::server::socket_path(&old.dir));
        self.player = player;
        self.reset_for_song();
        self.status = None;
        self.changed(cx);
        if !cfg!(test) {
            crate::server::serve(cx.entity().downgrade(), &dir, cx);
        }
        opened(&dir);
        self.view = View::Arrange;
        self.offer_kept(window, cx);
        self.refresh_songs(cx);
        cx.notify();
    }

    /// Starts a song from the sheet's choices, and opens it.
    fn create_song(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.new_song.as_ref() else {
            return;
        };
        let name = s.name.read(cx).text().to_string();
        let tempo = s
            .tempo
            .read(cx)
            .text()
            .trim()
            .parse::<f64>()
            .unwrap_or(120.0);
        let (key, signature, template) = (s.key, s.signature, s.template);
        match create(&root(), &name, tempo, key, signature, template) {
            Ok(dir) => {
                self.new_song = None;
                self.open_song(dir, window, cx);
            }
            Err(e) => self.status = Some(e),
        }
        cx.notify();
    }

    /// ⌘N: the sheet for a new song.
    pub fn new_song_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = cx.new(|cx| TextInput::field(&unique_name(&root(), "Untitled"), cx));
        let tempo = cx.new(|cx| {
            let mut t = TextInput::field("", cx);
            t.set_text("120", cx);
            t
        });
        let mut subs = Vec::new();
        for field in [&name, &tempo] {
            subs.push(
                cx.subscribe_in(field, window, |ws, _, e: &TextEvent, window, cx| match e {
                    TextEvent::Confirm(_) => ws.create_song(window, cx),
                    TextEvent::Cancel => {
                        ws.new_song = None;
                        cx.notify();
                    }
                }),
            );
        }
        let focus = gpui::Focusable::focus_handle(name.read(cx), cx);
        focus.focus(window, cx);
        self.new_song = Some(NewSong {
            name,
            tempo,
            key: None,
            signature: [4, 4],
            template: Template::Empty,
            _subs: subs,
        });
        cx.notify();
    }

    fn start_renaming(&mut self, dir: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let field = cx.new(|cx| TextInput::new(&name, cx));
        let from = dir.clone();
        let done = cx.subscribe_in(&field, window, move |ws, _, e: &TextEvent, window, cx| {
            if let TextEvent::Confirm(to) = e {
                ws.rename_song(&from, to, window, cx);
            }
            ws.renaming = None;
            cx.notify();
        });
        let focus = gpui::Focusable::focus_handle(field.read(cx), cx);
        focus.focus(window, cx);
        self.renaming = Some(Renaming {
            dir,
            field,
            _done: done,
        });
        cx.notify();
    }

    /// Renames a song; the one open is saved, renamed and opened again where it went.
    fn rename_song(&mut self, dir: &Path, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let open = dir == self.session.dir;
        if open && self.session.edited && !self.save(cx) {
            return;
        }
        match rename(dir, name) {
            Ok(to) if open => self.switch_to(to, window, cx),
            Ok(_) => self.refresh_songs(cx),
            Err(e) => self.status = Some(e),
        }
        cx.notify();
    }

    /// What a song's card or the title's menu does.
    pub fn song_action(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            MenuAction::OpenSong(dir) => self.open_song(dir, window, cx),
            MenuAction::NewSong => self.new_song_sheet(window, cx),
            MenuAction::ShowSongs => {
                self.refresh_songs(cx);
                self.view = View::Songs;
            }
            MenuAction::DuplicateSong(dir) => match duplicate(&dir) {
                Ok(to) => {
                    self.status = Some(format!(
                        "Duplicated as {}",
                        to.file_name().unwrap_or_default().to_string_lossy()
                    ));
                    self.refresh_songs(cx);
                }
                Err(e) => self.status = Some(e),
            },
            MenuAction::RenameSong(dir) => {
                self.refresh_songs(cx);
                self.view = View::Songs;
                self.start_renaming(dir, window, cx);
            }
            MenuAction::RevealSong(dir) => reveal(&dir),
            MenuAction::TrashSong(dir) => {
                if dir == self.session.dir {
                    self.status = Some(
                        "That's the song open: open another first, then move it to the Trash."
                            .into(),
                    );
                } else {
                    match trash(&dir) {
                        Ok(()) => {
                            self.status = Some(format!(
                                "Moved {} to the Trash",
                                dir.file_name().unwrap_or_default().to_string_lossy()
                            ));
                            self.refresh_songs(cx);
                        }
                        Err(e) => self.status = Some(e),
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// The menu under the song's name in the top bar.
    pub fn song_menu(&self, at: gpui::Point<gpui::Pixels>) -> Menu {
        let dir = self.session.dir.clone();
        let mut items = vec![
            ("New Song…    ⌘N".to_string(), MenuAction::NewSong),
            ("Songs…    ⌘O".to_string(), MenuAction::ShowSongs),
        ];
        let others: Vec<&Song> = self.songs.iter().filter(|s| s.dir != dir).take(5).collect();
        if !others.is_empty() {
            items.push(("Open lately".into(), MenuAction::Nothing));
            items.extend(others.into_iter().map(|s| {
                (
                    format!("    {}", s.name),
                    MenuAction::OpenSong(s.dir.clone()),
                )
            }));
        }
        items.push(("This song".into(), MenuAction::Nothing));
        items.extend([
            (
                "    Rename…".to_string(),
                MenuAction::RenameSong(dir.clone()),
            ),
            (
                "    Duplicate".to_string(),
                MenuAction::DuplicateSong(dir.clone()),
            ),
            (
                "    Show in Finder".to_string(),
                MenuAction::RevealSong(dir),
            ),
            ("    Save    ⌘S".to_string(), MenuAction::Save),
            ("    Export to WAV    ⌘B".to_string(), MenuAction::Export),
        ]);
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// The Songs page: every song, its cover drawn from its arrangement, newest first.
    pub fn songs_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.session.dir.clone();
        let renaming = self
            .renaming
            .as_ref()
            .map(|r| (r.dir.clone(), r.field.clone()));
        let cards: Vec<_> = self
            .songs
            .clone()
            .into_iter()
            .map(|s| {
                let here = s.dir == current;
                let field = renaming
                    .as_ref()
                    .filter(|(d, _)| *d == s.dir)
                    .map(|(_, f)| f.clone());
                song_card(s, here, field, cx)
            })
            .collect();
        let empty = cards.is_empty();
        div()
            .id("songs-page")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(BG))
            .child(
                div()
                    .px(px(36.))
                    .py(px(28.))
                    .flex()
                    .flex_col()
                    .gap(px(22.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(
                                        div()
                                            .text_size(px(24.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Songs"),
                                    )
                                    .child(
                                        div().text_size(px(12.5)).text_color(rgb(TEXT_3)).child(
                                            format!("{} in {}", self.songs.len(), root().display()),
                                        ),
                                    ),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id("new-song")
                                    .h(px(34.))
                                    .px(px(14.))
                                    .flex()
                                    .items_center()
                                    .gap(px(7.))
                                    .rounded(px(9.))
                                    .bg(rgb(TEXT))
                                    .text_color(rgb(BG))
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|d| d.bg(rgb(0xffffff)))
                                    .child(icon(Icon::Plus, 14.).text_color(rgb(BG)))
                                    .child("New Song")
                                    .debug_selector(|| "new-song".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, window, cx| {
                                            ws.new_song_sheet(window, cx)
                                        }),
                                    ),
                            ),
                    )
                    .child(div().flex().flex_wrap().gap(px(18.)).children(cards).when(
                        empty,
                        |d| {
                            d.child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgb(TEXT_3))
                                    .child("No songs yet. Start one with New Song."),
                            )
                        },
                    )),
            )
    }

    /// The new-song sheet, over the window.
    pub fn new_song_view(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let s = self.new_song.as_ref()?;
        let (key, signature, template) = (s.key, s.signature, s.template);
        let label = |t: &str| {
            div()
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_3))
                .child(t.to_uppercase())
        };
        let field = |f: &Entity<TextInput>, sel: &'static str| {
            div()
                .h(px(36.))
                .px(px(11.))
                .flex()
                .items_center()
                .rounded(px(9.))
                .bg(rgb(FIELD))
                .border_1()
                .border_color(rgb(LINE_STRONG))
                .text_size(px(13.5))
                .debug_selector(move || sel.into())
                .child(f.clone())
        };
        let seg = |text: String, on: bool| {
            div()
                .h(px(30.))
                .px(px(12.))
                .flex()
                .items_center()
                .rounded(px(8.))
                .text_size(px(12.5))
                .cursor_pointer()
                .border_1()
                .when(on, |d| {
                    d.bg(alpha(ACCENT, 0.2))
                        .border_color(alpha(ACCENT, 0.7))
                        .text_color(rgb(TEXT))
                })
                .when(!on, |d| {
                    d.bg(rgb(CARD))
                        .border_color(rgb(LINE))
                        .text_color(rgb(TEXT_2))
                        .hover(|d| d.bg(rgb(RAISED)))
                })
                .child(text)
        };
        let signatures = [[4, 4], [3, 4], [6, 8], [7, 8]].map(|sig| {
            seg(format!("{}/{}", sig[0], sig[1]), signature == sig).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    if let Some(s) = ws.new_song.as_mut() {
                        s.signature = sig;
                    }
                    cx.notify();
                }),
            )
        });
        let key_label = key.map_or("No key".to_string(), |k| k.name());
        let key_pick = seg(format!("{key_label}  ▾"), key.is_some())
            .debug_selector(|| "new-song-key".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    let mut items = vec![("No key".to_string(), MenuAction::NewSongKey(None))];
                    items.extend(
                        Key::all().map(|k| (k.name(), MenuAction::NewSongKey(Some(k.name())))),
                    );
                    ws.menu = Some(Menu {
                        at: e.position,
                        items,
                        columns: 2,
                        lead: 1,
                        ..Default::default()
                    });
                    cx.notify();
                }),
            );
        let templates = TEMPLATES.map(|t| {
            let on = t == template;
            let none = |_: &Path| None;
            let marks: Vec<_> = t
                .tracks()
                .into_iter()
                .map(|(name, instrument)| {
                    let track = Track {
                        name: name.into(),
                        instrument,
                        ..Default::default()
                    };
                    let l = look::look(&track, &none);
                    (l.icon, family_shade(l.family, 0))
                })
                .collect();
            div()
                .id(t.name())
                .w(px(132.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .p(px(11.))
                .rounded(px(11.))
                .cursor_pointer()
                .border_1()
                .when(on, |d| {
                    d.bg(alpha(ACCENT, 0.12)).border_color(alpha(ACCENT, 0.7))
                })
                .when(!on, |d| {
                    d.bg(rgb(CARD))
                        .border_color(rgb(LINE))
                        .hover(|d| d.bg(rgb(RAISED)))
                })
                .child(
                    div()
                        .h(px(20.))
                        .flex()
                        .gap(px(4.))
                        .children(
                            marks
                                .into_iter()
                                .map(|(i, c)| glyph(i, 16.).text_color(rgb(c))),
                        )
                        .when(t == Template::Empty, |d| {
                            d.child(icon(Icon::Plus, 16.).text_color(rgb(TEXT_3)))
                        }),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t.name()),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .line_height(px(15.))
                        .text_color(rgb(TEXT_3))
                        .child(t.about()),
                )
                .debug_selector(move || format!("template-{}", t.name()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if let Some(s) = ws.new_song.as_mut() {
                            s.template = t;
                        }
                        cx.notify();
                    }),
                )
        });
        let sheet = div()
            .id("new-song-sheet")
            .w(px(720.))
            .flex()
            .flex_col()
            .gap(px(18.))
            .p(px(24.))
            .rounded(px(16.))
            .bg(rgb(0x18181d))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("New Song"),
            )
            .child(
                div()
                    .flex()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(7.))
                            .child(label("Name"))
                            .child(field(&s.name, "new-song-name")),
                    )
                    .child(
                        div()
                            .w(px(110.))
                            .flex()
                            .flex_col()
                            .gap(px(7.))
                            .child(label("Tempo"))
                            .child(field(&s.tempo, "new-song-tempo")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(24.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(7.))
                            .child(label("Key"))
                            .child(key_pick),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(7.))
                            .child(label("Time"))
                            .child(div().flex().gap(px(6.)).children(signatures)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .child(label("Start with"))
                    .child(div().flex().gap(px(10.)).children(templates)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(10.))
                    .child(
                        div()
                            .id("new-song-cancel")
                            .h(px(34.))
                            .px(px(16.))
                            .flex()
                            .items_center()
                            .rounded(px(9.))
                            .bg(rgb(FIELD))
                            .text_size(px(13.))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(RAISED)))
                            .child("Cancel")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.new_song = None;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .id("new-song-create")
                            .h(px(34.))
                            .px(px(18.))
                            .flex()
                            .items_center()
                            .rounded(px(9.))
                            .bg(rgb(ACCENT))
                            .text_color(rgb(0xffffff))
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|d| d.opacity(0.9))
                            .child("Create")
                            .debug_selector(|| "new-song-create".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, window, cx| ws.create_song(window, cx)),
                            ),
                    ),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(alpha(0x000000, 0.55))
                .child(sheet),
        )
    }

    /// What goes back to the start when another song opens.
    fn reset_for_song(&mut self) {
        self.track = 0;
        self.region = None;
        self.note = None;
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
        self.picker = None;
        self.trying = None;
        self.menu = None;
        self.pattern = None;
        self.agent.clear();
        self.heard = Default::default();
        self.reset_links();
        self.chat.forget();
    }
}

/// A song's cover: its arrangement in miniature, each track a lane of its colour.
fn cover(lanes: &[(u32, Vec<(f32, f32)>)], w: f32, h: f32) -> Div {
    let rows = lanes.len().clamp(1, 14);
    let lane_h = ((h - 16.0) / rows as f32).min(9.0);
    div()
        .w(px(w))
        .h(px(h))
        .relative()
        .overflow_hidden()
        .rounded(px(10.))
        .bg(rgb(0x101015))
        .children(
            lanes
                .iter()
                .take(rows)
                .enumerate()
                .flat_map(|(i, (color, spans))| {
                    let top = 8.0 + i as f32 * lane_h;
                    let color = *color;
                    spans
                        .iter()
                        .map(move |(a, b)| {
                            div()
                                .absolute()
                                .top(px(top + 1.0))
                                .left(px(8.0 + a * (w - 16.0)))
                                .w(px(((b - a) * (w - 16.0) - 1.0).max(2.0)))
                                .h(px((lane_h - 2.0).max(2.0)))
                                .rounded(px(2.))
                                .bg(alpha(color, 0.78))
                        })
                        .collect::<Vec<_>>()
                }),
        )
}

/// A song's card on the Songs page.
fn song_card(
    s: Song,
    here: bool,
    renaming: Option<Entity<TextInput>>,
    cx: &mut Context<Workspace>,
) -> gpui::Stateful<Div> {
    let dir = s.dir.clone();
    let meta = {
        let mut parts = vec![format!("{:.0} BPM", s.tempo)];
        if let Some(k) = s.key.as_deref().and_then(Key::parse) {
            parts.push(k.short());
        }
        parts.push(format!(
            "{} track{}",
            s.tracks,
            if s.tracks == 1 { "" } else { "s" }
        ));
        if s.seconds > 0.5 {
            parts.push(length(s.seconds));
        }
        parts.join(" · ")
    };
    let open_dir = dir.clone();
    let menu_dir = dir.clone();
    div()
        .id(gpui::SharedString::from(format!("song-{}", s.name)))
        .w(px(264.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .p(px(10.))
        .rounded(px(14.))
        .bg(rgb(CARD))
        .border_1()
        .border_color(if here {
            alpha(ACCENT, 0.6)
        } else {
            rgb(LINE).into()
        })
        .cursor_pointer()
        .hover(|d| d.bg(rgb(0x1f1f25)))
        .debug_selector({
            let name = s.name.clone();
            move || format!("song-{name}")
        })
        .child(cover(&s.lanes, 244.0, 112.0))
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(match renaming {
                            Some(f) => div()
                                .h(px(24.))
                                .px(px(6.))
                                .rounded(px(6.))
                                .bg(rgb(FIELD))
                                .text_size(px(14.))
                                .child(f),
                            None => div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        })
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_3))
                                .child(meta),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(if here { ACCENT_TEXT } else { TEXT_4 }))
                                .child(if here {
                                    "Open now".to_string()
                                } else {
                                    when(s.modified)
                                }),
                        ),
                )
                .child(
                    div()
                        .id("song-more")
                        .size(px(26.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(7.))
                        .text_color(rgb(TEXT_3))
                        .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                        .child(icon(Icon::More, 15.))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let d = menu_dir.clone();
                                ws.menu = Some(Menu {
                                    at: e.position,
                                    items: vec![
                                        ("Open".into(), MenuAction::OpenSong(d.clone())),
                                        ("Rename…".into(), MenuAction::RenameSong(d.clone())),
                                        ("Duplicate".into(), MenuAction::DuplicateSong(d.clone())),
                                        (
                                            "Show in Finder".into(),
                                            MenuAction::RevealSong(d.clone()),
                                        ),
                                        ("Move to Trash".into(), MenuAction::TrashSong(d)),
                                    ],
                                    ..Default::default()
                                });
                                cx.notify();
                            }),
                        ),
                ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, window, cx| {
                if ws.renaming.is_none() {
                    ws.open_song(open_dir.clone(), window, cx);
                }
            }),
        )
}

/// A song in the Browser's results.
pub fn song_row(s: Song, on: bool, here: bool, cx: &mut Context<Workspace>) -> Div {
    let dir = s.dir.clone();
    div()
        .flex()
        .items_center()
        .gap(px(9.))
        .h(px(40.))
        .px(px(6.))
        .rounded(px(8.))
        .cursor_pointer()
        .when(on, |d| d.bg(alpha(ACCENT, 0.16)))
        .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
        .child(cover(&s.lanes, 44.0, 28.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child(s.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(if here {
                            "Open now".to_string()
                        } else {
                            format!("{:.0} BPM · {}", s.tempo, when(s.modified))
                        }),
                ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, _, window, cx| ws.open_song(dir.clone(), window, cx)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn songs_are_made_listed_copied_renamed_and_trashed() {
        let root = std::env::temp_dir().join(format!("tempo-test-songs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let a = create(
            &root,
            "Night",
            92.0,
            Key::parse("A minor"),
            [4, 4],
            Template::Beat,
        )
        .unwrap();
        let b = create(&root, "Night", 120.0, None, [3, 4], Template::Empty).unwrap();
        assert_eq!(b.file_name().unwrap(), "Night 2");
        assert!(create(&root, "Bad", 5.0, None, [4, 4], Template::Empty).is_err());
        let p = Project::load(&a).unwrap();
        assert_eq!(
            (p.tempo, p.key.as_deref(), p.tracks.len()),
            (92.0, Some("A minor"), 5)
        );
        let songs = list(&root, &[]);
        assert_eq!(songs.len(), 2);
        let night = songs.iter().find(|s| s.dir == a).unwrap();
        assert_eq!((night.tracks, night.lanes.len()), (5, 5));
        let copy = duplicate(&a).unwrap();
        assert_eq!(copy.file_name().unwrap(), "Night copy");
        assert!(Project::load(&copy).is_ok());
        let renamed = rename(&copy, "Morning").unwrap();
        assert!(renamed.join(FILE).is_file() && !copy.exists());
        assert!(rename(&renamed, "Night").is_err());
        assert!(rename(&renamed, "a/b").is_err());
        assert_eq!(Template::from_name("lofi"), Some(Template::LoFi));
        std::fs::remove_dir_all(root).unwrap();
    }
}
