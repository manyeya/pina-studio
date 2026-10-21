//! What the roll shows and where: its region, its pitches and ticks, focus and scroll.

use super::*;

impl Workspace {
    /// A pitch's name in the song's key.
    pub fn note_name(&self, pitch: u8) -> String {
        name_in(self.session.project.song_key(), pitch)
    }

    /// Where a tick falls in the song: bar, beat, sixteenth.
    pub(crate) fn position(&self, tick: u64) -> String {
        let (bar, beat) = (
            self.session.project.bar().max(1),
            self.session.project.beat().max(1),
        );
        let sixteenth = (PPQ / 4).max(1);
        format!(
            "{}.{}.{}",
            tick / bar + 1,
            tick % bar / beat + 1,
            tick % beat / sixteenth + 1
        )
    }

    /// The MIDI region the piano roll shows: the one selected, else the selected track's under
    /// the playhead, else its first.
    pub fn roll_region(&self) -> Option<At> {
        let p = &self.session.project;
        let midi = |at: At| matches!(p.region(at).content, Content::Midi { .. });
        self.region.filter(|&at| midi(at)).or_else(|| {
            let track = p.tracks.get(self.track)?;
            let tick = self.playhead();
            let mine: Vec<At> = (0..track.regions.len())
                .map(|r| (self.track, r))
                .filter(|&at| midi(at))
                .collect();
            mine.iter()
                .copied()
                .find(|&at| {
                    let r = p.region(at);
                    r.start <= tick && tick < r.start + r.length
                })
                .or(mine.first().copied())
        })
    }

    /// A MIDI region on `track` that `tick` is inside.
    pub fn midi_region_at(&self, track: usize, tick: u64) -> Option<At> {
        let t = self.session.project.tracks.get(track)?;
        t.regions
            .iter()
            .position(|r| {
                r.start <= tick
                    && tick < r.start + r.length
                    && matches!(r.content, Content::Midi { .. })
            })
            .map(|r| (track, r))
    }

    /// Grows a MIDI region to the end of the bar its last note starts in, when a note starts
    /// past its end, as far as the next region on its track allows.
    pub fn grow_to_notes(&mut self, at: At) {
        let p = &mut self.session.project;
        let bar = p.bar();
        let r = p.region(at);
        let Content::Midi { notes, .. } = &r.content else {
            return;
        };
        let last = notes.iter().map(|n| n.start).max().unwrap_or(0);
        if last < r.length as i64 {
            return;
        }
        let next = p.tracks[at.0]
            .regions
            .iter()
            .map(|o| o.start)
            .filter(|&s| s > r.start)
            .min();
        let want = (last as u64 / bar + 1) * bar;
        let room = next.map_or(u64::MAX, |n| n - r.start);
        let length = want.min(room).max(r.length);
        p.tracks[at.0].regions[at.1].length = length;
    }

    /// Scrolls the piano roll to the selected region's notes, bringing the region into view if
    /// it's off the side.
    pub fn focus_roll(&mut self) {
        if let Some(at) = self.roll_region() {
            let r = self.session.project.region(at);
            let (x0, x1) = (
                r.start as f32 * self.px_per_tick(),
                (r.start + r.length) as f32 * self.px_per_tick(),
            );
            let width = f32::from(self.lanes_bounds.get().size.width);
            if x0 < self.scroll_x || (width > 0.0 && x1 > self.scroll_x + width) {
                self.scroll_x = (x0 - 40.0).max(0.0);
            }
        }
        if let Some(at) = self.roll_region()
            && let Some(notes) = self.session.project.notes_mut(at)
            && let Some(top) = notes.iter().map(|n| n.pitch).max()
        {
            let kh = self.roll.key_h;
            self.roll_scroll_y =
                ((127 - top.saturating_add(4).min(127) as i32) as f32 * kh - kh * 2.0).max(0.0);
        }
    }

    /// Fits the shown notes' pitches to the piano roll's height.
    pub(crate) fn fit_roll(&mut self) {
        let Some(at) = self.roll_region() else { return };
        let Some(notes) = self.session.project.notes_mut(at) else {
            return;
        };
        let (Some(lo), Some(hi)) = (
            notes.iter().map(|n| n.pitch).min(),
            notes.iter().map(|n| n.pitch).max(),
        ) else {
            return;
        };
        let height = f32::from(self.roll_bounds.get().size.height).max(120.0);
        let rows = (hi - lo) as f32 + 7.0;
        self.roll.key_h = (height / rows).clamp(6.0, 26.0);
        self.roll_scroll_y =
            ((127 - hi.saturating_add(3).min(127)) as f32 * self.roll.key_h).max(0.0);
        self.focus_roll_x();
    }

