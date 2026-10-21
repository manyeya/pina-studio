//! Everything measured from a sample's audio, before any of it is named: level, envelope,
//! spectrum, timbre (mel cepstra), rhythm, pitch and notes. `analyze` turns these into an
//! `Analysis`; the classifier and the embedding read them too.

use super::{
    Envelope, Spectrum,
    dsp::{self, Mel, Stft},
    pitch, rhythm,
    tonal::{self, Tonal},
};

/// How much audio the spectral measures look at: a long file's first 30 seconds.
const MAX_SECONDS: f32 = 30.0;
/// Mel bands, and cepstra kept from them.
pub const MELS: usize = 40;
pub const CEPSTRA: usize = 13;

#[derive(Clone, Debug, Default)]
pub struct Features {
    pub seconds: f32,
    pub rate: f32,
    /// Peak magnitude (linear) over all channels.
    pub peak: f32,
    /// The loudest 400 ms, and integrated, in LUFS.
    pub loudness_short: f32,
    pub loudness_long: f32,
    /// The whole sound's envelope, from its loudest point.
    pub envelope: Envelope,
    /// A typical hit's envelope (medians over its hits), when it has several.
    pub hits: Option<Envelope>,
    /// Its level over the first 10 ms and the last 30 ms, in dB under its peak.
    pub start_db: f32,
    pub end_db: f32,
    pub spectrum: Spectrum,
    /// 99% roll-off: where nearly all its energy is under (Hz).
    pub bandwidth: f32,
    /// How far its spectrum spreads around its centroid (octaves).
    pub spread: f32,
    /// Spectral centroid over its first 30 ms after the peak, and after that (Hz).
    pub centroid_attack: f32,
    pub centroid_body: f32,
    pub width: f32,
    /// Mel cepstra: the mean and spread over its frames, weighted by loudness.
    pub mfcc_mean: [f32; CEPSTRA],
    pub mfcc_std: [f32; CEPSTRA],
    /// Peak over RMS of its sounding part (dB).
    pub crest_db: f32,
    /// Zero crossings a second.
    pub zcr: f32,
    /// The share of its samples in flat-topped runs at full scale.
    pub clipped: f32,
    /// Onset times (seconds), and the onset strength track at `fps`.
    pub onsets: Vec<f32>,
    pub strength: Vec<f32>,
    pub fps: f32,
    pub tempo: Option<rhythm::Tempo>,
    pub pitch: Option<pitch::Summary>,
    pub tonal: Tonal,
    /// Bursts in its first 80 ms: a clap's several, a snare's one.
    pub bursts: f32,
    /// How far its low end's centroid (under 400 Hz) falls from its first 25 ms to the 50 ms
    /// after (octaves): a kick's pitch drop.
    pub low_drop: f32,
    /// Its first 35 ms's cepstra (1 to 4) against its average: how its attack differs.
    pub attack_mfcc: [f32; 4],
    /// How many of its note's first 12 harmonics are within 30 dB of the fundamental.
    pub harmonics: f32,
    /// In its lows (under 300 Hz), mids and highs (over 3 kHz): how long each takes to fall
    /// 20 dB from its own peak (ln ms), and how much later than the sound's start each peaks
    /// (ln ms).
    pub band_decay: [f32; 3],
    pub band_peak: [f32; 3],
    /// Spectral contrast in six octave-ish bands: how far peaks stand over valleys (dB).
    pub contrast: [f32; 6],
}

