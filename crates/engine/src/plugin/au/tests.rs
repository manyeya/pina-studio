use super::*;
use crate::plugin::value_from_text;

const RATE: f64 = 48_000.0;
const BLOCK: usize = 512;

fn load(uid: &str) -> (AuPlugin, AuProcessor) {
    let c = by_uid(uid).unwrap_or_else(|| panic!("{uid} isn't installed"));
    AuPlugin::instantiate(&c, RATE, BLOCK, None).unwrap()
}

fn param(p: &AuPlugin, name: &str) -> PluginParam {
    let params = p.params();
    let found = params.iter().find(|p| p.name == name).cloned();
    found.unwrap_or_else(|| panic!("no {name:?} in {params:?}"))
}

/// Runs `blocks` blocks of `input` through it, `notes` and `controls` in the first, and
/// returns its left side.
fn run(
    p: &mut AuProcessor,
    blocks: usize,
    input: impl Fn(usize) -> f32,
    notes: &[Note],
    controls: &[Control],
) -> Vec<f32> {
    let mut out = Vec::new();
    for b in 0..blocks {
        let mut l: Vec<f32> = (0..BLOCK).map(|i| input(b * BLOCK + i)).collect();
        let mut r = l.clone();
        let (n, c) = if b == 0 {
            (notes, controls)
        } else {
            (&[][..], &[][..])
        };
        p.process([&mut l, &mut r], BLOCK, n, c, None);
        out.extend(l);
    }
    out
}

fn silence(_: usize) -> f32 {
    0.0
}

fn sine(hz: f64) -> impl Fn(usize) -> f32 {
    move |i| (std::f64::consts::TAU * hz * i as f64 / RATE).sin() as f32 * 0.5
}

fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

/// Its pitch, by how often it rises through zero.
fn pitch(s: &[f32]) -> f64 {
    let rises = s.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    rises as f64 * RATE / s.len() as f64
}

fn note(pitch: u8, velocity: f32) -> Note {
    Note {
        offset: 0,
        pitch,
        velocity,
    }
}

#[test]
fn apples_units_are_listed_with_their_bundles() {
    let units = list();
    let sampler = units
        .iter()
        .find(|u| u.uid.as_deref() == Some("aumu samp appl"))
        .unwrap();
    assert_eq!(sampler.format, Format::Au);
    assert_eq!(
        (&sampler.class[..], &sampler.vendor[..]),
        ("AUSampler", "Apple")
    );
    assert!(sampler.instrument);
    assert!(sampler.path.ends_with("CoreAudio.component"), "{sampler:?}");
    let delay = units.iter().find(|u| u.class == "AUDelay").unwrap();
    assert_eq!(delay.uid.as_deref(), Some("aufx dely appl"));
    assert!(!delay.instrument);
}

#[test]
fn uids_are_read_by_position() {
    let dls = parse_uid("aumu dls  appl").unwrap();
    assert_eq!(
        dls,
        [
            kAudioUnitType_MusicDevice,
            u32::from_be_bytes(*b"dls "),
            u32::from_be_bytes(*b"appl")
        ]
    );
    assert_eq!(
        by_uid("aumu dls  appl").map(|c| uid(&c.desc)).as_deref(),
        Some("aumu dls  appl")
    );
    assert_eq!(parse_uid("aumu dls appl"), None);
    assert_eq!(parse_uid("aumu samp appl x"), None);
    // Loading falls back on the name.
    let r = PluginRef {
        format: Format::Au,
        class: "AULowpass".into(),
        ..Default::default()
    };
    assert_eq!(
        instantiate(&r, RATE, BLOCK, None).unwrap().0.name(),
        "AULowpass"
    );
}

#[test]
fn an_instrument_plays_notes_and_hears_pitch_bend() {
    let (_, mut sampler) = load("aumu samp appl");
    // Its default sound is a sine: A is 440 Hz.
    let played = run(&mut sampler, 40, silence, &[note(69, 1.0)], &[]);
    let loudest = played.iter().fold(0f32, |m, x| m.max(x.abs()));
    assert!(loudest > 0.1, "{loudest}");
    let hz = pitch(&played[10 * BLOCK..]);
    assert!((hz - 440.0).abs() < 5.0, "{hz}");

    // Bent all the way up: two semitones.
    let (_, mut bent) = load("aumu samp appl");
    let up = Control {
        offset: 0,
        kind: ControlKind::PitchBend(1.0),
    };
    let played = run(&mut bent, 40, silence, &[note(69, 1.0)], &[up]);
    let hz = pitch(&played[10 * BLOCK..]);
    assert!((hz - 493.9).abs() < 5.0, "{hz}");

    // A note off lets it fade.
    let tail = run(&mut bent, 60, silence, &[note(69, 0.0)], &[]);
    assert!(
        rms(&tail[50 * BLOCK..]) < 1e-3,
        "{}",
        rms(&tail[50 * BLOCK..])
    );
}

