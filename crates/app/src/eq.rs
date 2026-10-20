//! The Channel EQ's window, as FabFilter's Pro-Q: one big display of the channel's sound, a live
//! analyzer of what goes in and comes out, and the EQ's curve, each band a dot to drag.
//! Double-click anywhere to add a band there (a cut at either end, a bell between), drag a dot to
//! move its band (⇧ fine), ⌘-drag or scroll on it for its width, ⌥-click it to switch it off,
//! double-click it to take it out, right-click it for everything else. The band picked shows its
//! controls along the bottom of the display: its shape and slope, frequency, gain and width, the
//! part of the sound it works on (stereo, left, right, mid, side), whether it moves with the sound
//! (dynamic: a range and threshold), and a button to hear it alone. Along the bottom of the window:
//! the analyzer, the display's range, the gain scale and the output level.
//!
//! The window floats over the song, the mixer or the tracks, and stays open while you work
//! elsewhere; it shows one channel's EQ at a time (opening another's moves it there).

use std::{cell::Cell, rc::Rc};

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, PathBuilder,
    Pixels, ScrollWheelEvent, anchored, canvas, deferred, div, fill, linear_color_stop,
    linear_gradient, point, prelude::*, px, rgb, size,
};
use tempo_engine::{
    eq::{TAP_LEN, band_sections, response_db, spectrum_db},
    project::{Dynamic, Eq, EqBand, MAX_BANDS, Placement, Shape},
};

use crate::{
    menu::{Menu, MenuAction},
    strip::Channel,
    theme::*,
    workspace::{Drag, Workspace, pt},
};

/// The bands' colors, round and round by their place.
pub const BAND_COLORS: [u32; 8] = [CORAL, AMBER, MINT, SKY, VIOLET, PINK, TEAL, 0xa3e635];

/// The display's lowest frequency, and its highest (or Nyquist, if that's lower).
const LOW: f64 = 10.0;
const HIGH: f64 = 30_000.0;
/// Room above and below the EQ's scale.
const PAD: f32 = 16.0;
const TITLE_H: f32 = 38.0;
const BAR_H: f32 = 42.0;
const MIN_W: f32 = 640.0;
const MIN_H: f32 = 420.0;
const NODE: f32 = 16.0;
/// The ranges the display can show, ± dB.
const RANGES: [u8; 4] = [3, 6, 12, 30];
/// The analyzer tilts up this many dB an octave about 1 kHz, as Pro-Q's does, so a mix reads
/// about level.
const TILT: f32 = 4.5;
/// The analyzer's scale: dB at the bottom and top.
const FLOOR: f32 = -96.0;
const CEIL: f32 = 6.0;

/// What the analyzer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Analyzer {
    /// What comes out, filled, and what goes in, a line.
    Both,
    Post,
    Pre,
    Off,
}

impl Analyzer {
    const ALL: [Analyzer; 4] = [Analyzer::Both, Analyzer::Post, Analyzer::Pre, Analyzer::Off];

    fn name(self) -> &'static str {
        match self {
            Analyzer::Both => "Pre + Post",
            Analyzer::Post => "Post",
            Analyzer::Pre => "Pre",
            Analyzer::Off => "Off",
        }
    }
}

/// The EQ window: whose EQ, where it is, what's picked and what the analyzer has heard.
pub struct EqView {
    pub ch: Channel,
    /// Its top left corner and size, in the window.
    pub at: (f32, f32),
    pub size: (f32, f32),
    /// The band picked, and the one under the pointer, by id.
    pub picked: Option<u16>,
    pub hover: Option<u16>,
    /// Where the pointer is over the display, from its top left.
    pub pointer: Option<(f32, f32)>,
    /// The display's range, ± dB.
    pub range: u8,
    pub analyzer: Analyzer,
    /// A band heard alone, by id.
    pub solo: Option<u16>,
    /// Clicked in last: Delete takes out its band rather than a region.
    pub focused: bool,
    /// The analyzer's curves, smoothed: dB at each of `columns`.
    pre: Vec<f32>,
    post: Vec<f32>,
    columns: Vec<f64>,
    written: usize,
    /// The channel has sound now (playing, or played live with the song stopped).
    hearing: bool,
    buf: Box<([f32; TAP_LEN], [f32; TAP_LEN])>,
    /// The display's place in the window, as last drawn.
    pub display: Rc<Cell<Bounds<Pixels>>>,
}

impl EqView {
    fn new(ch: Channel, at: (f32, f32), size: (f32, f32)) -> Self {
        EqView {
            ch,
            at,
            size,
            picked: None,
            hover: None,
            pointer: None,
            range: 12,
            analyzer: Analyzer::Both,
            solo: None,
            focused: true,
            pre: Vec::new(),
            post: Vec::new(),
            columns: Vec::new(),
            written: 0,
            hearing: false,
            buf: Box::new(([0.0; TAP_LEN], [0.0; TAP_LEN])),
            display: Rc::new(Cell::new(Bounds::default())),
        }
    }

    /// The display's size: the window less its title and bar.
    fn display_size(&self) -> (f32, f32) {
        (self.size.0 - 2.0, self.size.1 - TITLE_H - BAR_H - 2.0)
    }

    /// Whether the analyzer still shows something: it falls away when the sound stops.
    pub fn moving(&self) -> bool {
        self.analyzer != Analyzer::Off
            && (self.hearing || self.pre.iter().chain(&self.post).any(|&d| d > FLOOR + 1.0))
    }
}

/// What a click in the EQ window or one of its menus does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EqOp {
    Shape(u16, Shape),
    Slope(u16, u8),
    Placement(u16, Placement),
    /// Makes a band move with the sound, or still.
    Dynamic(u16),
    /// Switches a band off, or on.
    Off(u16),
    Delete(u16),
    /// Hears a band alone, or not.
    Solo(u16),
    Analyzer(Analyzer),
    Range(u8),
    Bypass,
}

/// A knob in the EQ window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqKnob {
    Freq,
    Gain,
    Q,
    Range,
    Threshold,
    Scale,
    Output,
}

impl EqKnob {
    /// Where a value sits on the knob, 0 to 1.
    fn pos(self, v: f32) -> f32 {
        match self {
            EqKnob::Freq => ((v / 10.0).ln() / 3000f32.ln()).clamp(0.0, 1.0),
            EqKnob::Q => ((v / 0.025).ln() / 1600f32.ln()).clamp(0.0, 1.0),
            EqKnob::Gain | EqKnob::Range => (v + 30.0) / 60.0,
            EqKnob::Threshold => (v + 80.0) / 80.0,
            EqKnob::Scale => v / 2.0,
            EqKnob::Output => (v + 24.0) / 48.0,
        }
    }

    /// The value at a place on the knob.
    fn value(self, pos: f32) -> f32 {
        let p = pos.clamp(0.0, 1.0);
        match self {
            EqKnob::Freq => 10.0 * 3000f32.powf(p),
            EqKnob::Q => 0.025 * 1600f32.powf(p),
            EqKnob::Gain | EqKnob::Range => ((p * 60.0 - 30.0) * 10.0).round() / 10.0,
            EqKnob::Threshold => (p * 80.0 - 80.0).round(),
            EqKnob::Scale => (p * 200.0).round() / 100.0,
            EqKnob::Output => ((p * 48.0 - 24.0) * 10.0).round() / 10.0,
        }
    }

