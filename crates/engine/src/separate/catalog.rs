//! Separation models that have been run with Pina Studio, and where their makers host them:
//! downloaded from there when the person asks (Pina Studio ships none), or known when the
//! person adds one they have. Each says what its host says of its licence; which to use, and
//! on what terms, is the person's to decide.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use super::{About, Input, Model};

/// A model that's been run, and where to get it.
pub struct Known {
    /// Its folder's name in Models.
    pub id: &'static str,
    /// Its model.json.
    pub json: &'static str,
    /// Where its maker hosts its ONNX file, and its size: how it's known when it's added, and
    /// that a download came down whole.
    pub url: &'static str,
    pub bytes: u64,
    /// What it gives, how well (measured against the parts of a mix of loops) and how long a
    /// three-minute song takes on an Apple silicon CPU.
    pub heard: &'static str,
    /// Its licence, as its host states it, and whether that's an open one.
    pub licence: &'static str,
    pub open: bool,
    /// Seconds it takes a second of sound on an Apple silicon CPU, as measured: for how long a
    /// recording will take.
    pub speed: f32,
}

pub const KNOWN: &[Known] = &[
    Known {
        id: "bs-roformer-sw",
        json: r#"{"name": "BS-RoFormer SW", "stems": ["bass", "drums", "other", "vocals", "guitar", "piano"],
            "input": "spec_split", "n_fft": 2048, "hop": 512,
            "source": "huggingface.co/elicwhite/bs-roformer-sw-6stem-onnx", "licence": "None stated by its host"}"#,
        url: "https://huggingface.co/elicwhite/bs-roformer-sw-6stem-onnx/resolve/main/bs_roformer_sw_6stem_fp16.onnx",
        bytes: 352_778_874,
        heard: "Drums, bass, vocals, guitar, piano and the rest · the best tested: drums 20.9 dB, bass 20.6, vocals 20.4 · about 8 min a song",
        licence: "None stated by its host",
        open: false,
        speed: 2.75,
    },
    Known {
        id: "mel-roformer-vocals",
        json: r#"{"name": "Mel-Band RoFormer vocals", "stems": ["vocals"], "rest": "instrumental",
            "input": "spec", "n_fft": 2048, "hop": 441,
            "source": "huggingface.co/audiojs/mel-roformer", "licence": "MIT"}"#,
        url: "https://huggingface.co/audiojs/mel-roformer/resolve/main/mel-roformer.fp16.onnx",
        bytes: 464_050_097,
        heard: "Vocals and instrumental, by Kimberley Jensen · the best vocals tested: vocals 21.2 dB, instrumental 29.2 · about 4½ min a song",
        licence: "MIT",
        open: true,
        speed: 1.5,
    },
    Known {
        id: "htdemucs-drums",
        json: r#"{"name": "HTDemucs drums", "stems": ["drums", "bass", "other", "vocals"], "input": "wave",
            "source": "huggingface.co/StemSplitio/htdemucs-ft-drums-onnx", "licence": "Code MIT; its weights' licence unclear"}"#,
        url: "https://huggingface.co/StemSplitio/htdemucs-ft-drums-onnx/resolve/main/htdemucs_ft_drums_fp16weights.onnx",
        bytes: 165_612_636,
        heard: "Drums (the other stems only rough) · drums 18.8 dB · about 2½ min a song",
        licence: "Code MIT; its weights' licence unclear",
        open: false,
        speed: 0.8,
    },
    Known {
        id: "kim-vocal-2",
        json: r#"{"name": "Kim Vocal 2", "stems": ["vocals"], "rest": "instrumental", "input": "spec",
            "n_fft": 7680, "hop": 1024, "bins": 3072,
            "source": "github.com/TRvlvr/model_repo", "licence": "None stated by its host"}"#,
        url: "https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/Kim_Vocal_2.onnx",
        bytes: 66_759_214,
        heard: "Vocals and instrumental, MDX-Net · vocals 18.0 dB, instrumental 26.0 · about 1½ min a song",
        licence: "None stated by its host",
        open: false,
        speed: 0.5,
    },
];

impl Known {
    pub fn about(&self) -> About {
        serde_json::from_str(self.json).expect("a known model's model.json")
    }

    /// The known model whose ONNX file is `bytes` long.
    pub fn sized(bytes: u64) -> Option<&'static Known> {
        KNOWN.iter().find(|k| k.bytes == bytes)
    }

    /// The known model called `name`.
    pub fn named(name: &str) -> Option<&'static Known> {
        KNOWN.iter().find(|k| k.about().name == name)
    }

    /// Whether it's in `dir` already: a model there by its name.
    pub fn installed(&self, models: &[Model]) -> bool {
        let name = self.about().name;
        models.iter().any(|m| m.about.name == name)
    }
}

