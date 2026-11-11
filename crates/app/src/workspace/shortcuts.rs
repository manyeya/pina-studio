//! The arrangement's keys for regions, tracks, the playhead, markers and the view: picking the
//! next region or track, nudging, transposing, looping, quantizing, joining and trimming
//! regions, moving the playhead by bars and markers, cycling and zooming to what's picked.

use super::*;
use crate::{
    ui::menu::{MARKER_NAMES, MenuAction},
    workspace::{
        context::{RegionOp, TrackOp},
        naming::Name,
    },
};
use pina_engine::song::project::Region;

/// How far ⌥← and ⌥→ move what's picked, as Logic's nudge value (⌃⌥D, ⌃⌥B, ⌃⌥M, ⌃⌥T).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nudge {
    /// A grid step.
    Division,
    Beat,
    Bar,
    Tick,
}

impl Nudge {
    pub fn name(self) -> &'static str {
        match self {
            Nudge::Division => "a grid step",
            Nudge::Beat => "a beat",
            Nudge::Bar => "a bar",
            Nudge::Tick => "a tick",
        }
    }
}

/// Which regions a Select command picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionPick {
    /// From the first selected (or the playhead) on, on every track.
    Following,
    /// From the first selected on, on its tracks.
    FollowingSameTrack,
    /// Starting inside the cycle.
    InsideCycle,
    /// Overlapping another on its track.
    Overlapped,
    /// Holding what a selected one holds.
    Equal,
    /// Of a selected one's kind: audio from its file, or MIDI, or a pattern.
    Similar,
    /// Starting at the same place in the bar as one selected.
    SameSubposition,
    Muted,
    /// A selected one's aliases.
    Aliases,
}

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
    /// A new track with the focused one's settings, empty (⌘D).
    DuplicateTrack,
    /// The focused track again, its regions too (⌥⇧⌘D).
    DuplicateWithContent,
    ToggleEditor,
    /// The region before or after added to the selection, or, back over the last, let go.
    ToggleRegion(bool),
    /// The first region on the track, or the last.
    EdgeRegion(bool),
    PickRegions(RegionPick),
    InvertRegions,
    DeselectAll,
    /// The selected regions' ends moved by the nudge value.
    Resize(i8),
    /// The region to the playhead, then the next one on its track selected.
    RegionToPlayheadNext,
    SplitAtLocators,
    /// Each selected region shortened where it runs into the next.
    RemoveOverlaps,
    /// Each selected region's end moved to where the next starts.
    TrimToNext,
    /// The selected regions cut to the cycle, what's outside it taken off.
    CropToLocators,
    /// MIDI regions fitted to their notes, out to whole bars.
    OptimalLengths,
    Dequantize,
    RenameRegion,
    NameByTrack,
    AliasToCopy,
    /// The selected regions onto the selected track.
    RegionsToTrack,
    NewAudioTrack,
    NewInstrumentTrack,
    /// The focused track's stack opened, or closed.
    Stack(bool),
    /// The focused track's lane taller, or shorter.
    TrackZoom(f32),
    ToggleTrackZoom,
    /// The focused track's lane back to the usual height (every track's).
    ResetTrackZoom(bool),
    UnmuteAll,
    UnsoloAll,
    /// The cycle's stretch taken out of the song, or silence that long put in at its start, or
    /// it played twice.
    CutBetweenLocators,
    InsertSilence,
    RepeatBetweenLocators,
    /// The playhead to the cycle's start, or its end.
    GoToLocator(bool),
    /// The playhead to the selection's start, or its end.
    GoToSelection(bool),
    /// The cycle moved its own length back, or on.
    MoveCycle(i8),
    /// The cycle twice as long, or half.
    ScaleCycle(bool),
    MarkerForSelection,
    DeleteMarker,
    RenameMarker,
    ToggleCountIn,
    CatchPlayhead,
    ScrollToSelection,
    ZoomToLocators,
    /// The next tool, or the one before.
    NextTool(i8),
    SetNudge(Nudge),
    DiscardRecording,
    /// The keys passed between the tracks and the editor open (Tab).
    SwitchKeys,
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
                let by = dir as i64 * self.nudge_ticks() as i64;
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
            Shortcut::DuplicateTrack => self.track_op(TrackOp::SameStrip(t), cx),
            Shortcut::DuplicateWithContent => self.track_op(TrackOp::Duplicate(t), cx),
            Shortcut::ToggleEditor => {
                let pane = self.bottom.unwrap_or(Bottom::PianoRoll);
                self.toggle_bottom(pane, cx);
            }
            _ => self.logic_shortcut(s, region, t, window, cx),
        }
        cx.notify();
    }

    /// How far a nudge moves things, in ticks.
    pub fn nudge_ticks(&self) -> u64 {
        let p = &self.session.project;
        match self.nudge {
            Nudge::Division => self.grid().max(PPQ / 16),
            Nudge::Beat => p.beat(),
            Nudge::Bar => p.bar(),
            Nudge::Tick => 1,
        }
    }

    /// Where a dragged region's edge lands: on the snap; with ⌃ a quarter of the grid; with ⌃⌥
    /// or ⌃⇧ the tick, as Logic moves them.
    pub fn drag_snap(&self, t: f64) -> u64 {
        let t = t.max(0.0);
        match (self.mods.control, self.mods.alt || self.mods.shift) {
            (true, true) => t.round() as u64,
            (true, false) => {
                let q = (self.grid() / 4).max(1) as f64;
                ((t / q).round() * q) as u64
            }
            _ => self.snap(t),
        }
    }

    /// The cycle, if there's one to work between; else why not, said.
    fn locators(&mut self) -> Option<(u64, u64)> {
        let c = self.session.project.cycle.filter(|c| c.end > c.start);
        if c.is_none() {
            self.status = Some("Sweep a cycle in the ruler first".into());
        }
        c.map(|c| (c.start, c.end))
    }

    /// The keys Pina took from Logic Pro's: selecting, trimming and cutting regions, tracks,
    /// the locators, markers and the view.
    fn logic_shortcut(
        &mut self,
        s: Shortcut,
        region: Option<At>,
        t: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let playhead = self.playhead();
        let selected = self.selected_regions();
        let p = &self.session.project;
        let bar = p.bar().max(1);
        // A track's regions by start.
        let order = |track: usize| -> Vec<usize> {
            let regions = &self.session.project.tracks[track].regions;
            let mut o: Vec<usize> = (0..regions.len()).collect();
            o.sort_by_key(|&i| regions[i].start);
            o
        };
        match s {
            Shortcut::ToggleRegion(next) => {
                let Some(at) = region else {
                    let s = if next {
                        Shortcut::NextRegion
                    } else {
                        Shortcut::PrevRegion
                    };
                    return self.shortcut(s, window, cx);
                };
                let o = order(at.0);
                let k = o.iter().position(|&i| i == at.1).unwrap_or(0);
                let to = match next {
                    true => o.get(k + 1),
                    false => k.checked_sub(1).and_then(|k| o.get(k)),
                };
                let Some(&to) = to else { return };
                let to = (at.0, to);
                let mut all = selected.clone();
                if all.contains(&to) {
                    all.retain(|&a| a != at);
                    all.retain(|&a| a != to);
                } else {
                    all.retain(|&a| a != to);
                }
                all.insert(0, to);
                self.select_regions(all);
            }
            Shortcut::EdgeRegion(last) => {
                let o = order(t);
                let to = if last { o.last() } else { o.first() };
                if let Some(&i) = to {
                    self.select_regions(vec![(t, i)]);
                }
            }
            Shortcut::PickRegions(rule) => {
                let chosen: Vec<&Region> = selected.iter().map(|&a| p.region(a)).collect();
                let needs_some = !matches!(
                    rule,
                    RegionPick::Following
                        | RegionPick::InsideCycle
                        | RegionPick::Overlapped
                        | RegionPick::Muted
                );
                if needs_some && chosen.is_empty() {
                    self.status = Some("Select a region first".into());
                    return;
                }
                let cycle = p.cycle.filter(|c| c.end > c.start);
                if rule == RegionPick::InsideCycle && cycle.is_none() {
                    self.status = Some("Sweep a cycle in the ruler first".into());
                    return;
                }
                let from = chosen.iter().map(|r| r.start).min().unwrap_or(playhead);
                let tracks: Vec<usize> = match selected.is_empty() {
                    true => vec![t],
                    false => selected.iter().map(|a| a.0).collect(),
                };
                let kind = |r: &Region| match &r.content {
                    Content::Audio { file, .. } => file.display().to_string(),
                    Content::Midi { .. } => "midi".into(),
                    Content::Pattern { .. } => "pattern".into(),
                    Content::Takes { .. } => "takes".into(),
                };
                let kinds: Vec<String> = chosen.iter().map(|r| kind(r)).collect();
                let places: Vec<u64> = chosen.iter().map(|r| r.start % bar).collect();
                let links: Vec<u64> = chosen.iter().filter_map(|r| r.link).collect();
                let mut hits = Vec::new();
                for (ti, track) in p.tracks.iter().enumerate() {
                    for (i, r) in track.regions.iter().enumerate() {
                        let keep = match rule {
                            RegionPick::Following => r.start >= from,
                            RegionPick::FollowingSameTrack => {
                                r.start >= from && tracks.contains(&ti)
                            }
                            RegionPick::InsideCycle => {
                                cycle.is_some_and(|c| (c.start..c.end).contains(&r.start))
                            }
                            RegionPick::Overlapped => {
                                track.regions.iter().enumerate().any(|(j, o)| {
                                    j != i
                                        && o.start < r.start + r.length
                                        && r.start < o.start + o.length
                                })
                            }
                            RegionPick::Equal => chosen
                                .iter()
                                .any(|c| c.content == r.content && c.length == r.length),
                            RegionPick::Similar => kinds.contains(&kind(r)),
                            RegionPick::SameSubposition => places.contains(&(r.start % bar)),
                            RegionPick::Muted => r.mute,
                            RegionPick::Aliases => r.link.is_some_and(|l| links.contains(&l)),
                        };
                        if keep {
                            hits.push((ti, i));
                        }
                    }
                }
                if rule == RegionPick::Aliases && links.is_empty() {
                    self.status = Some("The region selected has no aliases".into());
                    return;
                }
                // The one focused stays focused, if it's among them.
                if let Some(at) = region
                    && let Some(k) = hits.iter().position(|&h| h == at)
                {
                    hits.swap(0, k);
                }
                self.status = Some(match hits.len() {
                    1 => "1 region selected".into(),
                    n => format!("{n} regions selected"),
                });
                self.select_regions(hits);
            }
            Shortcut::InvertRegions => {
                let all: Vec<At> = (p.tracks.iter().enumerate())
                    .flat_map(|(ti, track)| (0..track.regions.len()).map(move |i| (ti, i)))
                    .filter(|a| !selected.contains(a))
                    .collect();
                self.select_regions(all);
            }
            Shortcut::DeselectAll => {
                self.region = None;
                self.picks.clear();
                self.auto_picked = None;
            }
            Shortcut::Resize(dir) => {
                let by = dir as i64 * self.nudge_ticks() as i64;
                let least = (PPQ / 16) as i64;
                self.edit(cx, |ws| {
                    for &(ti, i) in &selected {
                        let r = &mut ws.session.project.tracks[ti].regions[i];
                        r.length = (r.length as i64 + by).max(least) as u64;
                    }
                });
            }
            Shortcut::RegionToPlayheadNext => {
                let Some(at) = region else { return };
                let o = order(at.0);
                let next = o
                    .iter()
                    .position(|&i| i == at.1)
                    .and_then(|k| o.get(k + 1))
                    .map(|&i| p.region((at.0, i)).start);
                self.shortcut(Shortcut::RegionToPlayhead, window, cx);
                if let Some(start) = next {
                    let regions = &self.session.project.tracks[at.0].regions;
                    if let Some(i) = regions.iter().position(|r| r.start == start) {
                        self.select_regions(vec![(at.0, i)]);
                    }
                }
            }
            Shortcut::SplitAtLocators => {
                let Some((a, b)) = self.locators() else {
                    return;
                };
                if selected.is_empty() {
                    self.status = Some("Select the regions to split".into());
                    return;
                }
                self.edit(cx, |ws| {
                    // Later ones first: a split adds a region after the one split.
                    let mut all = selected.clone();
                    all.sort_by(|x, y| y.cmp(x));
                    for at in all {
                        if let Some(second) = ws.session.project.split_region(at, b) {
                            let _ = second;
                        }
                        ws.session.project.split_region(at, a);
                    }
                    ws.region = None;
                    ws.picks.clear();
                });
            }
            Shortcut::RemoveOverlaps | Shortcut::TrimToNext => {
                let all = match selected.is_empty() {
                    true => (0..p.tracks.get(t).map_or(0, |tr| tr.regions.len()))
                        .map(|i| (t, i))
                        .collect(),
                    false => selected.clone(),
                };
                let ends: Vec<(At, u64)> = all
                    .iter()
                    .filter_map(|&at| {
                        let r = p.region(at);
                        let next = (p.tracks[at.0].regions.iter())
                            .map(|o| o.start)
                            .filter(|&st| st > r.start)
                            .min()?;
                        let end = r.start + r.length;
                        match s {
                            Shortcut::RemoveOverlaps if end > next => Some((at, next)),
                            Shortcut::TrimToNext if end != next => Some((at, next)),
                            _ => None,
                        }
                    })
                    .collect();
                if ends.is_empty() {
                    self.status = Some("Nothing to trim".into());
                    return;
                }
                self.edit(cx, |ws| {
                    for (at, end) in ends {
                        ws.session.project.trim_end(at, end);
                    }
                });
            }
            Shortcut::CropToLocators => {
                let Some((a, b)) = self.locators() else {
                    return;
                };
                let mut all = selected.clone();
                all.sort_by(|x, y| y.cmp(x));
                self.edit(cx, |ws| {
                    for at in all {
                        let r = ws.session.project.region(at);
                        let (start, end) = (r.start, r.start + r.length);
                        if end <= a || start >= b {
                            ws.session.project.tracks[at.0].regions.remove(at.1);
                            continue;
                        }
                        if end > b {
                            ws.session.project.trim_end(at, b);
                        }
                        if start < a {
                            ws.session.project.trim_start(at, a);
                        }
                    }
                    ws.region = None;
                    ws.picks.clear();
                });
            }
            Shortcut::OptimalLengths => {
                let fits: Vec<(At, u64, u64)> = selected
                    .iter()
                    .filter_map(|&at| {
                        let r = p.region(at);
                        let Content::Midi { notes, .. } = &r.content else {
                            return None;
                        };
                        let first = notes.iter().map(|n| r.start as i64 + n.start).min()?;
                        let last = notes
                            .iter()
                            .map(|n| r.start as i64 + n.start + n.length as i64)
                            .max()?;
                        let from = first.max(0) as u64 / bar * bar;
                        let to = (last.max(0) as u64).div_ceil(bar) * bar;
                        Some((at, from, to.max(from + bar)))
                    })
                    .collect();
                if fits.is_empty() {
                    self.status = Some("Select MIDI regions with notes in them".into());
                    return;
                }
                self.edit(cx, |ws| {
                    for (at, from, to) in fits {
                        ws.session.project.trim_end(at, to);
                        ws.session.project.trim_start(at, from);
                    }
                });
            }
            Shortcut::Dequantize => self.edit(cx, |ws| {
                for &(ti, i) in &selected {
                    ws.session.project.tracks[ti].regions[i].quantize = None;
                }
            }),
            Shortcut::RenameRegion => {
                if let Some(at) = region {
                    self.start_naming(Name::Region(at), window, cx);
                }
            }
            Shortcut::NameByTrack => self.edit(cx, |ws| {
                for &(ti, i) in &selected {
                    let name = ws.session.project.tracks[ti].name.clone();
                    ws.session.project.tracks[ti].regions[i].name = name;
                }
            }),
            Shortcut::AliasToCopy => self.edit(cx, |ws| {
                for &(ti, i) in &selected {
                    ws.session.project.tracks[ti].regions[i].link = None;
                }
            }),
            Shortcut::RegionsToTrack => {
                let midi = p.tracks.get(t).is_some_and(|tr| tr.instrument.is_some());
                let moving: Vec<At> = selected
                    .iter()
                    .copied()
                    .filter(|&at| at.0 != t && p.region(at).content.is_midi() == midi)
                    .collect();
                if moving.is_empty() {
                    self.status = Some("Select regions that fit the selected track".into());
                    return;
                }
                let mut gone = moving.clone();
                gone.sort_by(|x, y| y.cmp(x));
                let mut landed: Vec<(u64, u64)> = Vec::new();
                self.edit(cx, |ws| {
                    let mut taken = Vec::new();
                    for (ti, i) in gone {
                        taken.push(ws.session.project.tracks[ti].regions.remove(i));
                    }
                    let regions = &mut ws.session.project.tracks[t].regions;
                    for r in taken {
                        landed.push((r.start, r.length));
                        regions.push(r);
                    }
                    regions.sort_by_key(|r| r.start);
                });
                let regions = &self.session.project.tracks[t].regions;
                let all: Vec<At> = (0..regions.len())
                    .filter(|&i| landed.contains(&(regions[i].start, regions[i].length)))
                    .map(|i| (t, i))
                    .collect();
                self.select_regions(all);
            }
            Shortcut::NewAudioTrack => self.track_op(TrackOp::NewAudio, cx),
            Shortcut::NewInstrumentTrack => self.track_op(TrackOp::NewInstrument, cx),
            Shortcut::Stack(open) => {
                let bus = p.tracks.get(t).and_then(|tr| tr.output);
                let Some(k) = p.auxes.iter().position(|a| a.stack && Some(a.bus) == bus) else {
                    self.status = Some("The selected track isn't in a stack".into());
                    return;
                };
                self.edit(cx, |ws| ws.session.project.auxes[k].folded = !open);
            }
            Shortcut::TrackZoom(by) => self.edit(cx, |ws| {
                if let Some(track) = ws.session.project.tracks.get_mut(t) {
                    let h = track.height.unwrap_or(ws.settings.track_height) * by;
                    track.height = Some(h.clamp(MIN_LANE_H, MAX_LANE_H));
                }
            }),
            Shortcut::ToggleTrackZoom => self.edit(cx, |ws| {
                let usual = ws.settings.track_height;
                if let Some(track) = ws.session.project.tracks.get_mut(t) {
                    track.height = match track.height.unwrap_or(usual) > usual * 1.5 {
                        true => None,
                        false => Some((usual * 2.6).min(MAX_LANE_H)),
                    };
                }
            }),
            Shortcut::ResetTrackZoom(all) => self.edit(cx, |ws| {
                for (i, track) in ws.session.project.tracks.iter_mut().enumerate() {
                    if all || i == t {
                        track.height = None;
                    }
                }
            }),
            Shortcut::UnmuteAll | Shortcut::UnsoloAll => self.edit(cx, |ws| {
                for track in &mut ws.session.project.tracks {
                    match s {
                        Shortcut::UnmuteAll => track.mute = false,
                        _ => track.solo = false,
                    }
                }
            }),
            Shortcut::CutBetweenLocators
            | Shortcut::InsertSilence
            | Shortcut::RepeatBetweenLocators => {
                let Some((a, b)) = self.locators() else {
                    return;
                };
                self.edit(cx, |ws| {
                    let p = &mut ws.session.project;
                    match s {
                        Shortcut::CutBetweenLocators => p.remove_time(a, b),
                        Shortcut::InsertSilence => p.insert_time(a, b - a),
                        _ => p.repeat_time(a, b),
                    }
                    // The locators stay where they were, as Logic's do.
                    if let Some(c) = &mut p.cycle {
                        (c.start, c.end) = (a, b);
                    }
                    ws.region = None;
                    ws.picks.clear();
                });
            }
            Shortcut::GoToLocator(end) => {
                if let Some((a, b)) = self.locators() {
                    self.locate_tick(if end { b } else { a }, cx);
                }
            }
            Shortcut::GoToSelection(end) => {
                let edge = selected.iter().map(|&at| {
                    let r = p.region(at);
                    if end { r.start + r.length } else { r.start }
                });
                let to = if end { edge.max() } else { edge.min() };
                if let Some(tick) = to {
                    self.locate_tick(tick, cx);
                }
            }
            Shortcut::MoveCycle(dir) => {
                let Some((a, b)) = self.locators() else {
                    return;
                };
                let len = b - a;
                let (a, b) = match dir > 0 {
                    true => (a + len, b + len),
                    false if a >= len => (a - len, b - len),
                    false => (0, len),
                };
                self.edit(cx, |ws| {
                    if let Some(c) = &mut ws.session.project.cycle {
                        (c.start, c.end) = (a, b);
                    }
                });
            }
            Shortcut::ScaleCycle(double) => {
                let Some((a, b)) = self.locators() else {
                    return;
                };
                let beat = self.session.project.beat();
                let len = match double {
                    true => (b - a) * 2,
                    false => ((b - a) / 2).max(beat),
                };
                self.edit(cx, |ws| {
                    if let Some(c) = &mut ws.session.project.cycle {
                        c.end = a + len;
                    }
                });
            }
            Shortcut::MarkerForSelection => {
                let span = selected.iter().map(|&at| p.region(at)).fold(None, |s, r| {
                    let (a, b) = (r.start, r.start + r.length);
                    Some(s.map_or((a, b), |(x, y): (u64, u64)| (x.min(a), y.max(b))))
                });
                let Some((a, b)) = span else {
                    self.status = Some("Select the regions to mark".into());
                    return;
                };
                let n = p.markers.len();
                let has_end = p.markers.iter().any(|m| m.tick == b);
                let name = MARKER_NAMES
                    .get(n)
                    .map_or(format!("Marker {}", n + 1), |s| s.to_string());
                self.choose(MenuAction::AddMarker(a, name), cx);
                if !has_end {
                    self.choose(MenuAction::AddMarker(b, format!("Marker {}", n + 2)), cx);
                }
            }
            Shortcut::DeleteMarker | Shortcut::RenameMarker => {
                // The marker at the playhead, or the last before it.
                let Some(i) = p.markers.iter().rposition(|m| m.tick <= playhead) else {
                    self.status = Some("There's no marker at or before the playhead".into());
                    return;
                };
                match s {
                    Shortcut::DeleteMarker => self.choose(MenuAction::DeleteMarker(i), cx),
                    _ => self.start_naming(Name::Marker(i), window, cx),
                }
            }
            Shortcut::ToggleCountIn => {
                self.settings.count_in = if self.settings.count_in == 0 { 1 } else { 0 };
                self.settings.save();
                self.status = Some(match self.settings.count_in {
                    0 => "Count-in off".into(),
                    n => format!("Count-in: {n} bar{}", if n == 1 { "" } else { "s" }),
                });
            }
            Shortcut::CatchPlayhead => {
                self.settings.follow_playhead = !self.settings.follow_playhead;
                self.settings.save();
                if self.settings.follow_playhead {
                    let width = f32::from(self.lanes_bounds.get().size.width).max(200.0);
                    let x = playhead as f32 * self.px_per_tick() - width * 0.1;
                    self.scroll_x = x.clamp(0.0, self.max_scroll_x());
                }
                self.status = Some(match self.settings.follow_playhead {
                    true => "Catch playhead on: the view follows it".into(),
                    false => "Catch playhead off".into(),
                });
            }
            Shortcut::ScrollToSelection => {
                let Some(at) = region else { return };
                let width = f32::from(self.lanes_bounds.get().size.width).max(200.0);
                let x = p.region(at).start as f32 * self.px_per_tick() - width * 0.1;
                let y = self.row_top(at.0) + self.scroll_y - 24.0;
                self.scroll_x = x.clamp(0.0, self.max_scroll_x());
                self.scroll_y = y.clamp(0.0, self.max_scroll_y());
            }
            Shortcut::ZoomToLocators => {
                let Some((a, b)) = self.locators() else {
                    return;
                };
                let width = f32::from(self.lanes_bounds.get().size.width).max(200.0);
                let beats = (b - a) as f32 / PPQ as f32;
                self.px_per_beat = (width * 0.9 / beats.max(0.25)).clamp(4.0, 800.0);
                self.scroll_x = (a as f32 * self.px_per_tick() - width * 0.05).max(0.0);
            }
            Shortcut::NextTool(dir) => {
                const TOOLS: [Tool; 11] = [
                    Tool::Pointer,
                    Tool::Pencil,
                    Tool::Scissors,
                    Tool::Eraser,
                    Tool::Text,
                    Tool::Glue,
                    Tool::Solo,
                    Tool::Mute,
                    Tool::Zoom,
                    Tool::Fade,
                    Tool::Stretch,
                ];
                let k = TOOLS.iter().position(|&x| x == self.tool).unwrap_or(0) as i64;
                let n = TOOLS.len() as i64;
                self.tool = TOOLS[(k + dir as i64).rem_euclid(n) as usize];
            }
            Shortcut::SetNudge(n) => {
                self.nudge = n;
                self.status = Some(format!("⌥← and ⌥→ now nudge by {}", n.name()));
            }
            Shortcut::DiscardRecording => {
                let Some(r) = self.recording.take() else {
                    return;
                };
                let from = match &r {
                    crate::workspace::record::Recording::Midi { start, .. }
                    | crate::workspace::record::Recording::Audio { start, .. } => *start,
                };
                drop(r);
                self.stop(cx);
                self.locate_tick(from, cx);
                self.status = Some("Recording thrown away".into());
            }
            Shortcut::SwitchKeys => {
                let Some(pane) = self.bottom else { return };
                self.tracks_keys = !self.tracks_keys;
                self.status = Some(match self.tracks_keys {
                    true => "Keys: the tracks".into(),
                    false => format!("Keys: the {}", pane.short()),
                });
            }
            _ => {}
        }
    }
}
