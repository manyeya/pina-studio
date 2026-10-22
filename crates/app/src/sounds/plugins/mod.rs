//! The Plugins page: the Browser's instruments and effects as a page of its own, as the sample
//! library has one. It opens from the Browser's Expand (or ⌃⌥D) and goes back with Compact, the
//! Browser's search and tab coming with it either way.
//!
//! On the left what to look for: the search, all, instruments or effects, the starred ones,
//! where from, the formats and the order; then the categories (and, with one picked, the kinds in
//! it: a compressor's opto or FET), the makers, your collections, what Health found (what won't
//! load, needs Rosetta, is installed twice or is in no song) and the library itself (its size,
//! what could go, the pictures). In the middle what's found, as pictures of each plugin's own
//! window, as a list, or on the keyboard to put plugins on keys. On the right the one picked: its
//! picture, what it is, Add to a track, its files, key and collections.

mod cards;
mod details;
mod facts;
mod filters;
mod health;
mod index;
mod map;
mod mini;
mod page;

pub use facts::*;
pub use index::*;
pub use map::*;
pub use mini::Mini;
pub use page::PluginOp;

use std::{cell::Cell, collections::HashMap, path::PathBuf, rc::Rc};

use gpui::{Context, Entity, Subscription, UniformListScrollHandle, prelude::*};
use pina_engine::song::project::{Format, Preset};
use serde::{Deserialize, Serialize};

use crate::{
    sounds::catalog::{Cat, Device, EFFECT_CATS, INSTRUMENT_CATS, Scope},
    ui::text::TextInput,
    workspace::Workspace,
};

/// What the middle shows: pictures, a list, or the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Shown {
    #[default]
    Grid,
    List,
    Keys,
}

/// Where what's shown comes from: plugins and Pina Studio's own, plugins only, Pina Studio's
/// own (built in), or saved presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    All,
    Plugins,
    Stock,
    Saved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Name,
    Used,
    Cpu,
}

/// What Health finds about a plugin, a filter of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthSet {
    WontLoad,
    NeedsRosetta,
    Copies,
    NeverInASong,
    InSongs,
}

impl HealthSet {
    pub const ALL: [HealthSet; 5] = [
        HealthSet::WontLoad,
        HealthSet::NeedsRosetta,
        HealthSet::Copies,
        HealthSet::NeverInASong,
        HealthSet::InSongs,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HealthSet::WontLoad => "Won't load",
            HealthSet::NeedsRosetta => "Needs Rosetta",
            HealthSet::Copies => "In two formats",
            HealthSet::NeverInASong => "In no song",
            HealthSet::InSongs => "In your songs",
        }
    }

    pub fn color(self) -> u32 {
        use crate::ui::theme::*;
        match self {
            HealthSet::WontLoad => RECORD,
            HealthSet::NeedsRosetta => AMBER,
            HealthSet::Copies => TEAL,
            HealthSet::NeverInASong => SLATE,
            HealthSet::InSongs => MINT,
        }
    }
}

/// A collection of the user's: a name, a colour, the devices in it (by key).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Collection {
    pub name: String,
    pub color: u32,
    #[serde(default)]
    pub devices: Vec<String>,
}

/// The colours a collection can be.
pub const COLLECTION_COLORS: [u32; 8] = [
    0xff9f43, 0x5b8cff, 0x3ddc97, 0xf472b6, 0xa78bfa, 0xffc53d, 0x38bdf8, 0xff6b6b,
];

/// A plugin shown: a device, or one of its presets.
#[derive(Clone, Debug, PartialEq)]
pub enum Card {
    Device(usize),
    Preset(usize, Preset),
}

impl Card {
    pub fn device(&self) -> usize {
        match self {
            Card::Device(d) | Card::Preset(d, _) => *d,
        }
    }
}

