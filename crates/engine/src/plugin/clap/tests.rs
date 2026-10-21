use super::*;
use crate::plugin::value_from_text;
use clap_sys::process::{CLAP_PROCESS_CONTINUE, clap_process_status};

/// What a fake plugin heard, each at its time.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Heard {
    On(u32, i16, f64),
    Off(u32, i16),
    Param(u32, u32, f64),
    Midi(u32, [u8; 3]),
    Expression(u32, i32, i16, f64),
}

/// A plugin of Pina Studio's own for the tests: an instrument playing its Level (parameter 1, 0
/// to 2), or an effect playing its input at its Level plus its sidechain. It keeps what it
/// hears, and checks it's started and shut down in order.
#[repr(C)]
struct Fake {
    clap: clap_plugin,
    effect: bool,
    level: Cell<f64>,
    /// Parameter 2, 0 to 3 in steps.
    mode: Cell<f64>,
    heard: RefCell<Vec<Heard>>,
    /// The last transport: its flags, position in beats and tempo.
    transport: Cell<Option<(u32, i64, f64)>>,
    active: Cell<bool>,
    processing: Cell<bool>,
}

struct Shared<T>(T);
unsafe impl<T> Sync for Shared<T> {}

static INSTRUMENT: Shared<[*const c_char; 2]> = Shared([c"instrument".as_ptr(), ptr::null()]);
static EFFECT: Shared<[*const c_char; 2]> = Shared([c"audio-effect".as_ptr(), ptr::null()]);
static DESCRIPTORS: [clap_plugin_descriptor; 2] = [
    descriptor(c"pina.test-synth", c"Test Synth", &INSTRUMENT.0),
    descriptor(c"pina.test-fx", c"Test Effect", &EFFECT.0),
];
static FACTORY: clap_plugin_factory = clap_plugin_factory {
    get_plugin_count: Some(fake_count),
    get_plugin_descriptor: Some(fake_descriptor),
    create_plugin: Some(fake_create),
};

const fn descriptor(
    id: &'static CStr,
    name: &'static CStr,
    features: &'static [*const c_char; 2],
) -> clap_plugin_descriptor {
    clap_plugin_descriptor {
        clap_version: CLAP_VERSION,
        id: id.as_ptr(),
        name: name.as_ptr(),
        vendor: c"Pina Studio".as_ptr(),
        url: ptr::null(),
        manual_url: ptr::null(),
        support_url: ptr::null(),
        version: ptr::null(),
        description: ptr::null(),
        features: features.as_ptr(),
    }
}

unsafe extern "C" fn fake_count(_: *const clap_plugin_factory) -> u32 {
    2
}

unsafe extern "C" fn fake_descriptor(
    _: *const clap_plugin_factory,
    i: u32,
) -> *const clap_plugin_descriptor {
    &DESCRIPTORS[i as usize]
}

unsafe extern "C" fn fake_create(
    _: *const clap_plugin_factory,
    _: *const clap_host,
    id: *const c_char,
) -> *const clap_plugin {
    let effect = unsafe { CStr::from_ptr(id) } == c"pina.test-fx";
    let fake = Box::new(Fake {
        clap: clap_plugin {
            desc: &DESCRIPTORS[effect as usize],
            plugin_data: ptr::null_mut(),
            init: Some(fake_init),
            destroy: Some(fake_destroy),
            activate: Some(fake_activate),
            deactivate: Some(fake_deactivate),
            start_processing: Some(fake_start),
            stop_processing: Some(fake_stop),
            reset: Some(fake_nothing),
            process: Some(fake_process),
            get_extension: Some(fake_extension),
            on_main_thread: Some(fake_nothing),
        },
        effect,
        level: Cell::new(1.0),
        mode: Cell::new(0.0),
        heard: RefCell::new(Vec::new()),
        transport: Cell::new(None),
        active: Cell::new(false),
        processing: Cell::new(false),
    });
    Box::into_raw(fake).cast()
}

unsafe fn fake<'a>(p: *const clap_plugin) -> &'a Fake {
    unsafe { &*p.cast::<Fake>() }
}

unsafe extern "C" fn fake_init(_: *const clap_plugin) -> bool {
    true
}

