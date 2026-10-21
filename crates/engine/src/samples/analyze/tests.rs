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
    let dir = std::env::temp_dir().join(format!("pina-test-aiff-{}", std::process::id()));
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