#[test]
fn a_general_midi_synth_hears_controllers() {
    let (_, mut dls) = load("aumu dls  appl");
    let loud = rms(&run(&mut dls, 20, silence, &[note(60, 0.8)], &[]));
    let (_, mut dls) = load("aumu dls  appl");
    let volume = Control {
        offset: 0,
        kind: ControlKind::Cc {
            number: 7,
            value: 0.0,
        },
    };
    let quiet = rms(&run(&mut dls, 20, silence, &[note(60, 0.8)], &[volume]));
    assert!(loud > 1e-3 && quiet < loud / 100.0, "{loud} {quiet}");
}

#[test]
fn parameters_are_named_turned_and_read_as_text() {
    let (lowpass, _p) = load("aufx lpas appl");
    let cutoff = param(&lowpass, "Cutoff Frequency");
    assert_eq!(cutoff.units, "Hz");
    assert_eq!(cutoff.steps, 0);
    lowpass.set_param(cutoff.id, 0.25);
    assert!((lowpass.param_value(cutoff.id) - 0.25).abs() < 1e-4);
    let v = value_from_text(&lowpass, &cutoff, "1000 Hz").unwrap();
    assert_eq!(lowpass.param_text(cutoff.id, v), "1000 Hz");

    // An indexed one, with names for its values.
    let (eq, _p) = load("aufx nbeq appl");
    let kind = eq.params().into_iter().find(|p| p.name == "Type").unwrap();
    assert_eq!(kind.steps, 10);
    assert_eq!(eq.param_text(kind.id, 0.0), "Parametric");
    assert_eq!(eq.param_from_text(kind.id, "resonant low pass"), Some(0.3));
    let switch = eq
        .params()
        .into_iter()
        .find(|p| p.name == "Bypass")
        .unwrap();
    assert_eq!(switch.steps, 1);
    assert_eq!(
        (eq.param_text(switch.id, 0.0), eq.param_text(switch.id, 1.0)),
        ("Off".into(), "On".into())
    );
}

#[test]
fn settings_round_trip() {
    let (delay, _p) = load("aufx dely appl");
    let time = param(&delay, "Delay Time");
    delay.set_param(time.id, 0.2);
    let state = delay.state().unwrap();
    let c = by_uid("aufx dely appl").unwrap();
    let (again, _p) = AuPlugin::instantiate(&c, RATE, BLOCK, Some(&state)).unwrap();
    assert!((again.param_value(time.id) - 0.2).abs() < 1e-4);
    let (third, _p) = load("aufx dely appl");
    third.set_state(&state).unwrap();
    assert!((third.param_value(time.id) - 0.2).abs() < 1e-4);
    assert!(third.set_state(b"junk").is_err());
    // Another unit's settings don't load.
    let (lowpass, _p) = load("aufx lpas appl");
    assert!(lowpass.set_state(&state).is_err());
}

#[test]
fn factory_presets_load() {
    let (reverb, _p) = load("aufx rvb2 appl");
    let presets = reverb.presets();
    let hall = presets.iter().find(|p| p.name == "Large Hall").unwrap();
    assert!(matches!(hall.source, PresetSource::Program { list: 0, .. }));
    let before: Vec<f64> = reverb.params().iter().map(|p| p.value).collect();
    reverb.load_preset(hall).unwrap();
    let after: Vec<f64> = reverb.params().iter().map(|p| p.value).collect();
    assert_ne!(before, after);
}

