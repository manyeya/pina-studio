//! MIDI files for the command line and agents: the library's found (by role, key, tempo and
//! words, each with the semitones that move it into a song's key), one studied in full (its key,
//! tempo, chords and progression, each part's role and how it plays, and, asked for, its notes,
//! bar and step by bar and step), several heard together for what they share, and one written:
//! the agent's own notes, saved beside the song to hear, compare and drag in.

use std::path::{Path, PathBuf};

use pina_engine::{
    music::Key,
    samples::{
        library::Library,
        midi::{self, MidiQuery},
    },
    song::{
        midi_file::{self, DRUMS, MidiFile, Part},
        project::PPQ,
    },
};
use serde_json::{Value, json};

/// A MIDI file studied: the library's study if it has the file, else read now; with `notes`,
/// each part's notes as text ("1.1 G#4 2 96": bar.step, note, sixteenths, velocity), its first
/// sixteen bars.
pub fn study(path: &Path, notes: bool) -> Result<Value, String> {
    let file = midi_file::open(path)?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let study = midi::study(&file, &name);
    let mut out = serde_json::to_value(&study).map_err(|e| e.to_string())?;
    out["file"] = json!(path);
    out["parts_mean"] = json!(
        "each part: its role; range; polyphony (notes at once, on average and at most); density \
         (notes a bar); steps (how often each of a bar's sixteen steps starts a note, the most 1); \
         swing (50 straight, 75 every second sixteenth half a step late); syncopation (the share \
         off the eighths); velocity (softest, hardest, average); length (sixteenths) and legato \
         (the share held into the next); a melody's contour; drums' patterns (0-9 a step)"
    );
    if notes {
        let [n, d] = study.time_signature;
        let bar = PPQ * 4 * n / d.max(1);
        let limit = (bar * 16) as i64;
        for (i, part) in file.parts.iter().enumerate() {
            let mut first = part.clone();
            first.notes.retain(|x| x.start < limit);
            out["parts"][i]["notes_text"] = json!(midi::notes_text(&first, bar));
        }
        out["notes_mean"] = json!(
            "a note a line: bar.step (from 1, sixteenths), its name, how many sixteenths, velocity"
        );
    }
    Ok(out)
}

/// Where a song keeps the MIDI its agent writes.
pub const MIDI_DIR: &str = "MIDI";

fn opened(path: &Path) -> Result<(MidiFile, midi::MidiStudy), String> {
    let mut file = midi_file::open(path)?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let study = midi::study(&file, &name);
    midi::mark_drums(&mut file, &study);
    Ok((file, study))
}

fn group_of(paths: &[PathBuf]) -> Result<Vec<(MidiFile, midi::MidiStudy)>, String> {
    paths.iter().map(|p| opened(p)).collect()
}

/// Several MIDI files heard together: what they share, each one in a line, and, given a
/// `candidate`, how close that comes to them and what would bring it closer.
pub fn group(paths: &[PathBuf], candidate: Option<&Path>) -> Result<Value, String> {
    if paths.is_empty() {
        return Err("midi_group needs at least one file".into());
    }
    let files = group_of(paths)?;
    let refs: Vec<(&MidiFile, &midi::MidiStudy)> = files.iter().map(|(f, s)| (f, s)).collect();
    let shared = midi::shared(&refs).ok_or("nothing to compare")?;
    let mut out = json!({
        "shared": shared,
        "shared_means": "alike: how much the files agree (1 the same, under 0.6 a mixed bag); \
            steps: how often each of a bar's sixteen steps starts a note, the most 1; degrees: how \
            much each semitone above the key's root is used, from the root; density: notes a bar \
            (average, fewest, most); length: sixteenths; span: semitones low to high; polyphony: \
            notes at once",
        "files": paths.iter().zip(&files).map(|(p, (_, s))| json!({
            "file": p,
            "summary": s.summary,
            "key": s.key,
            "tempo": s.tempo,
            "bars": s.bars,
            "role": s.role,
            "progression": s.progression,
        })).collect::<Vec<_>>(),
    });
    if let Some(c) = candidate {
        let (file, study) = opened(c)?;
        out["likeness"] = json!(shared.likeness(&file, &study));
    }
    Ok(out)
}

/// A file name for `name`: nothing a folder can't hold.
fn slug(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| match c.is_alphanumeric() || " -_".contains(c) {
            true => c,
            false => ' ',
        })
        .collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    match clean.is_empty() {
        true => "MIDI".into(),
        false => clean,
    }
}

