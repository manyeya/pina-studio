//! Auditioning presets: each played a phrase for the part it's wanted for (a bass's roots and
//! fifths, a pad's held chords, a lead's line) and heard as the sample library hears a sample,
//! so sounds are chosen by how they sound, not by what they're called. Each instrument plays in a
//! process of its own (one that crashes or hangs takes nothing with it), and what's heard is kept.

use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use pina_engine::{
    music::analysis,
    samples::{Category, analyze},
    song::project::{PluginRef, Preset, Project},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cli;

/// What a preset sounded like playing a part's phrase.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Heard {
    /// The model's likeliest few: what the sound is, and how sure.
    pub sounds_like: Vec<(Category, f32)>,
    pub traits: Vec<String>,
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub sustain: f32,
    pub centroid_hz: f32,
    /// Sub, bass, low mid, mid, presence, air: dB under the whole.
    pub bands_db: [f32; 6],
    pub lufs: f32,
    pub peak_db: f32,
    pub width_db: f32,
}

const BAR: u64 = 3840;

/// The phrase a part is heard playing, in C minor at 120 BPM over two bars: notes as
/// (start, length, pitch, velocity).
fn phrase(part: &str) -> Vec<(u64, u64, u8, u8)> {
    let chord = |at: u64, len: u64, root: u8, v: u8| -> Vec<(u64, u64, u8, u8)> {
        [0, 3, 7, 10]
            .iter()
            .map(|i| (at, len, root + i, v))
            .collect()
    };
    match part {
        "bass" => vec![
            (0, 900, 36, 105),
            (960, 900, 36, 100),
            (1920, 420, 36, 100),
            (2400, 420, 43, 95),
            (2880, 900, 48, 100),
            (BAR, 3700, 32, 105),
        ],
        "pad" | "texture" => [chord(0, BAR - 40, 48, 90), chord(BAR, BAR - 40, 44, 90)].concat(),
        "chords" => [
            chord(0, 420, 48, 100),
            chord(960, 420, 48, 95),
            chord(1920, 1800, 48, 100),
            chord(BAR, 420, 44, 100),
            chord(BAR + 960, 2800, 44, 95),
        ]
        .concat(),
        "arp" | "counter" => (0..32u64)
            .map(|i| {
                let up = [60, 63, 67, 72, 75, 72, 67, 63];
                (
                    i * 240,
                    220,
                    up[(i % 8) as usize] - if i >= 16 { 4 } else { 0 },
                    95,
                )
            })
            .collect(),
        "drums" | "perc" => (0..16u64)
            .flat_map(|i| {
                let at = i * 480;
                let mut hits = vec![(at, 200, 42, 80)];
                if i % 4 == 0 {
                    hits.push((at, 300, 36, 115));
                }
                if i % 4 == 2 {
                    hits.push((at, 300, 38, 110));
                }
                hits
            })
            .collect(),
        "fx" => vec![(0, 2 * BAR - 40, 60, 100)],
        // A lead, vocals, anything else: a line.
        _ => vec![
            (0, 440, 72, 100),
            (480, 440, 75, 95),
            (960, 920, 79, 105),
            (1920, 440, 77, 95),
            (2400, 440, 75, 90),
            (2880, 920, 74, 100),
            (BAR, 1900, 72, 105),
            (BAR + 1920, 1800, 67, 95),
        ],
    }
}

