//! Bars copied, and sounds for the moves between sections (risers, reverse cymbals, impacts,
//! downlifters, gaps), made here so they don't need a sample library. The arrangement itself is
//! the agent's to write.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    music::look::{self, Family},
    song::project::{AUDIO_DIR, ChordSpan, Content, Instrument, Note, Project, Region, Track},
};

/// What part a track plays in a song: what it's called, else what's written on it, else what
/// it sounds like.
pub fn role(t: &Track) -> &'static str {
    let name = t.name.to_lowercase();
    let says = |words: &[&str]| {
        words.iter().any(|w| {
            name.split(|c: char| !c.is_alphanumeric()).any(|x| x == *w)
                || name.contains(w) && w.len() > 3
        })
    };
    const BY_NAME: &[(&str, &[&str])] = &[
        (
            "fx",
            &["fx", "sfx", "riser", "impact", "sweep", "transition"],
        ),
        (
            "perc",
            &[
                "perc",
                "percussion",
                "conga",
                "bongo",
                "shaker",
                "tamb",
                "cowbell",
                "taiko",
                "timpani",
                "toms",
            ],
        ),
        (
            "drums",
            &[
                "drums", "drum", "kit", "beat", "kick", "snare", "hats", "hat", "clap", "break",
            ],
        ),
        ("bass", &["bass", "808", "sub", "log"]),
        ("vocals", &["vocal", "vocals", "vox", "voice", "choir"]),
        (
            "pad",
            &["pad", "pads", "strings", "string", "atmos", "atmosphere"],
        ),
        ("arp", &["arp", "arpeggio", "sequence"]),
        ("counter", &["counter"]),
        (
            "chords",
            &[
                "chords", "chord", "keys", "piano", "rhodes", "ep", "organ", "guitar", "stab",
                "stabs",
            ],
        ),
        (
            "lead",
            &[
                "lead", "melody", "hook", "bell", "bells", "pluck", "flute", "horn", "sax",
                "trumpet",
            ],
        ),
        (
            "texture",
            &["texture", "vinyl", "noise", "rain", "ambience", "foley"],
        ),
    ];
    if let Some((role, _)) = BY_NAME.iter().find(|(_, words)| says(words)) {
        return role;
    }
    // What Pina Studio wrote on it.
    match t.regions.first().map(|r| r.name.as_str()) {
        Some("Chords") => return "chords",
        Some("Bass") => return "bass",
        Some("Melody") => return "lead",
        Some("Counter") => return "counter",
        Some("Arp") => return "arp",
        Some("Drums") => return "drums",
        _ => {}
    }
    if t.instrument.as_ref().is_some_and(Instrument::is_kit) {
        return "drums";
    }
    match look::look(t, &|_| None).family {
        Family::Drums => "drums",
        Family::Percussion => "perc",
        Family::Bass => "bass",
        Family::Keys | Family::Guitar => "chords",
        Family::Strings | Family::Pad => "pad",
        Family::Vocals => "vocals",
        Family::Fx => "fx",
        Family::Texture => "texture",
        Family::Brass | Family::Woodwind | Family::Synth | Family::Bells => "lead",
        Family::Other => "other",
    }
}

/// The part of a region from `from` to `to` (song ticks), as a region of its own that plays just
/// what it did there: notes moved, a pattern's steps turned so they line up, audio's offset on.
/// None if it has nothing there.
pub fn clip(p: &Project, r: &Region, from: u64, to: u64) -> Option<Region> {
    let (s, e) = (r.start.max(from), (r.start + r.length).min(to));
    if s >= e {
        return None;
    }
    let skip = s - r.start;
    let length = e - s;
    let content = match &r.content {
        Content::Midi { notes, controls } => Content::Midi {
            notes: notes
                .iter()
                .filter(|n| n.start >= skip as i64 && n.start < (skip + length) as i64)
                .map(|n| Note {
                    start: n.start - skip as i64,
                    ..*n
                })
                .collect(),
            // ponytail: controller moves are cut, not chased to where they stood at `from`.
            controls: controls
                .iter()
                .filter(|c| c.start >= skip as i64 && c.start < (skip + length) as i64)
                .map(|c| crate::song::project::ControlPoint {
                    start: c.start - skip as i64,
                    ..*c
                })
                .collect(),
        },
        Content::Pattern(p) => {
            // Its rows turned to where the cut falls, so it plays on from there.
            let mut p = p.clone();
            let steps = p.steps();
            let turn = ((skip / p.step.max(1)) as usize % steps) as i32;
            for row in &mut p.rows {
                row.rotate(steps, -turn, None);
            }
            Content::Pattern(p)
        }
        Content::Audio { .. } => {
            let mut c = r.content.clone();
            let frame = p.frame_at(&r.content, r.start as f64, (r.start + skip) as f64);
            if let Content::Audio { offset: o, .. } = &mut c {
                *o = frame.unwrap_or(*o);
            }
            c
        }
        // A take folder is kept whole or not at all.
        Content::Takes { .. } if skip == 0 && length == r.length => r.content.clone(),
        Content::Takes { .. } => return None,
    };
    Some(Region {
        start: s,
        length,
        link: None,
        looped: false,
        content,
        ..r.clone()
    })
}

