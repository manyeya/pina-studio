use super::*;
use crate::audio::engine::MAX_BLOCK;
use crate::song::project::{SampleCharacter, SampleMode, Trigger};

fn sampler(zones: Vec<(Zone, Vec<f32>)>) -> Sampler {
    with_voice(zones, SamplerVoice::default())
}

fn with_voice(zones: Vec<(Zone, Vec<f32>)>, voice: SamplerVoice) -> Sampler {
    let zones = zones
        .into_iter()
        .map(|(zone, samples)| Loaded::new(&zone, vec![samples]))
        .collect();
    Sampler::with(zones, voice, 48_000.0)
}

fn note(offset: u32, pitch: u8, velocity: f32) -> Note {
    Note {
        offset,
        pitch,
        velocity,
    }
}

fn run(s: &mut Sampler, notes: &[Note]) -> Vec<f32> {
    run_with(s, notes, &[])
}

fn run_with(s: &mut Sampler, notes: &[Note], controls: &[Control]) -> Vec<f32> {
    let (mut l, mut r) = (vec![0f32; MAX_BLOCK], vec![0f32; MAX_BLOCK]);
    s.process([&mut l, &mut r], MAX_BLOCK, notes, controls);
    l
}

/// The slope of a ramp read back: how fast it's being read.
fn slope(out: &[f32], i: usize) -> f32 {
    (out[i + 1] - out[i]) * 48_000.0
}

fn peak(o: &[f32], a: usize, b: usize) -> f32 {
    o[a..b].iter().fold(0f32, |m, x| m.max(x.abs()))
}

#[test]
fn a_note_glides_to_its_tuning() {
    // A long ramp read at the root's speed, then tuned up an octave: twice as fast, once the
    // glide (a few milliseconds) settles.
    let ramp: Vec<f32> = (0..240_000).map(|i| i as f32 / 48_000.0).collect();
    let sub = Zone {
        note: 24,
        high: Some(48),
        root: Some(36),
        gate: true,
        ..Zone::default()
    };
    let mut s = sampler(vec![(sub, ramp)]);
    run(&mut s, &[note(0, 36, 1.0)]);
    let up = Control {
        offset: 0,
        kind: ControlKind::Expression {
            pitch: 36,
            kind: ExpressionKind::Tuning,
            value: 12.0,
        },
    };
    let first = run_with(&mut s, &[], &[up]);
    // Going faster within the block, and not yet twice as fast.
    assert!(
        slope(&first, 40) > 1.0 && slope(&first, 40) < 1.6,
        "{}",
        slope(&first, 40)
    );
    let settle = |s: &mut Sampler| (0..8).map(|_| run_with(s, &[], &[])).last().unwrap();
    let later = settle(&mut s);
    assert!(
        (slope(&later, 100) - 2.0).abs() < 0.01,
        "{}",
        slope(&later, 100)
    );
    // Pitch bend all the way down: two semitones under that.
    let down = Control {
        offset: 0,
        kind: ControlKind::PitchBend(-1.0),
    };
    run_with(&mut s, &[], &[down]);
    let bent = settle(&mut s);
    let want = 2.0 * 2f32.powf(-2.0 / 12.0);
    assert!(
        (slope(&bent, 100) - want).abs() < 0.01,
        "{}",
        slope(&bent, 100)
    );
}

#[test]
fn a_hit_plays_out_however_short_its_note() {
    let kick = Zone {
        note: 36,
        ..Zone::default()
    };
    let mut s = sampler(vec![(kick, vec![0.5; 300])]);
    let out = run(&mut s, &[note(10, 36, 1.0), note(20, 36, 0.0)]);
    assert_eq!(out[9], 0.0);
    assert!((out[10] - 0.5).abs() < 1e-6 && (out[200] - 0.5).abs() < 1e-6);
    assert_eq!(out[400], 0.0); // past its end
    // Another note, or a quieter one: nothing, and less.
    assert_eq!(run(&mut s, &[note(0, 37, 1.0)])[10], 0.0);
    let soft = run(&mut s, &[note(0, 36, 0.5)])[10];
    assert!(soft < 0.25 && soft > 0.1, "{soft}");
}