/// Measures audio: `channels` at `rate`. `named_bpm` is a tempo its name gives (and whether the
/// audio has to bear it out).
pub fn extract(channels: &[Vec<f32>], rate: f64, named_bpm: Option<(f32, bool)>) -> Features {
    let rate_f = rate as f32;
    let blocked: Vec<Vec<f32>> = channels.iter().map(|c| dsp::dc_block(c, rate_f)).collect();
    let channels = &blocked[..];
    let len = channels.first().map_or(0, Vec::len);
    let mut f = Features {
        seconds: len as f32 / rate_f,
        rate: rate_f,
        loudness_short: -70.0,
        loudness_long: -70.0,
        start_db: -120.0,
        end_db: -120.0,
        ..Default::default()
    };
    f.peak = channels.iter().flatten().fold(0f32, |m, s| m.max(s.abs()));
    if len == 0 || f.peak <= 1e-7 {
        return f;
    }
    let mono: Vec<f32> = (0..len)
        .map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / channels.len() as f32)
        .collect();
    f.loudness_short = dsp::loudness(channels, rate, true);
    f.loudness_long = dsp::loudness(channels, rate, false);
    f.width = width(channels);
    f.clipped = clipped(channels, f.peak);

    // The level envelope, a millisecond a point.
    let block = (rate_f / 1000.0).round().max(1.0) as usize;
    let ms = block as f32 * 1000.0 / rate_f;
    let fine: Vec<f32> = (0..len.div_ceil(block))
        .map(|b| {
            channels
                .iter()
                .flat_map(|c| &c[b * block..((b + 1) * block).min(len)])
                .fold(0f32, |m, s| m.max(s.abs()))
        })
        .collect();
    let peak_at = (0..fine.len())
        .max_by(|&a, &b| fine[a].total_cmp(&fine[b]))
        .unwrap_or(0);
    let smooth = dsp::running_max(&fine, 20);
    f.envelope = envelope(&fine, &smooth, peak_at, (0, fine.len()), false, ms);
    let level = |range: std::ops::Range<usize>| {
        let r = range.start.min(fine.len())..range.end.min(fine.len());
        dsp::amp_db(fine[r].iter().fold(0f32, |m, &v| m.max(v)) / f.peak)
    };
    f.start_db = level(0..(10.0 / ms).ceil() as usize);
    f.end_db = level(fine.len().saturating_sub((30.0 / ms).ceil() as usize)..fine.len());
    let sounding_end = (peak_at as f32 + f.envelope.tail_ms / ms) as usize;
    let start_block = peak_at.saturating_sub((f.envelope.attack_ms / ms) as usize);
    let (a, b) = (
        (start_block * block).min(len),
        (sounding_end * block + block).clamp(1, len),
    );
    let sounding = &mono[a.min(b - 1)..b];
    let rms = (sounding.iter().map(|v| v * v).sum::<f32>() / sounding.len() as f32).sqrt();
    f.crest_db = dsp::amp_db(f.peak / rms.max(1e-9));
    f.zcr = sounding
        .windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count() as f32
        / (sounding.len() as f32 / rate_f).max(1e-3);

    // Spectra, a frame every 11 ms or so.
    let cap = len.min((MAX_SECONDS * rate_f) as usize);
    let size = dsp::frame_size(rate_f);
    let stft = Stft::new(&mono[..cap], rate_f, size, size / 4);
    let bin = stft.bin_hz();
    f.fps = stft.fps();
    let mel = Mel::new(MELS, size, rate_f, 20.0, 16_000.0);
    let mut bands = vec![0f32; MELS];
    let mut log_mel = Vec::with_capacity(stft.frames.len());
    let mut sum = vec![0f32; size / 2 + 1];
    let energies: Vec<f32> = stft.frames.iter().map(|p| p.iter().sum()).collect();
    let loudest = energies.iter().fold(0f32, |m, &e| m.max(e));
    let (mut flat_sum, mut flat_weight) = (0f32, 0f32);
    let mut cepstra = Vec::new();
    for (p, &e) in stft.frames.iter().zip(&energies) {
        for (s, v) in sum.iter_mut().zip(p) {
            *s += v;
        }
        mel.apply(p, &mut bands);
        let logs: Vec<f32> = bands.iter().map(|&b| dsp::db(b + 1e-12)).collect();
        if e > loudest * 1e-4 {
            let weight = (e / loudest).sqrt();
            cepstra.push((dsp::dct(&logs, CEPSTRA), weight));
            flat_sum += flatness(p) * e;
            flat_weight += e;
        }
        log_mel.push(logs);
    }
    let top_db = log_mel.iter().flatten().fold(-200f32, |m, &v| m.max(v));
    let floor = top_db - 80.0;
    for frame in &mut log_mel {
        frame.iter_mut().for_each(|v| *v = v.max(floor));
    }
    let total: f32 = sum.iter().sum();
    if total > 0.0 {
        let centroid = sum
            .iter()
            .enumerate()
            .map(|(k, p)| k as f32 * bin * p)
            .sum::<f32>()
            / total;
        let rolloff = |share: f32| {
            let mut acc = 0.0;
            for (k, p) in sum.iter().enumerate() {
                acc += p;
                if acc >= share * total {
                    return k as f32 * bin;
                }
            }
            rate_f / 2.0
        };
        let band = |lo: f32, hi: f32| {
            let part: f32 = sum
                .iter()
                .enumerate()
                .filter(|(k, _)| (lo..hi).contains(&(*k as f32 * bin)))
                .map(|(_, p)| p)
                .sum();
            dsp::db(part / total).max(-100.0)
        };
        let log_centroid = centroid.max(20.0).log2();
        f.spread = (sum
            .iter()
            .enumerate()
            .skip(1)
            .map(|(k, p)| p * ((k as f32 * bin).log2() - log_centroid).powi(2))
            .sum::<f32>()
            / total)
            .sqrt();
        f.spectrum = Spectrum {
            centroid_hz: centroid,
            rolloff_hz: rolloff(0.85),
            flatness: (flat_sum / flat_weight.max(1e-20) / 0.5615).clamp(0.0, 1.0),
            bands_db: [
                band(0.0, 60.0),
                band(60.0, 250.0),
                band(250.0, 1000.0),
                band(1000.0, 4000.0),
                band(4000.0, 8000.0),
                band(8000.0, rate_f),
            ],
        };
        f.bandwidth = rolloff(0.99);
    }
    // How the spectrum moves: its first 30 ms after the peak against the rest.
    let peak_frame = (peak_at * block / stft.hop).min(stft.frames.len() - 1);
    let attack_frames = ((0.03 * f.fps).ceil() as usize).max(1);
    let centroid_of = |range: std::ops::Range<usize>| {
        let (mut num, mut den) = (0.0, 0.0);
        for p in &stft.frames[range.start.min(stft.frames.len())..range.end.min(stft.frames.len())]
        {
            for (k, v) in p.iter().enumerate() {
                num += k as f32 * bin * v;
                den += v;
            }
        }
        if den > 0.0 { num / den } else { 0.0 }
    };
    f.centroid_attack = centroid_of(peak_frame..peak_frame + attack_frames);
    f.centroid_body = centroid_of(peak_frame + attack_frames..stft.frames.len());
    if f.centroid_body <= 0.0 {
        f.centroid_body = f.centroid_attack;
    }
    // Cepstra, weighted by loudness.
    let weight: f32 = cepstra.iter().map(|c| c.1).sum();
    if weight > 0.0 {
        for i in 0..CEPSTRA {
            let mean = cepstra.iter().map(|c| c.0[i] * c.1).sum::<f32>() / weight;
            let var = cepstra
                .iter()
                .map(|c| (c.0[i] - mean).powi(2) * c.1)
                .sum::<f32>()
                / weight;
            f.mfcc_mean[i] = mean;
            f.mfcc_std[i] = var.sqrt();
        }
    }
    let frames = log_mel.len();
    let average = |range: std::ops::Range<usize>| {
        let range =
            range.start.min(frames - 1)..range.end.clamp(range.start.min(frames - 1) + 1, frames);
        let n = range.len() as f32;
        (0..MELS)
            .map(|m| log_mel[range.clone()].iter().map(|f| f[m]).sum::<f32>() / n)
            .collect::<Vec<f32>>()
    };
    let attack_frames = ((0.035 * f.fps).ceil() as usize).max(1);
    let attack = dsp::dct(&average(peak_frame..peak_frame + attack_frames), 5);
    for i in 0..4 {
        f.attack_mfcc[i] = attack[i + 1] - f.mfcc_mean[i + 1];
    }
    // The low end's centroid early and later: a kick's drop.
    let low_centroid = |range: std::ops::Range<usize>| {
        let (mut num, mut den) = (0.0, 0.0);
        for p in &stft.frames[range.start.min(frames)..range.end.min(frames)] {
            for (k, v) in p
                .iter()
                .enumerate()
                .take((400.0 / bin) as usize + 1)
                .skip(1)
            {
                num += (k as f32 * bin).log2() * v;
                den += v;
            }
        }
        (den > 0.0).then(|| num / den)
    };
    let step = |ms: f32| (ms / 1000.0 * f.fps).round() as usize;
    if let (Some(early), Some(late)) = (
        low_centroid(peak_frame..peak_frame + step(25.0).max(1)),
        low_centroid(peak_frame + step(40.0)..peak_frame + step(90.0).max(step(40.0) + 1)),
    ) {
        f.low_drop = early - late;
    }
    f.bursts = bursts(&fine, start_block, f.peak, ms);
    // Each band's own envelope, frame by frame.
    let edges = [0.0, 300.0, 3000.0, rate_f];
    let onset_frame = (start_block * block / stft.hop).min(frames - 1);
    for b in 0..3 {
        let (lo, hi) = (
            (edges[b] / bin) as usize,
            ((edges[b + 1] / bin) as usize).min(size / 2),
        );
        let level: Vec<f32> = stft
            .frames
            .iter()
            .map(|p| p[lo..=hi.max(lo)].iter().sum())
            .collect();
        let top = (onset_frame..frames)
            .max_by(|&a, &b| level[a].total_cmp(&level[b]))
            .unwrap_or(onset_frame);
        let floor = level[top] * 0.01;
        let last = (top..frames)
            .rev()
            .find(|&t| level[t] >= floor)
            .unwrap_or(top);
        let per_frame = 1000.0 / f.fps;
        f.band_decay[b] = (1.0 + (last - top) as f32 * per_frame).ln();
        f.band_peak[b] = (1.0 + (top - onset_frame) as f32 * per_frame).ln();
    }
    // Spectral contrast: the top and bottom fifths of each band's bins in the summed spectrum.
    let contrast_edges = [20.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 16_000.0];
    for b in 0..6 {
        let (lo, hi) = (
            ((contrast_edges[b] / bin) as usize).max(1),
            ((contrast_edges[b + 1].min(rate_f / 2.0) / bin) as usize).min(size / 2),
        );
        if hi <= lo + 2 {
            continue;
        }
        let mut band: Vec<f32> = sum[lo..hi]
            .iter()
            .map(|&v| dsp::db(v + total * 1e-12))
            .collect();
        band.sort_by(|a, b| a.total_cmp(b));
        let k = (band.len() / 5).max(1);
        let valley = band[..k].iter().sum::<f32>() / k as f32;
        let peak = band[band.len() - k..].iter().sum::<f32>() / k as f32;
        f.contrast[b] = peak - valley;
    }

    // Rhythm.
    f.strength = rhythm::onset_strength(&log_mel, floor);
    f.onsets = rhythm::onsets(&f.strength, f.fps)
        .into_iter()
        .map(|t| t as f32 / f.fps)
        .collect();
    let named = named_bpm.map(|(bpm, _)| bpm);
    f.tempo = rhythm::tempo(&f.strength, f.fps, cap as f32 / rate_f, named);
    if let (Some(t), Some((bpm, false))) = (&mut f.tempo, named_bpm) {
        // A name that says "BPM" is believed whatever the audio says.
        t.bpm = bpm;
        t.beats = rhythm::whole_beats(f.seconds, bpm);
    }
    if f.onsets.len() >= 2 {
        f.hits = Some(hit_envelope(&fine, &f.onsets, ms));
    }

    // Pitch, over the sustained part: from just after the peak until it's 35 dB down.
    let from = peak_at + (30.0 / ms) as usize;
    let quiet = f.peak * 10f32.powf(-35.0 / 20.0);
    let until = (from..smooth.len())
        .find(|&i| smooth[i] < quiet)
        .unwrap_or(smooth.len())
        .min(from + (3000.0 / ms) as usize);
    let (from, until) = if until > from + (60.0 / ms) as usize {
        (from, until)
    } else {
        // Too short to skip the attack: take it all.
        (start_block, until.max(start_block + 1))
    };
    let track = pitch::track(&mono, rate_f, from * block, until * block, 48);
    f.pitch = pitch::summarize(&track);
    if let Some(p) = &f.pitch {
        f.harmonics = harmonics(&sum, bin, p.hz);
    }

    // Notes.
    let tonal_from = start_block * block;
    let tonal_to = ((sounding_end + 1) * block).min(cap).max(tonal_from + 1);
    f.tonal = tonal::analyze(&mono, rate_f, tonal_from, tonal_to, 64);
    f
}