#[test]
fn presets_save_to_files_and_load_back() {
    let dir = std::env::temp_dir().join(format!("pina-test-aupreset-{}", std::process::id()));
    let (lowpass, _p) = load("aufx lpas appl");
    let cutoff = param(&lowpass, "Cutoff Frequency");
    lowpass.set_param(cutoff.id, 0.3);
    let file = lowpass.write_preset(&dir, "Dark: Very").unwrap();
    assert_eq!(file, dir.join("Dark- Very.aupreset"));
    let xml = std::fs::read_to_string(&file).unwrap();
    assert!(xml.contains("<string>Dark: Very</string>"), "{xml}");
    lowpass.set_param(cutoff.id, 0.9);
    let preset = Preset {
        name: "Dark".into(),
        source: PresetSource::File { file },
    };
    lowpass.load_preset(&preset).unwrap();
    assert!((lowpass.param_value(cutoff.id) - 0.3).abs() < 1e-4);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_effect_processes_its_input() {
    let (lowpass, mut p) = load("aufx lpas appl");
    let cutoff = param(&lowpass, "Cutoff Frequency");
    let v = value_from_text(&lowpass, &cutoff, "300 Hz").unwrap();
    lowpass.set_param(cutoff.id, v);
    let low = rms(&run(&mut p, 20, sine(100.0), &[], &[])[10 * BLOCK..]);
    let high = rms(&run(&mut p, 20, sine(8000.0), &[], &[])[10 * BLOCK..]);
    assert!(low > 0.3 && high < 0.01, "{low} {high}");
}

#[test]
fn automation_moves_parameters() {
    let (lowpass, mut p) = load("aufx lpas appl");
    let cutoff = param(&lowpass, "Cutoff Frequency");
    p.automate(cutoff.id, 0.1, 0.2);
    run(&mut p, 1, silence, &[], &[]);
    assert!((lowpass.param_value(cutoff.id) - 0.1).abs() < 1e-4);
}

#[test]
fn the_transport_reaches_the_host_callbacks() {
    let (_, mut p) = load("aumu samp appl");
    p.set_transport(true, 48_000, 90.0);
    let user = (&*p.unit.transport as *const Transport).cast_mut().cast();
    let (mut beat, mut tempo) = (0.0, 0.0);
    unsafe { beat_and_tempo(user, &mut beat, &mut tempo) };
    assert_eq!((beat, tempo), (1.5, 90.0));
    let (mut next, mut num, mut den, mut downbeat) = (0, 0.0, 0, 0.0);
    unsafe { musical_time(user, &mut next, &mut num, &mut den, &mut downbeat) };
    assert_eq!((next, num, den, downbeat), (16_000, 4.0, 4, 0.0));
    p.set_transport(false, 0, 90.0);
    let (mut playing, mut changed) = (1u8, 0u8);
    let none = ptr::null_mut();
    unsafe {
        transport_state2(
            user,
            &mut playing,
            none,
            &mut changed,
            ptr::null_mut(),
            none,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!((playing, changed), (0, 1));
    unsafe {
        transport_state2(
            user,
            &mut playing,
            none,
            &mut changed,
            ptr::null_mut(),
            none,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!(changed, 0);
}

/// With an instrument from elsewhere (its own editor, presets and settings):
/// `PINA_TEST_AU="aumu SgXT VmbA" cargo test -p pina-engine -- --ignored`
#[test]
#[ignore]
fn a_real_instrument_plays_saves_restores_and_shuts_down() {
    let uid = std::env::var("PINA_TEST_AU").expect("set PINA_TEST_AU");
    let (plugin, mut p) = load(&uid);
    let played = run(&mut p, 40, silence, &[note(60, 1.0)], &[]);
    let loudest = played.iter().fold(0f32, |m, x| m.max(x.abs()));
    assert!(loudest > 1e-3, "{loudest}");

    let presets = plugin.presets();
    if let Some(preset) = presets.get(1) {
        plugin.load_preset(preset).unwrap();
    }

    // Its parameters turn, though some snap to steps they don't declare.
    let params = plugin.params();
    let turned = |p: &&PluginParam| {
        let to = if p.value > 0.5 { 0.25 } else { 0.75 };
        plugin.set_param(p.id, to);
        (plugin.param_value(p.id) - to).abs() < 0.02
    };
    let param = params.iter().find(turned).expect("no parameter turns");
    let to = plugin.param_value(param.id);
    assert!(!plugin.param_text(param.id, to).is_empty());
    println!("{} parameters, {} presets", params.len(), presets.len());

    let state = plugin.state().unwrap();
    drop(plugin);
    drop(p);
    let c = by_uid(&uid).unwrap();
    let (again, _p) = AuPlugin::instantiate(&c, RATE, BLOCK, Some(&state)).unwrap();
    assert!((again.param_value(param.id) - to).abs() < 0.02);
}

#[test]
fn samplers_start_and_stop_on_several_threads_at_once() {
    let threads: Vec<_> = (0..3)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..30 {
                    drop(load("aumu samp appl"));
                }
            })
        })
        .collect();
    threads.into_iter().for_each(|t| t.join().unwrap());
}

#[test]
fn ranges_map_both_ways() {
    let range = |min, max, steps, curve| Range {
        min,
        max,
        steps,
        curve,
        ramps: false,
    };
    let log = range(20.0, 20_000.0, 0, Curve::Log);
    assert!((log.value(0.5) - 632.46).abs() < 0.01);
    assert!((log.position(632.456) - 0.5).abs() < 1e-5);
    let steps = range(1.0, 5.0, 4, Curve::Linear);
    assert_eq!((steps.value(0.6), steps.position(3.0)), (3.0, 0.5));
    let squared = range(0.0, 100.0, 0, Curve::Power(2.0));
    assert_eq!((squared.value(0.5), squared.position(25.0)), (25.0, 0.5));
    assert_eq!(range(1.0, 1.0, 0, Curve::Linear).position(1.0), 0.0);
}

#[test]
fn controls_become_midi() {
    let bend = |v| control_message(ControlKind::PitchBend(v));
    assert_eq!(bend(0.0), Some([0xE0, 0, 64]));
    assert_eq!(bend(1.0), Some([0xE0, 127, 127]));
    assert_eq!(bend(-1.0), Some([0xE0, 0, 0]));
    let cc = ControlKind::Cc {
        number: 64,
        value: 1.0,
    };
    assert_eq!(control_message(cc), Some([0xB0, 64, 127]));
    assert_eq!(note_message(&note(60, 0.001)), [0x90, 60, 1]);
    assert_eq!(note_message(&note(60, 0.0)), [0x80, 60, 0]);
}