/// Plays and hears each preset of `plugin` for `part`, here (the process `audition` starts).
pub fn hear_here(plugin: &PluginRef, presets: &[Preset], part: &str) -> Vec<Option<Heard>> {
    let dir = std::env::temp_dir().join(format!("pina-audition-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let notes: Vec<Value> = phrase(part)
        .into_iter()
        .map(|(start, length, pitch, velocity)| {
            json!({ "start": start, "length": length, "pitch": pitch, "velocity": velocity })
        })
        .collect();
    let out = presets
        .iter()
        .map(|preset| {
            let mut with = plugin.clone();
            with.preset = Some(preset.clone());
            with.params = Default::default();
            with.state = None;
            let project: Project = serde_json::from_value(json!({
                "tempo": 120.0,
                "sample_rate": 48000,
                "tracks": [{
                    "name": "Audition",
                    "instrument": { "plugin": with },
                    "regions": [{ "start": 0, "length": 2 * BAR, "midi": { "notes": notes } }],
                }],
            }))
            .ok()?;
            // Two bars and one more for what rings on.
            let [l, r] = cli::rendered(&project, &dir, Some((0, 3 * BAR))).ok()?;
            let level = analysis::analyze(&l, &r, 48_000.0);
            let s = &level.spectrum_db;
            let (heard, all) = analyze::hear(&[l, r], 48_000.0);
            Some(Heard {
                sounds_like: all.into_iter().take(4).collect(),
                traits: heard.traits,
                attack_ms: heard.envelope.attack_ms,
                decay_ms: heard.envelope.decay_ms,
                sustain: heard.envelope.sustain,
                centroid_hz: heard.spectrum.centroid_hz,
                bands_db: [s.sub, s.bass, s.low_mid, s.mid, s.presence, s.air],
                lufs: level.lufs,
                peak_db: level.peak_db[0].max(level.peak_db[1]),
                width_db: level.stereo.width_db,
            })
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// Plays and hears each preset of `plugin` for `part` in a process of its own, giving up on one
/// that takes longer than a few seconds a preset.
pub fn hear(plugin: &PluginRef, presets: &[Preset], part: &str) -> Vec<Option<Heard>> {
    let none = || vec![None; presets.len()];
    if presets.is_empty() {
        return Vec::new();
    }
    let Ok(exe) = std::env::current_exe() else {
        return none();
    };
    let (Ok(plugin_json), Ok(presets_json)) = (
        serde_json::to_string(plugin),
        serde_json::to_string(presets),
    ) else {
        return none();
    };
    let Ok(mut child) = Command::new(exe)
        .args(["__audition", &plugin_json, part, &presets_json])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return none();
    };
    let Some(mut stdout) = child.stdout.take() else {
        return none();
    };
    // Read as it writes, so a long answer doesn't fill the pipe and stall it.
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        stdout.read_to_string(&mut out).map(|_| out)
    });
    let deadline = Instant::now() + Duration::from_secs(15 + 8 * presets.len() as u64);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                return none();
            }
        }
    }
    reader
        .join()
        .ok()
        .and_then(Result::ok)
        .and_then(|text| serde_json::from_str::<Vec<Option<Heard>>>(&text).ok())
        .filter(|v| v.len() == presets.len())
        .unwrap_or_else(none)
}

/// The `__audition` command: plays `presets` (JSON) on `plugin` (JSON) for `part` and prints
/// what was heard, a JSON list in their order.
pub fn command(plugin: &str, part: &str, presets: &str) -> Result<(), String> {
    let plugin: PluginRef = serde_json::from_str(plugin).map_err(|e| e.to_string())?;
    let presets: Vec<Preset> = serde_json::from_str(presets).map_err(|e| e.to_string())?;
    let heard = hear_here(&plugin, &presets, part);
    println!(
        "{}",
        serde_json::to_string(&heard).map_err(|e| e.to_string())?
    );
    Ok(())
}

