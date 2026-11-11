//! Bloom's sounds: the factory's seeds and every plant saved, searched by name, kind and what
//! they're like, newest first. A plant saved keeps all of it (its seed, its branches as grown,
//! its family) in Pina's folder as `Sounds/Bloom/<name>.pinabloom`.

use std::path::PathBuf;

use gpui::Focusable;
use serde::{Deserialize, Serialize};

use super::*;

/// A plant saved.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct BloomSound {
    pub name: String,
    pub kind: SeedKind,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub made: u64,
    pub bloom: Bloom,
    /// The factory's, not on disk.
    #[serde(skip)]
    pub factory: bool,
}

/// The factory's seeds: eight of each kind, by name. What each is like is worked out from its
/// genes, so it says what it really is.
const FACTORY: [(SeedKind, [&str; 8]); 29] = [
    (
        SeedKind::Any,
        [
            "Wildflower",
            "Mutant",
            "Hybrid",
            "Volunteer",
            "Seedling",
            "Cross-Pollen",
            "Stray Spore",
            "Odd Bloom",
        ],
    ),
    (
        SeedKind::Bass,
        [
            "Moss Bass",
            "Root Knot",
            "Tuber",
            "Deep Loam",
            "Bulb Bass",
            "Rhizome",
            "Burrow",
            "Peat",
        ],
    ),
    (
        SeedKind::Sub,
        [
            "Bedrock",
            "Taproot Sub",
            "Groundwater",
            "Underlayer",
            "Cellar Hum",
            "Low Tide",
            "Core Sample",
            "Deep Soil",
        ],
    ),
    (
        SeedKind::Reese,
        [
            "Twisted Vine",
            "Bramble",
            "Creeper",
            "Kudzu",
            "Strangler Fig",
            "Wisteria Rope",
            "Tangle",
            "Ivy Wall",
        ],
    ),
    (
        SeedKind::Acid,
        [
            "Sundew",
            "Pitcher Plant",
            "Venus Snap",
            "Nettle",
            "Acid Rain",
            "Sour Sap",
            "Citrus Peel",
            "Bog Squelch",
        ],
    ),
    (
        SeedKind::Keys,
        [
            "Greenhouse Keys",
            "Lichen Rhodes",
            "Bark Clav",
            "Potting Bench",
            "Glasshouse Upright",
            "Fern Wurli",
            "Terrarium",
            "Seed Catalog",
        ],
    ),
    (
        SeedKind::Organ,
        [
            "Sap Organ",
            "Cathedral Pine",
            "Reed Pipes",
            "Bamboo Stops",
            "Chapel Garden",
            "Hollow Stem",
            "Drawbar Moss",
            "Vestry",
        ],
    ),
    (
        SeedKind::Bell,
        [
            "Bluebell",
            "Glass Petal",
            "Temple Bloom",
            "Chime Tree",
            "Snowdrop",
            "Harebell",
            "Foxglove",
            "Lily of the Valley",
        ],
    ),
    (
        SeedKind::Mallet,
        [
            "Woodpecker",
            "Seed Rattle",
            "Marimba Root",
            "Pebble",
            "Gourd",
            "Coconut Shell",
            "Teak Bar",
            "Acorn Cap",
        ],
    ),
    (
        SeedKind::Pluck,
        [
            "Dew Pluck",
            "Seed Pod",
            "Thorn",
            "Pea Shoot",
            "Snap Pea",
            "Burst Pod",
            "Cress",
            "Spark Seed",
        ],
    ),
    (
        SeedKind::Harp,
        [
            "Pollen Harp",
            "Willow Strings",
            "Spider Silk",
            "Dandelion Lyre",
            "Rain Harp",
            "Reed Lyre",
            "Gossamer",
            "Wind Harp",
        ],
    ),
    (
        SeedKind::Strings,
        [
            "Fern Section",
            "Willow Quartet",
            "Long Grass",
            "Cello Root",
            "Bowed Branch",
            "Meadow Strings",
            "Silk Cello",
            "Orchard Ensemble",
        ],
    ),
    (
        SeedKind::Stab,
        [
            "Blossom Hit",
            "Thorn Stab",
            "Petal Chord",
            "Sunburst",
            "Pop Pod",
            "Bud Burst",
            "Stem Snap",
            "Bloom Stab",
        ],
    ),
    (
        SeedKind::Lead,
        [
            "Sunflower Lead",
            "Vine Saw",
            "Stamen",
            "Tendril",
            "Hummingbird",
            "Firefly",
            "Sunbeam",
            "Dragonfly",
        ],
    ),
    (
        SeedKind::Brass,
        [
            "Trumpet Vine",
            "Horn of Plenty",
            "Brass Thistle",
            "Golden Rod",
            "Fanfare Fern",
            "Bugle Lily",
            "Copper Leaf",
            "Bronze Oak",
        ],
    ),
    (
        SeedKind::Flute,
        [
            "Reed Flute",
            "Hollow Reed",
            "Wind Through Grass",
            "Bamboo Pipe",
            "Birdsong",
            "Breath of Pine",
            "Gourd Ocarina",
            "Whistling Stem",
        ],
    ),
    (
        SeedKind::Choir,
        [
            "Fern Choir",
            "Moss Voices",
            "Cathedral Grove",
            "Forest Chant",
            "Morning Hymn",
            "Leaf Chorus",
            "Wood Nymphs",
            "Vowel Bloom",
        ],
    ),
    (
        SeedKind::Pad,
        [
            "Canopy",
            "Morning Mist",
            "Undergrowth",
            "Greenhouse Glow",
            "Meadow Haze",
            "Soft Moss",
            "Pollen Cloud",
            "Evening Garden",
        ],
    ),
    (
        SeedKind::Drone,
        [
            "Old Oak",
            "Riverbed",
            "Night Garden",
            "Taproot",
            "Deep Forest",
            "Standing Stone",
            "Root System",
            "Earth Hum",
        ],
    ),
    (
        SeedKind::Atmosphere,
        [
            "Dawn Air",
            "Fog Bank",
            "Distant Forest",
            "Mountain Air",
            "Clearing",
            "Rain Shadow",
            "Open Field",
            "Cloud Forest",
        ],
    ),
    (
        SeedKind::Swell,
        [
            "Sunrise",
            "Bud Opening",
            "Rising Sap",
            "Tide Bloom",
            "Unfurling Fern",
            "Spring Thaw",
            "Germination",
            "Slow Bloom",
        ],
    ),
    (
        SeedKind::Kick,
        [
            "Acorn Kick",
            "Fallen Log",
            "Coconut Thump",
            "Earth Drop",
            "Stone Fruit",
            "Seed Thud",
            "Root Kick",
            "Pumpkin",
        ],
    ),
    (
        SeedKind::Snare,
        [
            "Twig Snap",
            "Dry Leaves",
            "Bark Crack",
            "Husk",
            "Kindling",
            "Brittle Stem",
            "Pine Cone",
            "Straw",
        ],
    ),
    (
        SeedKind::Hat,
        [
            "Beetle Click",
            "Cricket",
            "Seed Shaker",
            "Dewdrop Tick",
            "Grass Hiss",
            "Cicada",
            "Sprinkler",
            "Rice Husk",
        ],
    ),
    (
        SeedKind::Perc,
        [
            "Hollow Log",
            "Woodblock Root",
            "Bamboo Knock",
            "Clay Pot",
            "Shell Tap",
            "Gourd Tom",
            "Nutshell",
            "Branch Knock",
        ],
    ),
    (
        SeedKind::Texture,
        [
            "Rain on Leaves",
            "Mycelium",
            "Spore Cloud",
            "Compost",
            "Rustling Hedge",
            "Crackling Fire",
            "Bubbling Bog",
            "Wind in Reeds",
        ],
    ),
    (
        SeedKind::Riser,
        [
            "Growing Season",
            "Vine Climb",
            "Sprout Rise",
            "Pressure Front",
            "Geyser",
            "Updraft",
            "Rising Tide",
            "Wildfire",
        ],
    ),
    (
        SeedKind::Zap,
        [
            "Firefly Zap",
            "Static Bloom",
            "Pollen Laser",
            "Lightning Bug",
            "Electric Eel",
            "Plasma Petal",
            "Zapweed",
            "Arc Flower",
        ],
    ),
    (
        SeedKind::Glitch,
        [
            "Corrupted Seed",
            "Broken Stem",
            "Pixel Moss",
            "Bitrot",
            "Mutation Error",
            "Circuit Vine",
            "Dead Pixel Fern",
            "Stutter Bloom",
        ],
    ),
];

