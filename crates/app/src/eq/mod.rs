//! The Channel EQ's window: one big display of the channel's sound,
//! a live analyzer of what goes in and comes out, and the EQ's curve, each band a dot.
//!
//! Hover the display to see the band a click would make there (a cut at either end, a notch
//! low down, a bell between; a shelf when dragged from either end); click to make it (once
//! nothing's picked; else double-click or ⌘-click), or drag the curve to make it and pull it,
//! ⌥ held for a dynamic one. Drag across the background to pick bands in a box, ⌘-click to add
//! one, ⇧-click for a run. Dragging a dot moves every band picked (⌥: one way only, ⇧: fine,
//! ⌘: their width); the wheel over one changes its width (a cut's slope), with ⌥ its dynamic
//! range, with ⌘ its gain. ⌥-click switches a band off, ⌘⌥-click changes its shape, ⌥⇧-click
//! its slope; double-click a dot to type its frequency ("2k", "A4", "C#2+13"), gain ("2x") or
//! width. Right-click a dot or the background for everything else.
//!
//! The bands picked show their controls in a panel under them: shape, slope, frequency, gain
//! (its dynamic range a ring round it), width, gain-Q interaction, the part of the sound they
//! work on (and a split into left and right, or mid and side), solo (hold it to hear what the
//! band works on; drag it to move the band, or up and down for the level), on/off and delete.
//! The display's frequency scale zooms (drag it up and down) and pans (sideways); double-click
//! puts it back; the piano display shows notes, and snaps bands to them. Along the top: the
//! channel, presets, undo, A/B and full screen; along the bottom, the analyzer and its
//! settings, the display's range, the output (bypass, phase, gain scale, level, pan).

mod bars;
mod display;
mod list;
mod panel;
mod tools;

pub use tools::Match;

use std::{cell::Cell, rc::Rc};

use gpui::{
    Bounds, Context, MouseDownEvent, Pixels, ScrollWheelEvent, anchored, deferred, div, point,
    prelude::*, px, rgb,
};
use pina_engine::{
    eq::{TAP_LEN, auto_times, band_sections, response_db, spectrum_db},
    project::{
        Bands, Character, Dynamic, Eq, EqBand, EqField, MAX_BANDS, Param, Placement, Processing,
        Shape,
    },
};

use crate::{
    menu::{Menu, MenuAction},
    strip::Channel,
    theme::*,
    workspace::{Drag, Name, Workspace, pt},
};

/// The bands' colors, round and round by their place.
pub const BAND_COLORS: [u32; 8] = [CORAL, AMBER, MINT, SKY, VIOLET, PINK, TEAL, 0xa3e635];

/// The overall curve; and the curves of bands on part of the sound alone.
pub const CURVE: u32 = 0xffd166;
const LEFT_CURVE: u32 = 0xf4f4f6;
const RIGHT_CURVE: u32 = 0xff6b6b;
const MID_CURVE: u32 = 0x5ee08a;
const SIDE_CURVE: u32 = 0x5aa9ff;

/// The frequencies the display can show.
const LOW: f64 = 10.0;
const HIGH: f64 = 30_000.0;
/// Room above and below the EQ's scale.
const PAD: f32 = 18.0;
const HEAD_H: f32 = 40.0;
const FOOT_H: f32 = 44.0;
/// The frequency scale (or piano), along the display's bottom; and to the plot's right, the
/// analyzer's scale and the output meter.
const SCALE_H: f32 = 22.0;
const SIDE_W: f32 = 58.0;
const MIN_W: f32 = 700.0;
const MIN_H: f32 = 440.0;
/// The ranges the display can show, ± dB.
pub const RANGES: [u8; 4] = [3, 6, 12, 30];

/// A band's color, by its place.
pub fn band_color(i: usize) -> u32 {
    BAND_COLORS[i % BAND_COLORS.len()]
}

/// The color of the curve for bands on one part of the sound.
fn part_color(p: Placement) -> u32 {
    match p {
        Placement::Stereo => CURVE,
        Placement::Left => LEFT_CURVE,
        Placement::Right => RIGHT_CURVE,
        Placement::Mid => MID_CURVE,
        Placement::Side => SIDE_CURVE,
    }
}

/// What the analyzer shows, and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Analysis {
    pub pre: bool,
    pub post: bool,
    /// Its scale, dB from top to bottom: 60, 90 or 120.
    pub range: u8,
    /// Its FFT: 1024 << this many points (0 to 3: low to maximum).
    pub resolution: u8,
    /// How fast it falls: 0 very slow to 4 very fast.
    pub speed: u8,
    /// dB an octave it tilts up about 1 kHz.
    pub tilt: f32,
    /// It holds the most it's seen, rather than falling.
    pub freeze: bool,
    /// Where this sound and the one compared with crowd each other, a red glow.
    pub collisions: bool,
    /// Resting the pointer on the spectrum grabs it: a peak to pull into a band.
    pub grab: bool,
}

impl Default for Analysis {
    fn default() -> Self {
        Analysis {
            pre: true,
            post: true,
            range: 90,
            resolution: 2,
            speed: 2,
            tilt: 4.5,
            freeze: false,
            collisions: true,
            grab: true,
        }
    }
}

impl Analysis {
    fn len(&self) -> usize {
        1024 << self.resolution.min(3)
    }

    /// dB it falls a frame.
    fn fall(&self) -> f32 {
        [0.12, 0.3, 0.7, 1.5, 3.0][self.speed.min(4) as usize]
    }
}

/// A pop-up over the window's bottom bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Popup {
    Analyzer,
    Output,
}

/// The plot: the display less its scales and meter, and the frequencies and gains it shows.
#[derive(Clone, Copy, Debug)]
pub struct Geo {
    pub w: f32,
    pub h: f32,
    pub lo: f64,
    pub hi: f64,
    pub range: f32,
}

impl Geo {
    pub fn x(&self, f: f64) -> f32 {
        ((f.max(1.0) / self.lo).ln() / (self.hi / self.lo).ln()) as f32 * self.w
    }

    pub fn freq(&self, x: f32) -> f64 {
        self.lo * (self.hi / self.lo).powf((x / self.w) as f64)
    }

    pub fn y(&self, db: f32) -> f32 {
        self.h / 2.0 - db / self.range * (self.h / 2.0 - PAD)
    }

    pub fn db(&self, y: f32) -> f32 {
        (self.h / 2.0 - y) / (self.h / 2.0 - PAD) * self.range
    }

    fn octaves(&self) -> f32 {
        (self.hi / self.lo).log2() as f32
    }
}

/// A band heard alone: which, and how loud (dB).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solo {
    pub id: u16,
    pub level: f32,
}

/// The EQ window: whose EQ, where it is, what's picked, and what the analyzer's heard.
pub struct EqView {
    pub ch: Channel,
    /// Its top left corner and size, in the app's window; and before full screen, those.
    pub at: (f32, f32),
    pub size: (f32, f32),
    restore: Option<((f32, f32), (f32, f32))>,
    /// The bands picked, by id, the one picked last last.
    pub picked: Vec<u16>,
    /// The band under the pointer, by id; and where the pointer is over the plot.
    pub hover: Option<u16>,
    pub pointer: Option<(f32, f32)>,
    /// Over the frequency scale: where.
    pub scale_pointer: Option<f32>,
    /// The display's gain range, ± dB.
    pub range: u8,
    /// The frequencies shown (all of them, or zoomed in).
    pub span: (f64, f64),
    pub piano: bool,
    pub analysis: Analysis,
    pub popup: Option<Popup>,
    pub solo: Option<Solo>,
    /// The dynamics' own settings shown, in the band panel.
    pub dyn_open: bool,
    /// Clicked in last: Delete, ⌘C, ⌘V and ⌘A work on its bands.
    pub focused: bool,
    /// A box being dragged across the plot to pick the bands in it.
    pub marquee: Option<((f32, f32), (f32, f32))>,
    /// The bands picked as a drag of them began.
    start: Vec<EqBand>,
    /// The other of A and B, and whether B is the one heard.
    other: Option<Eq>,
    pub on_b: bool,
    /// Another track to compare with (its spectrum drawn, and where they collide), by id.
    pub ext: Option<u64>,
    ext_spec: Vec<f32>,
    /// Spectrum Grab: when the pointer came to rest on the spectrum, and the spectrum held
    /// while grabbing.
    grab_since: Option<std::time::Instant>,
    pub grabbing: Option<Vec<f32>>,
    /// The instance list in the display's place, and its settings.
    pub list: bool,
    pub listing: list::Listing,
    /// EQ Match, while it's open.
    pub matching: Option<Match>,
    /// EQ Sketch: on, and the stroke drawn (plot coordinates, left to right).
    pub sketching: bool,
    pub stroke: Vec<(f32, f32)>,
    /// The analyzer's curves (dB at each of `columns`), and the frequencies those are at.
    pre: Vec<f32>,
    post: Vec<f32>,
    columns: Vec<f64>,
    written: usize,
    hearing: bool,
    buf: Box<[[f32; TAP_LEN]; 4]>,
    /// The output meter: each side's level now and the loudest since it was clicked (dB).
    pub meter: [f32; 2],
    pub peak: f32,
    /// The display's place in the app's window, as last drawn.
    pub display: Rc<Cell<Bounds<Pixels>>>,
}

impl EqView {
    fn new(ch: Channel, at: (f32, f32), size: (f32, f32)) -> Self {
        EqView {
            ch,
            at,
            size,
            restore: None,
            picked: Vec::new(),
            hover: None,
            pointer: None,
            scale_pointer: None,
            range: 12,
            span: (LOW, HIGH),
            piano: false,
            analysis: Analysis::default(),
            popup: None,
            solo: None,
            dyn_open: false,
            focused: true,
            marquee: None,
            start: Vec::new(),
            other: None,
            on_b: false,
            ext: None,
            ext_spec: Vec::new(),
            grab_since: None,
            grabbing: None,
            list: false,
            listing: list::Listing::default(),
            matching: None,
            sketching: false,
            stroke: Vec::new(),
            pre: Vec::new(),
            post: Vec::new(),
            columns: Vec::new(),
            written: 0,
            hearing: false,
            buf: Box::new([[0.0; TAP_LEN]; 4]),
            meter: [-120.0; 2],
            peak: -120.0,
            display: Rc::new(Cell::new(Bounds::default())),
        }
    }

    /// The plot's size and what it shows, at the project's rate.
    pub fn geo(&self, rate: f64) -> Geo {
        let (w, h) = self.display_size();
        Geo {
            w: (w - SIDE_W).max(100.0),
            h: (h - SCALE_H).max(100.0),
            lo: self.span.0,
            hi: self.span.1.min(HIGH.max(rate / 2.0)),
            range: self.range as f32,
        }
    }

    /// The display's size: the window less its top and bottom bars.
    fn display_size(&self) -> (f32, f32) {
        (self.size.0 - 2.0, self.size.1 - HEAD_H - FOOT_H - 2.0)
    }

    /// Whether the analyzer or meter still show something: they fall away when sound stops.
    pub fn moving(&self) -> bool {
        self.hearing
            || self.grab_since.is_some()
            || self.matching.is_some()
            || self.meter.iter().any(|&m| m > -100.0)
            || self
                .pre
                .iter()
                .chain(&self.post)
                .any(|&d| d > -(self.analysis.range as f32) + 1.0)
    }

    fn lead(&self) -> Option<u16> {
        self.picked.last().copied()
    }
}