/// The sounds a part can be, each with how much it is the part (keys are chords more than a
/// synth playing them is).
fn part_sounds(part: &str) -> &'static [(Category, f32)] {
    use Category::*;
    match part {
        "bass" => &[(Bass, 1.0), (Bass808, 1.0), (Synth, 0.6)],
        "pad" => &[
            (Pad, 1.0),
            (Strings, 0.9),
            (Synth, 0.7),
            (Texture, 0.6),
            (Vocal, 0.6),
        ],
        "chords" => &[
            (Keys, 1.0),
            (Chord, 1.0),
            (Guitar, 0.8),
            (Pluck, 0.7),
            (Bell, 0.6),
            (Synth, 0.6),
            (Strings, 0.5),
            (Brass, 0.5),
            (Pad, 0.4),
        ],
        "drums" | "perc" => &[
            (Drums, 1.0),
            (Kick, 1.0),
            (Snare, 1.0),
            (Perc, 1.0),
            (Tom, 1.0),
            (Clap, 1.0),
            (HatClosed, 1.0),
        ],
        "fx" | "texture" => &[(Fx, 1.0), (Texture, 1.0), (Reverse, 0.8), (Pad, 0.5)],
        "vocals" => &[(Vocal, 1.0), (Pad, 0.5), (Synth, 0.4)],
        _ => &[
            (Lead, 1.0),
            (Synth, 0.8),
            (Pluck, 0.8),
            (Bell, 0.8),
            (Brass, 0.8),
            (Woodwind, 0.8),
            (Keys, 0.6),
            (Strings, 0.6),
            (Guitar, 0.6),
        ],
    }
}

/// How well what was heard suits a part, 0 to 1, and what stood out: the model's say on what it
/// is (an instrument named in what's asked for, "rhodes", "brass", counting most), its envelope
/// against the part's (a pad swells, a pluck dies away, a bass speaks at once), its weight down
/// low for a bass, and the character asked for.
pub fn judge(
    part: &str,
    heard: &Heard,
    asked: &[String],
    asked_sounds: &[Category],
) -> (f32, Vec<String>) {
    let mut why = Vec::new();
    let weight = |c: Category| {
        if asked_sounds.contains(&c) {
            return 1.2;
        }
        part_sounds(part)
            .iter()
            .find(|(x, _)| *x == c)
            .map_or(0.0, |(_, w)| *w)
    };
    let share: f32 = heard.sounds_like.iter().map(|(c, s)| s * weight(*c)).sum();
    let kind = (share * 1.4).min(1.0);
    let (a, sus) = (heard.attack_ms, heard.sustain);
    let low = heard.bands_db[0].max(heard.bands_db[1]);
    let envelope = match part {
        "pad" | "texture" => match (a, sus) {
            (a, s) if a >= 60.0 && s >= 0.45 => 1.0,
            (_, s) if s >= 0.45 => 0.7,
            _ => 0.35,
        },
        "bass" => {
            let speaks = if a <= 40.0 {
                1.0
            } else if a <= 120.0 {
                0.6
            } else {
                0.2
            };
            let weight = if low > -6.0 {
                1.0
            } else if low > -12.0 {
                0.6
            } else {
                0.2
            };
            if weight < 0.5 {
                why.push("light down low for a bass".into());
            }
            speaks * 0.5 + weight * 0.5
        }
        "drums" | "perc" => {
            if a <= 15.0 {
                1.0
            } else {
                0.4
            }
        }
        "arp" | "counter" => match (a, heard.decay_ms) {
            (a, d) if a <= 30.0 && d <= 900.0 => 1.0,
            (a, _) if a <= 60.0 => 0.7,
            _ => 0.3,
        },
        "chords" => {
            if a <= 50.0 {
                1.0
            } else if a <= 200.0 {
                0.6
            } else {
                0.25
            }
        }
        "fx" => 0.8,
        _ => match (a, sus) {
            (a, s) if a <= 60.0 && s >= 0.3 => 1.0,
            (a, _) if a <= 120.0 => 0.7,
            _ => 0.3,
        },
    };
    if envelope < 0.5 && a > 150.0 && !matches!(part, "pad" | "texture" | "fx") {
        why.push(format!("slow to speak ({:.0} ms attack)", a));
    }
    let character = match asked.is_empty() {
        true => 0.6,
        false => {
            let has = asked.iter().filter(|t| heard.traits.contains(t)).count();
            has as f32 / asked.len() as f32
        }
    };
    let level = match heard.lufs {
        l if l < -60.0 => 0.0,
        l if l < -38.0 => {
            why.push(format!("very quiet ({l:.0} LUFS)"));
            0.4
        }
        l if l > -6.0 => 0.7,
        _ => 1.0,
    };
    if heard.peak_db >= 0.0 {
        why.push("clips".into());
    }
    // Sub under a part that should leave it to the bass and kick.
    let rumble = match part {
        "bass" | "drums" | "perc" | "fx" => 1.0,
        _ if heard.bands_db[0] > -12.0 => {
            why.push(format!(
                "heavy under 60 Hz ({:.0} dB) for a {part}",
                heard.bands_db[0]
            ));
            0.7
        }
        _ => 1.0,
    };
    // The envelope gates it: a pluck that swells in is no pluck, however bright.
    let score =
        (0.45 * kind + 0.35 * envelope + 0.2 * character) * (0.6 + 0.4 * envelope) * level * rumble;
    (score, why)
}

