//! A reference blueprint: a song's shape and sound, as far as Pina Studio can tell, from its audio
//! or from a picture of its spectrogram, in one form whichever it came from. Its energy in bands
//! across the spectrum, frame by frame; where things start, low, mid and high; its pitch classes
//! and the pitched lines that hold (tonal contours); its sections and how hard each plays; and,
//! from audio, its tempo, bars and key. Every finding says how it was found (measured, estimated,
//! or unknown) and from what, so nothing an image can't show is claimed from one.
//!
//! The same blueprint made from a song's render is what it's compared with (`compare`), section
//! by section. Writing a song like it is the agent's: every note its own, from the blueprint's
//! brief.

pub mod audio;
pub mod compare;
pub mod image;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Bumped when what a blueprint holds changes, so kept ones are made again.
pub const VERSION: u32 = 4;
/// Bands across the spectrum, a third of an octave or so each, from `LOW_HZ` to `HIGH_HZ`.
pub const BANDS: usize = 24;
pub const LOW_HZ: f32 = 30.0;
pub const HIGH_HZ: f32 = 16_000.0;
/// Frames a second, when how long it runs is known.
pub const FPS: f32 = 20.0;
/// Frames across a picture whose length isn't known: places in it, not times.
pub const SPREAD: usize = 400;
/// The quietest a band reads, under the loudest.
pub const FLOOR: f32 = -60.0;
/// Where the low, mid and high ranges part, for starts and for the broad balance.
pub const LOW_MID: f32 = 200.0;
pub const MID_HIGH: f32 = 4_000.0;

/// What a blueprint was made from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Audio,
    Image,
    /// A song's own render, to compare with its reference.
    Render,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Audio => "audio",
            Source::Image => "image",
            Source::Render => "render",
        }
    }
}

/// How a finding was come by.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Read from the audio.
    Measured,
    /// Inferred, from too little to be sure (an image's colours, a guess at its tempo).
    Estimated,
    /// Not there to be found.
    Unknown,
}

/// One finding, as people read it: what, its value, how it was come by and how sure.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Fact {
    pub name: String,
    pub value: String,
    pub basis: Basis,
    pub confidence: f32,
    pub from: Source,
}

/// How a spectrogram's rows run from its lowest frequency to its highest.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FreqScale {
    Linear,
    Log,
    Mel,
}

impl FreqScale {
    pub const ALL: [FreqScale; 3] = [FreqScale::Linear, FreqScale::Log, FreqScale::Mel];

    pub fn name(self) -> &'static str {
        match self {
            FreqScale::Linear => "linear",
            FreqScale::Log => "log",
            FreqScale::Mel => "mel",
        }
    }

    pub fn parse(s: &str) -> Option<FreqScale> {
        FreqScale::ALL
            .into_iter()
            .find(|f| f.name().eq_ignore_ascii_case(s.trim()))
    }

    /// The frequency at a place up the axis (0 its bottom, 1 its top).
    pub fn hz(self, low: f32, high: f32, up: f32) -> f32 {
        match self {
            FreqScale::Linear => low + (high - low) * up,
            FreqScale::Log => low.max(1.0) * (high / low.max(1.0)).powf(up),
            FreqScale::Mel => {
                let (a, b) = (mel(low), mel(high));
                unmel(a + (b - a) * up)
            }
        }
    }

    /// Where a frequency sits up the axis (0 its bottom, 1 its top; outside, off it).
    pub fn up(self, low: f32, high: f32, hz: f32) -> f32 {
        match self {
            FreqScale::Linear => (hz - low) / (high - low),
            FreqScale::Log => (hz / low.max(1.0)).ln() / (high / low.max(1.0)).ln(),
            FreqScale::Mel => (mel(hz) - mel(low)) / (mel(high) - mel(low)),
        }
    }
}

fn mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn unmel(m: f32) -> f32 {
    700.0 * (10f32.powf(m / 2595.0) - 1.0)
}

