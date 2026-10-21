//! The Song Study page: a finished song, from its file, taken apart. Its identity and vitals over
//! its shape (a bar a stroke, coloured by section), its form (the same colour for the same
//! music), its harmony on the circle of fifths, its groove as steps, what plays in it, its mix and
//! dynamics, its lyrics with their rhymes and hooks, and the playbooks it's nearest. From it, a
//! playbook, a mix reference, a new song like it, or the agent's reading of what it means.

mod form_view;
mod genres_view;
mod hero;
mod mix_view;
mod music_view;
mod page;
mod shelf;
mod words_view;

pub(crate) use hero::*;
pub(crate) use mix_view::*;
pub(crate) use music_view::*;

use std::path::PathBuf;

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, PathPromptOptions, Pixels, ScrollHandle,
    Stateful, Window, canvas, div, point, prelude::*, px, rgb,
};
use pina_engine::{
    music::Key,
    music::playbook::{self, Playbook},
    music::theory,
    song::project::{Project, Reference},
    study::{self, Study},
};

use crate::{
    cli,
    pages::genres::{
        BANDS, FLOOR, capital, genre_color, heading, label, role_color, swing_dots, tile,
    },
    ui::theme::*,
    workspace::{View, Workspace},
};

#[derive(Default)]
pub struct Studies {
    pub scroll: ScrollHandle,
    /// Every song studied, the latest first.
    pub list: Vec<Study>,
    /// The study shown, by its file.
    pub open: Option<PathBuf>,
    /// Files being studied now.
    pub busy: Vec<PathBuf>,
    /// The section picked in the form.
    pub section: Option<usize>,
    /// The playbooks, for their mix targets.
    books: Vec<Playbook>,
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
