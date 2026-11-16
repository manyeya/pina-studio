//! Smart Slice: where to cut a recording, and what each cut is. Any recording as it is: at its
//! hits and phrases (`dsp::sampler::onsets`), each slice named for what it sounds like by the
//! sample analysis (Kick, Snare, Closed Hat, Vocal, Chord Am, Keys D). Nothing is separated, so a
//! voice in a beat is sliced with it, as a soul sample is chopped. Or the beat without its voice
//! (`without_voice`: the vocals separated, what's left sliced). And whether there's a voice in it
//! (`voice`): a vocal alone is known at once; in a mix, a vocals model hears a few short
//! stretches. What's found is kept beside the song's audio (Audio Files/Slices), so a file's is
//! found once.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::samples::{Analysis, Category};

/// The most slices a recording is cut into: a whole song has a few hundred hits.
const MOST: usize = 1024;

/// Hits closer than this are one.
const CLOSEST: f32 = 0.06;

/// A voice in at least this share of it: there's a voice in it.
pub const VOICED: f32 = 0.1;

/// A voice this share of its sound or more: it's a voice alone (a vocals stem, an acapella; a
/// song's voice is a fifth of its sound or so, a vocals stem nearly all).
pub const VOICE_ALONE: f32 = 0.6;

/// What's in a recording: by how much of it a voice is (as heard), or as the person says.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InIt {
    /// No voice: a beat, a loop, an instrument.
    Beat,
    /// A voice in a beat, a song.
    Mixed,
    /// A voice alone.
    Voice,
}

impl InIt {
    /// From the share of its moments a voice is in, and the share of its sound that's a voice.
    pub fn of(moments: f32, sound: f32) -> InIt {
        if sound >= VOICE_ALONE {
            InIt::Voice
        } else if moments >= VOICED || sound >= 0.05 {
            InIt::Mixed
        } else {
            InIt::Beat
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            InIt::Beat => "No voice",
            InIt::Mixed => "A voice in a beat",
            InIt::Voice => "A voice alone",
        }
    }
}

/// A slice: from and to (seconds into its file), what it sounds like, and how sure that is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Slice {
    pub start: f32,
    pub end: f32,
    pub name: String,
    pub category: Category,
    pub confidence: f32,
    /// A voice is in it (known when its vocals have been separated).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub voice: bool,
}

impl Slice {
    /// A drum's or percussion's hit (not a chord, a note, a voice or an effect).
    pub fn drum(&self) -> bool {
        self.category.is_drum() || self.category == Category::Drums
    }
}

/// What's kept about a file: how much of it is a voice, once known, and its slices (of it as it
/// is, or of it without its voice), by the file sliced.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Kept {
    /// What's in it, as heard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heard: Option<InIt>,
    /// By the file sliced and the stretch of it ("Audio Files/a.wav 0.00-8.00").
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub slices: BTreeMap<String, Vec<Slice>>,
}

/// What a file's slices of a stretch of it are kept as (a voice's apart: they're named apart).
fn slices_key(file: &Path, from: f32, to: f32, voice: bool) -> String {
    let as_voice = if voice { " voice" } else { "" };
    format!("{} {from:.2}-{to:.2}{as_voice}", file.display())
}

fn kept_at(dir: &Path, file: &Path) -> PathBuf {
    let name = file
        .file_stem()
        .map_or("audio".into(), |s| s.to_string_lossy().into_owned());
    dir.join("Audio Files")
        .join("Slices")
        .join(format!("{name}.json"))
}