/// How a spectrogram's colours run from quiet to loud.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Colors {
    /// Black quiet, white loud.
    Gray,
    /// White quiet, black loud.
    GrayInverted,
    Viridis,
    Magma,
    Inferno,
    Plasma,
    Jet,
}

impl Colors {
    pub const ALL: [Colors; 7] = [
        Colors::Gray,
        Colors::GrayInverted,
        Colors::Viridis,
        Colors::Magma,
        Colors::Inferno,
        Colors::Plasma,
        Colors::Jet,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Colors::Gray => "gray",
            Colors::GrayInverted => "gray_inverted",
            Colors::Viridis => "viridis",
            Colors::Magma => "magma",
            Colors::Inferno => "inferno",
            Colors::Plasma => "plasma",
            Colors::Jet => "jet",
        }
    }

    pub fn parse(s: &str) -> Option<Colors> {
        let s = s.trim().to_lowercase().replace([' ', '-'], "_");
        Colors::ALL.into_iter().find(|c| c.name() == s)
    }
}

/// What's known of a spectrogram picture: where its plot is (x, y, width, height in pixels), how
/// long it runs, the frequencies at its bottom and top and how its rows run between them, its
/// colours, and how many dB its colours span. What's left out is found if it can be, or stays
/// unknown.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Calibration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crop: Option<[u32; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_hz: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high_hz: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<FreqScale>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<Colors>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_range: Option<f32>,
}

impl Calibration {
    /// Its frequency axis, if all of it is known.
    pub fn axis(&self) -> Option<(f32, f32, FreqScale)> {
        let (low, high) = (self.low_hz?, self.high_hz?);
        (high > low && low >= 0.0).then_some((low, high, self.scale?))
    }

    /// `over`'s values where it has them.
    pub fn over(&self, over: &Calibration) -> Calibration {
        Calibration {
            crop: over.crop.or(self.crop),
            seconds: over.seconds.or(self.seconds),
            low_hz: over.low_hz.or(self.low_hz),
            high_hz: over.high_hz.or(self.high_hz),
            scale: over.scale.or(self.scale),
            colors: over.colors.or(self.colors),
            db_range: over.db_range.or(self.db_range),
        }
    }
}

/// A pitched line that holds: from frame to frame, its pitch if the frequencies are known, and
/// where it sits up the spectrum (0 the bottom, 1 the top) either way.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Contour {
    pub start: usize,
    pub end: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hz: Option<f32>,
    pub up: f32,
    /// How loud, 0 to 1.
    pub strength: f32,
}

/// A note of a line (the bass, the melody): its frames and its pitch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LineNote {
    pub start: usize,
    pub end: usize,
    pub pitch: u8,
}

/// A section: its name and letter (the same letter, the same music), its frames, its bars if
/// they're known, how hard it plays (0 to 1), how sure its edges are, its chords (from audio),
/// and its starts folded onto a bar's sixteen steps, low, mid and high (when its bars are known).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bars: Option<usize>,
    pub energy: f32,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chords: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<[[f32; 16]; 3]>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Blueprint {
    pub version: u32,
    pub source: Source,
    pub file: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<Calibration>,
    /// How long it runs, if known; its frames are then `FPS` a second, else `SPREAD` places.
    pub seconds: Option<f32>,
    pub frames: usize,
    /// Each band's centre, Hz; empty when a picture's frequencies aren't known (its bands are
    /// then its rows, low to high).
    pub bands_hz: Vec<f32>,
    /// Each frame's level in each band, dB under the loudest (down to `FLOOR`).
    pub energy: Vec<[f32; BANDS]>,
    /// Each frame's starts, low, mid and high, 0 to 1.
    pub onsets: Vec<[f32; 3]>,
    /// Each frame's pitch classes, C first, where they can be told (else empty).
    pub chroma: Vec<[f32; 12]>,
    pub contours: Vec<Contour>,
    /// The bass and the melody, note by note, on a bar's sixteenths (from audio whose bars are
    /// known): estimates, for their shape and rhythm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bass: Vec<LineNote>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub melody: Vec<LineNote>,
    pub sections: Vec<Part>,
    pub tempo: Option<f32>,
    pub meter: Option<[u32; 2]>,
    /// Where each bar starts, in frames, when they're known.
    pub bars: Vec<f32>,
    pub key: Option<String>,
    pub facts: Vec<Fact>,
    /// What's needed to say more: a picture's missing axes, its colours.
    pub needs: Vec<String>,
}