unsafe extern "C" fn fake_destroy(p: *const clap_plugin) {
    let f = unsafe { Box::from_raw(p.cast_mut().cast::<Fake>()) };
    assert!(
        !f.processing.get() && !f.active.get(),
        "destroyed while running"
    );
}

unsafe extern "C" fn fake_activate(p: *const clap_plugin, _: f64, _: u32, _: u32) -> bool {
    unsafe { fake(p) }.active.set(true);
    true
}

unsafe extern "C" fn fake_deactivate(p: *const clap_plugin) {
    let f = unsafe { fake(p) };
    assert!(!f.processing.get(), "deactivated while processing");
    f.active.set(false);
}

unsafe extern "C" fn fake_start(p: *const clap_plugin) -> bool {
    assert!(AUDIO.get(), "started off the audio thread");
    unsafe { fake(p) }.processing.set(true);
    true
}

unsafe extern "C" fn fake_stop(p: *const clap_plugin) {
    assert!(AUDIO.get(), "stopped off the audio thread");
    unsafe { fake(p) }.processing.set(false);
}

unsafe extern "C" fn fake_nothing(_: *const clap_plugin) {}

impl Fake {
    fn hear(&self, events: *const clap_input_events) {
        let list = unsafe { &*events };
        for i in 0..unsafe { list.size.unwrap()(events) } {
            let e = unsafe { list.get.unwrap()(events, i) };
            let (time, kind) = unsafe { ((*e).time, (*e).type_) };
            let heard = match kind {
                CLAP_EVENT_NOTE_ON | CLAP_EVENT_NOTE_OFF => {
                    let n = unsafe { &*e.cast::<clap_event_note>() };
                    assert_eq!(n.note_id, n.key as i32);
                    match kind == CLAP_EVENT_NOTE_ON {
                        true => Heard::On(time, n.key, n.velocity),
                        false => Heard::Off(time, n.key),
                    }
                }
                CLAP_EVENT_PARAM_VALUE => {
                    let p = unsafe { &*e.cast::<clap_event_param_value>() };
                    match p.param_id {
                        1 => self.level.set(p.value),
                        _ => self.mode.set(p.value),
                    }
                    Heard::Param(time, p.param_id, p.value)
                }
                CLAP_EVENT_MIDI => {
                    Heard::Midi(time, unsafe { (*e.cast::<clap_event_midi>()).data })
                }
                CLAP_EVENT_NOTE_EXPRESSION => {
                    let x = unsafe { &*e.cast::<clap_event_note_expression>() };
                    Heard::Expression(time, x.expression_id, x.key, x.value)
                }
                other => panic!("unexpected event {other}"),
            };
            self.heard.borrow_mut().push(heard);
        }
    }
}

unsafe extern "C" fn fake_process(
    p: *const clap_plugin,
    data: *const clap_process,
) -> clap_process_status {
    let (f, d) = unsafe { (fake(p), &*data) };
    assert!(f.processing.get() && AUDIO.get());
    f.hear(d.in_events);
    let t = unsafe { &*d.transport };
    f.transport.set(Some((t.flags, t.song_pos_beats, t.tempo)));
    let n = d.frames_count as usize;
    let out = unsafe { &*d.audio_outputs };
    for c in 0..2 {
        let dst = unsafe { slice::from_raw_parts_mut(*out.data32.add(c), n) };
        for (i, s) in dst.iter_mut().enumerate() {
            let input = |port: usize| unsafe {
                let b = &*d.audio_inputs.add(port);
                *(*b.data32.add(c)).add(i)
            };
            *s = match f.effect {
                true => input(0) * f.level.get() as f32 + input(1),
                false => f.level.get() as f32,
            };
        }
    }
    CLAP_PROCESS_CONTINUE
}

static FAKE_PARAMS: clap_plugin_params = clap_plugin_params {
    count: Some(param_count),
    get_info: Some(param_info),
    get_value: Some(param_get),
    value_to_text: Some(param_to_text),
    text_to_value: Some(param_from_text),
    flush: Some(param_flush),
};
static FAKE_STATE: clap_plugin_state = clap_plugin_state {
    save: Some(state_save),
    load: Some(state_load),
};
static FAKE_PORTS: clap_plugin_audio_ports = clap_plugin_audio_ports {
    count: Some(port_count),
    get: Some(port_get),
};
static FAKE_NOTES: clap_plugin_note_ports = clap_plugin_note_ports {
    count: Some(note_count),
    get: Some(note_get),
};

