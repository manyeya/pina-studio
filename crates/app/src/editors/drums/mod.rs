//! The Drum Machine, in the pane under the tracks: a kit as a drum rack, its devices in a row
//! the way a device chain is laid out, each the same height whatever the pane's.
//!
//! A kit is a kit track whose notes play its pads, each pad a track of its own (its sampler,
//! channel and effects) in the kit's stack: a drum rack's chains.
//!
//! - The Drum Rack: its view column (show or hide its macros, chain list and devices, the
//!   chain list's I/O, sends and returns; auto select; the swap buttons on its pads); its
//!   macros; its pads, sixteen of the 128 notes at a time beside an overview of them all (drag
//!   it, or ↑ ↓ a bank, ⌥ a row), each named, with mute, preview and solo, and the sounds like
//!   it either side and a lock when its swap buttons show (click a pad to hear it, higher
//!   harder; drag one onto another to trade; drop samples on them); its chain list, a row a
//!   pad: on, solo and hot-swap, the note it receives and plays, its choke group, its volume,
//!   pan and sends; its returns.
//! - The picked pad's devices: its Drum Sampler (the sample, its start and length and gain,
//!   attack, hold and decay, Trigger or Gate, transpose and detune; a playback effect on an X/Y
//!   pad; the filter; volume, pan, velocity and what velocity or slide moves), then its
//!   effects, each opening its window.
//! - Hot-swap: the library's sounds for the pad, beside the devices: like it, its pack's, or
//!   its kind; a click puts one on, ↑ ↓ step through them.
//!
//! Along the bar: a kit fitted to the song, or one from a folder of the library.

mod chains;
mod effects;
mod rack;
mod sampler;
mod sounds;
mod workflow;

pub(crate) use sounds::Source;

pub(crate) use workflow::pads_for;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
    time::Instant,
};

use gpui::{
    Bounds, Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, Pixels,
    UniformListScrollHandle, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    samples::Category,
    song::project::{DrumModDest, FilterMode, Instrument, Layering, PlaybackFx, Sampler, Zone},
};

use crate::{
    sounds::browser::{category_color, primary},
    ui::menu::{Menu, MenuAction},
    ui::text::TextInput,
    ui::theme::*,
    workspace::{Bottom, Drag, Help, View, Workspace},
};

/// The pane's height when it opens: a device's, its bar and a margin.
pub const DRUMS_H: f32 = 470.0;

/// Every device's height, as in a device chain: its title and what it holds.
pub(crate) const DEVICE_H: f32 = 252.0;
const TITLE_H: f32 = 24.0;

/// What the Drum Machine changes from its menus.
#[derive(Clone, Debug, PartialEq)]
pub enum DrumOp {
    /// A kit from a library folder: a new one, or into the kit track `into` (each of its pads'
    /// sounds given the folder's on that note, new pads for the rest).
    Folder {
        folder: String,
        into: Option<usize>,
    },
    /// A library folder's kit as the browser's picks, to hear and change before it goes in.
    Picks {
        folder: String,
    },
    /// A pad's sound (its layer `layer`'s): the library's sample `index`.
    Sound {
        note: u8,
        layer: usize,
        index: usize,
    },
    /// The library's sample `index` layered on a pad.
    Layer {
        note: u8,
        index: usize,
    },
    /// A pad's layer taken out.
    Unlayer {
        note: u8,
        layer: usize,
    },
    /// How a pad's layers play.
    Layering {
        note: u8,
        how: Layering,
    },
    /// A pad's velocity layers blended into each other, or switched between.
    Blend {
        note: u8,
        on: bool,
    },
    /// A pad's choke group (1 to 16).
    Choke {
        note: u8,
        group: Option<u8>,
    },
    /// A pad taken out of the kit.
    Remove {
        note: u8,
    },
    /// A pad moved to another note (trading places with a pad there): the note it receives.
    Note {
        note: u8,
        to: u8,
    },
    /// The note a pad's sampler is played (none: the one it receives).
    Play {
        note: u8,
        play: Option<u8>,
    },
    /// Drums picked to go together and fit the song, loaded into the kit.
    Fit,
    /// The Drum Sampler's playback effect, filter and what its modulation moves.
    Fx {
        note: u8,
        fx: PlaybackFx,
    },
    Filter {
        note: u8,
        mode: FilterMode,
    },
    ModDest {
        note: u8,
        dest: DrumModDest,
    },
    /// The waveform's menu: hot-swap showing what's like it; the sound its likeness is
    /// measured from put back, or the one on it kept as that; played backwards.
    Similar {
        note: u8,
    },
    Reference {
        note: u8,
        layer: usize,
    },
    KeepReference {
        note: u8,
        layer: usize,
    },
    Reverse {
        note: u8,
        layer: usize,
    },
    /// A pad copied to the next free note; locked or let go; its chain out on a track of its
    /// own.
    Duplicate {
        note: u8,
    },
    Lock {
        note: u8,
    },
    Extract {
        note: u8,
    },
    /// One of the rack's options switched.
    Option(RackOption),
}

