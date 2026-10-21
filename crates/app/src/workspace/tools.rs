//! The tracks' tools beyond the pointer, pencil, scissors and eraser, in the Tool menu (T):
//! Text renames a region; Glue joins one to the next on its track; Solo plays one alone while
//! held; Mute mutes one; Zoom zooms in where clicked (⌥ out) or onto the bars dragged across;
//! Fade draws an audio region's fade in or out; Flex makes dragging a region's end stretch it.

use gpui::{Context, MouseDownEvent, Pixels, Point};
use pina_engine::song::project::{At, Content, PPQ, Region};

use crate::{
    ui::menu::{Menu, MenuAction},
    ui::theme::Icon,
    workspace::{Drag, Name, Tool, Workspace, pt},
};

impl Tool {
    pub const ALL: [Tool; 11] = [
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
        Tool::Flex,
    ];

    /// The four always in the toolbar; the rest are a click (or T) away.
    pub fn main(self) -> bool {
        matches!(
            self,
            Tool::Pointer | Tool::Pencil | Tool::Scissors | Tool::Eraser
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::Pointer => "Pointer",
            Tool::Pencil => "Pencil",
            Tool::Scissors => "Scissors",
            Tool::Eraser => "Eraser",
            Tool::Text => "Text",
            Tool::Glue => "Glue",
            Tool::Solo => "Solo",
            Tool::Mute => "Mute",
            Tool::Zoom => "Zoom",
            Tool::Fade => "Fade",
            Tool::Flex => "Flex",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Tool::Pointer => Icon::Pointer,
            Tool::Pencil => Icon::Pencil,
            Tool::Scissors => Icon::Scissors,
            Tool::Eraser => Icon::Eraser,
            Tool::Text => Icon::Text,
            Tool::Glue => Icon::Glue,
            Tool::Solo => Icon::Headphones,
            Tool::Mute => Icon::Mute,
            Tool::Zoom => Icon::Zoom,
            Tool::Fade => Icon::Fade,
            Tool::Flex => Icon::Flex,
        }
    }

    /// What it does, as the status bar says it.
    pub fn hint(self) -> &'static str {
        match self {
            Tool::Pointer => "",
            Tool::Pencil => "Click an empty lane for a new region · in the piano roll, a note",
            Tool::Scissors => "Click a region or note to cut it there",
            Tool::Eraser => "Click a region or note to delete it",
            Tool::Text => "Click a region to name it",
            Tool::Glue => "Click a region to join it to the next one on its track",
            Tool::Solo => "Hold a region to hear it alone, from where you hold it",
            Tool::Mute => "Click a region to mute it, or unmute it",
            Tool::Zoom => {
                "Click to zoom in · ⌥-click out · drag across bars to fill the window with them"
            }
            Tool::Fade => "Drag in from an audio region's start or end to fade it in or out",
            Tool::Flex => "Drag a region's end to stretch what's in it",
        }
    }
}

impl Workspace {
    /// The Tool menu (T): every tool, the one in hand ticked.
    pub fn tool_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let items = Tool::ALL
            .iter()
            .map(|&t| {
                let tick = if t == self.tool { "✓ " } else { "" };
                (format!("{tick}{}", t.name()), MenuAction::Tool(t))
            })
            .collect();
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// A region joined to the next on its track (starting where it ends, or after): MIDI's notes
    /// together in one region; audio only where it was cut from one take (else a bounce joins
    /// it).
    pub fn glue(&mut self, at: At, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let a = p.region(at).clone();
        let end = a.start + a.length;
        let next = p.tracks[at.0]
            .regions
            .iter()
            .enumerate()
            .filter(|(i, r)| *i != at.1 && r.start >= end)
            .min_by_key(|(_, r)| r.start)
            .map(|(i, r)| (i, r.clone()));
        let Some((j, b)) = next else {
            self.status = Some("Nothing after it on its track to glue it to.".into());
            cx.notify();
            return;
        };
        let joined = match (&a.content, &b.content) {
            (
                Content::Midi { notes, controls },
                Content::Midi {
                    notes: more,
                    controls: moves,
                },
            ) => {
                let shift = (b.start - a.start) as i64;
                let mut notes = notes.clone();
                notes.extend(more.iter().map(|n| pina_engine::song::project::Note {
                    start: n.start + shift,
                    ..*n
                }));
                notes.sort_by_key(|n| n.start);
                let mut controls = controls.clone();
                controls.extend(
                    moves
                        .iter()
                        .map(|c| pina_engine::song::project::ControlPoint {
                            start: c.start + shift,
                            ..*c
                        }),
                );
                controls.sort_by_key(|c| c.start);
                Some(Content::Midi { notes, controls })
            }
            (
                Content::Audio {
                    file,
                    offset,
                    stretch,
                    ..
                },
                Content::Audio {
                    file: f2,
                    offset: o2,
                    stretch: s2,
                    ..
                },
            ) if file == f2 && stretch == s2 && b.start == end => {
                // Where the first leaves off in its file is where the second picks up.
                let samples = p.samples(a.length, p.sample_rate as f64) as f64 / stretch;
                ((*offset as f64 + samples - *o2 as f64).abs() < 4.0).then(|| a.content.clone())
            }
            (Content::Audio { .. }, Content::Audio { .. }) => None,
            _ => {
                self.status = Some("Glue joins MIDI regions, or audio cut from one take.".into());
                cx.notify();
                return;
            }
        };
        let Some(content) = joined else {
            self.status =
                Some("These weren't cut from one take: Bounce in Place joins any audio.".into());
            cx.notify();
            return;
        };
        let length = b.start + b.length - a.start;
        self.edit(cx, |ws| {
            let regions = &mut ws.session.project.tracks[at.0].regions;
            regions[at.1] = Region {
                length,
                content,
                link: None,
                fade_out: b.fade_out,
                ..a
            };
            regions.remove(j);
            let i = if j < at.1 { at.1 - 1 } else { at.1 };
            ws.region = Some((at.0, i));
        });
    }