/// The page's state: what's picked to filter by and how it's ordered, the one picked, the facts
/// about each plugin, and the pictures being taken.
pub struct PluginPage {
    pub shown: Shown,
    pub source: Source,
    pub starred: bool,
    /// Formats left out.
    pub formats_off: Vec<Format>,
    pub cats: Vec<Cat>,
    pub kinds: Vec<&'static str>,
    pub makers: Vec<String>,
    /// Every maker listed, not just the biggest.
    pub all_makers: bool,
    pub collection: Option<usize>,
    pub set: Option<HealthSet>,
    pub sort: Sort,
    pub descending: bool,
    /// The plugin picked, by its device's key, and its preset if it's one; the one under the
    /// pointer.
    pub picked: Option<String>,
    pub preset: Option<Preset>,
    pub hovered: Option<String>,
    /// A key picked on the keyboard, waiting for a plugin.
    pub armed: Option<String>,
    /// What's shown, in order, as last drawn.
    pub cards: Vec<Card>,
    pub list: UniformListScrollHandle,
    /// The search field has the keyboard; it's to have it next frame.
    pub searching: bool,
    pub focus_search: bool,
    /// What's known of each device (its picture, its load), by key; and of each plugin file.
    pub facts: HashMap<String, Fact>,
    pub files: HashMap<PathBuf, FileFacts>,
    /// What the songs use, once looked through.
    pub songs: Option<SongsUse>,
    /// Pictures being taken.
    pub indexing: Option<Indexing>,
    /// How wide the results were drawn last, to lay the pictures out.
    pub width: Rc<Cell<f32>>,
    /// The picker a mixer slot opened, and its search; one asked for from a menu (which has
    /// no window to focus it in) next frame.
    pub mini: Option<Mini>,
    pub mini_search: Entity<TextInput>,
    pub mini_pending: Option<(
        crate::mix::strip::Channel,
        pina_engine::song::project::Which,
        gpui::Point<gpui::Pixels>,
    )>,
    /// A name to ask for next frame (a collection's), from a menu.
    pub ask: Option<crate::workspace::Name>,
    pub(crate) _subs: Vec<Subscription>,
}

impl PluginPage {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let mini_search = cx.new(|cx| TextInput::search("Search plugins", cx));
        let subs = vec![
            cx.observe(&mini_search, |ws, _, cx| {
                if let Some(m) = ws.plugin_page.mini.as_mut() {
                    m.picked = 0;
                }
                cx.notify()
            }),
            // Enter in the picker's search puts the picked one in.
            cx.subscribe(
                &mini_search,
                |ws, _, e: &crate::ui::text::TextEvent, cx| match e {
                    crate::ui::text::TextEvent::Confirm(_) => ws.mini_enter(cx),
                    crate::ui::text::TextEvent::Cancel => {
                        ws.plugin_page.mini = None;
                        ws.refocus = true;
                        cx.notify();
                    }
                },
            ),
        ];
        PluginPage {
            shown: Shown::Grid,
            source: Source::All,
            starred: false,
            formats_off: Vec::new(),
            cats: Vec::new(),
            kinds: Vec::new(),
            makers: Vec::new(),
            all_makers: false,
            collection: None,
            set: None,
            sort: Sort::Name,
            descending: false,
            picked: None,
            preset: None,
            hovered: None,
            armed: None,
            cards: Vec::new(),
            list: UniformListScrollHandle::new(),
            searching: false,
            focus_search: false,
            facts: facts::load_facts(),
            files: HashMap::new(),
            songs: None,
            indexing: None,
            width: Rc::new(Cell::new(0.0)),
            mini: None,
            mini_search,
            mini_pending: None,
            ask: None,
            _subs: subs,
        }
    }
}

/// The categories a scope shows.
pub fn scope_cats(scope: Scope) -> Vec<Cat> {
    match scope {
        Scope::Instruments => INSTRUMENT_CATS.to_vec(),
        Scope::Effects => EFFECT_CATS.to_vec(),
        _ => INSTRUMENT_CATS
            .iter()
            .chain(&EFFECT_CATS)
            .copied()
            .collect(),
    }
}

