//! Editing a lane with the mouse: points picked, dragged, drawn, deleted, copied and pasted.

use super::*;

/// Selected points: a lane's, by index.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Picked {
    pub track: u64,
    pub param: Param,
    pub points: Vec<usize>,
}

impl Workspace {
    /// A lane's parameter, by its track and place.
    pub(crate) fn lane_at(&self, track: usize, k: usize) -> Param {
        self.auto_lanes(track).get(k).copied().unwrap_or_default()
    }

    /// The lane under a window y on a track's row, if one is.
    pub(crate) fn lane_y(&self, track: usize, k: usize, y: f32) -> f32 {
        y - f32::from(self.lanes_bounds.get().origin.y) - self.lane_top(track, k)
    }

    /// A value at a window y in a lane.
    pub fn auto_value(&self, track: usize, k: usize, y: f32) -> f32 {
        from_y(
            self.lane_at(track, k),
            self.lane_y(track, k, y),
            self.auto_h(),
        )
    }

    /// A tick at a window x, on the grid unless ⌘ is held or the grid is off.
    pub(crate) fn auto_tick(&self, x: f32, free: bool) -> u64 {
        let tick = self.tick_at(x);
        match free || !self.settings.snap_automation {
            true => tick.round().max(0.0) as u64,
            false => self.snap(tick),
        }
    }