    /// The Solo tool held on a region: it plays alone, from where it's held, until let go.
    pub fn solo_hold(&mut self, at: At, tick: u64, cx: &mut Context<Self>) {
        self.solo_restore = Some((
            self.pattern_mode,
            self.pattern,
            self.player.playing(),
            self.region,
        ));
        self.region = Some(at);
        self.set_pattern_mode(true, cx);
        let r = self.session.project.region(at);
        let from = tick.clamp(r.start, r.start + r.length.saturating_sub(1));
        self.locate_tick(from, cx);
        self.player.play();
        self.begin_drag(Drag::SoloHold, cx);
    }

    /// Let go: back as it was.
    pub fn solo_release(&mut self, cx: &mut Context<Self>) {
        let Some((mode, pattern, playing, region)) = self.solo_restore.take() else {
            return;
        };
        if !playing {
            self.player.stop();
        }
        self.pattern = pattern;
        self.region = region;
        self.set_pattern_mode(mode, cx);
    }

    /// The Zoom tool pressed at a window x: ⌥ zooms out there; otherwise a drag picks the bars to
    /// fill the window with (a click zooms in).
    pub fn zoom_press(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let x = f32::from(e.position.x);
        let left = f32::from(self.lanes_bounds.get().origin.x);
        if e.modifiers.alt {
            self.zoom_around(self.px_per_beat / 2.0, x - left);
            cx.notify();
            return;
        }
        self.zoom_span = Some((x, x));
        self.begin_drag(Drag::ZoomSpan, cx);
    }

    /// The Zoom tool let go: onto the bars dragged across, or in where it was clicked.
    pub fn zoom_release(&mut self, cx: &mut Context<Self>) {
        let Some((a, b)) = self.zoom_span.take() else {
            return;
        };
        let left = f32::from(self.lanes_bounds.get().origin.x);
        if (b - a).abs() < 6.0 {
            self.zoom_around(self.px_per_beat * 2.0, a - left);
        } else {
            let (t0, t1) = (self.tick_at(a.min(b)), self.tick_at(a.max(b)));
            let width = f32::from(self.lanes_bounds.get().size.width).max(100.0);
            let beats = ((t1 - t0) / PPQ as f64).max(0.25) as f32;
            self.px_per_beat = (width / beats).clamp(4.0, 800.0);
            self.scroll_x = (t0 as f32 * self.px_per_tick()).max(0.0);
        }
        cx.notify();
    }

    /// The Fade tool pressed on a region: its fade in (the near half) or out (the far half).
    pub fn fade_press(&mut self, at: At, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let r = self.session.project.region(at);
        if !matches!(r.content, Content::Audio { .. }) {
            self.status = Some("Fades are for audio regions.".into());
            cx.notify();
            return;
        }
        let middle = r.start as f64 + r.length as f64 / 2.0;
        let out = self.tick_at(f32::from(e.position.x)) > middle;
        self.region = Some(at);
        self.begin_drag(Drag::Fade { at, out }, cx);
        self.drag_fade(at, out, pt(e.position));
    }

    /// A fade dragged to the pointer: as long as from the region's end to it.
    pub fn drag_fade(&mut self, at: At, out: bool, p: Point<f32>) {
        let tick = self.tick_at(p.x).round() as u64;
        let r = &mut self.session.project.tracks[at.0].regions[at.1];
        let end = r.start + r.length;
        match out {
            false => {
                r.fade_in = tick
                    .saturating_sub(r.start)
                    .min(r.length - r.fade_out.min(r.length))
            }
            true => {
                r.fade_out = end
                    .saturating_sub(tick)
                    .min(r.length - r.fade_in.min(r.length))
            }
        }
        let (ms_in, ms_out) = (
            r.fade_in as f64 / PPQ as f64 * 60.0 / self.session.project.tempo * 1000.0,
            r.fade_out as f64 / PPQ as f64 * 60.0 / self.session.project.tempo * 1000.0,
        );
        self.status = Some(format!("Fade in {ms_in:.0} ms · fade out {ms_out:.0} ms"));
    }

    /// The span a Zoom drag covers, in the lanes (x from their left), to draw.
    pub fn zoom_shown(&self) -> Option<(f32, f32)> {
        let (a, b) = self.zoom_span?;
        let left = f32::from(self.lanes_bounds.get().origin.x);
        ((b - a).abs() >= 6.0).then(|| (a.min(b) - left, (a - b).abs()))
    }

    /// Text on a region: its name, in its place.
    pub fn name_region(&mut self, at: At, cx: &mut Context<Self>) {
        self.region = Some(at);
        self.select_track(at.0);
        self.pending_name = Some(Name::Region(at));
        cx.notify();
    }
}
