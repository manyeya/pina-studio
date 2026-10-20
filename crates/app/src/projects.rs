//! Songs: every project in `~/Music/Pina Studio` (and those opened from elsewhere), each with a cover
//! drawn from its arrangement; to open in this window, start from a template, duplicate, rename,
//! show in the Finder, or move to the Trash (several at once, or the one open). With no song
//! open the Songs page is the whole window: where Pina Studio starts. `pina new` and `pina projects`
//! do the same from a terminal.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use gpui::{
    Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, Subscription, Window, div,
    prelude::*, px, rgb,
};
use pina_engine::{
    look::{self, Family},
    music::Key,
    project::{FILE, Instrument, Project, Track},
    session::Session,
};
use serde::Serialize;

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

/// Where songs are kept: `PINA_SONGS`, else `~/Music/Pina Studio`.
pub fn root() -> PathBuf {
    pina_engine::paths::songs()
}

fn recent_file() -> PathBuf {
    pina_engine::paths::support().join("recent.json")
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
        seconds: p.end() as f64 / pina_engine::project::PPQ as f64 * 60.0 / p.tempo,
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
    let _ = std::fs::remove_dir_all(to.join(pina_engine::project::RECOVERY_DIR));
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

/// Moves a song to the Trash, where it can be taken back from: as the Finder does, so it works
/// on any volume and without special access. (Tests remove it instead: they never touch the
/// person's Trash.)
pub fn trash(dir: &Path) -> Result<(), String> {
    if cfg!(test) {
        return std::fs::remove_dir_all(dir).map_err(|e| e.to_string());
    }
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let url = NSURL::fileURLWithPath(&NSString::from_str(&dir.to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| e.localizedDescription().to_string())
}

/// A new song not made yet, where songs are kept: what a window with no song open has.
pub fn scratch() -> PathBuf {
    let root = root();
    root.join(unique_name(&root, "Untitled"))
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

/// The Songs page's own: a new song's name, tempo, key and time (a template's click makes it),
/// the search, and the songs picked to act on together.
pub struct Home {
    pub name: Entity<TextInput>,
    pub tempo: Entity<TextInput>,
    pub key: Option<Key>,
    pub signature: [u64; 2],
    pub search: Entity<TextInput>,
    pub picks: Vec<PathBuf>,
    _subs: Vec<Subscription>,
}

impl Home {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Home {
        let name = cx.new(|cx| TextInput::search("Untitled", cx));
        let tempo = cx.new(|cx| {
            let mut t = TextInput::field("", cx);
            t.set_text("120", cx);
            t
        });
        let search = cx.new(|cx| TextInput::search("Search songs", cx));
        let mut subs = Vec::new();
        // Enter in either: an empty song, at once.
        for field in [&name, &tempo] {
            subs.push(
                cx.subscribe_in(field, window, |ws, _, e: &TextEvent, window, cx| {
                    if let TextEvent::Confirm(_) = e {
                        ws.create_song(Template::Empty, window, cx);
                    }
                }),
            );
        }
        subs.push(cx.observe(&search, |_, _, cx| cx.notify()));
        subs.push(cx.subscribe(&search, |_, search, e: &TextEvent, cx| {
            if let TextEvent::Cancel = e {
                search.update(cx, |s, cx| s.set_text("", cx));
            }
        }));
        Home {
            name,
            tempo,
            key: None,
            signature: [4, 4],
            search,
            picks: Vec::new(),
            _subs: subs,
        }
    }
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
        if cfg!(test) && std::env::var_os("PINA_TEST_SONGS").is_none() {
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

    /// ⌘O: the Songs page, or the tracks again (if there's a song open to go back to).
    pub fn toggle_songs(&mut self, cx: &mut Context<Self>) {
        self.view = match self.view {
            View::Songs if !self.home_alone() => View::Arrange,
            View::Songs => View::Songs,
            _ => {
                self.refresh_songs(cx);
                View::Songs
            }
        };
        cx.notify();
    }

    /// Whether the window has no song open: a new one not started, so only the Songs page.
    pub fn home_alone(&self) -> bool {
        self.view == View::Songs && !self.session.on_disk && !self.session.edited
    }

    /// ⌘N, or Songs: the Songs page; for ⌘N, its new song's name ready to type.
    pub fn show_home(&mut self, name: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.view = View::Songs;
        self.refresh_songs(cx);
        let home = self.home.get_or_insert_with(|| Home::new(window, cx));
        if name {
            let focus = gpui::Focusable::focus_handle(home.name.read(cx), cx);
            focus.focus(window, cx);
        }
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
            true => pina_engine::player::Player::silent(&session.project, &dir),
            false => match pina_engine::player::Player::start(&session.project, &dir) {
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
        // A song not made yet (the window closing one) opens on the Songs page.
        match self.session.on_disk {
            true => {
                opened(&dir);
                self.view = View::Arrange;
            }
            false => self.view = View::Songs,
        }
        if let Some(h) = self.home.as_mut() {
            h.picks.clear();
        }
        self.offer_kept(window, cx);
        self.refresh_songs(cx);
        cx.notify();
    }

    /// Starts a song from the Songs page's settings and a template, and opens it.
    pub fn create_song(&mut self, template: Template, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = self.home.as_ref() else {
            return;
        };
        let name = h.name.read(cx).text().trim().to_string();
        let tempo = h
            .tempo
            .read(cx)
            .text()
            .trim()
            .parse::<f64>()
            .unwrap_or(120.0);
        let name = if name.is_empty() { "Untitled" } else { &name };
        match create(&root(), name, tempo, h.key, h.signature, template) {
            Ok(dir) => {
                h.name.update(cx, |f, cx| f.set_text("", cx));
                self.open_song(dir, window, cx);
            }
            Err(e) => self.status = Some(e),
        }
        cx.notify();
    }

    /// Asks before songs go to the Trash.
    fn ask_to_trash(&mut self, dirs: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if dirs.is_empty() {
            return;
        }
        let name = |d: &Path| {
            d.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        let what = match dirs.as_slice() {
            [one] => format!("Move \u{201c}{}\u{201d} to the Trash?", name(one)),
            many => format!("Move {} songs to the Trash?", many.len()),
        };
        let open = dirs.contains(&self.session.dir);
        let detail = match open {
            true => "It's open: it closes first. You can take it back out of the Trash.",
            false => "You can take it back out of the Trash.",
        };
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &what,
            Some(detail),
            &["Move to Trash", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(0) = answer.await {
                this.update_in(cx, |ws, window, cx| ws.trash_songs(dirs, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Songs to the Trash; the one open, closed first (nothing of it kept: it's going).
    pub fn trash_songs(&mut self, dirs: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if dirs.contains(&self.session.dir) {
            self.session.forget_kept();
            self.switch_to(scratch(), window, cx);
        }
        let mut gone = 0;
        let mut problems = Vec::new();
        for d in &dirs {
            match trash(d) {
                Ok(()) => gone += 1,
                Err(e) => problems.push(e),
            }
        }
        self.status = Some(match (gone, problems.first()) {
            (_, Some(e)) => format!("Couldn't move it to the Trash: {e}"),
            (1, None) => format!(
                "Moved {} to the Trash",
                dirs[0].file_name().unwrap_or_default().to_string_lossy()
            ),
            (n, None) => format!("Moved {n} songs to the Trash"),
        });
        if let Some(h) = self.home.as_mut() {
            h.picks.retain(|p| !dirs.contains(p));
        }
        self.refresh_songs(cx);
        cx.notify();
    }

    /// A song from a folder anywhere, picked in the Finder.
    fn open_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            this.update(cx, |ws, cx| {
                let Some(dir) = paths.into_iter().next() else {
                    return;
                };
                match dir.join(FILE).is_file() {
                    true => ws.ask_to_open(dir, cx),
                    false => {
                        ws.status = Some(format!("No Pina Studio song in {}", dir.display()));
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
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
            MenuAction::NewSong => self.show_home(true, window, cx),
            MenuAction::ShowSongs => self.show_home(false, window, cx),
            MenuAction::CloseSong => {
                if !self.home_alone() {
                    self.open_song(scratch(), window, cx);
                }
            }
            MenuAction::TrashPicked => {
                let picks = self
                    .home
                    .as_ref()
                    .map(|h| h.picks.clone())
                    .unwrap_or_default();
                self.ask_to_trash(picks, window, cx);
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
            MenuAction::TrashSong(dir) => self.ask_to_trash(vec![dir], window, cx),
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
            ("Study a Song…".to_string(), MenuAction::Study),
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
                MenuAction::RevealSong(dir.clone()),
            ),
            ("    Save    ⌘S".to_string(), MenuAction::Save),
            ("    Bounce…    ⌘B".to_string(), MenuAction::Export),
            ("    Close Song".to_string(), MenuAction::CloseSong),
        ]);
        if self.session.on_disk {
            items.push(("    Move to Trash…".to_string(), MenuAction::TrashSong(dir)));
        }
        Menu {
            at,
            items,
            ..Default::default()
        }
    }

    /// The Songs page: a new song on the left (its name, tempo, key and time; a template's click
    /// makes it), every song on the right, newest first, to search, open, pick several of
    /// (⌘-click) and move to the Trash. With no song open, it's the whole window.
    pub fn songs_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.session.dir.clone();
        let open = self.session.on_disk.then(|| {
            current
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        let renaming = self
            .renaming
            .as_ref()
            .map(|r| (r.dir.clone(), r.field.clone()));
        let (search, picks) = match self.home.as_ref() {
            Some(h) => (
                h.search.read(cx).text().trim().to_lowercase(),
                h.picks.clone(),
            ),
            None => (String::new(), Vec::new()),
        };
        let songs: Vec<Song> = self
            .songs
            .iter()
            .filter(|s| search.is_empty() || s.name.to_lowercase().contains(&search))
            .cloned()
            .collect();
        let shown = songs.len();
        let cards: Vec<_> = songs
            .into_iter()
            .map(|s| {
                let here = s.dir == current;
                let picked = picks.contains(&s.dir);
                let field = renaming
                    .as_ref()
                    .filter(|(d, _)| *d == s.dir)
                    .map(|(_, f)| f.clone());
                song_card(s, here, picked, field, cx)
            })
            .collect();
        let empty = match (self.songs.is_empty(), shown) {
            (true, _) => Some("No songs yet. Pick how to start one, on the left.".to_string()),
            (false, 0) => Some(format!("No songs called \u{201c}{search}\u{201d}")),
            _ => None,
        };
        let picked_bar = (!picks.is_empty()).then(|| {
            div()
                .h(px(40.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(10.))
                .bg(alpha(ACCENT, 0.12))
                .border_1()
                .border_color(alpha(ACCENT, 0.4))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(match picks.len() {
                            1 => "1 song picked".to_string(),
                            n => format!("{n} songs picked"),
                        }),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .child("⌘-click to pick more · Delete to trash them"),
                )
                .child(div().flex_1())
                .child(
                    button("Move to Trash", false)
                        .text_color(rgb(RECORD))
                        .debug_selector(|| "songs-trash-picked".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, window, cx| {
                                ws.song_action(MenuAction::TrashPicked, window, cx)
                            }),
                        ),
                )
                .child(
                    button("Done", false)
                        .debug_selector(|| "songs-unpick".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                if let Some(h) = ws.home.as_mut() {
                                    h.picks.clear();
                                }
                                cx.notify();
                            }),
                        ),
                )
        });
        let new_song = self.new_song_column(open, cx);
        let search_field = self.home.as_ref().map(|h| {
            div()
                .w(px(240.))
                .h(px(34.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(7.))
                .rounded(px(9.))
                .bg(rgb(FIELD))
                .border_1()
                .border_color(rgb(LINE))
                .text_size(px(13.))
                .debug_selector(|| "songs-search".into())
                .child(icon(Icon::Search, 14.).text_color(rgb(TEXT_3)))
                .child(div().flex_1().child(h.search.clone()))
        });
        div()
            .id("songs-page")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .bg(rgb(BG))
            .child(new_song)
            .child(
                div()
                    .id("songs-list")
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .px(px(32.))
                            .py(px(28.))
                            .flex()
                            .flex_col()
                            .gap(px(18.))
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
                                                    .text_size(px(22.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child("Songs"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .text_color(rgb(TEXT_3))
                                                    .child(format!(
                                                        "{} in {}",
                                                        self.songs.len(),
                                                        root().display()
                                                    )),
                                            ),
                                    )
                                    .child(div().flex_1())
                                    .children(search_field),
                            )
                            .children(picked_bar)
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(18.))
                                    .children(cards)
                                    .children(empty.map(|e| {
                                        div()
                                            .py(px(40.))
                                            .text_size(px(13.))
                                            .text_color(rgb(TEXT_3))
                                            .child(e)
                                    })),
                            ),
                    ),
            )
    }

    /// The Songs page's left: back to the song open, if there is one; a new song's name,
    /// tempo, key and time, and the templates (a click makes it and opens it); a song from a
    /// folder anywhere; a song to study.
    fn new_song_column(&self, open: Option<String>, cx: &mut Context<Self>) -> Div {
        let label = |t: &'static str| section_title(t).mb(px(-2.));
        let Some(h) = self.home.as_ref() else {
            return div().w(px(360.));
        };
        let (key, signature) = (h.key, h.signature);
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
        let signatures = tray().children([[4, 4], [3, 4], [6, 8], [7, 8]].map(|sig| {
            seg(format!("{}/{}", sig[0], sig[1]), signature == sig, 28.).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    if let Some(h) = ws.home.as_mut() {
                        h.signature = sig;
                    }
                    cx.notify();
                }),
            )
        }));
        let key_pick = dropdown(key.map_or("No key".to_string(), |k| k.name()), 36.)
            .flex_1()
            .rounded(px(9.))
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
                .h(px(56.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(10.))
                .cursor_pointer()
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .hover(|d| d.bg(rgb(RAISED)).border_color(rgb(LINE_STRONG)))
                .child(
                    div()
                        .w(px(92.))
                        .flex()
                        .gap(px(3.))
                        .children(
                            marks
                                .into_iter()
                                .map(|(i, c)| glyph(i, 15.).text_color(rgb(c))),
                        )
                        .when(t == Template::Empty, |d| {
                            d.child(icon(Icon::Plus, 15.).text_color(rgb(TEXT_3)))
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(t.name()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(t.about()),
                        ),
                )
                .child(icon(Icon::Forward, 14.).text_color(rgb(TEXT_3)))
                .debug_selector(move || format!("template-{}", t.name()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, window, cx| ws.create_song(t, window, cx)),
                )
        });
        let link = |glyph: Icon, text: &'static str, sel: &'static str| {
            div()
                .h(px(34.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(9.))
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(rgb(TEXT_2))
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                .debug_selector(move || sel.into())
                .child(icon(glyph, 15.).text_color(rgb(TEXT_3)))
                .child(text)
        };
        div()
            .w(px(360.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .px(px(24.))
            .py(px(26.))
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(24.))
                            .font_weight(FontWeight::BOLD)
                            .child("Pina Studio"),
                    )
                    .children(open.map(|name| {
                        button(format!("Back to {name}"), false)
                            .max_w(px(200.))
                            .overflow_hidden()
                            .debug_selector(|| "songs-back".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.view = View::Arrange;
                                    cx.notify();
                                }),
                            )
                    })),
            )
            .child(label("NEW SONG"))
            .child(field(&h.name, "new-song-name"))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(92.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(div().flex_1().child(field(&h.tempo, "new-song-tempo")))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child("BPM"),
                            ),
                    )
                    .child(key_pick),
            )
            .child(div().flex().child(signatures))
            .child(label("START WITH"))
            .child(div().flex().flex_col().gap(px(8.)).children(templates))
            .child(div().flex_1())
            .child(div().h(px(1.)).bg(rgb(LINE)))
            .child(
                link(
                    Icon::Folder,
                    "Open a song from a folder…",
                    "songs-open-folder",
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.open_folder(cx)),
                ),
            )
            .child(
                link(Icon::Samples, "Study a song…", "songs-study").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.open_study(cx)),
                ),
            )
    }

    /// What goes back to the start when another song opens.
    fn reset_for_song(&mut self) {
        self.track = 0;
        self.region = None;
        self.picked.clear();
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

/// What can be done to a song, from its card's menu.
fn card_menu(dir: &Path, at: gpui::Point<gpui::Pixels>) -> Menu {
    let d = dir.to_path_buf();
    Menu {
        at,
        items: vec![
            ("Open".into(), MenuAction::OpenSong(d.clone())),
            ("Rename…".into(), MenuAction::RenameSong(d.clone())),
            ("Duplicate".into(), MenuAction::DuplicateSong(d.clone())),
            ("Show in Finder".into(), MenuAction::RevealSong(d.clone())),
            ("Move to Trash…".into(), MenuAction::TrashSong(d)),
        ],
        ..Default::default()
    }
}

/// A song's card on the Songs page: a click opens it, ⌘-click picks it (to trash several); its
/// menu (⋯, or a right-click) and, on hover, the Trash.
fn song_card(
    s: Song,
    here: bool,
    picked: bool,
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
    let (open_dir, menu_dir, right_dir, trash_dir) =
        (dir.clone(), dir.clone(), dir.clone(), dir.clone());
    let name = s.name.clone();
    let corner = |glyph: Icon, sel: String, hot: u32| {
        div()
            .size(px(26.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .text_color(rgb(TEXT_3))
            .hover(move |d| d.bg(rgb(RAISED)).text_color(rgb(hot)))
            .debug_selector(move || sel.clone())
            .child(icon(glyph, 15.))
    };
    div()
        .id(gpui::SharedString::from(format!("song-{}", s.name)))
        .group("song-card")
        .relative()
        .w(px(264.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .p(px(10.))
        .rounded(px(14.))
        .bg(rgb(if picked { 0x1c2233 } else { CARD }))
        .border_1()
        .border_color(match (picked, here) {
            (true, _) => alpha(ACCENT, 0.9),
            (false, true) => alpha(ACCENT, 0.5),
            _ => rgb(LINE).into(),
        })
        .cursor_pointer()
        .when(!picked, |d| d.hover(|d| d.bg(rgb(0x1f1f25))))
        .debug_selector({
            let name = s.name.clone();
            move || format!("song-{name}")
        })
        .child(cover(&s.lanes, 244.0, 112.0))
        .when(picked, |d| {
            d.child(
                div()
                    .absolute()
                    .top(px(16.))
                    .left(px(16.))
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .bg(rgb(ACCENT))
                    .child(icon(Icon::Check, 13.).text_color(rgb(0xffffff))),
            )
        })
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(4.))
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
                                .text_color(rgb(if here { ACCENT_TEXT } else { TEXT_3 }))
                                .child(if here {
                                    "Open now".to_string()
                                } else {
                                    when(s.modified)
                                }),
                        ),
                )
                .child(
                    corner(Icon::Trash, format!("song-trash-{name}"), RECORD)
                        .invisible()
                        .group_hover("song-card", |d| d.visible())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.song_pending = Some(MenuAction::TrashSong(trash_dir.clone()));
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    corner(Icon::More, format!("song-more-{}", s.name), TEXT).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.menu = Some(card_menu(&menu_dir, e.position));
                            cx.notify();
                        }),
                    ),
                ),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                // Several picked, this among them: the Trash for them all.
                let picks = ws
                    .home
                    .as_ref()
                    .map(|h| h.picks.clone())
                    .unwrap_or_default();
                ws.menu = Some(match picks.len() > 1 && picks.contains(&right_dir) {
                    true => Menu {
                        at: e.position,
                        items: vec![(
                            format!("Move {} Songs to Trash…", picks.len()),
                            MenuAction::TrashPicked,
                        )],
                        ..Default::default()
                    },
                    false => card_menu(&right_dir, e.position),
                });
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                if ws.renaming.is_some() {
                    return;
                }
                let picking = ws.home.as_ref().is_some_and(|h| !h.picks.is_empty());
                if e.modifiers.platform || e.modifiers.shift || picking {
                    if let Some(h) = ws.home.as_mut() {
                        match h.picks.iter().position(|p| *p == open_dir) {
                            Some(i) => {
                                h.picks.remove(i);
                            }
                            None => h.picks.push(open_dir.clone()),
                        }
                    }
                    cx.notify();
                    return;
                }
                ws.open_song(open_dir.clone(), window, cx);
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
        let root = std::env::temp_dir().join(format!("pina-test-songs-{}", std::process::id()));
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