/// How many clear bursts start a sound (in its first 80 ms): peaks over a third of the sound's
/// peak, each with a dip to under 70% before the next.
fn bursts(fine: &[f32], start: usize, peak: f32, ms: f32) -> f32 {
    let end = (start + (80.0 / ms) as usize).min(fine.len());
    let mut count = 0;
    let mut high: Option<f32> = None;
    let mut low = f32::MAX;
    for &v in &fine[start.min(end)..end] {
        match high {
            Some(h) if v > h => high = Some(v),
            Some(h) => {
                low = low.min(v);
                if low < 0.7 * h && v > low / 0.7 && v > peak / 3.0 {
                    count += 1;
                    high = Some(v);
                    low = f32::MAX;
                }
            }
            None if v > peak / 3.0 => {
                count += 1;
                high = Some(v);
            }
            None => {}
        }
    }
    count as f32
}

/// How many of the first 12 harmonics of `hz` show within 30 dB of the fundamental in a power
/// spectrum.
fn harmonics(power: &[f32], bin: f32, hz: f32) -> f32 {
    let at = |f: f32| {
        let (lo, hi) = (
            ((f * 0.97 / bin) as usize).max(1),
            ((f * 1.03 / bin).ceil() as usize).min(power.len() - 1),
        );
        power[lo..=hi.max(lo)].iter().fold(0f32, |m, &v| m.max(v))
    };
    let fundamental = at(hz);
    if fundamental <= 0.0 {
        return 0.0;
    }
    (2..=12)
        .filter(|&h| {
            hz * h as f32 * 1.03 < bin * power.len() as f32
                && at(hz * h as f32) > fundamental * 1e-3
        })
        .count() as f32
}