/// What a device is, within its category: a compressor's kind (an opto, a FET), a reverb's
/// (a hall, a plate), a synth's (FM, wavetable), from its name and what it says it is.
pub fn subcategory(d: &Device) -> Option<&'static str> {
    let said: String = d
        .formats
        .iter()
        .flat_map(|p| p.categories.iter())
        .map(|c| c.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let codes: String = d
        .formats
        .iter()
        .filter_map(|p| p.uid.as_deref())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let name = d.name.to_lowercase();
    let text = format!("{name} {said} {codes}");
    let has = |words: &[&str]| words.iter().any(|w| text.contains(w));
    let table: &[(&'static str, &[&str])] = match d.cat {
        Cat::Dynamics => &[
            ("Limiter", &["limit", "maximiz", "lmtr", "clipper"]),
            ("Multiband", &["multiband", "multi-band", "mcmp"]),
            ("De-esser", &["de-ess", "deess", "sibil"]),
            ("Transient", &["transient", "punch", "envelope shaper"]),
            ("Gate", &["gate", "expander"]),
            ("Rider", &["rider", "leveler", "leveller"]),
            ("Opto", &["opto", "optical", "la-2", "la2", "tla"]),
            ("FET", &["fet", "1176", "76"]),
            (
                "Vari-Mu",
                &[
                    "vari-mu",
                    "varimu",
                    "variable mu",
                    "tube comp",
                    "670",
                    "fairchild",
                ],
            ),
            ("VCA", &["vca", "bus comp", "ssl", "dbx", "2500"]),
            ("Compressor", &["comp", "dcmp", "dynamics"]),
        ],
        Cat::Eq => &[
            ("Dynamic EQ", &["dynamic eq", "dynamic-eq"]),
            ("Graphic", &["graphic", "greq"]),
            ("Shelf", &["shelf", "hshf", "lshf", "tilt"]),
            (
                "Vintage",
                &[
                    "vintage",
                    "passive",
                    "tube eq",
                    "console eq",
                    "pultec",
                    "eqp",
                    "1073",
                    "api",
                    "neve",
                    "550",
                ],
            ),
            ("Parametric", &["param", "pmeq", "nbeq", "eq", "equaliz"]),
        ],
        Cat::Reverb => &[
            ("Plate", &["plate"]),
            ("Hall", &["hall"]),
            ("Room", &["room"]),
            ("Spring", &["spring"]),
            ("Chamber", &["chamber"]),
            ("Shimmer", &["shimmer"]),
            ("Convolution", &["convol", "impulse", " ir "]),
            ("Early Reflections", &["early"]),
            ("Algorithmic", &["reverb", "verb", "rvb", "mrev"]),
        ],
        Cat::Delay => &[
            ("Tape", &["tape", "echoplex", "space echo"]),
            ("Ping-Pong", &["ping"]),
            ("Multi-tap", &["multi-tap", "multitap", "tap"]),
            ("Sample Delay", &["sample delay", "sdly"]),
            ("Echo", &["delay", "echo", "dely"]),
        ],
        Cat::Distortion => &[
            ("Bitcrush", &["bit", "crush", "decimat", "lofi", "lo-fi"]),
            ("Fuzz", &["fuzz"]),
            ("Amp", &["amp", "guitar"]),
            ("Cabinet", &["cab"]),
            ("Tube", &["tube", "valve"]),
            ("Tape", &["tape"]),
            ("Console", &["console", "channel strip"]),
            ("Saturation", &["satur", "warm", "drive"]),
            ("Distortion", &["dist"]),
        ],
        Cat::Modulation => &[
            ("Chorus", &["chorus", "ensemble"]),
            ("Flanger", &["flang"]),
            ("Phaser", &["phas"]),
            ("Tremolo", &["tremolo", "tremor"]),
            ("Rotary", &["rotary", "rotor", "leslie"]),
            ("Ring Mod", &["ring"]),
            ("Vibrato", &["vibrato"]),
            ("Frequency Shifter", &["frequency-shift", "freq shift"]),
        ],
        Cat::Filter => &[
            ("Low-pass", &["lowpass", "low-pass", "low pass", "lpas"]),
            (
                "High-pass",
                &["hipass", "highpass", "high-pass", "high pass", "hpas"],
            ),
            ("Band-pass", &["bandpass", "band-pass", "bpas"]),
            ("Formant", &["formant", "vowel"]),
            ("Multi-mode", &["filter", "filt"]),
        ],
        Cat::Pitch => &[
            ("Tuning", &["tune", "tuner"]),
            ("Harmonizer", &["harmon", "chord"]),
            ("Pitch Shift", &["pitch", "shift", "tmpt", "nutp", "octav"]),
        ],
        Cat::Space => &[
            ("Imager", &["imager", "width", "wide", "stereo"]),
            ("Panner", &["pan", "binaural", "3d", "surround"]),
        ],
        Cat::Utility => &[
            ("Meter", &["meter", "analy", "scope", "spectr", "loudness"]),
            (
                "Restoration",
                &["isolation", "denois", "de-nois", "noise", "restor", "vois"],
            ),
            ("Gain", &["gain", "trim", "volume"]),
            (
                "Routing",
                &[
                    "netsend", "nsnd", "send", "split", "rogr", "roger", "aac", "raac",
                ],
            ),
        ],
        Cat::Synth => &[
            (
                "FM",
                &[
                    " fm",
                    "fm ",
                    "frequency modulation",
                    "dx7",
                    "dexed",
                    "operator",
                    "fm8",
                ],
            ),
            (
                "Wavetable",
                &[
                    "wavetable",
                    "serum",
                    "vital",
                    "pigments",
                    "massive",
                    "ana 2",
                    "phase plant",
                ],
            ),
            ("Granular", &["granular"]),
            ("Modular", &["modular"]),
            ("Additive", &["additive"]),
            (
                "Analog",
                &[
                    "analog",
                    "virtual analog",
                    "subtractive",
                    "va",
                    "sylenth",
                    "diva",
                    "repro",
                    "noisemaker",
                    "moog",
                    "juno",
                    "prophet",
                    "obxd",
                ],
            ),
            ("Hybrid", &["synth"]),
        ],
        Cat::Keys => &[
            ("Piano", &["piano"]),
            (
                "Electric Piano",
                &["electric", "e-piano", "ep", "rhodes", "wurli"],
            ),
            ("Organ", &["organ", "tonewheel", "b3", "hammond"]),
        ],
        Cat::Drums => &[
            ("Drum Machine", &["808", "909", "machine", "beat"]),
            ("Acoustic Kit", &["kit", "drum"]),
        ],
        Cat::Sampler => &[("Sampler", &["sampl", "rompler", "samp", "dls"])],
        Cat::Orchestral => &[
            ("Strings", &["string"]),
            ("Brass", &["brass", "horn"]),
            ("Choir", &["choir", "vocal"]),
        ],
        _ => &[],
    };
    table.iter().find(|(_, words)| has(words)).map(|(n, _)| *n)
}

impl Workspace {
    /// Whether a device is in the kind (the Browser's tab), source, formats and starred shown.
    fn plugin_kind_fits(&self, d: &Device) -> bool {
        let page = &self.plugin_page;
        let kind = match self.catalog.scope {
            Scope::Instruments => d.instrument(),
            Scope::Effects => !d.instrument(),
            _ => true,
        };
        let source = match page.source {
            Source::All => true,
            Source::Plugins | Source::Saved => d.own.is_none(),
            Source::Stock => d.own.is_some(),
        };
        let formats = d.own.is_some()
            || d.formats
                .iter()
                .any(|p| !page.formats_off.contains(&p.format));
        let starred = !page.starred || self.catalog.favourites.contains(&d.key);
        kind && source && formats && starred
    }

    /// Whether a device is in a set Health found.
    pub(crate) fn in_set(&self, d: &Device, set: HealthSet) -> bool {
        let page = &self.plugin_page;
        let fact = page.facts.get(&d.key);
        match set {
            HealthSet::InSongs => page
                .songs
                .as_ref()
                .is_some_and(|s| s.used.contains_key(&d.key)),
            HealthSet::NeverInASong => {
                d.own.is_none()
                    && page
                        .songs
                        .as_ref()
                        .is_some_and(|s| !s.used.contains_key(&d.key))
            }
            HealthSet::Copies => d.formats.len() > 1,
            HealthSet::WontLoad => fact.is_some_and(|f| f.failed.is_some()),
            HealthSet::NeedsRosetta => d
                .formats
                .iter()
                .any(|p| page.files.get(&p.path).is_some_and(|f| f.intel_only)),
        }
    }

    /// The devices before the chips narrow them: the kind, source, formats, starred and the
    /// search.
    pub(crate) fn plugin_pool(&self, cx: &gpui::App) -> Vec<usize> {
        let query = self.catalog_query(cx);
        self.catalog
            .devices
            .iter()
            .enumerate()
            .filter(|(_, d)| self.plugin_kind_fits(d))
            .filter(|(_, d)| query.is_empty() || crate::sounds::catalog::score(d, &query).is_some())
            .map(|(i, _)| i)
            .collect()
    }

    /// Whether a device passes the chips, all but `skip`'s (to count what each would find).
    pub(crate) fn plugin_chips_fit(&self, d: &Device, skip: Chips) -> bool {
        let page = &self.plugin_page;
        let collection = page
            .collection
            .and_then(|c| self.catalog.collections.get(c));
        (skip == Chips::Cats || page.cats.is_empty() || page.cats.contains(&d.cat))
            && (skip == Chips::Kinds
                || page.kinds.is_empty()
                || subcategory(d).is_some_and(|s| page.kinds.contains(&s)))
            && (skip == Chips::Makers || page.makers.is_empty() || page.makers.contains(&d.vendor))
            && (skip == Chips::Collection || collection.is_none_or(|c| c.devices.contains(&d.key)))
            && (skip == Chips::Health || page.set.is_none_or(|s| self.in_set(d, s)))
    }

    /// What's shown, filtered and in order.
    pub(crate) fn plugin_cards(&mut self, cx: &gpui::App) -> Vec<Card> {
        let query = self.catalog_query(cx);
        let page = &self.plugin_page;
        let devices = &self.catalog.devices;
        let mut list: Vec<usize> = self
            .plugin_pool(cx)
            .into_iter()
            .filter(|&d| self.plugin_chips_fit(&devices[d], Chips::None))
            .collect();
        let uses = &self.catalog.uses;
        let facts = &page.facts;
        let name = |d: usize| devices[d].name.to_lowercase();
        match page.sort {
            // What's typed: the best match first.
            Sort::Name if !query.is_empty() => list.sort_by_key(|&d| {
                (
                    std::cmp::Reverse(crate::sounds::catalog::score(&devices[d], &query)),
                    name(d),
                )
            }),
            Sort::Name => list.sort_by_key(|&d| name(d)),
            Sort::Used => list.sort_by(|&a, &b| {
                let u = |d: usize| uses.get(&devices[d].key).copied().unwrap_or(0);
                u(b).cmp(&u(a)).then(name(a).cmp(&name(b)))
            }),
            Sort::Cpu => list.sort_by(|&a, &b| {
                let l = |d: usize| {
                    facts
                        .get(&devices[d].key)
                        .and_then(|f| f.load)
                        .unwrap_or(f32::MAX)
                };
                l(a).total_cmp(&l(b)).then(name(a).cmp(&name(b)))
            }),
        }
        if page.descending {
            list.reverse();
        }
        if page.source != Source::Saved {
            return list.into_iter().map(Card::Device).collect();
        }
        let mut out = Vec::new();
        for d in list {
            let presets = self.catalog.presets(d).to_vec();
            out.extend(presets.into_iter().map(|p| Card::Preset(d, p)));
        }
        out
    }

    /// The device picked, if it's still there.
    pub(crate) fn picked_plugin(&self) -> Option<usize> {
        self.catalog.device(self.plugin_page.picked.as_deref()?)
    }

    /// The tracks this song has a device on, by name.
    pub(crate) fn tracks_with(&self, d: &Device) -> Vec<String> {
        use crate::sounds::catalog::Own;
        use pina_engine::song::project::Instrument;
        let is = |r: &pina_engine::song::project::PluginRef| {
            d.formats
                .iter()
                .any(|f| f.class == r.class && f.path == r.path)
        };
        self.session
            .project
            .tracks
            .iter()
            .filter(|t| match d.own {
                Some(Own::Eq) => t.eq.is_some(),
                Some(Own::Comp) => t.comp.is_some(),
                Some(Own::Sat) => t.sat.is_some(),
                Some(Own::Synth) => matches!(t.instrument, Some(Instrument::Synth)),
                Some(Own::Drums) => matches!(t.instrument, Some(Instrument::Drums)),
                Some(Own::Sampler) => matches!(t.instrument, Some(Instrument::Sampler(_))),
                None => {
                    matches!(&t.instrument, Some(Instrument::Plugin(r)) if is(r))
                        || t.inserts.iter().any(is)
                }
            })
            .map(|t| t.name.clone())
            .collect()
    }
}

/// A group of the page's chips, left out when counting what each of its chips would find.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chips {
    None,
    Cats,
    Kinds,
    Makers,
    Collection,
    Health,
}
