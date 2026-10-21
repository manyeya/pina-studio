//! The sample browser, as a page of its own in the tracks' place (as the mixer is) and as a
//! compact list beside the tracks to drag sounds from.
//!
//! The page: on the left, what to look for: words, one-shots or loops, what they are, their key
//! and tempo, their character. In the middle, what's found: a list, a map of sound (near dots
//! sound alike) or a kit of what goes together. On the right, the selected sample in detail, and
//! what sounds like it. The compact browser: a search, what kind, the song's fit and what sounds,
//! a list, and the picked sound with what's like it.
//!
//! A click hears a sample (in the song's key, with Play in Key on); up and down hear the next.
//! Add to Song, or a drag onto the tracks, puts it in the song: a loop as audio, in time; a
//! one-shot on a sampler's note, in tune. A drag out of the page brings the tracks back to drop
//! it on. The library loads and scans in the background.

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use gpui::{
    App, Bounds, Context, Div, Entity, Focusable, FontWeight, Hsla, MouseButton, MouseDownEvent,
    MouseMoveEvent, PathBuilder, PathPromptOptions, Pixels, Point, ScrollDelta, ScrollStrategy,
    ScrollWheelEvent, SharedString, Subscription, UniformListScrollHandle, Window, canvas, div,
    fill, point, prelude::*, px, relative, rgb, size, uniform_list,
};
use pina_engine::{
    commands::{self, Command},
    music::Key,
    project::{At, Content, Instrument, PPQ, Sampler},
    samples::{
        Analysis, Category, Kind, NOTES, Pitch, Sample, TRAITS,
        library::Library,
        note_name,
        place::{loop_ticks, place_zone, transpose_to},
        search::{self, Aspects, Hit, KitSeed, KitSlot, Query},
    },
    stretch,
};

use crate::{
    sounds::catalog::Scope,
    ui::menu::{Menu, MenuAction},
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::{Bottom, Drag, Slider, View, Workspace, pt},
};

/// Takes a filter off, when nothing was found.
type Loosen = Box<dyn Fn(&mut Workspace)>;

/// The compact browser's usual and least width, and its rows' height.
pub const PANEL_W: f32 = 320.0;
const PANEL_MIN_W: f32 = 260.0;
/// The lanes keep at least this much beside the compact browser.
const LANES_MIN_W: f32 = 240.0;
const PANEL_ROW_H: f32 = 44.0;
const DETAILS_W: f32 = 312.0;
const ROW_H: f32 = 44.0;
const WAVE_W: f32 = 104.0;
// The list's columns, lined up under their heads.
const KEY_W: f32 = 84.0;
const SHIFT_W: f32 = 26.0;
const BPM_W: f32 = 34.0;
const LENGTH_W: f32 = 50.0;
const FIT_W: f32 = 24.0;
const ORANGE: u32 = 0xfb923c;
const FUCHSIA: u32 = 0xe879f9;
const GREY: u32 = 0x6e6e7a;
/// A preview's loudness, so quiet and loud samples sound alike (when loudness is measured).
const PREVIEW_LUFS: f32 = -14.0;
/// The aspects of a sound the Similar list weighs, as Sononym's dial does.
pub const ASPECTS: [&str; 4] = ["Timbre", "Pitch", "Envelope", "Spectrum"];

/// What the middle of the pane shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Results {
    List,
    Map,
    Kit,
}

/// The key the results keep to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum KeyFilter {
    Any,
    /// The song's, whatever it is.
    Song,
    Key(Key),
}

/// A kit's slot as the browser holds it: which of its sounds is chosen (0 its pick, then its
/// alternatives), and whether it's locked against new picks.
#[derive(Clone, Debug)]
pub struct KitPick {
    pub slot: KitSlot,
    pub choice: usize,
    pub locked: bool,
}

impl KitPick {
    pub fn options(&self) -> Vec<&Hit> {
        self.slot
            .pick
            .iter()
            .chain(&self.slot.alternatives)
            .collect()
    }

    pub fn hit(&self) -> Option<&Hit> {
        self.options().get(self.choice).copied()
    }
}

/// Where a sample dragged over the tracks would go.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drop {
    /// An audio region on an audio track, at a tick.
    Audio(usize, u64),
    /// A zone on a sampler track (a region at the tick, for a file from the Finder).
    Zone(usize, u64),
    /// A new audio track under the others, a region on it at the tick.
    New(u64),
    /// A track with an instrument of its own: a new audio track under it, a region at the tick.
    Under(usize, u64),
}

pub struct Browser {
    pub library: Library,
    /// Read from disk yet: the index can take a moment.
    pub loaded: bool,
    /// A scan in progress: samples analysed, of how many.
    pub scan: Option<Arc<[AtomicUsize; 2]>>,
    /// Folders were added while scanning: scan again after.
    rescan: bool,
    pub search: Entity<TextInput>,
    pub text: String,
    /// The search field has the keyboard (so letters type, not shortcuts).
    pub searching: bool,
    /// Give the search field the keyboard on the next frame.
    pub focus_search: bool,
    pub kind: Option<Kind>,
    pub categories: Vec<Category>,
    pub traits: Vec<String>,
    pub key: KeyFilter,
    /// Keys that go with the chosen one count too: its relative, and its neighbours by fifths.
    pub compatible: bool,
    /// Loops near the song's tempo only.
    pub song_tempo: bool,
    pub hits: Vec<Hit>,
    /// How many each category would find, the other filters as they are; how many of the hits
    /// have each trait.
    pub counts: HashMap<Category, usize>,
    pub trait_counts: HashMap<String, usize>,
    /// The selected and hovered samples, by index in the library.
    pub selected: Option<usize>,
    pub hovered: Option<usize>,
    /// Previews move into the song's key.
    pub play_in_key: bool,
    pub view: Results,
    pub similar: Vec<Hit>,
    pub aspects: Aspects,
    /// Every sample's place on the map, -1 to 1 each way; the map's zoom and middle.
    pub map: Vec<[f32; 2]>,
    pub zoom: f32,
    pub center: [f32; 2],
    pub map_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The samples in a box drawn on the map: the results keep to them.
    pub area: Option<HashSet<usize>>,
    pub kit: Vec<KitPick>,
    /// What's being heard: the sample, since when, and for how long.
    pub audition: Option<(usize, Instant, f32)>,
    /// Samples read for previews, by path, and as heard (at the song's tempo, in its key), by
    /// path, stretch and semitones, so going back to one is instant; and the one asked for last,
    /// so a slow read that's overtaken doesn't play.
    heard: HashMap<(PathBuf, u64, u32), Arc<pina_engine::engine::AudioData>>,
    /// The preview is a loop playing in time with the song, over and over while it plays.
    pub looping: bool,
    wanted: Option<usize>,
    pub list: UniformListScrollHandle,
    pub panel_list: UniformListScrollHandle,
    /// The compact list shows what sounds like this sample, not the results.
    pub like: Option<(usize, Vec<Hit>)>,
    /// The song's key and tempo when the results were last found.
    seen: (Option<Key>, f64),
    _subscriptions: [Subscription; 2],
}

impl Browser {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let search = cx.new(|cx| TextInput::search("Search: dark punchy kick", cx));
        // Typing searches as it goes.
        let typed = cx.observe(&search, |ws, input, cx| {
            let text = input.read(cx).text().trim().to_string();
            if text != ws.browser.text {
                ws.browser.text = text;
                ws.browser.like = None;
                ws.find_samples();
                cx.notify();
            }
        });
        // Enter hands the keyboard back to the list; Escape clears the search too.
        let done = cx.subscribe(&search, |ws, input, e: &TextEvent, cx| {
            if let TextEvent::Cancel = e {
                input.update(cx, |i, cx| i.set_text("", cx));
            }
            ws.refocus = true;
            cx.notify();
        });
        Browser {
            library: Library::default(),
            loaded: false,
            scan: None,
            rescan: false,
            search,
            text: String::new(),
            searching: false,
            focus_search: false,
            kind: None,
            categories: Vec::new(),
            traits: Vec::new(),
            key: KeyFilter::Any,
            compatible: false,
            song_tempo: false,
            hits: Vec::new(),
            counts: HashMap::new(),
            trait_counts: HashMap::new(),
            selected: None,
            hovered: None,
            play_in_key: false,
            view: Results::List,
            similar: Vec::new(),
            aspects: Aspects::default(),
            map: Vec::new(),
            zoom: 1.0,
            center: [0.0, 0.0],
            map_bounds: Rc::new(Cell::new(Bounds::default())),
            area: None,
            kit: Vec::new(),
            audition: None,
            heard: HashMap::new(),
            looping: false,
            wanted: None,
            list: UniformListScrollHandle::new(),
            panel_list: UniformListScrollHandle::new(),
            like: None,
            seen: (None, 0.0),
            _subscriptions: [typed, done],
        }
    }

    /// A preview is playing: the playhead over its waveform moves.
    pub fn auditioning(&self) -> bool {
        self.audition.is_some()
    }

    pub fn sample(&self, index: usize) -> Option<&Sample> {
        self.library.samples.get(index)
    }

    fn hit(&self, index: usize) -> Option<&Hit> {
        self.hits.iter().find(|h| h.index == index)
    }

    /// The map's places, once they're worked out for the library as it is.
    fn places(&self) -> Option<&[[f32; 2]]> {
        (self.map.len() == self.library.samples.len()).then_some(&self.map[..])
    }
}

/// One of the Similar list's aspects, by its place in `ASPECTS`.
pub fn aspect(a: &mut Aspects, i: usize) -> &mut f32 {
    match i {
        0 => &mut a.timbre,
        1 => &mut a.pitch,
        2 => &mut a.envelope,
        _ => &mut a.spectrum,
    }
}

/// A category's color: drums by what they are (kicks, snares, metals, percussion), the rest as
/// tracks are colored (bass amber, keys mint, leads sky, pads violet).
pub fn category_color(c: Category) -> u32 {
    use Category::*;
    match c {
        Kick | Drums => CORAL,
        Snare | Clap | Rim | Snap => PINK,
        HatClosed | HatOpen | Ride | Crash | Shaker | Tambourine => TEAL,
        Tom | Perc => ORANGE,
        Bass808 | Bass => AMBER,
        Keys | Chord | Pluck | Bell | Guitar => MINT,
        Lead | Synth | Brass | Woodwind => SKY,
        Pad | Strings => VIOLET,
        Vocal => FUCHSIA,
        Fx | Reverse | Texture | Foley => SLATE,
        Other => GREY,
    }
}

/// The map's legend: each color, and what it stands for.
const LEGEND: [(u32, &str); 10] = [
    (CORAL, "Kicks"),
    (PINK, "Snares"),
    (TEAL, "Hats"),
    (ORANGE, "Perc"),
    (AMBER, "Bass"),
    (MINT, "Keys"),
    (SKY, "Leads"),
    (VIOLET, "Pads"),
    (FUCHSIA, "Vocals"),
    (SLATE, "FX"),
];

fn seconds_text(s: f32) -> String {
    match s {
        s if s < 1.0 => format!("{s:.2} s"),
        s if s < 10.0 => format!("{s:.1} s"),
        s => format!("{s:.0} s"),
    }
}

fn note_text(p: &Pitch) -> String {
    match p.cents.round() as i32 {
        0 => note_name(p.note),
        c if c < 0 => format!("{} −{}¢", note_name(p.note), -c),
        c => format!("{} +{c}¢", note_name(p.note)),
    }
}

/// Its key, or its note.
fn tone_text(a: &Analysis) -> Option<String> {
    a.key
        .as_ref()
        .map(|k| k.name.clone())
        .or_else(|| a.pitch.as_ref().map(note_text))
}

/// "+3", "−5", "−0.2": semitones, with a true minus.
fn semis_text(s: f32) -> String {
    let s = (s * 10.0).round() / 10.0;
    let body = match s.fract() == 0.0 {
        true => format!("{}", s.abs() as i32),
        false => format!("{:.1}", s.abs()),
    };
    match s {
        s if s < 0.0 => format!("−{body}"),
        _ => format!("+{body}"),
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::OneShot => "One-shot",
        Kind::Loop => "Loop",
    }
}

/// A sample's outline, mirrored about its middle, filling `b`.
fn paint_wave(points: &[u8], b: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let n = points.len();
    if n < 2 {
        return;
    }
    let (x0, w) = (f32::from(b.origin.x), f32::from(b.size.width));
    let h = f32::from(b.size.height);
    let mid = f32::from(b.origin.y) + h / 2.0;
    let at = |i: usize, side: f32| {
        let v = (points[i] as f32 / 255.0 * h / 2.0).max(0.5);
        point(px(x0 + i as f32 / (n - 1) as f32 * w), px(mid - side * v))
    };
    let mut path = PathBuilder::fill();
    path.move_to(at(0, 1.0));
    for i in 1..n {
        path.line_to(at(i, 1.0));
    }
    for i in (0..n).rev() {
        path.line_to(at(i, -1.0));
    }
    path.close();
    if let Ok(p) = path.build() {
        window.paint_path(p, color);
    }
}

/// A sample's outline as bars, the part already heard brighter.
fn paint_bars(
    points: &[u8],
    b: Bounds<Pixels>,
    color: u32,
    heard: Option<f32>,
    window: &mut Window,
) {
    let n = points.len().max(1);
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let pitch = w / n as f32;
    let bar = (pitch - 1.0).max(1.0);
    for (i, &v) in points.iter().enumerate() {
        let x = i as f32 * pitch;
        let bh = (v as f32 / 255.0 * h).max(1.5);
        let lit = heard.is_some_and(|t| x / w <= t);
        let c = match (heard.is_some(), lit) {
            (false, _) => alpha(color, 0.85),
            (true, true) => alpha(tint(color, 0.25), 1.0),
            (true, false) => alpha(color, 0.45),
        };
        window.paint_quad(
            fill(
                Bounds::new(
                    point(b.origin.x + px(x), b.origin.y + px((h - bh) / 2.0)),
                    size(px(bar), px(bh)),
                ),
                c,
            )
            .corner_radii(px(1.)),
        );
    }
}

/// A row's waveform, in its category's color.
pub fn wave(points: Vec<u8>, color: u32, strength: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| paint_wave(&points, b, alpha(color, strength), window),
    )
    .size_full()
}

