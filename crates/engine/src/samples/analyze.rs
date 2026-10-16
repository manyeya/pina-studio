//! What a sample is, from its audio, with its name for hints: `features` measures, `classify`
//! names, and this puts it together.

use std::path::Path;

use super::{
    Analysis, Category, EMBEDDING, KeyGuess, Kind, NOTES, Pitch, WAVEFORM, classify,
    dsp::{self, Spectra},
    features::{self, Features},
    model, tonal,
};

/// Analyses a sample file. Its folder and name give hints ("Kick_03", "Loops/120bpm").
pub fn analyze_file(path: &Path) -> Result<Analysis, String> {
    let data = crate::engine::read_audio(path)?;
    let hint = path.to_string_lossy().to_lowercase();
    Ok(analyze(&data.0, data.1, &hint))
}

/// Analyses audio: `channels` at `rate`, with `hint` (its path, lower-cased) for naming hints.
pub fn analyze(channels: &[Vec<f32>], rate: f64, hint: &str) -> Analysis {
    let hints = classify::hints(hint);
    let f = features::extract(channels, rate, hints.bpm);
    let x = classify::vector(&f);
    let kind = classify::kind(&f, &x, &hints);
    let (category, confidence) = classify::category(&x, &hints, kind);
    describe(channels, &f, &x, kind, category, confidence)
}

/// An analysis from what was measured and what it was found to be.
pub fn describe(
    channels: &[Vec<f32>],
    f: &Features,
    x: &[f32],
    kind: Kind,
    category: Category,
    confidence: f32,
) -> Analysis {
    let silent = f.peak <= 1e-7;
    let noisy_drum = matches!(
        category,
        Category::Snare
            | Category::Clap
            | Category::Snap
            | Category::HatClosed
            | Category::HatOpen
            | Category::Ride
            | Category::Crash
            | Category::Shaker
            | Category::Tambourine
            | Category::Drums
            | Category::Texture
    );
    let pitch = f
        .pitch
        .filter(|p| kind == Kind::OneShot && !noisy_drum && p.confidence > 0.15)
        .map(|p| {
            let midi = super::pitch::midi(p.hz);
            let note = midi.round().clamp(0.0, 127.0);
            Pitch {
                note: note as u8,
                cents: ((midi - note) * 100.0 * 10.0).round() / 10.0,
                hz: (p.hz * 100.0).round() / 100.0,
                confidence: p.confidence.clamp(0.0, 1.0),
            }
        });
    let mut chroma = f.tonal.chroma;
    if chroma.iter().sum::<f32>() <= 0.0 {
        chroma = [1.0 / 12.0; 12];
    }
    let tonal = f.tonal.tonal.clamp(0.0, 1.0);
    // Several notes: a chord, or a line, not one note and its harmonics.
    let classes = chroma.iter().filter(|&&v| v > 0.08).count();
    let several = classes >= 3 || (classes == 2 && f.tonal.polyphony >= 1.5);
    // A one-shot's notes are a chord: its key is the chord's. A loop's, the key its line and
    // harmony fit.
    let key = (!category.is_drum() && tonal >= 0.3 && several)
        .then(|| match kind {
            Kind::OneShot => tonal::chord(&f.tonal),
            Kind::Loop => tonal::key(&f.tonal),
        })
        .flatten()
        .map(|(tonic, minor, fit)| KeyGuess {
            tonic,
            minor,
            name: format!(
                "{} {}",
                NOTES[tonic as usize],
                if minor { "minor" } else { "major" }
            ),
            confidence: fit.clamp(-1.0, 1.0),
        });
    let (bpm, bars) = match (kind, f.tempo) {
        (Kind::Loop, Some(t)) => {
            let bars = match t.beats {
                Some(b) => b as f32 / 4.0,
                None => (f.seconds * t.bpm / 240.0 * 4.0).round() / 4.0,
            };
            (Some((t.bpm * 100.0).round() / 100.0), Some(bars))
        }
        _ => (None, None),
    };
    let envelope = match kind {
        Kind::Loop => f.hits.unwrap_or(f.envelope),
        Kind::OneShot => f.envelope,
    };
    let loudness = match kind {
        Kind::OneShot => f.loudness_short,
        Kind::Loop => f.loudness_long,
    };
    let round = |v: f32, places: i32| {
        let k = 10f32.powi(places);
        (v * k).round() / k
    };
    let mut spectrum = f.spectrum;
    spectrum.centroid_hz = round(spectrum.centroid_hz, 0);
    spectrum.rolloff_hz = round(spectrum.rolloff_hz, 0);
    spectrum.flatness = round(spectrum.flatness, 3);
    spectrum.bands_db.iter_mut().for_each(|b| *b = round(*b, 1));
    Analysis {
        seconds: f.seconds,
        sample_rate: f.rate as u32,
        channels: channels.len() as u16,
        kind,
        category,
        confidence: round(confidence, 3),
        pitch,
        key,
        chroma: chroma.map(|c| round(c, 4)),
        tonal: round(tonal, 3),
        bpm,
        bars,
        loudness: round(loudness, 1),
        peak_db: round(dsp::amp_db(f.peak).max(-120.0), 1),
        envelope: super::Envelope {
            attack_ms: round(envelope.attack_ms, 1),
            decay_ms: round(envelope.decay_ms, 1),
            tail_ms: round(envelope.tail_ms, 1),
            sustain: round(envelope.sustain, 3),
        },
        spectrum,
        width: round(f.width, 3),
        traits: if silent {
            Vec::new()
        } else {
            traits(f, kind, category, loudness)
        },
        embedding: if silent {
            vec![0.0; EMBEDDING]
        } else {
            embedding(x)
        },
        fingerprint: fingerprint(channels, f.rate),
        waveform: waveform(channels, f.peak),
    }
}

