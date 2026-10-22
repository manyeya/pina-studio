//! What letting go does, for each kind of drag.

use super::*;

impl Workspace {
    pub(crate) fn end_drag(&mut self, cx: &mut Context<Self>) {
        // With "Automation moves with regions", a region moved takes what's under it along.
        if self.settings.carry_automation
            && let Some(Drag::Region {
                at,
                edge: Edge::Move,
                start,
                end,
                track,
                moved: true,
                copy,
                ..
            }) = self.drag
            && let Some(r) = self
                .session
                .project
                .tracks
                .get(at.0)
                .and_then(|t| t.regions.get(at.1))
        {
            let to = (at.0, r.start);
            self.carry_automation((track, start, end), to, copy);
            self.changed(cx);
        }
        match self.drag {
            Some(Drag::ZoomSpan) => self.zoom_release(cx),
            Some(Drag::SoloHold) => self.solo_release(cx),
            // A dot or knob clicked, not moved: no edit to undo; a dot's click may do something.
            Some(
                d @ (Drag::EqNode { .. }
                | Drag::EqKnob { .. }
                | Drag::EqSolo { .. }
                | Drag::EqListBand { .. }),
            ) => {
                let still = self.session.unchanged();
                if still {
                    self.session.cancel();
                }
                // A slope clicked rather than dragged: its menu.
                if let Drag::EqKnob {
                    id,
                    knob: crate::mix::eq::EqKnob::Slope,
                    from,
                    x,
                    ..
                } = d
                    && still
                {
                    self.drag = None;
                    self.eq_slope_menu(id, gpui::point(px(x), px(from)), cx);
                }
                if let Some(v) = self.eq_view.as_mut() {
                    v.solo = None;
                    v.grabbing = None;
                }
                self.eq_let_go();
                if let Drag::EqNode {
                    id,
                    alt,
                    moved: false,
                    deselect,
                    ..
                } = d
                {
                    self.drag = None;
                    self.release_eq_node(id, alt, deselect, cx);
                }
            }
            Some(Drag::EqPress {
                from, moved, alt, ..
            }) => {
                self.drag = None;
                self.release_eq_press(from, moved, alt, cx);
            }
            Some(Drag::EqSketch) => {
                self.drag = None;
                self.finish_sketch(cx);
            }
            _ => {}
        }
        if let Some(Drag::Modulator { track, index }) = self.drag
            && let Some(at) = self.mod_drag_at.take()
        {
            self.drop_modulator(track, index, at, cx);
        }
        self.steps_release();
        // A lane made taller is the song's to keep, not an undo step.
        if let Some(Drag::LaneHeight { .. }) = self.drag {
            self.session.edited = true;
        }
        // An option-click that didn't drag leaves no copy behind.
        if let Some(Drag::Region {
            at,
            moved: false,
            copy: true,
            ..
        }) = self.drag
        {
            self.session.project.tracks[at.0].regions.remove(at.1);
            self.region = None;
            self.changed(cx);
        }
        // Letting go of a control ends a Touch pass.
        let released = match self.drag {
            Some(Drag::Fader {
                channel: Channel::Track(t),
                ..
            })
            | Some(Drag::HFader { track: t, .. }) => Some((t, Param::Volume)),
            Some(Drag::Pan {
                channel: Channel::Track(t),
                ..
            }) => Some((t, Param::Pan)),
            Some(Drag::Knob { target, .. }) => self.target_param(target),
            Some(Drag::Send { track, index, .. })
            | Some(Drag::Slider {
                slider: Slider::Send { track, index },
                ..
            }) => {
                let bus = self.session.project.tracks[track].sends[index].bus;
                Some((track, Param::Send(bus)))
            }
            Some(Drag::ModValue {
                track,
                index,
                field,
                ..
            }) => crate::editors::modulation::setting_of(field)
                .map(|s| (track, Param::Mod(index as u8, s))),
            _ => None,
        };
        // A hand on a control lets go with the mouse: the engine hears its automation again.
        let held = !self.touches.is_empty()
            || self
                .taken
                .values()
                .any(|l| *l == crate::arrange::automation::Live::Held);
        if let Some((t, param)) = released {
            self.let_go(t, param);
        }
        if held {
            self.let_go_all();
            self.changed(cx);
        }
        match self.drag {
            Some(Drag::AutoMarquee {
                track,
                k,
                from,
                to,
                erase,
                free,
            }) => self.marquee_done(track, k, from, to, erase, free, cx),
            Some(Drag::AutoDraw { track, param, .. }) => {
                self.draw_done(track, param);
                self.changed(cx);
            }
            _ => {}
        }
        if let Some(
            Drag::Region { moved: false, .. }
            | Drag::Notes { moved: false, .. }
            | Drag::Auto { moved: false, .. }
            | Drag::AutoLine { moved: false, .. }
            | Drag::Marker { moved: false, .. },
        ) = self.drag
        {
            self.session.cancel()
        }
        if let Some(Drag::Marker {
            index,
            moved: false,
            ..
        }) = self.drag
            && let Some(m) = self.session.project.markers.get(index)
        {
            let at = self.session.project.samples(m.tick, self.player.rate());
            self.player.locate(at);
        }
        if let Some(Drag::Marker { moved: true, .. }) = self.drag {
            self.session.project.markers.sort_by_key(|m| m.tick);
        }
        // A click in the cycle strip, without sweeping, turns the cycle on or off.
        if let Some(Drag::Cycle {
            anchor,
            moved: false,
        }) = self.drag
        {
            let bar = self.session.project.bar();
            let cycle = &mut self.session.project.cycle;
            match cycle {
                Some(c) => c.off = !c.off,
                None => {
                    let start = anchor / bar * bar;
                    *cycle = Some(Cycle {
                        start,
                        end: start + bar * 4,
                        off: false,
                    });
                }
            }
            self.changed(cx);
        }
        if let Some(Drag::Comp { at, take, from, to }) = self.drag {
            self.drag = None;
            self.finish_comp(at, take, from, to, cx);
        }
        if let Some(Drag::Region {
            edge: Edge::Stretch,
            moved,
            ..
        }) = self.drag
        {
            self.stretching = None;
            if moved {
                self.changed(cx);
            }
        }
        if let Some(Drag::Sample { .. } | Drag::MapBox { .. }) = self.drag {
            let drag = self.drag.take();
            self.drop_sample(drag, cx);
        }
        if let Some(Drag::Device { .. }) = self.drag {
            let drag = self.drag.take();
            self.drop_device(drag, cx);
        }
        // Release whatever note the drag was sounding.
        match self.drag {
            Some(Drag::Key { track, pitch }) => self.player.preview(track, pitch, 0),
            Some(Drag::Notes { at, lead, .. }) => {
                let track = self.session.project.tracks[at.0].id;
                if let Some(n) = self.session.project.notes_mut(at).and_then(|n| n.get(lead)) {
                    let pitch = n.pitch;
                    self.player.preview(track, pitch, 0);
                }
            }
            Some(Drag::Paint { at, .. }) => {
                let track = self.session.project.tracks[at.0].id;
                for pitch in std::mem::take(&mut self.painting) {
                    self.player.preview(track, pitch, 0);
                }
            }
            _ => {}
        }
        self.drag = None;
        cx.notify();
    }
}
