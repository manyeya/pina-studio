//! The step sequencer: a pattern region's rows of steps, each row a sound (a drum, a note, a
//! melody) or a parameter (automation), with every step's settings a click or a drag away.
//!
//! Across the top, the menu bar: Edit, Functions and View; the pattern browser and inspector;
//! MIDI in (step input), MIDI out (hearing what's edited), mono; and the edit mode, On/Off beside
//! the last value mode picked (Velocity / Value, Gate, Tie, Note, Octave, Loop, Repeat, Chance,
//! Offset, Step Rate, Skip); then live recording, preview, and the zoom. Under it, the pattern's
//! controls (add a row, its rate and playback mode, rotate, down and up, randomize, clear) beside
//! the grid's header (the region's name, an overview of each page, the key and scale, the length).
//! Each row's header has its subrows' arrow, its icon (click to hear it), its assignment (the
//! sound or parameter), mute and solo, its own rate and playback mode, and the same buttons for
//! the row alone; a row's subrows show other settings of its steps at once. A row's steps follow
//! the edit mode: click to turn them on or off (drag to paint), drag up or down to set a value
//! (drag across to set more), ⌥-click to reset one (⌥-drag for more), ⌥⇧-drag for a ramp,
//! ⌘⌥-drag to tie, ⇧-click to select steps, ⌘-click to turn one on or off in a value mode.

mod bar;
mod browser;
mod empty;
mod gestures;
mod grid;
mod headers;
mod input;
mod inspector;
pub mod keys;
mod library;
mod ops;
mod patterns;

pub use gestures::StepDrag;
pub use inspector::Field;
pub use ops::*;

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, Pixels, div, prelude::*, px, rgb,
};
use pina_engine::{
    dsp::drums,
    music::Key,
    song::project::{
        At, Content, Instrument, Lane, PPQ, Pattern, PatternRow, PlayMode, Region, RowTarget,
        to_pattern,
    },
};

use crate::{
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Bottom, Drag, Workspace},
};

/// The rows a new drum pattern starts with.
const STARTING_ROWS: [u8; 5] = [36, 38, 42, 46, 39];

/// What a row's steps show and what clicking or dragging them does: on or off, one of their
/// settings, or the row's loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum EditMode {
    #[default]
    OnOff,
    Lane(Lane),
    Loop,
}

impl EditMode {
    /// The edit mode menu, in its order.
    pub const MENU: [EditMode; 11] = [
        EditMode::Lane(Lane::Velocity),
        EditMode::Lane(Lane::Gate),
        EditMode::Lane(Lane::Tie),
        EditMode::Lane(Lane::Note),
        EditMode::Lane(Lane::Octave),
        EditMode::Loop,
        EditMode::Lane(Lane::Repeat),
        EditMode::Lane(Lane::Chance),
        EditMode::Lane(Lane::Offset),
        EditMode::Lane(Lane::Rate),
        EditMode::Lane(Lane::Skip),
    ];

    pub fn lane(self) -> Option<Lane> {
        match self {
            EditMode::Lane(l) => Some(l),
            _ => None,
        }
    }

    /// Its name in the menu bar; a row's own says Velocity or Value.
    pub fn name(self) -> &'static str {
        match self {
            EditMode::OnOff => "On/Off",
            EditMode::Loop => "Loop",
            EditMode::Lane(l) => lane_name(l, None),
        }
    }
}

/// A lane's name: Velocity / Value in the menu bar, Velocity on a note row, Value on an automation
/// row.
pub fn lane_name(lane: Lane, automation: Option<bool>) -> &'static str {
    match (lane, automation) {
        (Lane::Velocity, None) => "Velocity / Value",
        (Lane::Velocity, Some(false)) => "Velocity",
        (Lane::Velocity, Some(true)) => "Value",
        (Lane::Gate, _) => "Gate",
        (Lane::Tie, _) => "Tie",
        (Lane::Note, _) => "Note",
        (Lane::Octave, _) => "Octave",
        (Lane::Repeat, _) => "Repeat",
        (Lane::Chance, _) => "Chance",
        (Lane::Offset, _) => "Offset",
        (Lane::Rate, _) => "Step Rate",
        (Lane::Skip, _) => "Skip",
    }
}