#[test]
fn a_range_repitches_and_a_gated_sound_stops_with_its_note() {
    // A ramp, so how fast it's read shows.
    let ramp: Vec<f32> = (0..4800).map(|i| i as f32 / 4800.0).collect();
    let bass = Zone {
        note: 24,
        high: Some(48),
        root: Some(36),
        gate: true,
        release_ms: Some(1.0),
        ..Zone::default()
    };
    let mut s = sampler(vec![(bass, ramp)]);
    // An octave up reads twice as fast.
    let out = run(&mut s, &[note(0, 48, 1.0)]);
    assert!((out[100] - 200.0 / 4800.0).abs() < 1e-4, "{}", out[100]);
    // Let go: gone within its millisecond.
    let out = run(&mut s, &[note(0, 48, 0.0)]);
    assert!(out[0] > 0.0 && out[60] == 0.0, "{} {}", out[0], out[60]);
    // Tuned up an octave, its root reads twice as fast too.
    s.zones[0].p.tune = 12.0;
    let out = run(&mut s, &[note(0, 36, 1.0)]);
    assert!((out[100] - 200.0 / 4800.0).abs() < 1e-4, "{}", out[100]);
}

#[test]
fn a_closed_hat_chokes_the_open_one() {
    let open = Zone {
        note: 46,
        choke: Some(1),
        ..Zone::default()
    };
    let closed = Zone {
        note: 42,
        choke: Some(1),
        ..Zone::default()
    };
    let mut s = sampler(vec![(open, vec![0.5; 48_000]), (closed, vec![0.25; 20])]);
    let out = run(&mut s, &[note(0, 46, 1.0), note(100, 42, 1.0)]);
    assert!((out[50] - 0.5).abs() < 1e-6);
    // The open hat is fading out under the closed one, and both are gone a little later.
    assert!(out[150] < 0.5);
    assert_eq!(out[400], 0.0);
}

#[test]
fn a_slice_plays_its_part_forwards_or_backwards() {
    let ramp: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
    let slice = Zone {
        note: 36,
        start: 0.5,
        end: Some(0.6),
        ..Zone::default()
    };
    let mut s = sampler(vec![(slice.clone(), ramp.clone())]);
    let out = run(&mut s, &[note(0, 36, 1.0)]);
    assert!(
        (out[0] - 0.5).abs() < 1e-3 && (out[50] - 0.55).abs() < 1e-3,
        "{} {}",
        out[0],
        out[50]
    );
    assert_eq!(out[120], 0.0); // past the slice's end
    let back = Zone {
        reverse: true,
        ..slice
    };
    let mut s = sampler(vec![(back, ramp)]);
    let out = run(&mut s, &[note(0, 36, 1.0)]);
    assert!(
        out[0] > 0.59 && out[50] < out[0] && out[50] > 0.5,
        "{} {}",
        out[0],
        out[50]
    );
}

#[test]
fn a_loop_holds_while_the_note_does_and_releases_after() {
    let ramp: Vec<f32> = (0..400).map(|i| 0.1 + i as f32 / 1000.0).collect();
    let pad = Zone {
        note: 60,
        loop_mode: LoopMode::Forward,
        loop_start: Some(0.5),
        ..Zone::default()
    };
    let voice = SamplerVoice {
        release_ms: 2.0,
        ..SamplerVoice::default()
    };
    let mut s = with_voice(vec![(pad, ramp)], voice);
    // Well past the 400-sample sample, still sounding, in its loop's half.
    let out = run(&mut s, &[note(0, 60, 1.0)]);
    assert!(out[MAX_BLOCK - 1] > 0.29, "{}", out[MAX_BLOCK - 1]);
    let out = run(&mut s, &[note(0, 60, 0.0)]);
    assert!(out[0] > 0.0 && out[300] == 0.0, "{} {}", out[0], out[300]);
}

