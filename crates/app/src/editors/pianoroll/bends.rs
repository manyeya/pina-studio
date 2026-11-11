//! A note's own expression (MPE) edited in the roll: its pitch, slide and pressure, each a line of
//! points, in the lane under the notes; its pitch also over the notes themselves while the Pitch
//! lane shows. A click adds a point; a drag draws, the line thinned to the points that shape it
//! when it's let go; a point dragged moves, double-clicked goes; ⌥ on a line bows it; ⇧ scales
//! the lines of the notes under the hand (the picked, if any); ⌥ anywhere else in the lane rubs
//! out. A pitch lands on whole semitones, or between them with ⌘.

use pina_engine::song::project::{Bend, Dim, shape, thin};

use super::*;

/// Room in the lane above a full value, and under an empty one.
const LANE_TOP: f32 = 10.0;
const LANE_FOOT: f32 = 8.0;
/// How near a point or a line the hand must be to take it, in pixels.
const NEAR_POINT: f32 = 6.0;
const NEAR_LINE: f32 = 5.0;

/// What a hand on the notes' expression is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BendHow {
    /// Note `note`'s `k`th point dragged.
    Point { note: usize, k: usize },
    /// The line on from note `note`'s `k`th point bowed, from the bow it had.
    Bow { note: usize, k: usize, was: f32 },
    /// Drawn, last at `tick` (into the region) and `value`; over the notes, on `lead` (and the
    /// notes picked with it), else on whatever it crosses.
    Draw {
        lead: Option<usize>,
        tick: i64,
        value: f32,
    },
    /// The lines of the notes in `held` scaled.
    Scale,
    /// Rubbed out, last at `tick`.
    Erase { tick: i64 },
}

/// A point, or the line on from one, under the hand: its note and which.
#[derive(Clone, Copy, Debug)]
enum Hit {
    Point(usize, usize),
    Line(usize, usize),
}

/// `hit`, `d` from the hand, if it's nearer than the best so far.
fn offer(best: &mut Option<(f32, Hit)>, d: f32, hit: Hit) {
    if best.as_ref().is_none_or(|(b, _)| d < *b) {
        *best = Some((d, hit));
    }
}

/// Where a value sits in the lane, 0 at its foot to 1 at its top: a bend about the middle,
/// reaching `PITCH_SHOWN` either way.
pub(crate) fn lane_level(dim: Dim, v: f32) -> f32 {
    match dim {
        Dim::Pitch => 0.5 + v / (2.0 * PITCH_SHOWN),
        _ => v,
    }
}

fn level_value(dim: Dim, level: f32) -> f32 {
    let (lo, hi) = dim.range();
    match dim {
        Dim::Pitch => ((level - 0.5) * 2.0 * PITCH_SHOWN).clamp(lo, hi),
        _ => level.clamp(lo, hi),
    }
}

/// The lane's y for a level, its bounds `b`.
pub(crate) fn lane_y(b: Bounds<gpui::Pixels>, level: f32) -> f32 {
    let h = f32::from(b.size.height) - LANE_TOP - LANE_FOOT;
    f32::from(b.origin.y) + LANE_TOP + (1.0 - level.clamp(0.0, 1.0)) * h
}

fn y_level(b: Bounds<gpui::Pixels>, y: f32) -> f32 {
    let h = (f32::from(b.size.height) - LANE_TOP - LANE_FOOT).max(1.0);
    1.0 - (y - f32::from(b.origin.y) - LANE_TOP) / h
}

/// A value as words.
pub(crate) fn say(dim: Dim, v: f32) -> String {
    match dim {
        Dim::Pitch if (v - v.round()).abs() < 0.005 => format!("{:+} st", v.round() as i32),
        Dim::Pitch => format!("{v:+.2} st"),
        _ => format!("{:.0}%", v * 100.0),
    }
}