    /// Whether its arc lights from the middle.
    fn bipolar(self) -> bool {
        matches!(
            self,
            EqKnob::Gain | EqKnob::Range | EqKnob::Output | EqKnob::Scale
        )
    }

    fn text(self, v: f32) -> String {
        match self {
            EqKnob::Freq => hz_text(v),
            EqKnob::Q => q_text(v),
            EqKnob::Gain | EqKnob::Range | EqKnob::Output => format!("{} dB", db_text(v)),
            EqKnob::Threshold => format!("{} dB", db_text(v).trim_end_matches(".0")),
            EqKnob::Scale => format!("{:.0}%", v * 100.0),
        }
    }

    fn label(self) -> &'static str {
        match self {
            EqKnob::Freq => "FREQ",
            EqKnob::Gain => "GAIN",
            EqKnob::Q => "Q",
            EqKnob::Range => "RANGE",
            EqKnob::Threshold => "THRESH",
            EqKnob::Scale => "SCALE",
            EqKnob::Output => "OUTPUT",
        }
    }
}

/// "850 Hz", "1.20 kHz", "12.5 kHz".
pub fn hz_text(hz: f32) -> String {
    match hz {
        h if h < 1_000.0 => format!("{h:.0} Hz"),
        h if h < 10_000.0 => format!("{:.2} kHz", h / 1_000.0),
        h => format!("{:.1} kHz", h / 1_000.0),
    }
}

fn q_text(q: f32) -> String {
    if q < 10.0 {
        format!("{q:.2}")
    } else {
        format!("{q:.1}")
    }
}

/// The note a frequency is nearest: "A4".
fn note_text(hz: f32) -> String {
    let midi = (69.0 + 12.0 * (hz / 440.0).log2())
        .round()
        .clamp(0.0, 127.0) as u8;
    tempo_engine::samples::note_name(midi)
}

/// A band's color, by its place.
pub fn band_color(i: usize) -> u32 {
    BAND_COLORS[i % BAND_COLORS.len()]
}

fn top_hz(rate: f64) -> f64 {
    HIGH.min(rate / 2.0)
}

/// Across the display: a frequency's x, and the frequency at an x.
fn x_of(freq: f64, w: f32, rate: f64) -> f32 {
    ((freq.max(LOW) / LOW).ln() / (top_hz(rate) / LOW).ln()) as f32 * w
}

fn freq_at(x: f32, w: f32, rate: f64) -> f64 {
    LOW * (top_hz(rate) / LOW).powf((x / w).clamp(0.0, 1.0) as f64)
}

/// Down the display: a gain's y, and the gain at a y.
fn y_of(db: f32, h: f32, range: f32) -> f32 {
    h / 2.0 - db / range * (h / 2.0 - PAD)
}

fn gain_at(y: f32, h: f32, range: f32) -> f32 {
    (h / 2.0 - y) / (h / 2.0 - PAD) * range
}

/// The analyzer's y for a level.
fn analyzer_y(db: f32, h: f32) -> f32 {
    (CEIL - db) / (CEIL - FLOOR) * h
}

/// A band the way it's to be drawn: its sections, where its dot is.
struct Drawn {
    id: u16,
    color: u32,
    band: EqBand,
    /// Its curve, dB at each column.
    curve: Vec<f32>,
    dot: (f32, f32),
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

/// The shape a band made at a frequency takes: a cut at either end, a bell between.
fn shape_at(freq: f64) -> Shape {
    match freq {
        f if f < 35.0 => Shape::LowCut,
        f if f > 15_000.0 => Shape::HighCut,
        _ => Shape::Bell,
    }
}

impl Workspace {
    /// A channel's EQ in its window (one added, if it had none): in the middle of the window, or
    /// where the window already is.
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
                (v.picked, v.hover, v.solo) = (None, None, None);
                v.focused = true;
            }
            None => {
                let w = (self.viewport_w - 80.0).clamp(MIN_W, 980.0);
                let h = (self.viewport_h - 140.0).clamp(MIN_H, 560.0);
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
        self.player.solo_band(None);
        cx.notify();
    }

    /// The EQ the window shows.
    fn shown_eq(&self) -> Option<(Channel, Eq)> {
        let ch = self.eq_view.as_ref()?.ch;
        Some((ch, self.strip_dsp(ch).0?))
    }

    fn eq_band(&self, id: u16) -> Option<(usize, EqBand)> {
        let (_, eq) = self.shown_eq()?;
        let i = eq.bands.find(id)?;
        Some((i, eq.bands[i]))
    }

    /// Changes the shown EQ's band `id`, as one edit.
    fn edit_band(&mut self, id: u16, cx: &mut Context<Self>, f: impl FnOnce(&mut EqBand)) {
        let Some(ch) = self.eq_view.as_ref().map(|v| v.ch) else {
            return;
        };
        self.edit(cx, |ws| {
            if let Some(eq) = ws.strip_dsp_mut(ch).0.as_mut()
                && let Some(i) = eq.bands.find(id)
            {
                f(&mut eq.bands[i]);
            }
        });
    }

    /// Each frame the window's open: tells the engine which EQ to tap and which band to solo,
    /// and moves the analyzer on with what it's heard.
    pub fn eq_frame(&mut self) {
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
        self.player.solo_band(view.solo);
        let rate = p.sample_rate as f64;
        let w = view.display_size().0.max(100.0);
        let n = (w / 3.0) as usize;
        if view.columns.len() != n {
            view.columns = (0..n)
                .map(|i| freq_at(i as f32 / (n - 1) as f32 * w, w, rate))
                .collect();
            view.pre = vec![FLOOR; n];
            view.post = vec![FLOOR; n];
        }
        if view.analyzer == Analyzer::Off {
            return;
        }
        let tap = self.player.eq_tap();
        let written = tap.read(&mut view.buf.0, &mut view.buf.1);
        let heard = written != view.written;
        view.written = written;
        // The last block or so going in: anything there, and the analyzer keeps moving.
        view.hearing = heard && view.buf.0[TAP_LEN - 1024..].iter().any(|x| x.abs() > 1e-4);
        let tilt = |f: f64| TILT * (f / 1_000.0).log2() as f32;
        let fresh = |s: &[f32]| -> Vec<f32> {
            match heard {
                true => spectrum_db(s, rate, &view.columns),
                false => vec![FLOOR - 20.0; n],
            }
        };
        let (pre, post) = (fresh(&view.buf.0[..]), fresh(&view.buf.1[..]));
        for (smooth, now) in [(&mut view.pre, pre), (&mut view.post, post)] {
            for (i, (s, v)) in smooth.iter_mut().zip(now).enumerate() {
                let v = (v + tilt(view.columns[i])).max(FLOOR);
                // Up quickly, down slowly, as an analyzer's ballistics do.
                *s = if v > *s {
                    *s + (v - *s) * 0.6
                } else {
                    (*s - 0.9).max(v)
                };
            }
        }
    }