#[test]
fn layers_by_velocity_and_round_robins_take_turns() {
    let flat = SamplerVoice {
        velocity: 0.0,
        ..SamplerVoice::default()
    };
    let soft = Zone {
        note: 38,
        velocities: Some((1, 63)),
        ..Zone::default()
    };
    let hard = Zone {
        note: 38,
        velocities: Some((64, 127)),
        ..Zone::default()
    };
    let mut s = with_voice(vec![(soft, vec![0.2; 100]), (hard, vec![0.8; 100])], flat);
    assert!((run(&mut s, &[note(0, 38, 0.3)])[10] - 0.2).abs() < 1e-6);
    assert!((run(&mut s, &[note(0, 38, 0.9)])[10] - 0.8).abs() < 1e-6);
    // Two snares on the same note and velocities: one, then the other.
    let a = Zone {
        note: 38,
        ..Zone::default()
    };
    let mut s = with_voice(
        vec![(a.clone(), vec![0.3; 100]), (a.clone(), vec![0.6; 100])],
        flat,
    );
    let first = run(&mut s, &[note(0, 38, 1.0)])[10];
    let second = run(&mut s, &[note(0, 38, 1.0)])[10];
    assert!(
        (first - 0.3).abs() < 1e-6 && (second - 0.6).abs() < 1e-6,
        "{first} {second}"
    );
    // On layers of their own, they play together.
    let b = Zone {
        layer: 1,
        ..a.clone()
    };
    let mut s = with_voice(vec![(a, vec![0.3; 100]), (b, vec![0.6; 100])], flat);
    for _ in 0..2 {
        let both = run(&mut s, &[note(0, 38, 1.0)])[10];
        assert!((both - 0.9).abs() < 1e-5, "{both}");
    }
}

#[test]
fn the_envelope_rises_and_the_filter_darkens() {
    // A tone near 6.9 kHz.
    let tone: Vec<f32> = (0..48_000).map(|i| ((i as f32) * 0.9).sin()).collect();
    let zone = Zone {
        note: 60,
        ..Zone::default()
    };
    let slow = SamplerVoice {
        attack_ms: 10.0,
        ..SamplerVoice::default()
    };
    let mut s = with_voice(vec![(zone.clone(), tone.clone())], slow);
    let out = run(&mut s, &[note(0, 60, 1.0)]);
    assert!(
        peak(&out, 0, 40) < 0.2 && peak(&out, 400, 512) > 0.5,
        "{} {}",
        peak(&out, 0, 40),
        peak(&out, 400, 512)
    );
    let dark = SamplerVoice {
        filter: FilterMode::LowPass,
        cutoff_hz: 500.0,
        resonance: 0.0,
        ..SamplerVoice::default()
    };
    let mut s = with_voice(vec![(zone, tone)], dark);
    let out = run(&mut s, &[note(0, 60, 1.0)]);
    assert!(peak(&out, 200, 512) < 0.1, "{}", peak(&out, 200, 512));
}

#[test]
fn mono_legato_glides_without_starting_over() {
    let ramp: Vec<f32> = (0..96_000).map(|i| i as f32 / 48_000.0).collect();
    let bass = Zone {
        note: 24,
        high: Some(72),
        root: Some(36),
        gate: true,
        ..Zone::default()
    };
    let voice = SamplerVoice {
        mono: true,
        legato: true,
        glide_ms: 1.0,
        ..SamplerVoice::default()
    };
    let mut s = with_voice(vec![(bass, ramp)], voice);
    run(&mut s, &[note(0, 36, 1.0)]);
    let before = run(&mut s, &[]);
    // An octave up over it: the same voice, not back at the sample's start, reading twice
    // as fast once it's glided.
    let out = run(&mut s, &[note(0, 48, 1.0)]);
    assert!(
        out[0] > before[MAX_BLOCK - 1] - 0.01,
        "{} {}",
        out[0],
        before[MAX_BLOCK - 1]
    );
    let later = (0..4).map(|_| run(&mut s, &[])).last().unwrap();
    assert!(
        (slope(&later, 100) - 2.0).abs() < 0.02,
        "{}",
        slope(&later, 100)
    );
    assert_eq!(s.voices.iter().filter(|v| v.on).count(), 1);
}