    pub fn picked_points(&self, track: usize, param: Param) -> Vec<usize> {
        let id = self.session.project.tracks[track].id;
        match &self.auto_picked {
            Some(p) if p.track == id && p.param == param => p.points.clone(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn pick_points(&mut self, track: usize, param: Param, points: Vec<usize>) {
        let id = self.session.project.tracks[track].id;
        self.auto_picked = Some(Picked {
            track: id,
            param,
            points,
        });
        self.auto_focus = Some((id, param));
    }

    /// The pointer pressed on a lane, not on a point: the pencil draws, the eraser and a drag
    /// select (the eraser takes out what it selects), a press on the line grabs it, and a click
    /// puts a point.
    pub fn lane_press(
        &mut self,
        track: usize,
        k: usize,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        let p = (f32::from(e.position.x), f32::from(e.position.y));
        self.select_track(track);
        self.region = None;
        let id = self.session.project.tracks[track].id;
        self.auto_focus = Some((id, param));
        if self.tool == Tool::Pencil {
            self.session.checkpoint();
            let tick = self.auto_tick(p.0, e.modifiers.platform);
            let value = self.auto_value(track, k, p.1);
            self.draw_to(track, param, None, tick, value);
            self.drag = Some(Drag::AutoDraw {
                track,
                param,
                last: (tick, value),
            });
            self.changed(cx);
            return;
        }
        // On the line between two points: up or down together.
        let points = self.session.project.tracks[track].automation.points(param);
        if self.tool != Tool::Eraser && !points.is_empty() && !e.modifiers.shift {
            let tick = self.tick_at(p.0);
            let y = self.lane_y(track, k, p.1);
            let line = value_at(points, tick).map(|v| to_y(param, v, self.auto_h()));
            if line.is_some_and(|l| (l - y).abs() < NEAR) {
                let i = points.partition_point(|q| (q.tick as f64) <= tick);
                let ends: Vec<usize> = match (i.checked_sub(1), i < points.len()) {
                    (Some(a), true) => vec![a, i],
                    (Some(a), false) => vec![a],
                    (None, _) => vec![0],
                };
                self.auto_grab = points.to_vec();
                self.pick_points(track, param, ends);
                self.begin_drag(
                    Drag::AutoLine {
                        track,
                        param,
                        k,
                        y: p.1,
                        moved: false,
                    },
                    cx,
                );
                return;
            }
        }
        if !e.modifiers.shift {
            self.auto_picked = None;
        }
        self.drag = Some(Drag::AutoMarquee {
            track,
            k,
            from: p,
            to: p,
            erase: self.tool == Tool::Eraser,
            free: e.modifiers.platform,
        });
        cx.notify();
    }

    /// A point pressed: ⌥, the eraser or a double-click takes it out; ⇧ adds it to the
    /// selection or takes it off; else it's dragged, with the others selected.
    pub fn point_press(
        &mut self,
        track: usize,
        k: usize,
        index: usize,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        self.select_track(track);
        self.region = None;
        if e.modifiers.alt || self.tool == Tool::Eraser || e.click_count == 2 {
            let mut gone = self.picked_points(track, param);
            if !gone.contains(&index) {
                gone = vec![index];
            }
            self.delete_points(track, param, gone, cx);
            return;
        }
        let mut picked = self.picked_points(track, param);
        if e.modifiers.shift {
            match picked.iter().position(|&i| i == index) {
                Some(j) => drop(picked.remove(j)),
                None => picked.push(index),
            }
            self.pick_points(track, param, picked);
            cx.notify();
            return;
        }
        if !picked.contains(&index) {
            picked = vec![index];
        }
        self.pick_points(track, param, picked);
        self.auto_grab = self.session.project.tracks[track]
            .automation
            .points(param)
            .to_vec();
        self.begin_drag(
            Drag::Auto {
                track,
                param,
                k,
                index,
                moved: false,
            },
            cx,
        );
    }

    /// Selected points dragged: the one held to the pointer (on the grid unless ⌘), the others
    /// as far, none past a point not selected.
    pub fn drag_points(
        &mut self,
        track: usize,
        param: Param,
        k: usize,
        index: usize,
        p: Point<f32>,
    ) {
        let picked = self.picked_points(track, param);
        let grab = &self.auto_grab;
        let Some(held) = grab.get(index).copied() else {
            return;
        };
        let h = self.auto_h();
        let free = self.fine;
        let tick = self.auto_tick(p.x, free) as i64;
        let mut dt = tick - held.tick as i64;
        // None past a neighbour that isn't moving.
        for &i in &picked {
            let t = grab[i].tick as i64;
            let prev = (0..i)
                .rev()
                .find(|j| !picked.contains(j))
                .map(|j| grab[j].tick as i64);
            let next = (i + 1..grab.len())
                .find(|j| !picked.contains(j))
                .map(|j| grab[j].tick as i64);
            dt = dt.max(prev.unwrap_or(0) - t).max(-t);
            if let Some(n) = next {
                dt = dt.min(n - t);
            }
        }
        let dy = self.lane_y(track, k, p.y) - to_y(param, held.value, h);
        let mut points = grab.clone();
        for &i in &picked {
            let q = grab[i];
            points[i].tick = (q.tick as i64 + dt).max(0) as u64;
            points[i].value = from_y(param, to_y(param, q.value, h) + dy, h);
        }
        // The one held goes exactly where the pointer is.
        points[index].value = from_y(param, self.lane_y(track, k, p.y), h);
        *self.session.project.tracks[track]
            .automation
            .points_mut(param) = points;
    }

    /// The line between two points dragged up or down: both ends with it.
    pub fn drag_line(&mut self, track: usize, param: Param, k: usize, y: f32, p: Point<f32>) {
        let _ = k;
        let h = self.auto_h();
        let dy = (p.y - y) * if self.fine { 0.25 } else { 1.0 };
        let mut points = self.auto_grab.clone();
        for i in self.picked_points(track, param) {
            if let Some(q) = points.get_mut(i) {
                q.value = from_y(param, to_y(param, q.value, h) + dy, h);
            }
        }
        *self.session.project.tracks[track]
            .automation
            .points_mut(param) = points;
    }

    /// A line's bend dragged from its middle: pulled towards the end it rises to, it gets
    /// there sooner.
    pub fn drag_bend(
        &mut self,
        track: usize,
        param: Param,
        index: usize,
        y: f32,
        curve: f32,
        p: Point<f32>,
    ) {
        let h = self.auto_h();
        let points = self.session.project.tracks[track]
            .automation
            .points_mut(param);
        let (Some(a), Some(b)) = (points.get(index).copied(), points.get(index + 1).copied())
        else {
            return;
        };
        let (ya, yb) = (to_y(param, a.value, h), to_y(param, b.value, h));
        let way = if ya > yb { 1.0 } else { -1.0 };
        points[index].curve = (curve + (p.y - y) / 50.0 * way).clamp(-1.0, 1.0);
        points[index].ease = false;
    }

    /// The pencil drawn from its last point to here: what was between goes.
    pub(crate) fn draw_to(
        &mut self,
        track: usize,
        param: Param,
        last: Option<u64>,
        tick: u64,
        value: f32,
    ) {
        let a = &mut self.session.project.tracks[track].automation;
        let points = a.points_mut(param);
        let (lo, hi) = match last {
            Some(l) => (l.min(tick), l.max(tick)),
            None => (tick, tick),
        };
        points.retain(|p| p.tick == last.unwrap_or(u64::MAX) || p.tick < lo || p.tick > hi);
        let i = points.partition_point(|p| p.tick < tick);
        points.insert(
            i,
            AutoPoint {
                step: param.steps(),
                ..AutoPoint::new(tick, value)
            },
        );
    }

    pub fn drag_draw(&mut self, track: usize, param: Param, last: (u64, f32), p: Point<f32>) {
        let k = self
            .auto_lanes(track)
            .iter()
            .position(|&q| q == param)
            .unwrap_or(0);
        let tick = self.auto_tick(p.x, false);
        let value = self.auto_value(track, k, p.y);
        if tick == last.0 {
            let points = self.session.project.tracks[track]
                .automation
                .points_mut(param);
            if let Some(q) = points.iter_mut().find(|q| q.tick == tick) {
                q.value = value;
            }
        } else {
            self.draw_to(track, param, Some(last.0), tick, value);
        }
        if let Some(Drag::AutoDraw { last, .. }) = &mut self.drag {
            *last = (tick, value);
        }
    }

    /// The pencil let go: what it drew, thinned.
    pub fn draw_done(&mut self, track: usize, param: Param) {
        if self.settings.thin_recorded {
            self.session.project.tracks[track]
                .automation
                .thin(param, 0.004);
        }
    }

    /// A marquee let go: the points inside selected (or erased); a click without dragging puts
    /// a point, on the line if it was near.
    #[allow(clippy::too_many_arguments)]
    pub fn marquee_done(
        &mut self,
        track: usize,
        k: usize,
        from: (f32, f32),
        to: (f32, f32),
        erase: bool,
        free: bool,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        let h = self.auto_h();
        if (to.0 - from.0).abs() < 4.0 && (to.1 - from.1).abs() < 4.0 {
            if erase {
                return;
            }
            let tick = self.auto_tick(from.0, free);
            let y = self.lane_y(track, k, from.1);
            let points = self.session.project.tracks[track].automation.points(param);
            let line = value_at(points, tick as f64);
            let value = match line {
                Some(v) if (to_y(param, v, h) - y).abs() < NEAR * 1.5 => v,
                _ => from_y(param, y, h),
            };
            let name = self.param_name(track, param);
            let mut index = 0;
            self.edit(cx, |ws| {
                let a = &mut ws.session.project.tracks[track].automation;
                index = a.add(
                    param,
                    AutoPoint {
                        step: param.steps(),
                        ..AutoPoint::new(tick, value)
                    },
                );
                a.name(param, &name);
            });
            self.pick_points(track, param, vec![index]);
            return;
        }
        let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
        let (y0, y1) = (
            self.lane_y(track, k, from.1.min(to.1)),
            self.lane_y(track, k, from.1.max(to.1)),
        );
        let (t0, t1) = (self.tick_at(x0), self.tick_at(x1));
        let points = self.session.project.tracks[track].automation.points(param);
        let inside: Vec<usize> = points
            .iter()
            .enumerate()
            .filter(|(_, q)| {
                let y = to_y(param, q.value, h);
                (q.tick as f64) >= t0 && (q.tick as f64) <= t1 && y >= y0 - 4.0 && y <= y1 + 4.0
            })
            .map(|(i, _)| i)
            .collect();
        if erase {
            self.delete_points(track, param, inside, cx);
            return;
        }
        let mut picked = self.picked_points(track, param);
        for i in inside {
            if !picked.contains(&i) {
                picked.push(i);
            }
        }
        self.pick_points(track, param, picked);
        cx.notify();
    }

    pub(crate) fn delete_points(
        &mut self,
        track: usize,
        param: Param,
        mut gone: Vec<usize>,
        cx: &mut Context<Self>,
    ) {
        gone.sort_unstable();
        gone.dedup();
        self.edit(cx, |ws| {
            let a = &mut ws.session.project.tracks[track].automation;
            let points = a.points_mut(param);
            for &i in gone.iter().rev() {
                if i < points.len() {
                    points.remove(i);
                }
            }
            a.tidy();
            ws.auto_picked = None;
        });
    }

    /// Delete with points selected takes them out.
    pub fn delete_picked(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(p) = self.auto_picked.clone().filter(|p| !p.points.is_empty()) else {
            return false;
        };
        let Some(track) = self
            .session
            .project
            .tracks
            .iter()
            .position(|t| t.id == p.track)
        else {
            return false;
        };
        self.delete_points(track, p.param, p.points, cx);
        true
    }

    /// ⌘C with points selected copies them.
    pub fn copy_picked(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(p) = self.auto_picked.clone().filter(|p| !p.points.is_empty()) else {
            return false;
        };
        let Some(t) = self.session.project.tracks.iter().find(|t| t.id == p.track) else {
            return false;
        };
        let points = t.automation.points(p.param);
        let chosen: Vec<AutoPoint> = p
            .points
            .iter()
            .filter_map(|&i| points.get(i).copied())
            .collect();
        let (Some(first), Some(last)) = (
            chosen.iter().map(|q| q.tick).min(),
            chosen.iter().map(|q| q.tick).max(),
        ) else {
            return false;
        };
        let mut lane: Vec<AutoPoint> = chosen
            .iter()
            .map(|q| AutoPoint {
                tick: q.tick - first,
                ..*q
            })
            .collect();
        lane.sort_by_key(|q| q.tick);
        self.auto_clip = Some(AutoClip {
            length: last - first,
            lanes: vec![(p.param, lane)],
        });
        self.status = Some(format!("{} points copied", chosen.len()));
        cx.notify();
        true
    }

    /// ⌘V after points were copied: them, at the playhead, on the lane last worked on.
    pub fn paste_points(&mut self, cx: &mut Context<Self>) -> bool {
        if self.auto_clip.is_none() || self.auto_focus.is_none() {
            return false;
        }
        let tick = self.playhead();
        self.paste_points_at(tick, cx)
    }

    pub(crate) fn paste_points_at(&mut self, tick: u64, cx: &mut Context<Self>) -> bool {
        let (Some(clip), Some((id, param))) = (self.auto_clip.clone(), self.auto_focus) else {
            return false;
        };
        let Some(track) = self.session.project.tracks.iter().position(|t| t.id == id) else {
            return false;
        };
        let Some((from, points)) = clip.lanes.first().cloned() else {
            return false;
        };
        if units(from) != units(param) {
            self.status = Some(format!(
                "{} points don't go on {}.",
                self.param_name(track, from),
                self.param_name(track, param)
            ));
            cx.notify();
            return true;
        }
        let one = AutoClip {
            length: clip.length,
            lanes: vec![(param, points)],
        };
        self.edit(cx, |ws| {
            ws.session.project.tracks[track].automation.put(tick, &one);
        });
        let a = &self.session.project.tracks[track].automation;
        let picked = a
            .points(param)
            .iter()
            .enumerate()
            .filter(|(_, q)| q.tick >= tick && q.tick <= tick + clip.length)
            .map(|(i, _)| i)
            .collect();
        self.pick_points(track, param, picked);
        true
    }
}
