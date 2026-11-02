//! Where things are: the side panel's width, rows and lanes, ticks and pixels, the grid, zoom and scrolling.

use super::*;

impl Workspace {
    /// The inspector's width.
    pub fn side_w(&self) -> f32 {
        self.inspector_w
    }

    pub fn px_per_tick(&self) -> f32 {
        self.px_per_beat / PPQ as f32
    }

    /// The tick under a window x position, in the lanes.
    pub fn tick_at(&self, x: f32) -> f64 {
        let left = f32::from(self.lanes_bounds.get().origin.x);
        ((x - left + self.scroll_x) / self.px_per_tick()).max(0.0) as f64
    }

    /// A track's row: its stack's row above it if it's the stack's first track, then its lane
    /// and its automation lanes (none of them while the stack's folded).
    pub fn row_h_of(&self, i: usize) -> f32 {
        let above = self.stack_above(i).map_or(0.0, |a| self.stack_h(a));
        match self.folded(i) {
            true => above,
            false => above + self.lane_h(i) + self.autos_h(i),
        }
    }

    /// The stack (by its place among the auxes) whose row is above track `i`: the first track
    /// in it.
    pub fn stack_above(&self, i: usize) -> Option<usize> {
        let p = &self.session.project;
        let bus = p.tracks.get(i)?.output?;
        let a = p.auxes.iter().position(|a| a.bus == bus && a.stack)?;
        p.tracks[..i]
            .iter()
            .all(|t| t.output != Some(bus))
            .then_some(a)
    }

    /// Whether track `i` is folded away in its stack.
    pub fn folded(&self, i: usize) -> bool {
        self.session.project.stack_of(i).is_some_and(|a| a.folded)
    }

    /// Track `i`'s lane height, as dragged (the usual, past the last track).
    pub fn lane_h(&self, i: usize) -> f32 {
        self.session
            .project
            .tracks
            .get(i)
            .and_then(|t| t.height)
            .unwrap_or(self.settings.track_height)
            .clamp(MIN_LANE_H, MAX_LANE_H)
    }

    /// A stack's row's height, as dragged (the usual, if it hasn't been).
    pub fn stack_h(&self, a: usize) -> f32 {
        let auxes = &self.session.project.auxes;
        let dragged = auxes.get(a).and_then(|x| x.height);
        dragged.unwrap_or(STACK_H).clamp(MIN_STACK_H, MAX_LANE_H)
    }

    /// Where track `i`'s row begins, from the top of the lanes, scrolled.
    pub fn row_top(&self, i: usize) -> f32 {
        let above = self.stack_above(i).map_or(0.0, |a| self.stack_h(a));
        (0..i).map(|j| self.row_h_of(j)).sum::<f32>() + above - self.scroll_y
    }

    /// Every row, top to bottom.
    pub fn rows_h(&self) -> f32 {
        (0..self.session.project.tracks.len())
            .map(|i| self.row_h_of(i))
            .sum()
    }

    /// The track whose row is at `y` (from the top of the lanes, as drawn), if one is.
    pub fn track_at_y(&self, y: f32) -> Option<usize> {
        let mut top = -self.scroll_y;
        for i in 0..self.session.project.tracks.len() {
            top += self.stack_above(i).map_or(0.0, |a| self.stack_h(a));
            if self.folded(i) {
                continue;
            }
            let h = self.lane_h(i) + self.autos_h(i);
            if y < top + h {
                return (y >= top).then_some(i);
            }
            top += h;
        }
        None
    }

    /// The editing grid, in ticks.
    pub fn grid(&self) -> u64 {
        match self.snap {
            Snap::Smart => match self.px_per_beat {
                z if z >= 96.0 => PPQ / 4,
                z if z >= 20.0 => self.session.project.beat(),
                _ => self.session.project.bar(),
            },
            Snap::Bar => self.session.project.bar(),
            Snap::Beat => self.session.project.beat(),
            Snap::Eighth => PPQ / 2,
            Snap::Sixteenth => PPQ / 4,
            Snap::Off => 1,
        }
    }

    /// The grid line at or before a tick: the cell it's in.
    pub fn snap_down(&self, tick: f64) -> u64 {
        let g = self.grid() as f64;
        ((tick / g).floor() * g).max(0.0) as u64
    }