/// How many sounds the factory has.
pub(crate) const FACTORY_SOUNDS: usize = FACTORY.len() * 8;

/// Factory sound `i` as a plant: planted, its branches grown a little each way.
pub(crate) fn factory_sound(i: usize) -> Option<BloomSound> {
    let (kind, names) = FACTORY.get(i / 8)?;
    let name = names[i % 8];
    let mut bloom = Bloom::default();
    bloom.plant(*kind, mix32(0xf10e_0000 ^ i as u32));
    bloom.ancestors.clear();
    bloom.generation = 0;
    bloom.sound = name.to_string();
    Some(BloomSound {
        name: name.into(),
        kind: *kind,
        about: describe(&bloom.seed()),
        made: 0,
        bloom,
        factory: true,
    })
}

pub(crate) fn bloom_dir() -> PathBuf {
    crate::editors::poly::library::sounds_dir().join("Bloom")
}

fn file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_'".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let clean = clean.trim().to_string();
    match clean.is_empty() {
        true => "Untitled".into(),
        false => clean,
    }
}

/// Every saved plant in `dir`, newest first.
pub(crate) fn read_sounds(dir: &std::path::Path) -> Vec<BloomSound> {
    let mut out: Vec<BloomSound> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "pinabloom"))
        .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
        .collect();
    out.sort_by(|a: &BloomSound, b| b.made.cmp(&a.made).then(a.name.cmp(&b.name)));
    out
}

