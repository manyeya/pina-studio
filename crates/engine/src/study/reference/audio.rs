//! A blueprint from audio: a song's file (its study giving its tempo, bars, sections, key,
//! chords and mix), or a song's own render (its tempo, bars and sections as its project has
//! them). Everything here is measured; what the study only guesses at (what plays) is estimated.

use std::path::PathBuf;

use super::*;
use crate::study::{Study, features};

/// What's known of a song's time: its tempo and meter, where its bars start (seconds), and its
/// sections (name, letter, start and end in seconds).
#[derive(Clone, Debug, Default)]
pub struct Shape {
    pub bpm: f32,
    pub meter: [u32; 2],
    pub bars: Vec<f32>,
    pub sections: Vec<(String, String, f32, f32)>,
}

impl Shape {
    /// A study's: its tempo, meter, bars and form.
    pub fn of_study(s: &Study) -> Shape {
        Shape {
            bpm: s.tempo.bpm,
            meter: s.tempo.time_signature,
            bars: s.tempo.bars.clone(),
            sections: s
                .sections
                .iter()
                .map(|x| (x.name.clone(), x.label.clone(), x.start, x.end))
                .collect(),
        }
    }
}

/// The analysis frame: about 93 ms, for pitch detail.
const FRAME_SECONDS: f32 = 0.093;
/// Pitched peaks a frame looks for, and the most a contour may drift a frame, in semitones.
const PEAKS: usize = 4;
const DRIFT: f32 = 0.5;
/// The shortest a held line is, in frames (0.4 s).
const HELD: usize = 8;

