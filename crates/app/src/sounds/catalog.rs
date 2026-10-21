//! The Browser: everything that can go on a track, found in one search. Pina Studio's own instruments
//! and effects and every plugin installed, one entry each however many formats it comes in, in
//! the categories its plugin says it belongs to (VST3's subcategories, CLAP's features, an Audio
//! Unit's type, VST2's category; its name where those are silent); its preset files; the song's
//! samples; and the songs themselves. Favourites and what's used lately are remembered.

use std::{collections::HashMap, path::PathBuf, sync::OnceLock};

use gpui::{
    Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, Subscription, Svg, div,
    prelude::*, px, rgb, svg,
};
use pina_engine::{
    plugin::scan::PluginInfo,
    song::project::{Format, Instrument, Preset, Which},
};
use serde::{Deserialize, Serialize};

use crate::{
    mix::strip::{Builtin, Channel},
    ui::looks::{glyph, hsl},
    ui::menu::MenuAction,
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::{Drag, Workspace, pt},
};

/// What a device is for: effects by what they do to a sound, instruments by what they play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cat {
    Eq,
    Dynamics,
    Reverb,
    Delay,
    Distortion,
    Modulation,
    Filter,
    Pitch,
    Space,
    Utility,
    OtherFx,
    Synth,
    Sampler,
    Drums,
    Keys,
    Orchestral,
    OtherInstrument,
}

pub const EFFECT_CATS: [Cat; 10] = [
    Cat::Eq,
    Cat::Dynamics,
    Cat::Reverb,
    Cat::Delay,
    Cat::Distortion,
    Cat::Modulation,
    Cat::Filter,
    Cat::Pitch,
    Cat::Space,
    Cat::Utility,
];
pub const INSTRUMENT_CATS: [Cat; 5] = [
    Cat::Synth,
    Cat::Sampler,
    Cat::Drums,
    Cat::Keys,
    Cat::Orchestral,
];