/// Saves `s` in `dir` (over one of the same name); where it went.
pub(crate) fn write_sound(dir: &std::path::Path, s: &BloomSound) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.pinabloom", file_name(&s.name)));
    let json = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Something done to the sounds.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundOp {
    /// By its place: factory first, then the saved.
    Load(usize),
    /// A kind or a family shown, or everything; where they come from; their order.
    Filter(Option<SeedKind>),
    Family(Option<SeedFamily>),
    From(From),
    Sort(SortBy),
    /// One of those shown, planted at random.
    Random,
    Delete(usize),
    Save,
    CancelSave,
}

/// Where the sounds shown come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    Everywhere,
    Saved,
    Factory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortBy {
    Newest,
    Name,
}

pub(crate) struct SoundsView {
    pub find: Entity<TextInput>,
    pub query: String,
    pub kind: Option<SeedKind>,
    pub family: Option<SeedFamily>,
    pub from: From,
    pub sort: SortBy,
    /// The factory's, made once; the saved ones, as last read.
    pub factory: Vec<BloomSound>,
    pub saved: Vec<BloomSound>,
    pub dir: PathBuf,
    /// Save's name, while it's asked for.
    pub naming: Option<Entity<TextInput>>,
}

impl SoundsView {
    pub fn new(cx: &mut Context<Workspace>) -> (SoundsView, Vec<Subscription>) {
        let find =
            cx.new(|cx| TextInput::search("Search sounds: names, kinds, what they're like", cx));
        let subs = vec![cx.observe(&find, |ws, input, cx| {
            let text = input.read(cx).text().trim().to_lowercase();
            if let Some(v) = ws.bloom_view.as_mut().filter(|v| v.sounds.query != text) {
                v.sounds.query = text;
                cx.notify();
            }
        })];
        let dir = bloom_dir();
        let saved = read_sounds(&dir);
        (
            SoundsView {
                find,
                query: String::new(),
                kind: None,
                family: None,
                from: From::Everywhere,
                sort: SortBy::Newest,
                factory: (0..FACTORY_SOUNDS).filter_map(factory_sound).collect(),
                saved,
                dir,
                naming: None,
            },
            subs,
        )
    }

