//! Bloom measured.

use super::*;

const RATE: f64 = 48_000.0;

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn pitch(x: &[f32]) -> f32 {
    let ups: Vec<f32> = x
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
        .map(|(i, w)| i as f32 + w[0] / (w[0] - w[1]))
        .collect();
    (ups.len() - 1) as f32 / (ups[ups.len() - 1] - ups[0]) * RATE as f32
}

/// `samples` of `notes` (pitch, velocity) all held for `hold`, in blocks; `wheel` from the start.
fn play_all(
    b: &Bloom,
    notes: &[(u8, f32)],
    hold: usize,
    samples: usize,
    wheel: f32,
) -> [Vec<f32>; 2] {
    let plan = BloomPlan::new(b);
    let mut s = BloomSynth::new(RATE);
    let (mut l, mut r) = (vec![0f32; samples], vec![0f32; samples]);
    let mut at = 0;
    while at < samples {
        let n = 256.min(samples - at);
        let mut played = Vec::new();
        let mut moves = Vec::new();
        if at == 0 {
            moves.push(Control {
                offset: 0,
                kind: ControlKind::Cc {
                    number: 1,
                    value: wheel,
                },
            });
            for &(p, v) in notes {
                played.push(Note {
                    offset: 0,
                    pitch: p,
                    velocity: v,
                    release: 0.0,
                });
            }
        }
        if at <= hold && hold < at + n {
            for &(p, _) in notes {
                played.push(Note {
                    offset: (hold - at) as u32,
                    pitch: p,
                    velocity: 0.0,
                    release: 0.0,
                });
            }
        }
        s.begin(&plan, std::iter::empty());
        s.process(
            &plan,
            [&mut l[at..at + n], &mut r[at..at + n]],
            n,
            &played,
            &moves,
        );
        at += n;
    }
    [l, r]
}

fn play(b: &Bloom, note: u8, hold: usize, samples: usize) -> [Vec<f32>; 2] {
    play_all(b, &[(note, 1.0)], hold, samples, 0.0)
}

/// A plain plant: the neutral seed, nothing grown, no echo or space, no LFO.
fn plain() -> Bloom {
    let mut b = Bloom::default();
    for k in 0..BRANCHES {
        b.params[branch_length(k)] = 0.0;
    }
    b.params[gene_param(44)] = 0.0;
    b.params[gene_param(45)] = 0.0;
    b.params[gene_param(37)] = 0.0;
    b
}

#[test]
fn a_seed_plays_in_tune_at_a_sensible_level_and_stops() {
    let mut b = plain();
    // A sine through an open filter, for counting its cycles.
    b.params[gene_param(16)] = 0.0;
    b.params[gene_param(22)] = 0.0;
    b.params[gene_param(32)] = 1.0;
    b.params[gene_param(31)] = 0.0;
    let [l, r] = play(&b, 69, 24_000, 72_000);
    let held = &l[4800..24_000];
    assert!((pitch(held) - 440.0).abs() < 1.5, "{}", pitch(held));
    assert!(rms(held) > 0.02 && rms(held) < 0.7, "{}", rms(held));
    assert!((l[6000] - r[6000]).abs() < 1e-6, "in the middle");
    assert!(
        rms(&l[60_000..]) < 1e-4,
        "it stops once let go: {}",
        rms(&l[60_000..])
    );
    // And costs nothing once it has.
    let mut s = BloomSynth::new(RATE);
    let plan = BloomPlan::new(&b);
    let (mut a, mut c) = (vec![1f32; 256], vec![1f32; 256]);
    s.begin(&plan, std::iter::empty());
    s.process(&plan, [&mut a, &mut c], 256, &[], &[]);
    assert!(a.iter().chain(&c).all(|x| *x == 0.0));
}

#[test]
fn each_note_of_the_octave_plays_its_own_branch() {
    let mut b = plain();
    for k in 0..BRANCHES {
        b.params[branch_length(k)] = 1.0;
    }
    let c = play(&b, 60, 40_000, 40_000)[0].clone();
    let c_up = play(&b, 72, 40_000, 40_000)[0].clone();
    let d = play(&b, 62, 40_000, 40_000)[0].clone();
    // An octave apart, the same branch: as loud (near enough). Another branch, another sound.
    let (lc, lu, ld) = (rms(&c[4800..]), rms(&c_up[4800..]), rms(&d[4800..]));
    assert!(lc > 0.001 && lu > 0.001 && ld > 0.001, "{lc} {lu} {ld}");
    let differ = (lc - ld).abs() / lc.max(ld);
    let same = (lc - lu).abs() / lc.max(lu);
    assert!(differ > 0.05 || same < differ, "C {lc}, C up {lu}, D {ld}");
}

