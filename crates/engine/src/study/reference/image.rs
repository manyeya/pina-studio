//! A blueprint from a picture of a spectrogram (PNG or JPEG), when there's no audio. Its plot is
//! found (the part that isn't margin, labels or frame), its colours matched to a colour scale,
//! and each pixel read as a place on it: how loud, relative to the rest. With the plot's time and
//! frequency axes (given by the person, or by an agent reading its labels) it says when and at
//! what pitch; without, only where, and asks for them. It never claims what a picture can't
//! show: chords, instruments, loudness, stereo width, lyrics.

use std::{collections::HashMap, path::Path};

use super::*;

/// A picture, as RGB pixels, row by row from the top.
#[derive(Clone, Debug)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 3]>,
}

impl Picture {
    pub fn at(&self, x: u32, y: u32) -> [u8; 3] {
        self.pixels[(y * self.width + x) as usize]
    }
}

/// Reads a PNG or JPEG.
pub fn read(path: &Path) -> Result<Picture, String> {
    let img = ::image::open(path)
        .map_err(|e| format!("couldn't read {} as a picture: {e}", path.display()))?
        .to_rgb8();
    let (width, height) = img.dimensions();
    if width < 16 || height < 16 {
        return Err("the picture is too small to read a spectrogram from".into());
    }
    Ok(Picture {
        width,
        height,
        pixels: img.pixels().map(|p| p.0).collect(),
    })
}

/// A picture saved as a PNG (a drawn spectrogram, for trying a reading out).
pub fn save(p: &Picture, path: &Path) -> Result<(), String> {
    ::image::RgbImage::from_fn(p.width, p.height, |x, y| ::image::Rgb(p.at(x, y)))
        .save(path)
        .map_err(|e| format!("couldn't write {}: {e}", path.display()))
}

/// A picture's size in pixels, without reading all of it.
pub fn size(path: &Path) -> Option<(u32, u32)> {
    ::image::image_dimensions(path).ok()
}

/// Whether a file is a picture this reads.
pub fn is_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_lowercase().as_str(), "png" | "jpg" | "jpeg"))
}

/// What was found in a picture on its own: its plot, how sure; its colour scale, how sure.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub crop: [u32; 4],
    pub crop_sure: f32,
    pub colors: Colors,
    pub colors_sure: f32,
}

/// Colour scales by their stops, quiet to loud.
fn stops(c: Colors) -> &'static [u32] {
    match c {
        Colors::Gray => &[0x000000, 0xffffff],
        Colors::GrayInverted => &[0xffffff, 0x000000],
        Colors::Viridis => &[
            0x440154, 0x482878, 0x3e4989, 0x31688e, 0x26828e, 0x1f9e89, 0x35b779, 0x6ece58,
            0xb5de2b, 0xfde725,
        ],
        Colors::Magma => &[
            0x000004, 0x180f3d, 0x440f76, 0x721f81, 0x9e2f7f, 0xcd4071, 0xf1605d, 0xfd9668,
            0xfeca8d, 0xfcfdbf,
        ],
        Colors::Inferno => &[
            0x000004, 0x1b0c41, 0x4a0c6b, 0x781c6d, 0xa52c60, 0xcf4446, 0xed6925, 0xfb9b06,
            0xf7d13d, 0xfcffa4,
        ],
        Colors::Plasma => &[
            0x0d0887, 0x46039f, 0x7201a8, 0x9c179e, 0xbd3786, 0xd8576b, 0xed7953, 0xfb9f3a,
            0xfdca26, 0xf0f921,
        ],
        Colors::Jet => &[
            0x00007f, 0x0000ff, 0x007fff, 0x00ffff, 0x7fff7f, 0xffff00, 0xff7f00, 0xff0000,
            0x7f0000,
        ],
    }
}

const STEPS: usize = 64;