/// Flatness of a frame over the band its energy is in (at least 32 bins): 0 for a sine, about
/// 0.56 for white noise.
fn flatness(p: &[f32]) -> f32 {
    let total: f32 = p.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let (mut acc, mut lo, mut hi) = (0.0, 0, p.len() - 1);
    for (k, v) in p.iter().enumerate() {
        acc += v;
        if acc < 0.01 * total {
            lo = k;
        }
        if acc >= 0.99 * total {
            hi = k;
            break;
        }
    }
    if hi < lo + 32 {
        hi = (lo + 32).min(p.len() - 1);
        lo = hi.saturating_sub(32);
    }
    let band = &p[lo..=hi];
    let floor = total * 1e-10;
    let mean = band.iter().sum::<f32>() / band.len() as f32;
    let log_mean = band.iter().map(|v| (v + floor).ln()).sum::<f32>() / band.len() as f32;
    (log_mean.exp() / mean.max(1e-30)).min(1.0)
}

/// How wide: the sides' share of the energy, doubled (0 mono, 1 for unrelated channels).
fn width(channels: &[Vec<f32>]) -> f32 {
    if channels.len() < 2 {
        return 0.0;
    }
    let (mut mid, mut side) = (0f64, 0f64);
    for (&l, &r) in channels[0].iter().zip(&channels[1]) {
        mid += ((l + r) as f64 / 2.0).powi(2);
        side += ((l - r) as f64 / 2.0).powi(2);
    }
    if mid + side <= 0.0 {
        0.0
    } else {
        (2.0 * side / (mid + side)).min(1.0) as f32
    }
}

