//! Reading audio files of every format, and resampling them to the song's rate.

use super::*;

/// An audio file's audio (WAV, AIFF, FLAC or MP3) at `rate`: converted, if it was recorded at
/// another.
pub(crate) fn load_wav(path: &Path, rate: f64) -> Result<AudioData, String> {
    let (sides, from) = read_audio(path)?;
    let channels = sides
        .iter()
        .map(|side| resample(side, from, rate))
        .collect();
    Ok(AudioData { channels })
}

/// An audio file's audio at `rate`, converted if it's at another: for previews read off the
/// audio and main threads.
pub fn load_audio(path: &Path, rate: f64) -> Result<AudioData, String> {
    load_wav(path, rate)
}

/// An audio file's first two channels (one, if it's mono) at its own rate, and that rate. WAV is
/// read with hound; AIFF, FLAC, MP3, and WAVs hound can't read, with Symphonia.
pub fn read_audio(path: &Path) -> Result<(Vec<Vec<f32>>, f64), String> {
    let wav = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav") || e.eq_ignore_ascii_case("wave"));
    match wav.then(|| read_wav(path)) {
        Some(Ok(audio)) => Ok(audio),
        Some(Err(e)) => decode(path).map_err(|_| e),
        None => decode(path),
    }
}

pub(crate) fn read_wav(path: &Path) -> Result<(Vec<Vec<f32>>, f64), String> {
    let err = |e: hound::Error| format!("{}: {e}", path.display());
    let mut wav = hound::WavReader::open(path).map_err(err)?;
    let spec = wav.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => wav
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(err)?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            wav.samples::<i32>()
                .map(|s| s.map(|s| s as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(err)?
        }
    };
    let n = spec.channels as usize;
    let sides = (0..n.min(2))
        .map(|c| samples.iter().skip(c).step_by(n).copied().collect())
        .collect();
    Ok((sides, spec.sample_rate as f64))
}

/// Decodes a file with Symphonia: its first audio track's first two channels.
pub(crate) fn decode(path: &Path) -> Result<(Vec<Vec<f32>>, f64), String> {
    use symphonia::core::{
        codecs::audio::AudioDecoderOptions,
        errors::Error,
        formats::{FormatOptions, TrackType, probe::Hint},
        io::MediaSourceStream,
        meta::MetadataOptions,
    };
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(|e| err(&e))?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| err(&e))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| err(&"no audio in it"))?;
    let id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| err(&"no audio in it"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| err(&e))?;
    let (mut sides, mut block, mut rate) = (Vec::<Vec<f32>>::new(), Vec::new(), 0);
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // A file cut short: keep what was read.
            Err(Error::IoError(_)) if !sides.is_empty() => break,
            Err(e) => return Err(err(&e)),
        };
        if packet.track_id != id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(audio) => {
                rate = audio.spec().rate();
                audio.copy_to_vecs_planar::<f32>(&mut block);
                if sides.is_empty() {
                    sides = vec![Vec::new(); block.len().min(2)];
                }
                for (side, part) in sides.iter_mut().zip(&block) {
                    side.extend_from_slice(part);
                }
            }
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(err(&e)),
        }
    }
    if sides.is_empty() || rate == 0 {
        return Err(err(&"no audio in it"));
    }
    Ok((sides, rate as f64))
}

/// Audio at `from` Hz, at `to` Hz, band-limited: a Blackman-windowed sinc, 32 taps a side, its
/// cutoff a little under the lower Nyquist, so going down doesn't alias and going up doesn't
/// image. The filter is worked out once, at 512 points between samples, and read between them.
pub(crate) fn resample(samples: &[f32], from: f64, to: f64) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    const TAPS: usize = 32;
    const PHASES: usize = 512;
    let ratio = from / to;
    let cutoff = (to / from).min(1.0) * 0.96;
    let sinc = |x: f64| match x.abs() < 1e-9 {
        true => 1.0,
        false => (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x),
    };
    let blackman = |t: f64| {
        let a = std::f64::consts::PI * t;
        0.42 + 0.5 * a.cos() + 0.08 * (2.0 * a).cos()
    };
    // At a phase p (p / PHASES past a sample), tap j is the sample j - TAPS + 1 from it.
    let table: Vec<[f32; 2 * TAPS]> = (0..=PHASES)
        .map(|p| {
            std::array::from_fn(|j| {
                let d = p as f64 / PHASES as f64 + TAPS as f64 - 1.0 - j as f64;
                (cutoff * sinc(cutoff * d) * blackman(d / TAPS as f64)) as f32
            })
        })
        .collect();
    let n = (samples.len() as f64 / ratio).round() as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * ratio;
            let c = x.floor();
            let at = (x - c) * PHASES as f64;
            let p = (at.floor() as usize).min(PHASES - 1);
            let w = (at - p as f64) as f32;
            let (lo, hi) = (&table[p], &table[p + 1]);
            let first = c as i64 - TAPS as i64 + 1;
            let mut acc = 0f32;
            for j in 0..2 * TAPS {
                let k = first + j as i64;
                if k >= 0 && (k as usize) < samples.len() {
                    acc += samples[k as usize] * (lo[j] + (hi[j] - lo[j]) * w);
                }
            }
            acc
        })
        .collect()
}