#[test]
fn the_wheel_grows_the_branches_under_a_held_note() {
    let mut b = plain();
    b.params[BLOOM_WILD] = 1.0;
    let still = play_all(&b, &[(60, 1.0)], 30_000, 30_000, 0.0)[0].clone();
    let grown = play_all(&b, &[(60, 1.0)], 30_000, 30_000, 1.0)[0].clone();
    let diff: f32 = still[9600..]
        .iter()
        .zip(&grown[9600..])
        .map(|(a, b)| (a - b).abs())
        .sum::<f32>()
        / 20_000.0;
    assert!(diff > 0.002, "{diff}");
    // The window hears how far it reached.
    let plan = BloomPlan::new(&b);
    let mut s = BloomSynth::new(RATE);
    let (mut l, mut r) = (vec![0f32; 256], vec![0f32; 256]);
    let note = Note {
        offset: 0,
        pitch: 62,
        velocity: 1.0,
        release: 0.0,
    };
    let wheel = Control {
        offset: 0,
        kind: ControlKind::Cc {
            number: 1,
            value: 0.5,
        },
    };
    s.begin(&plan, std::iter::empty());
    s.process(&plan, [&mut l, &mut r], 256, &[note], &[wheel]);
    let live = s.live();
    assert!(live[2] > 0.0, "D's branch sounds");
    assert!(
        (live[BRANCHES + 2] - 0.5).abs() < 0.01,
        "{}",
        live[BRANCHES + 2]
    );
    assert_eq!(live[2 * BRANCHES], 1.0);
}

#[test]
fn a_layer_plays_its_branches_together_and_one_plays_the_picked() {
    let mut b = plain();
    b.keys = BloomKeys::Layer;
    b.layer = 0b111;
    let plan = BloomPlan::new(&b);
    let mut s = BloomSynth::new(RATE);
    let (mut l, mut r) = (vec![0f32; 256], vec![0f32; 256]);
    let note = Note {
        offset: 0,
        pitch: 64,
        velocity: 1.0,
        release: 0.0,
    };
    s.begin(&plan, std::iter::empty());
    s.process(&plan, [&mut l, &mut r], 256, &[note], &[]);
    assert_eq!(s.live()[2 * BRANCHES], 3.0);
    b.keys = BloomKeys::One;
    b.chosen = 7;
    let plan = BloomPlan::new(&b);
    let mut s = BloomSynth::new(RATE);
    s.begin(&plan, std::iter::empty());
    s.process(&plan, [&mut l, &mut r], 256, &[note], &[]);
    assert!(s.live()[7] > 0.0 && s.live()[4] == 0.0);
}

#[test]
fn every_kind_of_seed_grown_anywhere_stays_finite_and_in_bounds() {
    let mut loud = 0;
    for (n, kind) in SeedKind::ALL.iter().enumerate() {
        for luck in 0..6u32 {
            let mut b = Bloom::default();
            b.plant(*kind, luck * 31 + n as u32);
            b.params[BLOOM_WILD] = 1.0;
            b.params[BLOOM_ATONAL] = (luck % 3) as f32 * 0.5;
            for k in 0..BRANCHES {
                b.params[branch_length(k)] = 1.0;
                b.params[branch_bend(k)] = (k as f32 / 6.0) - 1.0;
            }
            b.keys = BloomKeys::ALL[luck as usize % BloomKeys::ALL.len()];
            let chord = [(36 + luck as u8, 1.0), (60, 0.6), (67, 0.3), (84, 1.0)];
            let [l, r] = play_all(&b, &chord, 9600, 24_000, (luck % 2) as f32);
            let peak = l.iter().chain(&r).fold(0f32, |m, x| m.max(x.abs()));
            assert!(peak.is_finite() && peak < 8.0, "{kind:?} {luck}: {peak}");
            if rms(&l[..9600]) > 1e-3 {
                loud += 1;
            }
        }
    }
    // Nearly every seed makes itself heard.
    let all = SeedKind::ALL.len() * 6;
    assert!(loud * 10 >= all * 9, "{loud} of {all}");
}