/// Where each pad was drawn, by its note, and in which frame: they're redrawn after the frame
/// that asks where they are, so it finds the last frame's.
pub type PadCells = Rc<RefCell<(u64, Vec<(u8, Bounds<Pixels>)>)>>;

/// What the rack's panel shows, by its tabs: the picked pad's Drum Sampler, its effects, the
/// chain list, the macros and their variations, or hot-swap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackTab {
    Sampler,
    Effects,
    Chains,
    Macros,
    Swap,
}

/// The rack's options: the chain list's I/O, sends and returns; auto select; the swap buttons
/// on its pads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackOption {
    Io,
    Sends,
    Returns,
    Auto,
    Swaps,
}

/// The rack's options, as set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RackViews {
    pub io: bool,
    pub sends: bool,
    pub returns: bool,
    /// A pad that plays is picked.
    pub auto: bool,
    /// The sounds like a pad's, either side of it on the pad, and its lock.
    pub swaps: bool,
}

impl RackViews {
    pub fn on(&mut self, o: RackOption) -> &mut bool {
        match o {
            RackOption::Io => &mut self.io,
            RackOption::Sends => &mut self.sends,
            RackOption::Returns => &mut self.returns,
            RackOption::Auto => &mut self.auto,
            RackOption::Swaps => &mut self.swaps,
        }
    }
}

impl Default for RackViews {
    fn default() -> Self {
        RackViews {
            io: true,
            sends: false,
            returns: false,
            auto: false,
            swaps: true,
        }
    }
}