/// A filter chip: a color dot, its name, and how many it would find.
fn filter_chip(
    label: impl Into<SharedString>,
    count: Option<usize>,
    on: bool,
    dot: Option<u32>,
) -> Div {
    div()
        .h(px(24.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(6.))
        .border_1()
        .border_color(if on {
            alpha(ACCENT, 0.7)
        } else {
            rgb(LINE).into()
        })
        .bg(if on {
            alpha(ACCENT, 0.16)
        } else {
            rgb(CARD).into()
        })
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
        .whitespace_nowrap()
        .cursor_pointer()
        .when(!on, |d| {
            d.hover(|s| s.bg(rgb(OFF)).border_color(rgb(LINE_STRONG)))
        })
        .when_some(dot, |d, c| {
            d.child(div().size(px(6.)).flex_none().rounded(px(3.)).bg(rgb(c)))
        })
        .child(label.into())
        .when_some(count, |d, n| {
            d.child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(if on { ACCENT_TEXT } else { TEXT_3 }))
                    .child(n.to_string()),
            )
        })
}

/// A label over a part of a column, with something on its right.
fn heading(label: &'static str) -> Div {
    div()
        .h(px(22.))
        .flex()
        .items_center()
        .justify_between()
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(label.to_uppercase())
}

/// A small text link, like "Clear".
fn link(label: &'static str) -> Div {
    div()
        .text_size(px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT_3))
        .cursor_pointer()
        .hover(|s| s.text_color(rgb(TEXT)))
        .child(label)
}

/// The main action's button: filled with the accent.
fn primary(label: impl Into<SharedString>) -> Div {
    div()
        .h(px(30.))
        .px(px(14.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(8.))
        .bg(rgb(ACCENT))
        .text_color(rgb(0xffffff))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(tint(ACCENT, 0.12))))
        .child(label.into())
}

/// A fact about a sample: what it is, and its value.
fn fact(label: &'static str, value: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .min_w(px(0.))
        .child(
            div()
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .child(label),
        )
        .child(
            div()
                .font_family(MONO)
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(value.into()),
        )
}

impl Workspace {
    // The library.

