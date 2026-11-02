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
fn a_limiter_plays_its_automation_and_its_modulation() {
    use crate::dsp::modulate::{ModKind, ModRoute, ModTarget, Modulator};
    use crate::song::project::{LimitParam, Limiter};
    let mut t = synth_track("a", vec![note(0, PPQ * 2, 69)], PPQ * 2);
    t.volume_db = -12.0;
    let quiet = Limiter {
        ceiling_db: -20.0,
        ..Limiter::default()
    };
    t.limit = Some(quiet);
    let level =
        |t: &Track| peak(&render(&project(vec![t.clone()]), Path::new("."), BEAT)[0][BEAT / 2..]);
    let low = level(&t);
    // Its ceiling automated 6 dB up all along: 6 dB louder (the synth peaks at -12 dBFS, still
    // over it).
    let mut raised = t.clone();
    *raised
        .automation
        .points_mut(Param::Limit(LimitParam::Ceiling)) = vec![AutoPoint::new(0, -14.0)];
    let up = level(&raised);
    assert!((up / low - 2.0).abs() < 0.05, "{low} {up}");
    // A macro turned all the way, moving it the same.
    let mut moved = t.clone();
    moved.modulators = vec![Modulator {
        kind: ModKind::Macro { value: 1.0 },
        routes: vec![ModRoute {
            target: ModTarget::LimitCeiling,
            depth: 6.0,
        }],
    }];
    let up = level(&moved);
    assert!((up / low - 2.0).abs() < 0.05, "{low} {up}");
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
    let mut sat = Saturator::default();
    sat.bands[0].drive_db = 24.0;
    hot.master_sat = Some(sat);
    let wet = peak(&render(&hot, Path::new("."), BEAT)[0][BEAT / 2..]);
    assert!(wet < dry * 0.6, "{dry} {wet}");
}

#[test]
fn a_channels_effects_run_in_its_order() {
    // A 12 dB boost and a hard compressor: boosted then squashed comes out quieter than
    // squashed then boosted.
    let mut t = synth_track("a", vec![note(0, PPQ * 2, 69)], PPQ * 2);
    t.eq = Some(Eq {
        bands: crate::song::project::Bands::from_slice(&[crate::song::project::EqBand::new(
            crate::song::project::Shape::Bell,
            440.0,
            12.0,
        )]),
        ..Eq::default()
    });
    t.comp = Some(Comp {
        threshold_db: -30.0,
        ratio: 20.0,
        ..Comp::default()
    });
    let level =
        |t: &Track| peak(&render(&project(vec![t.clone()]), Path::new("."), BEAT)[0][BEAT / 2..]);
    let eq_first = level(&t);
    t.chain = vec![crate::song::project::Fx::Comp, crate::song::project::Fx::Eq];
    let comp_first = level(&t);
    assert!(comp_first > eq_first * 3.0, "{eq_first} {comp_first}");
}

#[test]
fn inserts_put_in_order_in_place_stay_themselves() {
    let mut items = vec!['a', 'b', 'c', 'd', 'e'];
    reorder(&mut items, &[4, 0, 3, 1, 2]);
    assert_eq!(items, ['e', 'a', 'd', 'b', 'c']);
}

#[test]
fn inserts_rearranged_in_place_keep_those_kept_and_give_back_the_rest() {
    /// A processor known by its latency.
    struct Tag(u32);
    impl crate::plugin::ProcessorApi for Tag {
        fn latency(&self) -> u32 {
            self.0
        }
        fn set_transport(&mut self, _: bool, _: u64, _: f64, _: f64) {}
        fn automate(&mut self, _: u32, _: f64, _: f64) {}
        fn pending(&self) -> bool {
            false
        }
        fn process(
            &mut self,
            _: [&mut [f32]; 2],
            _: usize,
            _: &[Note],
            _: &[Control],
            _: Option<[&[f32]; 2]>,
        ) {
        }
    }
    let mut inserts: Vec<Processor> = vec![Box::new(Tag(1)), Box::new(Tag(2)), Box::new(Tag(3))];
    // The third first, a new one, then the first; the second taken out.
    let mut r = Rearrange {
        slot: 0,
        seq: 1,
        from: vec![Some(2), None, Some(0)],
        fresh: vec![None, Some(Box::new(Tag(9))), None],
        old: (0..3).map(|_| None).collect(),
        out: Vec::with_capacity(3),
        gone: Vec::with_capacity(3),
    };
    r.apply(&mut inserts);
    assert_eq!(
        inserts.iter().map(|p| p.latency()).collect::<Vec<_>>(),
        [3, 9, 1]
    );
    assert_eq!(r.gone.iter().map(|p| p.latency()).collect::<Vec<_>>(), [2]);
}