unsafe extern "C" fn fake_extension(_: *const clap_plugin, id: *const c_char) -> *const c_void {
    match unsafe { CStr::from_ptr(id) } {
        id if id == CLAP_EXT_PARAMS => ptr::from_ref(&FAKE_PARAMS).cast(),
        id if id == CLAP_EXT_STATE => ptr::from_ref(&FAKE_STATE).cast(),
        id if id == CLAP_EXT_AUDIO_PORTS => ptr::from_ref(&FAKE_PORTS).cast(),
        id if id == CLAP_EXT_NOTE_PORTS => ptr::from_ref(&FAKE_NOTES).cast(),
        _ => ptr::null(),
    }
}

unsafe extern "C" fn param_count(_: *const clap_plugin) -> u32 {
    3
}

unsafe extern "C" fn param_info(_: *const clap_plugin, i: u32, info: *mut clap_param_info) -> bool {
    let (id, name, flags, max, default) = match i {
        0 => (1, "Level", 0, 2.0, 1.0),
        1 => (2, "Mode", CLAP_PARAM_IS_STEPPED, 3.0, 0.0),
        _ => (3, "Hidden", CLAP_PARAM_IS_HIDDEN, 1.0, 0.0),
    };
    let info = unsafe { &mut *info };
    info.id = id;
    info.flags = flags | CLAP_PARAM_IS_AUTOMATABLE;
    for (dst, &src) in info.name.iter_mut().zip(name.as_bytes()) {
        *dst = src as c_char;
    }
    (info.min_value, info.max_value, info.default_value) = (0.0, max, default);
    true
}

unsafe extern "C" fn param_get(p: *const clap_plugin, id: clap_id, v: *mut f64) -> bool {
    let f = unsafe { fake(p) };
    unsafe { *v = if id == 1 { f.level.get() } else { f.mode.get() } };
    true
}

const MODES: [&str; 4] = ["Sine", "Saw", "Square", "Noise"];

unsafe extern "C" fn param_to_text(
    _: *const clap_plugin,
    id: clap_id,
    v: f64,
    out: *mut c_char,
    size: u32,
) -> bool {
    let text = match id {
        1 => format!("{v:.2}"),
        _ => MODES[v as usize].to_string(),
    };
    let out = unsafe { slice::from_raw_parts_mut(out, size as usize) };
    for (dst, src) in out.iter_mut().zip(text.bytes().chain([0])) {
        *dst = src as c_char;
    }
    true
}

unsafe extern "C" fn param_from_text(
    _: *const clap_plugin,
    id: clap_id,
    text: *const c_char,
    v: *mut f64,
) -> bool {
    let text = unsafe { CStr::from_ptr(text) }.to_str().unwrap();
    let value = match id {
        1 => text.parse().ok(),
        _ => MODES.iter().position(|&m| m == text).map(|i| i as f64),
    };
    value.map(|x| unsafe { *v = x }).is_some()
}

unsafe extern "C" fn param_flush(
    p: *const clap_plugin,
    events: *const clap_input_events,
    _: *const clap_output_events,
) {
    assert!(AUDIO.get(), "flushed off the audio thread while active");
    unsafe { fake(p) }.hear(events);
}

unsafe extern "C" fn state_save(p: *const clap_plugin, s: *const clap_ostream) -> bool {
    let f = unsafe { fake(p) };
    let mut bytes = f.level.get().to_le_bytes().to_vec();
    bytes.extend(f.mode.get().to_le_bytes());
    let write = unsafe { (*s).write.unwrap() };
    unsafe { write(s, bytes.as_ptr().cast(), bytes.len() as u64) == 16 }
}

unsafe extern "C" fn state_load(p: *const clap_plugin, s: *const clap_istream) -> bool {
    let mut bytes = [0u8; 17];
    let read = unsafe { (*s).read.unwrap() };
    if unsafe { read(s, bytes.as_mut_ptr().cast(), 17) } != 16 {
        return false;
    }
    let f = unsafe { fake(p) };
    f.level
        .set(f64::from_le_bytes(bytes[..8].try_into().unwrap()));
    f.mode
        .set(f64::from_le_bytes(bytes[8..16].try_into().unwrap()));
    true
}

