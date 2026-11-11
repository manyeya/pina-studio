//! The points picked, worked as one: the edges of the box around them dragged to stretch or
//! squeeze them in time (left, right) or in value (top, bottom), ⌥ about the box's middle; and
//! keys that nudge them, raise or lower them, duplicate them and pick a whole lane.

use super::*;

/// An edge of the box around the points picked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// How far picked points can move of `dt` ticks: none past a point not picked, or before 0.
pub(crate) fn room(points: &[AutoPoint], picked: &[usize], mut dt: i64) -> i64 {
    for &i in picked {
        let Some(q) = points.get(i) else { continue };
        let (prev, next) = neighbours(points, picked, i);
        dt = dt.max(prev.unwrap_or(0) as i64 - q.tick as i64);
        if let Some(n) = next {
            dt = dt.min(n as i64 - q.tick as i64);
        }
    }
    dt
}

/// The ticks of the points not picked either side of point `i`.
fn neighbours(points: &[AutoPoint], picked: &[usize], i: usize) -> (Option<u64>, Option<u64>) {
    let prev = (0..i).rev().find(|j| !picked.contains(j));
    let next = (i + 1..points.len()).find(|j| !picked.contains(j));
    (prev.map(|j| points[j].tick), next.map(|j| points[j].tick))
}

/// The least and most picked points' times can be scaled by about `anchor`, none passing a
/// point not picked or going before 0.
pub(crate) fn stretch_limits(points: &[AutoPoint], picked: &[usize], anchor: f64) -> (f64, f64) {
    let (mut least, mut most) = (0.0f64, f64::INFINITY);
    for &i in picked {
        let Some(q) = points.get(i) else { continue };
        let d = q.tick as f64 - anchor;
        if d.abs() < 0.5 {
            continue;
        }
        let (prev, next) = neighbours(points, picked, i);
        let prev = prev.unwrap_or(0) as f64;
        if d > 0.0 {
            least = least.max((prev - anchor) / d);
            if let Some(n) = next {
                most = most.min((n as f64 - anchor) / d);
            }
        } else {
            most = most.min((anchor - prev) / -d);
            if let Some(n) = next {
                least = least.max((anchor - n as f64) / -d);
            }
        }
    }
    (least, most.max(least))
}

impl Workspace {
    /// The lane with points picked: its track, parameter and the points (those still there).
    pub(crate) fn picked_lane(&self) -> Option<(usize, Param, Vec<usize>)> {
        let p = self.auto_picked.as_ref().filter(|p| !p.points.is_empty())?;
        let track = self
            .session
            .project
            .tracks
            .iter()
            .position(|t| t.id == p.track)?;
        let n = self.session.project.tracks[track]
            .automation
            .points(p.param)
            .len();
        let points: Vec<usize> = p.points.iter().copied().filter(|&i| i < n).collect();
        (!points.is_empty()).then_some((track, p.param, points))
    }

    /// An edge of the picked points' box pressed: stretching them from there.
    pub fn stretch_press(
        &mut self,
        track: usize,
        param: Param,
        side: Side,
        e: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        self.select_track(track);
        self.auto_grab = self.session.project.tracks[track]
            .automation
            .points(param)
            .to_vec();
        self.begin_drag(
            Drag::AutoStretch {
                track,
                param,
                side,
                from: (f32::from(e.position.x), f32::from(e.position.y)),
                scale: 1.0,
                moved: false,
            },
            cx,
        );
    }

    /// The picked points stretched from an edge of their box, as they were when it was pressed:
    /// left and right scale their times (the edge on the grid unless ⇧), top and bottom their
    /// values; about the other edge, or the middle with ⌥.
    pub fn drag_stretch(
        &mut self,
        track: usize,
        param: Param,
        side: Side,
        from: (f32, f32),
        p: Point<f32>,
    ) {
        let picked = self.picked_points(track, param);
        let grab = self.auto_grab.clone();
        let chosen: Vec<AutoPoint> = picked
            .iter()
            .filter_map(|&i| grab.get(i).copied())
            .collect();
        if chosen.len() < 2 {
            return;
        }
        let (h, middle) = (self.auto_h(), self.alt);
        let mut points = grab.clone();
        let scale = match side {
            Side::Left | Side::Right => {
                let lo = chosen.iter().map(|q| q.tick).min().unwrap_or(0) as f64;
                let hi = chosen.iter().map(|q| q.tick).max().unwrap_or(0) as f64;
                if hi <= lo {
                    return;
                }
                let (edge, other) = if side == Side::Left {
                    (lo, hi)
                } else {
                    (hi, lo)
                };
                let anchor = if middle { (lo + hi) / 2.0 } else { other };
                let raw = (edge + ((p.x - from.0) / self.px_per_tick()) as f64).max(0.0);
                let to = match self.fine || !self.settings.snap_automation {
                    true => raw,
                    false => self.snap(raw) as f64,
                };
                let (least, most) = stretch_limits(&grab, &picked, anchor);
                let f = ((to - anchor) / (edge - anchor)).clamp(least, most);
                for &i in &picked {
                    let t = grab[i].tick as f64;
                    points[i].tick = (anchor + (t - anchor) * f).round().max(0.0) as u64;
                }
                f
            }
            Side::Top | Side::Bottom => {
                let ys: Vec<f32> = chosen.iter().map(|q| to_y(param, q.value, h)).collect();
                let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
                let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                if param.steps() || bottom - top < 0.5 {
                    return;
                }
                let (edge, other) = if side == Side::Top {
                    (top, bottom)
                } else {
                    (bottom, top)
                };
                let anchor = if middle { (top + bottom) / 2.0 } else { other };
                let f = ((edge + p.y - from.1 - anchor) / (edge - anchor)).max(0.0);
                for &i in &picked {
                    let y = to_y(param, grab[i].value, h);
                    points[i].value = from_y(param, anchor + (y - anchor) * f, h);
                }
                f as f64
            }
        };
        *self.session.project.tracks[track]
            .automation
            .points_mut(param) = points;
        if let Some(Drag::AutoStretch { scale: s, .. }) = &mut self.drag {
            *s = scale as f32;
        }
    }

