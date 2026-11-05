//! What letting go does, for each kind of drag.

use super::*;

impl Workspace {
    pub(crate) fn end_drag(&mut self, cx: &mut Context<Self>) {
        // A device let go on a card, a Browser row, a key or a collection is theirs to take:
        // their mouse-ups come after this one. What none took lands on the tracks after them.
        if let Some(Drag::Device { .. }) = self.drag {
            let this = cx.entity().downgrade();
            cx.defer(move |cx| {
                this.update(cx, |ws, cx| {
                    let drag = ws.drag.take();
                    ws.drop_device(drag, cx);
                    cx.notify();
                })
                .ok();
            });
            return;
        }
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
        // The drag mode: what's under the regions let go cut back, or their tracks closed up.
        if let Some(Drag::Region {
            edge,
            start,
            track,
            moved: true,
            ..
        }) = self.drag
            && self.drag_mode != DragMode::Overlap
        {
            self.settle_regions(edge == Edge::Move, (track, start));
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
            Some(Drag::Effect { .. }) => self.release_effect(cx),
            _ => {}
        }
        if let Some(Drag::Modulator { track, index }) = self.drag
            && let Some(at) = self.mod_drag_at.take()
        {
            self.drop_modulator(track, index, at, cx);
        }
        if let Some(Drag::PolySource { track, source }) = self.drag
            && let Some(at) = self.mod_drag_at.take()
        {
            self.drop_poly_source(track, source, at, cx);
        }
        match self.drag {
            Some(Drag::PolySource { .. }) => {
                if let Some(v) = self.poly_view.as_mut() {
                    v.cord = None;
                }
            }
            Some(Drag::PolyKeys) => self.poly_lift(),
            // A bend wheel springs back; the mod wheel stays where it's left.
            Some(Drag::PolyWheel {
                track, bend: true, ..
            }) => self.set_wheel(track, true, 0.0),
            _ => {}
        }
        self.steps_release();
        // A lane made taller is the song's to keep, not an undo step.
        if let Some(Drag::LaneHeight { .. } | Drag::StackHeight { .. }) = self.drag {
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
            | Some(Drag::HFader {
                channel: Channel::Track(t),
                ..
            }) => Some((t, Param::Volume)),
            Some(Drag::Pan {
                channel: Channel::Track(t),
                ..
            }) => Some((t, Param::Pan)),
            Some(Drag::Knob { target, .. } | Drag::PolySlider { target, .. }) => {
                self.target_param(target)
            }
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
            | Drag::Marker { moved: false, .. }
            | Drag::Tempo { moved: false, .. }
            | Drag::TempoPoint { moved: false, .. }
            | Drag::TempoLine { moved: false, .. },
        ) = self.drag
        {
            self.session.cancel()
        }
        // A box let go: the regions it touches, picked.
        if let Some(Drag::RegionBox {
            from,
            to,
            moved: true,
        }) = self.drag
        {
            self.box_pick(from, to);
        }
        // A click on the tempo display, not a drag: its menu.
        if let Some(Drag::Tempo {
            moved: false, x, y, ..
        }) = self.drag
        {
            self.menu = Some(self.tempo_menu(gpui::point(px(x), px(y + 14.))));
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
        if let Some(Drag::Marker {
            moved: true,
            how: MarkerHow::Marker,
            ..
        }) = self.drag
        {
            self.session.project.markers.sort_by_key(|m| m.tick);
        }
        // A section let go: it goes there with everything in it, or nothing changed.
        if let Some(Drag::Marker {
            index,
            at,
            how: how @ (MarkerHow::Section | MarkerHow::Copy),
            moved: true,
            ..
        }) = self.drag
        {
            match self.drop_section(index, at, how) {
                true => self.changed(cx),
                false => self.session.cancel(),
            }
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
        if let Some(Drag::Pad {
            note, at, moved, ..
        }) = self.drag
        {
            self.drop_pad(note, at, moved, cx);
        }
        if let Some(Drag::Sample { .. } | Drag::MapBox { .. }) = self.drag {
            let drag = self.drag.take();
            self.drop_sample(drag, cx);
        }
        if let Some(Drag::Midi { .. }) = self.drag {
            let drag = self.drag.take();
            self.drop_midi(drag, cx);
        }
        if let Some(Drag::ChatMidi { .. }) = self.drag {
            let drag = self.drag.take();
            self.drop_card(drag, cx);
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