/// A colour scale's colours, quiet to loud, `STEPS` of them.
pub fn table(c: Colors) -> Vec<[f32; 3]> {
    let s = stops(c);
    let rgb = |v: u32| {
        [
            (v >> 16) as f32,
            ((v >> 8) & 0xff) as f32,
            (v & 0xff) as f32,
        ]
    };
    (0..STEPS)
        .map(|i| {
            let at = i as f32 / (STEPS - 1) as f32 * (s.len() - 1) as f32;
            let (k, f) = (at.floor() as usize, at.fract());
            let (a, b) = (rgb(s[k]), rgb(s[(k + 1).min(s.len() - 1)]));
            std::array::from_fn(|j| a[j] + (b[j] - a[j]) * f)
        })
        .collect()
}

fn distance(a: [f32; 3], b: [u8; 3]) -> f32 {
    ((a[0] - b[0] as f32).powi(2) + (a[1] - b[1] as f32).powi(2) + (a[2] - b[2] as f32).powi(2))
        .sqrt()
}

/// A colour's place on a scale (0 quiet, 1 loud), and how far it is from the scale's nearest.
fn place(t: &[[f32; 3]], c: [u8; 3]) -> (f32, f32) {
    let (i, d) = t
        .iter()
        .enumerate()
        .map(|(i, s)| (i, distance(*s, c)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, 0.0));
    (i as f32 / (STEPS - 1) as f32, d)
}

/// The plot in a picture, and its colour scale. The plot is where the picture isn't its margin's
/// colour (the commonest round its edge): the longest run of rows mostly something else, then of
/// columns within them, a frame line at its edge trimmed off. A picture with no margin is all
/// plot. The colour scale is the one its plot's pixels lie nearest; gray, whichever way round
/// leaves most of it quiet.
pub fn find(p: &Picture) -> Found {
    let (w, h) = (p.width, p.height);
    let mut counts: HashMap<[u8; 3], usize> = HashMap::new();
    let quant = |c: [u8; 3]| c.map(|v| v & 0xe0);
    for x in 0..w {
        *counts.entry(quant(p.at(x, 0))).or_default() += 1;
        *counts.entry(quant(p.at(x, h - 1))).or_default() += 1;
    }
    for y in 0..h {
        *counts.entry(quant(p.at(0, y))).or_default() += 1;
        *counts.entry(quant(p.at(w - 1, y))).or_default() += 1;
    }
    let border = 2 * (w + h) as usize;
    let (common, most) = counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .unwrap_or(([0; 3], 0));
    // A margin is one plain colour (white, black or a gray) most of the way round; a picture
    // without one is all plot.
    let neutral = common.iter().max().unwrap_or(&0) - common.iter().min().unwrap_or(&0) <= 32;
    if !neutral || most * 10 < border * 6 {
        let crop = [0, 0, w, h];
        let (colors, colors_sure) = scale_of(p, crop);
        return Found {
            crop,
            crop_sure: 0.6,
            colors,
            colors_sure,
        };
    }
    let margin = common.map(|v| v as f32 + 16.0);
    let ink = |x: u32, y: u32| distance(margin, p.at(x, y)) > 56.0;
    let rows: Vec<f32> = (0..h)
        .map(|y| (0..w).filter(|&x| ink(x, y)).count() as f32 / w as f32)
        .collect();
    let (top, bottom) = longest(&rows, 0.35).unwrap_or((0, h as usize));
    let cols: Vec<f32> = (0..w)
        .map(|x| {
            (top as u32..bottom as u32).filter(|&y| ink(x, y)).count() as f32
                / (bottom - top).max(1) as f32
        })
        .collect();
    let (left, right) = longest(&cols, 0.35).unwrap_or((0, w as usize));
    let (mut x0, mut y0, mut x1, mut y1) = (left as u32, top as u32, right as u32, bottom as u32);
    // A frame line round the plot: a row or column of one colour at its edge, unlike the row or
    // column inside it (a quiet stretch of plot is one colour too, but so is its neighbour).
    let flat = |pts: &mut dyn Iterator<Item = (u32, u32)>, inner: &dyn Fn(u32, u32) -> [u8; 3]| {
        let mut first = None;
        let (mut n, mut apart) = (0usize, 0f32);
        for (x, y) in pts {
            let c = p.at(x, y);
            let f = *first.get_or_insert(c);
            if distance(f.map(|v| v as f32), c) >= 24.0 {
                return false;
            }
            apart += distance(c.map(|v| v as f32), inner(x, y));
            n += 1;
        }
        n > 0 && apart / n as f32 > 40.0
    };
    for _ in 0..3 {
        if y1 - y0 > 8 && flat(&mut (x0..x1).map(|x| (x, y0)), &|x, y| p.at(x, y + 1)) {
            y0 += 1;
        }
        if y1 - y0 > 8 && flat(&mut (x0..x1).map(|x| (x, y1 - 1)), &|x, y| p.at(x, y - 1)) {
            y1 -= 1;
        }
        if x1 - x0 > 8 && flat(&mut (y0..y1).map(|y| (x0, y)), &|x, y| p.at(x + 1, y)) {
            x0 += 1;
        }
        if x1 - x0 > 8 && flat(&mut (y0..y1).map(|y| (x1 - 1, y)), &|x, y| p.at(x - 1, y)) {
            x1 -= 1;
        }
    }
    let margins = [y0 > 2, y1 + 2 < h, x0 > 2, x1 + 2 < w]
        .iter()
        .filter(|&&m| m)
        .count();
    let crop_sure = match margins {
        0 => 0.6,
        1 | 2 => 0.5,
        _ => 0.8,
    };
    let crop = [x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)];
    let (colors, colors_sure) = scale_of(p, crop);
    Found {
        crop,
        crop_sure,
        colors,
        colors_sure,
    }
}

