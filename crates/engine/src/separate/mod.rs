//! Stems: a recording parted into its instruments (vocals, drums, bass, the rest) by a
//! separation model the person brings. Pina Studio ships no model and fetches none. A model is
//! a folder in `Models` (in the support folder) holding its ONNX file and a `model.json` saying
//! what it gives and how it reads sound:
//!
//! ```json
//! { "name": "BS-RoFormer SW", "stems": ["bass", "drums", "other", "vocals", "guitar", "piano"],
//!   "input": "spec_split", "n_fft": 2048, "hop": 512 }
//! ```
//!
//! Three ways in cover the separators people use: `wave` (stereo samples in, each stem's out:
//! HTDemucs), `spec` (a spectrogram, each channel's real and imaginary parts as planes, left's
//! then right's: MDX-Net, Mel-Band RoFormer) and `spec_split` (its real and imaginary parts as
//! two inputs: BS-RoFormer). Pina Studio does the rest: reads the file at the model's rate, its
//! STFT and inverse, runs the model a segment at a time, overlapping and crossfaded, and writes
//! each stem. The model runs in ONNX Runtime (`run`), on Apple silicon and Windows x64.

pub mod catalog;
pub mod stft;

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(windows, target_arch = "x86_64")
))]
mod run;

#[cfg(test)]
mod tests;

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use stft::{Stft, reflect};

/// How a model takes sound in (and gives its stems back the same way).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    /// Samples: [1, 2, segment] in, [1, stems, 2, segment] out.
    Wave,
    /// A spectrogram, left's real and imaginary parts then right's: [1, 4, bins, frames] in,
    /// [1, stems, 4, bins, frames] out.
    Spec,
    /// A spectrogram's real and imaginary parts as two inputs, [1, 2, bins, frames] each; two
    /// outputs, [1, stems, 2, bins, frames] each.
    SpecSplit,
}

/// A model's `model.json`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct About {
    pub name: String,
    /// Its ONNX file, in its folder.
    #[serde(default = "model_file")]
    pub file: String,
    /// What it gives, in its outputs' order.
    pub stems: Vec<String>,
    /// Also a stem of what's left, the sound less all of them, by this name ("instrumental").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<String>,
    /// The sample rate it was trained at.
    #[serde(default = "rate")]
    pub rate: u32,
    pub input: Input,
    /// Samples a run takes: as its input's shape says, if it doesn't say here.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub segment: usize,
    /// Runs over each segment's length: 2 overlaps each by half.
    #[serde(default = "overlap")]
    pub overlap: usize,
    /// A spectrogram's window and hop (samples).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub n_fft: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub hop: usize,
    /// Of a spectrogram's bins, how many it reads from the bottom (MDX-Net's `dim_f`); those
    /// over come back silent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bins: Option<usize>,
    /// Run on the GPU (Core ML on a Mac, DirectML on Windows) rather than the CPU: faster for a
    /// model that works there, but some never finish compiling or running on Core ML, so only
    /// when its model.json asks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gpu: bool,
    /// Whose it is and on what terms, as the person found it: shown, never checked.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub licence: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

fn model_file() -> String {
    "model.onnx".into()
}
fn rate() -> u32 {
    44_100
}
fn overlap() -> usize {
    2
}
fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// A model in its folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub dir: PathBuf,
    pub about: About,
}

impl Model {
    pub fn read(dir: &Path) -> Result<Model, String> {
        let json = dir.join("model.json");
        let text =
            std::fs::read_to_string(&json).map_err(|e| format!("{}: {e}", json.display()))?;
        let about: About =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", json.display()))?;
        if about.stems.is_empty() {
            return Err(format!("{}: no stems", json.display()));
        }
        let spec = about.input != Input::Wave;
        if spec && (about.n_fft < 2 || about.hop == 0) {
            return Err(format!(
                "{}: a spectrogram model needs its n_fft and hop",
                json.display()
            ));
        }
        if !dir.join(&about.file).is_file() {
            return Err(format!("{}: no {}", dir.display(), about.file));
        }
        Ok(Model {
            dir: dir.to_path_buf(),
            about,
        })
    }

