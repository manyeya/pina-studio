//! What the Bounce sheet does: its presets and choices, the jobs a bounce is made of, running them; bouncing in place and stems.

use super::*;

impl Workspace {
    /// Opens the Bounce sheet, set as it was last (or for the mix, the whole song, WAV 24-bit).
    pub fn open_export(&mut self, cx: &mut Context<Self>) {
        if self
            .export
            .as_ref()
            .is_some_and(|e| e.run.as_ref().is_some_and(|r| !r.finished))
        {
            return;
        }
        let p = &self.session.project;
        let song = self
            .session
            .dir
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());
        let name = cx.new(|cx| {
            let mut t = TextInput::field("", cx);
            t.set_text(&song, cx);
            t
        });
        let sub = cx.subscribe(&name, |ws, _, e: &TextEvent, cx| {
            if let TextEvent::Confirm(_) = e {
                ws.start_export(cx);
            }
        });
        let with = bounce::tracks_with_regions(p);
        let bars = p.end().div_ceil(p.bar().max(1)).max(1);
        self.export = Some(Export {
            what: What::Mix,
            span: Span::Song,
            bars: (1, bars),
            tracks: (0..p.tracks.len()).map(|i| with.contains(&i)).collect(),
            sections: vec![true; bounce::sections(p).len()],
            formats: vec![Format::Wav],
            bits: Bits::I24,
            rate: p.sample_rate,
            level: Level::Off,
            tail: true,
            dither: true,
            master_on_stems: false,
            folder: self.session.dir.join("Bounces"),
            name,
            run: None,
            _subs: vec![sub],
        });
        cx.notify();
    }

    pub(crate) fn close_export(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.export.as_ref().and_then(|e| e.run.as_ref()) {
            r.cancel.store(true, Ordering::Relaxed);
        }
        self.export = None;
        self.refocus = true;
        cx.notify();
    }

    pub(crate) fn preset(&mut self, preset: Preset, cx: &mut Context<Self>) {
        let rate = self.session.project.sample_rate;
        let Some(e) = self.export.as_mut() else {
            return;
        };
        e.what = if preset == Preset::Stems {
            What::Stems
        } else {
            What::Mix
        };
        e.tail = true;
        e.master_on_stems = false;
        (e.formats, e.bits, e.rate, e.level, e.dither) = match preset {
            Preset::Master => (
                vec![Format::Wav],
                Bits::I24,
                rate.max(44_100),
                Level::Off,
                true,
            ),
            Preset::Streaming => (
                vec![Format::Wav, Format::Mp3 { kbps: 320 }],
                Bits::I24,
                48_000,
                Level::Loudness {
                    lufs: -14.0,
                    ceiling: -1.0,
                },
                true,
            ),
            Preset::Cd => (
                vec![Format::Wav],
                Bits::I16,
                44_100,
                Level::Peak { db: -0.3 },
                true,
            ),
            Preset::Stems => (
                vec![Format::Wav],
                Bits::I24,
                rate.max(44_100),
                Level::Off,
                true,
            ),
            Preset::Demo => (
                vec![Format::Mp3 { kbps: 192 }],
                Bits::I24,
                44_100,
                Level::Loudness {
                    lufs: -14.0,
                    ceiling: -1.0,
                },
                true,
            ),
            Preset::Midi => (vec![Format::Midi], e.bits, e.rate, e.level, e.dither),
        };
        cx.notify();
    }

    pub(crate) fn toggle_format(&mut self, f: Format, cx: &mut Context<Self>) {
        let Some(e) = self.export.as_mut() else {
            return;
        };
        match e
            .formats
            .iter()
            .position(|o| std::mem::discriminant(o) == std::mem::discriminant(&f))
        {
            Some(i) if e.formats.len() > 1 => drop(e.formats.remove(i)),
            Some(_) => {}
            None => e.formats.push(f),
        }
        cx.notify();
    }

    /// The range bounced, in ticks; None for the whole song.
    pub(crate) fn export_range(&self) -> Option<(u64, u64)> {
        let e = self.export.as_ref()?;
        let p = &self.session.project;
        match e.span {
            Span::Song => None,
            Span::Cycle => p.cycle.map(|c| (c.start, c.end)),
            Span::Bars => {
                let bar = p.bar();
                Some(((e.bars.0.max(1) - 1) * bar, e.bars.1.max(e.bars.0) * bar))
            }
        }
    }

    /// What the sheet would bounce: a job a file name.
    pub(crate) fn export_jobs(&self, cx: &gpui::App) -> Vec<Job> {
        let Some(e) = self.export.as_ref() else {
            return Vec::new();
        };
        let p = &self.session.project;
        let base = bounce::file_safe(e.name.read(cx).text());
        let base = if base.is_empty() {
            "Bounce".to_string()
        } else {
            base
        };
        let range = self.export_range();
        match e.what {
            What::Mix => vec![Job {
                name: base,
                track: None,
                range,
                master: true,
            }],
            What::Stems => p
                .tracks
                .iter()
                .enumerate()
                .filter(|(i, _)| e.tracks.get(*i).copied().unwrap_or(false))
                .map(|(i, t)| Job {
                    name: format!("{base} - {}", bounce::file_safe(&t.name)),
                    track: Some(i),
                    range,
                    master: e.master_on_stems,
                })
                .collect(),
            What::Sections => bounce::sections(p)
                .into_iter()
                .enumerate()
                .filter(|(k, _)| e.sections.get(*k).copied().unwrap_or(false))
                .map(|(k, (name, from, to))| Job {
                    name: format!("{base} - {:02} {}", k + 1, bounce::file_safe(&name)),
                    track: None,
                    range: Some((from, to)),
                    master: true,
                })
                .collect(),
        }
    }

    pub(crate) fn export_options(&self) -> Option<Options> {
        let e = self.export.as_ref()?;
        Some(Options {
            formats: e.formats.clone(),
            bits: e.bits,
            rate: e.rate,
            level: e.level,
            tail: e.tail,
            dither: e.dither,
        })
    }

    /// Bounces what the sheet says, in the background, a job at a time.
    pub(crate) fn start_export(&mut self, cx: &mut Context<Self>) {
        let jobs = self.export_jobs(cx);
        let Some(opts) = self.export_options() else {
            return;
        };
        if jobs.is_empty() {
            self.status = Some("Nothing to bounce: tick a track or a section.".into());
            cx.notify();
            return;
        }
        let mut project = self.session.project.clone();
        if let Err(e) = self.player.save_states(&mut project) {
            self.status = Some(e);
            cx.notify();
            return;
        }
        let Some(e) = self.export.as_mut() else {
            return;
        };
        let (dir, folder) = (self.session.dir.clone(), e.folder.clone());
        let progress = Arc::new(Mutex::new(vec![0f32; jobs.len()]));
        let results = Arc::new(Mutex::new(
            (0..jobs.len()).map(|_| None).collect::<Vec<_>>(),
        ));
        let cancel = Arc::new(AtomicBool::new(false));
        e.run = Some(Run {
            names: jobs.iter().map(|j| j.name.clone()).collect(),
            progress: progress.clone(),
            results: results.clone(),
            cancel: cancel.clone(),
            finished: false,
        });
        self.exporting = true;
        cx.notify();
        let work = cx.background_spawn(async move {
            for (k, job) in jobs.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let p = progress.clone();
                let done = bounce::bounce(&project, &dir, job, &opts, &folder, &move |f| {
                    if let Ok(mut v) = p.lock() {
                        v[k] = f;
                    }
                });
                if let Ok(mut r) = results.lock() {
                    r[k] = Some(done);
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let mut work = Some(work);
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let finished = work.as_ref().is_some_and(|w| w.is_ready());
                let more = this.update(cx, |ws, cx| {
                    if finished && let Some(r) = ws.export.as_mut().and_then(|e| e.run.as_mut()) {
                        r.finished = true;
                    }
                    if finished {
                        ws.exporting = false;
                        let made = ws
                            .export
                            .as_ref()
                            .and_then(|e| e.run.as_ref())
                            .map_or(0, |r| {
                                r.results
                                    .lock()
                                    .map_or(0, |v| v.iter().flatten().filter(|d| d.is_ok()).count())
                            });
                        ws.status = Some(format!("Bounced {made} of them"));
                    }
                    cx.notify();
                });
                if more.is_err() || finished {
                    if let Some(w) = work.take() {
                        w.await;
                    }
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn pick_export_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Bounce Here".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dirs))) = picked.await else {
                return;
            };
            this.update(cx, |ws, cx| {
                if let (Some(e), Some(d)) = (ws.export.as_mut(), dirs.into_iter().next()) {
                    e.folder = d;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A track rendered to audio (through its own plugins, sends and fader, not the master's)
    /// on a new audio track under it, from its first region to its last's end and its tail; the
    /// track muted. One undo puts it back.
    pub fn bounce_in_place(&mut self, t: usize, cx: &mut Context<Self>) {
        self.bounce_range_in_place(t, None, cx);
    }

    /// The same over a range of ticks (a region's), or the whole track's regions.
    pub fn bounce_range_in_place(
        &mut self,
        t: usize,
        range: Option<(u64, u64)>,
        cx: &mut Context<Self>,
    ) {
        let p = &self.session.project;
        let Some(track) = p.tracks.get(t) else { return };
        let whole = (
            track.regions.iter().map(|r| r.start).min(),
            track.regions.iter().map(|r| r.start + r.length).max(),
        );
        let (Some(from), Some(to)) = range.map_or(whole, |(a, b)| (Some(a), Some(b))) else {
            self.status = Some(format!("{} has nothing to bounce.", track.name));
            cx.notify();
            return;
        };
        let mut project = p.clone();
        if let Err(e) = self.player.save_states(&mut project) {
            self.status = Some(e);
            cx.notify();
            return;
        }
        let name = format!("{} (bounced)", track.name);
        let job = Job {
            name: bounce::file_safe(&name),
            track: Some(t),
            range: Some((from, to)),
            master: false,
        };
        let (dir, rate) = (self.session.dir.clone(), p.sample_rate);
        self.status = Some(format!("Bouncing {} in place…", track.name));
        cx.notify();
        let work = cx.background_spawn(async move {
            let [l, r] = bounce::render(&project, &dir, &job, rate, true, &|_| {})?;
            let file = std::env::temp_dir().join(format!("{}.wav", job.name));
            pina_engine::music::analysis::write_wav(&file, &l, &r, rate)?;
            Ok::<_, String>(file)
        });
        cx.spawn(async move |this, cx| {
            let made = work.await;
            this.update(cx, |ws, cx| match made {
                Ok(file) => {
                    let dir = ws.session.dir.clone();
                    ws.session.checkpoint();
                    let at = t + 1;
                    let p = &mut ws.session.project;
                    let color = p.tracks[t].color.clone();
                    let added = commands::apply(
                        p,
                        &dir,
                        Command::AddTrack {
                            name: name.clone(),
                            instrument: None,
                            index: Some(at),
                        },
                    )
                    .and_then(|_| {
                        commands::apply(
                            p,
                            &dir,
                            Command::AddAudioRegion {
                                track: at,
                                file: file.clone(),
                                start: from,
                                offset: 0,
                                length: None,
                            },
                        )
                    });
                    let _ = std::fs::remove_file(&file);
                    match added {
                        Ok(_) => {
                            p.tracks[at].color = color;
                            p.tracks[t].mute = true;
                            ws.select_track(at);
                            ws.changed(cx);
                            ws.status =
                                Some(format!("{name}: the original muted (⌘Z puts it back)"));
                        }
                        Err(e) => {
                            ws.session.cancel();
                            ws.status = Some(e);
                            cx.notify();
                        }
                    }
                }
                Err(e) => {
                    ws.status = Some(format!("Couldn't bounce it: {e}"));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Opens the sheet on stems, with one track ticked.
    pub fn export_stem(&mut self, t: usize, cx: &mut Context<Self>) {
        self.open_export(cx);
        self.preset(Preset::Stems, cx);
        if let Some(e) = self.export.as_mut() {
            e.tracks = (0..e.tracks.len()).map(|i| i == t).collect();
        }
        cx.notify();
    }
}