    pub(crate) fn focus_roll_x(&mut self) {
        if let Some(at) = self.roll_region() {
            let r = self.session.project.region(at);
            self.scroll_x = (r.start as f32 * self.px_per_tick() - 40.0).max(0.0);
        }
    }

    /// The window y of a pitch's row in the piano roll.
    #[cfg(test)]
    pub fn pitch_y(&self, pitch: u8) -> f32 {
        let top = f32::from(self.roll_bounds.get().origin.y);
        top + (127 - pitch) as f32 * self.roll.key_h - self.roll_scroll_y
    }

    pub(crate) fn pitch_at(&self, y: f32) -> u8 {
        let top = f32::from(self.roll_bounds.get().origin.y);
        let row = ((y - top + self.roll_scroll_y) / self.roll.key_h).floor();
        (127.0 - row).clamp(0.0, 127.0) as u8
    }

    /// The pitch a note goes to: itself, or with "In key" on, the scale's nearest (the way it's
    /// moving, if it is).
    pub(crate) fn in_key(&self, pitch: u8, toward: i32) -> u8 {
        let Some(key) = self.session.project.song_key().filter(|_| self.roll.in_key) else {
            return pitch;
        };
        if key.contains(pitch) {
            return pitch;
        }
        let step = |d: i32| (pitch as i32 + d).clamp(0, 127) as u8;
        match toward.signum() {
            1 => step(1),
            -1 => step(-1),
            _ if key.contains(step(-1)) => step(-1),
            _ => step(1),
        }
    }

    /// The length a new note is written at: the last picked or stretched, else the grid.
    pub(crate) fn new_length(&self) -> u64 {
        match self.roll.length {
            0 => self.grid(),
            l => l,
        }
    }

    pub(crate) fn roll_scroll(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => p.map(f32::from),
            ScrollDelta::Lines(l) => l.map(|v| v * 24.0),
        };
        let b = self.roll_bounds.get();
        if e.modifiers.platform {
            let x = f32::from(e.position.x) - f32::from(b.origin.x);
            self.zoom_around(self.px_per_beat * (1.0 + d.y * 0.01), x);
        } else if e.modifiers.alt {
            // Taller or shorter rows, about the pitch under the mouse.
            let y = f32::from(e.position.y) - f32::from(b.origin.y);
            let row = (y + self.roll_scroll_y) / self.roll.key_h;
            self.roll.key_h = (self.roll.key_h * (1.0 + d.y * 0.01)).clamp(6.0, 28.0);
            self.roll_scroll_y = (row * self.roll.key_h - y).max(0.0);
        } else {
            let height = f32::from(b.size.height);
            let most = (128.0 * self.roll.key_h - height).max(0.0);
            self.roll_scroll_y = (self.roll_scroll_y - d.y).clamp(0.0, most);
            self.scroll_x = (self.scroll_x - d.x).max(0.0);
        }
        cx.notify();
    }

    /// The region a note at `tick` on the shown track goes in: one it's inside, or the shown
    /// one (grown to take it); else, with none there, a new region round it: drawing is all it
    /// takes to start writing. None if it can't (an audio track, or a region in the way).
    pub(crate) fn region_for(&mut self, tick: u64, cx: &mut Context<Self>) -> Option<Target> {
        let shown = self.roll_region();
        let track = shown.map_or(self.track, |at| at.0);
        let t = self.session.project.tracks.get(track)?;
        if t.instrument.is_none() {
            self.status = Some(format!(
                "{} plays audio: notes go on an instrument track (⌥⌘N, or an instrument from the Browser).",
                t.name
            ));
            cx.notify();
            return None;
        }
        let project = &self.session.project;
        let grows = shown.filter(|&at| {
            let r = project.region(at);
            let next = project.tracks[at.0]
                .regions
                .iter()
                .map(|o| o.start)
                .filter(|&s| s > r.start)
                .min()
                .unwrap_or(u64::MAX);
            tick >= r.start && tick < next
        });
        let target = self.midi_region_at(track, tick).or(grows);
        let span = match target {
            Some(_) => None,
            None => match self.new_region_span(track, tick) {
                Some(span) => Some(span),
                None => {
                    self.status = Some("A region's there already: click inside it.".into());
                    cx.notify();
                    return None;
                }
            },
        };
        Some((track, target, span))
    }

    pub(crate) fn hover_roll(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        let pitch = Some(self.pitch_at(f32::from(e.position.y)));
        if self.roll_hover != pitch {
            self.roll_hover = pitch;
            cx.notify();
        }
    }
}
