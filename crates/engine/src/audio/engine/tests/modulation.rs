//! Modulation and plugin automation as the engine plays them.

use super::*;

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
