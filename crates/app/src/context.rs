//! The right-click menus of the tracks area, as Logic's: a track's header (new tracks, the same
//! strip, duplicating, renaming, its colour and icon, mute, solo, record, its height, its pan,
//! moving it, stacking, bouncing, deleting), a pan knob (Balance, Stereo Pan, Binaural, and the
//! song's pan law), a region (cut, copy, paste, duplicate, split, mute, loop, rename, alias, into
//! a pattern or MIDI, quantize, bounce, open in its editor, delete) and an empty stretch of a lane
//! (paste, a new MIDI or pattern region, a marker, an audio file there). Regions copy and paste
//! with ⌘C ⌘X ⌘V, repeat with ⌘R and mute with ⌃M too.

use gpui::{Context, PathPromptOptions, Pixels, Point, point, px};
use tempo_engine::{
    music::NoteEdit,
    project::{At, Content, Instrument, PanLaw, PanMode, Region, Track},
};

use crate::{
    menu::{Menu, MenuAction},
    workspace::{Bottom, Name, Workspace},
};

/// Something done to a track, or to the tracks, from a menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackOp {
    NewAudio,
    NewInstrument,
    /// A new track with this one's instrument, plugins, sends and settings, empty.
    SameStrip(usize),
    /// This one again, regions and all.
    Duplicate(usize),
    Delete(usize),
    Mute(usize),
    Solo(usize),
    Arm(usize),
    Height(usize, Option<f32>),
    PanMode(usize, PanMode),
    PanLaw(PanLaw),
    ResetPan(usize),
    Move(usize, i32),
    Stack(usize),
    Look(usize, Point<Pixels>),
    /// The Binaural Panner, on a track.
    Panner(usize),
    /// Its modulators, in the editor.
    Modulation(usize),
    Automation,
    /// On an empty stretch of a lane, at a tick.
    NewRegion(usize, u64),
    NewPattern(usize, u64),
    Marker(u64),
    Paste(usize, u64),
    Import(usize, u64),
}

/// Something done to a region, from its menu or a key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RegionOp {
    Cut,
    Copy,
    Delete,
    Duplicate,
    Split,
    Mute,
    Loop,
    Rename,
    Alias,
    ToPattern,
    ToMidi,
    Quantize,
    Bounce,
    Open,
}

/// A rule between groups of a menu's items.
fn rule() -> (String, MenuAction) {
    (String::new(), MenuAction::Nothing)
}

fn ticked(on: bool, label: &str) -> String {
    format!("{}{label}", if on { "✓ " } else { "" })
}