    /// A band added where the display was double-clicked: picked, and dragging.
    fn add_band_at(&mut self, x: f32, y: f32, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(view) = self.eq_view.as_ref() else {
            return;
        };
        let (ch, range, (w, h)) = (view.ch, view.range as f32, view.display_size());
        let rate = self.session.project.sample_rate as f64;
        let freq = freq_at(x, w, rate);
        let shape = shape_at(freq);
        let gain = match shape.has_gain() {
            true => (gain_at(y, h, range) * 2.0).round() / 2.0,
            false => 0.0,
        };
        let mut id = None;
        self.edit(cx, |ws| {
            let eq = ws.strip_dsp_mut(ch).0.get_or_insert_default();
            if let Some(i) = eq.bands.push(EqBand::new(
                shape,
                freq.round() as f32,
                gain.clamp(-30.0, 30.0),
            )) {
                id = Some(eq.bands[i].id);
            }
        });
        let Some(id) = id else {
            self.status = Some(format!("An EQ has at most {MAX_BANDS} bands."));
            cx.notify();
            return;
        };
        // Dragging on from where it was made: the same edit, so one undo takes it out.
        self.grab_band(id, e, false, cx);
    }

    /// A band's dot pressed: picked, and dragging (its width, with ⌘).
    fn press_band(&mut self, id: u16, e: &MouseDownEvent, cx: &mut Context<Self>) {
        self.grab_band(id, e, true, cx);
    }

    fn grab_band(&mut self, id: u16, e: &MouseDownEvent, edit: bool, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        if let Some(v) = self.eq_view.as_mut() {
            v.picked = Some(id);
            v.focused = true;
        }
        let drag = Drag::EqNode {
            id,
            from: pt(e.position),
            start: (b.freq, b.gain_db, b.q),
            q: e.modifiers.platform,
        };
        match edit {
            true => self.begin_drag(drag, cx),
            false => {
                self.drag = Some(drag);
                cx.notify();
            }
        }
    }

    /// A dragged dot: across moves its frequency, up and down its gain (a cut's, notch's or band
    /// pass's width, as with ⌘ any band's).
    pub fn drag_eq_node(
        &mut self,
        id: u16,
        from: gpui::Point<f32>,
        start: (f32, f32, f32),
        q_only: bool,
        p: gpui::Point<f32>,
    ) {
        let Some(view) = self.eq_view.as_ref() else {
            return;
        };
        let (ch, range, (w, h)) = (view.ch, view.range as f32, view.display_size());
        let rate = self.session.project.sample_rate as f64;
        let fine = self.fine;
        let by = if fine { 0.1 } else { 1.0 };
        let (dx, dy) = ((p.x - from.x) * by, (p.y - from.y) * by);
        let Some(eq) = self.strip_dsp_mut(ch).0.as_mut() else {
            return;
        };
        let scale = eq.scale;
        let Some(i) = eq.bands.find(id) else {
            return;
        };
        let b = &mut eq.bands[i];
        let octaves = (top_hz(rate) / LOW).log2() as f32;
        b.freq = (start.0 * 2f32.powf(dx / w * octaves)).clamp(10.0, 30_000.0);
        if q_only || !b.shape.has_gain() {
            b.q = (start.2 * 2f32.powf(-dy / 60.0)).clamp(0.025, 40.0);
        } else if scale > 0.0 {
            let per_px = range / (h / 2.0 - PAD);
            let g = start.1 - dy * per_px / scale;
            b.gain_db = ((g * 10.0).round() / 10.0).clamp(-30.0, 30.0);
        }
        if !fine {
            b.freq = b.freq.round();
        }
    }

    /// The wheel over a band: its width.
    fn scroll_band(&mut self, id: u16, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let dy = f32::from(e.delta.pixel_delta(px(16.)).y);
        if dy == 0.0 {
            return;
        }
        let by = 2f32.powf(dy / if e.modifiers.shift { 400.0 } else { 120.0 });
        self.edit_band(id, cx, |b| b.q = (b.q * by).clamp(0.025, 40.0));
    }

    /// A knob dragged: up turns it up.
    pub fn drag_eq_knob(&mut self, id: u16, knob: EqKnob, from: f32, start: f32, y: f32) {
        let fine = if self.fine { 0.1 } else { 1.0 };
        let pos = knob.pos(start) + (from - y) / 200.0 * fine;
        self.set_eq_knob(id, knob, knob.value(pos));
    }

    fn set_eq_knob(&mut self, id: u16, knob: EqKnob, v: f32) {
        let Some(ch) = self.eq_view.as_ref().map(|v| v.ch) else {
            return;
        };
        let Some(eq) = self.strip_dsp_mut(ch).0.as_mut() else {
            return;
        };
        match knob {
            EqKnob::Scale => eq.scale = v,
            EqKnob::Output => eq.output_db = v,
            _ => {
                let Some(i) = eq.bands.find(id) else { return };
                let b = &mut eq.bands[i];
                match knob {
                    EqKnob::Freq => b.freq = v.round(),
                    EqKnob::Gain => b.gain_db = v,
                    EqKnob::Q => b.q = v,
                    EqKnob::Range => b.dynamic.get_or_insert_default().range_db = v,
                    EqKnob::Threshold => b.dynamic.get_or_insert_default().threshold_db = v,
                    _ => {}
                }
            }
        }
    }

    fn eq_knob_value(&self, id: u16, knob: EqKnob) -> f32 {
        let Some((_, eq)) = self.shown_eq() else {
            return 0.0;
        };
        let b = eq.bands.find(id).map(|i| eq.bands[i]).unwrap_or_default();
        let d = b.dynamic.unwrap_or_default();
        match knob {
            EqKnob::Freq => b.freq,
            EqKnob::Gain => b.gain_db,
            EqKnob::Q => b.q,
            EqKnob::Range => d.range_db,
            EqKnob::Threshold => d.threshold_db,
            EqKnob::Scale => eq.scale,
            EqKnob::Output => eq.output_db,
        }
    }

    /// What a knob goes back to on a double-click.
    fn eq_knob_reset(&mut self, id: u16, knob: EqKnob, cx: &mut Context<Self>) {
        let shape = self.eq_band(id).map_or(Shape::Bell, |(_, b)| b.shape);
        let d = Dynamic::default();
        let v = match knob {
            EqKnob::Freq => 1_000.0,
            EqKnob::Gain | EqKnob::Output => 0.0,
            EqKnob::Q => shape.default_q(),
            EqKnob::Range => d.range_db,
            EqKnob::Threshold => d.threshold_db,
            EqKnob::Scale => 1.0,
        };
        self.edit(cx, |ws| ws.set_eq_knob(id, knob, v));
    }