/// How closely a song follows its reference, in one respect.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Closeness {
    /// Its spirit: the shape in broad strokes.
    Loose,
    #[default]
    Close,
    /// As near as new notes can be.
    Tight,
}

impl Closeness {
    pub const ALL: [Closeness; 3] = [Closeness::Loose, Closeness::Close, Closeness::Tight];

    pub fn name(self) -> &'static str {
        match self {
            Closeness::Loose => "loose",
            Closeness::Close => "close",
            Closeness::Tight => "tight",
        }
    }

    pub fn parse(s: &str) -> Option<Closeness> {
        Closeness::ALL
            .into_iter()
            .find(|c| c.name().eq_ignore_ascii_case(s.trim()))
    }
}

/// A song's reference, kept with it: its file and what it is, how its picture reads (if it's
/// one), and the person's choices: a style in their words, and how closely to follow its
/// rhythm, form and sound.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Like {
    pub file: PathBuf,
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<Calibration>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub style: String,
    #[serde(default)]
    pub rhythm: Closeness,
    #[serde(default)]
    pub form: Closeness,
    #[serde(default)]
    pub sound: Closeness,
}

/// The bands' edges, Hz: even steps on a log scale.
pub fn band_edges() -> [f32; BANDS + 1] {
    std::array::from_fn(|i| LOW_HZ * (HIGH_HZ / LOW_HZ).powf(i as f32 / BANDS as f32))
}

/// The bands' centres, Hz.
pub fn band_centres() -> Vec<f32> {
    let e = band_edges();
    (0..BANDS).map(|b| (e[b] * e[b + 1]).sqrt()).collect()
}

/// The broad range a frequency is in: low, mid or high.
pub fn range_of(hz: f32) -> usize {
    match hz {
        h if h < LOW_MID => 0,
        h if h < MID_HIGH => 1,
        _ => 2,
    }
}

pub const RANGES: [&str; 3] = ["low", "mid", "high"];

impl Blueprint {
    /// When a frame is, in seconds, if its length is known.
    pub fn time(&self, frame: usize) -> Option<f32> {
        self.seconds.map(|_| frame as f32 / FPS)
    }

    /// Where a frame is, 0 its start to 1 its end.
    pub fn place(&self, frame: f32) -> f32 {
        frame / self.frames.max(1) as f32
    }

    /// Each band's broad range: by its frequency, or (frequencies unknown) by thirds up the
    /// picture.
    pub fn band_ranges(&self) -> [usize; BANDS] {
        std::array::from_fn(|b| match self.bands_hz.get(b) {
            Some(&hz) => range_of(hz),
            None => b * 3 / BANDS,
        })
    }

    /// The broad balance over frames: each range's level against the others', dB (their mean
    /// taken out, so it says the shape and not how loud).
    pub fn balance(&self, from: usize, to: usize) -> [f32; 3] {
        let ranges = self.band_ranges();
        let rows = &self.energy[from.min(self.energy.len())..to.min(self.energy.len())];
        let mut sum = [0f64; 3];
        let mut n = [0usize; 3];
        for row in rows {
            for (b, &v) in row.iter().enumerate() {
                if v > FLOOR {
                    sum[ranges[b]] += 10f64.powf(v as f64 / 10.0);
                }
                n[ranges[b]] += 1;
            }
        }
        let db: [f32; 3] = std::array::from_fn(|r| {
            (10.0 * (sum[r] / n[r].max(1) as f64).max(1e-9).log10()) as f32
        });
        let mean = db.iter().sum::<f32>() / 3.0;
        db.map(|v| v - mean)
    }