    pub fn file(&self) -> PathBuf {
        self.dir.join(&self.about.file)
    }
}

/// Where a song keeps a file's stems by the model called `model`, from the song's folder: each
/// file and model a folder of their own, so they're made once.
pub fn stems_folder(file: &Path, model: &str) -> PathBuf {
    let name = file
        .file_stem()
        .map_or("audio".into(), |s| s.to_string_lossy());
    PathBuf::from("Audio Files")
        .join("Stems")
        .join(format!("{name} · {model}"))
}

/// Where models are kept: a folder each (`PINA_MODELS` says elsewhere).
pub fn folder() -> PathBuf {
    match std::env::var_os("PINA_MODELS") {
        Some(d) => d.into(),
        None => crate::paths::support().join("Models"),
    }
}

/// The models in `dir`, by name; and each folder there that isn't one, with why.
pub fn models(dir: &Path) -> (Vec<Model>, Vec<String>) {
    let mut found = Vec::new();
    let mut wrong = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    // A model put straight in Models (its files there, not in a folder of their own) counts too.
    if dir.join("model.json").is_file() {
        dirs.insert(0, dir.to_path_buf());
    }
    for d in dirs {
        match Model::read(&d) {
            Ok(m) => found.push(m),
            Err(e) => wrong.push(e),
        }
    }
    found.sort_by_key(|m| m.about.name.to_lowercase());
    (found, wrong)
}

/// The model named `name` (any case, or the start of its name), or the first there is.
pub fn find(dir: &Path, name: Option<&str>) -> Result<Model, String> {
    let (found, wrong) = models(dir);
    let none = || match wrong.first() {
        Some(e) => format!("No separation model in {}: {e}", dir.display()),
        None => format!(
            "No separation model in {}: add a folder with its ONNX file and a model.json",
            dir.display()
        ),
    };
    match name {
        None => found.into_iter().next().ok_or_else(none),
        Some(n) => {
            let n = n.to_lowercase();
            let names: Vec<String> = found.iter().map(|m| m.about.name.clone()).collect();
            found
                .into_iter()
                .find(|m| m.about.name.to_lowercase().starts_with(&n))
                .ok_or_else(|| match names.is_empty() {
                    true => none(),
                    false => format!("No model called {n}: there's {}", names.join(", ")),
                })
        }
    }
}

/// A tensor as ONNX Runtime takes and gives it: its shape, and its values in order.
pub type Tensor = (Vec<usize>, Vec<f32>);

/// A stem: its name and its two channels.
pub type Stem = (String, [Vec<f32>; 2]);

