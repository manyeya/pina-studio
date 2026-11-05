//! The synth measured.

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

fn level_at(x: &[f32], hz: f32) -> f32 {
    let (mut re, mut im) = (0f32, 0f32);
    for (i, s) in x.iter().enumerate() {
        let ph = TAU * hz * i as f32 / RATE as f32;
        (re, im) = (re + s * ph.cos(), im + s * ph.sin());
    }
    (re * re + im * im).sqrt() / x.len() as f32 * 2.0
}

/// `samples` of a note held for `hold`, in blocks.
fn play(p: &Poly, note: u8, hold: usize, samples: usize) -> [Vec<f32>; 2] {
    let plan = PolyPlan::new(p);
    let mut s = PolySynth::new(RATE);
    let (mut l, mut r) = (vec![0f32; samples], vec![0f32; samples]);
    let mut at = 0;
    let by = Offsets::default();
    while at < samples {
        let n = 256.min(samples - at);
        let mut notes = Vec::new();
        if at == 0 {
            notes.push(Note {
                offset: 0,
                pitch: note,
                velocity: 1.0,
            });
        }
        if at <= hold && hold < at + n {
            notes.push(Note {
                offset: (hold - at) as u32,
                pitch: note,
                velocity: 0.0,
            });
        }
        s.begin(&plan, std::iter::empty(), &by);
        let (a, b) = (&mut l[at..at + n], &mut r[at..at + n]);
        s.process(&plan, [a, b], n, &notes, &[]);
        at += n;
    }
    [l, r]
}

/// One saw, nothing else, its phase set so it's repeatable.
fn saw() -> Poly {
    let mut p = Poly::default();
    p.params[osc_param(0, OscField::Position)] = 2.0 / 3.0;
    p.params[osc_param(0, OscField::Random)] = 0.0;
    p.params[POLY_DRIFT] = 0.0;
    p.params[POLY_VELOCITY] = 0.0;
    p
}

#[test]
fn a_note_plays_in_tune_at_a_sensible_level_and_stops() {
    let [l, r] = play(&saw(), 69, 24_000, 48_000);
    let held = &l[4800..24_000];
    assert!((pitch(held) - 440.0).abs() < 1.0, "{}", pitch(held));
    assert!(rms(held) > 0.08 && rms(held) < 0.6, "{}", rms(held));
    assert_eq!(l[6000], r[6000], "one voice in the middle");
    assert!(rms(&l[40_000..]) < 1e-3, "{}", rms(&l[40_000..]));
    // Each table and note in tune: a sine an octave and a fifth up.
    let mut p = saw();
    p.params[osc_param(0, OscField::Position)] = 0.0;
    p.params[osc_param(0, OscField::Octave)] = 1.0;
    p.params[osc_param(0, OscField::Semi)] = 7.0;
    let [l, _] = play(&p, 57, 24_000, 24_000);
    assert!(
        (pitch(&l[4800..]) - 660.0).abs() < 1.0,
        "{}",
        pitch(&l[4800..])
    );
}

#[test]
fn a_high_note_doesnt_alias() {
    // A saw at 6 kHz: what's between its harmonics is what folded back.
    let mut p = saw();
    p.params[POLY_MASTER] = 0.0;
    let [l, _] = play(&p, 112, 48_000, 48_000);
    let hz = 440.0 * 2f32.powf(43.0 / 12.0);
    let x = &l[4800..];
    let wanted = level_at(x, hz);
    let folded = [hz * 0.5, hz * 1.5, hz * 0.25, hz * 0.75]
        .into_iter()
        .fold(0f32, |m, f| m.max(level_at(x, f)));
    assert!(wanted > 0.1, "{wanted}");
    assert!(folded < wanted * 0.003, "{wanted} {folded}");
}