    pub fn eq_op(&mut self, op: EqOp, cx: &mut Context<Self>) {
        let Some(ch) = self.eq_view.as_ref().map(|v| v.ch) else {
            return;
        };
        match op {
            EqOp::Shape(id, shape) => self.edit_band(id, cx, |b| {
                if b.shape != shape {
                    b.q = shape.default_q();
                    // A shape that takes a slope keeps one it takes.
                    let slopes = shape.slopes();
                    if !slopes.is_empty() && !slopes.contains(&b.slope) {
                        b.slope = 12;
                    }
                }
                b.shape = shape;
            }),
            EqOp::Slope(id, slope) => self.edit_band(id, cx, |b| b.slope = slope),
            EqOp::Placement(id, place) => self.edit_band(id, cx, |b| b.placement = place),
            EqOp::Dynamic(id) => self.edit_band(id, cx, |b| {
                b.dynamic = match b.dynamic {
                    Some(_) => None,
                    None => Some(Dynamic::default()),
                }
            }),
            EqOp::Off(id) => self.edit_band(id, cx, |b| b.off = !b.off),
            EqOp::Delete(id) => {
                self.edit(cx, |ws| {
                    if let Some(eq) = ws.strip_dsp_mut(ch).0.as_mut()
                        && let Some(i) = eq.bands.find(id)
                    {
                        eq.bands.remove(i);
                    }
                });
                if let Some(v) = self.eq_view.as_mut() {
                    v.picked = v.picked.filter(|&p| p != id);
                    v.hover = v.hover.filter(|&p| p != id);
                    v.solo = v.solo.filter(|&p| p != id);
                }
            }
            EqOp::Solo(id) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.solo = if v.solo == Some(id) { None } else { Some(id) };
                }
            }
            EqOp::Analyzer(a) => {
                if let Some(v) = self.eq_view.as_mut() {
                    v.analyzer = a;
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
        }
        cx.notify();
    }

    /// Delete with the EQ window last clicked in and a band picked: that band goes.
    pub fn delete_eq_band(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self
            .eq_view
            .as_ref()
            .filter(|v| v.focused)
            .and_then(|v| v.picked)
        else {
            return false;
        };
        self.eq_op(EqOp::Delete(id), cx);
        true
    }