/// A note's line as drawn: its span (x from and to), its row's middle (over the notes), its
/// points (x, value, bow), its colour; whether it's picked, and shown full (picked, or none are).
pub(crate) struct NoteLine {
    pub from: f32,
    pub to: f32,
    pub mid: f32,
    pub points: Vec<(f32, f32, f32)>,
    pub ink: u32,
    pub picked: bool,
    pub bright: bool,
}

/// A point of a note's line, `size` across: white when its note is picked, else its colour,
/// ringed dark.
pub(crate) fn dot(window: &mut Window, (x, y): (f32, f32), ink: u32, picked: bool, size_: f32) {
    let r = size_ / 2.0;
    for (r, c) in [
        (r + 1.5, Hsla::from(rgb(0x0b0b0e))),
        (r, if picked { rgb(0xffffff) } else { rgb(ink) }.into()),
    ] {
        window.paint_quad(
            fill(
                Bounds::new(point(px(x - r), px(y - r)), size(px(2.0 * r), px(2.0 * r))),
                c,
            )
            .corner_radii(gpui::Corners::all(px(r))),
        );
    }
}

/// A line's points, but those crowding the one before (too close to take hold of apart); small
/// where they're close together, as along a vibrato.
pub(crate) fn dots(window: &mut Window, points: &[(f32, f32, f32)], ink: u32, picked: bool) {
    let close = points
        .windows(2)
        .any(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1) < 18.0);
    let size_ = if close { 4.0 } else { 7.0 };
    let mut last: Option<(f32, f32)> = None;
    for &(x, y, _) in points {
        if last.is_some_and(|(lx, ly)| (x - lx).hypot(y - ly) < 6.0) {
            continue;
        }
        dot(window, (x, y), ink, picked, size_);
        last = Some((x, y));
    }
}

/// A line through `points` (x, y and the bow on to the next), held flat from `from` to the
/// first and from the last to `to`.
pub(crate) fn paint_line(
    window: &mut Window,
    points: &[(f32, f32, f32)],
    (from, to): (f32, f32),
    width: f32,
    color: Hsla,
) {
    let Some(&(_, y0, _)) = points.first() else {
        return;
    };
    let mut path = gpui::PathBuilder::stroke(px(width));
    path.move_to(point(px(from), px(y0)));
    let mut last = (from, y0);
    for (k, &(x, y, bow)) in points.iter().enumerate() {
        let x = x.clamp(from, to);
        if k == 0 {
            path.line_to(point(px(x), px(y)));
        }
        if let Some(&(nx, ny, _)) = points.get(k + 1) {
            let nx = nx.clamp(from, to);
            let steps = match bow.abs() < 0.01 || (ny - y).abs() < 0.5 {
                true => 1,
                false => 16,
            };
            for s in 1..=steps {
                let f = s as f32 / steps as f32;
                path.line_to(point(
                    px(x + (nx - x) * f),
                    px(y + (ny - y) * shape(f, bow)),
                ));
            }
        }
        last = (x, y);
    }
    path.line_to(point(px(to.max(last.0)), px(last.1)));
    if let Ok(p) = path.build() {
        window.paint_path(p, color);
    }
}

/// `points` thinned where a stroke went (`lo` to `hi`, ticks into the note), the rest kept as
/// they are.
fn thin_within(points: &[Bend], (lo, hi): (u64, u64), tolerance: f32) -> Vec<Bend> {
    let inside: Vec<usize> = (0..points.len())
        .filter(|&k| (lo..=hi).contains(&points[k].at))
        .collect();
    let (Some(&first), Some(&last)) = (inside.first(), inside.last()) else {
        return points.to_vec();
    };
    // From the point before the stroke to the one after, so it joins as it did.
    let (a, z) = (first.saturating_sub(1), (last + 1).min(points.len() - 1));
    let mut out = points[..a].to_vec();
    out.extend(thin(&points[a..=z], tolerance));
    out.extend_from_slice(&points[z + 1..]);
    out
}

impl Workspace {
    /// The window x of a tick.
    fn tick_x(&self, tick: i64) -> f32 {
        f32::from(self.roll_bounds.get().origin.x) + tick as f32 * self.px_per_tick()
            - self.scroll_x
    }