/// Takes `from..to` out of a track: regions inside go, regions across its edges are cut back to
/// what's outside it.
pub fn clear(p: &mut Project, t: usize, from: u64, to: u64) {
    let regions = std::mem::take(&mut p.tracks[t].regions);
    let mut kept = Vec::new();
    for r in regions {
        let end = r.start + r.length;
        if end <= from || r.start >= to {
            kept.push(r);
            continue;
        }
        kept.extend(clip(p, &r, r.start, from));
        kept.extend(clip(p, &r, to, end));
    }
    p.tracks[t].regions = kept;
}

/// A track's regions from `from` for `length` ticks, cut to fit, their starts made relative.
fn take(p: &Project, t: usize, from: u64, length: u64) -> Vec<Region> {
    p.tracks[t]
        .regions
        .iter()
        .filter_map(|r| clip(p, r, from, from + length))
        .map(|r| Region {
            start: r.start - from,
            ..r
        })
        .collect()
}

/// Lays `pieces` (relative starts, a loop `length` long) from `at` to `end`, looping, the last
/// time cut short.
fn lay(
    p: &mut Project,
    t: usize,
    pieces: &[Region],
    length: u64,
    at: u64,
    end: u64,
    velocity: i8,
) -> usize {
    let mut placed = 0;
    let mut k = at;
    while k < end {
        for r in pieces {
            let moved = Region {
                start: k + r.start,
                ..r.clone()
            };
            if let Some(mut r) = clip(p, &moved, k, end) {
                r.velocity = (r.velocity as i16 + velocity as i16).clamp(-64, 64) as i8;
                p.tracks[t].regions.push(r);
                placed += 1;
            }
        }
        k += length.max(1);
    }
    placed
}

/// The chord track's chords from `from` for `length`, relative.
fn take_chords(p: &Project, from: u64, length: u64) -> Vec<ChordSpan> {
    p.chords
        .iter()
        .filter(|c| c.start < from + length && c.start + c.length > from)
        .map(|c| {
            let s = c.start.max(from);
            let e = (c.start + c.length).min(from + length);
            ChordSpan {
                start: s - from,
                length: e - s,
                chord: c.chord.clone(),
            }
        })
        .collect()
}

fn lay_chords(p: &mut Project, chords: &[ChordSpan], length: u64, at: u64, end: u64) {
    p.chords
        .retain(|c| c.start + c.length <= at || c.start >= end);
    let mut k = at;
    while k < end {
        for c in chords {
            let s = k + c.start;
            if s < end {
                p.chords.push(ChordSpan {
                    start: s,
                    length: c.length.min(end - s),
                    chord: c.chord.clone(),
                });
            }
        }
        k += length.max(1);
    }
    p.chords.sort_by_key(|c| c.start);
}

