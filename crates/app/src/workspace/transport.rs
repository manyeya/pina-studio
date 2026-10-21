//! Playing: the playhead, play and stop, the cycle, the metronome, pattern mode, following the playhead, meters, and musical typing.

use super::*;

impl Workspace {
    pub fn playhead(&self) -> u64 {
        self.session.project.ticks(self.player.position())
    }

    /// Moves the playhead to the grid line nearest a window x position.
    pub fn locate_at(&mut self, x: f32, cx: &mut Context<Self>) {
        let project = &self.session.project;
        let tick = self.snap(self.tick_at(x));
        self.player
            .locate(project.samples(tick, self.player.rate()));
        cx.notify();
    }

    pub fn locate_tick(&mut self, tick: u64, cx: &mut Context<Self>) {
        let at = self.session.project.samples(tick, self.player.rate());
        self.player.locate(at);
        cx.notify();
    }

    pub fn play_stop(&mut self, cx: &mut Context<Self>) {
        if self.player.playing() || self.recording.is_some() {
            self.stop(cx);
            // Back where play started.
            if self.settings.return_on_stop {
                let from = self.play_from;
                self.locate_tick(from, cx);
            }
        } else {
            self.play_from = self.playhead();
            self.player.play();
        }
        cx.notify();
    }

    /// Stops playing, and recording if it was.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.player.stop();
        self.finish_recording(cx);
        cx.notify();
    }

    pub fn go_to_start(&mut self, cx: &mut Context<Self>) {
        self.player.locate(0);
        self.scroll_x = 0.0;
        cx.notify();
    }

    /// Turns the cycle on or off. With a region selected that the cycle isn't already round, it
    /// cycles that region; with no cycle yet and nothing selected, four bars from the playhead's
    /// bar.
    pub fn toggle_cycle(&mut self, cx: &mut Context<Self>) {
        let bar = self.session.project.bar();
        let selected = self.region.map(|at| {
            let r = self.session.project.region(at);
            (r.start, r.start + r.length)
        });
        let current = self.session.project.cycle;
        let (start, end) = match selected {
            Some(span) => span,
            None => {
                let start = self.playhead() / bar * bar;
                (start, start + bar * 4)
            }
        };
        let moves = selected.is_some_and(|s| current.is_none_or(|c| (c.start, c.end) != s));
        self.edit(cx, |ws| match &mut ws.session.project.cycle {
            Some(c) if !moves => c.off = !c.off,
            cycle => {
                *cycle = Some(Cycle {
                    start,
                    end,
                    off: false,
                });
                ws.pattern_mode = false;
            }
        });
    }

    pub fn toggle_metronome(&mut self, cx: &mut Context<Self>) {
        self.player.set_metronome(!self.player.metronome());
        cx.notify();
    }

    pub fn set_pattern_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        if on && self.region.is_none() {
            self.status = Some("Select a region to play it as a pattern.".into());
            cx.notify();
            return;
        }
        self.pattern_mode = on;
        if on && let Some(at) = self.region {
            let r = self.session.project.region(at);
            let start = r.start;
            self.pattern = Some((at, r.start, r.length));
            self.locate_tick(start, cx);
        }
        self.changed(cx);
    }

    /// In Pattern mode, selecting another region plays it straight away, from the same place in
    /// its loop as the last one was at (or its start, if it's longer than that).
    pub(crate) fn follow_pattern(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.region.filter(|_| self.pattern_mode) else {
            return;
        };
        if self.pattern.is_some_and(|(p, ..)| p == at) {
            return;
        }
        let tick = self.playhead();
        let into = self.pattern.map_or(0, |(_, start, length)| {
            tick.saturating_sub(start) % length.max(1)
        });
        let r = self.session.project.region(at);
        let (start, length) = (r.start, r.length.max(1));
        self.pattern = Some((at, start, length));
        self.changed(cx);
        self.locate_tick(start + into % length, cx);
    }

    pub(crate) fn toggle_typing(&mut self, cx: &mut Context<Self>) {
        self.typing = if self.typing.is_some() { None } else { Some(0) };
        cx.notify();
    }

    /// Musical typing: A to K are white keys from C, W E T Y U O P the black ones;
    /// Z and X shift the octave.
    pub(crate) fn type_key(&mut self, e: &KeyDownEvent, window: &Window, cx: &mut Context<Self>) {
        // Typing in a field (a search, a question for the agent) is typing, not notes.
        if self.browser.searching || self.typing_in_field(window, cx) {
            return;
        }
        if self.naming.is_none() && self.menu_key(e, cx) {
            cx.stop_propagation();
            return;
        }
        let Some(octave) = self.typing else { return };
        let key = e.keystroke.key.as_str();
        if e.keystroke.modifiers.modified() {
            return;
        }
        match key {
            "z" => self.typing = Some((octave - 1).max(-4)),
            "x" => self.typing = Some((octave + 1).min(4)),
            _ => {
                const KEYS: &str = "awsedftgyhujkolp;'";
                let Some(i) = KEYS.find(key).filter(|_| key.len() == 1) else {
                    return;
                };
                if !e.is_held {
                    let pitch = (60 + i as i32 + octave as i32 * 12).clamp(0, 127) as u8;
                    self.typed.insert(key.to_string(), pitch);
                    self.player.send_live(LiveNote {
                        pitch,
                        velocity: 100,
                    });
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn release_key(&mut self, e: &KeyUpEvent, _: &mut Context<Self>) {
        if let Some(pitch) = self.typed.remove(e.keystroke.key.as_str()) {
            self.player.send_live(LiveNote { pitch, velocity: 0 });
        }
    }

    /// Reads the meters and compressors, letting them fall back gently, and the CPU load.
    pub(crate) fn read_meters(&mut self) {
        let ids: Vec<u64> = self
            .session
            .project
            .tracks
            .iter()
            .map(|t| t.id)
            .chain(self.session.project.auxes.iter().map(|a| a.id))
            .collect();
        for id in ids.iter().map(|&id| Some(id)).chain([None]) {
            let peak = match id {
                Some(id) => self.player.meter(id),
                None => self.player.master_meter(),
            };
            self.levels.entry(id).or_default().update(peak);
            let gr = self.reductions.entry(id).or_default();
            *gr = self.player.reduction(id).max(*gr * 0.9);
        }
        self.read_mods();
        // A spike shows, then falls away by half every half second.
        let since = std::mem::replace(&mut self.load_read, Instant::now()).elapsed();
        self.load = (self.player.load()).max(self.load * 0.5f32.powf(since.as_secs_f32() / 0.5));
    }

    /// Keeps the playhead on screen while playing, a page at a time.
    pub(crate) fn follow_playhead(&mut self) {
        let width = f32::from(self.lanes_bounds.get().size.width);
        let x = self.playhead() as f32 * self.px_per_tick() - self.scroll_x;
        if width > 0.0 && (x < 0.0 || x > width - 20.0) {
            self.scroll_x = (x + self.scroll_x - 20.0).max(0.0);
        }
    }
}