/// What a click in the EQ window or one of its menus does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EqOp {
    /// The bands picked made this shape, or this slope, or put on this part of the sound.
    Shape(Shape),
    Slope(f32),
    Placement(Placement),
    /// The bands picked split in two: left and right, or mid and side.
    Split,
    /// The bands picked made dynamic, or still.
    Dynamic,
    /// The bands picked switched off, or on.
    Off,
    Delete,
    /// A band picked (alone, or added to those picked).
    Pick(u16, bool),
    SelectAll,
    Copy,
    /// Every band copied.
    CopyAll,
    Paste,
    /// Every band taken out.
    Clear,
    /// The analyzer: pre or post on and off, its range, resolution, speed, tilt; freeze.
    Pre,
    Post,
    AnalyzerRange(u8),
    Resolution(u8),
    Speed(u8),
    Tilt(u8),
    Freeze,
    Range(u8),
    Bypass,
    Popup(Popup),
    Piano,
    /// Shows A or B; copies the one shown to the other.
    Ab,
    CopyAb,
    FullScreen,
    /// A preset: one of Pina Studio's (by its place), or one saved (by its place in the list).
    Factory(usize),
    Preset(usize),
    SavePreset,
    /// The bands picked: gain-Q interaction on or off.
    GainQ,
    /// The EQ's character, auto gain, phase, and whether its pan is mid against side.
    Character(Character),
    AutoGain,
    Invert,
    PanMs(bool),
    /// The dynamics' settings shown or hidden; the bands picked: dynamics bypassed, cleared,
    /// listening to the sidechain, or between two frequencies (or to what they cover);
    /// their threshold their own.
    DynExpand,
    DynBypass,
    DynClear,
    External,
    Trigger,
    AutoThreshold,
    /// The bands picked: spectral dynamics on or off; their tilt on or off.
    Spectral,
    SpectralTilt,
    /// How the EQ treats phase; how fine its linear phase and spectral dynamics work.
    Processing(Processing),
    PhaseResolution(u8),
    /// The track the EQ's sidechain is (by id), or none.
    Sidechain(Option<u64>),
    /// A track to compare with (by id), or none; collisions shown or not; Spectrum Grab on
    /// or off.
    Ext(Option<u64>),
    Collisions,
    Grab,
    /// The instance list shown or not; EQ Sketch on or off.
    List,
    Sketch,
    /// EQ Match: opened (comparing with a track, if given), its reference a track or a file
    /// asked for, matched, its bands fewer or more, finished, or closed.
    Match(Option<u64>),
    MatchTrack(u64),
    MatchFile,
    MatchRun,
    MatchBands(i8),
    MatchFinish,
    MatchCancel,
}

/// A knob (or value) in the EQ window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqKnob {
    Freq,
    Gain,
    Q,
    Range,
    Threshold,
    Scale,
    Output,
    /// A dynamic band's attack and release (ms), and the two frequencies it listens between.
    Attack,
    Release,
    Low,
    High,
    /// The output's balance.
    Pan,
    /// A spectral band's density: wide stretches moving together (0) or each frequency alone (1).
    Density,
    /// A band's slope, dB an octave (fractions too).
    Slope,
}

impl EqKnob {
    /// Where a value sits on the knob, 0 to 1.
    fn pos(self, v: f32) -> f32 {
        match self {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => {
                ((v / 10.0).ln() / 3000f32.ln()).clamp(0.0, 1.0)
            }
            EqKnob::Attack | EqKnob::Release => ((v / 0.1).ln() / 10_000f32.ln()).clamp(0.0, 1.0),
            EqKnob::Pan => (v + 1.0) / 2.0,
            EqKnob::Density => v.clamp(0.0, 1.0),
            EqKnob::Slope => (v / 96.0).clamp(0.0, 1.0),
            EqKnob::Q => ((v / 0.025).ln() / 1600f32.ln()).clamp(0.0, 1.0),
            EqKnob::Gain | EqKnob::Range => (v + 30.0) / 60.0,
            EqKnob::Threshold => (v + 80.0) / 80.0,
            EqKnob::Scale => v / 2.0,
            EqKnob::Output => (v + 36.0) / 72.0,
        }
    }

    /// The value at a place on the knob.
    fn value(self, pos: f32) -> f32 {
        let p = pos.clamp(0.0, 1.0);
        match self {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => 10.0 * 3000f32.powf(p),
            EqKnob::Attack | EqKnob::Release => ((0.1 * 10_000f32.powf(p)) * 10.0).round() / 10.0,
            EqKnob::Pan => ((p * 2.0 - 1.0) * 100.0).round() / 100.0,
            EqKnob::Density => (p * 100.0).round() / 100.0,
            EqKnob::Slope => (p * 960.0).round() / 10.0,
            EqKnob::Q => 0.025 * 1600f32.powf(p),
            EqKnob::Gain | EqKnob::Range => ((p * 60.0 - 30.0) * 100.0).round() / 100.0,
            EqKnob::Threshold => (p * 80.0 - 80.0).round(),
            EqKnob::Scale => (p * 200.0).round() / 100.0,
            EqKnob::Output => ((p * 72.0 - 36.0) * 100.0).round() / 100.0,
        }
    }

    fn bipolar(self) -> bool {
        matches!(
            self,
            EqKnob::Gain | EqKnob::Range | EqKnob::Output | EqKnob::Scale | EqKnob::Pan
        )
    }

    pub fn text(self, v: f32) -> String {
        match self {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => hz_text(v),
            EqKnob::Attack | EqKnob::Release => match v < 10.0 {
                true => format!("{v:.1} ms"),
                false => format!("{v:.0} ms"),
            },
            EqKnob::Pan => match (v * 100.0).round() as i32 {
                0 => "C".into(),
                p if p < 0 => format!("L {}", -p),
                p => format!("R {p}"),
            },
            EqKnob::Q => q_text(v),
            EqKnob::Gain | EqKnob::Range | EqKnob::Output => format!("{} dB", db_text(v)),
            EqKnob::Threshold => format!("{} dB", db_text(v).trim_end_matches(".0")),
            EqKnob::Scale | EqKnob::Density => format!("{:.0}%", v * 100.0),
            EqKnob::Slope => slope_text(v),
        }
    }

    fn label(self) -> &'static str {
        match self {
            EqKnob::Freq => "FREQ",
            EqKnob::Gain => "GAIN",
            EqKnob::Q => "Q",
            EqKnob::Range => "RANGE",
            EqKnob::Threshold => "THRESHOLD",
            EqKnob::Scale => "GAIN SCALE",
            EqKnob::Output => "OUTPUT",
            EqKnob::Attack => "ATTACK",
            EqKnob::Release => "RELEASE",
            EqKnob::Low => "LOW",
            EqKnob::High => "HIGH",
            EqKnob::Pan => "PAN",
            EqKnob::Density => "DENSITY",
            EqKnob::Slope => "SLOPE",
        }
    }

    /// A value typed for it, as it reads: frequencies as Hz, kHz ("2k") or notes ("A4",
    /// "C#2+13"); gains as dB or a ratio ("2x"); any as a share of the knob ("50%").
    pub fn parse(self, text: &str) -> Option<f32> {
        let t = text.trim().to_lowercase().replace(['−', '–'], "-");
        if let Some(share) = t
            .strip_suffix('%')
            .and_then(|n| n.trim().parse::<f32>().ok())
        {
            return Some(match self {
                EqKnob::Scale => share / 100.0,
                _ => self.value(share / 100.0),
            });
        }
        let number = |s: &str| s.trim().parse::<f32>().ok();
        let v = match self {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => parse_freq(&t)?,
            EqKnob::Attack | EqKnob::Release => number(t.trim_end_matches("ms"))?,
            EqKnob::Pan => match t.chars().next()? {
                'c' => 0.0,
                'l' => -number(&t[1..])? / 100.0,
                'r' => number(&t[1..])? / 100.0,
                _ => number(&t)?,
            },
            EqKnob::Gain | EqKnob::Range | EqKnob::Output | EqKnob::Threshold => {
                let t = t.trim_end_matches("db").trim();
                match t.strip_suffix('x') {
                    Some(r) => 20.0 * number(r)?.max(1e-6).log10(),
                    None => number(t.trim_start_matches('+'))?,
                }
            }
            EqKnob::Q => number(t.trim_start_matches('q'))?,
            EqKnob::Scale => number(&t)?,
            // As a share: "0.3", or "30".
            EqKnob::Density => number(&t).map(|n| if n > 1.0 { n / 100.0 } else { n })?,
            EqKnob::Slope => match t.trim() {
                "brickwall" | "bw" => Shape::BRICKWALL,
                t => number(t.trim_end_matches("db/oct").trim_end_matches("db"))?,
            },
        };
        v.is_finite().then_some(v)
    }
}

/// A frequency as typed: "100", "2k", "1.5 kHz", "A4", "C#2+13" (cents).
fn parse_freq(t: &str) -> Option<f32> {
    let t = t.trim();
    let hz = t.trim_end_matches("hz").trim();
    if let Some(k) = hz.strip_suffix('k') {
        return k.trim().parse::<f32>().ok().map(|k| k * 1_000.0);
    }
    if let Ok(v) = hz.parse::<f32>() {
        return Some(v);
    }
    // A note: its letter, a sharp or flat, its octave, and cents.
    let mut chars = t.chars().peekable();
    let letter = chars.next()?;
    let mut semis: i32 = match letter {
        'c' => 0,
        'd' => 2,
        'e' => 4,
        'f' => 5,
        'g' => 7,
        'a' => 9,
        'b' => 11,
        _ => return None,
    };
    match chars.peek() {
        Some('#') => {
            semis += 1;
            chars.next();
        }
        Some('b') => {
            semis -= 1;
            chars.next();
        }
        _ => {}
    }
    let rest: String = chars.collect();
    let (octave, cents) = match rest.find(['+', '-'].as_ref()).filter(|&i| i > 0) {
        Some(i) => (&rest[..i], rest[i..].trim().parse::<f32>().ok()?),
        None => (rest.as_str(), 0.0),
    };
    let octave: i32 = octave.trim().parse().ok()?;
    let midi = (octave + 1) * 12 + semis;
    Some(440.0 * 2f32.powf((midi as f32 - 69.0 + cents / 100.0) / 12.0))
}

/// "850 Hz", "1.20 kHz", "12.5 kHz".
pub fn hz_text(hz: f32) -> String {
    match hz {
        h if h < 1_000.0 => format!("{h:.0} Hz"),
        h if h < 10_000.0 => format!("{:.2} kHz", h / 1_000.0),
        h => format!("{:.1} kHz", h / 1_000.0),
    }
}

/// "24 dB/oct", "20.5 dB/oct", or "Brickwall".
pub fn slope_text(s: f32) -> String {
    match s {
        Shape::BRICKWALL => "Brickwall".into(),
        s if s.fract() == 0.0 => format!("{s:.0} dB/oct"),
        s => format!("{} dB/oct", (s * 10.0).round() / 10.0),
    }
}

/// A slope a step steeper or gentler (`by` steps): through its shape's slopes, or with `fine`
/// a dB an octave at a time, fractions between.
pub fn step_slope(b: &mut EqBand, by: i32, fine: bool) {
    let slopes = b.shape.slopes();
    if slopes.is_empty() || by == 0 {
        return;
    }
    if fine && !matches!(b.shape, Shape::Notch | Shape::AllPass) && b.slope < Shape::BRICKWALL {
        b.slope = b.shape.fit_slope((b.slope + by as f32).round());
        return;
    }
    // From a fraction, the next of its slopes that way.
    let at = match by > 0 {
        true => slopes
            .iter()
            .position(|&s| s > b.slope)
            .unwrap_or(slopes.len() - 1),
        false => slopes.iter().rposition(|&s| s < b.slope).unwrap_or(0),
    };
    let to = at as i32 + by - by.signum();
    b.slope = slopes[to.clamp(0, slopes.len() as i32 - 1) as usize];
}

