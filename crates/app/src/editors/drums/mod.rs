//! The Drum Machine, in the pane under the tracks: a kit's pads, to play, shape and swap.
//!
//! A kit is a kit track whose notes play its pads, each pad a track of its own (its sampler,
//! channel and effects) in the kit's stack. Down the left, the pads: four by four, the first at
//! the bottom left, a column beside them mapping every note so the grid can show any sixteen.
//! Click a pad to hear it (higher on it, harder) and pick it; drag it onto another to trade
//! places (the kit's patterns follow, so the beat sounds as it did); drop a sample from the
//! browser or the Finder on it to give it that sound, or on an empty one for a new pad. Each lights
//! as it plays. On the right, the pad picked: its sound (step through its folder's others and the
//! library's that sound like it, or the whole kit's at once), its layers (sounds stacked on it,
//! taking turns as a round robin, or split by how hard it's hit; drop a sample on its sound to
//! layer it), its waveform (drag its start and end), and its shaping: tune, level and pan;
//! attack, length and velocity; a pitch drop; the filter; punch, sub and noise. Along the bar: a
//! new kit fitted to the song, or one from a library folder.

mod grid;
mod pad;
mod workflow;

pub(crate) use workflow::pads_for;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    time::Instant,
};

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, Pixels, canvas, div, fill,
    point, prelude::*, px, rgb, size,
};
use pina_engine::{
    samples::Category,
    song::project::{Instrument, Layering, Sampler, Zone},
};

use crate::{
    sounds::browser::{category_color, primary},
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Bottom, Drag, Help, View, Workspace},
};

/// The pane's height when it opens: the grid and the pad's knobs.
pub const DRUMS_H: f32 = 470.0;
/// Pads the grid shows at a time.
pub const PADS: u8 = 16;

/// What the Drum Machine changes from its menus.
#[derive(Clone, Debug, PartialEq)]
pub enum DrumOp {
    /// A kit from a library folder: a new one, or into the kit track `into` (each of its pads'
    /// sounds given the folder's on that note, new pads for the rest).
    Folder { folder: String, into: Option<usize> },
    /// A library folder's kit as the browser's picks, to hear and change before it goes in.
    Picks { folder: String },
    /// A pad's sound (its layer `layer`'s): the library's sample `index`.
    Sound {
        note: u8,
        layer: usize,
        index: usize,
    },
    /// The library's sample `index` layered on a pad.
    Layer { note: u8, index: usize },
    /// A pad's layer taken out.
    Unlayer { note: u8, layer: usize },
    /// How a pad's layers play.
    Layering { note: u8, how: Layering },
    /// A pad's velocity layers blended into each other, or switched between.
    Blend { note: u8, on: bool },
    /// A pad's choke group.
    Choke { note: u8, group: Option<u8> },
    /// A pad taken out of the kit.
    Remove { note: u8 },
}

/// Where each pad was drawn, by its note, and in which frame: they're redrawn after the frame
/// that asks where they are, so it finds the last frame's.
pub type PadCells = Rc<RefCell<(u64, Vec<(u8, Bounds<Pixels>)>)>>;

pub struct DrumsUi {
    /// The lowest note the grid shows (a row of four at a time), and the pad picked, by note.
    pub from: u8,
    pub pad: u8,
    /// Where each pad was drawn, for dragging and dropping onto, and the frame drawing them;
    /// where the note map's top was.
    pub cells: PadCells,
    pub(crate) frame: u64,
    pub(crate) map_top: Rc<Cell<Option<f32>>>,
    /// The layer picked, on which pad; where the pad's sound strip was drawn, to drop a sample
    /// on to layer it.
    pub(crate) layer: (u8, usize),
    pub(crate) strip: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Each pad's sounds to step through, by its note and layer, as last worked out: the sound
    /// it had when they were (its library file), the library's samples, and which it's on.
    pub(crate) lanes: HashMap<(u8, usize), (PathBuf, Vec<usize>, usize)>,
    /// When each pad was last struck by hand, and how hard (0 to 1), for its flash.
    pub(crate) struck: HashMap<u8, (Instant, f32)>,
    /// Roles worked out for the library's files, by path.
    roles: RefCell<HashMap<PathBuf, Option<Category>>>,
}

