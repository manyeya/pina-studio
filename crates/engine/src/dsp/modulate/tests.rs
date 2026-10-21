use super::*;

fn input(beats: Option<f64>) -> ModInput {
    ModInput {
        beats,
        seconds: 0.01,
        tempo: 120.0,
        ..Default::default()
    }
}

#[test]
fn waves_rates_and_steps_follow_the_song() {
    // A beat-long sine: 0 on the beat, 1 a quarter of the way, -1 three quarters.
    let lfo = ModKind::lfo();
    let mut s = ModState::default();
    assert!(step(&lfo, &mut s, &input(Some(4.0))).abs() < 1e-4);
    assert!((step(&lfo, &mut s, &input(Some(4.25))) - 1.0).abs() < 1e-4);
    assert!((step(&lfo, &mut s, &input(Some(4.75))) + 1.0).abs() < 1e-4);
    for shape in LfoShape::ALL {
        for k in 0..20 {
            let v = shape.at(k as f64 / 20.0);
            assert!((-1.0..=1.0).contains(&v), "{shape:?} {v}");
        }
    }
    assert_eq!(LfoShape::Triangle.at(0.25), 1.0);
    assert_eq!(LfoShape::Triangle.at(0.75), -1.0);
    // Sixteenths of values: the third sixteenth's.
    let steps = ModKind::steps();
    let mut s = ModState::default();
    assert_eq!(step(&steps, &mut s, &input(Some(0.5))), 0.6);
    // Rates as musicians say them.
    let labels: Vec<String> = [16.0, 4.0, 1.0, 0.5, 0.375, 1.0 / 3.0, 0.25]
        .map(|b| ModRate::Beats(b).label())
        .into();
    assert_eq!(
        labels,
        ["4 bars", "1 bar", "1/4", "1/8", "1/16.", "1/8T", "1/16"]
    );
}

#[test]
fn an_envelope_rises_with_a_note_and_a_follower_with_a_level() {
    let env = ModKind::Envelope {
        attack_ms: 10.0,
        decay_ms: 100.0,
        sustain: 0.5,
        release_ms: 100.0,
    };
    let mut s = ModState::default();
    let on = ModInput {
        trigger: true,
        gate: true,
        ..input(None)
    };
    let held = ModInput {
        gate: true,
        ..input(None)
    };
    assert_eq!(step(&env, &mut s, &on), 1.0); // a 10 ms attack in a 10 ms block
    for _ in 0..30 {
        step(&env, &mut s, &held);
    }
    assert!((s.value - 0.5).abs() < 1e-4, "{}", s.value); // held at the sustain
    for _ in 0..12 {
        step(&env, &mut s, &input(None));
    }
    assert_eq!(s.value, 0.0); // released
    // A follower: up to a loud track fast, down slowly.
    let follow = ModKind::follower(1);
    let mut s = ModState::default();
    let loud = ModInput {
        follow: 1.0,
        ..input(None)
    };
    for _ in 0..5 {
        step(&follow, &mut s, &loud);
    }
    assert!(s.value > 0.99);
    step(&follow, &mut s, &input(None));
    assert!(s.value > 0.9);
}

#[test]
fn offsets_move_settings_in_their_units_and_stay_in_range() {
    let mut o = Offsets::default();
    o.add(ModTarget::Sampler(SamplerParam::Cutoff), 1.0);
    o.add(ModTarget::Sampler(SamplerParam::Cutoff), 1.0);
    o.add(ModTarget::Plugin(Which::Insert(0), 7), 0.25);
    o.add(ModTarget::EqGain(2), 30.0);
    let mut v = SamplerVoice {
        cutoff_hz: 1000.0,
        ..Default::default()
    };
    o.voice(&mut v);
    assert_eq!(v.cutoff_hz, 4000.0); // two octaves up
    assert_eq!(o.plugin(Which::Insert(0), 7), Some(0.25));
    o.add(ModTarget::LowCut, 2.0);
    let mut eq = Eq::classic();
    o.eq(&mut eq);
    // The old EQ's high mid, band 2; and its low cut, off, swept up from 20 Hz.
    assert_eq!(eq.bands[2].gain_db, 30.0);
    assert_eq!((eq.bands[0].freq, eq.bands[0].off), (80.0, false));
    assert_eq!(apply_pan(0.8, 0.5), 1.0);
    assert_eq!(depth_text(ModTarget::Volume, -12.0), "−12.0 dB");
}

#[test]
fn automation_moves_a_modulators_settings_over_its_own() {
    // A macro set at 0, automated to 0.75: it gives 0.75.
    let mac = ModKind::Macro { value: 0.0 };
    let mut over = ModOver::default();
    over.set(ModParam::Value, 0.75);
    let mut s = ModState::default();
    assert_eq!(step_over(&mac, &mut s, &input(None), &over), 0.75);
    assert_eq!(step(&mac, &mut s, &input(None)), 0.0);
    // An LFO's phase turned a quarter: a sine at its start reads 1, not 0.
    let lfo = ModKind::lfo();
    let mut over = ModOver::default();
    over.set(ModParam::Phase, 0.25);
    let at_start = step_over(&lfo, &mut ModState::default(), &input(Some(0.0)), &over);
    assert!((at_start - 1.0).abs() < 1e-3, "{at_start}");
    // Out of range, kept in it; a setting it hasn't, none to get or set.
    over.set(ModParam::Value, 3.0);
    assert_eq!(step_over(&mac, &mut s, &input(None), &over), 1.0);
    assert_eq!(ModParam::Attack.get(&mac), None);
    assert_eq!(ModParam::of(&lfo), [ModParam::Phase]);
    assert_eq!(ModParam::parse("Release"), Some(ModParam::Release));
}
