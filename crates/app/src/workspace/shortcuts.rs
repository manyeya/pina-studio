//! The arrangement's keys for regions, tracks, the playhead, markers and the view: picking the
//! next region or track, nudging, transposing, looping, quantizing, joining and trimming
//! regions, moving the playhead by bars and markers, cycling and zooming to what's picked.

use super::*;
use crate::{
    ui::menu::{MARKER_NAMES, MenuAction},
    workspace::context::{RegionOp, TrackOp},
};
use pina_engine::song::project::Region;

/// A key's job in the arrangement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shortcut {
    PrevRegion,
    NextRegion,
    PrevTrack,
    NextTrack,
    /// The region moved a grid step, earlier or later.
    Nudge(i8),
    /// The region moved in pitch, semitones.
    Transpose(i8),
    LoopRegion,
    QuantizeRegion,
    JoinRegions,
    RegionToPlayhead,
    StartToPlayhead,
    EndToPlayhead,
    ToggleSnap,
    BounceRegion,
    StretchToCycle,
    StretchToBar,
    /// The audio picked time-stretched to the song's tempo, or back as it was.
    StretchToTempo,
    CycleToSelection,
    AddMarker,
    PrevMarker,
    NextMarker,
    /// The playhead moved bars, back or on.
    Bars(i64),
    GoToEnd,
    PlayFromSelection,
    ZoomToFit,
    /// Every lane taller or shorter.
    LaneHeight(f32),
    MuteTrack,
    SoloTrack,
    ArmTrack,
    RenameTrack,
    DuplicateTrack,
    ToggleEditor,
}