    /// Reads the library from disk, then brings it up to date, in the background.
    pub fn load_library(&mut self, cx: &mut Context<Self>) {
        let read = cx.background_spawn(async { Library::load() });
        cx.spawn(async move |this, cx| {
            let library = read.await;
            this.update(cx, |ws, cx| {
                ws.set_library(library, cx);
                ws.rescan(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Takes a library in: what was selected stays selected, and the map is worked out again.
    pub fn set_library(&mut self, library: Library, cx: &mut Context<Self>) {
        let b = &mut self.browser;
        let path =
            |i: Option<usize>| i.and_then(|i| b.library.samples.get(i).map(|s| s.path.clone()));
        let (selected, auditioned) = (path(b.selected), path(b.audition.map(|a| a.0)));
        b.library = library;
        b.loaded = true;
        let find =
            |p: Option<PathBuf>| p.and_then(|p| b.library.samples.iter().position(|s| s.path == p));
        b.selected = find(selected);
        if let Some(a) = &mut b.audition {
            match find(auditioned) {
                Some(i) => a.0 = i,
                None => b.audition = None,
            }
        }
        b.hovered = None;
        b.area = None;
        b.like = None;
        // The map can take a while for a big library.
        let samples = b.library.samples.clone();
        let map = cx.background_spawn(async move { search::map(&samples) });
        cx.spawn(async move |this, cx| {
            let map = map.await;
            this.update(cx, |ws, cx| {
                ws.browser.map = map;
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.find_samples();
        self.find_similar();
        cx.notify();
    }

    /// Analyses what's new or changed in the library's folders, in the background, with
    /// progress in the pane.
    pub fn rescan(&mut self, cx: &mut Context<Self>) {
        if self.browser.scan.is_some() {
            self.browser.rescan = true;
            return;
        }
        let mut library = self.browser.library.clone();
        let progress = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
        self.browser.scan = Some(progress.clone());
        cx.notify();
        let heard = progress.clone();
        let job = cx.background_spawn(async move {
            let report = library.scan(&|done, of| {
                heard[0].store(done, Ordering::Relaxed);
                heard[1].store(of, Ordering::Relaxed);
            });
            (library, report)
        });
        cx.spawn(async move |this, cx| {
            let (library, report) = job.await;
            this.update(cx, |ws, cx| {
                ws.browser.scan = None;
                // Folders added meanwhile stay, and are scanned next.
                let folders = ws.browser.library.folders.clone();
                ws.set_library(Library { folders, ..library }, cx);
                if !report.failed.is_empty() {
                    let (path, e) = &report.failed[0];
                    ws.status = Some(format!(
                        "Couldn't read {} sample{} ({}: {e})",
                        report.failed.len(),
                        if report.failed.len() == 1 { "" } else { "s" },
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
                if std::mem::take(&mut ws.browser.rescan) {
                    ws.rescan(cx);
                }
            })
            .ok();
        })
        .detach();
        // Progress shows as it goes.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let scanning = this.update(cx, |ws, cx| {
                    cx.notify();
                    ws.browser.scan.is_some()
                });
                if !matches!(scanning, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Asks for folders to add to the library, and scans them.
    pub fn add_library_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Add to Library".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dirs))) = picked.await else {
                return;
            };
            this.update(cx, |ws, cx| {
                for dir in dirs {
                    if let Err(e) = ws.browser.library.add_folder(&dir) {
                        ws.status = Some(e);
                    }
                }
                ws.rescan(cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn remove_library_folder(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(dir) = self.browser.library.folders.get(index).cloned() else {
            return;
        };
        let mut library = self.browser.library.clone();
        library.remove_folder(&dir);
        let saved = library.clone();
        cx.background_spawn(async move {
            if let Err(e) = saved.save() {
                eprintln!("library: {e}");
            }
        })
        .detach();
        self.set_library(library, cx);
    }

    // Finding.

    /// The key the results keep to, if any.
    pub fn sample_key(&self) -> Option<Key> {
        match self.browser.key {
            KeyFilter::Any => None,
            KeyFilter::Song => self.session.project.song_key(),
            KeyFilter::Key(k) => Some(k),
        }
    }

    fn sample_bpm(&self) -> Option<f32> {
        self.browser
            .song_tempo
            .then_some(self.session.project.tempo as f32)
    }

    /// Whether the results follow the song's key and tempo.
    pub fn fits_song(&self) -> bool {
        self.browser.key == KeyFilter::Song && self.browser.song_tempo
    }

    pub fn sample_query(&self) -> Query {
        let b = &self.browser;
        Query {
            text: b.text.clone(),
            kind: b.kind,
            categories: b.categories.clone(),
            traits: b.traits.clone(),
            key: self.sample_key().map(|k| k.name()),
            bpm: self.sample_bpm(),
            limit: usize::MAX,
            ..Default::default()
        }
    }

    /// Finds `q`, in compatible keys too if they're asked for.
    fn find_keyed(&self, q: &Query) -> Vec<Hit> {
        let q = Query {
            compatible: self.browser.compatible,
            ..q.clone()
        };
        search::find(&self.browser.library.samples, &q)
    }

    /// Finds what the filters ask for, and how many each category and trait would give.
    pub fn find_samples(&mut self) {
        let q = self.sample_query();
        let pool = self.find_keyed(&Query {
            categories: Vec::new(),
            ..q.clone()
        });
        let samples = &self.browser.library.samples;
        let mut counts = HashMap::new();
        for h in &pool {
            *counts
                .entry(samples[h.index].analysis.category)
                .or_insert(0) += 1;
        }
        let mut hits = match q.categories.is_empty() {
            true => pool,
            false => self.find_keyed(&q),
        };
        if let Some(area) = &self.browser.area {
            hits.retain(|h| area.contains(&h.index));
        }
        let mut trait_counts = HashMap::new();
        for h in &hits {
            for t in &samples[h.index].analysis.traits {
                *trait_counts.entry(t.clone()).or_insert(0) += 1;
            }
        }
        let b = &mut self.browser;
        b.hits = hits;
        b.counts = counts;
        b.trait_counts = trait_counts;
        b.seen = (self.session.project.song_key(), self.session.project.tempo);
        if b.view == Results::Kit {
            self.build_kit();
        }
    }

    /// What sounds like the selected sample, weighing each aspect as the sliders say.
    pub fn find_similar(&mut self) {
        let b = &self.browser;
        let Some(s) = b.selected.and_then(|i| b.sample(i)) else {
            self.browser.similar.clear();
            return;
        };
        let me = b.selected;
        self.browser.similar = search::similar(&b.library.samples, &s.path, b.aspects, 13)
            .into_iter()
            .filter(|h| Some(h.index) != me)
            .take(12)
            .collect();
    }

    /// A filter changed: find again, and keep the list at the top.
    pub fn refilter(&mut self, cx: &mut Context<Self>) {
        self.browser.like = None;
        self.find_samples();
        self.browser.list.scroll_to_item(0, ScrollStrategy::Top);
        self.browser
            .panel_list
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    pub fn set_sample_key(&mut self, key: KeyFilter, cx: &mut Context<Self>) {
        self.browser.key = key;
        self.refilter(cx);
    }

    /// Follows the song's key and tempo, or lets them go.
    pub fn set_fits_song(&mut self, on: bool, cx: &mut Context<Self>) {
        self.browser.key = if on { KeyFilter::Song } else { KeyFilter::Any };
        self.browser.song_tempo = on;
        self.refilter(cx);
    }

    pub fn toggle_category(&mut self, c: Category, cx: &mut Context<Self>) {
        let cats = &mut self.browser.categories;
        match cats.iter().position(|&x| x == c) {
            Some(i) => drop(cats.remove(i)),
            None => cats.push(c),
        }
        self.refilter(cx);
    }

    pub fn toggle_trait(&mut self, t: &str, cx: &mut Context<Self>) {
        let traits = &mut self.browser.traits;
        match traits.iter().position(|x| x == t) {
            Some(i) => drop(traits.remove(i)),
            None => traits.push(t.to_string()),
        }
        self.refilter(cx);
    }

    /// The key filter's menu: any key, the song's, or one picked; and whether related keys count.
    pub fn sample_key_menu(&self, at: Point<Pixels>) -> Menu {
        let b = &self.browser;
        let tick = |on: bool, label: String| match on {
            true => format!("✓ {label}"),
            false => label,
        };
        let song = match self.session.project.song_key() {
            Some(k) => format!("Song key ({})", k.name()),
            None => "Song key (none yet)".into(),
        };
        let mut items = vec![
            (
                tick(b.key == KeyFilter::Any, "Any key".into()),
                MenuAction::SampleKey(KeyFilter::Any),
            ),
            (
                tick(b.key == KeyFilter::Song, song),
                MenuAction::SampleKey(KeyFilter::Song),
            ),
        ];
        items.extend(Key::all().map(|k| {
            (
                tick(b.key == KeyFilter::Key(k), k.name()),
                MenuAction::SampleKey(KeyFilter::Key(k)),
            )
        }));
        let tail = (
            match b.compatible {
                true => "✓ Compatible keys too".to_string(),
                false => "    Compatible keys too".to_string(),
            },
            MenuAction::SampleCompatible(at),
        );
        Menu {
            at,
            items,
            columns: 2,
            lead: 2,
            tail: Some(tail),
            ..Default::default()
        }
    }

    // Hearing and choosing.

    /// Semitones to move a preview by: into the key the results keep to (else the song's) with
    /// Play in Key on, as the search worked it out or else from the sample's note or key.
    pub fn sample_transpose(&self, index: usize) -> f32 {
        let b = &self.browser;
        if !b.play_in_key {
            return 0.0;
        }
        let Some(s) = b.sample(index) else {
            return 0.0;
        };
        if let Some(t) = b.hit(index).and_then(|h| h.transpose) {
            let cents = s.analysis.pitch.map_or(0.0, |p| p.cents);
            return t as f32 - cents / 100.0;
        }
        match self
            .sample_key()
            .or_else(|| self.session.project.song_key())
        {
            Some(key) => transpose_to(&s.analysis, key),
            None => 0.0,
        }
    }

    /// How much longer a loop plays at the song's tempo (1 for a one-shot, or a loop without one).
    fn preview_stretch(&self, a: &Analysis) -> f64 {
        let tempo = self.session.project.tempo;
        match a.kind {
            Kind::Loop => loop_ticks(a, tempo)
                .map(|t| t as f64 / PPQ as f64 * 60.0 / tempo / a.seconds.max(0.01) as f64)
                .filter(|r| (r - 1.0).abs() > 0.002)
                .unwrap_or(1.0),
            Kind::OneShot => 1.0,
        }
    }

    /// Plays a sample over the mix as it would be in the song: a loop at its tempo, in its key
    /// with Play in Key on (both by the same stretch the song uses); and while the song plays, a
    /// loop in time with it.
    pub fn audition_sample(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(s) = self.browser.sample(index) else {
            return;
        };
        let (path, a) = (s.path.clone(), &s.analysis);
        let semis = self.sample_transpose(index);
        let gain = match a.loudness {
            l if (-60.0..0.0).contains(&l) => (PREVIEW_LUFS - l).clamp(-12.0, 6.0),
            _ => 0.0,
        };
        let ratio = self.preview_stretch(a);
        let vocal = a.category == Category::Vocal;
        let synced = a.kind == Kind::Loop && a.bpm.is_some();
        let seconds = a.seconds * ratio as f32;
        self.browser.wanted = Some(index);
        let key = (path.clone(), ratio.to_bits(), semis.to_bits());
        if let Some(data) = self.browser.heard.get(&key).cloned() {
            self.play_heard(index, data, synced, gain, seconds, cx);
            return;
        }
        // Read and stretched off the UI thread; played when it's ready, if it's still wanted.
        let rate = self.player.rate();
        let raw = self
            .browser
            .heard
            .get(&(path.clone(), 1f64.to_bits(), 0f32.to_bits()))
            .cloned();
        let made = cx.background_spawn({
            let path = path.clone();
            async move {
                let raw = match raw {
                    Some(raw) => raw,
                    None => Arc::new(pina_engine::engine::load_audio(&path, rate)?),
                };
                let heard = match ratio == 1.0 && semis == 0.0 {
                    true => raw.clone(),
                    // A voice keeps its formants, moved into the key.
                    false => Arc::new(stretch::render(
                        &raw,
                        &stretch::Fit {
                            semitones: semis as f64,
                            formants: vocal,
                            ..stretch::Fit::ratio(ratio)
                        },
                        rate,
                    )),
                };
                Ok::<_, String>((raw, heard))
            }
        });
        cx.spawn(async move |ws, cx| {
            let made = made.await;
            ws.update(cx, |ws, cx| match made {
                Ok((raw, data)) => {
                    // ponytail: forgets everything past 64 files; an LRU if browsing gets big.
                    if ws.browser.heard.len() >= 64 {
                        ws.browser.heard.clear();
                    }
                    ws.browser
                        .heard
                        .insert((path, 1f64.to_bits(), 0f32.to_bits()), raw);
                    ws.browser.heard.insert(key, data.clone());
                    if ws.browser.wanted == Some(index) {
                        ws.play_heard(index, data, synced, gain, seconds, cx);
                    }
                }
                Err(e) => {
                    ws.status = Some(e);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn play_heard(
        &mut self,
        index: usize,
        data: Arc<pina_engine::engine::AudioData>,
        synced: bool,
        gain: f32,
        seconds: f32,
        cx: &mut Context<Self>,
    ) {
        // In time with the song, it starts where the song is in it.
        let looping = synced && self.player.playing();
        let since = match looping {
            true => {
                let len = data.channels()[0].len().max(1) as u64;
                let into = (self.player.position() % len) as f64 / self.player.rate();
                Instant::now() - Duration::from_secs_f64(into)
            }
            false => Instant::now(),
        };
        let played = match synced {
            true => self.player.audition_synced(data, gain),
            false => self.player.audition_audio(data, 0.0, gain),
        };
        match played {
            Ok(()) => {
                self.browser.audition = Some((index, since, seconds));
                self.browser.looping = looping;
            }
            Err(e) => self.status = Some(e),
        }
        cx.notify();
    }

    pub fn stop_audition(&mut self, cx: &mut Context<Self>) {
        self.player.stop_audition();
        self.browser.audition = None;
        cx.notify();
    }

    /// Selects a sample (and hears it): its details and what sounds like it show.
    pub fn select_sample(&mut self, index: usize, hear: bool, cx: &mut Context<Self>) {
        self.browser.selected = Some(index);
        self.find_similar();
        if hear {
            self.audition_sample(index, cx);
        }
        cx.notify();
    }

    /// Up and down: the next sample in the results, heard as it's picked.
    pub fn step_sample(&mut self, by: i32, cx: &mut Context<Self>) {
        let b = &self.browser;
        let panel = self.view == View::Arrange;
        let order: Vec<usize> = match (&b.like, b.view) {
            (Some((_, like)), _) if panel => like.iter().map(|h| h.index).collect(),
            (_, Results::Kit) if !panel => b
                .kit
                .iter()
                .filter_map(|k| k.hit().map(|h| h.index))
                .collect(),
            _ => b.hits.iter().map(|h| h.index).collect(),
        };
        if order.is_empty() {
            return;
        }
        let at = b.selected.and_then(|i| order.iter().position(|&o| o == i));
        let next = match at {
            Some(at) => (at as i32 + by).clamp(0, order.len() as i32 - 1) as usize,
            None if by > 0 => 0,
            None => order.len() - 1,
        };
        if panel {
            b.panel_list.scroll_to_item(next, ScrollStrategy::Nearest);
        } else if b.view != Results::Kit {
            b.list.scroll_to_item(next, ScrollStrategy::Nearest);
        }
        self.select_sample(order[next], true, cx);
    }

    /// The browser shows: as its page, or beside the tracks.
    /// The sample library showing: its page, or the Browser on Sounds.
    pub fn browser_open(&self) -> bool {
        self.view == View::Samples
            || (self.view == View::Arrange
                && self.show_browser
                && self.catalog.scope == Scope::Sounds)
    }

    /// F: the compact browser beside the tracks, or not; from the page or the mixer, the
    /// tracks with it beside them. It opens with the search field ready to type in.
    pub fn toggle_samples(&mut self, cx: &mut Context<Self>) {
        self.show_browser = self.view != View::Arrange || !self.show_browser;
        self.show_agent &= !self.show_browser;
        self.picking &= self.show_browser;
        self.view = View::Arrange;
        self.browser.focus_search = self.show_browser;
        cx.notify();
    }

    /// ⇧F: the browser's page, in the tracks' place, or the tracks again.
    pub fn toggle_samples_page(&mut self, cx: &mut Context<Self>) {
        self.view = match self.view {
            View::Samples => View::Arrange,
            _ => View::Samples,
        };
        self.browser.focus_search = self.view == View::Samples;
        cx.notify();
    }

    /// The compact list shows what sounds like a sample, or (again) the results.
    pub fn like_sample(&mut self, index: usize, cx: &mut Context<Self>) {
        let b = &self.browser;
        let like = match (&b.like, b.sample(index)) {
            (Some((i, _)), _) if *i == index => None,
            (_, None) => None,
            (_, Some(s)) => Some((
                index,
                search::similar(&b.library.samples, &s.path, b.aspects, 31)
                    .into_iter()
                    .filter(|h| h.index != index)
                    .take(30)
                    .collect(),
            )),
        };
        self.browser.like = like;
        self.browser
            .panel_list
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// The right panel's left edge (Samples' or Agent's): a strip to grab, lit while it's held or
    /// under the pointer. Double-click puts the width back.
    pub fn panel_edge(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let width = self.panel_width();
        let resizing = matches!(self.drag, Some(Drag::Panel { .. }));
        div()
            .id("panel-edge")
            .absolute()
            .left(px(-4.))
            .top_0()
            .h_full()
            .w(px(8.))
            .flex()
            .justify_center()
            .cursor_col_resize()
            .child(
                div()
                    .h_full()
                    .w(px(2.))
                    .when(resizing, |d| d.bg(rgb(ACCENT))),
            )
            .hover(|s| s.bg(alpha(ACCENT, 0.35)))
            .debug_selector(|| "panel-edge".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if e.click_count == 2 {
                        ws.panel_w = PANEL_W;
                        cx.notify();
                        return;
                    }
                    let x = pt(e.position).x;
                    ws.begin_drag(Drag::Panel { x, width }, cx);
                }),
            )
    }

    /// The compact browser's width: as dragged, but never so wide the lanes lose their room,
    /// nor so narrow its controls don't fit.
    pub fn panel_width(&self) -> f32 {
        let inspector = if self.show_inspector {
            self.side_w()
        } else {
            0.0
        };
        let room = self.viewport_w - inspector - self.header_w - LANES_MIN_W;
        self.panel_w.min(room).max(PANEL_MIN_W)
    }

    /// What the compact list shows: what sounds like a sample, or the results.
    fn panel_hits(&self) -> &[Hit] {
        match &self.browser.like {
            Some((_, like)) => like,
            None => &self.browser.hits,
        }
    }

    /// Each frame: who has the keyboard, whether the preview has ended, and whether the song's
    /// key or tempo moved under results that follow them.
    pub fn browser_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.browser_open();
        let focus = self.browser.search.focus_handle(cx);
        if std::mem::take(&mut self.browser.focus_search) {
            if open {
                window.focus(&focus, cx);
            } else if self.view == View::Arrange && self.show_browser {
                let devices = self.catalog.search.focus_handle(cx);
                window.focus(&devices, cx);
            }
        }
        let focused = focus.is_focused(window);
        // The field went with the pane: the keyboard comes back.
        if focused && !open {
            self.refocus = true;
        }
        self.browser.searching = focused && open;
        if let Some((_, since, seconds)) = &mut self.browser.audition
            && since.elapsed().as_secs_f32() > *seconds
        {
            // A loop in time with the song goes round again while it plays.
            match self.browser.looping && self.player.playing() {
                true => *since += Duration::from_secs_f32(*seconds),
                false => self.browser.audition = None,
            }
        }
        if open && self.browser.loaded {
            let follows = self.browser.key == KeyFilter::Song || self.browser.song_tempo;
            let now = (self.session.project.song_key(), self.session.project.tempo);
            if follows && now != self.browser.seen {
                self.find_samples();
            }
        }
    }

    // Into the song.

    /// Add to Song: a loop as audio on the selected audio track (or a new one) at the playhead;
    /// a one-shot as a zone on the selected sampler track (or a new one).
    pub fn add_sample(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(s) = self.browser.sample(index) else {
            return;
        };
        let track = self.session.project.tracks.get(self.track);
        match s.analysis.kind {
            Kind::Loop => {
                let audio = track.filter(|t| t.instrument.is_none()).map(|_| self.track);
                let tick = self.snap(self.playhead() as f64);
                self.add_audio(index, audio, tick, false, cx);
            }
            Kind::OneShot => {
                let sampler = track
                    .filter(|t| matches!(t.instrument, Some(Instrument::Sampler(_))))
                    .map(|_| self.track);
                self.add_zone(index, sampler, false, cx);
            }
        }
    }

    /// Where a new track goes: after the selected one, or (dropped under them) after the last.
    fn new_track_index(&self, at_end: bool) -> usize {
        let n = self.session.project.tracks.len();
        match at_end || n == 0 {
            true => n,
            false => (self.track + 1).min(n),
        }
    }

    /// A sample as an audio region at `tick` on `track` (a new audio track without one), stretched
    /// to the song's tempo if it's a loop that knows its own, and moved into the song's key if
    /// that's how it was heard.
    pub fn add_audio(
        &mut self,
        index: usize,
        track: Option<usize>,
        tick: u64,
        at_end: bool,
        cx: &mut Context<Self>,
    ) {
        let new = self.new_track_index(at_end);
        self.add_audio_at(index, track, Some(new), tick, cx);
    }

    /// As `add_audio`, a new track (without `track`) going at `new`.
    fn add_audio_at(
        &mut self,
        index: usize,
        track: Option<usize>,
        new: Option<usize>,
        tick: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.browser.sample(index).cloned() else {
            return;
        };
        let new = new.unwrap_or_else(|| self.new_track_index(true));
        // Heard in the song's key, it goes in in that key.
        let semis = self.sample_transpose(index).round() as i8;
        self.session.checkpoint();
        let placed = self.put_audio(&s.path, &s.name, Some(&s), semis, track, new, tick);
        self.placed(placed, &s.name, cx);
    }

    /// An audio file as a region at `tick` on `track` (a new audio track at `new` without one).
    /// A library sample (`s`) that's a loop knowing its tempo is stretched to the song's and
    /// loops on; `semis` moves it into the song's key.
    #[allow(clippy::too_many_arguments)]
    fn put_audio(
        &mut self,
        file: &Path,
        name: &str,
        s: Option<&Sample>,
        semis: i8,
        track: Option<usize>,
        new: usize,
        tick: u64,
    ) -> Result<At, String> {
        let dir = self.session.dir.clone();
        let p = &mut self.session.project;
        let t = match track {
            Some(t) => t,
            None => {
                let add = Command::AddTrack {
                    name: name.to_string(),
                    instrument: None,
                    index: Some(new),
                };
                commands::apply(p, &dir, add)?;
                new.min(p.tracks.len() - 1)
            }
        };
        let add = Command::AddAudioRegion {
            track: t,
            file: file.to_path_buf(),
            start: tick,
            offset: 0,
            length: None,
        };
        commands::apply(p, &dir, add)?;
        let at: At = (t, p.tracks[t].regions.len() - 1);
        let Some(s) = s else {
            return Ok(at);
        };
        if let Some(length) = loop_ticks(&s.analysis, p.tempo) {
            let now = p.region(at).length;
            if length.abs_diff(now) > now / 200 {
                p.flex(at, length)?;
            }
        }
        // Looping, as `pina samples put` leaves one, till the next region or the end; a
        // loop following the song's tempo.
        let r = &mut p.tracks[t].regions[at.1];
        r.transpose = semis;
        r.looped = s.analysis.kind == Kind::Loop;
        if let Content::Audio { follow, .. } = &mut r.content {
            *follow = s.analysis.kind == Kind::Loop;
        }
        Ok(at)
    }

    /// Audio files dropped from the Finder at `at` (a window position): each a region at the
    /// tick under the pointer, the first on the audio track there, the rest (or all, over any
    /// other track or under them) on new audio tracks. A file the library knows goes in as from
    /// the browser: a loop fitted to the song's tempo and key. One undo takes it all back.
    pub fn drop_files(&mut self, paths: &[PathBuf], at: Point<f32>, cx: &mut Context<Self>) {
        self.file_drag = None;
        let audio: Vec<&PathBuf> = paths
            .iter()
            .filter(|p| {
                p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    ["wav", "wave", "aif", "aiff", "flac", "mp3"]
                        .contains(&e.to_lowercase().as_str())
                })
            })
            .collect();
        if audio.is_empty() {
            self.status = Some("Pina Studio takes WAV, AIFF, FLAC and MP3 files.".into());
            cx.notify();
            return;
        }
        // On the Agent panel, sounds dropped on it are for the agent to use.
        let agent_left = self.viewport_w - self.panel_width();
        if self.view == View::Arrange && self.show_agent && !self.show_browser && at.x >= agent_left
        {
            let files: Vec<PathBuf> = audio.iter().map(|p| p.to_path_buf()).collect();
            self.give_agent_files(&files, cx);
            return;
        }
        // On the Sampler, with its editor open, samples dropped on it go into it.
        let editor_top = self.viewport_h - STATUS_H - self.editor_height();
        if self.view == View::Arrange
            && self.bottom == Some(Bottom::Sampler)
            && self.sampler_track().is_some()
            && at.y >= editor_top
        {
            let files: Vec<PathBuf> = audio.iter().map(|p| p.to_path_buf()).collect();
            self.add_samples(&files, cx);
            return;
        }
        // On the Song Study page, a song dropped is studied.
        if self.view == View::Study {
            for file in audio {
                self.study_file(file.clone(), cx);
            }
            return;
        }
        let n = self.session.project.tracks.len();
        let (mut track, tick, mut new) = match self.drop_target(at) {
            Some(Drop::Audio(t, tick)) => (Some(t), tick, t + 1),
            Some(Drop::Zone(t, tick) | Drop::Under(t, tick)) => (None, tick, t + 1),
            Some(Drop::New(tick)) => (None, tick, n),
            None => (None, 0, n),
        };
        self.session.checkpoint();
        let mut placed = Err("nothing dropped".to_string());
        for file in audio.iter().copied() {
            let known = self
                .browser
                .library
                .samples
                .iter()
                .position(|s| s.path == *file);
            let s = known.and_then(|k| self.browser.sample(k).cloned());
            let semis = known.map_or(0, |k| self.sample_transpose(k).round() as i8);
            let name = file
                .file_stem()
                .map_or("Audio".into(), |f| f.to_string_lossy().into_owned());
            placed = self.put_audio(file, &name, s.as_ref(), semis, track.take(), new, tick);
            match &placed {
                Ok((t, _)) => new = t + 1,
                Err(_) => break,
            }
        }
        let what = match audio.as_slice() {
            [one] => one
                .file_stem()
                .map_or(String::new(), |f| f.to_string_lossy().into_owned()),
            many => format!("{} files", many.len()),
        };
        self.placed(placed, &what, cx);
    }

    /// A one-shot as a zone on `track`'s sampler, or on a new sampler track without one.
    pub fn add_zone(
        &mut self,
        index: usize,
        track: Option<usize>,
        at_end: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.browser.sample(index).cloned() else {
            return;
        };
        let new = self.new_track_index(at_end);
        self.session.checkpoint();
        let dir = self.session.dir.clone();
        let p = &mut self.session.project;
        let placed = (|| match track {
            Some(t) => {
                let Some(Instrument::Sampler(mut sampler)) = p.tracks[t].instrument.clone() else {
                    return Err("That track isn't a sampler.".to_string());
                };
                place_zone(&mut sampler, &s, None)?;
                let set = Command::SetInstrument {
                    track: t,
                    instrument: Instrument::Sampler(sampler),
                };
                commands::apply(p, &dir, set)?;
                Ok((t, usize::MAX))
            }
            None => {
                let mut sampler = Sampler::default();
                place_zone(&mut sampler, &s, None)?;
                let add = Command::AddTrack {
                    name: s.name.clone(),
                    instrument: Some(Instrument::Sampler(sampler)),
                    index: Some(new),
                };
                commands::apply(p, &dir, add)?;
                Ok((new, usize::MAX))
            }
        })();
        self.placed(placed, &s.name, cx);
    }

    /// After adding a sample: selects where it went and says so, or undoes the attempt and says
    /// why.
    fn placed(&mut self, placed: Result<At, String>, name: &str, cx: &mut Context<Self>) {
        match placed {
            Ok((t, r)) => {
                self.select_track(t);
                self.region = (r != usize::MAX).then_some((t, r));
                let track = &self.session.project.tracks[t].name;
                self.status = Some(format!("Added {name} to {track}"));
                self.changed(cx);
                // `changed` keeps whatever the engine had to say.
                if self.status.is_none() {
                    let track = &self.session.project.tracks[t].name;
                    self.status = Some(format!("Added {name} to {track}"));
                }
            }
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    /// Where a sample dropped at `p` (a window position) would go, if it's over the tracks.
    pub fn drop_target(&self, p: Point<f32>) -> Option<Drop> {
        let b = self.lanes_bounds.get();
        let (left, top) = (f32::from(b.origin.x), f32::from(b.origin.y));
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if p.x < left || p.y < top || p.x > left + w || p.y > top + h {
            return None;
        }
        let row = self.track_at_y(p.y - top).unwrap_or(usize::MAX);
        let tick = self.snap(self.tick_at(p.x));
        Some(match self.session.project.tracks.get(row) {
            None => Drop::New(tick),
            Some(t) => match &t.instrument {
                None => Drop::Audio(row, tick),
                Some(Instrument::Sampler(_)) => Drop::Zone(row, tick),
                Some(_) => Drop::Under(row, tick),
            },
        })
    }

    /// A sample or a map box being dragged.
    pub fn drag_sample(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        let mut out = false;
        match &mut self.drag {
            Some(Drag::Sample {
                from, at, moved, ..
            }) => {
                *at = p;
                let far = (p.x - from.x).hypot(p.y - from.y);
                *moved |= far > 4.0;
                out = far > 40.0;
            }
            Some(Drag::MapBox { to, .. }) => *to = p,
            _ => {}
        }
        // Dragged out of the page: the tracks come back to drop it on, the compact browser
        // beside them.
        if out && self.view == View::Samples {
            self.view = View::Arrange;
            self.show_browser = true;
        }
        cx.notify();
    }

    /// Lets go of a sample over the tracks, or of a box on the map.
    pub fn drop_sample(&mut self, drag: Option<Drag>, cx: &mut Context<Self>) {
        match drag {
            Some(Drag::Sample {
                index,
                at,
                moved: true,
                ..
            }) => {
                // On the timeline, a sample is audio there; on a sampler track, a pad.
                match self.drop_target(at) {
                    Some(Drop::Audio(t, tick)) => self.add_audio(index, Some(t), tick, false, cx),
                    Some(Drop::Zone(t, _)) => self.add_zone(index, Some(t), false, cx),
                    Some(Drop::New(tick)) => self.add_audio_at(
                        index,
                        None,
                        Some(self.session.project.tracks.len()),
                        tick,
                        cx,
                    ),
                    Some(Drop::Under(t, tick)) => {
                        self.add_audio_at(index, None, Some(t + 1), tick, cx)
                    }
                    None => {}
                }
            }
            Some(Drag::MapBox { from, to }) => self.box_area(from, to),
            _ => {}
        }
        cx.notify();
    }

    // The kit.

    /// Builds a kit from the filters (key, tempo, words, traits); locked slots keep their sound.
    pub fn build_kit(&mut self) {
        let b = &self.browser;
        // Locked sounds stay, and the rest are picked to go with them.
        let keep = b
            .kit
            .iter()
            .filter(|k| k.locked)
            .filter_map(|k| Some(b.sample(k.hit()?.index)?.path.clone()))
            .collect();
        let seed = KitSeed {
            key: self.sample_key().map(|k| k.name()),
            bpm: self.sample_bpm(),
            text: b.text.clone(),
            traits: b.traits.clone(),
            alternatives: 6,
            keep,
            ..Default::default()
        };
        let kit = search::kit(&b.library.samples, &seed);
        let old = std::mem::take(&mut self.browser.kit);
        self.browser.kit = kit
            .slots
            .into_iter()
            .map(
                |slot| match old.iter().find(|o| o.locked && o.slot.role == slot.role) {
                    Some(o) => o.clone(),
                    None => KitPick {
                        slot,
                        choice: 0,
                        locked: false,
                    },
                },
            )
            .collect();
    }

    /// New picks for the slots that aren't locked.
    pub fn shuffle_kit(&mut self, cx: &mut Context<Self>) {
        let mut seed = Instant::now().elapsed().as_nanos() as u64 | 1;
        seed ^= std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        for k in self.browser.kit.iter_mut().filter(|k| !k.locked) {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let n = k.options().len().max(1);
            k.choice = (seed % n as u64) as usize;
        }
        cx.notify();
    }

    /// Makes the kit a sampler track: drums on their General MIDI notes (a closed hat choking an
    /// open one), tonal sounds across free notes, in tune.
    pub fn make_kit(&mut self, cx: &mut Context<Self>) {
        let samples = &self.browser.library.samples;
        let picks: Vec<(Category, Sample)> = self
            .browser
            .kit
            .iter()
            .filter_map(|k| Some((k.slot.role, samples.get(k.hit()?.index)?.clone())))
            .collect();
        if picks.is_empty() {
            self.status = Some("Nothing fits this kit: loosen the filters and try again.".into());
            cx.notify();
            return;
        }
        let mut sampler = Sampler::default();
        for (role, s) in &picks {
            // A sound that won't fit is left out; the rest still make the kit.
            let _ = place_zone(&mut sampler, s, Some(*role));
        }
        let n = sampler.zones.len();
        let name = match self.sample_key() {
            Some(k) => format!("Kit in {}", k.name()),
            None => "Kit".to_string(),
        };
        let index = self.new_track_index(false);
        self.session.checkpoint();
        let dir = self.session.dir.clone();
        let add = Command::AddTrack {
            name: name.clone(),
            instrument: Some(Instrument::Sampler(sampler)),
            index: Some(index),
        };
        match commands::apply(&mut self.session.project, &dir, add) {
            Ok(_) => {
                self.select_track(index);
                self.region = None;
                self.changed(cx);
                self.status = Some(format!("Made {name}: {n} sounds on a sampler"));
            }
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    // The map.

    /// A map position's place in the window.
    fn map_point(&self, b: Bounds<Pixels>, m: [f32; 2]) -> Point<f32> {
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        let z = self.browser.zoom;
        let c = self.browser.center;
        point(
            f32::from(b.origin.x) + w / 2.0 + (m[0] - c[0]) * w / 2.0 * 0.9 * z,
            f32::from(b.origin.y) + h / 2.0 + (m[1] - c[1]) * h / 2.0 * 0.84 * z,
        )
    }

    /// The sample whose dot is nearest a window position, within reach.
    fn map_dot_at(&self, p: Point<f32>) -> Option<usize> {
        let b = self.browser.map_bounds.get();
        let places = self.browser.places()?;
        places
            .iter()
            .enumerate()
            .map(|(i, &m)| {
                let q = self.map_point(b, m);
                (i, (q.x - p.x).hypot(q.y - p.y))
            })
            .filter(|&(_, d)| d < 9.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// The results keep to the samples in a box drawn on the map; a click without a box shows
    /// them all again.
    fn box_area(&mut self, from: Point<f32>, to: Point<f32>) {
        let b = self.browser.map_bounds.get();
        let (x0, x1) = (from.x.min(to.x), from.x.max(to.x));
        let (y0, y1) = (from.y.min(to.y), from.y.max(to.y));
        self.browser.area = match (x1 - x0 > 4.0 || y1 - y0 > 4.0, self.browser.places()) {
            (true, Some(places)) => Some(
                places
                    .iter()
                    .enumerate()
                    .filter(|&(_, &m)| {
                        let q = self.map_point(b, m);
                        (x0..=x1).contains(&q.x) && (y0..=y1).contains(&q.y)
                    })
                    .map(|(i, _)| i)
                    .collect(),
            ),
            _ => None,
        };
        self.find_samples();
    }

    /// Scrolling the map pans it; with ⌘ (or a pinch) it zooms round the pointer.
    fn scroll_map(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => p.map(f32::from),
            ScrollDelta::Lines(l) => l.map(|v| v * 24.0),
        };
        let b = self.browser.map_bounds.get();
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let z = self.browser.zoom;
        let (sx, sy) = (w / 2.0 * 0.9 * z, h / 2.0 * 0.84 * z);
        let c = &mut self.browser.center;
        if e.modifiers.platform {
            // Keep what's under the pointer there.
            let p = pt(e.position);
            let m = [
                c[0] + (p.x - f32::from(b.origin.x) - w / 2.0) / sx,
                c[1] + (p.y - f32::from(b.origin.y) - h / 2.0) / sy,
            ];
            let zoom = (z * (1.0 + d.y * 0.01)).clamp(0.6, 40.0);
            let k = z / zoom;
            *c = [m[0] - (m[0] - c[0]) * k, m[1] - (m[1] - c[1]) * k];
            self.browser.zoom = zoom;
        } else {
            c[0] -= d.x / sx;
            c[1] -= d.y / sy;
        }
        cx.notify();
    }

    fn hover_map(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        if self.drag.is_some() {
            return;
        }
        let dot = self.map_dot_at(p);
        if dot != self.browser.hovered {
            self.browser.hovered = dot;
            // As XO does: what's under the pointer plays.
            if let Some(i) = dot {
                self.audition_sample(i, cx);
            }
            cx.notify();
        }
    }

    // The views.

    /// The editor bar's detail: how many sounds, or how the scan is going.
    pub fn samples_detail(&self) -> String {
        let b = &self.browser;
        match &b.scan {
            Some(p) if p[1].load(Ordering::Relaxed) > 0 => "Analysing…".into(),
            _ if !b.loaded => "Loading…".into(),
            _ => format!("{} sounds", b.library.samples.len()),
        }
    }

    /// The editor bar's controls: List, Map or Kit, and Play in Key.
    pub fn samples_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let view = |label: &'static str, v: Results, on: bool, cx: &mut Context<Self>| {
            seg(label, on, 24.)
                .debug_selector(move || format!("results-{v:?}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.browser.view = v;
                        if v == Results::Kit {
                            ws.build_kit();
                        }
                        cx.notify();
                    }),
                )
        };
        let b = &self.browser;
        let key = self
            .sample_key()
            .or_else(|| self.session.project.song_key());
        div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .debug_selector(|| "play-in-key".into())
                    .child(switch(b.play_in_key))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(if b.play_in_key { TEXT } else { TEXT_2 }))
                            .whitespace_nowrap()
                            .child(match key {
                                Some(k) => format!("Play in {}", k.name()),
                                None => "Play in key".to_string(),
                            }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.browser.play_in_key = !ws.browser.play_in_key;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(view("List", Results::List, b.view == Results::List, cx))
                    .child(view("Map", Results::Map, b.view == Results::Map, cx))
                    .child(view("Kit", Results::Kit, b.view == Results::Kit, cx)),
            )
    }

    /// The search field: words, as they're typed.
    fn search_field(&self, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let focused = b.searching;
        div()
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .bg(rgb(WELL))
            .border_1()
            .border_color(if focused {
                alpha(ACCENT, 0.8)
            } else {
                rgb(LINE).into()
            })
            .cursor_text()
            .debug_selector(|| "sample-search".into())
            .child(icon(Icon::Search, 14.).text_color(rgb(if focused { TEXT_2 } else { TEXT_3 })))
            .child(div().flex_1().min_w(px(0.)).child(b.search.clone()))
            .when(!b.text.is_empty(), |d| {
                d.child(
                    div()
                        .size(px(18.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(9.))
                        .bg(rgb(RAISED))
                        .cursor_pointer()
                        .child(icon(Icon::Close, 11.).text_color(rgb(TEXT_2)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.browser.search.update(cx, |i, cx| i.set_text("", cx));
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    ws.browser.focus_search = true;
                    cx.notify();
                }),
            )
    }

    /// All, one-shots or loops.
    fn kind_tray(&self, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let kind = |label: &'static str, k: Option<Kind>, on: bool, cx: &mut Context<Self>| {
            seg(label, on, 24.)
                .flex_1()
                .debug_selector(move || format!("kind-{k:?}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.browser.kind = k;
                        ws.refilter(cx);
                    }),
                )
        };
        tray()
            .p(px(2.))
            .rounded(px(8.))
            .child(kind("Any", None, b.kind.is_none(), cx))
            .child(kind(
                "One-shots",
                Some(Kind::OneShot),
                b.kind == Some(Kind::OneShot),
                cx,
            ))
            .child(kind(
                "Loops",
                Some(Kind::Loop),
                b.kind == Some(Kind::Loop),
                cx,
            ))
    }

    /// A chip for each sound the library has, drums first, with how many it would find.
    fn category_chips(&self, cx: &mut Context<Self>) -> Vec<Div> {
        let b = &self.browser;
        // What the search's words name lights too; clicked, those words come out of it.
        let said = search::named(&b.text).0;
        Category::ALL
            .iter()
            .copied()
            .filter(|c| b.counts.get(c).is_some_and(|&n| n > 0) || b.categories.contains(c))
            .map(|c| {
                let picked = b.categories.contains(&c);
                let worded = said.contains(&c) && !picked;
                filter_chip(
                    c.name(),
                    Some(b.counts.get(&c).copied().unwrap_or(0)),
                    picked || worded,
                    Some(category_color(c)),
                )
                .debug_selector(move || format!("category-{}", c.name()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| match worded {
                        true => ws.unsay(Some(c), None, cx),
                        false => ws.toggle_category(c, cx),
                    }),
                )
            })
            .collect()
    }

    /// Takes the words naming a category or trait out of the search.
    fn unsay(&mut self, c: Option<Category>, t: Option<&'static str>, cx: &mut Context<Self>) {
        let text = search::unsay(&self.browser.text, c, t);
        self.browser
            .search
            .update(cx, |i, cx| i.set_text(&text, cx));
    }

    /// The browser's own page, in the tracks' place as the mixer is: what to look for, what's
    /// found (a list, the map or a kit), and the selected sample in detail.
    pub fn samples_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let toolbar = div()
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Samples"),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(self.samples_detail()),
            )
            .child(div().flex_1())
            .child(self.samples_controls(cx))
            .child(
                button("Compact", false)
                    .child(icon(Icon::PanelRight, 13.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .debug_selector(|| "samples-compact".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.toggle_samples(cx)),
                    ),
            );
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(self.samples_body(cx))
    }

    /// The Browser, beside the tracks: everything that goes in a song in one place, under one
    /// search. All (devices, presets, sounds and songs that match), Instruments, Effects and the
    /// sample library (Sounds); the search follows from tab to tab. Starred devices lead each
    /// list of devices.
    pub fn browser_panel(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let scope = self.catalog.scope;
        let sounds = scope == Scope::Sounds;
        let detail = match sounds {
            true => self.samples_detail(),
            false => format!("{} devices", self.catalog.devices.len()),
        };
        let header = div()
            .h(px(TOP_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(12.))
            .pr(px(7.))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Browser"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(detail),
            )
            // The sample library has a page of its own, with its map and kits.
            .when(sounds, |d| {
                d.child(
                    icon_seg(Icon::Expand, false, 26.)
                        .debug_selector(|| "samples-page".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.toggle_samples_page(cx)),
                        ),
                )
            })
            .child(
                icon_seg(Icon::Close, false, 26.)
                    .debug_selector(|| "close-samples".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.show_browser = false;
                            cx.notify();
                        }),
                    ),
            );
        let search = match sounds {
            true => self.search_field(cx),
            false => div()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(8.))
                .bg(rgb(WELL))
                .border_1()
                .border_color(rgb(LINE))
                .cursor_text()
                .child(icon(Icon::Search, 14.).text_color(rgb(TEXT_3)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .debug_selector(|| "browser-search".into())
                        .child(self.catalog.search.clone()),
                ),
        };
        let tab = |label: &'static str, s: Scope, cx: &mut Context<Self>| {
            seg(label, scope == s, 26.)
                .flex_auto()
                .justify_center()
                .debug_selector(move || format!("scope-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.browse(s, cx)),
                )
        };
        let tabs = tray()
            .rounded(px(9.))
            .child(tab("All", Scope::All, cx))
            .child(tab("Instruments", Scope::Instruments, cx))
            .child(tab("Effects", Scope::Effects, cx))
            .child(tab("Sounds", Scope::Sounds, cx));
        let body = match sounds {
            true => self.sounds_body(cx),
            false => self.device_browser(cx),
        };
        div()
            .w(px(self.panel_width()))
            .flex_none()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(header)
            .children(self.picking_bar(cx))
            .child(
                div()
                    .flex_none()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(search)
                    .child(tabs),
            )
            .child(body)
            .child(self.panel_edge(cx))
    }

    /// A Browser tab: its search carried over between the devices' field and the sounds', and
    /// the field ready to type in.
    pub fn browse(&mut self, scope: Scope, cx: &mut Context<Self>) {
        let (was, now) = (self.catalog.scope == Scope::Sounds, scope == Scope::Sounds);
        if was != now {
            let (from, to) = match now {
                true => (self.catalog.search.clone(), self.browser.search.clone()),
                false => (self.browser.search.clone(), self.catalog.search.clone()),
            };
            let text = from.read(cx).text().to_string();
            to.update(cx, |i, cx| i.set_text(&text, cx));
        }
        self.catalog.scope = scope;
        self.catalog.cat = self.catalog.cat.filter(|c| match scope {
            Scope::Instruments => c.instrument(),
            Scope::Effects => !c.instrument(),
            _ => true,
        });
        self.catalog.picked = None;
        self.show_browser = true;
        self.browser.focus_search = true;
        cx.notify();
    }

    /// The compact browser, beside the tracks: a search, what kind, the song's fit and what
    /// sounds; a list to hear and drag onto the tracks; and the picked sound. Its page has the
    /// rest.
    /// The Browser's Sounds: the sample library's kinds, the song's fit, its categories, the list
    /// and the picked sound.
    fn sounds_body(&mut self, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let p = &self.session.project;
        let song = match p.song_key() {
            Some(k) => format!("Fits {} · {:.0}", k.short(), p.tempo),
            None => format!("Fits {:.0} BPM", p.tempo),
        };
        let fits = self.fits_song();
        let in_key = b.play_in_key;
        let toggles = div()
            .flex()
            .gap(px(6.))
            .child(
                filter_chip(song, None, fits, None)
                    .flex_1()
                    .justify_center()
                    .debug_selector(|| "fits-song".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.set_fits_song(!fits, cx)),
                    ),
            )
            .child(
                filter_chip("Play in key", None, in_key, None)
                    .flex_1()
                    .justify_center()
                    .debug_selector(|| "play-in-key".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.browser.play_in_key = !ws.browser.play_in_key;
                            cx.notify();
                        }),
                    ),
            );
        let chips: Vec<Div> = self
            .category_chips(cx)
            .into_iter()
            .map(|c| c.flex_none())
            .collect();
        let like = b
            .like
            .as_ref()
            .and_then(|(i, _)| b.sample(*i))
            .map(|s| s.name.clone());
        let shown = self.panel_hits().len();
        let list_head = div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(12.))
            .border_t_1()
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .text_size(px(11.))
            .text_color(rgb(TEXT_3))
            .whitespace_nowrap()
            .child(match &like {
                Some(name) => div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(rgb(ACCENT_TEXT))
                    .child(format!("Sounds like {name}")),
                None => div()
                    .flex_1()
                    .child(format!("{shown} sounds · best match first")),
            })
            .when(like.is_some(), |d| {
                d.child(
                    link("Back")
                        .debug_selector(|| "like-back".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.browser.like = None;
                                cx.notify();
                            }),
                        ),
                )
            });
        let list = match shown {
            0 if like.is_none() => self.empty_results(cx).into_any_element(),
            n => uniform_list(
                "sample-panel-list",
                n,
                cx.processor(|ws, range: Range<usize>, _, cx| ws.panel_rows(range, cx)),
            )
            .track_scroll(&self.browser.panel_list)
            .flex_1()
            .into_any_element(),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .px(px(10.))
                    .pb(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(self.kind_tray(cx))
                    .child(toggles)
                    .when(!chips.is_empty(), |d| {
                        d.child(
                            div()
                                .id("panel-categories")
                                .flex()
                                .gap(px(5.))
                                .overflow_x_scroll()
                                .children(chips),
                        )
                    }),
            )
            .child(list_head)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .bg(rgb(LANE))
                    .child(list),
            )
            .children(self.panel_footer(cx))
    }

    /// Rows of the compact list, as the list draws them.
    fn panel_rows(
        &mut self,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        range
            .filter_map(|pos| {
                let hit = self.panel_hits().get(pos)?.clone();
                Some(self.panel_row(pos, &hit, cx))
            })
            .collect()
    }

    /// How far through its preview a sample is, 0 to 1, while it's heard.
    fn heard(&self, index: usize) -> Option<f32> {
        let (_, since, seconds) = self.browser.audition.filter(|a| a.0 == index)?;
        Some((since.elapsed().as_secs_f32() / seconds.max(0.01)).min(1.0))
    }

    /// A row of the compact list: its outline, name and what it is, its key or note, and its
    /// tempo. A click hears it, a drag puts it on a track, a double-click adds it to the song.
    fn panel_row(&self, pos: usize, hit: &Hit, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let b = &self.browser;
        let index = hit.index;
        let s = &b.library.samples[index];
        let a = &s.analysis;
        let color = category_color(a.category);
        let selected = b.selected == Some(index);
        let heard = self.heard(index);
        let semis = self.sample_transpose(index).round();
        let mut what = match (a.kind, a.bars) {
            (Kind::Loop, Some(bars)) => {
                let n = match (bars - bars.round()).abs() < 0.05 {
                    true => format!("{bars:.0}"),
                    false => format!("{bars:.1}"),
                };
                let s = if n == "1" { "" } else { "s" };
                format!("{} loop · {n} bar{s}", a.category.name())
            }
            (Kind::Loop, None) => {
                format!("{} loop · {}", a.category.name(), seconds_text(a.seconds))
            }
            (Kind::OneShot, _) => format!("{} · {}", a.category.name(), seconds_text(a.seconds)),
        };
        if let Some(t) = a.traits.first() {
            what = format!("{what} · {t}");
        }
        // Its key, short, or its note.
        let tone = a
            .key
            .as_ref()
            .map(|k| {
                let mode = if k.minor { "min" } else { "maj" };
                format!("{} {mode}", NOTES[k.tonic as usize % 12])
            })
            .or_else(|| a.pitch.map(|p| note_name(p.note)));
        let group = format!("panel-row-{pos}");
        let give = self.agent_button(&s.path, &group, cx);
        div()
            .id(("panel-row", pos))
            .group(group)
            .w_full()
            .h(px(PANEL_ROW_H))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(9.))
            .relative()
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .bg(if selected {
                alpha(ACCENT, 0.12)
            } else {
                gpui::transparent_black()
            })
            .when(!selected, |d| d.hover(|s| s.bg(rgb(HEADERS))))
            .cursor_pointer()
            .debug_selector(move || format!("panel-row-{pos}"))
            .when(selected, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(2.))
                        .h_full()
                        .bg(rgb(ACCENT)),
                )
            })
            .child(
                div()
                    .w(px(52.))
                    .h(px(26.))
                    .flex_none()
                    .relative()
                    .child(wave(
                        a.waveform.clone(),
                        color,
                        if selected || heard.is_some() {
                            1.0
                        } else {
                            0.7
                        },
                    ))
                    .when_some(heard, |d, f| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .left(relative(f))
                                .w(px(1.5))
                                .h_full()
                                .bg(rgb(TEXT)),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if selected { TEXT } else { TEXT_2 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(s.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .min_w(px(0.))
                            .child(
                                div()
                                    .size(px(6.))
                                    .flex_none()
                                    .rounded(px(3.))
                                    .bg(rgb(color)),
                            )
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(what),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(2.))
                    .font_family(MONO)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(if tone.is_some() { TEXT_2 } else { TEXT_4 }))
                            .child(tone.unwrap_or_else(|| "—".into())),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(5.))
                            .text_size(px(10.))
                            .when(semis != 0.0, |d| {
                                d.child(div().text_color(rgb(ACCENT_TEXT)).child(semis_text(semis)))
                            })
                            .when_some(a.bpm, |d, bpm| {
                                d.child(
                                    div().text_color(rgb(TEXT_3)).child(format!("{bpm:.0} BPM")),
                                )
                            }),
                    ),
            )
            .child(give)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    if e.click_count == 2 {
                        ws.add_sample(index, cx);
                        return;
                    }
                    ws.select_sample(index, true, cx);
                    let at = pt(e.position);
                    ws.begin_drag(
                        Drag::Sample {
                            index,
                            from: at,
                            at,
                            moved: false,
                        },
                        cx,
                    );
                }),
            )
    }

    /// Under the compact list: the picked sound, to hear, add to the song, or find more like.
    fn panel_footer(&self, cx: &mut Context<Self>) -> Option<Div> {
        let b = &self.browser;
        let index = b.selected?;
        let s = b.sample(index)?;
        let a = &s.analysis;
        let color = category_color(a.category);
        let heard = self.heard(index);
        let liking = b.like.as_ref().is_some_and(|l| l.0 == index);
        Some(
            div()
                .flex_none()
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .border_t_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(chip(a.category.name(), color))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(search::describe(s)),
                )
                .child(
                    div()
                        .h(px(34.))
                        .relative()
                        .cursor_pointer()
                        .debug_selector(|| "panel-wave".into())
                        .child(wave(a.waveform.clone(), color, 0.9))
                        .when_some(heard, |d, f| {
                            d.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left(relative(f))
                                    .w(px(1.5))
                                    .h_full()
                                    .bg(rgb(TEXT)),
                            )
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| ws.audition_sample(index, cx)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(
                            primary("Add to song")
                                .flex_1()
                                .child(icon(Icon::Plus, 14.).text_color(rgb(0xffffff)))
                                .flex_row_reverse()
                                .debug_selector(|| "panel-add".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.add_sample(index, cx)),
                                ),
                        )
                        .child(
                            button("More like it", liking)
                                .h(px(30.))
                                .debug_selector(|| "panel-like".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.like_sample(index, cx)),
                                ),
                        )
                        .child(self.give_button(s.path.clone(), 30., cx)),
                ),
        )
    }

