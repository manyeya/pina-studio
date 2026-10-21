use super::*;

#[test]
fn the_abi_is_laid_out_as_the_sdk_has_it() {
    assert_eq!(size_of::<AEffect>(), 192);
    assert_eq!(std::mem::offset_of!(AEffect, resvd1), 64);
    assert_eq!(std::mem::offset_of!(AEffect, unique_id), 112);
    assert_eq!(std::mem::offset_of!(AEffect, process_replacing), 120);
    assert_eq!(size_of::<VstTimeInfo>(), 88);
    assert_eq!(size_of::<VstMidiEvent>(), 32);
    assert_eq!(std::mem::offset_of!(VstEvents, events), 16);
    assert_eq!(size_of::<ParameterProperties>(), 152);
    assert_eq!(size_of::<ERect>(), 8);
}

#[test]
fn state_blobs_round_trip() {
    let chunk = Settings::Chunk(b"chunk"[..].into());
    assert_eq!(unpack(&pack(3, &chunk)), Some((3, chunk)));
    let params = Settings::Params(vec![0.0, 0.25, 1.0]);
    let blob = pack(-1, &params);
    assert_eq!(unpack(&blob), Some((-1, params)));
    assert_eq!(unpack(b"junk"), None);
    assert_eq!(unpack(&blob[..blob.len() - 1]), None); // cut short
    assert_eq!(unpack(&blob[..8]), None);
}

#[test]
fn fxp_presets_round_trip() {
    let params = Settings::Params(vec![0.5, 0.125]);
    let bytes = write_fxp(0x4466_5272, 7, 2, "Small Room", &params);
    assert_eq!(&bytes[..4], b"CcnK");
    assert_eq!(&bytes[8..12], b"FxCk");
    assert_eq!(bytes.len(), FXP_HEADER + 8);
    let fxp = read_fxp(&bytes).unwrap();
    assert_eq!(
        fxp,
        Fxp {
            id: 0x4466_5272,
            name: "Small Room".into(),
            settings: params
        }
    );

    // A chunk, and a name longer than the file has room for.
    let chunk = Settings::Chunk(b"opaque settings"[..].into());
    let long = "A name far longer than twenty-seven bytes";
    let bytes = write_fxp(42, 1, 12, long, &chunk);
    assert_eq!(&bytes[8..12], b"FPCh");
    let fxp = read_fxp(&bytes).unwrap();
    assert_eq!(fxp.settings, chunk);
    assert_eq!(fxp.name, long[..27]);
    let size = i32::from_be_bytes(bytes[4..8].try_into().unwrap());
    assert_eq!(size as usize, bytes.len() - 8);

    assert_eq!(read_fxp(&bytes[..bytes.len() - 1]), None); // cut short
    assert_eq!(read_fxp(b"CcnK"), None);
    let mut lying = write_fxp(42, 1, 2, "x", &Settings::Params(vec![0.5]));
    lying[24..28].copy_from_slice(&1000i32.to_be_bytes()); // more values than it holds
    assert_eq!(read_fxp(&lying), None);
}

#[test]
fn ids_read_as_their_characters_or_numbers() {
    assert_eq!(uid_text(i32::from_be_bytes(*b"DfRr")), "DfRr");
    assert_eq!(uid_text(1234), "1234");
    // Four digits are a number's characters, so they're written as the number they are.
    let digits = i32::from_be_bytes(*b"1234");
    assert_eq!(uid_text(digits), digits.to_string());
    for id in [i32::from_be_bytes(*b"DfRr"), 1234, digits, -5, 0] {
        assert_eq!(parse_uid(&uid_text(id)), Some(id));
    }
    assert_eq!(parse_uid("toolong"), None);
}

