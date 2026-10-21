//! An agent's ears for timbre: each track rendered alone and heard as the sample library hears a
//! sample (what it sounds like, its character, attack and decay, brightness, width, key), set
//! against the part it plays, with what stands out as wrong: a default patch, a sound unlike its
//! name or part, a fader doing the instrument's job, rumble under a part with no business there,
//! a wide low end, a loop in another key. Measurements and a model's guesses, not listening.

use std::path::Path;

use pina_engine::{
    analysis,
    arrange::role,
    music::Key,
    project::{Instrument, Project},
    samples::{Category, analyze, search},
};
use serde_json::{Value, json};

use crate::cli;

/// The sounds a part can be and still be that part.
fn expected(part: &str) -> &'static [Category] {
    use Category::*;
    match part {
        "drums" => &[
            Drums, Kick, Snare, Clap, Rim, Snap, HatClosed, HatOpen, Ride, Crash, Tom, Shaker,
            Tambourine, Perc,
        ],
        "perc" => &[
            Perc, Shaker, Tambourine, Rim, Snap, Tom, Drums, Clap, HatClosed, HatOpen,
        ],
        "bass" => &[Bass, Bass808, Synth],
        "chords" => &[Keys, Chord, Guitar, Synth, Pad, Strings, Brass, Pluck, Bell],
        "pad" => &[Pad, Strings, Synth, Texture, Vocal, Keys, Chord],
        "lead" | "counter" | "arp" => &[
            Lead, Synth, Pluck, Bell, Keys, Brass, Woodwind, Strings, Guitar, Vocal,
        ],
        "vocals" => &[Vocal],
        "fx" => &[Fx, Reverse, Texture],
        "texture" => &[Texture, Foley, Fx, Pad],
        _ => &[],
    }
}

/// Sounds near enough to one another that hearing one for the other is no mistake. An acoustic
/// instrument named isn't a synth: strings or brass that sound like one are the cheap patch.
fn akin(c: Category) -> &'static [Category] {
    use Category::*;
    match c {
        Strings => &[Pad],
        Pad => &[Strings, Synth, Texture],
        Keys => &[Chord, Bell, Pluck],
        Chord => &[Keys, Synth, Pad],
        Bell => &[Keys, Pluck, Lead],
        Pluck => &[Bell, Keys, Synth, Guitar],
        Lead => &[Synth, Pluck],
        Synth => &[Lead, Pad, Pluck, Bass],
        Bass => &[Bass808, Synth],
        Bass808 => &[Bass, Kick],
        Brass => &[Lead],
        Tom => &[Perc, Kick, Drums],
        Fx => &[Texture, Reverse, Foley],
        Texture => &[Fx, Foley, Pad],
        Vocal => &[Pad, Strings, Synth, Lead],
        Kick | Snare | Clap | HatClosed | HatOpen | Perc => &[Drums],
        _ => &[],
    }
}

/// Which broad kind of sound a category is: within one (keys, guitar, strings, synth…) the
/// model mixes them up often; across them (a bass for strings, noise for a snare) seldom.
fn family(c: Category) -> u8 {
    use Category::*;
    match c {
        Bass | Bass808 => 1,
        Kick | Snare | Clap | Rim | Snap | HatClosed | HatOpen | Ride | Crash | Tom | Shaker
        | Tambourine | Perc | Drums => 2,
        Vocal => 3,
        Fx | Reverse | Texture | Foley | Other => 4,
        _ => 0,
    }
}

/// What the name says the sound is ("Strings", "808", "Rhodes"), if it names one.
fn named(name: &str) -> Vec<Category> {
    search::named(name).0
}

/// The bars heard: those asked for, else eight from where the track first plays.
pub(crate) fn span(p: &Project, t: usize) -> Option<(u64, u64)> {
    let first = p.tracks[t].regions.iter().map(|r| r.start).min()?;
    let bar = p.bar();
    let from = first / bar * bar;
    let to = (from + 8 * bar).min(p.end().max(from + bar));
    (to > from).then_some((from, to))
}

/// A share as a percentage, for saying.
fn pct(share: f32) -> u32 {
    (share * 100.0).round() as u32
}

fn label(c: Category) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.replace('_', " ")))
        .unwrap_or_default()
}