#[test]
fn unison_goes_wide_and_stays_as_loud() {
    let one = play(&saw(), 57, 48_000, 48_000);
    let mut p = saw();
    p.params[osc_param(0, OscField::Unison)] = 8.0;
    p.params[osc_param(0, OscField::Random)] = 1.0;
    let many = play(&p, 57, 48_000, 48_000);
    let (a, b) = (rms(&one[0][4800..]), rms(&many[0][4800..]));
    assert!((b / a - 1.0).abs() < 0.5, "{a} {b}");
    let apart: f32 = many[0][4800..]
        .iter()
        .zip(&many[1][4800..])
        .map(|(l, r)| (l - r).abs())
        .sum();
    assert!(apart > 100.0, "{apart}");
    // Narrowed to nothing, the sides are the same again.
    p.params[osc_param(0, OscField::Width)] = 0.0;
    let mono = play(&p, 57, 48_000, 24_000);
    let apart: f32 = mono[0]
        .iter()
        .zip(&mono[1])
        .map(|(l, r)| (l - r).abs())
        .sum();
    assert!(apart < 1e-3, "{apart}");
}

#[test]
fn an_envelope_on_the_filter_opens_it_for_each_note() {
    let cut = filter_param(0, FilterField::Cutoff);
    let mut p = saw();
    p.filters[0] = PolyFilter::Ladder;
    p.params[cut] = 200.0;
    p.params[env_param(1, EnvField::Attack)] = 0.1;
    p.params[env_param(1, EnvField::Decay)] = 200.0;
    p.params[env_param(1, EnvField::Sustain)] = 0.0;
    let bright = |x: &[f32]| level_at(x, 1320.0);
    let [closed, _] = play(&p, 57, 48_000, 48_000);
    p.routes.push(PolyRoute {
        source: PolySource::Env(1),
        dest: PolyDest::Param(cut as u16),
        depth: 0.8,
        by: None,
    });
    let [swept, _] = play(&p, 57, 48_000, 48_000);
    assert!(bright(&swept[..2400]) > bright(&closed[..2400]) * 6.0);
    // Once it's closed again it's the same note.
    let (a, b) = (bright(&swept[40_000..]), bright(&closed[40_000..]));
    assert!((a - b).abs() < 0.01, "{a} {b}");
}

#[test]
fn an_lfo_moves_what_its_routed_to() {
    let level = osc_param(0, OscField::Level);
    let mut p = saw();
    p.params[lfo_param(0, LfoField::Rate)] = 5.0;
    p.routes.push(PolyRoute {
        source: PolySource::Lfo(0),
        dest: PolyDest::Param(level as u16),
        depth: 0.7,
        by: None,
    });
    let [x, _] = play(&p, 57, 48_000, 48_000);
    let bits: Vec<f32> = x[..48_000].chunks(1200).map(rms).collect();
    let (lo, hi) = bits[4..]
        .iter()
        .fold((1f32, 0f32), |m, v| (m.0.min(*v), m.1.max(*v)));
    assert!(hi > lo * 3.0, "{lo} {hi}");
    // By the macro only: at nothing, it does nothing.
    p.routes[0].by = Some(PolySource::Macro(0));
    let [x, _] = play(&p, 57, 48_000, 48_000);
    let bits: Vec<f32> = x[..48_000].chunks(1200).map(rms).collect();
    let (lo, hi) = bits[4..]
        .iter()
        .fold((1f32, 0f32), |m, v| (m.0.min(*v), m.1.max(*v)));
    assert!(hi < lo * 1.1, "{lo} {hi}");
}

