//! Standard MIDI Files: read (both layouts, every part with its notes, controllers, pitch bend
//! and pressure, its instrument and name; the file's tempo, tempo changes, meter and key), and
//! brought into a song as tracks that play straight away: drums on a kit, every other part on
//! General MIDI's instrument for its program (Apple's General MIDI synth, on macOS).

use std::path::Path;

use crate::{
    music::Key,
    song::project::{
        Content, ControlPoint, ControlValue, Format, Instrument, Note, PPQ, PluginRef, Preset,
        PresetSource, Project, Region, Track,
    },
};

/// General MIDI's instruments, by program.
pub const GM: [&str; 128] = [
    "Acoustic Grand Piano",
    "Bright Acoustic Piano",
    "Electric Grand Piano",
    "Honky-tonk Piano",
    "Electric Piano 1",
    "Electric Piano 2",
    "Harpsichord",
    "Clavinet",
    "Celesta",
    "Glockenspiel",
    "Music Box",
    "Vibraphone",
    "Marimba",
    "Xylophone",
    "Tubular Bells",
    "Dulcimer",
    "Drawbar Organ",
    "Percussive Organ",
    "Rock Organ",
    "Church Organ",
    "Reed Organ",
    "Accordion",
    "Harmonica",
    "Tango Accordion",
    "Acoustic Guitar (nylon)",
    "Acoustic Guitar (steel)",
    "Electric Guitar (jazz)",
    "Electric Guitar (clean)",
    "Electric Guitar (muted)",
    "Overdriven Guitar",
    "Distortion Guitar",
    "Guitar Harmonics",
    "Acoustic Bass",
    "Electric Bass (finger)",
    "Electric Bass (pick)",
    "Fretless Bass",
    "Slap Bass 1",
    "Slap Bass 2",
    "Synth Bass 1",
    "Synth Bass 2",
    "Violin",
    "Viola",
    "Cello",
    "Contrabass",
    "Tremolo Strings",
    "Pizzicato Strings",
    "Orchestral Harp",
    "Timpani",
    "String Ensemble 1",
    "String Ensemble 2",
    "Synth Strings 1",
    "Synth Strings 2",
    "Choir Aahs",
    "Voice Oohs",
    "Synth Voice",
    "Orchestra Hit",
    "Trumpet",
    "Trombone",
    "Tuba",
    "Muted Trumpet",
    "French Horn",
    "Brass Section",
    "Synth Brass 1",
    "Synth Brass 2",
    "Soprano Sax",
    "Alto Sax",
    "Tenor Sax",
    "Baritone Sax",
    "Oboe",
    "English Horn",
    "Bassoon",
    "Clarinet",
    "Piccolo",
    "Flute",
    "Recorder",
    "Pan Flute",
    "Blown Bottle",
    "Shakuhachi",
    "Whistle",
    "Ocarina",
    "Lead 1 (square)",
    "Lead 2 (sawtooth)",
    "Lead 3 (calliope)",
    "Lead 4 (chiff)",
    "Lead 5 (charang)",
    "Lead 6 (voice)",
    "Lead 7 (fifths)",
    "Lead 8 (bass + lead)",
    "Pad 1 (new age)",
    "Pad 2 (warm)",
    "Pad 3 (polysynth)",
    "Pad 4 (choir)",
    "Pad 5 (bowed)",
    "Pad 6 (metallic)",
    "Pad 7 (halo)",
    "Pad 8 (sweep)",
    "FX 1 (rain)",
    "FX 2 (soundtrack)",
    "FX 3 (crystal)",
    "FX 4 (atmosphere)",
    "FX 5 (brightness)",
    "FX 6 (goblins)",
    "FX 7 (echoes)",
    "FX 8 (sci-fi)",
    "Sitar",
    "Banjo",
    "Shamisen",
    "Koto",
    "Kalimba",
    "Bagpipe",
    "Fiddle",
    "Shanai",
    "Tinkle Bell",
    "Agogo",
    "Steel Drums",
    "Woodblock",
    "Taiko Drum",
    "Melodic Tom",
    "Synth Drum",
    "Reverse Cymbal",
    "Guitar Fret Noise",
    "Breath Noise",
    "Seashore",
    "Bird Tweet",
    "Telephone Ring",
    "Helicopter",
    "Applause",
    "Gunshot",
];

