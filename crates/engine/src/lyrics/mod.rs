//! Lyrics heard: the words sung in a recording, each where it's sung, by Whisper (run by
//! whisper.cpp, from a model the person downloads: OpenAI's, MIT), gathered into lines, each
//! line's start where the voice starts; and how often each line is sung (more than once, a
//! hook). What makes a line a loop is the song's (`Project::lyric_bars`).

pub mod flips;
#[cfg(test)]
mod tests;

use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use serde::{Deserialize, Serialize};

/// A word, and when it's sung: seconds into the recording.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: f32,
    pub end: f32,
}

/// A line sung: its words, from when it starts to when it ends (seconds), how many times it's
/// sung in all (2 or more: a hook), and each word where it's sung.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub start: f32,
    pub end: f32,
    pub sung: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<Word>,
}

/// A Whisper model to download: whisper.cpp's conversion of OpenAI's, MIT.
pub struct Whisper {
    pub file: &'static str,
    pub name: &'static str,
    pub bytes: u64,
    pub heard: &'static str,
}

const HOST: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

/// Best first. (Base isn't one: it hears next to nothing sung over music.)
pub const WHISPERS: &[Whisper] = &[
    Whisper {
        file: "ggml-large-v3-turbo-q5_0.bin",
        name: "Whisper large-v3 turbo",
        bytes: 574_041_195,
        heard: "The best: sung words over music, in any of the 99 languages Whisper knows",
    },
    Whisper {
        file: "ggml-small-q5_1.bin",
        name: "Whisper small",
        bytes: 190_085_487,
        heard: "Lighter: hears clear vocals, misses more of a song's",
    },
];

impl Whisper {
    pub fn url(&self) -> String {
        format!("{HOST}{}", self.file)
    }
}

/// Where Whisper models are kept: beside the separation models (`PINA_WHISPER` says elsewhere).
pub fn folder() -> PathBuf {
    match std::env::var_os("PINA_WHISPER") {
        Some(d) => d.into(),
        None => crate::paths::support().join("Whisper"),
    }
}

/// The Whisper models downloaded, best first.
pub fn installed(dir: &Path) -> Vec<&'static Whisper> {
    WHISPERS
        .iter()
        .filter(|w| dir.join(w.file).is_file())
        .collect()
}

/// Whether Whisper runs here (Apple silicon, Windows x64).
pub fn available() -> bool {
    crate::separate::available()
}

/// The lines sung in `sides` (one or two channels at `rate`), as the Whisper model `model`
/// hears them, in any language it knows: what it makes up over music left out (`words`), each
/// line starting where the voice does (`lines`).
pub fn hear(model: &Path, sides: &[Vec<f32>], rate: f64) -> Result<Vec<Line>, String> {
    let mono = mono_16k(sides, rate);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    ))]
    {
        let heard = words(run(model, &mono)?);
        Ok(lines(&heard, &onsets(&mono, 16_000.0)))
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    )))]
    {
        let _ = (model, mono);
        Err("Hearing lyrics needs a Mac with Apple silicon, or Windows on x64".into())
    }
}

/// One channel at Whisper's 16 kHz.
pub fn mono_16k(sides: &[Vec<f32>], rate: f64) -> Vec<f32> {
    let n = sides.iter().map(Vec::len).min().unwrap_or(0);
    let k = sides.len().max(1) as f32;
    let mono: Vec<f32> = (0..n)
        .map(|i| sides.iter().map(|s| s[i]).sum::<f32>() / k)
        .collect();
    crate::audio::engine::resample(&mono, rate, 16_000.0)
}