/// Copies bars `from..from+bars` of some tracks (all by default) to `to`, replacing what's there.
pub fn copy_bars(
    p: &mut Project,
    from: u64,
    bars: u64,
    to: u64,
    tracks: Option<Vec<usize>>,
) -> Result<Value, String> {
    let length = bars.max(1) * p.bar();
    let tracks = tracks.unwrap_or_else(|| (0..p.tracks.len()).collect());
    if let Some(t) = tracks.iter().find(|&&t| t >= p.tracks.len()) {
        return Err(format!("no track {t}"));
    }
    let mut placed = 0;
    for &t in &tracks {
        let pieces = take(p, t, from, length);
        clear(p, t, to, to + length);
        placed += lay(p, t, &pieces, length, to, to + length, 0);
    }
    let chords = take_chords(p, from, length);
    if !chords.is_empty() {
        lay_chords(p, &chords, length, to, to + length);
    }
    Ok(json!({ "regions": placed }))
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Move {
    /// Noise and a tone sweeping up, ending on the bar.
    Riser,
    /// A cymbal played backwards, swelling into the bar.
    Reverse,
    /// A low boom on the bar.
    Impact,
    /// A sweep falling away from the bar.
    Downlifter,
    /// Everything but the FX stops for the beat before the bar.
    Gap,
    /// A riser, a beat's gap and an impact: into a drop or chorus.
    Build,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    /// The downbeat it leads into (ticks): a section's start.
    pub at: u64,
    #[serde(rename = "kind")]
    pub kind: Move,
    /// How many bars a riser, reverse or downlifter lasts (2, 1, 2 by default); a gap's beats
    /// (1).
    #[serde(default)]
    pub length: Option<u64>,
    /// Where the FX go ("FX", made if there's none).
    #[serde(default)]
    pub track: Option<usize>,
}

/// The track the FX go on: one called FX that plays audio, made at the end if there's none.
fn fx_track(p: &mut Project) -> usize {
    if let Some(t) = p
        .tracks
        .iter()
        .position(|t| t.instrument.is_none() && role(t) == "fx")
    {
        return t;
    }
    let at = p.tracks.len();
    p.add_track(
        at,
        Track {
            name: "FX".into(),
            volume_db: -6.0,
            ..Default::default()
        },
    );
    at
}

pub fn transition(p: &mut Project, dir: &Path, tr: Transition) -> Result<Value, String> {
    let (bar, beat) = (p.bar(), p.beat());
    let at = tr.at;
    let mut done = Vec::new();
    let kinds: &[Move] = match tr.kind {
        Move::Build => &[Move::Riser, Move::Gap, Move::Impact],
        k => std::slice::from_ref(match k {
            Move::Riser => &Move::Riser,
            Move::Reverse => &Move::Reverse,
            Move::Impact => &Move::Impact,
            Move::Downlifter => &Move::Downlifter,
            _ => &Move::Gap,
        }),
    };
    for &kind in kinds {
        let given = if tr.kind == Move::Build {
            None
        } else {
            tr.length
        };
        match kind {
            Move::Gap => {
                let beats = given.unwrap_or(1).max(1);
                let from = at.saturating_sub(beats * beat);
                for t in 0..p.tracks.len() {
                    if role(&p.tracks[t]) != "fx" {
                        clear(p, t, from, at);
                    }
                }
                done.push(format!("{beats} beat gap"));
            }
            sound => {
                let bars = given.unwrap_or(match sound {
                    Move::Reverse => 1,
                    Move::Impact => 2,
                    _ => 2,
                });
                let (start, length) = match sound {
                    Move::Riser | Move::Reverse => {
                        (at.saturating_sub(bars * bar), (bars * bar).min(at))
                    }
                    _ => (at, bars * bar),
                };
                if length == 0 {
                    return Err("a riser needs bars before it: at is the song's start".into());
                }
                let file = make_fx(p, dir, sound, length)?;
                let t = match tr.track.filter(|_| tr.kind != Move::Build) {
                    Some(t) if t < p.tracks.len() && p.tracks[t].instrument.is_none() => t,
                    Some(t) => return Err(format!("track {t} isn't an audio track for FX")),
                    None => fx_track(p),
                };
                p.tracks[t].regions.push(Region {
                    name: format!("{sound:?}"),
                    start,
                    length,
                    content: Content::Audio {
                        file,
                        offset: 0,
                        stretch: 1.0,
                        stretch_mode: Default::default(),
                        follow: crate::song::project::Follow::Off,
                        tempo: None,
                        grid: None,
                        formants: false,
                        warps: Vec::new(),
                    },
                    ..Default::default()
                });
                done.push(format!(
                    "{} on {}",
                    format!("{sound:?}").to_lowercase(),
                    p.tracks[t].name
                ));
            }
        }
    }
    Ok(json!({ "done": done }))
}

/// Makes a transition's sound, `ticks` long at the song's tempo, in the project's audio folder
/// (once: the same sound and length is the same file).
fn make_fx(p: &Project, dir: &Path, kind: Move, ticks: u64) -> Result<PathBuf, String> {
    let rate = p.sample_rate;
    let frames = p.samples(ticks, rate as f64) as usize;
    let name = format!(
        "{AUDIO_DIR}/pina {} {}.wav",
        format!("{kind:?}").to_lowercase(),
        frames
    );
    let path = dir.join(&name);
    if path.exists() {
        return Ok(PathBuf::from(name));
    }
    let [l, r] = synth_fx(kind, frames, rate as f32);
    std::fs::create_dir_all(dir.join(AUDIO_DIR)).map_err(|e| e.to_string())?;
    crate::music::analysis::write_wav(&path, &l, &r, rate)?;
    Ok(PathBuf::from(name))
}

/// A state-variable filter (Simper's): low, band and high pass at once.
#[derive(Default, Clone, Copy)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    fn run(&mut self, x: f32, hz: f32, q: f32, rate: f32) -> (f32, f32, f32) {
        let g = (std::f32::consts::PI * hz.clamp(20.0, rate * 0.45) / rate).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let (a2, a3) = (g * a1, g * g * a1);
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - k * v1 - v2)
    }
}

