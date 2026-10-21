//! Controls moved by hand: what that does to their automation in each mode, playing or not.

use super::*;

/// A control taken over from its automation, and until when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Live {
    /// While a hand has it, or while it records.
    Held,
    /// Until playback starts: moved while stopped, in "until you play" mode.
    UntilPlay,
}

/// A control moved over its automation by Trim, or while stopped: the line as it was, where the
/// control was taken hold of, and how far it has been moved since, when.
#[derive(Clone, Debug)]
pub struct Touch {
    snapshot: Vec<AutoPoint>,
    base: f32,
    /// Where a Trim pass began, while playing; None moves the whole line.
    from: Option<u64>,
    by: Vec<(u64, f32)>,
    held: bool,
}

/// What moving an automated control does when the song is stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stopped {
    /// Moves its whole line up or down, keeping its shape.
    #[default]
    Trim,
    /// Sets the line at the playhead: a point there.
    Point,
    /// Changes it until playback starts, when the automation takes over again.
    UntilPlay,
}

impl Stopped {
    pub const ALL: [Stopped; 3] = [Stopped::Trim, Stopped::Point, Stopped::UntilPlay];

    pub fn name(self) -> &'static str {
        match self {
            Stopped::Trim => "Moves the line",
            Stopped::Point => "Sets a point",
            Stopped::UntilPlay => "Until play",
        }
    }
}

impl Workspace {
    /// The project the engine plays: controls a hand has taken over follow it, not their
    /// automation.
    pub fn without_live(&self, project: &mut Project) {
        for &(id, param) in self.taken.keys() {
            if let Some(t) = project.tracks.iter_mut().find(|t| t.id == id) {
                t.automation.points_mut(param).clear();
                t.automation.tidy();
            }
        }
    }

    /// A control on a track has been moved by hand to `value` (and set there): what that does
    /// to its automation, by the track's mode, playing or not.
    pub fn moved(&mut self, track: usize, param: Param, value: f32) {
        let Some(t) = self.session.project.tracks.get(track) else {
            return;
        };
        let (key, mode) = ((t.id, param), t.automation.mode);
        let has = !t.automation.points(param).is_empty();
        if mode == AutoMode::Off {
            return;
        }
        if self.player.playing() {
            match mode {
                AutoMode::Read if has => drop(self.taken.insert(key, Live::Held)),
                AutoMode::Read => {}
                AutoMode::Trim if has && !param.steps() => self.trim(track, param, value, true),
                _ => {
                    self.record_auto(track, param, value);
                    self.taken.insert(key, Live::Held);
                }
            }
            return;
        }
        if !has {
            return;
        }
        match self.settings.stopped {
            _ if param.steps() => self.point_at_playhead(track, param, value),
            Stopped::Trim => self.trim(track, param, value, false),
            Stopped::Point => self.point_at_playhead(track, param, value),
            Stopped::UntilPlay => drop(self.taken.insert(key, Live::UntilPlay)),
        }
    }

    /// Moved and let go at once: a value typed, a reset, a button, a swipe.
    pub fn moved_once(&mut self, track: usize, param: Param, value: f32) {
        self.moved(track, param, value);
        self.let_go(track, param);
    }

    /// Moves the line by how far the control has come since it was taken hold of: all of it
    /// when stopped; while playing (Trim), from where it was taken hold of, as long as it's held.
    pub(crate) fn trim(&mut self, track: usize, param: Param, value: f32, playing: bool) {
        let tick = self.playhead();
        let t = &self.session.project.tracks[track];
        let key = (t.id, param);
        let points = t.automation.points(param);
        let touch = self.touches.entry(key).or_insert_with(|| Touch {
            snapshot: points.to_vec(),
            base: value_at(points, tick as f64).unwrap_or(value),
            from: playing.then_some(tick),
            by: Vec::new(),
            held: true,
        });
        let by = apart(param, touch.base, value);
        let at = if playing { tick } else { 0 };
        match touch.by.last_mut() {
            Some(last) if last.0 >= at => last.1 = by,
            _ => touch.by.push((at, by)),
        }
        touch.held = true;
        self.retouch(track, param);
        self.written.insert(key);
    }

