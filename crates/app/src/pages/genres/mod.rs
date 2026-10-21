//! The Playbooks page: every genre Pina Studio's agents and critic know, to read and to change. A genre
//! shows as its song's shape (a landscape of its sections' energy), its feel, its song map (which
//! part plays in which section, toggled in a grid), its parts, its mix target and its notes;
//! every change is saved to the person's playbook file as it's made.

mod editing;
mod feel;
mod hero;
mod landscape_view;
mod mix_view;
mod page;
mod sections;
mod widgets;

pub(crate) use editing::*;
pub(crate) use feel::*;
pub(crate) use landscape_view::*;
pub(crate) use widgets::*;

use gpui::{
    Bounds, Context, Div, Entity, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels,
    ScrollHandle, Stateful, Subscription, Window, canvas, div, point, prelude::*, px, rgb,
};
use pina_engine::{
    music::Key,
    music::playbook::{self, Origin, Part, Playbook, Section},
    music::theory,
};

use crate::{
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::{View, Workspace},
};

/// The parts a genre can have, in the order they're shown.
const ROLES: [&str; 11] = [
    "drums", "perc", "bass", "chords", "pad", "lead", "counter", "arp", "vocals", "fx", "texture",
];

/// What the spectrum's six bands are called.
pub(crate) const BANDS: [(&str, &str); 6] = [
    ("Sub", "< 60"),
    ("Bass", "60–250"),
    ("Low mid", "250–1k"),
    ("Mid", "1–4k"),
    ("Presence", "4–8k"),
    ("Air", "> 8k"),
];

/// The floor of the spectrum editor, dB of the total.
pub(crate) const FLOOR: f32 = -32.0;

/// A genre's color: Pina Studio's have their own, the person's one from the palette by name.
pub fn genre_color(id: &str) -> u32 {
    match id {
        "lofi" => 0xffb86b,
        "boom_bap" => 0xff7a59,
        "trap" => 0xa78bfa,
        "drill" => 0x7dd3fc,
        "rnb" => 0xf472b6,
        "neo_soul" => 0xfb923c,
        "pop" => 0x38bdf8,
        "house" => 0x2dd4bf,
        "deep_house" => 0x5eead4,
        "techno" => 0xcbd5e1,
        "afrobeats" => 0xfacc15,
        "amapiano" => 0x3ddc97,
        "dnb" => 0x60a5fa,
        "reggaeton" => 0xf87171,
        "edm" => 0xc084fc,
        "cinematic" => 0xe8c27a,
        "ambient" => 0x93c5fd,
        other => {
            let palette = [CORAL, AMBER, MINT, SKY, VIOLET, PINK, TEAL];
            let h = other
                .bytes()
                .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
            palette[h as usize % palette.len()]
        }
    }
}

/// A part's color, the same everywhere on the page.
pub(crate) fn role_color(role: &str) -> u32 {
    match role {
        "drums" => CORAL,
        "perc" => AMBER,
        "bass" => VIOLET,
        "chords" => TEAL,
        "pad" => SKY,
        "lead" => PINK,
        "counter" => 0xfb923c,
        "arp" => MINT,
        "vocals" => 0xf9a8d4,
        "fx" => SLATE,
        _ => 0x8b8b96,
    }
}

#[derive(Default)]
pub struct Genres {
    pub scroll: ScrollHandle,
    pub list: Vec<Playbook>,
    pub problems: Vec<String>,
    /// The genre being edited, as it's being changed.
    pub draft: Option<Playbook>,
    /// The section picked in the song map.
    pub section: Option<usize>,
    fields: Option<Fields>,
}
