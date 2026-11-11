//! Edits: one undo step each, undo and redo, deleting, splitting, new tracks, picking tracks and stacking them, arming.

use super::*;

/// Which track is armed, to record and to hear the MIDI keyboard: the selected one, until
/// you arm one yourself; one you armed; or none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arm {
    Selected,
    Track(u64),
    Off,
}

impl Workspace {
    /// The aliases' shared contents, from the song as it is.
    pub fn reset_links(&mut self) {
        self.links = self.session.project.link_contents();
    }

    pub fn changed(&mut self, cx: &mut Context<Self>) {
        self.edits += 1;
        self.param_names.borrow_mut().clear();
        // Aliases of whatever changed change with it.
        self.session.project.sync_links(&mut self.links);
        let tracks = &self.session.project.tracks;
        self.track = self.track.min(tracks.len().saturating_sub(1));
        if let Some((t, r)) = self.region
            && tracks.get(t).is_none_or(|t| r >= t.regions.len())
        {
            self.region = None;
        }
        let notes = self
            .region
            .and_then(|at| self.session.project.notes_mut(at))
            .map(|n| n.len());
        self.picked.retain(|&i| notes.is_some_and(|n| i < n));
        let valid = match self.channel {
            Channel::Track(i) => i < self.session.project.tracks.len(),
            Channel::Aux(i) => i < self.session.project.auxes.len(),
            Channel::Master => true,
        };
        if !valid {
            self.channel = Channel::Master;
        }
        let project = self.engine_project().into_owned();
        self.status = self.player.sync(&project).err();
        self.refresh_looks(cx);
        cx.notify();
    }

    /// The armed track, by index.
    pub fn armed(&self) -> Option<usize> {
        let tracks = &self.session.project.tracks;
        match self.arm {
            Arm::Selected => (self.track < tracks.len()).then_some(self.track),
            Arm::Track(id) => tracks.iter().position(|t| t.id == id),
            Arm::Off => None,
        }
    }

    /// The R button: arms a track, or disarms it if it's the one armed.
    pub fn toggle_arm(&mut self, track: usize) {
        self.arm = match self.armed() == Some(track) {
            true => Arm::Off,
            false => Arm::Track(self.session.project.tracks[track].id),
        };
    }