impl Workspace {
    fn open_menu(
        &mut self,
        at: Point<Pixels>,
        items: Vec<(String, MenuAction)>,
        cx: &mut Context<Self>,
    ) {
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// A pan knob's menu: its mode, the song's pan law, and back to the middle.
    pub fn pan_menu(&mut self, t: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(track) = self.session.project.tracks.get(t) else {
            return;
        };
        let (mode, law) = (track.pan_mode, self.session.project.pan_law);
        let op = |label: String, o: TrackOp| (label, MenuAction::Track(o));
        let mut items = vec![
            op(
                ticked(mode == PanMode::Balance, "Balance"),
                TrackOp::PanMode(t, PanMode::Balance),
            ),
            op(
                ticked(mode == PanMode::Stereo, "Stereo Pan\t⌘-drag spreads"),
                TrackOp::PanMode(t, PanMode::Stereo),
            ),
            op(
                ticked(mode == PanMode::Binaural, "Binaural"),
                TrackOp::PanMode(t, PanMode::Binaural),
            ),
            rule(),
            ("Pan Law (all tracks)".into(), MenuAction::Nothing),
        ];
        for l in [
            PanLaw::Flat,
            PanLaw::Minus3,
            PanLaw::Minus3Compensated,
            PanLaw::Minus6,
        ] {
            items.push(op(ticked(law == l, l.name()), TrackOp::PanLaw(l)));
        }
        items.push(rule());
        if mode == PanMode::Binaural {
            items.push(op(
                "Binaural Panner…\tdouble-click".into(),
                TrackOp::Panner(t),
            ));
        }
        items.push(op("Centre\t⌥-click".into(), TrackOp::ResetPan(t)));
        self.open_menu(at, items, cx);
    }

    /// A track header's menu.
    pub fn track_menu(&mut self, t: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(track) = self.session.project.tracks.get(t) else {
            return;
        };
        let (mute, solo, armed) = (track.mute, track.solo, self.armed() == Some(t));
        let mode = track.pan_mode;
        let n = self.session.project.tracks.len();
        let op = |label: String, o: TrackOp| (label, MenuAction::Track(o));
        let items = vec![
            op("New Audio Track".into(), TrackOp::NewAudio),
            op(
                "New Software Instrument Track".into(),
                TrackOp::NewInstrument,
            ),
            op(
                "New Track with the Same Channel Strip".into(),
                TrackOp::SameStrip(t),
            ),
            op("Duplicate Track".into(), TrackOp::Duplicate(t)),
            rule(),
            ("Rename…".to_string(), MenuAction::Rename(Name::Track(t))),
            op("Colour and Icon…".into(), TrackOp::Look(t, at)),
            rule(),
            op(ticked(mute, "Mute"), TrackOp::Mute(t)),
            op(ticked(solo, "Solo"), TrackOp::Solo(t)),
            op(ticked(armed, "Record Enable"), TrackOp::Arm(t)),
            op(
                ticked(self.show_automation, "Show Automation\tA"),
                TrackOp::Automation,
            ),
            rule(),
            op("Height: Small".into(), TrackOp::Height(t, Some(48.0))),
            op("Height: Medium".into(), TrackOp::Height(t, None)),
            op("Height: Large".into(), TrackOp::Height(t, Some(120.0))),
            op("Height: Huge".into(), TrackOp::Height(t, Some(200.0))),
            rule(),
            op(
                ticked(mode == PanMode::Balance, "Pan: Balance"),
                TrackOp::PanMode(t, PanMode::Balance),
            ),
            op(
                ticked(mode == PanMode::Stereo, "Pan: Stereo Pan"),
                TrackOp::PanMode(t, PanMode::Stereo),
            ),
            op(
                ticked(mode == PanMode::Binaural, "Pan: Binaural"),
                TrackOp::PanMode(t, PanMode::Binaural),
            ),
            rule(),
            match t > 0 {
                true => op("Move Up".into(), TrackOp::Move(t, -1)),
                false => ("Move Up".into(), MenuAction::Nothing),
            },
            match t + 1 < n {
                true => op("Move Down".into(), TrackOp::Move(t, 1)),
                false => ("Move Down".into(), MenuAction::Nothing),
            },
            op("Create Track Stack\t⇧⌘D".into(), TrackOp::Stack(t)),
            rule(),
            op("Modulation…".into(), TrackOp::Modulation(t)),
            ("Bounce in Place".to_string(), MenuAction::BounceInPlace(t)),
            ("Export as Stem…".to_string(), MenuAction::ExportStem(t)),
            rule(),
            op("Delete Track\t⌘⌫".into(), TrackOp::Delete(t)),
        ];
        self.open_menu(at, items, cx);
    }

    /// A region's menu.
    pub fn region_menu(&mut self, r: At, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(region) = self
            .session
            .project
            .tracks
            .get(r.0)
            .and_then(|t| t.regions.get(r.1))
        else {
            return;
        };
        let (mute, looped, linked) = (region.mute, region.looped, region.link.is_some());
        let midi = matches!(region.content, Content::Midi { .. });
        let pattern = matches!(region.content, Content::Pattern { .. });
        let tick = self.playhead();
        let inside = (region.start + 1..region.start + region.length).contains(&tick);
        let op = |label: &str, o: RegionOp| (label.to_string(), MenuAction::Region(r, o));
        let mut items = vec![
            op("Cut\t⌘X", RegionOp::Cut),
            op("Copy\t⌘C", RegionOp::Copy),
            (
                "Paste at Playhead\t⌘V".to_string(),
                if self.region_clip.is_some() {
                    MenuAction::Track(TrackOp::Paste(r.0, tick))
                } else {
                    MenuAction::Nothing
                },
            ),
            op("Repeat After Itself\t⌘R", RegionOp::Duplicate),
            (
                "Split at Playhead\t⌘T".to_string(),
                if inside {
                    MenuAction::Region(r, RegionOp::Split)
                } else {
                    MenuAction::Nothing
                },
            ),
            rule(),
            (
                ticked(mute, "Mute\t⌃M"),
                MenuAction::Region(r, RegionOp::Mute),
            ),
            (
                ticked(looped, "Loop"),
                MenuAction::Region(r, RegionOp::Loop),
            ),
            op("Rename…", RegionOp::Rename),
            op("Make Alias", RegionOp::Alias),
        ];
        if linked {
            items.push(("Make It Its Own".into(), MenuAction::Unlink(r)));
        }
        items.push(rule());
        if midi {
            items.push(op("Open in Piano Roll", RegionOp::Open));
            items.push(op("Quantize Notes", RegionOp::Quantize));
            items.push(op("Make It a Step Pattern", RegionOp::ToPattern));
        }
        if pattern {
            items.push(op("Open in Step Sequencer", RegionOp::Open));
            items.push(op("Make It MIDI", RegionOp::ToMidi));
        }
        items.push(op("Bounce in Place", RegionOp::Bounce));
        items.push(rule());
        items.push(op("Delete\t⌫", RegionOp::Delete));
        self.open_menu(at, items, cx);
    }

    /// An empty stretch of a lane's menu, for the tick clicked.
    pub fn lane_menu(&mut self, t: usize, tick: u64, at: Point<Pixels>, cx: &mut Context<Self>) {
        let instrument = self
            .session
            .project
            .tracks
            .get(t)
            .is_some_and(|t| t.instrument.is_some());
        let op = |label: &str, o: TrackOp| (label.to_string(), MenuAction::Track(o));
        let mut items = vec![(
            "Paste Here\t⌘V".to_string(),
            if self.region_clip.is_some() {
                MenuAction::Track(TrackOp::Paste(t, tick))
            } else {
                MenuAction::Nothing
            },
        )];
        if instrument {
            items.push(op("New MIDI Region Here", TrackOp::NewRegion(t, tick)));
            items.push(op("New Step Pattern Here", TrackOp::NewPattern(t, tick)));
        } else {
            items.push(op("Add an Audio File Here…", TrackOp::Import(t, tick)));
        }
        items.push(op("Add a Marker Here", TrackOp::Marker(tick)));
        items.push(rule());
        items.push(op("New Audio Track", TrackOp::NewAudio));
        items.push(op("New Software Instrument Track", TrackOp::NewInstrument));
        self.open_menu(at, items, cx);
    }

    /// The window position of a tick on a track's lane: where a menu's action lands.
    fn lane_point(&self, t: usize, tick: u64) -> Point<Pixels> {
        let b = self.lanes_bounds.get();
        let x = f32::from(b.origin.x) + tick as f32 * self.px_per_tick() - self.scroll_x;
        let y = f32::from(b.origin.y) + self.row_top(t) + self.lane_h(t) / 2.0;
        point(px(x.max(f32::from(b.origin.x) + 1.0)), px(y))
    }

    pub fn track_op(&mut self, op: TrackOp, cx: &mut Context<Self>) {
        let n = self.session.project.tracks.len();
        match op {
            TrackOp::NewAudio => self.new_track(None, cx),
            TrackOp::NewInstrument => self.new_track(Some(Instrument::Synth), cx),
            TrackOp::SameStrip(t) | TrackOp::Duplicate(t) if t < n => {
                let mut copy: Track = self.session.project.tracks[t].clone();
                copy.name = format!("{} 2", copy.name);
                copy.solo = false;
                match op {
                    TrackOp::SameStrip(_) => {
                        copy.regions.clear();
                        copy.cells.clear();
                        copy.automation = Default::default();
                    }
                    // Its regions its own, not aliases of the first's.
                    _ => copy.regions.iter_mut().for_each(|r| r.link = None),
                }
                self.edit(cx, |ws| {
                    ws.session.project.add_track(t + 1, copy);
                    ws.select_track(t + 1);
                    ws.region = None;
                });
            }
            TrackOp::Delete(t) if t < n => {
                self.select_track(t);
                self.delete_track(cx);
            }
            TrackOp::Mute(t) if t < n => {
                self.edit(cx, |ws| ws.session.project.tracks[t].mute ^= true)
            }
            TrackOp::Solo(t) if t < n => {
                self.edit(cx, |ws| ws.session.project.tracks[t].solo ^= true)
            }
            TrackOp::Arm(t) if t < n => self.toggle_arm(t),
            TrackOp::Height(t, h) if t < n => {
                self.session.project.tracks[t].height = h;
                self.session.edited = true;
            }
            TrackOp::PanMode(t, mode) if t < n => {
                self.edit(cx, |ws| ws.session.project.tracks[t].pan_mode = mode);
                // Binaural opens its panner, as Logic's does.
                if mode == PanMode::Binaural {
                    self.open_panner(t, cx);
                }
            }
            TrackOp::Panner(t) if t < n => self.open_panner(t, cx),
            TrackOp::Modulation(t) if t < n => {
                self.select_track(t);
                self.show_bottom(Bottom::Modulation, cx);
            }
            TrackOp::PanLaw(law) => self.edit(cx, |ws| ws.session.project.pan_law = law),
            TrackOp::ResetPan(t) if t < n => self.edit(cx, |ws| {
                ws.session.project.tracks[t].pan = 0.0;
                ws.session.project.tracks[t].spread = None;
                ws.session.project.tracks[t].elevation = None;
                ws.session.project.tracks[t].distance = None;
            }),
            TrackOp::Move(t, by) if t < n => {
                let to = (t as i64 + by as i64).clamp(0, n as i64 - 1) as usize;
                if to != t {
                    self.edit(cx, |ws| {
                        ws.session.project.tracks.swap(t, to);
                        ws.select_track(to);
                        ws.region = None;
                    });
                }
            }
            TrackOp::Stack(t) if t < n => self.edit(cx, |ws| {
                ws.session.project.add_stack(&[t]);
            }),
            TrackOp::Look(t, at) if t < n => self.pending_look = Some((t, at)),
            TrackOp::Automation => self.show_automation ^= true,
            TrackOp::NewRegion(t, tick) if t < n => {
                let x = f32::from(self.lane_point(t, tick).x);
                self.select_track(t);
                if self.draw_region(t, x, cx).is_some() {
                    self.show_bottom(Bottom::PianoRoll, cx);
                }
            }
            TrackOp::NewPattern(t, tick) if t < n => {
                self.select_track(t);
                self.region = None;
                self.new_pattern_at(Some(tick), cx);
            }
            TrackOp::Marker(tick) => {
                let k = self.session.project.markers.len();
                let name = crate::menu::MARKER_NAMES
                    .get(k)
                    .map_or(format!("Marker {}", k + 1), |s| s.to_string());
                self.choose(MenuAction::AddMarker(tick, name), cx);
            }
            TrackOp::Paste(t, tick) if t < n => self.paste_region(t, tick, cx),
            TrackOp::Import(t, tick) if t < n => {
                let at = self.lane_point(t, tick);
                let picked = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: true,
                    prompt: Some("Add".into()),
                });
                cx.spawn(async move |this, cx| {
                    let Ok(Ok(Some(paths))) = picked.await else {
                        return;
                    };
                    this.update(cx, |ws, cx| ws.drop_files(&paths, at.map(f32::from), cx))
                        .ok();
                })
                .detach();
            }
            _ => {}
        }
        cx.notify();
    }

    /// The copied region on a track at a tick (snapped), if the track plays its kind.
    fn paste_region(&mut self, t: usize, tick: u64, cx: &mut Context<Self>) {
        let Some(mut r) = self.region_clip.clone() else {
            return;
        };
        let audio = !r.content.is_midi();
        let fits = self
            .session
            .project
            .tracks
            .get(t)
            .is_some_and(|tr| tr.instrument.is_none() == audio);
        if !fits {
            self.status = Some(match audio {
                true => "That's audio: paste it on an audio track.".into(),
                false => "That's notes: paste it on an instrument track.".into(),
            });
            cx.notify();
            return;
        }
        r.start = self.snap(tick as f64);
        self.edit(cx, |ws| {
            let regions = &mut ws.session.project.tracks[t].regions;
            regions.push(r);
            ws.region = Some((t, regions.len() - 1));
            ws.select_track(t);
        });
    }

    pub fn region_op(&mut self, at: At, op: RegionOp, cx: &mut Context<Self>) {
        let Some(region) = self
            .session
            .project
            .tracks
            .get(at.0)
            .and_then(|t| t.regions.get(at.1))
            .cloned()
        else {
            return;
        };
        match op {
            RegionOp::Copy | RegionOp::Cut => {
                self.region_clip = Some(Region {
                    link: None,
                    ..region.clone()
                });
                self.status = Some(format!(
                    "Copied {}",
                    if region.name.is_empty() {
                        "the region".to_string()
                    } else {
                        region.name.clone()
                    }
                ));
                if op == RegionOp::Cut {
                    self.region_op(at, RegionOp::Delete, cx);
                }
            }
            RegionOp::Delete => self.edit(cx, |ws| {
                ws.session.project.tracks[at.0].regions.remove(at.1);
                ws.region = None;
            }),
            // Again, right after itself.
            RegionOp::Duplicate => self.edit(cx, |ws| {
                let regions = &mut ws.session.project.tracks[at.0].regions;
                regions.push(Region {
                    start: region.start + region.length,
                    ..region.clone()
                });
                ws.region = Some((at.0, regions.len() - 1));
            }),
            RegionOp::Split => {
                let tick = self.playhead();
                self.split_at(at, tick, cx);
            }
            RegionOp::Mute => self.edit(cx, |ws| {
                let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                r.mute ^= true;
            }),
            RegionOp::Loop => self.edit(cx, |ws| {
                let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                r.looped ^= true;
            }),
            RegionOp::Rename => self.pending_name = Some(Name::Region(at)),
            RegionOp::Alias => self.choose(MenuAction::Alias(at), cx),
            RegionOp::ToPattern => self.midi_to_pattern(at, cx),
            RegionOp::ToMidi => self.pattern_to_midi(at, cx),
            RegionOp::Quantize => {
                let edit = NoteEdit::Quantize {
                    grid: self.roll.grid,
                    strength: self.roll.strength,
                    swing: self.roll.swing,
                };
                self.picked.clear();
                self.edit_notes(at, edit, cx);
            }
            RegionOp::Bounce => self.bounce_range_in_place(
                at.0,
                Some((region.start, region.start + region.length)),
                cx,
            ),
            RegionOp::Open => {
                self.region = Some(at);
                self.select_track(at.0);
                let pane = match region.content {
                    Content::Pattern { .. } => Bottom::Steps,
                    Content::Takes { .. } => Bottom::Takes,
                    _ => Bottom::PianoRoll,
                };
                self.show_bottom(pane, cx);
            }
        }
        cx.notify();
    }

    /// ⌘C ⌘X ⌘V ⌘R ⌃M on the tracks: the selected region (pasting on the selected track, at the
    /// playhead).
    pub fn region_key(&mut self, op: RegionOp, cx: &mut Context<Self>) {
        match self.region {
            Some(at) => self.region_op(at, op, cx),
            None => {
                if op == RegionOp::Copy || op == RegionOp::Cut {
                    self.status = Some("Select a region to copy it.".into());
                    cx.notify();
                }
            }
        }
    }

    pub fn paste_key(&mut self, cx: &mut Context<Self>) {
        let tick = self.playhead();
        let t = self.track;
        self.paste_region(t, tick, cx);
    }
}
