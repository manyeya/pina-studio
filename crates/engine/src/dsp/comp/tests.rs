use super::*;

const RATE: f64 = 48_000.0;

#[test]
fn both_sides_come_down_by_the_louder_and_let_go_after() {
    let c = Comp {
        threshold_db: -20.0,
        ratio: 4.0,
        attack_ms: 1.0,
        release_ms: 50.0,
        ..Comp::default()
    };
    let mut comp = Compressor::new(RATE);
    // Half a second loud on the left, quiet on the right; then quiet on both.
    let n = 48_000;
    let mut l: Vec<f32> = (0..n).map(|i| if i < n / 2 { 1.0 } else { 0.01 }).collect();
    let mut r = vec![0.01f32; n];
    let mut gr = vec![0f32; n];
    comp.process(&c, &mut l, &mut r, &mut gr);
    // 20 dB over comes out 5 over: 15 down, the right side with it.
    assert!((gr[n / 2 - 1] - 15.0).abs() < 0.2, "{}", gr[n / 2 - 1]);
    assert!((20.0 * r[n / 2 - 1].log10() + 55.0).abs() < 0.3);
    // A quarter second after, let go.
    assert!(gr[n - 1] < 0.01);
}

/// A tone of `hz` at `level`, `seconds` long, then silence as long; through `c`: (left side,
/// gain reduction).
fn through(c: &Comp, hz: f32, level: f32, seconds: f32) -> (Vec<f32>, Vec<f32>) {
    let n = (seconds * RATE as f32) as usize;
    let mut l: Vec<f32> = (0..2 * n)
        .map(|i| match i < n {
            true => (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin() * level,
            false => 0.0,
        })
        .collect();
    let mut r = l.clone();
    let mut gr = vec![0f32; l.len()];
    let mut comp = Compressor::new(RATE);
    for at in (0..l.len()).step_by(512) {
        let end = (at + 512).min(l.len());
        comp.process(c, &mut l[at..end], &mut r[at..end], &mut gr[at..end]);
    }
    (l, gr)
}

/// How long after `from` the reduction takes to fall under 1 dB, in seconds.
fn recovers(gr: &[f32], from: usize) -> f32 {
    gr[from..].iter().position(|&g| g < 1.0).unwrap_or(gr.len()) as f32 / RATE as f32
}

#[test]
fn a_bus_compressor_goes_in_its_steps() {
    // A ratio of 3 is the 4:1 step: 20 dB over comes out 5 over.
    let bus = Comp {
        style: CompStyle::Bus,
        threshold_db: -20.0,
        ratio: 3.0,
        attack_ms: 0.2,
        release_ms: 250.0,
        ..Comp::default()
    };
    let (_, gr) = through(&bus, 1_000.0, 1.0, 0.5);
    assert!((gr[23_000] - 15.0).abs() < 0.5, "{}", gr[23_000]);
    // 250 ms is the 300 ms step: three time constants out it's let go most of the way.
    let back = recovers(&gr, 24_000);
    assert!((0.5..1.2).contains(&back), "{back}");
}

#[test]
fn an_automatic_release_lets_a_hit_go_quickly_and_a_long_squeeze_slowly() {
    let auto = Comp {
        threshold_db: -20.0,
        attack_ms: 1.0,
        auto_release: true,
        ..Comp::default()
    };
    let (_, hit) = through(&auto, 1_000.0, 1.0, 0.03);
    let (_, held) = through(&auto, 1_000.0, 1.0, 2.0);
    let (quick, slow) = (recovers(&hit, 1_440), recovers(&held, 96_000));
    assert!(quick < 0.5 && slow > quick * 3.0, "{quick} {slow}");
}

#[test]
fn the_sidechain_high_pass_keeps_the_lows_from_pumping_it() {
    let plain = Comp {
        threshold_db: -20.0,
        attack_ms: 1.0,
        ..Comp::default()
    };
    let filtered = Comp {
        sidechain_hz: 150.0,
        ..plain
    };
    let (_, a) = through(&plain, 40.0, 0.8, 0.5);
    let (_, b) = through(&filtered, 40.0, 0.8, 0.5);
    assert!(b[20_000] < a[20_000] - 6.0, "{} {}", a[20_000], b[20_000]);
}

#[test]
fn half_mixed_is_halfway_between_dry_and_compressed() {
    let full = Comp {
        threshold_db: -20.0,
        attack_ms: 1.0,
        ..Comp::default()
    };
    let half = Comp { mix: 0.5, ..full };
    let peak = |s: &[f32]| s[20_000..23_000].iter().fold(0f32, |m, x| m.max(x.abs()));
    let (wet, _) = through(&full, 1_000.0, 1.0, 0.5);
    let (mixed, _) = through(&half, 1_000.0, 1.0, 0.5);
    assert!((peak(&mixed) - (1.0 + peak(&wet)) / 2.0).abs() < 0.01);
}

#[test]
fn smooth_eases_in_and_punchy_bites() {
    let c = |style| Comp {
        style,
        threshold_db: -20.0,
        attack_ms: 1.0,
        ..Comp::default()
    };
    // Just over the threshold, a wide knee's barely started; a tight one's well in.
    let at = |style| through(&c(style), 1_000.0, 0.13, 0.5).1[20_000];
    assert!(
        at(CompStyle::Smooth) < at(CompStyle::Punchy),
        "{} {}",
        at(CompStyle::Smooth),
        at(CompStyle::Punchy)
    );
    // Punchy's colour rounds off what's loudest.
    let peak = |s: &[f32]| s[20_000..23_000].iter().fold(0f32, |m, x| m.max(x.abs()));
    let loud = Comp {
        threshold_db: 0.0,
        makeup_db: 6.0,
        ..c(CompStyle::Punchy)
    };
    let clean = Comp {
        style: CompStyle::Clean,
        ..loud
    };
    assert!(
        peak(&through(&loud, 1_000.0, 0.9, 0.5).0)
            < peak(&through(&clean, 1_000.0, 0.9, 0.5).0) * 0.95
    );
}

#[test]
fn spectral_turns_down_the_kick_and_not_the_hats_over_it() {
    // A loud 60 Hz kick every half second over a steady quiet 6 kHz tone.
    let n = 2 * 48_000;
    let tone = |i: usize| (std::f32::consts::TAU * 6_000.0 * i as f32 / RATE as f32).sin() * 0.05;
    let kick = |i: usize| {
        let t = (i % 24_000) as f32 / RATE as f32;
        (std::f32::consts::TAU * 60.0 * t).sin() * (-t / 0.08).exp() * 0.9
    };
    // How far the tone dips while the kick's loudest, dB: its level mid-hit against just before.
    let dip = |spectral: f32| {
        let c = Comp {
            threshold_db: -24.0,
            ratio: 8.0,
            attack_ms: 1.0,
            release_ms: 100.0,
            spectral,
            ..Comp::default()
        };
        let mut l: Vec<f32> = (0..n).map(|i| tone(i) + kick(i)).collect();
        let mut r = l.clone();
        let mut gr = vec![0f32; n];
        Compressor::new(RATE).process(&c, &mut l, &mut r, &mut gr);
        // The tone alone: what's left once the kick's taken out (a high pass's worth: 6 kHz
        // over 600 samples is many cycles, 60 Hz under a fifth of one).
        let amp = |from: usize| {
            let (mut s, mut co) = (0f32, 0f32);
            for (i, x) in l.iter().enumerate().skip(from).take(480) {
                let w = std::f32::consts::TAU * 6_000.0 * i as f32 / RATE as f32;
                s += x * w.sin();
                co += x * w.cos();
            }
            2.0 * s.hypot(co) / 480.0
        };
        // Mid-hit in the second second, against the end of the beat before.
        let (hit, before) = (amp(48_000 + 1_200), amp(48_000 - 600));
        20.0 * (before / hit).log10()
    };
    let (alike, spectral) = (dip(0.0), dip(1.0));
    assert!(alike > 6.0, "{alike}");
    assert!(spectral < alike / 3.0, "{spectral} {alike}");
}
