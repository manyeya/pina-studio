//! Several regions picked at once: ⇧-click adds one or takes it out, a box dragged across empty
//! lanes picks what it touches. The focused region (`region`) is one of them; the others are
//! kept by their track, start and index, so they're found again whatever an edit does to the
//! order (the index first, then the start).
//! Dragging one moves them all; delete, mute, copy and the region keys act on them all.

use super::*;
use pina_engine::song::project::Region;

/// A region picked with the focused one: its track, its start, and where it was on its track.
pub type Pick = (usize, u64, usize);

impl Workspace {
    /// A picked region: where it was if it's still there, else the one on its track at its start.
    fn region_at(&self, (track, start, i): Pick) -> Option<At> {
        let regions = &self.session.project.tracks.get(track)?.regions;
        let free = |i: usize| regions[i].start == start && self.region != Some((track, i));
        (i < regions.len() && free(i))
            .then_some(i)
            .or_else(|| (0..regions.len()).find(|&i| free(i)))
            .map(|i| (track, i))
    }

    /// A region as a pick.
    fn pick(&self, at: At) -> Pick {
        (at.0, self.session.project.region(at).start, at.1)
    }

    /// The regions selected: the focused one first, then the others picked with it.
    pub fn selected_regions(&self) -> Vec<At> {
        let p = &self.session.project;
        let mut out: Vec<At> = self
            .region
            .filter(|at| p.tracks.get(at.0).is_some_and(|t| at.1 < t.regions.len()))
            .into_iter()
            .collect();
        for &pick in &self.picks {
            if let Some(at) = self.region_at(pick).filter(|at| !out.contains(at)) {
                out.push(at);
            }
        }
        out
    }

    pub fn is_selected(&self, at: At) -> bool {
        self.region == Some(at) || self.picks.iter().any(|&p| self.region_at(p) == Some(at))
    }

    /// ⇧-click: a region into the selection, or out of it.
    pub fn toggle_pick(&mut self, at: At) {
        if self.region == Some(at) {
            // The focus moves to another picked one, if there is one.
            let next = self.picks.pop();
            self.region = None;
            self.region = next.and_then(|p| self.region_at(p));
            return;
        }
        match self
            .picks
            .iter()
            .position(|&p| self.region_at(p) == Some(at))
        {
            Some(i) => drop(self.picks.remove(i)),
            None => match self.region {
                Some(_) => self.picks.push(self.pick(at)),
                None => self.region = Some(at),
            },
        }
    }

    /// A press on a region: it gets the focus; one not selected starts a selection of its own.
    pub fn focus_region(&mut self, at: At) {
        if !self.is_selected(at) {
            self.picks.clear();
        } else if self.region != Some(at) {
            let picks = std::mem::take(&mut self.picks);
            self.picks = picks
                .into_iter()
                .filter(|&p| self.region_at(p) != Some(at))
                .collect();
            if let Some(old) = self.region {
                self.picks.push(self.pick(old));
            }
        }
        self.region = Some(at);
    }

    /// The regions a box from one point to another (window positions, in the lanes) touches.
    pub fn box_pick(&mut self, a: Point<f32>, b: Point<f32>) {
        let (t0, t1) = (self.tick_at(a.x.min(b.x)), self.tick_at(a.x.max(b.x)));
        let top = f32::from(self.lanes_bounds.get().origin.y);
        let (y0, y1) = (a.y.min(b.y) - top, a.y.max(b.y) - top);
        let p = &self.session.project;
        let mut hits: Vec<At> = Vec::new();
        for (t, track) in p.tracks.iter().enumerate() {
            if self.folded(t) {
                continue;
            }
            let row = self.row_top(t);
            if row + self.lane_h(t) < y0 || row > y1 {
                continue;
            }
            for (i, r) in track.regions.iter().enumerate() {
                let (s, e) = (r.start as f64, (r.start + r.length) as f64);
                if e > t0 && s < t1 {
                    hits.push((t, i));
                }
            }
        }
        self.region = hits.first().copied();
        self.picks = hits.iter().skip(1).map(|&at| self.pick(at)).collect();
        if let Some(at) = self.region {
            self.select_track(at.0);
        }
        self.status = match hits.len() {
            0 | 1 => None,
            n => Some(format!("{n} regions picked")),
        };
    }