#[test]
fn notes_and_controls_pack_as_midi() {
    let on = note_midi(&Note {
        offset: 7,
        pitch: 60,
        velocity: 1.0,
    });
    assert_eq!((on.delta_frames, on.midi_data), (7, [0x90, 60, 127, 0]));
    assert_eq!((on.kind, on.byte_size), (MIDI_TYPE, 32));
    // The quietest note is still a note on.
    let soft = note_midi(&Note {
        offset: 0,
        pitch: 60,
        velocity: 0.001,
    });
    assert_eq!(soft.midi_data, [0x90, 60, 1, 0]);
    let off = note_midi(&Note {
        offset: 9,
        pitch: 60,
        velocity: 0.0,
    });
    assert_eq!(off.midi_data, [0x80, 60, 0, 0]);

    let control = |kind| control_midi(&Control { offset: 3, kind }).map(|e| e.midi_data);
    let pedal = ControlKind::Cc {
        number: 64,
        value: 1.0,
    };
    assert_eq!(control(pedal), Some([0xb0, 64, 127, 0]));
    assert_eq!(control(ControlKind::PitchBend(0.0)), Some([0xe0, 0, 64, 0]));
    assert_eq!(control(ControlKind::PitchBend(-1.0)), Some([0xe0, 0, 0, 0]));
    assert_eq!(
        control(ControlKind::PitchBend(1.0)),
        Some([0xe0, 127, 127, 0])
    );
    assert_eq!(
        control(ControlKind::Aftertouch(0.5)),
        Some([0xd0, 64, 0, 0])
    );
    let pressure = ControlKind::PolyPressure {
        pitch: 62,
        value: 0.0,
    };
    assert_eq!(control(pressure), Some([0xa0, 62, 0, 0]));
    let expression = ControlKind::Expression {
        pitch: 60,
        kind: crate::plugin::ExpressionKind::Tuning,
        value: 1.0,
    };
    assert_eq!(control(expression), None);
}

#[test]
fn the_event_list_points_at_its_events_and_never_grows() {
    let mut list = EventList::new();
    for i in 0..EVENTS + 10 {
        list.push(midi(i as u32, [0x90, 60, 100]));
    }
    assert_eq!(list.len(), EVENTS);
    let last = unsafe { *list.list.events[EVENTS - 1] };
    assert_eq!(last.delta_frames, EVENTS as i32 - 1);
    list.clear();
    assert_eq!(list.len(), 0);
}

#[test]
fn switches_and_integer_ranges_have_steps() {
    let mut p: ParameterProperties = unsafe { std::mem::zeroed() };
    assert_eq!(steps(&p), 0);
    p.flags = PARAM_IS_SWITCH;
    assert_eq!(steps(&p), 1);
    p.flags = PARAM_USES_INTEGER_MIN_MAX;
    (p.min_integer, p.max_integer) = (0, 3);
    assert_eq!(steps(&p), 3);
    p.flags |= PARAM_USES_INT_STEP;
    (p.max_integer, p.step_integer) = (10, 2);
    assert_eq!(steps(&p), 5);
}