impl Cat {
    pub fn name(self) -> &'static str {
        match self {
            Cat::Eq => "EQ",
            Cat::Dynamics => "Dynamics",
            Cat::Reverb => "Reverb",
            Cat::Delay => "Delay",
            Cat::Distortion => "Distortion",
            Cat::Modulation => "Modulation",
            Cat::Filter => "Filter",
            Cat::Pitch => "Pitch",
            Cat::Space => "Stereo & space",
            Cat::Utility => "Utility",
            Cat::OtherFx => "Effect",
            Cat::Synth => "Synths",
            Cat::Sampler => "Samplers",
            Cat::Drums => "Drums",
            Cat::Keys => "Keys",
            Cat::Orchestral => "Orchestral",
            Cat::OtherInstrument => "Instrument",
        }
    }

    pub fn instrument(self) -> bool {
        matches!(
            self,
            Cat::Synth
                | Cat::Sampler
                | Cat::Drums
                | Cat::Keys
                | Cat::Orchestral
                | Cat::OtherInstrument
        )
    }

    /// Words that mean it, in categories, names and searches: "echo" finds a delay, "space" a
    /// reverb, "glue" a compressor.
    fn words(self) -> &'static [&'static str] {
        match self {
            Cat::Eq => &[
                "eq",
                "equalizer",
                "equaliser",
                "tone",
                "shelf",
                "graphic",
                "parametric",
            ],
            Cat::Dynamics => &[
                "dynamics",
                "compressor",
                "compression",
                "comp",
                "limiter",
                "gate",
                "expander",
                "transient",
                "glue",
                "maximizer",
                "de-esser",
                "deesser",
            ],
            Cat::Reverb => &[
                "reverb", "verb", "room", "hall", "plate", "chamber", "space", "ambience", "early",
            ],
            Cat::Delay => &["delay", "echo", "tape echo", "repeat", "ping"],
            Cat::Distortion => &[
                "distortion",
                "saturation",
                "saturator",
                "overdrive",
                "drive",
                "fuzz",
                "amp",
                "bitcrush",
                "crusher",
                "lofi",
                "lo-fi",
                "warmth",
                "grit",
                "tape",
            ],
            Cat::Modulation => &[
                "modulation",
                "chorus",
                "flanger",
                "phaser",
                "tremolo",
                "vibrato",
                "rotary",
                "ensemble",
                "wobble",
            ],
            Cat::Filter => &[
                "filter",
                "lowpass",
                "highpass",
                "bandpass",
                "low pass",
                "high pass",
                "cutoff",
            ],
            Cat::Pitch => &[
                "pitch",
                "tuning",
                "tuner",
                "harmonizer",
                "pitch-shifter",
                "shifter",
                "autotune",
                "octave",
            ],
            Cat::Space => &[
                "stereo", "spatial", "surround", "imager", "panner", "width", "wide", "binaural",
            ],
            Cat::Utility => &[
                "utility",
                "tools",
                "analyzer",
                "analyser",
                "meter",
                "gain",
                "mastering",
                "restoration",
                "sound isolation",
                "network",
                "isolation",
            ],
            Cat::OtherFx => &["fx", "effect", "audio-effect"],
            Cat::Synth => &[
                "synth",
                "synthesizer",
                "synthesiser",
                "analog",
                "wavetable",
                "fm",
                "modular",
                "poly",
            ],
            Cat::Sampler => &["sampler", "rompler", "sample"],
            Cat::Drums => &["drum", "drums", "drum-machine", "beat", "percussion", "kit"],
            Cat::Keys => &[
                "piano",
                "keys",
                "organ",
                "electric piano",
                "rhodes",
                "keyboard",
            ],
            Cat::Orchestral => &[
                "strings",
                "orchestra",
                "orchestral",
                "ensemble strings",
                "brass",
                "choir",
            ],
            Cat::OtherInstrument => &["instrument", "generator"],
        }
    }

    /// Its colour in the Browser.
    pub fn color(self) -> u32 {
        let h = match self {
            Cat::Eq => 165.0,
            Cat::Dynamics => 42.0,
            Cat::Reverb => 232.0,
            Cat::Delay => 202.0,
            Cat::Distortion => 8.0,
            Cat::Modulation => 268.0,
            Cat::Filter => 186.0,
            Cat::Pitch => 318.0,
            Cat::Space => 92.0,
            Cat::Utility | Cat::OtherFx => 220.0,
            Cat::Synth => 202.0,
            Cat::Sampler => 30.0,
            Cat::Drums => 6.0,
            Cat::Keys => 163.0,
            Cat::Orchestral => 343.0,
            Cat::OtherInstrument => 256.0,
        };
        let s = if matches!(self, Cat::Utility | Cat::OtherFx) {
            0.22
        } else {
            0.78
        };
        hsl(h, s, 0.66)
    }

    /// Its icon: an instrument's as a track's, an effect's drawn for what it does.
    pub fn glyph(self, size: f32) -> Svg {
        let track = match self {
            Cat::Synth => Some("synth"),
            Cat::Sampler => Some("sampler"),
            Cat::Drums => Some("drums"),
            Cat::Keys => Some("keys"),
            Cat::Orchestral => Some("strings"),
            Cat::OtherInstrument => Some("midi"),
            Cat::OtherFx => Some("fx"),
            _ => None,
        };
        if let Some(id) = track {
            return glyph(id, size);
        }
        static CACHE: OnceLock<HashMap<&'static str, Vec<u8>>> = OnceLock::new();
        let shapes = CACHE.get_or_init(|| {
            [
                ("eq", r##"<path d="M3 16c2.6 0 3.4-8 6.5-8S12.6 17 15.6 17 18.4 12 21 12"/><path d="M3 20.5h18"/>"##),
                ("dynamics", r##"<path d="M3.5 3.5v17h17"/><path d="M3.5 20.5 12 12l8.5-4"/><path d="M12 12h.01"/>"##),
                ("reverb", r##"<path d="M2.5 19a9.5 9.5 0 0 1 19 0M6 19a6 6 0 0 1 12 0M9.5 19a2.5 2.5 0 0 1 5 0"/>"##),
                ("delay", r##"<circle cx="5" cy="12" r="2.6"/><circle cx="12" cy="12" r="2"/><circle cx="18" cy="12" r="1.4"/><path d="M21.5 12h.01"/>"##),
                ("distortion", r##"<path d="M2.5 12c1.2-4 2-5.5 3.5-5.5h3c1.3 0 1.6 11 3 11h3c1.3 0 1.7-5.5 3.2-5.5h3.3"/>"##),
                ("modulation", r##"<path d="M2.5 9c3-4.5 6.3 4.5 9.5 0s6.5-4.5 9.5 0M2.5 15c3 4.5 6.3-4.5 9.5 0s6.5 4.5 9.5 0"/>"##),
                ("filter", r##"<path d="M2.5 8h9c3.5 0 4.5 2.6 5.6 6.2L18.8 20"/><path d="M2.5 20.5h19"/>"##),
                ("pitch", r##"<path d="M7.5 20V4M3.5 8l4-4 4 4M16.5 4v16M12.5 16l4 4 4-4"/>"##),
                ("space", r##"<circle cx="8.8" cy="12" r="5.8"/><circle cx="15.2" cy="12" r="5.8"/>"##),
                ("utility", r##"<path d="M5 3.5v17M12 3.5v17M19 3.5v17"/><circle cx="5" cy="14" r="2"/><circle cx="12" cy="8" r="2"/><circle cx="19" cy="16" r="2"/>"##),
            ]
            .into_iter()
            .map(|(id, shape)| {
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">{shape}</svg>"##
                );
                (id, svg.into_bytes())
            })
            .collect()
        });
        let id = match self {
            Cat::Eq => "eq",
            Cat::Dynamics => "dynamics",
            Cat::Reverb => "reverb",
            Cat::Delay => "delay",
            Cat::Distortion => "distortion",
            Cat::Modulation => "modulation",
            Cat::Filter => "filter",
            Cat::Pitch => "pitch",
            Cat::Space => "space",
            _ => "utility",
        };
        svg().data(&shapes[id][..]).size(px(size)).flex_none()
    }
}

/// Pina Studio's own devices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Own {
    Synth,
    Drums,
    Sampler,
    Eq,
    Comp,
    Sat,
}

/// Something that can go on a track.
#[derive(Clone, Debug)]
pub struct Device {
    /// What favourites and recents keep: "own:eq", "plugin:surge synth team/surge xt".
    pub key: String,
    pub name: String,
    pub vendor: String,
    pub cat: Cat,
    pub own: Option<Own>,
    /// Its plugin in each format it's installed in, the one used first.
    pub formats: Vec<PluginInfo>,
    /// Lowercase words a search looks through.
    hay: String,
}

impl Device {
    pub fn instrument(&self) -> bool {
        self.cat.instrument()
    }

    /// What the formats are called, the one used first.
    pub fn badges(&self) -> Vec<&'static str> {
        self.formats
            .iter()
            .map(|p| match p.format {
                Format::Clap => "CLAP",
                Format::Vst3 => "VST3",
                Format::Au => "AU",
                Format::Vst2 => "VST2",
            })
            .collect()
    }
}

/// The formats, best first: CLAP, then VST3, Audio Units, VST2.
fn rank(f: Format) -> u8 {
    match f {
        Format::Clap => 0,
        Format::Vst3 => 1,
        Format::Au => 2,
        Format::Vst2 => 3,
    }
}

/// An Audio Unit's type and subtype codes, from its uid ("aufx dely appl").
fn au_codes(uid: Option<&str>) -> (String, String) {
    let mut parts = uid.unwrap_or_default().split(' ');
    (
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
    )
}

/// What a plugin is for: from what it says (its categories, or an Audio Unit's codes), else its
/// name.
pub fn categorize(p: &PluginInfo) -> Cat {
    let (kind, sub) = au_codes(p.uid.as_deref());
    if p.format == Format::Au {
        let by_code = match sub.as_str() {
            "dely" | "sdly" => Some(Cat::Delay),
            "rvb2" | "mrev" => Some(Cat::Reverb),
            "pmeq" | "nbeq" | "greq" | "hshf" | "lshf" => Some(Cat::Eq),
            "lpas" | "hpas" | "bpas" | "filt" => Some(Cat::Filter),
            "dcmp" | "mcmp" | "lmtr" => Some(Cat::Dynamics),
            "dist" => Some(Cat::Distortion),
            "tmpt" | "nutp" => Some(Cat::Pitch),
            "raac" | "nsnd" | "vois" | "rogr" => Some(Cat::Utility),
            "samp" => Some(Cat::Sampler),
            _ => None,
        };
        if let Some(c) = by_code {
            return c;
        }
        let _ = kind;
    }
    let said = p.categories.join(" ").to_lowercase();
    let name = format!("{} {}", p.class, p.vendor).to_lowercase();
    let has = |text: &str, c: Cat| {
        c.words().iter().any(|w| {
            text.split(|ch: char| !ch.is_alphanumeric() && ch != '-')
                .any(|t| t == *w)
                || (w.contains(' ') && text.contains(w))
        })
    };
    let order: &[Cat] = match p.instrument {
        true => &[
            Cat::Drums,
            Cat::Sampler,
            Cat::Keys,
            Cat::Orchestral,
            Cat::Synth,
        ],
        false => &[
            Cat::Reverb,
            Cat::Delay,
            Cat::Eq,
            Cat::Dynamics,
            Cat::Distortion,
            Cat::Modulation,
            Cat::Filter,
            Cat::Pitch,
            Cat::Space,
            Cat::Utility,
        ],
    };
    order
        .iter()
        .copied()
        .find(|c| has(&said, *c))
        .or_else(|| order.iter().copied().find(|c| has(&name, *c)))
        .unwrap_or(if p.instrument {
            Cat::Synth
        } else {
            Cat::OtherFx
        })
}

/// Every device: Pina Studio's own, then each plugin once, its formats together.
pub fn devices(plugins: &[PluginInfo]) -> Vec<Device> {
    let own = |key: &str, name: &str, cat: Cat, own: Own, about: &str| Device {
        key: format!("own:{key}"),
        name: name.into(),
        vendor: "Pina Studio".into(),
        cat,
        own: Some(own),
        formats: Vec::new(),
        hay: format!(
            "{name} pina studio built in {about} {}",
            cat.words().join(" ")
        )
        .to_lowercase(),
    };
    let mut out = vec![
        own("synth", "Sine Synth", Cat::Synth, Own::Synth, "sine simple"),
        own(
            "drums",
            "Drum Kit",
            Cat::Drums,
            Own::Drums,
            "808 kick snare hat",
        ),
        own(
            "sampler",
            "Sampler",
            Cat::Sampler,
            Own::Sampler,
            "samples zones pads",
        ),
        own("eq", "Channel EQ", Cat::Eq, Own::Eq, "low cut bell shelf"),
        own(
            "comp",
            "Compressor",
            Cat::Dynamics,
            Own::Comp,
            "soft knee glue",
        ),
        own(
            "sat",
            "Saturator",
            Cat::Distortion,
            Own::Sat,
            "drive tone warmth tape",
        ),
    ];
    let mut groups: Vec<(String, Vec<PluginInfo>)> = Vec::new();
    for p in plugins {
        let key = format!(
            "plugin:{}/{}",
            p.vendor.to_lowercase(),
            p.class.to_lowercase()
        );
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, g)) => g.push(p.clone()),
            None => groups.push((key, vec![p.clone()])),
        }
    }
    for (key, mut formats) in groups {
        formats.sort_by_key(|p| rank(p.format));
        let first = &formats[0];
        let cat = categorize(first);
        let hay = format!(
            "{} {} {} {} {}",
            first.class,
            first.vendor,
            cat.name(),
            cat.words().join(" "),
            first.categories.join(" ")
        )
        .to_lowercase();
        out.push(Device {
            key,
            name: first.class.clone(),
            vendor: first.vendor.clone(),
            cat,
            own: None,
            formats,
            hay,
        });
    }
    out
}

/// How well a device matches a search: every word somewhere, its name counting most. None if a
/// word is missing.
pub fn score(d: &Device, query: &str) -> Option<u32> {
    let name = d.name.to_lowercase();
    let mut total = 0;
    for w in query.split_whitespace() {
        total += match () {
            _ if name.starts_with(w) => 30,
            _ if name.split_whitespace().any(|t| t.starts_with(w)) => 20,
            _ if name.contains(w) => 12,
            _ if d.cat.words().iter().any(|c| c.starts_with(w)) => 10,
            _ if d.hay.contains(w) => 5,
            _ => return None,
        };
    }
    Some(total)
}

/// Where a device goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Track(usize),
    NewTrack,
}

/// What the Browser shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Instruments,
    Effects,
    /// The sample library.
    Sounds,
}

/// A line of results: a device, one of its presets, a sample, a song.
#[derive(Clone, Debug)]
pub enum Row {
    Heading(String),
    Device(usize),
    Preset(usize, Preset),
    Sample(usize),
    Song(crate::pages::projects::Song),
}

/// What's remembered: favourites and what's been used lately, newest first.
#[derive(Serialize, Deserialize, Default)]
struct Kept {
    #[serde(default)]
    favourites: Vec<String>,
    #[serde(default)]
    recent: Vec<String>,
}

fn kept_file() -> PathBuf {
    pina_engine::paths::support().join("browser.json")
}

pub struct Catalog {
    pub devices: Vec<Device>,
    pub search: Entity<TextInput>,
    pub scope: Scope,
    pub cat: Option<Cat>,
    /// The row picked with the keyboard, by its place in what's shown.
    pub picked: Option<usize>,
    /// The device whose presets are shown, and them, once looked for.
    pub open: Option<String>,
    presets: HashMap<String, Vec<Preset>>,
    pub favourites: Vec<String>,
    pub recent: Vec<String>,
    _typed: Subscription,
    _done: Subscription,
}

impl Catalog {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let search = cx.new(|cx| TextInput::search("Search devices, presets, sounds, songs", cx));
        let typed = cx.observe(&search, |ws, _, cx| {
            ws.catalog.picked = (!ws.catalog_query(cx).is_empty()).then_some(0);
            cx.notify();
        });
        // Enter puts the picked row in; Escape clears the search.
        let done = cx.subscribe(&search, |ws, input, e: &TextEvent, cx| match e {
            TextEvent::Confirm(_) => ws.use_picked(false, cx),
            TextEvent::Cancel => input.update(cx, |i, cx| i.set_text("", cx)),
        });
        let kept: Kept = match cfg!(test) {
            true => Kept::default(),
            false => std::fs::read(kept_file())
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default(),
        };
        Catalog {
            devices: devices(&[]),
            search,
            scope: Scope::All,
            cat: None,
            picked: None,
            open: None,
            presets: HashMap::new(),
            favourites: kept.favourites,
            recent: kept.recent,
            _typed: typed,
            _done: done,
        }
    }

    fn save(&self) {
        if cfg!(test) {
            return;
        }
        let kept = Kept {
            favourites: self.favourites.clone(),
            recent: self.recent.clone(),
        };
        let file = kept_file();
        let _ = std::fs::create_dir_all(file.parent().unwrap());
        let _ = std::fs::write(file, serde_json::to_vec_pretty(&kept).unwrap());
    }

    pub fn device(&self, key: &str) -> Option<usize> {
        self.devices.iter().position(|d| d.key == key)
    }

    /// A plugin's preset files, found the first time they're asked for.
    pub fn presets(&mut self, d: usize) -> &[Preset] {
        let device = &self.devices[d];
        self.presets.entry(device.key.clone()).or_insert_with(|| {
            let ext = |f: Format| match f {
                Format::Vst3 => "vstpreset",
                Format::Au => "aupreset",
                Format::Vst2 => "fxp",
                Format::Clap => "clap-preset",
            };
            let mut found = Vec::new();
            for p in &device.formats {
                let dirs = pina_engine::plugin::preset_dirs(&p.vendor, &p.class);
                found.extend(pina_engine::plugin::preset_files(&dirs, ext(p.format)));
            }
            found.dedup_by(|a, b| a.name == b.name);
            found
        })
    }
}

