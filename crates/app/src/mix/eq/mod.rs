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

mod knobs;
mod mouse;
mod ops;
mod presets;
mod text;
mod window;

pub use knobs::*;
pub use ops::*;
pub use presets::*;
pub use text::*;

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
    dsp::eq::{TAP_LEN, auto_times, band_sections, response_db, spectrum_db},
    song::project::{
        Bands, Character, Dynamic, Eq, EqBand, EqField, MAX_BANDS, Param, Placement, Processing,
        Shape,
    },
};

use crate::{
    mix::strip::Channel,
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
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

// For the shots test, which draws on macOS only.
#[cfg(all(test, target_os = "macos"))]
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
            .map(|(&f, &p)| p + pina_engine::dsp::eq::eq_response(eq, f, rate) as f32)
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
mod tests;
