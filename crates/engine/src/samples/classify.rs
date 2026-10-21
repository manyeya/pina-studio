//! What a sample is: a loop or a one-shot, and its category. The audio decides through small
//! models fitted on labelled packs (`model.rs`, written by `examples/sample_eval.rs --fit`);
//! the file's name and folders, when they say, are a strong prior on top.

use super::{Category, Kind, features::Features, model, rhythm};

/// What a sample's path says: evidence for each category (indexed as `Category::ALL`), loop or
/// one-shot, and a tempo.
#[derive(Clone, Debug, Default)]
pub struct Hints {
    pub categories: Vec<(Category, f32)>,
    pub kind: Option<(Kind, f32)>,
    pub bpm: Option<(f32, bool)>,
}

/// How strongly a word says a category: whole-word or (for longer words) inside a word, like
/// "ClosedHat" or "BassStabs".
const WORDS: &[(&str, Category, f32)] = &[
    ("kick", Category::Kick, 1.0),
    ("kik", Category::Kick, 1.0),
    ("bassdrum", Category::Kick, 1.0),
    ("bd", Category::Kick, 0.9),
    ("snare", Category::Snare, 1.0),
    ("snr", Category::Snare, 0.9),
    ("sd", Category::Snare, 0.8),
    ("clap", Category::Clap, 1.0),
    ("clp", Category::Clap, 0.9),
    ("cp", Category::Clap, 0.8),
    ("rim", Category::Rim, 1.0),
    ("rimshot", Category::Rim, 1.0),
    ("rs", Category::Rim, 0.7),
    ("snap", Category::Snap, 1.0),
    ("finger", Category::Snap, 0.6),
    ("closedhat", Category::HatClosed, 1.2),
    ("closed", Category::HatClosed, 0.5),
    ("chh", Category::HatClosed, 1.0),
    ("ch", Category::HatClosed, 0.7),
    ("openhat", Category::HatOpen, 1.2),
    ("open", Category::HatOpen, 0.5),
    ("ohh", Category::HatOpen, 1.0),
    ("oh", Category::HatOpen, 0.7),
    ("hihat", Category::HatClosed, 0.5),
    ("hihat", Category::HatOpen, 0.5),
    ("hat", Category::HatClosed, 0.5),
    ("hat", Category::HatOpen, 0.5),
    ("hh", Category::HatClosed, 0.5),
    ("hh", Category::HatOpen, 0.4),
    ("ride", Category::Ride, 1.0),
    ("crash", Category::Crash, 1.0),
    ("splash", Category::Crash, 0.9),
    ("china", Category::Crash, 0.7),
    ("cymbal", Category::Crash, 0.6),
    ("cymbal", Category::Ride, 0.4),
    ("cym", Category::Crash, 0.6),
    ("cym", Category::Ride, 0.4),
    ("cy", Category::Crash, 0.5),
    ("cy", Category::Ride, 0.3),
    ("tom", Category::Tom, 1.0),
    ("toms", Category::Tom, 1.0),
    ("floortom", Category::Tom, 1.0),
    ("lt", Category::Tom, 0.6),
    ("mt", Category::Tom, 0.6),
    ("ht", Category::Tom, 0.6),
    ("timbale", Category::Tom, 0.7),
    ("shaker", Category::Shaker, 1.0),
    ("shake", Category::Shaker, 0.8),
    ("maraca", Category::Shaker, 0.9),
    ("marcas", Category::Shaker, 0.9),
    ("cabasa", Category::Shaker, 0.9),
    ("tamb", Category::Tambourine, 1.0),
    ("tambourine", Category::Tambourine, 1.0),
    ("perc", Category::Perc, 0.8),
    ("percussion", Category::Perc, 0.8),
    ("conga", Category::Perc, 0.9),
    ("bongo", Category::Perc, 0.9),
    ("cowbell", Category::Perc, 0.9),
    ("cb", Category::Perc, 0.6),
    ("clave", Category::Perc, 0.9),
    ("claves", Category::Perc, 0.9),
    ("block", Category::Perc, 0.6),
    ("woodblock", Category::Perc, 0.9),
    ("triangle", Category::Perc, 0.8),
    ("guiro", Category::Perc, 0.8),
    ("cajon", Category::Perc, 0.8),
    ("djembe", Category::Perc, 0.8),
    ("tabla", Category::Perc, 0.8),
    ("darbuka", Category::Perc, 0.8),
    ("gourd", Category::Perc, 0.6),
    ("handdrum", Category::Perc, 0.8),
    ("metallic", Category::Perc, 0.5),
    ("metalic", Category::Perc, 0.5),
    ("drums", Category::Drums, 0.7),
    ("drum", Category::Drums, 0.4),
    ("beat", Category::Drums, 0.6),
    ("beats", Category::Drums, 0.6),
    ("break", Category::Drums, 0.7),
    ("breakbeat", Category::Drums, 0.9),
    ("groove", Category::Drums, 0.5),
    ("top", Category::Drums, 0.5),
    ("tops", Category::Drums, 0.6),
    ("808", Category::Bass808, 0.7),
    ("sub", Category::Bass808, 0.6),
    ("subbass", Category::Bass808, 0.8),
    ("bass", Category::Bass, 0.9),
    ("bassline", Category::Bass, 1.0),
    ("reese", Category::Bass, 0.8),
    ("keys", Category::Keys, 1.0),
    ("key", Category::Keys, 0.5),
    ("piano", Category::Keys, 1.0),
    ("rhodes", Category::Keys, 1.0),
    ("wurli", Category::Keys, 1.0),
    ("epiano", Category::Keys, 1.0),
    ("elpiano", Category::Keys, 1.0),
    ("epo", Category::Keys, 0.8),
    ("ep", Category::Keys, 0.7),
    ("organ", Category::Keys, 1.0),
    ("org", Category::Keys, 0.7),
    ("clav", Category::Keys, 0.8),
    ("pad", Category::Pad, 1.0),
    ("pads", Category::Pad, 1.0),
    ("drone", Category::Pad, 0.7),
    ("lead", Category::Lead, 1.0),
    ("pluck", Category::Pluck, 1.0),
    ("arp", Category::Pluck, 0.5),
    ("bell", Category::Bell, 1.0),
    ("bells", Category::Bell, 1.0),
    ("chime", Category::Bell, 0.9),
    ("glock", Category::Bell, 0.9),
    ("marimba", Category::Bell, 0.7),
    ("vibes", Category::Bell, 0.7),
    ("string", Category::Strings, 1.0),
    ("strings", Category::Strings, 1.0),
    ("violin", Category::Strings, 1.0),
    ("cello", Category::Strings, 1.0),
    ("orchestra", Category::Strings, 0.5),
    ("brass", Category::Brass, 1.0),
    ("horn", Category::Brass, 1.0),
    ("horns", Category::Brass, 1.0),
    ("trumpet", Category::Brass, 1.0),
    ("trombone", Category::Brass, 1.0),
    ("tuba", Category::Brass, 1.0),
    ("sax", Category::Woodwind, 1.0),
    ("flute", Category::Woodwind, 1.0),
    ("clarinet", Category::Woodwind, 1.0),
    ("oboe", Category::Woodwind, 1.0),
    ("woodwind", Category::Woodwind, 1.0),
    ("guitar", Category::Guitar, 1.0),
    ("gtr", Category::Guitar, 1.0),
    ("synth", Category::Synth, 0.7),
    ("synths", Category::Synth, 0.7),
    ("chord", Category::Chord, 0.9),
    ("chords", Category::Chord, 0.9),
    ("stab", Category::Chord, 0.6),
    ("stabs", Category::Chord, 0.6),
    ("vocal", Category::Vocal, 1.0),
    ("vocals", Category::Vocal, 1.0),
    ("vox", Category::Vocal, 1.0),
    ("voice", Category::Vocal, 1.0),
    ("choir", Category::Vocal, 0.9),
    ("chant", Category::Vocal, 0.8),
    ("acapella", Category::Vocal, 1.0),
    ("huh", Category::Vocal, 0.9),
    ("fx", Category::Fx, 0.7),
    ("sfx", Category::Fx, 0.8),
    ("riser", Category::Fx, 1.0),
    ("uplifter", Category::Fx, 1.0),
    ("downlifter", Category::Fx, 1.0),
    ("sweep", Category::Fx, 0.9),
    ("impact", Category::Fx, 0.9),
    ("whoosh", Category::Fx, 1.0),
    ("zap", Category::Fx, 0.8),
    ("laser", Category::Fx, 0.8),
    ("noise", Category::Texture, 0.6),
    ("reverse", Category::Reverse, 1.0),
    ("reversed", Category::Reverse, 1.0),
    ("rev", Category::Reverse, 0.8),
    ("crackle", Category::Texture, 1.0),
    ("vinyl", Category::Texture, 0.9),
    ("texture", Category::Texture, 1.0),
    ("ambience", Category::Texture, 1.0),
    ("ambient", Category::Texture, 0.8),
    ("atmos", Category::Texture, 0.9),
    ("atmosphere", Category::Texture, 0.9),
    ("field", Category::Texture, 0.6),
    ("foley", Category::Foley, 1.0),
];

