//! A compressor set by listening: what a channel plays is measured (how loud its quiet, usual and
//! loud moments are, how sharp its hits stand over its body) and the compressor is set for it:
//! its style, a threshold that takes the loud moments down by as much as the material wants, a
//! ratio from how far its level ranges, an attack that keeps hits' fronts when there are hits,
//! a release in time with the song, and the makeup that brings it back to as loud as it went in.

use super::*;

/// What's being compressed, as far as it changes how.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    /// Drums and percussion: sharp hits.
    Hits,
    /// A bass.
    Bass,
    /// A voice.
    Voice,
    /// Pads, chords and other held sounds.
    Held,
    /// A bus or the whole mix: glue.
    Bus,
    /// Anything else: the measurements decide.
    Other,
}

impl Material {
    /// What a track's part (as `music::arrange::role` names it) is to a compressor.
    pub fn of_part(part: &str) -> Material {
        match part {
            "drums" | "perc" => Material::Hits,
            "bass" => Material::Bass,
            "vocals" => Material::Voice,
            "pad" | "chords" | "texture" => Material::Held,
            _ => Material::Other,
        }
    }
}

/// A channel's level, window by window, the moments it plays: its RMS and peak (dBFS), and how
/// far its RMS rose over the 50 ms before (dB): a hit's front jumps, a swell creeps.
pub(crate) struct Levels {
    pub(crate) rms: Vec<f32>,
    pub(crate) peak: Vec<f32>,
    pub(crate) rise: Vec<f32>,
}

/// 30 ms windows every 10 ms of the louder side; the silent ones (under -60 dBFS, or 40 dB
/// under its loudest) left out.
pub(crate) fn levels(render: &[Vec<f32>; 2], rate: f32) -> Levels {
    let (win, hop) = ((0.03 * rate) as usize, (0.01 * rate) as usize);
    let n = render[0].len().min(render[1].len());
    let (mut rms, mut peak) = (Vec::new(), Vec::new());
    let mut at = 0;
    while at + win <= n {
        let (mut sum, mut top) = (0f32, 0f32);
        for (a, b) in render[0][at..at + win].iter().zip(&render[1][at..at + win]) {
            let x = a.abs().max(b.abs());
            sum += x * x;
            top = top.max(x);
        }
        rms.push(10.0 * (sum / win as f32 + 1e-12).log10());
        peak.push(20.0 * (top + 1e-9).log10());
        at += hop;
    }
    let loudest = rms.iter().copied().fold(f32::MIN, f32::max);
    let keep: Vec<bool> = rms
        .iter()
        .map(|&r| r > -60.0 && r > loudest - 40.0)
        .collect();
    let rise: Vec<f32> = (0..rms.len())
        .map(|i| rms[i] - rms[i.saturating_sub(5)].max(loudest - 40.0))
        .collect();
    let pick = |v: Vec<f32>| {
        v.into_iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(x, _)| x)
            .collect()
    };
    Levels {
        rms: pick(rms),
        peak: pick(peak),
        rise: pick(rise),
    }
}

/// The value `share` of the way up `v` (sorted).
pub(crate) fn percentile(v: &[f32], share: f32) -> f32 {
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s.get(((s.len() as f32 - 1.0) * share).round() as usize)
        .copied()
        .unwrap_or(-60.0)
}

/// How loud a render is overall: its RMS over the moments it plays, dB.
fn loudness(render: &[Vec<f32>; 2], rate: f32) -> f32 {
    let l = levels(render, rate);
    let mean = l.rms.iter().map(|&d| 10f32.powf(d / 10.0)).sum::<f32>() / l.rms.len().max(1) as f32;
    10.0 * (mean + 1e-12).log10()
}

