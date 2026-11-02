//! The tools measured: each does what it says, and nothing when it's set to nothing.

use super::*;
use crate::song::project::{
    DeEsser, FilterFx, FilterMode, Gate, Motion, MotionStyle, Shaper, Stereo, Unit,
};

const RATE: f64 = 48_000.0;
const CLOCK: Clock = Clock {
    bpm: 120.0,
    beats: 0.0,
};

fn sine(hz: f32, level: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| level * (TAU * hz * i as f32 / RATE as f32).sin())
        .collect()
}

fn run(t: &mut dyn Running, u: Unit, l: &[f32], r: &[f32]) -> [Vec<f32>; 2] {
    let (mut a, mut b) = (l.to_vec(), r.to_vec());
    for (x, y) in a.chunks_mut(256).zip(b.chunks_mut(256)) {
        t.run(&u, CLOCK, x, y);
    }
    [a, b]
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

#[test]
fn the_filter_cuts_and_its_lfo_and_level_move_it() {
    let high = sine(4000.0, 0.5, 24_000);
    let f = FilterFx {
        mode: FilterMode::LowPass,
        cutoff_hz: 500.0,
        resonance: 0.1,
        ..FilterFx::default()
    };
    let [out, _] = run(&mut Filtering::new(RATE), Unit::Filter(f), &high, &high);
    assert!(rms(&out[4800..]) < rms(&high) * 0.05, "{}", rms(&out));
    // Swept up by the LFO, it opens part of the time.
    let swept = FilterFx {
        lfo_depth: 4.0,
        lfo_beats: 0.0,
        lfo_hz: 4.0,
        ..f
    };
    let [out, _] = run(&mut Filtering::new(RATE), Unit::Filter(swept), &high, &high);
    assert!(rms(&out) > rms(&high) * 0.2, "{}", rms(&out));
    // Opened by the sound's own level.
    let wah = FilterFx {
        env_depth: 4.0,
        ..f
    };
    let [out, _] = run(&mut Filtering::new(RATE), Unit::Filter(wah), &high, &high);
    assert!(rms(&out[4800..]) > rms(&high) * 0.2, "{}", rms(&out));
    // All dry: the sound as it went in.
    let dry = FilterFx { mix: 0.0, ..f };
    let [out, _] = run(&mut Filtering::new(RATE), Unit::Filter(dry), &high, &high);
    assert_eq!(out, high);
}

#[test]
fn motion_moves_each_way() {
    let n = 96_000;
    let tone = sine(440.0, 0.5, n);
    // A tremolo: its level rises and falls; opposite sides at spread 1.
    let t = Motion {
        spread: 1.0,
        depth: 1.0,
        rate_hz: 4.0,
        ..MotionStyle::Tremolo.motion()
    };
    let [l, r] = run(&mut Moving::new(RATE), Unit::Motion(t), &tone, &tone);
    let at = |x: &[f32], ms: usize| rms(&x[ms * 48..(ms + 20) * 48]);
    // Four cycles a second: a quarter of the way round quiet, three quarters loud.
    let (quiet, loud) = (52, 177);
    assert!(
        at(&l, loud) > at(&l, quiet) * 5.0,
        "{} {}",
        at(&l, loud),
        at(&l, quiet)
    );
    assert!(
        at(&r, quiet) > at(&r, loud) * 5.0,
        "{} {}",
        at(&r, quiet),
        at(&r, loud)
    );
    // A chorus: its sides differ; a phaser's notch takes a band down and lets it back.
    let c = MotionStyle::Chorus.motion();
    let [l, r] = run(&mut Moving::new(RATE), Unit::Motion(c), &tone, &tone);
    let side: Vec<f32> = l.iter().zip(&r).map(|(a, b)| a - b).collect();
    assert!(rms(&side[4800..]) > rms(&l) * 0.1, "{}", rms(&side));
    assert!(l.iter().all(|s| s.is_finite()));
    let p = Motion {
        rate_hz: 2.0,
        depth: 1.0,
        mix: 0.5,
        ..MotionStyle::Phaser.motion()
    };
    let [l, _] = run(&mut Moving::new(RATE), Unit::Motion(p), &tone, &tone);
    let levels: Vec<f32> = (0..50).map(|k| at(&l, 100 + k * 20)).collect();
    let (lo, hi) = levels
        .iter()
        .fold((f32::MAX, 0f32), |(a, b), &x| (a.min(x), b.max(x)));
    assert!(lo < hi * 0.5, "{lo} {hi}");
    // Fed back hard, a flanger stays bounded.
    let f = Motion {
        feedback: 0.95,
        ..MotionStyle::Flanger.motion()
    };
    let [l, _] = run(&mut Moving::new(RATE), Unit::Motion(f), &tone, &tone);
    assert!(l.iter().all(|s| s.is_finite() && s.abs() < 20.0));
}

/// Hits: a loud front dying away, four a second.
fn hits() -> Vec<f32> {
    (0..48_000)
        .map(|i| {
            let k = (i % 12_000) as f32;
            0.5 * (k * 0.1).sin() * (-k / 4_000.0).exp()
        })
        .collect()
}

#[test]
fn the_shaper_moves_fronts_and_tails_and_nothing_if_asked_nothing() {
    let x = hits();
    let front = |v: &[f32]| rms(&v[12_000..12_300]);
    let tail = |v: &[f32]| rms(&v[20_000..23_000]);
    let shape = |attack, sustain| {
        let s = Shaper {
            attack,
            sustain,
            ..Shaper::default()
        };
        run(&mut Shaping::new(RATE), Unit::Shaper(s), &x, &x)[0].clone()
    };
    assert_eq!(shape(0.0, 0.0), x);
    assert!(front(&shape(1.0, 0.0)) > front(&x) * 1.4);
    assert!(front(&shape(-1.0, 0.0)) < front(&x) * 0.7);
    assert!(tail(&shape(0.0, 1.0)) > tail(&x) * 1.4);
    assert!(tail(&shape(0.0, -1.0)) < tail(&x) * 0.7);
}

#[test]
fn the_gate_shuts_under_its_threshold_and_opens_over_it() {
    // Loud for half a second, then quiet.
    let mut x = sine(200.0, 0.5, 24_000);
    x.extend(sine(200.0, 0.002, 24_000));
    let g = Gate {
        threshold_db: -30.0,
        range_db: 60.0,
        ratio: 20.0,
        hold_ms: 10.0,
        release_ms: 20.0,
        ..Gate::default()
    };
    let [out, _] = run(&mut Gating::new(RATE), Unit::Gate(g), &x, &x);
    assert!((rms(&out[4800..20_000]) / rms(&x[4800..20_000]) - 1.0).abs() < 0.02);
    assert!(
        rms(&out[36_000..]) < rms(&x[36_000..]) * 0.02,
        "{}",
        rms(&out[36_000..])
    );
    // Its ear can be kept off: a high tone can't open it through a low-pass.
    let high = sine(8000.0, 0.5, 24_000);
    let deaf = Gate {
        high_cut_hz: 200.0,
        ..g
    };
    let [out, _] = run(&mut Gating::new(RATE), Unit::Gate(deaf), &high, &high);
    assert!(rms(&out[12_000..]) < rms(&high) * 0.05, "{}", rms(&out));
}

#[test]
fn the_de_esser_takes_down_esses_and_leaves_the_rest() {
    let d = DeEsser {
        threshold_db: -30.0,
        range_db: 12.0,
        ..DeEsser::default()
    };
    let ess = sine(6500.0, 0.3, 24_000);
    let [out, _] = run(&mut DeEssing::new(RATE), Unit::DeEsser(d), &ess, &ess);
    assert!(rms(&out[4800..]) < rms(&ess) * 0.5, "{}", rms(&out));
    let low = sine(300.0, 0.3, 24_000);
    let [out, _] = run(&mut DeEssing::new(RATE), Unit::DeEsser(d), &low, &low);
    assert!(out == low, "the lows untouched");
}

#[test]
fn the_stereo_tool_narrows_widens_and_changes_nothing_as_it_comes() {
    let (l, r) = (sine(1000.0, 0.5, 9_600), sine(1500.0, 0.3, 9_600));
    let [a, b] = run(
        &mut Widening::new(RATE),
        Unit::Stereo(Stereo::default()),
        &l,
        &r,
    );
    let close = |x: &[f32], y: &[f32]| x.iter().zip(y).all(|(p, q)| (p - q).abs() < 1e-5);
    assert!(close(&a, &l) && close(&b, &r));
    let mono = Stereo {
        width: [0.0; 3],
        ..Stereo::default()
    };
    let [a, b] = run(&mut Widening::new(RATE), Unit::Stereo(mono), &l, &r);
    assert!(close(&a[4800..], &b[4800..]));
    // Lows mono under 200 Hz, the highs still apart.
    let bass = Stereo {
        mono_hz: 200.0,
        ..Stereo::default()
    };
    let (l, r) = (sine(60.0, 0.5, 24_000), sine(60.0, -0.5, 24_000));
    let [a, b] = run(&mut Widening::new(RATE), Unit::Stereo(bass), &l, &r);
    let side: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x - y).collect();
    assert!(rms(&side[12_000..]) < rms(&l) * 0.3, "{}", rms(&side));
    // Swapped; one side turned over; panned hard left.
    let [a, b] = run(
        &mut Widening::new(RATE),
        Unit::Stereo(Stereo {
            swap: true,
            ..Stereo::default()
        }),
        &sine(100.0, 0.5, 4_800),
        &vec![0.0; 4_800],
    );
    assert!(rms(&a) < 1e-4 && rms(&b) > 0.3);
    let [a, b] = run(
        &mut Widening::new(RATE),
        Unit::Stereo(Stereo {
            balance: -1.0,
            ..Stereo::default()
        }),
        &sine(100.0, 0.5, 4_800),
        &sine(100.0, 0.5, 4_800),
    );
    assert!(rms(&b) < 1e-4 && rms(&a) > 0.3);
}

