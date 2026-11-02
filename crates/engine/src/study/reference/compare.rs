//! A song compared with its reference, section by section, as music rather than pixels: where its
//! parts start (the groove), its pitch classes and held lines (tonal), its balance low to high,
//! and how hard each section plays against the rest (energy). Sections are matched one to one
//! when the song has as many as the reference, else by where they fall in time. What a picture
//! can't say (balance with no frequency axis) isn't compared; a picture's levels are relative,
//! as every measure here is. The biggest differences come first, each saying which parts to
//! change; a revision asked for in words is aimed at the measures and sections it names, and
//! judged by whether they moved.

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    Groove,
    Tonal,
    Balance,
    Energy,
}

impl Measure {
    pub const ALL: [Measure; 4] = [
        Measure::Groove,
        Measure::Tonal,
        Measure::Balance,
        Measure::Energy,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Measure::Groove => "groove",
            Measure::Tonal => "tonal",
            Measure::Balance => "balance",
            Measure::Energy => "energy",
        }
    }
}

/// How alike, 0 to 1, by each measure; none where it can't be told.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Scores {
    pub groove: Option<f32>,
    pub tonal: Option<f32>,
    pub balance: Option<f32>,
    pub energy: Option<f32>,
}

impl Scores {
    pub fn get(&self, m: Measure) -> Option<f32> {
        match m {
            Measure::Groove => self.groove,
            Measure::Tonal => self.tonal,
            Measure::Balance => self.balance,
            Measure::Energy => self.energy,
        }
    }

    fn set(&mut self, m: Measure, v: Option<f32>) {
        match m {
            Measure::Groove => self.groove = v,
            Measure::Tonal => self.tonal = v,
            Measure::Balance => self.balance = v,
            Measure::Energy => self.energy = v,
        }
    }
}

/// A section of the reference against where it falls in the song.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub label: String,
    /// Its frames in the reference, and in the song.
    pub reference: (usize, usize),
    pub song: (usize, usize),
    pub scores: Scores,
    /// The song's less the reference's: each range's balance, dB; its level against its whole
    /// song, dB; starts a second, each range, as a share of the reference's.
    pub balance_db: Option<[f32; 3]>,
    pub energy_db: f32,
    pub density: [Option<f32>; 3],
}

/// A difference worth changing: in which section, by which measure, how big (0 to 1), said for
/// people, the parts to change, and what to ask the agent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Difference {
    pub section: usize,
    pub measure: Measure,
    pub size: f32,
    pub says: String,
    pub parts: Vec<String>,
    pub ask: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Report {
    /// The reference's levels are relative (a picture with no level scale).
    pub relative: bool,
    pub overall: Scores,
    pub sections: Vec<Section>,
    /// The biggest first.
    pub differences: Vec<Difference>,
}

/// The parts that make each range: low, mid, high.
const RANGE_PARTS: [&[&str]; 3] = [
    &["kick", "bass", "808"],
    &["snare", "clap", "chords", "keys"],
    &["hats", "shaker", "cymbals", "lead"],
];