    /// How loud frames are, dB under the loudest: the power in all bands.
    pub fn level(&self, from: usize, to: usize) -> f32 {
        let rows = &self.energy[from.min(self.energy.len())..to.min(self.energy.len())];
        let p: f64 = rows
            .iter()
            .flat_map(|r| r.iter())
            .map(|&v| 10f64.powf(v as f64 / 10.0))
            .sum::<f64>()
            / (rows.len().max(1) * BANDS) as f64;
        (10.0 * p.max(1e-9).log10()) as f32
    }

    /// Starts in each range over frames, each a second (or, its length unknown, each hundredth
    /// of it): frames past a threshold that start a run.
    pub fn density(&self, from: usize, to: usize) -> [f32; 3] {
        let rows = &self.onsets[from.min(self.onsets.len())..to.min(self.onsets.len())];
        let per = match self.seconds {
            Some(_) => rows.len() as f32 / FPS,
            None => rows.len() as f32 / self.frames.max(1) as f32 * 100.0,
        };
        std::array::from_fn(|r| {
            let hits = (0..rows.len())
                .filter(|&i| rows[i][r] > 0.3 && (i == 0 || rows[i - 1][r] <= 0.3))
                .count();
            hits as f32 / per.max(1e-3)
        })
    }

    /// Pitch classes over frames, summing to 1 (none when they can't be told).
    pub fn pitch_classes(&self, from: usize, to: usize) -> Option<[f32; 12]> {
        let rows = &self.chroma[from.min(self.chroma.len())..to.min(self.chroma.len())];
        if rows.is_empty() {
            return None;
        }
        let mut acc = [0f32; 12];
        for r in rows {
            let t: f32 = r.iter().sum::<f32>().max(1e-9);
            for k in 0..12 {
                acc[k] += r[k] / t;
            }
        }
        let t: f32 = acc.iter().sum::<f32>().max(1e-9);
        Some(acc.map(|v| v / t))
    }

    /// The share of frames a held pitched line sounds in.
    pub fn held(&self, from: usize, to: usize) -> f32 {
        let span = to.saturating_sub(from).max(1);
        let mut on = vec![false; span];
        for c in &self.contours {
            for f in c.start.max(from)..c.end.min(to) {
                on[f - from] = true;
            }
        }
        on.iter().filter(|&&b| b).count() as f32 / span as f32
    }