/// The share of samples in runs of three or more at the peak, when the peak is at full scale.
fn clipped(channels: &[Vec<f32>], peak: f32) -> f32 {
    if peak < 0.97 {
        return 0.0;
    }
    let level = peak * 0.999;
    let mut count = 0usize;
    let mut total = 0usize;
    for c in channels {
        let mut run = 0;
        for &s in c {
            if s.abs() >= level {
                run += 1;
            } else {
                if run >= 3 {
                    count += run;
                }
                run = 0;
            }
        }
        if run >= 3 {
            count += run;
        }
        total += c.len();
    }
    count as f32 / total.max(1) as f32
}

/// The envelope of the sound from `peak_at` (a millisecond-ish point of `fine`), looking no
/// earlier than `lo` or later than `hi`. With `valley`, its start is no earlier than the low
/// point before its peak (for a hit in a loop, where the hit before may never die away).
fn envelope(
    fine: &[f32],
    smooth: &[f32],
    peak_at: usize,
    (lo, hi): (usize, usize),
    valley: bool,
    ms: f32,
) -> Envelope {
    let peak = fine[peak_at];
    if peak <= 0.0 {
        return Envelope::default();
    }
    let under = |db: f32| peak * 10f32.powf(db / 20.0);
    // Back to where it starts (30 dB under), then forward to where it first comes within 1 dB.
    let nearby = |i: usize| {
        fine[i.saturating_sub(2)..(i + 3).min(fine.len())]
            .iter()
            .fold(0f32, |m, &v| m.max(v))
    };
    let mut start = peak_at;
    while start > lo
        && nearby(start - 1) > under(-30.0)
        && (!valley || nearby(start - 1) <= nearby(start))
    {
        start -= 1;
    }
    let top = (start..=peak_at)
        .find(|&i| fine[i] >= under(-1.0))
        .unwrap_or(peak_at);
    // The smooth envelope reaches 20 points (ms) past the sound; take that back off.
    let last_above = |db: f32| {
        (peak_at..hi)
            .rev()
            .find(|&i| smooth[i] >= under(db))
            .map_or(peak_at, |i| i.saturating_sub(20).max(peak_at))
    };
    let decay = last_above(-20.0);
    let tail = last_above(-60.0);
    let (a, b) = (
        peak_at + (tail - peak_at) / 4,
        (peak_at + 3 * (tail - peak_at) / 4).max(peak_at + (tail - peak_at) / 4 + 1),
    );
    let middle = &smooth[a.min(smooth.len() - 1)..b.min(smooth.len())];
    let sustain = if middle.is_empty() {
        0.0
    } else {
        middle.iter().sum::<f32>() / middle.len() as f32 / peak
    };
    Envelope {
        attack_ms: (top - start) as f32 * ms,
        decay_ms: (decay - peak_at) as f32 * ms,
        tail_ms: (tail - peak_at) as f32 * ms,
        sustain: sustain.clamp(0.0, 1.0),
    }
}

