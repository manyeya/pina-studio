//! The Bounce sheet (⌘B, or Export in the control bar), bouncing and exporting in one sheet:
//! what to bounce (the mix, each track as a stem, or each marked
//! section) over which range (the song, the cycle, any bars), on the left; how (formats, several
//! at once; bits, sample rate, the MP3's and AAC's bitrate; level: as it plays, to a peak, or to a
//! loudness under a ceiling; the tail; dither; stems through the master or not), on the right;
//! presets across the top; where it goes, what it'll be called and how much it makes, at the
//! foot. Bouncing shows each file's progress, then its loudness and peak, to play or show in the
//! Finder. Bounce in Place renders a track to audio on a track of its own, the original muted.

mod jobs;
mod view;

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use gpui::{
    Context, Div, Entity, FontWeight, MouseButton, PathPromptOptions, div, prelude::*, px, rgb,
};
use pina_engine::{
    audio::bounce::{self, Bits, Done, Format, Job, Level, Options},
    song::commands::{self, Command},
};

use crate::{
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::Workspace,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum What {
    Mix,
    Stems,
    Sections,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Span {
    Song,
    Cycle,
    Bars,
}

/// The formats, in the order they're offered, with their defaults.
const FORMATS: [Format; 7] = [
    Format::Wav,
    Format::Aiff,
    Format::Flac,
    Format::Mp3 { kbps: 320 },
    Format::Aac { kbps: 256 },
    Format::Alac,
    Format::Midi,
];

/// Loudness targets, as the places they're for want them.
const TARGETS: [(f32, &str); 4] = [
    (-9.0, "Club"),
    (-14.0, "Streaming"),
    (-16.0, "Apple"),
    (-23.0, "Broadcast"),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Preset {
    Master,
    Streaming,
    Cd,
    Stems,
    Demo,
    Midi,
}

impl Preset {
    const ALL: [Preset; 6] = [
        Preset::Master,
        Preset::Streaming,
        Preset::Cd,
        Preset::Stems,
        Preset::Demo,
        Preset::Midi,
    ];

    fn name(self) -> &'static str {
        match self {
            Preset::Master => "Master",
            Preset::Streaming => "Streaming",
            Preset::Cd => "CD",
            Preset::Stems => "Stems",
            Preset::Demo => "Demo MP3",
            Preset::Midi => "MIDI",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Preset::Master => "WAV 24-bit, as it plays",
            Preset::Streaming => "WAV + MP3, −14 LUFS",
            Preset::Cd => "WAV 16-bit 44.1, dithered",
            Preset::Stems => "Each track, 24-bit",
            Preset::Demo => "MP3 192, −14 LUFS",
            Preset::Midi => "The notes, a .mid",
        }
    }
}

/// What's come of each job, as it comes.
type Results = Vec<Option<Result<Done, String>>>;

/// A bounce running: each job's name and progress, and what's come of each.
pub struct Run {
    names: Vec<String>,
    progress: Arc<Mutex<Vec<f32>>>,
    results: Arc<Mutex<Results>>,
    cancel: Arc<AtomicBool>,
    pub finished: bool,
}

pub struct Export {
    pub what: What,
    pub span: Span,
    pub bars: (u64, u64),
    pub tracks: Vec<bool>,
    pub sections: Vec<bool>,
    pub formats: Vec<Format>,
    pub bits: Bits,
    pub rate: u32,
    pub level: Level,
    pub tail: bool,
    pub dither: bool,
    pub master_on_stems: bool,
    pub folder: PathBuf,
    pub name: Entity<TextInput>,
    pub run: Option<Run>,
    _subs: Vec<gpui::Subscription>,
}

/// Why a bounce came out quieter than its loudness target: its peaks met the ceiling first.
fn short_of(target: Option<(f32, f32)>, lufs: f32) -> Option<String> {
    let (want, ceiling) = target?;
    (lufs < want - 0.5).then(|| {
        format!("held at its {ceiling:.0} dB ceiling: a limiter on the master gets it louder")
    })
}

fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}