/// whisper.cpp's words: each a segment of its own (split on words), its times in seconds.
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64")
))]
fn run(model: &Path, mono: &[f32]) -> Result<Vec<Word>, String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
    let ctx = WhisperContext::new_with_params(model, WhisperContextParameters::default())
        .map_err(|e| format!("{}: {e}", model.display()))?;
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;
    let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    p.set_token_timestamps(true);
    p.set_split_on_word(true);
    p.set_max_len(1);
    // Each stretch heard afresh: a line heard wrong isn't carried on into the next.
    p.set_no_context(true);
    p.set_print_progress(false);
    p.set_print_realtime(false);
    p.set_print_special(false);
    p.set_language(None);
    state.full(p, mono).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for i in 0..state.full_n_segments() {
        let Some(seg) = state.get_segment(i) else {
            continue;
        };
        // Silence or music taken for speech.
        if seg.no_speech_probability() > 0.6 {
            continue;
        }
        let text = seg.to_str_lossy().map_err(|e| e.to_string())?.to_string();
        out.push(Word {
            text,
            start: seg.start_timestamp() as f32 / 100.0,
            end: seg.end_timestamp() as f32 / 100.0,
        });
    }
    Ok(out)
}

/// The words worth keeping: trimmed; what Whisper makes up over music left out (empty ones,
/// what's bracketed, "(music)", "[singing in foreign language]", a note, and its loops: a word
/// four times or more on end, "yo yo yo yo", or stuttered, "ca-ca-ca-ca").
pub fn words(raw: Vec<Word>) -> Vec<Word> {
    let mut out = Vec::new();
    let mut bracketed = false;
    for mut w in raw {
        let t = w.text.trim();
        if t.starts_with(['(', '[']) {
            bracketed = true;
        }
        let closes = t.ends_with([')', ']']);
        if bracketed || t.is_empty() || t.contains('♪') {
            bracketed &= !closes;
            continue;
        }
        // Its own stutter ("ca-ca-ca-ca"): made up.
        let parts: Vec<&str> = t.split('-').collect();
        if parts.len() >= 4 && parts.iter().all(|p| p.eq_ignore_ascii_case(parts[0])) {
            continue;
        }
        w.text = t.to_string();
        out.push(w);
    }
    // One word four times or more on end ("yo yo yo yo…"), as Whisper makes up over music.
    let key = |w: &Word| {
        w.text
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric(), "")
    };
    let mut kept = Vec::new();
    let mut i = 0;
    while i < out.len() {
        let run = out[i..]
            .iter()
            .take_while(|w| key(w) == key(&out[i]))
            .count();
        if run < 4 {
            kept.extend(out[i..i + run].iter().cloned());
        }
        i += run;
    }
    kept
}

/// The words as lines: a new one after a pause, after a word that ends a phrase (. ! ? , ;), or
/// at a capital (Whisper starts a sung line with one), but not "I". Each line's start is
/// where the voice starts (`onsets`, seconds, a little before Whisper's time if there's one),
/// and each says how often it's sung.
pub fn lines(words: &[Word], onsets: &[f32]) -> Vec<Line> {
    let mut groups: Vec<Vec<&Word>> = Vec::new();
    for w in words {
        let first = w.text.chars().next();
        let capital = first.is_some_and(char::is_uppercase)
            && !matches!(
                w.text
                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '\''),
                "I" | "I'm" | "I'll" | "I've" | "I'd"
            );
        let new = match groups.last() {
            None => true,
            Some(g) => {
                let prev = g[g.len() - 1];
                w.start - prev.end >= 0.6
                    || prev.text.ends_with(['.', '!', '?', ',', ';'])
                    || (capital && g.len() >= 2)
            }
        };
        match new {
            true => groups.push(vec![w]),
            false => groups.last_mut().expect("a line").push(w),
        }
    }
    // A line run on (Whisper sometimes runs two together, "…one pulse I came in heavy"): split at
    // its widest gap between words, again and again, till each is ten words at most.
    let mut i = 0;
    while i < groups.len() {
        let g = &groups[i];
        if g.len() <= 10 {
            i += 1;
            continue;
        }
        let at = (3..g.len() - 2)
            .max_by(|&a, &b| {
                let gap = |k: usize| g[k].start - g[k - 1].end;
                gap(a).total_cmp(&gap(b))
            })
            .unwrap_or(g.len() / 2);
        let rest = groups[i].split_off(at);
        groups.insert(i + 1, rest);
    }
    let mut out: Vec<Line> = groups
        .iter()
        .map(|g| {
            let text = g
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
                .trim_end_matches([',', ';'])
                .to_string();
            let start = g[0].start;
            // The voice's start: the onset nearest before Whisper's time (it hears a word late,
            // never early), within 0.3 s; Whisper's own without one.
            let start = onsets
                .iter()
                .copied()
                .filter(|&o| o <= start + 0.05 && o >= start - 0.3)
                .reduce(f32::max)
                .unwrap_or(start);
            Line {
                text,
                start,
                end: g[g.len() - 1].end,
                sung: 0,
                words: g.iter().map(|w| (*w).clone()).collect(),
            }
        })
        .collect();
    let key = |t: &str| -> String {
        t.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let keys: Vec<String> = out.iter().map(|l| key(&l.text)).collect();
    for (i, l) in out.iter_mut().enumerate() {
        l.sung = keys.iter().filter(|k| **k == keys[i]).count();
    }
    out
}