#[test]
fn the_warps_change_the_sound() {
    // How bright it is: how much it moves sample to sample.
    let top = |p: &Poly| {
        let [x, _] = play(p, 45, 24_000, 24_000);
        let steps: Vec<f32> = x[4800..].windows(2).map(|w| w[1] - w[0]).collect();
        rms(&steps) / rms(&x[4800..])
    };
    let mut plain = saw();
    plain.params[osc_param(0, OscField::Position)] = 0.0;
    let base = top(&plain);
    for w in [
        Warp::Sync,
        Warp::Fold,
        Warp::Pwm,
        Warp::BendUp,
        Warp::Quantize,
        Warp::PhaseDist,
    ] {
        let mut p = plain.clone();
        p.warps[0][0] = w;
        p.params[osc_param(0, OscField::WarpA)] = 0.8;
        assert!(top(&p) > base * 1.1, "{w:?}: {base} {}", top(&p));
    }
    // FM from an earlier oscillator: a sine gets sidebands.
    let mut p = plain.clone();
    p.params[osc_param(1, OscField::Level)] = 0.75;
    p.params[osc_param(0, OscField::Level)] = 0.0;
    p.params[osc_param(1, OscField::Position)] = 0.0;
    p.params[osc_param(0, OscField::Octave)] = 1.0;
    p.warps[1][0] = Warp::FmFromA;
    p.params[osc_param(1, OscField::WarpA)] = 0.5;
    assert!(top(&p) > base * 1.5, "{}", top(&p));
}

#[test]
fn notes_share_and_steal_voices_and_mono_glides() {
    let plan = PolyPlan::new(&Poly {
        voicing: Voicing {
            polyphony: 4,
            ..Voicing::default()
        },
        ..saw()
    });
    let mut s = PolySynth::new(RATE);
    let by = Offsets::default();
    let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
    let notes: Vec<Note> = (0..8)
        .map(|i| Note {
            offset: i * 10,
            pitch: 60 + i as u8,
            velocity: 0.8,
        })
        .collect();
    s.begin(&plan, std::iter::empty(), &by);
    s.process(&plan, [&mut l, &mut r], 512, &notes, &[]);
    assert_eq!(s.voices.iter().filter(|v| v.on).count(), 4);
    assert!(l.iter().all(|v| v.is_finite()));
    // Glide: from the note before, not at once.
    let mut p = saw();
    p.voicing.mono = true;
    p.params[POLY_GLIDE] = 100.0;
    let plan = PolyPlan::new(&p);
    let mut s = PolySynth::new(RATE);
    let at = |pitch, velocity, offset| Note {
        offset,
        pitch,
        velocity,
    };
    s.begin(&plan, std::iter::empty(), &by);
    s.process(&plan, [&mut l, &mut r], 512, &[at(60, 1.0, 0)], &[]);
    s.begin(&plan, std::iter::empty(), &by);
    s.process(&plan, [&mut l, &mut r], 512, &[at(72, 1.0, 0)], &[]);
    let p = s.voices.iter().find(|v| v.on).unwrap().pitch;
    assert!(p > 60.5 && p < 71.5, "{p}");
    assert_eq!(s.voices.iter().filter(|v| v.on).count(), 1);
}

#[test]
fn automation_and_modulators_reach_it() {
    let cut = filter_param(0, FilterField::Cutoff);
    let mut p = saw();
    p.filters[0] = PolyFilter::Ladder;
    let plan = PolyPlan::new(&p);
    let run = |auto: Option<f32>, by: &Offsets| {
        let mut s = PolySynth::new(RATE);
        let (mut l, mut r) = (vec![0f32; 9728], vec![0f32; 9728]);
        let mut at = 0;
        while at < 9728 {
            let notes = [Note {
                offset: 0,
                pitch: 57,
                velocity: 1.0,
            }];
            s.begin(
                &plan,
                auto.map(|a| (cut, a - plan.pos[cut])).into_iter(),
                by,
            );
            let (a, b) = (&mut l[at..at + 256], &mut r[at..at + 256]);
            s.process(&plan, [a, b], 256, if at == 0 { &notes } else { &[] }, &[]);
            at += 256;
        }
        level_at(&l[4800..], 2200.0)
    };
    let open = run(None, &Offsets::default());
    assert!(run(Some(0.2), &Offsets::default()) < open * 0.2);
    let mut by = Offsets::default();
    by.add(ModTarget::Poly(cut as u16), -0.8);
    assert!(run(None, &by) < open * 0.2);
}