#[test]
fn stacked_built_ins_play_where_the_chain_has_them() {
    use crate::song::project::{Fx, Unit};
    let t = synth_track("a", vec![note(0, PPQ * 2, 69)], PPQ * 2);
    let level =
        |t: &Track| peak(&render(&project(vec![t.clone()]), Path::new("."), BEAT)[0][BEAT / 2..]);
    let dry = level(&t);
    // Another saturator, driven hard: it rounds the sine off.
    let mut hot = Saturator::default();
    hot.bands[0].drive_db = 24.0;
    let mut stacked = t.clone();
    stacked.more = vec![Unit::Sat(hot)];
    let wet = level(&stacked);
    assert!(wet < dry * 0.6, "{dry} {wet}");
    // Two of them, one after the other, round it off further.
    stacked.more.push(Unit::Sat(hot));
    let twice = level(&stacked);
    assert!(twice < wet, "{wet} {twice}");
    // A boost then a hard compressor of its own, against the compressor first: the order
    // holds for the stacked ones too.
    let mut ordered = t.clone();
    ordered.more = vec![
        Unit::Eq(Eq {
            bands: crate::song::project::Bands::from_slice(&[crate::song::project::EqBand::new(
                crate::song::project::Shape::Bell,
                440.0,
                12.0,
            )]),
            ..Eq::default()
        }),
        Unit::Comp(Comp {
            threshold_db: -30.0,
            ratio: 20.0,
            ..Comp::default()
        }),
    ];
    let eq_first = level(&ordered);
    ordered.chain = vec![Fx::Eq, Fx::Comp, Fx::Sat, Fx::Limit];
    ordered.more.swap(0, 1);
    let comp_first = level(&ordered);
    assert!(comp_first > eq_first * 3.0, "{eq_first} {comp_first}");
    // Bypassed, it's as if it weren't there.
    let mut off = hot;
    off.bypass = true;
    let mut skipped = t.clone();
    skipped.more = vec![Unit::Sat(off)];
    assert!((level(&skipped) - dry).abs() < 1e-6);
}

#[test]
fn a_reverb_and_a_delay_ring_on_after_the_sound_stops() {
    use crate::song::project::{Delay, Reverb, Unit};
    let t = synth_track("a", vec![note(0, PPQ / 4, 69)], PPQ / 4);
    let tail = |more: Vec<Unit>| {
        let mut t = t.clone();
        t.more = more;
        // A beat on: the note's long gone.
        peak(&render(&project(vec![t]), Path::new("."), BEAT * 2)[0][BEAT..])
    };
    assert!(tail(Vec::new()) < 1e-4);
    let verb = Reverb {
        mix: 1.0,
        decay_s: 3.0,
        ..Reverb::default()
    };
    assert!(tail(vec![Unit::Reverb(verb)]) > 1e-3);
    // An eighth's echo, then its repeats.
    let echo = Delay {
        mix: 1.0,
        feedback: 0.6,
        ..Delay::default()
    };
    assert!(tail(vec![Unit::Delay(echo)]) > 1e-2);
}