/// What `hear_file` heard: the lines, the file they're sung in (its vocals stem, from the song's
/// folder, if they were separated first), and that file's audio (its channels and rate).
pub struct Heard {
    pub lines: Vec<Line>,
    pub file: PathBuf,
    pub audio: (Vec<Vec<f32>>, f64),
}

/// What's kept of a file's lyrics: the file they were heard in, and its lines.
#[derive(Serialize, Deserialize)]
struct Kept {
    file: PathBuf,
    lines: Vec<Line>,
}

/// The words heard before in `file` (relative to the song's folder `dir`) itself, by any Whisper
/// model: none if they haven't been (or were heard in its vocals stem, not it). A line kept
/// without its words counts as one word.
pub fn kept_words(dir: &Path, file: &Path) -> Vec<Word> {
    let Some(name) = file.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        return Vec::new();
    };
    let Ok(all) = std::fs::read_dir(dir.join("Audio Files").join("Lyrics")) else {
        return Vec::new();
    };
    for e in all.flatten() {
        if !e
            .file_name()
            .to_string_lossy()
            .starts_with(&format!("{name} · "))
        {
            continue;
        }
        let Some(k) = std::fs::read_to_string(e.path())
            .ok()
            .and_then(|t| serde_json::from_str::<Kept>(&t).ok())
        else {
            continue;
        };
        if k.file == file && !k.lines.is_empty() {
            return k
                .lines
                .into_iter()
                .flat_map(|l| match l.words.is_empty() {
                    true => vec![Word {
                        text: l.text,
                        start: l.start,
                        end: l.end,
                    }],
                    false => l.words,
                })
                .collect();
        }
    }
    Vec::new()
}

/// The lines sung in `file` (from the song's folder `dir`, or a whole path), heard by
/// `whisper`: as it is if it's a voice alone (`voice_alone`); else from its vocals, separated
/// first by the best vocals model there is (`vocals_model`) into the song's stems, as Separate
/// Stems keeps them; as it is with none. What's heard is kept in the song's `Audio Files/Lyrics`,
/// so a file's heard once. `progress` is told how far separating has got (0 to 1); `stop`
/// stops it.
pub fn hear_file(
    dir: &Path,
    file: &Path,
    whisper: &Whisper,
    alone: bool,
    progress: impl FnMut(f32),
    stop: &AtomicBool,
) -> Result<Heard, String> {
    use crate::{audio::engine::read_audio, separate};
    let name = file
        .file_stem()
        .map_or("audio".into(), |s| s.to_string_lossy().into_owned());
    let kept = dir
        .join("Audio Files")
        .join("Lyrics")
        .join(format!("{name} · {}.json", whisper.name));
    // Kept from before (once, as the bare lines of the file itself).
    let before = std::fs::read_to_string(&kept).ok().and_then(|t| {
        serde_json::from_str::<Kept>(&t).ok().or_else(|| {
            let lines = serde_json::from_str::<Vec<Line>>(&t).ok()?;
            Some(Kept {
                file: file.to_path_buf(),
                lines,
            })
        })
    });
    if let Some(k) = before.filter(|k| !k.lines.is_empty()) {
        let audio = read_audio(&dir.join(&k.file))?;
        return Ok(Heard {
            lines: k.lines,
            file: k.file,
            audio,
        });
    }
    let source = dir.join(file);
    let (sides, rate) = read_audio(&source)?;
    // Known (by Smart Slice's ears) or heard here to be a voice alone: heard as it is.
    let vocals = match alone || voice_alone(&sides, rate, &name) {
        true => None,
        false => vocals_model(&separate::folder()),
    };
    let (heard, audio) = match vocals {
        None => (file.to_path_buf(), (sides, rate)),
        Some(model) => {
            let folder = separate::stems_folder(file, &model.about.name);
            let stem = folder.join("vocals.wav");
            if !dir.join(&stem).is_file() {
                separate::separate(&source, &model, &dir.join(&folder), progress, stop)?;
            }
            (stem.clone(), read_audio(&dir.join(&stem))?)
        }
    };
    let lines = hear(&folder().join(whisper.file), &audio.0, audio.1)?;
    let keep = Kept {
        file: heard.clone(),
        lines,
    };
    // Kept for next time (not if nothing was heard: a better model may hear it); not kept,
    // it's only heard again.
    if !keep.lines.is_empty() {
        if let Some(parent) = kept.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&kept, serde_json::to_string(&keep).unwrap_or_default());
    }
    Ok(Heard {
        lines: keep.lines,
        file: heard,
        audio,
    })
}

