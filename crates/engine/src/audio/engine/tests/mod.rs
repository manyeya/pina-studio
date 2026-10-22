mod audio;
mod latency;
mod live;
mod mixing;
mod modulation;
mod notes;

use super::*;
use crate::song::project::{AuxSend, Note as MidiNote, PPQ, Track};

const RATE: f64 = 48_000.0;
const BEAT: usize = 24_000; // samples per beat at 120 bpm
const RELEASE: usize = 2_400; // the synth's 50 ms

fn synth_track(name: &str, notes: Vec<MidiNote>, length: u64) -> Track {
    Track {
        name: name.into(),
        instrument: Some(Instrument::Synth),
        regions: vec![Region {
            start: 0,
            length,
            content: Content::Midi {
                notes,
                controls: Vec::new(),
            },
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn note(start: i64, length: u64, pitch: u8) -> MidiNote {
    MidiNote {
        start,
        length,
        pitch,
        velocity: 127,
    }
}

fn run(engine: &mut Engine, frames: usize) -> [Vec<f32>; 2] {
    let mut out = [vec![0f32; frames], vec![0f32; frames]];
    for at in (0..frames).step_by(MAX_BLOCK) {
        let n = MAX_BLOCK.min(frames - at);
        let [l, r] = &mut out;
        engine.process([&mut l[at..], &mut r[at..]], n);
    }
    out
}

fn render(project: &Project, dir: &Path, frames: usize) -> [Vec<f32>; 2] {
    let (mut engine, _) = Engine::load(project, dir, RATE).unwrap();
    engine.play();
    run(&mut engine, frames)
}

fn project(tracks: Vec<Track>) -> Project {
    Project {
        tracks,
        ..Project::new(RATE as u32)
    }
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0, |m, x| m.max(x.abs()))
}

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pina-test-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A plugin stand-in: it notes the controls it hears (at the sample they fall on) and the
/// first sample its sidechain isn't silent; as an effect it passes audio through.
#[derive(Default, Clone)]
struct Probe {
    heard: std::sync::Arc<std::sync::Mutex<Vec<(u64, ControlKind)>>>,
    sidechain_from: std::sync::Arc<std::sync::Mutex<Option<u64>>>,
    at: u64,
}

impl crate::plugin::ProcessorApi for Probe {
    fn latency(&self) -> u32 {
        0
    }
    fn set_transport(&mut self, _: bool, position: u64, _: f64) {
        self.at = position;
    }
    fn automate(&mut self, _: u32, _: f64, _: f64) {}
    fn sidechain(&self) -> bool {
        true
    }
    fn pending(&self) -> bool {
        false
    }
    fn process(
        &mut self,
        _: [&mut [f32]; 2],
        frames: usize,
        _: &[Note],
        controls: &[Control],
        sidechain: Option<[&[f32]; 2]>,
    ) {
        let mut heard = self.heard.lock().unwrap();
        heard.extend(controls.iter().map(|c| (self.at + c.offset as u64, c.kind)));
        if let Some([l, _]) = sidechain
            && let Some(i) = l[..frames].iter().position(|x| x.abs() > 1e-3)
        {
            let mut from = self.sidechain_from.lock().unwrap();
            from.get_or_insert(self.at + i as u64);
        }
        self.at += frames as u64;
    }
}

/// An engine for `p` whose tracks have their built-in instruments, but `probe` as the
/// instrument of the tracks with a plugin and the insert of tracks with inserts.
fn probed(p: &Project, probe: &Probe) -> Engine {
    let mut lanes = Vec::new();
    for t in &p.tracks {
        let built_in = t
            .instrument
            .as_ref()
            .filter(|i| !matches!(i, Instrument::Plugin(_)));
        let made = make_lane(built_in, &[], RATE, &mut Modules::new(), Path::new("."));
        let mut lane = made.unwrap().0;
        if matches!(t.instrument, Some(Instrument::Plugin(_))) {
            lane.source = Source::Plugin(Box::new(probe.clone()));
        }
        if !t.inserts.is_empty() {
            lane.inserts.push(Box::new(probe.clone()));
        }
        lanes.push(Some(Box::new(lane)));
    }
    let tracks: Vec<_> = (0..p.tracks.len()).map(|i| (i, 0)).collect();
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (arrangement, problems) = compile(p, RATE, &tracks, &[], None, &mut files);
    assert!(problems.is_empty(), "{problems:?}");
    Engine::new(RATE, arrangement, lanes)
}

fn note_ons(p: &Project) -> Vec<(u64, u8, u8)> {
    let mut files = AudioFiles::new(Path::new("."), RATE);
    let (a, _) = compile(p, RATE, &[(0, 0)], &[], None, &mut files);
    a.plans[0]
        .events
        .iter()
        .filter(|e| e.velocity > 0.0)
        .map(|e| (e.at, e.pitch, (e.velocity * 127.0).round() as u8))
        .collect()
}
