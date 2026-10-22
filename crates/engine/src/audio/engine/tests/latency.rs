//! Latency: delay compensation, linear phase waited for, sidechains in the same block, the cycle read ahead.

use super::*;

#[test]
fn compensation_lines_everything_up() {
    let c = compensate(&[0, 100, 30], &[50, 0]);
    assert_eq!(
        c,
        Compensation {
            lookahead: 150,
            tracks: vec![100, 0, 70],
            direct: 50,
            auxes: vec![0, 50]
        }
    );
    assert_eq!(compensate(&[], &[]).lookahead, 0);
}

#[test]
fn delay_lines_delay() {
    let mut d = Delay::new();
    let (mut l, mut r) = ([1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]);
    d.process(&mut l, &mut r, 2);
    assert_eq!((l, r), ([0.0, 0.0, 1.0, 2.0], [0.0, 0.0, 5.0, 6.0]));
}

#[test]
fn delay_compensation_waits_for_the_slowest_track() {
    // Two synth tracks, left and right; say the left one's plugins are 100 samples late.
    // The right one is delayed to match, and the engine reads 100 samples ahead. The synth
    // isn't really late, so the left is heard 100 samples early and the right on time.
    let mut a = synth_track("a", vec![note(PPQ as i64, PPQ, 69)], PPQ * 2);
    a.pan = -1.0;
    let mut b = a.clone();
    b.pan = 1.0;
    let p = project(vec![a, b]);
    let mut lanes = Vec::new();
    for t in &p.tracks {
        let (lane, _, _) = make_lane(
            t.instrument.as_ref(),
            &[],
            RATE,
            &mut Modules::new(),
            Path::new("."),
        )
        .unwrap();
        lanes.push(Some(Box::new(lane)));
    }
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(&p, RATE, &[(0, 100), (1, 0)], &[], None, &mut files);
    let mut engine = Engine::new(RATE, arrangement, lanes);
    engine.play();
    let [l, r] = run(&mut engine, BEAT * 2);
    let onset = |s: &[f32]| s.iter().position(|x| x.abs() > 0.0).unwrap();
    assert_eq!(onset(&r) - onset(&l), 100);
    assert!((onset(&r) as i64 - BEAT as i64).abs() <= 1);
}

#[test]
fn an_eq_in_linear_phase_is_waited_for() {
    // Two synth tracks, left and right, the left's EQ in linear phase (some 2,300 samples
    // late): the right waits for it, and the engine reads ahead, so both are heard on time.
    // The master's EQ in natural phase, likewise.
    let mut a = synth_track("a", vec![note(PPQ as i64, PPQ, 69)], PPQ * 2);
    a.pan = -1.0;
    let mut b = a.clone();
    b.pan = 1.0;
    let plain = project(vec![a.clone(), b.clone()]);
    a.eq = Some(Eq {
        processing: crate::song::project::Processing::Linear,
        resolution: 0,
        ..Eq::default()
    });
    let mut p = project(vec![a, b]);
    p.master_eq = Some(Eq {
        processing: crate::song::project::Processing::Natural,
        ..Eq::default()
    });
    assert!(crate::dsp::eq::latency(p.tracks[0].eq.as_ref().unwrap()) > 2_000);
    let onset = |s: &[f32]| s.iter().position(|x| x.abs() > 1e-3).unwrap() as i64;
    let [l, r] = render(&p, Path::new("."), BEAT * 2);
    let [on_time, _] = render(&plain, Path::new("."), BEAT * 2);
    assert!(
        (onset(&l) - onset(&r)).abs() <= 1,
        "{} {}",
        onset(&l),
        onset(&r)
    );
    assert!((onset(&r) - onset(&on_time)).abs() <= 1, "{}", onset(&r));
}

#[test]
fn a_sidechain_hears_its_source_in_the_same_block() {
    // The track hearing the sidechain comes first, so the source must be worked out before it.
    let mut source = synth_track("kick", vec![note(PPQ as i64, PPQ, 45)], PPQ * 4);
    source.id = 7;
    let mut hears = synth_track("bass", vec![], PPQ * 4);
    hears.inserts = vec![PluginRef {
        sidechain: Some(7),
        ..Default::default()
    }];
    let probe = Probe::default();
    let mut engine = probed(&project(vec![hears, source]), &probe);
    engine.play();
    run(&mut engine, BEAT * 3);
    let from = probe
        .sidechain_from
        .lock()
        .unwrap()
        .expect("the sidechain was silent");
    assert!((BEAT as u64..BEAT as u64 + 64).contains(&from), "{from}");
}

#[test]
fn sidechain_sources_are_worked_out_first() {
    assert_eq!(
        sidechain_order(&[vec![Some(2)], vec![], vec![Some(1)]]),
        [1, 2, 0]
    );
    // A loop keeps everyone, each once.
    let mut order = sidechain_order(&[vec![Some(1)], vec![Some(0)]]);
    order.sort();
    assert_eq!(order, [0, 1]);
}

#[test]
fn the_cycle_wraps_what_plugins_read_ahead() {
    // A track 64 samples late (as a plugin's latency would make it): the engine reads 64
    // ahead, so each pass of the cycle must read its start before the last one's heard end.
    let mut p = project(vec![synth_track("a", vec![note(0, 240, 69)], PPQ)]);
    p.cycle = Some(crate::song::project::Cycle {
        start: 0,
        end: PPQ,
        off: false,
    });
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(&p, RATE, &[(0, 64)], &[], None, &mut files);
    let lane = make_lane(
        Some(&Instrument::Synth),
        &[],
        RATE,
        &mut Modules::new(),
        Path::new("."),
    )
    .unwrap()
    .0;
    let mut engine = Engine::new(RATE, arrangement, vec![Some(Box::new(lane))]);
    engine.play();
    let [l, _] = run(&mut engine, 2 * BEAT);
    // The note sounds again as the second pass begins, and not long before.
    assert!(peak(&l[BEAT - 64..BEAT + 2000]) > 0.1);
    assert!(peak(&l[BEAT / 2..BEAT - 100]) < 1e-3);
}