    /// The window y over the notes of `pitch` bent `v` semitones.
    fn bent_y(&self, pitch: u8, v: f32) -> f32 {
        let kh = self.roll.key_h;
        f32::from(self.roll_bounds.get().origin.y) + (127 - pitch) as f32 * kh - self.roll_scroll_y
            + kh / 2.0
            - v * kh
    }

    /// Where a value of a note's line shows: over the notes, or in the lane.
    fn bend_y(&self, dim: Dim, grid: bool, pitch: u8, v: f32) -> f32 {
        match grid {
            true => self.bent_y(pitch, v),
            false => lane_y(self.lane_bounds.get(), lane_level(dim, v)),
        }
    }

    /// The value at `y` for the note at `pitch`: a pitch on whole semitones, unless ⌘.
    fn bend_value(&self, dim: Dim, grid: bool, pitch: u8, y: f32) -> f32 {
        let v = match grid {
            true => (self.bent_y(pitch, 0.0) - y) / self.roll.key_h,
            false => level_value(dim, y_level(self.lane_bounds.get(), y)),
        };
        let (lo, hi) = dim.range();
        match dim {
            Dim::Pitch if !self.cmd => v.round(),
            _ => v,
        }
        .clamp(lo, hi)
    }

    /// The point or line of the notes' expression nearest `p`: a point before a line, a picked
    /// note's before another's.
    fn bend_hit(&self, at: At, dim: Dim, grid: bool, p: Point<f32>) -> Option<Hit> {
        let r = self.session.project.region(at);
        let Content::Midi { notes, .. } = &r.content else {
            return None;
        };
        let start = r.start as i64;
        let xy = |n: &Note, b: &Bend| {
            (
                self.tick_x(start + n.start + b.at as i64),
                self.bend_y(dim, grid, n.pitch, b.value),
            )
        };
        let favour = |i: usize| match self.picked.contains(&i) {
            true => 0.75,
            false => 0.0,
        };
        let mut best: Option<(f32, Hit)> = None;
        for (i, n) in notes.iter().enumerate() {
            for (k, b) in n.expression.get(dim).iter().enumerate() {
                let (x, y) = xy(n, b);
                let d = (x - p.x).hypot(y - p.y);
                if d <= NEAR_POINT {
                    offer(&mut best, d - favour(i), Hit::Point(i, k));
                }
            }
        }
        if best.is_some() {
            return best.map(|b| b.1);
        }
        for (i, n) in notes.iter().enumerate() {
            for (k, w) in n.expression.get(dim).windows(2).enumerate() {
                let ((xa, ya), (xb, yb)) = (xy(n, &w[0]), xy(n, &w[1]));
                if p.x < xa || p.x > xb || xb - xa < 1.0 {
                    continue;
                }
                let y = ya + (yb - ya) * shape((p.x - xa) / (xb - xa), w[0].curve);
                let d = (y - p.y).abs();
                if d <= NEAR_LINE {
                    offer(&mut best, d - favour(i), Hit::Line(i, k));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// The notes sounding at `tick` (into the region): the picked ones, if any of them are.
    fn sounding_at(&self, at: At, tick: i64) -> Vec<usize> {
        let notes = self.session.project.region(at).content.notes(0);
        let notes = notes.as_deref().unwrap_or(&[]);
        let under = |i: &usize| {
            let n = &notes[*i];
            n.start <= tick && tick < n.start + n.length as i64
        };
        let picked: Vec<usize> = self.picked.iter().copied().filter(under).collect();
        match picked.is_empty() {
            true => (0..notes.len()).filter(under).collect(),
            false => picked,
        }
    }

    /// A press on the notes' expression, over the notes (`grid`, on note `on` if it was one) or
    /// in the lane under them. Whether it took the press: over the notes, one on nothing is the
    /// grid's.
    pub(crate) fn expression_down(
        &mut self,
        e: &MouseDownEvent,
        grid: bool,
        on: Option<usize>,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(at), Some(dim)) = (self.roll_region(), self.roll.lane.dim()) else {
            return false;
        };
        let p = e.position.map(f32::from);
        let hit = self.bend_hit(at, dim, grid, p);
        if e.click_count == 2
            && let Some(Hit::Point(i, k)) = hit
        {
            self.edit(cx, |ws| {
                if let Some(n) = ws
                    .session
                    .project
                    .notes_mut(at)
                    .and_then(|ns| ns.get_mut(i))
                {
                    n.expression.get_mut(dim).remove(k);
                }
            });
            return true;
        }
        let start = self.session.project.region(at).start as i64;
        let tick = self.tick_at(p.x) as i64 - start;
        let notes = self.session.project.region(at).content.notes(0);
        let notes = notes.as_deref().unwrap_or(&[]).to_vec();
        let (alt, shift) = (e.modifiers.alt, e.modifiers.shift);
        let how = match hit {
            Some(Hit::Point(note, k)) if !shift => BendHow::Point { note, k },
            Some(Hit::Line(note, k)) if alt => BendHow::Bow {
                note,
                k,
                was: notes[note].expression.get(dim)[k].curve,
            },
            _ if shift => BendHow::Scale,
            _ if alt && !grid => BendHow::Erase { tick },
            _ => {
                // Over the notes, on the note pressed, or the one whose line it was.
                let lead = match (grid, hit) {
                    (false, _) => None,
                    (true, Some(Hit::Line(i, _) | Hit::Point(i, _))) => on.or(Some(i)),
                    (true, None) => on,
                };
                if grid && lead.is_none() {
                    return false;
                }
                let pitch = lead.map_or(60, |i| notes[i].pitch);
                BendHow::Draw {
                    lead,
                    tick,
                    value: self.bend_value(dim, grid, pitch, p.y),
                }
            }
        };
        // Over the notes, the note pressed is picked (⇧ adds it); pressed among others picked,
        // let go without moving it's picked alone.
        let alone = on.is_some_and(|i| !shift && self.picked.len() > 1 && self.picked.contains(&i));
        if let Some(i) = on
            && !self.picked.contains(&i)
        {
            match shift {
                true => self.picked.push(i),
                false => self.picked = vec![i],
            }
        }
        let held: Vec<usize> = match how {
            BendHow::Point { note, .. } | BendHow::Bow { note, .. } => vec![note],
            BendHow::Scale => self.sounding_at(at, tick),
            _ => Vec::new(),
        };
        self.held = held
            .into_iter()
            .filter_map(|i| notes.get(i).map(|n| (i, n.clone())))
            .collect();
        self.bend_touched.clear();
        self.bend_stroke = None;
        self.begin_drag(
            Drag::Bend {
                at,
                dim,
                grid,
                how,
                from: p,
                moved: false,
                alone,
            },
            cx,
        );
        // A click in the lane puts a point there at once; over the notes, it waits to be dragged.
        if let BendHow::Draw { value, .. } = how
            && !grid
        {
            self.draw_bend(at, dim, None, (tick, value), (tick, value));
        }
        self.bend_readout = match how {
            BendHow::Point { note, k } => notes[note]
                .expression
                .get(dim)
                .get(k)
                .map(|b| (p, say(dim, b.value))),
            _ => None,
        };
        cx.notify();
        true
    }

    /// A stroke from (`tick`, `value`) to the next: across the notes it crosses (or `lead` and
    /// those picked with it), what was there replaced by the line between, a 32nd at a time.
    fn draw_bend(
        &mut self,
        at: At,
        dim: Dim,
        lead: Option<usize>,
        (t0, v0): (i64, f32),
        (t1, v1): (i64, f32),
    ) {
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        let picked = self.picked.clone();
        let step = (PPQ / 32) as i64;
        let Some(notes) = self.session.project.notes_mut(at) else {
            return;
        };
        let crosses = |n: &Note| n.start <= hi && lo < n.start + n.length as i64;
        let targets: Vec<usize> = match lead {
            Some(l) if picked.contains(&l) => picked.clone(),
            Some(l) => vec![l],
            None if picked.iter().any(|&i| notes.get(i).is_some_and(crosses)) => picked.clone(),
            None => (0..notes.len()).collect(),
        };
        for i in targets {
            let Some(n) = notes.get_mut(i).filter(|n| crosses(n)) else {
                continue;
            };
            let (s, e) = (n.start, n.start + n.length as i64);
            let (a, z) = (lo.max(s), hi.min(e));
            let points = n.expression.get_mut(dim);
            points.retain(|b| (b.at as i64) < a - s || (b.at as i64) > z - s);
            let mut t = a;
            loop {
                let f = match t1 == t0 {
                    true => 1.0,
                    false => ((t - t0) as f32 / (t1 - t0) as f32).clamp(0.0, 1.0),
                };
                points.push(Bend::new((t - s) as u64, v0 + (v1 - v0) * f));
                if t >= z {
                    break;
                }
                t = (t + step).min(z);
            }
            points.sort_by_key(|b| b.at);
            if !self.bend_touched.contains(&i) {
                self.bend_touched.push(i);
            }
        }
        self.bend_stroke = Some(match self.bend_stroke {
            Some((a, z)) => (a.min(lo), z.max(hi)),
            None => (lo, hi),
        });
    }

    /// The hand on the notes' expression moved.
    pub(crate) fn drag_expression(&mut self, p: Point<f32>) {
        let Some(Drag::Bend {
            at,
            dim,
            grid,
            how,
            from,
            moved,
            alone,
        }) = self.drag
        else {
            return;
        };
        if !moved && (p.x - from.x).hypot(p.y - from.y) < 3.0 {
            return;
        }
        let start = self.session.project.region(at).start as i64;
        let tick = self.tick_at(p.x) as i64 - start;
        let mut how = how;
        match how {
            BendHow::Point { note, k } => {
                let Some(n) = self
                    .session
                    .project
                    .region(at)
                    .content
                    .notes(0)
                    .and_then(|ns| ns.get(note).cloned())
                else {
                    return;
                };
                let v = self.bend_value(dim, grid, n.pitch, p.y);
                let t = (tick - n.start).clamp(0, n.length as i64) as u64;
                if let Some(points) = self
                    .session
                    .project
                    .notes_mut(at)
                    .and_then(|ns| ns.get_mut(note))
                    .map(|n| n.expression.get_mut(dim))
                    && k < points.len()
                {
                    // Kept between its neighbours in time.
                    let lo = k.checked_sub(1).map_or(0, |j| points[j].at);
                    let hi = points.get(k + 1).map_or(n.length, |b| b.at);
                    points[k].at = t.clamp(lo, hi.max(lo));
                    points[k].value = v;
                }
                self.bend_readout = Some((p, say(dim, v)));
            }
            BendHow::Bow { note, k, was } => {
                let Some((_, n)) = self.held.first().cloned() else {
                    return;
                };
                let points = n.expression.get(dim);
                let (Some(a), Some(b)) = (points.get(k), points.get(k + 1)) else {
                    return;
                };
                // Up bows the line up: a rising one fast then slow, a falling one slow then fast.
                let sign = if b.value >= a.value { -1.0 } else { 1.0 };
                let bow = (was + sign * (from.y - p.y) / 40.0).clamp(-1.0, 1.0);
                if let Some(b) = self
                    .session
                    .project
                    .notes_mut(at)
                    .and_then(|ns| ns.get_mut(note))
                    .and_then(|n| n.expression.get_mut(dim).get_mut(k))
                {
                    b.curve = bow;
                }
                self.bend_readout = Some((p, format!("bow {:+.0}", bow * 100.0)));
            }
            BendHow::Scale => {
                let by = 2f32.powf((from.y - p.y) / 60.0);
                let (lo, hi) = dim.range();
                let held = self.held.clone();
                if let Some(notes) = self.session.project.notes_mut(at) {
                    for (i, was) in held {
                        if let Some(n) = notes.get_mut(i) {
                            *n.expression.get_mut(dim) = was
                                .expression
                                .get(dim)
                                .iter()
                                .map(|b| Bend {
                                    value: (b.value * by).clamp(lo, hi),
                                    ..*b
                                })
                                .collect();
                        }
                    }
                }
                self.bend_readout = Some((p, format!("× {by:.2}")));
            }
            BendHow::Draw {
                lead,
                tick: t0,
                value: v0,
            } => {
                let pitch = lead
                    .and_then(|i| {
                        self.session
                            .project
                            .region(at)
                            .content
                            .notes(0)?
                            .get(i)
                            .map(|n| n.pitch)
                    })
                    .unwrap_or(60);
                let v = self.bend_value(dim, grid, pitch, p.y);
                // Over the notes, the first move draws from where it pressed.
                self.draw_bend(at, dim, lead, (t0, v0), (tick, v));
                how = BendHow::Draw {
                    lead,
                    tick,
                    value: v,
                };
                self.bend_readout = Some((p, say(dim, v)));
            }
            BendHow::Erase { tick: t0 } => {
                let (lo, hi) = (t0.min(tick), t0.max(tick));
                let picked = self.picked.clone();
                if let Some(notes) = self.session.project.notes_mut(at) {
                    for (i, n) in notes.iter_mut().enumerate() {
                        if picked.is_empty() || picked.contains(&i) {
                            let s = n.start;
                            n.expression
                                .get_mut(dim)
                                .retain(|b| !(lo..=hi).contains(&(s + b.at as i64)));
                        }
                    }
                }
                how = BendHow::Erase { tick };
            }
        }
        self.drag = Some(Drag::Bend {
            at,
            dim,
            grid,
            how,
            from,
            moved: true,
            alone,
        });
    }

    /// The hand let go: a line drawn thinned to the points that shape it; nothing changed,
    /// nothing to undo.
    pub(crate) fn end_expression(&mut self, cx: &mut Context<Self>) {
        let Some(Drag::Bend {
            at,
            dim,
            how,
            moved,
            alone,
            ..
        }) = self.drag
        else {
            return;
        };
        self.bend_readout = None;
        if let (BendHow::Draw { lead: Some(i), .. }, false, true) = (how, moved, alone) {
            self.picked = vec![i];
        }
        if let (BendHow::Draw { .. }, true, Some((lo, hi))) = (how, moved, self.bend_stroke) {
            let touched = std::mem::take(&mut self.bend_touched);
            if let Some(notes) = self.session.project.notes_mut(at) {
                for i in touched {
                    if let Some(n) = notes.get_mut(i) {
                        let range = ((lo - n.start).max(0) as u64, (hi - n.start).max(0) as u64);
                        let points = n.expression.get_mut(dim);
                        *points = thin_within(points, range, dim.tolerance());
                    }
                }
            }
        }
        match self.session.unchanged() {
            true => self.session.cancel(),
            false => self.changed(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stroke_thins_where_it_went_and_keeps_the_rest() {
        let b = |at, value| Bend::new(at, value);
        // Points put by hand at 0 and 100, then a straight stroke from 200 to 400.
        let points: Vec<Bend> = [b(0, 0.0), b(100, 1.0)]
            .into_iter()
            .chain((0..=20).map(|k| b(200 + k * 10, 1.0 + k as f32 * 0.1)))
            .collect();
        let thin = thin_within(&points, (200, 400), 0.03);
        assert_eq!(thin, [b(0, 0.0), b(100, 1.0), b(200, 1.0), b(400, 3.0)]);
        // Nothing where the stroke went: as it was.
        assert_eq!(thin_within(&points, (500, 600), 0.03), points);
    }

    #[test]
    fn a_bend_reads_in_semitones_and_the_others_in_percent() {
        assert_eq!(say(Dim::Pitch, 2.0), "+2 st");
        assert_eq!(say(Dim::Pitch, -0.5), "-0.50 st");
        assert_eq!(say(Dim::Press, 0.25), "25%");
        assert!((level_value(Dim::Pitch, lane_level(Dim::Pitch, 7.0)) - 7.0).abs() < 1e-4);
    }
}