#[test]
fn a_new_plan_with_the_same_seed_sounds_the_same() {
    let mut b = Bloom::default();
    b.plant(SeedKind::Pluck, 5);
    let one = play(&b, 60, 4000, 9600)[0].clone();
    let two = play(&b, 60, 4000, 9600)[0].clone();
    assert_eq!(one, two);
}

#[test]
fn automation_moves_a_gene() {
    let b = plain();
    let plan = BloomPlan::new(&b);
    let mut s = BloomSynth::new(RATE);
    let mut out = |cut: Option<f32>| {
        let (mut l, mut r) = (vec![0f32; 4096], vec![0f32; 4096]);
        let note = Note {
            offset: 0,
            pitch: 48,
            velocity: 1.0,
            release: 0.0,
        };
        let mut at = 0;
        while at < 4096 {
            let auto: Vec<(usize, f32)> = cut
                .map(|c| (gene_param(32), c - plan.pos(gene_param(32))))
                .into_iter()
                .collect();
            s.begin(&plan, auto.into_iter());
            let notes = if at == 0 { vec![note] } else { vec![] };
            s.process(
                &plan,
                [&mut l[at..at + 256], &mut r[at..at + 256]],
                256,
                &notes,
                &[],
            );
            at += 256;
        }
        let off = Note {
            offset: 0,
            pitch: 48,
            velocity: 0.0,
            release: 0.0,
        };
        s.process(&plan, [&mut l[..256], &mut r[..256]], 256, &[off], &[]);
        l
    };
    let open = rms(&out(Some(1.0))[2048..]);
    let shut = rms(&out(Some(0.05))[2048..]);
    assert!(open > shut * 2.0, "{open} {shut}");
}

/// How long a second of sixteen grown voices takes: run with --ignored to see it.
#[test]
#[ignore]
fn sixteen_voices_for_a_second() {
    let mut b = Bloom::default();
    b.plant(SeedKind::Pad, 3);
    for k in 0..BRANCHES {
        b.params[branch_length(k)] = 0.7;
    }
    let chord: Vec<(u8, f32)> = (0..16).map(|k| (40 + 3 * k as u8, 0.8)).collect();
    let t = std::time::Instant::now();
    play_all(&b, &chord, 48_000, 48_000, 0.3);
    eprintln!("16 voices, 1 s: {:?}", t.elapsed());
}

#[test]
#[ignore]
fn where_the_time_goes() {
    let mut b = Bloom::default();
    b.plant(SeedKind::Pad, 3);
    for k in 0..BRANCHES {
        b.params[branch_length(k)] = 0.7;
    }
    let time = |b: &Bloom, n: usize, what: &str| {
        let chord: Vec<(u8, f32)> = (0..n).map(|k| (40 + 3 * k as u8, 0.8)).collect();
        let t = std::time::Instant::now();
        play_all(b, &chord, 48_000, 48_000, 0.3);
        eprintln!("{what}: {:?}", t.elapsed());
    };
    time(&b, 1, "1 voice");
    time(&b, 16, "16 voices");
    let mut dry = b.clone();
    dry.params[BLOOM_EFFECT] = 0.0;
    time(&dry, 1, "1 voice dry");
    time(&dry, 16, "16 voices dry");
    let mut draft = dry.clone();
    draft.quality = PolyQuality::Draft;
    time(&draft, 16, "16 voices dry draft");
    let mut plain = draft.clone();
    for g in [28, 39, 43, 29, 25, 31, 30] {
        plain.params[gene_param(g)] = 0.0;
        plain.locks |= 1 << g;
    }
    time(&plain, 16, "16 voices dry draft plain");
}

#[test]
#[ignore]
fn how_loud_seeds_are() {
    for kind in SeedKind::ALL {
        let mut levels = Vec::new();
        for luck in 0..12u32 {
            let mut b = Bloom::default();
            b.plant(kind, luck * 7919 + 3);
            let [l, r] = play(&b, 60, 24_000, 24_000);
            let x: Vec<f32> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect();
            levels.push(20.0 * rms(&x[2400..]).max(1e-9).log10());
        }
        levels.sort_by(|a, b| a.total_cmp(b));
        eprintln!(
            "{kind:?}: min {:.1} median {:.1} max {:.1} dB",
            levels[0], levels[6], levels[11]
        );
    }
}