#[test]
fn its_matrix_reaches_the_effects_on_its_track() {
    let mut p = saw();
    p.params[macro_param(2)] = 0.5;
    p.routes.push(PolyRoute {
        source: PolySource::Macro(2),
        dest: PolyDest::Effect(1, 3),
        depth: 0.4,
        by: None,
    });
    let plan = PolyPlan::new(&p);
    let mut s = PolySynth::new(RATE);
    let (mut l, mut r) = (vec![0f32; 256], vec![0f32; 256]);
    s.begin(&plan, std::iter::empty(), &Offsets::default());
    s.process(&plan, [&mut l, &mut r], 256, &[], &[]);
    let mut moved = Offsets::default();
    s.outward(&plan, &mut moved);
    assert_eq!(moved.tool_count, 1);
    assert_eq!(moved.tools[0], (1, 3, 0.2));
}

#[test]
fn the_arpeggiator_steps_through_whats_held() {
    let mut p = saw();
    p.arp = Arp {
        mode: ArpMode::Up,
        beats: 0.25,
        octaves: 2,
        gate: 0.5,
        swing: 0.0,
    };
    p.params[env_param(0, EnvField::Release)] = 1.0;
    let plan = PolyPlan::new(&p);
    let mut s = PolySynth::new(RATE);
    let by = Offsets::default();
    let (mut l, mut r) = (vec![0f32; 256], vec![0f32; 256]);
    let held = [
        Note {
            offset: 0,
            pitch: 60,
            velocity: 1.0,
        },
        Note {
            offset: 0,
            pitch: 64,
            velocity: 1.0,
        },
    ];
    s.set_tempo(120.0);
    let mut heard = Vec::new();
    // A sixteenth at 120 is 6000 samples; four steps a pattern, twice over.
    for i in 0..((8 * 6000) / 256 + 1) {
        s.begin(&plan, std::iter::empty(), &by);
        s.process(
            &plan,
            [&mut l, &mut r],
            256,
            if i == 0 { &held } else { &[] },
            &[],
        );
        if let Some(p) = s.arp_note
            && heard.last() != Some(&p)
        {
            heard.push(p);
        }
    }
    assert_eq!(heard[..8], [60, 64, 72, 76, 60, 64, 72, 76], "{heard:?}");
}

#[test]
fn nothing_breaks_it() {
    // Everything at its extremes, on every filter, for a while.
    let mut seed = 0x1234_5678u32;
    for round in 0..40 {
        let mut p = Poly::default();
        for (i, q) in POLY_PARAMS.iter().enumerate() {
            let t = chance(&mut seed);
            p.params[i] = q.scale.value(if t < 0.3 {
                0.0
            } else if t > 0.7 {
                1.0
            } else {
                t
            });
        }
        p.params[POLY_MASTER] = -6.0;
        p.params[osc_param(0, OscField::Level)] = 1.0;
        p.filters = [
            PolyFilter::ALL[round % 20],
            PolyFilter::ALL[(round * 7) % 20],
        ];
        p.routing = [
            FilterRouting::Series,
            FilterRouting::Parallel,
            FilterRouting::Stereo,
        ][round % 3];
        p.warps[0] = [Warp::ALL[round % 20], Warp::ALL[(round * 3) % 20]];
        p.warps[1] = [Warp::ALL[(round * 5) % 20], Warp::ALL[(round * 11) % 20]];
        p.quality = [PolyQuality::Draft, PolyQuality::Good, PolyQuality::Best][round % 3];
        for k in 0..12 {
            p.routes.push(PolyRoute {
                source: [
                    PolySource::Env(1),
                    PolySource::Lfo(2),
                    PolySource::Velocity,
                    PolySource::Random,
                ][k % 4],
                dest: PolyDest::Param(
                    (chance(&mut seed) * PARAM_COUNT as f32) as u16 % PARAM_COUNT as u16,
                ),
                depth: chance(&mut seed) * 2.0 - 1.0,
                by: None,
            });
        }
        let [l, r] = play(&p, (20 + round as u8 * 2).min(120), 2400, 4800);
        assert!(
            l.iter().chain(&r).all(|v| v.is_finite() && v.abs() < 200.0),
            "round {round}"
        );
    }
}