/// `k` downloaded from where its maker hosts it into a folder of its own in `dir`, with its
/// model.json; by curl (the system's, on macOS and Windows alike). `progress` is told the bytes
/// come down so far; `stop` stops it, leaving nothing. Its folder.
pub fn download(
    k: &Known,
    dir: &Path,
    progress: impl FnMut(u64),
    stop: &AtomicBool,
) -> Result<PathBuf, String> {
    let folder = dir.join(k.id);
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let fetched =
        fetch(k.url, k.bytes, &folder.join("model.onnx"), progress, stop).and_then(|_| {
            std::fs::write(folder.join("model.json"), k.json)
                .map_err(|e| format!("{}: {e}", folder.display()))
        });
    if let Err(e) = fetched {
        let _ = std::fs::remove_dir_all(&folder);
        return Err(e);
    }
    Ok(folder)
}

/// The file at `url`, `bytes` long, downloaded to `to` by curl (the system's, on macOS and
/// Windows alike): written beside it and moved there only once it's all come down, so a stop
/// or a short download leaves nothing. `progress` is told the bytes come down so far.
pub fn fetch(
    url: &str,
    bytes: u64,
    to: &Path,
    mut progress: impl FnMut(u64),
    stop: &AtomicBool,
) -> Result<(), String> {
    let part = to.with_extension("part");
    let gone = || {
        let _ = std::fs::remove_file(&part);
    };
    let mut curl = Command::new("curl")
        .args(["-L", "--fail", "-sS", "-A", "PinaStudio", "-o"])
        .arg(&part)
        .arg(url)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start curl to download it: {e}"))?;
    let size = || std::fs::metadata(&part).map_or(0, |m| m.len());
    let done = loop {
        if stop.load(Ordering::Relaxed) {
            let _ = curl.kill();
            let _ = curl.wait();
            gone();
            return Err("Stopped".into());
        }
        match curl.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                progress(size());
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => {
                gone();
                return Err(e.to_string());
            }
        }
    };
    let name = to.file_name().map_or("it".into(), |n| n.to_string_lossy());
    if !done.success() {
        let mut why = String::new();
        if let Some(mut e) = curl.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut e, &mut why);
        }
        gone();
        return Err(format!("Couldn't download {name}: {}", why.trim()));
    }
    if size() != bytes {
        let got = size();
        gone();
        return Err(format!(
            "{name} came down {got} bytes, not its {bytes}: try again"
        ));
    }
    std::fs::rename(&part, to).map_err(|e| format!("{}: {e}", to.display()))?;
    progress(bytes);
    Ok(())
}

/// An ONNX file the person has, copied into a folder of its own in `dir`: with its model.json
/// if it's a known one; else a model.json guessed from its inputs and outputs, its stems left
/// for the person to name. Its folder, and whether it was known.
pub fn add(file: &Path, dir: &Path) -> Result<(PathBuf, bool), String> {
    let bytes = std::fs::metadata(file)
        .map_err(|e| format!("{}: {e}", file.display()))?
        .len();
    let known = Known::sized(bytes);
    let stem = file
        .file_stem()
        .map_or("model".into(), |s| s.to_string_lossy());
    let id = known.map_or(stem.to_string(), |k| k.id.to_string());
    let folder = (0..)
        .map(|n| match n {
            0 => dir.join(&id),
            n => dir.join(format!("{id} {}", n + 1)),
        })
        .find(|f| !f.exists())
        .expect("a free name");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let json = match known {
        Some(k) => k.json.to_string(),
        None => guess(file, &stem),
    };
    let copied = std::fs::copy(file, folder.join("model.onnx"))
        .and_then(|_| std::fs::write(folder.join("model.json"), json));
    if let Err(e) = copied {
        let _ = std::fs::remove_dir_all(&folder);
        return Err(format!("{}: {e}", folder.display()));
    }
    Ok((folder, known.is_some()))
}

/// A model.json for an unknown model, as far as its shapes say: how it takes sound in, its
/// shapes kept for the person to read. Its stems are left empty, for the person to name.
fn guess(file: &Path, name: &str) -> String {
    let shapes = super::inspect(file).ok();
    let input = match &shapes {
        Some((ins, _)) if ins.len() == 2 => Input::SpecSplit,
        Some((ins, _)) if ins.first().is_some_and(|i| i.1.len() == 4) => Input::Spec,
        _ => Input::Wave,
    };
    let list = |v: &[(String, Vec<i64>)]| -> Vec<serde_json::Value> {
        v.iter()
            .map(|(n, s)| serde_json::json!({"name": n, "shape": s}))
            .collect()
    };
    let mut json = serde_json::json!({
        "name": name,
        "stems": [],
        "input": input,
    });
    if input != Input::Wave {
        json["n_fft"] = 2048.into();
        json["hop"] = 512.into();
    }
    if let Some((ins, outs)) = shapes {
        json["inputs (from the file)"] = list(&ins).into();
        json["outputs (from the file)"] = list(&outs).into();
    }
    serde_json::to_string_pretty(&json).unwrap()
}
