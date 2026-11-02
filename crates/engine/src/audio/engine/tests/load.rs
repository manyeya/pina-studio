//! What the engine costs: not a test of right or wrong, a number to watch.

use super::*;
use crate::song::project::poly_patch;

/// A bar's chord on each of `tracks` synths, held for two bars so they overlap, as a song does.
fn synths(patch: &str, tracks: usize) -> Project {
    let bar = PPQ * 4;
    let chords = [
        [48, 55, 60, 64],
        [45, 52, 60, 64],
        [41, 48, 57, 65],
        [43, 50, 59, 67],
    ];
    project(
        (0..tracks)
            .map(|t| {
                let notes = (0..16)
                    .flat_map(|b| {
                        chords[b % 4]
                            .map(|p| note(b as i64 * bar as i64, bar * 2, p + 12 * (t % 2) as u8))
                    })
                    .collect();
                Track {
                    id: t as u64 + 1,
                    instrument: Some(Instrument::Poly(Box::new(poly_patch(patch).unwrap()))),
                    ..synth_track(&format!("{t}"), notes, bar * 18)
                }
            })
            .collect(),
    )
}

#[test]
#[ignore = "a benchmark: cargo test --release -p pina-engine eight_synths -- --ignored --nocapture"]
fn eight_synths() {
    let dir = scratch_dir("load");
    let helpers = helpers_wanted();
    println!(
        "{:<16}{:>8}{:>8}  {helpers} helpers",
        "8 tracks", "mean", "peak"
    );
    for patch in [
        "Init",
        "Reese",
        "Supersaw",
        "Trance Lead",
        "Warm Pad",
        "Pluck",
    ] {
        let (mut engine, _) = Engine::load(&synths(patch, 8), &dir, RATE).unwrap();
        engine.crew = Crew::new(helpers, MAX_BLOCK as f64 / RATE);
        engine.play();
        let (mut l, mut r) = ([0f32; MAX_BLOCK], [0f32; MAX_BLOCK]);
        let (mut took, mut most) = (0f64, 0f64);
        let blocks = 48_000 * 8 / MAX_BLOCK;
        for _ in 0..blocks {
            let start = Instant::now();
            engine.process([&mut l, &mut r], MAX_BLOCK);
            let t = start.elapsed().as_secs_f64();
            took += t;
            most = most.max(t);
        }
        let budget = MAX_BLOCK as f64 / RATE;
        println!(
            "{patch:<16}{:>7.0}%{:>7.0}%",
            took / blocks as f64 / budget * 100.0,
            most / budget * 100.0
        );
    }
}

/// Synths with sends before and after their faders, into an aux, and one whose EQ listens to
/// another: everything lanes share.
fn busy_song() -> Project {
    let mut song = synths("Supersaw", 6);
    song.auxes.push(crate::song::project::Aux {
        id: 900,
        bus: 1,
        name: "Room".into(),
        ..Default::default()
    });
    for (i, t) in song.tracks.iter_mut().enumerate() {
        t.volume_db = -3.0 * i as f32;
        if i < 3 {
            t.sends.push(AuxSend {
                bus: 1,
                level_db: -6.0,
                pre: i == 1,
            });
        }
    }
    song.tracks[5].eq = Some(crate::song::project::Eq {
        sidechain: Some(1),
        ..Default::default()
    });
    song
}

#[test]
fn sharing_tracks_out_changes_nothing_heard() {
    let dir = scratch_dir("crew");
    let song = busy_song();
    let play = |helpers: usize| {
        let (mut engine, _) = Engine::load(&song, &dir, RATE).unwrap();
        engine.crew = Crew::new(helpers, 0.01);
        engine.play();
        run(&mut engine, 48_000)
    };
    let alone = play(0);
    assert!(peak(&alone[0]) > 0.01, "it played");
    for _ in 0..3 {
        // To the bit: what each track makes is mixed in the same order whoever made it.
        assert!(play(3) == alone);
    }
}

#[test]
fn a_track_never_runs_alongside_one_it_hears() {
    // 2 hears 0, 3 hears 2; 1 hears nobody.
    let hears = [vec![], vec![], vec![Some(0)], vec![Some(2)]];
    let order = sidechain_order(&hears);
    let hearing = |o: usize, t: usize| hears[t].contains(&Some(o));
    let rounds = rounds(&order, hearing);
    assert_eq!(*rounds.last().unwrap(), 4);
    let round_of = |t: usize| {
        let at = order.iter().position(|&o| o == t).unwrap();
        rounds.iter().position(|&end| at < end).unwrap()
    };
    assert!(round_of(0) < round_of(2) && round_of(2) < round_of(3));
    // With nobody hearing anybody, it's one round.
    assert_eq!(super::rounds(&[0, 1, 2], |_, _| false), [3]);
}