pub struct DrumsUi {
    /// The pad picked, by note.
    pub pad: u8,
    /// The lowest note of the sixteen pads shown (a row's first, a multiple of four).
    pub view: u8,
    pub views: RackViews,
    pub tab: RackTab,
    /// The pads' size (width, height), as the pane's height has them.
    pub(crate) pad_size: (f32, f32),
    /// How many macro knobs show (1 to 16), and whether Map is on.
    pub macros: usize,
    pub mapping: bool,
    /// Where each pad was drawn, for dragging and dropping onto, and the frame drawing them.
    pub cells: PadCells,
    pub(crate) frame: u64,
    /// Where the overview was drawn, to find the row under the pointer.
    pub(crate) overview: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The layer picked, on which pad; where the pad's sound strip was drawn, to drop a sample
    /// on to layer it.
    pub(crate) layer: (u8, usize),
    pub(crate) strip: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Each pad's sounds to step through, by its note and layer, as last worked out: the sound
    /// it had when they were (its library file), the library's samples, and which it's on.
    pub(crate) lanes: HashMap<(u8, usize), (PathBuf, Vec<usize>, usize)>,
    /// When each pad was last struck by hand, and how hard (0 to 1), for its flash.
    pub(crate) struck: HashMap<u8, (Instant, f32)>,
    /// Each pad's level as last drawn, for auto select to see which started playing.
    pub(crate) heard: HashMap<u8, f32>,
    /// Roles worked out for the library's files, by path.
    roles: RefCell<HashMap<PathBuf, Option<Category>>>,
    /// Hot-swap: where its sounds come from, the words narrowing them, what it found (worked
    /// out again only when what it's for changes), its scroll, and whether it shows.
    pub(crate) source: Source,
    pub(crate) find: Option<Entity<TextInput>>,
    pub(crate) list: Option<sounds::SoundList>,
    pub(crate) scroll: UniformListScrollHandle,
    /// The rack panel's scroll, down, when what it shows is taller than the pane.
    pub(crate) devices_scroll: gpui::ScrollHandle,
    /// Hot-swap's target: the kit as a whole (its kits to load), not the picked pad.
    pub(crate) swap_rack: bool,
    pub(crate) find_subs: Vec<gpui::Subscription>,
    /// A sound pressed in hot-swap (for its pad and layer): let go without dragging, it goes
    /// on the pad.
    pub(crate) pressed: Option<(u8, usize, usize)>,
}

impl Default for DrumsUi {
    fn default() -> Self {
        DrumsUi {
            pad: 36,
            view: 36,
            views: RackViews::default(),
            tab: RackTab::Sampler,
            pad_size: (72.0, 56.0),
            macros: 8,
            mapping: false,
            cells: Default::default(),
            frame: 0,
            overview: Default::default(),
            layer: (36, 0),
            strip: Default::default(),
            lanes: Default::default(),
            struck: Default::default(),
            heard: Default::default(),
            roles: Default::default(),
            source: Source::Like,
            find: None,
            list: None,
            scroll: UniformListScrollHandle::new(),
            devices_scroll: gpui::ScrollHandle::new(),
            swap_rack: false,
            find_subs: Vec::new(),
            pressed: None,
        }
    }
}

/// A note's name, as the piano roll names it.
pub(crate) fn note_name(n: u8) -> String {
    crate::editors::pianoroll::name(n)
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

/// The General MIDI drum map's name for a note, where it has one.
pub(crate) fn gm_name(note: u8) -> Option<&'static str> {
    Some(match note {
        35 => "Acoustic Bass Drum",
        36 => "Bass Drum",
        37 => "Side Stick",
        38 => "Acoustic Snare",
        39 => "Hand Clap",
        40 => "Electric Snare",
        41 => "Low Floor Tom",
        42 => "Closed Hi-Hat",
        43 => "High Floor Tom",
        44 => "Pedal Hi-Hat",
        45 => "Low Tom",
        46 => "Open Hi-Hat",
        47 => "Low-Mid Tom",
        48 => "Hi-Mid Tom",
        49 => "Crash Cymbal 1",
        50 => "High Tom",
        51 => "Ride Cymbal 1",
        52 => "Chinese Cymbal",
        53 => "Ride Bell",
        54 => "Tambourine",
        55 => "Splash Cymbal",
        56 => "Cowbell",
        57 => "Crash Cymbal 2",
        58 => "Vibraslap",
        59 => "Ride Cymbal 2",
        60 => "Hi Bongo",
        61 => "Low Bongo",
        62 => "Mute Hi Conga",
        63 => "Open Hi Conga",
        64 => "Low Conga",
        65 => "High Timbale",
        66 => "Low Timbale",
        67 => "High Agogo",
        68 => "Low Agogo",
        69 => "Cabasa",
        70 => "Maracas",
        71 => "Short Whistle",
        72 => "Long Whistle",
        73 => "Short Guiro",
        74 => "Long Guiro",
        75 => "Claves",
        76 => "Hi Wood Block",
        77 => "Low Wood Block",
        78 => "Mute Cuica",
        79 => "Open Cuica",
        80 => "Mute Triangle",
        81 => "Open Triangle",
        _ => return None,
    })
}

