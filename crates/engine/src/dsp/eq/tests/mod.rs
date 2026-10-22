mod analyzer;
mod dynamics;
mod output;
mod phase;
mod shapes;

use super::*;
use crate::song::project::{Bands, Dynamic};

const RATE: f64 = 48_000.0;

fn eq(bands: &[EqBand]) -> Eq {
    Eq {
        bands: Bands::from_slice(bands),
        ..Eq::default()
    }
}

fn band(shape: Shape, freq: f32, gain: f32) -> EqBand {
    EqBand::new(shape, freq, gain)
}

fn at(b: &EqBand, freq: f64) -> f64 {
    band_response(b, 1.0, freq, RATE)
}

fn close(a: f64, b: f64, by: f64) -> bool {
    (a - b).abs() <= by
}

/// A sine's level through an EQ, once it's settled, in dB.
fn through(e: &Eq, freq: f64, side: bool) -> ([f64; 2], [f32; 2]) {
    let mut p = Processor::new(RATE);
    let n = 48_000;
    let sine = |i: usize| (2.0 * PI * freq * i as f64 / RATE).sin() as f32 * 0.25;
    let (mut peak, mut last) = ([0f64; 2], [0f32; 2]);
    let mut i = 0;
    while i < n {
        let frames = MAX_BLOCK.min(n - i);
        let mut l: Vec<f32> = (i..i + frames).map(sine).collect();
        let mut r: Vec<f32> = l.iter().map(|&x| if side { -x } else { x }).collect();
        p.process(e, None, None, &mut l, &mut r, None, None);
        if i > n / 2 {
            for k in 0..frames {
                peak = [
                    peak[0].max(l[k].abs() as f64),
                    peak[1].max(r[k].abs() as f64),
                ];
            }
        }
        last = [l[frames - 1], r[frames - 1]];
        i += frames;
    }
    (
        [
            20.0 * (peak[0] / 0.25).log10(),
            20.0 * (peak[1] / 0.25).log10(),
        ],
        last,
    )
}

/// Runs a second of stereo sound through an EQ, a block at a time; what came out.
fn run(e: &Eq, l: &[f32], r: &[f32], sc: Option<[&[f32]; 2]>) -> (Vec<f32>, Vec<f32>, Processor) {
    let mut p = Processor::new(RATE);
    let (mut ol, mut or) = (l.to_vec(), r.to_vec());
    for at in (0..l.len()).step_by(MAX_BLOCK) {
        let end = (at + MAX_BLOCK).min(l.len());
        let side = sc.map(|[a, b]| [&a[at..end], &b[at..end]]);
        p.process(
            e,
            None,
            None,
            &mut ol[at..end],
            &mut or[at..end],
            side,
            None,
        );
    }
    (ol, or, p)
}

fn tone(freq: f64, level: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (2.0 * PI * freq * i as f64 / RATE).sin() as f32 * level)
        .collect()
}

fn peak_db(s: &[f32]) -> f64 {
    20.0 * (s.iter().fold(0f32, |m, x| m.max(x.abs())) as f64)
        .max(1e-9)
        .log10()
}

/// How loud a harmonic of `f` is in `s`, against `f` (dB).
fn harmonic(s: &[f32], f: f64, k: f64) -> f64 {
    let dft = |f: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &x) in s.iter().enumerate() {
            let w = 2.0 * PI * f * i as f64 / RATE;
            re += x as f64 * w.cos();
            im += x as f64 * w.sin();
        }
        (re * re + im * im).sqrt()
    };
    20.0 * (dft(f * k) / dft(f)).max(1e-12).log10()
}

/// An impulse through an EQ as the engine runs it, what it needs made first: what came out.
fn impulse(e: &Eq, n: usize) -> Vec<f32> {
    let made = Made::new(e, RATE);
    let mut p = Processor::new(RATE);
    let mut l = vec![0f32; n];
    l[0] = 1.0;
    let mut r = l.clone();
    for at in (0..n).step_by(MAX_BLOCK) {
        let end = (at + MAX_BLOCK).min(n);
        p.process(
            e,
            Some(&made),
            None,
            &mut l[at..end],
            &mut r[at..end],
            None,
            None,
        );
    }
    l
}