/// White noise, the same every time.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// A transition's sound, `frames` long, peaking at -3 dBFS.
fn synth_fx(kind: Move, frames: usize, rate: f32) -> [Vec<f32>; 2] {
    let mut out = [vec![0f32; frames], vec![0f32; frames]];
    let (mut nl, mut nr) = (Noise(0x1234_5678), Noise(0x9abc_def1));
    let (mut fl, mut fr, mut tone_f) = (Svf::default(), Svf::default(), Svf::default());
    let n = frames.max(1) as f32;
    let mut phase = 0f32;
    let [out_l, out_r] = &mut out;
    for (i, (ol, or)) in out_l.iter_mut().zip(out_r.iter_mut()).enumerate() {
        let x = i as f32 / n;
        let secs = i as f32 / rate;
        let (a, b) = (nl.next(), nr.next());
        let (l, r) = match kind {
            Move::Riser => {
                // Band-passed noise rising 200 Hz to 12 kHz, widening, with a saw sweeping up
                // two octaves under it.
                let hz = 200.0 * 60f32.powf(x * x);
                let width = 0.2 + 0.8 * x;
                let mid = (a + b) * 0.5;
                let (_, bl, _) = fl.run(mid + (a - mid) * width, hz, 2.5, rate);
                let (_, br, _) = fr.run(mid + (b - mid) * width, hz, 2.5, rate);
                phase = (phase + 110.0 * 4f32.powf(x) / rate).fract();
                let (saw, _, _) = tone_f.run(phase * 2.0 - 1.0, 400.0 + 4000.0 * x, 0.7, rate);
                let env = x.powf(2.2);
                ((bl * 1.6 + saw * 0.25) * env, (br * 1.6 + saw * 0.25) * env)
            }
            Move::Downlifter => {
                let hz = 9000.0 * (1.0 / 45.0f32).powf(x.sqrt());
                let (_, bl, _) = fl.run(a, hz, 2.0, rate);
                let (_, br, _) = fr.run(b, hz, 2.0, rate);
                let env = (1.0 - x).powf(2.0);
                (bl * env * 1.6, br * env * 1.6)
            }
            Move::Reverse => {
                // A cymbal's wash (high-passed noise decaying), heard backwards: so it swells.
                let back = 1.0 - x;
                let (_, _, hl) = fl.run(a, 5000.0, 0.7, rate);
                let (_, _, hr) = fr.run(b, 5000.0, 0.7, rate);
                let env = (-back * 5.0).exp() * x.min(0.98).powf(0.3) * (1.0 - x).min(0.02) * 50.0;
                (hl * env, hr * env)
            }
            _ => {
                // An impact: a sine dropping 70 to 32 Hz, a short noise thump, and a dark tail.
                let hz = 32.0 + 38.0 * (-secs * 6.0).exp();
                phase = (phase + hz / rate).fract();
                let boom = (phase * std::f32::consts::TAU).sin() * (-secs * 1.4).exp();
                let (ll, _, _) = fl.run(a, 900.0 * (-secs * 3.0).exp() + 120.0, 0.7, rate);
                let (lr, _, _) = fr.run(b, 900.0 * (-secs * 3.0).exp() + 120.0, 0.7, rate);
                let thump = (-secs * 9.0).exp() * 1.4 + (-secs * 1.2).exp() * 0.25;
                (boom + ll * thump, boom + lr * thump)
            }
        };
        (*ol, *or) = (l, r);
    }
    let peak = out.iter().flatten().fold(0f32, |m, v| m.max(v.abs()));
    if peak > 0.0 {
        let gain = 0.708 / peak;
        out.iter_mut().flatten().for_each(|v| *v *= gain);
    }
    out
}

#[cfg(test)]
mod tests;
