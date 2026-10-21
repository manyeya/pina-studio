//! The New Track palette, under the "+" over the track headers (and ⌥⌘N): one search across
//! everything a track can start as, with the kinds of track as tiles
//! when nothing's typed. Instruments (Pina Studio's and every plugin installed), their presets (their own
//! programs and preset files, as the sounds tool reads them), kits made from the sample library's
//! packs, and single sounds (an 808, a bass note) played across the keys on a sampler. Arrows pick,
//! Enter adds, Escape closes; a star keeps an instrument among the favourites, ▶ plays a sound.

use std::collections::HashMap;

use gpui::{
    Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, Subscription,
    Window, anchored, deferred, div, prelude::*, px, rgb,
};
use pina_engine::{
    samples::{
        Category, Kind,
        place::place_zone,
        search::{self, KitSeed, Query},
    },
    song::commands::{self, Command},
    song::project::{Instrument, Preset, Sampler},
};

use crate::{
    sounds::catalog::{Cat, Target, score},
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::{Bottom, Workspace},
};

/// The kinds of track, as tiles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind2 {
    Instrument,
    Drums,
    Sampler,
    Audio,
    Beat,
}

impl Kind2 {
    const ALL: [Kind2; 5] = [
        Kind2::Instrument,
        Kind2::Drums,
        Kind2::Sampler,
        Kind2::Audio,
        Kind2::Beat,
    ];

    fn name(self) -> &'static str {
        match self {
            Kind2::Instrument => "Instrument",
            Kind2::Drums => "Drums",
            Kind2::Sampler => "Sampler",
            Kind2::Audio => "Audio",
            Kind2::Beat => "Beat",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Kind2::Instrument => "Synths, plugins",
            Kind2::Drums => "Kits, machines",
            Kind2::Sampler => "Samples on keys",
            Kind2::Audio => "Record, import",
            Kind2::Beat => "Kit + steps",
        }
    }

    fn color(self) -> u32 {
        match self {
            Kind2::Instrument => SKY,
            Kind2::Drums => CORAL,
            Kind2::Sampler => VIOLET,
            Kind2::Audio => MINT,
            Kind2::Beat => AMBER,
        }
    }

    /// Words that find it in a search.
    fn words(self) -> &'static [&'static str] {
        match self {
            Kind2::Instrument => &["instrument", "software", "midi", "synth"],
            Kind2::Drums => &["drums", "drum", "kit", "percussion"],
            Kind2::Sampler => &["sampler", "sample"],
            Kind2::Audio => &["audio", "record", "vocal", "voice", "guitar", "mic"],
            Kind2::Beat => &["beat", "pattern", "steps", "step", "sequencer"],
        }
    }
}

/// A line of the palette.
#[derive(Clone, Debug)]
enum Item {
    Heading(String),
    Kind(Kind2),
    Device(usize),
    Preset(usize, Preset),
    /// A kit made from a pack in the library.
    Kit(String, usize),
    /// A sample in the library, on a sampler.
    Sound(usize),
}

pub struct Adder {
    pub at: Point<Pixels>,
    pub search: Entity<TextInput>,
    /// A tile picked: the list narrowed to it.
    only: Option<Kind2>,
    /// The row the arrows are on, among those that can be added.
    picked: usize,
    _subs: Vec<Subscription>,
}

/// What the palette knows that takes a while to find: instruments' presets (by device key), and
/// the library's packs with drums enough for a kit.
#[derive(Default)]
pub struct Sounds {
    presets: HashMap<String, Vec<Preset>>,
    asked: bool,
    reading: usize,
}

/// A sample's pack: the first folder under its library folder.
fn pack_of(folder: &str) -> &str {
    folder.split('/').next().unwrap_or(folder)
}

impl Workspace {
    /// Opens the palette at `at`, its search taking the keyboard.
    pub fn open_adder(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let search =
            cx.new(|cx| TextInput::search("Search instruments, presets, kits, sounds…", cx));
        let typed = cx.observe(&search, |ws, _, cx| {
            if let Some(a) = ws.adder.as_mut() {
                a.picked = 0;
            }
            cx.notify();
        });
        let done = cx.subscribe_in(
            &search,
            window,
            |ws, _, e: &TextEvent, window, cx| match e {
                TextEvent::Confirm(_) => ws.add_picked(window, cx),
                TextEvent::Cancel => ws.close_adder(cx),
            },
        );
        let focus = gpui::Focusable::focus_handle(search.read(cx), cx);
        focus.focus(window, cx);
        self.adder = Some(Adder {
            at,
            search,
            only: None,
            picked: 0,
            _subs: vec![typed, done],
        });
        self.read_presets(cx);
        cx.notify();
    }

