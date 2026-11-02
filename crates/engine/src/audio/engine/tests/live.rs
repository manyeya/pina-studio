//! The live engine: its messages, the cycle, auditions and the metronome.

use super::*;

#[test]
fn the_live_engine_follows_its_messages() {
    let (mut tx, rx) = rtrb::RingBuffer::new(16);
    let (gtx, mut garbage) = rtrb::RingBuffer::new(16);
    let (mut live, live_rx) = rtrb::RingBuffer::new(16);
    let (played_tx, mut played) = rtrb::RingBuffer::new(16);
    let shared = Arc::new(Shared::new());
    let link = Link {
        rx,
        garbage: gtx,
        shared: shared.clone(),
        live: live_rx,
        played: played_tx,
    };
    let mut engine = Engine::live(RATE, link);
    let p = project(vec![synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8)]);
    let t = &p.tracks[0];
    let (lane, _, _) = make_lane(
        t.instrument.as_ref(),
        &[],
        &mut Vec::new(),
        RATE,
        &mut Modules::new(),
        Path::new("."),
    )
    .unwrap();
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(&p, RATE, &[(7, 0)], &[], None, &mut files);
    for m in [
        Msg::Insert(7, Box::new(lane)),
        Msg::Arrange(Box::new(arrangement)),
        Msg::Play,
    ] {
        assert!(tx.push(m).is_ok());
    }
    assert!(peak(&run(&mut engine, MAX_BLOCK)[0]) > 0.0);
    assert!(shared.playing());
    assert_eq!(shared.position(), MAX_BLOCK as u64);
    assert!(shared.take_meter(7)[0] > 0.0 && shared.take_meter(MASTER)[1] > 0.0);
    // The empty arrangement it started with comes back.
    assert!(matches!(garbage.pop(), Ok(Garbage::Arrangement(_))));

    // A live note on the armed lane plays, and comes back as played at the playhead.
    assert!(tx.push(Msg::Stop).is_ok());
    run(&mut engine, RELEASE + MAX_BLOCK);
    shared.arm(Some(7));
    assert!(
        live.push(LiveNote {
            pitch: 72,
            velocity: 100
        })
        .is_ok()
    );
    assert!(peak(&run(&mut engine, MAX_BLOCK)[0]) > 0.0);
    let at = MAX_BLOCK as u64;
    assert_eq!(
        played.pop(),
        Ok(Played {
            at,
            pitch: 72,
            velocity: 100
        })
    );
    assert!(
        tx.push(Msg::Note {
            slot: 7,
            pitch: 72,
            velocity: 0
        })
        .is_ok()
    );

    assert!(tx.push(Msg::Remove(7)).is_ok());
    assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
    assert!(matches!(garbage.pop(), Ok(Garbage::Lane(_))));
}

#[test]
fn the_cycle_loops_playback() {
    let mut p = project(vec![synth_track("a", vec![note(0, PPQ * 4, 69)], PPQ * 4)]);
    p.cycle = Some(crate::song::project::Cycle {
        start: PPQ,
        end: PPQ * 2,
        off: false,
    });
    let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
    engine.locate(BEAT as u64);
    engine.play();
    run(&mut engine, BEAT + 1000);
    assert_eq!(engine.position(), BEAT as u64 + 1000);
    // After it, it goes back to its start rather than playing on past it.
    let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
    engine.locate(3 * BEAT as u64);
    engine.play();
    run(&mut engine, 1000);
    assert!((BEAT as u64..2 * BEAT as u64).contains(&engine.position()));
    // Before the cycle, it plays on into it; switched off, it plays straight through.
    p.cycle.as_mut().unwrap().off = true;
    let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
    engine.locate(BEAT as u64);
    engine.play();
    run(&mut engine, BEAT + 1000);
    assert_eq!(engine.position(), 2 * BEAT as u64 + 1000);
}

#[test]
fn an_audition_plays_over_the_mix_and_lets_go_at_its_end() {
    let (mut tx, rx) = rtrb::RingBuffer::new(16);
    let (gtx, mut garbage) = rtrb::RingBuffer::new(16);
    let (_live, live_rx) = rtrb::RingBuffer::new(16);
    let (played_tx, _played) = rtrb::RingBuffer::new(16);
    let link = Link {
        rx,
        garbage: gtx,
        shared: Arc::new(Shared::new()),
        live: live_rx,
        played: played_tx,
    };
    let mut engine = Engine::live(RATE, link);
    // A thousand samples of 0.5, an octave up: five hundred samples of it, stopped or not.
    let data = Arc::new(AudioData::mono(vec![0.5; 1000]));
    let a = Audition::new(data, 12.0, 0.0);
    assert!(tx.push(Msg::Audition(Some(Box::new(a)))).is_ok());
    let [l, r] = run(&mut engine, MAX_BLOCK * 2);
    assert!((l[10] - 0.5).abs() < 1e-6 && (r[490] - 0.5).abs() < 1e-6);
    assert_eq!(peak(&l[502..]), 0.0);
    // Done, it goes back to the main thread to be dropped.
    run(&mut engine, MAX_BLOCK);
    assert!(matches!(garbage.pop(), Ok(Garbage::Audition(_))));
}