/// How alike a song's render is to its reference, section by section.
pub fn compare(reference: &Blueprint, song: &Blueprint) -> Report {
    let spans = align(reference, song);
    let whole_ref = reference.level(0, reference.frames);
    let whole_song = song.level(0, song.frames);
    let freq_known = !reference.bands_hz.is_empty();
    // A picture with no level scale: its dB are the song's times some unknown factor.
    let relative = reference.source == Source::Image
        && reference
            .calibration
            .as_ref()
            .is_none_or(|c| c.db_range.is_none());
    // Starts a second where both are recordings; else each against its own whole song's: a
    // picture counts them per hundredth of it when its length isn't known, and at most one a
    // column (a few a second across a whole song) when it is.
    let timed = reference.source != Source::Image && song.source != Source::Image;
    let (ref_whole, song_whole) = (
        reference.density(0, reference.frames),
        song.density(0, song.frames),
    );
    let starts = |d: [f32; 3], whole: [f32; 3]| match timed {
        true => d,
        false => std::array::from_fn(|k| d[k] / whole[k].max(1e-3)),
    };
    let mut sections = Vec::new();
    let mut differences = Vec::new();
    for (i, (p, &(a, z))) in reference.sections.iter().zip(&spans).enumerate() {
        let (ra, rz) = (p.start, p.end.max(p.start + 1));
        let mut scores = Scores::default();
        // Energy: the section's level against its whole song's, in each.
        let energy_db = (song.level(a, z) - whole_song) - (reference.level(ra, rz) - whole_ref);
        scores.energy = Some((-energy_db.abs() / 4.0).exp());
        // Balance: only where the reference's frequencies are known.
        let balance_db = freq_known.then(|| {
            let (r, mut s) = (reference.balance(ra, rz), song.balance(a, z));
            // Relative levels: the song's shape scaled to fit the picture's (least squares), so
            // only its shape is compared.
            if relative {
                let (rs, ss): (f32, f32) = (0..3).fold((0.0, 0.0), |acc, k| {
                    (acc.0 + r[k] * s[k], acc.1 + s[k] * s[k])
                });
                let fit = (rs / ss.max(1e-6)).clamp(0.5, 1.5);
                s = s.map(|v| v * fit);
            }
            let d: [f32; 3] = std::array::from_fn(|k| s[k] - r[k]);
            let m = d.iter().sum::<f32>() / 3.0;
            d.map(|v| v - m)
        });
        if let Some(d) = balance_db {
            let rms = (d.iter().map(|v| v * v).sum::<f32>() / 3.0).sqrt();
            scores.balance = Some((-rms / 4.0).exp());
        }
        // Groove: the bar's steps where both have them, else when starts come.
        let (rd, sd) = (
            starts(reference.density(ra, rz), ref_whole),
            starts(song.density(a, z), song_whole),
        );
        let density: [Option<f32>; 3] =
            std::array::from_fn(|k| (rd[k] > 0.2).then(|| sd[k] / rd[k]));
        let song_grid = song
            .sections
            .iter()
            .find(|q| q.start <= a + 2 && q.end + 2 >= z)
            .and_then(|q| q.grid);
        let groove_by_range: Vec<(usize, f32)> = match (p.grid, song_grid) {
            (Some(rg), Some(sg)) => (0..3)
                .filter(|&k| rg[k].iter().sum::<f32>() > 1.0)
                .map(|k| (k, cosine(&rg[k], &sg[k])))
                .collect(),
            _ if freq_known => (0..3)
                .filter(|&k| rd[k] > 0.2)
                .map(|k| {
                    let ratio = density[k].unwrap_or(1.0).max(1e-3);
                    (k, (-ratio.ln().abs()).exp())
                })
                .collect(),
            _ => {
                // A picture with no frequency axis: all its starts together.
                let r: f32 = rd.iter().sum();
                let s: f32 = sd.iter().sum();
                match r > 0.2 {
                    true => vec![(1, (-(s / r).max(1e-3).ln().abs()).exp())],
                    false => Vec::new(),
                }
            }
        };
        // The melody's rhythm, where both have one: where its notes start on the bar.
        let melody_steps = (
            reference.line_steps(&reference.melody, ra, rz),
            song.line_steps(&song.melody, a, z),
        );
        let melody_rhythm = match melody_steps {
            (Some(r), Some(s)) => Some(cosine(&r, &s)),
            _ => None,
        };
        let rhythms: Vec<f32> = groove_by_range
            .iter()
            .map(|g| g.1)
            .chain(melody_rhythm)
            .collect();
        if !rhythms.is_empty() {
            scores.groove = Some(rhythms.iter().sum::<f32>() / rhythms.len() as f32);
        }
        // Tonal: pitch classes where both have them; held lines either way.
        let held = (reference.held(ra, rz), song.held(a, z));
        let tonal = match (reference.pitch_classes(ra, rz), song.pitch_classes(a, z)) {
            (Some(r), Some(s)) => {
                Some(0.75 * cosine(&r, &s) + 0.25 * (1.0 - (held.0 - held.1).abs()))
            }
            _ if !reference.contours.is_empty() => Some(1.0 - (held.0 - held.1).abs()),
            _ => None,
        };
        // The bass's and the melody's notes, where both have them.
        let line = |r: &[LineNote], s: &[LineNote]| match (
            reference.line_classes(r, ra, rz),
            song.line_classes(s, a, z),
        ) {
            (Some(x), Some(y)) => Some((cosine(&x, &y), x, y)),
            _ => None,
        };
        let lines = [
            line(&reference.bass, &song.bass),
            line(&reference.melody, &song.melody),
        ];
        let notes: Vec<f32> = tonal
            .into_iter()
            .chain(lines.iter().flatten().map(|l| l.0))
            .collect();
        scores.tonal = (!notes.is_empty()).then(|| notes.iter().sum::<f32>() / notes.len() as f32);
        let section = Section {
            name: p.name.clone(),
            label: p.label.clone(),
            reference: (ra, rz),
            song: (a, z),
            scores,
            balance_db,
            energy_db,
            density,
        };
        differences.extend(differ(i, &section, reference, song, &groove_by_range, held));
        differences.extend(differ_lines(&section.name, i, &lines, melody_steps));
        sections.push(section);
    }
    let mut overall = Scores::default();
    for m in Measure::ALL {
        let (sum, n) = sections
            .iter()
            .filter_map(|s| {
                let w = (s.reference.1 - s.reference.0) as f32;
                s.scores.get(m).map(|v| (v * w, w))
            })
            .fold((0f32, 0f32), |acc, x| (acc.0 + x.0, acc.1 + x.1));
        overall.set(m, (n > 0.0).then(|| sum / n));
    }
    differences.sort_by(|a, b| b.size.total_cmp(&a.size));
    differences.truncate(8);
    Report {
        relative,
        overall,
        sections,
        differences,
    }
}

