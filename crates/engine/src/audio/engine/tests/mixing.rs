//! Mixing as the engine plays it: volume, pan and pan laws, binaural, sends, mute and solo, the strip's effects.

use super::*;

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