/// What traits are judged by, against the sample's own category: centroid (log2 Hz), decay and
/// tail (ln ms), sub and low energy, low mids and air (dB under the total), attack (ln ms),
/// crest (dB) and loudness (LUFS).
pub fn trait_inputs(f: &Features, loudness: f32) -> [f32; 10] {
    let b = &f.spectrum.bands_db;
    let ln1 = |v: f32| (1.0 + v.max(0.0)).ln();
    let low = dsp::db(10f32.powf(b[0] / 10.0) + 10f32.powf(b[1] / 10.0));
    [
        f.spectrum.centroid_hz.max(20.0).log2(),
        ln1(f.envelope.decay_ms),
        ln1(f.envelope.tail_ms),
        b[0].max(-80.0),
        low.max(-80.0),
        b[2].max(-80.0),
        b[5].max(-80.0),
        ln1(f.envelope.attack_ms),
        f.crest_db,
        loudness,
    ]
}

/// Words for how it sounds, from `TRAITS`: measured against what's usual for its category, so
/// a bright kick is bright for a kick.
fn traits(f: &Features, kind: Kind, category: Category, loudness: f32) -> Vec<String> {
    let inputs = trait_inputs(f, loudness);
    let stats = model::TRAIT_STATS
        .iter()
        .find(|(c, _)| *c == category)
        .or_else(|| {
            model::TRAIT_STATS
                .iter()
                .find(|(c, _)| *c == Category::Other)
        })
        .map(|(_, s)| *s);
    // How far from usual for its kind, in spreads (0 without statistics).
    let z = |i: usize| stats.map_or(0.0, |s| (inputs[i] - s[i].0) / s[i].1.max(1e-3));
    let e = match kind {
        Kind::Loop => f.hits.unwrap_or(f.envelope),
        Kind::OneShot => f.envelope,
    };
    let b = &f.spectrum.bands_db;
    let tonal = f.tonal.tonal;
    let mut out: Vec<&str> = Vec::new();
    let percussive = category.is_drum()
        || matches!(
            category,
            Category::Bass808 | Category::Bass | Category::Chord | Category::Pluck
        );
    if percussive && e.attack_ms < 8.0 && z(8) > -0.5 && z(7) < 0.5 {
        out.push("punchy");
    }
    if e.attack_ms > 30.0 || (e.attack_ms > 10.0 && z(7) > 1.5) {
        out.push("soft");
    }
    if e.attack_ms < 8.0 && z(1) < -0.3 && z(0) > 0.3 && f.envelope.decay_ms < 250.0 {
        out.push("snappy");
    }
    // A one-shot by how long it rings; a loop by how long its notes or hits hold.
    let (short, long) = match kind {
        Kind::OneShot => (
            f.envelope.tail_ms < 250.0,
            f.envelope.tail_ms > 1500.0 && e.sustain > 0.02,
        ),
        Kind::Loop => (e.decay_ms < 80.0, e.decay_ms > 800.0),
    };
    if short {
        out.push("short");
    }
    if long {
        out.push("long");
    }
    if z(0) > 0.8 {
        out.push("bright");
    } else if z(0) < -0.8 {
        out.push("dark");
    }
    if z(5) > 0.4 && z(6) < 0.0 && z(0) < 0.3 && b[2] > -12.0 {
        out.push("warm");
    }
    if z(6) > 0.8 && b[5] > -30.0 {
        out.push("airy");
    }
    if b[0] > -8.0 || (z(3) > 1.0 && b[0] > -15.0) {
        out.push("sub");
    }
    if z(4) > 0.7 && z(1) > 0.5 && inputs[4] > -6.0 {
        out.push("boomy");
    }
    if z(4) < -1.0 && inputs[4] < -15.0 {
        out.push("thin");
    }
    if tonal >= 0.6 {
        out.push("tonal");
    } else if tonal < 0.2 && f.spectrum.flatness > 0.3 {
        out.push("noisy");
    }
    let ringing = f.envelope.tail_ms - f.envelope.decay_ms;
    if kind == Kind::OneShot {
        if z(2) > 0.7 && ringing > 400.0 && f.envelope.tail_ms > 2.5 * f.envelope.decay_ms {
            out.push("wet");
        } else if z(2) < 0.0 && ringing < 300.0 {
            out.push("dry");
        }
    }
    if f.width > 0.3 {
        out.push("wide");
    } else if f.width < 0.02 {
        out.push("mono");
    }
    let distorted = f.clipped > 0.001 || z(8) < -1.8;
    if distorted {
        out.push("distorted");
    }
    let band_limited = !matches!(
        category,
        Category::Kick | Category::Bass808 | Category::Bass | Category::Tom
    ) && f.bandwidth < 9000.0
        && f.rate > 30_000.0
        && z(6) < -1.2;
    if band_limited {
        out.push("lo-fi");
    }
    if !distorted && !band_limited && tonal >= 0.4 {
        out.push("clean");
    }
    if z(9) > 1.0 {
        out.push("loud");
    }
    out.into_iter().map(String::from).collect()
}