fn q_text(q: f32) -> String {
    if q < 10.0 {
        format!("{q:.2}")
    } else {
        format!("{q:.1}")
    }
}

/// The note a frequency is nearest, and how many cents off it is.
fn note_of(hz: f32) -> (u8, f32) {
    let exact = 69.0 + 12.0 * (hz.max(1.0) / 440.0).log2();
    let note = exact.round().clamp(0.0, 127.0);
    (note as u8, (exact - note) * 100.0)
}

fn note_text(hz: f32) -> String {
    let (note, cents) = note_of(hz);
    let name = pina_engine::samples::note_name(note);
    match cents.round() as i32 {
        0 => name,
        c => format!("{name} {c:+}"),
    }
}

fn note_hz(note: u8) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}

/// The Inspector's quick EQ knobs, each a band of the channel's EQ found by what it is, and
/// added when first turned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quick {
    /// A bell between 1.5 and 6 kHz.
    Presence,
    /// A high shelf from 5 kHz up.
    Air,
}

impl Quick {
    pub fn find(self, eq: &Eq) -> Option<usize> {
        eq.bands.iter().position(|b| match self {
            Quick::Presence => b.shape == Shape::Bell && (1_500.0..=6_000.0).contains(&b.freq),
            Quick::Air => b.shape == Shape::HighShelf && b.freq >= 5_000.0,
        })
    }

    pub fn gain(self, eq: &Eq) -> f32 {
        self.find(eq).map_or(0.0, |i| eq.bands[i].gain_db)
    }

    /// Its band, added if it isn't there.
    pub fn band(self, eq: &mut Eq) -> Option<usize> {
        self.find(eq).or_else(|| {
            eq.bands.push(match self {
                Quick::Presence => EqBand::new(Shape::Bell, 3_000.0, 0.0),
                Quick::Air => EqBand::new(Shape::HighShelf, 10_000.0, 0.0),
            })
        })
    }
}

/// The EQ's first low cut, if it's on: its frequency (else 0).
pub fn low_cut(eq: &Eq) -> f32 {
    eq.nth(Shape::LowCut, 0)
        .map(|i| eq.bands[i])
        .filter(|b| !b.off)
        .map_or(0.0, |b| b.freq)
}

/// Sets the EQ's first low cut (adding one at the front if it has none): 20 Hz or less is off.
pub fn set_low_cut(eq: &mut Eq, hz: f32) {
    let on = hz > 20.0;
    match eq.nth(Shape::LowCut, 0) {
        Some(i) => {
            let b = &mut eq.bands[i];
            (b.freq, b.off) = (hz.max(20.0), !on);
        }
        None if on => {
            eq.bands.insert(0, EqBand::new(Shape::LowCut, hz, 0.0));
        }
        None => {}
    }
}

/// The band a click (or, `drag`ged, a pull of the curve) would make at a place on the plot.
fn band_at(geo: &Geo, x: f32, y: f32, drag: bool) -> EqBand {
    let freq = geo.freq(x).clamp(10.0, 30_000.0);
    let edge = x / geo.w;
    let gain = (geo.db(y) * 2.0).round() / 2.0;
    let shape = match () {
        _ if edge < 0.04 => {
            if drag {
                Shape::LowShelf
            } else {
                Shape::LowCut
            }
        }
        _ if edge > 0.96 => {
            if drag {
                Shape::HighShelf
            } else {
                Shape::HighCut
            }
        }
        _ if !drag && gain <= -geo.range * 0.85 => Shape::Notch,
        _ => Shape::Bell,
    };
    let gain = match shape.has_gain() && !drag {
        true => gain.clamp(-30.0, 30.0),
        false => 0.0,
    };
    EqBand::new(shape, freq.round() as f32, gain)
}

/// Pina Studio's own presets: starting points, by what they're for.
pub fn factory() -> Vec<(&'static str, Eq)> {
    let b = |shape: Shape, freq: f32, gain: f32, q: Option<f32>, slope: u8| {
        let mut band = EqBand::new(shape, freq, gain);
        band.q = q.unwrap_or(band.q);
        band.slope = slope as f32;
        band
    };
    let eq = |bands: &[EqBand]| Eq {
        bands: Bands::from_slice(bands),
        ..Eq::default()
    };
    let dynamic = |mut band: EqBand, range: f32, threshold: f32| {
        band.dynamic = Some(Dynamic {
            range_db: range,
            threshold_db: Some(threshold),
            ..Dynamic::default()
        });
        band
    };
    vec![
        (
            "Vocal: clear and present",
            eq(&[
                b(Shape::LowCut, 90.0, 0.0, None, 24),
                b(Shape::Bell, 300.0, -2.5, Some(1.2), 12),
                dynamic(b(Shape::Bell, 5_500.0, 0.0, Some(3.0), 12), -4.0, -30.0),
                b(Shape::Bell, 3_000.0, 2.0, Some(1.0), 12),
                b(Shape::HighShelf, 11_000.0, 2.5, None, 12),
            ]),
        ),
        (
            "Kick: tight and punchy",
            eq(&[
                b(Shape::LowCut, 28.0, 0.0, None, 24),
                b(Shape::Bell, 60.0, 3.0, Some(1.4), 12),
                b(Shape::Bell, 350.0, -4.0, Some(1.5), 12),
                b(Shape::Bell, 4_000.0, 2.5, Some(1.2), 12),
            ]),
        ),
        (
            "Bass: clean low end",
            eq(&[
                b(Shape::LowCut, 32.0, 0.0, None, 24),
                b(Shape::Bell, 90.0, 2.0, Some(1.0), 12),
                dynamic(b(Shape::Bell, 220.0, 0.0, Some(1.5), 12), -4.0, -28.0),
                b(Shape::HighCut, 7_000.0, 0.0, None, 12),
            ]),
        ),
        (
            "Drum bus: snap",
            eq(&[
                b(Shape::LowCut, 30.0, 0.0, None, 12),
                b(Shape::Bell, 450.0, -2.0, Some(1.0), 12),
                b(Shape::Bell, 5_000.0, 2.0, Some(0.8), 12),
                b(Shape::HighShelf, 12_000.0, 1.5, None, 12),
            ]),
        ),
        (
            "Pads: out of the way",
            eq(&[
                b(Shape::LowCut, 200.0, 0.0, None, 24),
                b(Shape::Bell, 2_500.0, -2.0, Some(0.8), 12),
                b(Shape::HighShelf, 9_000.0, -2.0, None, 12),
            ]),
        ),
        (
            "Mix: smile",
            eq(&[
                b(Shape::LowShelf, 90.0, 1.5, None, 12),
                b(Shape::Bell, 400.0, -1.0, Some(0.7), 12),
                b(Shape::HighShelf, 10_000.0, 1.5, None, 12),
            ]),
        ),
        (
            "Mastering: mono lows",
            eq(&[{
                let mut cut = b(Shape::LowCut, 120.0, 0.0, None, 12);
                cut.placement = Placement::Side;
                cut
            }]),
        ),
        (
            "Telephone",
            eq(&[
                b(Shape::LowCut, 400.0, 0.0, None, 24),
                b(Shape::Bell, 1_500.0, 4.0, Some(1.0), 12),
                b(Shape::HighCut, 3_400.0, 0.0, None, 24),
            ]),
        ),
    ]
}

/// Where saved presets go (a folder of their own while testing).
fn presets_dir() -> std::path::PathBuf {
    match cfg!(test) {
        true => std::env::temp_dir().join(format!("pina-test-eq-presets-{}", std::process::id())),
        false => pina_engine::paths::support().join("EQ Presets"),
    }
}

/// The presets saved, by name.
pub fn saved_presets() -> Vec<(String, std::path::PathBuf)> {
    let mut list: Vec<(String, std::path::PathBuf)> = std::fs::read_dir(presets_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| {
            let name = p
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (name, p)
        })
        .collect();
    list.sort();
    list
}

impl Workspace {
    /// A channel's EQ in its window (one added, if it had none).
    pub fn open_eq(&mut self, ch: Channel, cx: &mut Context<Self>) {
        if self.strip_dsp(ch).0.is_none() {
            self.edit(cx, |ws| *ws.strip_dsp_mut(ch).0 = Some(Eq::default()));
        }
        self.channel = ch;
        if let Channel::Track(i) = ch {
            self.track = i;
        }
        match &mut self.eq_view {
            Some(v) if v.ch == ch => v.focused = true,
            Some(v) => {
                v.ch = ch;
                v.picked.clear();
                (v.hover, v.solo, v.other, v.on_b) = (None, None, None, false);
                v.focused = true;
            }
            None => {
                let w = (self.viewport_w - 80.0).clamp(MIN_W, 1_000.0);
                let h = (self.viewport_h - 120.0).clamp(MIN_H, 600.0);
                let at = (
                    ((self.viewport_w - w) / 2.0).max(0.0),
                    ((self.viewport_h - h) / 2.0).max(CONTROL_BAR_H),
                );
                self.eq_view = Some(EqView::new(ch, at, (w, h)));
            }
        }
        cx.notify();
    }

    pub fn close_eq(&mut self, cx: &mut Context<Self>) {
        self.eq_view = None;
        self.player.watch_eq(None);
        self.player.watch_lanes(false);
        self.player.solo_band(None);
        cx.notify();
    }

    /// The EQ the window shows.
    fn shown_eq(&self) -> Option<(Channel, Eq)> {
        let ch = self.eq_view.as_ref()?.ch;
        Some((ch, self.strip_dsp(ch).0?))
    }

    /// The shown EQ as it's heard now: its bands where their automation has them.
    pub(super) fn eq_as_heard(&self, ch: Channel, mut eq: Eq) -> Eq {
        let Channel::Track(t) = ch else { return eq };
        for (i, b) in eq.bands.iter_mut().enumerate() {
            for field in [EqField::Freq, EqField::Gain, EqField::Q] {
                if let Some(v) = self.automated(t, Param::Eq(i as u8, field)) {
                    match field {
                        EqField::Freq => b.freq = v,
                        EqField::Gain => b.gain_db = v,
                        EqField::Q => b.q = v,
                    }
                }
            }
        }
        eq
    }

    /// The bands with `ids` moved by hand: what that does to their automation (recorded in
    /// Touch, Latch or Write; taken over from it in Read), as any track control.
    fn eq_moved(&mut self, ids: &[u16], fields: &[EqField]) {
        let Some((Channel::Track(t), eq)) = self.shown_eq() else {
            return;
        };
        let mut moves = Vec::new();
        for id in ids {
            let Some(i) = eq.bands.find(*id) else {
                continue;
            };
            let b = eq.bands[i];
            for &field in fields {
                if field == EqField::Gain && !b.shape.has_gain() {
                    continue;
                }
                let v = match field {
                    EqField::Freq => b.freq,
                    EqField::Gain => b.gain_db,
                    EqField::Q => b.q,
                };
                moves.push((Param::Eq(i as u8, field), v));
            }
        }
        for (param, v) in moves {
            self.moved(t, param, v);
        }
    }

    /// Let go of the EQ's bands: their automation carries on (or stops recording).
    pub fn eq_let_go(&mut self) {
        let Some(Channel::Track(t)) = self.eq_view.as_ref().map(|v| v.ch) else {
            return;
        };
        let Some(id) = self.session.project.tracks.get(t).map(|tr| tr.id) else {
            return;
        };
        let held: Vec<Param> = self
            .taken
            .keys()
            .chain(self.touches.keys())
            .filter(|(track, p)| *track == id && matches!(p, Param::Eq(..)))
            .map(|&(_, p)| p)
            .collect();
        for p in held {
            self.let_go(t, p);
        }
    }

