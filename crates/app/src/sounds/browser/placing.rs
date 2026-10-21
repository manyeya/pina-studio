//! Putting samples in the song: on new or existing tracks, dropped from the Finder, as sampler zones.

use super::*;

impl Workspace {
    /// Add to Song: a loop as audio on the selected audio track (or a new one) at the playhead;
    /// a one-shot as a zone on the selected sampler track (or a new one).
    pub fn add_sample(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(s) = self.browser.sample(index) else {
            return;
        };
        let track = self.session.project.tracks.get(self.track);
        match s.analysis.kind {
            Kind::Loop => {
                let audio = track.filter(|t| t.instrument.is_none()).map(|_| self.track);
                let tick = self.snap(self.playhead() as f64);
                self.add_audio(index, audio, tick, false, cx);
            }
            Kind::OneShot => {
                let sampler = track
                    .filter(|t| matches!(t.instrument, Some(Instrument::Sampler(_))))
                    .map(|_| self.track);
                self.add_zone(index, sampler, false, cx);
            }
        }
    }

    /// Where a new track goes: after the selected one, or (dropped under them) after the last.
    pub(crate) fn new_track_index(&self, at_end: bool) -> usize {
        let n = self.session.project.tracks.len();
        match at_end || n == 0 {
            true => n,
            false => (self.track + 1).min(n),
        }
    }

    /// A sample as an audio region at `tick` on `track` (a new audio track without one), stretched
    /// to the song's tempo if it's a loop that knows its own, and moved into the song's key if
    /// that's how it was heard.
    pub fn add_audio(
        &mut self,
        index: usize,
        track: Option<usize>,
        tick: u64,
        at_end: bool,
        cx: &mut Context<Self>,
    ) {
        let new = self.new_track_index(at_end);
        self.add_audio_at(index, track, Some(new), tick, cx);
    }

    /// As `add_audio`, a new track (without `track`) going at `new`.
    pub(crate) fn add_audio_at(
        &mut self,
        index: usize,
        track: Option<usize>,
        new: Option<usize>,
        tick: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.browser.sample(index).cloned() else {
            return;
        };
        let new = new.unwrap_or_else(|| self.new_track_index(true));
        // Heard in the song's key, it goes in in that key.
        let semis = self.sample_transpose(index).round() as i8;
        self.session.checkpoint();
        let placed = self.put_audio(&s.path, &s.name, Some(&s), semis, track, new, tick);
        self.placed(placed, &s.name, cx);
    }

