//! What moving does, for each kind of drag.

use super::*;

impl Workspace {
    pub(crate) fn drag_to(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        let ppt = self.px_per_tick();
        if let Some(
            Drag::Region { moved, .. }
            | Drag::Notes { moved, .. }
            | Drag::Auto { moved, .. }
            | Drag::AutoLine { moved, .. }
            | Drag::Cycle { moved, .. }
            | Drag::Marker { moved, .. },
        ) = &mut self.drag
        {
            *moved = true;
        }
        match self.drag {
            Some(Drag::Region {
                at,
                edge,
                from,
                start,
                end,
                track,
                ..
            }) => {
                let dt = ((p.x - from.x) / ppt) as f64;
                match edge {
                    Edge::Move => {
                        let start = self.snap(start as f64 + dt);
                        // To the track under the pointer (its own, above the first or past the
                        // last), however tall each is.
                        let _ = (from, track);
                        let rel = p.y - f32::from(self.lanes_bounds.get().origin.y);
                        let last = self.session.project.tracks.len().saturating_sub(1);
                        let to = self
                            .track_at_y(rel)
                            .unwrap_or(if rel < 0.0 { 0 } else { last });
                        let now = self.session.project.move_region(at, to, start);
                        if let Some(Drag::Region { at, .. }) = &mut self.drag {
                            *at = now;
                        }
                        self.region = Some(now);
                        self.select_track(now.0);
                    }
                    Edge::Start => {
                        let start = self.snap(start as f64 + dt);
                        self.session.project.trim_start(at, start);
                    }
                    Edge::End => {
                        let end = self.snap(end as f64 + dt);
                        self.session.project.trim_end(at, end);
                    }
                    // MIDI and patterns stretch from how they were, so rounding doesn't build up.
                    Edge::Stretch if self.stretching.is_some() => {
                        let length = self.snap(end as f64 + dt).max(start + 1) - start;
                        let ratio = length as f64 / (end - start).max(1) as f64;
                        let made = self.stretching.as_ref().and_then(|c| c.stretched(ratio));
                        if let Some(content) = made {
                            let r = &mut self.session.project.tracks[at.0].regions[at.1];
                            r.content = content;
                            r.length = length;
                            r.link = None;
                        }
                        self.status = Some(format!("Time stretch ×{ratio:.2}"));
                    }
                    // Stretching audio is slow, so the engine hears of it when the drag ends.
                    Edge::Stretch => {
                        let length = self.snap(end as f64 + dt).max(start + 1) - start;
                        let _ = self.session.project.flex(at, length);
                        let ratio = length as f64 / (end - start).max(1) as f64;
                        self.status = Some(format!("Flex ×{ratio:.2}"));
                        cx.notify();
                        return;
                    }
                    // Loops to the grid line nearest the pointer; back inside the region, none.
                    Edge::Loop => {
                        let to = self.snap(self.tick_at(p.x));
                        let r = &mut self.session.project.tracks[at.0].regions[at.1];
                        r.looped = to > r.start + r.length;
                        r.loop_length = r.looped.then(|| to - r.start);
                    }
                }
            }
            Some(Drag::Fader {
                channel,
                y,
                pos,
                height,
            }) => {
                let db = strip::fader_db((pos - (p.y - y) / height).clamp(0.0, 1.0));
                self.set_volume(channel, db);
                if let Channel::Track(t) = channel {
                    self.moved(t, Param::Volume, db);
                }
            }
            Some(Drag::Pan { channel, x, y, pan }) => {
                let (dx, dy) = (p.x - x, y - p.y);
                let by = if dx.abs() > dy.abs() { dx } else { dy };
                let mut to = (pan + by * if self.fine { 0.002 } else { 0.01 }).clamp(-1.0, 1.0);
                // A catch at the middle, as it's crossed.
                if pan != 0.0 && to != 0.0 && pan.signum() != to.signum() {
                    to = 0.0;
                }
                if let Some(Drag::Pan { x, y, pan, .. }) = &mut self.drag {
                    (*x, *y, *pan) = (p.x, p.y, to);
                }
                let pan = to;
                self.set_pan(channel, pan);
                if let Channel::Track(t) = channel {
                    self.moved(t, Param::Pan, pan);
                }
            }
            Some(Drag::Send {
                track,
                index,
                y,
                level,
            }) => {
                let db = (level - (p.y - y) / 2.0).clamp(strip::SILENT, 6.0);
                self.session.project.tracks[track].sends[index].level_db = db;
                let bus = self.session.project.tracks[track].sends[index].bus;
                self.moved(track, Param::Send(bus), db);
            }
            Some(Drag::Tempo { y, bpm }) => {
                let to = (bpm + ((y - p.y) / 4.0) as f64).round().clamp(20.0, 400.0);
                if to != self.session.project.tempo {
                    self.session.project.set_tempo(to);
                }
            }
            Some(Drag::Locate) => self.locate_at(p.x, cx),
            Some(Drag::Notes { .. }) => self.drag_notes(p),
            Some(Drag::Marquee { .. }) => {
                self.drag_marquee(p);
                cx.notify();
                return;
            }
            Some(Drag::Paint { .. }) => self.drag_paint(p),
            Some(Drag::Velocity { at, y }) => {
                let dv = -(p.y - y) * 127.0 / crate::editors::pianoroll::VELOCITY_H;
                let held = self.held.clone();
                if let Some(notes) = self.session.project.notes_mut(at) {
                    for (i, was) in held {
                        if let Some(n) = notes.get_mut(i) {
                            n.velocity = (was.velocity as f32 + dv).round().clamp(1.0, 127.0) as u8;
                        }
                    }
                }
            }
            Some(Drag::VelocityLine { .. }) => self.drag_velocity_line(p),
            Some(Drag::Control { .. }) => self.drag_control(p),
            Some(Drag::Mark { .. }) => self.drag_mark(p),
            Some(Drag::Width { side, x, width }) => {
                let w = width + (p.x - x);
                match side {
                    true => self.inspector_w = w.clamp(220.0, 520.0),
                    false => self.header_w = w.clamp(280.0, 520.0),
                }
                cx.notify();
                return;
            }
            Some(Drag::LaneHeight { track, y, height }) => {
                if let Some(t) = self.session.project.tracks.get_mut(track) {
                    t.height = Some((height + (p.y - y)).clamp(MIN_LANE_H, MAX_LANE_H).round());
                }
                cx.notify();
                return;
            }
            Some(Drag::HFader {
                track,
                x,
                pos,
                width,
            }) => {
                let fine = if self.fine { 0.2 } else { 1.0 };
                let pos = (pos + (p.x - x) / width.max(1.0) * fine).clamp(0.0, 1.0);
                if let Some(Drag::HFader { x, pos: was, .. }) = &mut self.drag {
                    (*x, *was) = (p.x, pos);
                }
                let db = strip::fader_db(pos);
                self.set_volume(Channel::Track(track), db);
                self.moved(track, Param::Volume, db);
            }
            Some(Drag::Orbit { x, y, yaw, pitch }) => {
                self.orbit = (
                    yaw + (p.x - x) * 0.4,
                    (pitch + (p.y - y) * 0.3).clamp(4.0, 88.0),
                );
                cx.notify();
                return;
            }
            Some(Drag::Source { track, height }) => self.drag_source(track, height, p),
            Some(Drag::Modulator { .. }) => {
                self.mod_drag_at = Some(p);
                cx.notify();
                return;
            }
            Some(Drag::ModDepth {
                track,
                index,
                target,
                x,
                y,
                depth,
            }) => self.drag_depth((track, index, target), (x, y), depth, p),
            Some(Drag::ModValue {
                track,
                index,
                field,
                y,
                value,
            }) => {
                let fine = if self.fine { 0.2 } else { 1.0 };
                self.set_mod_field(track, index, field, value + (y - p.y) / 150.0 * fine);
                // Turned while its automation records, it's recorded.
                if let Some(setting) = crate::editors::modulation::setting_of(field) {
                    let param = pina_engine::song::project::Param::Mod(index as u8, setting);
                    let now = self.own_value(track, param);
                    self.moved(track, param, now);
                }
            }
            Some(Drag::ModSteps { track, index }) => self.paint_mod_step(track, index, p),
            Some(Drag::ZoomSpan) => {
                if let Some((_, b)) = &mut self.zoom_span {
                    *b = p.x;
                }
                cx.notify();
                return;
            }
            Some(Drag::SoloHold) => return,
            Some(Drag::Fade { at, out }) => self.drag_fade(at, out, p),
            Some(Drag::Raise { track, y, height }) => {
                self.raise_source(track, height + (y - p.y) / 80.0)
            }
            Some(Drag::Spread {
                track,
                x,
                y,
                spread,
            }) => {
                let (dx, dy) = (p.x - x, y - p.y);
                let by = if dx.abs() > dy.abs() { dx } else { dy };
                let to = (spread + by * if self.fine { 0.002 } else { 0.01 }).clamp(-1.0, 1.0);
                if let Some(Drag::Spread { x, y, spread, .. }) = &mut self.drag {
                    (*x, *y, *spread) = (p.x, p.y, to);
                }
                if let Some(t) = self.session.project.tracks.get_mut(track) {
                    t.spread = Some(to);
                }
            }
            Some(Drag::StepSeq(_)) => return self.steps_drag_to(p, cx),
            Some(Drag::Auto {
                track,
                param,
                k,
                index,
                ..
            }) => self.drag_points(track, param, k, index, p),
            Some(Drag::AutoLine {
                track, param, k, y, ..
            }) => self.drag_line(track, param, k, y, p),
            Some(Drag::AutoBend {
                track,
                param,
                index,
                y,
                curve,
            }) => self.drag_bend(track, param, index, y, curve, p),
            Some(Drag::AutoMarquee { .. }) => {
                if let Some(Drag::AutoMarquee { to, .. }) = &mut self.drag {
                    *to = (p.x, p.y);
                }
                cx.notify();
                return;
            }
            Some(Drag::AutoDraw { track, param, last }) => self.drag_draw(track, param, last, p),
            Some(Drag::Comp { at, .. }) => {
                let tick = self.region_tick(at, p.x);
                if let Some(Drag::Comp { to, .. }) = &mut self.drag {
                    *to = tick;
                }
                cx.notify();
                return;
            }
            Some(Drag::Cycle { anchor, .. }) => {
                let here = self.snap(self.tick_at(p.x));
                let (start, end) = (anchor.min(here), anchor.max(here));
                if end > start {
                    self.session.project.cycle = Some(Cycle {
                        start,
                        end,
                        off: false,
                    });
                    // A stretch swept out is what plays: the song, not one region's pattern.
                    self.pattern_mode = false;
                }
            }
            Some(Drag::Marker { index, x, tick, .. }) => {
                let to = self.snap(tick as f64 + ((p.x - x) / ppt) as f64);
                let markers = &mut self.session.project.markers;
                if let Some(m) = markers.get_mut(index) {
                    m.tick = to;
                }
            }
            Some(Drag::Knob { target, y, value }) => {
                let v = (value - (p.y - y) / 150.0).clamp(0.0, 1.0);
                self.set_target(target, v);
                if let Some((t, param)) = self.target_param(target) {
                    self.moved(t, param, v);
                }
            }
            Some(Drag::Slider {
                slider,
                left,
                width,
            }) => {
                let v = ((p.x - left) / width.max(1.0)).clamp(0.0, 1.0);
                self.set_slider(slider, v);
                if let Slider::Send { track, index } = slider {
                    let s = &self.session.project.tracks[track].sends[index];
                    let (bus, db) = (s.bus, s.level_db);
                    self.moved(track, Param::Send(bus), db);
                }
                if !matches!(slider, Slider::Send { .. }) {
                    cx.notify();
                    return;
                }
            }
            Some(Drag::EqNode {
                id,
                from,
                q,
                alt,
                axis,
                ..
            }) => {
                let axis = self.drag_eq_node(id, from, q, alt, axis, p);
                if let Some(Drag::EqNode { axis: a, moved, .. }) = &mut self.drag {
                    *a = axis;
                    *moved |= (p.x - from.x).hypot(p.y - from.y) > 2.0;
                }
            }
            Some(Drag::EqPress {
                from,
                on_curve,
                alt,
                ..
            }) => {
                if let Some(Drag::EqPress { moved, .. }) = &mut self.drag {
                    *moved |= (p.x - from.x).hypot(p.y - from.y) > 3.0;
                }
                self.drag_eq_press(from, on_curve, alt, p, cx);
                cx.notify();
                return;
            }
            Some(Drag::EqSolo { id, from, freq }) => self.drag_eq_solo(id, from, freq, p),
            Some(Drag::EqScale { from, span }) => {
                self.drag_eq_scale(from, span, p);
                cx.notify();
                return;
            }
            Some(Drag::EqSketch) => {
                self.drag_eq_sketch(p);
                cx.notify();
                return;
            }
            Some(Drag::EqKnob {
                id,
                knob,
                from,
                start,
                ..
            }) => self.drag_eq_knob(id, knob, from, start, p.y),
            Some(Drag::EqListBand { id, frame }) => self.drag_list_band(id, frame, p),
            Some(Drag::EqWindow {
                from,
                start,
                resize,
            }) => {
                self.drag_eq_window(from, start, resize, p);
                cx.notify();
                return;
            }
            Some(Drag::Editor { y, height }) => {
                self.editor_h = height - (p.y - y);
                self.editor_h = self.editor_height();
                cx.notify();
                return;
            }
            Some(Drag::Panel { x, width }) => {
                self.panel_w = width - (p.x - x);
                self.panel_w = self.panel_width();
                cx.notify();
                return;
            }
            Some(Drag::Sample { .. } | Drag::MapBox { .. }) => {
                self.drag_sample(p, cx);
                return;
            }
            Some(Drag::Device { .. }) => {
                if let Some(Drag::Device {
                    from, at, moved, ..
                }) = &mut self.drag
                {
                    *at = p;
                    *moved |= (p.x - from.x).hypot(p.y - from.y) > 4.0;
                }
                cx.notify();
                return;
            }
            Some(Drag::Warp { at, from }) => self.drag_warp(at, from, p.x),
            Some(Drag::Key { .. }) | None => return,
        }
        self.changed(cx);
    }

    /// The automatable parameter a knob turns, if it is one: a track and its parameter.
    pub(crate) fn target_param(&self, target: Target) -> Option<(usize, Param)> {
        let Target::Param { track, param } = target else {
            return None;
        };
        let t = self
            .session
            .project
            .tracks
            .iter()
            .position(|t| t.id == track)?;
        Some((t, Param::Plugin(Which::Instrument, param)))
    }
}