/// What's kept about `file` (relative to the song's folder `dir`); nothing, at first.
pub fn kept(dir: &Path, file: &Path) -> Kept {
    std::fs::read_to_string(kept_at(dir, file))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn keep(dir: &Path, file: &Path, k: &Kept) {
    let at = kept_at(dir, file);
    if let Some(parent) = at.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(at, serde_json::to_string(k).unwrap_or_default());
}

/// The note names, by pitch class.
const NOTES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// What a slice sounds like, from its analysis: its category; a chord with its key ("Chord Am"),
/// a pitched sound with its note ("Bass F#"). What the library would call an effect, a texture or
/// a beat, cut from a mix, is a hit (short) or a phrase, in its key if it has one.
fn name_of(a: &Analysis, seconds: f32) -> String {
    let c = a.category;
    if matches!(
        c,
        Category::Fx | Category::Texture | Category::Other | Category::Drums
    ) {
        let what = if seconds < 0.3 { "Hit" } else { "Phrase" };
        return match &a.key {
            Some(k) if a.tonal > 0.5 && k.confidence > 0.2 => format!(
                "{what} {}{}",
                NOTES[(k.tonic % 12) as usize],
                if k.minor { "m" } else { "" }
            ),
            _ => what.to_string(),
        };
    }
    let pitched = !c.is_drum()
        && !matches!(
            c,
            Category::Drums
                | Category::Vocal
                | Category::Fx
                | Category::Reverse
                | Category::Texture
                | Category::Foley
                | Category::Other
        );
    match (&a.key, &a.pitch) {
        (Some(k), _) if c == Category::Chord && k.confidence > 0.2 => format!(
            "Chord {}{}",
            NOTES[(k.tonic % 12) as usize],
            if k.minor { "m" } else { "" }
        ),
        (_, Some(p)) if pitched && p.confidence > 0.5 => {
            format!("{} {}", c.name(), NOTES[(p.note % 12) as usize])
        }
        _ => c.name().to_string(),
    }
}

/// `sides` (at `rate`) cut at its hits, each slice to the next hit, named for what it sounds
/// like; silence left out. A `voice` (a voice alone: a vocals stem, an acapella) is cut at its
/// syllables and phrases, each a vocal chop named for its note, never for an instrument the
/// library would hear in it. `progress` goes 0 to 1; `stop` ends it with what's cut so far.
pub fn slices(
    sides: &[Vec<f32>],
    rate: f64,
    voice: bool,
    mut progress: impl FnMut(f32),
    stop: &AtomicBool,
) -> Vec<Slice> {
    let n = sides.iter().map(Vec::len).min().unwrap_or(0);
    if n == 0 {
        return Vec::new();
    }
    let mono: Vec<f32> = (0..n)
        .map(|i| sides.iter().map(|s| s[i]).sum::<f32>() / sides.len() as f32)
        .collect();
    let secs = n as f32 / rate as f32;
    let mut cuts: Vec<f32> = std::iter::once(0.0)
        .chain(
            crate::dsp::sampler::onsets(&mono, rate as f32)
                .into_iter()
                .map(|(at, _)| at * secs),
        )
        .collect();
    cuts.sort_by(f32::total_cmp);
    cuts.dedup_by(|b, a| *b - *a < CLOSEST);
    cuts.truncate(MOST);
    let mut out = Vec::new();
    for k in 0..cuts.len() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let (a, b) = (cuts[k], cuts.get(k + 1).copied().unwrap_or(secs));
        let (fa, fb) = (
            (a as f64 * rate) as usize,
            ((b as f64 * rate) as usize).min(n),
        );
        if fb <= fa + (0.02 * rate) as usize {
            continue;
        }
        // It ends where it dies away (the last hundredth of a second that isn't silence), not
        // with the silence after it.
        let hundredth = ((0.01 * rate) as usize).max(1);
        let loud = |f: usize| {
            let g = (f + hundredth).min(fb);
            (mono[f..g].iter().map(|x| x * x).sum::<f32>() / (g - f).max(1) as f32).sqrt() > 0.003
        };
        // Silence between hits isn't a slice.
        // The next hit's first moments (an onset found a hair late) aren't this one's.
        let lead = (0.02 * rate) as usize;
        let body = match fb > fa + 2 * lead && (fb - lead..fb).step_by(hundredth).any(&loud) {
            true => fb - lead,
            false => fb,
        };
        let Some(last) = (fa..body).step_by(hundredth).rev().find(|&f| loud(f)) else {
            progress((k + 1) as f32 / cuts.len() as f32);
            continue;
        };
        let fb = (last + 2 * hundredth).min(fb);
        let b = (fb as f64 / rate) as f32;
        {
            let part: Vec<Vec<f32>> = sides.iter().map(|s| s[fa..fb].to_vec()).collect();
            let a_ = crate::samples::analyze::analyze(&part, rate, "");
            out.push(match voice {
                true => Slice {
                    start: a,
                    end: b,
                    name: match &a_.pitch {
                        Some(p) if p.confidence > 0.4 => {
                            format!("Vocal {}", NOTES[(p.note % 12) as usize])
                        }
                        _ => "Vocal".into(),
                    },
                    category: Category::Vocal,
                    confidence: a_.pitch.as_ref().map_or(0.5, |p| p.confidence),
                    voice: true,
                },
                false => Slice {
                    start: a,
                    end: b,
                    name: name_of(&a_, b - a),
                    category: a_.category,
                    confidence: a_.confidence,
                    voice: false,
                },
            });
        }
        progress((k + 1) as f32 / cuts.len() as f32);
    }
    out
}

