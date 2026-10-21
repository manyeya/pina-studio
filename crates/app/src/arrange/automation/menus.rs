//! What the lanes' menus do: shapes, smoothing, modes.

use super::*;

/// What an automation menu item does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AutoOp {
    /// The lanes: a parameter for one, another under them, one taken away.
    Lane(usize, Param),
    AddLane,
    RemoveLane(usize),
    Mode(AutoMode),
    /// The picked points' lines.
    Straight,
    SlowStart,
    FastStart,
    /// An S: eased out of the point and into the next.
    SCurve,
    Step,
    /// The selection's line (or the lane's) smoothed, over about so many ticks.
    Smooth(u64),
    Delete,
    Copy,
    /// Points copied, at a tick.
    Paste(u64),
    SelectAll,
    Thin,
    Clear,
    /// A shape from tick to tick, a cycle every so many.
    Shape(AutoShape, u64, u64, u64),
    /// A setting automated from now on: its lane shown, a point where it is if it has none.
    Automate(Param),
}

impl Workspace {
    /// An automation menu's item, on a track.
    pub fn auto_op(&mut self, id: u64, op: AutoOp, cx: &mut Context<Self>) {
        let Some(track) = self.session.project.tracks.iter().position(|t| t.id == id) else {
            return;
        };
        let focus = self.auto_focus.filter(|f| f.0 == id).map(|f| f.1);
        let param = focus.unwrap_or_else(|| self.lane_param(track));
        let picked = self.picked_points(track, param);
        let reshape = |ws: &mut Workspace, f: &dyn Fn(&mut AutoPoint)| {
            let points = ws.session.project.tracks[track]
                .automation
                .points_mut(param);
            for &i in &picked {
                if let Some(q) = points.get_mut(i) {
                    f(q);
                }
            }
        };
        match op {
            AutoOp::Lane(k, p) => {
                self.set_lane(track, k, p);
                self.auto_focus = Some((id, p));
            }
            AutoOp::AddLane => self.add_lane(track),
            AutoOp::Automate(param) => {
                if self.session.project.tracks[track]
                    .automation
                    .points(param)
                    .is_empty()
                {
                    let value = self.own_value(track, param);
                    self.edit(cx, |ws| {
                        ws.session.project.tracks[track]
                            .automation
                            .points_mut(param)
                            .push(AutoPoint::new(0, value));
                    });
                }
                self.select_track(track);
                self.show_lane(track, param);
                self.show_automation = true;
            }
            AutoOp::RemoveLane(k) => self.remove_lane(track, k),
            AutoOp::Mode(mode) => self.edit(cx, |ws| {
                ws.session.project.tracks[track].automation.mode = mode;
                ws.writing.retain(|k, _| k.0 != id);
                ws.recorded.retain(|k, _| k.0 != id);
                ws.taken.retain(|k, _| k.0 != id);
            }),
            AutoOp::Straight => self.edit(cx, |ws| {
                reshape(ws, &|q| (q.curve, q.step, q.ease) = (0.0, false, false))
            }),
            AutoOp::SlowStart => self.edit(cx, |ws| {
                reshape(ws, &|q| (q.curve, q.step, q.ease) = (0.6, false, false))
            }),
            AutoOp::FastStart => self.edit(cx, |ws| {
                reshape(ws, &|q| (q.curve, q.step, q.ease) = (-0.6, false, false))
            }),
            AutoOp::SCurve => self.edit(cx, |ws| {
                reshape(ws, &|q| (q.curve, q.step, q.ease) = (0.0, false, true))
            }),
            AutoOp::Step => self.edit(cx, |ws| reshape(ws, &|q| (q.step, q.ease) = (true, false))),
            AutoOp::Smooth(width) => {
                // The selection's span; one point, from the one before it to the one after; or
                // all of it.
                let points = self.session.project.tracks[track].automation.points(param);
                let ticks: Vec<u64> = picked
                    .iter()
                    .filter_map(|&i| points.get(i).map(|q| q.tick))
                    .collect();
                let (from, to) = match (ticks.iter().min(), ticks.iter().max()) {
                    (Some(&a), Some(&b)) if b > a => (a, b),
                    (Some(_), _) => {
                        let i = picked[0];
                        let last = points.len().saturating_sub(1);
                        (
                            points[i.saturating_sub(1)].tick,
                            points[(i + 1).min(last)].tick,
                        )
                    }
                    _ => match (points.first(), points.last()) {
                        (Some(a), Some(b)) => (a.tick, b.tick),
                        _ => (0, 0),
                    },
                };
                if param.steps() || to <= from {
                    self.status = Some(match param.steps() {
                        true => "Mute is all steps: nothing to smooth.".into(),
                        false => "Nothing there to smooth.".into(),
                    });
                } else {
                    self.edit(cx, |ws| {
                        let a = &mut ws.session.project.tracks[track].automation;
                        a.smooth(param, from, to, width);
                        let n = a.points(param).len();
                        ws.status = Some(format!("Smoothed: {n} points"));
                        // What was smoothed stays selected, to smooth again.
                        let picked = a
                            .points(param)
                            .iter()
                            .enumerate()
                            .filter(|(_, q)| q.tick >= from && q.tick <= to)
                            .map(|(i, _)| i)
                            .collect();
                        ws.auto_picked = Some(Picked {
                            track: id,
                            param,
                            points: picked,
                        });
                    });
                }
            }
            AutoOp::Delete => self.delete_points(track, param, picked, cx),
            AutoOp::Copy => drop(self.copy_picked(cx)),
            AutoOp::Paste(tick) => drop(self.paste_points_at(tick, cx)),
            AutoOp::SelectAll => {
                let n = self.session.project.tracks[track]
                    .automation
                    .points(param)
                    .len();
                self.pick_points(track, param, (0..n).collect());
            }
            AutoOp::Thin => self.edit(cx, |ws| {
                let a = &mut ws.session.project.tracks[track].automation;
                let before = a.points(param).len();
                a.thin(param, 0.01);
                let after = a.points(param).len();
                ws.status = Some(format!("{} points taken out", before - after));
                ws.auto_picked = None;
            }),
            AutoOp::Clear => self.edit(cx, |ws| {
                let a = &mut ws.session.project.tracks[track].automation;
                a.points_mut(param).clear();
                a.tidy();
                ws.auto_picked = None;
            }),
            AutoOp::Shape(shape, from, to, every) => {
                let (low, high) = self.shape_range(track, param, shape, from);
                let name = self.param_name(track, param);
                self.edit(cx, |ws| {
                    let a = &mut ws.session.project.tracks[track].automation;
                    a.shape(param, shape, from, to, every, low, high);
                    a.name(param, &name);
                    ws.auto_picked = None;
                });
            }
        }
        cx.notify();
    }

