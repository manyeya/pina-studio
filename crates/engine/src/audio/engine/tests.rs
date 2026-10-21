use super::*;
use crate::song::project::{AuxSend, Note as MidiNote, PPQ, Track};

const RATE: f64 = 48_000.0;
const BEAT: usize = 24_000; // samples per beat at 120 bpm
const RELEASE: usize = 2_400; // the synth's 50 ms

fn synth_track(name: &str, notes: Vec<MidiNote>, length: u64) -> Track {
    Track {
        name: name.into(),
        instrument: Some(Instrument::Synth),
        regions: vec![Region {
            start: 0,
            length,
            content: Content::Midi {
                notes,
                controls: Vec::new(),
            },
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn note(start: i64, length: u64, pitch: u8) -> MidiNote {
    MidiNote {
        start,
        length,
        pitch,
        velocity: 127,
    }
}

fn run(engine: &mut Engine, frames: usize) -> [Vec<f32>; 2] {
    let mut out = [vec![0f32; frames], vec![0f32; frames]];
    for at in (0..frames).step_by(MAX_BLOCK) {
        let n = MAX_BLOCK.min(frames - at);
        let [l, r] = &mut out;
        engine.process([&mut l[at..], &mut r[at..]], n);
    }
    out
}

fn render(project: &Project, dir: &Path, frames: usize) -> [Vec<f32>; 2] {
    let (mut engine, _) = Engine::load(project, dir, RATE).unwrap();
    engine.play();
    run(&mut engine, frames)
}

fn project(tracks: Vec<Track>) -> Project {
    Project {
        tracks,
        ..Project::new(RATE as u32)
    }
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0, |m, x| m.max(x.abs()))
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pina-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn notes_start_on_their_beat_and_play_past_the_region_end() {
    // A beat-long note from beat 2 in a region that ends halfway through it, and a note
    // starting after the region ends, which doesn't play.
    let notes = vec![note(PPQ as i64, PPQ, 69), note(PPQ as i64 * 3, PPQ, 69)];
    let p = project(vec![synth_track("a", notes, PPQ * 3 / 2)]);
    let [l, _] = render(&p, Path::new("."), BEAT * 4);
    assert_eq!(peak(&l[..BEAT + 1]), 0.0);
    assert!(peak(&l[BEAT..BEAT + 1000]) > 0.1);
    assert!(peak(&l[BEAT * 2 - 1000..BEAT * 2]) > 0.1);
    assert_eq!(peak(&l[BEAT * 2 + RELEASE + 10..]), 0.0);
}

#[test]
fn volume_and_pan() {
    let mut t = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
    let loud = render(&project(vec![t.clone()]), Path::new("."), BEAT);
    t.volume_db = -20.0 * 2f32.log10(); // half
    t.pan = -1.0;
    let [l, r] = render(&project(vec![t]), Path::new("."), BEAT);
    assert!((peak(&l) / peak(&loud[0]) - 0.5).abs() < 1e-4);
    assert_eq!(peak(&r), 0.0);
}

#[test]
fn pan_laws_and_stereo_pan_place_each_side() {
    let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-3);
    let h = std::f32::consts::FRAC_1_SQRT_2;
    // Balance, as it was: the middle untouched; and by the laws.
    assert!(close(
        pan_matrix(PanMode::Balance, PanLaw::Flat, 0.0, 1.0, 1.0),
        [1.0, 0.0, 0.0, 1.0]
    ));
    assert!(close(
        pan_matrix(PanMode::Balance, PanLaw::Minus3, 0.0, 1.0, 1.0),
        [h, 0.0, 0.0, h]
    ));
    assert!(close(
        pan_matrix(PanMode::Balance, PanLaw::Minus3Compensated, -1.0, 1.0, 1.0),
        [std::f32::consts::SQRT_2, 0.0, 0.0, 0.0]
    ));
    assert!(close(
        pan_matrix(PanMode::Balance, PanLaw::Minus6, 0.0, 1.0, 1.0),
        [0.5, 0.0, 0.0, 0.5]
    ));
    // Stereo pan: full spread leaves the sides where they were; none folds them to the
    // middle; panned hard left with no spread, both sides go left.
    assert!(close(
        pan_matrix(PanMode::Stereo, PanLaw::Flat, 0.0, 1.0, 1.0),
        [1.0, 0.0, 0.0, 1.0]
    ));
    assert!(close(
        pan_matrix(PanMode::Stereo, PanLaw::Flat, 0.0, 0.0, 1.0),
        [h, h, h, h]
    ));
    assert!(close(
        pan_matrix(PanMode::Stereo, PanLaw::Flat, -1.0, 0.0, 1.0),
        [1.0, 1.0, 0.0, 0.0]
    ));
}

#[test]
fn binaural_pan_puts_the_far_ear_later_and_quieter_and_a_muted_region_is_silent() {
    let mut t = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
    t.pan = -0.5; // 90° left
    t.pan_mode = PanMode::Binaural;
    let [l, r] = render(&project(vec![t.clone()]), Path::new("."), BEAT);
    let first = |x: &[f32]| x.iter().position(|s| s.abs() > 1e-4).unwrap();
    assert!(peak(&l) > peak(&r) && peak(&r) > 0.0);
    assert!(first(&r) > first(&l), "{} {}", first(&l), first(&r));
    t.regions[0].mute = true;
    let [l, _] = render(&project(vec![t]), Path::new("."), BEAT);
    assert_eq!(peak(&l), 0.0);
}

#[test]
fn an_lfo_moves_the_volume_and_a_follower_ducks_one_track_under_another() {
    use crate::dsp::modulate::{LfoShape, ModRate};
    // A square LFO a beat long, 24 dB down while high: the first half of each beat quiet.
    let mut pad = synth_track("pad", vec![note(0, PPQ * 4, 60)], PPQ * 4);
    pad.modulators = vec![Modulator {
        kind: ModKind::Lfo {
            shape: LfoShape::Square,
            rate: ModRate::Beats(1.0),
            phase: 0.0,
            unipolar: false,
        },
        routes: vec![ModRoute {
            target: ModTarget::Volume,
            depth: -24.0,
        }],
    }];
    let [l, _] = render(&project(vec![pad.clone()]), Path::new("."), BEAT * 2);
    let (low, high) = (
        peak(&l[BEAT + 3000..BEAT + 9000]),
        peak(&l[BEAT + 15000..BEAT + 21000]),
    );
    assert!(high > low * 10.0, "{low} {high}");

    // The pad follows a kick: ducked just after it hits, back up a beat on.
    let kick = Track {
        id: 7,
        ..synth_track("kick", vec![note(0, PPQ / 8, 36)], PPQ * 4)
    };
    let mut pad = Track {
        id: 8,
        ..synth_track("pad", vec![note(0, PPQ * 4, 72)], PPQ * 4)
    };
    let both = |pad: &Track| {
        let [l, _] = render(
            &project(vec![kick.clone(), pad.clone()]),
            Path::new("."),
            BEAT * 2,
        );
        // After the kick's note and its release: the pad alone.
        (peak(&l[6_500..8_000]), peak(&l[BEAT + 6_500..BEAT + 8_000]))
    };
    let (plain, _) = both(&pad);
    pad.modulators = vec![Modulator {
        kind: ModKind::follower(7),
        routes: vec![ModRoute {
            target: ModTarget::Volume,
            depth: -30.0,
        }],
    }];
    let (ducked, later) = both(&pad);
    assert!(ducked < plain * 0.5, "{ducked} {plain}");
    assert!(later > ducked * 2.0, "{later} {ducked}");
}

#[test]
fn a_stacks_shared_lfo_moves_each_track_in_it_as_it_routes_it() {
    use crate::dsp::modulate::{LfoShape, ModRate};
    // Two tracks in a stack, one square LFO on the stack: each routes it to its own volume,
    // one down, the other not at all.
    let mut p = project(vec![
        synth_track("a", vec![note(0, PPQ * 4, 60)], PPQ * 4),
        synth_track("b", vec![note(0, PPQ * 4, 67)], PPQ * 4),
    ]);
    p.assign_ids();
    let (bus, _) = p.add_stack(&[0, 1]);
    let lfo = ModKind::Lfo {
        shape: LfoShape::Square,
        rate: ModRate::Beats(1.0),
        phase: 0.0,
        unipolar: false,
    };
    assert_eq!(p.add_shared(bus, lfo.clone()), Some(0));
    assert_eq!(
        p.tracks[1].modulators[0].kind,
        ModKind::Shared { modulator: 0 }
    );
    assert_eq!(p.mod_kind(1, 0), Some(&lfo));
    let swing = |p: &Project| {
        let [l, _] = render(p, Path::new("."), BEAT * 2);
        let (low, high) = (
            peak(&l[BEAT + 3000..BEAT + 9000]),
            peak(&l[BEAT + 15000..BEAT + 21000]),
        );
        high / low.max(1e-9)
    };
    assert!(swing(&p) < 1.5, "unrouted, it moves nothing");
    p.tracks[0].modulators[0].routes.push(ModRoute {
        target: ModTarget::Volume,
        depth: -24.0,
    });
    p.tracks[1].modulators[0].routes.push(ModRoute {
        target: ModTarget::Volume,
        depth: -24.0,
    });
    assert!(swing(&p) > 10.0, "{}", swing(&p));
    // Off the stack, it goes from every track in it.
    p.remove_shared(bus, 0);
    assert!(p.tracks.iter().all(|t| t.modulators.is_empty()));
    assert!(swing(&p) < 1.5);
}

#[test]
fn binaural_behind_is_duller_above_brighter_and_far_off_quieter() {
    // A high note (8.4 kHz), where the ears' filters tell.
    let level = |pan: f32, elevation: Option<f32>, distance: Option<f32>| {
        let mut t = synth_track("a", vec![note(0, PPQ, 120)], PPQ);
        (t.pan, t.pan_mode, t.elevation, t.distance) =
            (pan, PanMode::Binaural, elevation, distance);
        let [l, r] = render(&project(vec![t]), Path::new("."), BEAT);
        peak(&l[2_000..]).max(peak(&r[2_000..]))
    };
    let front = level(0.0, None, None);
    assert!(level(1.0, None, None) < front * 0.8, "behind");
    assert!(level(0.0, Some(90.0), None) > front * 1.2, "above");
    assert!(level(0.0, None, Some(3.0)) < front * 0.6, "far");
    // Its place, there and back.
    let p = Place::from_xyz(
        Place {
            azimuth: -120.0,
            elevation: 30.0,
            distance: 2.0,
        }
        .xyz(),
    );
    assert!((p.azimuth + 120.0).abs() < 0.01 && (p.elevation - 30.0).abs() < 0.01);
}

#[test]
fn volume_automation_overrides_the_fader() {
    // A held note, faded from 0 dB to silence over the first beat.
    let mut t = synth_track("a", vec![note(0, PPQ * 4, 69)], PPQ * 4);
    t.volume_db = -60.0;
    t.automation.volume = vec![AutoPoint::new(0, 0.0), AutoPoint::new(PPQ, -144.0)];
    let [l, _] = render(&project(vec![t]), Path::new("."), BEAT * 2);
    // Straight in dB: about -6 dB a 24th of the way in, about -30 dB at a fifth.
    let early = peak(&l[500..1_500]);
    let later = peak(&l[4_500..5_500]);
    assert!(early > 0.08, "{early}"); // the fader alone would give 0.00025
    assert!(later < early / 4.0 && later > 1e-4, "{early} {later}");
    assert!(peak(&l[BEAT + MAX_BLOCK..]) < 1e-6);
}

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
fn sends_reach_their_aux_after_the_fader() {
    // A unity send to a unity aux: the track is heard twice as loud, direct and through it.
    let track = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
    let [alone, _] = render(&project(vec![track.clone()]), Path::new("."), BEAT);
    let mut p = project(vec![track]);
    p.tracks[0].sends = vec![AuxSend {
        bus: 1,
        level_db: 0.0,
        pre: false,
    }];
    p.add_aux();
    let [l, _] = render(&p, Path::new("."), BEAT);
    assert!(peak(&alone) > 0.1);
    assert_eq!(l, alone.iter().map(|x| x * 2.0).collect::<Vec<_>>());

    p.auxes[0].mute = true;
    assert_eq!(render(&p, Path::new("."), BEAT)[0], alone);
    p.auxes[0].mute = false;
    p.tracks[0].volume_db = -144.0; // post-fader: the send goes quiet too
    assert!(peak(&render(&p, Path::new("."), BEAT)[0]) < 1e-6);
}

#[test]
fn mute_and_send_automation_play() {
    // Muted by automation for its second beat: silent there, sounding either side.
    let track = synth_track("a", vec![note(0, PPQ * 3, 69)], PPQ * 3);
    let mut p = project(vec![track.clone()]);
    p.tracks[0].automation.mute = vec![
        AutoPoint {
            step: true,
            ..AutoPoint::new(0, 0.0)
        },
        AutoPoint {
            step: true,
            ..AutoPoint::new(PPQ, 1.0)
        },
        AutoPoint {
            step: true,
            ..AutoPoint::new(PPQ * 2, 0.0)
        },
    ];
    let [l, _] = render(&p, Path::new("."), BEAT * 3);
    assert!(peak(&l[..BEAT - MAX_BLOCK]) > 0.1);
    assert!(peak(&l[BEAT + MAX_BLOCK..BEAT * 2 - MAX_BLOCK]) < 1e-6);
    assert!(peak(&l[BEAT * 2 + MAX_BLOCK..]) > 0.1);

    // A send automated from silence up to unity over the first beat: the aux hears nothing,
    // then all of it.
    let mut p = project(vec![track]);
    p.tracks[0].sends = vec![AuxSend {
        bus: 1,
        level_db: 0.0,
        pre: false,
    }];
    p.add_aux();
    p.tracks[0].automation.sends = vec![crate::song::project::SendAuto {
        bus: 1,
        points: vec![
            AutoPoint::new(0, -144.0),
            AutoPoint::new(PPQ, -144.0),
            AutoPoint::new(PPQ + 1, 0.0),
        ],
    }];
    let [l, _] = render(&p, Path::new("."), BEAT * 2);
    let [alone, _] = render(
        &project(vec![p.tracks[0].clone()]),
        Path::new("."),
        BEAT * 2,
    );
    let (a, b) = (
        peak(&l[..BEAT - MAX_BLOCK]),
        peak(&alone[..BEAT - MAX_BLOCK]),
    );
    assert!((a - b).abs() < 1e-4, "{a} {b}");
    let (a, b) = (
        peak(&l[BEAT + MAX_BLOCK..]),
        peak(&alone[BEAT + MAX_BLOCK..]),
    );
    assert!((a - b * 2.0).abs() < 1e-3, "{a} {b}");
}

#[test]
fn a_track_routed_to_a_stack_plays_through_it() {
    let track = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
    let [alone, _] = render(&project(vec![track.clone()]), Path::new("."), BEAT);
    let mut p = project(vec![track]);
    p.add_stack(&[0]);
    p.auxes[0].volume_db = -20.0 * 2f32.log10(); // half
    let [l, _] = render(&p, Path::new("."), BEAT);
    assert!(peak(&alone) > 0.1);
    assert!((peak(&l) / peak(&alone) - 0.5).abs() < 1e-4);
    p.auxes[0].mute = true; // the stack's channel carries it all
    assert_eq!(peak(&render(&p, Path::new("."), BEAT)[0]), 0.0);
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
fn mute_and_solo() {
    let a = synth_track("a", vec![note(0, PPQ, 60)], PPQ);
    let b = synth_track("b", vec![note(0, PPQ, 72)], PPQ);
    let alone = |t: &Track| render(&project(vec![t.clone()]), Path::new("."), BEAT);
    let (a_alone, b_alone) = (alone(&a), alone(&b));

    let mut solo_b = b.clone();
    solo_b.solo = true;
    assert_eq!(
        render(&project(vec![a.clone(), solo_b]), Path::new("."), BEAT),
        b_alone
    );

    let mut mute_b = b;
    mute_b.mute = true;
    assert_eq!(
        render(&project(vec![a, mute_b]), Path::new("."), BEAT),
        a_alone
    );
}

#[test]
fn audio_regions_play_their_slice_of_the_file() {
    let dir = scratch_dir("audio");
    let ramp: Vec<f32> = (0..BEAT * 2)
        .map(|i| i as f32 / (BEAT * 2) as f32)
        .collect();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(dir.join("ramp.wav"), spec).unwrap();
    ramp.iter().for_each(|&s| w.write_sample(s).unwrap());
    w.finalize().unwrap();

    // Half a beat in, one beat long, starting 100 samples into the file.
    let region = Region {
        start: PPQ / 2,
        length: PPQ,
        content: Content::Audio {
            file: "ramp.wav".into(),
            offset: 100,
            stretch: 1.0,
            flex: Flex::default(),
            follow: false,
            formants: false,
            warps: Vec::new(),
        },
        ..Default::default()
    };
    let track = Track {
        name: "audio".into(),
        regions: vec![region],
        ..Default::default()
    };
    let [l, r] = render(&project(vec![track]), &dir, BEAT * 2);
    let start = BEAT / 2;
    assert_eq!(peak(&l[..start]), 0.0);
    assert_eq!(&l[start..start + BEAT], &ramp[100..100 + BEAT]);
    assert_eq!(peak(&l[start + BEAT..]), 0.0);
    assert_eq!(l, r);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_audio_regions_fades_bring_it_in_and_take_it_out() {
    let dir = scratch_dir("fades");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(dir.join("flat.wav"), spec).unwrap();
    (0..BEAT * 2).for_each(|_| w.write_sample(0.5f32).unwrap());
    w.finalize().unwrap();
    // Two beats, faded in over the first half beat and out over the last.
    let region = Region {
        start: 0,
        length: PPQ * 2,
        fade_in: PPQ / 2,
        fade_out: PPQ / 2,
        content: Content::Audio {
            file: "flat.wav".into(),
            offset: 0,
            stretch: 1.0,
            flex: Flex::default(),
            follow: false,
            formants: false,
            warps: Vec::new(),
        },
        ..Default::default()
    };
    let track = Track {
        name: "audio".into(),
        regions: vec![region],
        ..Default::default()
    };
    let [l, _] = render(&project(vec![track]), &dir, BEAT * 2);
    let half = BEAT / 2;
    assert!(l[0].abs() < 1e-3); // silent at the start
    assert!(l[half / 2] > 0.3 && l[half / 2] < 0.4, "{}", l[half / 2]); // sin 45°: 0.707 of 0.5
    assert!((l[BEAT] - 0.5).abs() < 1e-6); // full in the middle
    assert!(l[BEAT * 2 - 1] < 0.01); // gone by the end
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_audio_file_leaves_out_its_region_only() {
    let region = Region {
        start: 0,
        length: PPQ,
        content: Content::Audio {
            file: "gone.wav".into(),
            offset: 0,
            stretch: 1.0,
            flex: Flex::default(),
            follow: false,
            formants: false,
            warps: Vec::new(),
        },
        ..Default::default()
    };
    let audio = Track {
        name: "audio".into(),
        regions: vec![region],
        ..Default::default()
    };
    let synth = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
    let p = project(vec![audio, synth]);
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, problems) = compile(&p, RATE, &[(0, 0), (1, 0)], &[], None, &mut files);
    assert_eq!(problems.len(), 1);
    assert!(arrangement.plans[0].clips.is_empty());
    assert_eq!(arrangement.plans[1].events.len(), 2);
    assert!(Engine::load(&p, Path::new("."), RATE).is_err()); // a bounce does fail
}

#[test]
fn a_pattern_loops_over_its_region() {
    use crate::song::project::PatternRow;
    // Kick on steps 1 and 3 of four sixteenths, over a beat and a half: it comes round again.
    let rows = vec![PatternRow {
        pitch: 36,
        cells: vec![100, 0, 90, 0],
        mute: false,
        ..Default::default()
    }];
    let region = Region {
        start: 0,
        length: PPQ * 3 / 2,
        content: Content::Pattern {
            step: PPQ / 4,
            rows,
            swing: 50.0,
        },
        ..Default::default()
    };
    let track = Track {
        name: "d".into(),
        instrument: Some(Instrument::Drums),
        regions: vec![region],
        ..Default::default()
    };
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(
        &project(vec![track.clone()]),
        RATE,
        &[(0, 0)],
        &[],
        None,
        &mut files,
    );
    let ons: Vec<u64> = arrangement.plans[0]
        .events
        .iter()
        .filter(|e| e.velocity > 0.0)
        .map(|e| e.at)
        .collect();
    let sixteenth = BEAT as u64 / 4;
    assert_eq!(ons, [0, 2 * sixteenth, 4 * sixteenth]);
    assert!(peak(&render(&project(vec![track]), Path::new("."), BEAT)[0]) > 0.1); // the kit plays it
}

#[test]
fn a_take_folder_plays_its_comp() {
    use crate::song::project::{CompPiece, Take};
    // Two takes, an octave apart; the comp plays the first for a beat, then the second.
    let take = |pitch| Take {
        start: 0,
        content: Content::Midi {
            notes: vec![note(0, PPQ / 2, pitch), note(PPQ as i64, PPQ / 2, pitch)],
            controls: Vec::new(),
        },
    };
    let comp = vec![
        CompPiece { from: 0, take: 0 },
        CompPiece { from: PPQ, take: 1 },
    ];
    let region = Region {
        start: 0,
        length: PPQ * 2,
        content: Content::Takes {
            takes: vec![take(57), take(69)],
            comp,
        },
        ..Default::default()
    };
    let track = Track {
        name: "t".into(),
        instrument: Some(Instrument::Synth),
        regions: vec![region],
        ..Default::default()
    };
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, _) = compile(
        &project(vec![track]),
        RATE,
        &[(0, 0)],
        &[],
        None,
        &mut files,
    );
    let ons: Vec<(u64, u8)> = arrangement.plans[0]
        .events
        .iter()
        .filter(|e| e.velocity > 0.0)
        .map(|e| (e.at, e.pitch))
        .collect();
    assert_eq!(ons, [(0, 57), (BEAT as u64, 69)]);
}

#[test]
fn flex_stretches_audio_and_keeps_its_pitch() {
    // A second of 441 Hz, flexed to fill two seconds (four beats at 120 bpm).
    let dir = scratch_dir("flex");
    let tone: Vec<f32> = (0..48_000)
        .map(|i| 0.5 * (std::f32::consts::TAU * 441.0 * i as f32 / 48_000.0).sin())
        .collect();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(dir.join("tone.wav"), spec).unwrap();
    tone.iter().for_each(|&s| w.write_sample(s).unwrap());
    w.finalize().unwrap();
    let content = Content::Audio {
        file: "tone.wav".into(),
        offset: 0,
        stretch: 1.0,
        flex: Flex::default(),
        follow: false,
        formants: false,
        warps: Vec::new(),
    };
    let region = Region {
        start: 0,
        length: PPQ * 2,
        content,
        ..Default::default()
    };
    let mut p = project(vec![Track {
        name: "a".into(),
        regions: vec![region],
        ..Default::default()
    }]);
    p.flex((0, 0), PPQ * 4).unwrap();
    assert!(matches!(
        p.region((0, 0)).content,
        Content::Audio { stretch: 2.0, .. }
    ));
    let [l, _] = render(&p, &dir, BEAT * 5);
    // Sound to the end of the second second, and silence after.
    assert!(peak(&l[BEAT * 3..BEAT * 4 - 4_000]) > 0.2);
    assert!(peak(&l[BEAT * 4 + 2_000..]) < 0.05);
    // Still 441 Hz: about 441 upward zero crossings a second.
    let second = &l[24_000..72_000];
    let ups = second
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    assert!((ups as i64 - 441).abs() <= 3, "{ups}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_player_plays_a_stand_in_while_a_stretch_is_made_off_its_thread() {
    let dir = scratch_dir("background");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(dir.join("tone.wav"), spec).unwrap();
    for i in 0..48_000 {
        let s = 0.5 * (std::f32::consts::TAU * 441.0 * i as f32 / 48_000.0).sin();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
    let mut files = AudioFiles::new(&dir, 48_000.0).in_background();
    let get = |files: &mut AudioFiles| {
        files.begin();
        let data = files
            .stretched(Path::new("tone.wav"), stretch::Fit::ratio(1.5))
            .unwrap();
        files.end();
        data
    };
    // At once: as long as the stretch will be.
    let stand_in = get(&mut files);
    assert_eq!(stand_in.channels[0].len(), 72_000);
    assert!(files.rendering());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !files.fresh() {
        assert!(Instant::now() < deadline, "never made");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Made: the next arrangement has it, and nothing's left to make.
    let made = get(&mut files);
    assert!(!Arc::ptr_eq(&stand_in, &made));
    assert_eq!(made.channels[0].len(), 72_000);
    assert!(!files.rendering());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_transposed_audio_region_moves_in_pitch_not_length() {
    // A second of 441 Hz, an octave up: 882 Hz, still a second.
    let dir = scratch_dir("transpose");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(dir.join("tone.wav"), spec).unwrap();
    for i in 0..48_000 {
        w.write_sample(0.5 * (std::f32::consts::TAU * 441.0 * i as f32 / 48_000.0).sin())
            .unwrap();
    }
    w.finalize().unwrap();
    let region = Region {
        start: 0,
        length: PPQ * 2,
        transpose: 12,
        content: Content::Audio {
            file: "tone.wav".into(),
            offset: 0,
            stretch: 1.0,
            flex: Flex::default(),
            follow: false,
            formants: false,
            warps: Vec::new(),
        },
        ..Default::default()
    };
    let p = project(vec![Track {
        name: "a".into(),
        regions: vec![region],
        ..Default::default()
    }]);
    let [l, _] = render(&p, &dir, BEAT * 3);
    let second = &l[6_000..42_000];
    let ups = second
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    assert!((ups as i64 - 661).abs() <= 8, "{ups}"); // 882 Hz over 0.75 s
    assert!(peak(&l[BEAT * 2 - 4_000..BEAT * 2 - 1_000]) > 0.2);
    assert!(peak(&l[BEAT * 2 + 3_000..]) < 0.05);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_launched_cell_loops_instead_of_the_track() {
    use crate::song::project::Cell;
    // The track's own region is silent; its cell is a beat-long loop with a short note.
    let mut t = synth_track("a", vec![], PPQ * 8);
    let notes = vec![note(0, PPQ / 4, 69)];
    let region = Region {
        start: 0,
        length: PPQ,
        content: Content::Midi {
            notes,
            controls: Vec::new(),
        },
        ..Default::default()
    };
    t.cells.push(Cell { scene: 0, region });
    let (mut engine, _) = Engine::load(&project(vec![t]), Path::new("."), RATE).unwrap();
    engine.play();
    assert_eq!(peak(&run(&mut engine, BEAT)[0]), 0.0);
    // Launched at beat 2: it plays there, rests, and comes round again at beat 3.
    engine.launch(0, Some(0), BEAT as u64 * 2);
    let [l, _] = run(&mut engine, BEAT * 3);
    let beat = |b: usize, from: f32, to: f32| {
        peak(&l[b * BEAT + (BEAT as f32 * from) as usize..b * BEAT + (BEAT as f32 * to) as usize])
    };
    assert_eq!(beat(0, 0.0, 1.0), 0.0); // beat 2 of the song is before the launch
    assert!(beat(1, 0.02, 0.2) > 0.1 && beat(1, 0.5, 0.95) == 0.0);
    assert!(beat(2, 0.02, 0.2) > 0.1);
    // Stopped, the track's own (silent) region plays again.
    engine.launch(0, None, engine.position());
    let [l, _] = run(&mut engine, BEAT);
    assert_eq!(peak(&l[RELEASE + MAX_BLOCK..]), 0.0);
}

/// A plugin stand-in: it notes the controls it hears (at the sample they fall on) and the
/// first sample its sidechain isn't silent; as an effect it passes audio through.
#[derive(Default, Clone)]
struct Probe {
    heard: std::sync::Arc<std::sync::Mutex<Vec<(u64, ControlKind)>>>,
    sidechain_from: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
    at: u64,
}

impl crate::plugin::ProcessorApi for Probe {
    fn latency(&self) -> u32 {
        0
    }
    fn set_transport(&mut self, _: bool, position: u64, _: f64) {
        self.at = position;
    }
    fn automate(&mut self, _: u32, _: f64, _: f64) {}
    fn sidechain(&self) -> bool {
        true
    }
    fn pending(&self) -> bool {
        false
    }
    fn process(
        &mut self,
        _: [&mut [f32]; 2],
        frames: usize,
        _: &[Note],
        controls: &[Control],
        sidechain: Option<[&[f32]; 2]>,
    ) {
        let mut heard = self.heard.lock().unwrap();
        heard.extend(controls.iter().map(|c| (self.at + c.offset as u64, c.kind)));
        if let Some([l, _]) = sidechain
            && let Some(i) = l[..frames].iter().position(|x| x.abs() > 1e-3)
        {
            let mut from = self.sidechain_from.lock().unwrap();
            from.get_or_insert(self.at + i as u64);
        }
        self.at += frames as u64;
    }
}

/// An engine for `p` whose tracks have their built-in instruments, but `probe` as the
/// instrument of the tracks with a plugin and the insert of tracks with inserts.
fn probed(p: &Project, probe: &Probe) -> Engine {
    let mut lanes = Vec::new();
    for t in &p.tracks {
        let built_in = t
            .instrument
            .as_ref()
            .filter(|i| !matches!(i, Instrument::Plugin(_)));
        let made = make_lane(built_in, &[], RATE, &mut Modules::new(), Path::new("."));
        let mut lane = made.unwrap().0;
        if matches!(t.instrument, Some(Instrument::Plugin(_))) {
            lane.source = Source::Plugin(Box::new(probe.clone()));
        }
        if !t.inserts.is_empty() {
            lane.inserts.push(Box::new(probe.clone()));
        }
        lanes.push(Some(Box::new(lane)));
    }
    let tracks: Vec<_> = (0..p.tracks.len()).map(|i| (i, 0)).collect();
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, problems) = compile(p, RATE, &tracks, &[], None, &mut files);
    assert!(problems.is_empty(), "{problems:?}");
    Engine::new(RATE, arrangement, lanes)
}

#[test]
fn controllers_reach_the_instrument_as_they_stand_at_each_pass() {
    use crate::song::project::{ControlPoint, ControlValue::Cc};
    // A beat-long region from beat 2, looping: the mod wheel at 10 from before it starts,
    // then 100 halfway through.
    let controls = vec![
        ControlPoint {
            start: -100,
            kind: Cc {
                number: 1,
                value: 10,
            },
        },
        ControlPoint {
            start: PPQ as i64 / 2,
            kind: Cc {
                number: 1,
                value: 100,
            },
        },
    ];
    let region = Region {
        start: PPQ,
        length: PPQ,
        looped: true,
        content: Content::Midi {
            notes: vec![],
            controls,
        },
        ..Default::default()
    };
    let t = Track {
        name: "p".into(),
        instrument: Some(Instrument::Plugin(PluginRef::default())),
        regions: vec![
            region,
            Region {
                start: PPQ * 3,
                length: PPQ,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let probe = Probe::default();
    let mut engine = probed(&project(vec![t]), &probe);
    engine.play();
    run(&mut engine, BEAT * 4);
    let wheel = |v: u8| ControlKind::Cc {
        number: 1,
        value: v as f32 / 127.0,
    };
    let half = BEAT as u64 / 2;
    let (b1, b2) = (BEAT as u64, BEAT as u64 * 2);
    // Each pass of the loop starts where the mod wheel stood before it, and moves halfway.
    assert_eq!(
        *probe.heard.lock().unwrap(),
        [
            (b1, wheel(10)),
            (b1 + half, wheel(100)),
            (b2, wheel(10)),
            (b2 + half, wheel(100))
        ]
    );
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
fn a_launched_audio_cell_plays_its_loop() {
    use crate::song::project::Cell;
    // A quarter second of tone, as a beat-long loop on an audio track with no regions.
    let dir = scratch_dir("audio-cell");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(dir.join("loop.wav"), spec).unwrap();
    (0..12_000).for_each(|i| w.write_sample((i as f32 * 0.06).sin() * 0.5).unwrap());
    w.finalize().unwrap();
    let region = Region {
        start: 0,
        length: PPQ,
        content: Content::Audio {
            file: "loop.wav".into(),
            offset: 0,
            stretch: 1.0,
            flex: Flex::default(),
            follow: false,
            formants: false,
            warps: Vec::new(),
        },
        ..Default::default()
    };
    let t = Track {
        name: "a".into(),
        cells: vec![Cell { scene: 0, region }],
        ..Default::default()
    };
    let (mut engine, _) = Engine::load(&project(vec![t]), &dir, RATE).unwrap();
    engine.play();
    engine.launch(0, Some(0), BEAT as u64);
    let [l, _] = run(&mut engine, BEAT * 3);
    assert_eq!(peak(&l[..BEAT]), 0.0);
    // Each beat: the tone, then the rest of the beat quiet.
    for b in 1..3 {
        assert!(peak(&l[b * BEAT + 100..b * BEAT + 11_000]) > 0.4);
        assert!(peak(&l[b * BEAT + 13_000..(b + 1) * BEAT]) < 0.01);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn stopping_releases_held_notes() {
    let p = project(vec![synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8)]);
    let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
    engine.play();
    assert!(peak(&run(&mut engine, MAX_BLOCK * 10)[0]) > 0.1);
    engine.stop();
    run(&mut engine, RELEASE + MAX_BLOCK);
    assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
    assert_eq!(engine.position(), (MAX_BLOCK * 10) as u64);
}

#[test]
fn stopping_releases_overlapping_notes_of_one_pitch() {
    // Two notes on A, the second starting while the first still sounds: two voices.
    let notes = vec![note(0, PPQ * 8, 69), note(PPQ as i64, PPQ * 8, 69)];
    let p = project(vec![synth_track("a", notes, PPQ * 8)]);
    let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
    engine.play();
    run(&mut engine, BEAT * 2);
    engine.stop();
    run(&mut engine, RELEASE + MAX_BLOCK);
    assert_eq!(peak(&run(&mut engine, MAX_BLOCK)[0]), 0.0);
}

#[test]
fn an_edit_while_playing_lets_go_of_notes_the_song_no_longer_has() {
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
    // A long A, and a long C that the edit keeps.
    let notes = vec![note(0, PPQ * 8, 69), note(0, PPQ * 8, 60)];
    let p = project(vec![synth_track("a", notes, PPQ * 8)]);
    let lane = make_lane(
        Some(&Instrument::Synth),
        &[],
        RATE,
        &mut Modules::new(),
        Path::new("."),
    )
    .unwrap()
    .0;
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let arrange = |p: &Project, files: &mut AudioFiles| {
        Box::new(compile(p, RATE, &[(3, 0)], &[], None, files).0)
    };
    for m in [
        Msg::Insert(3, Box::new(lane)),
        Msg::Arrange(arrange(&p, &mut files)),
        Msg::Play,
    ] {
        assert!(tx.push(m).is_ok());
    }
    run(&mut engine, BEAT);
    // The A goes (as a key change or a deleted note would take it); the C stays.
    let mut edited = p.clone();
    if let Content::Midi { notes, .. } = &mut edited.tracks[0].regions[0].content {
        notes.retain(|n| n.pitch == 60);
    }
    assert!(tx.push(Msg::Arrange(arrange(&edited, &mut files))).is_ok());
    run(&mut engine, RELEASE + MAX_BLOCK);
    let [l, _] = run(&mut engine, BEAT / 2);
    // Only the C rings on: its 261.6 Hz, and no 440 Hz.
    let level = |f: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, &x) in l.iter().enumerate() {
            let w = 2.0 * std::f64::consts::PI * f * i as f64 / RATE;
            re += x as f64 * w.cos();
            im += x as f64 * w.sin();
        }
        (re * re + im * im).sqrt() * 2.0 / l.len() as f64
    };
    let (c, a) = (level(261.63), level(440.0));
    assert!(c > 0.05 && a < c / 20.0, "C {c}, A {a}"); // what's left of A is the C's spill
}

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
fn bounce_writes_a_stereo_float_wav() {
    let dir = scratch_dir("bounce");
    let p = project(vec![synth_track("a", vec![note(0, PPQ, 69)], PPQ)]);
    let (mut engine, _) = Engine::load(&p, &dir, RATE).unwrap();
    engine.bounce(&dir.join("out.wav"), 30_000).unwrap();
    let wav = hound::WavReader::open(dir.join("out.wav")).unwrap();
    assert_eq!((wav.spec().channels, wav.duration()), (2, 30_000));
    std::fs::remove_dir_all(dir).unwrap();
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
fn the_channel_eq_and_compressor_shape_the_sound() {
    let t = synth_track("a", vec![note(0, PPQ * 2, 69)], PPQ * 2);
    let dry = peak(&render(&project(vec![t.clone()]), Path::new("."), BEAT)[0][BEAT / 2..]);
    let mut boosted = t.clone();
    boosted.eq = Some(Eq {
        bands: crate::song::project::Bands::from_slice(&[crate::song::project::EqBand::new(
            crate::song::project::Shape::Bell,
            440.0,
            6.0,
        )]),
        ..Eq::default()
    });
    let louder =
        peak(&render(&project(vec![boosted.clone()]), Path::new("."), BEAT)[0][BEAT / 2..]);
    assert!((louder / dry - 2.0).abs() < 0.05, "{dry} {louder}");
    // Bypassed, it's as if it weren't there.
    boosted.eq.as_mut().unwrap().bypass = true;
    let same = peak(&render(&project(vec![boosted]), Path::new("."), BEAT)[0][BEAT / 2..]);
    assert!((same - dry).abs() < 1e-6);

    let mut squashed = t;
    squashed.comp = Some(Comp {
        threshold_db: -40.0,
        ratio: 20.0,
        ..Comp::default()
    });
    let quieter = peak(&render(&project(vec![squashed]), Path::new("."), BEAT)[0][BEAT / 2..]);
    assert!(quieter < dry / 4.0, "{dry} {quieter}");
}

#[test]
fn an_eq_band_plays_its_automation() {
    let t = synth_track("a", vec![note(0, PPQ * 2, 69)], PPQ * 2);
    let dry = peak(&render(&project(vec![t.clone()]), Path::new("."), BEAT)[0][BEAT / 2..]);
    let mut boosted = t.clone();
    boosted.eq = Some(Eq {
        bands: crate::song::project::Bands::from_slice(&[crate::song::project::EqBand::new(
            crate::song::project::Shape::Bell,
            440.0,
            0.0,
        )]),
        ..Eq::default()
    });
    // Flat as set; its automation has it 6 dB up all along.
    *boosted.automation.points_mut(Param::Eq(0, EqField::Gain)) = vec![AutoPoint::new(0, 6.0)];
    let louder = peak(&render(&project(vec![boosted]), Path::new("."), BEAT)[0][BEAT / 2..]);
    assert!((louder / dry - 2.0).abs() < 0.05, "{dry} {louder}");
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

fn note_ons(p: &Project) -> Vec<(u64, u8, u8)> {
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (a, _) = compile(p, RATE, &[(0, 0)], &[], None, &mut files);
    a.plans[0]
        .events
        .iter()
        .filter(|e| e.velocity > 0.0)
        .map(|e| (e.at, e.pitch, (e.velocity * 127.0).round() as u8))
        .collect()
}

#[test]
fn a_looped_region_plays_until_the_next_region() {
    let mut t = synth_track("a", vec![note(0, 240, 60)], PPQ);
    t.regions[0].looped = true;
    t.regions.push(Region {
        start: PPQ * 3,
        length: PPQ,
        content: Content::Midi {
            notes: vec![note(0, 240, 72)],
            controls: Vec::new(),
        },
        ..Default::default()
    });
    let b = BEAT as u64;
    assert_eq!(
        note_ons(&project(vec![t])),
        [
            (0, 60, 127),
            (b, 60, 127),
            (2 * b, 60, 127),
            (3 * b, 72, 127)
        ]
    );
}

#[test]
fn loops_dragged_out_stop_where_they_were_dragged_and_stretching_spreads_notes() {
    // Looped to three beats, with the next region at five: three plays.
    let mut t = synth_track("a", vec![note(0, 240, 60)], PPQ);
    t.regions[0].looped = true;
    t.regions[0].loop_length = Some(PPQ * 3);
    t.regions.push(Region {
        start: PPQ * 5,
        length: PPQ,
        content: Content::Midi {
            notes: vec![note(0, 240, 72)],
            controls: Vec::new(),
        },
        ..Default::default()
    });
    let b = BEAT as u64;
    let mut p = project(vec![t]);
    assert_eq!(
        note_ons(&p),
        [
            (0, 60, 127),
            (b, 60, 127),
            (2 * b, 60, 127),
            (5 * b, 72, 127)
        ]
    );
    // Trimming its start keeps where its loops stop.
    p.trim_start((0, 0), PPQ / 2);
    assert_eq!(p.loop_end((0, 0)), PPQ * 3);
    // Stretched to twice as long: the note starts and lasts twice as late and long.
    p.tracks[0].regions[1].start = PPQ * 5;
    p.flex((0, 1), PPQ * 2).unwrap();
    let r = &p.tracks[0].regions[1];
    let Content::Midi { notes, .. } = &r.content else {
        unreachable!()
    };
    assert_eq!(
        (r.length, notes[0].start, notes[0].length),
        (PPQ * 2, 0, 480)
    );
}

#[test]
fn region_playback_settings_change_what_plays_not_the_notes() {
    let mut t = synth_track("a", vec![note(100, 240, 60), note(350, 240, 64)], PPQ);
    let r = &mut t.regions[0];
    r.transpose = 12;
    r.velocity = -27;
    r.quantize = Some(240);
    let p = project(vec![t]);
    let sixteenth = BEAT as u64 / 4;
    assert_eq!(note_ons(&p), [(0, 72, 100), (sixteenth, 76, 100)]);
    let Content::Midi { notes, .. } = &p.tracks[0].regions[0].content else {
        unreachable!()
    };
    assert_eq!((notes[0].start, notes[0].pitch), (100, 60));
}

#[test]
fn pre_fader_sends_ignore_the_fader_but_not_mute() {
    let track = synth_track("a", vec![note(0, PPQ, 69)], PPQ);
    let mut p = project(vec![track]);
    p.tracks[0].sends = vec![AuxSend {
        bus: 1,
        level_db: 0.0,
        pre: true,
    }];
    p.add_aux();
    p.tracks[0].volume_db = -144.0;
    assert!(peak(&render(&p, Path::new("."), BEAT)[0]) > 0.1);
    p.tracks[0].mute = true;
    assert!(peak(&render(&p, Path::new("."), BEAT)[0]) < 1e-6);
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

#[test]
fn the_master_saturator_rounds_off_the_mix() {
    let t = synth_track("a", vec![note(0, PPQ * 2, 69)], PPQ * 2);
    let p = project(vec![t]);
    let dry = peak(&render(&p, Path::new("."), BEAT)[0][BEAT / 2..]);
    let mut hot = p.clone();
    hot.master_sat = Some(Saturator {
        drive_db: 24.0,
        ..Saturator::default()
    });
    let wet = peak(&render(&hot, Path::new("."), BEAT)[0][BEAT / 2..]);
    assert!(wet < dry * 0.6, "{dry} {wet}");
}

/// With a real instrument, which CI doesn't have:
/// `PINA_TEST_VST3="/path/Some Synth.vst3" cargo test -p pina-engine -- --ignored`
#[test]
#[ignore]
fn a_plugin_parameter_follows_its_automation() {
    let path = std::env::var("PINA_TEST_VST3").expect("set PINA_TEST_VST3");
    let class = crate::plugin::scan::inspect(Path::new(&path))
        .unwrap()
        .remove(0)
        .class;
    let plugin = PluginRef {
        path: path.into(),
        class,
        ..Default::default()
    };
    let mut t = synth_track("s", vec![note(0, PPQ * 8, 57)], PPQ * 8);
    t.instrument = Some(Instrument::Plugin(plugin));
    let p = project(vec![t]);
    // Its loudest-sounding level parameter, turned right down by automation.
    let (_engine, lanes) = Engine::load(&p, Path::new("."), RATE).unwrap();
    let params = lanes[0].instrument.as_ref().unwrap().params();
    let volume = params
        .iter()
        .find(|q| q.name.to_lowercase().contains("volume"))
        .unwrap_or_else(|| panic!("no volume among {} params", params.len()));
    let loud = peak(&render(&p, Path::new("."), BEAT * 2)[0][BEAT..]);
    let mut quiet = p.clone();
    quiet.tracks[0]
        .automation
        .plugins
        .push(crate::song::project::PluginAuto {
            plugin: Which::Instrument,
            param: volume.id,
            name: volume.name.clone(),
            points: vec![AutoPoint::new(0, 0.0)],
        });
    let soft = peak(&render(&quiet, Path::new("."), BEAT * 2)[0][BEAT..]);
    assert!(loud > 0.01 && soft < loud / 4.0, "{loud} {soft}");
    // A value the project sets, as Smart Controls and agents do, holds in a render too.
    let mut set = p.clone();
    if let Some(Instrument::Plugin(r)) = &mut set.tracks[0].instrument {
        r.params.insert(volume.id, 0.0);
    }
    let held = peak(&render(&set, Path::new("."), BEAT * 2)[0][BEAT..]);
    assert!(held < loud / 4.0, "{loud} {held}");
    // Off, the automation is ignored.
    quiet.tracks[0].automation.mode = AutoMode::Off;
    let off = peak(&render(&quiet, Path::new("."), BEAT * 2)[0][BEAT..]);
    assert!((off - loud).abs() < loud * 0.1, "{loud} {off}");
}

#[test]
fn audio_at_another_rate_is_converted() {
    // A 1 kHz sine at 44.1 kHz, converted to 48 kHz: the same second, the same pitch.
    let sine: Vec<f32> = (0..44_100)
        .map(|i| (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 44_100.0).sin() as f32)
        .collect();
    let out = resample(&sine, 44_100.0, 48_000.0);
    assert_eq!(out.len(), 48_000);
    let level = |f: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        let tail = &out[1000..47_000];
        for (i, &x) in tail.iter().enumerate() {
            let w = 2.0 * std::f64::consts::PI * f * (i + 1000) as f64 / 48_000.0;
            re += x as f64 * w.cos();
            im += x as f64 * w.sin();
        }
        (re * re + im * im).sqrt() * 2.0 / tail.len() as f64
    };
    assert!((level(1000.0) - 1.0).abs() < 0.01, "{}", level(1000.0));
    assert!(level(1100.0) < 0.01);
    // And down: 48 kHz to 44.1 kHz keeps the length in time.
    assert_eq!(resample(&out, 48_000.0, 44_100.0).len(), 44_100);
}
