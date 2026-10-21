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
    song::project::{AUDIO_DIR, Content, Note, Region},
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
    },
    Audio {
        track: u64,
        start: u64,
        /// Relative to the project folder.
        file: PathBuf,
        recorder: AudioRecorder,
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
            self.player.played(); // notes from before don't count
            Recording::Midi {
                track: id,
                start,
                held: HashMap::new(),
                notes: Vec::new(),
            }
        } else {
            let file = take_file(&self.session.dir, &track.name);
            let path = self.session.dir.join(&file);
            match AudioRecorder::start(&path, project.sample_rate, track.input) {
                Ok(recorder) => Recording::Audio {
                    track: id,
                    start,
                    file,
                    recorder,
                },
                Err(e) => {
                    self.status = Some(e);
                    cx.notify();
                    return;
                }
            }
        });
        self.player.play();
        cx.notify();
    }

    /// Takes in what came in since the last frame. Live notes that arrive while not recording
    /// are dropped.
    pub fn capture(&mut self, cx: &mut Context<Self>) {
        let played = self.player.played();
        let project = &self.session.project;
        match &mut self.recording {
            Some(Recording::Midi {
                start, held, notes, ..
            }) => {
                for p in played {
                    let tick = project.ticks(p.at);
                    if p.velocity > 0 {
                        held.insert(p.pitch, (tick, p.velocity));
                    } else if let Some((on, velocity)) = held.remove(&p.pitch) {
                        notes.push(note(*start, on, tick, p.pitch, velocity));
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
                let content = Content::Midi {
                    notes,
                    controls: Vec::new(),
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
            Recording::Audio {
                track,
                start,
                file,
                recorder,
            } => match recorder.finish() {
                Ok(0) => return,
                Ok(frames) => {
                    let length = self.session.project.ticks(frames).max(1);
                    let content = Content::Audio {
                        file,
                        offset: 0,
                        stretch: 1.0,
                        flex: Default::default(),
                        follow: false,
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
                Err(e) => {
                    self.status = Some(e);
                    return;
                }
            },
        };
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
    }
}