    /// Where a shape swings, by what it moves: a level from where it is down 12 dB (a ramp
    /// from silence), a pan most of the way across, a plugin's most of its range.
    pub(crate) fn shape_range(
        &self,
        track: usize,
        param: Param,
        shape: AutoShape,
        at: u64,
    ) -> (f32, f32) {
        let ramp = matches!(shape, AutoShape::RampUp | AutoShape::RampDown);
        match param {
            Param::Volume | Param::Send(_) => {
                let top = self
                    .automated(track, param)
                    .and_then(|_| {
                        let t = &self.session.project.tracks[track];
                        value_at(t.automation.points(param), at as f64)
                    })
                    .unwrap_or_else(|| self.own_value(track, param))
                    .clamp(-30.0, 6.0);
                (if ramp { SILENT_DB } else { top - 12.0 }, top)
            }
            Param::Pan => match ramp {
                true => (-1.0, 1.0),
                false => (-0.8, 0.8),
            },
            Param::Mute => (0.0, 1.0),
            Param::Plugin(..) => match ramp {
                true => (0.0, 1.0),
                false => (0.1, 0.9),
            },
            // A sweep: from 200 Hz to 8 kHz; a gain either way; a width from wide to narrow.
            Param::Eq(_, EqField::Freq) => (200.0, 8_000.0),
            Param::Eq(_, EqField::Gain) => match ramp {
                true => (0.0, 6.0),
                false => (-6.0, 6.0),
            },
            Param::Eq(_, EqField::Q) => (0.5, 4.0),
            Param::Mod(..) => match ramp {
                true => (param.from_unit(0.0), param.from_unit(1.0)),
                false => (param.from_unit(0.1), param.from_unit(0.9)),
            },
        }
    }