    /// What it is, briefly, for an agent writing a song like it: what's known and how, its
    /// sections with their bars, energy, chords (from audio), rhythm and balance, and what it
    /// can't say. The frames themselves stay in the cache.
    pub fn brief(&self) -> Value {
        let note = |hz: f32| crate::samples::note_name(hz_to_midi(hz));
        let sections: Vec<Value> = self
            .sections
            .iter()
            .map(|p| {
                let balance = self.balance(p.start, p.end);
                let density = self.density(p.start, p.end);
                let pitched: Vec<String> = {
                    let mut lines: Vec<&Contour> = self
                        .contours
                        .iter()
                        .filter(|c| c.end > p.start && c.start < p.end)
                        .collect();
                    lines.sort_by(|a, b| {
                        ((b.end - b.start) as f32 * b.strength)
                            .total_cmp(&((a.end - a.start) as f32 * a.strength))
                    });
                    lines
                        .iter()
                        .take(4)
                        .map(|c| match c.hz {
                            Some(hz) => format!("{} ({:.0} Hz)", note(hz), hz),
                            None => format!("{:.0}% up the picture", c.up * 100.0),
                        })
                        .collect()
                };
                let mut s = json!({
                    "name": p.name,
                    "letter": p.label,
                    "energy": (p.energy * 100.0).round() / 100.0,
                    "sure": (p.confidence * 100.0).round() / 100.0,
                    "starts_per_second": {
                        "low": round1(density[0]), "mid": round1(density[1]), "high": round1(density[2])
                    },
                    "balance_db": {
                        "low": round1(balance[0]), "mid": round1(balance[1]), "high": round1(balance[2])
                    },
                    "held_lines": round2(self.held(p.start, p.end)),
                    "lines": pitched,
                });
                match self.time(p.start) {
                    Some(t) => {
                        s["start_s"] = json!(round1(t));
                        s["end_s"] = json!(round1(p.end as f32 / FPS));
                    }
                    None => {
                        s["from"] = json!(round2(self.place(p.start as f32)));
                        s["to"] = json!(round2(self.place(p.end as f32)));
                    }
                }
                if let Some(b) = p.bars {
                    s["bars"] = json!(b);
                }
                if !p.chords.is_empty() {
                    s["chords_heard"] = json!(p.chords);
                }
                // Its opening bars' bass and melody, as notes and how many sixteenths each lasts.
                if let Some(step) = self.step_frames(p.start) {
                    for (key, line, bars) in [("bass", &self.bass, 2.0), ("melody", &self.melody, 4.0)] {
                        let phrase = phrase(line, p.start, (p.start as f32 + step * 16.0 * bars) as usize, step);
                        if !phrase.is_empty() {
                            s[key] = json!(phrase);
                        }
                    }
                }
                if let Some(g) = &p.grid {
                    s["steps"] = json!({
                        "low": steps(&g[0]), "mid": steps(&g[1]), "high": steps(&g[2])
                    });
                }
                if let Some(pc) = self.pitch_classes(p.start, p.end) {
                    let mut top: Vec<(usize, f32)> = pc.iter().copied().enumerate().collect();
                    top.sort_by(|a, b| b.1.total_cmp(&a.1));
                    s["pitch_classes"] = json!(
                        top.iter()
                            .take(4)
                            .map(|(k, _)| NAMES[*k])
                            .collect::<Vec<_>>()
                    );
                }
                s
            })
            .collect();
        let facts: Vec<Value> = self
            .facts
            .iter()
            .map(|f| {
                json!({
                    "what": f.name, "value": f.value,
                    "how": match f.basis { Basis::Measured => "measured", Basis::Estimated => "estimated", Basis::Unknown => "unknown" },
                    "sure": round2(f.confidence), "from": f.from.name()
                })
            })
            .collect();
        json!({
            "source": self.source.name(),
            "file": self.file,
            "seconds": self.seconds.map(round1),
            "tempo": self.tempo.map(round1),
            "meter": self.meter.map(|[a, b]| format!("{a}/{b}")),
            "key": self.key,
            "sections": sections,
            "facts": facts,
            "needs": self.needs,
            "steps_mean": "steps: a bar's sixteen steps, how often each starts something (0-9), low (kick, bass), mid (snare, claps, keys), high (hats, shakers)",
            "balance_mean": "balance_db: each range against the others, dB",
            "bass_mean": "the reference's bass through a section's first two bars, estimated: note:sixteenths, - a rest; its shape and rhythm to write your own on, not to copy",
            "melody_mean": "the strongest line in its singing range (200 Hz to 1.2 kHz) through a section's first four bars, estimated as the bass is: its contour and rhythm, in new notes",
            "chords_heard_mean": "a machine's guesses at a section's chords from the audio, often wrong: take their mood and how often they change, not the chords",
        })
    }
}

impl Blueprint {
    /// A sixteenth's length in frames, at the bar a frame is in (none without bars).
    pub fn step_frames(&self, frame: usize) -> Option<f32> {
        let i = self.bars.iter().rposition(|&b| b <= frame as f32 + 0.5)?;
        let len = self
            .bars
            .get(i + 1)
            .map(|n| n - self.bars[i])
            .or_else(|| (i > 0).then(|| self.bars[i] - self.bars[i - 1]))?;
        let steps = self.meter.map_or(16, |[n, d]| (n * 16 / d.max(1)).max(1));
        Some(len / steps as f32)
    }
}

impl Blueprint {
    /// A line's pitch classes between frames, by how long each sounds, summing to 1.
    pub fn line_classes(&self, line: &[LineNote], from: usize, to: usize) -> Option<[f32; 12]> {
        let mut acc = [0f32; 12];
        for n in line {
            let (a, z) = (n.start.max(from), n.end.min(to));
            if z > a {
                acc[n.pitch as usize % 12] += (z - a) as f32;
            }
        }
        let t: f32 = acc.iter().sum();
        (t > 0.0).then(|| acc.map(|v| v / t))
    }