    /// A touched lane, drawn again from its snapshot and how far it's been moved.
    pub(crate) fn retouch(&mut self, track: usize, param: Param) {
        let key = (self.session.project.tracks[track].id, param);
        let Some(touch) = self.touches.get(&key) else {
            return;
        };
        let snap = &touch.snapshot;
        let last = touch.by.last().map_or(0.0, |b| b.1);
        let by_at = |tick: u64| -> f32 {
            let i = touch.by.partition_point(|b| b.0 <= tick);
            match (i.checked_sub(1).map(|i| touch.by[i]), touch.by.get(i)) {
                (Some(a), Some(b)) => {
                    let f = (tick - a.0) as f32 / (b.0 - a.0).max(1) as f32;
                    a.1 + (b.1 - a.1) * f
                }
                (Some(a), None) => a.1,
                (None, Some(b)) => b.1,
                (None, None) => 0.0,
            }
        };
        let points: Vec<AutoPoint> = match touch.from {
            None => snap
                .iter()
                .map(|p| AutoPoint {
                    value: shifted(param, p.value, last),
                    ..*p
                })
                .collect(),
            Some(from) => {
                let end = match touch.held {
                    true => u64::MAX,
                    false => touch.by.last().map_or(from, |b| b.0),
                };
                let mut ticks: Vec<u64> = snap
                    .iter()
                    .map(|p| p.tick)
                    .filter(|&t| t >= from && t <= end)
                    .chain(touch.by.iter().map(|b| b.0))
                    .chain([from])
                    .chain((end != u64::MAX).then_some(end))
                    .collect();
                ticks.sort_unstable();
                ticks.dedup();
                let mut out: Vec<AutoPoint> =
                    snap.iter().filter(|p| p.tick < from).copied().collect();
                // As it was up to where it was taken hold of.
                if from > 0
                    && let Some(v) = value_at(snap, (from - 1) as f64)
                {
                    out.push(AutoPoint::new(from - 1, v));
                }
                for (n, &t) in ticks.iter().enumerate() {
                    let own = snap.iter().position(|p| p.tick == t);
                    // A point of the line keeps its shape on to the next, if that's next here.
                    let shape = own
                        .filter(|&j| snap.get(j + 1).map(|p| p.tick) == ticks.get(n + 1).copied());
                    let v = value_at(snap, t as f64).unwrap_or(touch.base);
                    out.push(AutoPoint {
                        tick: t,
                        value: shifted(param, v, by_at(t)),
                        curve: shape.map_or(0.0, |j| snap[j].curve),
                        step: shape.is_some_and(|j| snap[j].step),
                        ease: shape.is_some_and(|j| snap[j].ease),
                    });
                }
                // Let go, it ramps back to the line as it was.
                let back = end.saturating_add(RAMP);
                if end != u64::MAX
                    && let Some(v) = value_at(snap, back as f64)
                {
                    out.push(AutoPoint::new(back, v));
                }
                out.extend(snap.iter().filter(|p| p.tick > back).copied());
                out
            }
        };
        let a = &mut self.session.project.tracks[track].automation;
        *a.points_mut(param) = points;
    }

    /// The line set at the playhead: the point there moved, or one put there.
    pub(crate) fn point_at_playhead(&mut self, track: usize, param: Param, value: f32) {
        let tick = self.playhead();
        let a = &mut self.session.project.tracks[track].automation;
        let points = a.points_mut(param);
        match points.iter().position(|p| p.tick == tick) {
            Some(i) => points[i].value = value,
            None => {
                a.add(
                    param,
                    AutoPoint {
                        step: param.steps(),
                        ..AutoPoint::new(tick, value)
                    },
                );
            }
        }
    }

    /// While the song plays, a control moved on a track whose mode records records its value
    /// at the playhead.
    pub fn record_auto(&mut self, track: usize, param: Param, value: f32) {
        if !self.player.playing() {
            return;
        }
        let name = matches!(param, Param::Plugin(..)).then(|| self.param_name(track, param));
        let tick = self.playhead();
        let Some(t) = self.session.project.tracks.get_mut(track) else {
            return;
        };
        if !t.automation.mode.writes() {
            return;
        }
        let key = (t.id, param);
        let after = self.writing.get(&key).map(|w| w.0);
        // The line as it was, to ramp back to when a Touch is let go.
        if after.is_none() {
            self.recorded
                .insert(key, t.automation.points(param).to_vec());
        }
        t.automation.write(param, after, tick, value);
        if let Some(name) = name {
            t.automation.name(param, &name);
        }
        self.writing.insert(key, (tick, value, true));
        self.written.insert(key);
        self.show_lane(track, param);
    }

    /// Lets go of a control: Read hands back to the automation; Touch and Trim stop recording;
    /// Latch and Write carry on with its last value until playback stops.
    pub fn let_go(&mut self, track: usize, param: Param) {
        let Some(t) = self.session.project.tracks.get(track) else {
            return;
        };
        let key = (t.id, param);
        let carries = matches!(t.automation.mode, AutoMode::Latch | AutoMode::Write);
        if let Some(touch) = self.touches.get_mut(&key) {
            if touch.from.is_some() {
                let tick = self.session.project.ticks(self.player.position());
                let last = touch.by.last().map_or(0.0, |b| b.1);
                if touch.by.last().is_none_or(|b| b.0 < tick) {
                    touch.by.push((tick, last));
                }
            }
            touch.held = false;
            self.retouch(track, param);
            self.touches.remove(&key);
        }
        match (carries, self.writing.get_mut(&key)) {
            (true, Some(w)) => w.2 = false,
            (_, _) => {
                // Touch let go: back to the line as it was, over a moment.
                if let (Some(w), Some(was)) =
                    (self.writing.remove(&key), self.recorded.remove(&key))
                {
                    let back = w.0 + RAMP;
                    if let Some(v) = value_at(&was, back as f64) {
                        let points = self.session.project.tracks[track]
                            .automation
                            .points_mut(param);
                        points.retain(|p| p.tick <= w.0 || p.tick > back);
                        let i = points.partition_point(|p| p.tick <= w.0);
                        points.insert(
                            i,
                            AutoPoint {
                                step: param.steps(),
                                ..AutoPoint::new(back, v)
                            },
                        );
                    }
                }
                if self.taken.get(&key) == Some(&Live::Held) {
                    self.taken.remove(&key);
                }
            }
        }
    }