#[test]
fn it_keeps_up() {
    // A full chord of a heavy patch: twenty-one saws a note through a ladder. Optimized, as it
    // ships, that's a small part of one core; these tests themselves aren't, so they get more.
    let mut p = saw();
    for o in 0..3 {
        p.params[osc_param(o, OscField::Level)] = 0.5;
        p.params[osc_param(o, OscField::Unison)] = 7.0;
    }
    p.filters = [PolyFilter::Ladder, PolyFilter::None];
    let most = if cfg!(debug_assertions) { 1.5 } else { 0.25 };
    let share = share(&p, 8);
    assert!(share < most, "{:.1}% of a core", share * 100.0);
}

#[test]
fn an_idle_synth_costs_nothing_and_rings_out_first() {
    let plan = PolyPlan::new(&saw());
    let mut s = PolySynth::new(RATE);
    let by = Offsets::default();
    let (mut l, mut r) = (vec![0f32; 64], vec![0f32; 64]);
    let note = |velocity| Note {
        offset: 0,
        pitch: 60,
        velocity,
    };
    s.begin(&plan, std::iter::empty(), &by);
    s.process(&plan, [&mut l, &mut r], 64, &[note(0.9)], &[]);
    s.process(&plan, [&mut l, &mut r], 64, &[note(0.0)], &[]);
    while s.voices.iter().any(|v| v.on) {
        s.process(&plan, [&mut l, &mut r], 64, &[], &[]);
    }
    // The voice is gone; the decimators have a little left to say, then stop being run at all.
    s.process(&plan, [&mut l, &mut r], 64, &[], &[]);
    s.process(&plan, [&mut l, &mut r], 64, &[], &[]);
    assert_eq!(s.tail, 0);
    l.fill(1.0);
    s.process(&plan, [&mut l, &mut r], 64, &[], &[]);
    assert!(l.iter().all(|&v| v == 0.0), "and it still writes silence");
    // A note wakes it.
    s.process(&plan, [&mut l, &mut r], 64, &[note(0.9)], &[]);
    s.process(&plan, [&mut l, &mut r], 64, &[], &[]);
    assert!(rms(&l) > 0.0);
}

#[test]
fn the_decimator_stops_what_would_fold_back_and_passes_what_wouldnt() {
    let run = |hz: f32| {
        let mut down = Down::new();
        let mut x: Vec<f32> = (0..2048)
            .map(|i| (TAU * hz * i as f32 / 96_000.0).sin())
            .collect();
        let n = down.run(&mut x[..1024]);
        // The second block, past the start, through what the first left behind.
        let m = down.run(&mut x[1024..]);
        assert_eq!((n, m), (512, 512));
        rms(&x[1024 + 128..1024 + 512])
    };
    assert!((run(1000.0) - 0.707).abs() < 0.01, "{}", run(1000.0));
    assert!(run(40_000.0) < 0.001, "{}", run(40_000.0));
}