/// A blueprint of a song's audio. `shape` gives its tempo, bars and sections when they're known
/// (a study's, or a render's project's); without, sections are found where its sound changes.
pub fn from_audio(
    l: &[f32],
    r: &[f32],
    rate: f32,
    source: Source,
    file: PathBuf,
    shape: Option<&Shape>,
    study: Option<&Study>,
) -> Blueprint {
    let mono: Vec<f32> = l.iter().zip(r).map(|(a, b)| 0.5 * (a + b)).collect();
    let seconds = mono.len() as f32 / rate;
    // At its own rate, not decimated: its top octaves (cymbals, air) are in a picture of it too.
    let (x, sr) = (&mono, rate);
    let size = ((sr * FRAME_SECONDS) as usize).next_power_of_two();
    let hop = (sr / FPS).round().max(1.0) as usize;
    let frames = x.len().div_ceil(hop).max(1);
    let bin = sr / size as f32;
    let edges = band_edges();
    let band_of: Vec<Option<usize>> = (0..size / 2 + 1)
        .map(|k| {
            let f = k as f32 * bin;
            (0..BANDS).find(|&b| f >= edges[b] && f < edges[b + 1])
        })
        .collect();
    // Bands too narrow to hold a bin take their nearest bin's power.
    let nearest: Vec<usize> = band_centres()
        .iter()
        .map(|&c| ((c / bin).round() as usize).clamp(1, size / 2))
        .collect();
    let mut spectra = crate::samples::dsp::Spectra::new(size);
    let mut power = vec![0f32; size / 2 + 1];
    let mut raw: Vec<[f32; BANDS]> = Vec::with_capacity(frames);
    let mut peaks: Vec<Vec<(f32, f32)>> = Vec::with_capacity(frames);
    for t in 0..frames {
        spectra.power(x, (t * hop) as isize - (size / 2) as isize, &mut power);
        let mut bands = [0f32; BANDS];
        let mut count = [0usize; BANDS];
        for (k, p) in power.iter().enumerate() {
            if let Some(b) = band_of[k] {
                bands[b] += p;
                count[b] += 1;
            }
        }
        // Each band's power a bin, as a spectrogram's pixels show it (a picture's levels are
        // its bins', not its bands' sums), so audio and pictures read alike.
        for b in 0..BANDS {
            bands[b] = match count[b] {
                0 => power[nearest[b]],
                n => bands[b] / n as f32,
            };
        }
        raw.push(bands.map(|p| 10.0 * p.max(1e-12).log10()));
        peaks.push(pitched_peaks(&power, bin));
    }
    let top = raw
        .iter()
        .flat_map(|r| r.iter())
        .copied()
        .fold(f32::MIN, f32::max);
    let energy: Vec<[f32; BANDS]> = raw
        .iter()
        .map(|r| r.map(|v| (v - top).max(FLOOR)))
        .collect();
    // Starts, fine (a few ms) and per frame; pitch classes per frame.
    let f = features::frames(&mono, rate);
    let fine = fine_onsets(&f);
    // Starts per frame as a picture's are read (from the bands rising), so they compare; the
    // fine ones place a bar's steps.
    let ranges: [usize; BANDS] = std::array::from_fn(|b| range_of(band_centres()[b]));
    let onsets = rises(&energy, &ranges, &[true; BANDS]);
    let per_chroma = f.rate / FPS;
    let chroma: Vec<[f32; 12]> = (0..frames)
        .map(|t| {
            let a = (t as f32 * per_chroma) as usize;
            let z = (((t + 1) as f32 * per_chroma) as usize).max(a + 1);
            features::mean(&f.chroma, a, z)
        })
        .collect();
    let contours = track(&peaks, top);
    let mut sections = match shape {
        Some(sh) if !sh.sections.is_empty() => {
            let spans: Vec<(usize, usize)> = sh
                .sections
                .iter()
                .map(|s| {
                    let a = ((s.2 * FPS) as usize).min(frames);
                    (a, ((s.3 * FPS) as usize).clamp(a + 1, frames.max(a + 1)))
                })
                .collect();
            let mut parts = parts_of(&energy, &spans, |_, _| 0.0, 0.8);
            for (p, s) in parts.iter_mut().zip(&sh.sections) {
                (p.name, p.label) = (s.0.clone(), s.1.clone());
                p.bars = Some(
                    sh.bars
                        .iter()
                        .filter(|&&b| b >= s.2 - 0.05 && b < s.3 - 0.05)
                        .count()
                        .max(1),
                );
            }
            parts
        }
        _ => segment(&energy, &onsets, true),
    };
    if let Some(s) = study {
        for (p, x) in sections.iter_mut().zip(&s.sections) {
            p.chords = x.chords.clone();
        }
    }
    let bar_times: &[f32] = shape.map_or(&[], |s| &s.bars);
    let (bass, melody) = match shape {
        Some(sh) if bar_times.len() > 1 => lines(
            &f, &fine, &energy, &peaks, top, bar_times, seconds, sh.meter,
        ),
        _ => (Vec::new(), Vec::new()),
    };
    for p in &mut sections {
        p.grid = grid(
            &fine,
            f.onset_rate,
            &bar_times
                .iter()
                .copied()
                .chain([seconds])
                .collect::<Vec<_>>(),
            p.start as f32 / FPS,
            p.end as f32 / FPS,
        );
    }
    let mut bp = Blueprint {
        version: VERSION,
        source,
        file,
        calibration: None,
        seconds: Some(seconds),
        frames,
        bands_hz: band_centres(),
        energy,
        onsets,
        chroma,
        contours,
        bass,
        melody,
        sections,
        tempo: shape.map(|s| s.bpm),
        meter: shape.map(|s| s.meter),
        bars: bar_times.iter().map(|b| b * FPS).collect(),
        key: study.map(|s| s.key.name.clone()),
        facts: Vec::new(),
        needs: Vec::new(),
    };
    bp.facts = facts(&bp, shape, study);
    bp
}

