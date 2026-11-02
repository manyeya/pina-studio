//! MIDI files heard together: what a group has in common, and how close another comes to it.
//!
//! Each is reduced to a profile that doesn't care what key it's in: where its notes land on a
//! bar's sixteen steps, which degrees of its key it leans on, how busy, long, loud, wide and thick
//! it is. A group's average of those is what its files share, and the distance from that is what
//! tells an agent whether what it wrote is like them, and what to change if not.

use serde::Serialize;

use super::*;

/// One file, reduced.
#[derive(Clone, Debug, PartialEq)]
struct Profile {
    steps: [f32; 16],
    degrees: [f32; 12],
    density: f32,
    length: f32,
    velocity: f32,
    span: f32,
    polyphony: f32,
}

fn profile(file: &MidiFile, study: &MidiStudy) -> Profile {
    let [n, d] = file.time_signature.unwrap_or([4, 4]);
    let bar = (PPQ * 4 * n / d.max(1)).max(1);
    let root = study
        .key
        .as_deref()
        .and_then(Key::parse)
        .map_or(0, |k| k.root as usize);
    let (mut steps, mut degrees) = ([0f32; 16], [0f32; 12]);
    let (mut count, mut velocity, mut low, mut high) = (0f32, 0f32, 127u8, 0u8);
    for part in &file.parts {
        for note in &part.notes {
            let at = note.start.max(0) as u64 % bar;
            steps[(at * 16 / bar) as usize % 16] += 1.0;
            velocity += note.velocity as f32;
            count += 1.0;
            if !part.is_drums() {
                degrees[(note.pitch as usize + 12 - root) % 12] += 1.0;
                low = low.min(note.pitch);
                high = high.max(note.pitch);
            }
        }
    }
    let of = |f: fn(&PartStudy) -> f32| {
        let n = study.parts.len().max(1) as f32;
        study.parts.iter().map(f).sum::<f32>() / n
    };
    let most = |x: &mut [f32]| {
        let top = x.iter().fold(0f32, |m, v| m.max(*v)).max(1e-6);
        x.iter_mut().for_each(|v| *v /= top);
    };
    most(&mut steps);
    most(&mut degrees);
    Profile {
        steps,
        degrees,
        density: count / file.bars().max(1) as f32,
        length: of(|p| p.length),
        velocity: velocity / count.max(1.0),
        span: high.saturating_sub(low) as f32,
        polyphony: of(|p| p.polyphony),
    }
}

/// How alike two shapes are, 0 to 1.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let size = |x: &[f32]| x.iter().map(|v| v * v).sum::<f32>().sqrt();
    match size(a) * size(b) {
        0.0 => 0.0,
        s => (dot / s).clamp(0.0, 1.0),
    }
}

/// How alike two amounts are, 0 to 1.
fn ratio(a: f32, b: f32) -> f32 {
    match a.max(b) {
        m if m <= 0.0 => 1.0,
        m => a.min(b).max(0.0) / m,
    }
}

/// What a group of MIDI files has in common.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Shared {
    pub files: usize,
    /// How much they're of one mind: 1 the same file over, under about 0.6 a mixed bag.
    pub alike: f32,
    pub role: String,
    pub tempo: Option<f32>,
    pub bars: u64,
    /// Their keys, the most common first, with how many of them.
    pub keys: Vec<(String, usize)>,
    /// Their progressions, by numerals, the most common first.
    pub progressions: Vec<(String, usize)>,
    /// How often each of a bar's sixteen steps starts a note, the most 1.
    pub steps: Vec<f32>,
    /// How much each of the twelve degrees above their keys' roots is used, the most 1.
    pub degrees: Vec<f32>,
    /// Notes a bar, and their fewest and most.
    pub density: [f32; 3],
    /// Sixteenths a note.
    pub length: f32,
    pub velocity: f32,
    /// Semitones from their lowest note to their highest.
    pub span: f32,
    /// Notes at once.
    pub polyphony: f32,
    #[serde(skip)]
    profile: Option<GroupProfile>,
}

#[derive(Clone, Debug, PartialEq)]
struct GroupProfile(Profile);

/// How close a file comes to a group.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Likeness {
    /// All of it, 0 to 1. Over 0.85 it's clearly one of them; over 0.97 it's too near a copy.
    pub score: f32,
    pub rhythm: f32,
    pub notes: f32,
    pub density: f32,
    pub length: f32,
    pub dynamics: f32,
    pub voicing: f32,
    /// What would bring it closer.
    pub advice: Vec<String>,
}

fn counted(items: impl Iterator<Item = String>) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for item in items.filter(|i| !i.is_empty()) {
        match out.iter_mut().find(|o| o.0 == item) {
            Some(o) => o.1 += 1,
            None => out.push((item, 1)),
        }
    }
    out.sort_by_key(|o| std::cmp::Reverse(o.1));
    out
}