impl Workspace {
    /// The plugins came in from their scan: the Browser has them.
    pub fn catalog_plugins(&mut self) {
        let plugins: Vec<PluginInfo> = self.plugins.iter().flatten().cloned().collect();
        self.catalog.devices = devices(&plugins);
    }

    pub fn catalog_query(&self, cx: &gpui::App) -> String {
        self.catalog.search.read(cx).text().trim().to_lowercase()
    }

    /// The devices this song uses, by key.
    fn in_song(&self) -> Vec<String> {
        let p = &self.session.project;
        let mut keys = Vec::new();
        let plugin_key = |class: &str, path: &std::path::Path| {
            self.catalog
                .devices
                .iter()
                .find(|d| d.formats.iter().any(|f| f.class == class && f.path == path))
                .map(|d| d.key.clone())
        };
        for t in &p.tracks {
            match &t.instrument {
                Some(Instrument::Synth) => keys.push("own:synth".into()),
                Some(Instrument::Drums) => keys.push("own:drums".into()),
                Some(Instrument::Sampler(_)) => keys.push("own:sampler".into()),
                Some(Instrument::Plugin(r)) => keys.extend(plugin_key(&r.class, &r.path)),
                None => {}
            }
            if t.eq.is_some() {
                keys.push("own:eq".into());
            }
            if t.comp.is_some() {
                keys.push("own:comp".into());
            }
            if t.sat.is_some() {
                keys.push("own:sat".into());
            }
            for r in &t.inserts {
                keys.extend(plugin_key(&r.class, &r.path));
            }
        }
        for r in p
            .auxes
            .iter()
            .flat_map(|a| &a.inserts)
            .chain(&p.master_inserts)
        {
            keys.extend(plugin_key(&r.class, &r.path));
        }
        let mut seen = Vec::new();
        keys.retain(|k| {
            let new = !seen.contains(k);
            seen.push(k.clone());
            new
        });
        keys
    }