/// `audio` (two channels at the model's rate) parted into its stems, `infer` running the model
/// on each segment `segment` samples long; `progress` told how far it's got (0 to 1). Stopped,
/// with an error, when `stop` is set.
pub fn demix(
    about: &About,
    segment: usize,
    audio: &[Vec<f32>; 2],
    mut infer: impl FnMut(Vec<Tensor>) -> Result<Vec<Tensor>, String>,
    mut progress: impl FnMut(f32),
    stop: &AtomicBool,
) -> Result<Vec<Stem>, String> {
    let len = audio[0].len().min(audio[1].len());
    let segment = segment.max(1);
    let step = (segment / about.overlap.max(1)).max(1);
    // Padded by reflection, as Music-Source-Separation-Training's demix does, so the ends are
    // heard as much as the middle.
    let pad = segment - step;
    let padded = [
        reflect(&audio[0][..len], pad),
        reflect(&audio[1][..len], pad),
    ];
    let total = padded[0].len();
    // Each run's weight: a linear fade over a tenth of it at each end.
    let fade = (segment / 10).max(1) as f32;
    let window: Vec<f32> = (0..segment)
        .map(|i| {
            ((i + 1) as f32 / fade)
                .min((segment - i) as f32 / fade)
                .min(1.0)
        })
        .collect();
    let stft = (about.input != Input::Wave).then(|| Stft::new(about.n_fft, about.hop));
    let count = about.stems.len();
    let mut sum = vec![[vec![0f32; total], vec![0f32; total]]; count];
    let mut weight = vec![0f32; total];
    let starts: Vec<usize> = (0..pad + len).step_by(step).collect();
    for (k, &at) in starts.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            return Err("Stopped".into());
        }
        let part = |c: usize| -> Vec<f32> {
            (at..at + segment)
                .map(|i| padded[c].get(i).copied().unwrap_or(0.0))
                .collect()
        };
        let seg = [part(0), part(1)];
        let outs = infer(tensors(about, stft.as_ref(), &seg))?;
        let stems = stems(about, stft.as_ref(), outs, segment)?;
        for (s, stem) in stems.iter().enumerate() {
            for c in 0..2 {
                for (i, v) in stem[c].iter().enumerate().take(total.saturating_sub(at)) {
                    sum[s][c][at + i] += v * window[i];
                }
            }
        }
        for (i, w) in window.iter().enumerate().take(total.saturating_sub(at)) {
            weight[at + i] += w;
        }
        progress((k + 1) as f32 / starts.len() as f32);
    }
    let mut out: Vec<Stem> = about
        .stems
        .iter()
        .zip(sum)
        .map(|(name, [l, r])| {
            let even = |x: Vec<f32>| -> Vec<f32> {
                (pad..pad + len)
                    .map(|i| match weight[i] > 1e-6 {
                        true => x[i] / weight[i],
                        false => 0.0,
                    })
                    .collect()
            };
            (name.clone(), [even(l), even(r)])
        })
        .collect();
    if let Some(name) = &about.rest {
        let rest = |c: usize| -> Vec<f32> {
            (0..len)
                .map(|i| audio[c][i] - out.iter().map(|s| s.1[c][i]).sum::<f32>())
                .collect()
        };
        let stem = (name.clone(), [rest(0), rest(1)]);
        out.push(stem);
    }
    Ok(out)
}

/// A segment as the model takes it.
fn tensors(about: &About, stft: Option<&Stft>, seg: &[Vec<f32>; 2]) -> Vec<Tensor> {
    let n = seg[0].len();
    let Some(stft) = stft else {
        return vec![(vec![1, 2, n], [seg[0].as_slice(), &seg[1]].concat())];
    };
    let specs = [stft.forward(&seg[0]), stft.forward(&seg[1])];
    let frames = specs[0].len();
    let bins = about.bins.unwrap_or(stft.bins()).min(stft.bins());
    // A plane: one channel's real or imaginary part, bin by bin, each across the frames.
    let plane = |c: usize, im: bool| -> Vec<f32> {
        (0..bins)
            .flat_map(|f| {
                specs[c]
                    .iter()
                    .map(move |frame| if im { frame[f].im } else { frame[f].re })
            })
            .collect()
    };
    match about.input {
        Input::Spec => {
            let data = [
                plane(0, false),
                plane(0, true),
                plane(1, false),
                plane(1, true),
            ];
            vec![(vec![1, 4, bins, frames], data.concat())]
        }
        _ => vec![
            (
                vec![1, 2, bins, frames],
                [plane(0, false), plane(1, false)].concat(),
            ),
            (
                vec![1, 2, bins, frames],
                [plane(0, true), plane(1, true)].concat(),
            ),
        ],
    }
}