unsafe extern "C" fn port_count(p: *const clap_plugin, input: bool) -> u32 {
    match (input, unsafe { fake(p) }.effect) {
        (false, _) => 1,
        (true, true) => 2,
        (true, false) => 0,
    }
}

unsafe extern "C" fn port_get(
    _: *const clap_plugin,
    i: u32,
    _: bool,
    info: *mut clap_audio_port_info,
) -> bool {
    let info = unsafe { &mut *info };
    info.id = i;
    info.channel_count = 2;
    info.flags = if i == 0 { CLAP_AUDIO_PORT_IS_MAIN } else { 0 };
    true
}

unsafe extern "C" fn note_count(p: *const clap_plugin, input: bool) -> u32 {
    (input && !unsafe { fake(p) }.effect) as u32
}

unsafe extern "C" fn note_get(
    _: *const clap_plugin,
    _: u32,
    _: bool,
    info: *mut clap_note_port_info,
) -> bool {
    unsafe { (*info).supported_dialects = CLAP_NOTE_DIALECT_CLAP | CLAP_NOTE_DIALECT_MIDI };
    true
}

fn load(id: &CStr, state: Option<&[u8]>) -> (ClapPlugin, ClapProcessor) {
    let library = Library {
        entry: ptr::null(),
        factory: &FACTORY,
    };
    let d = library
        .descriptors()
        .into_iter()
        .find(|d| d.id.as_c_str() == id)
        .unwrap();
    ClapPlugin::instantiate(library, &d, 48_000.0, 512, state).unwrap()
}

fn heard(p: &ClapProcessor) -> Vec<Heard> {
    unsafe { fake(p.plugin) }.heard.take()
}

#[test]
fn notes_controls_and_automation_arrive_in_time_order() {
    use ControlKind::*;
    let (_plugin, mut processor) = load(c"pina.test-synth", None);
    // A second in, at 120 BPM: two beats.
    processor.set_transport(true, 48_000, 120.0);
    processor.automate(2, 0.0, 1.0);
    let note = |offset, velocity| Note {
        offset,
        pitch: 60,
        velocity,
    };
    let at = |offset, kind| Control { offset, kind };
    let controls = [
        at(
            10,
            Cc {
                number: 1,
                value: 1.0,
            },
        ),
        at(20, PitchBend(1.0)),
        at(30, Aftertouch(0.5)),
        at(
            40,
            PolyPressure {
                pitch: 60,
                value: 0.25,
            },
        ),
        at(
            50,
            Expression {
                pitch: 60,
                kind: ExpressionKind::Tuning,
                value: 2.0,
            },
        ),
    ];
    let (mut l, mut r) = (vec![0f32; 128], vec![0f32; 128]);
    let notes = [note(0, 1.0), note(100, 0.0)];
    processor.process([&mut l, &mut r], 128, &notes, &controls, None);
    use Heard as H;
    assert_eq!(
        heard(&processor),
        [
            H::Param(0, 2, 0.0),
            H::On(0, 60, 1.0),
            H::Midi(10, [0xb0, 1, 127]),
            H::Midi(20, [0xe0, 0x7f, 0x7f]),
            H::Midi(30, [0xd0, 64, 0]),
            H::Expression(40, CLAP_NOTE_EXPRESSION_PRESSURE, 60, 0.25),
            H::Expression(50, CLAP_NOTE_EXPRESSION_TUNING, 60, 2.0),
            H::Off(100, 60),
            H::Param(127, 2, 3.0),
        ]
    );
    let (flags, beats, tempo) = unsafe { fake(processor.plugin) }.transport.get().unwrap();
    assert!(flags & CLAP_TRANSPORT_IS_PLAYING != 0);
    assert_eq!((beats, tempo), (2 * CLAP_BEATTIME_FACTOR, 120.0));
    // Its level, 1, on both sides.
    assert!(l.iter().chain(&r).all(|&s| s == 1.0));
    // Automation is for one block; the transport moves on with it.
    processor.process([&mut l, &mut r], 128, &[], &[], None);
    assert_eq!(heard(&processor), []);
    let beats = unsafe { fake(processor.plugin) }.transport.get().unwrap().1;
    let next = transport(true, 48_128, 120.0, 48_000.0);
    assert_eq!(beats, next.song_pos_beats);
}