/// A typical hit's envelope: each hit measured up to the next, the medians taken.
fn hit_envelope(fine: &[f32], onsets: &[f32], ms: f32) -> Envelope {
    let smooth = dsp::running_max(fine, 5);
    let (mut attacks, mut decays, mut sustains) = (Vec::new(), Vec::new(), Vec::new());
    let point = |t: f32| ((t * 1000.0 / ms) as usize).min(fine.len() - 1);
    let mut tail = 0.0;
    for (i, &t) in onsets.iter().enumerate() {
        // Onset frames lag the hit a little: start 15 ms early.
        let lo = point((t - 0.015).max(0.0));
        let hi = onsets
            .get(i + 1)
            .map_or(fine.len(), |&n| point((n - 0.015).max(0.0)))
            .max(lo + 1);
        let peak_at = (lo..hi.min(lo + (150.0 / ms) as usize).max(lo + 1))
            .max_by(|&a, &b| fine[a].total_cmp(&fine[b]))
            .unwrap_or(lo);
        let e = envelope(fine, &smooth, peak_at, (lo, hi), true, ms);
        attacks.push(e.attack_ms);
        decays.push(e.decay_ms);
        sustains.push(e.sustain);
        if i + 1 == onsets.len() {
            tail = e.tail_ms;
        }
    }
    Envelope {
        attack_ms: dsp::median(&mut attacks),
        decay_ms: dsp::median(&mut decays),
        tail_ms: tail,
        sustain: dsp::median(&mut sustains),
    }
}

#[cfg(test)]
mod tests;