/// Whether `sides` (at `rate`, from the file called `name`) are a voice alone, to hear as they
/// are: a vocals stem, or what the sample library would call a vocal (an acapella, a vocal
/// loop). Else (a song, a mix) its vocals are separated first.
pub fn voice_alone(sides: &[Vec<f32>], rate: f64, name: &str) -> bool {
    // A vocals stem Pina Studio separated.
    if name.eq_ignore_ascii_case("vocals") {
        return true;
    }
    // Its first half-minute says enough, and a song's whole would take a while.
    let n = (30.0 * rate) as usize;
    let start: Vec<Vec<f32>> = sides.iter().map(|s| s[..n.min(s.len())].to_vec()).collect();
    crate::samples::analyze::analyze(&start, rate, name).category == crate::samples::Category::Vocal
}

/// The separation model to take a song's vocals out with: the vocal specialists first (one
/// stem, vocals, and the rest), the tested ones in their order, then any with a vocals stem.
pub fn vocals_model(dir: &Path) -> Option<crate::separate::Model> {
    let (models, _) = crate::separate::models(dir);
    let rank = |m: &crate::separate::Model| {
        let known = crate::separate::catalog::KNOWN
            .iter()
            .position(|k| k.about().name == m.about.name)
            .unwrap_or(usize::MAX);
        (m.about.stems.len(), known)
    };
    models
        .into_iter()
        .filter(|m| m.about.stems.iter().any(|s| s == "vocals"))
        // HTDemucs drums gives vocals only roughly.
        .filter(|m| !m.about.name.contains("HTDemucs drums"))
        .min_by_key(rank)
}

/// The quickest model with a vocals stem, to hear whether there's a voice at all (rough vocals
/// are enough to tell): by how fast each known one runs, HTDemucs included.
pub fn quickest_vocals_model(dir: &Path) -> Option<crate::separate::Model> {
    let (models, _) = crate::separate::models(dir);
    let speed = |m: &crate::separate::Model| {
        crate::separate::catalog::Known::named(&m.about.name).map_or(2.0, |k| k.speed)
    };
    models
        .into_iter()
        .filter(|m| m.about.stems.iter().any(|s| s == "vocals"))
        .min_by(|a, b| speed(a).total_cmp(&speed(b)))
}

/// Where a voice's syllables start in `mono` at `rate`, seconds: for `lines`.
pub fn onsets(mono: &[f32], rate: f32) -> Vec<f32> {
    let secs = mono.len() as f32 / rate;
    crate::dsp::sampler::onsets(mono, rate)
        .into_iter()
        .map(|(at, _)| at * secs)
        .collect()
}
