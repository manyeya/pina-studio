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
        moves: rtrb::RingBuffer::new(16).1,
        heard: rtrb::RingBuffer::new(16).0,
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
            velocity: 100,
            release: 0,
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
            velocity: 100,
            release: 0,
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
        moves: rtrb::RingBuffer::new(16).1,
        heard: rtrb::RingBuffer::new(16).0,
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
        moves: rtrb::RingBuffer::new(16).1,
        heard: rtrb::RingBuffer::new(16).0,
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
        moves: rtrb::RingBuffer::new(16).1,
        heard: rtrb::RingBuffer::new(16).0,
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

/// An engine of its own, its shared state, and the way to send it messages.
fn linked() -> (Engine, Arc<Shared>, rtrb::Producer<Msg>) {
    let (tx, rx) = rtrb::RingBuffer::new(16);
    let shared = Arc::new(Shared::new());
    let link = Link {
        rx,
        garbage: rtrb::RingBuffer::new(16).0,
        shared: shared.clone(),
        live: rtrb::RingBuffer::new(16).1,
        played: rtrb::RingBuffer::new(16).0,
        moves: rtrb::RingBuffer::new(16).1,
        heard: rtrb::RingBuffer::new(16).0,
    };
    (Engine::live(RATE, link), shared, tx)
}

#[test]
fn counting_in_clicks_a_bar_while_the_playhead_waits_then_plays() {
    let (mut engine, shared, mut tx) = linked();
    // The click off: counting in clicks anyway.
    assert!(!shared.metronome());
    assert!(tx.push(Msg::CountIn(1, true)).is_ok());
    let _ = run(&mut engine, MAX_BLOCK);
    assert!(engine.counting());
    // A bar of 4 at 120 bpm: four beats half a second apart, the playhead at 0 all along.
    let [l, _] = run(&mut engine, BEAT * 4);
    assert_eq!(engine.position, 0);
    for b in 0..4 {
        let at = b * BEAT;
        assert!(peak(&l[at..at + 300]) > 0.05, "beat {b}");
        assert!(
            peak(&l[at + 2000..at + BEAT - 10]) < 1e-4,
            "quiet after beat {b}"
        );
    }
    // Counted in: playing, the playhead moving from the sample the count ended on.
    let _ = run(&mut engine, MAX_BLOCK);
    assert!(!engine.counting() && engine.playing);
    assert!(engine.position > 0 && engine.position <= MAX_BLOCK as u64);
    // Heard only (Settings' Hear it): the click, then still stopped; and a stop ends a count.
    let (mut engine, _, mut tx) = linked();
    assert!(tx.push(Msg::CountIn(1, false)).is_ok());
    let _ = run(&mut engine, MAX_BLOCK);
    assert!(peak(&run(&mut engine, BEAT * 4)[0]) > 0.05);
    assert!(!engine.counting() && !engine.playing);
    assert!(tx.push(Msg::CountIn(2, true)).is_ok());
    let _ = run(&mut engine, MAX_BLOCK);
    assert!(tx.push(Msg::Stop).is_ok());
    let _ = run(&mut engine, BEAT * 9);
    assert!(
        !engine.counting() && !engine.playing,
        "stopped while counting: no play after"
    );
}

#[test]
fn the_click_sounds_as_its_style_says_and_only_when_let() {
    let (mut engine, shared, mut tx) = linked();
    assert!(tx.push(Msg::Play).is_ok());
    let _ = run(&mut engine, MAX_BLOCK);
    shared.set_metronome(true);
    // Twice a beat: one between the beats too.
    shared.click_style.store(
        pack_click(ClickStyle {
            every: 2,
            ..Default::default()
        }),
        Ordering::Relaxed,
    );
    let [l, _] = run(&mut engine, BEAT);
    let half = BEAT / 2 - MAX_BLOCK;
    assert!(peak(&l[half..half + 300]) > 0.03, "between the beats");
    // Its level down 40 dB: barely there.
    shared.click_style.store(
        pack_click(ClickStyle {
            level_db: -40.0,
            ..Default::default()
        }),
        Ordering::Relaxed,
    );
    let [l, _] = run(&mut engine, BEAT);
    // Past the last one's ring, struck as it was.
    assert!(peak(&l[3000..]) < 0.01 && peak(&l[3000..]) > 0.0);
    // Not let (to click only while recording, and nothing is): silent.
    shared
        .click_style
        .store(pack_click(ClickStyle::default()), Ordering::Relaxed);
    shared.click_gate.store(false, Ordering::Relaxed);
    assert_eq!(peak(&run(&mut engine, BEAT)[0]), 0.0);
    assert_eq!(
        unpack_click(pack_click(ClickStyle {
            level_db: -12.0,
            sound: 2,
            every: 4,
            accent: false
        })),
        ClickStyle {
            level_db: -12.0,
            sound: 2,
            every: 4,
            accent: false
        }
    );
}

