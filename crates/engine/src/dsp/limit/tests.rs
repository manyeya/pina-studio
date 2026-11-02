use super::*;
use std::f64::consts::PI;

const RATE: f64 = 48_000.0;

/// Something hard to limit: a bass line, a hi-hat's noise and a kick every quarter second.
pub(super) fn music(seconds: f64) -> Vec<f32> {
    let n = (seconds * RATE) as usize;
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    (0..n)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let noise = (seed >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0;
            let t = i as f64 / RATE;
            let beat = t % 0.25;
            let kick = (2.0 * PI * 55.0 * beat).sin() * (-beat * 30.0).exp();
            let bass = 0.3 * (2.0 * PI * 82.4 * t).sin();
            (kick * 0.8 + bass) as f32 + noise * 0.1
        })
        .collect()
}

fn run(lim: &Limiter, input: &[f32], live: bool) -> (Vec<f32>, Vec<f32>, Limiting) {
    let mut limiting = Limiting::new(RATE);
    let (mut l, mut r) = (
        input.to_vec(),
        input.iter().map(|x| x * 0.8).collect::<Vec<_>>(),
    );
    for at in (0..l.len()).step_by(512) {
        let end = (at + 512).min(l.len());
        limiting.process(lim, &mut l[at..end], &mut r[at..end], live);
    }
    (l, r, limiting)
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0, |m, x| m.max(x.abs()))
}

#[test]
fn nothing_goes_over_the_ceiling_in_any_style() {
    let input = music(3.0);
    for style in LimitStyle::ALL {
        for oversample in [1, 2, 4, 8] {
            let lim = Limiter {
                style,
                gain_db: 18.0,
                ceiling_db: -1.0,
                oversample,
                clip: 0.3,
                ..Default::default()
            };
            let (l, r, limiting) = run(&lim, &input, true);
            let ceiling = 10f32.powf(-1.0 / 20.0);
            assert!(peak(&l) <= ceiling && peak(&r) <= ceiling, "{style:?}");
            // The hard stop is only for rounding: the limiter itself never lets a peak by.
            // The hard stop is only for rounding: the limiter itself never lets a peak by.
            assert_eq!(
                limiting.caught,
                0,
                "{style:?} {oversample}x: {:.2} dB over",
                20.0 * limiting.worst.log10()
            );
            // And it's loud: its true peaks right up at the ceiling (a clipped edge's sample
            // peaks can sit lower, the points between them at the top).
            let tp = limiting.meter.true_peak;
            assert!(tp > ceiling * 0.97, "{style:?} {oversample}x {tp}");
        }
    }
}

#[test]
fn true_peaks_stay_under_the_ceiling_too() {
    let input = music(3.0);
    for style in LimitStyle::ALL {
        for oversample in [1, 2, 4, 8] {
            let lim = Limiter {
                style,
                gain_db: 20.0,
                ceiling_db: -1.0,
                oversample,
                ..Default::default()
            };
            let (_, _, limiting) = run(&lim, &input, true);
            let tp = 20.0 * limiting.meter.true_peak.log10();
            assert!(tp <= -0.99, "{style:?} {oversample}x: {tp} dBTP");
        }
    }
}

#[test]
fn quiet_sound_comes_out_untouched_just_late() {
    let input: Vec<f32> = music(0.5).iter().map(|x| x * 0.2).collect();
    for style in [
        LimitStyle::Transparent,
        LimitStyle::Safe,
        LimitStyle::Classic,
    ] {
        let lim = Limiter {
            style,
            ..Default::default()
        };
        let late = latency(&lim, RATE) as usize;
        assert_eq!(late, 96 + TP_DELAY);
        let (l, _, _) = run(&lim, &input, false);
        assert_eq!(&l[late..], &input[..input.len() - late], "{style:?}");
    }
}

#[test]
fn bypassed_it_still_holds_the_sound_back() {
    let input = music(0.3);
    let lim = Limiter {
        gain_db: 12.0,
        bypass: true,
        ..Default::default()
    };
    let late = latency(&lim, RATE) as usize;
    let (l, _, _) = run(&lim, &input, false);
    assert_eq!(&l[late..], &input[..input.len() - late]);
}

#[test]
fn delta_is_what_it_takes_away() {
    let input = music(1.0);
    let lim = Limiter {
        gain_db: 12.0,
        style: LimitStyle::Allround,
        ..Default::default()
    };
    let (out, _, _) = run(&lim, &input, true);
    let (delta, _, _) = run(&Limiter { delta: true, ..lim }, &input, true);
    let late = latency(&lim, RATE) as usize;
    let gain = 10f32.powf(12.0 / 20.0);
    // Clipping adds a little that isn't in the sound; the rest adds back exactly.
    for i in late..input.len() {
        let back = out[i] + delta[i];
        assert!((back - input[i - late] * gain).abs() < 0.05, "{i}");
    }
    assert!(peak(&delta) > 0.1);
    // And in a bounce, no listening aids.
    let (bounced, _, _) = run(&Limiter { delta: true, ..lim }, &input, false);
    assert_eq!(bounced, out);
}

