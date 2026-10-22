//! The sampler edited: its track, zones added and sliced, settings set, marks dragged.

use super::*;

impl Workspace {
    /// The selected track, if it plays the built-in sampler.
    pub fn sampler_track(&self) -> Option<usize> {
        let t = self.session.project.tracks.get(self.track)?;
        matches!(t.instrument, Some(Instrument::Sampler(_))).then_some(self.track)
    }

    pub(crate) fn sampler_of(&self, t: usize) -> Option<&Sampler> {
        match &self.session.project.tracks.get(t)?.instrument {
            Some(Instrument::Sampler(s)) => Some(s),
            _ => None,
        }
    }

    /// An edit to a sampler's settings (its samples the same): undone like any edit, and
    /// followed by the playing sampler without a reload.
    pub(crate) fn edit_sampler(
        &mut self,
        t: usize,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Sampler),
    ) {
        self.edit(cx, |ws| {
            if let Some(Instrument::Sampler(s)) = &mut ws.session.project.tracks[t].instrument {
                f(s);
            }
        });
    }

    /// A sampler setting's knob position (for `Target::Sampler`).
    pub fn sampler_pos(&self, t: usize, f: Field) -> f32 {
        self.sampler_of(t).map_or(0.0, |s| field_pos(s, f))
    }

    pub fn sampler_set(&mut self, t: usize, f: Field, v: f32) {
        if let Some(Some(Instrument::Sampler(s))) = self
            .session
            .project
            .tracks
            .get_mut(t)
            .map(|t| &mut t.instrument)
        {
            field_set(s, f, v);
        }
    }

    /// A sample's waveform, read in the background the first time it's asked for.
    pub(crate) fn wave(&mut self, file: &Path, cx: &mut Context<Self>) -> Option<Arc<Wave>> {
        if let Some(w) = self.sampler_ui.waves.get(file) {
            return w.clone();
        }
        self.sampler_ui.waves.insert(file.to_path_buf(), None);
        let (key, full) = (file.to_path_buf(), self.session.dir.join(file));
        let job = cx.background_spawn(async move {
            let (sides, rate) = read_audio(&full).ok()?;
            let n = sides.first()?.len();
            let mono: Vec<f32> = (0..n)
                .map(|i| sides.iter().map(|s| s[i]).sum::<f32>() / sides.len() as f32)
                .collect();
            Some(Wave {
                peaks: Peaks::from_samples(&mono),
                len: n,
                seconds: n as f32 / rate as f32,
                onsets: onsets(&mono, rate as f32),
            })
        });
        cx.spawn(async move |this, cx| {
            let wave = job.await;
            this.update(cx, |ws, cx| {
                ws.sampler_ui.waves.insert(key, wave.map(Arc::new));
                cx.notify();
            })
            .ok();
        })
        .detach();
        None
    }

    /// Samples added to the selected sampler: a sound on each free note from C1 (a kit), or one
    /// played across the keys (with none yet, and a single file).
    pub fn add_samples(&mut self, files: &[PathBuf], cx: &mut Context<Self>) {
        let Some(t) = self.sampler_track() else {
            return;
        };
        let Some(mut sampler) = self.sampler_of(t).cloned() else {
            return;
        };
        let across = sampler.zones.is_empty() && files.len() == 1;
        for f in files {
            let name = f
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
            let zone = match across {
                true => Zone {
                    file: f.clone(),
                    name,
                    note: 0,
                    high: Some(127),
                    root: Some(60),
                    gate: true,
                    ..Zone::default()
                },
                false => {
                    let taken = |n: u8| sampler.zones.iter().any(|z| z.covers(n));
                    let Some(note) = (36..=127).find(|&n| !taken(n)) else {
                        self.status = Some("There's no free note left on this sampler.".into());
                        break;
                    };
                    Zone {
                        file: f.clone(),
                        name,
                        note,
                        ..Zone::default()
                    }
                }
            };
            sampler.zones.push(zone);
        }
        let count = sampler.zones.len();
        let dir = self.session.dir.clone();
        self.session.checkpoint();
        match commands::apply(
            &mut self.session.project,
            &dir,
            Command::SetInstrument {
                track: t,
                instrument: Instrument::Sampler(sampler),
            },
        ) {
            Ok(_) => {
                self.sampler_ui.zone = count.saturating_sub(1);
                self.changed(cx);
            }
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    pub(crate) fn pick_samples(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add to Sampler".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            this.update(cx, |ws, cx| ws.add_samples(&paths, cx)).ok();
        })
        .detach();
    }

    /// A new track with an empty sampler, selected, its editor open.
    pub fn new_sampler_track(&mut self, cx: &mut Context<Self>) {
        let n = self.session.project.tracks.len();
        let dir = self.session.dir.clone();
        self.session.checkpoint();
        let add = Command::AddTrack {
            name: format!("Sampler {}", n + 1),
            instrument: Some(Instrument::Sampler(Sampler::default())),
            index: None,
        };
        match commands::apply(&mut self.session.project, &dir, add) {
            Ok(_) => {
                self.select_track(self.session.project.tracks.len() - 1);
                self.sampler_ui.zone = 0;
                self.changed(cx);
            }
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    /// The picked zone's sample cut into slices across the keys, in place of the zones that
    /// play it.
    pub fn slice_zone(&mut self, how: Slicing, cx: &mut Context<Self>) {
        let Some(t) = self.sampler_track() else {
            return;
        };
        let Some(s) = self.sampler_of(t) else { return };
        let Some(z) = s.zones.get(self.sampler_ui.zone).cloned() else {
            return;
        };
        let Some(Some(wave)) = self.sampler_ui.waves.get(&z.file).cloned() else {
            self.status = Some("Still reading that sample: try again in a moment.".into());
            cx.notify();
            return;
        };
        let cut: Vec<f32> = match how {
            Slicing::Hits(most) => cuts(&wave.onsets, most),
            Slicing::Equal(n) => (0..n).map(|k| k as f32 / n as f32).collect(),
        };
        // From C1; or, slicing slices again, from where they were.
        let same: Vec<&Zone> = s.zones.iter().filter(|o| o.file == z.file).collect();
        let first = match same.iter().all(|o| o.high.is_none()) {
            true => same.iter().map(|o| o.note).min().unwrap_or(36),
            false => 36,
        };
        let file = z.file.clone();
        let count = cut.len();
        self.edit_sampler(t, cx, |s| {
            let at = s
                .zones
                .iter()
                .position(|o| o.file == file)
                .unwrap_or(s.zones.len());
            s.zones.retain(|o| o.file != file);
            let mut new = slices(&file, &cut, first);
            // Out of the way of the other zones: sounds already on those notes move up.
            for o in s.zones.iter_mut() {
                if o.note >= first && o.note < first + new.len() as u8 && o.high.is_none() {
                    o.note = o.note.saturating_add(new.len() as u8).min(127);
                }
            }
            let at = at.min(s.zones.len());
            for (k, zone) in new.drain(..).enumerate() {
                s.zones.insert(at + k, zone);
            }
        });
        self.status = Some(format!(
            "Sliced into {count} across the keys from {}",
            name_in(None, first)
        ));
        cx.notify();
    }

    /// A mark on the waveform dragged: the zone's start, end or loop points.
    pub(crate) fn drag_mark(&mut self, p: Point<f32>) {
        let Some(Drag::Mark {
            track,
            zone,
            mark,
            left,
            width,
        }) = self.drag
        else {
            return;
        };
        let at = ((p.x - left) / width.max(1.0)).clamp(0.0, 1.0);
        if let Some(Some(Instrument::Sampler(s))) = self
            .session
            .project
            .tracks
            .get_mut(track)
            .map(|t| &mut t.instrument)
            && let Some(z) = s.zones.get_mut(zone)
        {
            let end = z.end.unwrap_or(1.0);
            match mark {
                Mark::Start => z.start = at.min(end - 0.001),
                Mark::End => z.end = Some(at.max(z.start + 0.001)).filter(|&e| e < 0.9995),
                Mark::LoopStart => {
                    z.loop_start = Some(at.clamp(z.start, z.loop_end.unwrap_or(end) - 0.001))
                }
                Mark::LoopEnd => {
                    z.loop_end = Some(at.clamp(z.loop_start.unwrap_or(z.start) + 0.001, end))
                }
            }
        }
    }
}
