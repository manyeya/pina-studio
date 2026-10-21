//! The main window, over one open project: the control bar; then either the arrangement (the
//! inspector, the tracks, and an editor under them) or the mixer; and a status bar. Every edit
//! checkpoints the session (for undo), changes the project, and syncs the player.

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::HashMap,
    path::Path,
    rc::Rc,
    time::{Duration, Instant, SystemTime},
};

pub use crate::record::Recording;

use gpui::{
    App, Bounds, Context, DispatchPhase, DragMoveEvent, Entity, ExternalPaths, FocusHandle,
    Focusable, FontWeight, KeyBinding, KeyDownEvent, KeyUpEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, PromptLevel, ScrollDelta, ScrollWheelEvent, Subscription, Window, actions,
    canvas, div, prelude::*, px, rgb,
};
use pina_engine::{
    engine::LiveNote,
    player::Player,
    project::{
        At, AutoMode, Comp, Content, Cycle, Eq, Instrument, PPQ, Param, Place, Project,
        RECOVERY_DIR, STATES_DIR, Track, Which, value_at,
    },
    scan::{self, PluginInfo},
    session::Session,
};

use crate::{
    context::RegionOp,
    menu::Menu,
    pianoroll::RollOp,
    strip::{self, Channel},
    text::{TextEvent, TextInput},
    theme::*,
};

actions!(
    tempo,
    [
        PlayStop,
        ToggleAgent,
        NewSong,
        ShowSongs,
        CloseSong,
        EditPlaybooks,
        StudySong,
        BrowserUp,
        BrowserDown,
        BrowserNewTrack,
        GoToStart,
        Undo,
        Redo,
        Delete,
        DeleteTrack,
        Split,
        ToggleMixer,
        ToggleInspector,
        Save,
        NewTrack,
        ZoomIn,
        ZoomOut,
        SelectAllNotes,
        EqSelectAll,
        EqDeselect,
        CopyNotes,
        CutNotes,
        PasteNotes,
        DuplicateNotes,
        NotesUp,
        NotesDown,
        NotesOctaveUp,
        NotesOctaveDown,
        NudgeLeft,
        NudgeRight,
        NudgeLeftFine,
        NudgeRightFine,
        QuantizeNotes,
        DeselectNotes,
        AdderUp,
        AdderDown,
        Quit,
        TogglePianoRoll,
        ToggleAutomation,
        Record,
        ToggleTyping,
        CreateStack,
        NewPattern,
        ToggleLoops,
        ToggleScore,
        ToggleCycle,
        ToggleMetronome,
        Export,
        ToggleSamples,
        ToggleSamplesPage,
        PreviousSample,
        NextSample,
        ShowSettings,
        ToolMenu,
        CopyRegion,
        CutRegion,
        PasteRegion,
        RepeatRegion,
        MuteRegion,
    ]
);

/// Logic's shortcuts, where it has one. While a name is being typed the workspace's context is
/// "Naming", so these keys type instead.
/// The window's shortcuts. Those with no ⌘, ⌃ or ⌥ stand down while a text field has the
/// keyboard, so typing "a" types it rather than showing automation.
pub fn bind_keys(cx: &mut App) {
    crate::text::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("space", PlayStop, Some("Workspace && !TextInput")),
        KeyBinding::new("enter", GoToStart, Some("Workspace && !TextInput")),
        KeyBinding::new("cmd-z", Undo, Some("Workspace")),
        KeyBinding::new("cmd-shift-z", Redo, Some("Workspace")),
        KeyBinding::new("backspace", Delete, Some("Workspace && !TextInput")),
        KeyBinding::new("delete", Delete, Some("Workspace && !TextInput")),
        KeyBinding::new("cmd-backspace", DeleteTrack, Some("Workspace")),
        KeyBinding::new("cmd-t", Split, Some("Workspace")),
        // Letters are notes while musical typing is on, so these only work in the arrangement.
        KeyBinding::new("x", ToggleMixer, Some("Arrange && !TextInput")),
        KeyBinding::new("i", ToggleInspector, Some("Arrange && !TextInput")),
        KeyBinding::new("p", TogglePianoRoll, Some("Arrange && !TextInput")),
        KeyBinding::new("l", ToggleLoops, Some("Arrange && !TextInput")),
        KeyBinding::new("n", ToggleScore, Some("Arrange && !TextInput")),
        KeyBinding::new("a", ToggleAutomation, Some("Arrange && !TextInput")),
        KeyBinding::new("c", ToggleCycle, Some("Arrange && !TextInput")),
        KeyBinding::new("k", ToggleMetronome, Some("Arrange && !TextInput")),
        KeyBinding::new("f", ToggleSamples, Some("Arrange && !TextInput")),
        KeyBinding::new("shift-f", ToggleSamplesPage, Some("Arrange && !TextInput")),
        // In the sample browser, as Splice: the next sound plays as it's picked.
        // Arrows walk the results while their search is typed.
        KeyBinding::new("up", PreviousSample, Some("Samples")),
        KeyBinding::new("down", NextSample, Some("Samples")),
        KeyBinding::new("r", Record, Some("Workspace && !TextInput")),
        KeyBinding::new("cmd-k", ToggleTyping, Some("Workspace")),
        KeyBinding::new("cmd-l", ToggleAgent, None),
        KeyBinding::new("cmd-n", NewSong, None),
        KeyBinding::new("cmd-o", ShowSongs, None),
        // The Browser's results, walked while its search is typed.
        KeyBinding::new("up", BrowserUp, Some("DeviceBrowser")),
        KeyBinding::new("down", BrowserDown, Some("DeviceBrowser")),
        KeyBinding::new("shift-enter", BrowserNewTrack, Some("DeviceBrowser")),
        KeyBinding::new("cmd-shift-d", CreateStack, Some("Workspace")),
        KeyBinding::new("cmd-alt-p", NewPattern, Some("Workspace")),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-b", Export, None),
        KeyBinding::new("cmd-alt-n", NewTrack, Some("Workspace")),
        KeyBinding::new("cmd-right", ZoomIn, Some("Workspace")),
        KeyBinding::new("cmd-left", ZoomOut, Some("Workspace")),
        // The piano roll's notes, while it's open.
        // Regions, while the piano roll doesn't have them.
        KeyBinding::new("cmd-c", CopyRegion, Some("Arrange && !Roll")),
        KeyBinding::new("cmd-x", CutRegion, Some("Arrange && !Roll")),
        KeyBinding::new("cmd-v", PasteRegion, Some("Arrange && !Roll")),
        KeyBinding::new("cmd-r", RepeatRegion, Some("Arrange && !Roll")),
        KeyBinding::new("ctrl-m", MuteRegion, Some("Arrange && !Roll")),
        KeyBinding::new("cmd-a", SelectAllNotes, Some("Roll")),
        KeyBinding::new("cmd-a", EqSelectAll, Some("Eq")),
        KeyBinding::new("escape", EqDeselect, Some("Eq")),
        KeyBinding::new("cmd-c", CopyNotes, Some("Roll")),
        KeyBinding::new("cmd-x", CutNotes, Some("Roll")),
        KeyBinding::new("cmd-v", PasteNotes, Some("Roll")),
        KeyBinding::new("cmd-d", DuplicateNotes, Some("Roll")),
        KeyBinding::new("up", NotesUp, Some("Roll")),
        KeyBinding::new("down", NotesDown, Some("Roll")),
        KeyBinding::new("shift-up", NotesOctaveUp, Some("Roll")),
        KeyBinding::new("shift-down", NotesOctaveDown, Some("Roll")),
        KeyBinding::new("left", NudgeLeft, Some("Roll")),
        KeyBinding::new("right", NudgeRight, Some("Roll")),
        KeyBinding::new("alt-left", NudgeLeftFine, Some("Roll")),
        KeyBinding::new("alt-right", NudgeRightFine, Some("Roll")),
        KeyBinding::new("q", QuantizeNotes, Some("Roll")),
        KeyBinding::new("escape", DeselectNotes, Some("Roll")),
        KeyBinding::new("up", AdderUp, Some("Adder")),
        KeyBinding::new("down", AdderDown, Some("Adder")),
        KeyBinding::new("cmd-,", ShowSettings, None),
        KeyBinding::new("t", ToolMenu, Some("Arrange && !TextInput")),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-w", Quit, None),
    ]);
}

#[derive(Clone, Copy, PartialEq)]
pub enum Edge {
    Move,
    Start,
    End,
    /// The end, time-stretching what's in it rather than trimming it (⌥).
    Stretch,
    /// The top of the end: loops it out to where it's dragged, as Logic's loop tool does.
    Loop,
}

/// What a vertical drag turns: a knob, as a value from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    /// A parameter of a track's instrument plugin.
    Param {
        track: u64,
        param: u32,
    },
    /// A channel's EQ's first low cut.
    LowCut(Channel),
    /// A quick EQ knob: a band of the channel's EQ, found by what it is.
    Quick(Channel, crate::eq::Quick),
    Comp(Channel, CompField),
    Sat(Channel, SatField),
    /// A setting of a track's built-in sampler (by track index).
    Sampler {
        track: usize,
        field: crate::sampler::Field,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SatField {
    Drive,
    Tone,
    Mix,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompField {
    Threshold,
    Ratio,
    Attack,
    Release,
    Makeup,
}

/// What a sideways drag sets: a slider, as a value from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slider {
    Strength,
    Swing,
    Zoom,
    Send {
        track: usize,
        index: usize,
    },
    /// How much one aspect of a sound counts in the sample browser's Similar list.
    Aspect(usize),
    /// A binaural track's sound: the way round, how high, how far.
    Azimuth(usize),
    Elevation(usize),
    Distance(usize),
}

/// A mouse drag in progress. Edits checkpoint when they start, so a drag is one undo step.
#[derive(Clone, Copy)]
pub enum Drag {
    Region {
        at: At,
        edge: Edge,
        from: Point<f32>,
        start: u64,
        end: u64,
        track: usize,
        moved: bool,
        /// Dragging a copy (or an alias) just made, the original staying put.
        copy: bool,
    },
    Fader {
        channel: Channel,
        y: f32,
        pos: f32,
        height: f32,
    },
    /// A pan knob: up or right turns it right. Each move adds to where it was, so ⇧ (fine)
    /// can come and go mid-drag.
    Pan {
        channel: Channel,
        x: f32,
        y: f32,
        pan: f32,
    },
    /// A send's level: a dB for every 2 pixels.
    Send {
        track: usize,
        index: usize,
        y: f32,
        level: f32,
    },
    Tempo {
        y: f32,
        bpm: f64,
    },
    Locate,
    /// Notes in the piano roll, picked up together: each as it began, `lead` the one grabbed
    /// (its snapping moves them all).
    Notes {
        at: At,
        lead: usize,
        edge: Edge,
        from: Point<f32>,
        moved: bool,
    },
    /// A rubber band over the piano roll, picking the notes it touches (and those already
    /// picked, with shift: `held_picks`).
    Marquee {
        from: Point<f32>,
        to: Point<f32>,
    },
    /// The pencil dragged along: a note (or chord) painted at each step it passes, `last` the
    /// latest's start (`painting`, the pitches it sounds).
    Paint {
        at: At,
        last: i64,
    },
    /// Velocities, in the lane under the piano roll: the dragged dot's notes (`held`).
    Velocity {
        at: At,
        y: f32,
    },
    /// Drawing velocities across the lane, from the x it was at.
    VelocityLine {
        at: At,
        x: f32,
    },
    /// The track headers' column (or, `side`, the inspector or browser) made wider or narrower,
    /// from where the drag began and the width it had.
    Width {
        side: bool,
        x: f32,
        width: f32,
    },
    /// A track's lane made taller or shorter by its header's bottom edge.
    LaneHeight {
        track: usize,
        y: f32,
        height: f32,
    },
    /// A track's fader in its header, dragged across.
    HFader {
        track: usize,
        x: f32,
        pos: f32,
        width: f32,
    },
    /// A track's stereo pan spread, dragged (⇧ on its pan knob).
    Spread {
        track: usize,
        x: f32,
        y: f32,
        spread: f32,
    },
    /// The Binaural Panner's view, looked round; its sound dragged round the head at a height,
    /// or (⌘) up and down.
    Orbit {
        x: f32,
        y: f32,
        yaw: f32,
        pitch: f32,
    },
    Source {
        track: usize,
        height: f32,
    },
    /// A modulator's ↗, carried to a knob; a route's depth, a card's knob, its steps painted.
    Modulator {
        track: usize,
        index: usize,
    },
    ModDepth {
        track: usize,
        index: usize,
        target: pina_engine::modulate::ModTarget,
        x: f32,
        y: f32,
        depth: f32,
    },
    ModValue {
        track: usize,
        index: usize,
        field: crate::modulation::ModField,
        y: f32,
        value: f32,
    },
    ModSteps {
        track: usize,
        index: usize,
    },
    /// The Zoom tool's drag across bars; the Solo tool held; the Fade tool's drag (`out` at the
    /// region's end).
    ZoomSpan,
    SoloHold,
    Fade {
        at: At,
        out: bool,
    },
    Raise {
        track: usize,
        y: f32,
        height: f32,
    },
    /// Steps painted in the step sequencer: each the mouse passes over takes `paint` (0 a rest).
    Steps {
        at: At,
        paint: u8,
    },
    /// The step sequencer's velocity lane, drawn.
    StepVelocity,
    /// A mark on the Sampler's waveform: a zone's start, end or loop point; `left` and `width`
    /// where the waveform is.
    Mark {
        track: usize,
        zone: usize,
        mark: crate::sampler::Mark,
        left: f32,
        width: f32,
    },
    /// Drawing a controller's moves in the lane under the piano roll, from the tick it was at.
    Control {
        at: At,
        last: Option<i64>,
        erase: bool,
    },
    /// Automation points, the one held by index (the rest selected with it), from a window
    /// position.
    Auto {
        track: usize,
        param: Param,
        k: usize,
        index: usize,
        moved: bool,
    },
    /// The line between two points, both ends up or down with it.
    AutoLine {
        track: usize,
        param: Param,
        k: usize,
        y: f32,
        moved: bool,
    },
    /// A line's bend, from its middle.
    AutoBend {
        track: usize,
        param: Param,
        index: usize,
        y: f32,
        curve: f32,
    },
    /// A marquee over a lane's points (to select, or with the eraser to take out), window
    /// positions; ⌘ puts a point clicked off the grid.
    AutoMarquee {
        track: usize,
        k: usize,
        from: (f32, f32),
        to: (f32, f32),
        erase: bool,
        free: bool,
    },
    /// The pencil drawing a lane, from its last point.
    AutoDraw {
        track: usize,
        param: Param,
        last: (u64, f32),
    },
    /// A piano roll key held down, sounding.
    Key {
        track: u64,
        pitch: u8,
    },
    /// A swipe across a take, comping it: ticks from the region's start.
    Comp {
        at: At,
        take: usize,
        from: u64,
        to: u64,
    },
    /// Sweeping out the cycle in the ruler, from the tick where it began.
    Cycle {
        anchor: u64,
        moved: bool,
    },
    /// A marker, from the x where it was picked up.
    Marker {
        index: usize,
        x: f32,
        tick: u64,
        moved: bool,
    },
    /// A knob turned by dragging up and down.
    Knob {
        target: Target,
        y: f32,
        value: f32,
    },
    Slider {
        slider: Slider,
        left: f32,
        width: f32,
    },
    /// A band's dot in the EQ window (the bands picked, from where they were), from where it
    /// was pressed: `q` drags their widths, `alt` keeps to one way (`axis`: 0 not yet, 1
    /// across, 2 up and down; without moving, it switches the band off); `deselect` lets it go
    /// if it isn't moved.
    EqNode {
        id: u16,
        from: Point<f32>,
        q: bool,
        alt: bool,
        axis: u8,
        moved: bool,
        deselect: bool,
    },
    /// The EQ's plot pressed where there's no dot: a click, a pull of the curve, or a box.
    EqPress {
        from: Point<f32>,
        on_curve: bool,
        alt: bool,
        moved: bool,
    },
    /// A band's solo held: sideways moves it, up and down sets the level it's heard at.
    EqSolo {
        id: u16,
        from: Point<f32>,
        freq: f32,
    },
    /// EQ Sketch: a curve being drawn.
    EqSketch,
    /// The EQ's frequency scale dragged, from the frequencies it showed: zoom and pan.
    EqScale {
        from: Point<f32>,
        span: (f64, f64),
    },
    /// A knob in the EQ window (a band's by id; 0 for the EQ's own), from the y it was pressed
    /// at (and x) and its value then.
    EqKnob {
        id: u16,
        knob: crate::eq::EqKnob,
        from: f32,
        x: f32,
        start: f32,
    },
    /// A band on the instance list dragged, over its row's display (where it was).
    EqListBand {
        id: u16,
        frame: Bounds<Pixels>,
    },
    /// The EQ window, moved by its title or resized by its corner.
    EqWindow {
        from: Point<f32>,
        start: (f32, f32),
        resize: bool,
    },
    /// The editor's top edge, from the y and height it had.
    Editor {
        y: f32,
        height: f32,
    },
    /// A hit of an audio region (a frame of its file), or its warp marker, moved: a warp marker
    /// where it's dropped.
    Warp {
        at: At,
        from: u64,
    },
    /// The compact sample browser's left edge, from the x and width it had.
    Panel {
        x: f32,
        width: f32,
    },
    /// A sample from the browser, by its index in the library, on its way to the tracks: where
    /// it was picked up, and where it is.
    Sample {
        index: usize,
        from: Point<f32>,
        at: Point<f32>,
        moved: bool,
    },
    /// A device from the Browser, to drop on a track.
    Device {
        device: usize,
        from: Point<f32>,
        at: Point<f32>,
        moved: bool,
    },
    /// A box being drawn on the sample browser's map, from corner to corner.
    MapBox {
        from: Point<f32>,
        to: Point<f32>,
    },
}

/// What the pane under the tracks shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bottom {
    PianoRoll,
    Steps,
    Sampler,
    Modulation,
    Score,
    Takes,
    Loops,
}