impl Default for DrumsUi {
    fn default() -> Self {
        DrumsUi {
            from: 36,
            pad: 36,
            cells: Default::default(),
            frame: 0,
            map_top: Default::default(),
            layer: (36, 0),
            strip: Default::default(),
            lanes: Default::default(),
            struck: Default::default(),
            roles: Default::default(),
        }
    }
}

/// A note's name, as drum machines number their pads: C1 is 36.
pub(crate) fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    format!("{}{}", NAMES[n as usize % 12], n as i32 / 12 - 2)
}

/// What the General MIDI drum map has on a note.
fn gm_role(note: u8) -> Category {
    match note {
        35 | 36 => Category::Kick,
        37 => Category::Rim,
        38 | 40 => Category::Snare,
        39 => Category::Clap,
        42 | 44 => Category::HatClosed,
        46 => Category::HatOpen,
        41 | 43 | 45 | 47 | 48 | 50 => Category::Tom,
        49 | 52 | 55 | 57 => Category::Crash,
        51 | 53 | 59 => Category::Ride,
        54 => Category::Tambourine,
        69 | 70 => Category::Shaker,
        _ => Category::Perc,
    }
}

impl Workspace {
    /// The kit the Drum Machine shows: the selected track's, if it's a kit track or a pad.
    pub fn drum_kit(&self) -> Option<usize> {
        self.session.project.kit_of(self.track)
    }

    /// A kit's pad on a note, by track index, and its sampler.
    pub(crate) fn drum_pad(&self, kit: usize, note: u8) -> Option<(usize, &Sampler)> {
        let t = self.session.project.pad_on(kit, note)?;
        match &self.session.project.tracks[t].instrument {
            Some(Instrument::Sampler(s)) => Some((t, s)),
            _ => None,
        }
    }

    /// A pad's library file, if its sound came from the library.
    pub(crate) fn pad_from(&self, kit: usize, note: u8) -> Option<PathBuf> {
        self.layer_from(kit, note, 0)
    }

    /// A pad's layer's library file, if its sound came from the library.
    pub(crate) fn layer_from(&self, kit: usize, note: u8, layer: usize) -> Option<PathBuf> {
        let (_, s) = self.drum_pad(kit, note)?;
        let z = s.zones.get(layer)?;
        z.from
            .clone()
            .or_else(|| z.file.is_absolute().then(|| z.file.clone()))
    }

    /// What a pad is: its sound's kind as the library heard it, else as its name says, else the
    /// drum map's sound on its note.
    pub(crate) fn pad_role(&self, kit: usize, note: u8) -> Category {
        let heard = self.pad_from(kit, note).and_then(|from| {
            let mut roles = self.drums.roles.borrow_mut();
            *roles.entry(from.clone()).or_insert_with(|| {
                self.browser
                    .library
                    .samples
                    .iter()
                    .find(|s| s.path == from)
                    .map(|s| s.analysis.category)
            })
        });
        heard
            .or_else(|| {
                let t = self.session.project.pad_on(kit, note)?;
                let name = &self.session.project.tracks[t].name;
                pina_engine::song::commands::drum_note(name).map(gm_role)
            })
            .unwrap_or_else(|| gm_role(note))
    }

    pub(crate) fn pad_color(&self, kit: usize, note: u8) -> u32 {
        category_color(self.pad_role(kit, note))
    }

    /// A pad struck: heard through the kit (its shaping, channel and effects), at `velocity`.
    pub(crate) fn hit_pad(&mut self, kit: usize, note: u8, velocity: u8) {
        let id = self.session.project.tracks[kit].id;
        self.player.preview(id, note, velocity);
        self.player.preview(id, note, 0);
        self.drums
            .struck
            .insert(note, (Instant::now(), velocity as f32 / 127.0));
    }

    /// The layer picked on a pad: the first, unless another of its own was.
    pub(crate) fn pad_layer(&self, kit: usize, note: u8) -> usize {
        let n = self.drum_pad(kit, note).map_or(1, |(_, s)| s.zones.len());
        match self.drums.layer {
            (on, l) if on == note => l.min(n.saturating_sub(1)),
            _ => 0,
        }
    }

    /// Whether the Drum Machine is showing: the tracks' page, its tab open on a kit.
    fn drums_showing(&self) -> Option<usize> {
        let showing = self.view == View::Arrange && self.bottom == Some(Bottom::Drums);
        self.drum_kit().filter(|_| showing)
    }