    /// A band's menu: its shape, slope and part of the sound, and the rest.
    pub(crate) fn band_menu(&mut self, id: u16, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some((_, b)) = self.eq_band(id) else {
            return;
        };
        let tick = |on: bool| if on { "✓ " } else { "" };
        let mut items = vec![("Shape".to_string(), MenuAction::Nothing)];
        items.extend(Shape::ALL.iter().map(|&s| {
            (
                format!("{}{}", tick(s == b.shape), s.name()),
                MenuAction::Eq(EqOp::Shape(id, s)),
            )
        }));
        if !b.shape.slopes().is_empty() {
            items.push((String::new(), MenuAction::Nothing));
            items.push(("Slope".into(), MenuAction::Nothing));
            items.extend(b.shape.slopes().iter().map(|&s| {
                (
                    format!("{}{s} dB/oct", tick(s == b.slope)),
                    MenuAction::Eq(EqOp::Slope(id, s)),
                )
            }));
        }
        items.push((String::new(), MenuAction::Nothing));
        items.push(("Works on".into(), MenuAction::Nothing));
        items.extend(Placement::ALL.iter().map(|&p| {
            (
                format!("{}{}", tick(p == b.placement), p.name()),
                MenuAction::Eq(EqOp::Placement(id, p)),
            )
        }));
        items.push((String::new(), MenuAction::Nothing));
        if b.shape.has_gain() {
            items.push((
                format!("{}Dynamic", tick(b.dynamic.is_some())),
                MenuAction::Eq(EqOp::Dynamic(id)),
            ));
        }
        let solo = self.eq_view.as_ref().is_some_and(|v| v.solo == Some(id));
        items.push((
            format!("{}Hear alone", tick(solo)),
            MenuAction::Eq(EqOp::Solo(id)),
        ));
        items.push((
            if b.off { "Switch on" } else { "Switch off" }.into(),
            MenuAction::Eq(EqOp::Off(id)),
        ));
        items.push(("Delete band".into(), MenuAction::Eq(EqOp::Delete(id))));
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    fn eq_choice_menu(
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

    /// The EQ window, if it's open.
    pub fn eq_window(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let (ch, eq) = match self.shown_eq() {
            Some(s) => s,
            None => {
                // Its channel or EQ is gone.
                if self.eq_view.is_some() {
                    self.eq_view = None;
                    self.player.watch_eq(None);
                    self.player.solo_band(None);
                }
                return None;
            }
        };
        let rate = self.session.project.sample_rate as f64;
        let (name, color) = self.channel_look(ch);
        let playing = self.player.playing();
        let view = self.eq_view.as_ref()?;
        let (w, h) = view.display_size();
        let range = view.range as f32;
        let (picked, hover, solo) = (view.picked, view.hover, view.solo);
        let analyzer = view.analyzer;
        let columns = view.columns.clone();
        let (pre, post) = (view.pre.clone(), view.post.clone());
        let display_bounds = view.display.clone();
        let pointer = view.pointer;
        let at = view.at;
        let dragging = matches!(self.drag, Some(Drag::EqNode { .. }));
        let tap = self.player.eq_tap();

        // Each band: its curve at the analyzer's columns, and where its dot goes.
        let drawn: Vec<Drawn> = eq
            .bands
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let moved = if playing { tap.moved(i) } else { 0.0 };
                let mut heard = *b;
                heard.gain_db += if eq.scale > 0.0 {
                    moved / eq.scale
                } else {
                    0.0
                };
                let sections = band_sections(&heard, eq.scale, rate);
                let curve = columns
                    .iter()
                    .map(|&f| response_db(&sections, f, rate) as f32)
                    .collect();
                let dot_db = match b.shape {
                    s if s.has_gain() => heard.gain(eq.scale),
                    Shape::LowCut | Shape::HighCut => {
                        response_db(&sections, b.freq as f64, rate) as f32
                    }
                    _ => 0.0,
                };
                Drawn {
                    id: b.id,
                    color: band_color(i),
                    band: *b,
                    curve,
                    dot: (
                        x_of(b.freq as f64, w, rate),
                        y_of(dot_db.clamp(-range, range), h, range),
                    ),
                }
            })
            .collect();
        // The curve the middle of the sound gets (its stereo and mid bands); and the edges', or
        // a side's, where bands work on them alone.
        let part = |also: Placement| -> Vec<f32> {
            (0..columns.len())
                .map(|c| {
                    drawn
                        .iter()
                        .filter(|d| {
                            !d.band.off && matches!(d.band.placement, p if p == Placement::Stereo || p == also)
                        })
                        .map(|d| d.curve[c])
                        .sum::<f32>()
                        + eq.output_db
                })
                .collect()
        };
        let total = part(Placement::Mid);
        let parts: Vec<(Vec<f32>, &'static str)> = [
            (Placement::Side, "S"),
            (Placement::Left, "L"),
            (Placement::Right, "R"),
        ]
        .into_iter()
        .filter(|(p, _)| drawn.iter().any(|d| !d.band.off && d.band.placement == *p))
        .map(|(p, label)| (part(p), label))
        .collect();

        let display = self.eq_display(
            &eq, &drawn, &total, &parts, &columns, &pre, &post, analyzer, w, h, range, rate, cx,
        );
        // A dynamic band: a line from its dot to as far as it can move.
        let reach: Vec<Div> = drawn
            .iter()
            .filter(|d| !d.band.off && d.band.shape.has_gain())
            .filter_map(|d| {
                let dynamic = d.band.dynamic?;
                let to = y_of(
                    ((d.band.gain_db + dynamic.range_db) * eq.scale).clamp(-range, range),
                    h,
                    range,
                );
                let (top, bottom) = (to.min(d.dot.1), to.max(d.dot.1));
                Some(
                    div()
                        .absolute()
                        .left(px(d.dot.0 - 1.0))
                        .top(px(top))
                        .w(px(2.))
                        .h(px((bottom - top).max(1.0)))
                        .bg(alpha(d.color, 0.55))
                        .child(
                            div()
                                .absolute()
                                .left(px(-4.0))
                                .top(px(if to < d.dot.1 {
                                    0.0
                                } else {
                                    bottom - top - 2.0
                                }))
                                .w(px(10.))
                                .h(px(2.))
                                .bg(rgb(d.color)),
                        ),
                )
            })
            .collect();
        // The part curves' names, at their right ends.
        let part_labels: Vec<Div> = parts
            .iter()
            .map(|(curve, label)| {
                let y = y_of(curve.last().copied().unwrap_or(0.0), h, range).clamp(8.0, h - 24.0);
                div()
                    .absolute()
                    .left(px(w - 64.0))
                    .top(px(y - 16.0))
                    .px(px(4.))
                    .rounded(px(3.))
                    .bg(alpha(0xffffff, 0.12))
                    .text_size(px(9.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(TEXT_2))
                    .child(*label)
            })
            .collect();
        let nodes: Vec<Div> = drawn
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let id = d.id;
                let on = picked == Some(id);
                let lit = on || hover == Some(id);
                let size = if lit { NODE + 2.0 } else { NODE };
                div()
                    .absolute()
                    .left(px(d.dot.0 - size / 2.0))
                    .top(px(d.dot.1 - size / 2.0))
                    .size(px(size))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(9.))
                    .font_weight(FontWeight::BOLD)
                    .when(!d.band.off, |n| {
                        n.bg(rgb(d.color)).text_color(rgb(0x101014))
                    })
                    .when(d.band.off, |n| {
                        n.bg(rgb(0x15161b))
                            .border_2()
                            .border_color(alpha(d.color, 0.6))
                            .text_color(alpha(d.color, 0.8))
                    })
                    .when(on, |n| n.border_2().border_color(rgb(0xffffff)))
                    .when(solo == Some(id), |n| {
                        n.shadow(vec![gpui::BoxShadow {
                            color: alpha(SOLO, 0.7),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(8.),
                            spread_radius: px(2.),
                            inset: false,
                        }])
                    })
                    .debug_selector(move || format!("eq-band-{i}"))
                    .child(format!("{}", i + 1))
                    .on_mouse_move(cx.listener(move |ws, _: &MouseMoveEvent, _, cx| {
                        if let Some(v) = ws.eq_view.as_mut()
                            && v.hover != Some(id)
                        {
                            v.hover = Some(id);
                            cx.notify();
                        }
                    }))
                    .on_scroll_wheel(cx.listener(move |ws, e: &ScrollWheelEvent, _, cx| {
                        cx.stop_propagation();
                        ws.scroll_band(id, e, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.eq_op(EqOp::Delete(id), cx);
                            } else if e.modifiers.alt {
                                ws.eq_op(EqOp::Off(id), cx);
                            } else {
                                ws.press_band(id, e, cx);
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if let Some(v) = ws.eq_view.as_mut() {
                                v.picked = Some(id);
                            }
                            ws.band_menu(id, e.position, cx);
                        }),
                    )
            })
            .collect();
        // What the band under the pointer (or being dragged) is set to.
        let told = if dragging { picked } else { hover.or(picked) };
        let tooltip = told
            .and_then(|id| drawn.iter().position(|d| d.id == id))
            .filter(|_| dragging || hover.is_some())
            .map(|i| {
                let d = &drawn[i];
                let b = d.band;
                let mut text = format!("{}  {}", hz_text(b.freq), note_text(b.freq));
                if b.shape.has_gain() {
                    text += &format!("   {} dB", db_text(b.gain_db));
                }
                text += &format!("   Q {}", q_text(b.q));
                let left = (d.dot.0 - 90.0).clamp(4.0, w - 184.0);
                let top = if d.dot.1 > 44.0 {
                    d.dot.1 - 36.0
                } else {
                    d.dot.1 + 16.0
                };
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(180.))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(5.))
                            .bg(alpha(0x0a0a0d, 0.92))
                            .border_1()
                            .border_color(alpha(d.color, 0.6))
                            .font_family(MONO)
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_2))
                            .whitespace_nowrap()
                            .child(text),
                    )
            });
        // Over the empty display: where a double-click would add a band.
        let ghost = pointer
            .filter(|_| hover.is_none() && !dragging && eq.bands.len() < MAX_BANDS)
            .map(|(x, y)| {
                let shape = shape_at(freq_at(x, w, rate));
                let y = if shape.has_gain() { y } else { h / 2.0 };
                div()
                    .absolute()
                    .left(px(x - 7.0))
                    .top(px(y - 7.0))
                    .size(px(14.))
                    .rounded_full()
                    .border_1()
                    .border_color(alpha(0xffffff, 0.35))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .text_color(alpha(0xffffff, 0.45))
                    .child("+")
            });
        let panel = picked
            .and_then(|id| drawn.iter().position(|d| d.id == id))
            .map(|i| self.band_panel(i, &drawn[i], solo, w, h, cx));
        let empty = eq.bands.is_empty().then(|| {
            div()
                .absolute()
                .left(px(0.))
                .top(px(h / 2.0 - 34.0))
                .w(px(w))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .text_size(px(13.))
                .text_color(rgb(TEXT_3))
                .child("Double-click anywhere to add a band")
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_4))
                        .child("A cut at either end, a bell between"),
                )
        });

        let display_area = div()
            .id("eq-display")
            .relative()
            .w(px(w))
            .h(px(h))
            .overflow_hidden()
            .debug_selector(|| "eq-display".into())
            .child(
                canvas(move |b, _, _| display_bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(display)
            .children(part_labels)
            .children(reach)
            .children(empty)
            .children(ghost)
            .children(nodes)
            .children(tooltip)
            .children(panel)
            .on_mouse_move(cx.listener(move |ws, e: &MouseMoveEvent, _, cx| {
                let Some(v) = ws.eq_view.as_mut() else { return };
                let o = v.display.get().origin;
                let p = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
                v.pointer = Some(p);
                v.hover = None;
                cx.notify();
            }))
            .on_hover(cx.listener(|ws, over: &bool, _, cx| {
                if !over && let Some(v) = ws.eq_view.as_mut() {
                    (v.pointer, v.hover) = (None, None);
                    cx.notify();
                }
            }))
            .on_scroll_wheel(cx.listener(|ws, e: &ScrollWheelEvent, _, cx| {
                if let Some(id) = ws.eq_view.as_ref().and_then(|v| v.picked) {
                    ws.scroll_band(id, e, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    let Some(v) = ws.eq_view.as_mut() else { return };
                    v.focused = true;
                    let o = v.display.get().origin;
                    let (x, y) = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
                    if e.click_count == 2 {
                        ws.add_band_at(x, y, e, cx);
                    } else {
                        v.picked = None;
                        cx.notify();
                    }
                }),
            );

        let title = div()
            .h(px(TITLE_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .border_b_1()
            .border_color(rgb(LINE))
            .cursor(gpui::CursorStyle::OpenHand)
            .debug_selector(|| "eq-title".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    let Some(v) = ws.eq_view.as_mut() else { return };
                    v.focused = true;
                    let start = v.at;
                    ws.begin_drag(
                        Drag::EqWindow {
                            from: pt(e.position),
                            start,
                            resize: false,
                        },
                        cx,
                    );
                }),
            )
            .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(color)))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Channel EQ"),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(name),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_4))
                    .child(format!(
                        "{} band{}",
                        eq.bands.len(),
                        if eq.bands.len() == 1 { "" } else { "s" }
                    )),
            )
            .child(
                div()
                    .h(px(24.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .when(eq.bypass, |d| d.bg(alpha(SOLO, 0.16)).text_color(rgb(SOLO)))
                    .when(!eq.bypass, |d| {
                        d.bg(rgb(FIELD))
                            .text_color(rgb(TEXT_2))
                            .hover(|d| d.bg(rgb(RAISED)))
                    })
                    .debug_selector(|| "eq-bypass".into())
                    .child(if eq.bypass { "Bypassed" } else { "On" })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.eq_op(EqOp::Bypass, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(rgb(TEXT_3))
                    .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                    .debug_selector(|| "eq-close".into())
                    .child(icon(Icon::Close, 11.).text_color(rgb(TEXT_3)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.close_eq(cx);
                        }),
                    ),
            );

        let bar = self.eq_bar(&eq, analyzer, range, cx);
        let (ww, wh) = self.eq_view.as_ref().map(|v| v.size)?;
        let frame = div()
            .id("eq-window")
            .occlude()
            .w(px(ww))
            .h(px(wh))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .overflow_hidden()
            .bg(rgb(0x111216))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(title)
            .child(display_area)
            .child(bar)
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

    /// The display's picture: the grid, the analyzer, each band's area and the EQ's curve.
    #[allow(clippy::too_many_arguments)]
    fn eq_display(
        &self,
        eq: &Eq,
        drawn: &[Drawn],
        total: &[f32],
        parts: &[(Vec<f32>, &'static str)],
        columns: &[f64],
        pre: &[f32],
        post: &[f32],
        analyzer: Analyzer,
        w: f32,
        h: f32,
        range: f32,
        rate: f64,
        _cx: &mut Context<Self>,
    ) -> Div {
        let picked = self.eq_view.as_ref().and_then(|v| v.picked);
        let hover = self.eq_view.as_ref().and_then(|v| v.hover);
        let xs: Vec<f32> = columns.iter().map(|&f| x_of(f, w, rate)).collect();
        let bands: Vec<(Vec<f32>, u32, f32, bool)> = drawn
            .iter()
            .map(|d| {
                let strong = picked == Some(d.id) || hover == Some(d.id);
                // What takes a lot away (a cut, a notch) fills lightly, or it would cover all.
                let light = if d.band.shape.has_gain() { 1.0 } else { 0.4 };
                let a = match (d.band.off, strong) {
                    (true, _) => 0.04,
                    (false, true) => 0.26 * light,
                    (false, false) => 0.12 * light,
                };
                (d.curve.clone(), d.color, a, strong && !d.band.off)
            })
            .collect();
        let total = total.to_vec();
        let parts: Vec<Vec<f32>> = parts.iter().map(|(c, _)| c.clone()).collect();
        let (pre, post) = (pre.to_vec(), post.to_vec());
        let bypass = eq.bypass;
        let grid_freqs: Vec<(f32, bool)> = [
            20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 200.0, 300.0, 400.0, 500.0,
            600.0, 700.0, 800.0, 900.0, 1e3, 2e3, 3e3, 4e3, 5e3, 6e3, 7e3, 8e3, 9e3, 1e4, 2e4,
        ]
        .iter()
        .filter(|&&f| f < top_hz(rate))
        .map(|&f| (x_of(f, w, rate), [100.0, 1e3, 1e4].contains(&f)))
        .collect();
        let step = match range as u8 {
            3 => 1.0,
            6 => 2.0,
            12 => 3.0,
            _ => 6.0,
        };
        let grid_dbs: Vec<f32> = (1..=(range / step) as i32)
            .flat_map(|k| [k as f32 * step, -(k as f32) * step])
            .collect();
        let labels_f: Vec<Div> = [20.0, 50.0, 100.0, 200.0, 500.0, 1e3, 2e3, 5e3, 1e4, 2e4]
            .iter()
            .filter(|&&f| f < top_hz(rate))
            .map(|&f| {
                let x = x_of(f, w, rate);
                let text = if f >= 1e3 {
                    format!("{}k", f / 1e3)
                } else {
                    format!("{f}")
                };
                div()
                    .absolute()
                    .left(px(x + 3.0))
                    .bottom(px(3.))
                    .font_family(MONO)
                    .text_size(px(9.5))
                    .text_color(rgb(TEXT_4))
                    .child(text)
            })
            .collect();
        let labels_db: Vec<Div> = grid_dbs
            .iter()
            .chain(&[0.0])
            .filter(|d| d.abs() < range)
            .map(|&d| {
                div()
                    .absolute()
                    .right(px(6.))
                    .top(px(y_of(d, h, range) - 7.0))
                    .font_family(MONO)
                    .text_size(px(9.5))
                    .text_color(rgb(if d == 0.0 { TEXT_3 } else { TEXT_4 }))
                    .child(if d > 0.0 {
                        format!("+{d}")
                    } else {
                        db_text(d).trim_end_matches(".0").to_string()
                    })
            })
            .collect();
        let picture = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let o = b.origin;
                let at = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
                window.paint_quad(fill(
                    b,
                    linear_gradient(
                        180.,
                        linear_color_stop(rgb(0x0d0e12), 0.),
                        linear_color_stop(rgb(0x121318), 1.),
                    ),
                ));
                // The grid.
                for &(x, major) in &grid_freqs {
                    window.paint_quad(fill(
                        Bounds::new(at(x, 0.0), size(px(1.), b.size.height)),
                        alpha(0xffffff, if major { 0.07 } else { 0.03 }),
                    ));
                }
                for &d in &grid_dbs {
                    window.paint_quad(fill(
                        Bounds::new(at(0.0, y_of(d, h, range)), size(b.size.width, px(1.))),
                        alpha(0xffffff, 0.035),
                    ));
                }
                window.paint_quad(fill(
                    Bounds::new(at(0.0, y_of(0.0, h, range)), size(b.size.width, px(1.))),
                    alpha(0xffffff, 0.12),
                ));
                let n = xs.len();
                if n < 2 {
                    return;
                }
                // The analyzer: what comes out filled, what goes in a line.
                if matches!(analyzer, Analyzer::Both | Analyzer::Post) {
                    let mut area = PathBuilder::fill();
                    area.move_to(at(xs[0], h));
                    for i in 0..n {
                        area.line_to(at(xs[i], analyzer_y(post[i], h)));
                    }
                    area.line_to(at(xs[n - 1], h));
                    area.close();
                    if let Ok(p) = area.build() {
                        window.paint_path(
                            p,
                            linear_gradient(
                                180.,
                                linear_color_stop(alpha(ACCENT, 0.32), 0.),
                                linear_color_stop(alpha(ACCENT, 0.05), 1.),
                            ),
                        );
                    }
                    let mut line = PathBuilder::stroke(px(1.));
                    line.move_to(at(xs[0], analyzer_y(post[0], h)));
                    for i in 1..n {
                        line.line_to(at(xs[i], analyzer_y(post[i], h)));
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, alpha(ACCENT_TEXT, 0.45));
                    }
                }
                if matches!(analyzer, Analyzer::Both | Analyzer::Pre) {
                    let mut line = PathBuilder::stroke(px(1.));
                    line.move_to(at(xs[0], analyzer_y(pre[0], h)));
                    for i in 1..n {
                        line.line_to(at(xs[i], analyzer_y(pre[i], h)));
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, alpha(0xffffff, 0.22));
                    }
                }
                // Each band's area, from 0 dB to its curve.
                let zero = y_of(0.0, h, range);
                let clamp_y = |d: f32| y_of(d, h, range).clamp(-2.0, h + 2.0);
                for (curve, color, a, strong) in &bands {
                    let mut area = PathBuilder::fill();
                    area.move_to(at(xs[0], zero));
                    for i in 0..n {
                        area.line_to(at(xs[i], clamp_y(curve[i])));
                    }
                    area.line_to(at(xs[n - 1], zero));
                    area.close();
                    if let Ok(p) = area.build() {
                        window.paint_path(p, alpha(*color, *a));
                    }
                    if *strong {
                        let mut line = PathBuilder::stroke(px(1.2));
                        line.move_to(at(xs[0], clamp_y(curve[0])));
                        for i in 1..n {
                            line.line_to(at(xs[i], clamp_y(curve[i])));
                        }
                        if let Ok(p) = line.build() {
                            window.paint_path(p, alpha(*color, 0.8));
                        }
                    }
                }
                // The edges' or a side's curve, where bands work on them alone.
                for curve in &parts {
                    let mut line = PathBuilder::stroke(px(1.4));
                    line.move_to(at(xs[0], clamp_y(curve[0])));
                    for i in 1..n {
                        line.line_to(at(xs[i], clamp_y(curve[i])));
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, alpha(0xf4f4f6, 0.45));
                    }
                }
                // The EQ's curve, with a glow under it.
                let curve_color = if bypass { 0x6a6a74 } else { 0xf4f4f6 };
                for (width, a) in [(5.0, 0.10), (2.0, 1.0)] {
                    let mut line = PathBuilder::stroke(px(width));
                    line.move_to(at(xs[0], clamp_y(total[0])));
                    for i in 1..n {
                        line.line_to(at(xs[i], clamp_y(total[i])));
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, alpha(curve_color, a));
                    }
                }
            },
        )
        .absolute()
        .size_full();
        div()
            .absolute()
            .size_full()
            .child(picture)
            .children(labels_f)
            .children(labels_db)
    }

    /// The picked band's controls, along the bottom of the display.
    fn band_panel(
        &self,
        i: usize,
        d: &Drawn,
        solo: Option<u16>,
        w: f32,
        h: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let b = d.band;
        let id = d.id;
        let knob = |k: EqKnob, cx: &mut Context<Self>| self.eq_knob(id, k, d.color, cx);
        let pick = |label: String, sel: &'static str, cx: &mut Context<Self>| {
            dropdown(label, 24.)
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let Some((_, b)) = ws.eq_band(id) else { return };
                        let tick = |on: bool| if on { "✓ " } else { "" };
                        let items: Vec<(String, MenuAction)> = match sel {
                            "eq-shape" => Shape::ALL
                                .iter()
                                .map(|&s| {
                                    (
                                        format!("{}{}", tick(s == b.shape), s.name()),
                                        MenuAction::Eq(EqOp::Shape(id, s)),
                                    )
                                })
                                .collect(),
                            "eq-slope" => b
                                .shape
                                .slopes()
                                .iter()
                                .map(|&s| {
                                    (
                                        format!("{}{s} dB/oct", tick(s == b.slope)),
                                        MenuAction::Eq(EqOp::Slope(id, s)),
                                    )
                                })
                                .collect(),
                            _ => Placement::ALL
                                .iter()
                                .map(|&p| {
                                    (
                                        format!("{}{}", tick(p == b.placement), p.name()),
                                        MenuAction::Eq(EqOp::Placement(id, p)),
                                    )
                                })
                                .collect(),
                        };
                        ws.eq_choice_menu(e.position, items, cx);
                    }),
                )
        };
        let toggle_btn = |label: &'static str,
                          on: bool,
                          on_color: u32,
                          sel: &'static str,
                          op: EqOp,
                          cx: &mut Context<Self>| {
            div()
                .h(px(24.))
                .px(px(8.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .when(on, |d| {
                    d.bg(alpha(on_color, 0.18)).text_color(rgb(on_color))
                })
                .when(!on, |d| {
                    d.bg(rgb(FIELD))
                        .text_color(rgb(TEXT_3))
                        .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT_2)))
                })
                .debug_selector(move || sel.into())
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.eq_op(op, cx);
                    }),
                )
        };
        let has_slope = !b.shape.slopes().is_empty();
        let panel_w = 560.0f32.min(w - 24.0);
        div()
            .absolute()
            .left(px((w - panel_w) / 2.0))
            .top(px(h - 104.0))
            .w(px(panel_w))
            .h(px(84.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(12.))
            .rounded(px(12.))
            .bg(alpha(0x16171c, 0.94))
            .border_1()
            .border_color(alpha(d.color, 0.45))
            .shadow_lg()
            .debug_selector(|| "eq-panel".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(v) = ws.eq_view.as_mut() {
                        v.focused = true;
                    }
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(div().size(px(9.)).rounded(px(5.)).bg(rgb(d.color)))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_2))
                                    .child(format!("Band {}", i + 1)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .child(pick(b.shape.name().into(), "eq-shape", cx))
                            .when(has_slope, |d| {
                                d.child(pick(format!("{} dB/oct", b.slope), "eq-slope", cx))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .child(pick(b.placement.name().into(), "eq-place", cx))
                            .when(b.shape.has_gain(), |d| {
                                d.child(toggle_btn(
                                    "Dynamic",
                                    b.dynamic.is_some(),
                                    AGENT,
                                    "eq-dynamic",
                                    EqOp::Dynamic(id),
                                    cx,
                                ))
                            }),
                    ),
            )
            .child(div().w(px(1.)).h(px(56.)).bg(rgb(LINE)))
            .child(knob(EqKnob::Freq, cx))
            .when(b.shape.has_gain(), |d| d.child(knob(EqKnob::Gain, cx)))
            .child(knob(EqKnob::Q, cx))
            .when(b.dynamic.is_some() && b.shape.has_gain(), |d| {
                d.child(knob(EqKnob::Range, cx))
                    .child(knob(EqKnob::Threshold, cx))
            })
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(toggle_btn(
                        "Solo",
                        solo == Some(id),
                        SOLO,
                        "eq-solo",
                        EqOp::Solo(id),
                        cx,
                    ))
                    .child(toggle_btn(
                        if b.off { "Off" } else { "On" },
                        !b.off,
                        MINT,
                        "eq-band-off",
                        EqOp::Off(id),
                        cx,
                    ))
                    .child(toggle_btn(
                        "Delete",
                        false,
                        RECORD,
                        "eq-delete",
                        EqOp::Delete(id),
                        cx,
                    )),
            )
    }

    /// A knob: its name, the knob to drag (double-click resets it) and its value.
    fn eq_knob(&self, id: u16, k: EqKnob, color: u32, cx: &mut Context<Self>) -> Div {
        let v = self.eq_knob_value(id, k);
        let pos = k.pos(v);
        let shown = if k.bipolar() { pos * 2.0 - 1.0 } else { pos };
        div()
            .w(px(56.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .child(
                div()
                    .text_size(px(9.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_4))
                    .child(k.label()),
            )
            .child(
                div()
                    .cursor_ns_resize()
                    .debug_selector(move || format!("eq-knob-{}", k.label().to_lowercase()))
                    .child(knob(shown, 32., color, k.bipolar()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.eq_knob_reset(id, k, cx);
                                return;
                            }
                            let start = ws.eq_knob_value(id, k);
                            ws.begin_drag(
                                Drag::EqKnob {
                                    id,
                                    knob: k,
                                    from: f32::from(e.position.y),
                                    start,
                                },
                                cx,
                            );
                        }),
                    ),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_2))
                    .whitespace_nowrap()
                    .child(k.text(v)),
            )
    }

    /// Along the bottom: the analyzer, the range, the gain scale and output, and a way to resize.
    fn eq_bar(&self, eq: &Eq, analyzer: Analyzer, range: f32, cx: &mut Context<Self>) -> Div {
        let value = |k: EqKnob, v: f32, cx: &mut Context<Self>| {
            div()
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(6.))
                .bg(rgb(FIELD))
                .cursor_ns_resize()
                .debug_selector(move || format!("eq-{}", k.label().to_lowercase()))
                .child(
                    div()
                        .text_size(px(9.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_4))
                        .child(k.label()),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_2))
                        .child(k.text(v)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if e.click_count == 2 {
                            ws.eq_knob_reset(0, k, cx);
                            return;
                        }
                        let start = ws.eq_knob_value(0, k);
                        ws.begin_drag(
                            Drag::EqKnob {
                                id: 0,
                                knob: k,
                                from: f32::from(e.position.y),
                                start,
                            },
                            cx,
                        );
                    }),
                )
        };
        let menu_btn = |label: String, sel: &'static str, cx: &mut Context<Self>| {
            dropdown(label, 26.)
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let Some(v) = ws.eq_view.as_ref() else { return };
                        let tick = |on: bool| if on { "✓ " } else { "" };
                        let items = match sel {
                            "eq-analyzer" => Analyzer::ALL
                                .iter()
                                .map(|&a| {
                                    (
                                        format!("{}{}", tick(a == v.analyzer), a.name()),
                                        MenuAction::Eq(EqOp::Analyzer(a)),
                                    )
                                })
                                .collect(),
                            _ => RANGES
                                .iter()
                                .map(|&r| {
                                    (
                                        format!("{}± {r} dB", tick(r == v.range)),
                                        MenuAction::Eq(EqOp::Range(r)),
                                    )
                                })
                                .collect(),
                        };
                        ws.eq_choice_menu(e.position, items, cx);
                    }),
                )
        };
        div()
            .h(px(BAR_H))
            .flex_none()
            .relative()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child("Analyzer"),
            )
            .child(menu_btn(analyzer.name().into(), "eq-analyzer", cx))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child("Range"),
            )
            .child(menu_btn(format!("± {range} dB"), "eq-range", cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(
                        "Double-click to add · drag a dot · ⌘-drag or scroll: width · \
                         ⌥-click: off · double-click it: delete",
                    ),
            )
            .child(value(EqKnob::Scale, eq.scale, cx))
            .child(value(EqKnob::Output, eq.output_db, cx))
            .child(
                div()
                    .absolute()
                    .right(px(0.))
                    .bottom(px(0.))
                    .size(px(14.))
                    .cursor(gpui::CursorStyle::ResizeUpLeftDownRight)
                    .debug_selector(|| "eq-resize".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let Some(v) = ws.eq_view.as_ref() else { return };
                            let start = v.size;
                            ws.begin_drag(
                                Drag::EqWindow {
                                    from: pt(e.position),
                                    start,
                                    resize: true,
                                },
                                cx,
                            );
                        }),
                    ),
            )
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
        if resize {
            v.size = (
                (start.0 + dx).clamp(MIN_W, vw.max(MIN_W)),
                (start.1 + dy).clamp(MIN_H, vh.max(MIN_H)),
            );
        } else {
            v.at = (
                (start.0 + dx).clamp(-v.size.0 + 80.0, vw - 80.0),
                (start.1 + dy).clamp(0.0, vh - TITLE_H),
            );
        }
    }
}