    fn eq_band(&self, id: u16) -> Option<(usize, EqBand)> {
        let (_, eq) = self.shown_eq()?;
        let i = eq.bands.find(id)?;
        Some((i, eq.bands[i]))
    }

    fn eq_geo(&self) -> Option<Geo> {
        let rate = self.session.project.sample_rate as f64;
        Some(self.eq_view.as_ref()?.geo(rate))
    }

    /// The shown EQ, to change (no edit of its own: inside one, or a drag's).
    fn eq_mut(&mut self) -> Option<&mut Eq> {
        let ch = self.eq_view.as_ref()?.ch;
        self.strip_dsp_mut(ch).0.as_mut()
    }

    /// Changes the bands picked, as one edit.
    fn edit_picked(&mut self, cx: &mut Context<Self>, f: impl Fn(&mut EqBand)) {
        let picked = self.eq_view.as_ref().map(|v| v.picked.clone());
        let Some(picked) = picked.filter(|p| !p.is_empty()) else {
            return;
        };
        self.edit(cx, |ws| {
            if let Some(eq) = ws.eq_mut() {
                for id in &picked {
                    if let Some(i) = eq.bands.find(*id) {
                        f(&mut eq.bands[i]);
                    }
                }
            }
        });
    }

    /// Each frame the window's open: tells the engine which EQ to tap and which band to solo,
    /// and moves the analyzer and meter on with what it's heard.
    pub fn eq_frame(&mut self) {
        self.list_frame();
        let rate = self.session.project.sample_rate as f64;
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        let p = &self.session.project;
        let id = match view.ch {
            Channel::Track(i) => p.tracks.get(i).map(|t| Some(t.id)),
            Channel::Aux(i) => p.auxes.get(i).map(|a| Some(a.id)),
            Channel::Master => Some(None),
        };
        self.player.watch_eq(id);
        self.player.solo_band(view.solo.map(|s| (s.id, s.level)));
        let geo = view.geo(rate);
        let n = (geo.w / 3.0) as usize;
        let fresh_columns = view.columns.len() != n
            || view
                .columns
                .first()
                .is_none_or(|&f| (f - geo.lo).abs() > 1e-6)
            || view
                .columns
                .last()
                .is_none_or(|&f| (f - geo.hi).abs() > 1e-3);
        let floor = -(view.analysis.range as f32);
        if fresh_columns {
            view.columns = (0..n)
                .map(|i| geo.freq(i as f32 / (n - 1) as f32 * geo.w))
                .collect();
            view.pre = vec![floor; n];
            view.post = vec![floor; n];
        }
        self.player.watch_ext(view.ext);
        let tap = self.player.eq_tap();
        let [pre, left, right, ext] = &mut *view.buf;
        let written = tap.read(pre, left, right);
        let ext_written = tap.read_ext(ext);
        let heard = written != view.written;
        view.written = written;
        view.hearing = heard && pre[TAP_LEN - 1024..].iter().any(|x| x.abs() > 1e-4);
        // The meter: each side's peak over the last block or so, falling away.
        for (side, buf) in [&*left, &*right].into_iter().enumerate() {
            let peak = buf[TAP_LEN - 1024..]
                .iter()
                .fold(0f32, |m, x| m.max(x.abs()));
            let db = if heard {
                20.0 * peak.max(1e-6).log10()
            } else {
                -120.0
            };
            view.meter[side] = db.max(view.meter[side] - 1.5);
            view.peak = view.peak.max(view.meter[side]);
        }
        // EQ Match learns while it listens: this track going in, and the reference.
        if let Some(m) = view.matching.as_mut()
            && heard
        {
            m.learn(
                &pre[..],
                (ext_written > 0 && view.ext.is_some()).then_some(&ext[..]),
                rate,
            );
        }
        // Spectrum Grab: the pointer resting on the spectrum a moment grabs it.
        let on_spectrum = view.pointer.is_some_and(|(x, y)| {
            let i = ((x / geo.w) * (n.max(2) - 1) as f32).round() as usize;
            let floor = -(view.analysis.range as f32);
            view.post.get(i).is_some_and(|&db| {
                db > floor + 3.0 && ((-db / view.analysis.range as f32 * geo.h) - y).abs() < 14.0
            })
        });
        let can_grab = view.analysis.grab
            && view.analysis.post
            && view.hover.is_none()
            && view.grabbing.is_none()
            && !view.sketching
            && view.matching.is_none();
        match (can_grab && on_spectrum, view.grab_since) {
            (true, None) => view.grab_since = Some(std::time::Instant::now()),
            (true, Some(at)) if at.elapsed().as_secs_f32() > 1.2 => {
                view.grabbing = Some(view.post.clone());
                view.grab_since = None;
            }
            (false, _) => view.grab_since = None,
            _ => {}
        }
        if view.grabbing.is_some() && view.pointer.is_none() {
            view.grabbing = None;
        }
        if !(view.analysis.pre || view.analysis.post) {
            return;
        }
        let len = view.analysis.len();
        let tilt = view.analysis.tilt;
        let fall = view.analysis.fall();
        let freeze = view.analysis.freeze;
        let post: Vec<f32> = left[TAP_LEN - len..]
            .iter()
            .zip(&right[TAP_LEN - len..])
            .map(|(a, b)| 0.5 * (a + b))
            .collect();
        let fresh = |s: &[f32], on: bool| -> Vec<f32> {
            match heard && on {
                true => spectrum_db(s, rate, &view.columns),
                false => vec![floor - 30.0; n],
            }
        };
        let now = (
            fresh(&pre[TAP_LEN - len..], view.analysis.pre),
            fresh(&post, view.analysis.post),
        );
        // The track compared with.
        let ext_now = match (view.ext.is_some(), ext_written != 0) {
            (true, true) => spectrum_db(&ext[TAP_LEN - len..], rate, &view.columns),
            _ => vec![floor - 30.0; n],
        };
        if view.ext_spec.len() != n {
            view.ext_spec = vec![floor; n];
        }
        for (smooth, now) in [
            (&mut view.pre, now.0),
            (&mut view.post, now.1),
            (&mut view.ext_spec, ext_now),
        ] {
            for (i, (s, v)) in smooth.iter_mut().zip(now).enumerate() {
                let v = (v + tilt * (view.columns[i] / 1_000.0).log2() as f32).max(floor);
                // Up quickly, down slowly; frozen, it only rises.
                *s = match (freeze, v > *s) {
                    (_, true) => *s + (v - *s) * 0.6,
                    (true, false) => *s,
                    (false, false) => (*s - fall).max(v),
                };
            }
        }
    }