/// The longest run of values over `at_least`, small gaps (a few rows of quiet) bridged.
fn longest(v: &[f32], at_least: f32) -> Option<(usize, usize)> {
    let smooth: Vec<f32> = (0..v.len())
        .map(|i| {
            let (a, z) = (i.saturating_sub(2), (i + 3).min(v.len()));
            v[a..z].iter().copied().fold(0f32, f32::max)
        })
        .collect();
    let mut best: Option<(usize, usize)> = None;
    let mut start = None;
    for (i, &x) in smooth.iter().chain([&0.0]).enumerate() {
        match (x >= at_least, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if best.is_none_or(|(a, z)| i - s > z - a) {
                    best = Some((s, i));
                }
                start = None;
            }
            _ => {}
        }
    }
    // Its ends where the values themselves (not their smoothing) reach it.
    let (mut a, mut z) = best.filter(|(a, z)| z - a >= 8)?;
    while a + 1 < z && v[a] < at_least {
        a += 1;
    }
    while z > a + 1 && v[z - 1] < at_least {
        z -= 1;
    }
    Some((a, z))
}

/// The colour scale a plot's pixels lie nearest, and how sure (how near they lie).
fn scale_of(p: &Picture, crop: [u32; 4]) -> (Colors, f32) {
    let [x0, y0, w, h] = crop;
    let step = ((w as u64 * h as u64 / 4000).max(1) as f64).sqrt().max(1.0) as u32;
    let sample: Vec<[u8; 3]> = (y0..y0 + h)
        .step_by(step as usize)
        .flat_map(|y| (x0..x0 + w).step_by(step as usize).map(move |x| (x, y)))
        .map(|(x, y)| p.at(x, y))
        .collect();
    let mut best = (Colors::Gray, f32::MAX);
    for c in [
        Colors::Gray,
        Colors::Viridis,
        Colors::Magma,
        Colors::Inferno,
        Colors::Plasma,
        Colors::Jet,
    ] {
        let t = table(c);
        let d = sample.iter().map(|&px| place(&t, px).1).sum::<f32>() / sample.len().max(1) as f32;
        if d < best.1 {
            best = (c, d);
        }
    }
    // Gray reads either way round: most of a spectrogram is quiet.
    if best.0 == Colors::Gray {
        let t = table(Colors::Gray);
        let mut v: Vec<f32> = sample.iter().map(|&px| place(&t, px).0).collect();
        v.sort_by(f32::total_cmp);
        if v.get(v.len() / 2).is_some_and(|&m| m > 0.5) {
            best.0 = Colors::GrayInverted;
        }
    }
    (best.0, (1.0 - best.1 / 40.0).clamp(0.1, 0.95))
}

