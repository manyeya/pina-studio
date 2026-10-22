//! Songs: every project in `~/Music/Pina Studio` (and those opened from elsewhere), each with a cover
//! drawn from its arrangement; to open in this window, start from a template, duplicate, rename,
//! show in the Finder, or move to the Trash (several at once, or the one open). With no song
//! open the Songs page is the whole window: where Pina Studio starts. `pina new` and `pina projects`
//! do the same from a terminal.

mod cards;
pub(crate) mod files;
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
    ui::looks::{family_shade, glyph},
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

/// The Songs page's own: a new song's name, tempo, key and time (a template's click makes it),
/// the search, and the songs picked to act on together.
pub struct Home {
    pub name: Entity<TextInput>,
    pub tempo: Entity<TextInput>,
    pub key: Option<Key>,
    pub signature: [u64; 2],
    pub search: Entity<TextInput>,
    pub picks: Vec<PathBuf>,
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