/// The bass and the melody, note by note on each bar's sixteenths. The bass from the low end's
/// pitch classes: a note starts where its pitch changes or a low hit lands, and rests where the
/// low end falls quiet. The melody from the strongest pitched peak in the singing range (200 Hz
/// to 1.2 kHz): a note starts where it moves a semitone or more. Estimates, for their shape and
/// rhythm: a mix's chords and pads can stand in for a melody.
#[allow(clippy::too_many_arguments)]
fn lines(
    f: &features::Frames,
    fine: &[[f32; 3]],
    energy: &[[f32; BANDS]],
    peaks: &[Vec<(f32, f32)>],
    top: f32,
    bars: &[f32],
    end: f32,
    meter: [u32; 2],
) -> (Vec<LineNote>, Vec<LineNote>) {
    let per_bar = (meter[0] * 16 / meter[1].max(1)).max(1) as usize;
    let steps: Vec<(f32, f32)> = bars
        .iter()
        .copied()
        .chain([end])
        .collect::<Vec<_>>()
        .windows(2)
        .flat_map(|w| {
            let len = (w[1] - w[0]) / per_bar as f32;
            (0..per_bar).map(move |k| (w[0] + len * k as f32, w[0] + len * (k + 1) as f32))
        })
        .collect();
    let lows: Vec<usize> = (0..BANDS)
        .filter(|&b| band_centres()[b] < LOW_MID)
        .collect();
    let low_db = |a: f32, z: f32| {
        let (fa, fz) = (
            (a * FPS) as usize,
            ((z * FPS) as usize).max((a * FPS) as usize + 1),
        );
        let rows = &energy[fa.min(energy.len())..fz.min(energy.len())];
        let p: f32 = rows
            .iter()
            .flat_map(|r| lows.iter().map(|&b| 10f32.powf(r[b] / 10.0)))
            .sum::<f32>()
            / (rows.len() * lows.len()).max(1) as f32;
        10.0 * p.max(1e-9).log10()
    };
    let levels: Vec<f32> = steps.iter().map(|&(a, z)| low_db(a, z)).collect();
    let loudest = levels.iter().copied().fold(f32::MIN, f32::max);
    let (mut bass, mut melody) = (Vec::new(), Vec::new());
    // The note sounding in each line: where it started (frame) and its pitch.
    let mut b_now: Option<(usize, f32)> = None;
    let mut m_now = b_now;
    let frame = |t: f32| (t * FPS) as usize;
    let close = |line: &mut Vec<LineNote>, now: &mut Option<(usize, f32)>, at: f32| {
        if let Some((start, pitch)) = now.take() {
            line.push(LineNote {
                start,
                end: frame(at).max(start + 1),
                pitch: pitch.round().clamp(0.0, 127.0) as u8,
            });
        }
    };
    // A beat's steps: the bass's pitch is its beat's (a log drum's bend or a slide doesn't make
    // new notes), its rhythm the step's.
    let per_beat = (16 / meter[1].max(1)).max(1) as usize;
    // The song's tuning, so a note a little off A440 doesn't read as its neighbours.
    let tune = f.tuning / 100.0;
    for (k, &(a, z)) in steps.iter().enumerate() {
        // The bass: its beat's pitch class, if the low end sounds in the step.
        let beat = k - k % per_beat;
        let (ba, bz) = (
            steps[beat].0,
            steps[(beat + per_beat - 1).min(steps.len() - 1)].1,
        );
        let (fa, fz) = (
            (ba * f.rate) as usize,
            ((bz * f.rate) as usize).max((ba * f.rate) as usize + 1),
        );
        let pcs = features::mean(&f.bass, fa, fz);
        let sum: f32 = pcs.iter().sum();
        let (pc, most) = pcs
            .iter()
            .copied()
            .enumerate()
            .max_by(|x, y| x.1.total_cmp(&y.1))
            .unwrap_or((0, 0.0));
        let sounding = levels[k] > loudest - 24.0 && sum > 0.0 && most / sum > 0.2;
        let (oa, oz) = (
            ((a - 0.03).max(0.0) * f.onset_rate) as usize,
            ((a + (z - a) * 0.5) * f.onset_rate) as usize,
        );
        let hit = fine[oa.min(fine.len())..oz.min(fine.len())]
            .iter()
            .any(|o| o[0] > 0.5);
        // Its note from the strongest low peak over the beat, placed between bins and tuned (low
        // notes sit closer together than the bins); else its pitch class, in the second octave.
        let low_peak = peaks
            [frame(ba).min(peaks.len())..frame(bz).max(frame(ba) + 1).min(peaks.len())]
            .iter()
            .flat_map(|fr| fr.iter().copied())
            .filter(|&(hz, _)| hz < 250.0)
            .max_by(|x, y| x.1.total_cmp(&y.1));
        let pitch = match low_peak {
            Some((hz, _)) => (semitones(hz) - tune).round(),
            None => 36.0 + pc as f32,
        };
        match (sounding, b_now) {
            (false, _) => close(&mut bass, &mut b_now, a),
            (true, Some((_, p))) if (p - pitch).abs() < 0.5 && !hit => {}
            (true, _) => {
                close(&mut bass, &mut b_now, a);
                b_now = Some((frame(a), pitch));
            }
        }
        // The melody: the strongest peak in the singing range over the step.
        let best = peaks[frame(a).min(peaks.len())..frame(z).max(frame(a) + 1).min(peaks.len())]
            .iter()
            .flat_map(|fr| fr.iter().copied())
            .filter(|&(hz, db)| (200.0..1200.0).contains(&hz) && db > top - 35.0)
            .max_by(|x, y| x.1.total_cmp(&y.1));
        match (best, m_now) {
            (None, _) => close(&mut melody, &mut m_now, a),
            (Some((hz, _)), Some((_, p))) if (semitones(hz) - tune - p).abs() < 0.75 => {}
            (Some((hz, _)), _) => {
                close(&mut melody, &mut m_now, a);
                m_now = Some((frame(a), semitones(hz) - tune));
            }
        }
    }
    let last = steps.last().map_or(end, |s| s.1);
    close(&mut bass, &mut b_now, last);
    close(&mut melody, &mut m_now, last);
    // The melody's shape: a note shorter than an eighth is a blip of the one before it.
    let eighth = ((steps.first().map_or(0.1, |s| s.1 - s.0) * 2.0 * FPS) as usize).max(1);
    let mut shaped: Vec<LineNote> = Vec::new();
    for n in melody {
        match shaped.last_mut() {
            Some(prev) if n.end - n.start < eighth && n.start <= prev.end + 1 => prev.end = n.end,
            Some(prev) if prev.pitch == n.pitch && n.start <= prev.end + 1 => prev.end = n.end,
            _ => shaped.push(n),
        }
    }
    (bass, shaped)
}