    /// Every sound, factory first, each with its place.
    pub fn all(&self) -> Vec<(usize, BloomSound)> {
        self.factory
            .iter()
            .chain(&self.saved)
            .cloned()
            .enumerate()
            .collect()
    }

    /// Those where they're from and the search leave, before the kind is narrowed to: for the
    /// counts beside each kind.
    fn found(&self) -> Vec<(usize, &BloomSound)> {
        let words: Vec<&str> = self.query.split_whitespace().collect();
        self.factory
            .iter()
            .chain(&self.saved)
            .enumerate()
            .filter(|(_, s)| match self.from {
                From::Everywhere => true,
                From::Saved => !s.factory,
                From::Factory => s.factory,
            })
            .filter(|(_, s)| {
                let family = s.kind.family().map_or("", |f| f.name());
                let hay =
                    format!("{} {} {} {}", s.name, s.kind.name(), family, s.about).to_lowercase();
                words.iter().all(|w| hay.contains(w))
            })
            .collect()
    }

    /// How many of the found are of `kind`, or of `family`, or all of them.
    pub fn count(&self, kind: Option<SeedKind>, family: Option<SeedFamily>) -> usize {
        self.found()
            .iter()
            .filter(|(_, s)| kind.is_none_or(|k| s.kind == k))
            .filter(|(_, s)| family.is_none_or(|f| s.kind.family() == Some(f)))
            .count()
    }

    /// Those shown: found, of the kind or family picked, in order (the saved first, newest
    /// first, or all by name).
    pub fn shown(&self) -> Vec<(usize, BloomSound)> {
        let mut out: Vec<(usize, BloomSound)> = self
            .found()
            .into_iter()
            .filter(|(_, s)| self.kind.is_none_or(|k| s.kind == k))
            .filter(|(_, s)| self.family.is_none_or(|f| s.kind.family() == Some(f)))
            .map(|(i, s)| (i, s.clone()))
            .collect();
        match self.sort {
            SortBy::Newest => out.sort_by(|a, b| {
                a.1.factory
                    .cmp(&b.1.factory)
                    .then(b.1.made.cmp(&a.1.made))
                    .then(a.0.cmp(&b.0))
            }),
            SortBy::Name => out.sort_by_key(|x| x.1.name.to_lowercase()),
        }
        out
    }
}