#[test]
fn a_loop_is_sliced_where_its_hits_start() {
    // Four clicks a quarter of a second apart, each a decaying burst.
    let rate = 48_000.0;
    let mut x = vec![0f32; 48_000];
    for k in 0..4 {
        for i in 0..2000 {
            let n = k * 12_000 + i;
            x[n] = (i as f32 * 0.7).sin() * (-(i as f32) / 300.0).exp();
        }
    }
    let found = onsets(&x, rate);
    let cut = cuts(&found, 4);
    assert_eq!(cut.len(), 4, "{found:?}");
    for (k, c) in cut.iter().enumerate() {
        assert!((c - k as f32 * 0.25).abs() < 0.01, "{cut:?}");
    }
    let zones = slices(Path::new("loop.wav"), &cut, 36);
    assert_eq!(
        zones.iter().map(|z| z.note).collect::<Vec<_>>(),
        [36, 37, 38, 39]
    );
    assert_eq!(zones[1].end, Some(cut[2]));
    assert_eq!(zones[3].end, None);
}

#[test]
fn changed_settings_are_followed_without_a_reload() {
    let zone = Zone {
        note: 36,
        ..Zone::default()
    };
    let mut s = sampler(vec![(zone.clone(), vec![0.5; 1000])]);
    let louder = project::Sampler {
        zones: vec![Zone {
            gain_db: 6.0,
            ..zone
        }],
        voice: SamplerVoice::default(),
    };
    s.follow(&louder, 1);
    let out = run(&mut s, &[note(0, 36, 1.0)]);
    assert!(
        (out[10] - 0.5 * 10f32.powf(6.0 / 20.0)).abs() < 1e-4,
        "{}",
        out[10]
    );
}

/// `seconds` of a sine at `hz` and `level`.
fn sine(hz: f32, seconds: f32, level: f32) -> Vec<f32> {
    (0..(seconds * 48_000.0) as usize)
        .map(|i| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin() * level)
        .collect()
}

/// `frames` of both sides out of `s`, `notes` played at the start.
fn render(s: &mut Sampler, notes: &[Note], frames: usize) -> [Vec<f32>; 2] {
    let mut out = [vec![0f32; frames], vec![0f32; frames]];
    for at in (0..frames).step_by(MAX_BLOCK) {
        let n = MAX_BLOCK.min(frames - at);
        let [l, r] = &mut out;
        let first: &[Note] = if at == 0 { notes } else { &[] };
        s.process([&mut l[at..at + n], &mut r[at..at + n]], n, first, &[]);
    }
    out
}

/// The frequency of a stretch of sound, by how often it crosses zero going up.
fn frequency(x: &[f32]) -> f32 {
    let ups = x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    ups as f32 * 48_000.0 / x.len() as f32
}

fn tone(note: u8) -> Zone {
    Zone {
        note: 48,
        high: Some(84),
        root: Some(note),
        gate: true,
        ..Zone::default()
    }
}

#[test]
fn stretch_keeps_the_pitch_while_the_speed_changes_and_repitch_doesnt() {
    let slow = |mode| {
        let voice = SamplerVoice {
            mode,
            speed: 0.5,
            ..SamplerVoice::default()
        };
        let mut s = with_voice(vec![(tone(60), sine(1_000.0, 1.0, 0.5))], voice);
        render(&mut s, &[note(0, 60, 1.0)], 96_000)
    };
    let stretched = slow(SampleMode::Stretch);
    let f = frequency(&stretched[0][24_000..72_000]);
    assert!((f - 1_000.0).abs() < 15.0, "{f}");
    // Half as fast: the one-second sample still sounds a second and a half in.
    assert!(stretched[0][70_000..72_000].iter().any(|x| x.abs() > 0.1));
    let repitched = slow(SampleMode::Repitch);
    let f = frequency(&repitched[0][4_800..40_000]);
    assert!((f - 500.0).abs() < 10.0, "{f}");
}