    /// Keys on the points picked: moved `ticks` (none past a point not picked) and raised
    /// `share` of the lane's height (mute: on or off). False with none picked.
    pub(crate) fn nudge_picked(&mut self, ticks: i64, share: f32, cx: &mut Context<Self>) -> bool {
        let Some((track, param, picked)) = self.picked_lane() else {
            return false;
        };
        let h = self.auto_h();
        let share = match param.steps() && share != 0.0 {
            true => share.signum(),
            false => share,
        };
        self.edit(cx, |ws| {
            let points = ws.session.project.tracks[track]
                .automation
                .points_mut(param);
            let dt = room(points, &picked, ticks);
            for &i in &picked {
                let q = &mut points[i];
                q.tick = (q.tick as i64 + dt).max(0) as u64;
                if share != 0.0 {
                    q.value = from_y(param, to_y(param, q.value, h) - share * (h - 2.0 * PAD), h);
                }
            }
        });
        true
    }

    /// ⌘D on points: them again straight after themselves (from a grid step past the last), the
    /// copies picked.
    pub(crate) fn duplicate_picked(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((track, param, picked)) = self.picked_lane() else {
            return false;
        };
        let points = self.session.project.tracks[track].automation.points(param);
        let mut chosen: Vec<AutoPoint> = picked.iter().map(|&i| points[i]).collect();
        chosen.sort_by_key(|q| q.tick);
        let (first, last) = (chosen[0].tick, chosen[chosen.len() - 1].tick);
        let grid = self.grid().max(1);
        let at = first + (last - first + 1).div_ceil(grid) * grid;
        let clip = AutoClip {
            length: last - first,
            lanes: vec![(
                param,
                chosen
                    .iter()
                    .map(|q| AutoPoint {
                        tick: q.tick - first,
                        ..*q
                    })
                    .collect(),
            )],
        };
        self.put_clip(track, param, at, &clip, cx);
        true
    }

    /// ⌘A with a lane last worked on (and showing): every point on it.
    pub(crate) fn pick_lane(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((id, param)) = self.auto_focus.filter(|_| self.show_automation) else {
            return false;
        };
        let Some(track) = self.session.project.tracks.iter().position(|t| t.id == id) else {
            return false;
        };
        let n = self.session.project.tracks[track]
            .automation
            .points(param)
            .len();
        if n == 0 {
            return false;
        }
        self.pick_points(track, param, (0..n).collect());
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ticks: &[u64]) -> Vec<AutoPoint> {
        ticks.iter().map(|&t| AutoPoint::new(t, 0.5)).collect()
    }

    #[test]
    fn picked_points_stop_at_the_ones_not_picked() {
        let points = at(&[0, 100, 200, 300]);
        assert_eq!(room(&points, &[1, 2], 500), 100);
        assert_eq!(room(&points, &[1, 2], -500), -100);
        assert_eq!(room(&points, &[0, 1], -500), 0);
        assert_eq!(room(&points, &[3], 500), 500);
    }

    #[test]
    fn a_stretch_goes_no_further_than_the_neighbours_allow() {
        let points = at(&[0, 100, 200, 400]);
        // 100 and 200 picked, stretched from 100: 200 can reach 400, three times as far.
        assert_eq!(stretch_limits(&points, &[1, 2], 100.0), (0.0, 3.0));
        // About their middle (150): out to the neighbours either side.
        let (least, most) = stretch_limits(&points, &[1, 2], 150.0);
        assert_eq!((least, most), (0.0, 3.0));
        // Picked across one not picked: squeezed no tighter than it.
        let (least, _) = stretch_limits(&points, &[0, 2], 0.0);
        assert_eq!(least, 0.5);
    }
}