/// What's in `file`, by its sound (never its name): as the quickest vocals model hears a few
/// stretches of it, how much of its sound is a voice; without such a model, a voice alone if the
/// sample analysis hears a vocal, else (it can't tell a voice in a beat) a beat, and `false`: a
/// guess. Kept once heard.
pub fn hear(
    dir: &Path,
    file: &Path,
    progress: impl FnMut(f32),
    stop: &AtomicBool,
) -> Result<(InIt, bool), String> {
    let mut k = kept(dir, file);
    if let Some(h) = k.heard {
        return Ok((h, true));
    }
    let (sides, rate) = crate::audio::engine::read_audio(&dir.join(file))?;
    let Some(model) = crate::lyrics::quickest_vocals_model(&crate::separate::folder()) else {
        let name = file
            .file_stem()
            .map_or("audio".into(), |s| s.to_string_lossy().into_owned());
        return Ok((
            match crate::lyrics::voice_alone(&sides, rate, &name) {
                true => InIt::Voice,
                false => InIt::Beat,
            },
            false,
        ));
    };
    let (moments, sound) = crate::separate::vocal_share(&model, &sides, rate, progress, stop)?;
    let heard = InIt::of(moments, sound);
    if !stop.load(Ordering::Relaxed) {
        k.heard = Some(heard);
        keep(dir, file, &k);
    }
    Ok((heard, true))
}

/// What's in `file` put right by the person: kept, so it's sliced that way from now on.
pub fn put_right(dir: &Path, file: &Path, heard: InIt) -> Result<(), String> {
    let mut k = kept(dir, file);
    k.heard = Some(heard);
    keep(dir, file, &k);
    Ok(())
}

/// A voice's chops named by the words sung in them (each word in the chop its middle falls in),
/// once its words have been heard: "letting go". A chop with no word in it (a breath, an ad lib)
/// keeps its note.
pub fn name_by_words(slices: &mut [Slice], words: &[crate::lyrics::Word]) {
    for s in slices {
        let sung: Vec<&str> = words
            .iter()
            .filter(|w| (s.start..s.end).contains(&((w.start + w.end) / 2.0)))
            .map(|w| w.text.trim())
            .filter(|t| !t.is_empty())
            .collect();
        s.name = match sung.len() {
            0 => continue,
            1..=4 => sung.join(" "),
            _ => format!("{} …", sung[..3].join(" ")),
        };
    }
}

/// `file` without its voice: its vocals separated by the best vocals model (kept, as Lyrics keeps
/// them), what's left: the model's instrumental stem, or its other stems together.
pub fn without_voice(
    dir: &Path,
    file: &Path,
    progress: impl FnMut(f32),
    stop: &AtomicBool,
) -> Result<PathBuf, String> {
    use crate::{audio::engine::read_audio, separate};
    let model = crate::lyrics::vocals_model(&separate::folder()).ok_or(
        "Taking the voice out needs a separation model with vocals: add one in Settings › Stems & \
         lyrics",
    )?;
    let folder = separate::stems_folder(file, &model.about.name);
    let rest = model
        .about
        .rest
        .clone()
        .unwrap_or_else(|| "instrumental".into());
    let wanted = folder.join(format!("{rest}.wav"));
    if dir.join(&wanted).is_file() {
        return Ok(wanted);
    }
    let made = separate::separate(&dir.join(file), &model, &dir.join(&folder), progress, stop)?;
    if dir.join(&wanted).is_file() {
        return Ok(wanted);
    }
    // No stem of what's left: the stems that aren't the voice, together.
    let mut sum: Option<(Vec<f32>, Vec<f32>, f64)> = None;
    for path in made.iter().filter(|p| !p.ends_with("vocals.wav")) {
        let (sides, rate) = read_audio(path)?;
        let first = sides.first().ok_or("an empty stem")?;
        let second = sides.get(1).unwrap_or(first);
        match &mut sum {
            None => sum = Some((first.clone(), second.clone(), rate)),
            Some((l, r, _)) => {
                for (a, b) in l.iter_mut().zip(first) {
                    *a += b;
                }
                for (a, b) in r.iter_mut().zip(second) {
                    *a += b;
                }
            }
        }
    }
    let (l, r, rate) = sum.ok_or("the separation gave nothing but the voice")?;
    separate::write(&dir.join(&wanted), &l, &r, rate.round() as u32)?;
    Ok(wanted)
}