/// A blueprint of the spectrogram in a picture's file, as far as `given` (and what's found) lets
/// it say.
pub fn from_image(path: &Path, given: &Calibration) -> Result<Blueprint, String> {
    let p = read(path)?;
    Ok(from_picture(&p, path, given))
}

/// A picture's spectrogram read: its plot's pixels as places on its colour scale, frame by frame
/// and band by band; its starts, held lines and sections; and what can't be said without what's
/// missing.
pub fn from_picture(p: &Picture, file: &Path, given: &Calibration) -> Blueprint {
    let found = find(p);
    let cal = Calibration {
        crop: Some(found.crop),
        colors: Some(found.colors),
        ..Default::default()
    }
    .over(given);
    let [x0, y0, w, h] = cal.crop.unwrap_or(found.crop);
    let (x0, y0) = (x0.min(p.width - 1), y0.min(p.height - 1));
    let (w, h) = (
        w.clamp(1, p.width - x0) as usize,
        h.clamp(1, p.height - y0) as usize,
    );
    let colors = cal.colors.unwrap_or(found.colors);
    let t = table(colors);
    let mut memo: HashMap<[u8; 3], f32> = HashMap::new();
    // Each pixel's place on the scale, rows from the top.
    let v: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| {
            let c = p.at(x0 + x as u32, y0 + y as u32);
            *memo.entry(c).or_insert_with(|| place(&t, c).0)
        })
        .collect();
    let seconds = cal.seconds.filter(|s| *s > 0.0);
    let axis = cal.axis();
    let frames = match seconds {
        Some(s) => ((s * FPS).ceil() as usize).max(1),
        None => SPREAD.min(w).max(1),
    };
    let range = cal.db_range.filter(|d| *d > 0.0).unwrap_or(-FLOOR);
    // Each band's rows, from the top (or, past the axis, none).
    let rows_of: Vec<Option<(usize, usize)>> = {
        let edges = band_edges();
        (0..BANDS)
            .map(|b| {
                let (lo, hi) = match axis {
                    Some((low, high, scale)) => (
                        scale.up(low, high, edges[b]),
                        scale.up(low, high, edges[b + 1]),
                    ),
                    None => (b as f32 / BANDS as f32, (b + 1) as f32 / BANDS as f32),
                };
                if hi <= 0.0 || lo >= 1.0 {
                    return None;
                }
                let top = ((1.0 - hi.min(1.0)) * h as f32).floor() as usize;
                let bottom = ((1.0 - lo.max(0.0)) * h as f32).ceil() as usize;
                Some((top.min(h - 1), bottom.clamp(top.min(h - 1) + 1, h)))
            })
            .collect()
    };
    let column = |t: usize| {
        let a = t * w / frames;
        (a, ((t + 1) * w / frames).max(a + 1).min(w))
    };
    let mut energy: Vec<[f32; BANDS]> = (0..frames)
        .map(|f| {
            let (a, z) = column(f);
            std::array::from_fn(|b| match rows_of[b] {
                Some((top, bottom)) => {
                    // Each pixel's level as power, averaged, as audio's bins are: a strong line
                    // in a band counts as it sounds, not as one row of many.
                    let mut sum = 0f64;
                    for y in top..bottom {
                        for x in a..z {
                            sum += 10f64.powf(((v[y * w + x] - 1.0) * range) as f64 / 10.0);
                        }
                    }
                    let mean = sum / ((bottom - top) * (z - a)) as f64;
                    ((10.0 * mean.max(1e-12).log10()) as f32).max(FLOOR)
                }
                None => FLOOR,
            })
        })
        .collect();
    let top = energy
        .iter()
        .flat_map(|r| r.iter())
        .copied()
        .fold(f32::MIN, f32::max);
    for row in &mut energy {
        for x in row.iter_mut() {
            *x = (*x - top).max(FLOOR);
        }
    }
    let bands_hz = match axis {
        Some(_) => band_centres(),
        None => Vec::new(),
    };
    let ranges: [usize; BANDS] = std::array::from_fn(|b| match axis {
        Some(_) => range_of(band_centres()[b]),
        None => b * 3 / BANDS,
    });
    let onsets = rises(
        &energy,
        &ranges,
        &std::array::from_fn(|b| rows_of[b].is_some()),
    );
    let contours = ridges(&v, w, h, frames, axis);
    // Pitch classes only where rows are fine enough to tell semitones apart.
    let fine = axis.is_some_and(|(low, high, scale)| {
        let mid = 440.0f32.clamp(low * 1.5, high / 1.5);
        (scale.up(low, high, mid * 1.0595) - scale.up(low, high, mid)) * h as f32 >= 0.8
    });
    let chroma: Vec<[f32; 12]> = match fine && !contours.is_empty() {
        true => {
            let mut c = vec![[0f32; 12]; frames];
            for line in &contours {
                if let Some(hz) = line.hz {
                    let pc = hz_to_midi(hz) as usize % 12;
                    for row in &mut c[line.start..line.end.min(frames)] {
                        row[pc] += line.strength;
                    }
                }
            }
            c
        }
        false => Vec::new(),
    };
    let sections = segment(&energy, &onsets, seconds.is_some());
    let px_per_second = seconds.map(|s| w as f32 / s);
    let tempo = match px_per_second {
        Some(pps) if pps >= 16.0 => tempo_of(&v, w, h, pps),
        _ => None,
    };
    let key = (!chroma.is_empty())
        .then(|| {
            let mut all = [0f32; 12];
            for row in &chroma {
                for k in 0..12 {
                    all[k] += row[k];
                }
            }
            let scored = crate::study::harmony::keys(&all, &all);
            scored
                .first()
                .filter(|k| k.2 > 0.5)
                .map(|k| (k.0.name(), k.2))
        })
        .flatten();
    let mut needs = Vec::new();
    if seconds.is_none() {
        needs.push("How long the plot runs, in seconds (its time axis)".to_string());
    }
    if axis.is_none() {
        needs.push(
            "Its frequency axis: the lowest and highest frequency, and whether it runs linear, log \
             or mel"
                .to_string(),
        );
    }
    if given.colors.is_none() && found.colors_sure < 0.5 {
        needs.push(
            "Its colour scale (gray, viridis, magma, inferno, plasma or jet): its colours match none \
             well"
                .to_string(),
        );
    }
    if given.crop.is_none() && found.crop_sure < 0.6 {
        needs.push("Check the plot's edges: they were found, not given".to_string());
    }
    let mut bp = Blueprint {
        version: VERSION,
        source: Source::Image,
        file: file.to_path_buf(),
        calibration: Some(Calibration {
            crop: Some([x0, y0, w as u32, h as u32]),
            colors: Some(colors),
            ..cal.clone()
        }),
        seconds,
        frames,
        bands_hz,
        energy,
        onsets,
        chroma,
        contours,
        bass: Vec::new(),
        melody: Vec::new(),
        sections,
        tempo: tempo.map(|t| t.0),
        meter: None,
        bars: Vec::new(),
        key: key.as_ref().map(|k| k.0.clone()),
        facts: Vec::new(),
        needs,
    };
    bp.facts = facts(&bp, &found, given, tempo, key, fine);
    bp
}

