//! The Song Study page: a song or a loop, from its file, taken apart, on lanes (`lanes.rs`). What
//! it is decides what's shown: a loop has no form, drums no key. Its identity and vitals over
//! its shape (a bar a stroke, coloured by section), its form (the same colour for the same
//! music), its harmony on the circle of fifths, its groove as steps, what plays in it, its mix and
//! dynamics, its lyrics with their rhymes and hooks, and the playbooks it's nearest; its blueprint
//! (what a song like it is built from, each finding measured, estimated or unknown). From it, a
//! playbook, a mix reference, a new song like it for the agent to write (then compared with it,
//! and revised as asked), or the agent's reading of what it means. A picture of a spectrogram
//! studies too, as far as a picture can say.

mod genres_view;
mod hero;
mod lanes;
mod like;
mod like_view;
mod mix_view;
mod music_view;
mod page;
mod shelf;
mod words_view;

pub(crate) use hero::*;
pub(crate) use lanes::{Dock, Only, Pick, lane, origin, rect, title_of};
#[cfg(test)]
pub(crate) use like::Revision;
pub(crate) use like_view::act;

/// How many passes a revision takes, at most.
#[cfg(test)]
pub(crate) fn like_passes() -> u8 {
    like::PASSES
}
pub(crate) use mix_view::*;
pub(crate) use music_view::*;

use std::path::{Path, PathBuf};

use gpui::{
    App, Bounds, Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, PathPromptOptions,
    Pixels, ScrollHandle, Stateful, Window, canvas, div, fill, point, prelude::*, px, rgb,
};
use pina_engine::{
    music::Key,
    music::playbook::{self, Playbook},
    music::theory,
    song::project::Reference,
    study::{self, Study},
};

use crate::{
    cli,
    pages::genres::{
        BANDS, FLOOR, capital, field_box, genre_color, heading, label, role_color, swing_dots, tile,
    },
    ui::{text::TextInput, theme::*},
    workspace::{View, Workspace},
};

#[derive(Default)]
pub struct Studies {
    pub scroll: ScrollHandle,
    /// What's read out at the side, and what's open under the lanes.
    pub pick: Pick,
    pub dock: Dock,
    /// The rail's search and what's typed in it, and which studies it shows.
    pub search: Option<Entity<TextInput>>,
    typed: Option<gpui::Subscription>,
    pub query: String,
    pub only: Only,
    /// Spectrum pictures by file, and those being painted.
    pub spectrograms: std::collections::HashMap<PathBuf, PathBuf>,
    painting: Vec<PathBuf>,
    /// The open study's audio, read once; the file being read.
    audio: Option<(
        PathBuf,
        std::sync::Arc<pina_engine::audio::engine::AudioData>,
    )>,
    reading: Option<PathBuf>,
    /// Playing, since when and from where in it; else where the playhead rests.
    pub playing: Option<(std::time::Instant, f32)>,
    pub at: f32,
    /// Where the lanes' tracks were drawn, for clicks on them.
    track: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// Every song studied, the latest first.
    pub list: Vec<Study>,
    /// The study shown, by its file.
    pub open: Option<PathBuf>,
    /// Songs an older Pina Studio studied, still to study again.
    pub stale: Vec<PathBuf>,
    /// Files being studied now.
    pub busy: Vec<PathBuf>,
    /// The playbooks, for their mix targets.
    books: Vec<Playbook>,
    /// References: pictures studied, blueprints, songs made like them and compared.
    pub like: like::Likes,
}

/// A section's colour, by its letter: the same music, the same colour.
fn letter_color(label: &str) -> u32 {
    let palette = [SKY, CORAL, MINT, VIOLET, AMBER, PINK, TEAL, SLATE];
    let k = label
        .bytes()
        .next()
        .map_or(0, |b| b.wrapping_sub(b'A') as usize);
    palette[k % palette.len()]
}

/// A rhyme's colour, by its letter.
fn rhyme_color(c: char) -> u32 {
    let palette = [AMBER, SKY, PINK, MINT, VIOLET, CORAL, TEAL];
    palette[(c as u8).wrapping_sub(b'A') as usize % palette.len()]
}

fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The study's page colour: its loudest section's.
fn tint_of(s: &Study) -> u32 {
    s.sections
        .iter()
        .max_by(|a, b| a.energy.total_cmp(&b.energy))
        .map_or(ACCENT, |x| letter_color(&x.label))
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned())
}
