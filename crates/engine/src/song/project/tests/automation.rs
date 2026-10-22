//! Automation: shapes, smoothing, order, recording, and lanes going with their plugin.

use super::*;

#[test]
fn automation_bends_steps_and_takes_shapes() {
    let p = |tick, value| AutoPoint::new(tick, value);
    // Straight, bent slow-then-fast, and a step.
    let line = [p(0, 0.0), p(100, 1.0)];
    assert_eq!(value_at(&line, 50.0), Some(0.5));
    let slow = [
        AutoPoint {
            curve: 0.8,
            ..p(0, 0.0)
        },
        p(100, 1.0),
    ];
    let fast = [
        AutoPoint {
            curve: -0.8,
            ..p(0, 0.0)
        },
        p(100, 1.0),
    ];
    assert!(value_at(&slow, 50.0).unwrap() < 0.2);
    assert!(value_at(&fast, 50.0).unwrap() > 0.8);
    assert_eq!(value_at(&slow, 100.0), Some(1.0));
    let step = [
        AutoPoint {
            step: true,
            ..p(0, 0.0)
        },
        p(100, 1.0),
    ];
    assert_eq!(value_at(&step, 99.0), Some(0.0));
    assert_eq!(value_at(&step, 100.0), Some(1.0));

    // A sine over a bar, a cycle a beat, drawn into a line that was flat at -6 dB: it stays
    // -6 either side, and swings between -12 and 0 inside.
    let mut a = Automation {
        volume: vec![p(0, -6.0), p(PPQ * 8, -6.0)],
        ..Default::default()
    };
    a.shape(
        Param::Volume,
        AutoShape::Sine,
        PPQ * 2,
        PPQ * 6,
        PPQ,
        -12.0,
        0.0,
    );
    let v = |t: u64| value_at(&a.volume, t as f64).unwrap();
    assert_eq!(v(PPQ), -6.0);
    assert_eq!(v(PPQ * 7), -6.0);
    assert!((v(PPQ * 2 + PPQ / 4) - 0.0).abs() < 1e-4);
    assert!((v(PPQ * 2 + PPQ * 3 / 4) + 12.0).abs() < 1e-4);
    assert!(
        (v(PPQ * 2 + PPQ / 8) + 1.8).abs() < 0.8,
        "{}",
        v(PPQ * 2 + PPQ / 8)
    );
    // A square steps; mute is always steps.
    a.shape(Param::Mute, AutoShape::Square, 0, PPQ * 2, PPQ, 0.0, 1.0);
    assert_eq!(value_at(&a.mute, (PPQ / 2 - 1) as f64), Some(1.0));
    assert_eq!(value_at(&a.mute, (PPQ / 2) as f64), Some(0.0));
    assert!(a.mute.iter().all(|p| p.step));

    // Thinning takes out what changes nothing, and keeps the shape.
    let mut a = Automation {
        pan: (0..=100)
            .map(|i| p(i * 10, (i as f32 / 100.0).min(0.5)))
            .collect(),
        ..Default::default()
    };
    a.thin(Param::Pan, 0.002);
    assert_eq!(a.pan.len(), 3, "{:?}", a.pan);
    assert_eq!(value_at(&a.pan, 250.0), Some(0.25));

    // Lifted out and put down elsewhere: the same line there.
    let mut a = Automation {
        volume: vec![p(0, -20.0), p(PPQ * 4, 0.0)],
        pan: vec![p(PPQ, -1.0), p(PPQ * 3, 1.0)],
        ..Default::default()
    };
    let clip = a.take(PPQ, PPQ * 3);
    assert_eq!(clip.length, PPQ * 2);
    a.put(PPQ * 8, &clip);
    for t in [0, PPQ / 2, PPQ, PPQ * 3 / 2, PPQ * 2 - 1] {
        let (was, now) = (PPQ + t, PPQ * 8 + t);
        assert!(
            (value_at(&a.volume, was as f64).unwrap() - value_at(&a.volume, now as f64).unwrap())
                .abs()
                < 0.01
        );
        assert!(
            (value_at(&a.pan, was as f64).unwrap() - value_at(&a.pan, now as f64).unwrap()).abs()
                < 0.01
        );
    }
    // Trimmed: all of it up, never past the top.
    a.offset(Param::Volume, 10.0);
    assert_eq!(a.volume[0].value, -10.0);
    assert!(a.volume.iter().all(|p| p.value <= 6.0));
    // Erased: the line either side holds.
    a.erase(PPQ * 8, PPQ * 10);
    assert!(
        a.pan
            .iter()
            .all(|p| !(PPQ * 8 + 1..PPQ * 10).contains(&p.tick))
    );
}

