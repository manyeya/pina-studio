//! Songs: every project in `~/Music/Pina Studio` (and those opened from elsewhere), each with a cover
//! drawn from its arrangement; to open in this window, start from a template, duplicate, rename,
//! show in the Finder, or move to the Trash (several at once, or the one open). With no song
//! open the Songs page is the whole window: where Pina Studio starts. `pina new` and `pina projects`
//! do the same from a terminal.

mod cards;
pub(crate) mod files;
pub(crate) mod opening;
mod page;
mod songs;

pub use cards::*;
pub use files::*;

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use gpui::{
    Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, Subscription, Window, div,
    prelude::*, px, rgb,
};
use pina_engine::{
    music::Key,
    music::look::{self, Family},
    song::project::{FILE, Instrument, Project, Track},
    song::session::Session,
};
use serde::Serialize;

use crate::{
    ui::looks::family_shade,
    ui::menu::{Menu, MenuAction},
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::{View, Workspace},
};

/// A song, as its card shows it.
#[derive(Clone, Debug, Serialize)]
pub struct Song {
    pub dir: PathBuf,
    pub name: String,
    #[serde(skip)]
    pub modified: SystemTime,
    pub tempo: f64,
    pub key: Option<String>,
    pub signature: [u64; 2],
    pub tracks: usize,
    pub seconds: f64,
    /// Its arrangement, small: each track's colour, and where its regions are (0 to 1 along).
    #[serde(skip)]
    pub lanes: Vec<(u32, Vec<(f32, f32)>)>,
}

/// What a new song starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    Empty,
    Beat,
    Band,
    LoFi,
    Electronic,
}

pub const TEMPLATES: [Template; 5] = [
    Template::Empty,
    Template::Beat,
    Template::Band,
    Template::LoFi,
    Template::Electronic,
];

impl Template {
    pub fn name(self) -> &'static str {
        match self {
            Template::Empty => "Empty",
            Template::Beat => "Beat",
            Template::Band => "Band",
            Template::LoFi => "Lo-fi",
            Template::Electronic => "Electronic",
        }
    }

    pub fn from_name(s: &str) -> Option<Template> {
        TEMPLATES.into_iter().find(|t| {
            t.name()
                .replace('-', "")
                .eq_ignore_ascii_case(&s.replace('-', ""))
        })
    }

    /// Its tracks: their names (which say how they look) and instruments.
    pub fn tracks(self) -> Vec<(&'static str, Option<Instrument>)> {
        let synth = || Some(Instrument::Synth);
        let drums = || Some(Instrument::Drums);
        match self {
            Template::Empty => vec![],
            Template::Beat => vec![
                ("Drums", drums()),
                ("808", synth()),
                ("Keys", synth()),
                ("Lead", synth()),
                ("Vocals", None),
            ],
            Template::Band => vec![
                ("Drums", drums()),
                ("Bass", synth()),
                ("Keys", synth()),
                ("Guitar", None),
                ("Vocals", None),
            ],
            Template::LoFi => vec![
                ("Vinyl", None),
                ("Drums", drums()),
                ("Keys", synth()),
                ("Bass", synth()),
            ],
            Template::Electronic => vec![
                ("Drums", drums()),
                ("Bass", synth()),
                ("Lead", synth()),
                ("Pad", synth()),
                ("FX", None),
            ],
        }
    }

    /// What it suits, for its card.
    pub fn about(self) -> &'static str {
        match self {
            Template::Empty => "Nothing yet",
            Template::Beat => "Drums, 808, keys, lead, vocals",
            Template::Band => "Drums, bass, keys, guitar, vocals",
            Template::LoFi => "Vinyl, drums, keys, bass",
            Template::Electronic => "Drums, bass, lead, pad, FX",
        }
    }
}

/// The order songs are listed in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Edited,
    Name,
    Tempo,
    Length,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Edited, Sort::Name, Sort::Tempo, Sort::Length];

    pub fn name(self) -> &'static str {
        match self {
            Sort::Edited => "Last edited",
            Sort::Name => "Name",
            Sort::Tempo => "Tempo",
            Sort::Length => "Length",
        }
    }
}

/// How fast the songs shown are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pace {
    #[default]
    Any,
    Slow,
    Mid,
    Fast,
}

impl Pace {
    pub const ALL: [Pace; 4] = [Pace::Any, Pace::Slow, Pace::Mid, Pace::Fast];