    /// Lets go of every control held (a mouse let go of anywhere).
    pub fn let_go_all(&mut self) {
        let held: Vec<(u64, Param)> = self
            .touches
            .keys()
            .copied()
            .chain(
                self.taken
                    .iter()
                    .filter(|(k, l)| **l == Live::Held && !self.writing.contains_key(k))
                    .map(|(k, _)| *k),
            )
            .chain(self.writing.iter().filter(|(_, w)| w.2).map(|(k, _)| *k))
            .collect();
        for (id, param) in held {
            if let Some(t) = self.session.project.tracks.iter().position(|t| t.id == id) {
                self.let_go(t, param);
            }
        }
    }

    /// Each frame: playback starting or stopping begins or ends a pass; while it plays, held
    /// and latched controls keep writing as the playhead moves, and Trim keeps moving the line.
    pub fn automation_frame(&mut self, cx: &mut Context<Self>) {
        // A control moved without a drag (a click, a swipe, a key) has been let go.
        let held = !self.touches.is_empty()
            || self
                .taken
                .iter()
                .any(|(k, l)| *l == Live::Held && !self.writing.contains_key(k));
        if self.drag.is_none() && held {
            self.let_go_all();
            self.changed(cx);
        }
        let playing = self.player.playing();
        if playing != self.was_playing {
            self.was_playing = playing;
            match playing {
                true => self.pass_starts(cx),
                false => self.pass_ends(cx),
            }
        }
        if !playing || (self.writing.is_empty() && self.touches.is_empty()) {
            return;
        }
        let tick = self.playhead();
        let mut wrote = false;
        for (&(id, param), w) in self.writing.iter_mut() {
            let Some(t) = self.session.project.tracks.iter_mut().find(|t| t.id == id) else {
                continue;
            };
            if tick > w.0 {
                t.automation.write(param, Some(w.0), tick, w.1);
                w.0 = tick;
                wrote = true;
            }
        }
        let trims: Vec<(u64, Param)> = self
            .touches
            .iter()
            .filter(|(_, t)| t.held && t.from.is_some())
            .map(|(k, _)| *k)
            .collect();
        for (id, param) in trims {
            let Some(track) = self.session.project.tracks.iter().position(|t| t.id == id) else {
                continue;
            };
            if let Some(touch) = self.touches.get_mut(&(id, param)) {
                let last = touch.by.last().map_or(0.0, |b| b.1);
                if touch.by.last().is_none_or(|b| b.0 < tick) {
                    touch.by.push((tick, last));
                }
            }
            self.retouch(track, param);
            wrote = true;
        }
        if wrote {
            self.changed(cx);
        }
    }

    /// Playback starts: what was changed until play goes back to its automation, and Write
    /// starts writing every automated control on its tracks.
    pub(crate) fn pass_starts(&mut self, cx: &mut Context<Self>) {
        self.taken.retain(|_, l| *l != Live::UntilPlay);
        let tick = self.playhead();
        let writes: Vec<usize> = (0..self.session.project.tracks.len())
            .filter(|&t| self.session.project.tracks[t].automation.mode == AutoMode::Write)
            .collect();
        if !writes.is_empty() {
            self.session.checkpoint();
        }
        for track in writes {
            let t = &self.session.project.tracks[track];
            let id = t.id;
            for param in t.automation.params() {
                let points = self.session.project.tracks[track].automation.points(param);
                let value =
                    value_at(points, tick as f64).unwrap_or_else(|| self.own_value(track, param));
                self.set_own(track, param, value);
                self.writing.insert((id, param), (tick, value, false));
                self.taken.insert((id, param), Live::Held);
                self.written.insert((id, param));
            }
        }
        self.changed(cx);
    }

    /// Playback stops: recording does; what was recorded is thinned of points that change
    /// nothing, and Write turns to Latch after a pass.
    pub(crate) fn pass_ends(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<(u64, Param)> = self.touches.keys().copied().collect();
        for (id, param) in ids {
            if let Some(t) = self.session.project.tracks.iter().position(|t| t.id == id) {
                self.let_go(t, param);
            }
        }
        self.writing.clear();
        self.recorded.clear();
        self.taken.retain(|_, l| *l != Live::Held);
        let thin = self.settings.thin_recorded;
        let latch = self.settings.write_to_latch;
        for (id, param) in std::mem::take(&mut self.written) {
            if let Some(t) = self.session.project.tracks.iter_mut().find(|t| t.id == id) {
                if thin {
                    t.automation.thin(param, 0.004);
                }
                if latch && t.automation.mode == AutoMode::Write {
                    t.automation.mode = AutoMode::Latch;
                }
            }
        }
        self.changed(cx);
    }
}