#[test]
fn the_tools_stacked_on_a_track_do_what_they_say() {
    use crate::song::project::{
        DeEsser, FilterFx, FilterMode, Gate, Motion, MotionStyle, Shaper, Stereo, Unit,
    };
    let t = synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8);
    let heard = |u: Option<Unit>| {
        let mut t = t.clone();
        t.more = u.into_iter().collect();
        let [l, r] = render(&project(vec![t]), Path::new("."), BEAT * 2);
        [peak(&l[BEAT / 2..]), peak(&r[BEAT / 2..])]
    };
    let [dry, _] = heard(None);
    assert!(dry > 0.01, "{dry}");
    // A low-pass far under the note, the note gone.
    let low = heard(Some(Unit::Filter(FilterFx {
        mode: FilterMode::LowPass,
        cutoff_hz: 60.0,
        ..FilterFx::default()
    })));
    assert!(low[0] < dry * 0.3, "{dry} {low:?}");
    // A gate that never opens, down by its range.
    let shut = heard(Some(Unit::Gate(Gate {
        threshold_db: 0.0,
        range_db: 40.0,
        ..Gate::default()
    })));
    assert!(shut[0] < dry * 0.05, "{dry} {shut:?}");
    // A de-esser listening to its band: a 440 Hz sine has nothing there.
    let band = heard(Some(Unit::DeEsser(DeEsser {
        listen: true,
        freq_hz: 9000.0,
        ..DeEsser::default()
    })));
    assert!(band[0] < dry * 0.05, "{dry} {band:?}");
    // The shaper's output, the stereo tool's balance, a tremolo all the way down.
    let quiet = heard(Some(Unit::Shaper(Shaper {
        output_db: -12.0,
        ..Shaper::default()
    })));
    assert!((quiet[0] / dry - 0.25).abs() < 0.03, "{dry} {quiet:?}");
    let left = heard(Some(Unit::Stereo(Stereo {
        balance: -1.0,
        ..Stereo::default()
    })));
    assert!(
        left[1] < dry * 0.02 && left[0] > dry * 0.5,
        "{dry} {left:?}"
    );
    let moved = heard(Some(Unit::Motion(Motion {
        rate_hz: 4.0,
        ..MotionStyle::Tremolo.motion()
    })));
    assert!(moved[0] <= dry * 1.01, "{dry} {moved:?}");
}

#[test]
fn a_stacked_effects_knob_follows_its_automation_and_its_modulators() {
    use crate::dsp::modulate::{ModKind, ModRoute, ModTarget, Modulator};
    use crate::song::project::{AutoPoint, FilterFx, FilterMode, Param, Unit};
    let mut t = synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8);
    t.more = vec![Unit::Filter(FilterFx {
        mode: FilterMode::LowPass,
        cutoff_hz: 20_000.0,
        ..FilterFx::default()
    })];
    let heard = |t: &Track| {
        peak(&render(&project(vec![t.clone()]), Path::new("."), BEAT * 2)[0][BEAT / 2..])
    };
    let open = heard(&t);
    assert!(open > 0.01, "{open}");
    // Cutoff is its first knob: all the way down, the note's gone.
    let mut shut = t.clone();
    shut.automation
        .points_mut(Param::Tool(0, 0))
        .push(AutoPoint::new(0, 0.0));
    assert!(heard(&shut) < open * 0.2, "{open} {}", heard(&shut));
    // A macro, turned all the way, takes it down its travel the same.
    let mut moved = t.clone();
    moved.modulators = vec![Modulator {
        kind: ModKind::Macro { value: 1.0 },
        routes: vec![ModRoute {
            target: ModTarget::Tool(0, 0),
            depth: -1.0,
        }],
    }];
    assert!(heard(&moved) < open * 0.2, "{open} {}", heard(&moved));
}

#[test]
fn a_tracks_own_compressor_and_a_stacked_eq_follow_their_automation() {
    use crate::song::project::{AutoPoint, Bands, Comp, Eq, EqBand, OWN_COMP, Param, Shape, Unit};
    let mut t = synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8);
    let heard = |t: &Track| {
        peak(&render(&project(vec![t.clone()]), Path::new("."), BEAT * 2)[0][BEAT / 2..])
    };
    let dry = heard(&t);
    // Its own compressor, its threshold brought all the way down.
    t.comp = Some(Comp {
        threshold_db: 0.0,
        ratio: 20.0,
        ..Comp::default()
    });
    let open = heard(&t);
    t.automation
        .points_mut(Param::Tool(OWN_COMP, 0))
        .push(AutoPoint::new(0, 0.0));
    let squeezed = heard(&t);
    assert!(squeezed < open * 0.3, "{open} {squeezed}");
    // A stacked EQ's bell, its gain thrown up (band one's gain is its second knob).
    let mut e = synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8);
    e.more = vec![Unit::Eq(Eq {
        bands: Bands::from_slice(&[EqBand::new(Shape::Bell, 440.0, 0.0)]),
        ..Eq::default()
    })];
    e.automation
        .points_mut(Param::Tool(0, 1))
        .push(AutoPoint::new(0, 0.7));
    assert!(heard(&e) > dry * 2.0, "{dry} {}", heard(&e));
}