    pub fn name(self) -> &'static str {
        match self {
            Pace::Any => "Any",
            Pace::Slow => "Under 90",
            Pace::Mid => "90–120",
            Pace::Fast => "Over 120",
        }
    }

    pub fn holds(self, bpm: f64) -> bool {
        match self {
            Pace::Any => true,
            Pace::Slow => bpm < 90.0,
            Pace::Mid => (90.0..=120.0).contains(&bpm),
            Pace::Fast => bpm > 120.0,
        }
    }
}

/// A change to which songs the Songs page shows, and how.
#[derive(Clone, Debug)]
pub enum Shelf {
    Sort(Sort),
    Pace(Pace),
    Key(Option<String>),
    Time(Option<[u64; 2]>),
    /// Every filter off.
    Clear,
}

/// The order and filters the Songs page shows its songs with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shelving {
    pub sort: Sort,
    pub pace: Pace,
    pub key: Option<String>,
    pub time: Option<[u64; 2]>,
}

impl Shelving {
    pub fn filtered(&self) -> bool {
        self.pace != Pace::Any || self.key.is_some() || self.time.is_some()
    }

    pub fn change(&mut self, change: Shelf) {
        match change {
            Shelf::Sort(s) => self.sort = s,
            Shelf::Pace(p) => self.pace = p,
            Shelf::Key(k) => self.key = k,
            Shelf::Time(t) => self.time = t,
            Shelf::Clear => (self.pace, self.key, self.time) = (Pace::Any, None, None),
        }
    }

    /// The songs to show: those whose name has `search` (lowercase) in it, of the pace, key
    /// and time asked for, in the order asked for.
    pub fn shown(&self, songs: &[Song], search: &str) -> Vec<Song> {
        let mut out: Vec<Song> = songs
            .iter()
            .filter(|s| search.is_empty() || s.name.to_lowercase().contains(search))
            .filter(|s| self.pace.holds(s.tempo))
            .filter(|s| self.key.is_none() || s.key == self.key)
            .filter(|s| self.time.is_none_or(|t| s.signature == t))
            .cloned()
            .collect();
        match self.sort {
            Sort::Edited => out.sort_by_key(|s| std::cmp::Reverse(s.modified)),
            Sort::Name => out.sort_by_key(|s| s.name.to_lowercase()),
            Sort::Tempo => out.sort_by(|a, b| a.tempo.total_cmp(&b.tempo)),
            Sort::Length => out.sort_by(|a, b| b.seconds.total_cmp(&a.seconds)),
        }
        out
    }
}

/// Which stretch of time a song was last edited in, for the list's headings.
pub(crate) fn era(t: SystemTime) -> &'static str {
    match t.elapsed().map_or(0, |d| d.as_secs() / 86_400) {
        0 => "Today",
        1..7 => "This week",
        7..31 => "This month",
        _ => "Earlier",
    }
}

/// The Songs page's own: a new song's name, tempo, key and time (a template's click makes it),
/// the search, the order and filters, and the songs picked to act on together.
pub struct Home {
    pub name: Entity<TextInput>,
    pub tempo: Entity<TextInput>,
    pub key: Option<Key>,
    pub signature: [u64; 2],
    pub search: Entity<TextInput>,
    pub picks: Vec<PathBuf>,
    pub shelving: Shelving,
    _subs: Vec<Subscription>,
}

impl Home {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Home {
        let name = cx.new(|cx| TextInput::search("Untitled", cx));
        let tempo = cx.new(|cx| {
            let mut t = TextInput::field("", cx);
            t.set_text("120", cx);
            t
        });
        let search = cx.new(|cx| TextInput::search("Search songs", cx));
        let mut subs = Vec::new();
        // Enter in either: an empty song, at once.
        for field in [&name, &tempo] {
            subs.push(
                cx.subscribe_in(field, window, |ws, _, e: &TextEvent, window, cx| {
                    if let TextEvent::Confirm(_) = e {
                        ws.create_song(Template::Empty, window, cx);
                    }
                }),
            );
        }
        subs.push(cx.observe(&search, |_, _, cx| cx.notify()));
        subs.push(cx.subscribe(&search, |_, search, e: &TextEvent, cx| {
            if let TextEvent::Cancel = e {
                search.update(cx, |s, cx| s.set_text("", cx));
            }
        }));
        Home {
            name,
            tempo,
            key: None,
            signature: [4, 4],
            search,
            picks: Vec::new(),
            shelving: Shelving::default(),
            _subs: subs,
        }
    }
}

/// A song being renamed on its card.
pub struct Renaming {
    pub dir: PathBuf,
    pub field: Entity<TextInput>,
    _done: Subscription,
}

#[cfg(test)]
mod tests;
