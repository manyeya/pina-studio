//! Kits as stacks: a kit track, a track a pad, its notes split out onto them.

use super::*;

fn pad(name: &str, note: u8, choke: Option<u8>, steps: &[usize]) -> NewPad {
    NewPad {
        name: name.into(),
        sampler: Sampler {
            zones: vec![Zone {
                file: format!("{name}.wav").into(),
                note,
                ..Default::default()
            }],
            ..Default::default()
        },
        note,
        choke,
        steps: steps.to_vec(),
    }
}

#[test]
fn a_kit_is_a_stack_of_its_pads() {
    let mut p = project(vec![midi(0, PPQ, &[])], true);
    let made = p.add_kit(
        "Kit",
        vec![
            pad("Kick", 36, None, &[0, 8]),
            pad("Hat", 42, Some(1), &[2, 6]),
        ],
        1,
        0,
        true,
    );
    // The kit track, then a track a pad, all in the stack called the kit's name.
    assert_eq!(made.kit, 1);
    assert_eq!(made.pads, [2, 3]);
    let stack = p.auxes.iter().find(|a| a.bus == made.bus).unwrap();
    assert!(stack.stack && stack.name == "Kit");
    for t in [1, 2, 3] {
        assert_eq!(p.tracks[t].output, Some(made.bus));
    }
    assert_eq!(p.tracks[1].instrument, Some(Instrument::Kit));
    let kit = p.tracks[1].id;
    assert_eq!(
        p.tracks[3].pad,
        Some(Pad {
            kit,
            note: 42,
            choke: Some(1)
        })
    );
    // Its starter pattern: a row a pad, on their steps.
    let Content::Pattern(pt) = &p.tracks[1].regions[0].content else {
        panic!()
    };
    assert_eq!(
        pt.rows.iter().map(|r| r.pitch).collect::<Vec<_>>(),
        [36, 42]
    );
    assert!(pt.rows[0].on(0) && pt.rows[0].on(8) && pt.rows[1].on(2));
    // Split: each pad's row a pattern of its own on its track; nothing left on the kit's.
    assert_eq!(p.split_kit(1), 2);
    assert!(p.tracks[1].regions.is_empty());
    for (t, pitch) in [(2, 36), (3, 42)] {
        let Content::Pattern(pt) = &p.tracks[t].regions[0].content else {
            panic!()
        };
        assert_eq!(pt.rows.len(), 1);
        assert_eq!(pt.rows[0].pitch, pitch);
    }
}