#[test]
fn frozen_grains_hold_their_place_for_as_long_as_the_note() {
    let voice = SamplerVoice {
        mode: SampleMode::Grains,
        freeze: true,
        position: 0.5,
        density: 40.0,
        ..SamplerVoice::default()
    };
    let mut s = with_voice(vec![(tone(60), sine(440.0, 0.2, 0.5))], voice);
    // Three seconds from a fifth of a second of sound, and its playheads in the middle.
    let out = render(&mut s, &[note(0, 60, 1.0)], 144_000);
    assert!(out[0][140_000..144_000].iter().any(|x| x.abs() > 0.05));
    let playing = s.tap().read();
    assert!(playing.iter().any(|p| p.grain), "{playing:?}");
    assert!(
        playing.iter().any(|p| !p.grain && p.at == 0.5),
        "{playing:?}"
    );
    assert!(
        playing.iter().all(|p| (0.0..=1.0).contains(&p.at)),
        "{playing:?}"
    );
}

#[test]
fn a_vintage_sampler_holds_its_steps_and_its_bits() {
    let voice = SamplerVoice {
        character: SampleCharacter::Vintage,
        crush_khz: 12.0,
        bits: 4,
        ..SamplerVoice::default()
    };
    let mut s = with_voice(vec![(tone(60), sine(300.0, 0.5, 0.9))], voice);
    let out = render(&mut s, &[note(0, 60, 1.0)], 4_800);
    // A new value no more often than every four samples (12 kHz of 48), each one of 4 bits'
    // steps.
    let changes = out[0][100..4_100]
        .windows(2)
        .filter(|w| w[0] != w[1])
        .count();
    assert!((300..=1_000).contains(&changes), "{changes}");
    assert!(
        out[0][100..4_100]
            .iter()
            .all(|x| ((x * 8.0) - (x * 8.0).round()).abs() < 1e-4)
    );
}

#[test]
fn unison_spreads_the_voices_and_keeps_the_level() {
    let voice = SamplerVoice {
        unison: 4,
        detune_cents: 20.0,
        spread: 1.0,
        ..SamplerVoice::default()
    };
    let mut s = with_voice(vec![(tone(60), sine(300.0, 1.0, 0.5))], voice);
    let out = render(&mut s, &[note(0, 60, 1.0)], 24_000);
    let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
    let (l, r) = (rms(&out[0][2_000..24_000]), rms(&out[1][2_000..24_000]));
    // Wide: the sides differ; about as loud as one voice (0.35 rms), not four times.
    assert!(out[0][5_000..6_000] != out[1][5_000..6_000]);
    assert!(
        (0.15..0.6).contains(&l) && (0.15..0.6).contains(&r),
        "{l} {r}"
    );
}

#[test]
fn layers_blend_over_their_velocity_fades_and_release_samples_play_as_it_is_let_go() {
    let flat = SamplerVoice {
        velocity: 0.0,
        ..SamplerVoice::default()
    };
    let soft = Zone {
        note: 38,
        velocities: Some((1, 63)),
        velocity_fade: 20,
        ..Zone::default()
    };
    let hard = Zone {
        velocities: Some((64, 127)),
        ..soft.clone()
    };
    let squeak = Zone {
        note: 38,
        trigger: Trigger::Release,
        ..Zone::default()
    };
    let mut s = with_voice(
        vec![
            (soft, vec![0.2; 5_000]),
            (hard, vec![0.4; 5_000]),
            (squeak, vec![0.05; 5_000]),
        ],
        flat,
    );
    // At 64: the hard layer whole, the soft one fading out, both sounding.
    let both = run(&mut s, &[note(0, 38, 64.0 / 127.0)])[10];
    assert!(both > 0.42 && both < 0.6, "{both}");
    // Far up: the hard layer alone; and the release sample as the note's let go.
    let mut s2 = s;
    let hard_only = run(&mut s2, &[note(0, 38, 0.0), note(10, 38, 1.0)])[200];
    assert!(hard_only > 0.4, "{hard_only}");
    let off = run(&mut s2, &[note(0, 38, 0.0)]);
    assert!(off[10] > 0.4 + 0.04, "{}", off[10]);
}

