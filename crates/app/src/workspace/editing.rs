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
        let Some((t, r)) = self.region else { return };
        self.edit(cx, |ws| {
            ws.session.project.tracks[t].regions.remove(r);
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