/// The subrows a row opens with when it has none chosen: a drum's or note's settings, a melodic
/// row's notes and octaves, an automation row's values and chance.
pub fn default_subrows(row: &PatternRow) -> Vec<Lane> {
    match (row.is_automation(), row.is_melodic()) {
        (true, _) => vec![Lane::Velocity, Lane::Chance],
        (false, true) => vec![Lane::Note, Lane::Octave],
        (false, false) => vec![
            Lane::Velocity,
            Lane::Repeat,
            Lane::Tie,
            Lane::Note,
            Lane::Octave,
            Lane::Chance,
        ],
    }
}

/// Which pane shows left of the row headers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Browser,
    Inspector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Pattern,
    Row,
    Step,
}

/// When steps show their values as numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShowValues {
    OnlyWhenEditing,
    #[default]
    SelectedRow,
    AllRows,
}

/// Learning rows from what's played or moved: a new row for each sound or control that has none
/// (Add), or the selected row given the next one (Assign).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Learn {
    Add,
    Assign,
}

/// What was copied: a row, or steps.
#[derive(Clone, Debug, PartialEq)]
pub enum StepsClip {
    Row(Box<PatternRow>),
    Steps(Vec<pina_engine::song::project::StepCopy>),
}

/// A step a line shows: (row, subrow's lane, step).
pub type StepAt = (usize, Option<Lane>, usize);

/// Where the grid's lines were drawn last frame, to find the step under the pointer.
#[derive(Default)]
pub struct Geometry {
    /// Each line: its row, its subrow's lane (none for the row itself), and where it is.
    pub lines: HashMap<(usize, Option<Lane>), Bounds<Pixels>>,
    /// The steps shown, first and how many.
    pub shown: (usize, usize),
    /// The grid's width and height, as last laid out.
    pub width: f32,
    pub height: f32,
    /// The menu bar's zoom slider.
    pub zoom_bar: Bounds<Pixels>,
}

/// The step sequencer's view: its edit mode, the row and steps picked, the page, zoom, panes,
/// the input modes, and what's copied.
pub struct StepsView {
    pub mode: EditMode,
    /// The value mode the edit mode button shows beside On/Off: the last picked.
    pub value_mode: EditMode,
    pub row: usize,
    /// Steps picked with ⇧-click, as (row, step).
    pub picked: Vec<(usize, usize)>,
    /// The step that typing a note fills (MIDI in), as (row, step).
    pub cursor: Option<(usize, usize)>,
    pub page: usize,
    /// Steps at their widest (fewer to a page) rather than narrowest.
    pub wide: bool,
    /// Each row's height, and whether they fill the editor's height.
    pub row_h: f32,
    pub fit: bool,
    pub pane: Option<Pane>,
    pub tab: InspectorTab,
    pub midi_in: bool,
    pub midi_out: bool,
    pub mono: bool,
    pub learn: Option<Learn>,
    pub live: bool,
    /// Live recording takes the velocity played, the length held (as gate and ties), and the
    /// timing (as offsets, unless quantized).
    pub live_velocity: bool,
    pub live_length: bool,
    pub live_quantize: bool,
    pub values: ShowValues,
    pub colors: bool,
    pub clip: Option<StepsClip>,
    /// A page's steps copied, every row's.
    pub page_clip: Option<Vec<Vec<pina_engine::song::project::StepCopy>>>,
    /// Rows sorted last to first.
    pub sort_desc: bool,
    /// A name to ask for next frame (from a menu, which has no window to type in): a row's,
    /// a pattern's or a folder's to save.
    pub ask: Option<crate::workspace::Name>,
    /// Preview playing, and the pattern mode it put back when it stops.
    pub preview: Option<bool>,
    /// Notes held while recording live.
    pub holding: Vec<input::Holding>,
    /// While learning: the track's plugin parameters as they were, and a control moved.
    pub learn_base: Vec<(pina_engine::song::project::Which, u32, f64)>,
    pub learned: Option<(usize, pina_engine::song::project::Param)>,
    pub randomize: pina_engine::song::project::Randomize,
    /// The browser's folder (as names from the top) and what's typed in its search.
    pub folder: Vec<String>,
    pub query: String,
    /// The value being dragged, and the step it's over: (row, subrow's lane, step).
    pub tip: Option<(String, StepAt)>,
    /// Keys held for step input: when the last comes up, the cursor moves on.
    pub held: usize,
    pub geom: Rc<RefCell<Geometry>>,
    /// Where the last menu opened, for one it opens.
    pub menu_at: gpui::Point<Pixels>,
}

