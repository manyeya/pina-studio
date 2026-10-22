//! Audio regions: their slice of the file, fades, stretching, transposing, other rates, cells, and bouncing.

use super::*;

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
