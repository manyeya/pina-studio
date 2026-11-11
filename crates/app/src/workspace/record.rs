//! Recording onto the selected track, armed: an instrument track records
//! the notes played on it (MIDI keyboards, musical typing), an audio track records the default
//! input into a new file under `Audio Files/`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use gpui::Context;
use pina_engine::{
    audio::record::AudioRecorder,
    song::project::{AUDIO_DIR, Content, ControlPoint, ControlValue, Note, Region},
};

use crate::workspace::Workspace;

pub enum Recording {
    Midi {
        track: u64,
        /// Where recording began, in ticks.
        start: u64,
        /// Notes still held: their start and velocity, by pitch.
        held: HashMap<u8, (u64, u8)>,
        notes: Vec<Note>,
        /// Controller moves and each note's own expression, from where recording began.
        controls: Vec<ControlPoint>,
    },
    Audio {
        track: u64,
        start: u64,
        /// Relative to the project folder.
        file: PathBuf,
        recorder: AudioRecorder,
        /// Samples off the file's front: counting in, and the input's lateness.
        skip: u64,
    },
}

impl Recording {
    pub fn track(&self) -> u64 {
        match self {
            Recording::Midi { track, .. } | Recording::Audio { track, .. } => *track,
        }
    }

    pub fn start(&self) -> u64 {
        match self {
            Recording::Midi { start, .. } | Recording::Audio { start, .. } => *start,
        }
    }
}

/// A file for a new take on a track: `Audio Files/<track> #<n>.wav`, the first n that's free.
fn take_file(dir: &Path, track: &str) -> PathBuf {
    let name = track.replace(['/', ':'], "-");
    (1..)
        .map(|n| PathBuf::from(AUDIO_DIR).join(format!("{name} #{n}.wav")))
        .find(|f| !dir.join(f).exists())
        .unwrap()
}

fn note(start: u64, on: u64, off: u64, pitch: u8, velocity: u8) -> Note {
    Note {
        start: on as i64 - start as i64,
        length: off.saturating_sub(on).max(1),
        pitch,
        velocity,
        ..Default::default()
    }
}

impl Workspace {
    /// Starts recording on the armed track at the playhead, or stops if it's recording.
    pub fn record(&mut self, cx: &mut Context<Self>) {
        if self.recording.is_some() {
            self.stop(cx);
            return;
        }
        let armed = self.armed();
        let project = &self.session.project;
        let Some(track) = armed.and_then(|t| project.tracks.get(t)) else {
            self.status = Some(match project.tracks.is_empty() {
                true => "Add a track to record on.".into(),
                false => "Arm a track to record on: its R button.".into(),
            });
            cx.notify();
            return;
        };
        let (id, start) = (track.id, self.playhead());
        self.recording = Some(if track.instrument.is_some() {
            // Notes and moves from before don't count.
            self.player.played();
            self.player.moves();
            Recording::Midi {
                track: id,
                start,
                held: HashMap::new(),
                notes: Vec::new(),
                controls: Vec::new(),
            }
        } else {
            let file = take_file(&self.session.dir, &track.name);
            let path = self.session.dir.join(&file);
            // At the engine's rate if the input can; from the input picked, as deep as set.
            let input = self.settings.audio.input.clone();
            match AudioRecorder::start_from(
                &path,
                self.player.rate() as u32,
                track.input,
                input.as_deref(),
                self.settings.record_depth,
            ) {
                Ok(recorder) => {
                    // Off the front: what came in while counting in, and the input's lateness.
                    let secs =
                        self.count_in_secs() + self.settings.record_offset_ms as f64 / 1000.0;
                    let skip = (secs.max(0.0) * recorder.rate() as f64) as u64;
                    Recording::Audio {
                        track: id,
                        start,
                        file,
                        recorder,
                        skip,
                    }
                }
                Err(e) => {
                    self.status = Some(e);
                    cx.notify();
                    return;
                }
            }
        });
        match self.settings.count_in {
            0 => self.player.play(),
            bars => self.player.play_counting(bars as u32),
        }
        self.gate_click();
        cx.notify();
    }

    /// How long counting in takes from the playhead, in seconds (none: 0).
    pub(crate) fn count_in_secs(&self) -> f64 {
        let p = &self.session.project;
        let at = self.playhead();
        let bars = self.settings.count_in as f64;
        (p.seconds_at(at as f64 + p.bar() as f64) - p.seconds_at(at as f64)) * bars
    }