    /// The rows the Browser shows, as the search, scope and category say.
    pub fn catalog_rows(&mut self, cx: &gpui::App) -> Vec<Row> {
        let query = self.catalog_query(cx);
        let c = &self.catalog;
        let fits = |d: &Device| {
            (match c.scope {
                Scope::All | Scope::Sounds => true,
                Scope::Instruments => d.instrument(),
                Scope::Effects => !d.instrument(),
            }) && c.cat.is_none_or(|cat| d.cat == cat)
        };
        let mut rows = Vec::new();
        if !query.is_empty() {
            let mut found: Vec<(u32, usize)> = c
                .devices
                .iter()
                .enumerate()
                .filter(|(_, d)| fits(d))
                .filter_map(|(i, d)| {
                    let fav = if c.favourites.contains(&d.key) { 3 } else { 0 };
                    score(d, &query).map(|s| (s + fav, i))
                })
                .collect();
            found.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then(c.devices[a.1].name.cmp(&c.devices[b.1].name))
            });
            if !found.is_empty() {
                rows.push(Row::Heading(format!("Devices · {}", found.len())));
                rows.extend(found.into_iter().map(|(_, i)| Row::Device(i)));
            }
            // Sounds and songs that say it too, when the scope isn't only devices.
            if matches!(c.scope, Scope::All) && c.cat.is_none() {
                let sounds = self.catalog_samples(&query, 5);
                if !sounds.is_empty() {
                    rows.push(Row::Heading("Sounds".into()));
                    rows.extend(sounds.into_iter().map(Row::Sample));
                }
                let songs: Vec<crate::pages::projects::Song> = self
                    .songs
                    .iter()
                    .filter(|s| {
                        query
                            .split_whitespace()
                            .all(|w| s.name.to_lowercase().contains(w))
                    })
                    .take(4)
                    .cloned()
                    .collect();
                if !songs.is_empty() {
                    rows.push(Row::Heading("Songs".into()));
                    rows.extend(songs.into_iter().map(Row::Song));
                }
            }
            return rows;
        }
        let keyed = |keys: &[String]| -> Vec<usize> {
            keys.iter()
                .filter_map(|k| c.device(k))
                .filter(|&i| fits(&c.devices[i]))
                .collect()
        };
        let in_song = self.in_song();
        let section = |rows: &mut Vec<Row>, title: &str, list: Vec<usize>| {
            if !list.is_empty() {
                rows.push(Row::Heading(title.into()));
                rows.extend(list.into_iter().map(Row::Device));
            }
        };
        if c.cat.is_none() {
            section(&mut rows, "Favourites", keyed(&c.favourites));
            section(&mut rows, "In this song", keyed(&in_song));
            section(
                &mut rows,
                "Used lately",
                keyed(&c.recent[..c.recent.len().min(6)]),
            );
        }
        // Then every device, by category.
        let cats: Vec<Cat> = match c.cat {
            Some(cat) => vec![cat],
            None => INSTRUMENT_CATS
                .iter()
                .chain(&[Cat::OtherInstrument])
                .chain(EFFECT_CATS.iter())
                .chain(&[Cat::OtherFx])
                .copied()
                .collect(),
        };
        for cat in cats {
            let mut list: Vec<usize> = (0..c.devices.len())
                .filter(|&i| c.devices[i].cat == cat && fits(&c.devices[i]))
                .collect();
            list.sort_by_key(|&i| (c.devices[i].own.is_none(), c.devices[i].name.to_lowercase()));
            section(&mut rows, cat.name(), list);
        }
        // A plugin's presets, under it.
        if let Some(key) = c.open.clone()
            && let Some(d) = self.catalog.device(&key)
        {
            let presets: Vec<Preset> = self.catalog.presets(d).to_vec();
            let mut out = Vec::new();
            for r in rows {
                let here = matches!(r, Row::Device(i) if i == d);
                out.push(r);
                if here {
                    out.extend(presets.iter().cloned().map(|p| Row::Preset(d, p)));
                    // Only under its first place.
                    if presets.is_empty() {
                        out.push(Row::Heading("No preset files for it".into()));
                    }
                }
            }
            return out;
        }
        rows
    }

    /// Samples in the library that a search's words find.
    fn catalog_samples(&self, query: &str, limit: usize) -> Vec<usize> {
        let q = pina_engine::samples::search::Query {
            text: query.to_string(),
            limit,
            ..Default::default()
        };
        pina_engine::samples::search::find(&self.browser.library.samples, &q)
            .into_iter()
            .map(|h| h.index)
            .collect()
    }

    /// Puts a device on a track, or a new track for it: an instrument plays it (in place of
    /// what it had), an effect goes after the track's others. With a preset, that loads too; it
    /// all undoes as one.
    pub fn use_device(
        &mut self,
        d: usize,
        target: Target,
        preset: Option<Preset>,
        cx: &mut Context<Self>,
    ) {
        let Some(device) = self.catalog.devices.get(d).cloned() else {
            return;
        };
        let tracks = self.session.project.tracks.len();
        let t = match target {
            Target::Track(t) if t < tracks => t,
            Target::Track(_) | Target::NewTrack
                if !device.instrument() && tracks > 0 && target != Target::NewTrack =>
            {
                self.track.min(tracks - 1)
            }
            _ => {
                // A new track: an instrument's, or for an effect an audio track.
                let instrument = device
                    .instrument()
                    .then(|| instrument_of(&device))
                    .flatten();
                let plays = instrument.is_some();
                // Named for what it plays: the preset, else the instrument.
                let name = preset
                    .as_ref()
                    .filter(|_| plays)
                    .map_or(device.name.clone(), |p| p.name.clone());
                self.new_track_named(instrument, Some(name), cx);
                self.remember(&device.key);
                // The new track goes after the selected one, and is selected now.
                let t = self.track;
                if plays && let Some(p) = preset.clone() {
                    self.choose(
                        MenuAction::Preset {
                            channel: Channel::Track(t),
                            which: Which::Instrument,
                            preset: p,
                        },
                        cx,
                    );
                }
                if plays {
                    self.status = Some(format!(
                        "{} is ready: play it on your keyboard (⌘K for the computer's), or double-click its lane to write notes.",
                        self.session.project.tracks[t].name
                    ));
                    // A plugin shows its window, as it does in any DAW.
                    if device.own.is_none() {
                        self.open_editor_when_loaded(Channel::Track(t), Which::Instrument, cx);
                    }
                    return;
                }
                // An effect: on the new audio track, as on any other.
                t
            }
        };
        self.track = t;
        let channel = Channel::Track(t);
        let action = match (device.own, device.instrument()) {
            (Some(Own::Eq), _) => MenuAction::AddBuiltin {
                channel,
                builtin: Builtin::Eq,
            },
            (Some(Own::Comp), _) => MenuAction::AddBuiltin {
                channel,
                builtin: Builtin::Comp,
            },
            (Some(Own::Sat), _) => MenuAction::AddBuiltin {
                channel,
                builtin: Builtin::Sat,
            },
            (_, true) => {
                let track = &self.session.project.tracks[t];
                if track.instrument.is_none() && !track.regions.is_empty() {
                    self.status = Some(format!(
                        "{} is an audio track with audio on it: drop {} under the tracks for a track of its own.",
                        track.name, device.name
                    ));
                    cx.notify();
                    return;
                }
                MenuAction::Instrument {
                    track: t,
                    instrument: instrument_of(&device).unwrap_or(Instrument::Synth),
                }
            }
            (_, false) => MenuAction::Insert {
                channel,
                index: self.session.project.tracks[t].inserts.len(),
                plugin: device.formats.first().map(|p| p.plugin_ref()),
            },
        };
        let which = match &action {
            MenuAction::Insert { index, .. } => Which::Insert(*index),
            _ => Which::Instrument,
        };
        let plugin = device.own.is_none();
        self.choose(action, cx);
        if plugin {
            self.open_editor_when_loaded(channel, which, cx);
        }
        if let Some(p) = preset {
            self.choose(
                MenuAction::Preset {
                    channel,
                    which,
                    preset: p,
                },
                cx,
            );
        }
        self.remember(&device.key);
    }

    fn remember(&mut self, key: &str) {
        let c = &mut self.catalog;
        c.recent.retain(|k| k != key);
        c.recent.insert(0, key.to_string());
        c.recent.truncate(24);
        c.save();
    }

    pub(crate) fn toggle_favourite(&mut self, key: &str, cx: &mut Context<Self>) {
        let c = &mut self.catalog;
        match c.favourites.iter().position(|k| k == key) {
            Some(i) => drop(c.favourites.remove(i)),
            None => c.favourites.push(key.to_string()),
        }
        c.save();
        cx.notify();
    }

    /// Enter: the picked row goes in (⇧: on a new track).
    pub fn use_picked(&mut self, new: bool, cx: &mut Context<Self>) {
        let rows: Vec<Row> = self.catalog_rows(cx);
        let usable: Vec<&Row> = rows
            .iter()
            .filter(|r| !matches!(r, Row::Heading(_)))
            .collect();
        let Some(row) = self
            .catalog
            .picked
            .and_then(|i| usable.get(i).copied())
            .cloned()
        else {
            return;
        };
        let target = if new {
            Target::NewTrack
        } else {
            Target::Track(self.track)
        };
        match row {
            Row::Device(d) => self.use_device(d, target, None, cx),
            Row::Preset(d, p) => self.use_device(d, target, Some(p), cx),
            Row::Sample(i) => self.select_sample(i, true, cx),
            Row::Song(s) => self.ask_to_open(s.dir, cx),
            Row::Heading(_) => {}
        }
    }

    /// ↑ ↓ in the Browser: the row picked moves.
    pub fn move_picked(&mut self, by: isize, cx: &mut Context<Self>) {
        let n = self
            .catalog_rows(cx)
            .iter()
            .filter(|r| !matches!(r, Row::Heading(_)))
            .count();
        if n == 0 {
            return;
        }
        let at = self.catalog.picked.map_or(-1, |p| p as isize) + by;
        self.catalog.picked = Some(at.clamp(0, n as isize - 1) as usize);
        cx.notify();
    }

    /// Lets go of a device over the tracks: on the track under it, or under them a new one.
    pub fn drop_device(&mut self, drag: Option<Drag>, cx: &mut Context<Self>) {
        let Some(Drag::Device {
            device,
            at,
            moved: true,
            ..
        }) = drag
        else {
            return;
        };
        if let Some(target) = self.device_target(at) {
            self.use_device(device, target, None, cx);
        }
    }

    /// The track a device dragged to `p` goes on: the row under it, headers or lanes; under the
    /// tracks, a new one.
    pub fn device_target(&self, p: gpui::Point<f32>) -> Option<Target> {
        let b = self.lanes_bounds.get();
        let (left, top) = (f32::from(b.origin.x) - self.header_w, f32::from(b.origin.y));
        let (w, h) = (
            f32::from(b.size.width) + self.header_w,
            f32::from(b.size.height),
        );
        if p.x < left || p.y < top || p.x > left + w || p.y > top + h {
            return None;
        }
        let row = self.track_at_y(p.y - top).unwrap_or(usize::MAX);
        Some(match row < self.session.project.tracks.len() {
            true => Target::Track(row),
            false => Target::NewTrack,
        })
    }

    /// The device being dragged, under the pointer, saying where it'll go.
    pub fn device_ghost(&self) -> Option<impl IntoElement + use<>> {
        let Some(Drag::Device {
            device,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let d = self.catalog.devices.get(device)?;
        let hint = match self.device_target(at) {
            Some(Target::Track(t)) => {
                let name = &self.session.project.tracks[t].name;
                match d.instrument() {
                    true => format!("Plays {name}"),
                    false => format!("On {name}"),
                }
            }
            Some(Target::NewTrack) => "A new track".into(),
            None => "Drop it on a track".into(),
        };
        let color = d.cat.color();
        Some(
            div()
                .absolute()
                .left(px(at.x + 14.))
                .top(px(at.y + 10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(10.))
                .py(px(6.))
                .rounded(px(9.))
                .bg(rgb(0x1f1f26))
                .border_1()
                .border_color(alpha(color, 0.5))
                .shadow_lg()
                .text_size(px(12.))
                .child(d.cat.glyph(15.).text_color(rgb(color)))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(d.name.clone()),
                )
                .child(div().text_color(rgb(TEXT_3)).child(hint)),
        )
    }

    /// The Browser's devices, presets, sounds and songs: under its search and tabs, in the
    /// right panel (`browser.rs`).
    pub fn device_browser(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = self.catalog_rows(cx);
        let query = self.catalog_query(cx);
        let c = &self.catalog;
        let (scope, cat, picked) = (c.scope, c.cat, c.picked);
        let target = self
            .session
            .project
            .tracks
            .get(self.track)
            .map_or("No track: a device makes one".to_string(), |t| {
                format!("Adds to {}", t.name)
            });
        let chips: Vec<Cat> = match scope {
            Scope::Instruments => INSTRUMENT_CATS.to_vec(),
            Scope::Effects => EFFECT_CATS.to_vec(),
            _ => INSTRUMENT_CATS
                .iter()
                .chain(EFFECT_CATS.iter())
                .copied()
                .collect(),
        };
        let chip_row = div()
            .flex()
            .flex_wrap()
            .gap(px(5.))
            .children(chips.into_iter().map(|k| {
                let on = cat == Some(k);
                let color = k.color();
                div()
                    .id(k.name())
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .h(px(24.))
                    .px(px(8.))
                    .rounded(px(12.))
                    .text_size(px(11.5))
                    .cursor_pointer()
                    .border_1()
                    .when(on, |d| {
                        d.bg(alpha(color, 0.18))
                            .border_color(alpha(color, 0.55))
                            .text_color(rgb(TEXT))
                    })
                    .when(!on, |d| {
                        d.bg(rgb(CARD))
                            .border_color(rgb(LINE_SOFT))
                            .text_color(rgb(TEXT_2))
                            .hover(|d| d.bg(rgb(RAISED)))
                    })
                    .child(k.glyph(12.).text_color(rgb(color)))
                    .child(k.name())
                    .debug_selector(move || format!("cat-{}", k.name()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.catalog.cat = if ws.catalog.cat == Some(k) {
                                None
                            } else {
                                Some(k)
                            };
                            ws.catalog.picked = None;
                            cx.notify();
                        }),
                    )
            }));
        let favourites = self.catalog.favourites.clone();
        let open = self.catalog.open.clone();
        let current = self.session.dir.clone();
        let mut n = 0usize;
        let mut list = Vec::new();
        for row in rows {
            if let Row::Heading(h) = &row {
                list.push(
                    div()
                        .pt(px(10.))
                        .pb(px(4.))
                        .px(px(4.))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_3))
                        .child(h.to_uppercase())
                        .into_any_element(),
                );
                continue;
            }
            let on = picked == Some(n);
            n += 1;
            list.push(match row {
                Row::Device(d) => {
                    let dev = &self.catalog.devices[d];
                    let fav = favourites.contains(&dev.key);
                    let shown = open.as_deref() == Some(dev.key.as_str());
                    self.device_row(d, dev.clone(), on, fav, shown, cx)
                        .into_any_element()
                }
                Row::Preset(d, p) => self.preset_row(d, p, on, cx).into_any_element(),
                Row::Sample(i) => self.sound_row(i, on, cx).into_any_element(),
                Row::Song(s) => {
                    let here = s.dir == current;
                    crate::pages::projects::song_row(s, on, here, cx).into_any_element()
                }
                Row::Heading(_) => unreachable!(),
            });
        }
        let empty = list.is_empty();
        div()
            .key_context("DeviceBrowser")
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(10.))
            .pb(px(10.))
            .when(query.is_empty() || cat.is_some(), |d| d.child(chip_row))
            .child(
                div()
                    .id("browser-results")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(list)
                    .when(empty, |d| {
                        d.child(
                            div()
                                .pt(px(20.))
                                .text_size(px(12.))
                                .text_color(rgb(TEXT_3))
                                .child(match (scope, query.is_empty()) {
                                    (_, true) => "Looking for plugins…".to_string(),
                                    _ => format!("Nothing called \u{201c}{query}\u{201d}."),
                                }),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child(div().text_color(rgb(TEXT_3)).child(target))
                    .child("Click adds · double-click or ⇧↵ a new track · drag onto a track"),
            )
    }

    fn device_row(
        &self,
        d: usize,
        dev: Device,
        on: bool,
        fav: bool,
        shown: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let color = dev.cat.color();
        let key = dev.key.clone();
        let fav_key = key.clone();
        let open_key = key.clone();
        let plugin = dev.own.is_none();
        let sub = match (&dev.own, dev.badges().as_slice()) {
            (Some(_), _) => format!("Pina Studio · {}", dev.cat.name()),
            (None, b) => format!("{} · {}", dev.vendor, b.join(" ")),
        };
        div()
            .group("device-row")
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(38.))
            .px(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .when(on, |d| d.bg(alpha(ACCENT, 0.16)))
            .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
            .debug_selector(move || format!("device-{key}"))
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(alpha(color, 0.14))
                    .child(dev.cat.glyph(15.).text_color(rgb(color))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(dev.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(sub),
                    ),
            )
            .when(plugin, |row| {
                row.child(
                    div()
                        .id(("presets", d))
                        .size(px(22.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.))
                        .text_color(rgb(if shown { TEXT } else { TEXT_4 }))
                        .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT_2)))
                        .child(icon(Icon::Chevron, 12.).text_color(rgb(if shown {
                            TEXT
                        } else {
                            TEXT_3
                        })))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.catalog.open =
                                    match ws.catalog.open.as_deref() == Some(&open_key) {
                                        true => None,
                                        false => Some(open_key.clone()),
                                    };
                                cx.notify();
                            }),
                        ),
                )
            })
            .child(self.device_agent_button(d, "device-row", cx))
            .child(
                div()
                    .size(px(22.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .text_size(px(13.))
                    .text_color(rgb(if fav { AMBER } else { TEXT_4 }))
                    .hover(|d| d.bg(rgb(RAISED)))
                    .child(if fav { "★" } else { "☆" })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.toggle_favourite(&fav_key, cx);
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let p = pt(e.position);
                    if e.click_count == 2 {
                        ws.use_device(d, Target::NewTrack, None, cx);
                        return;
                    }
                    ws.drag = Some(Drag::Device {
                        device: d,
                        from: p,
                        at: p,
                        moved: false,
                    });
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |ws, e: &gpui::MouseUpEvent, _, cx| {
                    // A click that didn't drag: onto the selected track. (Two clicks make a
                    // track of its own, above.)
                    if let Some(Drag::Device { moved: false, .. }) = ws.drag
                        && e.click_count < 2
                    {
                        ws.drag = None;
                        let t = ws.track;
                        ws.use_device(d, Target::Track(t), None, cx);
                    }
                }),
            )
    }

    fn preset_row(&self, d: usize, p: Preset, on: bool, cx: &mut Context<Self>) -> Div {
        let name = p.name.clone();
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(28.))
            .pl(px(41.))
            .pr(px(8.))
            .rounded(px(7.))
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(rgb(TEXT_2))
            .when(on, |d| d.bg(alpha(ACCENT, 0.16)))
            .when(!on, |d| d.hover(|d| d.bg(rgb(OFF)).text_color(rgb(TEXT))))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(name),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_4))
                    .child("preset"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let target = match e.click_count {
                        2 => Target::NewTrack,
                        _ => Target::Track(ws.track),
                    };
                    ws.use_device(d, target, Some(p.clone()), cx);
                }),
            )
    }

    fn sound_row(&self, i: usize, on: bool, cx: &mut Context<Self>) -> Div {
        let Some(s) = self.browser.sample(i) else {
            return div();
        };
        let color = crate::sounds::browser::category_color(s.analysis.category);
        let about = pina_engine::samples::search::describe(s);
        div()
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(36.))
            .px(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .when(on, |d| d.bg(alpha(ACCENT, 0.16)))
            .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(alpha(color, 0.14))
                    .child(
                        glyph(
                            pina_engine::music::look::category_icon(s.analysis.category),
                            15.,
                        )
                        .text_color(rgb(color)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(s.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(about),
                    ),
            )
            // Click to hear it (in the Samples browser); drag it onto the tracks as there.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let p = pt(e.position);
                    ws.select_sample(i, true, cx);
                    ws.drag = Some(Drag::Sample {
                        index: i,
                        from: p,
                        at: p,
                        moved: false,
                    });
                }),
            )
    }
}