/// A compressor set for `render` (a channel alone, at `rate`), `material` saying what it is,
/// the song at `bpm`. None when there's nothing to hear.
pub fn learn(render: &[Vec<f32>; 2], rate: f64, bpm: f64, material: Material) -> Option<Comp> {
    let rate = rate as f32;
    let l = levels(render, rate);
    if l.rms.len() < 10 {
        return None;
    }
    // How sharply it comes in: how far its sharpest fronts jump in 50 ms.
    let sharp = percentile(&l.rise, 0.98);
    let range = percentile(&l.rms, 0.9) - percentile(&l.rms, 0.2);
    let style = match material {
        Material::Hits => CompStyle::Punchy,
        Material::Voice | Material::Held => CompStyle::Smooth,
        Material::Bus => CompStyle::Bus,
        Material::Bass => CompStyle::Clean,
        Material::Other if sharp > 10.0 => CompStyle::Punchy,
        Material::Other if sharp < 3.0 => CompStyle::Smooth,
        Material::Other => CompStyle::Clean,
    };
    // Drums that swing far between soft and loud hits: squashed hard and mixed in under the dry
    // hits (parallel compression), so the fronts stay and the quiet hits come up.
    let parallel = matches!(material, Material::Hits) && range > 14.0;
    // How far it takes the loud moments down.
    let wanted = match material {
        _ if parallel => 10.0,
        Material::Hits => 5.0,
        Material::Voice => 5.0,
        Material::Bass => 4.0,
        Material::Held => 3.0,
        Material::Bus => 2.0,
        Material::Other => 4.0,
    };
    let ratio = match style {
        _ if parallel => 6.0,
        CompStyle::Bus if range > 15.0 => 4.0,
        CompStyle::Bus => 2.0,
        _ => (1.5 + range / 6.0).clamp(2.0, 6.0),
    };
    // What it hears at those moments: the RMS for an RMS ear, the peak for the rest.
    let heard = match style {
        CompStyle::Smooth => percentile(&l.rms, 0.9),
        _ => percentile(&l.peak, 0.9),
    };
    let threshold = (heard - wanted / (1.0 - 1.0 / ratio)).clamp(-60.0, 0.0);
    let beat = 60_000.0 / bpm.max(20.0) as f32;
    let (attack, release, auto) = match style {
        // Hits keep their fronts; the reduction lets go in an eighth, in time with the groove.
        CompStyle::Punchy => ((5.0 + sharp * 0.75).clamp(10.0, 30.0), beat / 2.0, false),
        CompStyle::Smooth => (5.0, beat / 2.0, true),
        // The classic glue: a slow attack, an automatic release.
        CompStyle::Bus => (30.0, 600.0, true),
        CompStyle::Clean => (10.0, beat, false),
    };
    // Where many parts play at once, the one that pushes it over takes the reduction; one
    // sound alone (a bass) has nothing to keep from pumping.
    let spectral = match material {
        Material::Bus => 0.4,
        Material::Hits | Material::Held | Material::Voice => 0.3,
        Material::Bass => 0.0,
        Material::Other => 0.2,
    };
    let mut comp = Comp {
        threshold_db: threshold.round(),
        ratio: (ratio * 10.0).round() / 10.0,
        attack_ms: attack.round(),
        release_ms: release.clamp(40.0, 1000.0).round(),
        style,
        auto_release: auto,
        sidechain_hz: if style == CompStyle::Bus { 90.0 } else { 0.0 },
        spectral,
        mix: if parallel { 0.5 } else { 1.0 },
        ..Comp::default()
    };
    // As loud as it went in: the makeup the compressed render needs to measure the same.
    let mut out = render.clone();
    let mut gr = vec![0f32; out[0].len()];
    let mut c = Compressor::new(rate as f64);
    let [a, b] = &mut out;
    c.process(&comp, a, b, &mut gr);
    comp.makeup_db = ((loudness(render, rate) - loudness(&out, rate)) * 2.0).round() / 2.0;
    comp.makeup_db = comp.makeup_db.clamp(0.0, 18.0);
    Some(comp)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    /// Four seconds of `hit`s: decaying noise bursts on each beat at 120, some soft, some loud.
    fn drums() -> [Vec<f32>; 2] {
        let n = 4 * 48_000;
        let mut seed = 3u32;
        let mut v = vec![0f32; n];
        for beat in 0..8 {
            let at = beat * 24_000;
            let level = if beat % 2 == 0 { 0.9 } else { 0.35 };
            for j in 0..12_000 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 9) as f32 / (1 << 23) as f32 * 2.0 - 1.0;
                v[at + j] += noise * level * (-(j as f32) / 2_000.0).exp();
            }
        }
        [v.clone(), v]
    }

    /// Four seconds of a pad: a chord swelling and falling.
    fn pad() -> [Vec<f32>; 2] {
        let v: Vec<f32> = (0..4 * 48_000)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let swell = 0.15 + 0.5 * (t * 0.8).sin().abs();
                [220.0, 277.0, 330.0]
                    .iter()
                    .map(|f| (std::f32::consts::TAU * f * t).sin())
                    .sum::<f32>()
                    * swell
                    / 3.0
            })
            .collect();
        [v.clone(), v]
    }

    #[test]
    fn hits_get_a_punchy_compressor_that_keeps_their_fronts() {
        let c = learn(&drums(), RATE, 120.0, Material::Other).unwrap();
        assert_eq!(c.style, CompStyle::Punchy, "{c:?}");
        assert!(c.attack_ms >= 10.0, "{c:?}");
        // An eighth at 120.
        assert_eq!(c.release_ms, 250.0);
    }

    #[test]
    fn drums_far_between_soft_and_loud_are_compressed_in_parallel() {
        let c = learn(&drums(), RATE, 120.0, Material::Hits).unwrap();
        let range = {
            let l = levels(&drums(), RATE as f32);
            percentile(&l.rms, 0.9) - percentile(&l.rms, 0.2)
        };
        assert!(range > 14.0, "{range}");
        assert_eq!(
            (c.style, c.ratio, c.mix),
            (CompStyle::Punchy, 6.0, 0.5),
            "{c:?}"
        );
        assert!(c.makeup_db > 0.0, "{c:?}");
    }

    #[test]
    fn held_sounds_get_a_smooth_one_and_come_out_as_loud_as_they_went_in() {
        let input = pad();
        let c = learn(&input, RATE, 100.0, Material::Other).unwrap();
        assert_eq!(c.style, CompStyle::Smooth, "{c:?}");
        let mut out = input.clone();
        let mut gr = vec![0f32; out[0].len()];
        let [a, b] = &mut out;
        Compressor::new(RATE).process(&c, a, b, &mut gr);
        // It works (the swells come down) and it's no quieter for it.
        assert!(gr.iter().fold(0f32, |m, &g| m.max(g)) > 1.5, "{c:?}");
        let (was, now) = (loudness(&input, 48_000.0), loudness(&out, 48_000.0));
        assert!((was - now).abs() < 1.0, "{was} {now} {c:?}");
    }

    #[test]
    fn a_bus_gets_glue_and_silence_gets_nothing() {
        let c = learn(&drums(), RATE, 120.0, Material::Bus).unwrap();
        assert_eq!(
            (c.style, c.auto_release, c.sidechain_hz),
            (CompStyle::Bus, true, 90.0)
        );
        assert!(
            learn(
                &[vec![0.0; 48_000], vec![0.0; 48_000]],
                RATE,
                120.0,
                Material::Other
            )
            .is_none()
        );
    }
}