    /// Where a line's notes start on a bar's sixteen steps, between frames: how often each
    /// step starts one, the most 1.
    pub fn line_steps(&self, line: &[LineNote], from: usize, to: usize) -> Option<[f32; 16]> {
        let mut acc = [0f32; 16];
        for n in line.iter().filter(|n| n.start >= from && n.start < to) {
            let bar = self.bars.iter().rposition(|&b| b <= n.start as f32 + 0.5)?;
            let step = self.step_frames(n.start)?;
            let k = ((n.start as f32 - self.bars[bar]) / step).round() as usize;
            acc[k.min(15)] += 1.0;
        }
        let top = acc.iter().copied().fold(0f32, f32::max);
        (top > 0.0).then(|| acc.map(|v| v / top))
    }
}

/// A line's notes between frames, as "D2:4 -:2 F#2:2": each note and how many sixteenths it
/// lasts, rests between.
fn phrase(line: &[LineNote], from: usize, to: usize, step: f32) -> String {
    let steps = |f: usize| ((f as f32 / step).round() as usize).max(1);
    let mut out = Vec::new();
    let mut at = from;
    for n in line.iter().filter(|n| n.end > from && n.start < to) {
        let (a, z) = (n.start.max(from), n.end.min(to));
        if a > at + (step * 0.5) as usize {
            out.push(format!("-:{}", steps(a - at)));
        }
        out.push(format!(
            "{}:{}",
            crate::samples::note_name(n.pitch),
            steps(z - a)
        ));
        at = z;
    }
    out.join(" ")
}

const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Rounded for reading, as JSON writes it (an `f32` would carry its binary tail).
fn round1(v: f32) -> f64 {
    (v as f64 * 10.0).round() / 10.0
}

fn round2(v: f32) -> f64 {
    (v as f64 * 100.0).round() / 100.0
}