#[test]
fn an_audition_starts_where_its_asked_to() {
    let (mut tx, rx) = rtrb::RingBuffer::new(16);
    let (gtx, _garbage) = rtrb::RingBuffer::new(16);
    let (_live, live_rx) = rtrb::RingBuffer::new(16);
    let (played_tx, _played) = rtrb::RingBuffer::new(16);
    let link = Link {
        rx,
        garbage: gtx,
        shared: Arc::new(Shared::new()),
        live: live_rx,
        played: played_tx,
    };
    let mut engine = Engine::live(RATE, link);
    // Silence, then a step: from half a second in, it's heard at once.
    let half = (RATE / 2.0) as usize;
    let data = Arc::new(AudioData::mono(
        (0..RATE as usize)
            .map(|i| if i < half { 0.0 } else { 0.5 })
            .collect(),
    ));
    let a = Audition::new(data, 0.0, 0.0).from(0.5, RATE);
    assert!(tx.push(Msg::Audition(Some(Box::new(a)))).is_ok());
    let [l, _] = run(&mut engine, MAX_BLOCK);
    assert!((l[0] - 0.5).abs() < 1e-6, "{}", l[0]);
}

#[test]
fn the_metronome_clicks_on_the_beat_while_playing() {
    let (mut tx, rx) = rtrb::RingBuffer::new(16);
    let (gtx, _garbage) = rtrb::RingBuffer::new(16);
    let (_live, live_rx) = rtrb::RingBuffer::new(16);
    let (played_tx, _played) = rtrb::RingBuffer::new(16);
    let shared = Arc::new(Shared::new());
    let link = Link {
        rx,
        garbage: gtx,
        shared: shared.clone(),
        live: live_rx,
        played: played_tx,
    };
    let mut engine = Engine::live(RATE, link);
    assert!(tx.push(Msg::Play).is_ok());
    assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
    shared.set_metronome(true);
    // The next beat, at 120 bpm, is half a second in.
    let [l, _] = run(&mut engine, BEAT);
    let at = BEAT - MAX_BLOCK;
    assert_eq!(peak(&l[..at - 1]), 0.0);
    assert!(peak(&l[at..at + 500]) > 0.1);
    engine.note_load(Instant::now() - Duration::from_millis(1), MAX_BLOCK);
    assert!(shared.take_load() > 0.0);
}

#[test]
fn stacked_built_ins_get_their_running_state_by_message() {
    use crate::song::project::Unit;
    let (mut tx, rx) = rtrb::RingBuffer::new(16);
    let (gtx, mut garbage) = rtrb::RingBuffer::new(16);
    let (_live, live_rx) = rtrb::RingBuffer::new(16);
    let (played_tx, _played) = rtrb::RingBuffer::new(16);
    let shared = Arc::new(Shared::new());
    let link = Link {
        rx,
        garbage: gtx,
        shared: shared.clone(),
        live: live_rx,
        played: played_tx,
    };
    let mut engine = Engine::live(RATE, link);
    let mut p = project(vec![synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8)]);
    let mut hot = crate::song::project::Saturator::default();
    hot.bands[0].drive_db = 24.0;
    p.tracks[0].more = vec![Unit::Sat(hot), Unit::Sat(hot)];
    let t = &p.tracks[0];
    let (lane, _, _) = make_lane(
        t.instrument.as_ref(),
        &[],
        &mut Vec::new(),
        RATE,
        &mut Modules::new(),
        Path::new("."),
    )
    .unwrap();
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(&p, RATE, &[(3, 0)], &[], None, &mut files);
    // One strip at first, then another after it, the first kept.
    let strip = || Some(crate::dsp::Strip::new(RATE));
    for m in [
        Msg::Insert(3, Box::new(lane)),
        Msg::Strips(Box::new(Strips::new(3, vec![strip()], 0))),
        Msg::Strips(Box::new(Strips::new(3, vec![None, strip()], 1))),
        Msg::Arrange(Box::new(arrangement)),
        Msg::Play,
    ] {
        assert!(tx.push(m).is_ok());
    }
    use crate::audio::engine::SCOPE_LEN;
    use std::sync::atomic::Ordering;
    // The second one's window open: what it plays is there for its scope.
    shared.fx_more.store(1, Ordering::Relaxed);
    let [l, _] = run(&mut engine, MAX_BLOCK * 4);
    let wrote = shared.scope_written.load(Ordering::Relaxed);
    assert!(wrote >= MAX_BLOCK, "{wrote}");
    let last = shared.scope[0][(wrote - 1) % SCOPE_LEN].load(Ordering::Relaxed);
    assert_eq!(f32::from_bits(last), l[MAX_BLOCK * 4 - 1]);
    // Both saturators ran: the sine is squared off, well under its dry peak.
    let mut dry = p.clone();
    dry.tracks[0].more.clear();
    let clean = peak(&render(&dry, Path::new("."), MAX_BLOCK * 4)[0][MAX_BLOCK..]);
    let wet = peak(&l[MAX_BLOCK..]);
    assert!(wet < clean * 0.6, "{clean} {wet}");
    // The lists they came in go back to be dropped.
    let mut back = 0;
    while let Ok(g) = garbage.pop() {
        back += matches!(g, Garbage::Strips(_)) as usize;
    }
    assert_eq!(back, 2);
}