#[test]
fn values_turn_into_clap_s() {
    assert_eq!((bend(-1.0), bend(0.0), bend(1.0)), (0, 8192, 16383));
    assert_eq!(seven(0.5), 64);
    let pan = expression(ExpressionKind::Pan, -1.0);
    assert_eq!(pan, (CLAP_NOTE_EXPRESSION_PAN, 0.0));
    let volume = expression(ExpressionKind::Volume, 1.0);
    assert_eq!(volume, (CLAP_NOTE_EXPRESSION_VOLUME, 1.0));
    let mode = Range {
        id: 2,
        min: 0.0,
        max: 3.0,
        stepped: true,
    };
    let (plain, normal) = (mode.plain(0.4), mode.normal(2.0));
    assert_eq!((plain, normal, mode.steps()), (1.0, 2.0 / 3.0, 3));
    // Five and a half beats in: the second bar, which starts on beat 4.
    let t = transport(false, 132_000, 120.0, 48_000.0);
    assert_eq!((t.bar_number, t.bar_start), (1, 4 * CLAP_BEATTIME_FACTOR));
    assert_eq!(t.flags & CLAP_TRANSPORT_IS_PLAYING, 0);
}

#[test]
fn a_plugin_that_only_takes_midi_hears_notes_as_midi() {
    let note = Note {
        offset: 3,
        pitch: 64,
        velocity: 0.5,
    };
    let on = note_event(&note, 3, true);
    assert_eq!(unsafe { on.midi.data }, [0x90, 64, 64]);
    let off = note_event(
        &Note {
            velocity: 0.0,
            ..note
        },
        3,
        true,
    );
    assert_eq!(unsafe { off.midi.data }, [0x80, 64, 0]);
    let vibrato = Control {
        offset: 0,
        kind: ControlKind::Expression {
            pitch: 64,
            kind: ExpressionKind::Vibrato,
            value: 1.0,
        },
    };
    assert!(control_event(&vibrato, 0, true).is_none());
}

#[test]
fn parameters_are_read_turned_and_typed_as_the_plugin_says() {
    let (plugin, processor) = load(c"pina.test-synth", None);
    let params = plugin.params();
    let shown: Vec<_> = params
        .iter()
        .map(|p| (p.id, &p.name[..], p.steps))
        .collect();
    assert_eq!(shown, [(1, "Level", 0), (2, "Mode", 3)]);
    assert_eq!((params[0].value, params[0].default), (0.5, 0.5));
    // Nothing's running it, so a change takes at once.
    plugin.set_param(1, 0.75);
    assert_eq!(plugin.param_value(1), 0.75);
    assert!(!processor.pending());
    assert_eq!(plugin.param_text(2, 1.0), "Noise");
    assert_eq!(plugin.param_text(1, 0.5), "1.00");
    assert_eq!(plugin.param_from_text(1, "0.5"), Some(0.25));
    assert_eq!(value_from_text(&plugin, &params[1], "Saw"), Ok(1.0 / 3.0));
}

#[test]
fn edits_go_through_a_running_processor() {
    let (plugin, mut processor) = load(c"pina.test-synth", None);
    let (mut l, mut r) = (vec![0f32; 64], vec![0f32; 64]);
    processor.process([&mut l, &mut r], 64, &[], &[], None);
    // Just now, however slow the machine running the test.
    processor.inst.processed.store(now(), SeqCst);
    plugin.set_param(1, 0.25);
    assert!(processor.pending());
    // It reads back as set, though the plugin hasn't had it yet.
    let fake = unsafe { fake(processor.plugin) };
    let level = || fake.level.get();
    assert_eq!((plugin.param_value(1), level()), (0.25, 1.0));
    processor.process([&mut l, &mut r], 64, &[], &[], None);
    assert!(!processor.pending());
    assert_eq!((plugin.param_value(1), level(), l[0]), (0.25, 0.5, 0.5));
    // Once it has, the plugin's own value counts again (moved in its editor, say).
    fake.level.set(2.0);
    assert_eq!(plugin.param_value(1), 1.0);
}