/// A device: its title bar (what it is on the left, its buttons on the right) over what it
/// holds, as tall as every device.
pub(crate) fn device(
    title: impl IntoElement,
    buttons: impl IntoElement,
    body: impl IntoElement,
) -> Div {
    // At least a device's height, as tall as its row (the rows fill the pane); never wider than
    // the pane, so what's in it wraps rather than running off the side.
    div()
        .flex_none()
        .min_h(px(DEVICE_H))
        .max_w(gpui::relative(1.))
        .flex()
        .flex_col()
        .rounded(px(8.))
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(LINE))
        .overflow_hidden()
        .child(
            div()
                .h(px(TITLE_H))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .bg(rgb(HEADERS))
                .border_b_1()
                .border_color(rgb(LINE_SOFT))
                .child(title)
                .child(div().flex_1().min_w(px(8.)))
                .child(buttons),
        )
        .child(div().flex_1().min_h(px(0.)).flex().child(body))
}

/// A scroll bar down the right of what `handle` scrolls, when it's taller than it shows: how
/// much shows and where; click or drag along it to go there.
pub(crate) fn scroll_bar(
    handle: gpui::ScrollHandle,
    sel: &'static str,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let (seen, pressed, dragged) = (handle.clone(), handle.clone(), handle.clone());
    let jump = |h: &gpui::ScrollHandle, y: f32| {
        let b = h.bounds();
        let max = f32::from(h.max_offset().y);
        let at = ((y - f32::from(b.origin.y)) / f32::from(b.size.height).max(1.0)).clamp(0.0, 1.0);
        h.set_offset(point(px(0.), px(-at * max)));
    };
    div()
        .absolute()
        .top(px(8.))
        .bottom(px(8.))
        .right(px(2.))
        .w(px(8.))
        .debug_selector(move || sel.into())
        .child(
            canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let max = f32::from(seen.max_offset().y);
                    if max <= 0.5 {
                        return;
                    }
                    let h = f32::from(b.size.height);
                    let thumb = (h * h / (h + max)).max(24.0);
                    let at = (-f32::from(seen.offset().y) / max).clamp(0.0, 1.0);
                    window.paint_quad(
                        fill(Bounds::new(b.origin, b.size), alpha(0xffffff, 0.05))
                            .corner_radii(px(4.)),
                    );
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(b.origin.x, b.origin.y + px((h - thumb) * at)),
                                size(b.size.width, px(thumb)),
                            ),
                            alpha(0xffffff, 0.32),
                        )
                        .corner_radii(px(4.)),
                    );
                },
            )
            .size_full(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, e: &MouseDownEvent, _, cx| {
                if f32::from(pressed.max_offset().y) > 0.5 {
                    cx.stop_propagation();
                    jump(&pressed, f32::from(e.position.y));
                    cx.notify();
                }
            }),
        )
        .on_mouse_move(cx.listener(move |_, e: &gpui::MouseMoveEvent, _, cx| {
            if e.pressed_button == Some(MouseButton::Left)
                && f32::from(dragged.max_offset().y) > 0.5
            {
                jump(&dragged, f32::from(e.position.y));
                cx.notify();
            }
        }))
}