/// Words that mark a loop, or a one-shot.
const LOOP_WORDS: &[&str] = &[
    "loop", "loops", "beat", "beats", "groove", "grooves", "break",
];
const SHOT_WORDS: &[&str] = &[
    "oneshot", "oneshots", "shot", "shots", "hit", "hits", "multi",
];

/// A path's words: each part split at anything not a letter or digit, and between letters and
/// digits ("Kick03" is "kick" and "03").
fn words(part: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut digits = false;
    for c in part.chars() {
        if c.is_alphanumeric() {
            let d = c.is_ascii_digit();
            if !word.is_empty() && d != digits {
                out.push(std::mem::take(&mut word));
            }
            digits = d;
            word.push(c);
        } else if !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// Whether `word` says `key`: the same, a plural, or (for keys of four letters or more) inside
/// it, as "closedhat" or "bassstabs" do.
fn says(word: &str, key: &str) -> bool {
    word == key
        || word.strip_suffix('s') == Some(key)
        || (key.len() >= 4 && word.len() > key.len() && word.contains(key))
}

/// What a path (lower-cased) says about its sample: its name most, its folders less.
pub fn hints(path: &str) -> Hints {
    let mut parts: Vec<&str> = path.rsplit('/').take(4).collect();
    if let Some(name) = parts.first_mut() {
        *name = name.rsplit_once('.').map_or(*name, |(stem, _)| stem);
    }
    let mut scores = vec![0f32; Category::ALL.len()];
    let (mut looped, mut shot) = (0f32, 0f32);
    let mut named = false;
    for (depth, part) in parts.iter().enumerate() {
        let weight = [1.0, 0.45, 0.25, 0.15][depth];
        let words = words(part);
        let mut seen = vec![0f32; Category::ALL.len()];
        for (i, w) in words.iter().enumerate() {
            // "Bass Drum" is a kick, not a bass.
            if w == "bass" && words.get(i + 1).is_some_and(|n| n == "drum") {
                seen[index(Category::Kick)] = seen[index(Category::Kick)].max(1.0);
                continue;
            }
            for &(key, category, strength) in WORDS {
                if says(w, key) {
                    let c = index(category);
                    seen[c] = seen[c].max(strength);
                }
            }
            if LOOP_WORDS.iter().any(|k| says(w, k)) {
                looped = looped.max(weight);
            }
            if SHOT_WORDS.iter().any(|k| says(w, k)) || w == "one" {
                shot = shot.max(weight);
            }
        }
        // "kick" beats "808" and "sub" said with it: an 808 kick is a kick.
        if seen[index(Category::Kick)] > 0.0 {
            seen[index(Category::Bass808)] *= 0.3;
            seen[index(Category::Bass)] *= 0.3;
        }
        // "Beat_01_Kick", "Drum_Hits/Snare": a drum named is more than "drum" or "beat".
        let drums = index(Category::Drums);
        if seen
            .iter()
            .enumerate()
            .any(|(i, &v)| i != drums && v >= 0.6)
        {
            seen[drums] *= 0.25;
            // Its own name says what it is: "Beat_01/Beat_01_GuitarStabs" is a guitar.
            named |= depth == 0;
        }
        if named {
            seen[drums] *= 0.25;
        }
        for (s, v) in scores.iter_mut().zip(&seen) {
            *s += weight * v;
        }
    }
    let bpm = rhythm::named_tempo(path);
    if bpm.is_some_and(|(_, weak)| !weak) {
        looped = looped.max(1.0);
    }
    let kind = match (looped, shot) {
        (l, s) if l > s => Some((Kind::Loop, l)),
        (l, s) if s > l => Some((Kind::OneShot, s)),
        _ => None,
    };
    Hints {
        categories: Category::ALL
            .iter()
            .zip(scores)
            .filter(|(_, s)| *s > 0.0)
            .map(|(c, s)| (*c, s))
            .collect(),
        kind,
        bpm,
    }
}

pub fn index(c: Category) -> usize {
    Category::ALL.iter().position(|&x| x == c).unwrap()
}

/// The numbers the models read, in a fixed order.
pub fn vector(f: &Features) -> Vec<f32> {
    let ln1 = |v: f32| (1.0 + v.max(0.0)).ln();
    let log2 = |v: f32| (v.max(0.0) + 20.0).log2();
    let e = &f.envelope;
    let h = f.hits.unwrap_or(f.envelope);
    let s = &f.spectrum;
    let (pc, phz, glide, spread, rich) = match &f.pitch {
        Some(p) => (
            p.confidence,
            p.confidence * (p.hz.max(20.0).log2() - 7.0),
            p.confidence * (p.glide / 1200.0).clamp(-2.0, 2.0),
            p.confidence * (p.spread / 100.0).min(6.0),
            // How bright it is for its note: an 808 is nearly all fundamental, a bass not.
            p.confidence
                * (s.centroid_hz.max(20.0) / p.hz.max(20.0))
                    .log2()
                    .clamp(-2.0, 8.0),
        ),
        None => (0.0, 0.0, 0.0, 0.0, 0.0),
    };
    let chroma_top = f.tonal.chroma.iter().fold(0f32, |m, &v| m.max(v));
    let chroma_many = f.tonal.chroma.iter().filter(|&&v| v > 0.08).count() as f32;
    let mut v = vec![
        (f.seconds + 0.01).ln(),
        ln1(e.attack_ms),
        ln1(e.decay_ms),
        ln1(e.tail_ms),
        e.sustain,
        ln1(h.attack_ms),
        ln1(h.decay_ms),
        h.sustain,
        log2(s.centroid_hz),
        log2(s.rolloff_hz),
        log2(f.bandwidth),
        f.spread,
        s.flatness,
        log2(f.centroid_attack) - log2(f.centroid_body),
        f.tonal.tonal,
        f.tonal.polyphony.min(6.0) / 2.0,
        chroma_top,
        chroma_many / 6.0,
        pc,
        phz,
        glide,
        spread,
        rich,
        ln1(f.onsets.len() as f32),
        ln1(f.onsets.len() as f32 / f.seconds.max(0.05)),
        f.tempo.map_or(0.0, |t| t.strength),
        f.tempo.is_some_and(|t| t.beats.is_some_and(|b| b % 4 == 0)) as u8 as f32,
        f.crest_db / 10.0,
        ln1(f.zcr) / 5.0,
        f.width,
        (f.clipped * 100.0).min(1.0),
        (f.start_db / 20.0).clamp(-6.0, 0.0),
        (f.end_db / 20.0).clamp(-6.0, 0.0),
    ];
    v.extend([
        f.bursts.min(5.0),
        f.low_drop.clamp(-2.0, 3.0),
        f.harmonics / 11.0,
    ]);
    v.extend(f.attack_mfcc.iter().map(|c| c / 10.0));
    v.extend(f.band_decay);
    v.extend(f.band_peak);
    v.extend(f.contrast.iter().map(|c| c / 10.0));
    v.extend(s.bands_db.iter().map(|b| b.clamp(-60.0, 0.0) / 10.0));
    v.extend(f.mfcc_mean[1..].iter().map(|c| c / 10.0));
    v.extend(f.mfcc_std[1..].iter().map(|c| c / 10.0));
    v
}

/// An ensemble's probabilities for its classes, from the vector: its nets' averaged.
pub fn softmax(nets: &[model::Net], x: &[f32]) -> Vec<f32> {
    let mut sum: Vec<f32> = Vec::new();
    for m in nets {
        let p = net(m, x);
        if sum.is_empty() {
            sum = p;
        } else {
            sum.iter_mut().zip(p).for_each(|(s, v)| *s += v);
        }
    }
    sum.iter_mut().for_each(|s| *s /= nets.len() as f32);
    sum
}

/// Every net's hidden layer, one after another: features the nets learned, which the embedding
/// reads too.
pub fn hidden(nets: &[model::Net], x: &[f32]) -> Vec<f32> {
    nets.iter()
        .filter(|m| x.len() == m.mean.len())
        .flat_map(|m| {
            let z: Vec<f32> = x
                .iter()
                .zip(m.mean)
                .zip(m.scale)
                .map(|((v, mu), s)| ((v - mu) / s).clamp(-6.0, 6.0))
                .collect();
            m.hidden
                .iter()
                .zip(m.hidden_bias)
                .map(move |(w, b)| (b + w.iter().zip(&z).map(|(a, v)| a * v).sum::<f32>()).max(0.0))
        })
        .collect()
}

/// One net's probabilities.
fn net(m: &model::Net, x: &[f32]) -> Vec<f32> {
    if m.bias.is_empty() || x.len() != m.mean.len() {
        return Vec::new();
    }
    let z: Vec<f32> = x
        .iter()
        .zip(m.mean)
        .zip(m.scale)
        .map(|((v, mu), s)| ((v - mu) / s).clamp(-6.0, 6.0))
        .collect();
    let h: Vec<f32> = if m.hidden.is_empty() {
        z
    } else {
        m.hidden
            .iter()
            .zip(m.hidden_bias)
            .map(|(w, b)| (b + w.iter().zip(&z).map(|(a, v)| a * v).sum::<f32>()).max(0.0))
            .collect()
    };
    let logits: Vec<f32> = m
        .weights
        .iter()
        .zip(m.bias)
        .map(|(w, b)| b + w.iter().zip(&h).map(|(a, v)| a * v).sum::<f32>())
        .collect();
    let top = logits.iter().fold(f32::MIN, |a, &b| a.max(b));
    let exp: Vec<f32> = logits.iter().map(|l| (l - top).exp()).collect();
    let sum: f32 = exp.iter().sum();
    exp.iter().map(|e| e / sum).collect()
}

/// Loop or one-shot: the audio's say (a model over length, onsets, tempo and how it ends), then
/// the name's.
pub fn kind(f: &Features, x: &[f32], hints: &Hints) -> Kind {
    let audio = softmax(model::KIND, x);
    let mut looped = match audio.as_slice() {
        [_, l] => *l,
        _ => {
            if f.seconds > 4.0 {
                0.7
            } else {
                0.2
            }
        }
    };
    // Too short to loop, or a single hit that dies away: a one-shot, unless its name says. (A
    // held chord can loop, but a loop is cut, not faded.)
    if f.seconds < 0.6 || f.tempo.is_none() || (f.onsets.len() < 2 && f.end_db < -40.0) {
        looped = looped.min(0.05);
    }
    if let Some((k, strength)) = hints.kind {
        let odds = 1.0 + 30.0 * strength;
        let o = looped / (1.0 - looped).max(1e-6);
        let o = if k == Kind::Loop { o * odds } else { o / odds };
        looped = o / (1.0 + o);
    }
    if looped > 0.5 {
        Kind::Loop
    } else {
        Kind::OneShot
    }
}

/// The category, and how sure (0 to 1): the audio model's probabilities times the name's prior.
pub fn category(x: &[f32], hints: &Hints, kind: Kind) -> (Category, f32) {
    categories(x, hints, kind)[0]
}

/// Every category, likeliest first, each with its share (they sum to 1): what `category` picks
/// from, for saying what else a sound could be.
pub fn categories(x: &[f32], hints: &Hints, kind: Kind) -> Vec<(Category, f32)> {
    let nets = match kind {
        Kind::OneShot => model::ONE_SHOT,
        Kind::Loop => model::LOOP,
    };
    let audio = softmax(nets, x);
    let n = Category::ALL.len();
    // The model's say, softened (it's sure of itself on sounds unlike what it learned from), and
    // a share for categories it doesn't know, so a name can still say them.
    let known = audio.len().max(1) as f32;
    let tempered: Vec<f32> = audio.iter().map(|q| q.powf(0.7)).collect();
    let sum: f32 = tempered.iter().sum::<f32>().max(1e-20);
    let mut p = vec![0.1 / known; n];
    if audio.is_empty() {
        p.iter_mut().for_each(|v| *v = 1.0 / n as f32);
    }
    let classes = nets.first().map_or(&[][..], |m| m.classes);
    for (c, q) in classes.iter().zip(&tempered) {
        p[index(*c)] = 0.9 * q / sum + 0.1 / known;
    }
    // A whole beat is a loop.
    if kind == Kind::OneShot {
        p[index(Category::Drums)] *= 0.1;
    }
    let total: f32 = hints.categories.iter().map(|(_, s)| s).sum();
    if total > 0.0 {
        // Names are seldom wrong: what one says outweighs all but the surest audio.
        let strength = hints
            .categories
            .iter()
            .fold(0f32, |m, (_, s)| m.max(*s))
            .min(1.0);
        let floor = 0.001 + 0.01 * (1.0 - strength);
        let mut prior = vec![floor; n];
        for (c, s) in &hints.categories {
            prior[index(*c)] += s / total;
        }
        p.iter_mut().zip(&prior).for_each(|(v, q)| *v *= q);
    }
    let sum: f32 = p.iter().sum::<f32>().max(1e-20);
    let mut all: Vec<(Category, f32)> = (0..n).map(|i| (Category::ALL[i], p[i] / sum)).collect();
    all.sort_by(|a, b| b.1.total_cmp(&a.1));
    all
}

#[cfg(test)]
mod tests;