/// What `files` (each with its study) share.
pub fn shared(files: &[(&MidiFile, &MidiStudy)]) -> Option<Shared> {
    let profiles: Vec<Profile> = files.iter().map(|(f, s)| profile(f, s)).collect();
    let n = profiles.len() as f32;
    let first = profiles.first()?;
    let mean = |f: fn(&Profile) -> f32| profiles.iter().map(f).sum::<f32>() / n;
    let mut average = first.clone();
    for k in 0..16 {
        average.steps[k] = profiles.iter().map(|p| p.steps[k]).sum::<f32>() / n;
    }
    for k in 0..12 {
        average.degrees[k] = profiles.iter().map(|p| p.degrees[k]).sum::<f32>() / n;
    }
    average.density = mean(|p| p.density);
    average.length = mean(|p| p.length);
    average.velocity = mean(|p| p.velocity);
    average.span = mean(|p| p.span);
    average.polyphony = mean(|p| p.polyphony);
    let alike = profiles
        .iter()
        .map(|p| close(p, &average).score)
        .sum::<f32>()
        / n;
    let mut tempos: Vec<f32> = files.iter().filter_map(|(_, s)| s.tempo).collect();
    tempos.sort_by(f32::total_cmp);
    let mut bars: Vec<u64> = files.iter().map(|(_, s)| s.bars).collect();
    bars.sort_unstable();
    let roles = counted(files.iter().map(|(_, s)| s.role.clone()));
    let densities = profiles.iter().map(|p| p.density);
    Some(Shared {
        files: files.len(),
        alike,
        role: roles.first().map_or(String::new(), |r| r.0.clone()),
        tempo: tempos.get(tempos.len() / 2).copied(),
        bars: bars[bars.len() / 2],
        keys: counted(files.iter().filter_map(|(_, s)| s.key.clone())),
        progressions: counted(files.iter().map(|(_, s)| s.progression.clone())),
        steps: average.steps.to_vec(),
        degrees: average.degrees.to_vec(),
        density: [
            average.density,
            densities.clone().fold(f32::MAX, f32::min),
            densities.fold(0.0, f32::max),
        ],
        length: average.length,
        velocity: average.velocity,
        span: average.span,
        polyphony: average.polyphony,
        profile: Some(GroupProfile(average)),
    })
}

const DEGREES: [&str; 12] = [
    "1", "b2", "2", "b3", "3", "4", "b5", "5", "b6", "6", "b7", "7",
];

/// The places most used of `shape`, by `names`.
fn strongest(shape: &[f32], name: impl Fn(usize) -> String) -> String {
    let mut order: Vec<usize> = (0..shape.len()).filter(|&i| shape[i] > 0.05).collect();
    order.sort_by(|&a, &b| shape[b].total_cmp(&shape[a]));
    let mut top: Vec<usize> = order.into_iter().take(5).collect();
    top.sort_unstable();
    top.into_iter().map(name).collect::<Vec<_>>().join(", ")
}

fn close(p: &Profile, g: &Profile) -> Likeness {
    // Drums have no degrees to compare: that's agreement, not a mismatch.
    let tuned = |x: &Profile| x.degrees.iter().any(|d| *d > 0.0);
    let notes = match (tuned(p), tuned(g)) {
        (false, false) => 1.0,
        _ => cosine(&p.degrees, &g.degrees),
    };
    let rhythm = cosine(&p.steps, &g.steps);
    let density = ratio(p.density, g.density);
    let length = ratio(p.length, g.length);
    let dynamics = ratio(p.velocity, g.velocity);
    let voicing = (ratio(p.polyphony, g.polyphony) + ratio(p.span, g.span)) / 2.0;
    let score = 0.35 * rhythm
        + 0.3 * notes
        + 0.15 * density
        + 0.08 * length
        + 0.04 * dynamics
        + 0.08 * voicing;
    let mut advice = Vec::new();
    if rhythm < 0.8 {
        advice.push(format!(
            "Rhythm: they land on steps {}.",
            strongest(&g.steps, |i| (i + 1).to_string())
        ));
    }
    if notes < 0.8 {
        advice.push(format!(
            "Notes: they lean on degrees {}.",
            strongest(&g.degrees, |i| DEGREES[i].to_string())
        ));
    }
    if density < 0.75 {
        advice.push(format!(
            "Density: they play {:.1} notes a bar, this {:.1}.",
            g.density, p.density
        ));
    }
    if length < 0.7 {
        advice.push(format!(
            "Length: their notes last {:.1} sixteenths, these {:.1}.",
            g.length, p.length
        ));
    }
    if voicing < 0.7 {
        advice.push(format!(
            "Voicing: they sound {:.1} notes at once over {:.0} semitones, this {:.1} over {:.0}.",
            g.polyphony, g.span, p.polyphony, p.span
        ));
    }
    Likeness {
        score,
        rhythm,
        notes,
        density,
        length,
        dynamics,
        voicing,
        advice,
    }
}

impl Shared {
    /// How close a file comes to them.
    pub fn likeness(&self, file: &MidiFile, study: &MidiStudy) -> Option<Likeness> {
        let g = self.profile.as_ref()?;
        Some(close(&profile(file, study), &g.0))
    }
}
