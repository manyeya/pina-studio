//! Plays a project: each track becomes a lane (its regions as note events and audio clips, its
//! instrument, inserts and gain) and the engine mixes the lanes a block at a time. The same engine
//! runs live, from the audio callback, and offline, for a bounce.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{
    plugin::{AUDIO_MODULE, Module, Note, Plugin, Processor},
    project::{Content, Instrument, PluginRef, Project},
    synth::Synth,
};

/// The most frames one `process` call handles.
pub const MAX_BLOCK: usize = 512;
/// Most note events one lane takes in one block; more are dropped.
const BLOCK_NOTES: usize = 256;

/// An audio file in memory, one or two channels.
// ponytail: whole files in RAM; stream from disk when sessions outgrow memory.
pub struct AudioData {
    channels: Vec<Vec<f32>>,
}

struct Clip {
    /// Timeline position and length, and where in the file it starts, all in samples.
    start: u64,
    len: u64,
    offset: u64,
    data: Arc<AudioData>,
}

#[derive(Clone, Copy)]
struct Event {
    at: u64,
    pitch: u8,
    /// 0 is a note off.
    velocity: f32,
}

#[allow(clippy::large_enum_variant)] // one per track
enum Source {
    Audio,
    Synth(Synth),
    Plugin(Processor),
}

/// A track, ready to play.
struct Lane {
    source: Source,
    /// Sorted by start.
    clips: Vec<Clip>,
    /// Sorted by time, offs before ons at the same sample.
    events: Vec<Event>,
    inserts: Vec<Processor>,
    gain: [f32; 2],
    /// Notes this lane's instrument is holding, to release on stop or locate.
    held: [bool; 128],
}

pub struct Engine {
    rate: f64,
    tempo: f64,
    lanes: Vec<Lane>,
    master: f32,
    position: u64,
    playing: bool,
    /// Release every held note at the start of the next block.
    release: bool,
    scratch: [Vec<f32>; 2],
}

