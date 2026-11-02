//! The delay measured: its echoes on the beat, each as loud as its feedback says, bouncing
//! side to side, in ms, held past full feedback, and frozen.

use super::*;

const RATE: f64 = 48_000.0;

fn open(d: Delay) -> Delay {
    Delay {
        mix: 1.0,
        low_cut_hz: 20.0,
        high_cut_hz: 20_000.0,
        ..d
    }
}

/// `d`'s answer to a click on each side (`sides`), `seconds` long.
fn impulse(d: &Delay, sides: [f32; 2], seconds: f32) -> [Vec<f32>; 2] {
    let n = (RATE as f32 * seconds) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    (l[0], r[0]) = (sides[0], sides[1]);
    let mut v = Delaying::new(RATE);
    for (a, b) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
        v.process(d, 120.0, a, b);
    }
    [l, r]
}

/// Where the loudest sample is in `range`, and how loud.
fn peak(x: &[f32], range: std::ops::Range<usize>) -> (usize, f32) {
    let s = range.start;
    x[range]
        .iter()
        .enumerate()
        .map(|(i, v)| (s + i, v.abs()))
        .fold((0, 0.0), |a, b| if b.1 > a.1 { b } else { a })
}

#[test]
fn its_echoes_land_on_the_beat_each_as_loud_as_feedback_leaves_it() {
    // An eighth at 120: 250 ms, 12,000 samples.
    let d = open(Delay {
        feedback: 0.5,
        ..Delay::default()
    });
    let [l, _] = impulse(&d, [0.5, 0.5], 1.0);
    let (at1, _) = peak(&l, 6_000..18_000);
    let (at2, _) = peak(&l, 18_000..30_000);
    assert!(at1.abs_diff(12_000) <= 3, "{at1}");
    assert!(at2.abs_diff(24_000) <= 6, "{at2}");
    let energy = |r: std::ops::Range<usize>| l[r].iter().map(|s| s * s).sum::<f32>();
    let ratio = (energy(18_000..30_000) / energy(6_000..18_000)).sqrt();
    assert!((ratio - 0.5).abs() < 0.08, "{ratio}");
    assert!(l[1..11_990].iter().all(|s| s.abs() < 1e-4));
}

#[test]
fn ping_pong_bounces_and_ms_times_hold() {
    let d = open(DelayStyle::PingPong.delay());
    // A dotted eighth at 120: 375 ms, 18,000 samples; first left, then right.
    let [l, r] = impulse(&d, [1.0, 1.0], 1.0);
    let (at, first) = peak(&l, 12_000..24_000);
    assert!(at.abs_diff(18_000) <= 3 && first > 0.3, "{at} {first}");
    assert!(peak(&r, 12_000..24_000).1 < first * 0.05);
    let (at, second) = peak(&r, 30_000..42_000);
    assert!(at.abs_diff(36_000) <= 6 && second > 0.1, "{at} {second}");
    // In ms, each side its own.
    let ms = open(Delay {
        sync: false,
        ms: [100.0, 300.0],
        feedback: 0.0,
        ..Delay::default()
    });
    let [l, r] = impulse(&ms, [1.0, 1.0], 0.5);
    assert!(peak(&l, 2_000..8_000).0.abs_diff(4_800) <= 3);
    assert!(peak(&r, 10_000..20_000).0.abs_diff(14_400) <= 3);
}

#[test]
fn past_full_feedback_it_holds_and_frozen_it_keeps_going() {
    let d = open(Delay {
        feedback: 1.1,
        drive: 0.5,
        ..Delay::default()
    });
    let mut v = Delaying::new(RATE);
    let mut seed = 3u32;
    let mut most = 0f32;
    for _ in 0..1_000 {
        let mut l: Vec<f32> = (0..256)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
            })
            .collect();
        let mut r = l.clone();
        v.process(&d, 120.0, &mut l, &mut r);
        most = most.max(peak(&l, 0..256).1);
    }
    assert!(most.is_finite() && most < 2.0, "{most}");
    // Frozen: the tenth echo nearly as loud as the first.
    let frozen = open(Delay {
        freeze: false,
        feedback: 0.0,
        ..Delay::default()
    });
    let n = (RATE * 3.0) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    (l[0], r[0]) = (0.5, 0.5);
    let mut v = Delaying::new(RATE);
    for (k, (a, b)) in l.chunks_mut(256).zip(r.chunks_mut(256)).enumerate() {
        // Frozen just after the first echo's gone in.
        let d = Delay {
            freeze: k * 256 > 100,
            ..frozen
        };
        v.process(&d, 120.0, a, b);
    }
    let first = peak(&l, 6_000..18_000).1;
    let tenth = peak(&l, 114_000..126_000).1;
    assert!(tenth > first * 0.7, "{first} {tenth}");
}