/// The model's outputs for a segment `n` samples long, as each stem's two channels.
fn stems(
    about: &About,
    stft: Option<&Stft>,
    outs: Vec<Tensor>,
    n: usize,
) -> Result<Vec<[Vec<f32>; 2]>, String> {
    let count = about.stems.len();
    let short = |got: usize, want: usize| {
        format!(
            "{} gave {got} values a run, not the {want} its {count} stems need: are its stems, \
             input and segment right in its model.json?",
            about.name
        )
    };
    let first = outs.first().map(|o| &o.1);
    let Some(stft) = stft else {
        let v = first.ok_or("The model gave nothing")?;
        if v.len() < count * 2 * n {
            return Err(short(v.len(), count * 2 * n));
        }
        return Ok((0..count)
            .map(|s| [0, 1].map(|c| v[(s * 2 + c) * n..][..n].to_vec()))
            .collect());
    };
    let frames = stft.frames(n);
    let bins = about.bins.unwrap_or(stft.bins()).min(stft.bins());
    let size = bins * frames;
    // Each stem's channel's real and imaginary planes.
    let planes: Vec<[[&[f32]; 2]; 2]> = match about.input {
        Input::Spec => {
            let v = first.ok_or("The model gave nothing")?;
            if v.len() < count * 4 * size {
                return Err(short(v.len(), count * 4 * size));
            }
            let at = |k: usize| &v[k * size..][..size];
            (0..count)
                .map(|s| [0, 1].map(|c| [at(s * 4 + c * 2), at(s * 4 + c * 2 + 1)]))
                .collect()
        }
        _ => {
            let (Some(re), Some(im)) = (outs.first(), outs.get(1)) else {
                return Err(format!(
                    "{} gave one output, not two (real, imaginary)",
                    about.name
                ));
            };
            if re.1.len() < count * 2 * size || im.1.len() < count * 2 * size {
                return Err(short(re.1.len().min(im.1.len()), count * 2 * size));
            }
            (0..count)
                .map(|s| {
                    [0, 1].map(|c| {
                        let k = s * 2 + c;
                        [&re.1[k * size..][..size], &im.1[k * size..][..size]]
                    })
                })
                .collect()
        }
    };
    Ok(planes
        .iter()
        .map(|stem| {
            stem.map(|[re, im]| {
                let spec: Vec<Vec<Complex32>> = (0..frames)
                    .map(|t| {
                        (0..stft.bins())
                            .map(|f| match f < bins {
                                true => Complex32::new(re[f * frames + t], im[f * frames + t]),
                                false => Complex32::default(),
                            })
                            .collect()
                    })
                    .collect();
                stft.inverse(&spec, n)
            })
        })
        .collect())
}

/// A model's inputs and outputs: each its name and shape.
pub type Shapes = (Vec<(String, Vec<i64>)>, Vec<(String, Vec<i64>)>);

/// The inputs and outputs of the ONNX file `file`, to write its model.json from.
pub fn inspect(file: &Path) -> Result<Shapes, String> {
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    ))]
    return run::Runner::inspect(file);
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    )))]
    {
        let _ = file;
        Err("Reading models needs a Mac with Apple silicon, or Windows on x64".into())
    }
}

/// Whether models can run here (Apple silicon, Windows x64).
pub fn available() -> bool {
    cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    ))
}

/// `file` parted by `model` into a WAV a stem in `out` (made if it isn't there), at the file's
/// own rate and as long, so a stem stands in for it sample for sample: each stem's file, in its
/// order. `progress` is told how far it's got (0 to 1); setting
/// `stop` stops it.
pub fn separate(
    file: &Path,
    model: &Model,
    out: &Path,
    progress: impl FnMut(f32),
    stop: &AtomicBool,
) -> Result<Vec<PathBuf>, String> {
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    ))]
    {
        use crate::audio::engine::{read_audio, resample};
        let about = &model.about;
        let (sides, own) = read_audio(file)?;
        let first = sides
            .first()
            .ok_or(format!("{}: no audio", file.display()))?;
        let at = |side: &[f32]| resample(side, own, about.rate as f64);
        let stereo = [at(first), at(sides.get(1).unwrap_or(first))];
        let mut runner = run::Runner::new(model)?;
        let segment = match about.segment {
            0 => runner.segment(about).ok_or(format!(
                "{}: its input's length isn't fixed: give its segment in model.json",
                about.name
            ))?,
            n => n,
        };
        let stems = demix(about, segment, &stereo, |t| runner.run(t), progress, stop)?;
        std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
        stems
            .iter()
            .map(|(name, [l, r])| {
                let path = out.join(format!("{name}.wav"));
                let back = |side: &[f32]| {
                    let mut v = resample(side, about.rate as f64, own);
                    v.resize(first.len(), 0.0);
                    v
                };
                write(&path, &back(l), &back(r), own.round() as u32)?;
                Ok(path)
            })
            .collect()
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    )))]
    {
        let _ = (file, model, out, progress, stop);
        Err("Separating stems needs a Mac with Apple silicon, or Windows on x64".into())
    }
}