/// The kick's, snare's and hats' starts, each against its loudest (its 98th percentile), 0 to 1.
fn fine_onsets(f: &features::Frames) -> Vec<[f32; 3]> {
    let scale = |v: &[f32]| {
        let mut s: Vec<f32> = v.to_vec();
        s.sort_by(f32::total_cmp);
        s.get(s.len() * 98 / 100).copied().unwrap_or(1.0).max(1e-6)
    };
    let k = [scale(&f.kick), scale(&f.snare), scale(&f.hats)];
    (0..f.kick.len())
        .map(|i| {
            [
                (f.kick[i] / k[0]).clamp(0.0, 1.0),
                (f.snare.get(i).copied().unwrap_or(0.0) / k[1]).clamp(0.0, 1.0),
                (f.hats.get(i).copied().unwrap_or(0.0) / k[2]).clamp(0.0, 1.0),
            ]
        })
        .collect()
}

/// A frame's strongest pitched peaks, 60 Hz to 5 kHz: (Hz, dB), each a local top within 30 dB of
/// the frame's loudest, placed between bins.
fn pitched_peaks(power: &[f32], bin: f32) -> Vec<(f32, f32)> {
    let (lo, hi) = (
        (30.0 / bin) as usize,
        ((5000.0 / bin) as usize).min(power.len() - 2),
    );
    let db = |k: usize| 10.0 * power[k].max(1e-12).log10();
    let loudest = (lo..=hi).map(db).fold(f32::MIN, f32::max);
    let mut found: Vec<(f32, f32)> = (lo.max(1)..=hi)
        .filter(|&k| power[k] > power[k - 1] && power[k] >= power[k + 1] && db(k) > loudest - 30.0)
        .map(|k| {
            let (a, b, c) = (db(k - 1), db(k), db(k + 1));
            let d = a - 2.0 * b + c;
            let shift = if d.abs() > 1e-6 {
                0.5 * (a - c) / d
            } else {
                0.0
            };
            ((k as f32 + shift.clamp(-0.5, 0.5)) * bin, b)
        })
        .collect();
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.truncate(PEAKS);
    found
}

fn semitones(hz: f32) -> f32 {
    69.0 + 12.0 * (hz.max(1.0) / 440.0).log2()
}