    /// Takes in what came in since the last frame. Live notes that arrive while not recording
    /// are dropped.
    pub fn capture(&mut self, cx: &mut Context<Self>) {
        let played = self.player.played();
        let moves = self.player.moves();
        let notes: Vec<(u64, u8, u8)> =
            played.iter().map(|p| (p.at, p.pitch, p.velocity)).collect();
        self.steps_input(&notes, cx);
        let (project, rate) = (&self.session.project, self.player.rate());
        match &mut self.recording {
            Some(Recording::Midi {
                start,
                held,
                notes,
                controls,
                ..
            }) => {
                for (at, kind) in moves {
                    let tick = project.ticks(at, rate);
                    controls.push(ControlPoint {
                        start: tick as i64 - *start as i64,
                        kind: ControlValue::of(kind),
                    });
                }
                for p in played {
                    let tick = project.ticks(p.at, rate);
                    if p.velocity > 0 {
                        held.insert(p.pitch, (tick, p.velocity));
                    } else if let Some((on, velocity)) = held.remove(&p.pitch) {
                        let mut n = note(*start, on, tick, p.pitch, velocity);
                        if p.release > 0 {
                            n.release = p.release;
                        }
                        notes.push(n);
                    }
                }
            }
            Some(Recording::Audio { recorder, .. }) => {
                if let Err(e) = recorder.drain() {
                    self.status = Some(e);
                    self.stop(cx);
                }
            }
            None => {}
        }
    }

    /// Ends the recording: what it caught becomes a region on its track, one undoable edit.
    pub fn finish_recording(&mut self, cx: &mut Context<Self>) {
        self.capture(cx);
        let Some(recording) = self.recording.take() else {
            return;
        };
        let end = self.playhead();
        let (track, region) = match recording {
            Recording::Midi {
                track,
                start,
                held,
                mut notes,
                controls,
            } => {
                for (pitch, (on, velocity)) in held {
                    notes.push(note(start, on, end, pitch, velocity));
                }
                if notes.is_empty() {
                    return;
                }
                // Whole bars, at least to the end of the last note.
                let last = notes
                    .iter()
                    .map(|n| n.start as u64 + n.length)
                    .max()
                    .unwrap();
                let bar = self.session.project.bar();
                let length = end.saturating_sub(start).max(last).div_ceil(bar).max(1) * bar;
                // Each note's own bend, slide and pressure onto it.
                let mut content = Content::Midi { notes, controls };
                content.adopt_expression();
                (
                    track,
                    Region {
                        start,
                        length,
                        content,
                        ..Default::default()
                    },
                )
            }
            Recording::Audio {
                track,
                start,
                file,
                recorder,
                skip,
            } => match (recorder.rate(), recorder.finish()) {
                (_, Ok(0)) => return,
                (rate, Ok(frames)) => {
                    // Its frames at the song's rate, past what came off the front.
                    let kept = frames.saturating_sub(skip);
                    if kept == 0 {
                        return;
                    }
                    let song = self.session.project.sample_rate as f64;
                    let at_song = (kept as f64 * song / rate as f64).round() as u64;
                    let length = self.session.project.ticks_for(start, at_song).max(1);
                    let content = Content::Audio {
                        file,
                        offset: skip,
                        stretch: 1.0,
                        stretch_mode: Default::default(),
                        follow: pina_engine::song::project::Follow::Off,
                        tempo: None,
                        grid: None,
                        formants: false,
                        warps: Vec::new(),
                    };
                    (
                        track,
                        Region {
                            start,
                            length,
                            content,
                            ..Default::default()
                        },
                    )
                }
                (_, Err(e)) => {
                    self.status = Some(e);
                    return;
                }
            },
        };
        self.gate_click();
        let Some(t) = self
            .session
            .project
            .tracks
            .iter()
            .position(|t| t.id == track)
        else {
            return;
        };
        // Over a region of its kind, the take joins it in a take folder.
        self.edit(cx, |ws| {
            let end = region.start + region.length;
            let at = ws
                .session
                .project
                .add_take(t, region.start, end, region.content);
            ws.region = Some(at);
            ws.picked.clear();
        });
        // A take of its own (not in a folder): listened to for its tempo, the song's tempo mode
        // saying what's done with it.
        if let Some(at) = self.region
            && matches!(
                self.session.project.region(at).content,
                Content::Audio { .. }
            )
        {
            self.find_tempo(at, crate::arrange::tempo::After::Mode, cx);
        }
    }
}