/// The arrangement, or the mixer or the sample browser's page in its place.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    Arrange,
    Mixer,
    Samples,
    /// Every song, to open, start or manage.
    Songs,
    /// The genres' playbooks, to read and change.
    Playbooks,
    /// Songs taken apart.
    Study,
    /// What the window shows (⌘,).
    Settings,
}

/// A plugin's parameters: their ids and names.
pub type ParamNames = Rc<Vec<(u32, String)>>;

/// What a name being typed is for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Name {
    Track(usize),
    Aux(usize),
    Marker(usize),
    Region(At),
    /// A track's volume or pan, typed: "-6", "L30", "C".
    Volume(usize),
    Pan(usize),
    /// A value of the EQ window's: a band's (by id; 0 the EQ's own), typed.
    Eq(u16, crate::eq::EqKnob),
}

/// A name being typed, in a text field where the name shows.
pub struct Naming {
    pub what: Name,
    pub input: Entity<TextInput>,
    _subscriptions: [Subscription; 2],
}

/// Which track is armed, to record and to hear the MIDI keyboard: the selected one, as Logic
/// does until you arm one yourself; one you armed; or none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arm {
    Selected,
    Track(u64),
    Off,
}

/// What clicking in the lanes and the piano roll does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tool {
    Pointer,
    Pencil,
    Scissors,
    Eraser,
    Text,
    Glue,
    Solo,
    Mute,
    Zoom,
    Fade,
    Flex,
}

/// What edits snap to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Snap {
    /// Finer as you zoom in.
    Smart,
    Bar,
    Beat,
    Eighth,
    Sixteenth,
    Off,
}

impl Snap {
    pub const ALL: [Snap; 6] = [
        Snap::Smart,
        Snap::Bar,
        Snap::Beat,
        Snap::Eighth,
        Snap::Sixteenth,
        Snap::Off,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Snap::Smart => "Smart",
            Snap::Bar => "Bar",
            Snap::Beat => "Beat",
            Snap::Eighth => "1/8",
            Snap::Sixteenth => "1/16",
            Snap::Off => "Off",
        }
    }
}

/// Which channels the mixer shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Channels {
    All,
    Tracks,
    Buses,
}

/// An edit an agent made through the socket.
#[derive(Clone, Debug)]
pub struct AgentEdit {
    pub at: Instant,
    /// What it did, in words: "set notes · Bass, region 1".
    pub what: String,
    pub track: Option<usize>,
    pub region: Option<usize>,
}

impl AgentEdit {
    /// How long ago, in words: "now", "12s", "3m".
    pub fn ago(&self) -> String {
        let s = self.at.elapsed().as_secs();
        match s {
            0..=4 => "now".into(),
            5..=59 => format!("{s}s ago"),
            _ => format!("{}m ago", s / 60),
        }
    }
}

/// How often unsaved edits are kept, for if Pina Studio stops before they're saved.
const KEEP_EVERY: Duration = Duration::from_secs(30);