#[test]
fn the_multiband_adds_back_flat_and_squeezes_only_the_band_that_is_over() {
    use crate::song::project::Multiband;
    let flat = Multiband {
        threshold_db: [0.0; 3],
        ratio: [1.0; 3],
        ..Multiband::default()
    };
    // Set to nothing, every frequency comes out as loud as it went in.
    for hz in [60.0, 160.0, 800.0, 2500.0, 9000.0] {
        let x = sine(hz, 0.25, 24_000);
        let [y, _] = run(&mut Multibanding::new(RATE), Unit::Multiband(flat), &x, &x);
        let gain = rms(&y[6000..]) / rms(&x);
        assert!((gain - 1.0).abs() < 0.015, "{hz} {gain}");
    }
    // The lows over their threshold come down; the highs, set the same but quiet, don't.
    let m = Multiband {
        threshold_db: [-30.0, 0.0, 0.0],
        ratio: [8.0, 1.0, 1.0],
        attack_ms: 1.0,
        ..Multiband::default()
    };
    let low = sine(60.0, 0.5, 48_000);
    let high = sine(8000.0, 0.5, 48_000);
    let mut t = Multibanding::new(RATE);
    let [y, _] = run(&mut t, Unit::Multiband(m), &low, &low);
    assert!(rms(&y[24_000..]) < rms(&low) * 0.2, "{}", rms(&y[24_000..]));
    assert!(
        t.reading()[0] > 10.0 && t.reading()[2] == 0.0,
        "{:?}",
        t.reading()
    );
    let [y, _] = run(
        &mut Multibanding::new(RATE),
        Unit::Multiband(m),
        &high,
        &high,
    );
    assert!((rms(&y[24_000..]) / rms(&high) - 1.0).abs() < 0.02);
    // Lifted, a quiet band comes up; silence stays silent.
    let lifted = Multiband {
        lift: [1.0; 3],
        ..flat
    };
    let quiet = sine(800.0, 0.02, 48_000);
    let [y, _] = run(
        &mut Multibanding::new(RATE),
        Unit::Multiband(lifted),
        &quiet,
        &quiet,
    );
    assert!(
        rms(&y[24_000..]) > rms(&quiet) * 2.0,
        "{}",
        rms(&y[24_000..])
    );
    let none = vec![0.0; 9600];
    let [y, _] = run(
        &mut Multibanding::new(RATE),
        Unit::Multiband(lifted),
        &none,
        &none,
    );
    assert!(rms(&y) < 1e-9, "{}", rms(&y));
}