#[test]
fn a_test_tone_plays_on_the_side_asked() {
    let (mut engine, shared, _tx) = linked();
    shared.tone.store(1, Ordering::Relaxed);
    let [l, r] = run(&mut engine, BEAT);
    assert!(peak(&l) > 0.1 && peak(&r) == 0.0);
    // Then it ends, a second and a half on.
    let _ = run(&mut engine, BEAT * 2);
    assert_eq!(peak(&run(&mut engine, BEAT)[0]), 0.0);
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
        moves: rtrb::RingBuffer::new(16).1,
        heard: rtrb::RingBuffer::new(16).0,
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

#[test]
fn the_synth_window_hears_its_synth_and_moves_its_wheels() {
    use crate::dsp::poly::{LIVE_SOURCES, POLY_LIVE};
    use crate::song::project::{Instrument, PolySource, poly_patch};
    use std::sync::atomic::Ordering;
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
        moves: rtrb::RingBuffer::new(16).1,
        heard: rtrb::RingBuffer::new(16).0,
    };
    let mut engine = Engine::live(RATE, link);
    let mut p = project(vec![synth_track("a", vec![], PPQ * 8)]);
    p.tracks[0].instrument = poly_patch("Wobble").map(|s| Instrument::Poly(Box::new(s)));
    let (lane, _, _) = make_lane(
        p.tracks[0].instrument.as_ref(),
        &[],
        &mut Vec::new(),
        RATE,
        &mut Modules::new(),
        Path::new("."),
    )
    .unwrap();
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(&p, RATE, &[(3, 0)], &[], None, &mut files);
    for m in [
        Msg::Insert(3, Box::new(lane)),
        Msg::Arrange(Box::new(arrangement)),
        Msg::Note {
            slot: 3,
            pitch: 48,
            velocity: 100,
        },
        Msg::Control {
            slot: 3,
            kind: crate::plugin::ControlKind::Cc {
                number: 1,
                value: 0.5,
            },
        },
    ] {
        assert!(tx.push(m).is_ok());
    }
    // Watched, it says where its sources are and keeps what it played.
    shared.poly_slot.store(3, Ordering::Relaxed);
    let [l, _] = run(&mut engine, MAX_BLOCK * 4);
    assert!(peak(&l) > 0.0);
    let live: Vec<f32> = shared
        .poly_live
        .iter()
        .map(|v| f32::from_bits(v.load(Ordering::Relaxed)))
        .collect();
    let at = |s: PolySource| live[LIVE_SOURCES.iter().position(|x| *x == s).unwrap()];
    assert_eq!(at(PolySource::Wheel), 0.5, "the wheel reached it");
    assert!(at(PolySource::Env(0)) > 0.0, "{live:?}");
    assert_eq!(live[POLY_LIVE - 1], 1.0, "one voice");
    let wrote = shared.poly_written.load(Ordering::Relaxed);
    assert!(wrote >= MAX_BLOCK * 3, "{wrote}");
    // Not watched, it keeps nothing.
    shared.poly_slot.store(usize::MAX, Ordering::Relaxed);
    run(&mut engine, MAX_BLOCK);
    assert_eq!(shared.poly_written.load(Ordering::Relaxed), wrote);
}

#[test]
fn what_is_heard_now_is_read_from_when_each_buffer_plays() {
    let shared = Shared::new();
    assert_eq!(shared.heard_now(RATE), None, "nothing heard yet");
    let ms = 1_000_000;
    // The clock starts at the first read: far enough on to look back 50 ms.
    clock();
    std::thread::sleep(std::time::Duration::from_millis(60));
    let now = clock();
    // A buffer from 1000 heard 5 ms ago; the next, from 1512, not heard for another 20 ms.
    shared.note_heard(1000, 512, now - 5 * ms);
    shared.note_heard(1512, 512, now + 20 * ms);
    let heard = shared.heard_now(RATE).unwrap();
    let into = (0.005 * RATE) as u64;
    assert!(heard >= 1000 + into && heard < 1000 + into + 100, "{heard}");
    // Never past the end of the buffer being heard.
    shared.note_heard(4000, 64, now - 50 * ms);
    assert_eq!(shared.heard_now(RATE), Some(4064));
    // Played again (or moved): what's left of before isn't now. Till the new play's first is
    // heard, the playhead waits where it starts.
    shared.new_play();
    assert_eq!(shared.heard_now(RATE), None);
    shared.note_heard(9000, 512, now + 30 * ms);
    shared.note_heard(9512, 512, now + 40 * ms);
    assert_eq!(shared.heard_now(RATE), Some(9000));
    // Heard later than the device says: the playhead later by as much.
    shared.note_heard(20_000, 4800, now - 50 * ms);
    let on_time = shared.heard_now(RATE).unwrap();
    shared
        .heard_later
        .store(20 * ms as i64, std::sync::atomic::Ordering::Relaxed);
    let later = shared.heard_now(RATE).unwrap();
    let by = on_time - later;
    assert!((by as f64 - 0.020 * RATE).abs() < 0.004 * RATE, "{by}");
}

#[test]
fn each_block_is_heard_in_turn_and_the_cycle_starts_again_where_it_loops() {
    // A cycle round 0..1000 played from 900: a 512 buffer heard at `at` is 100 samples to the
    // end, then the cycle's start.
    let (mut engine, shared, _tx) = linked();
    engine.arrangement.cycle = Some((0, 1000));
    engine.position = 900;
    engine.playing = true;
    let at = clock() + 1_000_000_000;
    engine.note_heard(at);
    let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
    engine.process([&mut l, &mut r], 512);
    let k = shared.heard_at.load(std::sync::atomic::Ordering::Relaxed);
    let entry = |k: usize| {
        [0, 1, 2]
            .map(|f| shared.heard[k % HEARD_KEPT][f].load(std::sync::atomic::Ordering::Relaxed))
    };
    let (before, after) = (entry(k + HEARD_KEPT - 1), entry(k));
    assert_eq!((before[0], before[1], before[2]), (900, 100, at));
    let later = at + (100.0 * 1e9 / RATE) as u64;
    assert_eq!((after[0], after[1], after[2]), (0, 412, later));
}