/// With a real plugin, which CI doesn't have; an effect hears an impulse, an instrument
/// plays a note:
/// `PINA_TEST_VST2=/path/DragonflyRoomReverb.vst cargo test -p pina-engine -- --ignored`
#[test]
#[ignore]
fn a_real_plugin_plays_saves_restores_and_shuts_down() {
    let path = std::env::var("PINA_TEST_VST2").expect("set PINA_TEST_VST2");
    let found = inspect(Path::new(&path)).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    let info = &found[0];
    println!("{info:?}");
    assert_eq!(info.format, Format::Vst2);
    let mut libraries = HashMap::new();
    let r = info.plugin_ref();
    let (plugin, mut processor) = instantiate(&mut libraries, &r, 48_000.0, 512, None).unwrap();
    assert_eq!(plugin.name(), info.class);

    // Sound, well after a burst of sine (a reverb's tail) or a note's start.
    let (mut l, mut rt) = (vec![0f32; 512], vec![0f32; 512]);
    let note = Note {
        offset: 0,
        pitch: 60,
        velocity: 1.0,
    };
    match info.instrument {
        true => processor.process([&mut l, &mut rt], 512, &[note], &[], None),
        false => {
            for (n, (a, b)) in l.iter_mut().zip(&mut rt).enumerate() {
                *a = (n as f32 * 0.06).sin() * 0.5;
                *b = *a;
            }
            processor.process([&mut l, &mut rt], 512, &[], &[], None);
        }
    }
    let mut peaks = Vec::new();
    for _ in 0..20 {
        l.fill(0.0);
        rt.fill(0.0);
        processor.process([&mut l, &mut rt], 512, &[], &[], None);
        assert!(l.iter().chain(&rt).all(|x| x.is_finite()));
        peaks.push(l.iter().chain(&rt).fold(0f32, |m, x| m.max(x.abs())));
    }
    println!("peaks: {peaks:?}");
    let later = peaks[4..12].iter().fold(0f32, |m, &x| m.max(x));
    assert!(later > 1e-3, "{later}");

    // Its parameters can be read, turned, shown and typed.
    let params = plugin.params();
    for p in &params {
        println!(
            "{} {:?} = {:.3} ({}) default {:.3} steps {} units {:?}",
            p.id,
            p.name,
            p.value,
            plugin.param_text(p.id, p.value),
            p.default,
            p.steps,
            p.units
        );
    }
    // A continuous one, in units other than percent if it has one.
    let continuous = || params.iter().filter(|p| p.steps == 0);
    let p = continuous()
        .find(|p| !p.units.is_empty() && p.units != "%")
        .or_else(|| continuous().next())
        .expect("a continuous parameter");
    let to = if p.value > 0.5 { 0.25 } else { 0.75 };
    plugin.set_param(p.id, to);
    assert!((plugin.param_value(p.id) - to).abs() < 1e-6);
    assert!(processor.pending());
    crate::plugin::settle(&mut processor, 512);
    assert!(!processor.pending());
    // Now the plugin has it, and shows it.
    let now = plugin.param_value(p.id);
    assert!((now - to).abs() < 0.02, "{now}");
    let shown = plugin.param_text(p.id, to);
    let other = plugin.param_text(p.id, 0.1);
    println!("{} at {to}: {shown:?}; at 0.1: {other:?}", p.name);
    assert!(!shown.is_empty());
    // Another value is shown as the plugin would show it, not as a percentage: unless the
    // plugin shows percentages, the two differ.
    if p.units != "%" {
        assert_ne!(other, "10%");
    }
    match plugin.param_from_text(p.id, &shown) {
        Some(v) => {
            println!("{shown:?} reads as {v}");
            assert!((v - to).abs() < 0.02, "{v}");
        }
        None => println!("it can't read typed values"),
    }
    // Typed values are read, by the plugin or by its display, without touching what plays.
    let typed = crate::plugin::value_from_text(&*plugin, p, &other).unwrap();
    println!("{other:?} reads as {typed}");
    assert!((typed - 0.1).abs() < 0.01, "{typed}");
    assert!((plugin.param_value(p.id) - to).abs() < 0.02);

    // Its settings come back in a new instance.
    let state = plugin.state().unwrap();
    let mut presets = plugin.presets();
    println!("presets: {presets:?}");
    drop(plugin);
    drop(processor);
    let (again, _processor) = instantiate(&mut libraries, &r, 48_000.0, 512, Some(&state)).unwrap();
    assert!((again.param_value(p.id) - to).abs() < 0.02);
    assert_eq!(again.state().unwrap(), state);

    // Programs load.
    presets.retain(|p| matches!(p.source, PresetSource::Program { .. }));
    if let Some(last) = presets.last() {
        again.load_preset(last).unwrap();
    }

    // A preset saved, the parameter moved, and the preset loaded puts it back.
    again.set_state(&state).unwrap();
    let file = again.save_preset("pina test preset").unwrap();
    again.set_param(p.id, 1.0 - to);
    let preset = Preset {
        name: "pina test preset".into(),
        source: PresetSource::File { file: file.clone() },
    };
    again.load_preset(&preset).unwrap();
    let back = again.param_value(p.id);
    let listed = again.presets().contains(&preset);
    std::fs::remove_file(&file).unwrap();
    // Its folders too, if the test made them.
    let _ = std::fs::remove_dir(file.parent().unwrap());
    let _ = std::fs::remove_dir(file.parent().unwrap().parent().unwrap());
    assert!((back - to).abs() < 0.02, "{back}");
    assert!(listed);
}
