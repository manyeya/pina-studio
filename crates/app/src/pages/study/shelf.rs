//! The studies kept: picked, opened, studied from a file, forgotten, and made into a playbook or a mix reference.

use super::*;

impl Workspace {
    /// The Song Study page, on the latest study.
    pub fn open_study(&mut self, cx: &mut Context<Self>) {
        self.studies.list = cli::study::studied();
        self.studies.stale = cli::study::stale();
        self.study_next_stale(cx);
        self.studies.like.pictures = cli::reference::pictures();
        self.studies.books = playbook::load().0;
        if self.studies.open.as_ref().is_none_or(|f| {
            !self.studies.list.iter().any(|s| s.file == *f)
                && !self.studies.like.pictures.iter().any(|b| b.file == *f)
        }) {
            self.studies.open = self.studies.list.first().map(|s| s.file.clone());
            self.studies.pick = Pick::File;
        }
        if self.studies.search.is_none() {
            let input = cx.new(|cx| TextInput::search("Search studies", cx));
            self.studies.typed = Some(cx.observe(&input, |ws, input, cx| {
                let text = input.read(cx).text().trim().to_lowercase();
                if text != ws.studies.query {
                    ws.studies.query = text;
                    cx.notify();
                }
            }));
            self.studies.search = Some(input);
        }
        self.view = View::Study;
        // A song made like a reference opens on it, the song against it.
        self.show_like_reference(cx);
        cx.notify();
    }

    /// Songs studied by an older Pina Studio are studied again, one at a time, in the background.
    fn study_next_stale(&mut self, cx: &mut Context<Self>) {
        if !self.studies.busy.is_empty() {
            return;
        }
        let Some(path) = self.studies.stale.pop() else {
            return;
        };
        self.studies.busy.push(path.clone());
        let job = {
            let path = path.clone();
            cx.background_spawn(async move { cli::study::study(&path, !cfg!(test)) })
        };
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |ws, cx| {
                ws.studies.busy.retain(|p| *p != path);
                if let Ok(s) = result {
                    ws.studies.list.retain(|x| x.file != s.file);
                    ws.studies.list.push(s);
                    if ws.studies.open.is_none() {
                        ws.studies.open = ws.studies.list.first().map(|s| s.file.clone());
                    }
                }
                ws.study_next_stale(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
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
        if pina_engine::study::reference::image::is_picture(&path) {
            return self.study_picture(path, cx);
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
            cx.background_spawn(async move { cli::study::study(&path, !cfg!(test)) })
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
                        ws.studies.pick = Pick::File;
                        ws.studies.at = 0.0;
                    }
                    Err(e) => ws.status = Some(e),
                }
                ws.study_next_stale(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn show_study(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        if self.studies.open.as_ref() != Some(&file) {
            if self.studies.playing.is_some() {
                self.study_stop(cx);
            }
            self.studies.open = Some(file);
            self.studies.pick = Pick::File;
            self.studies.at = 0.0;
        }
        cx.notify();
    }

    pub(crate) fn forget_study(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        cli::study::forget(&file);
        self.studies.list.retain(|s| s.file != file);
        if self.studies.open.as_ref() == Some(&file) {
            self.studies.open = self.studies.list.first().map(|s| s.file.clone());
            self.studies.pick = Pick::File;
        }
        cx.notify();
    }

    pub(crate) fn opened_study(&self) -> Option<Study> {
        let f = self.studies.open.as_ref()?;
        self.studies.list.iter().find(|s| s.file == *f).cloned()
    }

    /// The study as a playbook of the person's, saved.
    pub(crate) fn save_study_playbook(&mut self, s: &Study) -> Result<Playbook, String> {
        let p = study::song_book(s);
        playbook::save(&p)?;
        Ok(p)
    }

    pub(crate) fn study_to_playbook(&mut self, cx: &mut Context<Self>) {
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
    pub(crate) fn study_as_reference(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.opened_study() else { return };
        let r = reference(&s);
        self.edit(cx, |ws| ws.session.project.reference = Some(r));
        self.status = Some(format!(
            "{} is this song's mix reference",
            file_name(&s.file)
        ));
    }

    /// The agent reads the song: its theme and message, its imagery, why it works.
    pub(crate) fn ask_about_study(&mut self, cx: &mut Context<Self>) {
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
}
