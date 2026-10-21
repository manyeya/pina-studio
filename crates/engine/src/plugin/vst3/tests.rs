use super::*;

/// With a real instrument, which CI doesn't have:
/// `PINA_TEST_VST3="/path/Some Synth.vst3" cargo test -p pina-engine -- --ignored`
#[test]
#[ignore]
fn a_real_instrument_plays_saves_restores_and_shuts_down() {
    let path = std::env::var("PINA_TEST_VST3").expect("set PINA_TEST_VST3");
    let module = Module::load(Path::new(&path)).unwrap();
    let classes = module.classes();
    let class = classes.iter().find(|c| c.category == AUDIO_MODULE).unwrap();
    assert!(class.instrument, "{} isn't an instrument", class.name);
    let (plugin, mut processor) =
        Vst3Plugin::instantiate(&module, class, 48_000.0, 512, None).unwrap();
    let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
    let note = Note {
        offset: 0,
        pitch: 60,
        velocity: 1.0,
    };
    processor.process([&mut l, &mut r], 512, &[note], &[], None);
    let mut loudest = 0f32;
    for _ in 0..20 {
        processor.process([&mut l, &mut r], 512, &[], &[], None);
        loudest = l.iter().fold(loudest, |m, x| m.max(x.abs()));
    }
    assert!(loudest > 1e-3, "{loudest}");

    // Its parameters can be read and turned, as Smart Controls do.
    let params = plugin.params();
    assert!(!params.is_empty());
    let p = &params[0];
    let to = if p.value > 0.5 { 0.25 } else { 0.75 };
    plugin.set_param(p.id, to);
    assert!(
        (plugin.param_value(p.id) - to).abs() < 0.02,
        "{}",
        plugin.param_value(p.id)
    );
    assert!(!plugin.param_text(p.id, to).is_empty());
    plugin.set_param(p.id, p.value);

    let state = plugin.state().unwrap();
    drop(plugin);
    drop(processor);
    let (again, _processor) =
        Vst3Plugin::instantiate(&module, class, 48_000.0, 512, Some(&state)).unwrap();
    assert_eq!(again.state().unwrap(), state);
}

#[test]
fn state_blobs_round_trip() {
    let blob = pack(b"component", b"controller");
    assert_eq!(unpack(&blob), Some((&b"component"[..], &b"controller"[..])));
    assert_eq!(unpack(b"junk"), None);
    assert_eq!(unpack(&blob[..10]), None); // cut short
}