    pub fn close_adder(&mut self, cx: &mut Context<Self>) {
        self.adder = None;
        self.refocus = true;
        cx.notify();
    }

    /// Instruments' presets, read off the UI thread: those kept from before at once, then (once
    /// a session) the rest asked of the instruments themselves.
    fn read_presets(&mut self, cx: &mut Context<Self>) {
        if self.sounds.asked || cfg!(test) {
            return;
        }
        self.sounds.asked = true;
        let wanted: Vec<(String, pina_engine::plugin::scan::PluginInfo)> = self
            .catalog
            .devices
            .iter()
            .filter(|d| d.instrument() && d.own.is_none())
            .filter_map(|d| Some((d.key.clone(), d.formats.first()?.clone())))
            .collect();
        if wanted.is_empty() {
            return;
        }
        self.sounds.reading = wanted.len();
        let job = cx.background_spawn(async move {
            let plugins: Vec<_> = wanted.iter().map(|w| w.1.clone()).collect();
            let kept = crate::agent::produce::presets_of(&plugins, false);
            (wanted, kept)
        });
        cx.spawn(async move |this, cx| {
            let (wanted, kept) = job.await;
            let missing: Vec<(String, pina_engine::plugin::scan::PluginInfo)> = wanted
                .iter()
                .zip(&kept)
                .filter(|(_, k)| k.is_empty())
                .map(|(w, _)| w.clone())
                .collect();
            this.update(cx, |ws, cx| {
                for ((key, _), list) in wanted.iter().zip(kept) {
                    ws.sounds.presets.insert(key.clone(), list);
                }
                ws.sounds.reading = missing.len();
                cx.notify();
            })
            .ok();
            if missing.is_empty() {
                return;
            }
            let asked = cx
                .background_spawn(async move {
                    let plugins: Vec<_> = missing.iter().map(|w| w.1.clone()).collect();
                    let lists = crate::agent::produce::presets_of(&plugins, true);
                    missing
                        .into_iter()
                        .map(|w| w.0)
                        .zip(lists)
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |ws, cx| {
                for (key, list) in asked {
                    ws.sounds.presets.insert(key, list);
                }
                ws.sounds.reading = 0;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The presets kept from before, read now (for the shots test, which has no background).
    #[cfg(test)]
    pub fn presets_now(&mut self) {
        for d in self
            .catalog
            .devices
            .iter()
            .filter(|d| d.instrument() && d.own.is_none())
        {
            if let Some(p) = d.formats.first() {
                let list = crate::agent::produce::presets_of(std::slice::from_ref(p), false)
                    .pop()
                    .unwrap_or_default();
                self.sounds.presets.insert(d.key.clone(), list);
            }
        }
    }

    /// The packs in the library with drums enough for a kit (four hits or more), and how many.
    fn packs(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for s in &self.browser.library.samples {
            if s.analysis.kind == Kind::OneShot && s.analysis.category.drum_note().is_some() {
                *counts.entry(pack_of(&s.folder)).or_default() += 1;
            }
        }
        let mut packs: Vec<(String, usize)> = counts
            .into_iter()
            .filter(|(_, n)| *n >= 4)
            .map(|(p, n)| (p.to_string(), n))
            .collect();
        packs.sort_by_key(|p| p.0.to_lowercase());
        packs
    }

    fn adder_query(&self, cx: &gpui::App) -> String {
        self.adder.as_ref().map_or(String::new(), |a| {
            a.search.read(cx).text().trim().to_lowercase()
        })
    }

    /// What the palette lists, as typed (or a tile picked).
    fn adder_items(&self, cx: &gpui::App) -> Vec<Item> {
        let Some(a) = &self.adder else {
            return Vec::new();
        };
        let query = self.adder_query(cx);
        let c = &self.catalog;
        let instruments: Vec<usize> = (0..c.devices.len())
            .filter(|&i| c.devices[i].instrument())
            .collect();
        let mut out = Vec::new();
        let section = |out: &mut Vec<Item>, title: String, items: Vec<Item>| {
            if !items.is_empty() {
                out.push(Item::Heading(title));
                out.extend(items);
            }
        };
        let by_cat = |out: &mut Vec<Item>, cats: &[Cat]| {
            for &cat in cats {
                let mut list: Vec<usize> = instruments
                    .iter()
                    .copied()
                    .filter(|&i| c.devices[i].cat == cat)
                    .collect();
                list.sort_by_key(|&i| {
                    (c.devices[i].own.is_none(), c.devices[i].name.to_lowercase())
                });
                section(
                    out,
                    cat.name().to_string(),
                    list.into_iter().map(Item::Device).collect(),
                );
            }
        };
        let kits = |out: &mut Vec<Item>, words: &[&str]| {
            let found: Vec<Item> = self
                .packs()
                .into_iter()
                .filter(|(p, _)| words.iter().all(|w| p.to_lowercase().contains(w)))
                .take(if words.is_empty() { 40 } else { 6 })
                .map(|(p, n)| Item::Kit(p, n))
                .collect();
            section(out, "Kits from your library".into(), found);
        };
        if !query.is_empty() {
            let words: Vec<&str> = query.split_whitespace().collect();
            // Kinds of track the words name.
            let kinds: Vec<Item> = Kind2::ALL
                .into_iter()
                .filter(|k| {
                    words
                        .iter()
                        .any(|w| k.words().iter().any(|kw| kw.starts_with(w)))
                })
                .map(Item::Kind)
                .collect();
            section(&mut out, "New track".into(), kinds);
            let mut found: Vec<(u32, usize)> = instruments
                .iter()
                .filter_map(|&i| {
                    score(&c.devices[i], &query).map(|s| {
                        (
                            s + if c.favourites.contains(&c.devices[i].key) {
                                3
                            } else {
                                0
                            },
                            i,
                        )
                    })
                })
                .collect();
            found.sort_by(|x, y| {
                y.0.cmp(&x.0)
                    .then(c.devices[x.1].name.cmp(&c.devices[y.1].name))
            });
            section(
                &mut out,
                "Instruments".into(),
                found
                    .into_iter()
                    .take(8)
                    .map(|(_, i)| Item::Device(i))
                    .collect(),
            );
            // Presets whose name (or instrument and name) has every word.
            let mut presets = Vec::new();
            for &i in &instruments {
                let d = &c.devices[i];
                let Some(list) = self.sounds.presets.get(&d.key) else {
                    continue;
                };
                let dev = d.name.to_lowercase();
                for p in list {
                    let name = p.name.to_lowercase();
                    if words.iter().all(|w| name.contains(w) || dev.contains(w))
                        && words.iter().any(|w| name.contains(w))
                    {
                        presets.push(Item::Preset(i, p.clone()));
                        if presets.len() >= 14 {
                            break;
                        }
                    }
                }
            }
            section(&mut out, "Presets".into(), presets);
            kits(&mut out, &words);
            // Single sounds, on a sampler across the keys (or on a note, a hit).
            let q = Query {
                text: query.clone(),
                kind: Some(Kind::OneShot),
                limit: 6,
                ..Default::default()
            };
            let sounds: Vec<Item> = search::find(&self.browser.library.samples, &q)
                .into_iter()
                .map(|h| Item::Sound(h.index))
                .collect();
            section(&mut out, "Sounds, on a sampler".into(), sounds);
            return out;
        }
        match a.only {
            Some(Kind2::Instrument) => by_cat(
                &mut out,
                &[
                    Cat::Synth,
                    Cat::Keys,
                    Cat::Orchestral,
                    Cat::Sampler,
                    Cat::OtherInstrument,
                ],
            ),
            Some(Kind2::Drums) => {
                by_cat(&mut out, &[Cat::Drums]);
                kits(&mut out, &[]);
            }
            _ => {
                let keyed = |keys: &[String]| -> Vec<Item> {
                    keys.iter()
                        .filter_map(|k| c.device(k))
                        .filter(|&i| c.devices[i].instrument())
                        .take(5)
                        .map(Item::Device)
                        .collect()
                };
                section(&mut out, "Used lately".into(), keyed(&c.recent));
                section(&mut out, "Favourites".into(), keyed(&c.favourites));
                by_cat(
                    &mut out,
                    &[
                        Cat::Synth,
                        Cat::Keys,
                        Cat::Drums,
                        Cat::Orchestral,
                        Cat::Sampler,
                        Cat::OtherInstrument,
                    ],
                );
            }
        }
        out
    }

    /// Moves the arrows' row.
    pub fn adder_move(&mut self, by: isize, cx: &mut Context<Self>) {
        let n = self
            .adder_items(cx)
            .iter()
            .filter(|i| !matches!(i, Item::Heading(_)))
            .count();
        if let Some(a) = self.adder.as_mut()
            && n > 0
        {
            a.picked = (a.picked as isize + by).rem_euclid(n as isize) as usize;
        }
        cx.notify();
    }

    fn add_picked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picked) = self.adder.as_ref().map(|a| a.picked) else {
            return;
        };
        let items: Vec<Item> = self
            .adder_items(cx)
            .into_iter()
            .filter(|i| !matches!(i, Item::Heading(_)))
            .collect();
        if let Some(item) = items.get(picked).cloned() {
            self.add_item(item, window, cx);
        }
    }

    fn add_item(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        match item {
            Item::Heading(_) => return,
            Item::Kind(k @ (Kind2::Instrument | Kind2::Drums)) => {
                // A tile with choices narrows the list to them.
                if let Some(a) = self.adder.as_mut() {
                    a.only = (a.only != Some(k)).then_some(k);
                    a.picked = 0;
                    a.search.update(cx, |s, cx| s.set_text("", cx));
                }
                let _ = window;
                cx.notify();
                return;
            }
            Item::Kind(Kind2::Audio) => self.new_track(None, cx),
            Item::Kind(Kind2::Sampler) => {
                self.new_sampler_track(cx);
                self.show_bottom(Bottom::Sampler, cx);
            }
            Item::Kind(Kind2::Beat) => {
                self.new_track(Some(Instrument::Drums), cx);
                self.new_pattern(cx);
            }
            Item::Device(d) => self.use_device(d, Target::NewTrack, None, cx),
            Item::Preset(d, p) => self.use_device(d, Target::NewTrack, Some(p), cx),
            Item::Kit(pack, _) => self.kit_from_pack(&pack, cx),
            Item::Sound(i) => self.sound_on_sampler(i, cx),
        }
        self.close_adder(cx);
    }

    /// A kit from one pack in the library: a hit for each drum, on its General MIDI note.
    fn kit_from_pack(&mut self, pack: &str, cx: &mut Context<Self>) {
        let samples: Vec<pina_engine::samples::Sample> = self
            .browser
            .library
            .samples
            .iter()
            .filter(|s| pack_of(&s.folder) == pack)
            .cloned()
            .collect();
        let kit = search::kit(
            &samples,
            &KitSeed {
                alternatives: 1,
                ..Default::default()
            },
        );
        let mut sampler = Sampler::default();
        for slot in kit.slots {
            if let Some(hit) = slot.pick
                && let Some(s) = samples.get(hit.index)
            {
                let _ = place_zone(&mut sampler, s, Some(slot.role));
            }
        }
        if sampler.zones.is_empty() {
            self.status = Some(format!(
                "{pack} has no drums Pina Studio could make a kit of."
            ));
            cx.notify();
            return;
        }
        let n = sampler.zones.len();
        self.add_sampler_track(format!("{pack} Kit"), sampler, cx);
        self.status = Some(format!(
            "A {n}-piece kit from {pack}: play it, or ⌥⌘P for a pattern."
        ));
    }

    /// A single sound on a sampler: across the keys if it has a pitch, else on its drum's note.
    fn sound_on_sampler(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(s) = self.browser.library.samples.get(i).cloned() else {
            return;
        };
        let mut sampler = Sampler::default();
        let role = (s.analysis.category != Category::Other).then_some(s.analysis.category);
        if let Err(e) = place_zone(&mut sampler, &s, role) {
            self.status = Some(e);
            cx.notify();
            return;
        }
        self.add_sampler_track(s.name.clone(), sampler, cx);
        self.show_bottom(Bottom::Sampler, cx);
    }

    fn add_sampler_track(&mut self, name: String, sampler: Sampler, cx: &mut Context<Self>) {
        let n = self.session.project.tracks.len();
        let index = if n == 0 { 0 } else { (self.track + 1).min(n) };
        let dir = self.session.dir.clone();
        self.session.checkpoint();
        let add = Command::AddTrack {
            name,
            instrument: Some(Instrument::Sampler(sampler)),
            index: Some(index),
        };
        match commands::apply(&mut self.session.project, &dir, add) {
            Ok(_) => {
                self.select_track(index);
                self.region = None;
                self.changed(cx);
            }
            Err(e) => {
                self.session.cancel();
                self.status = Some(e);
                cx.notify();
            }
        }
    }

    /// The palette, if it's open.
    pub fn adder_view(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let a = self.adder.as_ref()?;
        let (at, only, picked, search) = (a.at, a.only, a.picked, a.search.clone());
        let query = self.adder_query(cx);
        let items = self.adder_items(cx);
        let tiles = Kind2::ALL.map(|k| {
            let on = only == Some(k);
            let c = k.color();
            div()
                .flex_1()
                .min_w(px(0.))
                .h(px(84.))
                .p(px(10.))
                .flex()
                .flex_col()
                .justify_between()
                .rounded(px(11.))
                .border_1()
                .border_color(if on { alpha(c, 0.8) } else { rgb(LINE).into() })
                .bg(if on {
                    alpha(c, 0.12)
                } else {
                    rgb(0x1e1e24).into()
                })
                .cursor_pointer()
                .hover(|d| d.border_color(alpha(c, 0.6)).bg(alpha(c, 0.08)))
                .debug_selector(move || format!("adder-{}", k.name()))
                .child(
                    div()
                        .size(px(26.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(7.))
                        .bg(alpha(c, 0.16))
                        .child(kind_glyph(k, 15.).text_color(rgb(c))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(k.name()),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(TEXT_3))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(k.about()),
                        ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, window, cx| {
                        cx.stop_propagation();
                        ws.add_item(Item::Kind(k), window, cx);
                    }),
                )
        });
        let mut n = 0usize;
        let rows: Vec<Div> = items
            .into_iter()
            .map(|item| {
                if let Item::Heading(h) = &item {
                    return div()
                        .pt(px(10.))
                        .pb(px(4.))
                        .px(px(8.))
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_3))
                        .child(h.to_uppercase());
                }
                let index = n;
                n += 1;
                self.adder_row(item, index == picked, index, cx)
            })
            .collect();
        let empty = rows.is_empty();
        let reading = self.sounds.reading;
        let panel = div()
            .id("adder")
            .w(px(580.))
            .rounded(px(14.))
            .bg(rgb(0x17171c))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .flex()
            .flex_col()
            .overflow_hidden()
            .debug_selector(|| "adder".into())
            .key_context("Adder")
            .on_action(cx.listener(|ws, _: &crate::workspace::AdderUp, _, cx| ws.adder_move(-1, cx)))
            .on_action(cx.listener(|ws, _: &crate::workspace::AdderDown, _, cx| ws.adder_move(1, cx)))
            .child(
                div()
                    .h(px(46.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(icon(Icon::Search, 15.).text_color(rgb(TEXT_3)))
                    .child(div().flex_1().text_size(px(14.)).child(search))
                    .child(div().px(px(6.)).py(px(2.)).rounded(px(5.)).bg(rgb(RAISED)).font_family(MONO).text_size(px(10.)).text_color(rgb(TEXT_3)).child("esc")),
            )
            .when(query.is_empty(), |d| d.child(div().p(px(12.)).pb(px(4.)).flex().gap(px(8.)).children(tiles)))
            .child(
                div()
                    .id("adder-list")
                    .max_h(px(360.))
                    .overflow_y_scroll()
                    .px(px(6.))
                    .pb(px(8.))
                    .children(rows)
                    .when(empty, |d| d.child(div().p(px(18.)).text_size(px(12.5)).text_color(rgb(TEXT_3)).child(format!("Nothing called “{query}”. Try a sound (“808”, “warm pad”) or a kind of track (“audio”)."))))
                    .on_mouse_down(MouseButton::Left, |_, _, _| {}),
            )
            .child(
                div()
                    .h(px(30.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child("↑↓ choose")
                    .child("↵ add a track")
                    .child("★ keep")
                    .child(div().flex_1())
                    .when(reading > 0, |d| d.child(div().text_color(rgb(TEXT_3)).child(format!("Reading {reading} instruments' presets…")))),
            )
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| ws.close_adder(cx)));
        Some(deferred(anchored().position(at).snap_to_window().child(panel)).with_priority(2))
    }

    fn adder_row(&self, item: Item, on: bool, index: usize, cx: &mut Context<Self>) -> Div {
        let (color, glyph, title, sub, key): (
            u32,
            gpui::AnyElement,
            String,
            String,
            Option<String>,
        ) = match &item {
            Item::Kind(k) => (
                k.color(),
                kind_glyph(*k, 15.)
                    .text_color(rgb(k.color()))
                    .into_any_element(),
                format!("New {} track", k.name().to_lowercase()),
                k.about().into(),
                None,
            ),
            Item::Device(d) => {
                let dev = &self.catalog.devices[*d];
                let sub = match (&dev.own, dev.badges().as_slice()) {
                    (Some(_), _) => format!("Pina Studio · {}", dev.cat.name()),
                    (None, b) => format!("{} · {}", dev.vendor, b.join(" ")),
                };
                (
                    dev.cat.color(),
                    dev.cat
                        .glyph(15.)
                        .text_color(rgb(dev.cat.color()))
                        .into_any_element(),
                    dev.name.clone(),
                    sub,
                    Some(dev.key.clone()),
                )
            }
            Item::Preset(d, p) => {
                let dev = &self.catalog.devices[*d];
                (
                    dev.cat.color(),
                    icon(Icon::Keys, 14.)
                        .text_color(rgb(dev.cat.color()))
                        .into_any_element(),
                    p.name.clone(),
                    format!("Preset · {}", dev.name),
                    None,
                )
            }
            Item::Kit(pack, count) => (
                CORAL,
                Cat::Drums
                    .glyph(15.)
                    .text_color(rgb(CORAL))
                    .into_any_element(),
                format!("{pack} Kit"),
                format!("A kit of its {count} hits, on a sampler"),
                None,
            ),
            Item::Sound(i) => {
                let s = &self.browser.library.samples[*i];
                let how = match s.analysis.pitch.as_ref() {
                    Some(_) => "across the keys",
                    None => "on a pad",
                };
                (
                    VIOLET,
                    icon(Icon::Wave, 14.)
                        .text_color(rgb(VIOLET))
                        .into_any_element(),
                    s.name.clone(),
                    format!("{} · {how}", pack_of(&s.folder)),
                    None,
                )
            }
            Item::Heading(_) => unreachable!(),
        };
        let fav = key
            .as_ref()
            .is_some_and(|k| self.catalog.favourites.contains(k));
        let sound = match &item {
            Item::Sound(i) => Some(*i),
            _ => None,
        };
        let item2 = item.clone();
        div()
            .h(px(40.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(8.))
            .cursor_pointer()
            .when(on, |d| d.bg(alpha(ACCENT, 0.18)))
            .when(!on, |d| d.hover(|d| d.bg(rgb(OFF))))
            .debug_selector(move || format!("adder-row-{index}"))
            .child(
                div()
                    .size(px(28.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(alpha(color, 0.14))
                    .text_color(rgb(color))
                    .child(glyph),
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
                            .child(title),
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
            .when_some(sound, |d, i| {
                d.child(
                    div()
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .hover(|d| d.bg(rgb(RAISED)))
                        .child(icon(Icon::Play, 11.).text_color(rgb(TEXT_2)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.audition_sample(i, cx);
                            }),
                        ),
                )
            })
            .when_some(key, |d, k| {
                d.child(
                    div()
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .text_size(px(13.))
                        .text_color(rgb(if fav { AMBER } else { TEXT_4 }))
                        .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(AMBER)))
                        .child(if fav { "★" } else { "☆" })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.toggle_favourite(&k, cx);
                            }),
                        ),
                )
            })
            .when(on, |d| {
                d.child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .child("↵"),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ws.add_item(item2.clone(), window, cx);
                }),
            )
    }
}

/// A kind of track's glyph.
fn kind_glyph(k: Kind2, size: f32) -> gpui::Svg {
    match k {
        Kind2::Instrument => Cat::Synth.glyph(size),
        Kind2::Drums => Cat::Drums.glyph(size),
        Kind2::Sampler => Cat::Sampler.glyph(size),
        Kind2::Audio => icon(Icon::Wave, size),
        Kind2::Beat => icon(Icon::Grid, size),
    }
}

#[cfg(test)]
mod tests;