/// Where each of the reference's sections falls in the song: section for section when it has as
/// many, else at the same time (or, a length unknown, the same place through it).
fn align(reference: &Blueprint, song: &Blueprint) -> Vec<(usize, usize)> {
    if song.sections.len() == reference.sections.len() && !song.sections.is_empty() {
        return song
            .sections
            .iter()
            .map(|s| (s.start, s.end.max(s.start + 1)))
            .collect();
    }
    let same_time = match (reference.seconds, song.seconds) {
        (Some(r), Some(s)) => (s / r - 1.0).abs() < 0.05,
        _ => false,
    };
    reference
        .sections
        .iter()
        .map(|p| {
            let at = |f: usize| match same_time {
                true => f.min(song.frames),
                false => ((reference.place(f as f32) * song.frames as f32).round() as usize)
                    .min(song.frames),
            };
            let a = at(p.start).min(song.frames.saturating_sub(1));
            (a, at(p.end).max(a + 1))
        })
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut ab, mut aa, mut bb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    match aa * bb {
        d if d < 1e-12 => (aa < 1e-12 && bb < 1e-12) as u8 as f32,
        d => (ab / d.sqrt()).clamp(0.0, 1.0),
    }
}

/// A bar's steps that start something, 1 to 16: "1, 7, 9".
fn hits(g: &[f32; 16]) -> String {
    let on: Vec<String> = (0..16)
        .filter(|&s| g[s] > 0.5)
        .map(|s| (s + 1).to_string())
        .collect();
    match on.is_empty() {
        true => "none".into(),
        false => on.join(", "),
    }
}