/// A recording sliced: the file cut (the region's own, or it without its voice), its slices,
/// and its audio.
pub struct Sliced {
    pub file: PathBuf,
    pub slices: Vec<Slice>,
    pub audio: (Vec<Vec<f32>>, f64),
}

/// How far a `slice_file` has got, for a progress bar: taking the voice out (0 to 1), then
/// cutting (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Going {
    Separating(f32),
    Slicing(f32),
}

/// `file` (relative to the song's folder `dir`) sliced, from `from` to `to` seconds into it (the
/// stretch its region plays; all of it if none): as it is, or `unvoiced` (its voice taken out
/// first). A `voice` alone (as heard, or as the person says) is cut into vocal chops, named by
/// their words once they're heard (else their notes); else, as it is, a slice with the voice in
/// it is marked so when its vocals have been separated before. Kept, so slicing it again is
/// instant; slice times are seconds into the file.
pub fn slice_file(
    dir: &Path,
    file: &Path,
    stretch: Option<(f32, f32)>,
    unvoiced: bool,
    voice: bool,
    mut progress: impl FnMut(Going),
    stop: &AtomicBool,
) -> Result<Sliced, String> {
    let cut = match unvoiced {
        true => without_voice(dir, file, |p| progress(Going::Separating(p)), stop)?,
        false => file.to_path_buf(),
    };
    let audio = crate::audio::engine::read_audio(&dir.join(&cut))?;
    let (sides, rate) = &audio;
    let secs = sides.first().map_or(0, Vec::len) as f32 / *rate as f32;
    let voice = voice && !unvoiced;
    let words = match voice {
        true => crate::lyrics::kept_words(dir, file),
        false => Vec::new(),
    };
    let (from, to) = stretch.map_or((0.0, secs), |(a, b)| {
        (a.clamp(0.0, secs), b.clamp(0.0, secs))
    });
    let key = slices_key(&cut, from, to, voice);
    let mut k = kept(dir, file);
    if let Some(s) = k.slices.get(&key).filter(|s| !s.is_empty()) {
        let mut slices = s.clone();
        name_by_words(&mut slices, &words);
        return Ok(Sliced {
            file: cut,
            slices,
            audio,
        });
    }
    let (a, b) = ((from as f64 * rate) as usize, (to as f64 * rate) as usize);
    let part: Vec<Vec<f32>> = sides
        .iter()
        .map(|s| s[a.min(s.len())..b.min(s.len())].to_vec())
        .collect();
    let mut slices = slices(&part, *rate, voice, |p| progress(Going::Slicing(p)), stop);
    for s in &mut slices {
        s.start += from;
        s.end += from;
    }
    if stop.load(Ordering::Relaxed) {
        return Err("Stopped".into());
    }
    if !unvoiced
        && !voice
        && let Some(vocals) = kept_vocals(dir, file)
    {
        mark_voice(&mut slices, &audio, &vocals);
    }
    if !slices.is_empty() {
        k.slices.insert(key, slices.clone());
        keep(dir, file, &k);
    }
    name_by_words(&mut slices, &words);
    Ok(Sliced {
        file: cut,
        slices,
        audio,
    })
}

/// `file`'s vocals stem, if they've been separated (by any model).
fn kept_vocals(dir: &Path, file: &Path) -> Option<(Vec<Vec<f32>>, f64)> {
    let name = file.file_stem()?.to_string_lossy().into_owned();
    let stems = dir.join("Audio Files").join("Stems");
    std::fs::read_dir(&stems)
        .ok()?
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("{name} · "))
        })
        .map(|e| e.path().join("vocals.wav"))
        .find(|p| p.is_file())
        .and_then(|p| crate::audio::engine::read_audio(&p).ok())
}