/// How much of `sides` (at `rate`) is a voice, as `model` (one with a vocals stem) hears it in up
/// to three 4-second stretches spread through it (all of it, if it's short): the share of its
/// tenths of a second, those not silent, where the vocals stem is loud against the whole (is a
/// voice in it); and the share of its sound that's the voice (is it a voice alone: a vocal's
/// breaths, tails and silences don't count against it).
pub fn vocal_share(
    model: &Model,
    sides: &[Vec<f32>],
    rate: f64,
    mut progress: impl FnMut(f32),
    stop: &AtomicBool,
) -> Result<(f32, f32), String> {
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    ))]
    {
        use crate::audio::engine::resample;
        let about = &model.about;
        if !about.stems.iter().any(|s| s == "vocals") {
            return Err(format!("{} has no vocals stem", about.name));
        }
        let first = sides.first().ok_or("no audio")?;
        let at = |side: &[f32]| resample(side, rate, about.rate as f64);
        let (l, r) = (at(first), at(sides.get(1).unwrap_or(first)));
        let len = l.len().min(r.len());
        let win = 4 * about.rate as usize;
        let windows: Vec<(usize, usize)> = match len <= 3 * win {
            true => vec![(0, len)],
            false => [len / 6, len / 2, len * 5 / 6]
                .iter()
                .map(|&c| (c - win / 2, c + win / 2))
                .collect(),
        };
        let mut runner = run::Runner::new(model)?;
        let segment = match about.segment {
            0 => runner
                .segment(about)
                .ok_or(format!("{}: its segment isn't known", about.name))?,
            n => n,
        };
        let hop = (about.rate as usize / 10).max(1);
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt();
        let (mut voiced, mut heard) = (0usize, 0usize);
        let (mut voice_power, mut all_power) = (0f64, 0f64);
        let n = windows.len() as f32;
        for (k, &(a, b)) in windows.iter().enumerate() {
            let audio = [l[a..b].to_vec(), r[a..b].to_vec()];
            let stems = demix(
                about,
                segment,
                &audio,
                |t| runner.run(t),
                |p| progress((k as f32 + p) / n),
                stop,
            )?;
            let (_, [vl, vr]) = stems
                .iter()
                .find(|(name, _)| name == "vocals")
                .ok_or("no vocals stem came out")?;
            let mono = |x: &[f32], y: &[f32]| -> Vec<f32> {
                x.iter().zip(y).map(|(a, b)| (a + b) * 0.5).collect()
            };
            let (mix, voice) = (mono(&audio[0], &audio[1]), mono(vl, vr));
            voice_power += voice.iter().map(|x| (x * x) as f64).sum::<f64>();
            all_power += mix.iter().map(|x| (x * x) as f64).sum::<f64>();
            for f in (0..mix.len().min(voice.len())).step_by(hop) {
                let g = (f + hop).min(mix.len()).min(voice.len());
                let (m, v) = (rms(&mix[f..g]), rms(&voice[f..g]));
                // Silence says nothing either way.
                if m > 0.003 {
                    heard += 1;
                    if v > 0.003 && v > 0.3 * m {
                        voiced += 1;
                    }
                }
            }
        }
        Ok((
            voiced as f32 / heard.max(1) as f32,
            (voice_power / all_power.max(1e-12)).min(1.0) as f32,
        ))
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    )))]
    {
        let _ = (model, sides, rate, &mut progress, stop);
        Err("Listening for a voice needs a Mac with Apple silicon, or Windows on x64".into())
    }
}

/// A stereo WAV, 32-bit float, written beside and moved over, so stopping halfway leaves none.
#[cfg_attr(
    not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(windows, target_arch = "x86_64")
    )),
    allow(dead_code)
)]
pub(crate) fn write(path: &Path, l: &[f32], r: &[f32], rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let part = path.with_extension("wav.part");
    let err = |e: hound::Error| format!("{}: {e}", path.display());
    let mut w = hound::WavWriter::create(&part, spec).map_err(err)?;
    for (a, b) in l.iter().zip(r) {
        w.write_sample(*a).map_err(err)?;
        w.write_sample(*b).map_err(err)?;
    }
    w.finalize().map_err(err)?;
    std::fs::rename(&part, path).map_err(|e| format!("{}: {e}", path.display()))
}
