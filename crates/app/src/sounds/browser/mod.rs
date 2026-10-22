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

mod audition;
mod details;
mod filters;
mod finding;
mod kits;
mod library;
mod page;
mod panel;
mod placing;
mod results;
mod sound_map;
mod widgets;

pub use widgets::*;

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
    audio::stretch,
    music::Key,
    samples::{
        Analysis, Category, Kind, NOTES, Pitch, Sample, TRAITS,
        library::Library,
        note_name,
        place::{loop_ticks, place_zone, transpose_to},
        search::{self, Aspects, Hit, KitSeed, KitSlot, Query},
    },
    song::commands::{self, Command},
    song::project::{At, Content, Instrument, PPQ, Sampler},
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
    heard: HashMap<(PathBuf, u64, u32), Arc<pina_engine::audio::engine::AudioData>>,
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