    /// A band made where the plot was clicked (or pulled): picked; ⌥, dynamic.
    fn make_band(&mut self, mut band: EqBand, alt: bool, cx: &mut Context<Self>) -> Option<u16> {
        if alt && band.shape.has_gain() {
            band.dynamic = Some(Dynamic::default());
        }
        let mut id = None;
        self.edit(cx, |ws| {
            if let Some(eq) = ws.eq_mut()
                && let Some(i) = eq.bands.push(band)
            {
                id = Some(eq.bands[i].id);
            }
        });
        match id {
            Some(id) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked = vec![id];
                }
            }
            None => {
                self.session.cancel();
                self.status = Some(format!("An EQ has at most {MAX_BANDS} bands."));
            }
        }
        cx.notify();
        id
    }

    /// The bands picked as they are now: where a drag of them starts from.
    fn hold_picked(&mut self) {
        let Some((_, eq)) = self.shown_eq() else {
            return;
        };
        if let Some(v) = self.eq_view.as_mut() {
            v.start = v
                .picked
                .iter()
                .filter_map(|&id| eq.bands.find(id).map(|i| eq.bands[i]))
                .collect();
        }
    }

    /// A band's dot pressed: picked (⌘ adds it, ⇧ a run to it), and dragging the bands picked.
    fn press_dot(&mut self, id: u16, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let m = e.modifiers;
        if e.click_count == 2 && !m.alt {
            // Double-click: type its frequency (the field opens once the click's done).
            self.pending_name = Some(Name::Eq(id, EqKnob::Freq));
            cx.notify();
            return;
        }
        if m.alt && m.platform {
            return self.cycle_shape(id, cx);
        }
        if m.alt && m.shift {
            return self.cycle_slope(id, cx);
        }
        let order = self.eq_order();
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        view.focused = true;
        let picked = view.picked.contains(&id);
        let mut deselect = false;
        if m.platform {
            match picked {
                true => deselect = true,
                false => view.picked.push(id),
            }
        } else if m.shift && !picked {
            // A run: from the band picked last to this one, as they lie across the display.
            let from = view.lead().and_then(|l| order.iter().position(|&o| o == l));
            let to = order.iter().position(|&o| o == id);
            match (from, to) {
                (Some(a), Some(b)) => {
                    let (a, b) = (a.min(b), a.max(b));
                    for &o in &order[a..=b] {
                        if !view.picked.contains(&o) {
                            view.picked.push(o);
                        }
                    }
                }
                _ => view.picked = vec![id],
            }
        } else if !picked {
            view.picked = vec![id];
        }
        // The one pressed leads.
        if let Some(i) = view.picked.iter().position(|&p| p == id) {
            let lead = view.picked.remove(i);
            view.picked.push(lead);
        }
        self.hold_picked();
        self.begin_drag(
            Drag::EqNode {
                id,
                from: pt(e.position),
                q: m.platform && !deselect,
                alt: m.alt,
                axis: 0,
                moved: false,
                deselect,
            },
            cx,
        );
    }

    /// The bands' ids as they lie across the display, low to high.
    fn eq_order(&self) -> Vec<u16> {
        let Some((_, eq)) = self.shown_eq() else {
            return Vec::new();
        };
        let mut bands: Vec<(f32, u16)> = eq.bands.iter().map(|b| (b.freq, b.id)).collect();
        bands.sort_by(|a, b| a.0.total_cmp(&b.0));
        bands.into_iter().map(|(_, id)| id).collect()
    }

    fn cycle_shape(&mut self, id: u16, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        let i = Shape::ALL.iter().position(|&s| s == b.shape).unwrap_or(0);
        let next = Shape::ALL[(i + 1) % Shape::ALL.len()];
        if let Some(v) = self.eq_view.as_mut() {
            v.picked = vec![id];
        }
        self.eq_op(EqOp::Shape(next), cx);
    }

    fn cycle_slope(&mut self, id: u16, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        let slopes = b.shape.slopes();
        if slopes.is_empty() {
            return;
        }
        // The next steeper; from the steepest, the gentlest.
        let next = slopes
            .iter()
            .copied()
            .find(|&s| s > b.slope)
            .unwrap_or(slopes[0]);
        if let Some(v) = self.eq_view.as_mut() {
            v.picked = vec![id];
        }
        self.eq_op(EqOp::Slope(next), cx);
    }

    /// The picked bands dragged by one of them: across moves their frequencies (each by the
    /// same ratio), up and down their gains (each in proportion), or with ⌘ their widths (a
    /// cut's, notch's or band pass's always). ⌥ keeps to the way it first moves.
    #[allow(clippy::too_many_arguments)]
    pub fn drag_eq_node(
        &mut self,
        id: u16,
        from: gpui::Point<f32>,
        q_only: bool,
        alt: bool,
        axis: u8,
        p: gpui::Point<f32>,
    ) -> u8 {
        let Some(geo) = self.eq_geo() else {
            return axis;
        };
        let piano = self.eq_view.as_ref().is_some_and(|v| v.piano);
        let start = self
            .eq_view
            .as_ref()
            .map(|v| v.start.clone())
            .unwrap_or_default();
        let Some(lead) = start.iter().find(|b| b.id == id).copied() else {
            return axis;
        };
        let by = if self.fine { 0.1 } else { 1.0 };
        let (mut dx, mut dy) = ((p.x - from.x) * by, (p.y - from.y) * by);
        // ⌥: one way only, the way it first went.
        let axis = match (alt, axis) {
            (true, 0) if dx.abs().max(dy.abs()) > 3.0 => {
                if dx.abs() > dy.abs() {
                    1
                } else {
                    2
                }
            }
            (_, a) => a,
        };
        match (alt, axis) {
            (true, 1) => dy = 0.0,
            (true, 2) => dx = 0.0,
            (true, _) => (dx, dy) = (0.0, 0.0),
            _ => {}
        }
        let ratio = 2f32.powf(dx / geo.w * geo.octaves());
        let mut lead_freq = (lead.freq * ratio).clamp(10.0, 30_000.0);
        if piano {
            lead_freq = note_hz(note_of(lead_freq).0);
        }
        let ratio = lead_freq / lead.freq;
        let fine = self.fine;
        let Some(eq) = self.eq_mut() else { return axis };
        let scale = eq.scale.max(0.01);
        let per_px = geo.range / (geo.h / 2.0 - PAD) / scale;
        let lead_gain = lead.gain_db - dy * per_px;
        let q_by = 2f32.powf(-dy / 60.0);
        let mut widest = 0f32;
        for s in &start {
            let Some(i) = eq.bands.find(s.id) else {
                continue;
            };
            let b = &mut eq.bands[i];
            b.freq = (s.freq * ratio).clamp(10.0, 30_000.0);
            if !fine && !piano {
                b.freq = b.freq.round();
            }
            if q_only || !b.shape.has_gain() {
                b.q = (s.q * q_by).clamp(0.025, 40.0);
            } else if dy != 0.0 {
                // The rest go in proportion to the one dragged; from flat, by as much.
                let g = match lead.gain_db.abs() > 0.5 {
                    true => s.gain_db * lead_gain / lead.gain_db,
                    false => s.gain_db + (lead_gain - lead.gain_db),
                };
                b.gain_db = ((g * 10.0).round() / 10.0).clamp(-30.0, 30.0);
                widest = widest.max(b.gain_db.abs() * scale);
            }
        }
        // Dragged past the display's range: it grows to show it.
        if let Some(v) = self.eq_view.as_mut()
            && widest > v.range as f32
        {
            v.range = RANGES
                .into_iter()
                .find(|&r| r as f32 >= widest)
                .unwrap_or(30);
        }
        let ids: Vec<u16> = start.iter().map(|b| b.id).collect();
        self.eq_moved(&ids, &[EqField::Freq, EqField::Gain, EqField::Q]);
        axis
    }

    /// The plot pressed where there's no dot: a click makes a band (once none's picked; else it
    /// lets them go), a pull of the curve makes one and drags it, a drag elsewhere picks the
    /// bands in a box.
    fn press_plot(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(geo) = self.eq_geo() else { return };
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        view.focused = true;
        view.popup = None;
        let o = view.display.get().origin;
        let (x, y) = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
        let m = e.modifiers;
        let sketching = view.sketching;
        // Spectrum Grab: a peak pressed, a bell to pull.
        if view.grabbing.is_some() && self.grab_peak(x, e, cx) {
            return;
        }
        if let Some(v) = self.eq_view.as_mut() {
            v.grabbing = None;
        }
        if e.click_count == 2 || m.platform {
            let band = band_at(&geo, x, y, false);
            self.make_band(band, m.alt, cx);
            return;
        }
        let curve = self.eq_curve_at(geo.freq(x));
        let on_curve = (geo.y(curve) - y).abs() < 8.0;
        // EQ Sketch, or on an empty EQ a drag off the line: a curve drawn.
        let empty = self.shown_eq().is_some_and(|(_, e)| e.bands.is_empty());
        if sketching || (empty && !on_curve) {
            if let Some(v) = self.eq_view.as_mut() {
                v.stroke = vec![(x, y)];
            }
            self.begin_drag(Drag::EqSketch, cx);
            return;
        }
        self.begin_drag(
            Drag::EqPress {
                from: pt(e.position),
                on_curve,
                alt: m.alt,
                moved: false,
            },
            cx,
        );
    }

    /// What the shown EQ does at a frequency, as the overall curve has it.
    fn eq_curve_at(&self, freq: f64) -> f32 {
        let rate = self.session.project.sample_rate as f64;
        self.shown_eq().map_or(0.0, |(_, eq)| {
            pina_engine::eq::eq_response(&eq, freq, rate) as f32
        })
    }

    /// A press on the plot dragged: off the curve, a box picking bands; on it, a band made
    /// there (a shelf at either end) and pulled.
    pub fn drag_eq_press(
        &mut self,
        from: gpui::Point<f32>,
        on_curve: bool,
        alt: bool,
        p: gpui::Point<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some(geo) = self.eq_geo() else { return };
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        let o = pt(view.display.get().origin);
        let (a, b) = ((from.x - o.x, from.y - o.y), (p.x - o.x, p.y - o.y));
        if (p.x - from.x).hypot(p.y - from.y) < 4.0 {
            return;
        }
        if !on_curve {
            view.marquee = Some((a, b));
            let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
            let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
            let Some((_, eq)) = self.shown_eq() else {
                return;
            };
            let scale = eq.scale;
            let inside: Vec<u16> = eq
                .bands
                .iter()
                .filter(|band| {
                    let (x, y) = (geo.x(band.freq as f64), geo.y(dot_db(band, scale)));
                    (x0..=x1).contains(&x) && (y0..=y1).contains(&y)
                })
                .map(|band| band.id)
                .collect();
            if let Some(v) = self.eq_view.as_mut() {
                v.picked = inside;
            }
            cx.notify();
            return;
        }
        // Pulled off the curve: a band made where it was pressed, and dragged from there.
        let mut band = band_at(&geo, a.0, a.1, true);
        band.gain_db = 0.0;
        let Some(id) = self.make_band(band, alt, cx) else {
            self.drag = None;
            return;
        };
        self.hold_picked();
        self.drag = Some(Drag::EqNode {
            id,
            from,
            q: false,
            alt: false,
            axis: 0,
            moved: true,
            deselect: false,
        });
        self.drag_eq_node(id, from, false, false, 0, p);
        self.changed(cx);
    }

    /// The plot let go of: a click (no drag) makes a band, or lets the picked ones go.
    pub fn release_eq_press(
        &mut self,
        from: gpui::Point<f32>,
        moved: bool,
        alt: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(geo) = self.eq_geo() else { return };
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        if view.marquee.take().is_some() || moved {
            cx.notify();
            return;
        }
        if !view.picked.is_empty() {
            view.picked.clear();
            cx.notify();
            return;
        }
        let o = pt(view.display.get().origin);
        let band = band_at(&geo, from.x - o.x, from.y - o.y, false);
        self.make_band(band, alt, cx);
    }

    /// A dot let go of without moving: ⌥ switches it off, ⌘ on one picked lets it go, on the
    /// piano it snaps to its note.
    pub fn release_eq_node(&mut self, id: u16, alt: bool, deselect: bool, cx: &mut Context<Self>) {
        if alt {
            if let Some(v) = self.eq_view.as_mut() {
                v.picked = vec![id];
            }
            self.eq_op(EqOp::Off, cx);
            return;
        }
        if deselect {
            if let Some(v) = self.eq_view.as_mut() {
                v.picked.retain(|&p| p != id);
            }
            cx.notify();
            return;
        }
        if self.eq_view.as_ref().is_some_and(|v| v.piano) {
            self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut()
                    && let Some(i) = eq.bands.find(id)
                {
                    let b = &mut eq.bands[i];
                    b.freq = note_hz(note_of(b.freq).0);
                }
            });
        }
    }

    /// The wheel over a band: its width (a cut's slope, a step at a time); with ⌥ its dynamic
    /// range, with ⌘ its gain, with both one traded for the other. ⇧ fine. If it's picked, the
    /// others picked go with it.
    fn wheel_band(&mut self, id: u16, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        // With ⇧ a mouse's wheel may come sideways.
        let d = e.delta.pixel_delta(px(16.));
        let dy = f32::from(if d.y != px(0.) { d.y } else { d.x });
        if dy == 0.0 {
            return;
        }
        let m = e.modifiers;
        let fine = if m.shift { 0.25 } else { 1.0 };
        let ids: Vec<u16> = match self.eq_view.as_ref() {
            Some(v) if v.picked.contains(&id) => v.picked.clone(),
            _ => vec![id],
        };
        let steps = (dy / 16.0) * fine;
        self.edit(cx, |ws| {
            let Some(eq) = ws.eq_mut() else { return };
            for id in &ids {
                let Some(i) = eq.bands.find(*id) else {
                    continue;
                };
                let b = &mut eq.bands[i];
                match (m.alt, m.platform) {
                    (true, true) => {
                        if let Some(d) = b.dynamic.as_mut() {
                            d.range_db = (d.range_db - steps * 0.5).clamp(-30.0, 30.0);
                            b.gain_db = (b.gain_db + steps * 0.5).clamp(-30.0, 30.0);
                        }
                    }
                    (true, false) if b.shape.has_gain() => {
                        let d = b.dynamic.get_or_insert_default();
                        d.range_db = (d.range_db + steps * 0.5).clamp(-30.0, 30.0);
                    }
                    (false, true) if b.shape.has_gain() => {
                        b.gain_db = (b.gain_db + steps * 0.5).clamp(-30.0, 30.0);
                    }
                    (false, false) if matches!(b.shape, Shape::LowCut | Shape::HighCut) => {
                        step_slope(b, steps.signum() as i32, m.shift);
                    }
                    _ => b.q = (b.q * 2f32.powf(steps / 8.0)).clamp(0.025, 40.0),
                }
            }
        });
        // An unchanged band (a cut at its steepest) leaves nothing to undo.
        if self.session.unchanged() {
            self.session.cancel();
        }
    }

    /// A knob dragged: up turns it up. On a band picked with others, the others go with it.
    pub fn drag_eq_knob(&mut self, id: u16, knob: EqKnob, from: f32, start: f32, y: f32) {
        let fine = if self.fine { 0.1 } else { 1.0 };
        let pos = knob.pos(start) + (from - y) / 200.0 * fine;
        let v = knob.value(pos);
        let others: Vec<EqBand> = self
            .eq_view
            .as_ref()
            .filter(|v| v.picked.contains(&id))
            .map(|v| v.start.clone())
            .unwrap_or_default();
        let lead = others.iter().find(|b| b.id == id).copied();
        self.set_eq_knob(id, knob, v);
        // The others picked: their frequencies and widths by the same ratio, gains by as much.
        if let (Some(lead), Some(eq)) = (lead, self.eq_mut()) {
            for s in others.iter().filter(|s| s.id != id) {
                let Some(i) = eq.bands.find(s.id) else {
                    continue;
                };
                let b = &mut eq.bands[i];
                match knob {
                    EqKnob::Freq => b.freq = (s.freq * v / lead.freq).clamp(10.0, 30_000.0).round(),
                    EqKnob::Q => b.q = (s.q * v / lead.q).clamp(0.025, 40.0),
                    EqKnob::Gain => b.gain_db = (s.gain_db + v - lead.gain_db).clamp(-30.0, 30.0),
                    _ => {}
                }
            }
        }
        let field = match knob {
            EqKnob::Freq => Some(EqField::Freq),
            EqKnob::Gain => Some(EqField::Gain),
            EqKnob::Q => Some(EqField::Q),
            _ => None,
        };
        if let Some(field) = field {
            let mut ids: Vec<u16> = others.iter().map(|b| b.id).collect();
            if !ids.contains(&id) {
                ids.push(id);
            }
            self.eq_moved(&ids, &[field]);
        }
    }

    pub fn set_eq_knob(&mut self, id: u16, knob: EqKnob, v: f32) {
        let Some(eq) = self.eq_mut() else { return };
        match knob {
            EqKnob::Scale => eq.scale = v.clamp(0.0, 2.0),
            EqKnob::Output => eq.output_db = v.clamp(-36.0, 36.0),
            EqKnob::Pan => eq.pan = v.clamp(-1.0, 1.0),
            _ => {
                let Some(i) = eq.bands.find(id) else { return };
                let b = &mut eq.bands[i];
                match knob {
                    EqKnob::Freq => b.freq = v.clamp(10.0, 30_000.0),
                    EqKnob::Gain => b.gain_db = v.clamp(-30.0, 30.0),
                    EqKnob::Q => b.q = v.clamp(0.025, 40.0),
                    EqKnob::Range => {
                        b.dynamic.get_or_insert_default().range_db = v.clamp(-30.0, 30.0)
                    }
                    EqKnob::Threshold => {
                        b.dynamic.get_or_insert_default().threshold_db = Some(v.clamp(-80.0, 0.0))
                    }
                    EqKnob::Attack => {
                        b.dynamic.get_or_insert_default().attack_ms = Some(v.clamp(0.1, 1_000.0))
                    }
                    EqKnob::Release => {
                        b.dynamic.get_or_insert_default().release_ms = Some(v.clamp(1.0, 5_000.0))
                    }
                    EqKnob::Density => {
                        b.dynamic.get_or_insert_default().density = v.clamp(0.0, 1.0)
                    }
                    EqKnob::Slope => b.slope = b.shape.fit_slope(v),
                    EqKnob::Low | EqKnob::High => {
                        let d = b.dynamic.get_or_insert_default();
                        let mut t = d.trigger.unwrap_or([b.freq / 2.0, b.freq * 2.0]);
                        t[(knob == EqKnob::High) as usize] = v.clamp(10.0, 30_000.0);
                        d.trigger = Some(t);
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn eq_knob_value(&self, id: u16, knob: EqKnob) -> f32 {
        let Some((_, eq)) = self.shown_eq() else {
            return 0.0;
        };
        let b = eq.bands.find(id).map(|i| eq.bands[i]).unwrap_or_default();
        let d = b.dynamic.unwrap_or_default();
        let (attack, release) = auto_times(b.freq);
        let trigger = d.trigger.unwrap_or([b.freq / 2.0, b.freq * 2.0]);
        match knob {
            EqKnob::Freq => b.freq,
            EqKnob::Gain => b.gain_db,
            EqKnob::Q => b.q,
            EqKnob::Range => d.range_db,
            EqKnob::Threshold => d.threshold_db.unwrap_or(-30.0),
            EqKnob::Scale => eq.scale,
            EqKnob::Output => eq.output_db,
            EqKnob::Attack => d.attack_ms.unwrap_or(attack as f32),
            EqKnob::Release => d.release_ms.unwrap_or(release as f32),
            EqKnob::Low => trigger[0],
            EqKnob::High => trigger[1],
            EqKnob::Pan => eq.pan,
            EqKnob::Density => d.density,
            EqKnob::Slope => b.slope,
        }
    }

    /// What a knob goes back to on ⌘-click.
    fn eq_knob_reset(&mut self, id: u16, knob: EqKnob, cx: &mut Context<Self>) {
        let shape = self.eq_band(id).map_or(Shape::Bell, |(_, b)| b.shape);
        let d = Dynamic::default();
        // A dynamic band's threshold, attack and release go back to its own.
        if matches!(knob, EqKnob::Threshold | EqKnob::Attack | EqKnob::Release) {
            return self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut()
                    && let Some(i) = eq.bands.find(id)
                    && let Some(d) = eq.bands[i].dynamic.as_mut()
                {
                    match knob {
                        EqKnob::Threshold => d.threshold_db = None,
                        EqKnob::Attack => d.attack_ms = None,
                        _ => d.release_ms = None,
                    }
                }
            });
        }
        let v = match knob {
            EqKnob::Freq => 1_000.0,
            EqKnob::Gain | EqKnob::Output | EqKnob::Pan => 0.0,
            EqKnob::Q => shape.default_q(),
            EqKnob::Range => d.range_db,
            EqKnob::Density => d.density,
            EqKnob::Slope => 12.0,
            EqKnob::Scale => 1.0,
            EqKnob::Low => 100.0,
            EqKnob::High => 10_000.0,
            _ => return,
        };
        self.edit(cx, |ws| ws.set_eq_knob(id, knob, v));
    }

    /// A knob pressed: dragging it (⌘-click resets it, a double-click types a value).
    fn press_knob(&mut self, id: u16, knob: EqKnob, e: &MouseDownEvent, cx: &mut Context<Self>) {
        if e.modifiers.platform {
            return self.eq_knob_reset(id, knob, cx);
        }
        if e.click_count == 2 && knob != EqKnob::Slope {
            self.pending_name = Some(Name::Eq(id, knob));
            cx.notify();
            return;
        }
        self.hold_picked();
        let start = self.eq_knob_value(id, knob);
        self.begin_drag(
            Drag::EqKnob {
                id,
                knob,
                from: f32::from(e.position.y),
                x: f32::from(e.position.x),
                start,
            },
            cx,
        );
    }

    /// A band's slopes to pick from.
    pub fn eq_slope_menu(
        &mut self,
        id: u16,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        let items = b
            .shape
            .slopes()
            .iter()
            .map(|&s| {
                let tick = if s == b.slope { "✓ " } else { "" };
                (
                    format!("{tick}{}", slope_text(s)),
                    MenuAction::Eq(EqOp::Slope(s)),
                )
            })
            .collect();
        self.eq_menu(at, items, cx);
    }

    /// The wheel over a band's slope: a step through its slopes, ⇧ a dB an octave.
    fn wheel_slope(&mut self, id: u16, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        // With ⇧ a mouse's wheel may come sideways.
        let d = e.delta.pixel_delta(px(16.));
        let dy = f32::from(if d.y != px(0.) { d.y } else { d.x });
        if dy == 0.0 {
            return;
        }
        let fine = e.modifiers.shift;
        self.edit(cx, |ws| {
            if let Some(eq) = ws.eq_mut()
                && let Some(i) = eq.bands.find(id)
            {
                step_slope(&mut eq.bands[i], dy.signum() as i32, fine);
            }
        });
        if self.session.unchanged() {
            self.session.cancel();
        }
    }

    /// The text a typed value starts as.
    pub fn eq_value_text(&self, id: u16, knob: EqKnob) -> String {
        let v = self.eq_knob_value(id, knob);
        match knob {
            EqKnob::Freq | EqKnob::Low | EqKnob::High => match v < 1_000.0 {
                true => format!("{v:.0}"),
                false => format!("{:.2}k", v / 1_000.0),
            },
            EqKnob::Pan => EqKnob::Pan.text(v),
            EqKnob::Scale => format!("{:.0}%", v * 100.0),
            EqKnob::Q => q_text(v),
            _ => format!("{v:.1}"),
        }
    }

    /// A typed value set, if it reads as one.
    pub fn eq_value_typed(&mut self, id: u16, knob: EqKnob, text: &str, cx: &mut Context<Self>) {
        match knob.parse(text) {
            Some(v) => self.edit(cx, |ws| ws.set_eq_knob(id, knob, v)),
            None => {
                self.status = Some(format!(
                    "{text:?} isn't a {}: try {}",
                    knob.label().to_lowercase(),
                    match knob {
                        EqKnob::Freq => "100, 2k, A4 or C#2+13",
                        EqKnob::Gain => "-3, +4.5 or 2x",
                        _ => "a number",
                    }
                ));
                cx.notify();
            }
        }
    }

    /// Solo held on a band: what it works on, heard; dragged sideways it moves the band, up
    /// and down the level it's heard at.
    fn press_solo(&mut self, id: u16, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        if let Some(v) = self.eq_view.as_mut() {
            v.solo = Some(Solo { id, level: 0.0 });
            v.picked = vec![id];
        }
        self.hold_picked();
        self.begin_drag(
            Drag::EqSolo {
                id,
                from: pt(e.position),
                freq: b.freq,
            },
            cx,
        );
    }

    pub fn drag_eq_solo(
        &mut self,
        id: u16,
        from: gpui::Point<f32>,
        freq: f32,
        p: gpui::Point<f32>,
    ) {
        let Some(geo) = self.eq_geo() else { return };
        let level = ((from.y - p.y) / 6.0).clamp(-24.0, 12.0);
        if let Some(v) = self.eq_view.as_mut() {
            v.solo = Some(Solo { id, level });
        }
        let f = (freq * 2f32.powf((p.x - from.x) / geo.w * geo.octaves())).clamp(10.0, 30_000.0);
        if let Some(eq) = self.eq_mut()
            && let Some(i) = eq.bands.find(id)
        {
            eq.bands[i].freq = f.round();
        }
    }

    /// The frequency scale dragged: up zooms in round where it was pressed, down out;
    /// sideways pans.
    pub fn drag_eq_scale(&mut self, from: gpui::Point<f32>, span: (f64, f64), p: gpui::Point<f32>) {
        let Some(geo) = self.eq_geo() else { return };
        let Some(view) = self.eq_view.as_mut() else {
            return;
        };
        let o = pt(view.display.get().origin);
        let at = ((from.x - o.x) / geo.w).clamp(0.0, 1.0) as f64;
        let octaves = (span.1 / span.0).log2();
        let zoomed =
            (octaves * 2f64.powf((p.y - from.y) as f64 / 80.0)).clamp(1.0, (HIGH / LOW).log2());
        let anchor = span.0 * 2f64.powf(octaves * at);
        let pan = -((p.x - from.x) / geo.w) as f64 * zoomed;
        let lo =
            (anchor / 2f64.powf(zoomed * at) * 2f64.powf(pan)).clamp(LOW, HIGH / 2f64.powf(zoomed));
        view.span = (lo, lo * 2f64.powf(zoomed));
    }

    pub fn eq_op(&mut self, op: EqOp, cx: &mut Context<Self>) {
        let Some(ch) = self.eq_view.as_ref().map(|v| v.ch) else {
            return;
        };
        match op {
            EqOp::Shape(shape) => self.edit_picked(cx, |b| {
                if b.shape != shape {
                    b.q = shape.default_q();
                    // Its slope as near as the new shape takes (a brickwall's no bell's).
                    b.slope = match b.slope >= Shape::BRICKWALL {
                        true => 12.0,
                        false => shape.fit_slope(b.slope),
                    };
                    if !shape.has_gain() {
                        b.dynamic = None;
                    }
                }
                b.shape = shape;
            }),
            EqOp::Slope(slope) => self.edit_picked(cx, |b| {
                if b.shape.slope_range().is_some() {
                    b.slope = b.shape.fit_slope(slope);
                }
            }),
            EqOp::Placement(place) => self.edit_picked(cx, |b| b.placement = place),
            EqOp::Split => {
                let picked = self
                    .eq_view
                    .as_ref()
                    .map(|v| v.picked.clone())
                    .unwrap_or_default();
                let mut made = Vec::new();
                self.edit(cx, |ws| {
                    let Some(eq) = ws.eq_mut() else { return };
                    for id in &picked {
                        let Some(i) = eq.bands.find(*id) else {
                            continue;
                        };
                        // Left and right from a band on both; mid and side from one on either.
                        let (a, b) = match eq.bands[i].placement {
                            Placement::Mid | Placement::Side => (Placement::Mid, Placement::Side),
                            _ => (Placement::Left, Placement::Right),
                        };
                        eq.bands[i].placement = a;
                        let mut twin = eq.bands[i];
                        (twin.placement, twin.id) = (b, 0);
                        if let Some(j) = eq.bands.insert(i + 1, twin) {
                            made.push(eq.bands[j].id);
                        }
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked.extend(made);
                }
            }
            EqOp::Dynamic => {
                let all = self.picked_bands().iter().all(|b| b.dynamic.is_some());
                self.edit_picked(cx, |b| {
                    b.dynamic = match (all, b.shape.has_gain()) {
                        (false, true) => Some(b.dynamic.unwrap_or_default()),
                        _ => None,
                    }
                })
            }
            EqOp::Off => {
                let all_off = self.picked_bands().iter().all(|b| b.off);
                self.edit_picked(cx, |b| b.off = !all_off)
            }
            EqOp::Delete => {
                let picked = self
                    .eq_view
                    .as_ref()
                    .map(|v| v.picked.clone())
                    .unwrap_or_default();
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.eq_mut() {
                        for id in &picked {
                            if let Some(i) = eq.bands.find(*id) {
                                eq.bands.remove(i);
                            }
                        }
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked.clear();
                    v.hover = None;
                    v.solo = v.solo.filter(|s| !picked.contains(&s.id));
                }
            }
            EqOp::Pick(id, add) => {
                if let Some(v) = self.eq_view.as_mut() {
                    match add {
                        true if !v.picked.contains(&id) => v.picked.push(id),
                        true => {}
                        false => v.picked = vec![id],
                    }
                }
            }
            EqOp::SelectAll => {
                let ids = self.eq_order();
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked = ids;
                }
            }
            EqOp::Copy | EqOp::CopyAll => {
                let bands = match op {
                    EqOp::Copy => self.picked_bands(),
                    _ => self
                        .shown_eq()
                        .map(|(_, e)| e.bands.to_vec())
                        .unwrap_or_default(),
                };
                if !bands.is_empty() {
                    let n = bands.len();
                    self.eq_clipboard = bands;
                    self.status = Some(format!("{n} band{} copied", if n == 1 { "" } else { "s" }));
                }
            }
            EqOp::Paste => {
                let bands = self.eq_clipboard.clone();
                let mut made = Vec::new();
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.eq_mut() {
                        for b in &bands {
                            if let Some(i) = eq.bands.push(EqBand { id: 0, ..*b }) {
                                made.push(eq.bands[i].id);
                            }
                        }
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked = made;
                }
            }
            EqOp::Clear => {
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.eq_mut() {
                        eq.bands = Bands::default();
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked.clear();
                    v.solo = None;
                }
            }
            EqOp::Pre => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.pre = !v.analysis.pre;
                }
            }
            EqOp::Post => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.post = !v.analysis.post;
                }
            }
            EqOp::AnalyzerRange(r) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.range = r;
                    v.columns.clear();
                }
            }
            EqOp::Resolution(r) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.resolution = r;
                }
            }
            EqOp::Speed(s) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.speed = s;
                }
            }
            EqOp::Tilt(t) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.tilt = t as f32 * 1.5;
                }
            }
            EqOp::Freeze => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.freeze = !v.analysis.freeze;
                }
            }
            EqOp::Range(r) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.range = r;
                }
            }
            EqOp::Bypass => self.edit(cx, |ws| {
                if let Some(eq) = ws.strip_dsp_mut(ch).0.as_mut() {
                    eq.bypass = !eq.bypass;
                }
            }),
            EqOp::Popup(p) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.popup = if v.popup == Some(p) { None } else { Some(p) };
                }
            }
            EqOp::Piano => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.piano = !v.piano;
                }
            }
            EqOp::Ab => {
                let Some((_, now)) = self.shown_eq() else {
                    return;
                };
                let other = self.eq_view.as_ref().and_then(|v| v.other).unwrap_or(now);
                self.edit(cx, |ws| *ws.strip_dsp_mut(ch).0 = Some(other));
                if let Some(v) = self.eq_view.as_mut() {
                    v.other = Some(now);
                    v.on_b = !v.on_b;
                    v.picked.clear();
                }
            }
            EqOp::CopyAb => {
                let now = self.shown_eq().map(|(_, e)| e);
                if let Some(v) = self.eq_view.as_mut() {
                    v.other = now;
                    self.status = Some(format!(
                        "{} copied to {}",
                        if v.on_b { "B" } else { "A" },
                        if v.on_b { "A" } else { "B" }
                    ));
                }
            }
            EqOp::FullScreen => {
                let (vw, vh) = (self.viewport_w, self.viewport_h);
                if let Some(v) = self.eq_view.as_mut() {
                    match v.restore.take() {
                        Some((at, size)) => (v.at, v.size) = (at, size),
                        None => {
                            v.restore = Some((v.at, v.size));
                            v.at = (8.0, CONTROL_BAR_H + 4.0);
                            v.size = (vw - 16.0, vh - CONTROL_BAR_H - STATUS_H - 8.0);
                        }
                    }
                }
            }
            EqOp::Factory(i) => {
                if let Some((name, eq)) = factory().into_iter().nth(i) {
                    self.edit(cx, |ws| *ws.strip_dsp_mut(ch).0 = Some(eq));
                    self.status = Some(format!("Loaded {name}"));
                }
            }
            EqOp::Preset(i) => {
                if let Some((name, path)) = saved_presets().into_iter().nth(i) {
                    let read = std::fs::read(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|b| serde_json::from_slice::<Eq>(&b).map_err(|e| e.to_string()));
                    match read {
                        Ok(eq) => {
                            self.edit(cx, |ws| *ws.strip_dsp_mut(ch).0 = Some(eq));
                            self.status = Some(format!("Loaded {name}"));
                        }
                        Err(e) => self.status = Some(format!("{name}: {e}")),
                    }
                }
            }
            EqOp::GainQ => {
                let all = self.picked_bands().iter().all(|b| b.gain_q);
                self.edit_picked(cx, |b| b.gain_q = !all)
            }
            EqOp::Character(c) => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.character = c;
                }
            }),
            EqOp::AutoGain => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.auto_gain = !eq.auto_gain;
                }
            }),
            EqOp::Invert => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.invert = !eq.invert;
                }
            }),
            EqOp::PanMs(ms) => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.pan_ms = ms;
                }
            }),
            EqOp::DynExpand => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.dyn_open = !v.dyn_open;
                }
            }
            EqOp::DynBypass => {
                let all = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.bypass));
                self.edit_picked(cx, |b| {
                    if let Some(d) = b.dynamic.as_mut() {
                        d.bypass = !all;
                    }
                })
            }
            EqOp::DynClear => self.edit_picked(cx, |b| b.dynamic = None),
            EqOp::Spectral => {
                let all = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.spectral));
                self.edit_picked(cx, |b| {
                    if let Some(d) = b.dynamic.as_mut() {
                        d.spectral = !all;
                    }
                })
            }
            EqOp::SpectralTilt => {
                let all = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.tilt));
                self.edit_picked(cx, |b| {
                    if let Some(d) = b.dynamic.as_mut() {
                        d.tilt = !all;
                    }
                })
            }
            EqOp::Processing(p) => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.processing = p;
                }
            }),
            EqOp::PhaseResolution(r) => self.edit(cx, |ws| {
                if let Some(eq) = ws.eq_mut() {
                    eq.resolution = r.min(3);
                }
            }),
            EqOp::External => {
                let all = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.external));
                self.edit_picked(cx, |b| {
                    if let Some(d) = b.dynamic.as_mut() {
                        d.external = !all;
                    }
                })
            }
            EqOp::Trigger => {
                let free = self
                    .picked_bands()
                    .iter()
                    .all(|b| b.dynamic.is_some_and(|d| d.trigger.is_some()));
                self.edit_picked(cx, |b| {
                    let freq = b.freq;
                    if let Some(d) = b.dynamic.as_mut() {
                        d.trigger = match free {
                            true => None,
                            false => Some([(freq / 2.0).max(10.0), (freq * 2.0).min(30_000.0)]),
                        };
                    }
                })
            }
            EqOp::AutoThreshold => self.edit_picked(cx, |b| {
                if let Some(d) = b.dynamic.as_mut() {
                    d.threshold_db = None;
                }
            }),
            EqOp::Sidechain(source) => {
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.eq_mut() {
                        eq.sidechain = source;
                    }
                });
                // Picked from a band's dynamics: that band listens to it.
                if source.is_some() {
                    self.edit_picked(cx, |b| {
                        if let Some(d) = b.dynamic.as_mut() {
                            d.external = true;
                        }
                    });
                }
            }
            EqOp::Ext(source) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.ext = source;
                    v.ext_spec.clear();
                }
            }
            EqOp::Collisions => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.collisions = !v.analysis.collisions;
                }
            }
            EqOp::Grab => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analysis.grab = !v.analysis.grab;
                }
            }
            EqOp::List => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.list = !v.list;
                }
                self.open_list(cx);
            }
            EqOp::Sketch => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.sketching = !v.sketching;
                    v.stroke.clear();
                    v.picked.clear();
                }
            }
            EqOp::Match(..)
            | EqOp::MatchTrack(_)
            | EqOp::MatchFile
            | EqOp::MatchRun
            | EqOp::MatchBands(_)
            | EqOp::MatchFinish
            | EqOp::MatchCancel => self.match_op(op, cx),
            EqOp::SavePreset => {
                let Some((_, eq)) = self.shown_eq() else {
                    return;
                };
                let name = format!("{} EQ", self.channel_look(ch).0);
                let dir = presets_dir();
                let mut path = dir.join(format!("{name}.json"));
                let mut n = 2;
                while path.exists() {
                    path = dir.join(format!("{name} {n}.json"));
                    n += 1;
                }
                let saved = std::fs::create_dir_all(&dir).and_then(|_| {
                    std::fs::write(&path, serde_json::to_vec_pretty(&eq).unwrap_or_default())
                });
                self.status = Some(match saved {
                    Ok(()) => format!(
                        "Saved the preset {}",
                        path.file_stem().unwrap_or_default().to_string_lossy()
                    ),
                    Err(e) => format!("Couldn't save the preset: {e}"),
                });
            }
        }
        cx.notify();
    }

    fn picked_bands(&self) -> Vec<EqBand> {
        let Some((_, eq)) = self.shown_eq() else {
            return Vec::new();
        };
        let picked = self
            .eq_view
            .as_ref()
            .map(|v| v.picked.clone())
            .unwrap_or_default();
        picked
            .iter()
            .filter_map(|&id| eq.bands.find(id).map(|i| eq.bands[i]))
            .collect()
    }

    /// Delete, with the window last clicked in and bands picked: they go.
    pub fn delete_eq_band(&mut self, cx: &mut Context<Self>) -> bool {
        if !self
            .eq_view
            .as_ref()
            .is_some_and(|v| v.focused && !v.picked.is_empty())
        {
            return false;
        }
        self.eq_op(EqOp::Delete, cx);
        true
    }

    /// ⌘C and ⌘V with the window last clicked in: its bands.
    pub fn eq_key(&mut self, op: EqOp, cx: &mut Context<Self>) -> bool {
        if !self.eq_view.as_ref().is_some_and(|v| v.focused) {
            return false;
        }
        self.eq_op(op, cx);
        true
    }

    /// A band's menu: its shape, slope and part of the sound, and the rest.
    pub(crate) fn band_menu(&mut self, id: u16, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        if let Some(v) = self.eq_view.as_mut()
            && !v.picked.contains(&id)
        {
            v.picked = vec![id];
        }
        let tick = |on: bool| if on { "✓ " } else { "" };
        let mut items = vec![("Shape".to_string(), MenuAction::Nothing)];
        items.extend(Shape::ALL.iter().map(|&s| {
            (
                format!("{}{}", tick(s == b.shape), s.name()),
                MenuAction::Eq(EqOp::Shape(s)),
            )
        }));
        if !b.shape.slopes().is_empty() {
            items.push((String::new(), MenuAction::Nothing));
            items.push(("Slope".into(), MenuAction::Nothing));
            items.extend(b.shape.slopes().iter().map(|&s| {
                (
                    format!("{}{}", tick(s == b.slope), slope_text(s)),
                    MenuAction::Eq(EqOp::Slope(s)),
                )
            }));
        }
        items.push((String::new(), MenuAction::Nothing));
        items.push(("Works on".into(), MenuAction::Nothing));
        items.extend(Placement::ALL.iter().map(|&p| {
            (
                format!("{}{}", tick(p == b.placement), p.name()),
                MenuAction::Eq(EqOp::Placement(p)),
            )
        }));
        items.push(("Split".into(), MenuAction::Eq(EqOp::Split)));
        items.push((String::new(), MenuAction::Nothing));
        if b.shape.has_gain() {
            items.push((
                if b.dynamic.is_some() {
                    "Make Still"
                } else {
                    "Make Dynamic"
                }
                .into(),
                MenuAction::Eq(EqOp::Dynamic),
            ));
        }
        items.push((
            if b.off { "Switch On" } else { "Switch Off" }.into(),
            MenuAction::Eq(EqOp::Off),
        ));
        items.push(("Copy".into(), MenuAction::Eq(EqOp::Copy)));
        items.push(("Delete".into(), MenuAction::Eq(EqOp::Delete)));
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// The display's own menu: paste, copy, pick or clear its bands.
    fn plot_menu(&mut self, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let n = self.eq_clipboard.len();
        let mut items = vec![];
        if n > 0 {
            items.push((
                format!("Paste {n} band{}", if n == 1 { "" } else { "s" }),
                MenuAction::Eq(EqOp::Paste),
            ));
        }
        items.extend([
            ("Copy All Bands".to_string(), MenuAction::Eq(EqOp::CopyAll)),
            ("Select All".to_string(), MenuAction::Eq(EqOp::SelectAll)),
            (String::new(), MenuAction::Nothing),
            ("Clear All Bands".to_string(), MenuAction::Eq(EqOp::Clear)),
        ]);
        self.eq_menu(at, items, cx);
    }

    fn eq_menu(
        &mut self,
        at: gpui::Point<Pixels>,
        items: Vec<(String, MenuAction)>,
        cx: &mut Context<Self>,
    ) {
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// The window moved by its title, or resized by its corner.
    pub fn drag_eq_window(
        &mut self,
        from: gpui::Point<f32>,
        start: (f32, f32),
        resize: bool,
        p: gpui::Point<f32>,
    ) {
        let (vw, vh) = (self.viewport_w, self.viewport_h);
        let Some(v) = self.eq_view.as_mut() else {
            return;
        };
        let (dx, dy) = (p.x - from.x, p.y - from.y);
        v.restore = None;
        if resize {
            v.size = (
                (start.0 + dx).clamp(MIN_W, vw.max(MIN_W)),
                (start.1 + dy).clamp(MIN_H, vh.max(MIN_H)),
            );
        } else {
            v.at = (
                (start.0 + dx).clamp(-v.size.0 + 80.0, vw - 80.0),
                (start.1 + dy).clamp(0.0, vh - HEAD_H),
            );
        }
    }

    /// The EQ window, if it's open.
    pub fn eq_window(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let Some((ch, eq)) = self.shown_eq() else {
            // Its channel or EQ is gone.
            if self.eq_view.is_some() {
                self.eq_view = None;
                self.player.watch_eq(None);
                self.player.solo_band(None);
            }
            return None;
        };
        let eq = self.eq_as_heard(ch, eq);
        let view = self.eq_view.as_ref()?;
        let (at, size) = (view.at, view.size);
        let head = self.eq_head(ch, &eq, cx);
        let display = self.eq_display_area(&eq, cx);
        let foot = self.eq_foot(&eq, cx);
        let popup = self.eq_popup(&eq, cx);
        let frame = div()
            .id("eq-window")
            .occlude()
            .relative()
            .w(px(size.0))
            .h(px(size.1))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .overflow_hidden()
            .bg(rgb(0x101115))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(head)
            .child(display)
            .child(foot)
            .children(popup)
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                if let Some(v) = ws.eq_view.as_mut()
                    && v.focused
                    && ws.drag.is_none()
                {
                    v.focused = false;
                    cx.notify();
                }
            }));
        Some(
            deferred(
                anchored()
                    .position(point(px(at.0), px(at.1)))
                    .snap_to_window()
                    .child(frame),
            )
            .with_priority(1),
        )
    }
}