#[test]
fn settings_round_trip() {
    let synth = c"pina.test-synth";
    let (plugin, _processor) = load(synth, None);
    plugin.set_param(1, 0.25);
    plugin.set_param(2, 1.0);
    let state = plugin.state().unwrap();
    let (again, _p) = load(synth, Some(&state));
    assert_eq!((again.param_value(1), again.param_value(2)), (0.25, 1.0));
    plugin.set_param(1, 1.0);
    plugin.set_state(&state).unwrap();
    assert_eq!(plugin.param_value(1), 0.25);
    assert!(plugin.set_state(b"junk").is_err());
}

#[test]
fn an_effect_hears_its_input_and_sidechain() {
    let (plugin, mut fx) = load(c"pina.test-fx", None);
    assert!(plugin.sidechain() && fx.sidechain());
    assert!(!load(c"pina.test-synth", None).0.sidechain());
    let (mut l, mut r) = (vec![1f32; 64], vec![0.5f32; 64]);
    let side = vec![0.25f32; 64];
    fx.process([&mut l, &mut r], 64, &[], &[], Some([&side, &side]));
    assert!(l.iter().all(|&s| s == 1.25) && r.iter().all(|&s| s == 0.75));
    // Either half can go last: the plugin is shut down then, in order.
    drop(fx);
    drop(plugin);
}

/// Runs `blocks` blocks of 512, the notes and controls in the first, and returns what it
/// played on the left.
fn run(p: &mut Processor, blocks: usize, notes: &[Note], controls: &[Control]) -> Vec<f32> {
    let mut played = Vec::new();
    for b in 0..blocks {
        let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
        let (notes, controls) = if b == 0 {
            (notes, controls)
        } else {
            (&[][..], &[][..])
        };
        p.process([&mut l, &mut r], 512, notes, controls, None);
        played.extend(l);
    }
    played
}

fn loudest(s: &[f32]) -> f32 {
    s.iter().fold(0f32, |m, x| m.max(x.abs()))
}

/// How often it goes up through zero: its pitch, roughly.
fn rises(s: &[f32]) -> usize {
    s.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count()
}

/// With a real instrument, which CI doesn't have:
/// `PINA_TEST_CLAP="/path/Some Synth.clap" cargo test -p pina-engine clap -- --ignored`
/// (skipped without it, so the other formats' real-plugin tests run alone).
#[test]
#[ignore]
fn a_real_instrument_plays_saves_restores_and_shuts_down() {
    let Ok(path) = std::env::var("PINA_TEST_CLAP") else {
        return eprintln!("skipped: set PINA_TEST_CLAP to a .clap instrument");
    };
    let found = inspect(Path::new(&path)).unwrap();
    let info = found.iter().find(|p| p.instrument).expect("no instrument");
    let r = info.plugin_ref();
    let mut libraries = HashMap::new();
    let (plugin, mut processor) =
        super::instantiate(&mut libraries, &r, 48_000.0, 512, None).unwrap();
    let note = |velocity| Note {
        offset: 0,
        pitch: 60,
        velocity,
    };
    let played = run(&mut processor, 20, &[note(1.0)], &[]);
    assert!(loudest(&played) > 1e-3, "{}", loudest(&played));

    // Pitch bend reaches it: the held note goes up.
    let bent = |v| Control {
        offset: 0,
        kind: ControlKind::PitchBend(v),
    };
    let before = rises(&run(&mut processor, 40, &[], &[]));
    run(&mut processor, 4, &[], &[bent(1.0)]);
    let after = rises(&run(&mut processor, 40, &[], &[]));
    eprintln!("bending up: {before} rises, then {after}");
    assert!(after as f64 > before as f64 * 1.05);
    run(&mut processor, 1, &[note(0.0)], &[bent(0.0)]);

    // Its parameters can be read and turned, while it runs and while it doesn't.
    let params = plugin.params();
    assert!(!params.is_empty());
    let p = params.iter().find(|p| p.steps == 0).unwrap();
    let to = if p.value > 0.5 { 0.25 } else { 0.75 };
    plugin.set_param(p.id, to);
    run(&mut processor, 1, &[], &[]);
    let read = plugin.param_value(p.id);
    assert!((read - to).abs() < 0.02, "{} is {read}", p.name);
    std::thread::sleep(Duration::from_millis(2 * RUNNING_MS));
    plugin.set_param(p.id, p.value);
    let read = plugin.param_value(p.id);
    assert!((read - p.value).abs() < 0.02, "{} is {read}", p.name);
    eprintln!("{}: {} parameters", plugin.name(), params.len());

    // And typed: a display reads back as the value shown. Only where the display follows
    // the value asked about: Surge's macros and Dexed's parameters show what's set.
    let varies = |p: &&PluginParam| plugin.param_text(p.id, 0.25) != plugin.param_text(p.id, 0.75);
    match params.iter().filter(|p| p.steps == 0).find(varies) {
        Some(p) => {
            let shown = plugin.param_text(p.id, to);
            let typed = value_from_text(&*plugin, p, &shown).unwrap();
            eprintln!("{} at {to} shows {shown:?}, which reads as {typed}", p.name);
            assert!((typed - to).abs() < 0.02);
        }
        None => eprintln!("its displays show what's set, whatever's asked"),
    }

    // Its settings survive a reload.
    let state = plugin.state().unwrap();
    drop(plugin);
    drop(processor);
    let (again, mut processor) =
        super::instantiate(&mut libraries, &r, 48_000.0, 512, Some(&state)).unwrap();
    let read = again.param_value(p.id);
    assert!((read - p.value).abs() < 0.02, "{} is {read}", p.name);
    assert!(loudest(&run(&mut processor, 20, &[note(1.0)], &[])) > 1e-3);
    run(&mut processor, 1, &[note(0.0)], &[]);

    // Its presets: those it finds, and Pina Studio's own files.
    let presets = again.presets();
    let some: Vec<_> = presets.iter().take(3).collect();
    eprintln!("{} presets, like {some:?}", presets.len());
    if let Some(preset) = presets.get(presets.len() / 2) {
        let before = again.state().unwrap();
        again.load_preset(preset).unwrap();
        // A plugin may load it in the background while it plays (Surge does): give it time.
        let changed = (0..200).any(|_| {
            run(&mut processor, 1, &[], &[]);
            std::thread::sleep(Duration::from_millis(10));
            again.state().unwrap() != before
        });
        assert!(changed, "{preset:?} changed nothing");
    }
    let saved = again.save_preset("pina clap test").unwrap();
    let file = PresetSource::File {
        file: saved.clone(),
    };
    let ours = again.presets().into_iter().find(|p| p.source == file);
    let loaded = ours.map(|p| again.load_preset(&p));
    std::fs::remove_file(&saved).unwrap();
    loaded.expect("its saved preset isn't listed").unwrap();
}