/// The channel General MIDI keeps for drums (the tenth).
pub const DRUMS: u8 = 9;

/// A part of a MIDI file: the notes one channel of one of its tracks plays, at `PPQ`.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: String,
    pub channel: u8,
    /// Its first program change, if it has one.
    pub program: Option<u8>,
    pub notes: Vec<Note>,
    pub controls: Vec<ControlPoint>,
}

impl Part {
    pub fn is_drums(&self) -> bool {
        self.channel == DRUMS
    }

    /// The tick its last note ends at.
    pub fn end(&self) -> u64 {
        self.notes
            .iter()
            .map(|n| n.start.max(0) as u64 + n.length)
            .max()
            .unwrap_or(0)
    }
}

/// A MIDI file, read: its parts, and what it says of the song (tempo, its changes, meter, key).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MidiFile {
    pub parts: Vec<Part>,
    /// Quarter notes a minute at its start, and later changes (tick, bpm).
    pub tempo: Option<f64>,
    pub tempos: Vec<(u64, f64)>,
    pub time_signature: Option<[u64; 2]>,
    pub key: Option<Key>,
    /// Its name, from its first track's name if it has one.
    pub name: Option<String>,
}

impl MidiFile {
    /// Where its last note ends.
    pub fn end(&self) -> u64 {
        self.parts.iter().map(Part::end).max().unwrap_or(0)
    }

    /// Its length in whole bars (of its meter, 4/4 if it doesn't say).
    pub fn bars(&self) -> u64 {
        let [n, d] = self.time_signature.unwrap_or([4, 4]);
        let bar = PPQ * 4 * n / d.max(1);
        self.end().div_ceil(bar.max(1)).max(1)
    }
}

/// A number as MIDI writes lengths.
fn put_vlq(mut v: u64, out: &mut Vec<u8>) {
    let mut bytes = vec![(v & 0x7f) as u8];
    v >>= 7;
    while v > 0 {
        bytes.push((v & 0x7f) as u8 | 0x80);
        v >>= 7;
    }
    out.extend(bytes.iter().rev());
}

/// Timed events as a track.
pub(crate) fn chunk(mut events: Vec<(u64, Vec<u8>)>) -> Vec<u8> {
    events.sort_by_key(|e| e.0);
    let mut body = Vec::new();
    let mut now = 0u64;
    for (at, bytes) in events {
        put_vlq(at - now, &mut body);
        body.extend(bytes);
        now = at;
    }
    body.extend([0x00, 0xff, 0x2f, 0x00]);
    let mut out = b"MTrk".to_vec();
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
    out
}

