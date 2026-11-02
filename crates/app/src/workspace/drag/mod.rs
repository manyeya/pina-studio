//! Every drag: what started it, what moving does, what letting go does.

mod listeners;
mod moving;
mod release;

use super::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Edge {
    Move,
    Start,
    End,
    /// The end, time-stretching what's in it rather than trimming it (⌥).
    Stretch,
    /// The top of the end: loops it out to where it's dragged.
    Loop,
}

/// A mouse drag in progress. Edits checkpoint when they start, so a drag is one undo step.
/// What dragging a marker does: its section moves, a copy of it goes, or the marker alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerHow {
    Section,
    Copy,
    Marker,
}

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
    /// The tempo display dragged up or down (`at`: the change it sets, 0 the song's own); a
    /// click, not moving, opens its menu.
    Tempo {
        y: f32,
        x: f32,
        bpm: f64,
        at: u64,
        moved: bool,
    },
    /// A box dragged across empty lanes, picking the regions it touches.
    RegionBox {
        from: Point<f32>,
        to: Point<f32>,
        moved: bool,
    },
    /// A tempo change's point in the tempo lane, from where it was picked up (`now` where it is
    /// as it's dragged), moved in time and tempo; `range` the lane's tempos then.
    TempoPoint {
        tick: u64,
        now: u64,
        x: f32,
        y: f32,
        bpm: f64,
        range: (f64, f64),
        moved: bool,
    },
    /// A stretch of the tempo lane dragged up or down (`at`: its change, 0 the song's own).
    TempoLine {
        at: u64,
        y: f32,
        bpm: f64,
        range: (f64, f64),
        moved: bool,
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
    /// Text being selected in the Studio Agent's conversation.
    ChatText,
    /// A stack's row, by its bottom edge.
    StackHeight {
        aux: usize,
        y: f32,
        height: f32,
    },
    /// A track's or stack's fader in its header, dragged across.
    HFader {
        channel: Channel,
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
        target: pina_engine::dsp::modulate::ModTarget,
        x: f32,
        y: f32,
        depth: f32,
    },
    ModValue {
        track: usize,
        index: usize,
        field: crate::editors::modulation::ModField,
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
    /// Something in the step sequencer: steps painted or set, a loop, a row moved.
    StepSeq(crate::editors::steps::StepDrag),
    /// A mark on the Sampler's waveform: a zone's start, end or loop point; `left` and `width`
    /// where the waveform is.
    Mark {
        track: usize,
        zone: usize,
        mark: crate::editors::sampler::Mark,
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
    /// A marker, from the x where it was picked up (`at`: where it is now): its section moved
    /// or copied with everything in it, or (⌘) the marker alone.
    Marker {
        index: usize,
        x: f32,
        at: f32,
        tick: u64,
        how: MarkerHow,
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
        knob: crate::mix::eq::EqKnob,
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
    /// A point of a sampler envelope's graph (1 attack, 2 decay and the level held, 4
    /// release), the graph where it was drawn.
    SamplerEnv {
        track: usize,
        env: crate::editors::sampler::EnvKind,
        point: u8,
        left: f32,
        top: f32,
        width: f32,
        height: f32,
    },
    /// The sampler filter's point: its cutoff across, its resonance up and down.
    SamplerFilter {
        track: usize,
        left: f32,
        top: f32,
        width: f32,
        height: f32,
    },
    /// An effect in a strip, or its card in the mixer's panel (its `row`, pressed at `from`, the
    /// pointer `at`), dragged to another row; let go unmoved, it opens.
    Effect {
        ch: Channel,
        row: usize,
        to: usize,
        card: bool,
        from: (f32, f32),
        at: (f32, f32),
        moved: bool,
    },
    /// The navigation bar's view of the song on screen, dragged to scroll, held `grab` px into
    /// it; and the tracks' scrollbar, likewise.
    Overview {
        grab: f32,
    },
    TrackScroll {
        grab: f32,
    },
    /// The compressor's or saturator's window, moved by its title.
    /// The synth's window moved by its title.
    PolyWindow {
        from: Point<f32>,
        start: (f32, f32),
    },
    /// A point on one of the synth's pictures.
    PolyPoint {
        id: u8,
    },
    /// One of the synth's sources on its way to a knob.
    PolySource {
        track: usize,
        source: pina_engine::song::project::PolySource,
    },
    /// A route's depth, from the x and depth it had.
    PolyDepth {
        track: usize,
        route: usize,
        x: f32,
        depth: f32,
    },
    FxWindow {
        from: Point<f32>,
        start: (f32, f32),
    },
    /// A split between the saturator's bands (the `k`th), moved sideways.
    SatSplit {
        k: usize,
    },
    /// A point on the reverb's or delay's displays, dragged; `span` the seconds across the
    /// time picture as the drag began (so it holds still under the pointer).
    Space {
        handle: crate::mix::fx::SpaceHandle,
        span: f32,
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
    /// A Drum Machine pad (by its note) dragged to another, to trade places with it.
    Pad {
        note: u8,
        from: Point<f32>,
        at: Point<f32>,
        moved: bool,
    },
    /// A sample from the browser, by its index in the library, on its way to the tracks: where
    /// it was picked up, and where it is.
    Sample {
        index: usize,
        from: Point<f32>,
        at: Point<f32>,
        moved: bool,
    },
    /// A MIDI file from the browser, by its index in the library's, on its way to the tracks.
    Midi {
        index: usize,
        /// Semitones it's moved, as it was heard when it was picked up.
        shift: i32,
        from: Point<f32>,
        at: Point<f32>,
        moved: bool,
    },
    /// A MIDI card from the Agent panel, by its place on the shelf, on its way to the tracks.
    ChatMidi {
        card: usize,
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

impl Workspace {
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
                | Drag::Midi { .. }
                | Drag::ChatMidi { .. }
                | Drag::Device { .. }
                | Drag::MapBox { .. }
                | Drag::Marquee { .. }
                | Drag::RegionBox { .. }
                | Drag::Width { .. }
                | Drag::LaneHeight { .. }
                | Drag::StackHeight { .. }
                | Drag::ChatText
                | Drag::Orbit { .. }
                | Drag::Modulator { .. }
                | Drag::ZoomSpan
                | Drag::SoloHold
                | Drag::AutoMarquee { .. }
                | Drag::AutoDraw { .. }
                | Drag::EqWindow { .. }
                | Drag::FxWindow { .. }
                | Drag::PolyWindow { .. }
                | Drag::PolySource { .. }
                | Drag::Overview { .. }
                | Drag::Pad { .. }
                | Drag::TrackScroll { .. }
                | Drag::Effect { .. }
                | Drag::EqPress { .. }
                | Drag::EqScale { .. }
                | Drag::EqSketch
                | Drag::StepSeq(crate::editors::steps::StepDrag::Zoom)
        );
        if edits {
            self.session.checkpoint();
        }
        self.drag = Some(drag);
        cx.notify();
    }
}