/// How long ago, in words: "just now", "4 minutes ago", "2 hours ago", "3 days ago".
fn ago(at: SystemTime) -> String {
    let s = at.elapsed().map_or(0, |d| d.as_secs());
    let (n, unit) = match s {
        0..60 => return "just now".into(),
        60..3_600 => (s / 60, "minute"),
        3_600..86_400 => (s / 3_600, "hour"),
        _ => (s / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// How long an agent's outline stays on what it touched.
pub const PRESENCE: Duration = Duration::from_secs(12);

/// The piano roll's settings for its quantize and tools.
#[derive(Clone, Copy, Debug)]
pub struct RollSettings {
    /// Ticks.
    pub grid: u64,
    pub strength: f32,
    /// 50 straight to 75.
    pub swing: f32,
    /// The track whose notes show faintly behind, by id.
    pub ghost: Option<u64>,
    /// Shading the rows out of the song's key.
    pub scale: bool,
    /// Notes drawn and moved land on the key's notes.
    pub in_key: bool,
    /// A chord drawn for each note, as FL Studio's stamps.
    pub stamp: crate::pianoroll::Stamp,
    /// Notes coloured by velocity, cool to hot, as Logic does; else the track's colour.
    pub by_velocity: bool,
    /// The length a new note is drawn at: the last note picked or stretched (0, the grid).
    pub length: u64,
    /// How tall a pitch's row is, as zoomed.
    pub key_h: f32,
    /// What the lane under the notes shows and edits.
    pub lane: crate::pianoroll::Lane,
}

/// A level meter's reading: falling back gently, and its peak held a moment.
#[derive(Clone, Copy, Default)]
pub struct Level {
    pub now: [f32; 2],
    pub held: [f32; 2],
    since: Option<Instant>,
}

impl Level {
    fn update(&mut self, peak: [f32; 2]) {
        for (now, peak) in self.now.iter_mut().zip(peak) {
            *now = peak.max(*now * 0.85);
        }
        let loudest = self.now[0].max(self.now[1]);
        let expired = self
            .since
            .is_none_or(|t| t.elapsed() > Duration::from_millis(1500));
        if expired || loudest >= self.held[0].max(self.held[1]) {
            self.held = self.now;
            self.since = Some(Instant::now());
        }
    }

    pub fn moving(&self) -> bool {
        self.now.iter().chain(&self.held).any(|&l| l > 1e-4)
    }
}

pub struct Workspace {
    pub session: Session,
    pub player: Player,
    focus: FocusHandle,
    /// The selected track, and region.
    pub track: usize,
    /// When ⌘- or ⇧-clicks picked several tracks, all of them by id (the selected one too).
    pub tracks_picked: Vec<u64>,
    pub region: Option<At>,
    /// Horizontal zoom, and scroll in pixels.
    pub px_per_beat: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub view: View,
    pub bottom: Option<Bottom>,
    pub show_inspector: bool,
    /// The compact sample browser, beside the tracks.
    pub show_browser: bool,
    /// The Agent panel shows beside the tracks, where the Samples panel would.
    pub show_agent: bool,
    /// The Browser is open to pick sounds, instruments or effects for the agent.
    pub picking: bool,
    /// The conversation with Claude in it.
    pub chat: crate::chat::Chat,
    /// The Browser: devices, presets, sounds and songs.
    pub catalog: crate::catalog::Catalog,
    /// Every song, for the Songs page and the menus.
    pub songs: Vec<crate::projects::Song>,
    /// The Playbooks page's genres, and the one being edited.
    pub genres: crate::genres::Genres,
    /// The Song Study page's songs, and the one shown.
    pub studies: crate::study::Studies,
    /// How many times the song has changed, for what's worked out from it (the critic).
    pub edits: u64,
    /// A song to open, once the next frame has the window to ask about saving with.
    pub opening: Option<std::path::PathBuf>,
    /// A song action from a menu, for the next frame.
    pub song_pending: Option<crate::menu::MenuAction>,
    /// The Songs page's new-song settings, search and songs picked, once it's been shown.
    pub home: Option<crate::projects::Home>,
    /// A song being renamed on its card.
    pub renaming: Option<crate::projects::Renaming>,
    /// Each track's icon and colour, as what it plays says or as chosen (`looks.rs`).
    pub looks: Vec<crate::looks::Shown>,
    /// The analysis of the song's audio the looks are worked out from.
    pub heard: crate::looks::Heard,
    /// The colour and icon picker, open on a track.
    pub picker: Option<crate::looks::Picker>,
    /// A colour or icon being tried on a track, under the pointer in the picker.
    pub trying: Option<(usize, crate::looks::Trying)>,
    pub tool: Tool,
    pub snap: Snap,
    /// Playing the selected region alone, looped, as FL Studio's pattern mode; and the region it
    /// plays, with the start and length it had then.
    pub pattern_mode: bool,
    pub pattern: Option<(At, u64, u64)>,
    /// Whether choosing a key moves the music into it, or only names it.
    pub key_moves_notes: bool,
    /// The channel the mixer's side panel shows.
    pub channel: Channel,
    pub channels: Channels,
    /// The mixer's rows: EQ, inserts and sends.
    pub mixer_rows: [bool; 3],
    /// The notes picked in the piano roll (or score), by index in the region shown; the last
    /// the one clicked.
    pub picked: Vec<usize>,
    /// Notes copied in the piano roll, their starts from the first's.
    pub clip: Vec<pina_engine::project::Note>,
    /// What a piano roll drag carries: the notes picked up, as they began; the notes picked
    /// before a rubber band; the pitches the pencil sounds.
    pub held: Vec<(usize, pina_engine::project::Note)>,
    pub held_picks: Vec<usize>,
    pub painting: Vec<u8>,
    /// The pitch under the mouse in the piano roll.
    pub roll_hover: Option<u8>,
    /// Where the lane under the piano roll was drawn last frame.
    pub lane_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The Bounce sheet, when it's open (and what it's bouncing).
    pub export: Option<crate::export::Export>,
    /// The track headers' column and the inspector, as wide as they've been dragged.
    pub header_w: f32,
    pub inspector_w: f32,
    /// Regions copied (⌘C on the tracks), to paste at the playhead.
    pub region_clip: Option<pina_engine::project::Region>,
    /// A MIDI or pattern region's contents as they were when its time-stretch drag began.
    pub stretching: Option<pina_engine::project::Content>,
    /// ⇧ held while dragging a fader or knob: it moves a fifth as far.
    pub fine: bool,
    /// Modulation: each track's modulators' values and their recent ones; the knobs a
    /// modulator can be dropped on, where they were drawn; the steps' editors; the modulator
    /// being dragged, where; the one picked (knobs it moves set its depth); a clock for those
    /// running free.
    pub mod_values: HashMap<u64, [f32; pina_engine::modulate::MAX_MODS]>,
    pub mod_trace: HashMap<(u64, usize), std::collections::VecDeque<f32>>,
    pub mod_targets: crate::modulation::DropTargets,
    /// Plugins' parameter names, by track and plugin (`param_names`).
    param_names: RefCell<HashMap<(u64, Which), (std::time::Instant, ParamNames)>>,
    pub mod_steps: crate::modulation::StepsAt,
    pub mod_drag_at: Option<Point<f32>>,
    pub mod_edit: Option<(usize, usize)>,
    /// A modulator waiting to be mapped to whatever's turned next in a plugin's own window.
    pub mod_learn: Option<crate::modulation::Learn>,
    pub mod_clock: Instant,
    /// The Zoom tool's drag, where it began and is now (window x); what the Solo tool's hold
    /// changed, to put back (pattern mode, its region, playing, the selection).
    pub zoom_span: Option<(f32, f32)>,
    #[allow(clippy::type_complexity)]
    pub solo_restore: Option<(bool, Option<(At, u64, u64)>, bool, Option<At>)>,
    /// The Binaural Panner, open on a track, where; where its view looks from (degrees round
    /// and down); and where the view is on screen.
    pub panner: Option<(usize, Point<Pixels>)>,
    pub orbit: (f32, f32),
    pub panner_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The last control a sideways swipe moved, and when: one undo for each swipe.
    pub swiped: Option<(Name, std::time::Instant)>,
    /// A track's colour and icon picker waiting for the next frame (from a menu), and where.
    pub pending_look: Option<(usize, Point<Pixels>)>,
    /// The New Track palette, when it's open; and the presets it searches.
    pub adder: Option<crate::adder::Adder>,
    pub sounds: crate::adder::Sounds,
    /// The step sequencer's picked row (its velocities in the lane), and where its lane was drawn;
    /// and a row picked by pressing a step, picked when the mouse comes up (so the lane doesn't
    /// move the rows under a drag).
    pub steps_row: usize,
    pub steps_pick: Option<usize>,
    /// What the window shows, kept for every song (Settings, ⌘,); the page's section shown;
    /// where play last started (to go back to on stop, if Settings say).
    pub settings: crate::settings::Settings,
    pub settings_section: crate::settings::Section,
    pub play_from: u64,
    /// What the lane under the picked sound shows.
    pub steps_lane_mode: crate::steps::LaneMode,
    pub steps_lane: Rc<Cell<Bounds<Pixels>>>,
    /// The Sampler editor: the zone picked and the waveforms read; where its waveform was drawn.
    pub sampler_ui: crate::sampler::SamplerUi,
    pub sampler_wave_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The piano roll's vertical scroll, and where its grid was drawn last frame.
    pub roll_scroll_y: f32,
    pub roll_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub roll: RollSettings,
    pub show_automation: bool,
    /// How tall the editor under the tracks stands, as dragged.
    pub editor_h: f32,
    /// The compact sample browser's width, as dragged.
    pub panel_w: f32,
    /// The parameters each track's automation lanes show, top to bottom, by track id.
    pub auto_lanes: HashMap<u64, Vec<Param>>,
    /// Automation points selected, the lane last worked on, points copied, and a lane as it was
    /// when a drag on it began.
    pub auto_picked: Option<crate::automation::Picked>,
    pub auto_focus: Option<(u64, Param)>,
    pub auto_clip: Option<pina_engine::project::AutoClip>,
    pub auto_grab: Vec<pina_engine::project::AutoPoint>,
    /// Controls a hand has taken over from their automation, those being trimmed, and those
    /// recorded this pass (to thin when it ends).
    pub taken: HashMap<(u64, Param), crate::automation::Live>,
    pub touches: HashMap<(u64, Param), crate::automation::Touch>,
    pub written: std::collections::HashSet<(u64, Param)>,
    /// The lines Touch is recording over, as they were.
    pub recorded: HashMap<(u64, Param), Vec<pina_engine::project::AutoPoint>>,
    /// Whether the song was playing last frame, to tell a pass starting or ending.
    pub was_playing: bool,
    /// What each group of aliases played after the last edit, to tell which one changed.
    links: HashMap<u64, Content>,
    /// Automation being recorded (Touch or Latch), by track id and parameter: the last tick
    /// written, the control's value, and whether it's still held.
    pub writing: HashMap<(u64, Param), (u64, f32, bool)>,
    pub recording: Option<Recording>,
    /// Musical typing's octave shift, when it's on; and the notes its held keys play.
    pub typing: Option<i8>,
    typed: HashMap<String, u8>,
    /// An open pop-up menu.
    pub menu: Option<Menu>,
    /// A name being typed.
    pub naming: Option<Naming>,
    /// Something to start naming on the next frame (from a menu, which has no window to hand).
    pub pending_name: Option<Name>,
    /// Which track records and hears the MIDI keyboard.
    pub arm: Arm,
    /// Give the workspace back the keyboard on the next frame (after naming, or searching).
    pub refocus: bool,
    /// Live Loops launched, by track id: the scene (None, stopped) and the sample it begins at.
    pub live: HashMap<u64, (Option<usize>, u64)>,
    /// The plugins installed, once the scan is done.
    pub plugins: Option<Vec<PluginInfo>>,
    pub drag: Option<Drag>,
    /// Where files dragged in from the Finder are, and which, while they're over the window.
    pub file_drag: Option<(Point<f32>, Vec<std::path::PathBuf>)>,
    /// Meter readings by track or aux id; None is the master.
    pub levels: HashMap<Option<u64>, Level>,
    /// Compressor gain reduction, in dB, by channel id; None is the master.
    pub reductions: HashMap<Option<u64>, f32>,
    /// How busy the audio thread is: 1 is all of its time; and when that was last read.
    pub load: f32,
    load_read: Instant,
    pub status: Option<String>,
    /// What agents have done, newest last.
    pub agent: Vec<AgentEdit>,
    /// The parameters Smart Controls shows for each track's instrument plugin, by track id.
    pub smart: HashMap<u64, (String, Vec<(u32, String)>)>,
    pub exporting: bool,
    /// Where the lanes and the EQ graph were drawn last frame, for turning pointer positions into
    /// ticks and frequencies.
    pub lanes_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The Channel EQ's window, when it's open; and bands copied from it.
    pub eq_view: Option<crate::eq::EqView>,
    pub eq_clipboard: Vec<pina_engine::project::EqBand>,
    /// The window's size, for what fits.
    pub viewport_h: f32,
    pub viewport_w: f32,
    /// Where each slider was drawn last frame.
    pub slider_bounds: Rc<RefCell<HashMap<Slider, Bounds<Pixels>>>>,
    /// The sample browser: the library, what's searched for, and what's found.
    pub browser: crate::browser::Browser,
    title: String,
}

pub fn pt(p: Point<Pixels>) -> Point<f32> {
    p.map(f32::from)
}

impl Workspace {
    pub fn new(
        session: Session,
        mut player: Player,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let status = player.sync(&session.project).err();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |ws, cx| ws.should_close(window, cx))
                .unwrap_or(true)
        });
        let links = session.project.link_contents();
        let browser = crate::browser::Browser::new(cx);
        let chat = crate::chat::Chat::new(cx);
        let catalog = crate::catalog::Catalog::new(cx);
        // A song not saved yet opens on the Songs page.
        let view = match session.on_disk {
            true => View::Arrange,
            false => View::Songs,
        };
        // The CPU load and meters refresh while nothing else redraws.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(KEEP_EVERY).await;
                if this.update(cx, |ws, _| ws.keep_edits()).is_err() {
                    break;
                }
            }
        })
        .detach();
        let mut ws = Workspace {
            session,
            player,
            focus,
            track: 0,
            tracks_picked: Vec::new(),
            region: None,
            px_per_beat: 48.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            view,
            bottom: None,
            show_inspector: true,
            show_browser: false,
            show_agent: false,
            picking: false,
            tool: Tool::Pointer,
            snap: Snap::Smart,
            pattern_mode: false,
            pattern: None,
            key_moves_notes: true,
            channel: Channel::Track(0),
            channels: Channels::All,
            mixer_rows: [true; 3],
            picked: Vec::new(),
            clip: Vec::new(),
            held: Vec::new(),
            held_picks: Vec::new(),
            painting: Vec::new(),
            roll_hover: None,
            lane_bounds: Rc::new(Cell::new(Bounds::default())),
            export: None,
            header_w: HEADER_W,
            inspector_w: INSPECTOR_W,
            region_clip: None,
            stretching: None,
            fine: false,
            panner: None,
            mod_values: HashMap::new(),
            mod_trace: HashMap::new(),
            mod_targets: Default::default(),
            param_names: Default::default(),
            mod_steps: Default::default(),
            mod_drag_at: None,
            mod_edit: None,
            mod_learn: None,
            mod_clock: Instant::now(),
            zoom_span: None,
            solo_restore: None,
            orbit: crate::binaural::ORBIT,
            panner_bounds: Default::default(),
            swiped: None,
            pending_look: None,
            adder: None,
            sounds: Default::default(),
            steps_row: 0,
            steps_pick: None,
            steps_lane_mode: Default::default(),
            settings: crate::settings::Settings::load(),
            settings_section: Default::default(),
            play_from: 0,
            steps_lane: Rc::new(Cell::new(Bounds::default())),
            sampler_ui: Default::default(),
            sampler_wave_bounds: Rc::new(Cell::new(Bounds::default())),
            roll_scroll_y: crate::pianoroll::scroll_to(72),
            roll_bounds: Rc::new(Cell::new(Bounds::default())),
            roll: RollSettings {
                grid: PPQ / 4,
                strength: 1.0,
                swing: 50.0,
                ghost: None,
                scale: true,
                in_key: false,
                stamp: crate::pianoroll::Stamp::Note,
                by_velocity: false,
                length: 0,
                key_h: crate::pianoroll::KEY_H,
                lane: crate::pianoroll::Lane::Velocity,
            },
            show_automation: false,
            editor_h: EDITOR_H,
            panel_w: crate::browser::PANEL_W,
            auto_lanes: HashMap::new(),
            auto_picked: None,
            auto_focus: None,
            auto_clip: None,
            auto_grab: Vec::new(),
            taken: HashMap::new(),
            touches: HashMap::new(),
            written: Default::default(),
            recorded: HashMap::new(),
            was_playing: false,
            links,
            writing: HashMap::new(),
            recording: None,
            typing: None,
            typed: HashMap::new(),
            menu: None,
            naming: None,
            pending_name: None,
            arm: Arm::Selected,
            refocus: false,
            plugins: None,
            live: HashMap::new(),
            drag: None,
            file_drag: None,
            levels: HashMap::new(),
            reductions: HashMap::new(),
            load: 0.0,
            load_read: Instant::now(),
            status,
            agent: Vec::new(),
            smart: HashMap::new(),
            exporting: false,
            lanes_bounds: Rc::new(Cell::new(Bounds::default())),
            eq_view: None,
            eq_clipboard: Vec::new(),
            slider_bounds: Rc::new(RefCell::new(HashMap::new())),
            viewport_h: 900.0,
            viewport_w: 1440.0,
            browser,
            chat,
            catalog,
            songs: Vec::new(),
            genres: Default::default(),
            studies: Default::default(),
            edits: 0,
            opening: None,
            song_pending: None,
            home: None,
            renaming: None,
            looks: Vec::new(),
            heard: Default::default(),
            picker: None,
            trying: None,
            title: String::new(),
        };
        ws.refresh_looks(cx);
        ws.refresh_songs(cx);
        // For screenshots and docs: PINA_PAGE=playbooks[:genre[@scroll]] (or study, agent)
        // opens on that page.
        if let Ok(page) = std::env::var("PINA_PAGE") {
            match page.split_once(':').unwrap_or((page.as_str(), "")) {
                ("playbooks", rest) => {
                    let (genre, down) = rest.split_once('@').unwrap_or((rest, "0"));
                    ws.open_genres((!genre.is_empty()).then_some(genre), cx);
                    let down: f32 = down.parse().unwrap_or(0.0);
                    ws.genres.scroll.set_offset(gpui::point(px(0.), px(-down)));
                }
                ("study", _) => ws.open_study(cx),
                ("agent", rest) => {
                    ws.show_agent = true;
                    ws.chat.critic.open = rest == "critic";
                }
                _ => {}
            }
        }
        ws
    }

    /// The inspector's width.
    pub fn side_w(&self) -> f32 {
        self.inspector_w
    }

    /// The aliases' shared contents, from the song as it is.
    pub fn reset_links(&mut self) {
        self.links = self.session.project.link_contents();
    }

    pub fn px_per_tick(&self) -> f32 {
        self.px_per_beat / PPQ as f32
    }

    /// The tick under a window x position, in the lanes.
    pub fn tick_at(&self, x: f32) -> f64 {
        let left = f32::from(self.lanes_bounds.get().origin.x);
        ((x - left + self.scroll_x) / self.px_per_tick()).max(0.0) as f64
    }

    /// A track's row: its stack's row above it if it's the stack's first track, then its lane
    /// and its automation lanes (none of them while the stack's folded).
    pub fn row_h_of(&self, i: usize) -> f32 {
        let above = self.stack_above(i).map_or(0.0, |_| STACK_H);
        match self.folded(i) {
            true => above,
            false => above + self.lane_h(i) + self.autos_h(i),
        }
    }

    /// The stack (by its place among the auxes) whose row is above track `i`: the first track
    /// in it.
    pub fn stack_above(&self, i: usize) -> Option<usize> {
        let p = &self.session.project;
        let bus = p.tracks.get(i)?.output?;
        let a = p.auxes.iter().position(|a| a.bus == bus && a.stack)?;
        p.tracks[..i]
            .iter()
            .all(|t| t.output != Some(bus))
            .then_some(a)
    }

    /// Whether track `i` is folded away in its stack.
    pub fn folded(&self, i: usize) -> bool {
        self.session.project.stack_of(i).is_some_and(|a| a.folded)
    }

    /// Track `i`'s lane height, as dragged (the usual, past the last track).
    pub fn lane_h(&self, i: usize) -> f32 {
        self.session
            .project
            .tracks
            .get(i)
            .and_then(|t| t.height)
            .unwrap_or(self.settings.track_height)
            .clamp(MIN_LANE_H, MAX_LANE_H)
    }

    /// Where track `i`'s row begins, from the top of the lanes, scrolled.
    pub fn row_top(&self, i: usize) -> f32 {
        let above = self.stack_above(i).map_or(0.0, |_| STACK_H);
        (0..i).map(|j| self.row_h_of(j)).sum::<f32>() + above - self.scroll_y
    }

    /// Every row, top to bottom.
    pub fn rows_h(&self) -> f32 {
        (0..self.session.project.tracks.len())
            .map(|i| self.row_h_of(i))
            .sum()
    }

    /// The track whose row is at `y` (from the top of the lanes, as drawn), if one is.
    pub fn track_at_y(&self, y: f32) -> Option<usize> {
        let mut top = -self.scroll_y;
        for i in 0..self.session.project.tracks.len() {
            top += self.stack_above(i).map_or(0.0, |_| STACK_H);
            if self.folded(i) {
                continue;
            }
            let h = self.lane_h(i) + self.autos_h(i);
            if y < top + h {
                return (y >= top).then_some(i);
            }
            top += h;
        }
        None
    }

    /// The editing grid, in ticks.
    pub fn grid(&self) -> u64 {
        match self.snap {
            Snap::Smart => match self.px_per_beat {
                z if z >= 96.0 => PPQ / 4,
                z if z >= 20.0 => self.session.project.beat(),
                _ => self.session.project.bar(),
            },
            Snap::Bar => self.session.project.bar(),
            Snap::Beat => self.session.project.beat(),
            Snap::Eighth => PPQ / 2,
            Snap::Sixteenth => PPQ / 4,
            Snap::Off => 1,
        }
    }

    /// The grid line at or before a tick: the cell it's in.
    pub fn snap_down(&self, tick: f64) -> u64 {
        let g = self.grid() as f64;
        ((tick / g).floor() * g).max(0.0) as u64
    }

    pub fn snap(&self, tick: f64) -> u64 {
        let g = self.grid() as f64;
        ((tick / g).round() * g).max(0.0) as u64
    }

    pub fn playhead(&self) -> u64 {
        self.session.project.ticks(self.player.position())
    }

    /// The color a track's regions, header and strip use.
    pub fn color(&self, track: usize) -> u32 {
        self.shown(track).color
    }

    /// What the engine plays: the project, or in pattern mode just the selected region, looped.
    fn engine_project(&self) -> Cow<'_, Project> {
        let mut p = self.pattern_project();
        // Controls a hand has taken over play as it has them.
        if !self.taken.is_empty() {
            self.without_live(p.to_mut());
        }
        p
    }

    fn pattern_project(&self) -> Cow<'_, Project> {
        let project = &self.session.project;
        let Some(((t, r), ..)) = self.pattern.filter(|&((t, r), ..)| {
            self.pattern_mode && project.tracks.get(t).is_some_and(|t| r < t.regions.len())
        }) else {
            return Cow::Borrowed(project);
        };
        let mut p = project.clone();
        let region = &project.tracks[t].regions[r];
        p.cycle = Some(Cycle {
            start: region.start,
            end: region.start + region.length,
            off: false,
        });
        for (i, track) in p.tracks.iter_mut().enumerate() {
            track.solo = false;
            track.mute = i != t;
            if i == t {
                let keep = track.regions[r].clone();
                track.regions = vec![keep];
            }
        }
        Cow::Owned(p)
    }

    /// After an edit (or undo): keeps the selection valid and brings the engine up to date.
    /// A plugin's parameters' ids and names, asked of it now and then (an Audio Unit answers
    /// one call a parameter): kept until an edit, or two seconds, for a plugin swapped or loaded.
    pub fn param_names(&self, track: u64, which: Which) -> ParamNames {
        if let Some((at, names)) = self.param_names.borrow().get(&(track, which))
            && at.elapsed() < std::time::Duration::from_secs(2)
        {
            return names.clone();
        }
        let names: ParamNames = Rc::new(
            self.player
                .params(track, which)
                .into_iter()
                .map(|p| (p.id, p.name))
                .collect(),
        );
        // A plugin still loading has none yet: asked again next time.
        if !names.is_empty() {
            self.param_names
                .borrow_mut()
                .insert((track, which), (std::time::Instant::now(), names.clone()));
        }
        names
    }

    pub fn changed(&mut self, cx: &mut Context<Self>) {
        self.edits += 1;
        self.param_names.borrow_mut().clear();
        // Aliases of whatever changed change with it.
        self.session.project.sync_links(&mut self.links);
        let tracks = &self.session.project.tracks;
        self.track = self.track.min(tracks.len().saturating_sub(1));
        if let Some((t, r)) = self.region
            && tracks.get(t).is_none_or(|t| r >= t.regions.len())
        {
            self.region = None;
        }
        let notes = self
            .region
            .and_then(|at| self.session.project.notes_mut(at))
            .map(|n| n.len());
        self.picked.retain(|&i| notes.is_some_and(|n| i < n));
        let valid = match self.channel {
            Channel::Track(i) => i < self.session.project.tracks.len(),
            Channel::Aux(i) => i < self.session.project.auxes.len(),
            Channel::Master => true,
        };
        if !valid {
            self.channel = Channel::Master;
        }
        let project = self.engine_project().into_owned();
        self.status = self.player.sync(&project).err();
        self.refresh_looks(cx);
        cx.notify();
    }

    /// The editor's height: as dragged, but never so tall the tracks lose their room, nor so
    /// short its tools don't fit.
    pub fn editor_height(&self) -> f32 {
        let room = self.viewport_h - CONTROL_BAR_H - STATUS_H - MARKERS_H - RULER_H - LANE_H * 2.0;
        self.editor_h.min(room).max(EDITOR_MIN_H)
    }

    /// A plugin's name on a track, from its bundle.
    pub fn plugin_name(&self, track: usize, which: Which) -> String {
        self.session.project.tracks[track]
            .plugin(which)
            .and_then(|r| r.path.file_stem())
            .map_or_else(|| "Plugin".into(), |s| s.to_string_lossy().into_owned())
    }

    /// What a parameter is called: Volume, Pan, or the plugin's name for it.
    pub fn param_name(&self, track: usize, param: Param) -> String {
        let t = &self.session.project.tracks[track];
        match param {
            Param::Volume => "Volume".into(),
            Param::Pan => "Pan".into(),
            Param::Mute => "Mute".into(),
            Param::Send(bus) => {
                let aux = self.session.project.auxes.iter().find(|a| a.bus == bus);
                format!("Send to {}", aux.map_or("a bus", |a| a.name.as_str()))
            }
            Param::Eq(band, field) => {
                let what = match field {
                    pina_engine::project::EqField::Freq => "Frequency",
                    pina_engine::project::EqField::Gain => "Gain",
                    pina_engine::project::EqField::Q => "Q",
                };
                let shape =
                    t.eq.and_then(|e| e.bands.get(band as usize).map(|b| b.shape.name()))
                        .unwrap_or("Band");
                format!("EQ {} {shape}: {what}", band + 1)
            }
            Param::Mod(k, setting) => {
                format!("{}: {}", self.mod_name(track, k as usize), setting.name())
            }
            Param::Plugin(which, id) => t
                .automation
                .plugins
                .iter()
                .find(|a| a.plugin == which && a.param == id && !a.name.is_empty())
                .map(|a| a.name.clone())
                .or_else(|| {
                    let names = self.param_names(t.id, which);
                    names.iter().find(|p| p.0 == id).map(|p| p.1.clone())
                })
                .unwrap_or_else(|| format!("Parameter {id}")),
        }
    }

    /// Shows automated plugin parameters' values in their plugins (and Smart Controls) as the
    /// playhead moves: the engine plays them, the controllers only show them.
    fn show_automation_values(&self) {
        let tick = self.playhead() as f64;
        for t in &self.session.project.tracks {
            if t.automation.mode == AutoMode::Off {
                continue;
            }
            for a in &t.automation.plugins {
                if let Some(v) = value_at(&a.points, tick) {
                    self.player.show_param(t.id, a.plugin, a.param, v as f64);
                }
            }
        }
    }

    /// The armed track, by index.
    pub fn armed(&self) -> Option<usize> {
        let tracks = &self.session.project.tracks;
        match self.arm {
            Arm::Selected => (self.track < tracks.len()).then_some(self.track),
            Arm::Track(id) => tracks.iter().position(|t| t.id == id),
            Arm::Off => None,
        }
    }

    /// The R button: arms a track, or disarms it if it's the one armed.
    pub fn toggle_arm(&mut self, track: usize) {
        self.arm = match self.armed() == Some(track) {
            true => Arm::Off,
            false => Arm::Track(self.session.project.tracks[track].id),
        };
    }

    /// What something is called now.
    pub fn name_of(&self, what: Name) -> String {
        let p = &self.session.project;
        match what {
            Name::Track(i) => p.tracks.get(i).map(|t| t.name.clone()),
            Name::Aux(i) => p.auxes.get(i).map(|a| a.name.clone()),
            Name::Marker(i) => p.markers.get(i).map(|m| m.name.clone()),
            Name::Region(at) => p
                .tracks
                .get(at.0)
                .and_then(|t| t.regions.get(at.1))
                .map(|r| r.name.clone()),
            Name::Volume(i) => p.tracks.get(i).map(|t| match t.volume_db <= strip::SILENT {
                true => "-inf".into(),
                false => format!("{:.1}", t.volume_db),
            }),
            Name::Pan(i) => p.tracks.get(i).map(|t| strip::pan_label(t.pan)),
            Name::Eq(id, knob) => Some(self.eq_value_text(id, knob)),
        }
        .unwrap_or_default()
    }

    /// Opens a text field to name something, where its name shows.
    pub fn start_naming(&mut self, what: Name, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_naming(None, cx);
        let input = cx.new(|cx| TextInput::new(&self.name_of(what), cx));
        let done = cx.subscribe(&input, |ws, _, e: &TextEvent, cx| {
            let text = match e {
                TextEvent::Confirm(text) => Some(text.clone()),
                TextEvent::Cancel => None,
            };
            ws.finish_naming(text, cx);
        });
        let focus = input.read(cx).focus_handle(cx);
        // Clicking elsewhere keeps what's typed.
        let blur = cx.on_blur(&focus, window, |ws, _, cx| {
            if let Some(n) = &ws.naming {
                n.input.update(cx, |i, cx| i.confirm(cx));
            }
        });
        window.focus(&focus, cx);
        // Started from a click: the workspace mustn't take the keyboard back as it bubbles up.
        window.prevent_default();
        self.naming = Some(Naming {
            what,
            input,
            _subscriptions: [done, blur],
        });
        cx.notify();
    }

    /// Closes the text field, naming the thing `text` if given. A track, bus or marker keeps its
    /// name rather than going without; a region without one goes by its track's.
    pub fn finish_naming(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        let Some(naming) = self.naming.take() else {
            return;
        };
        self.refocus = true;
        let what = naming.what;
        let text = text.filter(|t| *t != self.name_of(what));
        if let (Name::Eq(id, knob), Some(text)) = (what, &text) {
            self.eq_value_typed(id, knob, text, cx);
            cx.notify();
            return;
        }
        // A volume or pan typed: set if it reads as one.
        if let (Name::Volume(i) | Name::Pan(i), Some(text)) = (what, &text) {
            match (what, strip::parse_db(text), strip::parse_pan(text)) {
                (Name::Volume(_), Some(db), _) => self.edit(cx, |ws| {
                    ws.set_volume(Channel::Track(i), db);
                    ws.moved_once(i, Param::Volume, db);
                }),
                (Name::Pan(_), _, Some(pan)) => self.edit(cx, |ws| {
                    ws.set_pan(Channel::Track(i), pan);
                    ws.moved_once(i, Param::Pan, pan);
                }),
                (Name::Volume(_), ..) => {
                    self.status = Some(format!("{text:?} isn't a level: type dB, as -6 or 0"))
                }
                _ => self.status = Some(format!("{text:?} isn't a pan: type L30, C or R45")),
            }
            cx.notify();
            return;
        }
        if let Some(text) = text.filter(|t| !t.is_empty() || matches!(what, Name::Region(_))) {
            self.edit(cx, |ws| {
                let p = &mut ws.session.project;
                match what {
                    Name::Track(i) => p.tracks[i].name = text,
                    Name::Aux(i) => p.auxes[i].name = text,
                    Name::Marker(i) => p.markers[i].name = text,
                    Name::Region(at) => p.tracks[at.0].regions[at.1].name = text,
                    Name::Volume(_) | Name::Pan(_) | Name::Eq(..) => {}
                }
            });
        }
        cx.notify();
    }

    /// A sideways swipe over a track's fader or pan knob: nudges it, one undo a swipe.
    pub fn swipe(&mut self, what: Name, dx: f32, cx: &mut Context<Self>) {
        let now = std::time::Instant::now();
        if self
            .swiped
            .is_none_or(|(w, at)| w != what || now - at > std::time::Duration::from_millis(600))
        {
            self.session.checkpoint();
        }
        self.swiped = Some((what, now));
        match what {
            Name::Volume(i) => {
                let pos = strip::fader_pos(self.now_value(i, Param::Volume));
                let db = strip::fader_db((pos + dx * 0.002).clamp(0.0, 1.0));
                self.set_volume(Channel::Track(i), db);
                self.moved_once(i, Param::Volume, db);
            }
            Name::Pan(i) => {
                let pan = (self.now_value(i, Param::Pan) + dx * 0.004).clamp(-1.0, 1.0);
                self.set_pan(Channel::Track(i), pan);
                self.moved_once(i, Param::Pan, pan);
            }
            _ => return,
        }
        self.changed(cx);
    }

    /// The text field for `what`, if it's being named.
    pub fn name_field(&self, what: Name) -> Option<Entity<TextInput>> {
        self.naming
            .as_ref()
            .filter(|n| n.what == what)
            .map(|n| n.input.clone())
    }

    /// One undoable edit.
    pub fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Self)) {
        self.session.checkpoint();
        f(self);
        self.changed(cx);
    }

    /// Selects a track: in the arrangement, the inspector and the mixer.
    pub fn select_track(&mut self, i: usize) {
        self.track = i;
        self.channel = Channel::Track(i);
        self.tracks_picked.clear();
    }

    /// ⌘-click: a track into the selection, or out of it; ⇧-click: every track from the selected
    /// one to it.
    pub fn pick_track(&mut self, i: usize, range: bool) {
        let ids: Vec<u64> = self.session.project.tracks.iter().map(|t| t.id).collect();
        if i >= ids.len() || self.track >= ids.len() {
            return;
        }
        if range {
            // The selected track stays where the range starts, for the next ⇧-click.
            let (a, b) = (self.track.min(i), self.track.max(i));
            self.tracks_picked = ids[a..=b].to_vec();
            return;
        }
        if self.tracks_picked.is_empty() {
            self.tracks_picked.push(ids[self.track]);
        }
        match self.tracks_picked.iter().position(|&id| id == ids[i]) {
            None => {
                self.tracks_picked.push(ids[i]);
                self.track = i;
            }
            // The last one stays.
            Some(_) if self.tracks_picked.len() == 1 => {}
            Some(at) => {
                self.tracks_picked.remove(at);
                if i == self.track {
                    let next = self.tracks_picked[0];
                    self.track = ids.iter().position(|&id| id == next).unwrap_or(i);
                }
            }
        }
        self.channel = Channel::Track(self.track);
    }

    /// A stack of these tracks, gathered under the first of them; they stay selected, with the
    /// stack's channel shown.
    pub fn make_stack(&mut self, tracks: &[usize], cx: &mut Context<Self>) {
        let ids: Vec<u64> = tracks
            .iter()
            .filter_map(|&t| self.session.project.tracks.get(t).map(|t| t.id))
            .collect();
        if ids.is_empty() {
            return;
        }
        let mut bus = 0;
        self.edit(cx, |ws| bus = ws.session.project.add_stack(tracks).0);
        let p = &self.session.project;
        let first = p.tracks.iter().position(|t| t.id == ids[0]).unwrap_or(0);
        let aux = p.auxes.iter().position(|a| a.bus == bus);
        self.select_track(first);
        self.tracks_picked = ids;
        self.region = None;
        self.mod_edit = None;
        if let Some(a) = aux {
            self.channel = Channel::Aux(a);
        }
        cx.notify();
    }

    pub fn is_picked(&self, i: usize) -> bool {
        i == self.track
            || self
                .session
                .project
                .tracks
                .get(i)
                .is_some_and(|t| self.tracks_picked.contains(&t.id))
    }

    /// The selected tracks, top to bottom: the ⌘- or ⇧-clicked ones, or the one.
    pub fn picked_tracks(&self) -> Vec<usize> {
        (0..self.session.project.tracks.len())
            .filter(|&i| self.is_picked(i))
            .collect()
    }

    pub fn begin_drag(&mut self, drag: Drag, cx: &mut Context<Self>) {
        // A knob a picked modulator moves: its depth, not the knob.
        let drag = self.depth_drag(drag);
        let edits = !matches!(
            drag,
            Drag::Locate
                | Drag::Key { .. }
                | Drag::Comp { .. }
                | Drag::Slider {
                    slider: Slider::Strength | Slider::Swing | Slider::Zoom,
                    ..
                }
                | Drag::Slider {
                    slider: Slider::Aspect(_),
                    ..
                }
                | Drag::Editor { .. }
                | Drag::Panel { .. }
                | Drag::Sample { .. }
                | Drag::Device { .. }
                | Drag::MapBox { .. }
                | Drag::Marquee { .. }
                | Drag::Width { .. }
                | Drag::LaneHeight { .. }
                | Drag::Orbit { .. }
                | Drag::Modulator { .. }
                | Drag::ZoomSpan
                | Drag::SoloHold
                | Drag::AutoMarquee { .. }
                | Drag::AutoDraw { .. }
                | Drag::EqWindow { .. }
                | Drag::EqPress { .. }
                | Drag::EqScale { .. }
                | Drag::EqSketch
        );
        if edits {
            self.session.checkpoint();
        }
        self.drag = Some(drag);
        cx.notify();
    }

    fn drag_to(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        let ppt = self.px_per_tick();
        if let Some(
            Drag::Region { moved, .. }
            | Drag::Notes { moved, .. }
            | Drag::Auto { moved, .. }
            | Drag::AutoLine { moved, .. }
            | Drag::Cycle { moved, .. }
            | Drag::Marker { moved, .. },
        ) = &mut self.drag
        {
            *moved = true;
        }
        match self.drag {
            Some(Drag::Region {
                at,
                edge,
                from,
                start,
                end,
                track,
                ..
            }) => {
                let dt = ((p.x - from.x) / ppt) as f64;
                match edge {
                    Edge::Move => {
                        let start = self.snap(start as f64 + dt);
                        // To the track under the pointer (its own, above the first or past the
                        // last), however tall each is.
                        let _ = (from, track);
                        let rel = p.y - f32::from(self.lanes_bounds.get().origin.y);
                        let last = self.session.project.tracks.len().saturating_sub(1);
                        let to = self
                            .track_at_y(rel)
                            .unwrap_or(if rel < 0.0 { 0 } else { last });
                        let now = self.session.project.move_region(at, to, start);
                        if let Some(Drag::Region { at, .. }) = &mut self.drag {
                            *at = now;
                        }
                        self.region = Some(now);
                        self.select_track(now.0);
                    }
                    Edge::Start => {
                        let start = self.snap(start as f64 + dt);
                        self.session.project.trim_start(at, start);
                    }
                    Edge::End => {
                        let end = self.snap(end as f64 + dt);
                        self.session.project.trim_end(at, end);
                    }
                    // MIDI and patterns stretch from how they were, so rounding doesn't build up.
                    Edge::Stretch if self.stretching.is_some() => {
                        let length = self.snap(end as f64 + dt).max(start + 1) - start;
                        let ratio = length as f64 / (end - start).max(1) as f64;
                        let made = self.stretching.as_ref().and_then(|c| c.stretched(ratio));
                        if let Some(content) = made {
                            let r = &mut self.session.project.tracks[at.0].regions[at.1];
                            r.content = content;
                            r.length = length;
                            r.link = None;
                        }
                        self.status = Some(format!("Time stretch ×{ratio:.2}"));
                    }
                    // Stretching audio is slow, so the engine hears of it when the drag ends.
                    Edge::Stretch => {
                        let length = self.snap(end as f64 + dt).max(start + 1) - start;
                        let _ = self.session.project.flex(at, length);
                        let ratio = length as f64 / (end - start).max(1) as f64;
                        self.status = Some(format!("Flex ×{ratio:.2}"));
                        cx.notify();
                        return;
                    }
                    // Loops to the grid line nearest the pointer; back inside the region, none.
                    Edge::Loop => {
                        let to = self.snap(self.tick_at(p.x));
                        let r = &mut self.session.project.tracks[at.0].regions[at.1];
                        r.looped = to > r.start + r.length;
                        r.loop_length = r.looped.then(|| to - r.start);
                    }
                }
            }
            Some(Drag::Fader {
                channel,
                y,
                pos,
                height,
            }) => {
                let db = strip::fader_db((pos - (p.y - y) / height).clamp(0.0, 1.0));
                self.set_volume(channel, db);
                if let Channel::Track(t) = channel {
                    self.moved(t, Param::Volume, db);
                }
            }
            Some(Drag::Pan { channel, x, y, pan }) => {
                let (dx, dy) = (p.x - x, y - p.y);
                let by = if dx.abs() > dy.abs() { dx } else { dy };
                let mut to = (pan + by * if self.fine { 0.002 } else { 0.01 }).clamp(-1.0, 1.0);
                // A catch at the middle, as it's crossed.
                if pan != 0.0 && to != 0.0 && pan.signum() != to.signum() {
                    to = 0.0;
                }
                if let Some(Drag::Pan { x, y, pan, .. }) = &mut self.drag {
                    (*x, *y, *pan) = (p.x, p.y, to);
                }
                let pan = to;
                self.set_pan(channel, pan);
                if let Channel::Track(t) = channel {
                    self.moved(t, Param::Pan, pan);
                }
            }
            Some(Drag::Send {
                track,
                index,
                y,
                level,
            }) => {
                let db = (level - (p.y - y) / 2.0).clamp(strip::SILENT, 6.0);
                self.session.project.tracks[track].sends[index].level_db = db;
                let bus = self.session.project.tracks[track].sends[index].bus;
                self.moved(track, Param::Send(bus), db);
            }
            Some(Drag::Tempo { y, bpm }) => {
                let to = (bpm + ((y - p.y) / 4.0) as f64).round().clamp(20.0, 400.0);
                if to != self.session.project.tempo {
                    self.session.project.set_tempo(to);
                }
            }
            Some(Drag::Locate) => self.locate_at(p.x, cx),
            Some(Drag::Notes { .. }) => self.drag_notes(p),
            Some(Drag::Marquee { .. }) => {
                self.drag_marquee(p);
                cx.notify();
                return;
            }
            Some(Drag::Paint { .. }) => self.drag_paint(p),
            Some(Drag::Velocity { at, y }) => {
                let dv = -(p.y - y) * 127.0 / crate::pianoroll::VELOCITY_H;
                let held = self.held.clone();
                if let Some(notes) = self.session.project.notes_mut(at) {
                    for (i, was) in held {
                        if let Some(n) = notes.get_mut(i) {
                            n.velocity = (was.velocity as f32 + dv).round().clamp(1.0, 127.0) as u8;
                        }
                    }
                }
            }
            Some(Drag::VelocityLine { .. }) => self.drag_velocity_line(p),
            Some(Drag::Control { .. }) => self.drag_control(p),
            Some(Drag::Mark { .. }) => self.drag_mark(p),
            Some(Drag::Width { side, x, width }) => {
                let w = width + (p.x - x);
                match side {
                    true => self.inspector_w = w.clamp(220.0, 520.0),
                    false => self.header_w = w.clamp(280.0, 520.0),
                }
                cx.notify();
                return;
            }
            Some(Drag::LaneHeight { track, y, height }) => {
                if let Some(t) = self.session.project.tracks.get_mut(track) {
                    t.height = Some((height + (p.y - y)).clamp(MIN_LANE_H, MAX_LANE_H).round());
                }
                cx.notify();
                return;
            }
            Some(Drag::HFader {
                track,
                x,
                pos,
                width,
            }) => {
                let fine = if self.fine { 0.2 } else { 1.0 };
                let pos = (pos + (p.x - x) / width.max(1.0) * fine).clamp(0.0, 1.0);
                if let Some(Drag::HFader { x, pos: was, .. }) = &mut self.drag {
                    (*x, *was) = (p.x, pos);
                }
                let db = strip::fader_db(pos);
                self.set_volume(Channel::Track(track), db);
                self.moved(track, Param::Volume, db);
            }
            Some(Drag::Orbit { x, y, yaw, pitch }) => {
                self.orbit = (
                    yaw + (p.x - x) * 0.4,
                    (pitch + (p.y - y) * 0.3).clamp(4.0, 88.0),
                );
                cx.notify();
                return;
            }
            Some(Drag::Source { track, height }) => self.drag_source(track, height, p),
            Some(Drag::Modulator { .. }) => {
                self.mod_drag_at = Some(p);
                cx.notify();
                return;
            }
            Some(Drag::ModDepth {
                track,
                index,
                target,
                x,
                y,
                depth,
            }) => self.drag_depth((track, index, target), (x, y), depth, p),
            Some(Drag::ModValue {
                track,
                index,
                field,
                y,
                value,
            }) => {
                let fine = if self.fine { 0.2 } else { 1.0 };
                self.set_mod_field(track, index, field, value + (y - p.y) / 150.0 * fine);
                // Turned while its automation records, it's recorded.
                if let Some(setting) = crate::modulation::setting_of(field) {
                    let param = pina_engine::project::Param::Mod(index as u8, setting);
                    let now = self.own_value(track, param);
                    self.moved(track, param, now);
                }
            }
            Some(Drag::ModSteps { track, index }) => self.paint_mod_step(track, index, p),
            Some(Drag::ZoomSpan) => {
                if let Some((_, b)) = &mut self.zoom_span {
                    *b = p.x;
                }
                cx.notify();
                return;
            }
            Some(Drag::SoloHold) => return,
            Some(Drag::Fade { at, out }) => self.drag_fade(at, out, p),
            Some(Drag::Raise { track, y, height }) => {
                self.raise_source(track, height + (y - p.y) / 80.0)
            }
            Some(Drag::Spread {
                track,
                x,
                y,
                spread,
            }) => {
                let (dx, dy) = (p.x - x, y - p.y);
                let by = if dx.abs() > dy.abs() { dx } else { dy };
                let to = (spread + by * if self.fine { 0.002 } else { 0.01 }).clamp(-1.0, 1.0);
                if let Some(Drag::Spread { x, y, spread, .. }) = &mut self.drag {
                    (*x, *y, *spread) = (p.x, p.y, to);
                }
                if let Some(t) = self.session.project.tracks.get_mut(track) {
                    t.spread = Some(to);
                }
            }
            // Each step, and each bar of the lane, paints itself as the mouse comes over it.
            Some(Drag::Steps { .. } | Drag::StepVelocity) => return,
            Some(Drag::Auto {
                track,
                param,
                k,
                index,
                ..
            }) => self.drag_points(track, param, k, index, p),
            Some(Drag::AutoLine {
                track, param, k, y, ..
            }) => self.drag_line(track, param, k, y, p),
            Some(Drag::AutoBend {
                track,
                param,
                index,
                y,
                curve,
            }) => self.drag_bend(track, param, index, y, curve, p),
            Some(Drag::AutoMarquee { .. }) => {
                if let Some(Drag::AutoMarquee { to, .. }) = &mut self.drag {
                    *to = (p.x, p.y);
                }
                cx.notify();
                return;
            }
            Some(Drag::AutoDraw { track, param, last }) => self.drag_draw(track, param, last, p),
            Some(Drag::Comp { at, .. }) => {
                let tick = self.region_tick(at, p.x);
                if let Some(Drag::Comp { to, .. }) = &mut self.drag {
                    *to = tick;
                }
                cx.notify();
                return;
            }
            Some(Drag::Cycle { anchor, .. }) => {
                let here = self.snap(self.tick_at(p.x));
                let (start, end) = (anchor.min(here), anchor.max(here));
                if end > start {
                    self.session.project.cycle = Some(Cycle {
                        start,
                        end,
                        off: false,
                    });
                    // A stretch swept out is what plays: the song, not one region's pattern.
                    self.pattern_mode = false;
                }
            }
            Some(Drag::Marker { index, x, tick, .. }) => {
                let to = self.snap(tick as f64 + ((p.x - x) / ppt) as f64);
                let markers = &mut self.session.project.markers;
                if let Some(m) = markers.get_mut(index) {
                    m.tick = to;
                }
            }
            Some(Drag::Knob { target, y, value }) => {
                let v = (value - (p.y - y) / 150.0).clamp(0.0, 1.0);
                self.set_target(target, v);
                if let Some((t, param)) = self.target_param(target) {
                    self.moved(t, param, v);
                }
            }
            Some(Drag::Slider {
                slider,
                left,
                width,
            }) => {
                let v = ((p.x - left) / width.max(1.0)).clamp(0.0, 1.0);
                self.set_slider(slider, v);
                if let Slider::Send { track, index } = slider {
                    let s = &self.session.project.tracks[track].sends[index];
                    let (bus, db) = (s.bus, s.level_db);
                    self.moved(track, Param::Send(bus), db);
                }
                if !matches!(slider, Slider::Send { .. }) {
                    cx.notify();
                    return;
                }
            }
            Some(Drag::EqNode {
                id,
                from,
                q,
                alt,
                axis,
                ..
            }) => {
                let axis = self.drag_eq_node(id, from, q, alt, axis, p);
                if let Some(Drag::EqNode { axis: a, moved, .. }) = &mut self.drag {
                    *a = axis;
                    *moved |= (p.x - from.x).hypot(p.y - from.y) > 2.0;
                }
            }
            Some(Drag::EqPress {
                from,
                on_curve,
                alt,
                ..
            }) => {
                if let Some(Drag::EqPress { moved, .. }) = &mut self.drag {
                    *moved |= (p.x - from.x).hypot(p.y - from.y) > 3.0;
                }
                self.drag_eq_press(from, on_curve, alt, p, cx);
                cx.notify();
                return;
            }
            Some(Drag::EqSolo { id, from, freq }) => self.drag_eq_solo(id, from, freq, p),
            Some(Drag::EqScale { from, span }) => {
                self.drag_eq_scale(from, span, p);
                cx.notify();
                return;
            }
            Some(Drag::EqSketch) => {
                self.drag_eq_sketch(p);
                cx.notify();
                return;
            }
            Some(Drag::EqKnob {
                id,
                knob,
                from,
                start,
                ..
            }) => self.drag_eq_knob(id, knob, from, start, p.y),
            Some(Drag::EqListBand { id, frame }) => self.drag_list_band(id, frame, p),
            Some(Drag::EqWindow {
                from,
                start,
                resize,
            }) => {
                self.drag_eq_window(from, start, resize, p);
                cx.notify();
                return;
            }
            Some(Drag::Editor { y, height }) => {
                self.editor_h = height - (p.y - y);
                self.editor_h = self.editor_height();
                cx.notify();
                return;
            }
            Some(Drag::Panel { x, width }) => {
                self.panel_w = width - (p.x - x);
                self.panel_w = self.panel_width();
                cx.notify();
                return;
            }
            Some(Drag::Sample { .. } | Drag::MapBox { .. }) => {
                self.drag_sample(p, cx);
                return;
            }
            Some(Drag::Device { .. }) => {
                if let Some(Drag::Device {
                    from, at, moved, ..
                }) = &mut self.drag
                {
                    *at = p;
                    *moved |= (p.x - from.x).hypot(p.y - from.y) > 4.0;
                }
                cx.notify();
                return;
            }
            Some(Drag::Warp { at, from }) => self.drag_warp(at, from, p.x),
            Some(Drag::Key { .. }) | None => return,
        }
        self.changed(cx);
    }

    /// The automatable parameter a knob turns, if it is one: a track and its parameter.
    fn target_param(&self, target: Target) -> Option<(usize, Param)> {
        let Target::Param { track, param } = target else {
            return None;
        };
        let t = self
            .session
            .project
            .tracks
            .iter()
            .position(|t| t.id == track)?;
        Some((t, Param::Plugin(Which::Instrument, param)))
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        // With "Automation moves with regions", a region moved takes what's under it along.
        if self.settings.carry_automation
            && let Some(Drag::Region {
                at,
                edge: Edge::Move,
                start,
                end,
                track,
                moved: true,
                copy,
                ..
            }) = self.drag
            && let Some(r) = self
                .session
                .project
                .tracks
                .get(at.0)
                .and_then(|t| t.regions.get(at.1))
        {
            let to = (at.0, r.start);
            self.carry_automation((track, start, end), to, copy);
            self.changed(cx);
        }
        match self.drag {
            Some(Drag::ZoomSpan) => self.zoom_release(cx),
            Some(Drag::SoloHold) => self.solo_release(cx),
            // A dot or knob clicked, not moved: no edit to undo; a dot's click may do something.
            Some(
                d @ (Drag::EqNode { .. }
                | Drag::EqKnob { .. }
                | Drag::EqSolo { .. }
                | Drag::EqListBand { .. }),
            ) => {
                let still = self.session.unchanged();
                if still {
                    self.session.cancel();
                }
                // A slope clicked rather than dragged: its menu.
                if let Drag::EqKnob {
                    id,
                    knob: crate::eq::EqKnob::Slope,
                    from,
                    x,
                    ..
                } = d
                    && still
                {
                    self.drag = None;
                    self.eq_slope_menu(id, gpui::point(px(x), px(from)), cx);
                }
                if let Some(v) = self.eq_view.as_mut() {
                    v.solo = None;
                    v.grabbing = None;
                }
                self.eq_let_go();
                if let Drag::EqNode {
                    id,
                    alt,
                    moved: false,
                    deselect,
                    ..
                } = d
                {
                    self.drag = None;
                    self.release_eq_node(id, alt, deselect, cx);
                }
            }
            Some(Drag::EqPress {
                from, moved, alt, ..
            }) => {
                self.drag = None;
                self.release_eq_press(from, moved, alt, cx);
            }
            Some(Drag::EqSketch) => {
                self.drag = None;
                self.finish_sketch(cx);
            }
            _ => {}
        }
        if let Some(Drag::Modulator { track, index }) = self.drag
            && let Some(at) = self.mod_drag_at.take()
        {
            self.drop_modulator(track, index, at, cx);
        }
        if let Some(r) = self.steps_pick.take() {
            self.steps_row = r;
        }
        // A lane made taller is the song's to keep (as Logic keeps it), not an undo step.
        if let Some(Drag::LaneHeight { .. }) = self.drag {
            self.session.edited = true;
        }
        // An option-click that didn't drag leaves no copy behind.
        if let Some(Drag::Region {
            at,
            moved: false,
            copy: true,
            ..
        }) = self.drag
        {
            self.session.project.tracks[at.0].regions.remove(at.1);
            self.region = None;
            self.changed(cx);
        }
        // Letting go of a control ends a Touch pass.
        let released = match self.drag {
            Some(Drag::Fader {
                channel: Channel::Track(t),
                ..
            })
            | Some(Drag::HFader { track: t, .. }) => Some((t, Param::Volume)),
            Some(Drag::Pan {
                channel: Channel::Track(t),
                ..
            }) => Some((t, Param::Pan)),
            Some(Drag::Knob { target, .. }) => self.target_param(target),
            Some(Drag::Send { track, index, .. })
            | Some(Drag::Slider {
                slider: Slider::Send { track, index },
                ..
            }) => {
                let bus = self.session.project.tracks[track].sends[index].bus;
                Some((track, Param::Send(bus)))
            }
            Some(Drag::ModValue {
                track,
                index,
                field,
                ..
            }) => crate::modulation::setting_of(field).map(|s| (track, Param::Mod(index as u8, s))),
            _ => None,
        };
        // A hand on a control lets go with the mouse: the engine hears its automation again.
        let held = !self.touches.is_empty()
            || self
                .taken
                .values()
                .any(|l| *l == crate::automation::Live::Held);
        if let Some((t, param)) = released {
            self.let_go(t, param);
        }
        if held {
            self.let_go_all();
            self.changed(cx);
        }
        match self.drag {
            Some(Drag::AutoMarquee {
                track,
                k,
                from,
                to,
                erase,
                free,
            }) => self.marquee_done(track, k, from, to, erase, free, cx),
            Some(Drag::AutoDraw { track, param, .. }) => {
                self.draw_done(track, param);
                self.changed(cx);
            }
            _ => {}
        }
        if let Some(
            Drag::Region { moved: false, .. }
            | Drag::Notes { moved: false, .. }
            | Drag::Auto { moved: false, .. }
            | Drag::AutoLine { moved: false, .. }
            | Drag::Marker { moved: false, .. },
        ) = self.drag
        {
            self.session.cancel()
        }
        if let Some(Drag::Marker {
            index,
            moved: false,
            ..
        }) = self.drag
            && let Some(m) = self.session.project.markers.get(index)
        {
            let at = self.session.project.samples(m.tick, self.player.rate());
            self.player.locate(at);
        }
        if let Some(Drag::Marker { moved: true, .. }) = self.drag {
            self.session.project.markers.sort_by_key(|m| m.tick);
        }
        // A click in the cycle strip, without sweeping, turns the cycle on or off.
        if let Some(Drag::Cycle {
            anchor,
            moved: false,
        }) = self.drag
        {
            let bar = self.session.project.bar();
            let cycle = &mut self.session.project.cycle;
            match cycle {
                Some(c) => c.off = !c.off,
                None => {
                    let start = anchor / bar * bar;
                    *cycle = Some(Cycle {
                        start,
                        end: start + bar * 4,
                        off: false,
                    });
                }
            }
            self.changed(cx);
        }
        if let Some(Drag::Comp { at, take, from, to }) = self.drag {
            self.drag = None;
            self.finish_comp(at, take, from, to, cx);
        }
        if let Some(Drag::Region {
            edge: Edge::Stretch,
            moved,
            ..
        }) = self.drag
        {
            self.stretching = None;
            if moved {
                self.changed(cx);
            }
        }
        if let Some(Drag::Sample { .. } | Drag::MapBox { .. }) = self.drag {
            let drag = self.drag.take();
            self.drop_sample(drag, cx);
        }
        if let Some(Drag::Device { .. }) = self.drag {
            let drag = self.drag.take();
            self.drop_device(drag, cx);
        }
        // Release whatever note the drag was sounding.
        match self.drag {
            Some(Drag::Key { track, pitch }) => self.player.preview(track, pitch, 0),
            Some(Drag::Notes { at, lead, .. }) => {
                let track = self.session.project.tracks[at.0].id;
                if let Some(n) = self.session.project.notes_mut(at).and_then(|n| n.get(lead)) {
                    let pitch = n.pitch;
                    self.player.preview(track, pitch, 0);
                }
            }
            Some(Drag::Paint { at, .. }) => {
                let track = self.session.project.tracks[at.0].id;
                for pitch in std::mem::take(&mut self.painting) {
                    self.player.preview(track, pitch, 0);
                }
            }
            _ => {}
        }
        self.drag = None;
        cx.notify();
    }

    /// Moves the playhead to the grid line nearest a window x position.
    pub fn locate_at(&mut self, x: f32, cx: &mut Context<Self>) {
        let project = &self.session.project;
        let tick = self.snap(self.tick_at(x));
        self.player
            .locate(project.samples(tick, self.player.rate()));
        cx.notify();
    }

    pub fn locate_tick(&mut self, tick: u64, cx: &mut Context<Self>) {
        let at = self.session.project.samples(tick, self.player.rate());
        self.player.locate(at);
        cx.notify();
    }

    fn scroll(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => p.map(f32::from),
            ScrollDelta::Lines(l) => l.map(|v| v * 24.0),
        };
        if e.modifiers.platform {
            // Zoom around the pointer.
            let x = f32::from(e.position.x) - f32::from(self.lanes_bounds.get().origin.x);
            self.zoom_around(self.px_per_beat * (1.0 + d.y * 0.01), x);
        } else {
            let height = f32::from(self.lanes_bounds.get().size.height);
            let content = self.rows_h();
            self.scroll_x = (self.scroll_x - d.x).max(0.0);
            self.scroll_y = (self.scroll_y - d.y).clamp(0.0, (content - height + LANE_H).max(0.0));
        }
        cx.notify();
    }

    /// Zooms to `px_per_beat`, keeping what's `x` pixels into the lanes where it is.
    pub fn zoom_around(&mut self, px_per_beat: f32, x: f32) {
        let tick = (x + self.scroll_x) / self.px_per_tick();
        self.px_per_beat = px_per_beat.clamp(4.0, 800.0);
        self.scroll_x = (tick * self.px_per_tick() - x).max(0.0);
    }

    pub fn on_scroll(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static {
        cx.listener(|this, e, _, cx| this.scroll(e, cx))
    }

    pub fn play_stop(&mut self, cx: &mut Context<Self>) {
        if self.player.playing() || self.recording.is_some() {
            self.stop(cx);
            // Back where play started, as Logic's Return to Last Play Position.
            if self.settings.return_on_stop {
                let from = self.play_from;
                self.locate_tick(from, cx);
            }
        } else {
            self.play_from = self.playhead();
            self.player.play();
        }
        cx.notify();
    }

    /// Stops playing, and recording if it was.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.player.stop();
        self.finish_recording(cx);
        cx.notify();
    }

    pub fn go_to_start(&mut self, cx: &mut Context<Self>) {
        self.player.locate(0);
        self.scroll_x = 0.0;
        cx.notify();
    }

    /// Turns the cycle on or off. With a region selected that the cycle isn't already round, it
    /// cycles that region; with no cycle yet and nothing selected, four bars from the playhead's
    /// bar.
    pub fn toggle_cycle(&mut self, cx: &mut Context<Self>) {
        let bar = self.session.project.bar();
        let selected = self.region.map(|at| {
            let r = self.session.project.region(at);
            (r.start, r.start + r.length)
        });
        let current = self.session.project.cycle;
        let (start, end) = match selected {
            Some(span) => span,
            None => {
                let start = self.playhead() / bar * bar;
                (start, start + bar * 4)
            }
        };
        let moves = selected.is_some_and(|s| current.is_none_or(|c| (c.start, c.end) != s));
        self.edit(cx, |ws| match &mut ws.session.project.cycle {
            Some(c) if !moves => c.off = !c.off,
            cycle => {
                *cycle = Some(Cycle {
                    start,
                    end,
                    off: false,
                });
                ws.pattern_mode = false;
            }
        });
    }

    pub fn toggle_metronome(&mut self, cx: &mut Context<Self>) {
        self.player.set_metronome(!self.player.metronome());
        cx.notify();
    }

    pub fn set_pattern_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        if on && self.region.is_none() {
            self.status = Some("Select a region to play it as a pattern.".into());
            cx.notify();
            return;
        }
        self.pattern_mode = on;
        if on && let Some(at) = self.region {
            let r = self.session.project.region(at);
            let start = r.start;
            self.pattern = Some((at, r.start, r.length));
            self.locate_tick(start, cx);
        }
        self.changed(cx);
    }

    /// In Pattern mode, selecting another region plays it straight away, from the same place in
    /// its loop as the last one was at (or its start, if it's longer than that).
    fn follow_pattern(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.region.filter(|_| self.pattern_mode) else {
            return;
        };
        if self.pattern.is_some_and(|(p, ..)| p == at) {
            return;
        }
        let tick = self.playhead();
        let into = self.pattern.map_or(0, |(_, start, length)| {
            tick.saturating_sub(start) % length.max(1)
        });
        let r = self.session.project.region(at);
        let (start, length) = (r.start, r.length.max(1));
        self.pattern = Some((at, start, length));
        self.changed(cx);
        self.locate_tick(start + into % length, cx);
    }

    pub(crate) fn undo(&mut self, cx: &mut Context<Self>) {
        if self.session.undo() {
            self.changed(cx);
        }
    }

    pub(crate) fn redo(&mut self, cx: &mut Context<Self>) {
        if self.session.redo() {
            self.changed(cx);
        }
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        // On the Songs page: the songs picked, to the Trash (asked first).
        if self.view == View::Songs {
            if self.home.as_ref().is_some_and(|h| !h.picks.is_empty()) {
                self.song_pending = Some(crate::menu::MenuAction::TrashPicked);
                cx.notify();
            }
            return;
        }
        if self.delete_eq_band(cx) || self.delete_picked(cx) {
            return;
        }
        let notes_shown = matches!(self.bottom, Some(Bottom::PianoRoll | Bottom::Score));
        if let (Some(at), false, true) = (self.region, self.picked.is_empty(), notes_shown) {
            self.delete_notes(at, cx);
            return;
        }
        let Some((t, r)) = self.region else { return };
        self.edit(cx, |ws| {
            ws.session.project.tracks[t].regions.remove(r);
            ws.region = None;
        });
    }

    pub fn delete_track(&mut self, cx: &mut Context<Self>) {
        if self.session.project.tracks.is_empty() {
            return;
        }
        let t = self.track;
        self.edit(cx, |ws| {
            ws.session.project.tracks.remove(t);
            ws.region = None;
        });
    }

    /// Splits the selected region at the playhead, as Logic's ⌘T does.
    fn split(&mut self, cx: &mut Context<Self>) {
        let (Some(at), tick) = (self.region, self.playhead()) else {
            return;
        };
        self.split_at(at, tick, cx);
    }

    pub fn split_at(&mut self, at: At, tick: u64, cx: &mut Context<Self>) {
        self.session.checkpoint();
        match self.session.project.split_region(at, tick) {
            Some(second) => {
                self.region = Some(second);
                self.changed(cx);
            }
            None => self.session.cancel(),
        }
    }

    /// Adds a track after the selected one: an instrument track (the synth, the drum kit) or,
    /// without an instrument, an audio track.
    pub fn new_track(&mut self, instrument: Option<Instrument>, cx: &mut Context<Self>) {
        self.new_track_named(instrument, None, cx);
    }

    /// A new track after the selected one, selected (so it plays what's played on a keyboard);
    /// called `name`, or after its instrument.
    pub fn new_track_named(
        &mut self,
        instrument: Option<Instrument>,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let n = self.session.project.tracks.len();
        let at = if n == 0 { 0 } else { self.track + 1 };
        let name = name.unwrap_or_else(|| match &instrument {
            Some(Instrument::Drums) => format!("Drums {}", n + 1),
            Some(Instrument::Plugin(p)) => p.class.clone(),
            Some(Instrument::Sampler(_)) => format!("Sampler {}", n + 1),
            Some(Instrument::Synth) => format!("Synth {}", n + 1),
            None => format!("Audio {}", n + 1),
        });
        self.edit(cx, |ws| {
            let track = Track {
                name,
                instrument,
                ..Default::default()
            };
            ws.session.project.add_track(at, track);
            ws.select_track(at);
            ws.region = None;
        });
    }

    pub fn toggle_bottom(&mut self, pane: Bottom, cx: &mut Context<Self>) {
        self.view = View::Arrange;
        self.bottom = if self.bottom == Some(pane) {
            None
        } else {
            Some(pane)
        };
        if self.bottom == Some(Bottom::PianoRoll) {
            self.focus_roll();
        }
        cx.notify();
    }

    pub fn show_bottom(&mut self, pane: Bottom, cx: &mut Context<Self>) {
        self.view = View::Arrange;
        self.bottom = Some(pane);
        // The Sampler wants room for its waveform, keys and knobs.
        if pane == Bottom::Sampler {
            self.editor_h = self.editor_h.max(500.0);
        }
        if pane == Bottom::PianoRoll {
            self.focus_roll();
        }
        cx.notify();
    }

    pub fn toggle_mixer(&mut self, cx: &mut Context<Self>) {
        self.view = match self.view {
            View::Mixer => View::Arrange,
            _ => View::Mixer,
        };
        cx.notify();
    }

    fn toggle_typing(&mut self, cx: &mut Context<Self>) {
        self.typing = if self.typing.is_some() { None } else { Some(0) };
        cx.notify();
    }

    /// Musical typing, as in Logic: A to K are white keys from C, W E T Y U O P the black ones;
    /// Z and X shift the octave.
    fn type_key(&mut self, e: &KeyDownEvent, window: &Window, cx: &mut Context<Self>) {
        // Typing in a field (a search, a question for the agent) is typing, not notes.
        if self.browser.searching || self.typing_in_field(window, cx) {
            return;
        }
        if self.naming.is_none() && self.menu_key(e, cx) {
            cx.stop_propagation();
            return;
        }
        let Some(octave) = self.typing else { return };
        let key = e.keystroke.key.as_str();
        if e.keystroke.modifiers.modified() {
            return;
        }
        match key {
            "z" => self.typing = Some((octave - 1).max(-4)),
            "x" => self.typing = Some((octave + 1).min(4)),
            _ => {
                const KEYS: &str = "awsedftgyhujkolp;'";
                let Some(i) = KEYS.find(key).filter(|_| key.len() == 1) else {
                    return;
                };
                if !e.is_held {
                    let pitch = (60 + i as i32 + octave as i32 * 12).clamp(0, 127) as u8;
                    self.typed.insert(key.to_string(), pitch);
                    self.player.send_live(LiveNote {
                        pitch,
                        velocity: 100,
                    });
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn release_key(&mut self, e: &KeyUpEvent, _: &mut Context<Self>) {
        if let Some(pitch) = self.typed.remove(e.keystroke.key.as_str()) {
            self.player.send_live(LiveNote { pitch, velocity: 0 });
        }
    }

    /// Moves or resizes a note in the piano roll by `d` pixels from where the drag began.
    fn zoom(&mut self, by: f32, cx: &mut Context<Self>) {
        self.px_per_beat = (self.px_per_beat * by).clamp(4.0, 800.0);
        cx.notify();
    }

    /// A channel's built-in EQ and compressor.
    pub fn strip_dsp(&self, ch: Channel) -> (Option<Eq>, Option<Comp>) {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => (p.tracks[i].eq, p.tracks[i].comp),
            Channel::Aux(i) => (p.auxes[i].eq, p.auxes[i].comp),
            Channel::Master => (p.master_eq, p.master_comp),
        }
    }

    pub fn strip_dsp_mut(&mut self, ch: Channel) -> (&mut Option<Eq>, &mut Option<Comp>) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => {
                let t = &mut p.tracks[i];
                (&mut t.eq, &mut t.comp)
            }
            Channel::Aux(i) => {
                let a = &mut p.auxes[i];
                (&mut a.eq, &mut a.comp)
            }
            Channel::Master => (&mut p.master_eq, &mut p.master_comp),
        }
    }

    /// A knob's value now, from 0 to 1.
    pub fn target(&self, target: Target) -> f32 {
        use crate::strip::{eq_gain_pos, low_cut_pos};
        match target {
            Target::Param { track, param } => {
                self.player
                    .param_value(track, pina_engine::player::Which::Instrument, param)
                    as f32
            }
            Target::LowCut(ch) => {
                low_cut_pos(self.strip_dsp(ch).0.map_or(0.0, |e| crate::eq::low_cut(&e)))
            }
            Target::Quick(ch, q) => eq_gain_pos(self.strip_dsp(ch).0.map_or(0.0, |e| q.gain(&e))),
            Target::Comp(ch, field) => {
                crate::strip::comp_pos(field, &self.strip_dsp(ch).1.unwrap_or_default())
            }
            Target::Sat(ch, field) => {
                crate::strip::sat_pos(field, &self.sat(ch).unwrap_or_default())
            }
            Target::Sampler { track, field } => self.sampler_pos(track, field),
        }
    }

    /// Turns a knob to `v` (0 to 1); a channel's EQ or compressor appears if it had none.
    pub fn set_target(&mut self, target: Target, v: f32) {
        use crate::strip::{comp_set, eq_gain_db, low_cut_hz};
        match target {
            // In the project, so it's undone like any edit; the player passes it on.
            Target::Param { track, param } => {
                let tracks = &mut self.session.project.tracks;
                if let Some(r) = tracks
                    .iter_mut()
                    .find(|t| t.id == track)
                    .and_then(|t| t.plugin_mut(Which::Instrument))
                {
                    r.params.insert(param, v as f64);
                }
            }
            Target::LowCut(ch) => crate::eq::set_low_cut(
                self.strip_dsp_mut(ch).0.get_or_insert_default(),
                low_cut_hz(v),
            ),
            Target::Quick(ch, q) => {
                let eq = self.strip_dsp_mut(ch).0.get_or_insert_default();
                if let Some(i) = q.band(eq) {
                    eq.bands[i].gain_db = eq_gain_db(v);
                }
            }
            Target::Comp(ch, field) => {
                comp_set(field, self.strip_dsp_mut(ch).1.get_or_insert_default(), v)
            }
            Target::Sampler { track, field } => self.sampler_set(track, field, v),
            Target::Sat(ch, field) => {
                crate::strip::sat_set(field, self.sat_mut(ch).get_or_insert_default(), v)
            }
        }
    }

    pub fn slider(&self, slider: Slider) -> f32 {
        match slider {
            Slider::Strength => self.roll.strength,
            Slider::Swing => (self.roll.swing - 50.0) / 25.0,
            Slider::Zoom => (self.px_per_beat / 4.0).ln() / 200f32.ln(),
            Slider::Send { track, index } => {
                let bus = self.session.project.tracks[track].sends[index].bus;
                strip::fader_pos(self.now_value(track, Param::Send(bus)))
            }
            Slider::Aspect(i) => *crate::browser::aspect(&mut self.browser.aspects.clone(), i),
            Slider::Azimuth(t) | Slider::Elevation(t) | Slider::Distance(t) => {
                let Some(place) = self.place_of(t) else {
                    return 0.0;
                };
                match slider {
                    Slider::Azimuth(_) => (place.azimuth / 180.0 + 1.0) / 2.0,
                    Slider::Elevation(_) => (place.elevation + 90.0) / 180.0,
                    _ => (place.distance - Place::NEAREST) / (Place::FARTHEST - Place::NEAREST),
                }
            }
        }
    }

    pub fn set_slider(&mut self, slider: Slider, v: f32) {
        match slider {
            Slider::Strength => self.roll.strength = (v * 100.0).round() / 100.0,
            Slider::Swing => self.roll.swing = (50.0 + v * 25.0).round(),
            Slider::Zoom => {
                let width = f32::from(self.lanes_bounds.get().size.width);
                self.zoom_around(4.0 * 200f32.powf(v), width / 2.0)
            }
            Slider::Send { track, index } => {
                self.session.project.tracks[track].sends[index].level_db = strip::fader_db(v)
            }
            Slider::Aspect(i) => {
                *crate::browser::aspect(&mut self.browser.aspects, i) = (v * 20.0).round() / 20.0;
                self.find_similar();
            }
            Slider::Azimuth(t) | Slider::Elevation(t) | Slider::Distance(t) => {
                let Some(mut place) = self.place_of(t) else {
                    return;
                };
                match slider {
                    Slider::Azimuth(_) => place.azimuth = ((v * 2.0 - 1.0) * 180.0).round(),
                    Slider::Elevation(_) => place.elevation = (v * 180.0 - 90.0).round(),
                    _ => {
                        let d = Place::NEAREST + v * (Place::FARTHEST - Place::NEAREST);
                        place.distance = (d * 20.0).round() / 20.0;
                    }
                }
                self.set_place(t, place);
            }
        }
    }

    /// Starts dragging a slider, setting it to where it was clicked.
    pub fn start_slider(&mut self, slider: Slider, p: Point<f32>, cx: &mut Context<Self>) {
        let Some(b) = self.slider_bounds.borrow().get(&slider).copied() else {
            return;
        };
        let (left, width) = (f32::from(b.origin.x), f32::from(b.size.width));
        self.begin_drag(
            Drag::Slider {
                slider,
                left,
                width,
            },
            cx,
        );
        self.drag_to(p, cx);
    }

    pub fn save(&mut self, cx: &mut Context<Self>) -> bool {
        let result = self
            .player
            .save_states(&mut self.session.project)
            .and_then(|()| self.session.save());
        if result.is_ok() {
            self.session.forget_kept();
        }
        self.status = Some(match &result {
            Ok(()) => format!("Saved {}", self.session.dir.display()),
            Err(e) => e.clone(),
        });
        cx.notify();
        result.is_ok()
    }

    /// Keeps unsaved edits in the project's recovery folder, for if Pina Studio stops before they're
    /// saved: it's offered them when the project next opens.
    pub fn keep_edits(&mut self) {
        if !self.session.edited || self.session.kept {
            return;
        }
        let mut project = self.session.project.clone();
        let states = Path::new(RECOVERY_DIR).join(STATES_DIR);
        let result = self
            .player
            .save_states_in(&mut project, &states)
            .and_then(|()| self.session.keep(&project));
        if let Err(e) = result {
            self.status = Some(format!("Couldn't keep unsaved edits: {e}"));
        }
    }

    /// Asks whether to restore edits kept from when Pina Studio last stopped without saving them.
    pub fn offer_kept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.session.kept_at() else {
            return;
        };
        let detail = format!(
            "Pina Studio closed before they were saved. They were kept {}.",
            ago(at)
        );
        // After the window is up, for the question to sit on it.
        cx.spawn_in(window, async move |this, cx| {
            let Ok(answer) = this.update_in(cx, |_, window, cx| {
                window.prompt(
                    PromptLevel::Warning,
                    "Restore your unsaved changes to this project?",
                    Some(&detail),
                    &["Restore", "Discard"],
                    cx,
                )
            }) else {
                return;
            };
            let Ok(choice) = answer.await else { return };
            this.update(cx, |ws, cx| {
                if choice == 0 {
                    ws.restore_kept(cx);
                } else {
                    ws.session.forget_kept();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Opens the kept edits in place of the saved project (undo goes back to it).
    pub fn restore_kept(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.session.restore() {
            self.status = Some(format!("Couldn't restore unsaved changes: {e}"));
            cx.notify();
            return;
        }
        // Every plugin anew, from the settings kept with the edits.
        let rate = self.session.project.sample_rate;
        let _ = self.player.sync(&Project::new(rate));
        self.changed(cx);
        self.status = Some("Restored unsaved changes".into());
    }

    /// Quits, asking first if there are unsaved changes.
    fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.edited {
            cx.quit();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "Save changes to this project before quitting?",
            None,
            &["Save", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else { return };
            this.update(cx, |this, cx| match choice {
                0 if this.save(cx) => cx.quit(),
                1 => {
                    this.session.forget_kept();
                    cx.quit()
                }
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.session.edited {
            self.quit(window, cx);
        }
        !self.session.edited
    }

    /// Catches pointer moves and releases anywhere in the window while dragging.
    fn drag_listener(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let t = this.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        t.update(cx, |ws, cx| {
                            ws.fine = e.modifiers.shift;
                            ws.drag_to(pt(e.position), cx)
                        })
                        .ok();
                    }
                });
                let t = this.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        t.update(cx, |ws, cx| ws.end_drag(cx)).ok();
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }

    /// While files from the Finder are over the window: letting go drops them where the pointer
    /// is, wherever that is.
    fn file_listener(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let t = this.clone();
                window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        t.update(cx, |ws, cx| {
                            if let Some((_, paths)) = ws.file_drag.take() {
                                ws.drop_files(&paths, pt(e.position), cx);
                            }
                        })
                        .ok();
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }

    /// Reads the meters and compressors, letting them fall back gently, and the CPU load.
    fn read_meters(&mut self) {
        let ids: Vec<u64> = self
            .session
            .project
            .tracks
            .iter()
            .map(|t| t.id)
            .chain(self.session.project.auxes.iter().map(|a| a.id))
            .collect();
        for id in ids.iter().map(|&id| Some(id)).chain([None]) {
            let peak = match id {
                Some(id) => self.player.meter(id),
                None => self.player.master_meter(),
            };
            self.levels.entry(id).or_default().update(peak);
            let gr = self.reductions.entry(id).or_default();
            *gr = self.player.reduction(id).max(*gr * 0.9);
        }
        self.read_mods();
        // A spike shows, then falls away by half every half second.
        let since = std::mem::replace(&mut self.load_read, Instant::now()).elapsed();
        self.load = (self.player.load()).max(self.load * 0.5f32.powf(since.as_secs_f32() / 0.5));
    }

    /// Keeps the playhead on screen while playing, a page at a time, as Logic does.
    fn follow_playhead(&mut self) {
        let width = f32::from(self.lanes_bounds.get().size.width);
        let x = self.playhead() as f32 * self.px_per_tick() - self.scroll_x;
        if width > 0.0 && (x < 0.0 || x > width - 20.0) {
            self.scroll_x = (x + self.scroll_x - 20.0).max(0.0);
        }
    }

    /// Whether the agent's message box or the look picker's search has the keyboard.
    fn typing_in_field(&self, window: &Window, cx: &App) -> bool {
        let focused = |input: &Entity<crate::text::TextInput>| {
            input.read(cx).focus_handle(cx).is_focused(window)
        };
        focused(&self.chat.input) || self.picker.as_ref().is_some_and(|p| focused(&p.search))
    }

    /// The agent's most recent edit, while its outline still shows.
    pub fn agent_presence(&self) -> Option<&AgentEdit> {
        self.agent.last().filter(|a| a.at.elapsed() < PRESENCE)
    }

    fn status_bar(&self) -> impl IntoElement {
        let hint = match (self.view, self.tool, self.bottom) {
            (View::Songs, ..) => {
                "Click a song to open it · ⌘-click to pick several, Delete to move them to the Trash · right-click for more"
            }
            (View::Mixer, ..) => {
                "Click a slot to open it · right-click a send for pre or post · drag a fader, ⌥-click to reset · double-click a name to rename"
            }
            (View::Samples, ..) if self.browser.view == crate::browser::Results::Map => {
                "Hover a dot to hear it · click to pick it · drag a box to keep what's in it · ⌘-scroll to zoom"
            }
            (View::Samples, ..) => {
                "Click a sound to hear it · ↑ ↓ for the next · drag one out to put it on a track · F for the compact browser"
            }
            (_, _, None) if self.show_browser => {
                "Click a sound to hear it · ↑ ↓ for the next · drag it onto a track, or double-click to add it · F hides it"
            }
            (_, t, _) if t != Tool::Pointer => t.hint(),
            (_, _, Some(Bottom::PianoRoll)) => {
                "Double-click writes · drag a band to pick · ⌥-drag copies · ↑↓ ⇧↑↓ ←→ move · ⌘C ⌘V ⌘D · Q quantize · right-click for more"
            }
            _ if self.show_automation => {
                "Click for a point · drag to select · drag a line or its ◇ to move or bend it · ⌥-click deletes · ⌘C ⌘V · right-click for shapes · the pencil draws"
            }
            _ => {
                "Drag to move · ⌥-drag to copy · ⌥⇧-drag for an alias · drag an edge to trim · drag the top of its end to loop · ⌥-drag its end to time-stretch"
            }
        };
        let (text, color) = match &self.status {
            Some(s) => (s.clone(), TEXT_2),
            None => (hint.to_string(), TEXT_3),
        };
        let rate = self.player.rate();
        let buffer = self.player.buffer();
        // 48 kHz, 44.1 kHz.
        let khz = format!("{:.1}", rate / 1000.0);
        let khz = khz.trim_end_matches(".0");
        let format = if buffer > 0 {
            format!(
                "{khz} kHz · {buffer} smp · {:.1} ms",
                buffer as f64 / rate * 1000.0
            )
        } else {
            format!("{khz} kHz")
        };
        let agent = self
            .agent
            .last()
            .filter(|a| a.at.elapsed() < Duration::from_secs(60));
        div()
            .h(px(STATUS_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(12.))
            .bg(rgb(BG))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .text_size(px(11.))
            .text_color(rgb(TEXT_3))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_color(rgb(color))
                    .child(text),
            )
            .when_some(agent, |d, a| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_color(rgb(AGENT_TEXT))
                        .whitespace_nowrap()
                        .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(AGENT)))
                        .child(format!("Agent: {} · {}", a.what, a.ago())),
                )
            })
            .child(format!("CPU {:.0}%", (self.load * 100.0).min(999.0)))
            .child(div().font_family(MONO).child(format))
            .when_some(self.typing, |d, octave| {
                // Musical typing is on: letters are notes, so say so, and where they are.
                let low = (60 + octave as i32 * 12).clamp(0, 127) as u8;
                let high = low.saturating_add(17).min(127);
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_color(rgb(ACCENT_TEXT))
                        .whitespace_nowrap()
                        .child(icon(Icon::Keys, 12.).text_color(rgb(ACCENT_TEXT)))
                        .child(format!(
                            "Musical Typing · {}–{} · Z X octave · ⌘K off",
                            self.note_name(low),
                            self.note_name(high)
                        )),
                )
            })
            .child(match (self.session.edited, self.session.on_disk) {
                (true, _) => "Edited",
                (false, true) => "Saved",
                (false, false) => "Not saved",
            })
    }

    /// Finds the installed plugins in the background, for the plugin menus.
    pub fn start_scan(&mut self, cx: &mut Context<Self>) {
        let Ok(exe) = std::env::current_exe() else {
            self.plugins = Some(Vec::new());
            return;
        };
        let found = cx.background_spawn(async move {
            scan::scan(&exe, &scan::everything(), &scan::cache_file())
        });
        cx.spawn(async move |this, cx| {
            let plugins = found.await;
            this.update(cx, |ws, cx| {
                ws.plugins = Some(plugins);
                ws.catalog_plugins();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The knobs a modulator can be dropped on, as this frame draws them.
        self.mod_targets.borrow_mut().clear();
        if let Some(what) = self.pending_name.take() {
            self.start_naming(what, window, cx);
        }
        if let Some((track, at)) = self.pending_look.take() {
            self.open_picker(track, at, window, cx);
        }
        if let Some(dir) = self.opening.take() {
            self.open_song(dir, window, cx);
        }
        if let Some(action) = self.song_pending.take() {
            self.song_action(action, window, cx);
        }
        if self.view == View::Songs && self.home.is_none() {
            self.home = Some(crate::projects::Home::new(window, cx));
        }
        self.browser_frame(window, cx);
        if std::mem::take(&mut self.refocus) && self.naming.is_none() {
            self.focus.focus(window, cx);
        }
        self.viewport_h = f32::from(window.viewport_size().height);
        self.viewport_w = f32::from(window.viewport_size().width);
        self.player.collect();
        self.player.idle();
        // A stretch made in the background goes in, in place of what stood in for it.
        if self.player.fresh() {
            let project = self.engine_project().into_owned();
            self.status = self.player.sync(&project).err();
        }
        self.follow_pattern(cx);
        self.read_meters();
        self.eq_frame();
        self.capture(cx);
        self.automation_frame(cx);
        self.show_automation_values();
        let armed = self.armed().map(|t| self.session.project.tracks[t].id);
        self.player.arm(armed);
        let playing = self.player.playing();
        if playing && self.settings.follow_playhead {
            self.follow_playhead();
        }
        // ponytail: redraws every frame while anything moves; fine until the UI gets heavy.
        let busy = playing
            || self.recording.is_some()
            || !self.writing.is_empty()
            || self.levels.values().any(Level::moving)
            || self.reductions.values().any(|&g| g > 0.01)
            || self.agent_presence().is_some()
            || self.browser.auditioning()
            || self.player.rendering()
            || self.eq_view.as_ref().is_some_and(|v| playing || v.moving())
            || (self.bottom == Some(Bottom::Modulation)
                && self
                    .session
                    .project
                    .tracks
                    .get(self.track)
                    .is_some_and(|t| !t.modulators.is_empty()));
        if busy || self.drag.is_some() {
            window.request_animation_frame();
        }
        let dir = self
            .session
            .dir
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy());
        let title = format!(
            "{dir}{}",
            if self.session.edited {
                " — Edited"
            } else {
                ""
            }
        );
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .font_weight(FontWeight::NORMAL)
            .track_focus(&self.focus)
            .key_context({
                let context = match (
                    &self.naming,
                    self.menu.as_ref().is_some_and(|m| m.searchable),
                ) {
                    (Some(_), _) => "Naming",
                    (None, true) => "Menu",
                    // The search field has the keyboard: letters and space type.
                    _ if self.browser.searching => "Search",
                    _ if self.typing.is_some() => "Workspace Typing",
                    // The piano roll open: arrows, copy and paste work on its notes.
                    _ if self.view == View::Arrange
                        && self.bottom == Some(Bottom::PianoRoll)
                        && !self.browser_open() =>
                    {
                        "Workspace Arrange Roll"
                    }
                    _ => "Workspace Arrange",
                };
                // Up and down pick sounds while the browser shows.
                let mut context = match self.browser_open() {
                    true => format!("{context} Samples"),
                    false => context.to_string(),
                };
                // The EQ window clicked in last: ⌘A and Escape are its bands'.
                if self.naming.is_none() && self.eq_view.as_ref().is_some_and(|v| v.focused) {
                    context += " Eq";
                }
                gpui::KeyContext::parse(&context).unwrap_or_default()
            })
            .on_key_down(cx.listener(|ws, e: &KeyDownEvent, window, cx| ws.type_key(e, window, cx)))
            .on_key_up(cx.listener(|ws, e: &KeyUpEvent, _, cx| ws.release_key(e, cx)))
            .on_action(
                cx.listener(|ws, _: &TogglePianoRoll, _, cx| {
                    ws.toggle_bottom(Bottom::PianoRoll, cx)
                }),
            )
            .on_action(cx.listener(|ws, _: &ToggleAutomation, _, cx| {
                ws.show_automation = !ws.show_automation;
                cx.notify();
            }))
            .on_action(cx.listener(|ws, _: &Record, _, cx| ws.record(cx)))
            .on_action(cx.listener(|ws, _: &NewPattern, _, cx| ws.new_pattern(cx)))
            .on_action(
                cx.listener(|ws, _: &ToggleScore, _, cx| ws.toggle_bottom(Bottom::Score, cx)),
            )
            .on_action(
                cx.listener(|ws, _: &ToggleLoops, _, cx| ws.toggle_bottom(Bottom::Loops, cx)),
            )
            .on_action(cx.listener(|ws, _: &ToggleSamples, _, cx| ws.toggle_samples(cx)))
            .on_action(cx.listener(|ws, _: &ToggleSamplesPage, _, cx| ws.toggle_samples_page(cx)))
            .on_action(cx.listener(|ws, _: &PreviousSample, _, cx| ws.step_sample(-1, cx)))
            .on_action(cx.listener(|ws, _: &NextSample, _, cx| ws.step_sample(1, cx)))
            .on_action(cx.listener(|ws, _: &CreateStack, _, cx| {
                // A summing stack of the selected tracks, as Logic's ⇧⌘D makes of the selection.
                let tracks = ws.picked_tracks();
                if !tracks.is_empty() {
                    ws.make_stack(&tracks, cx);
                }
            }))
            .on_action(cx.listener(|ws, _: &ToggleTyping, _, cx| ws.toggle_typing(cx)))
            .on_action(cx.listener(|ws, _: &PlayStop, _, cx| ws.play_stop(cx)))
            .on_action(cx.listener(|ws, _: &GoToStart, _, cx| ws.go_to_start(cx)))
            .on_action(cx.listener(|ws, _: &Undo, _, cx| ws.undo(cx)))
            .on_action(cx.listener(|ws, _: &EqSelectAll, _, cx| {
                ws.eq_key(crate::eq::EqOp::SelectAll, cx);
            }))
            .on_action(cx.listener(|ws, _: &EqDeselect, _, cx| {
                if let Some(v) = ws.eq_view.as_mut() {
                    v.picked.clear();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|ws, _: &Redo, _, cx| ws.redo(cx)))
            .on_action(cx.listener(|ws, _: &Delete, _, cx| ws.delete(cx)))
            .on_action(cx.listener(|ws, _: &DeleteTrack, _, cx| ws.delete_track(cx)))
            .on_action(cx.listener(|ws, _: &Split, _, cx| ws.split(cx)))
            .on_action(cx.listener(|ws, _: &CopyRegion, _, cx| {
                if ws.eq_key(crate::eq::EqOp::Copy, cx) {
                    return;
                }
                if !ws.copy_picked(cx) {
                    ws.auto_clip = None;
                    ws.region_key(RegionOp::Copy, cx)
                }
            }))
            .on_action(cx.listener(|ws, _: &CutRegion, _, cx| {
                if ws.copy_picked(cx) {
                    ws.delete_picked(cx);
                } else {
                    ws.auto_clip = None;
                    ws.region_key(RegionOp::Cut, cx)
                }
            }))
            .on_action(cx.listener(|ws, _: &PasteRegion, _, cx| {
                if ws.eq_key(crate::eq::EqOp::Paste, cx) {
                    return;
                }
                if !ws.paste_points(cx) {
                    ws.paste_key(cx)
                }
            }))
            .on_action(
                cx.listener(|ws, _: &RepeatRegion, _, cx| ws.region_key(RegionOp::Duplicate, cx)),
            )
            .on_action(cx.listener(|ws, _: &MuteRegion, _, cx| ws.region_key(RegionOp::Mute, cx)))
            .on_action(cx.listener(|ws, _: &NewTrack, window, cx| {
                let at = gpui::point(px(12.), px(CONTROL_BAR_H + 44.));
                ws.open_adder(at, window, cx)
            }))
            .on_action(cx.listener(|ws, _: &ZoomIn, _, cx| ws.zoom(1.25, cx)))
            .on_action(cx.listener(|ws, _: &ZoomOut, _, cx| ws.zoom(0.8, cx)))
            .on_action(cx.listener(|ws, _: &Save, _, cx| {
                ws.save(cx);
            }))
            .on_action(cx.listener(|ws, _: &Export, _, cx| ws.open_export(cx)))
            .on_action(cx.listener(|ws, _: &ToggleCycle, _, cx| ws.toggle_cycle(cx)))
            .on_action(cx.listener(|ws, _: &ToggleMetronome, _, cx| ws.toggle_metronome(cx)))
            .on_action(cx.listener(|ws, _: &Quit, window, cx| ws.quit(window, cx)))
            // Audio dragged in from the Finder: followed here, and let go by `file_listener`.
            // (gpui's own drop needs the pointer to count as hovering, which it doesn't after a
            // key press, as a Finder drag isn't a mouse move.)
            .on_drag_move(cx.listener(|ws, e: &DragMoveEvent<ExternalPaths>, _, cx| {
                ws.file_drag = Some((pt(e.event.position), e.drag(cx).paths().to_vec()));
                cx.notify();
            }))
            .on_action(cx.listener(|ws, _: &ToggleMixer, _, cx| ws.toggle_mixer(cx)))
            .on_action(cx.listener(|ws, _: &ToggleAgent, window, cx| ws.toggle_agent(window, cx)))
            .on_action(cx.listener(|ws, _: &NewSong, window, cx| ws.show_home(true, window, cx)))
            .on_action(cx.listener(|ws, _: &CloseSong, window, cx| {
                ws.song_action(crate::menu::MenuAction::CloseSong, window, cx)
            }))
            .on_action(cx.listener(|ws, _: &ShowSongs, _, cx| ws.toggle_songs(cx)))
            .on_action(cx.listener(|ws, _: &EditPlaybooks, _, cx| ws.open_genres(None, cx)))
            .on_action(cx.listener(|ws, _: &StudySong, _, cx| ws.open_study(cx)))
            .on_action(cx.listener(|ws, _: &ShowSettings, _, cx| ws.open_settings(cx)))
            .on_action(cx.listener(|ws, _: &ToolMenu, _, cx| {
                // Under the toolbar, as Logic opens it at the pointer.
                let x = if ws.show_inspector { ws.side_w() } else { 0.0 } + 14.0;
                ws.tool_menu(gpui::point(px(x), px(CONTROL_BAR_H + TOP_H)), cx);
            }))
            .on_action(
                cx.listener(|ws, _: &SelectAllNotes, _, cx| ws.roll_op(RollOp::SelectAll, cx)),
            )
            .on_action(cx.listener(|ws, _: &CopyNotes, _, cx| ws.roll_op(RollOp::Copy, cx)))
            .on_action(cx.listener(|ws, _: &CutNotes, _, cx| ws.roll_op(RollOp::Cut, cx)))
            .on_action(cx.listener(|ws, _: &PasteNotes, _, cx| ws.roll_op(RollOp::Paste, cx)))
            .on_action(
                cx.listener(|ws, _: &DuplicateNotes, _, cx| ws.roll_op(RollOp::Duplicate, cx)),
            )
            .on_action(cx.listener(|ws, _: &NotesUp, _, cx| ws.roll_op(RollOp::Transpose(1), cx)))
            .on_action(
                cx.listener(|ws, _: &NotesDown, _, cx| ws.roll_op(RollOp::Transpose(-1), cx)),
            )
            .on_action(
                cx.listener(|ws, _: &NotesOctaveUp, _, cx| ws.roll_op(RollOp::Transpose(12), cx)),
            )
            .on_action(
                cx.listener(|ws, _: &NotesOctaveDown, _, cx| {
                    ws.roll_op(RollOp::Transpose(-12), cx)
                }),
            )
            .on_action(cx.listener(|ws, _: &NudgeLeft, _, cx| ws.roll_op(RollOp::Nudge(-4), cx)))
            .on_action(cx.listener(|ws, _: &NudgeRight, _, cx| ws.roll_op(RollOp::Nudge(4), cx)))
            .on_action(
                cx.listener(|ws, _: &NudgeLeftFine, _, cx| ws.roll_op(RollOp::Nudge(-1), cx)),
            )
            .on_action(
                cx.listener(|ws, _: &NudgeRightFine, _, cx| ws.roll_op(RollOp::Nudge(1), cx)),
            )
            .on_action(cx.listener(|ws, _: &QuantizeNotes, _, cx| ws.roll_op(RollOp::Quantize, cx)))
            .on_action(cx.listener(|ws, _: &DeselectNotes, _, cx| ws.roll_op(RollOp::Deselect, cx)))
            .on_action(cx.listener(|ws, _: &BrowserUp, _, cx| ws.move_picked(-1, cx)))
            .on_action(cx.listener(|ws, _: &BrowserDown, _, cx| ws.move_picked(1, cx)))
            .on_action(cx.listener(|ws, _: &BrowserNewTrack, _, cx| ws.use_picked(true, cx)))
            .on_action(cx.listener(|ws, _: &ToggleInspector, _, cx| {
                ws.show_inspector = !ws.show_inspector;
                cx.notify();
            }))
            // With no song open, the Songs page is the whole window: nothing to play.
            .when(!self.home_alone(), |d| d.child(self.control_bar(cx)))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .when(self.view == View::Mixer, |d| d.child(self.mixer_view(cx)))
                    .when(self.view == View::Samples, |d| {
                        d.child(self.samples_page(cx))
                    })
                    .when(self.view == View::Songs, |d| d.child(self.songs_page(cx)))
                    .when(self.view == View::Playbooks, |d| {
                        d.child(self.genres_page(cx))
                    })
                    .when(self.view == View::Study, |d| d.child(self.study_page(cx)))
                    .when(self.view == View::Settings, |d| {
                        d.child(self.settings_page(cx))
                    })
                    .when(self.view == View::Arrange, |d| {
                        d.when(self.show_inspector, |d| d.child(self.inspector(cx)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .flex()
                                    .flex_col()
                                    .child(self.tracks_area(cx))
                                    .when_some(self.bottom, |d, pane| {
                                        d.child(self.editor(pane, cx))
                                    }),
                            )
                            .when(self.show_browser, |d| d.child(self.browser_panel(cx)))
                            .when(self.show_agent && !self.show_browser, |d| {
                                d.child(self.agent_panel(cx))
                            })
                    }),
            )
            .when(!self.home_alone() || self.status.is_some(), |d| {
                d.child(self.status_bar())
            })
            .when(self.drag.is_some(), |d| d.child(self.drag_listener(cx)))
            .when(self.file_drag.is_some(), |d| {
                d.child(self.file_listener(cx))
            })
            .children(self.sample_ghost())
            .children(self.device_ghost())
            .children(self.panner_view(cx))
            .children(self.mod_ghost())
            .children(self.eq_window(cx))
            .children(self.menu_view(cx))
            .children(self.adder_view(cx))
            .children(self.export_view(cx))
            .children(self.picker_view(cx))
    }
}