/// Marks the slices whose stretch of `voice` (the mix's vocals stem) is loud against the mix.
fn mark_voice(slices: &mut [Slice], mix: &(Vec<Vec<f32>>, f64), voice: &(Vec<Vec<f32>>, f64)) {
    let rms = |sides: &[Vec<f32>], rate: f64, a: f32, b: f32| {
        let (fa, fb) = ((a as f64 * rate) as usize, (b as f64 * rate) as usize);
        let mut sum = 0.0f32;
        let mut n = 0usize;
        for s in sides {
            for x in &s[fa.min(s.len())..fb.min(s.len())] {
                sum += x * x;
                n += 1;
            }
        }
        (sum / n.max(1) as f32).sqrt()
    };
    for s in slices {
        let m = rms(&mix.0, mix.1, s.start, s.end);
        let v = rms(&voice.0, voice.1, s.start, s.end);
        s.voice = v > 0.003 && v > 0.3 * m;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bar of a beat at 120 bpm and 48 kHz: a low thump on 1 and 3, a noise burst on 2 and 4,
    /// short ticks on the eighths between, and silence after.
    fn beat() -> Vec<Vec<f32>> {
        let rate = 48_000.0;
        let mut x = vec![0.0f32; 48_000 * 3];
        let mut seed = 1u32;
        let mut noise = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32 * 2.0 - 1.0
        };
        for eighth in 0..8 {
            let at = eighth * 12_000;
            for i in 0..11_000 {
                let t = i as f32 / rate;
                let fade = (-t * 40.0).exp();
                x[at + i] += match eighth {
                    0 | 4 => (t * 55.0 * std::f32::consts::TAU).sin() * fade * 0.9,
                    2 | 6 => noise() * fade * 0.6,
                    _ if i < 1_500 => noise() * (-t * 120.0).exp() * 0.3,
                    _ => 0.0,
                };
            }
        }
        vec![x]
    }

    #[test]
    fn a_beat_is_cut_at_its_hits_each_slice_named_and_silence_left_out() {
        let s = slices(&beat(), 48_000.0, false, |_| {}, &AtomicBool::new(false));
        // Eight hits a quarter second apart, none in the silence after them.
        assert_eq!(s.len(), 8, "{s:#?}");
        for (k, x) in s.iter().enumerate() {
            assert!((x.start - k as f32 * 0.25).abs() < 0.03, "{k}: {x:?}");
        }
        // The last ends where it dies away, not a second later with the silence.
        assert!(s[7].end < 2.05, "{:?}", s[7]);
        // Named for what they are: something low on the ones, something noisy between.
        assert!(s.iter().all(|x| !x.name.is_empty()));
        assert_ne!(s[0].category, s[2].category, "{s:#?}");
    }

    fn wav(path: &Path, x: &[f32]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        x.iter().for_each(|v| w.write_sample(*v).unwrap());
        w.finalize().unwrap();
    }

    #[test]
    fn a_stretch_is_sliced_as_it_is_its_voiced_slices_marked_from_a_kept_vocals_stem() {
        let dir = std::env::temp_dir().join(format!("pina-smart-file-{}", std::process::id()));
        let file = PathBuf::from("Audio Files/song.wav");
        let mut mix = beat().remove(0);
        // A voice on beat 3 (1.0 s to 1.25 s), in the mix and alone in its vocals stem.
        let mut voice = vec![0.0f32; mix.len()];
        for i in 48_000..60_000 {
            let v = (i as f32 / 48_000.0 * 330.0 * std::f32::consts::TAU).sin() * 0.5;
            voice[i] = v;
            mix[i] += v;
        }
        wav(&dir.join(&file), &mix);
        wav(
            &dir.join("Audio Files/Stems/song · Test/vocals.wav"),
            &voice,
        );
        let stop = AtomicBool::new(false);
        let cut = slice_file(&dir, &file, Some((0.5, 2.0)), false, false, |_| {}, &stop).unwrap();
        assert_eq!(cut.file, file);
        // Only the stretch, in the file's seconds.
        assert!(
            cut.slices
                .iter()
                .all(|x| x.start >= 0.5 && x.end <= 2.0 + 1e-3)
        );
        assert!(
            (cut.slices[0].start - 0.5).abs() < 0.03,
            "{:?}",
            cut.slices[0]
        );
        let voiced: Vec<f32> = cut
            .slices
            .iter()
            .filter(|x| x.voice)
            .map(|x| x.start)
            .collect();
        assert!(
            !voiced.is_empty() && voiced.iter().all(|&t| (0.95..1.3).contains(&t)),
            "{voiced:?}"
        );
        // Kept: the same again, without slicing.
        assert_eq!(kept(&dir, &file).slices.len(), 1);
        let again = slice_file(
            &dir,
            &file,
            Some((0.5, 2.0)),
            false,
            false,
            |_| panic!("sliced again"),
            &stop,
        );
        assert_eq!(again.unwrap().slices, cut.slices);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_voice_alone_is_cut_into_vocal_chops_named_by_note_then_by_the_words_sung() {
        let dir = std::env::temp_dir().join(format!("pina-smart-voice-{}", std::process::id()));
        // Named nothing like a voice: it's what's heard (or said) that counts.
        let file = PathBuf::from("Audio Files/take 3.wav");
        // Three sung notes (A, C#, E) with breaths between: what the library might call a brass
        // stab or a bell.
        let mut x = vec![0.0f32; 48_000 * 3];
        for (k, hz) in [220.0f32, 277.18, 329.63].iter().enumerate() {
            let at = k * 48_000;
            for i in 0..30_000 {
                let t = i as f32 / 48_000.0;
                let env = (t * 40.0).min(1.0) * (1.0 - i as f32 / 30_000.0);
                x[at + i] = (t * hz * std::f32::consts::TAU).sin() * env * 0.5;
            }
        }
        wav(&dir.join(&file), &x);
        let stop = AtomicBool::new(false);
        let cut = slice_file(&dir, &file, None, false, true, |_| {}, &stop).unwrap();
        assert_eq!(cut.slices.len(), 3, "{:#?}", cut.slices);
        assert!(
            cut.slices.iter().all(|s| s.name.starts_with("Vocal")),
            "{:#?}",
            cut.slices
        );
        assert_eq!(cut.slices[0].name, "Vocal A");
        // Each to where it dies away, not into the next one's breath.
        assert!(
            cut.slices.iter().all(|s| s.end - s.start < 0.75),
            "{:#?}",
            cut.slices
        );
        assert!(
            cut.slices
                .iter()
                .all(|s| s.voice && s.category == Category::Vocal)
        );
        // Its words heard (Lyrics): each chop named by what's sung in it, kept slices too.
        let lyrics = dir.join("Audio Files/Lyrics/take 3 · Whisper small.json");
        std::fs::create_dir_all(lyrics.parent().unwrap()).unwrap();
        let word = |t: &str, a: f32, b: f32| serde_json::json!({"text": t, "start": a, "end": b});
        let lines = serde_json::json!({"file": file, "lines": [{
            "text": "falling for you", "start": 0.0, "end": 2.6, "sung": 1,
            "words": [word("falling", 0.0, 0.6), word("for", 1.0, 1.3), word("you", 2.0, 2.6)]}]});
        std::fs::write(&lyrics, lines.to_string()).unwrap();
        let again = slice_file(&dir, &file, None, false, true, |_| {}, &stop).unwrap();
        let named: Vec<&str> = again.slices.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(named, ["falling", "for", "you"]);
        // Said to be a beat: sliced and kept apart, not as a voice.
        let beat = slice_file(&dir, &file, None, false, false, |_| {}, &stop).unwrap();
        assert!(beat.slices.iter().all(|s| !s.voice));
        assert_eq!(kept(&dir, &file).slices.len(), 2);
        // As measured: a vocals stem (0.65 of its moments, 0.94 of its sound), a song (0.47,
        // 0.18), drums (0, 0).
        assert_eq!(InIt::of(0.65, 0.94), InIt::Voice);
        assert_eq!(InIt::of(0.47, 0.18), InIt::Mixed);
        assert_eq!(InIt::of(0.0, 0.0002), InIt::Beat);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn whats_found_is_kept_and_read_back_by_file() {
        let dir = std::env::temp_dir().join(format!("pina-smart-{}", std::process::id()));
        let file = PathBuf::from("Audio Files/beat.wav");
        let mut k = kept(&dir, &file);
        assert!(k.heard.is_none() && k.slices.is_empty());
        k.heard = Some(InIt::Mixed);
        k.slices.insert(
            slices_key(&file, 0.0, 1.0, false),
            vec![Slice {
                start: 0.0,
                end: 0.25,
                name: "Kick".into(),
                category: Category::Kick,
                confidence: 0.9,
                voice: false,
            }],
        );
        keep(&dir, &file, &k);
        let back = kept(&dir, &file);
        assert_eq!(back.heard, Some(InIt::Mixed));
        assert_eq!(
            back.slices[&slices_key(&file, 0.0, 1.0, false)][0].name,
            "Kick"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