    /// The pad whose sound strip is under a window point, to layer a sample dropped on it.
    pub(crate) fn strip_under(&self, p: gpui::Point<f32>) -> Option<u8> {
        let kit = self.drums_showing()?;
        let b = self.drums.strip.get()?;
        let note = self.drums.pad;
        (b.contains(&point(px(p.x), px(p.y))) && self.drum_pad(kit, note).is_some()).then_some(note)
    }

    /// The pad under a window point, as the grid was last drawn, by note.
    pub(crate) fn pad_under(&self, p: gpui::Point<f32>) -> Option<u8> {
        self.drums_showing()?;
        let at = point(px(p.x), px(p.y));
        self.drums
            .cells
            .borrow()
            .1
            .iter()
            .find(|(_, b)| b.contains(&at))
            .map(|c| c.0)
    }

    pub fn drums_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        self.drums.frame += 1;
        let Some(kit) = self.drum_kit() else {
            return self.drums_empty(cx);
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .bg(rgb(BG))
            .child(self.drum_grid(kit, cx))
            .child(self.pad_editor(kit, cx))
    }

    /// With no kit picked: what a kit is, and ways to make one.
    fn drums_empty(&mut self, cx: &mut Context<Self>) -> Div {
        let folders = !self.drum_folders().is_empty();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(BG))
            .child(
                div()
                    .w(px(460.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(14.))
                    .child(self.pad_glyph())
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::BOLD)
                            .child("Drum Machine"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(TEXT_3))
                            .text_center()
                            .child(
                                "Pick a kit track to play and shape its pads, or make a kit: \
                                 drums picked to go together and fit the song, or a folder of \
                                 your samples, a pad a sound.",
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                primary("New kit")
                                    .h(px(30.))
                                    .debug_selector(|| "drums-new-kit".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| {
                                            ws.new_drum_kit(cx);
                                        }),
                                    ),
                            )
                            .when(folders, |d| {
                                d.child(
                                    button("From a folder", false)
                                        .h(px(30.))
                                        .child(icon(Icon::Folder, 13.).text_color(rgb(TEXT_3)))
                                        .flex_row_reverse()
                                        .debug_selector(|| "drums-folder".into())
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                                ws.folder_menu(e.position, None, cx)
                                            }),
                                        ),
                                )
                            }),
                    ),
            )
    }

    /// Sixteen little pads, lit in a kit's colours: the empty pane's picture.
    fn pad_glyph(&self) -> Div {
        const LIT: [u32; 16] = [
            CORAL, LINE, PINK, LINE, TEAL, TEAL, LINE, TEAL, LINE, AMBER, LINE, LINE, CORAL, LINE,
            PINK, TEAL,
        ];
        div()
            .w(px(104.))
            .flex()
            .flex_wrap()
            .gap(px(4.))
            .children(LIT.map(|c| {
                div().size(px(23.)).rounded(px(5.)).bg(match c {
                    LINE => rgb(RAISED).into(),
                    c => alpha(c, 0.75),
                })
            }))
    }

    /// The bar's own controls: a new kit, a kit from a folder, and the whole kit's sounds
    /// stepped through at once.
    pub fn drums_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let kit = self.drum_kit();
        let step = |label: &'static str, i: Icon, by: i32, cx: &mut Context<Self>| {
            icon_seg(i, false, 24.)
                .debug_selector(move || format!("drums-kit-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.step_kit(by, cx);
                    }),
                )
        };
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .when(kit.is_some(), |d| {
                d.child(
                    div()
                        .h(px(28.))
                        .pl(px(8.))
                        .pr(px(2.))
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .rounded(px(8.))
                        .bg(rgb(BG))
                        .border_1()
                        .border_color(rgb(LINE))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .mr(px(2.))
                                .child("Kit"),
                        )
                        .child(step("back", Icon::Back, -1, cx))
                        .child(step("next", Icon::Forward, 1, cx)),
                )
            })
            .child(
                icon_seg(Icon::Folder, false, 28.)
                    .help("A kit from a library folder", cx)
                    .debug_selector(|| "drums-bar-folder".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.folder_menu(e.position, kit, cx);
                        }),
                    ),
            )
            .child(
                icon_seg(Icon::Shuffle, false, 28.)
                    .help("A new kit, picked to fit the song", cx)
                    .debug_selector(|| "drums-bar-new".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.new_drum_kit(cx);
                        }),
                    ),
            )
    }
}