/// Where a band's dot sits, in dB: a gain shape's at its gain as heard, a cut's on its curve at
/// its frequency, others' at 0.
fn dot_db(b: &EqBand, scale: f32) -> f32 {
    match b.shape {
        s if s.has_gain() => b.gain(scale),
        Shape::LowCut | Shape::HighCut => {
            let sections = band_sections(b, scale, 96_000.0);
            response_db(&sections, b.freq as f64, 96_000.0) as f32
        }
        _ => 0.0,
    }
}

#[cfg(test)]
impl EqView {
    /// A mix's spectrum, as if it were playing through `eq`: for pictures.
    pub fn pretend(&mut self, eq: &Eq, rate: f64) {
        let geo = self.geo(rate);
        let n = (geo.w / 3.0) as usize;
        self.columns = (0..n)
            .map(|i| geo.freq(i as f32 / (n - 1) as f32 * geo.w))
            .collect();
        let mut seed = 7u32;
        let tilt = self.analysis.tilt;
        self.pre = self
            .columns
            .iter()
            .map(|&f| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let jitter = (seed >> 8) as f32 / (1 << 24) as f32 * 5.0 - 2.5;
                let octave = (f / 1_000.0).log2() as f32;
                let kick = 9.0 * (-((f / 60.0).log2().powi(2) as f32) / 0.6).exp();
                let body = 4.0 * (-((f / 250.0).log2().powi(2) as f32) / 2.0).exp();
                let top = if f > 14_000.0 {
                    -((f / 14_000.0).log2() as f32) * 30.0
                } else {
                    0.0
                };
                -40.0 - 4.5 * octave + kick + body + top + jitter + tilt * octave
            })
            .collect();
        self.post = self
            .columns
            .iter()
            .zip(&self.pre)
            .map(|(&f, &p)| p + pina_engine::eq::eq_response(eq, f, rate) as f32)
            .collect();
        self.meter = [-9.5, -10.2];
        self.peak = -6.8;
        // Another track, for comparing: a bass's sound, loud low down.
        self.ext_spec = self
            .columns
            .iter()
            .map(|&f| {
                let octave = (f / 1_000.0).log2() as f32;
                let low = 14.0 * (-((f / 110.0).log2().powi(2) as f32) / 1.2).exp();
                -44.0 - 9.0 * octave.max(0.0) + low + tilt * octave
            })
            .collect();
    }

    /// A stage of a tool, for pictures: the instance list, EQ Match fitted, a sketch drawn,
    /// the spectrum grabbed.
    pub fn pretend_tool(&mut self, tool: &str, rate: f64) {
        let g = self.geo(rate);
        match tool {
            "list" => self.list = true,
            "match" => {
                let mut m = Match::new(tools::MatchSource::None);
                let freqs = m.freqs().to_vec();
                let input: Vec<f32> = freqs.iter().map(|_| -40.0).collect();
                let reference: Vec<f32> = freqs
                    .iter()
                    .map(|&f| {
                        let bump = |c: f64, w: f64, g: f32| {
                            g * (-((f / c).log2().powi(2)) / w).exp() as f32
                        };
                        -40.0
                            + bump(3_000.0, 0.5, 5.0)
                            + bump(250.0, 0.8, -4.0)
                            + bump(9_000.0, 1.5, 3.0)
                    })
                    .collect();
                m.pretend(&input, &reference);
                m.run(rate);
                self.matching = Some(m);
            }
            "sketch" => {
                self.sketching = true;
                self.stroke = (0..80)
                    .map(|i| {
                        let x = i as f32 / 79.0 * g.w * 0.7;
                        let o = x / g.w;
                        let db = 6.0 * (-((o - 0.12).powi(2)) / 0.004).exp()
                            - 5.0 * (-((o - 0.45).powi(2)) / 0.006).exp();
                        (x, g.y(db))
                    })
                    .collect();
            }
            "grab" => {
                self.grabbing = Some(self.post.clone());
                self.pointer = Some((g.w * 0.3, g.h * 0.5));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knobs_typed_values_and_the_display_map_both_ways() {
        for k in [
            EqKnob::Freq,
            EqKnob::Q,
            EqKnob::Gain,
            EqKnob::Threshold,
            EqKnob::Scale,
            EqKnob::Output,
        ] {
            for v in [0.2f32, 0.5, 0.8] {
                let back = k.pos(k.value(v));
                assert!((back - v).abs() < 0.01, "{k:?} {v} {back}");
            }
        }
        let geo = Geo {
            w: 800.0,
            h: 400.0,
            lo: LOW,
            hi: HIGH,
            range: 12.0,
        };
        assert!((geo.freq(geo.x(1_000.0)) - 1_000.0).abs() < 0.01);
        assert!((geo.db(geo.y(6.0)) - 6.0).abs() < 1e-4);
        assert_eq!(hz_text(850.0), "850 Hz");
        assert_eq!(hz_text(1_200.0), "1.20 kHz");
        assert_eq!(note_text(440.0), "A4");
        // Typed: frequencies as Hz, kHz or notes (and cents); gains as dB or a ratio.
        let f = |t: &str| EqKnob::Freq.parse(t).unwrap();
        assert_eq!(f("100"), 100.0);
        assert_eq!(f("2k"), 2_000.0);
        assert_eq!(f("1.5 kHz"), 1_500.0);
        assert!((f("A4") - 440.0).abs() < 0.01);
        assert!((f("C#2+13") - 69.82).abs() < 0.1, "{}", f("C#2+13"));
        assert!((EqKnob::Gain.parse("2x").unwrap() - 6.02).abs() < 0.01);
        assert_eq!(EqKnob::Gain.parse("-3 dB"), Some(-3.0));
        assert_eq!(EqKnob::Scale.parse("50%"), Some(0.5));
        assert_eq!(EqKnob::Freq.parse("loud"), None);
        // A click makes a cut at either end, a notch low down, a bell between; pulled, shelves.
        assert_eq!(band_at(&geo, 10.0, 200.0, false).shape, Shape::LowCut);
        assert_eq!(band_at(&geo, 10.0, 200.0, true).shape, Shape::LowShelf);
        assert_eq!(band_at(&geo, 795.0, 200.0, false).shape, Shape::HighCut);
        assert_eq!(band_at(&geo, 400.0, 395.0, false).shape, Shape::Notch);
        let bell = band_at(&geo, 400.0, 100.0, false);
        assert!(bell.shape == Shape::Bell && bell.gain_db > 5.0, "{bell:?}");
        // The quick knobs find their bands, and add them.
        let mut eq = Eq::classic();
        assert_eq!(Quick::Presence.find(&eq), Some(2));
        assert_eq!(Quick::Air.find(&eq), Some(3));
        let mut empty = Eq::default();
        assert_eq!(Quick::Air.band(&mut empty), Some(0));
        set_low_cut(&mut eq, 90.0);
        assert_eq!(low_cut(&eq), 90.0);
        set_low_cut(&mut eq, 0.0);
        assert_eq!((low_cut(&eq), eq.bands.len()), (0.0, 4));
        // Pina Studio's presets are all valid EQs.
        for (name, eq) in factory() {
            assert!(!eq.bands.is_empty(), "{name}");
        }
    }
}