/// How much of real time a synth takes: `chord` notes held, 256 samples a block at 48 kHz.
fn share(patch: &Poly, chord: usize) -> f64 {
    let plan = PolyPlan::new(patch);
    let mut s = PolySynth::new(RATE);
    let by = Offsets::default();
    let notes: Vec<Note> = (0..chord)
        .map(|i| Note {
            offset: 0,
            pitch: [48, 55, 60, 64, 67, 72, 76, 79][i],
            velocity: 0.8,
        })
        .collect();
    let (mut l, mut r) = (vec![0f32; 256], vec![0f32; 256]);
    let blocks = 300;
    let mut took = std::time::Duration::ZERO;
    for i in 0..blocks {
        let now = if i == 0 { &notes[..] } else { &[] };
        let start = std::time::Instant::now();
        s.begin(&plan, std::iter::empty(), &by);
        s.process(&plan, [&mut l, &mut r], 256, now, &[]);
        took += start.elapsed();
    }
    took.as_secs_f64() / (blocks as f64 * 256.0 / RATE)
}

#[test]
#[ignore = "a benchmark: cargo test --release -p pina-engine poly_load -- --ignored --nocapture"]
fn poly_load() {
    println!(
        "{:<18}{:>8}{:>8}{:>8}",
        "patch", "idle", "1 note", "4 notes"
    );
    let mut worst = (0.0, "");
    let mut sum = 0.0;
    for (name, _, _) in POLY_PATCHES {
        let p = poly_patch(name).unwrap();
        let (idle, one, four) = (share(&p, 0), share(&p, 1), share(&p, 4));
        println!(
            "{name:<18}{:>7.2}%{:>7.2}%{:>7.2}%",
            idle * 100.0,
            one * 100.0,
            four * 100.0
        );
        sum += four;
        if four > worst.0 {
            worst = (four, name);
        }
    }
    println!(
        "mean 4 notes {:.2}%, worst {:.2}% ({})",
        sum / POLY_PATCHES.len() as f64 * 100.0,
        worst.0 * 100.0,
        worst.1
    );
}

#[test]
fn every_macro_on_every_patch_leaves_it_alone_down_and_moves_it_up() {
    let peak = |x: &[f32]| x.iter().fold(0f32, |m, s| m.max(s.abs()));
    let mut idle = Vec::new();
    for (name, ..) in POLY_PATCHES {
        let p = poly_patch(name).unwrap();
        assert_eq!(p.macros.clone().map(|n| n.is_empty()), [false; MACROS]);
        let mut made = p.clone();
        made.routes
            .retain(|r| !matches!(r.source, PolySource::Macro(_)));
        for o in 0..OSCS {
            for (k, f) in [(0, OscField::WarpA), (1, OscField::WarpB)] {
                if made.params[osc_param(o, f)] == 0.0 {
                    made.warps[o][k] = Warp::None;
                }
            }
        }
        let note = 60;
        let [a, _] = play(&p, note, 24_000, 48_000);
        let [b, _] = play(&made, note, 24_000, 48_000);
        let most = a
            .iter()
            .zip(&b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max);
        assert!(
            most < 0.002 * peak(&b).max(0.05),
            "{name}: its macros changed it by {most} while down"
        );
        for m in 0..MACROS {
            let mut up = p.clone();
            up.params[macro_param(m)] = 1.0;
            let [c, _] = play(&up, note, 24_000, 72_000);
            let [d, _] = play(&p, note, 24_000, 72_000);
            let moved = c
                .iter()
                .zip(&d)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0, f32::max);
            if moved <= 0.005 * peak(&d).max(0.05) {
                idle.push(format!("{name}: {}", p.macros[m]));
            }
        }
    }
    assert!(idle.is_empty(), "macros that do nothing: {idle:?}");
}

#[test]
fn a_drawn_wave_is_what_the_warps_make_of_it() {
    let t = table("Basic");
    let plain = drawn_wave(&t, 0.5, [Warp::None; 2], [0.0; 2], 0, 256);
    assert_eq!(plain.len(), 256);
    let at_nothing = drawn_wave(&t, 0.5, [Warp::Sync, Warp::PhaseDist], [0.0; 2], 0, 256);
    let same = plain
        .iter()
        .zip(&at_nothing)
        .all(|(a, b)| (a - b).abs() < 1e-4);
    assert!(same, "a warp at 0 changes nothing");
    let folded = drawn_wave(&t, 0.5, [Warp::Sync, Warp::None], [0.8, 0.0], 0, 256);
    let far = plain
        .iter()
        .zip(&folded)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(far > 0.2, "a warp up changes the wave: {far}");
}