/// A bar's steps as digits, how often each starts something: "9002 0090 9020 0090".
fn steps(g: &[f32; 16]) -> String {
    g.chunks(4)
        .map(|c| {
            c.iter()
                .map(|v| char::from(b'0' + (v * 9.0).round().clamp(0.0, 9.0) as u8))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn hz_to_midi(hz: f32) -> u8 {
    (69.0 + 12.0 * (hz.max(1.0) / 440.0).log2())
        .round()
        .clamp(0.0, 127.0) as u8
}

/// Starts, frame by frame, low, mid and high: where the bands in each range rise from the frame
/// before (bands outside a picture's axis, `kept` false, left out), each range against its
/// loudest (its 98th percentile). Audio and pictures are read alike this way, so their starts
/// compare.
pub fn rises(
    energy: &[[f32; BANDS]],
    ranges: &[usize; BANDS],
    kept: &[bool; BANDS],
) -> Vec<[f32; 3]> {
    let mut onsets: Vec<[f32; 3]> = (0..energy.len())
        .map(|f| {
            let mut o = [0f32; 3];
            if f > 0 {
                for b in (0..BANDS).filter(|&b| kept[b]) {
                    o[ranges[b]] += (energy[f][b] - energy[f - 1][b]).max(0.0);
                }
            }
            o
        })
        .collect();
    for r in 0..3 {
        let mut s: Vec<f32> = onsets.iter().map(|o| o[r]).collect();
        s.sort_by(f32::total_cmp);
        let scale = s.get(s.len() * 98 / 100).copied().unwrap_or(1.0).max(1e-3);
        for o in &mut onsets {
            o[r] = (o[r] / scale).clamp(0.0, 1.0);
        }
    }
    onsets
}

/// Sections found where the sound changes: the frames pooled into blocks (a second each, or a
/// sixty-fourth of a picture of no known length), each block's spectrum and starts compared with
/// its neighbours' (a checkerboard over their likeness), its edges where that peaks. Alike
/// sections share a letter; how hard each plays is its level against the loudest's.
pub fn segment(energy: &[[f32; BANDS]], onsets: &[[f32; 3]], timed: bool) -> Vec<Part> {
    let n = energy.len();
    if n == 0 {
        return Vec::new();
    }
    let block = match timed {
        true => FPS as usize,
        false => (n / 64).max(1),
    };
    let blocks = n.div_ceil(block);
    let feat: Vec<Vec<f32>> = (0..blocks)
        .map(|i| {
            let (a, z) = (i * block, ((i + 1) * block).min(n));
            let mut v = vec![0f32; BANDS + 3];
            for (f, row) in energy.iter().enumerate().take(z).skip(a) {
                for (x, e) in v.iter_mut().zip(row) {
                    *x += e;
                }
                for r in 0..3 {
                    v[BANDS + r] += onsets.get(f).map_or(0.0, |o| o[r]) * 20.0;
                }
            }
            let len = (z - a).max(1) as f32;
            v.iter_mut().for_each(|x| *x /= len);
            v
        })
        .collect();
    let like = |a: &[f32], b: &[f32]| -> f32 {
        let (ma, mb) = (mean(a), mean(b));
        let (mut ab, mut aa, mut bb) = (0f32, 0f32, 0f32);
        for (x, y) in a.iter().zip(b) {
            ab += (x - ma) * (y - mb);
            aa += (x - ma).powi(2);
            bb += (y - mb).powi(2);
        }
        let d = (aa * bb).sqrt();
        if d < 1e-6 { 1.0 } else { ab / d }
    };
    // The distance between blocks: how far apart their spectra are, in dB.
    let apart = |a: &[f32], b: &[f32]| -> f32 {
        let d: f32 = a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f32>();
        (d / a.len() as f32).sqrt()
    };
    let width = 6usize.min(blocks / 3).max(1);
    let novelty: Vec<f32> = (0..blocks)
        .map(|i| {
            if i < width || i + width > blocks {
                return 0.0;
            }
            let mut across = 0f32;
            let mut within = 0f32;
            for a in i - width..i {
                for b in i..i + width {
                    across += apart(&feat[a], &feat[b]);
                }
            }
            for a in i - width..i {
                for b in i - width..i {
                    within += apart(&feat[a], &feat[b]);
                }
            }
            for a in i..i + width {
                for b in i..i + width {
                    within += apart(&feat[a], &feat[b]);
                }
            }
            let w2 = (width * width) as f32;
            (across / w2 - within / (2.0 * w2)).max(0.0)
        })
        .collect();
    let m = mean(&novelty);
    let sd = (novelty.iter().map(|v| (v - m).powi(2)).sum::<f32>() / blocks as f32).sqrt();
    let least = match timed {
        true => 8usize,
        false => 5,
    };
    let mut edges: Vec<usize> = Vec::new();
    let mut order: Vec<usize> = (0..blocks).collect();
    order.sort_by(|&a, &b| novelty[b].total_cmp(&novelty[a]));
    for i in order {
        if novelty[i] <= m + 0.5 * sd || novelty[i] < 1.5 {
            break;
        }
        if i < least || blocks - i < least || edges.iter().any(|&e| e.abs_diff(i) < least) {
            continue;
        }
        edges.push(i);
        if edges.len() >= 11 {
            break;
        }
    }
    edges.sort();
    let mut starts = vec![0usize];
    starts.extend(edges.iter().map(|&e| e * block));
    let ends: Vec<usize> = starts.iter().skip(1).copied().chain([n]).collect();
    let spans: Vec<(usize, usize)> = starts.into_iter().zip(ends).collect();
    let sure = (0.35 + 0.1 * (sd / m.max(1e-3))).clamp(0.3, 0.7);
    parts_of(energy, &spans, |a, b| like(a, b), sure)
}

/// Spans made sections: lettered by likeness (their mean spectra), each with its energy.
pub fn parts_of(
    energy: &[[f32; BANDS]],
    spans: &[(usize, usize)],
    like: impl Fn(&[f32], &[f32]) -> f32,
    sure: f32,
) -> Vec<Part> {
    let profile = |a: usize, z: usize| -> Vec<f32> {
        let mut v = vec![0f32; BANDS];
        for row in &energy[a.min(energy.len())..z.min(energy.len())] {
            for b in 0..BANDS {
                v[b] += row[b];
            }
        }
        let len = z.saturating_sub(a).max(1) as f32;
        v.iter_mut().for_each(|x| *x /= len);
        v
    };
    let levels: Vec<f32> = spans
        .iter()
        .map(|&(a, z)| {
            let rows = &energy[a.min(energy.len())..z.min(energy.len())];
            let p: f64 = rows
                .iter()
                .flat_map(|r| r.iter())
                .map(|&v| 10f64.powf(v as f64 / 10.0))
                .sum::<f64>()
                / (rows.len().max(1) * BANDS) as f64;
            (10.0 * p.max(1e-9).log10()) as f32
        })
        .collect();
    let top = levels.iter().copied().fold(f32::MIN, f32::max);
    let profiles: Vec<Vec<f32>> = spans.iter().map(|&(a, z)| profile(a, z)).collect();
    let mut labels: Vec<char> = Vec::new();
    let mut next = b'A';
    for i in 0..spans.len() {
        let same = (0..i).find(|&j| {
            like(&profiles[i], &profiles[j]) > 0.95 && (levels[i] - levels[j]).abs() < 3.0
        });
        labels.push(match same {
            Some(j) => labels[j],
            None => {
                let c = next as char;
                next = next.saturating_add(1).min(b'Z');
                c
            }
        });
    }
    let last = spans.len().saturating_sub(1);
    spans
        .iter()
        .enumerate()
        .map(|(i, &(a, z))| {
            let energy = ((levels[i] - (top - 18.0)) / 18.0).clamp(0.05, 1.0);
            let name = match i {
                0 if spans.len() > 2 && energy < 0.6 => "Intro".to_string(),
                i if i == last && spans.len() > 2 && energy < 0.6 => "Outro".to_string(),
                _ => format!("Part {}", labels[i]),
            };
            Part {
                name,
                label: labels[i].to_string(),
                start: a,
                end: z,
                bars: None,
                energy,
                confidence: sure,
                chords: String::new(),
                grid: None,
            }
        })
        .collect()
}

fn mean(v: &[f32]) -> f32 {
    v.iter().sum::<f32>() / v.len().max(1) as f32
}

/// A section's starts folded onto a bar's sixteen steps, low, mid and high: fine starts (`rate`
/// a second) over the bars (seconds) that fall in it, each step's mean, the strongest 1.
pub fn grid(
    fine: &[[f32; 3]],
    rate: f32,
    bars: &[f32],
    from: f32,
    to: f32,
) -> Option<[[f32; 16]; 3]> {
    let mut sum = [[0f32; 16]; 3];
    let mut n = 0;
    for w in bars.windows(2) {
        let (a, z) = (w[0], w[1]);
        if a < from - 0.05 || z > to + 0.05 {
            continue;
        }
        n += 1;
        for step in 0..16 {
            let t0 = a + (z - a) * step as f32 / 16.0;
            let t1 = a + (z - a) * (step as f32 + 1.0) / 16.0;
            // Around the step's start, where its hit lands: a little before to most of it.
            let lead = (t1 - t0) * 0.3;
            let (i0, i1) = (
                ((t0 - lead).max(0.0) * rate) as usize,
                ((t0 + (t1 - t0) * 0.6) * rate) as usize,
            );
            for (r, row) in sum.iter_mut().enumerate() {
                row[step] += fine[i0.min(fine.len())..i1.min(fine.len())]
                    .iter()
                    .map(|o| o[r])
                    .fold(0f32, f32::max);
            }
        }
    }
    if n == 0 {
        return None;
    }
    Some(sum.map(|row| {
        let top = row.iter().copied().fold(0f32, f32::max).max(1e-6);
        row.map(|v| v / top)
    }))
}