/// Peaks joined frame to frame into held lines: a peak within half a semitone of a line that
/// sounded a frame or two before carries it on; lines that hold at least `HELD` frames are kept,
/// the strongest and longest first.
fn track(peaks: &[Vec<(f32, f32)>], top: f32) -> Vec<Contour> {
    struct Line {
        start: usize,
        last: usize,
        pitch: f32,
        sum_pitch: f32,
        sum_db: f32,
        n: usize,
    }
    let mut open: Vec<Line> = Vec::new();
    let mut done: Vec<Contour> = Vec::new();
    let close = |l: &Line, done: &mut Vec<Contour>| {
        let len = l.last + 1 - l.start;
        if len >= HELD {
            let pitch = l.sum_pitch / l.n as f32;
            let hz = 440.0 * 2f32.powf((pitch - 69.0) / 12.0);
            done.push(Contour {
                start: l.start,
                end: l.last + 1,
                hz: Some(hz),
                up: ((hz / LOW_HZ).ln() / (HIGH_HZ / LOW_HZ).ln()).clamp(0.0, 1.0),
                strength: ((l.sum_db / l.n as f32 - top - FLOOR) / -FLOOR).clamp(0.0, 1.0),
            });
        }
    };
    for (t, frame) in peaks.iter().enumerate() {
        let mut taken = vec![false; open.len()];
        for &(hz, db) in frame {
            let p = semitones(hz);
            let near = (0..taken.len())
                .filter(|&i| !taken[i] && (open[i].pitch - p).abs() < DRIFT)
                .min_by(|&a, &b| {
                    (open[a].pitch - p)
                        .abs()
                        .total_cmp(&(open[b].pitch - p).abs())
                });
            match near {
                Some(i) => {
                    taken[i] = true;
                    let l = &mut open[i];
                    (l.last, l.pitch) = (t, p);
                    l.sum_pitch += p;
                    l.sum_db += db;
                    l.n += 1;
                }
                None => open.push(Line {
                    start: t,
                    last: t,
                    pitch: p,
                    sum_pitch: p,
                    sum_db: db,
                    n: 1,
                }),
            }
        }
        let (keep, gone): (Vec<Line>, Vec<Line>) = open.into_iter().partition(|l| t <= l.last + 1);
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

/// What audio says, each finding with how sure: its length, tempo, meter, key, form, chords,
/// groove, loudness and width, what plays (estimated), its lyrics if they were found; and its
/// balance.
fn facts(bp: &Blueprint, shape: Option<&Shape>, study: Option<&Study>) -> Vec<Fact> {
    let from = bp.source;
    let fact = |name: &str, value: String, basis: Basis, confidence: f32| Fact {
        name: name.into(),
        value,
        basis,
        confidence,
        from,
    };
    let mut out = vec![fact(
        "Length",
        format!("{:.0} s", bp.seconds.unwrap_or(0.0)),
        Basis::Measured,
        1.0,
    )];
    let tempo_sure = study.map_or(1.0, |s| s.tempo.confidence);
    if let Some(sh) = shape {
        out.push(fact(
            "Tempo",
            format!("{:.1} BPM", sh.bpm),
            Basis::Measured,
            tempo_sure,
        ));
        out.push(fact(
            "Meter",
            format!("{}/{}", sh.meter[0], sh.meter[1]),
            Basis::Measured,
            tempo_sure.min(0.8),
        ));
    }
    if let Some(s) = study {
        out.push(fact(
            "Key",
            s.key.name.clone(),
            Basis::Measured,
            s.key.fit.clamp(0.0, 1.0),
        ));
        out.push(fact(
            "Swing",
            format!("{:.0}%", s.groove.swing),
            Basis::Measured,
            0.7,
        ));
        out.push(fact(
            "Loudness",
            format!("{:.1} LUFS", s.mix.overall.lufs),
            Basis::Measured,
            1.0,
        ));
        out.push(fact(
            "Width",
            match s.mix.overall.stereo.width_db {
                w if w < -60.0 => "mono".to_string(),
                w => format!("{w:+.1} dB sides"),
            },
            Basis::Measured,
            1.0,
        ));
        for e in s.instruments.iter().take(6) {
            out.push(fact(
                "Plays",
                format!("{} ({})", e.name, e.role),
                Basis::Estimated,
                e.confidence,
            ));
        }
        if s.lyrics.is_some() {
            out.push(fact("Lyrics", "found online".into(), Basis::Measured, 0.9));
        }
    }
    let sure = bp.sections.first().map_or(0.0, |p| p.confidence);
    out.push(fact(
        "Form",
        bp.sections
            .iter()
            .map(|p| p.label.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        match shape {
            Some(_) => Basis::Measured,
            None => Basis::Estimated,
        },
        sure,
    ));
    if study.is_some() {
        out.push(fact(
            "Chords",
            "each section's loop".into(),
            Basis::Measured,
            0.6,
        ));
    }
    let b = bp.balance(0, bp.frames);
    out.push(fact(
        "Balance",
        format!(
            "low {:+.1} dB, mid {:+.1} dB, high {:+.1} dB",
            b[0], b[1], b[2]
        ),
        Basis::Measured,
        1.0,
    ));
    out.push(fact(
        "Held lines",
        format!("{}", bp.contours.len()),
        Basis::Measured,
        0.6,
    ));
    out
}