/// A device's name in its title bar, beside its colour.
pub(crate) fn device_name(name: impl Into<gpui::SharedString>, color: u32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .min_w(px(0.))
        .child(
            div()
                .flex_none()
                .size(px(8.))
                .rounded(px(2.))
                .bg(rgb(color)),
        )
        .child(
            div()
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(name.into()),
        )
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

    /// The kit's pads by note, lowest first.
    pub(crate) fn kit_notes(&self, kit: usize) -> Vec<u8> {
        let p = &self.session.project;
        let mut notes: Vec<u8> = p
            .pads_of(kit)
            .into_iter()
            .filter_map(|t| p.tracks[t].pad.map(|p| p.note))
            .collect();
        notes.sort_unstable();
        notes
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

    /// The pads shown moved by `rows` rows of four (a bank is four), kept in 0 to 127.
    pub(crate) fn shift_pad_view(&mut self, rows: i32, cx: &mut Context<Self>) {
        let view = (self.drums.view as i32 + rows * 4).clamp(0, 112);
        self.drums.view = view as u8;
        cx.notify();
    }

    /// The Drum Machine's arrows: with hot-swap open, the pad's next or last sound (`sound`);
    /// else the pads shown a bank up or down (`sound` too: ⌥, a row); the next or last pad
    /// (`pad`), picked and heard.
    pub(crate) fn drum_key(&mut self, sound: i32, pad: i32, cx: &mut Context<Self>) {
        let Some(kit) = self.drum_kit() else {
            return;
        };
        let note = self.drums.pad;
        if sound != 0 {
            if self.drums.tab == RackTab::Swap {
                return self.step_sound(kit, note, sound, cx);
            }
            return self.shift_pad_view(-sound * 4, cx);
        }
        let notes = self.kit_notes(kit);
        let to = match notes.iter().position(|&n| n == note) {
            Some(at) => notes.get((at as i32 + pad).rem_euclid(notes.len() as i32) as usize),
            None => notes.first(),
        };
        if let Some(&to) = to {
            self.pick_pad(to);
            self.hit_pad(kit, to, 100);
            cx.notify();
        }
    }

    /// D: hot-swap opened on the picked pad, or its target switched between the pad and the
    /// kit.
    pub(crate) fn hot_swap_target(&mut self, cx: &mut Context<Self>) {
        match self.drums.tab == RackTab::Swap {
            false => {
                self.drums.tab = RackTab::Swap;
                self.drums.swap_rack = false;
            }
            true => self.drums.swap_rack ^= true,
        }
        cx.notify();
    }

    /// A pad picked, and the pads shown moved to show it.
    pub(crate) fn pick_pad(&mut self, note: u8) {
        self.drums.pad = note;
        let view = self.drums.view;
        if note < view || note >= view.saturating_add(16) {
            self.drums.view = (note / 4 * 4).min(112);
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

    /// The pad under a window point, as the pads were last drawn, by note.
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
        self.auto_select(kit);
        // The rack on the left; its panel (the picked pad's Drum Sampler, its effects, the
        // chain list, the macros, hot-swap: one at a time, by its tabs) taking the rest.
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .gap(px(8.))
            .p(px(8.))
            .bg(rgb(BG))
            .child(self.drum_rack(kit, cx))
            .child(self.rack_panel(kit, cx))
    }

    /// Auto select: the pad that started playing since the last frame is picked.
    fn auto_select(&mut self, kit: usize) {
        if !self.drums.views.auto {
            self.drums.heard.clear();
            return;
        }
        let p = &self.session.project;
        let mut started = None;
        let mut heard = HashMap::new();
        for t in p.pads_of(kit) {
            let Some(pad) = p.tracks[t].pad else {
                continue;
            };
            let level = self
                .levels
                .get(&Some(p.tracks[t].id))
                .map_or(0.0, |l| l.now[0].max(l.now[1]));
            let was = self.drums.heard.get(&pad.note).copied().unwrap_or(0.0);
            if level > 0.05 && level > was * 1.6 && started.is_none_or(|(_, l)| level > l) {
                started = Some((pad.note, level));
            }
            heard.insert(pad.note, level);
        }
        self.drums.heard = heard;
        if let Some((note, _)) = started {
            self.pick_pad(note);
        }
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
                                "Pick a kit track to play and shape its pads, or make a kit: an \
                                 empty one to drop your sounds on, or a folder of your samples, \
                                 a pad a sound.",
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
}