fn db(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

impl Engine {
    /// Builds the engine for `project` (whose files are in `dir`) at `rate`. The plugins it returns
    /// are the main-thread halves of the engine's plugin processors: keep them as long as it.
    pub fn load(project: &Project, dir: &Path, rate: f64) -> Result<(Engine, Vec<Plugin>), String> {
        let mut modules: HashMap<PathBuf, Module> = HashMap::new();
        let mut plugins = Vec::new();
        let mut instantiate = |r: &PluginRef| -> Result<Processor, String> {
            if !modules.contains_key(&r.path) {
                modules.insert(r.path.clone(), Module::load(&r.path)?);
            }
            let module = &modules[&r.path];
            let classes = module.classes();
            let class = classes
                .iter()
                .find(|c| c.category == AUDIO_MODULE && c.name == r.class)
                .ok_or_else(|| format!("{}: no plugin named {:?}", r.path.display(), r.class))?;
            let (plugin, processor) = Plugin::instantiate(module, class, rate, MAX_BLOCK)?;
            plugins.push(plugin);
            Ok(processor)
        };

        let mut files: HashMap<&Path, Arc<AudioData>> = HashMap::new();
        let any_solo = project.tracks.iter().any(|t| t.solo);
        let mut lanes = Vec::new();
        for t in &project.tracks {
            let source = match &t.instrument {
                None => Source::Audio,
                Some(Instrument::Synth) => Source::Synth(Synth::new(rate)),
                Some(Instrument::Vst3(r)) => Source::Plugin(instantiate(r)?),
            };
            let inserts = t
                .inserts
                .iter()
                .map(&mut instantiate)
                .collect::<Result<_, _>>()?;

            let (mut clips, mut events) = (Vec::new(), Vec::new());
            for region in &t.regions {
                let start = project.samples(region.start, rate);
                let end = project.samples(region.start + region.length, rate);
                match &region.content {
                    // Notes stop at the region's end, and notes starting after it don't play.
                    Content::Midi { notes } => {
                        for n in notes.iter().filter(|n| n.start < region.length) {
                            let at = project.samples(region.start + n.start, rate);
                            let off = project
                                .samples(region.start + n.start + n.length, rate)
                                .min(end);
                            events.push(Event {
                                at,
                                pitch: n.pitch,
                                velocity: n.velocity as f32 / 127.0,
                            });
                            events.push(Event {
                                at: off,
                                pitch: n.pitch,
                                velocity: 0.0,
                            });
                        }
                    }
                    Content::Audio { file, offset } => {
                        let data = match files.get(file.as_path()) {
                            Some(d) => d.clone(),
                            None => {
                                let d = Arc::new(load_wav(&dir.join(file), rate)?);
                                files.insert(file, d.clone());
                                d
                            }
                        };
                        clips.push(Clip {
                            start,
                            len: end - start,
                            offset: *offset,
                            data,
                        });
                    }
                }
            }
            // Offs first at the same sample, so back-to-back notes of one pitch both sound.
            events.sort_by_key(|e| (e.at, e.velocity > 0.0));
            clips.sort_by_key(|c| c.start);

            let g = if t.mute || (any_solo && !t.solo) {
                0.0
            } else {
                db(t.volume_db)
            };
            // Balance: panning turns the other side down.
            let pan = t.pan.clamp(-1.0, 1.0);
            let gain = [g * (1.0 - pan).min(1.0), g * (1.0 + pan).min(1.0)];
            lanes.push(Lane {
                source,
                clips,
                events,
                inserts,
                gain,
                held: [false; 128],
            });
        }

        let engine = Engine {
            rate,
            tempo: project.tempo,
            lanes,
            master: db(project.master_volume_db),
            position: 0,
            playing: false,
            release: false,
            scratch: [vec![0.0; MAX_BLOCK], vec![0.0; MAX_BLOCK]],
        };
        Ok((engine, plugins))
    }

    pub fn rate(&self) -> f64 {
        self.rate
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.release = true;
    }

    pub fn locate(&mut self, sample: u64) {
        self.position = sample;
        self.release = true;
    }

    /// Writes the next `frames` (at most `MAX_BLOCK`) of the mix into `out` and, if playing, moves
    /// the playhead on. Instruments and effects run while stopped too, so tails ring out.
    /// Allocation-free: safe on the audio thread.
    pub fn process(&mut self, out: [&mut [f32]; 2], frames: usize) {
        let [out_l, out_r] = out;
        out_l[..frames].fill(0.0);
        out_r[..frames].fill(0.0);
        let (from, to) = (self.position, self.position + frames as u64);
        let release = std::mem::take(&mut self.release);
        let (playing, position, tempo) = (self.playing, self.position, self.tempo);
        let [l, r] = &mut self.scratch;
        let (l, r) = (&mut l[..frames], &mut r[..frames]);

        for lane in &mut self.lanes {
            l.fill(0.0);
            r.fill(0.0);
            let mut notes = [Note::default(); BLOCK_NOTES];
            let mut n = 0;
            if release {
                for (pitch, held) in lane.held.iter_mut().enumerate().filter(|(_, h)| **h) {
                    *held = false;
                    notes[n] = Note {
                        offset: 0,
                        pitch: pitch as u8,
                        velocity: 0.0,
                    };
                    n += 1;
                }
            }
            if playing {
                let first = lane.events.partition_point(|e| e.at < from);
                for e in lane.events[first..].iter().take_while(|e| e.at < to) {
                    if n == BLOCK_NOTES {
                        break;
                    }
                    lane.held[e.pitch as usize] = e.velocity > 0.0;
                    notes[n] = Note {
                        offset: (e.at - from) as u32,
                        pitch: e.pitch,
                        velocity: e.velocity,
                    };
                    n += 1;
                }
                // ponytail: walks the lane's clips from the first every block; index them by time
                // if a track gets thousands of regions.
                for c in lane.clips.iter().take_while(|c| c.start < to) {
                    let (a, b) = (from.max(c.start), to.min(c.start + c.len));
                    for (ch, dst) in [&mut *l, &mut *r].into_iter().enumerate() {
                        let src = &c.data.channels[ch.min(c.data.channels.len() - 1)];
                        for t in a..b {
                            let i = (c.offset + t - c.start) as usize;
                            dst[(t - from) as usize] += src.get(i).copied().unwrap_or(0.0);
                        }
                    }
                }
            }
            let notes = &notes[..n];
            match &mut lane.source {
                Source::Audio => {}
                Source::Synth(s) => s.process([&mut *l, &mut *r], frames, notes),
                Source::Plugin(p) => {
                    p.set_transport(playing, position, tempo);
                    p.process([&mut *l, &mut *r], frames, notes);
                }
            }
            for p in &mut lane.inserts {
                p.set_transport(playing, position, tempo);
                p.process([&mut *l, &mut *r], frames, &[]);
            }
            for i in 0..frames {
                out_l[i] += l[i] * lane.gain[0];
                out_r[i] += r[i] * lane.gain[1];
            }
        }
        for s in out_l.iter_mut().chain(out_r.iter_mut()).take(frames * 2) {
            *s *= self.master;
        }
        if playing {
            self.position = to;
        }
    }

    /// Plays `frames` from the start into a stereo 32-bit float WAV.
    pub fn bounce(&mut self, path: &Path, frames: u64) -> Result<(), String> {
        let err = |e: hound::Error| format!("{}: {e}", path.display());
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: self.rate as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut wav = hound::WavWriter::create(path, spec).map_err(err)?;
        let (mut l, mut r) = (vec![0f32; MAX_BLOCK], vec![0f32; MAX_BLOCK]);
        self.locate(0);
        self.play();
        let mut left = frames;
        while left > 0 {
            let n = left.min(MAX_BLOCK as u64) as usize;
            self.process([&mut l, &mut r], n);
            for i in 0..n {
                wav.write_sample(l[i]).map_err(err)?;
                wav.write_sample(r[i]).map_err(err)?;
            }
            left -= n as u64;
        }
        self.stop();
        wav.finalize().map_err(err)
    }

    /// Plays the engine through the default output device, set to the engine's rate.
    pub fn start_output(mut self) -> Result<cpal::Stream, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no audio output device")?;
        let rate = self.rate as u32;
        let config = device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .find(|c| {
                c.sample_format() == cpal::SampleFormat::F32
                    && c.channels() >= 2
                    && (c.min_sample_rate()..=c.max_sample_rate()).contains(&rate)
            })
            .ok_or_else(|| format!("the output device can't run at {rate} Hz"))?
            .with_sample_rate(rate)
            .config();
        let ch = config.channels as usize;
        let (mut l, mut r) = ([0f32; MAX_BLOCK], [0f32; MAX_BLOCK]);
        let stream = device
            .build_output_stream(
                config,
                move |out: &mut [f32], _| {
                    for chunk in out.chunks_mut(MAX_BLOCK * ch) {
                        let frames = chunk.len() / ch;
                        self.process([&mut l, &mut r], frames);
                        for (f, frame) in chunk.chunks_mut(ch).enumerate() {
                            frame[0] = l[f];
                            frame[1] = r[f];
                            frame[2..].fill(0.0);
                        }
                    }
                },
                |e| eprintln!("audio: {e}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(stream)
    }
}

// ponytail: WAV only, at the engine's rate; add symphonia and rubato for AIFF/MP3 and resampling.
fn load_wav(path: &Path, rate: f64) -> Result<AudioData, String> {
    let err = |e: hound::Error| format!("{}: {e}", path.display());
    let mut wav = hound::WavReader::open(path).map_err(err)?;
    let spec = wav.spec();
    if spec.sample_rate as f64 != rate {
        return Err(format!(
            "{}: {} Hz, but the project runs at {rate} Hz",
            path.display(),
            spec.sample_rate
        ));
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => wav
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(err)?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            wav.samples::<i32>()
                .map(|s| s.map(|s| s as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(err)?
        }
    };
    let n = spec.channels as usize;
    let channels = (0..n.min(2))
        .map(|c| samples.iter().skip(c).step_by(n).copied().collect())
        .collect();
    Ok(AudioData { channels })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Note as MidiNote, PPQ, Region, Track};

    const RATE: f64 = 48_000.0;
    const BEAT: usize = 24_000; // samples per beat at 120 bpm
    const RELEASE: usize = 2_400; // the synth's 50 ms

    fn synth_track(name: &str, notes: Vec<MidiNote>, length: u64) -> Track {
        Track {
            name: name.into(),
            instrument: Some(Instrument::Synth),
            inserts: vec![],
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            regions: vec![Region {
                start: 0,
                length,
                content: Content::Midi { notes },
            }],
        }
    }

    fn note(start: u64, length: u64, pitch: u8) -> MidiNote {
        MidiNote {
            start,
            length,
            pitch,
            velocity: 127,
        }
    }

    fn render(project: &Project, dir: &Path, frames: usize) -> [Vec<f32>; 2] {
        let (mut engine, _) = Engine::load(project, dir, RATE).unwrap();
        engine.play();
        let mut out = [vec![0f32; frames], vec![0f32; frames]];
        for at in (0..frames).step_by(MAX_BLOCK) {
            let n = MAX_BLOCK.min(frames - at);
            let [l, r] = &mut out;
            engine.process([&mut l[at..], &mut r[at..]], n);
        }
        out
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
        let dir = std::env::temp_dir().join(format!("tempo-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn notes_start_on_their_beat_and_stop_at_the_region_end() {
        // A note from beat 2 that runs past the region, which ends at beat 3.
        let p = project(vec![synth_track(
            "a",
            vec![note(PPQ, PPQ * 4, 69)],
            PPQ * 2,
        )]);
        let [l, _] = render(&p, Path::new("."), BEAT * 4);
        assert_eq!(peak(&l[..BEAT + 1]), 0.0);
        assert!(peak(&l[BEAT..BEAT + 1000]) > 0.1);
        assert!(peak(&l[BEAT * 2 - 1000..BEAT * 2]) > 0.1);
        assert_eq!(peak(&l[BEAT * 2 + RELEASE + 10..]), 0.0);
    }

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
    fn audio_regions_play_their_slice_of_the_file() {
        let dir = scratch_dir("audio");
        let ramp: Vec<f32> = (0..BEAT * 2)
            .map(|i| i as f32 / (BEAT * 2) as f32)
            .collect();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: RATE as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(dir.join("ramp.wav"), spec).unwrap();
        ramp.iter().for_each(|&s| w.write_sample(s).unwrap());
        w.finalize().unwrap();

        // Half a beat in, one beat long, starting 100 samples into the file.
        let region = Region {
            start: PPQ / 2,
            length: PPQ,
            content: Content::Audio {
                file: "ramp.wav".into(),
                offset: 100,
            },
        };
        let track = Track {
            instrument: None,
            regions: vec![region],
            ..synth_track("audio", vec![], 0)
        };
        let [l, r] = render(&project(vec![track]), &dir, BEAT * 2);
        let start = BEAT / 2;
        assert_eq!(peak(&l[..start]), 0.0);
        assert_eq!(&l[start..start + BEAT], &ramp[100..100 + BEAT]);
        assert_eq!(peak(&l[start + BEAT..]), 0.0);
        assert_eq!(l, r);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stopping_releases_held_notes() {
        let p = project(vec![synth_track("a", vec![note(0, PPQ * 8, 69)], PPQ * 8)]);
        let (mut engine, _) = Engine::load(&p, Path::new("."), RATE).unwrap();
        let (mut l, mut r) = (vec![0f32; MAX_BLOCK], vec![0f32; MAX_BLOCK]);
        engine.play();
        for _ in 0..10 {
            engine.process([&mut l, &mut r], MAX_BLOCK);
        }
        assert!(peak(&l) > 0.1);
        engine.stop();
        for _ in 0..RELEASE / MAX_BLOCK + 1 {
            engine.process([&mut l, &mut r], MAX_BLOCK);
        }
        engine.process([&mut l, &mut r], MAX_BLOCK);
        assert_eq!(peak(&l), 0.0);
        assert_eq!(engine.position(), (MAX_BLOCK * 10) as u64);
    }

    #[test]
    fn bounce_writes_a_stereo_float_wav() {
        let dir = scratch_dir("bounce");
        let p = project(vec![synth_track("a", vec![note(0, PPQ, 69)], PPQ)]);
        let (mut engine, _) = Engine::load(&p, &dir, RATE).unwrap();
        engine.bounce(&dir.join("out.wav"), 30_000).unwrap();
        let wav = hound::WavReader::open(dir.join("out.wav")).unwrap();
        assert_eq!((wav.spec().channels, wav.duration()), (2, 30_000));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