#[test]
fn tracks_sharing_a_lane_are_never_worked_on_at_once() {
    // Tracks with no id of their own end up on one lane: wrong, but it mustn't be a data race.
    let mut song = synths("Init", 4);
    song.tracks.iter_mut().for_each(|t| t.id = 7);
    let dir = scratch_dir("shared-lane");
    let mut player = crate::audio::player::Player::silent(&song, &dir);
    player.sync(&song).unwrap();
    let (mut engine, _) = Engine::load(&song, &dir, RATE).unwrap();
    let a = &mut engine.arrangement;
    a.plans.iter_mut().for_each(|p| p.slot = 0);
    let slots: Vec<usize> = a.plans.iter().map(|p| p.slot).collect();
    assert_eq!(rounds(&a.order, |o, t| slots[o] == slots[t]), [1, 2, 3, 4]);
}

/// On the real output, in real time, its helpers real-time too, and silent: the master is down.
/// The percentage flatters neither way: the system slows the chip to just meet a deadline, so only
/// a late buffer means anything. Sixteen of these are too many for one core.
#[test]
#[ignore = "uses the audio device: cargo test --release -p pina-engine real_device -- --ignored --nocapture"]
fn eight_synths_on_the_real_device() {
    use crate::audio::player::Player;
    let dir = scratch_dir("device");
    let mut song = synths("Trance Lead", 16);
    song.master_volume_db = -144.0;
    let setup = AudioSetup::default();
    let mut player = Player::start(&song, &dir, &setup).unwrap();
    player.sync(&song).unwrap();
    player.play();
    let listen = |player: &Player, seconds: u64| {
        let (mut most, mut sum, mut n) = (0f32, 0f32, 0);
        for _ in 0..seconds * 50 {
            std::thread::sleep(Duration::from_millis(20));
            let load = player.load();
            if load > 0.0 {
                (most, sum, n) = (most.max(load), sum + load, n + 1);
            }
        }
        (sum / n.max(1) as f32 * 100.0, most * 100.0)
    };
    let (mean, most) = listen(&player, 6);
    println!(
        "{} samples: mean {mean:.0}%, peak {most:.0}%",
        player.buffer()
    );
    assert!(most < 100.0, "a buffer came late: {most:.0}%");
    // Another buffer, while it plays, and the song plays on.
    for size in buffer_sizes(None)
        .into_iter()
        .filter(|&b| b == 256 || b == 1024)
    {
        let setup = AudioSetup {
            buffer: Some(size),
            ..Default::default()
        };
        player.set_output(&setup).unwrap();
        let before = player.position();
        let (mean, most) = listen(&player, 3);
        assert!(player.position() > before, "it went on playing");
        assert_eq!(player.buffer(), size);
        println!("{size} samples: mean {mean:.0}%, peak {most:.0}%");
    }
    player.set_output(&AudioSetup::default()).unwrap();
}

/// A file heard from part way in plays out on the real output, for as long as what's left of it:
/// a second of it from 0.75, so a quarter of a second. Sixty dB down: nothing to hear.
#[test]
#[ignore = "uses the audio device: cargo test --release -p pina-engine real_device -- --ignored --nocapture"]
fn a_file_heard_part_way_in_on_the_real_device() {
    use crate::audio::player::Player;
    let dir = scratch_dir("heard");
    let song = crate::song::project::Project::new(48_000);
    let mut player = Player::start(&song, &dir, &AudioSetup::default()).unwrap();
    let rate = player.rate();
    let data = Arc::new(AudioData::mono(vec![0.1; rate as usize]));
    let before = player.auditions_ended();
    let from = std::time::Instant::now();
    player.audition_from(data, 0.75, -60.0).unwrap();
    while player.auditions_ended() == before && from.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(5));
        player.collect();
    }
    let took = from.elapsed().as_secs_f32();
    println!("played out in {took:.3} s");
    assert!(player.auditions_ended() > before, "it never played out");
    assert!((0.2..0.6).contains(&took), "{took:.3} s");
}