/// Its timbre as a unit vector: the feature vector standardised, with what the one-shot
/// classifier's nets make of it, projected to `EMBEDDING` dimensions by a linear discriminant
/// analysis fitted on labelled packs, so what's near sounds alike in the ways that tell sounds
/// apart. Without fitted constants, the standardised vector's first dimensions.
fn embedding(x: &[f32]) -> Vec<f32> {
    let mut z: Vec<f32> = x
        .iter()
        .zip(model::EMBED_MEAN)
        .zip(model::EMBED_SCALE)
        .map(|((v, m), s)| ((v - m) / s).clamp(-6.0, 6.0))
        .collect();
    z.extend(classify::hidden(model::ONE_SHOT, x));
    let mut v: Vec<f32> = if model::EMBED_PROJECTION.is_empty() {
        (0..EMBEDDING)
            .map(|i| x.get(i).copied().unwrap_or(0.0))
            .collect()
    } else {
        model::EMBED_PROJECTION
            .iter()
            .map(|p| p.iter().zip(&z).map(|(a, b)| a * b).sum())
            .collect()
    };
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut()
            .for_each(|x| *x = (*x / norm * 1e4).round() / 1e4);
    }
    v
}

/// A fingerprint: the same for the same sound, whatever its file's name, format or bit depth.
/// Of the sound (from 40 dB under its peak to 50 dB under at the end): 32 bits for its shape
/// in time (cut in eight, each part's energy in five bands, a bit for each pair of neighbouring
/// bands set when the lower is stronger against the higher than it is on average), and 32 for
/// its notes (its energy in 33 bands a whole tone wide from 60 Hz, a bit set where a band is
/// stronger than the next). Comparisons, so gain and dither don't move them. Its length (to
/// 50 ms) is mixed in.
fn fingerprint(channels: &[Vec<f32>], rate: f32) -> u64 {
    let len = channels.first().map_or(0, Vec::len);
    let mono: Vec<f32> = (0..len)
        .map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / channels.len().max(1) as f32)
        .collect();
    let peak = mono.iter().fold(0f32, |m, v| m.max(v.abs()));
    if peak <= 1e-6 {
        return 0;
    }
    let start = mono.iter().position(|v| v.abs() > peak * 0.01).unwrap_or(0);
    let end = mono
        .iter()
        .rposition(|v| v.abs() > peak * 0.003)
        .unwrap_or(len - 1)
        .max(start + 1);
    let sound = &mono[..end + 1];
    let span = end - start;
    let size = 8192;
    let mut spectra = Spectra::new(size);
    let mut power = vec![0f32; size / 2 + 1];
    let bin = rate / size as f32;
    let edges = [0.0, 200.0, 800.0, 3200.0, 12_800.0, 1e9];
    let mut levels = [[0f32; 5]; 8];
    let mut whole = vec![0f32; size / 2 + 1];
    for (part, level) in levels.iter_mut().enumerate() {
        let (a, b) = (start + part * span / 8, start + (part + 1) * span / 8);
        let frames = ((b - a) / size).clamp(1, 2);
        for i in 0..frames {
            let at = a + i * (b - a) / frames;
            spectra.power(sound, at as isize, &mut power);
            for (k, p) in power.iter().enumerate() {
                let band = edges
                    .iter()
                    .rposition(|&e| k as f32 * bin >= e)
                    .unwrap_or(0)
                    .min(4);
                level[band] += p;
                whole[k] += p;
            }
        }
    }
    let mut bits = 0u64;
    let loudest = levels.iter().flatten().fold(0f32, |m, &v| m.max(v));
    let floor = loudest * 1e-6 + 1e-20;
    let diff = |t: usize, b: usize| (levels[t][b] + floor).ln() - (levels[t][b + 1] + floor).ln();
    for b in 0..4 {
        let mean = (0..8).map(|t| diff(t, b)).sum::<f32>() / 8.0;
        for t in 0..8 {
            if diff(t, b) > mean + 1e-3 {
                bits |= 1 << (t * 4 + b);
            }
        }
    }
    let band = |k: usize| {
        let (lo, hi) = (
            60.0 * 2f32.powf(k as f32 / 6.0),
            60.0 * 2f32.powf((k + 1) as f32 / 6.0),
        );
        whole[(lo / bin) as usize..((hi / bin) as usize).max((lo / bin) as usize + 1)]
            .iter()
            .sum::<f32>()
    };
    let notes: Vec<f32> = (0..33).map(band).collect();
    let floor = notes.iter().fold(0f32, |m, &v| m.max(v)) * 1e-6 + 1e-20;
    for k in 0..32 {
        if notes[k] + floor > (notes[k + 1] + floor) * 1.001 {
            bits |= 1 << (32 + k);
        }
    }
    let length = ((span as f32 / rate) * 20.0).round() as u64;
    mix(bits ^ mix(length))
}