fn round1(v: f32) -> f64 {
    (v as f64 * 10.0).round() / 10.0
}

/// Whether a key heard sits with the song's: its scale all but one note the song's (the same key,
/// its relative, a neighbour on the circle of fifths: a loop on the song's VI or IV).
fn sits_with(heard: Key, song: Key) -> bool {
    (0..12u8)
        .filter(|&pc| heard.contains(pc) && song.contains(pc))
        .count()
        >= 6
}

/// Hears each track (or `only`), over bars `bars` (from 1, both in) or eight bars from where
/// each first plays.
pub fn listen(dir: &Path, only: Option<usize>, bars: Option<(u64, u64)>) -> Result<Value, String> {
    let p = cli::project(dir)?;
    let given = bars.map(|(a, b)| cli::bars(&p, a, b)).transpose()?;
    let tracks: Vec<usize> = match only {
        Some(t) if t < p.tracks.len() => vec![t],
        Some(t) => return Err(format!("no track {t}")),
        None => (0..p.tracks.len())
            .filter(|&t| !p.tracks[t].regions.is_empty())
            .collect(),
    };
    let rate = p.sample_rate as f64;
    let song_key = p.song_key();
    let mut out = Vec::new();
    for t in tracks {
        let track = &p.tracks[t];
        let part = role(track);
        let mut issues: Vec<String> = Vec::new();
        match &track.instrument {
            Some(Instrument::Plugin(r)) if r.preset.is_none() && r.params.is_empty() => issues
                .push(format!(
                    "{} with no preset loaded and nothing set: likely its default patch. `sounds` \
                     finds presets for a {part}, by how they sound",
                    r.class
                )),
            Some(Instrument::Synth) => issues.push(
                "the built-in sine: a placeholder, not a finished sound. `sounds` finds real ones"
                    .into(),
            ),
            _ => {}
        }
        if track.volume_db > 6.0 {
            issues.push(format!(
                "its fader is at {:+.1} dB, doing the sound's job: turn it up at its source (a \
                 preset's volume, a zone's gain_db) and mix again",
                track.volume_db
            ));
        }
        let Some(range) = given.or_else(|| span(&p, t)) else {
            continue;
        };
        let [l, r] = cli::rendered(&cli::alone(&p, t), dir, Some(range))?;
        let level = analysis::analyze(&l, &r, rate);
        let heard_bars = format!(
            "{}-{}",
            range.0 / p.bar() + 1,
            range.1.div_ceil(p.bar()).max(range.0 / p.bar() + 1)
        );
        if level.lufs < -60.0 {
            issues.insert(0, "silent over these bars".into());
            out.push(json!({ "track": t, "name": track.name, "part": part, "bars": heard_bars, "issues": issues }));
            continue;
        }
        let (heard, all) = analyze::hear(&[l, r], rate);
        let sounds_like: Vec<String> = all
            .iter()
            .take(3)
            .filter(|(_, s)| *s >= 0.08)
            .map(|(c, s)| format!("{} {}%", label(*c), pct(*s)))
            .collect();
        // What it should be: what its name says, else its part. Near sounds count with it.
        let by_name = named(&track.name);
        let wanted: Vec<Category> = match by_name.is_empty() {
            true => expected(part).to_vec(),
            false => by_name.clone(),
        };
        if !wanted.is_empty() {
            let fits =
                |c: Category| wanted.contains(&c) || wanted.iter().any(|w| akin(*w).contains(&c));
            let share: f32 = all.iter().filter(|(c, _)| fits(*c)).map(|(_, s)| s).sum();
            let (top, top_share) = all[0];
            // Only when the model's sure: it learned from samples, not from patches, and mixes
            // up keys, guitars, strings and synths among themselves.
            let near = wanted.iter().any(|w| family(*w) == family(top));
            let sure = match near {
                true => top_share >= 0.6 && share < 0.12,
                false => top_share >= 0.45 && share < 0.15,
            };
            if !fits(top) && sure {
                let what = match by_name.is_empty() {
                    true => format!("a {part}"),
                    false => format!("named {}", track.name),
                };
                issues.push(format!(
                    "{what}, but it sounds like {} ({}%): the wrong sound, or a patch that \
                     doesn't suit the part",
                    label(top),
                    pct(top_share)
                ));
            }
        }
        // A patch that swells in where the part wants its notes on time.
        let slow_ok = [
            Category::Pad,
            Category::Strings,
            Category::Texture,
            Category::Vocal,
            Category::Fx,
        ]
        .iter()
        .any(|c| wanted.contains(c));
        let midi = track
            .regions
            .iter()
            .any(|r| !matches!(r.content, pina_engine::project::Content::Audio { .. }));
        if midi && !slow_ok && heard.envelope.attack_ms > 400.0 {
            issues.push(format!(
                "a slow attack ({:.1} s) for its {part} part: its notes swell in late, as a pad's do; \
                 shorten its attack or pick another patch",
                heard.envelope.attack_ms / 1000.0
            ));
        }
        if level.clipped > 0 {
            issues.push(format!(
                "clips on its own ({} samples): turn its source down",
                level.clipped
            ));
        }
        let s = &level.spectrum_db;
        let low_end = matches!(part, "bass" | "drums" | "perc" | "fx");
        if !low_end && s.sub > -14.0 {
            issues.push(format!(
                "{:.0} dB of it is under 60 Hz: rumble under its {part} part, mud in the mix; cut its \
                 lows",
                s.sub
            ));
        }
        if part == "bass" && s.sub < -18.0 && s.bass < -9.0 {
            issues.push("little weight under 250 Hz for a bass: a thin patch, or too high".into());
        }
        if low_end && part == "bass" && level.stereo.width_db > -14.0 {
            issues.push(format!(
                "wide down low (sides {:.0} dB): a bass wants its low end mono",
                level.stereo.width_db
            ));
        }
        if matches!(part, "pad" | "chords" | "texture") && s.presence.max(s.air) > -5.0 {
            issues.push(format!(
                "bright for its {part} part ({:.0} dB of bite over 4 kHz): it will fight the lead",
                s.presence.max(s.air)
            ));
        }
        // A loop or a recording in another key than the song's.
        let audio = track
            .regions
            .iter()
            .any(|r| matches!(r.content, pina_engine::project::Content::Audio { .. }));
        let key_heard = heard
            .key
            .as_ref()
            .filter(|k| k.confidence >= 0.4 && heard.tonal >= 0.4);
        if audio
            && let (Some(k), Some(song)) = (key_heard, song_key)
            && let Some(heard_key) = Key::parse(&k.name)
            && !sits_with(heard_key, song)
        {
            issues.push(format!(
                "sounds in {}, the song is in {}: transpose it (set_region transpose) or find \
                 one in key",
                k.name,
                song.name()
            ));
        }
        let e = &heard.envelope;
        out.push(json!({
            "track": t,
            "name": track.name,
            "part": part,
            "bars": heard_bars,
            "sounds_like": sounds_like.join(" · "),
            "character": heard.traits,
            "attack_ms": e.attack_ms.round() as i64,
            "decay_ms": e.decay_ms.round() as i64,
            "sustain": round1(e.sustain),
            "brightness_hz": heard.spectrum.centroid_hz.round() as i64,
            "spectrum_db": {
                "sub": round1(s.sub), "bass": round1(s.bass), "low_mid": round1(s.low_mid),
                "mid": round1(s.mid), "presence": round1(s.presence), "air": round1(s.air),
            },
            "width_db": round1(level.stereo.width_db),
            "lufs": round1(level.lufs),
            "key": key_heard.map(|k| k.name.clone()),
            "issues": issues,
        }));
    }
    Ok(json!({
        "tracks": out,
        "how": "each track rendered alone (through its effects and sends) and heard by the sample \
                library's model: sounds_like is its guess at what the sound is, character its \
                traits. Measurements and guesses, not listening: render for the person to \
                audition.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_key_sits_with_the_song() {
        let k = |s: &str| Key::parse(s).unwrap();
        assert!(sits_with(k("A minor"), k("C major")));
        assert!(sits_with(k("C major"), k("A minor")));
        assert!(sits_with(k("F minor"), k("F minor")));
        assert!(sits_with(k("F major"), k("A minor")));
        assert!(!sits_with(k("G minor"), k("F minor")));
        assert!(!sits_with(k("E major"), k("C major")));
    }
}