impl MidiFile {
    /// As a standard MIDI file (type 1): its tempo, meter and key, then a track a part.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut conductor = Vec::new();
        if let Some(bpm) = self.tempo {
            let micros = (60_000_000.0 / bpm.max(1.0)).round() as u32;
            let mut e = vec![0xff, 0x51, 0x03];
            e.extend(&micros.to_be_bytes()[1..]);
            conductor.push((0, e));
        }
        if let Some([beats, unit]) = self.time_signature {
            let unit = (unit.max(1) as f32).log2() as u8;
            conductor.push((0, vec![0xff, 0x58, 0x04, beats as u8, unit, 24, 8]));
        }
        if let Some(k) = self.key {
            // Fifths from C: its own major's, or its relative major's.
            let major = if k.minor { (k.root + 3) % 12 } else { k.root } as i32;
            let sharps = (major * 7 + 5).rem_euclid(12) - 5;
            conductor.push((0, vec![0xff, 0x59, 0x02, sharps as i8 as u8, k.minor as u8]));
        }
        let mut tracks = vec![chunk(conductor)];
        for part in &self.parts {
            let mut events: Vec<(u64, Vec<u8>)> = Vec::new();
            let name = part.name.as_bytes();
            let mut meta = vec![0xff, 0x03, name.len().min(127) as u8];
            meta.extend(&name[..name.len().min(127)]);
            events.push((0, meta));
            if let Some(program) = part.program {
                events.push((0, vec![0xc0 | part.channel, program & 0x7f]));
            }
            // A note's end goes before the next's start on the same tick.
            let mut offs = Vec::new();
            for n in &part.notes {
                let on = n.start.max(0) as u64;
                events.push((
                    on,
                    vec![0x90 | part.channel, n.pitch, n.velocity.clamp(1, 127)],
                ));
                offs.push((on + n.length.max(1), vec![0x80 | part.channel, n.pitch, 0]));
            }
            offs.append(&mut events);
            tracks.push(chunk(offs));
        }
        let mut out = b"MThd".to_vec();
        out.extend(6u32.to_be_bytes());
        out.extend(1u16.to_be_bytes());
        out.extend((tracks.len() as u16).to_be_bytes());
        out.extend((PPQ as u16).to_be_bytes());
        tracks.iter().for_each(|t| out.extend(t));
        out
    }
}

/// Reads a MIDI file from disk.
pub fn open(path: &Path) -> Result<MidiFile, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("couldn't read {}: {e}", path.display()))?;
    read(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Whether a file is a MIDI file, by its name.
pub fn is_midi(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_lowercase().as_str(), "mid" | "midi" | "smf"))
}