/// SplitMix64's finaliser: spreads bits.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Its outline: the peak of each of `WAVEFORM` stretches, 0 to 255 of the whole's peak.
fn waveform(channels: &[Vec<f32>], peak: f32) -> Vec<u8> {
    let len = channels.first().map_or(0, Vec::len);
    (0..WAVEFORM)
        .map(|i| {
            let (a, b) = (
                i * len / WAVEFORM,
                ((i + 1) * len / WAVEFORM).max(i * len / WAVEFORM + 1),
            );
            let p = channels
                .iter()
                .flat_map(|c| c.get(a..b.min(c.len())).unwrap_or(&[]))
                .fold(0f32, |m, s| m.max(s.abs()));
            (p / peak.max(1e-9) * 255.0) as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 44_100.0;

    fn noise(seconds: f32, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..(seconds * RATE as f32) as usize)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (s >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            })
            .collect()
    }

    /// A decaying sine at `hz`, an 808-ish note, dying away to silence.
    fn note(hz: f32, seconds: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                0.8 * (std::f32::consts::TAU * hz * t).sin() * (-t * 5.0).exp()
            })
            .collect()
    }

    #[test]
    fn a_low_note_has_its_pitch_and_no_key() {
        let x = note(49.0, 1.5); // G1
        let a = analyze(&[x.clone(), x], RATE, "/samples/808_g.wav");
        let p = a.pitch.expect("a pitch");
        assert_eq!(p.note, 31);
        assert!(p.cents.abs() < 10.0, "{p:?}");
        assert!(a.key.is_none());
        assert!(a.tonal > 0.7, "{}", a.tonal);
        assert!(a.traits.contains(&"sub".to_string()), "{:?}", a.traits);
        assert!(a.traits.contains(&"mono".to_string()), "{:?}", a.traits);
        assert_eq!(a.kind, Kind::OneShot);
    }

    #[test]
    fn a_noise_burst_is_noisy_and_keyless() {
        let mut x = noise(0.2, 9);
        for (i, v) in x.iter_mut().enumerate() {
            *v *= (-(i as f32) / 900.0).exp();
        }
        let a = analyze(&[x], RATE, "");
        assert!(a.pitch.is_none());
        assert!(a.key.is_none());
        assert!(a.tonal < 0.2, "{}", a.tonal);
        assert!(a.spectrum.flatness > 0.5, "{:?}", a.spectrum);
        assert!(a.traits.contains(&"short".to_string()), "{:?}", a.traits);
    }

    #[test]
    fn the_embedding_is_unit_length_and_alike_for_alike() {
        let a = analyze(&[note(55.0, 1.0)], RATE, "");
        let b = analyze(&[note(58.0, 1.0)], RATE, "");
        let c = analyze(&[noise(1.0, 4)], RATE, "");
        let norm = a.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
        let dot = |x: &[f32], y: &[f32]| x.iter().zip(y).map(|(p, q)| p * q).sum::<f32>();
        assert!(dot(&a.embedding, &b.embedding) > dot(&a.embedding, &c.embedding));
    }

    #[test]
    fn a_fingerprint_survives_gain_and_bit_depth() {
        let x = note(110.0, 0.8);
        let quieter: Vec<f32> = x.iter().map(|v| v * 0.5).collect();
        let crushed: Vec<f32> = x.iter().map(|v| (v * 32767.0).round() / 32767.0).collect();
        let a = fingerprint(std::slice::from_ref(&x), RATE as f32);
        assert_eq!(a, fingerprint(&[quieter], RATE as f32));
        assert_eq!(a, fingerprint(&[crushed], RATE as f32));
        assert_ne!(a, fingerprint(&[note(220.0, 0.8)], RATE as f32));
        assert_ne!(a, fingerprint(&[noise(0.8, 1)], RATE as f32));
    }

    /// A 16-bit big-endian AIFF of `x` (mono), written by hand.
    fn aiff(path: &Path, x: &[f32], rate: f64) {
        let frames = x.len() as u32;
        let mut comm = Vec::new();
        comm.extend(1u16.to_be_bytes());
        comm.extend(frames.to_be_bytes());
        comm.extend(16u16.to_be_bytes());
        // The rate as an 80-bit extended float.
        let exponent = (rate.log2().floor()) as u16;
        let mantissa = (rate / 2f64.powi(exponent as i32) * 2f64.powi(63)) as u64;
        comm.extend((16383 + exponent).to_be_bytes());
        comm.extend(mantissa.to_be_bytes());
        let mut ssnd = vec![0u8; 8];
        for v in x {
            ssnd.extend(((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_be_bytes());
        }
        let mut body = b"AIFF".to_vec();
        for (id, chunk) in [(b"COMM", comm), (b"SSND", ssnd)] {
            body.extend(id);
            body.extend((chunk.len() as u32).to_be_bytes());
            body.extend(chunk);
        }
        let mut file = b"FORM".to_vec();
        file.extend((body.len() as u32).to_be_bytes());
        file.extend(body);
        std::fs::write(path, file).unwrap();
    }

    #[test]
    fn an_aiff_reads_and_fingerprints_as_its_wav_does() {
        let x = note(110.0, 0.5);
        let dir = std::env::temp_dir().join(format!("tempo-test-aiff-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(dir.join("note.wav"), spec).unwrap();
        for v in &x {
            w.write_sample((v * 32767.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        aiff(&dir.join("note.aif"), &x, RATE);
        let wav = analyze_file(&dir.join("note.wav")).unwrap();
        let aif = analyze_file(&dir.join("note.aif")).unwrap();
        assert_eq!((aif.sample_rate, aif.channels), (44_100, 1));
        assert_eq!(aif.fingerprint, wav.fingerprint);
        assert_eq!(aif.pitch.map(|p| p.note), Some(45));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_loop_named_with_its_tempo_has_it() {
        // Four bars of clicks at 95 BPM, a kick-ish thump on each beat.
        let bpm = 95.0;
        let seconds = 16.0 * 60.0 / bpm;
        let len = (seconds * RATE as f32) as usize;
        let mut x = vec![0f32; len];
        for beat in 0..16 {
            let at = (beat as f32 * 60.0 / bpm * RATE as f32) as usize;
            for i in 0..4000.min(len - at) {
                let t = i as f32 / RATE as f32;
                x[at + i] += (std::f32::consts::TAU * 60.0 * t).sin() * (-t * 30.0).exp();
            }
        }
        let a = analyze(&[x], RATE, "/loops/kick_loop(95bpm).wav");
        assert_eq!(a.kind, Kind::Loop);
        assert_eq!(a.bpm, Some(95.0));
        assert_eq!(a.bars, Some(4.0));
    }
}
