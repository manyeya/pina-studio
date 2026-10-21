use super::*;
use crate::audio::engine::{AudioData, MAX_BLOCK};

fn sampler(zones: Vec<(Zone, Vec<f32>)>) -> Sampler {
    with_voice(zones, SamplerVoice::default())
}

fn with_voice(zones: Vec<(Zone, Vec<f32>)>, voice: SamplerVoice) -> Sampler {
    let zones = zones
        .into_iter()
        .map(|(zone, samples)| Loaded {
            p: Params::of(&zone),
            data: Arc::new(AudioData::mono(samples)),
        })
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
    let mut s = with_voice(vec![(a.clone(), vec![0.3; 100]), (a, vec![0.6; 100])], flat);
    let first = run(&mut s, &[note(0, 38, 1.0)])[10];
    let second = run(&mut s, &[note(0, 38, 1.0)])[10];
    assert!(
        (first - 0.3).abs() < 1e-6 && (second - 0.6).abs() < 1e-6,
        "{first} {second}"
    );
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