#[test]
fn pads_are_added_swapped_and_given_new_sounds() {
    let mut p = project(vec![midi(0, PPQ, &[])], true);
    let made = p.add_kit(
        "Kit",
        vec![
            pad("Kick", 36, None, &[0, 8]),
            pad("Snare", 38, None, &[4, 12]),
        ],
        1,
        0,
        true,
    );
    let kit = made.kit;
    assert_eq!(p.kit_of(made.pads[1]), Some(kit));
    assert_eq!(p.kit_of(kit), Some(kit));
    assert_eq!(p.kit_of(0), None);
    // A pad added: in the stack, after the others, and a row in the kit's pattern.
    let clap = p.add_pad(kit, pad("Clap", 39, None, &[]));
    assert_eq!(clap, made.pads[1] + 1);
    assert_eq!(p.tracks[clap].output, Some(made.bus));
    assert_eq!(p.pad_on(kit, 39), Some(clap));
    let rows = |p: &Project| {
        let Content::Pattern(pt) = &p.tracks[kit].regions[0].content else {
            panic!()
        };
        pt.rows
            .iter()
            .map(|r| {
                (
                    r.pitch,
                    r.name.clone(),
                    r.cells.iter().filter(|&&c| c > 0).count(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&p)[2], (39, "Clap".to_string(), 0));
    // Kick and snare trade places: the beat follows, so it plays as it did.
    let (k, s) = (p.pad_on(kit, 36).unwrap(), p.pad_on(kit, 38).unwrap());
    p.swap_pads(kit, 36, 38);
    assert_eq!((p.pad_on(kit, 38), p.pad_on(kit, 36)), (Some(k), Some(s)));
    let Some(Instrument::Sampler(sampler)) = &p.tracks[k].instrument else {
        panic!()
    };
    assert_eq!(sampler.zones[0].note, 38);
    assert_eq!(rows(&p)[0], (38, "Kick".to_string(), 2));
    // Onto an empty note: it moves there.
    p.swap_pads(kit, 39, 40);
    assert_eq!(p.pad_on(kit, 40), Some(clap));
    assert_eq!(p.pad_on(kit, 39), None);
    // A new sound on the kick's pad: the pad's level and choke stay, the row's renamed.
    if let Some(Instrument::Sampler(s)) = &mut p.tracks[k].instrument {
        s.zones[0].gain_db = -3.0;
        s.voice.decay_ms = 120.0;
    }
    let zone = Zone {
        file: "Boom.wav".into(),
        note: 60,
        high: Some(72),
        ..Default::default()
    };
    p.set_pad_sound(k, 0, zone, "Boom");
    let Some(Instrument::Sampler(sampler)) = &p.tracks[k].instrument else {
        panic!()
    };
    let z = &sampler.zones[0];
    assert_eq!(
        (z.file.to_str(), z.note, z.high, z.gain_db),
        (Some("Boom.wav"), 38, None, -3.0)
    );
    assert_eq!(sampler.voice.decay_ms, 120.0);
    assert_eq!(p.tracks[k].name, "Boom");
    assert_eq!(rows(&p)[0].1, "Boom");
}

#[test]
fn a_pads_layers_stack_take_turns_or_split_by_velocity() {
    let mut p = project(vec![midi(0, PPQ, &[])], true);
    let made = p.add_kit("Kit", vec![pad("Snare", 38, None, &[])], 1, 0, false);
    let t = made.pads[0];
    let sampler = |p: &Project| match &p.tracks[t].instrument {
        Some(Instrument::Sampler(s)) => s.clone(),
        _ => panic!(),
    };
    let clap = Zone {
        file: "Clap.wav".into(),
        name: "Clap".into(),
        note: 60,
        gate: true,
        ..Default::default()
    };
    // A clap on the snare: on its note, stacked over it.
    assert_eq!(p.add_pad_layer(t, clap.clone()), Some(1));
    let s = sampler(&p);
    assert_eq!((s.zones[1].note, s.zones[1].gate), (38, false));
    assert_eq!(s.layering(), Some(Layering::Stack));
    // In turn, then by velocity: the soft first.
    if let Some(Instrument::Sampler(s)) = &mut p.tracks[t].instrument {
        s.set_layering(Layering::Cycle);
        assert_eq!(s.layering(), Some(Layering::Cycle));
        s.set_layering(Layering::Velocity);
    }
    let s = sampler(&p);
    assert_eq!(s.layering(), Some(Layering::Velocity));
    let ranges: Vec<_> = s.zones.iter().map(|z| z.velocities).collect();
    assert_eq!(ranges, [Some((1, 63)), Some((64, 127))]);
    // Blended, they fade into each other over a third of their ranges, and keep blending as
    // layers come and go.
    if let Some(Instrument::Sampler(s)) = &mut p.tracks[t].instrument {
        s.set_blend(true);
        assert!(s.blends());
        assert_eq!(s.zones[0].velocity_fade, 20);
    }
    // A third layer is split in too; a new sound on the second keeps its place.
    assert_eq!(p.add_pad_layer(t, clap.clone()), Some(2));
    p.set_pad_sound(
        t,
        1,
        Zone {
            file: "Snap.wav".into(),
            name: "Snap".into(),
            ..clap
        },
        "Snare",
    );
    let s = sampler(&p);
    assert_eq!(s.zones[1].velocities, Some((43, 84)));
    assert_eq!(s.zones[2].velocity_fade, 14);
    assert_eq!(
        (s.zones[1].name.as_str(), p.tracks[t].name.as_str()),
        ("Snap", "Snare")
    );
    // Taken out, the rest share the velocities again; the last can't go.
    assert!(p.remove_pad_layer(t, 0));
    assert!(p.remove_pad_layer(t, 0));
    assert!(!p.remove_pad_layer(t, 0));
    assert_eq!(sampler(&p).zones[0].velocities, Some((1, 127)));
    for _ in 1..PAD_LAYERS {
        p.add_pad_layer(t, Zone::default());
    }
    assert_eq!(p.add_pad_layer(t, Zone::default()), None);
}