impl Workspace {
    /// What a key does in the arrangement.
    pub fn shortcut(&mut self, s: Shortcut, window: &mut Window, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let (playhead, bar, grid) = (self.playhead(), p.bar(), self.grid().max(PPQ / 16));
        let region = self
            .region
            .filter(|at| p.tracks.get(at.0).is_some_and(|t| at.1 < t.regions.len()));
        let t = self.track.min(p.tracks.len().saturating_sub(1));
        match s {
            Shortcut::PrevRegion | Shortcut::NextRegion => {
                let track = region.map_or(t, |at| at.0);
                let Some(regions) = p.tracks.get(track).map(|t| &t.regions) else {
                    return;
                };
                let mut order: Vec<usize> = (0..regions.len()).collect();
                order.sort_by_key(|&i| regions[i].start);
                let next = s == Shortcut::NextRegion;
                let pick = match region.and_then(|at| order.iter().position(|&i| i == at.1)) {
                    Some(k) if next => order.get(k + 1),
                    Some(k) => k.checked_sub(1).and_then(|k| order.get(k)),
                    None if next => order.iter().find(|&&i| regions[i].start >= playhead),
                    None => order.iter().rev().find(|&&i| regions[i].start < playhead),
                };
                if let Some(&i) = pick {
                    self.region = Some((track, i));
                    self.track = track;
                }
            }
            Shortcut::PrevTrack | Shortcut::NextTrack => {
                let n = p.tracks.len();
                let to = match s {
                    Shortcut::NextTrack => (t + 1).min(n.saturating_sub(1)),
                    _ => t.saturating_sub(1),
                };
                self.select_track(to);
                self.region = None;
            }
            Shortcut::Nudge(dir) => {
                let by = dir as i64 * grid as i64;
                if self.each_selected(cx, |r| r.start = (r.start as i64 + by).max(0) as u64) {
                    return cx.notify();
                }
                let Some(at) = region else { return };
                let start = self.session.project.region(at).start as i64 + by;
                let mut moved = at;
                self.edit(cx, |ws| {
                    moved = ws
                        .session
                        .project
                        .move_region(at, at.0, start.max(0) as u64)
                });
                self.region = Some(moved);
            }
            Shortcut::Transpose(by) => {
                let shift = |r: &mut Region| {
                    r.transpose = (r.transpose as i16 + by as i16).clamp(-48, 48) as i8
                };
                if self.each_selected(cx, shift) {
                    return cx.notify();
                }
                let Some(at) = region else { return };
                self.edit(cx, |ws| {
                    let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                    r.transpose = (r.transpose as i16 + by as i16).clamp(-48, 48) as i8;
                });
            }
            Shortcut::LoopRegion => {
                if self.each_selected(cx, |r| r.looped ^= true) {
                    return cx.notify();
                }
                if let Some(at) = region {
                    self.region_op(at, RegionOp::Loop, cx)
                }
            }
            // Audio: its hits onto the grid.
            Shortcut::QuantizeRegion
                if region
                    .is_some_and(|at| matches!(p.region(at).content, Content::Audio { .. })) =>
            {
                let all = self.selected_regions();
                if !self.quantize_audio(&all, cx) {
                    self.status = Some("Its hits aren't found yet: try again in a moment".into());
                }
            }
            Shortcut::QuantizeRegion => {
                let on = region.is_some_and(|at| p.region(at).quantize != Some(grid));
                let q = |r: &mut Region| r.quantize = on.then_some(grid);
                if self.each_selected(cx, q) {
                    return cx.notify();
                }
                let Some(at) = region else { return };
                self.edit(cx, |ws| {
                    let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                    r.quantize = match r.quantize == Some(grid) {
                        true => None,
                        false => Some(grid),
                    };
                });
            }
            Shortcut::JoinRegions => {
                if let Some(at) = region {
                    self.glue(at, cx)
                }
            }
            Shortcut::RegionToPlayhead => {
                let Some(at) = region else { return };
                let mut moved = at;
                self.edit(cx, |ws| {
                    moved = ws.session.project.move_region(at, at.0, playhead)
                });
                self.region = Some(moved);
            }
            Shortcut::StartToPlayhead | Shortcut::EndToPlayhead => {
                let Some(at) = region else { return };
                let r = p.region(at);
                let inside = playhead > r.start && playhead < r.start + r.length;
                match (s, inside) {
                    (Shortcut::StartToPlayhead, true) => {
                        self.edit(cx, |ws| ws.session.project.trim_start(at, playhead))
                    }
                    (Shortcut::EndToPlayhead, _) if playhead > r.start => {
                        self.edit(cx, |ws| ws.session.project.trim_end(at, playhead))
                    }
                    _ => self.status = Some("The playhead isn't inside the region.".into()),
                }
            }
            Shortcut::ToggleSnap => {
                (self.snap, self.snap_was) = match self.snap {
                    Snap::Off => (self.snap_was, Snap::Off),
                    on => (Snap::Off, on),
                };
                self.status = Some(match self.snap {
                    Snap::Off => "Snap off: things move freely".into(),
                    s => format!("Snap: {}", s.label()),
                });
            }
            Shortcut::BounceRegion => {
                if let Some(at) = region {
                    self.region_op(at, RegionOp::Bounce, cx)
                }
            }
            Shortcut::StretchToCycle | Shortcut::StretchToBar => {
                let Some(at) = region else { return };
                let length = match s {
                    Shortcut::StretchToCycle => match p.cycle {
                        Some(c) if c.end > c.start => c.end - c.start,
                        _ => {
                            self.status =
                                Some("Sweep a cycle in the ruler to stretch it to.".into());
                            return cx.notify();
                        }
                    },
                    _ => ((p.region(at).length as f64 / bar as f64).round() as u64).max(1) * bar,
                };
                let mut said = Ok(());
                self.edit(cx, |ws| said = ws.session.project.time_stretch(at, length));
                if let Err(e) = said {
                    self.status = Some(e);
                }
            }
            Shortcut::StretchToTempo => self.stretch_to_tempo(self.selected_regions(), cx),
            Shortcut::CycleToSelection => {
                // The region picked, else the section the playhead's in, else its bar.
                let section = p
                    .sections()
                    .into_iter()
                    .find(|s| (s.0..s.1).contains(&playhead));
                let (start, end) = match (region, section) {
                    (Some(at), _) => {
                        let r = p.region(at);
                        (r.start, r.start + r.length)
                    }
                    (None, Some(s)) => s,
                    (None, None) => (playhead / bar * bar, playhead / bar * bar + bar),
                };
                self.edit(cx, |ws| {
                    ws.session.project.cycle = Some(Cycle {
                        start,
                        end,
                        off: false,
                    })
                });
            }
            Shortcut::AddMarker => {
                let tick = playhead / bar * bar;
                let n = p.markers.len();
                let name = MARKER_NAMES
                    .get(n)
                    .map_or(format!("Marker {}", n + 1), |s| s.to_string());
                self.choose(MenuAction::AddMarker(tick, name), cx);
            }
            Shortcut::PrevMarker | Shortcut::NextMarker => {
                let to = match s {
                    Shortcut::NextMarker => {
                        p.markers.iter().map(|m| m.tick).find(|&m| m > playhead)
                    }
                    _ => p
                        .markers
                        .iter()
                        .rev()
                        .map(|m| m.tick)
                        .find(|&m| m < playhead),
                };
                if let Some(tick) = to {
                    self.locate_tick(tick, cx);
                }
            }
            Shortcut::Bars(n) => {
                let at = (playhead / bar) as i64 + n;
                self.locate_tick(at.max(0) as u64 * bar, cx);
            }
            Shortcut::GoToEnd => self.locate_tick(p.end(), cx),
            Shortcut::PlayFromSelection => {
                let from = region.map_or(playhead, |at| p.region(at).start);
                self.locate_tick(from, cx);
                if !self.player.playing() {
                    self.play_stop(cx);
                }
            }
            Shortcut::ZoomToFit => self.zoom_to_fit(),
            Shortcut::LaneHeight(by) => self.edit(cx, |ws| {
                for track in &mut ws.session.project.tracks {
                    let h = track.height.unwrap_or(LANE_H) * by;
                    track.height = Some(h.clamp(MIN_LANE_H, MAX_LANE_H));
                }
            }),
            Shortcut::MuteTrack => self.track_op(TrackOp::Mute(t), cx),
            Shortcut::SoloTrack => self.track_op(TrackOp::Solo(t), cx),
            Shortcut::ArmTrack => self.track_op(TrackOp::Arm(t), cx),
            Shortcut::RenameTrack => self.start_naming(Name::Track(t), window, cx),
            Shortcut::DuplicateTrack => self.track_op(TrackOp::Duplicate(t), cx),
            Shortcut::ToggleEditor => {
                let pane = self.bottom.unwrap_or(Bottom::PianoRoll);
                self.toggle_bottom(pane, cx);
            }
        }
        cx.notify();
    }
}