/// Held lines in a picture: in each frame's columns, the rows that stand out from those round
/// them (a peak over its neighbours), joined frame to frame while they stay within a row or two,
/// kept if they hold.
fn ridges(
    v: &[f32],
    w: usize,
    h: usize,
    frames: usize,
    axis: Option<(f32, f32, FreqScale)>,
) -> Vec<Contour> {
    let reach = (h / 200).max(1);
    let profile = |f: usize| -> Vec<f32> {
        let a = f * w / frames;
        let z = ((f + 1) * w / frames).max(a + 1).min(w);
        (0..h)
            .map(|y| v[y * w + a..y * w + z].iter().sum::<f32>() / (z - a) as f32)
            .collect()
    };
    struct Line {
        start: usize,
        last: usize,
        row: usize,
        sum_row: usize,
        sum_v: f32,
        n: usize,
    }
    let mut open: Vec<Line> = Vec::new();
    let mut done: Vec<Contour> = Vec::new();
    let least = 8usize.max(frames / 50);
    let close = |l: &Line, done: &mut Vec<Contour>| {
        if l.last + 1 - l.start >= least {
            let row = l.sum_row as f32 / l.n as f32;
            let up = 1.0 - (row + 0.5) / h as f32;
            done.push(Contour {
                start: l.start,
                end: l.last + 1,
                hz: axis.map(|(low, high, scale)| scale.hz(low, high, up)),
                up,
                strength: (l.sum_v / l.n as f32).clamp(0.0, 1.0),
            });
        }
    };
    for f in 0..frames {
        let col = profile(f);
        let span = 3usize;
        let peaks: Vec<usize> = (span..h.saturating_sub(span))
            .filter(|&y| {
                let x = col[y];
                let around = col[y - span..=y + span]
                    .iter()
                    .copied()
                    .fold(f32::MAX, f32::min);
                x > 0.35 && x >= col[y - 1] && x >= col[y + 1] && x - around > 0.12
            })
            .collect();
        let mut taken = vec![false; open.len()];
        for y in peaks {
            match (0..taken.len()).find(|&i| !taken[i] && open[i].row.abs_diff(y) <= reach) {
                Some(i) => {
                    taken[i] = true;
                    let l = &mut open[i];
                    (l.last, l.row) = (f, y);
                    l.sum_row += y;
                    l.sum_v += col[y];
                    l.n += 1;
                }
                None => open.push(Line {
                    start: f,
                    last: f,
                    row: y,
                    sum_row: y,
                    sum_v: col[y],
                    n: 1,
                }),
            }
        }
        let (keep, gone): (Vec<Line>, Vec<Line>) = open.into_iter().partition(|l| f <= l.last + 1);
        for l in &gone {
            close(l, &mut done);
        }
        open = keep;
    }
    for l in &open {
        close(l, &mut done);
    }
    done.sort_by(|a, b| {
        ((b.end - b.start) as f32 * b.strength).total_cmp(&((a.end - a.start) as f32 * a.strength))
    });
    done.truncate(240);
    done.sort_by_key(|c| c.start);
    done
}