#[test]
fn a_crossfaded_loop_seam_doesnt_jump() {
    // A ramp looped from its middle: the jump back is from 1 down to 0.5, unless faded.
    let ramp: Vec<f32> = (0..4_000).map(|i| i as f32 / 4_000.0).collect();
    let jump = |fade: f32| {
        let z = Zone {
            note: 60,
            loop_mode: LoopMode::Forward,
            loop_start: Some(0.5),
            loop_fade: fade,
            ..Zone::default()
        };
        let mut s = sampler(vec![(z, ramp.clone())]);
        let out = render(&mut s, &[note(0, 60, 1.0)], 8_000);
        out[0][1_000..8_000]
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0f32, f32::max)
    };
    let (hard, faded) = (jump(0.0), jump(0.5));
    assert!(hard > 0.4, "{hard}");
    assert!(faded < 0.01, "{faded}");
}

#[test]
fn played_high_it_doesnt_alias() {
    // 15 kHz two octaves up is 60 kHz: over Nyquist, nothing should come back down.
    let mut s = sampler(vec![(tone(60), sine(15_000.0, 0.5, 0.5))]);
    let out = render(&mut s, &[note(0, 84, 1.0)], 4_000);
    let peak = out[0][500..3_000].iter().fold(0f32, |m, x| m.max(x.abs()));
    assert!(peak < 0.05, "{peak}");
}

#[test]
fn the_sub_adds_an_octave_under_and_pitch_is_heard() {
    let voice = SamplerVoice {
        sub: 1.0,
        ..SamplerVoice::default()
    };
    // Silence with the sub under it: a sine at half the note's frequency (A3 at A4's note).
    let mut s = with_voice(vec![(tone(69), vec![0.0; 48_000])], voice);
    let out = render(&mut s, &[note(0, 69, 1.0)], 24_000);
    let f = frequency(&out[0][2_000..24_000]);
    assert!((f - 220.0).abs() < 3.0, "{f}");
    let hz = pitch(&sine(220.0, 1.0, 0.5), 48_000.0).unwrap();
    assert!((hz - 220.0).abs() < 1.0, "{hz}");
    assert_eq!(note_of(hz).0, 57);
}

/// How much of real time a sampler takes, playing hard: `cargo test --release -p pina-engine
/// sampler_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn sampler_cost() {
    let time = |voice: SamplerVoice, notes: u8| {
        let mut s = with_voice(vec![(tone(60), sine(220.0, 4.0, 0.5))], voice);
        let chord: Vec<Note> = (0..notes).map(|k| note(0, 48 + k * 3, 0.8)).collect();
        let began = std::time::Instant::now();
        render(&mut s, &chord, 48_000);
        began.elapsed().as_secs_f64() * 100.0
    };
    let plain = time(SamplerVoice::default(), 8);
    let full = SamplerVoice {
        filter: FilterMode::Ladder,
        drive: 0.5,
        unison: 4,
        sub: 0.3,
        fm: 0.2,
        ..SamplerVoice::default()
    };
    let rich = time(full, 8);
    let grains = time(
        SamplerVoice {
            mode: SampleMode::Grains,
            density: 60.0,
            grain_ms: 120.0,
            ..SamplerVoice::default()
        },
        8,
    );
    eprintln!(
        "% of real time, 8 notes: plain {plain:.2}, ladder+unison 4+sub+fm {rich:.2}, grains {grains:.2}"
    );
}