    /// Right-click on a lane: shapes to draw over the selection, the cycle or the bar
    /// clicked, a cycle every grid step; and the lane's own items.
    pub fn auto_lane_menu(
        &mut self,
        track: usize,
        k: usize,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        let id = self.session.project.tracks[track].id;
        self.auto_focus = Some((id, param));
        let tick = self.tick_at(f32::from(at.x)).max(0.0) as u64;
        let p = &self.session.project;
        let points = p.tracks[track].automation.points(param);
        let picked: Vec<u64> = self
            .picked_points(track, param)
            .iter()
            .filter_map(|&i| points.get(i).map(|q| q.tick))
            .collect();
        let bar = p.bar();
        let (from, to, over) = match (picked.iter().min(), picked.iter().max(), p.cycle) {
            (Some(&a), Some(&b), _) if b > a => (a, b, "the selection".to_string()),
            (_, _, Some(c)) if !c.off && c.start <= tick && tick < c.end => {
                (c.start, c.end, "the cycle".to_string())
            }
            _ => {
                let start = tick / bar * bar;
                (start, start + bar, format!("bar {}", start / bar + 1))
            }
        };
        let every = match self.grid() {
            1 => p.beat(),
            g => g,
        };
        let rate = match every {
            e if e == bar => "bar".to_string(),
            PPQ_BEAT => "beat".to_string(),
            e if e % PPQ_BEAT == 0 => format!("{} beats", e / PPQ_BEAT),
            e => format!("1/{}", PPQ_BEAT * 4 / e.max(1)),
        };
        let mut items: Vec<(String, MenuAction)> = vec![(
            format!("Shapes over {over}, a cycle every {rate}"),
            MenuAction::Nothing,
        )];
        let shapes = match param.steps() {
            true => vec![AutoShape::Square, AutoShape::Random],
            false => AutoShape::ALL.to_vec(),
        };
        for s in shapes {
            items.push((
                s.name().to_string(),
                MenuAction::Auto(id, AutoOp::Shape(s, from, to, every)),
            ));
        }
        items.push((String::new(), MenuAction::Nothing));
        if self.auto_clip.is_some() {
            let snapped = self.snap(tick as f64);
            items.push((
                "Paste\t⌘V".into(),
                MenuAction::Auto(id, AutoOp::Paste(snapped)),
            ));
        }
        if !param.steps() {
            let what = match picked.len() > 1 {
                true => "Selection",
                false => "Lane",
            };
            items.push((
                format!("Smooth {what}"),
                MenuAction::Auto(id, AutoOp::Smooth(PPQ / 2)),
            ));
            items.push((
                format!("Smooth {what} More"),
                MenuAction::Auto(id, AutoOp::Smooth(PPQ * 2)),
            ));
        }
        items.push(("Select All".into(), MenuAction::Auto(id, AutoOp::SelectAll)));
        items.push(("Thin Points".into(), MenuAction::Auto(id, AutoOp::Thin)));
        items.push(("Clear Lane".into(), MenuAction::Auto(id, AutoOp::Clear)));
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// Right-click on a point: its line's shape (and the selection's), or out it goes.
    pub fn point_menu(
        &mut self,
        track: usize,
        k: usize,
        index: usize,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let param = self.lane_at(track, k);
        let id = self.session.project.tracks[track].id;
        let mut picked = self.picked_points(track, param);
        if !picked.contains(&index) {
            picked = vec![index];
        }
        self.pick_points(track, param, picked);
        let q = self.session.project.tracks[track].automation.points(param)[index];
        let tick = |on: bool| if on { "✓ " } else { "" };
        let mut items = Vec::new();
        if !param.steps() {
            items.extend([
                (
                    format!("{}Straight", tick(!q.shaped())),
                    MenuAction::Auto(id, AutoOp::Straight),
                ),
                (
                    format!("{}S-Curve", tick(!q.step && q.ease)),
                    MenuAction::Auto(id, AutoOp::SCurve),
                ),
                (
                    format!(
                        "{}Curve: Slow Start",
                        tick(!q.step && !q.ease && q.curve > 0.0)
                    ),
                    MenuAction::Auto(id, AutoOp::SlowStart),
                ),
                (
                    format!(
                        "{}Curve: Fast Start",
                        tick(!q.step && !q.ease && q.curve < 0.0)
                    ),
                    MenuAction::Auto(id, AutoOp::FastStart),
                ),
                (
                    format!("{}Step", tick(q.step)),
                    MenuAction::Auto(id, AutoOp::Step),
                ),
                (String::new(), MenuAction::Nothing),
                (
                    "Smooth".into(),
                    MenuAction::Auto(id, AutoOp::Smooth(PPQ / 2)),
                ),
                (
                    "Smooth More".into(),
                    MenuAction::Auto(id, AutoOp::Smooth(PPQ * 2)),
                ),
                (String::new(), MenuAction::Nothing),
            ]);
        }
        items.extend([
            ("Copy\t⌘C".into(), MenuAction::Auto(id, AutoOp::Copy)),
            ("Delete\t⌫".into(), MenuAction::Auto(id, AutoOp::Delete)),
        ]);
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// The modes, for a track's lane or strip.
    pub fn mode_menu(&mut self, track: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        let t = &self.session.project.tracks[track];
        let (id, now) = (t.id, t.automation.mode);
        let items = AutoMode::ALL
            .iter()
            .map(|&m| {
                let tick = if m == now { "✓ " } else { "" };
                (
                    format!("{tick}{}\t{}", m.name(), mode_what(m)),
                    MenuAction::Auto(id, AutoOp::Mode(m)),
                )
            })
            .collect();
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }
}
