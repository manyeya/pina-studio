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
    assert!(shared.take_load() > 0.0);
}