#[cfg(test)]
impl EqView {
    /// A mix's spectrum, as if it were playing through `eq`: for pictures.
    pub fn pretend(&mut self, eq: &Eq, rate: f64) {
        let w = self.display_size().0.max(100.0);
        let n = (w / 3.0) as usize;
        self.columns = (0..n)
            .map(|i| freq_at(i as f32 / (n - 1) as f32 * w, w, rate))
            .collect();
        let mut seed = 7u32;
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
                -40.0 - 4.5 * octave + kick + body + top + jitter + TILT * octave
            })
            .collect();
        self.post = self
            .columns
            .iter()
            .zip(&self.pre)
            .map(|(&f, &p)| p + tempo_engine::eq::eq_response(eq, f, rate) as f32)
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knobs_and_the_display_map_both_ways() {
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
        let rate = 48_000.0;
        let x = x_of(1_000.0, 800.0, rate);
        assert!((freq_at(x, 800.0, rate) - 1_000.0).abs() < 0.01);
        assert!((gain_at(y_of(6.0, 400.0, 12.0), 400.0, 12.0) - 6.0).abs() < 1e-4);
        assert_eq!(hz_text(850.0), "850 Hz");
        assert_eq!(hz_text(1_200.0), "1.20 kHz");
        assert_eq!(note_text(440.0), "A4");
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
        set_low_cut(&mut empty, 120.0);
        assert_eq!(
            (empty.bands[0].shape, empty.bands[0].freq),
            (Shape::LowCut, 120.0)
        );
    }
}