    pub fn snap(&self, tick: f64) -> u64 {
        let g = self.grid() as f64;
        ((tick / g).round() * g).max(0.0) as u64
    }

    /// The editor's height: as dragged, but never so tall the tracks lose their room, nor so
    /// short its tools don't fit.
    pub fn editor_height(&self) -> f32 {
        let tempo = match self.tempo_lane_shown() {
            true => crate::arrange::tempo::TEMPO_H,
            false => 0.0,
        };
        let room =
            self.viewport_h - CONTROL_BAR_H - STATUS_H - MARKERS_H - RULER_H - tempo - LANE_H * 2.0;
        self.editor_h.min(room).max(EDITOR_MIN_H)
    }

    /// How far the song runs for scrolling, in ticks: its end (or the cycle's, or the
    /// playhead's), and eight bars more to write into.
    pub fn timeline_end(&self) -> u64 {
        let p = &self.session.project;
        let cycle = p.cycle.map_or(0, |c| c.end);
        p.end().max(cycle).max(self.playhead()) + p.bar() * 8
    }

    /// The furthest right the lanes scroll.
    pub fn max_scroll_x(&self) -> f32 {
        let width = f32::from(self.lanes_bounds.get().size.width);
        (self.timeline_end() as f32 * self.px_per_tick() - width).max(0.0)
    }

    /// The furthest down the tracks scroll.
    pub fn max_scroll_y(&self) -> f32 {
        let height = f32::from(self.lanes_bounds.get().size.height);
        (self.rows_h() - height + LANE_H).max(0.0)
    }

    /// Scrolled sideways by hand: while playing, the view stops following the playhead until
    /// play starts again or the playhead's moved.
    pub(crate) fn scrolled_by_hand(&mut self) {
        if self.player.playing() {
            self.caught = false;
        }
    }

    /// The region picked (and a little either side) filling the lanes, else the whole song.
    pub fn zoom_to_fit(&mut self) {
        let p = &self.session.project;
        let region = self
            .region
            .filter(|at| p.tracks.get(at.0).is_some_and(|t| at.1 < t.regions.len()));
        let (from, to) = match region {
            Some(at) => {
                let r = p.region(at);
                (r.start, r.start + r.length)
            }
            None => (0, p.end().max(p.bar() * 4)),
        };
        let width = f32::from(self.lanes_bounds.get().size.width).max(200.0);
        let beats = (to - from) as f32 / PPQ as f32;
        self.px_per_beat = (width * 0.9 / beats.max(0.25)).clamp(4.0, 800.0);
        self.scroll_x = (from as f32 * self.px_per_tick() - width * 0.05).max(0.0);
    }

    pub(crate) fn scroll(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => p.map(f32::from),
            ScrollDelta::Lines(l) => l.map(|v| v * 24.0),
        };
        // ⇧ turns a mouse's wheel sideways.
        let d = match e.modifiers.shift && d.x == 0.0 {
            true => gpui::point(d.y, 0.0),
            false => d,
        };
        if e.modifiers.platform {
            // Zoom around the pointer.
            let x = f32::from(e.position.x) - f32::from(self.lanes_bounds.get().origin.x);
            self.zoom_around(self.px_per_beat * (1.0 + d.y * 0.01), x);
        } else {
            self.scroll_x = (self.scroll_x - d.x).clamp(0.0, self.max_scroll_x());
            self.scroll_y = (self.scroll_y - d.y).clamp(0.0, self.max_scroll_y());
            if d.x != 0.0 {
                self.scrolled_by_hand();
            }
        }
        cx.notify();
    }

    /// Zooms to `px_per_beat`, keeping what's `x` pixels into the lanes where it is.
    pub fn zoom_around(&mut self, px_per_beat: f32, x: f32) {
        let tick = (x + self.scroll_x) / self.px_per_tick();
        self.px_per_beat = px_per_beat.clamp(4.0, 800.0);
        self.scroll_x = (tick * self.px_per_tick() - x).max(0.0);
    }

    pub fn on_scroll(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static {
        cx.listener(|this, e, _, cx| this.scroll(e, cx))
    }

    /// Moves or resizes a note in the piano roll by `d` pixels from where the drag began.
    pub(crate) fn zoom(&mut self, by: f32, cx: &mut Context<Self>) {
        self.px_per_beat = (self.px_per_beat * by).clamp(4.0, 800.0);
        cx.notify();
    }
}