/// A section's differences worth changing.
fn differ(
    i: usize,
    s: &Section,
    reference: &Blueprint,
    song: &Blueprint,
    groove: &[(usize, f32)],
    held: (f32, f32),
) -> Vec<Difference> {
    let name = &s.name;
    // What a picture can't back up isn't asked to be changed (its likeness still shows): its
    // notes, read from its held lines, and its groove when it has too few pixels a second to
    // place hits (on real songs, both were mostly wrong against the same song's audio).
    let picture = reference.source == Source::Image;
    let resolves_time = match (reference.seconds, &reference.calibration) {
        (Some(secs), Some(c)) => c.crop.is_some_and(|[_, _, w, _]| w as f32 / secs >= 8.0),
        _ => !picture,
    };
    let mut out = Vec::new();
    let mut push = |measure, size: f32, says: String, parts: &[&str], ask: String| {
        out.push(Difference {
            section: i,
            measure,
            size: size.clamp(0.0, 1.0),
            says,
            parts: parts.iter().map(|p| p.to_string()).collect(),
            ask,
        })
    };
    if s.energy_db.abs() > 2.0 {
        let (softer, more) = match s.energy_db < 0.0 {
            true => (
                "softer",
                "lift it: more parts playing, fuller voicings, harder hits",
            ),
            false => (
                "harder",
                "calm it: fewer parts, sparser playing, softer hits",
            ),
        };
        push(
            Measure::Energy,
            s.energy_db.abs() / 8.0,
            format!(
                "{name} plays {:.0} dB {softer} against the rest of the song than the reference's",
                s.energy_db.abs()
            ),
            &["drums", "bass", "chords", "lead"],
            format!(
                "In {name}, {more}, so it sits {:.0} dB {} against the rest of the song.",
                s.energy_db.abs(),
                if s.energy_db < 0.0 { "higher" } else { "lower" }
            ),
        );
    }
    if let Some(d) = s.balance_db {
        for (k, &v) in d.iter().enumerate() {
            if v.abs() <= 3.0 {
                continue;
            }
            let (word, fix) = match (k, v < 0.0) {
                (0, true) => (
                    "thinner low end",
                    "more weight in the kick and bass (a lower or longer bass, a sub)",
                ),
                (0, false) => (
                    "heavier low end",
                    "less low end: shorter or quieter bass and kick, a low cut on the rest",
                ),
                (1, true) => (
                    "hollower middle",
                    "more in the mids: chords or keys, a fuller snare",
                ),
                (1, false) => (
                    "thicker middle",
                    "less in the mids: thin the chords, cut 300 Hz to 1 kHz",
                ),
                (2, true) => (
                    "darker top",
                    "more brightness: hats, a lead or pad up high, a high shelf",
                ),
                (_, _) => ("brighter top", "less top: darker hats and lead, a high cut"),
            };
            push(
                Measure::Balance,
                v.abs() / 10.0,
                format!(
                    "{name} has a {word} than the reference ({v:+.0} dB in the {})",
                    RANGES[k]
                ),
                RANGE_PARTS[k],
                format!("In {name}, {fix}."),
            );
        }
    }
    let rp = reference.sections.get(i);
    let sg = song
        .sections
        .iter()
        .find(|q| q.start <= s.song.0 + 2 && q.end + 2 >= s.song.1)
        .and_then(|q| q.grid);
    for &(k, sim) in groove {
        if sim >= 0.6 || (picture && !resolves_time) {
            continue;
        }
        let parts = RANGE_PARTS[k];
        let says = match (rp.and_then(|p| p.grid), sg) {
            (Some(rg), Some(sg)) => format!(
                "{name}'s {} starts fall differently: the reference's land on steps {}, the song's on {}",
                RANGES[k],
                hits(&rg[k]),
                hits(&sg[k])
            ),
            _ => match s.density[k] {
                Some(r) if r < 1.0 => format!(
                    "{name} has about {:.0}% as many {} starts as the reference",
                    r * 100.0,
                    RANGES[k]
                ),
                Some(r) => format!(
                    "{name} has about {:.1}× the {} starts of the reference",
                    r, RANGES[k]
                ),
                None => format!("{name}'s {} starts differ from the reference's", RANGES[k]),
            },
        };
        push(
            Measure::Groove,
            1.0 - sim,
            says.clone(),
            parts,
            format!(
                "In {name}, rework the {} so that: {says}. Write new notes; don't copy.",
                parts.join(", ")
            ),
        );
    }
    if let Some(t) = s.scores.tonal
        && t < 0.7
        && !picture
    {
        let leans = reference
            .pitch_classes(s.reference.0, s.reference.1)
            .map(|pc| {
                let mut top: Vec<(usize, f32)> = pc.iter().copied().enumerate().collect();
                top.sort_by(|a, b| b.1.total_cmp(&a.1));
                top.iter()
                    .take(3)
                    .map(|(k, _)| NAMES[*k])
                    .collect::<Vec<_>>()
                    .join(", ")
            });
        let says = match (leans, held.0 > held.1 + 0.25, held.1 > held.0 + 0.25) {
            (_, true, _) => format!(
                "{name} holds fewer pitched lines than the reference ({:.0}% of it against {:.0}%)",
                held.1 * 100.0,
                held.0 * 100.0
            ),
            (_, _, true) => format!(
                "{name} holds more pitched lines than the reference ({:.0}% of it against {:.0}%)",
                held.1 * 100.0,
                held.0 * 100.0
            ),
            (Some(l), _, _) => format!("{name}'s notes lean elsewhere: the reference's on {l}"),
            (None, _, _) => format!("{name}'s pitched lines differ from the reference's"),
        };
        push(
            Measure::Tonal,
            1.0 - t,
            says.clone(),
            &["chords", "bass", "lead", "pad"],
            format!(
                "In {name}: {says}. Change the harmony or held parts to match its shape, in the song's own notes."
            ),
        );
    }
    out
}