/// A variable-length number: seven bits a byte, the top bit set on all but the last.
fn vlq(b: &[u8], at: &mut usize) -> Result<u64, String> {
    let mut v = 0u64;
    for _ in 0..4 {
        let byte = *b
            .get(*at)
            .ok_or("the file ends in the middle of an event")?;
        *at += 1;
        v = (v << 7) | (byte & 0x7f) as u64;
        if byte & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err("a length longer than MIDI allows".into())
}

/// Reads a Standard MIDI File (format 0, 1 or 2): every part (a channel of a track) with its
/// notes (a note-on of velocity 0 ending one), controllers, pitch bend and pressure, timed at
/// `PPQ`.
pub fn read(b: &[u8]) -> Result<MidiFile, String> {
    if b.len() < 14 || &b[0..4] != b"MThd" {
        return Err("not a MIDI file".into());
    }
    let header = u32::from_be_bytes([b[4], b[5], b[6], b[7]]) as usize;
    let tracks = u16::from_be_bytes([b[10], b[11]]) as usize;
    let division = u16::from_be_bytes([b[12], b[13]]);
    if division & 0x8000 != 0 {
        return Err("timed in SMPTE frames, not beats: not supported".into());
    }
    let division = division.max(1) as u64;
    let to_ppq = |t: u64| t * PPQ / division;
    let mut at = 8 + header;
    let mut file = MidiFile::default();
    // (track, channel) -> part, as they're found.
    let mut parts: Vec<(usize, Part)> = Vec::new();
    let mut names: Vec<Option<String>> = Vec::new();
    for track in 0..tracks {
        if at + 8 > b.len() {
            break;
        }
        let len = u32::from_be_bytes([b[at + 4], b[at + 5], b[at + 6], b[at + 7]]) as usize;
        if &b[at..at + 4] != b"MTrk" {
            at += 8 + len;
            continue;
        }
        let data = &b[at + 8..(at + 8 + len).min(b.len())];
        at += 8 + len;
        let mut i = 0;
        let mut tick = 0u64;
        let mut status = 0u8;
        let mut name = None;
        // Notes sounding, by channel and pitch: when they started and how hard.
        let mut held: Vec<(u8, u8, u64, u8)> = Vec::new();
        let part_of = |channel: u8, parts: &mut Vec<(usize, Part)>| -> usize {
            match parts
                .iter()
                .position(|(t, p)| *t == track && p.channel == channel)
            {
                Some(k) => k,
                None => {
                    parts.push((
                        track,
                        Part {
                            name: String::new(),
                            channel,
                            program: None,
                            notes: Vec::new(),
                            controls: Vec::new(),
                        },
                    ));
                    parts.len() - 1
                }
            }
        };
        while i < data.len() {
            tick += vlq(data, &mut i)?;
            let mut byte = *data.get(i).ok_or("an event with nothing in it")?;
            if byte & 0x80 != 0 {
                i += 1;
                if byte < 0xf0 {
                    status = byte;
                }
            } else {
                // Running status: the last one, again.
                byte = status;
            }
            match byte {
                0xff => {
                    let kind = *data.get(i).ok_or("a meta event cut short")?;
                    i += 1;
                    let len = vlq(data, &mut i)? as usize;
                    let body = data.get(i..i + len).ok_or("a meta event cut short")?;
                    i += len;
                    match kind {
                        0x03 if name.is_none() => {
                            name = Some(String::from_utf8_lossy(body).trim().to_string())
                        }
                        0x51 if len == 3 => {
                            let us =
                                ((body[0] as u64) << 16) | ((body[1] as u64) << 8) | body[2] as u64;
                            let bpm = (60_000_000.0 / us.max(1) as f64 * 1000.0).round() / 1000.0;
                            // 120 until a file says otherwise, as MIDI has it.
                            match (file.tempo, to_ppq(tick)) {
                                (None, 0) => file.tempo = Some(bpm),
                                (None, t) => {
                                    file.tempo = Some(120.0);
                                    file.tempos.push((t, bpm));
                                }
                                (Some(_), t) => file.tempos.push((t, bpm)),
                            }
                        }
                        0x58 if len >= 2 && file.time_signature.is_none() => {
                            file.time_signature =
                                Some([body[0].max(1) as u64, 1u64 << body[1].min(6)])
                        }
                        0x59 if len == 2 && file.key.is_none() => {
                            let sharps = body[0] as i8 as i32;
                            let major = (sharps * 7).rem_euclid(12) as u8;
                            file.key = Some(match body[1] {
                                1 => Key {
                                    root: (major + 9) % 12,
                                    minor: true,
                                },
                                _ => Key {
                                    root: major,
                                    minor: false,
                                },
                            });
                        }
                        _ => {}
                    }
                }
                0xf0 | 0xf7 => {
                    let len = vlq(data, &mut i)? as usize;
                    i += len;
                }
                s if s >= 0x80 => {
                    let channel = s & 0x0f;
                    let size = match s & 0xf0 {
                        0xc0 | 0xd0 => 1,
                        _ => 2,
                    };
                    let d = data.get(i..i + size).ok_or("a channel event cut short")?;
                    i += size;
                    let t = to_ppq(tick);
                    let k = part_of(channel, &mut parts);
                    let part = &mut parts[k].1;
                    match s & 0xf0 {
                        0x90 if d[1] > 0 => held.push((channel, d[0] & 0x7f, t, d[1].min(127))),
                        0x80 | 0x90 => {
                            if let Some(h) = held
                                .iter()
                                .position(|h| h.0 == channel && h.1 == d[0] & 0x7f)
                            {
                                let (_, pitch, start, velocity) = held.remove(h);
                                part.notes.push(Note {
                                    start: start as i64,
                                    length: t.saturating_sub(start).max(1),
                                    pitch,
                                    velocity,
                                    ..Default::default()
                                });
                            }
                        }
                        0xa0 => part.controls.push(ControlPoint {
                            start: t as i64,
                            kind: ControlValue::PolyPressure {
                                pitch: d[0] & 0x7f,
                                value: d[1] & 0x7f,
                            },
                        }),
                        0xb0 => part.controls.push(ControlPoint {
                            start: t as i64,
                            kind: ControlValue::Cc {
                                number: d[0] & 0x7f,
                                value: d[1] & 0x7f,
                            },
                        }),
                        0xc0 => {
                            if part.program.is_none() {
                                part.program = Some(d[0] & 0x7f);
                            }
                        }
                        0xd0 => part.controls.push(ControlPoint {
                            start: t as i64,
                            kind: ControlValue::Aftertouch { value: d[0] & 0x7f },
                        }),
                        _ => {
                            let v = (((d[1] & 0x7f) as i16) << 7 | (d[0] & 0x7f) as i16) - 8192;
                            part.controls.push(ControlPoint {
                                start: t as i64,
                                kind: ControlValue::PitchBend { value: v },
                            });
                        }
                    }
                }
                _ => return Err("an event Pina Studio can't read".into()),
            }
        }
        // Notes never let go end where the track does.
        let end = to_ppq(tick);
        for (channel, pitch, start, velocity) in held {
            if let Some((_, p)) = parts
                .iter_mut()
                .find(|(t, p)| *t == track && p.channel == channel)
            {
                p.notes.push(Note {
                    start: start as i64,
                    length: end.saturating_sub(start).max(PPQ / 4),
                    pitch,
                    velocity,
                    ..Default::default()
                });
            }
        }
        names.push(name);
    }
    file.name = names.iter().flatten().find(|n| !n.is_empty()).cloned();
    let keep: Vec<(usize, Part)> = parts
        .into_iter()
        .filter(|(_, p)| !p.notes.is_empty())
        .collect();
    let several = |track: usize| keep.iter().filter(|(t, _)| *t == track).count() > 1;
    let mut out: Vec<Part> = Vec::new();
    for (track, mut p) in keep.iter().cloned() {
        p.notes.sort_by_key(|n| (n.start, n.pitch));
        p.controls.sort_by_key(|c| c.start);
        let own = names
            .get(track)
            .cloned()
            .flatten()
            .filter(|n| !n.is_empty());
        let sound = match (p.is_drums(), p.program) {
            (true, _) => "Drums".to_string(),
            (false, Some(prog)) => GM[prog as usize].to_string(),
            (false, None) => format!("Channel {}", p.channel + 1),
        };
        p.name = match own {
            Some(n) if !several(track) => n,
            Some(n) => format!("{n} · {sound}"),
            None => sound,
        };
        out.push(p);
    }
    file.parts = out;
    if file.parts.is_empty() {
        return Err("it has no notes".into());
    }
    Ok(file)
}

/// What a part plays on: drums on the built-in kit (General MIDI's drum notes), the rest on
/// Apple's General MIDI synth set to its program (piano if it names none); elsewhere the
/// built-in synth.
pub fn instrument_for(part: &Part) -> Instrument {
    if part.is_drums() {
        return Instrument::Drums;
    }
    if !cfg!(target_os = "macos") {
        return Instrument::Synth;
    }
    let program = part.program.unwrap_or(0);
    Instrument::Plugin(PluginRef {
        format: Format::Au,
        class: "DLSMusicDevice".into(),
        uid: Some("aumu dls  appl".into()),
        preset: Some(Preset {
            name: GM[program as usize].into(),
            source: PresetSource::Program {
                list: 0,
                program: program as u32,
            },
        }),
        ..Default::default()
    })
}

/// A MIDI file heard: played at `tempo`, moved `transpose` semitones (drums never), on the
/// instruments bringing it into a song would give it, over its whole bars; stereo at `rate`.
/// For hearing a file before it's in a song.
pub fn render(
    file: &MidiFile,
    tempo: f64,
    transpose: i32,
    rate: f64,
    dir: &Path,
) -> Result<[Vec<f32>; 2], String> {
    render_all(&[(file, transpose)], tempo, rate, dir)
}

/// MIDI files heard together, each moved as it says, the shorter going round until the longest
/// ends: as `render` for one.
pub fn render_all(
    files: &[(&MidiFile, i32)],
    tempo: f64,
    rate: f64,
    dir: &Path,
) -> Result<[Vec<f32>; 2], String> {
    let mut p = Project::new(rate as u32);
    let mut made = Vec::new();
    for (file, transpose) in files {
        made.extend(p.import_midi(file, 0, None, *transpose));
    }
    // The song's tempo, not the file's: it's heard as it would play in the song.
    (p.tempo, p.tempos) = (tempo.clamp(20.0, 999.0), Vec::new());
    let length = match p.loop_together(&made) {
        0 => p.bar(),
        length => length,
    };
    let frames = p.samples(length, rate);
    let (mut engine, _plugins) = crate::audio::engine::Engine::load(&p, dir, rate)?;
    Ok(engine.render_from(0, frames))
}

impl Project {
    /// `tracks` regions go round until the one that ends last does: how long that is.
    pub fn loop_together(&mut self, tracks: &[usize]) -> u64 {
        let of = |p: &Project| {
            let regions = tracks.iter().flat_map(|&t| &p.tracks[t].regions);
            let spans = regions.map(|r| (r.start, r.start + r.length));
            spans.fold((u64::MAX, 0), |(a, b), (s, e)| (a.min(s), b.max(e)))
        };
        let (start, end) = of(self);
        for &t in tracks {
            for r in &mut self.tracks[t].regions {
                if r.start + r.length < end {
                    r.looped = true;
                    r.loop_length = Some(end - r.start);
                }
            }
        }
        end.saturating_sub(start)
    }

    /// A MIDI file brought in at `at` (ticks): a track a part (from `index` on, the end if not
    /// given), each with a region of its notes, controllers and bends, on an instrument that
    /// plays it; `transpose` semitones (drums never). A song with nothing in it yet takes the
    /// file's tempo, its changes and meter, and its key if it has none. Answers the tracks made.
    pub fn import_midi(
        &mut self,
        file: &MidiFile,
        at: u64,
        index: Option<usize>,
        transpose: i32,
    ) -> Vec<usize> {
        let empty = self.tracks.iter().all(|t| t.regions.is_empty());
        if empty {
            if let Some(bpm) = file.tempo {
                self.tempo = bpm.clamp(20.0, 999.0);
            }
            self.tempos.clear();
            for &(tick, bpm) in &file.tempos {
                self.set_tempo_at(at + tick, bpm.clamp(20.0, 999.0));
            }
            if let Some(ts) = file.time_signature {
                self.time_signature = ts;
            }
            // The key its notes end up in; one already chosen stays.
            if let Some(k) = file.key.filter(|_| self.key.is_none()) {
                let root = (k.root as i32 + transpose).rem_euclid(12) as u8;
                self.key = Some(Key { root, ..k }.name());
            }
        }
        let bar = self.bar().max(1);
        let length = file.end().div_ceil(bar).max(1) * bar;
        let first = index.unwrap_or(self.tracks.len()).min(self.tracks.len());
        let mut made = Vec::new();
        for (k, part) in file.parts.iter().enumerate() {
            let index = first + k;
            let shift = if part.is_drums() { 0 } else { transpose };
            let notes = part
                .notes
                .iter()
                .map(|n| Note {
                    pitch: (n.pitch as i32 + shift).clamp(0, 127) as u8,
                    ..*n
                })
                .collect();
            self.add_track(
                index,
                Track {
                    name: part.name.clone(),
                    instrument: Some(instrument_for(part)),
                    regions: vec![Region {
                        start: at,
                        length,
                        name: part.name.clone(),
                        content: Content::Midi {
                            notes,
                            controls: part.controls.clone(),
                        },
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            );
            made.push(index);
        }
        made
    }
}

#[cfg(test)]
mod tests;
