//! The reverb measured: how long it rings against what's asked, its sides apart, its pre-delay,
//! freeze and shimmer, and a dry mix that's the sound untouched.

use super::*;

const RATE: f64 = 48_000.0;

/// `r`'s answer to a click, `seconds` long, both sides.
fn impulse(r: &Reverb, seconds: f32) -> [Vec<f32>; 2] {
    let n = (RATE as f32 * seconds) as usize;
    let (mut l, mut rr) = (vec![0.0f32; n], vec![0.0f32; n]);
    (l[0], rr[0]) = (1.0, 1.0);
    let mut v = Reverbing::new(RATE);
    for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
        v.process(r, 120.0, a, b);
    }
    [l, rr]
}

/// A plain tail: no first reflections, nothing cut, no colour or movement.
fn plain(decay_s: f32) -> Reverb {
    Reverb {
        decay_s,
        mix: 1.0,
        early: 0.0,
        predelay_ms: 0.0,
        low_cut_hz: 20.0,
        high_cut_hz: 20_000.0,
        damping_hz: 20_000.0,
        bass: 1.0,
        modulation: 0.0,
        ..Reverb::default()
    }
}

/// How long it takes to fall 60 dB, from how long its energy takes from -5 to -25 dB (T20).
fn rt60(x: &[Vec<f32>; 2]) -> f32 {
    let e: Vec<f64> = x[0]
        .iter()
        .zip(&x[1])
        .map(|(a, b)| ((a + b) as f64 * 0.5).powi(2))
        .collect();
    // Schroeder: the energy still to come, from each sample on.
    let mut left = vec![0.0f64; e.len()];
    let mut sum = 0.0;
    for i in (0..e.len()).rev() {
        sum += e[i];
        left[i] = sum;
    }
    let db = |i: usize| 10.0 * (left[i] / left[0]).log10();
    let t5 = (0..e.len()).find(|&i| db(i) < -5.0).unwrap();
    let t25 = (0..e.len()).find(|&i| db(i) < -25.0).unwrap();
    3.0 * (t25 - t5) as f32 / RATE as f32
}

#[test]
fn it_rings_as_long_as_its_decay_asks() {
    for want in [0.8f32, 2.0, 4.0] {
        let got = rt60(&impulse(&plain(want), want * 1.6 + 0.5));
        assert!(
            (got / want - 1.0).abs() < 0.2,
            "asked {want} s, rang {got} s"
        );
    }
}

#[test]
fn its_sides_differ_and_the_dry_mix_is_the_sound_untouched() {
    let x = impulse(&plain(2.0), 2.0);
    let tail = 4_800..48_000;
    let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| (x * y) as f64).sum::<f64>();
    let (l, r) = (&x[0][tail.clone()], &x[1][tail]);
    let corr = dot(l, r) / (dot(l, l) * dot(r, r)).sqrt();
    assert!(corr.abs() < 0.3, "{corr}");
    // Its level about the click's energy: neither silent nor blown up.
    let energy = dot(&x[0], &x[0]);
    assert!((0.05..20.0).contains(&energy), "{energy}");
    // Dry, the sound as it went in.
    let dry = impulse(
        &Reverb {
            mix: 0.0,
            ..plain(2.0)
        },
        0.1,
    );
    assert_eq!(dry[0][0], 1.0);
    assert!(dry[0][1..].iter().all(|&s| s == 0.0));
}

#[test]
fn its_predelay_holds_everything_back() {
    let r = Reverb {
        predelay_ms: 100.0,
        early: 1.0,
        style: ReverbStyle::Hall,
        ..plain(1.5)
    };
    let x = impulse(&r, 0.5);
    let before: f32 = x[0][1..4_700].iter().map(|s| s.abs()).sum();
    let after: f32 = x[0][4_800..12_000].iter().map(|s| s.abs()).sum();
    assert!(before < 1e-3 && after > 0.01, "{before} {after}");
    // In beats: a sixteenth at 120 is 125 ms.
    let beats = Reverb {
        predelay_beats: Some(0.25),
        ..r
    };
    assert_eq!(beats.predelay_s(120.0), 0.125);
}

#[test]
fn frozen_it_holds_and_never_grows() {
    let mut v = Reverbing::new(RATE);
    let r = plain(1.5);
    let mut seed = 7u32;
    let mut noise = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    };
    let block = |v: &mut Reverbing, r: &Reverb, f: &mut dyn FnMut() -> f32| {
        let mut l: Vec<f32> = (0..256).map(|_| f()).collect();
        let mut rr = l.clone();
        v.process(r, 120.0, &mut l, &mut rr);
        l.iter().map(|s| s * s).sum::<f32>()
    };
    for _ in 0..100 {
        block(&mut v, &r, &mut noise);
    }
    let frozen = Reverb { freeze: true, ..r };
    let first: f32 = (0..20).map(|_| block(&mut v, &frozen, &mut noise)).sum();
    for _ in 0..300 {
        block(&mut v, &frozen, &mut noise);
    }
    let later: f32 = (0..20).map(|_| block(&mut v, &frozen, &mut noise)).sum();
    assert!(
        later > first * 0.5 && later < first * 2.0,
        "{first} {later}"
    );
}

#[test]
fn shimmer_climbs_an_octave() {
    // A 440 Hz burst: with shimmer, its tail has far more at 880.
    let at = |shimmer: f32| {
        let r = Reverb {
            shimmer,
            ..plain(3.0)
        };
        let n = (RATE * 2.0) as usize;
        let mut l: Vec<f32> = (0..n)
            .map(|i| match i < 9_600 {
                true => 0.5 * (TAU * 440.0 * i as f32 / RATE as f32).sin(),
                false => 0.0,
            })
            .collect();
        let mut rr = l.clone();
        let mut v = Reverbing::new(RATE);
        for (a, b) in l.chunks_mut(256).zip(rr.chunks_mut(256)) {
            v.process(&r, 120.0, a, b);
        }
        // Goertzel at 880 Hz over the tail.
        let tail = &l[48_000..];
        let w = TAU * 880.0 / RATE as f32;
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &x in tail {
            let s = x + 2.0 * w.cos() * s1 - s2;
            (s2, s1) = (s1, s);
        }
        s1 * s1 + s2 * s2 - 2.0 * w.cos() * s1 * s2
    };
    let (off, on) = (at(0.0), at(1.0));
    assert!(on > off * 4.0, "{off} {on}");
}