    /// An audio file as a region at `tick` on `track` (a new audio track at `new` without one).
    /// A library sample (`s`) that's a loop knowing its tempo is stretched to the song's and
    /// loops on; `semis` moves it into the song's key.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn put_audio(
        &mut self,
        file: &Path,
        name: &str,
        s: Option<&Sample>,
        semis: i8,
        track: Option<usize>,
        new: usize,
        tick: u64,
    ) -> Result<At, String> {
        let dir = self.session.dir.clone();
        let p = &mut self.session.project;
        let t = match track {
            Some(t) => t,
            None => {
                let add = Command::AddTrack {
                    name: name.to_string(),
                    instrument: None,
                    index: Some(new),
                };
                commands::apply(p, &dir, add)?;
                new.min(p.tracks.len() - 1)
            }
        };
        let add = Command::AddAudioRegion {
            track: t,
            file: file.to_path_buf(),
            start: tick,
            offset: 0,
            length: None,
        };
        commands::apply(p, &dir, add)?;
        let at: At = (t, p.tracks[t].regions.len() - 1);
        let Some(s) = s else {
            return Ok(at);
        };
        if let Some(length) = loop_ticks(&s.analysis, p.tempo) {
            let now = p.region(at).length;
            if length.abs_diff(now) > now / 200 {
                p.flex(at, length)?;
            }
        }
        // Looping, as `pina samples put` leaves one, till the next region or the end; a
        // loop following the song's tempo.
        let r = &mut p.tracks[t].regions[at.1];
        r.transpose = semis;
        r.looped = s.analysis.kind == Kind::Loop;
        if let Content::Audio { follow, .. } = &mut r.content {
            *follow = s.analysis.kind == Kind::Loop;
        }
        Ok(at)
    }

    /// Audio files dropped from the Finder at `at` (a window position): each a region at the
    /// tick under the pointer, the first on the audio track there, the rest (or all, over any
    /// other track or under them) on new audio tracks. A file the library knows goes in as from
    /// the browser: a loop fitted to the song's tempo and key. One undo takes it all back.
    pub fn drop_files(&mut self, paths: &[PathBuf], at: Point<f32>, cx: &mut Context<Self>) {
        self.file_drag = None;
        let audio: Vec<&PathBuf> = paths
            .iter()
            .filter(|p| {
                p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    ["wav", "wave", "aif", "aiff", "flac", "mp3"]
                        .contains(&e.to_lowercase().as_str())
                })
            })
            .collect();
        if audio.is_empty() {
            self.status = Some("Pina Studio takes WAV, AIFF, FLAC and MP3 files.".into());
            cx.notify();
            return;
        }
        // On the Agent panel, sounds dropped on it are for the agent to use.
        let agent_left = self.viewport_w - self.panel_width();
        if self.view == View::Arrange && self.show_agent && !self.show_browser && at.x >= agent_left
        {
            let files: Vec<PathBuf> = audio.iter().map(|p| p.to_path_buf()).collect();
            self.give_agent_files(&files, cx);
            return;
        }
        // On the Sampler, with its editor open, samples dropped on it go into it.
        let editor_top = self.viewport_h - STATUS_H - self.editor_height();
        if self.view == View::Arrange
            && self.bottom == Some(Bottom::Sampler)
            && self.sampler_track().is_some()
            && at.y >= editor_top
        {
            let files: Vec<PathBuf> = audio.iter().map(|p| p.to_path_buf()).collect();
            self.add_samples(&files, cx);
            return;
        }
        // On the Song Study page, a song dropped is studied.
        if self.view == View::Study {
            for file in audio {
                self.study_file(file.clone(), cx);
            }
            return;
        }
        let n = self.session.project.tracks.len();
        let (mut track, tick, mut new) = match self.drop_target(at) {
            Some(Drop::Audio(t, tick)) => (Some(t), tick, t + 1),
            Some(Drop::Zone(t, tick) | Drop::Under(t, tick)) => (None, tick, t + 1),
            Some(Drop::New(tick)) => (None, tick, n),
            None => (None, 0, n),
        };
        self.session.checkpoint();
        let mut placed = Err("nothing dropped".to_string());
        for file in audio.iter().copied() {
            let known = self
                .browser
                .library
                .samples
                .iter()
                .position(|s| s.path == *file);
            let s = known.and_then(|k| self.browser.sample(k).cloned());
            let semis = known.map_or(0, |k| self.sample_transpose(k).round() as i8);
            let name = file
                .file_stem()
                .map_or("Audio".into(), |f| f.to_string_lossy().into_owned());
            placed = self.put_audio(file, &name, s.as_ref(), semis, track.take(), new, tick);
            match &placed {
                Ok((t, _)) => new = t + 1,
                Err(_) => break,
            }
        }
        let what = match audio.as_slice() {
            [one] => one
                .file_stem()
                .map_or(String::new(), |f| f.to_string_lossy().into_owned()),
            many => format!("{} files", many.len()),
        };
        self.placed(placed, &what, cx);
    }

    /// A one-shot as a zone on `track`'s sampler, or on a new sampler track without one.
    pub fn add_zone(
        &mut self,
        index: usize,
        track: Option<usize>,
        at_end: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.browser.sample(index).cloned() else {
            return;
        };
        let new = self.new_track_index(at_end);
        self.session.checkpoint();
        let dir = self.session.dir.clone();
        let p = &mut self.session.project;
        let placed = (|| match track {
            Some(t) => {
                let Some(Instrument::Sampler(mut sampler)) = p.tracks[t].instrument.clone() else {
                    return Err("That track isn't a sampler.".to_string());
                };
                place_zone(&mut sampler, &s, None)?;
                let set = Command::SetInstrument {
                    track: t,
                    instrument: Instrument::Sampler(sampler),
                };
                commands::apply(p, &dir, set)?;
                Ok((t, usize::MAX))
            }
            None => {
                let mut sampler = Sampler::default();
                place_zone(&mut sampler, &s, None)?;
                let add = Command::AddTrack {
                    name: s.name.clone(),
                    instrument: Some(Instrument::Sampler(sampler)),
                    index: Some(new),
                };
                commands::apply(p, &dir, add)?;
                Ok((new, usize::MAX))
            }
        })();
        self.placed(placed, &s.name, cx);
    }

    /// After adding a sample: selects where it went and says so, or undoes the attempt and says
    /// why.
    pub(crate) fn placed(
        &mut self,
        placed: Result<At, String>,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        match placed {
            Ok((t, r)) => {
                self.select_track(t);
                self.region = (r != usize::MAX).then_some((t, r));
                let track = &self.session.project.tracks[t].name;
                self.status = Some(format!("Added {name} to {track}"));
                self.changed(cx);
                // `changed` keeps whatever the engine had to say.
                if self.status.is_none() {
                    let track = &self.session.project.tracks[t].name;
                    self.status = Some(format!("Added {name} to {track}"));
                }
            }
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    /// Where a sample dropped at `p` (a window position) would go, if it's over the tracks.
    pub fn drop_target(&self, p: Point<f32>) -> Option<Drop> {
        let b = self.lanes_bounds.get();
        let (left, top) = (f32::from(b.origin.x), f32::from(b.origin.y));
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if p.x < left || p.y < top || p.x > left + w || p.y > top + h {
            return None;
        }
        let row = self.track_at_y(p.y - top).unwrap_or(usize::MAX);
        let tick = self.snap(self.tick_at(p.x));
        Some(match self.session.project.tracks.get(row) {
            None => Drop::New(tick),
            Some(t) => match &t.instrument {
                None => Drop::Audio(row, tick),
                Some(Instrument::Sampler(_)) => Drop::Zone(row, tick),
                Some(_) => Drop::Under(row, tick),
            },
        })
    }

    /// A sample or a map box being dragged.
    pub fn drag_sample(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        let mut out = false;
        match &mut self.drag {
            Some(Drag::Sample {
                from, at, moved, ..
            }) => {
                *at = p;
                let far = (p.x - from.x).hypot(p.y - from.y);
                *moved |= far > 4.0;
                out = far > 40.0;
            }
            Some(Drag::MapBox { to, .. }) => *to = p,
            _ => {}
        }
        // Dragged out of the page: the tracks come back to drop it on, the compact browser
        // beside them.
        if out && self.view == View::Samples {
            self.view = View::Arrange;
            self.show_browser = true;
        }
        cx.notify();
    }

    /// Lets go of a sample over the tracks, or of a box on the map.
    pub fn drop_sample(&mut self, drag: Option<Drag>, cx: &mut Context<Self>) {
        match drag {
            Some(Drag::Sample {
                index,
                at,
                moved: true,
                ..
            }) => {
                // On the timeline, a sample is audio there; on a sampler track, a pad.
                match self.drop_target(at) {
                    Some(Drop::Audio(t, tick)) => self.add_audio(index, Some(t), tick, false, cx),
                    Some(Drop::Zone(t, _)) => self.add_zone(index, Some(t), false, cx),
                    Some(Drop::New(tick)) => self.add_audio_at(
                        index,
                        None,
                        Some(self.session.project.tracks.len()),
                        tick,
                        cx,
                    ),
                    Some(Drop::Under(t, tick)) => {
                        self.add_audio_at(index, None, Some(t + 1), tick, cx)
                    }
                    None => {}
                }
            }
            Some(Drag::MapBox { from, to }) => self.box_area(from, to),
            _ => {}
        }
        cx.notify();
    }
}