impl Workspace {
    pub(crate) fn bloom_sound_op(&mut self, t: usize, op: SoundOp, cx: &mut Context<Self>) {
        match op {
            SoundOp::Filter(_) | SoundOp::Family(_) | SoundOp::From(_) | SoundOp::Sort(_) => {
                if let Some(v) = self.bloom_view.as_mut().map(|v| &mut v.sounds) {
                    match op {
                        SoundOp::Filter(k) => (v.kind, v.family) = (k, None),
                        SoundOp::Family(f) => (v.kind, v.family) = (None, f),
                        SoundOp::From(f) => v.from = f,
                        SoundOp::Sort(by) => v.sort = by,
                        _ => {}
                    }
                }
                cx.notify();
            }
            SoundOp::Random => {
                let luck = self.bloom_view.as_mut().map_or(1, |v| {
                    v.luck = v.luck.wrapping_add(0x2545_f491);
                    v.luck
                });
                let pick = self.bloom_view.as_ref().and_then(|v| {
                    let shown = v.sounds.shown();
                    shown
                        .get(mix32(luck) as usize % shown.len().max(1))
                        .map(|x| x.0)
                });
                if let Some(i) = pick {
                    self.bloom_sound_op(t, SoundOp::Load(i), cx);
                }
            }
            SoundOp::Load(i) => {
                let Some(s) = self
                    .bloom_view
                    .as_ref()
                    .and_then(|v| v.sounds.all().into_iter().find(|(k, _)| *k == i))
                    .map(|x| x.1)
                else {
                    return;
                };
                let name = s.name.clone();
                self.edit(cx, |ws| {
                    if let Some(b) = ws.bloom_mut(t) {
                        let mut bloom = s.bloom.clone();
                        bloom.sound = s.name.clone();
                        *b = bloom;
                    }
                });
                if let Some(v) = self.bloom_view.as_mut() {
                    for k in 0..BRANCHES {
                        v.shown[k] = 0.0;
                        v.speed[k] = -0.5 * k as f32;
                    }
                }
                self.status = Some(format!("{name} is planted"));
            }
            SoundOp::Delete(i) => {
                let Some(v) = self.bloom_view.as_mut() else {
                    return;
                };
                let all = v.sounds.all();
                let Some((_, s)) = all.into_iter().find(|(k, s)| *k == i && !s.factory) else {
                    return;
                };
                let path = v
                    .sounds
                    .dir
                    .join(format!("{}.pinabloom", file_name(&s.name)));
                match crate::pages::projects::files::trash(&path) {
                    Ok(()) => {
                        v.sounds.saved = read_sounds(&v.sounds.dir);
                        self.status = Some(format!("{} is in the Trash", s.name));
                    }
                    Err(e) => self.status = Some(e),
                }
                cx.notify();
            }
            SoundOp::Save => {
                let Some(v) = self.bloom_view.as_ref() else {
                    return;
                };
                let name = v
                    .sounds
                    .naming
                    .as_ref()
                    .map(|n| n.read(cx).text().trim().to_string())
                    .unwrap_or_default();
                if name.is_empty() {
                    return;
                }
                let Some(mut bloom) = self.bloom(t).cloned() else {
                    return;
                };
                bloom.sound = name.clone();
                let sound = BloomSound {
                    name: name.clone(),
                    kind: bloom.kind,
                    about: String::new(),
                    made: crate::editors::poly::library::now(),
                    bloom,
                    factory: false,
                };
                let dir = v.sounds.dir.clone();
                match write_sound(&dir, &sound) {
                    Ok(_) => {
                        self.edit(cx, |ws| {
                            if let Some(b) = ws.bloom_mut(t) {
                                b.sound = name.clone();
                            }
                        });
                        if let Some(v) = self.bloom_view.as_mut() {
                            v.sounds.saved = read_sounds(&dir);
                            v.sounds.naming = None;
                        }
                        self.status = Some(format!("{name} is saved in your Bloom sounds"));
                    }
                    Err(e) => self.status = Some(format!("Couldn't save {name}: {e}")),
                }
                cx.notify();
            }
            SoundOp::CancelSave => {
                if let Some(v) = self.bloom_view.as_mut() {
                    v.sounds.naming = None;
                }
                cx.notify();
            }
        }
    }

    /// Save, asked for its name.
    pub(crate) fn bloom_start_save(
        &mut self,
        t: usize,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(b) = self.bloom(t) else { return };
        let name = match b.sound.is_empty() {
            true => format!("{} {}", b.kind.name(), b.generation + 1),
            false => b.sound.clone(),
        };
        let field = cx.new(|cx| TextInput::form(&name, "What it's called", cx));
        cx.subscribe(
            &field,
            move |ws, _, e: &crate::ui::text::TextEvent, cx| match e {
                crate::ui::text::TextEvent::Confirm(_) => {
                    if ws
                        .bloom_view
                        .as_ref()
                        .is_some_and(|v| v.sounds.naming.is_some())
                    {
                        ws.bloom_sound_op(t, SoundOp::Save, cx);
                    }
                }
                crate::ui::text::TextEvent::Cancel => ws.bloom_sound_op(t, SoundOp::CancelSave, cx),
            },
        )
        .detach();
        window.focus(&field.read(cx).focus_handle(cx), cx);
        if let Some(v) = self.bloom_view.as_mut() {
            v.sounds.naming = Some(field);
        }
        cx.notify();
    }