/// The agent's notes as a MIDI file beside the song, under a name of its own: `parts`, each with
/// its `notes` as `midi_study` reads them, at the song's tempo, key and meter unless given. It
/// doesn't touch the song: the person hears it, and drags it in or asks for it to be. Given
/// `like`, it says how close it came to those.
pub fn write(
    project: &Path,
    name: &str,
    tempo: Option<f64>,
    key: Option<&str>,
    parts: &[Value],
    like: &[PathBuf],
) -> Result<Value, String> {
    let p = crate::cli::project(project)?;
    let [n, d] = p.time_signature;
    let bar = PPQ * 4 * n / d.max(1);
    let mut made = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let text = part["notes"].as_str().ok_or("each part needs `notes`")?;
        let notes = midi::parse_notes(text, bar).map_err(|e| format!("part {}: {e}", i + 1))?;
        made.push(Part {
            name: part["name"].as_str().unwrap_or(name).to_string(),
            channel: match part["drums"].as_bool().unwrap_or(false) {
                true => DRUMS,
                false => (i as u8).min(15).min(DRUMS.saturating_sub(1)),
            },
            program: part["program"].as_u64().map(|p| p.min(127) as u8),
            notes,
            controls: Vec::new(),
        });
    }
    let notes: usize = made.iter().map(|p| p.notes.len()).sum();
    if notes == 0 {
        return Err("midi_write needs some notes".into());
    }
    let key = match key {
        Some(k) => Some(Key::parse(k).ok_or(format!("no key {k}"))?),
        None => p.song_key(),
    };
    let file = MidiFile {
        parts: made,
        tempo: Some(tempo.unwrap_or(p.tempo)),
        tempos: Vec::new(),
        time_signature: Some([n, d]),
        key,
        name: Some(name.to_string()),
    };
    let dir = project.join(MIDI_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stem = slug(name);
    let path = (1..)
        .map(|k| match k {
            1 => dir.join(format!("{stem}.mid")),
            k => dir.join(format!("{stem} {k}.mid")),
        })
        .find(|p| !p.exists())
        .ok_or("no name left")?;
    std::fs::write(&path, file.to_bytes()).map_err(|e| e.to_string())?;
    // It said which parts are drums: its name doesn't get a say ("Off Beat Stabs" aren't).
    let read = midi_file::open(&path)?;
    let study = midi::study(&read, "");
    let mut out = json!({
        "file": path,
        "notes": notes,
        "bars": study.bars,
        "key": study.key,
        "tempo": study.tempo,
        "summary": study.summary,
        "progression": study.progression,
        "said": "It's in the Studio Agent panel for them to hear and drag in. It isn't in the song: bring \
            it in with import_midi only if they ask.",
    });
    if !like.is_empty() {
        let files = group_of(like)?;
        let refs: Vec<(&MidiFile, &midi::MidiStudy)> = files.iter().map(|(f, s)| (f, s)).collect();
        if let Some(shared) = midi::shared(&refs) {
            out["likeness"] = json!(shared.likeness(&read, &study));
            out["likeness_means"] = json!(
                "score over 0.85 is clearly one of them, over 0.97 too near a copy; rewrite and \
                 write again, under a new name, if it's low: `advice` says what's off"
            );
        }
    }
    Ok(out)
}

/// The library's MIDI files that fit: by `words`, `role`, `key` and `tempo` (a project's own key
/// and tempo when it's given and they aren't), best first, each with what it is and the
/// semitones that move it into the key.
pub fn find(mut q: MidiQuery, project: Option<&Path>) -> Result<Value, String> {
    if let Some(dir) = project {
        let p = crate::cli::project(dir)?;
        if q.key.is_none() {
            q.key = p.song_key().map(|k| k.name());
        }
        if q.tempo.is_none() {
            q.tempo = Some(p.tempo as f32);
        }
    }
    let library = Library::load();
    if library.midi.is_empty() {
        return Ok(json!({
            "found": [],
            "note": "the library has no MIDI files yet: add a folder that has some (pina samples add), or scan it again (pina samples scan)",
        }));
    }
    let hits = midi::find(&library.midi, &q);
    let found: Vec<Value> = hits
        .iter()
        .filter_map(|h| {
            let m = library.midi.get(h.index)?;
            Some(json!({
                "file": m.path,
                "name": m.name,
                "folder": m.folder,
                "role": m.study.role,
                "key": m.study.key,
                "tempo": m.study.tempo,
                "bars": m.study.bars,
                "progression": m.study.progression,
                "summary": m.study.summary,
                "why": h.why,
                "transpose": h.transpose,
            }))
        })
        .collect();
    Ok(json!({ "found": found, "of": library.midi.len() }))
}