#[test]
fn smoothing_rounds_corners_and_wobbles_off_and_leaves_its_ends() {
    let p = |tick, value| AutoPoint::new(tick, value);
    // An S eases out and in: slow at both ends, halfway at the middle.
    let s = [
        AutoPoint {
            ease: true,
            ..p(0, 0.0)
        },
        p(100, 1.0),
    ];
    assert!(value_at(&s, 10.0).unwrap() < 0.05);
    assert_eq!(value_at(&s, 50.0), Some(0.5));
    assert!(value_at(&s, 90.0).unwrap() > 0.95);

    // A pan recorded with a wobble every sixteenth over two bars, flat either side.
    let wobble: Vec<AutoPoint> = (0..=32)
        .map(|i| p(PPQ * 2 + i * PPQ / 4, if i % 2 == 0 { -0.2 } else { 0.2 }))
        .collect();
    let mut a = Automation {
        pan: [vec![p(0, -0.2)], wobble, vec![p(PPQ * 16, -0.2)]].concat(),
        ..Default::default()
    };
    let before = a.pan.clone();
    a.smooth(Param::Pan, PPQ * 2, PPQ * 10, PPQ / 2);
    let v = |a: &Automation, t: u64| value_at(&a.pan, t as f64).unwrap();
    // The wobble's gone (it swung 0.4), the ends are where they were, outside untouched.
    let swing = (PPQ * 3..PPQ * 9)
        .step_by(30)
        .map(|t| v(&a, t))
        .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
    assert!(swing.1 - swing.0 < 0.05, "{swing:?}");
    assert_eq!(v(&a, PPQ * 2), -0.2);
    assert!((v(&a, PPQ * 10) - value_at(&before, (PPQ * 10) as f64).unwrap()).abs() < 1e-3);
    assert_eq!(
        v(&a, PPQ * 12),
        v(
            &Automation {
                pan: before.clone(),
                ..Default::default()
            },
            PPQ * 12
        )
    );
    assert!(a.pan.len() < before.len(), "{}", a.pan.len());

    // A sharp peak in the volume, rounded off: lower at the top, still rising to it; the
    // silence at its ends stays silence.
    let mut a = Automation {
        volume: vec![p(0, -144.0), p(PPQ * 4, 0.0), p(PPQ * 8, -144.0)],
        ..Default::default()
    };
    a.smooth(Param::Volume, 0, PPQ * 8, PPQ);
    let peak = value_at(&a.volume, (PPQ * 4) as f64).unwrap();
    assert!(peak < -0.5 && peak > -30.0, "{peak}");
    let rise = |t: u64| value_at(&a.volume, t as f64).unwrap();
    assert!(rise(PPQ * 2) < rise(PPQ * 3) && rise(PPQ * 3) < rise(PPQ * 4));
    assert_eq!(value_at(&a.volume, 0.0), Some(-144.0));
    assert_eq!(value_at(&a.volume, (PPQ * 8) as f64), Some(-144.0));
    // Mute's steps stay steps.
    let mut a = Automation {
        mute: vec![
            AutoPoint {
                step: true,
                ..p(0, 0.0)
            },
            AutoPoint {
                step: true,
                ..p(PPQ, 1.0)
            },
        ],
        ..Default::default()
    };
    let was = a.mute.clone();
    a.smooth(Param::Mute, 0, PPQ, PPQ);
    assert_eq!(a.mute, was);
}

#[test]
fn automation_points_stay_in_order_and_interpolate() {
    let mut a = Automation::default();
    assert_eq!(value_at(a.points(Param::Volume), 0.0), None);
    a.add(Param::Volume, AutoPoint::new(960, -12.0));
    a.add(Param::Volume, AutoPoint::new(0, 0.0));
    assert_eq!(a.add(Param::Volume, AutoPoint::new(480, -6.0)), 1);
    let v = a.points(Param::Volume);
    assert_eq!(value_at(v, 240.0), Some(-3.0));
    assert_eq!(value_at(v, 5000.0), Some(-12.0));
    a.set(Param::Volume, 1, 2000, 0.0); // can't pass the point after it
    assert_eq!(a.points(Param::Volume)[1].tick, 960);
    assert!(a.points(Param::Pan).is_empty());
}

#[test]
fn recording_automation_replaces_the_line_it_passes() {
    let mut a = Automation::default();
    for tick in [0, 3840] {
        a.add(Param::Volume, AutoPoint::new(tick, -6.0));
    }
    a.write(Param::Volume, None, 960, 0.0);
    a.write(Param::Volume, Some(960), 1440, -3.0);
    a.write(Param::Volume, Some(1440), 1920, -3.0);
    a.write(Param::Volume, Some(1920), 2400, -3.0); // held still: the last point moves on
    let points: Vec<(u64, f32)> = a.volume.iter().map(|p| (p.tick, p.value)).collect();
    assert_eq!(
        points,
        [
            (0, -6.0),
            (959, -6.0), // the line as it was, up to where recording began
            (960, 0.0),
            (1440, -3.0),
            (2400, -3.0),
            (3840, -6.0)
        ]
    );
    // Plugin parameters record the same way, in lanes of their own.
    let cutoff = Param::Plugin(Which::Instrument, 7);
    a.write(cutoff, None, 0, 0.25);
    a.name(cutoff, "Cutoff");
    assert_eq!(a.points(cutoff).len(), 1);
    assert_eq!(a.plugins[0].name, "Cutoff");
    assert_eq!(a.params(), [Param::Volume, cutoff]);
}

#[test]
fn a_slots_automation_goes_with_its_plugin() {
    let plugin = |class: &str| PluginRef {
        path: "/p.vst3".into(),
        class: class.into(),
        ..Default::default()
    };
    let mut t = Track {
        inserts: vec![plugin("a"), plugin("b")],
        ..Default::default()
    };
    for i in 0..2 {
        t.automation.add(
            Param::Plugin(Which::Insert(i), 1),
            AutoPoint::new(0, i as f32),
        );
    }
    t.set_insert(0, None).unwrap();
    assert_eq!(t.automation.plugins.len(), 1);
    assert_eq!(t.automation.plugins[0].plugin, Which::Insert(0)); // b's, moved up
    assert_eq!(t.automation.plugins[0].points[0].value, 1.0);
    // The same plugin with other settings keeps it; another plugin doesn't.
    t.set_insert(0, Some(plugin("b"))).unwrap();
    assert_eq!(t.automation.plugins.len(), 1);
    t.set_insert(0, Some(plugin("c"))).unwrap();
    assert!(t.automation.plugins.is_empty());
}