#[test]
fn the_window_can_see_a_note_and_its_controllers() {
    let mut p = poly_patch("Wobble").unwrap();
    p.routes.push(PolyRoute {
        source: PolySource::Env(1),
        dest: PolyDest::Param(filter_param(0, FilterField::Cutoff) as u16),
        depth: 0.2,
        by: None,
    });
    let plan = PolyPlan::new(&p);
    let mut s = PolySynth::new(RATE);
    let (mut l, mut r) = (vec![0f32; 256], vec![0f32; 256]);
    let notes = [Note {
        offset: 0,
        pitch: 60,
        velocity: 0.75,
    }];
    let wheel = [Control {
        offset: 0,
        kind: ControlKind::Cc {
            number: 1,
            value: 0.5,
        },
    }];
    s.begin(&plan, std::iter::empty(), &Offsets::default());
    s.process(&plan, [&mut l[..], &mut r[..]], 256, &notes, &wheel);
    let live = s.live(&plan);
    let at = |w: PolySource| live[LIVE_SOURCES.iter().position(|s| *s == w).unwrap()];
    assert!(at(PolySource::Env(1)) > 0.0, "{live:?}");
    assert_eq!(at(PolySource::Velocity), 0.75);
    assert_eq!(at(PolySource::Wheel), 0.5);
    assert_eq!(live[POLY_LIVE - 1], 1.0);
}

#[test]
fn each_voice_follows_its_own_notes_pitch_slide_and_press() {
    let p = saw();
    let plan = PolyPlan::new(&p);
    let mut s = PolySynth::new(RATE);
    let by = Offsets::default();
    let on = |pitch| Note {
        offset: 0,
        pitch,
        velocity: 1.0,
    };
    let block = |s: &mut PolySynth, notes: &[Note], controls: &[Control]| {
        let (mut l, mut r) = (vec![0f32; 4096], vec![0f32; 4096]);
        s.begin(&plan, std::iter::empty(), &by);
        s.process(&plan, [&mut l[..], &mut r[..]], 4096, notes, controls);
        l
    };
    // A3 and E4 held; then E4 alone bent a fifth up to B4, slid and pressed (frequencies none
    // of A3's harmonics are near).
    block(&mut s, &[on(57), on(64)], &[]);
    let before = block(&mut s, &[], &[]);
    let level = |l: &[f32]| [220.0, 329.63, 493.88].map(|f| level_at(l, f));
    let at = |kind, value| Control {
        offset: 0,
        kind: ControlKind::Expression {
            pitch: 64,
            kind,
            value,
        },
    };
    let pressed = Control {
        offset: 0,
        kind: ControlKind::PolyPressure {
            pitch: 64,
            value: 0.8,
        },
    };
    let moves = [
        at(ExpressionKind::Tuning, 7.0),
        at(ExpressionKind::Brightness, 0.6),
        pressed,
    ];
    block(&mut s, &[], &moves);
    let after = block(&mut s, &[], &[]);
    let ([a3, e4, b4], [a3_, e4_, b4_]) = (level(&before), level(&after));
    assert!(
        e4_ < e4 * 0.4 && b4_ > e4 * 0.8,
        "E4 went to B4: {e4} {b4} → {e4_} {b4_}"
    );
    assert!((a3_ / a3 - 1.0).abs() < 0.2, "A3 stayed: {a3} → {a3_}");
    let live = s.live(&plan);
    let source = |w: PolySource| live[LIVE_SOURCES.iter().position(|s| *s == w).unwrap()];
    assert_eq!(source(PolySource::Slide), 0.6);
    assert_eq!(source(PolySource::Pressure), 0.8);
}