    pub fn samples_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let middle = match self.browser.view {
            Results::List => self.results_list(cx).into_any_element(),
            Results::Map => self.results_map(cx).into_any_element(),
            Results::Kit => self.kit_view(cx).into_any_element(),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .bg(rgb(LANE))
            .child(self.filters(cx))
            .child(div().flex_1().min_w(px(0.)).flex().flex_col().child(middle))
            .child(self.details(cx))
    }

    /// The left column: what to look for, and the library.
    fn filters(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = &self.browser;
        let search = self.search_field(cx);
        let song_key = self.session.project.song_key();
        let tempo = self.session.project.tempo;
        let fits = self.fits_song();
        let fits_row = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .p(px(10.))
            .rounded(px(8.))
            .bg(if fits {
                alpha(ACCENT, 0.1)
            } else {
                rgb(CARD).into()
            })
            .border_1()
            .border_color(if fits {
                alpha(ACCENT, 0.45)
            } else {
                rgb(LINE).into()
            })
            .cursor_pointer()
            .debug_selector(|| "fits-song".into())
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Fits the song"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(match song_key {
                                Some(k) => format!("{} · {tempo:.0} BPM", k.name()),
                                None => format!("Its key and {tempo:.0} BPM"),
                            }),
                    ),
            )
            .child(switch(fits))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| ws.set_fits_song(!fits, cx)),
            );
        let key_label = match b.key {
            KeyFilter::Any => "Any key".to_string(),
            KeyFilter::Song => match song_key {
                Some(k) => format!("Song key ({})", k.short()),
                None => "Song key".to_string(),
            },
            KeyFilter::Key(k) => k.name(),
        };
        let key_label = match b.compatible && b.key != KeyFilter::Any {
            true => format!("{key_label} +"),
            false => key_label,
        };
        let labelled = |label: &'static str, control: Div| {
            div()
                .h(px(28.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .child(label),
                )
                .child(control)
        };
        let key_row = labelled(
            "Key",
            dropdown(key_label, 26.)
                .max_w(px(140.))
                .debug_selector(|| "sample-key".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        ws.menu = Some(ws.sample_key_menu(e.position));
                        cx.notify();
                    }),
                ),
        );
        let tempo_seg = |label: String, on: bool, song: bool, cx: &mut Context<Self>| {
            seg(label, on, 22.)
                .debug_selector(move || format!("sample-tempo-{song}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.browser.song_tempo = song;
                        ws.refilter(cx);
                    }),
                )
        };
        let tempo_row = labelled(
            "Tempo",
            tray()
                .p(px(2.))
                .rounded(px(8.))
                .child(tempo_seg("Any".into(), !b.song_tempo, false, cx))
                .child(tempo_seg(format!("{tempo:.0} BPM"), b.song_tempo, true, cx)),
        );
        let categories = self.category_chips(cx);
        let said_traits = search::named(&b.text).1;
        let traits: Vec<Div> = TRAITS
            .iter()
            .map(|(t, _)| *t)
            .filter(|t| {
                b.trait_counts.get(*t).is_some_and(|&n| n > 0) || b.traits.iter().any(|x| x == t)
            })
            .map(|t| {
                let picked = b.traits.iter().any(|x| x == t);
                let worded = said_traits.iter().any(|x| x == t) && !picked;
                filter_chip(t, b.trait_counts.get(t).copied(), picked || worded, None)
                    .debug_selector(move || format!("trait-{t}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| match worded {
                            true => ws.unsay(None, Some(t), cx),
                            false => ws.toggle_trait(t, cx),
                        }),
                    )
            })
            .collect();
        let any_sounds = !b.categories.is_empty();
        let any_traits = !b.traits.is_empty();
        let has_traits = !traits.is_empty();
        let folders: Vec<gpui::Stateful<Div>> = b
            .library
            .folders
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let count = b
                    .library
                    .samples
                    .iter()
                    .filter(|s| s.path.starts_with(f))
                    .count();
                let name = f.file_name().map_or_else(
                    || f.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                div()
                    .id(SharedString::from(format!("folder-{i}")))
                    .group("folder")
                    .h(px(26.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .hover(|s| s.bg(rgb(OFF)))
                    .child(icon(Icon::Folder, 13.).text_color(rgb(TEXT_3)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_2))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_3))
                            .child(count.to_string()),
                    )
                    .child(
                        div()
                            .invisible()
                            .group_hover("folder", |s| s.visible())
                            .cursor_pointer()
                            .child(icon(Icon::Close, 12.).text_color(rgb(TEXT_3)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.remove_library_folder(i, cx)),
                            ),
                    )
            })
            .collect();
        let scanning = b.scan.is_some();
        div()
            .id("sample-filters")
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(search)
                    .child(self.kind_tray(cx))
                    .child(fits_row)
                    .child(div().flex().flex_col().child(key_row).child(tempo_row))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(heading("Sounds").when(any_sounds, |d| {
                                d.child(link("Clear").on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        ws.browser.categories.clear();
                                        ws.refilter(cx);
                                    }),
                                ))
                            }))
                            .child(match categories.is_empty() {
                                true => {
                                    div()
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_4))
                                        .child(match b.library.samples.is_empty() {
                                            true => "What the library holds shows here.",
                                            false => "None with these filters.",
                                        })
                                }
                                false => div().flex().flex_wrap().gap(px(5.)).children(categories),
                            }),
                    )
                    .when(has_traits, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(heading("Character").when(any_traits, |d| {
                                    d.child(link("Clear").on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| {
                                            ws.browser.traits.clear();
                                            ws.refilter(cx);
                                        }),
                                    ))
                                }))
                                .child(div().flex().flex_wrap().gap(px(5.)).children(traits)),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                heading("Library").child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.))
                                        .cursor_pointer()
                                        .text_color(rgb(TEXT_3))
                                        .hover(|s| s.text_color(rgb(TEXT)))
                                        .debug_selector(|| "rescan".into())
                                        .child(icon(Icon::Rescan, 12.).text_color(rgb(TEXT_3)))
                                        .child(if scanning { "Scanning" } else { "Rescan" })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|ws, _, _, cx| ws.rescan(cx)),
                                        ),
                                ),
                            )
                            .children(folders)
                            .child(
                                button("Add folder…", false)
                                    .mt(px(4.))
                                    .child(icon(Icon::Plus, 12.).text_color(rgb(TEXT_3)))
                                    .flex_row_reverse()
                                    .debug_selector(|| "add-folder".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _, _, cx| ws.add_library_folder(cx)),
                                    ),
                            ),
                    ),
            )
    }

    /// The scan's progress, as a line and a bar.
    fn scan_progress(&self) -> Option<Div> {
        let p = self.browser.scan.as_ref()?;
        let (done, of) = (p[0].load(Ordering::Relaxed), p[1].load(Ordering::Relaxed));
        let text = match of {
            0 => "Looking for new samples…".to_string(),
            of => format!("Analysing {done} of {of}…"),
        };
        let part = if of == 0 {
            0.0
        } else {
            done as f32 / of as f32
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(11.))
                .text_color(rgb(TEXT_2))
                .whitespace_nowrap()
                .child(text)
                .child(
                    div()
                        .w(px(80.))
                        .h(px(4.))
                        .rounded(px(2.))
                        .bg(rgb(RAISED))
                        .child(
                            div()
                                .h_full()
                                .w(relative(part))
                                .rounded(px(2.))
                                .bg(rgb(ACCENT)),
                        ),
                ),
        )
    }

    /// Over the results: how many, and the scan's progress; for the list, its columns.
    fn results_header(&self, columns: bool, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let n = b.hits.len();
        let col = |label: &'static str, w: f32| {
            div()
                .w(px(w))
                .flex_none()
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_4))
                .child(label.to_uppercase())
        };
        div()
            .h(px(34.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{n} {}", if n == 1 { "sound" } else { "sounds" })),
                    )
                    .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(
                        match b.text.is_empty() && b.key == KeyFilter::Any {
                            true => "by name",
                            false => "best match first",
                        },
                    )),
            )
            .when_some(b.area.as_ref(), |d, area| {
                d.child(
                    chip(format!("In the box · {}", area.len()), ACCENT)
                        .gap(px(4.))
                        .cursor_pointer()
                        .child(icon(Icon::Close, 10.).text_color(rgb(ACCENT)))
                        .debug_selector(|| "clear-area".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.browser.area = None;
                                ws.refilter(cx);
                            }),
                        ),
                )
            })
            .child(div().flex_1())
            .children(self.scan_progress())
            .when(columns && b.scan.is_none(), |d| {
                d.child(col("Key", KEY_W))
                    .child(col("", SHIFT_W))
                    .child(col("BPM", BPM_W))
                    .child(col("Length", LENGTH_W))
                    .child(col("Fit", FIT_W))
                    // Over the agent's buttons.
                    .child(col("", 22.))
            })
    }

    /// Nothing to show: says why, and what to do.
    fn empty_results(&self, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let center = |title: String, body: String| {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .p(px(24.))
                .child(
                    div()
                        .size(px(40.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(12.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE))
                        .child(icon(Icon::Samples, 20.).text_color(rgb(TEXT_3))),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .max_w(px(420.))
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .text_center()
                        .child(body),
                )
        };
        let add = |cx: &mut Context<Self>| {
            primary("Add folder…")
                .mt(px(6.))
                .debug_selector(|| "add-folder-empty".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.add_library_folder(cx)),
                )
        };
        if !b.loaded {
            return center("Loading the library…".into(), String::new());
        }
        if b.library.samples.is_empty() {
            if b.scan.is_some() {
                return center(
                    "Analysing your samples".into(),
                    "Each one is listened to once: what it is, its key and tempo, how it sounds. They show here as they're done."
                        .into(),
                )
                .children(self.scan_progress());
            }
            return match b.library.folders.is_empty() {
                true => center(
                    "Your library is empty".into(),
                    "Add a folder of samples. tempo listens to every sound in it, so you can find one by what it is, its key, its tempo, or how it sounds."
                        .into(),
                ),
                false => center(
                    "No samples here yet".into(),
                    format!(
                        "Nothing was found in {}. Add a folder with WAV, AIFF, FLAC or MP3 files.",
                        b.library
                            .folders
                            .iter()
                            .map(|f| f.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ),
            }
            .child(add(cx));
        }
        // Filters that found nothing: offer to loosen each.
        let mut loosen: Vec<(&'static str, Loosen)> = Vec::new();
        if !b.text.is_empty() {
            loosen.push((
                "Clear the search",
                Box::new(|ws: &mut Workspace| ws.browser.text.clear()),
            ));
        }
        if b.key != KeyFilter::Any {
            loosen.push((
                "Any key",
                Box::new(|ws: &mut Workspace| ws.browser.key = KeyFilter::Any),
            ));
        }
        if b.song_tempo {
            loosen.push((
                "Any tempo",
                Box::new(|ws: &mut Workspace| ws.browser.song_tempo = false),
            ));
        }
        if b.kind.is_some() {
            loosen.push((
                "All kinds",
                Box::new(|ws: &mut Workspace| ws.browser.kind = None),
            ));
        }
        if !b.categories.is_empty() {
            loosen.push((
                "Every sound",
                Box::new(|ws: &mut Workspace| ws.browser.categories.clear()),
            ));
        }
        if !b.traits.is_empty() {
            loosen.push((
                "Any character",
                Box::new(|ws: &mut Workspace| ws.browser.traits.clear()),
            ));
        }
        if b.area.is_some() {
            loosen.push((
                "The whole map",
                Box::new(|ws: &mut Workspace| ws.browser.area = None),
            ));
        }
        let what = match b.text.is_empty() {
            true => "Nothing matches all of these filters.".to_string(),
            false => format!("Nothing matches “{}” with these filters.", b.text),
        };
        let buttons: Vec<Div> = loosen
            .into_iter()
            .enumerate()
            .map(|(i, (label, undo))| {
                button(label, false)
                    .debug_selector(move || format!("loosen-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            undo(ws);
                            if ws.browser.text.is_empty() {
                                ws.browser.search.update(cx, |i, cx| i.set_text("", cx));
                            }
                            ws.refilter(cx);
                        }),
                    )
            })
            .collect();
        center(
            "Nothing found".into(),
            format!("{what} Loosen one to see more:"),
        )
        .child(
            div()
                .mt(px(4.))
                .flex()
                .flex_wrap()
                .justify_center()
                .gap(px(6.))
                .children(buttons),
        )
    }

    fn results_list(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.browser.hits.len();
        let body = match n {
            0 => self.empty_results(cx).into_any_element(),
            n => uniform_list(
                "sample-list",
                n,
                cx.processor(|ws, range: Range<usize>, _, cx| ws.sample_rows(range, cx)),
            )
            .track_scroll(&self.browser.list)
            .flex_1()
            .into_any_element(),
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(self.results_header(n > 0, cx))
            .child(body)
            .child(self.info_line())
    }

    /// Rows of the results list, as the list draws them.
    fn sample_rows(
        &mut self,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        range
            .filter_map(|pos| {
                let hit = self.browser.hits.get(pos)?.clone();
                Some(self.sample_row(pos, &hit, cx))
            })
            .collect()
    }

    fn sample_row(&self, pos: usize, hit: &Hit, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let b = &self.browser;
        let index = hit.index;
        let s = &b.library.samples[index];
        let a = &s.analysis;
        let color = category_color(a.category);
        let selected = b.selected == Some(index);
        let playing = b.audition.is_some_and(|x| x.0 == index);
        let semis = self.sample_transpose(index);
        let traits = a
            .traits
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        let mono = |s: String, w: f32, c: u32| {
            div()
                .w(px(w))
                .flex_none()
                .font_family(MONO)
                .text_size(px(11.5))
                .text_color(rgb(c))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(s)
        };
        let tone = tone_text(a);
        let has_tone = tone.is_some();
        let give = self.agent_button(&s.path, "sample-row", cx);
        div()
            .id(("sample-row", pos))
            .group("sample-row")
            .w_full()
            .h(px(ROW_H))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(10.))
            .relative()
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .bg(if selected {
                alpha(ACCENT, 0.12)
            } else {
                gpui::transparent_black()
            })
            .when(!selected, |d| d.hover(|s| s.bg(rgb(HEADERS))))
            .cursor_pointer()
            .debug_selector(move || format!("sample-row-{pos}"))
            .when(selected, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(2.))
                        .h_full()
                        .bg(rgb(ACCENT)),
                )
            })
            .child(
                div()
                    .size(px(22.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(11.))
                    .bg(rgb(if playing { ACCENT } else { CARD }))
                    .border_1()
                    .border_color(rgb(if playing { ACCENT } else { LINE }))
                    .group_hover("sample-row", |s| s.border_color(rgb(LINE_STRONG)))
                    .child(icon(Icon::Play, 10.).text_color(rgb(if playing {
                        0xffffff
                    } else {
                        TEXT_2
                    }))),
            )
            .child(div().w(px(WAVE_W)).h(px(26.)).flex_none().child(wave(
                a.waveform.clone(),
                color,
                if selected { 1.0 } else { 0.75 },
            )))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if selected { TEXT } else { TEXT_2 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(s.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(chip(a.category.name(), color).h(px(16.)).px(px(5.)))
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(kind_name(a.kind)),
                            )
                            .when(!traits.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_size(px(10.5))
                                        .text_color(rgb(TEXT_3))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(format!("· {traits}")),
                                )
                            }),
                    ),
            )
            .child(
                mono(
                    tone.unwrap_or_else(|| "—".into()),
                    KEY_W,
                    if has_tone { TEXT_2 } else { TEXT_4 },
                )
                .text_ellipsis(),
            )
            // How far Play in Key moves it, to the nearest semitone.
            .child(
                mono(
                    match semis.round() {
                        0.0 => String::new(),
                        s => semis_text(s),
                    },
                    SHIFT_W,
                    ACCENT_TEXT,
                )
                .text_size(px(10.5)),
            )
            .child(mono(
                a.bpm.map_or("—".into(), |b| format!("{b:.0}")),
                BPM_W,
                if a.bpm.is_some() { TEXT_2 } else { TEXT_4 },
            ))
            .child(mono(seconds_text(a.seconds), LENGTH_W, TEXT_3))
            .child(
                div()
                    .w(px(FIT_W))
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .size(px(8.))
                            .rounded(px(4.))
                            .bg(alpha(ACCENT, 0.2 + 0.8 * hit.score.clamp(0.0, 1.0))),
                    ),
            )
            .child(give)
            .on_hover(cx.listener(move |ws, over: &bool, _, cx| {
                let now = if *over { Some(index) } else { None };
                if ws.browser.hovered != now && (*over || ws.browser.hovered == Some(index)) {
                    ws.browser.hovered = now;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    ws.select_sample(index, true, cx);
                    let at = pt(e.position);
                    ws.begin_drag(
                        Drag::Sample {
                            index,
                            from: at,
                            at,
                            moved: false,
                        },
                        cx,
                    );
                }),
            )
    }

    /// Under the results: the hovered (else selected) sample in a line, and why it's there.
    fn info_line(&self) -> Div {
        let b = &self.browser;
        let shown = b
            .hovered
            .or(b.selected)
            .and_then(|i| Some((i, b.sample(i)?)));
        div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .bg(rgb(SURFACE))
            .text_size(px(11.))
            .whitespace_nowrap()
            .overflow_hidden()
            .children(shown.map(|(i, s)| {
                let reasons = b.hit(i).map(|h| h.reasons.join(", ")).unwrap_or_default();
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w(px(0.))
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded(px(3.))
                            .bg(rgb(category_color(s.analysis.category))),
                    )
                    .child(
                        div()
                            .text_color(rgb(TEXT_2))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(search::describe(s)),
                    )
                    .when(!reasons.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_color(rgb(ACCENT_TEXT))
                                .child(format!("Why: {reasons}")),
                        )
                    })
            }))
            .when(shown.is_none(), |d| {
                d.text_color(rgb(TEXT_4))
                    .child("Hover a sound to see what it is · drag one onto the tracks to use it")
            })
    }

    fn results_map(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = &self.browser;
        let bounds = b.map_bounds.clone();
        let places: Vec<([f32; 2], u32, bool, usize)> = match b.places() {
            Some(places) => {
                let shown: HashSet<usize> = b.hits.iter().map(|h| h.index).collect();
                places
                    .iter()
                    .enumerate()
                    .map(|(i, &m)| {
                        let c = category_color(b.library.samples[i].analysis.category);
                        (m, c, shown.contains(&i), i)
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        let mb = b.map_bounds.get();
        let to_screen = {
            let (z, c) = (b.zoom, b.center);
            move |b: Bounds<Pixels>, m: [f32; 2]| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                point(
                    f32::from(b.origin.x) + w / 2.0 + (m[0] - c[0]) * w / 2.0 * 0.9 * z,
                    f32::from(b.origin.y) + h / 2.0 + (m[1] - c[1]) * h / 2.0 * 0.84 * z,
                )
            }
        };
        let (selected, hovered, playing) = (b.selected, b.hovered, b.audition.map(|a| a.0));
        let rect = match self.drag {
            Some(Drag::MapBox { from, to }) => Some((from, to)),
            _ => None,
        };
        let area = b.area.is_some();
        let painted = places.clone();
        let map = canvas(
            move |bnd, _, _| bounds.set(bnd),
            move |bnd, _, window, _| {
                let (w, h) = (f32::from(bnd.size.width), f32::from(bnd.size.height));
                // A faint grid, so panning and zooming show.
                for k in 1..8 {
                    let x = bnd.origin.x + px(w * k as f32 / 8.0);
                    let y = bnd.origin.y + px(h * k as f32 / 8.0);
                    window.paint_quad(fill(
                        Bounds::new(point(x, bnd.origin.y), size(px(1.), bnd.size.height)),
                        rgb(LINE_SOFT),
                    ));
                    window.paint_quad(fill(
                        Bounds::new(point(bnd.origin.x, y), size(bnd.size.width, px(1.))),
                        rgb(LINE_SOFT),
                    ));
                }
                let dot = |window: &mut Window, p: Point<f32>, r: f32, c: Hsla| {
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(px(p.x - r), px(p.y - r)),
                                size(px(r * 2.), px(r * 2.)),
                            ),
                            c,
                        )
                        .corner_radii(px(r)),
                    );
                };
                // What the filters leave out, faintly, then what they find over it.
                for pass in [false, true] {
                    for &(m, c, shown, _) in painted.iter().filter(|p| p.2 == pass) {
                        let p = to_screen(bnd, m);
                        if p.x < f32::from(bnd.origin.x) - 6.0
                            || p.x > f32::from(bnd.origin.x) + w + 6.0
                        {
                            continue;
                        }
                        match shown {
                            true => dot(window, p, 3.5, alpha(c, 0.9)),
                            false => dot(window, p, 2.5, alpha(c, if area { 0.12 } else { 0.18 })),
                        }
                    }
                }
                for (i, ring) in [(hovered, TEXT_2), (playing, ACCENT_TEXT), (selected, TEXT)] {
                    if let Some(&(m, c, ..)) = i.and_then(|i| painted.get(i)) {
                        let p = to_screen(bnd, m);
                        dot(window, p, 8.0, alpha(ring, 0.9));
                        dot(window, p, 6.5, rgb(LANE).into());
                        dot(window, p, 5.0, rgb(c).into());
                    }
                }
                if let Some((a, b)) = rect {
                    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
                    let r = Bounds::new(
                        point(px(x0), px(y0)),
                        size(px((a.x - b.x).abs()), px((a.y - b.y).abs())),
                    );
                    window.paint_quad(
                        fill(r, alpha(ACCENT, 0.1))
                            .border_widths(px(1.))
                            .border_color(alpha(ACCENT, 0.8)),
                    );
                }
            },
        )
        .absolute()
        .size_full();
        // The hovered sound's name, by its dot.
        let label = hovered
            .or(selected)
            .and_then(|i| Some((i, places.get(i)?.0, self.browser.sample(i)?)))
            .map(|(_, m, s)| {
                let p = to_screen(mb, m);
                div()
                    .absolute()
                    .left(px(p.x - f32::from(mb.origin.x) + 12.))
                    .top(px(p.y - f32::from(mb.origin.y) - 11.))
                    .h(px(22.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(0x1c1c22))
                    .border_1()
                    .border_color(rgb(LINE_STRONG))
                    .shadow_md()
                    .text_size(px(11.))
                    .whitespace_nowrap()
                    .child(s.name.clone())
                    .child(
                        div()
                            .text_color(rgb(TEXT_3))
                            .child(s.analysis.category.name()),
                    )
            });
        let present: HashSet<u32> = places.iter().map(|p| p.1).collect();
        let legend = LEGEND
            .iter()
            .filter(|(c, _)| present.contains(c))
            .map(|&(c, name)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(c)))
                    .child(name)
            });
        let empty = places.is_empty();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(self.results_header(false, cx))
            .child(
                div()
                    .id("sample-map")
                    .flex_1()
                    .min_h(px(0.))
                    .relative()
                    .overflow_hidden()
                    .cursor_crosshair()
                    .debug_selector(|| "sample-map".into())
                    .child(map)
                    .children(label)
                    .when(empty, |d| {
                        d.child(
                            div()
                                .absolute()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(TEXT_3))
                                .child(match self.browser.library.samples.is_empty() {
                                    true => "The map shows every sample once the library has some.",
                                    false => "Placing sounds on the map…",
                                }),
                        )
                    })
                    .on_mouse_move(
                        cx.listener(|ws, e: &MouseMoveEvent, _, cx| {
                            ws.hover_map(pt(e.position), cx)
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            let p = pt(e.position);
                            match ws.map_dot_at(p) {
                                // A dot: select it, and drag it to the tracks if you like.
                                Some(i) => {
                                    ws.select_sample(i, true, cx);
                                    ws.begin_drag(
                                        Drag::Sample {
                                            index: i,
                                            from: p,
                                            at: p,
                                            moved: false,
                                        },
                                        cx,
                                    );
                                }
                                // Empty space: draw a box round the ones to keep.
                                None => ws.begin_drag(Drag::MapBox { from: p, to: p }, cx),
                            }
                        }),
                    )
                    .on_scroll_wheel(
                        cx.listener(|ws, e: &ScrollWheelEvent, _, cx| ws.scroll_map(e, cx)),
                    ),
            )
            // The legend, and the way back to the whole map.
            .child(
                div()
                    .h(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(12.))
                    .border_t_1()
                    .border_color(rgb(LINE_SOFT))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .children(legend)
                    .child(div().flex_1())
                    .child(
                        button("Fit", false)
                            .h(px(24.))
                            .debug_selector(|| "map-fit".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    ws.browser.zoom = 1.0;
                                    ws.browser.center = [0.0, 0.0];
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(self.info_line())
    }

    fn kit_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = &self.browser;
        let mut seed: Vec<String> = Vec::new();
        if let Some(k) = self.sample_key() {
            seed.push(k.name());
        }
        if let Some(bpm) = self.sample_bpm() {
            seed.push(format!("{bpm:.0} BPM"));
        }
        if !b.text.is_empty() {
            seed.push(format!("“{}”", b.text));
        }
        seed.extend(b.traits.iter().cloned());
        let cards: Vec<Div> = b
            .kit
            .iter()
            .enumerate()
            .map(|(n, k)| self.kit_card(n, k, cx))
            .collect();
        let header = div()
            .h(px(44.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .child("A kit that goes together"),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .min_w(px(0.))
                    .overflow_hidden()
                    .children(match seed.is_empty() {
                        true => vec![chip("from everything", TEXT_3)],
                        false => seed.into_iter().map(|s| chip(s, ACCENT_TEXT)).collect(),
                    }),
            )
            .child(div().flex_1())
            .child(
                button("New picks", false)
                    .child(icon(Icon::Shuffle, 13.).text_color(rgb(TEXT_3)))
                    .flex_row_reverse()
                    .debug_selector(|| "shuffle-kit".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.shuffle_kit(cx)),
                    ),
            )
            .child(
                primary("Make kit")
                    .h(px(28.))
                    .debug_selector(|| "make-kit".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.make_kit(cx)),
                    ),
            );
        let empty = cards.is_empty();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("kit")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(12.))
                    .when(empty, |d| {
                        d.flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(TEXT_3))
                            .child("Nothing to build a kit from yet.")
                    })
                    .when(!empty, |d| {
                        d.child(div().grid().grid_cols(3).gap(px(8.)).children(cards))
                    }),
            )
            .child(self.info_line())
    }

    fn kit_card(&self, n: usize, k: &KitPick, cx: &mut Context<Self>) -> Div {
        let b = &self.browser;
        let role = k.slot.role;
        let color = category_color(role);
        let options = k.options().len();
        let hit = k.hit().cloned();
        let s = hit.as_ref().and_then(|h| b.sample(h.index));
        let selected = hit.as_ref().is_some_and(|h| b.selected == Some(h.index));
        let playing = hit
            .as_ref()
            .is_some_and(|h| b.audition.is_some_and(|a| a.0 == h.index));
        let step = |i: Icon, by: i32, cx: &mut Context<Self>| {
            div()
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(RAISED)))
                .child(icon(i, 13.).text_color(rgb(TEXT_2)))
                .debug_selector(move || format!("kit-{n}-{by}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        let Some(k) = ws.browser.kit.get_mut(n) else {
                            return;
                        };
                        let len = k.options().len().max(1) as i32;
                        k.choice = (k.choice as i32 + by).rem_euclid(len) as usize;
                        if let Some(i) = k.hit().map(|h| h.index) {
                            ws.select_sample(i, true, cx);
                        }
                    }),
                )
        };
        let index = hit.as_ref().map(|h| h.index);
        div()
            .h(px(104.))
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .rounded(px(10.))
            .bg(rgb(CARD))
            .border_1()
            .border_color(if selected {
                alpha(color, 0.8)
            } else {
                rgb(LINE).into()
            })
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(LINE_STRONG)))
            .debug_selector(move || format!("kit-{n}"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(div().size(px(8.)).rounded(px(2.)).bg(rgb(color)))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(role.name()),
                    )
                    .when(options > 1, |d| {
                        d.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(10.))
                                .text_color(rgb(TEXT_3))
                                .child(format!("{}/{options}", k.choice + 1)),
                        )
                    })
                    .child(
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .bg(if k.locked {
                                alpha(ACCENT, 0.2)
                            } else {
                                gpui::transparent_black()
                            })
                            .hover(|s| s.bg(rgb(RAISED)))
                            .debug_selector(move || format!("kit-lock-{n}"))
                            .child(
                                icon(if k.locked { Icon::Lock } else { Icon::Unlock }, 13.)
                                    .text_color(rgb(if k.locked { ACCENT_TEXT } else { TEXT_3 })),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(k) = ws.browser.kit.get_mut(n) {
                                        k.locked = !k.locked;
                                    }
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(match s {
                Some(s) => div()
                    .h(px(24.))
                    .child(wave(s.analysis.waveform.clone(), color, 0.85))
                    .into_any_element(),
                None => div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_4))
                    .child(format!(
                        "No {} fits these filters",
                        role.name().to_lowercase()
                    ))
                    .into_any_element(),
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_2))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(s.map_or(String::new(), |s| match tone_text(&s.analysis) {
                                Some(t) => format!("{} · {t}", s.name),
                                None => s.name.clone(),
                            })),
                    )
                    .when(options > 1, |d| {
                        d.child(step(Icon::Back, -1, cx))
                            .child(step(Icon::Forward, 1, cx))
                    })
                    .when_some(index, |d, i| {
                        d.child(
                            div()
                                .size(px(22.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(11.))
                                .bg(rgb(if playing { ACCENT } else { RAISED }))
                                .cursor_pointer()
                                .debug_selector(move || format!("kit-play-{n}"))
                                .child(icon(Icon::Play, 11.).text_color(rgb(TEXT)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.audition_sample(i, cx);
                                    }),
                                ),
                        )
                    }),
            )
            .when_some(index, |d, i| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        ws.select_sample(i, true, cx);
                        let at = pt(e.position);
                        ws.begin_drag(
                            Drag::Sample {
                                index: i,
                                from: at,
                                at,
                                moved: false,
                            },
                            cx,
                        );
                    }),
                )
            })
    }

    /// The right column: the selected sample, what it is, and what sounds like it.
    fn details(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = &self.browser;
        let column = div()
            .id("sample-details")
            .w(px(DETAILS_W))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE));
        let Some((index, s)) = b.selected.and_then(|i| Some((i, b.sample(i)?))) else {
            return column.child(
                div()
                    .size_full()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .text_center()
                    .child(icon(Icon::Wave, 22.).text_color(rgb(TEXT_4)))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_2))
                            .child("Pick a sound"),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child("Its details show here: what it is, its note or key, tempo and character, and what sounds like it."),
                    ),
            );
        };
        let a = &s.analysis;
        let color = category_color(a.category);
        let heard = b
            .audition
            .filter(|x| x.0 == index)
            .map(|(_, since, secs)| (since.elapsed().as_secs_f32() / secs.max(0.01)).min(1.0));
        let points = a.waveform.clone();
        let big = canvas(
            |_, _, _| {},
            move |bnd, _, window, _| paint_bars(&points, bnd, color, heard, window),
        )
        .size_full();
        let playing = heard.is_some();
        let semis = self.sample_transpose(index);
        // The facts: what it is, its note or key, tempo, length, level and shape.
        let mut facts = vec![fact(
            "Category",
            format!("{} · {:.0}%", a.category.name(), a.confidence * 100.0),
        )];
        if let Some(k) = &a.key {
            facts.push(fact("Key", k.name.clone()));
        } else if let Some(p) = &a.pitch {
            facts.push(fact("Note", note_text(p)));
        }
        if let Some(bpm) = a.bpm {
            facts.push(fact(
                "Tempo",
                match a.bars {
                    Some(bars) => format!(
                        "{bpm:.0} BPM · {bars:.0} bar{}",
                        if bars == 1.0 { "" } else { "s" }
                    ),
                    None => format!("{bpm:.0} BPM"),
                },
            ));
        }
        facts.push(fact("Length", seconds_text(a.seconds)));
        if (-70.0..0.0).contains(&a.loudness) && a.loudness > -69.9 {
            facts.push(fact("Loudness", format!("{} LUFS", db_text(a.loudness))));
        }
        facts.push(fact("Peak", format!("{} dB", db_text(a.peak_db))));
        if a.envelope.attack_ms > 0.0 || a.envelope.decay_ms > 0.0 {
            facts.push(fact(
                "Attack · decay",
                format!(
                    "{:.0} · {:.0} ms",
                    a.envelope.attack_ms, a.envelope.decay_ms
                ),
            ));
        }
        if a.spectrum.centroid_hz > 0.0 {
            let hz = a.spectrum.centroid_hz;
            facts.push(fact(
                "Brightness",
                match hz >= 1000.0 {
                    true => format!("{:.1} kHz", hz / 1000.0),
                    false => format!("{hz:.0} Hz"),
                },
            ));
        }
        facts.push(fact(
            "Stereo",
            match a.channels {
                1 => "Mono".to_string(),
                _ if a.width < 0.05 => "Mono (2 ch)".to_string(),
                _ => format!("Wide {:.0}%", a.width * 100.0),
            },
        ));
        // Where its energy is, band by band.
        let bands = a.spectrum.bands_db;
        let has_bands = bands.iter().any(|&d| d != 0.0);
        let spectrum = div().flex().gap(px(6.)).items_end().children(
            ["Sub", "Bass", "Low", "Mid", "Pres", "Air"]
                .into_iter()
                .zip(bands)
                .map(|(name, db)| {
                    let v = ((db + 48.0) / 48.0).clamp(0.04, 1.0);
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.))
                        .child(
                            div()
                                .w_full()
                                .h(px(34.))
                                .flex()
                                .flex_col()
                                .justify_end()
                                .child(
                                    div()
                                        .w_full()
                                        .h(relative(v))
                                        .rounded(px(2.))
                                        .bg(alpha(color, 0.75)),
                                ),
                        )
                        .child(div().text_size(px(9.5)).text_color(rgb(TEXT_3)).child(name))
                }),
        );
        let reasons = b.hit(index).map(|h| h.reasons.clone()).unwrap_or_default();
        let similar: Vec<Div> = b
            .similar
            .iter()
            .enumerate()
            .filter_map(|(n, h)| {
                let s = b.sample(h.index)?;
                let c = category_color(s.analysis.category);
                let i = h.index;
                Some(
                    div()
                        .h(px(30.))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(OFF)))
                        .debug_selector(move || format!("similar-{n}"))
                        .child(div().w(px(44.)).h(px(16.)).flex_none().child(wave(
                            s.analysis.waveform.clone(),
                            c,
                            0.8,
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_2))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        )
                        .child(
                            div()
                                .font_family(MONO)
                                .text_size(px(10.))
                                .text_color(rgb(TEXT_3))
                                .child(format!("{:.0}%", h.score.clamp(0.0, 1.0) * 100.0)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                ws.select_sample(i, true, cx);
                                let at = pt(e.position);
                                ws.begin_drag(
                                    Drag::Sample {
                                        index: i,
                                        from: at,
                                        at,
                                        moved: false,
                                    },
                                    cx,
                                );
                            }),
                        ),
                )
            })
            .collect();
        let sliders: Vec<Div> =
            (0..ASPECTS.len())
                .map(|i| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child(ASPECTS[i])
                                .child(div().font_family(MONO).child(format!(
                                    "{:.0}",
                                    *aspect(&mut { b.aspects }, i) * 100.0
                                ))),
                        )
                        .child(
                            self.slider_bar(Slider::Aspect(i), cx)
                                .debug_selector(move || format!("aspect-{i}")),
                        )
                })
                .collect();
        let into = match a.kind {
            Kind::Loop => "As audio, in time with the song",
            Kind::OneShot => "On a sampler, in tune",
        };
        column.child(
            div()
                .p(px(14.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(match s.folder.is_empty() {
                                    true => s
                                        .path
                                        .parent()
                                        .map_or(String::new(), |p| p.display().to_string()),
                                    false => s.folder.clone(),
                                }),
                        ),
                )
                .child(
                    div()
                        .h(px(64.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .size(px(36.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(18.))
                                .bg(rgb(if playing { ACCENT } else { TEXT }))
                                .cursor_pointer()
                                .debug_selector(|| "audition".into())
                                .child(
                                    icon(if playing { Icon::Stop } else { Icon::Play }, 16.)
                                        .text_color(rgb(if playing { 0xffffff } else { BG })),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| match playing {
                                        true => ws.stop_audition(cx),
                                        false => ws.audition_sample(index, cx),
                                    }),
                                ),
                        )
                        .child(div().flex_1().h_full().child(big)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .flex_wrap()
                        .child(chip(a.category.name(), color))
                        .child(chip(kind_name(a.kind), TEXT_3))
                        .when(semis.abs() >= 0.05, |d| {
                            d.child(chip(format!("Plays {} st", semis_text(semis)), ACCENT_TEXT))
                        })
                        .children(a.traits.iter().map(|t| chip(t.clone(), TEXT_2))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            primary("Add to song")
                                .w_full()
                                .child(icon(Icon::Plus, 14.).text_color(rgb(0xffffff)))
                                .flex_row_reverse()
                                .debug_selector(|| "add-to-song".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.add_sample(index, cx)),
                                ),
                        )
                        .child(
                            self.give_button(s.path.clone(), 30., cx)
                                .w_full()
                                .justify_center(),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .text_center()
                                .child(format!("{into} · or drag it onto a track")),
                        ),
                )
                .when(!reasons.is_empty(), |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(heading("Why it's here"))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(4.))
                                    .children(reasons.into_iter().map(|r| chip(r, ACCENT_TEXT))),
                            ),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(heading("Sound"))
                        .child(
                            div()
                                .grid()
                                .grid_cols(2)
                                .gap_x(px(12.))
                                .gap_y(px(10.))
                                .children(facts),
                        ),
                )
                .when(has_bands, |d| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(heading("Spectrum"))
                            .child(spectrum),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(heading("Similar"))
                        .child(
                            div()
                                .grid()
                                .grid_cols(2)
                                .gap_x(px(14.))
                                .gap_y(px(6.))
                                .children(sliders),
                        )
                        .child(match similar.is_empty() {
                            true => div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_4))
                                .child("Nothing else sounds like it yet."),
                            false => div().mt(px(4.)).flex().flex_col().children(similar),
                        }),
                ),
        )
    }

    /// The sample being dragged, by the pointer, saying where it would go.
    pub fn sample_ghost(&self) -> Option<impl IntoElement + use<>> {
        let Some(Drag::Sample {
            index,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let s = self.browser.sample(index)?;
        let color = category_color(s.analysis.category);
        let target = self.drop_target(at);
        let (hint, ok) = match target {
            Some(Drop::Audio(t, _)) => (
                format!("Audio on {}", self.session.project.tracks[t].name),
                true,
            ),
            Some(Drop::Zone(t, _)) => (
                format!("A pad on {}", self.session.project.tracks[t].name),
                true,
            ),
            Some(Drop::New(_)) => ("Audio on a new track".to_string(), true),
            Some(Drop::Under(t, _)) => (
                format!(
                    "Audio on a new track under {}",
                    self.session.project.tracks[t].name
                ),
                true,
            ),
            None => ("Drop it on a track".to_string(), true),
        };
        Some(
            div()
                .absolute()
                .left(px(at.x + 14.))
                .top(px(at.y + 10.))
                .w(px(230.))
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .rounded(px(9.))
                .bg(rgb(0x1c1c22))
                .border_1()
                .border_color(alpha(color, 0.7))
                .shadow_lg()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().w(px(56.)).h(px(20.)).flex_none().child(wave(
                            s.analysis.waveform.clone(),
                            color,
                            0.9,
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(s.name.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(if ok { TEXT_3 } else { RECORD }))
                        .child(hint),
                ),
        )
    }

    /// In the lanes, while a sample is dragged over them: where it would land.
    pub fn sample_drop_view(&self) -> Option<Div> {
        let Some(Drag::Sample {
            index,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let s = self.browser.sample(index)?;
        let ppt = self.px_per_tick();
        let color = category_color(s.analysis.category);
        let lane = |row: usize| {
            div()
                .absolute()
                .left_0()
                .top(px(self.row_top(row) + 2.))
                .w_full()
                .h(px(self.lane_h(row) - 4.))
                .rounded(px(6.))
        };
        let p = &self.session.project;
        // An audio region's outline, as long as it will be in the song.
        let region = |row: usize, tick: u64| {
            let length = loop_ticks(&s.analysis, p.tempo).unwrap_or_else(|| {
                p.ticks((s.analysis.seconds as f64 * p.sample_rate as f64) as u64)
                    .max(1)
            });
            div()
                .absolute()
                .left(px(tick as f32 * ppt - self.scroll_x))
                .top(px(self.row_top(row) + 4.))
                .w(px((length as f32 * ppt).max(6.)))
                .h(px(self.lane_h(row) - 8.))
                .rounded(px(6.))
                .bg(alpha(color, 0.18))
                .border_1()
                .border_color(alpha(color, 0.9))
        };
        Some(match self.drop_target(at)? {
            Drop::Audio(t, tick) => region(t, tick),
            Drop::New(tick) => region(p.tracks.len(), tick),
            // A new track goes in under it: the region shows on the line between.
            Drop::Under(t, tick) => region(t, tick).top(px(self.row_top(t + 1) - 6.)).h(px(12.)),
            Drop::Zone(t, _) => lane(t)
                .bg(alpha(color, 0.1))
                .border_1()
                .border_color(alpha(color, 0.8)),
        })
    }

    /// In the lanes, while files are dragged in from the Finder: where they'd land.
    pub fn file_drop_view(&self, cx: &App) -> Option<Div> {
        let at = self.file_drag.as_ref().filter(|_| cx.has_active_drag())?.0;
        let lane = |row: usize| {
            div()
                .absolute()
                .left_0()
                .top(px(self.row_top(row) + 2.))
                .w_full()
                .h(px(self.lane_h(row) - 4.))
                .rounded(px(6.))
                .bg(alpha(ACCENT, 0.08))
                .border_1()
                .border_color(alpha(ACCENT, 0.7))
        };
        let tick = |t: u64| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(t as f32 * self.px_per_tick() - self.scroll_x))
                .w(px(2.))
                .bg(rgb(ACCENT))
        };
        Some(match self.drop_target(at)? {
            Drop::Audio(t, at) => lane(t).child(tick(at)),
            Drop::New(at) => lane(self.session.project.tracks.len()).child(tick(at)),
            Drop::Zone(t, at) | Drop::Under(t, at) => lane(t + 1)
                .h(px(4.))
                .top(px(self.row_top(t + 1) - 2.))
                .child(tick(at)),
        })
    }
}
