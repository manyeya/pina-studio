//! Songs from the window: listed, opened, switched to, made, trashed and renamed.

use super::*;

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
            true => pina_engine::audio::player::Player::silent(&session.project, &dir),
            false => match pina_engine::audio::player::Player::start(&session.project, &dir) {
                Ok(p) => p,
                Err(e) => {
                    self.status = Some(e);
                    cx.notify();
                    return;
                }
            },
        };
        let old = std::mem::replace(&mut self.session, session);
        let _ = std::fs::remove_file(crate::cli::server::socket_path(&old.dir));
        self.player = player;
        self.reset_for_song();
        self.status = None;
        self.changed(cx);
        if !cfg!(test) {
            crate::cli::server::serve(cx.entity().downgrade(), &dir, cx);
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
    pub(crate) fn ask_to_trash(
        &mut self,
        dirs: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(crate) fn open_folder(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn start_renaming(
        &mut self,
        dir: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(crate) fn rename_song(
        &mut self,
        dir: &Path,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    /// What goes back to the start when another song opens.
    pub(crate) fn reset_for_song(&mut self) {
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