/// What was heard, said shortly: "pad 61% · strings 22%; soft, wide, dark; 380 ms attack".
pub fn say(heard: &Heard) -> String {
    let like: Vec<String> = heard
        .sounds_like
        .iter()
        .filter(|(_, s)| *s >= 0.1)
        .take(2)
        .map(|(c, s)| {
            let name = serde_json::to_value(c)
                .ok()
                .and_then(|v| v.as_str().map(|x| x.replace('_', " ")))
                .unwrap_or_default();
            format!("{name} {:.0}%", s * 100.0)
        })
        .collect();
    format!(
        "{}; {}; {:.0} ms attack, {:.0} Hz bright",
        like.join(" · "),
        heard.traits.join(", "),
        heard.attack_ms,
        heard.centroid_hz
    )
}

/// The cache of what presets sounded like: kept in Pina Studio's caches, by plugin, preset and
/// part.
pub struct Kept {
    file: std::path::PathBuf,
    map: serde_json::Map<String, Value>,
    changed: bool,
}

impl Kept {
    pub fn open() -> Kept {
        let file = pina_engine::paths::caches().join("auditions.json");
        let map = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Kept {
            file,
            map,
            changed: false,
        }
    }

    fn key(plugin: &PluginRef, preset: &Preset, part: &str) -> String {
        format!(
            "{:?}|{}|{}|{}|{}|{}",
            plugin.format,
            plugin.path.display(),
            plugin.class,
            plugin.uid.as_deref().unwrap_or(""),
            serde_json::to_string(&preset.source).unwrap_or_default(),
            phrase_of(part)
        )
    }

    /// What it sounded like, if heard before (`Some(None)`: it was silent or failed).
    pub fn get(&self, plugin: &PluginRef, preset: &Preset, part: &str) -> Option<Option<Heard>> {
        let v = self.map.get(&Kept::key(plugin, preset, part))?;
        Some(serde_json::from_value(v.clone()).ok())
    }

    pub fn put(&mut self, plugin: &PluginRef, preset: &Preset, part: &str, heard: &Option<Heard>) {
        self.map.insert(
            Kept::key(plugin, preset, part),
            serde_json::to_value(heard).unwrap_or(Value::Null),
        );
        self.changed = true;
    }

    pub fn save(&self) {
        if !self.changed {
            return;
        }
        if let Some(parent) = self.file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            &self.file,
            serde_json::to_vec(&self.map).unwrap_or_default(),
        );
    }
}

/// Parts heard with the same phrase share what was heard.
fn phrase_of(part: &str) -> &'static str {
    match part {
        "bass" => "bass",
        "pad" | "texture" => "pad",
        "chords" => "chords",
        "arp" | "counter" => "arp",
        "drums" | "perc" => "drums",
        "fx" => "fx",
        _ => "line",
    }
}

#[cfg(test)]
mod tests;