/// A tempo guessed from a picture's columns (`pps` a second): how their rises repeat, the
/// strongest repeat between 70 and 180 BPM, and how much it stands out.
fn tempo_of(v: &[f32], w: usize, h: usize, pps: f32) -> Option<(f32, f32)> {
    // Rises in the lower two thirds, where kicks and snares are.
    let rows = h / 3..h;
    let col = |x: usize| rows.clone().map(|y| v[y * w + x]).sum::<f32>() / rows.len() as f32;
    let cols: Vec<f32> = (0..w).map(col).collect();
    let rise: Vec<f32> = (1..w).map(|x| (cols[x] - cols[x - 1]).max(0.0)).collect();
    let m = rise.iter().sum::<f32>() / rise.len().max(1) as f32;
    let centred: Vec<f32> = rise.iter().map(|r| r - m).collect();
    let corr = |lag: usize| -> f32 {
        centred
            .iter()
            .zip(&centred[lag.min(centred.len())..])
            .map(|(a, b)| a * b)
            .sum::<f32>()
    };
    let (lo, hi) = (
        (60.0 * pps / 180.0) as usize,
        (60.0 * pps / 70.0).ceil() as usize,
    );
    if lo < 2 || hi >= centred.len() / 2 {
        return None;
    }
    let scores: Vec<(usize, f32)> = (lo..=hi).map(|l| (l, corr(l))).collect();
    let (lag, best) = scores.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1))?;
    let mean = scores.iter().map(|s| s.1).sum::<f32>() / scores.len() as f32;
    let zero = corr(0).max(1e-6);
    let sure = ((best - mean) / zero * 2.0).clamp(0.0, 0.6);
    if sure < 0.1 {
        return None;
    }
    // Between columns, where the peak would be.
    let at = |l: usize| scores.iter().find(|s| s.0 == l).map_or(best, |s| s.1);
    let (a, c) = (at(lag.saturating_sub(1)), at(lag + 1));
    let d = a - 2.0 * best + c;
    let shift = if d.abs() > 1e-9 {
        0.5 * (a - c) / d
    } else {
        0.0
    };
    let bpm = 60.0 * pps / (lag as f32 + shift.clamp(-0.5, 0.5));
    Some(((bpm * 10.0).round() / 10.0, sure))
}