    /// While the focused region is dragged `dstart` ticks and `dtrack` tracks from where it was:
    /// the others picked with it go as far, from where they were (`picks_from`).
    pub fn move_picks(&mut self, dstart: i64, dtrack: i64) {
        let n = self.session.project.tracks.len() as i64;
        for k in 0..self.picks.len().min(self.picks_from.len()) {
            let (t, s, _) = self.picks[k];
            let (ot, os, _) = self.picks_from[k];
            let to = (
                (ot as i64 + dtrack).clamp(0, n - 1) as usize,
                (os as i64 + dstart).max(0) as u64,
            );
            if (t, s) == to {
                continue;
            }
            let Some(at) = self.region_at(self.picks[k]) else {
                continue;
            };
            // Moving it changes the order on its track: the focus and the other picks are found
            // again (by where they were, and their start).
            let focus = self.region.map(|f| self.pick(f));
            let others: Vec<Option<At>> = self.picks.iter().map(|&p| self.region_at(p)).collect();
            let now = self.session.project.move_region(at, to.0, to.1);
            let shift = |a: At| match a.0 == at.0 && a.1 > at.1 {
                true => (a.0, a.1 - 1),
                false => a,
            };
            for (j, o) in others.into_iter().enumerate() {
                if let (Some(o), true) = (o, j != k) {
                    self.picks[j] = self.pick(shift(o));
                }
            }
            self.picks[k] = self.pick(now);
            self.region = focus.map(|(ft, _, fi)| shift((ft, fi)));
        }
    }

    /// After the selected regions moved (`moved`, from track and start `from`) or changed length:
    /// the drag mode's way with what's round them.
    pub fn settle_regions(&mut self, moved: bool, from: (usize, u64)) {
        let all = self.selected_regions();
        match self.drag_mode {
            DragMode::Overlap => {}
            DragMode::NoOverlap => {
                // Each found again, as the ones before it may have cut others out.
                let picks: Vec<Pick> = all.iter().map(|&at| self.pick(at)).collect();
                self.region = None;
                let mut focus = None;
                for pick in picks {
                    if let Some(at) = self.region_at(pick) {
                        let now = self.session.project.clear_under(at);
                        focus = focus.or(Some(self.pick(now)));
                    }
                }
                self.region = focus.and_then(|f| self.region_at(f));
                self.picks.clear();
            }
            DragMode::Shuffle => {
                let p = &self.session.project;
                let mut tracks: Vec<(usize, u64)> =
                    all.iter().map(|&at| (at.0, p.region(at).start)).collect();
                if moved {
                    tracks.push(from);
                }
                tracks.sort();
                tracks.dedup_by_key(|t| t.0);
                for (t, from) in tracks {
                    self.session.project.close_up(t, from);
                }
                self.picks.clear();
            }
        }
    }

    /// Every selected region taken out. Whether there were several.
    pub fn delete_selected(&mut self, cx: &mut Context<Self>) -> bool {
        let mut all = self.selected_regions();
        if all.len() < 2 {
            return false;
        }
        all.sort_by(|a, b| b.cmp(a));
        self.edit(cx, |ws| {
            for (t, i) in all {
                let r = ws.session.project.tracks[t].regions.remove(i);
                if ws.drag_mode == DragMode::Shuffle {
                    ws.session.project.close_up(t, r.start);
                }
            }
            ws.region = None;
            ws.picks.clear();
        });
        true
    }

    /// `f` on every selected region, one undo. Whether there were several.
    pub fn each_selected(&mut self, cx: &mut Context<Self>, f: impl Fn(&mut Region)) -> bool {
        let all = self.selected_regions();
        if all.len() < 2 {
            return false;
        }
        self.edit(cx, |ws| {
            for &(t, i) in &all {
                f(&mut ws.session.project.tracks[t].regions[i]);
            }
        });
        // Picks follow their regions if they moved.
        self.picks = all.iter().skip(1).map(|&at| self.pick(at)).collect();
        true
    }

    /// The selected regions copied, as they lie relative to the first. Whether there were
    /// several.
    pub fn copy_selected(&mut self) -> bool {
        let all = self.selected_regions();
        if all.len() < 2 {
            return false;
        }
        let p = &self.session.project;
        let top = all.iter().map(|a| a.0).min().unwrap_or(0);
        let first = all.iter().map(|&a| p.region(a).start).min().unwrap_or(0);
        self.region_clips = all
            .iter()
            .map(|&a| {
                let r = p.region(a);
                (
                    a.0 - top,
                    Region {
                        start: r.start - first,
                        link: None,
                        ..r.clone()
                    },
                )
            })
            .collect();
        self.status = Some(format!("Copied {} regions", all.len()));
        true
    }

    /// The copied regions laid down from track `t` at `tick`, as they lay. Whether there were
    /// several.
    pub fn paste_selected(&mut self, t: usize, tick: u64, cx: &mut Context<Self>) -> bool {
        if self.region_clips.len() < 2 {
            return false;
        }
        let clips = self.region_clips.clone();
        let tick = self.snap(tick as f64);
        let mut placed = Vec::new();
        self.edit(cx, |ws| {
            let n = ws.session.project.tracks.len();
            for (dt, r) in clips {
                let track = (t + dt).min(n.saturating_sub(1));
                let fits =
                    ws.session.project.tracks[track].instrument.is_none() != r.content.is_midi();
                if fits {
                    let regions = &mut ws.session.project.tracks[track].regions;
                    regions.push(Region {
                        start: tick + r.start,
                        ..r
                    });
                    placed.push((track, regions.len() - 1));
                }
            }
        });
        self.region = placed.first().copied();
        self.picks = placed.iter().skip(1).map(|&at| self.pick(at)).collect();
        true
    }
}