    pub(crate) fn bloom_save_dialog(&mut self, t: usize, cx: &mut Context<Self>) -> Option<Div> {
        let field = self.bloom_view.as_ref()?.sounds.naming.clone()?;
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(alpha(0x000000, 0.45))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_sound_op(t, SoundOp::CancelSave, cx);
                    }),
                )
                .child(
                    div()
                        .w(px(380.))
                        .p(px(18.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .rounded(px(12.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE_STRONG))
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(title_text("Save this plant"))
                        .child(hint(
                            "All of it is kept: the seed, every branch as it's grown, and its family.",
                        ))
                        .child(crate::pages::genres::field_box(Some(field), "bloom-save-name"))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.))
                                .child(button("Cancel", false).on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.bloom_sound_op(t, SoundOp::CancelSave, cx);
                                    }),
                                ))
                                .child(
                                    button("Save", true)
                                        .debug_selector(|| "bloom-save-confirm".into())
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |ws, _, _, cx| {
                                                cx.stop_propagation();
                                                ws.bloom_sound_op(t, SoundOp::Save, cx);
                                            }),
                                        ),
                                ),
                        ),
                ),
        )
    }

    pub(crate) fn bloom_sounds_page(&mut self, t: usize, b: &Bloom, cx: &mut Context<Self>) -> Div {
        let Some(v) = self.bloom_view.as_ref() else {
            return div();
        };
        let sv = &v.sounds;
        let shown = sv.shown();
        let (kind, family, from, sort) = (sv.kind, sv.family, sv.from, sv.sort);
        let total = sv.factory.len() + sv.saved.len();
        let find = sv.find.clone();
        // Where they come from.
        let sources = [
            ("Everything", From::Everywhere, total),
            ("Your sounds", From::Saved, sv.saved.len()),
            ("Factory", From::Factory, sv.factory.len()),
        ]
        .map(|(name, f, n)| {
            let on = from == f;
            div()
                .h(px(30.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .rounded(px(8.))
                .cursor_pointer()
                .bg(if on { rgb(RAISED) } else { rgb(SURFACE) })
                .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                .hover(|d| d.bg(rgb(CARD)))
                .debug_selector(move || {
                    format!("bloom-from-{}", name.to_lowercase().replace(' ', "-"))
                })
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(name),
                )
                .child(small(n.to_string()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::Sound(SoundOp::From(f)), cx);
                    }),
                )
        });
        // Each family, its kinds under it as chips, how many of each beside them.
        let families: Vec<Div> = std::iter::once(None)
            .chain(SeedFamily::ALL.map(Some))
            .map(|f| {
                let hue = f.map_or(0.33, |f| f.hue());
                let on = family == f && kind.is_none() && f.is_some();
                let name = f.map_or("Anything", |f| f.name());
                let n = match f {
                    Some(f) => sv.count(None, Some(f)),
                    None => sv.count(Some(SeedKind::Any), None),
                };
                let header = div()
                    .h(px(26.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(px(7.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(gpui::hsla(hue, 0.5, 0.5, 0.16)))
                    .hover(|d| d.bg(rgb(CARD)))
                    .debug_selector(move || {
                        format!(
                            "bloom-family-{}",
                            name.to_lowercase().replace([' ', '&'], "")
                        )
                    })
                    .child(
                        div()
                            .size(px(8.))
                            .rounded_full()
                            .bg(gpui::hsla(hue, 0.65, 0.6, 1.0)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(gpui::hsla(hue, 0.45, 0.78, 1.0))
                            .child(name),
                    )
                    .child(small(n.to_string()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            let op = match f {
                                Some(f) => SoundOp::Family(Some(f)),
                                None => SoundOp::Filter(Some(SeedKind::Any)),
                            };
                            ws.bloom_op(t, BloomOp::Sound(op), cx);
                        }),
                    );
                let chips = f.map(|f| {
                    div()
                        .pl(px(15.))
                        .pb(px(4.))
                        .flex()
                        .flex_wrap()
                        .gap(px(4.))
                        .children(f.kinds().iter().map(|&k| {
                            let on = kind == Some(k);
                            let n = sv.count(Some(k), None);
                            div()
                                .h(px(24.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .rounded(px(12.))
                                .border_1()
                                .border_color(if on {
                                    gpui::hsla(hue, 0.6, 0.6, 0.9)
                                } else {
                                    gpui::rgb(LINE).into()
                                })
                                .bg(if on {
                                    gpui::hsla(hue, 0.55, 0.5, 0.2)
                                } else {
                                    gpui::rgb(CARD).into()
                                })
                                .cursor_pointer()
                                .hover(|d| d.bg(rgb(RAISED)))
                                .debug_selector(move || {
                                    format!("bloom-filter-{}", k.name().to_lowercase())
                                })
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                                        .child(k.name()),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.5))
                                        .text_color(rgb(TEXT_4))
                                        .child(n.to_string()),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        let to = if on { None } else { Some(k) };
                                        ws.bloom_op(t, BloomOp::Sound(SoundOp::Filter(to)), cx);
                                    }),
                                )
                        }))
                });
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(header)
                    .children(chips)
            })
            .collect();
        let current = b.sound.clone();
        let cards: Vec<Div> = shown
            .iter()
            .map(|(i, s)| self.bloom_sound_card(t, *i, s, s.name == current, cx))
            .collect();
        let empty = shown.is_empty();
        let narrowed = match (kind, family) {
            (Some(k), _) => format!(" · {}", k.name()),
            (_, Some(f)) => format!(" · {}", f.name()),
            _ => String::new(),
        };
        let sort_seg = |name: &'static str, by: SortBy, cx: &mut Context<Self>| {
            seg(name, sort == by, 26.)
                .debug_selector(move || format!("bloom-sort-{}", name.to_lowercase()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.bloom_op(t, BloomOp::Sound(SoundOp::Sort(by)), cx);
                    }),
                )
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .child(
                div()
                    .id("bloom-kinds")
                    .w(px(LEFT_W + 24.0))
                    .flex_none()
                    .h_full()
                    .overflow_y_scroll()
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(section_title("Library").mb(px(4.)))
                            .children(sources),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .child(section_title("Kinds"))
                                    .when(kind.is_some() || family.is_some(), |d| {
                                        d.child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(rgb(ACCENT_TEXT))
                                                .cursor_pointer()
                                                .debug_selector(|| "bloom-filter-clear".into())
                                                .child("Show all")
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |ws, _, _, cx| {
                                                        cx.stop_propagation();
                                                        ws.bloom_op(
                                                            t,
                                                            BloomOp::Sound(SoundOp::Filter(None)),
                                                            cx,
                                                        );
                                                    }),
                                                ),
                                        )
                                    }),
                            )
                            .children(families),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .p(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(div().flex_1().child(crate::pages::genres::field_box(
                                Some(find),
                                "bloom-sound-search",
                            )))
                            .child(
                                tray()
                                    .p(px(2.))
                                    .rounded(px(8.))
                                    .child(sort_seg("Newest", SortBy::Newest, cx))
                                    .child(sort_seg("A–Z", SortBy::Name, cx)),
                            )
                            .child(
                                button("Surprise me", false)
                                    .h(px(30.))
                                    .debug_selector(|| "bloom-sound-random".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.bloom_op(t, BloomOp::Sound(SoundOp::Random), cx);
                                        }),
                                    ),
                            ),
                    )
                    .child(small(format!(
                        "{} of {} sounds{narrowed}",
                        shown.len(),
                        total
                    )))
                    .child(
                        div()
                            .id("bloom-sounds")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_wrap()
                            .content_start()
                            .gap(px(10.))
                            .when(empty, |d| {
                                d.child(hint("Nothing matches: try fewer words, or show all."))
                            })
                            .children(cards),
                    ),
            )
    }

    /// A sound's card: its flower as its genes draw it, its name and kind, what it's like.
    fn bloom_sound_card(
        &self,
        t: usize,
        i: usize,
        s: &BloomSound,
        on: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let hue = s.kind.family().map_or(0.33, |f| f.hue());
        let genes = s.bloom.seed();
        let factory = s.factory;
        let made = match s.made {
            0 => "Factory".to_string(),
            m => {
                let now = crate::editors::poly::library::now();
                ago(Duration::from_secs(now.saturating_sub(m)))
            }
        };
        let portrait = crate::ui::scale::scaled_canvas(
            |_, _, _| {},
            move |bb, _, window, _| {
                let c = (
                    f32::from(bb.origin.x) + f32::from(bb.size.width) / 2.0,
                    f32::from(bb.origin.y) + f32::from(bb.size.height) / 2.0 - 4.0,
                );
                super::garden::paint_portrait(window, bb, c, &genes, hue);
            },
        )
        .size_full();
        div()
            .w(px(246.))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .overflow_hidden()
            .border_1()
            .border_color(if on {
                gpui::hsla(hue, 0.6, 0.6, 1.0)
            } else {
                gpui::rgb(LINE).into()
            })
            .bg(rgb(CARD))
            .cursor_pointer()
            .hover(|d| d.border_color(gpui::hsla(hue, 0.5, 0.55, 0.8)))
            .debug_selector(move || format!("bloom-sound-{i}"))
            .child(div().h(px(92.)).bg(gpui::rgb(0x090c0b)).child(portrait))
            .child(
                div()
                    .p(px(10.))
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(s.name.clone()),
                            )
                            .child(small(made)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .h(px(18.))
                                    .px(px(6.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(4.))
                                    .bg(gpui::hsla(hue, 0.55, 0.5, 0.18))
                                    .text_color(gpui::hsla(hue, 0.55, 0.75, 1.0))
                                    .text_size(px(10.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .whitespace_nowrap()
                                    .child(s.kind.name()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(s.about.clone()),
                            ),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.bloom_op(t, BloomOp::Sound(SoundOp::Load(i)), cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let mut items = vec![(
                        "Plant it".to_string(),
                        MenuAction::Bloom(t, BloomOp::Sound(SoundOp::Load(i))),
                    )];
                    if !factory {
                        items.push((
                            "Move to the Trash".into(),
                            MenuAction::Bloom(t, BloomOp::Sound(SoundOp::Delete(i))),
                        ));
                    }
                    ws.bloom_menu(e.position, items, cx);
                }),
            )
    }
}

fn title_text(s: &'static str) -> Div {
    div()
        .text_size(px(15.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_factory_seed_plants_and_a_saved_plant_comes_back() {
        let names: Vec<String> = (0..FACTORY_SOUNDS)
            .filter_map(factory_sound)
            .map(|s| s.name)
            .collect();
        assert_eq!(names.len(), 232);
        // Every kind has its eight, each saying what it's like.
        for k in SeedKind::ALL {
            let of: Vec<BloomSound> = (0..FACTORY_SOUNDS)
                .filter_map(factory_sound)
                .filter(|s| s.kind == k)
                .collect();
            assert_eq!(of.len(), 8, "{k:?}");
            assert!(of.iter().all(|s| !s.about.is_empty()));
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "a name twice");
        let dir = crate::editors::poly::library::sounds_dir().join("Bloom");
        let mut s = factory_sound(3).unwrap();
        s.name = "Mine/Odd:Name".into();
        s.factory = false;
        s.made = 5;
        write_sound(&dir, &s).unwrap();
        let back = read_sounds(&dir);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].bloom, s.bloom);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