    /// One undoable edit.
    pub fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Self)) {
        self.session.checkpoint();
        f(self);
        self.changed(cx);
    }

    /// Where track `from`, dragged to window y `y`, would go: the gap nearest the pointer,
    /// a stack's tracks kept among themselves and the others kept out of stacks.
    pub fn track_drop(&self, from: usize, y: f32) -> usize {
        let p = &self.session.project;
        let n = p.tracks.len();
        let rel = y - f32::from(self.lanes_bounds.get().origin.y);
        // The gap before track `slot`, `slot` running to n (after the last).
        let slot = match self.track_at_y(rel) {
            Some(t) => {
                let mid = self.row_top(t) + (self.lane_h(t) + self.autos_h(t)) / 2.0;
                if rel < mid { t } else { t + 1 }
            }
            None if rel < 0.0 => 0,
            None => n,
        };
        let stack = |t: usize| p.tracks.get(t).and_then(|t| t.output);
        let members = |bus: Option<u32>| -> Vec<usize> {
            (0..n)
                .filter(|&t| bus.is_some() && stack(t) == bus && p.stack_of(t).is_some())
                .collect()
        };
        let mine = members(stack(from).filter(|_| p.stack_of(from).is_some()));
        let slot = match (mine.first(), mine.last()) {
            // In a stack: among its own.
            (Some(&a), Some(&b)) => slot.clamp(a, b + 1),
            // Not: never inside one.
            _ => {
                let inside = (0..n).find(|&t| {
                    let m = members(stack(t).filter(|_| p.stack_of(t).is_some()));
                    m.first().is_some_and(|&a| slot > a) && m.last().is_some_and(|&b| slot <= b)
                });
                match inside {
                    Some(t) => {
                        let m = members(stack(t));
                        let (a, b) = (m[0], *m.last().unwrap());
                        if slot - a < b + 1 - slot { a } else { b + 1 }
                    }
                    None => slot,
                }
            }
        };
        if slot > from { slot - 1 } else { slot }.min(n.saturating_sub(1))
    }

    /// The line where a dragged track would go, over the headers.
    pub(crate) fn track_drop_line(&self) -> Option<gpui::Div> {
        let Some(Drag::TrackMove {
            from,
            to,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let n = self.session.project.tracks.len();
        // The gap it goes into: before `to`, or (moving down) after it.
        let y = match to > from {
            true => self.row_top(to) + self.lane_h(to) + self.autos_h(to),
            false => self.row_top(to),
        };
        let _ = n;
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(y - 1.5))
                .h(px(3.))
                .rounded(px(2.))
                .bg(rgb(ACCENT))
                .debug_selector(|| "track-drop-line".into()),
        )
    }

    /// Track `from` moved to `to`, one undo; what's selected and open follows it.
    pub fn move_track(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let map = |i: usize| match i {
            i if i == from => to,
            i if from < to && i > from && i <= to => i - 1,
            i if to < from && i >= to && i < from => i + 1,
            i => i,
        };
        self.edit(cx, |ws| {
            ws.session.project.move_track(from, to);
            ws.track = map(ws.track);
            ws.region = ws.region.map(|(t, r)| (map(t), r));
            for pick in &mut ws.picks {
                pick.0 = map(pick.0);
            }
            if let Channel::Track(t) = ws.channel {
                ws.channel = Channel::Track(map(t));
            }
            if let Some(v) = ws.midi_fx_view.as_mut() {
                v.track = map(v.track);
            }
            if let Some(v) = ws.poly_view.as_mut() {
                v.track = map(v.track);
            }
            if let Some(v) = ws.bloom_view.as_mut() {
                v.track = map(v.track);
            }
            if let Some(v) = ws.mpe_view.as_mut() {
                v.track = map(v.track);
            }
        });
    }

    /// Selects a track: in the arrangement, the inspector and the mixer.
    pub fn select_track(&mut self, i: usize) {
        self.track = i;
        self.channel = Channel::Track(i);
        self.tracks_picked.clear();
    }

    /// ⌘-click: a track into the selection, or out of it; ⇧-click: every track from the selected
    /// one to it.
    pub fn pick_track(&mut self, i: usize, range: bool) {
        let ids: Vec<u64> = self.session.project.tracks.iter().map(|t| t.id).collect();
        if i >= ids.len() || self.track >= ids.len() {
            return;
        }
        if range {
            // The selected track stays where the range starts, for the next ⇧-click.
            let (a, b) = (self.track.min(i), self.track.max(i));
            self.tracks_picked = ids[a..=b].to_vec();
            return;
        }
        if self.tracks_picked.is_empty() {
            self.tracks_picked.push(ids[self.track]);
        }
        match self.tracks_picked.iter().position(|&id| id == ids[i]) {
            None => {
                self.tracks_picked.push(ids[i]);
                self.track = i;
            }
            // The last one stays.
            Some(_) if self.tracks_picked.len() == 1 => {}
            Some(at) => {
                self.tracks_picked.remove(at);
                if i == self.track {
                    let next = self.tracks_picked[0];
                    self.track = ids.iter().position(|&id| id == next).unwrap_or(i);
                }
            }
        }
        self.channel = Channel::Track(self.track);
    }

    /// A stack of these tracks, gathered under the first of them; they stay selected, with the
    /// stack's channel shown.
    pub fn make_stack(&mut self, tracks: &[usize], cx: &mut Context<Self>) {
        let ids: Vec<u64> = tracks
            .iter()
            .filter_map(|&t| self.session.project.tracks.get(t).map(|t| t.id))
            .collect();
        if ids.is_empty() {
            return;
        }
        let mut bus = 0;
        self.edit(cx, |ws| bus = ws.session.project.add_stack(tracks).0);
        let p = &self.session.project;
        let first = p.tracks.iter().position(|t| t.id == ids[0]).unwrap_or(0);
        let aux = p.auxes.iter().position(|a| a.bus == bus);
        self.select_track(first);
        self.tracks_picked = ids;
        self.region = None;
        self.mod_edit = None;
        if let Some(a) = aux {
            self.channel = Channel::Aux(a);
        }
        cx.notify();
    }

    pub fn is_picked(&self, i: usize) -> bool {
        i == self.track
            || self
                .session
                .project
                .tracks
                .get(i)
                .is_some_and(|t| self.tracks_picked.contains(&t.id))
    }

    /// The selected tracks, top to bottom: the ⌘- or ⇧-clicked ones, or the one.
    pub fn picked_tracks(&self) -> Vec<usize> {
        (0..self.session.project.tracks.len())
            .filter(|&i| self.is_picked(i))
            .collect()
    }

    pub(crate) fn undo(&mut self, cx: &mut Context<Self>) {
        if self.session.undo() {
            self.changed(cx);
        }
    }

    pub(crate) fn redo(&mut self, cx: &mut Context<Self>) {
        if self.session.redo() {
            self.changed(cx);
        }
    }

    pub(crate) fn delete(&mut self, cx: &mut Context<Self>) {
        // On the Songs page: the songs picked, to the Trash (asked first).
        if self.view == View::Songs {
            if self.home.as_ref().is_some_and(|h| !h.picks.is_empty()) {
                self.song_pending = Some(crate::ui::menu::MenuAction::TrashPicked);
                cx.notify();
            }
            return;
        }
        if self.delete_eq_band(cx) || self.delete_picked(cx) {
            return;
        }
        let notes_shown = matches!(self.bottom, Some(Bottom::PianoRoll | Bottom::Score));
        if let (Some(at), false, true) = (self.region, self.picked.is_empty(), notes_shown) {
            self.delete_notes(at, cx);
            return;
        }
        if self.delete_selected(cx) {
            return;
        }
        let Some((t, r)) = self.region else { return };
        self.edit(cx, |ws| {
            let gone = ws.session.project.tracks[t].regions.remove(r);
            // Shuffle: the gap it leaves closed up.
            if ws.drag_mode == DragMode::Shuffle {
                ws.session.project.close_up(t, gone.start);
            }
            ws.region = None;
        });
    }

    pub fn delete_track(&mut self, cx: &mut Context<Self>) {
        if self.session.project.tracks.is_empty() {
            return;
        }
        let t = self.track;
        self.edit(cx, |ws| {
            ws.session.project.tracks.remove(t);
            ws.region = None;
        });
    }

    /// Splits the selected region at the playhead (⌘T).
    pub(crate) fn split(&mut self, cx: &mut Context<Self>) {
        let (Some(at), tick) = (self.region, self.playhead()) else {
            return;
        };
        self.split_at(at, tick, cx);
    }

    pub fn split_at(&mut self, at: At, tick: u64, cx: &mut Context<Self>) {
        self.session.checkpoint();
        match self.session.project.split_region(at, tick) {
            Some(second) => {
                self.region = Some(second);
                self.changed(cx);
            }
            None => self.session.cancel(),
        }
    }

    /// Adds a track after the selected one: an instrument track (the synth, the drum kit) or,
    /// without an instrument, an audio track.
    pub fn new_track(&mut self, instrument: Option<Instrument>, cx: &mut Context<Self>) {
        self.new_track_named(instrument, None, cx);
    }

    /// A new track after the selected one, selected (so it plays what's played on a keyboard);
    /// called `name`, or after its instrument.
    pub fn new_track_named(
        &mut self,
        instrument: Option<Instrument>,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let n = self.session.project.tracks.len();
        let at = if n == 0 { 0 } else { self.track + 1 };
        let name = name.unwrap_or_else(|| match &instrument {
            Some(Instrument::Drums) => format!("Drums {}", n + 1),
            Some(Instrument::Plugin(p)) => p.class.clone(),
            Some(Instrument::Sampler(_)) => format!("Sampler {}", n + 1),
            Some(Instrument::Poly(_)) => {
                format!("{} {}", pina_engine::song::project::POLY_NAME, n + 1)
            }
            Some(Instrument::Bloom(_)) => {
                format!("{} {}", pina_engine::song::project::BLOOM_NAME, n + 1)
            }
            Some(Instrument::Kit) => format!("Kit {}", n + 1),
            Some(Instrument::Synth) => format!("Synth {}", n + 1),
            None => format!("Audio {}", n + 1),
        });
        self.edit(cx, |ws| {
            let track = Track {
                name,
                instrument,
                ..Default::default()
            };
            ws.session.project.add_track(at, track);
            ws.select_track(at);
            ws.region = None;
        });
    }
}