impl Default for StepsView {
    fn default() -> Self {
        StepsView {
            mode: EditMode::OnOff,
            value_mode: EditMode::Lane(Lane::Velocity),
            row: 0,
            picked: Vec::new(),
            cursor: None,
            page: 0,
            wide: false,
            row_h: ROW_H,
            fit: false,
            pane: None,
            tab: InspectorTab::Pattern,
            midi_in: false,
            midi_out: true,
            mono: false,
            learn: None,
            live: false,
            live_velocity: true,
            live_length: true,
            live_quantize: true,
            values: ShowValues::SelectedRow,
            colors: true,
            clip: None,
            page_clip: None,
            sort_desc: false,
            ask: None,
            preview: None,
            holding: Vec::new(),
            learn_base: Vec::new(),
            learned: None,
            randomize: Default::default(),
            folder: Vec::new(),
            query: String::new(),
            tip: None,
            held: 0,
            geom: Default::default(),
            menu_at: Default::default(),
        }
    }
}

/// A row's height, as usual, and the least and most the zoom goes.
pub const ROW_H: f32 = 46.0;
pub const ROW_MIN: f32 = 30.0;
pub const ROW_MAX: f32 = 96.0;
/// The row headers' width, the pane's, and the gaps between steps and between groups of four.
pub const HEADER_W: f32 = 318.0;
pub const PANE_W: f32 = 250.0;
pub const GAP: f32 = 3.0;
pub const GROUP_GAP: f32 = 7.0;
/// The narrowest and widest a step is drawn.
pub const STEP_MIN: f32 = 18.0;
pub const STEP_MAX: f32 = 64.0;

/// The notes a pattern on a melodic instrument can have rows for: two octaves of the song's key
/// (C major without one) from the C below middle C, highest first.
fn scale_notes(key: Option<Key>) -> Vec<u8> {
    let key = key.unwrap_or(Key {
        root: 0,
        minor: false,
    });
    (48..=72).rev().filter(|&p| key.contains(p)).collect()
}

/// A sound's name on a note: a drum on the kit, a sampler's sound on its note, else the note
/// spelled by the key.
pub(crate) fn row_name(instrument: Option<&Instrument>, key: Option<Key>, pitch: u8) -> String {
    match instrument {
        Some(Instrument::Drums) => drums::name(pitch),
        Some(Instrument::Sampler(s)) => match s.zones.iter().find(|z| z.covers(pitch)) {
            Some(z) if z.high.is_none() && !z.name.is_empty() => z.name.clone(),
            Some(z) if z.high.is_none() => z
                .file
                .file_stem()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            _ => crate::editors::pianoroll::name_in(key, pitch),
        },
        _ => crate::editors::pianoroll::name_in(key, pitch),
    }
}

/// A row's colour, by what it sounds like (a kick pink, hats teal), else the palette in turn.
pub(crate) fn row_color(name: &str, index: usize) -> u32 {
    let n = name.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|w| n.contains(w));
    match () {
        _ if has(&["kick", "bd", "808"]) => PINK,
        _ if has(&["snare", "sd", "rim", "stick"]) => AMBER,
        _ if has(&["clap", "snap"]) => CORAL,
        _ if has(&["open"]) => SKY,
        _ if has(&["hat", "hh"]) => TEAL,
        _ if has(&["tom"]) => MINT,
        _ if has(&["crash", "ride", "cymbal", "splash", "china"]) => SLATE,
        _ if has(&[
            "perc", "shaker", "conga", "bongo", "tamb", "cow", "clave", "wood",
        ]) =>
        {
            VIOLET
        }
        _ => [SKY, TEAL, MINT, AMBER, CORAL, PINK, VIOLET, SLATE][index % 8],
    }
}

/// The colours a row can be given.
pub const ROW_COLORS: [(&str, u32); 8] = [
    ("Pink", PINK),
    ("Amber", AMBER),
    ("Coral", CORAL),
    ("Sky", SKY),
    ("Teal", TEAL),
    ("Mint", MINT),
    ("Violet", VIOLET),
    ("Slate", SLATE),
];