/// The three pitch classes a line leans on most: "D, F#, A".
fn leans(pc: &[f32; 12]) -> String {
    let mut top: Vec<(usize, f32)> = pc.iter().copied().enumerate().collect();
    top.sort_by(|a, b| b.1.total_cmp(&a.1));
    top.iter()
        .take(3)
        .map(|(k, _)| NAMES[*k])
        .collect::<Vec<_>>()
        .join(", ")
}

/// A section's bass and melody against the reference's: notes that lean elsewhere, a melody
/// that starts on other steps of the bar.
type Line = Option<(f32, [f32; 12], [f32; 12])>;

fn differ_lines(
    name: &str,
    i: usize,
    lines: &[Line; 2],
    melody_steps: (Option<[f32; 16]>, Option<[f32; 16]>),
) -> Vec<Difference> {
    let mut out = Vec::new();
    for (k, (what, parts)) in [
        ("bass", &["bass", "808"][..]),
        ("melody", &["lead", "vocals"][..]),
    ]
    .into_iter()
    .enumerate()
    {
        if let Some((sim, r, s)) = &lines[k]
            && *sim < 0.6
        {
            let says = format!(
                "{name}'s {what} leans on {}, the reference's on {}",
                leans(s),
                leans(r)
            );
            out.push(Difference {
                section: i,
                measure: Measure::Tonal,
                size: (1.0 - sim).clamp(0.0, 1.0),
                says: says.clone(),
                parts: parts.iter().map(|p| p.to_string()).collect(),
                ask: format!(
                    "In {name}, rewrite the {what} so it leans on the reference's notes, as \
                     the song's key has them: {says}. New notes, not a copy."
                ),
            });
        }
    }
    if let (Some(r), Some(s)) = melody_steps {
        let sim = cosine(&r, &s);
        if sim < 0.5 {
            let says = format!(
                "{name}'s melody starts its notes on steps {}, the reference's on {}",
                hits(&s),
                hits(&r)
            );
            out.push(Difference {
                section: i,
                measure: Measure::Groove,
                size: (1.0 - sim).clamp(0.0, 1.0),
                says: says.clone(),
                parts: vec!["lead".into(), "vocals".into()],
                ask: format!(
                    "In {name}, give the melody the reference's rhythm: {says}. Its own notes."
                ),
            });
        }
    }
    out
}

/// What a revision asked for in words aims at: the measures it names (all, if none) and the
/// sections (by name or letter; all, if none).
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub measures: Vec<Measure>,
    pub sections: Vec<usize>,
}

