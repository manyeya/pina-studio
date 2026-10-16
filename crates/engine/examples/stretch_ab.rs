//! Renders loops to new tempos with each way of stretching, and with the Signalsmith stretch
//! tempo used before, to hear side by side; and measures them: how long each took, how near its
//! hits land to their new times, and how sharp their attacks stay.
//!
//! `cargo run --release -p tempo-engine --example stretch_ab -- <out dir> <loop.wav> <bpm>...`
//! (a loop and its tempo, again for more): each goes to 120 and to 80 BPM.

use std::{path::Path, time::Instant};

use tempo_engine::{
    engine::{AudioData, load_audio},
    stretch::{Flex, onsets, render},
};

const RATE: f64 = 44_100.0;

fn signalsmith(data: &AudioData, ratio: f64) -> AudioData {
    let channels = data.channels().len();
    let frames = data.channels()[0].len();
    let input: Vec<f32> = (0..frames)
        .flat_map(|i| data.channels().iter().map(move |c| c[i]))
        .collect();
    let out_frames = (frames as f64 * ratio).round() as usize;
    let mut output = vec![0f32; out_frames * channels];
    let mut stretch = signalsmith_stretch::Stretch::preset_default(channels as u32, RATE as u32);
    stretch.exact(&input, &mut output);
    AudioData::new(
        (0..channels)
            .map(|c| output.iter().skip(c).step_by(channels).copied().collect())
            .collect(),
    )
}

fn write(path: &Path, data: &AudioData) {
    let spec = hound::WavSpec {
        channels: data.channels().len() as u16,
        sample_rate: RATE as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..data.channels()[0].len() {
        for c in data.channels() {
            w.write_sample(c[i]).unwrap();
        }
    }
    w.finalize().unwrap();
}

/// Of the input's hits, how many land within 20 ms of their new time, how far off they are on
/// average (ms), how loud their first 10 ms peak against the recording's (1 is as recorded), and
/// how loud the 30 ms before each is against the recording's (over 1 is pre-echo, smeared ahead
/// of the hit).
fn measure(input: &AudioData, output: &AudioData, ratio: f64) -> (usize, usize, f64, f64, f64) {
    let want = onsets(input.channels(), RATE);
    let got = onsets(output.channels(), RATE);
    let peak = |d: &AudioData, at: usize| {
        let c = &d.channels()[0];
        c[at.min(c.len())..(at + 441).min(c.len())]
            .iter()
            .fold(0f32, |m, s| m.max(s.abs())) as f64
    };
    let rms = |d: &AudioData, at: usize| {
        let c = &d.channels()[0];
        let part = &c[at.saturating_sub(1323).min(c.len())..at.saturating_sub(44).min(c.len())];
        (part.iter().map(|s| (s * s) as f64).sum::<f64>() / part.len().max(1) as f64).sqrt()
    };
    let (mut pre, mut counted) = (0.0, 0);
    for &w in &want {
        let at = (w as f64 * ratio).round() as usize;
        let before = rms(input, w);
        if before > 1e-4 {
            pre += rms(output, at) / before;
            counted += 1;
        }
    }
    let (mut found, mut off, mut sharp) = (0, 0.0, 0.0);
    for &w in &want {
        let at = (w as f64 * ratio).round() as i64;
        if let Some(&g) = got.iter().min_by_key(|&&g| (g as i64 - at).abs())
            && (g as i64 - at).abs() as f64 <= 0.02 * RATE
        {
            found += 1;
            off += (g as i64 - at).abs() as f64 / RATE * 1000.0;
            sharp += peak(output, at as usize) / peak(input, w).max(1e-6);
        }
    }
    let n = found.max(1) as f64;
    (
        found,
        want.len(),
        off / n,
        sharp / n,
        pre / counted.max(1) as f64,
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = Path::new(&args[0]);
    for pair in args[1..].chunks(2) {
        let (file, bpm) = (Path::new(&pair[0]), pair[1].parse::<f64>().unwrap());
        let name = file.file_stem().unwrap().to_string_lossy().to_string();
        let dir = out.join(&name);
        std::fs::create_dir_all(&dir).unwrap();
        let input = load_audio(file, RATE).unwrap();
        write(&dir.join("0 original.wav"), &input);
        println!(
            "\n{name} ({bpm} BPM, {:.1} s)",
            input.channels()[0].len() as f64 / RATE
        );
        println!(
            "  {:<28} {:>8} {:>10} {:>9} {:>7} {:>9}",
            "to", "time", "hits", "off (ms)", "attack", "pre-echo"
        );
        for to in [120.0, 80.0] {
            let ratio = bpm / to;
            type Way<'a> = (&'a str, Box<dyn Fn() -> AudioData + 'a>);
            let ways: [Way; 4] = [
                (
                    "Signalsmith (before)",
                    Box::new(|| signalsmith(&input, ratio)),
                ),
                (
                    "Polyphonic",
                    Box::new(|| render(&input, ratio, 0.0, Flex::Polyphonic, RATE)),
                ),
                (
                    "Slicing",
                    Box::new(|| render(&input, ratio, 0.0, Flex::Slicing, RATE)),
                ),
                (
                    "Speed",
                    Box::new(|| render(&input, ratio, 0.0, Flex::Speed, RATE)),
                ),
            ];
            for (way, f) in ways {
                let t = Instant::now();
                let o = f();
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                let (found, of, off, sharp, pre) = measure(&input, &o, ratio);
                println!(
                    "  {:<28} {:>6.0} ms {:>4}/{:<4} {:>9.2} {:>7.2} {:>9.2}",
                    format!("{to:.0} BPM {way}"),
                    ms,
                    found,
                    of,
                    off,
                    sharp,
                    pre
                );
                write(&dir.join(format!("{to:.0} BPM {way}.wav")), &o);
            }
        }
    }
}