/// The sounds a kit has to add rows for: the drum kit's, or a sampler's.
fn kit_sounds(instrument: Option<&Instrument>) -> Option<Vec<(u8, String)>> {
    match instrument {
        Some(Instrument::Drums) => Some(
            drums::SOUNDS
                .iter()
                .map(|&(pitch, name)| (pitch, name.to_string()))
                .collect(),
        ),
        Some(i @ Instrument::Sampler(s)) if i.is_kit() => Some(
            s.zones
                .iter()
                .map(|z| (z.note, row_name(Some(i), None, z.note)))
                .collect(),
        ),
        _ => None,
    }
}

/// A row as the grid shows it: its name, colour, and whether it's an automation row.
#[derive(Clone)]
pub struct RowLook {
    pub name: String,
    pub color: u32,
    pub automation: bool,
}

impl Workspace {
    /// The pattern the step sequencer shows, and where.
    pub(crate) fn shown_pattern(&self) -> Option<(At, &Pattern)> {
        let at = self.pattern()?;
        match &self.session.project.region(at).content {
            Content::Pattern(p) => Some((at, p)),
            _ => None,
        }
    }

    /// The shown pattern, to change in place (a drag's moves; the drag is the undo step).
    pub(crate) fn pattern_now(&mut self) -> Option<&mut Pattern> {
        let at = self.pattern()?;
        match &mut self.session.project.tracks[at.0].regions[at.1].content {
            Content::Pattern(p) => Some(p),
            _ => None,
        }
    }

    /// Changes the shown pattern as one undo step, and tidies it.
    pub(crate) fn edit_pattern(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Pattern)) {
        if self.pattern().is_none() {
            return;
        }
        self.edit(cx, |ws| {
            if let Some(p) = ws.pattern_now() {
                f(p);
                p.tidy();
            }
        });
        if let Some((_, p)) = self.shown_pattern() {
            self.steps.row = self.steps.row.min(p.rows.len().saturating_sub(1));
        }
    }

    /// Changes one row of the shown pattern as one undo step.
    pub(crate) fn edit_row(
        &mut self,
        r: usize,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut PatternRow, usize),
    ) {
        self.edit_pattern(cx, |p| {
            let steps = p.steps();
            if let Some(row) = p.rows.get_mut(r) {
                f(row, steps);
            }
        });
    }

    /// Each row's name, colour and kind, as shown.
    pub(crate) fn row_looks(&self, at: At, p: &Pattern) -> Vec<RowLook> {
        let project = &self.session.project;
        let track = at.0;
        let instrument = project.tracks[track].instrument.as_ref();
        let key = project.song_key();
        p.rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let name = self.row_label(track, instrument, key, row);
                let color = row
                    .color
                    .unwrap_or_else(|| match (row.target, self.steps.colors) {
                        (Some(_), _) => SLATE,
                        (None, true) => row_color(&name, i),
                        (None, false) => self.color(track),
                    });
                RowLook {
                    name,
                    color,
                    automation: row.is_automation(),
                }
            })
            .collect()
    }

    /// A row's name, as its header shows it: its own, an automation row's parameter, "Melodic",
    /// else its sound.
    pub(crate) fn row_label(
        &self,
        track: usize,
        instrument: Option<&Instrument>,
        key: Option<Key>,
        row: &PatternRow,
    ) -> String {
        if !row.name.is_empty() {
            return row.name.clone();
        }
        match row.target {
            Some(t) => self.row_target_name(track, t),
            None if row.is_melodic() && !matches!(instrument, Some(Instrument::Drums)) => {
                "Melodic".into()
            }
            None => row_name(instrument, key, row.pitch),
        }
    }

    /// What an automation row moves, by name.
    pub(crate) fn row_target_name(&self, track: usize, t: RowTarget) -> String {
        match t.param() {
            Some(p) => self.param_name(track, p),
            None => t.name(),
        }
    }

    /// Hears a row's sound (a step's own note, if given), if MIDI out is on.
    pub(crate) fn audition(&mut self, row: usize, step: Option<usize>) {
        if !self.steps.midi_out {
            return;
        }
        let Some((at, p)) = self.shown_pattern() else {
            return;
        };
        let Some(r) = p.rows.get(row).filter(|r| !r.is_automation()) else {
            return;
        };
        let pitch = step.map_or(r.pitch, |i| r.pitch_at(i));
        let velocity = step.map_or(100, |i| r.velocity_at(i));
        let id = self.session.project.tracks[at.0].id;
        self.player.preview(id, pitch, velocity);
        self.player.preview(id, pitch, 0);
    }
}
