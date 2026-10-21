//! The studies kept: picked, opened, studied from a file, forgotten, and made into a playbook or a mix reference.

use super::*;

impl Workspace {
    /// The Song Study page, on the latest study.
    pub fn open_study(&mut self, cx: &mut Context<Self>) {
        self.studies.list = cli::study::studied();
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

    pub(crate) fn show_study(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        if self.studies.open.as_ref() != Some(&file) {
            self.studies.open = Some(file);
            self.studies.section = None;
            self.studies.scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }

    pub(crate) fn forget_study(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        cli::study::forget(&file);
        self.studies.list.retain(|s| s.file != file);
        if self.studies.open.as_ref() == Some(&file) {
            self.studies.open = self.studies.list.first().map(|s| s.file.clone());
            self.studies.section = None;
        }
        cx.notify();
    }

    pub(crate) fn opened_study(&self) -> Option<Study> {
        let f = self.studies.open.as_ref()?;
        self.studies.list.iter().find(|s| s.file == *f).cloned()
    }

    /// The study as a playbook of the person's, saved.
    pub(crate) fn save_study_playbook(&mut self, s: &Study) -> Result<Playbook, String> {
        let taken: Vec<String> = playbook::load().0.into_iter().map(|p| p.id).collect();
        let p = study::to_playbook(s, &taken);
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

    /// A new song like the study's: its tempo, key and meter, its playbook as the genre, its mix
    /// as the reference. Opened, for the agent to write with.
    pub(crate) fn song_like_study(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            let dir = crate::pages::projects::create(
                &crate::pages::projects::root(),
                &title,
                (s.tempo.bpm as f64).round().clamp(20.0, 999.0),
                Key::parse(&s.key.name),
                [beats as u64, unit as u64],
                crate::pages::projects::Template::Empty,
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