/// What a picture says, each finding with how it was come by: what was given is measured, what
/// was found estimated, and what a spectrogram can't show is unknown.
fn facts(
    bp: &Blueprint,
    found: &Found,
    given: &Calibration,
    tempo: Option<(f32, f32)>,
    key: Option<(String, f32)>,
    fine: bool,
) -> Vec<Fact> {
    let fact = |name: &str, value: String, basis: Basis, confidence: f32| Fact {
        name: name.into(),
        value,
        basis,
        confidence,
        from: Source::Image,
    };
    let cal = bp.calibration.clone().unwrap_or_default();
    let [x, y, w, h] = cal.crop.unwrap_or(found.crop);
    let mut out = vec![
        match given.crop {
            Some(_) => fact(
                "Plot",
                format!("{w}×{h} px at {x}, {y}"),
                Basis::Measured,
                0.9,
            ),
            None => fact(
                "Plot",
                format!("{w}×{h} px at {x}, {y}"),
                Basis::Estimated,
                found.crop_sure,
            ),
        },
        match given.colors {
            Some(c) => fact("Colours", c.name().into(), Basis::Measured, 0.9),
            None => fact(
                "Colours",
                found.colors.name().into(),
                Basis::Estimated,
                found.colors_sure,
            ),
        },
        match bp.seconds {
            Some(s) => fact("Length", format!("{s:.0} s (given)"), Basis::Measured, 0.9),
            None => fact("Length", "unknown".into(), Basis::Unknown, 0.0),
        },
        match cal.axis() {
            Some((lo, hi, scale)) => fact(
                "Frequencies",
                format!("{lo:.0} Hz to {hi:.0} Hz, {} (given)", scale.name()),
                Basis::Measured,
                0.9,
            ),
            None => fact("Frequencies", "unknown".into(), Basis::Unknown, 0.0),
        },
        match given.db_range {
            Some(d) => fact(
                "Level scale",
                format!("{d:.0} dB (given)"),
                Basis::Measured,
                0.9,
            ),
            None => fact(
                "Level scale",
                format!("relative only (read as {:.0} dB)", -FLOOR),
                Basis::Estimated,
                0.3,
            ),
        },
        fact(
            "Form",
            bp.sections
                .iter()
                .map(|p| p.label.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            Basis::Estimated,
            bp.sections.first().map_or(0.0, |p| p.confidence),
        ),
    ];
    let b = bp.balance(0, bp.frames);
    let names = match cal.axis() {
        Some(_) => ["low", "mid", "high"],
        None => ["bottom", "middle", "top"],
    };
    out.push(fact(
        "Balance",
        format!(
            "{} {:+.1}, {} {:+.1}, {} {:+.1} (relative)",
            names[0], b[0], names[1], b[1], names[2], b[2]
        ),
        Basis::Estimated,
        0.4,
    ));
    out.push(match tempo {
        Some((bpm, sure)) => fact("Tempo", format!("{bpm:.0} BPM"), Basis::Estimated, sure),
        None => fact("Tempo", "unknown".into(), Basis::Unknown, 0.0),
    });
    out.push(fact(
        "Held lines",
        format!("{}", bp.contours.len()),
        Basis::Estimated,
        0.4,
    ));
    out.push(match (&key, fine) {
        (Some((k, sure)), true) => fact("Tonal centre", k.clone(), Basis::Estimated, sure * 0.5),
        (_, true) => fact("Tonal centre", "unclear".into(), Basis::Unknown, 0.0),
        (_, false) => fact(
            "Pitches",
            "can't be told at this resolution".into(),
            Basis::Unknown,
            0.0,
        ),
    });
    for what in [
        "Chords",
        "Instruments",
        "Loudness (LUFS)",
        "Stereo width",
        "Lyrics",
    ] {
        out.push(fact(
            what,
            "a spectrogram doesn't show it".into(),
            Basis::Unknown,
            0.0,
        ));
    }
    out
}