/// The instrument a device is.
pub(crate) fn instrument_of(d: &Device) -> Option<Instrument> {
    match d.own {
        Some(Own::Synth) => Some(Instrument::Synth),
        Some(Own::Drums) => Some(Instrument::Drums),
        Some(Own::Sampler) => Some(Instrument::Sampler(Default::default())),
        Some(_) => None,
        None => d
            .formats
            .first()
            .map(|p| Instrument::Plugin(p.plugin_ref())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn plugin(
        format: Format,
        class: &str,
        vendor: &str,
        instrument: bool,
        cats: &[&str],
        uid: Option<&str>,
    ) -> PluginInfo {
        PluginInfo {
            format,
            path: Path::new("/p").join(class),
            class: class.into(),
            uid: uid.map(str::to_string),
            vendor: vendor.into(),
            instrument,
            categories: cats.iter().map(|c| c.to_string()).collect(),
        }
    }

    #[test]
    fn plugins_go_where_they_say_once_each() {
        let all = [
            plugin(
                Format::Vst3,
                "Dragonfly Hall Reverb",
                "Michael Willis",
                false,
                &["Fx", "Reverb"],
                None,
            ),
            plugin(
                Format::Clap,
                "Dragonfly Hall Reverb",
                "Michael Willis",
                false,
                &["audio-effect", "reverb"],
                Some("x"),
            ),
            plugin(
                Format::Au,
                "AUDelay",
                "Apple",
                false,
                &[],
                Some("aufx dely appl"),
            ),
            plugin(
                Format::Clap,
                "Surge XT",
                "Surge Synth Team",
                true,
                &["instrument", "synthesizer"],
                Some("s"),
            ),
            plugin(
                Format::Vst3,
                "Tape Echo Thing",
                "Someone",
                false,
                &["Fx"],
                None,
            ),
            plugin(Format::Vst3, "Glue Box", "Someone", false, &[], None),
        ];
        let d = devices(&all);
        let find = |name: &str| d.iter().find(|x| x.name == name).unwrap();
        let hall = find("Dragonfly Hall Reverb");
        assert_eq!(hall.cat, Cat::Reverb);
        // Both formats, one entry, CLAP first.
        assert_eq!(hall.badges(), ["CLAP", "VST3"]);
        assert_eq!(
            d.iter()
                .filter(|x| x.name == "Dragonfly Hall Reverb")
                .count(),
            1
        );
        assert_eq!(find("AUDelay").cat, Cat::Delay);
        assert_eq!(find("Surge XT").cat, Cat::Synth);
        // Silent categories: its name says.
        assert_eq!(find("Tape Echo Thing").cat, Cat::Delay);
        assert_eq!(find("Glue Box").cat, Cat::Dynamics);
        // Searches go by words that mean it: "echo" finds the delays, "space" the reverb.
        let found = |q: &str| -> Vec<&str> {
            let mut s: Vec<(u32, &str)> = d
                .iter()
                .filter_map(|x| score(x, q).map(|n| (n, x.name.as_str())))
                .collect();
            s.sort_by_key(|a| std::cmp::Reverse(a.0));
            s.into_iter().map(|x| x.1).collect()
        };
        assert_eq!(found("echo")[0], "Tape Echo Thing");
        assert!(found("echo").contains(&"AUDelay"));
        assert_eq!(found("space"), ["Dragonfly Hall Reverb"]);
        assert_eq!(found("surge")[0], "Surge XT");
        assert!(found("zzz").is_empty());
    }
}
