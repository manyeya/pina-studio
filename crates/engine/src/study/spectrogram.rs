//! A song's spectrum over time as a picture: frequency on a log scale, `LOW_HZ` to `HIGH_HZ`,
//! low at the bottom, in the magma colours, against its own loudest moment. The blueprint's bands
//! are for comparing songs; this is for looking at one.

use std::path::Path;

use super::reference::{Colors, HIGH_HZ, LOW_HZ, image::table};

const WIDTH: usize = 1800;
const HEIGHT: usize = 320;
const FRAME: usize = 4096;
/// How far under its loudest a picture goes before it's black, dB.
const RANGE: f32 = 80.0;

/// Each pixel's level, 0 to 1, row by row from the top.
fn levels(x: &[f32], rate: f32, width: usize, height: usize) -> Vec<f32> {
    let mut spectra = crate::samples::dsp::Spectra::new(FRAME);
    let mut power = vec![0f32; FRAME / 2 + 1];
    let bin = rate / FRAME as f32;
    // The bins each row gathers: at least one, however low the row is.
    let edge = |row: f32| LOW_HZ * (HIGH_HZ / LOW_HZ).powf(row / height as f32);
    let rows: Vec<(usize, usize)> = (0..height)
        .map(|y| {
            let a = (edge((height - y - 1) as f32) / bin) as usize;
            let z = (edge((height - y) as f32) / bin).ceil() as usize;
            (a.min(FRAME / 2), z.clamp(a + 1, FRAME / 2 + 1))
        })
        .collect();
    let mut db = vec![0f32; width * height];
    let mut top = f32::MIN;
    for c in 0..width {
        let at = (c * x.len() / width) as isize - (FRAME / 2) as isize;
        spectra.power(x, at, &mut power);
        for (y, &(a, z)) in rows.iter().enumerate() {
            let p = power[a..z].iter().copied().fold(0.0, f32::max);
            let v = 10.0 * p.max(1e-12).log10();
            db[y * width + c] = v;
            top = top.max(v);
        }
    }
    db.iter()
        .map(|v| ((v - top + RANGE) / RANGE).clamp(0.0, 1.0))
        .collect()
}

/// Paints a song's spectrum to a PNG.
pub fn paint(l: &[f32], r: &[f32], rate: f32, to: &Path) -> Result<(), String> {
    let mono: Vec<f32> = l.iter().zip(r).map(|(a, b)| 0.5 * (a + b)).collect();
    let v = levels(&mono, rate, WIDTH, HEIGHT);
    let palette = table(Colors::Magma);
    let last = palette.len() - 1;
    let mut pixels = Vec::with_capacity(v.len() * 3);
    for t in v {
        let [r, g, b] = palette[(t * last as f32) as usize];
        pixels.extend([r as u8, g as u8, b as u8]);
    }
    image::RgbImage::from_raw(WIDTH as u32, HEIGHT as u32, pixels)
        .ok_or("a picture of the wrong size")?
        .save(to)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tone_shows_where_it_sounds_and_nowhere_else() {
        let rate = 44_100.0;
        let tone: Vec<f32> = (0..44_100)
            .map(|i| (i as f32 / rate * 440.0 * std::f32::consts::TAU).sin())
            .collect();
        let (w, h) = (20, 64);
        let v = levels(&tone, rate, w, h);
        let row = (0..h)
            .max_by(|&a, &b| v[a * w + 10].total_cmp(&v[b * w + 10]))
            .unwrap();
        let want = (1.0 - (440.0 / LOW_HZ).ln() / (HIGH_HZ / LOW_HZ).ln()) * h as f32;
        assert!((row as f32 - want).abs() <= 2.0, "row {row}, wanted {want}");
        assert!(v[w + 10] < 0.5, "its top is quiet: {}", v[w + 10]);
    }
}