/// The measures and sections a request names: "closer drums" is the groove everywhere,
/// "brighter chorus" the balance in the choruses.
pub fn target(request: &str, report: &Report) -> Target {
    let words = request.to_lowercase();
    let has = |list: &[&str]| list.iter().any(|w| words.contains(w));
    let mut measures = Vec::new();
    if has(&[
        "drum",
        "groove",
        "rhythm",
        "beat",
        "kick",
        "snare",
        "hat",
        "percussion",
        "swing",
    ]) {
        measures.push(Measure::Groove);
    }
    if has(&[
        "bright", "dark", "balance", "tone", "low end", "bass", "high", "mud", "thin", "warm", "eq",
    ]) {
        measures.push(Measure::Balance);
    }
    if has(&[
        "energy",
        "louder",
        "softer",
        "lift",
        "bigger",
        "calm",
        "build",
        "dynamic",
        "hit harder",
        "punch",
    ]) {
        measures.push(Measure::Energy);
    }
    if has(&[
        "chord", "harmony", "melody", "note", "tonal", "pitch", "pad", "key",
    ]) {
        measures.push(Measure::Tonal);
    }
    if measures.is_empty() {
        measures = Measure::ALL.to_vec();
    }
    // A numbered section named ("verse 2") is that one; else a name names every one of it
    // ("chorus": every chorus), or a letter its sections.
    let named = |exact: bool| -> Vec<usize> {
        report
            .sections
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                let n = s.name.to_lowercase();
                let base = n.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ');
                match exact {
                    true => base.len() < n.len() && words.contains(&n),
                    false => {
                        (!base.is_empty() && words.contains(base))
                            || words.contains(&format!("section {}", s.label.to_lowercase()))
                            || words.contains(&format!("part {}", s.label.to_lowercase()))
                    }
                }
            })
            .map(|(i, _)| i)
            .collect()
    };
    let sections = match named(true) {
        exact if !exact.is_empty() => exact,
        _ => named(false),
    };
    Target {
        sections: match sections.is_empty() {
            true => (0..report.sections.len()).collect(),
            false => sections,
        },
        measures,
    }
}

/// How alike a target's measures are over its sections, 0 to 1 (none if none can be told).
pub fn score(report: &Report, t: &Target) -> Option<f32> {
    let v: Vec<f32> = t
        .sections
        .iter()
        .filter_map(|&i| report.sections.get(i))
        .flat_map(|s| t.measures.iter().filter_map(|&m| s.scores.get(m)))
        .collect();
    (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
}

/// How a revision went, by its target and the rest.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Close enough: stop.
    Reached,
    /// Better: another pass may help.
    Better,
    /// No better: stop, it isn't helping.
    Stalled,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Progress {
    pub before: Option<f32>,
    pub after: Option<f32>,
    /// How the measures it didn't aim at moved, on average (below 0, worse).
    pub others: f32,
    pub verdict: Verdict,
}

/// Whether a pass moved what it aimed at: reached (0.85 or more), better (by 0.02 or more), or
/// stalled; and how the rest moved.
pub fn progress(before: &Report, after: &Report, t: &Target) -> Progress {
    let (b, a) = (score(before, t), score(after, t));
    let rest = Target {
        measures: Measure::ALL
            .into_iter()
            .filter(|m| !t.measures.contains(m))
            .collect(),
        sections: (0..after.sections.len()).collect(),
    };
    let others = match (score(before, &rest), score(after, &rest)) {
        (Some(x), Some(y)) => y - x,
        _ => 0.0,
    };
    let verdict = match (b, a) {
        (_, Some(a)) if a >= 0.85 => Verdict::Reached,
        (Some(b), Some(a)) if a >= b + 0.02 => Verdict::Better,
        (None, Some(_)) => Verdict::Better,
        _ => Verdict::Stalled,
    };
    Progress {
        before: b,
        after: a,
        others,
        verdict,
    }
}