/// `PINA_TEST_CLAP_FX=/path/DragonflyRoomReverb.clap cargo test -p pina-engine clap -- --ignored`
#[test]
#[ignore]
fn a_real_effect_processes_audio() {
    let Ok(path) = std::env::var("PINA_TEST_CLAP_FX") else {
        return eprintln!("skipped: set PINA_TEST_CLAP_FX to a .clap effect");
    };
    let found = inspect(Path::new(&path)).unwrap();
    let info = found.iter().find(|p| !p.instrument).expect("no effect");
    let mut libraries = HashMap::new();
    let (plugin, mut fx) =
        super::instantiate(&mut libraries, &info.plugin_ref(), 48_000.0, 512, None).unwrap();
    let (mut heard, mut changed, mut tail) = (0f32, 0f32, 0f32);
    for b in 0..60 {
        let sine = |i: usize| {
            let t = (b * 512 + i) as f32 / 48_000.0;
            (t * 440.0 * std::f32::consts::TAU).sin() * 0.5
        };
        let input: Vec<f32> = (0..512)
            .map(|i| if b < 10 { sine(i) } else { 0.0 })
            .collect();
        let (mut l, mut r) = (input.clone(), input.clone());
        fx.process([&mut l, &mut r], 512, &[], &[], None);
        if b < 10 {
            heard = heard.max(loudest(&l));
            let diff = l.iter().zip(&input).map(|(a, b)| (a - b).abs());
            changed = diff.fold(changed, f32::max);
        } else {
            tail = tail.max(loudest(&l));
        }
    }
    eprintln!(
        "{}: loudest {heard}, most changed {changed}, tail {tail}, sidechain {}, {} parameters",
        plugin.name(),
        fx.sidechain(),
        plugin.params().len()
    );
    assert!(heard > 1e-3 && changed > 1e-3);
    let state = plugin.state().unwrap();
    plugin.set_state(&state).unwrap();
}