#[test]
fn unity_hears_it_at_the_level_it_came_in() {
    let input: Vec<f32> = music(0.5).iter().map(|x| x * 0.1).collect();
    let lim = Limiter {
        gain_db: 6.0,
        unity: true,
        ..Default::default()
    };
    let late = latency(&lim, RATE) as usize;
    let (l, _, _) = run(&lim, &input, true);
    for i in late..input.len() {
        assert!((l[i] - input[i - late]).abs() < 1e-5);
    }
}

#[test]
fn its_meter_reads_what_the_analysis_does() {
    // Loud then quiet then loud, so the gate matters.
    let mut input = music(4.0);
    for (i, x) in input.iter_mut().enumerate() {
        if (1.5..2.5).contains(&(i as f64 / RATE)) {
            *x *= 0.02;
        }
    }
    let lim = Limiter {
        gain_db: 6.0,
        ..Default::default()
    };
    let (l, r, limiting) = run(&lim, &input, true);
    let late = latency(&lim, RATE) as usize;
    let want = crate::music::analysis::loudness(&l[late..], &r[late..], RATE);
    let got = limiting.meter.integrated;
    assert!((got - want).abs() < 0.2, "meter {got}, analysis {want}");
    assert!(limiting.meter.short.is_finite() && limiting.meter.momentary.is_finite());
}

#[test]
fn a_whole_block_of_it_stays_cheap() {
    // A sanity check that nothing in it allocates or blows up on an empty or tiny block.
    let mut limiting = Limiting::new(RATE);
    let lim = Limiter::default();
    limiting.process(&lim, &mut [], &mut [], true);
    let (mut l, mut r) = ([2.0f32], [2.0f32]);
    limiting.process(&lim, &mut l, &mut r, true);
    assert!(l[0].abs() <= 1.0);
}

#[test]
fn oversampled_its_quiet_sound_still_comes_out_just_late() {
    // A tone under the ceiling, through each oversampling: as it went in, only as late as it
    // says (the trips are linear in phase, so only the 8x's half sample of rounding is left).
    let tone: Vec<f32> = (0..9_600)
        .map(|i| (2.0 * PI * 1_000.0 * i as f64 / RATE).sin() as f32 * 0.3)
        .collect();
    for oversample in [2, 4, 8] {
        let lim = Limiter {
            oversample,
            ..Default::default()
        };
        let late = latency(&lim, RATE) as usize;
        let (l, _, _) = run(&lim, &tone, false);
        let worst = (late + 2_000..tone.len())
            .map(|i| (l[i] - tone[i - late]).abs())
            .fold(0f32, f32::max);
        assert!(worst < 0.02, "{oversample}x: {worst}");
    }
}

#[test]
fn oversampling_keeps_what_the_gain_makes_from_folding_back() {
    // A high tone pumped by a low one: what the gain makes of the high tone lands at 18,030 Hz
    // plus and minus the hundreds; what folds back over the top of the band lands between,
    // at 70 past the hundreds. Oversampled, there's far less of the second.
    let n = (2.0 * RATE) as usize;
    let input: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / RATE;
            (0.5 * (2.0 * PI * 100.0 * t).sin() + 0.3 * (2.0 * PI * 18_030.0 * t).sin()) as f32
        })
        .collect();
    let folded = |oversample: u8| {
        let lim = Limiter {
            style: LimitStyle::Aggressive,
            gain_db: 12.0,
            release_ms: 5.0,
            auto_release: false,
            clip: 0.0,
            oversample,
            ..Default::default()
        };
        let (l, _, _) = run(&lim, &input, false);
        // A second's worth, a whole number of every frequency in it: each lands on its bin.
        let s = &l[n / 2..];
        let power = |f: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, x) in s.iter().enumerate() {
                let a = 2.0 * PI * f * i as f64 / RATE;
                re += *x as f64 * a.cos();
                im += *x as f64 * a.sin();
            }
            re * re + im * im
        };
        let folded: f64 = (0..230).map(|k| power(70.0 + 100.0 * k as f64)).sum();
        let made: f64 = (0..230).map(|k| power(30.0 + 100.0 * k as f64)).sum();
        folded / made
    };
    let (plain, over) = (folded(1), folded(4));
    eprintln!(
        "folded back against made: 1x {:.1} dB, 4x {:.1} dB",
        10.0 * plain.log10(),
        10.0 * over.log10()
    );
    assert!(over < plain * 0.1, "1x {plain:.2e}, 4x {over:.2e}");
}
