//! Parts written into a song from its chords: what `set_chords`, `write_chords`, `write_bass`,
//! `write_melody` and `write_drums` do, with `theory` doing the music.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    music::Key,
    project::{ChordSpan, Content, Instrument, Note, PatternRow, Project, Region},
    theory::{self, BassStyle, Chord, ChordRhythm, MelodyStyle, Span, Voicing},
};

/// The song's key, which numerals and melodies need.
pub fn key(p: &Project) -> Result<Key, String> {
    p.song_key()
        .ok_or_else(|| "the song has no key yet: set_key first (\"A minor\")".into())
}

/// Every note a track plays, at its tick in the song: its regions' (looped ones repeated to
/// where they stop), after their playback settings. Take folders aside.
pub fn heard_notes(p: &Project, t: usize) -> Vec<Note> {
    let mut out = Vec::new();
    for (r, region) in p.tracks[t].regions.iter().enumerate() {
        let Some(notes) = region.content.notes(region.length) else {
            continue;
        };
        let stop = if region.looped {
            p.loop_end((t, r))
        } else {
            region.start + region.length
        };
        let mut at = region.start;
        while at < stop {
            for n in notes.iter().map(|n| region.played(n)) {
                let on = at as i64 + n.start;
                if on >= at as i64 && on < (at + region.length).min(stop) as i64 {
                    out.push(Note { start: on, ..n });
                }
            }
            at += region.length.max(1);
        }
    }
    out.sort_by_key(|n| (n.start, n.pitch));
    out
}

/// The chord track's chords from `start` to `end`, cut to fit.
pub fn chord_track(p: &Project, start: u64, end: u64) -> Vec<Span> {
    p.chords
        .iter()
        .filter(|c| c.start < end && c.start + c.length > start)
        .filter_map(|c| {
            Some(Span {
                start: c.start.max(start),
                end: (c.start + c.length).min(end),
                chord: Chord::parse(&c.chord)?,
            })
        })
        .collect()
}

/// The chords the song's pitched parts play, bar by bar, for a song with no chord track there.
pub fn heard_chords(p: &Project, start: u64, end: u64) -> Vec<Span> {
    let bar = p.bar();
    let notes: Vec<Note> = (0..p.tracks.len())
        .filter(|&t| p.tracks[t].instrument.as_ref().is_some_and(|i| !i.is_kit()))
        .flat_map(|t| heard_notes(p, t))
        .collect();
    let mut out: Vec<Span> = Vec::new();
    let mut b = start;
    while b < end {
        let e = (b + bar).min(end);
        let mut weights = [0f32; 12];
        let mut lowest: Option<u8> = None;
        for n in &notes {
            let (ns, ne) = (n.start.max(0) as u64, n.start.max(0) as u64 + n.length);
            let held = ne.min(e).saturating_sub(ns.max(b));
            if held > 0 {
                weights[(n.pitch % 12) as usize] += held as f32 / bar as f32;
                lowest = Some(lowest.map_or(n.pitch, |l| l.min(n.pitch)));
            }
        }
        let pcs: Vec<(u8, f32)> = (0..12u8)
            .filter(|&p| weights[p as usize] > 0.05)
            .map(|p| (p, weights[p as usize]))
            .collect();
        if let Some(chord) = theory::name_notes(&pcs, lowest.map(|l| l % 12)) {
            match out.last_mut() {
                Some(last) if last.chord == chord && last.end == b => last.end = e,
                _ => out.push(Span {
                    start: b,
                    end: e,
                    chord,
                }),
            }
        }
        b = e;
    }
    out
}

/// The chords to write over: the chord track's, or else what the parts play.
pub fn chords_at(p: &Project, start: u64, end: u64) -> Vec<Span> {
    let set = chord_track(p, start, end);
    if set.is_empty() {
        heard_chords(p, start, end)
    } else {
        set
    }
}

/// Where a part goes: from `start` (0 by default) for `bars` bars, or else to the end of the
/// chord track, or else 4 bars.
fn range(p: &Project, start: Option<u64>, bars: Option<u64>) -> (u64, u64) {
    let start = start.unwrap_or(0);
    let bar = p.bar();
    let bars = bars.filter(|&b| b > 0).unwrap_or_else(|| {
        let last = p
            .chords
            .iter()
            .map(|c| c.start + c.length)
            .max()
            .unwrap_or(0);
        if last > start {
            (last - start).div_ceil(bar)
        } else {
            4
        }
    });
    (start, start + bars.min(1024) * bar)
}

/// The song's chords as "bar: chord" lines, for an agent to read back.
fn listed(p: &Project, spans: &[Span], key: Option<&Key>) -> Vec<String> {
    let bar = p.bar();
    let beat = p.beat();
    spans
        .iter()
        .map(|s| {
            let at = s.start / bar + 1;
            let beat_in = (s.start % bar) / beat + 1;
            let name = s.chord.name_in(key);
            match beat_in {
                1 => format!("bar {at}: {name}"),
                b => format!("bar {at} beat {b}: {name}"),
            }
        })
        .collect()
}

/// Sets the song's chords from `start` for `bars` bars (one pass of `text` by default),
/// replacing what was there; with no `text`, clears them.
pub fn set_chords(
    p: &mut Project,
    text: Option<&str>,
    start: Option<u64>,
    bars: Option<u64>,
) -> Result<Value, String> {
    let bar = p.bar();
    let start = start.unwrap_or(0);
    let Some(text) = text else {
        let end = bars.map_or(u64::MAX, |b| start + b * bar);
        p.chords
            .retain(|c| c.start + c.length <= start || c.start >= end);
        return Ok(json!({ "chords": [] }));
    };
    let key = p.song_key();
    let k = key.unwrap_or(Key {
        root: 0,
        minor: false,
    });
    let groups = text.split('|').filter(|g| !g.trim().is_empty()).count() as u64;
    let bars = bars.filter(|&b| b > 0).unwrap_or(groups.max(1));
    if key.is_none()
        && text
            .split_whitespace()
            .any(|c| c != "|" && Chord::parse(c).is_none())
    {
        return Err(
            "numerals need the song's key: set_key first, or write chords as symbols (\"Am9\")"
                .into(),
        );
    }
    let spans = theory::lay_out(text, &k, start, bars.min(1024), bar)?;
    let end = start + bars * bar;
    // What was there is cut around the new chords.
    let mut kept: Vec<ChordSpan> = Vec::new();
    for c in p.chords.drain(..) {
        let ce = c.start + c.length;
        if ce <= start || c.start >= end {
            kept.push(c);
            continue;
        }
        if c.start < start {
            kept.push(ChordSpan {
                length: start - c.start,
                ..c.clone()
            });
        }
        if ce > end {
            kept.push(ChordSpan {
                start: end,
                length: ce - end,
                ..c
            });
        }
    }
    kept.extend(spans.iter().map(|s| ChordSpan {
        start: s.start,
        length: s.end - s.start,
        chord: s.chord.name_in(key.as_ref()),
    }));
    kept.sort_by_key(|c| c.start);
    p.chords = kept;
    Ok(json!({ "chords": listed(p, &spans, key.as_ref()) }))
}

/// A pitched part's track: an instrument that plays notes as pitches.
fn pitched(p: &Project, t: usize, what: &str) -> Result<(), String> {
    match &p.tracks.get(t).ok_or(format!("no track {t}"))?.instrument {
        None => Err(format!(
            "track {t} is an audio track: {what} goes on an instrument track"
        )),
        Some(i) if i.is_kit() => Err(format!(
            "track {t} plays a drum kit: {what} goes on a track with a synth, sampler or instrument plugin"
        )),
        Some(_) => Ok(()),
    }
}

/// Puts a part on a track as a new region from `start` to `end`, after taking away the regions
/// wholly inside that stretch (what it replaces). Answers its index and what it replaced.
fn place(p: &mut Project, t: usize, start: u64, end: u64, name: &str, content: Content) -> Value {
    let tr = &mut p.tracks[t];
    let before = tr.regions.len();
    tr.regions
        .retain(|r| !(r.start >= start && r.start + r.length <= end));
    let replaced = before - tr.regions.len();
    tr.regions.push(Region {
        name: name.into(),
        start,
        length: end - start,
        content,
        ..Default::default()
    });
    json!({ "region": [t, tr.regions.len() - 1], "replaced": replaced })
}

fn register(given: Option<[u8; 2]>, default: (u8, u8)) -> Result<(u8, u8), String> {
    match given {
        None => Ok(default),
        Some([lo, hi]) if lo < hi && hi <= 127 && hi - lo >= 12 => Ok((lo, hi)),
        Some(_) => Err("register is [lowest, highest] MIDI notes, at least an octave apart".into()),
    }
}

fn seed(p: &Project, t: usize, given: Option<u64>) -> u64 {
    given.unwrap_or_else(|| p.tracks[t].regions.len() as u64 * 7919 + t as u64)
}

/// The chords there, or an error that says how to give some.
fn need_chords(p: &Project, start: u64, end: u64) -> Result<Vec<Span>, String> {
    let spans = chords_at(p, start, end);
    if spans.is_empty() {
        return Err(
            "no chords there: set_chords first (\"Am9 | Fmaj9 | C | G\"), or give chords".into(),
        );
    }
    Ok(spans)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ChordPart {
    pub track: usize,
    pub start: Option<u64>,
    pub bars: Option<u64>,
    pub chords: Option<String>,
    pub voicing: Option<Voicing>,
    pub rhythm: Option<ChordRhythm>,
    pub register: Option<[u8; 2]>,
    pub seed: Option<u64>,
}

pub fn write_chords(p: &mut Project, w: ChordPart) -> Result<Value, String> {
    pitched(p, w.track, "chords")?;
    if let Some(text) = &w.chords {
        let groups = text.split('|').filter(|g| !g.trim().is_empty()).count() as u64;
        set_chords(p, Some(text), w.start, w.bars.or(Some(groups.max(1))))?;
    }
    let (start, end) = range(p, w.start, w.bars);
    let spans = need_chords(p, start, end)?;
    let voicing = w.voicing.unwrap_or_default();
    let reg = register(
        w.register,
        match voicing {
            Voicing::Spread => (52, 84),
            Voicing::Open => (55, 79),
            _ => (55, 76),
        },
    )?;
    let notes = theory::play_chords(
        &spans,
        voicing,
        w.rhythm.unwrap_or_default(),
        reg,
        p.beat(),
        start,
        seed(p, w.track, w.seed),
    );
    let count = notes.len();
    let key = p.song_key();
    let names = listed(p, &spans, key.as_ref());
    let mut out = place(
        p,
        w.track,
        start,
        end,
        "Chords",
        Content::Midi {
            notes,
            controls: Vec::new(),
        },
    );
    out["notes"] = json!(count);
    out["chords"] = json!(names);
    Ok(out)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BassPart {
    pub track: usize,
    pub start: Option<u64>,
    pub bars: Option<u64>,
    pub style: Option<BassStyle>,
    pub register: Option<[u8; 2]>,
    pub seed: Option<u64>,
    pub kick_track: Option<usize>,
}

/// Where the kick plays from `start` to `end`, as (ticks from `start`, velocity): on
/// `kick_track`, or else the first kit that has one.
fn kicks(
    p: &Project,
    kick_track: Option<usize>,
    start: u64,
    end: u64,
) -> Result<Vec<(u64, u8)>, String> {
    let tracks: Vec<usize> = match kick_track {
        Some(t) if t < p.tracks.len() => vec![t],
        Some(t) => return Err(format!("no track {t}")),
        None => (0..p.tracks.len())
            .filter(|&t| {
                p.tracks[t]
                    .instrument
                    .as_ref()
                    .is_some_and(Instrument::is_kit)
            })
            .collect(),
    };
    for t in tracks {
        let mut hits: Vec<(u64, u8)> = heard_notes(p, t)
            .into_iter()
            .filter(|n| {
                matches!(n.pitch, 35 | 36) && n.start >= start as i64 && n.start < end as i64
            })
            .map(|n| (n.start as u64 - start, n.velocity))
            .collect();
        hits.dedup_by_key(|h| h.0);
        if !hits.is_empty() {
            return Ok(hits);
        }
    }
    Err("no kick plays there to follow: write_drums first, or give kick_track".into())
}

pub fn write_bass(p: &mut Project, w: BassPart) -> Result<Value, String> {
    pitched(p, w.track, "a bassline")?;
    let (start, end) = range(p, w.start, w.bars);
    let spans = need_chords(p, start, end)?;
    let style = w.style.unwrap_or_default();
    let reg = register(
        w.register,
        match style {
            BassStyle::Kick => (26, 45),
            _ => (28, 52),
        },
    )?;
    let hits = match style {
        BassStyle::Kick => kicks(p, w.kick_track, start, end)?,
        _ => Vec::new(),
    };
    let key = key(p).unwrap_or(Key {
        root: spans[0].chord.root,
        minor: spans[0].chord.quality.minor(),
    });
    let notes = theory::bassline(
        &spans,
        style,
        reg,
        p.beat(),
        start,
        &hits,
        &key,
        seed(p, w.track, w.seed),
    );
    let count = notes.len();
    let mut out = place(
        p,
        w.track,
        start,
        end,
        "Bass",
        Content::Midi {
            notes,
            controls: Vec::new(),
        },
    );
    out["notes"] = json!(count);
    Ok(out)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MelodyPart {
    pub track: usize,
    pub start: Option<u64>,
    pub bars: Option<u64>,
    pub style: Option<MelodyStyle>,
    pub register: Option<[u8; 2]>,
    pub density: Option<f32>,
    pub seed: Option<u64>,
}

pub fn write_melody(p: &mut Project, w: MelodyPart) -> Result<Value, String> {
    pitched(p, w.track, "a melody")?;
    let key = key(p)?;
    let (start, end) = range(p, w.start, w.bars);
    let spans = need_chords(p, start, end)?;
    let style = w.style.unwrap_or_default();
    let reg = register(
        w.register,
        match style {
            MelodyStyle::Counter => (55, 74),
            MelodyStyle::Arp => (60, 84),
            _ => (62, 86),
        },
    )?;
    let density = w.density.unwrap_or(0.7);
    if !density.is_finite() {
        return Err("density is 0 to 1".into());
    }
    let bars = (end - start) / p.bar();
    let notes = theory::melody(
        &spans,
        &key,
        style,
        reg,
        density.clamp(0.0, 1.0),
        p.beat(),
        start,
        bars,
        seed(p, w.track, w.seed),
    );
    let count = notes.len();
    let name = match style {
        MelodyStyle::Counter => "Counter",
        MelodyStyle::Arp => "Arp",
        _ => "Melody",
    };
    let mut out = place(
        p,
        w.track,
        start,
        end,
        name,
        Content::Midi {
            notes,
            controls: Vec::new(),
        },
    );
    out["notes"] = json!(count);
    Ok(out)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DrumPart {
    pub track: usize,
    pub start: Option<u64>,
    pub bars: Option<u64>,
    pub style: Option<String>,
    pub fill_every: Option<u64>,
    pub seed: Option<u64>,
}

/// The notes a kit has a sound on; None for a plugin, which may play any.
fn sounds(i: &Instrument) -> Option<Vec<u8>> {
    match i {
        Instrument::Drums => Some(crate::drums::SOUNDS.iter().map(|s| s.0).collect()),
        Instrument::Sampler(s) => Some(
            s.zones
                .iter()
                .flat_map(|z| z.note..=z.high.unwrap_or(z.note))
                .collect(),
        ),
        _ => None,
    }
}

pub fn write_drums(p: &mut Project, w: DrumPart) -> Result<Value, String> {
    let tr = p
        .tracks
        .get(w.track)
        .ok_or(format!("no track {}", w.track))?;
    let playable = match &tr.instrument {
        None => {
            return Err(format!(
                "track {} is an audio track: drums go on a drum kit, a sampler kit or a drum plugin",
                w.track
            ));
        }
        Some(Instrument::Synth) => {
            return Err(format!(
                "track {} plays the sine synth: drums go on a drum kit, a sampler kit or a drum plugin",
                w.track
            ));
        }
        Some(i) => sounds(i),
    };
    let style = w
        .style
        .clone()
        .or(p.genre.clone())
        .unwrap_or_else(|| "boom_bap".into());
    let groove = theory::groove(&style);
    let (start, end) = range(p, w.start, w.bars);
    let bars = (end - start) / p.bar();
    let fill = w.fill_every.unwrap_or(if bars >= 8 { 8 } else { 0 });
    let (per_bar, rows) = theory::drums(groove, bars, fill, seed(p, w.track, w.seed));
    let mut skipped: Vec<String> = Vec::new();
    let rows: Vec<PatternRow> = rows
        .into_iter()
        .filter(|(pitch, cells)| {
            let keep = playable.as_ref().is_none_or(|ok| ok.contains(pitch));
            if !keep && cells.iter().any(|&c| c > 0) {
                skipped.push(format!(
                    "{} (note {pitch})",
                    crate::drums::name(*pitch).to_lowercase()
                ));
            }
            keep
        })
        .map(|(pitch, cells)| PatternRow {
            pitch,
            cells,
            mute: false,
            ..Default::default()
        })
        .collect();
    if rows.is_empty() {
        return Err(format!("track {} has no sound on any drum note", w.track));
    }
    let content = Content::Pattern {
        step: p.bar() / per_bar as u64,
        rows,
        swing: groove.swing,
    };
    let mut out = place(p, w.track, start, end, "Drums", content);
    out["groove"] = json!(groove.id);
    out["swing"] = json!(groove.swing);
    if fill > 0 {
        out["fills"] = json!(format!("every {fill} bars, a crash after"));
    }
    if !skipped.is_empty() {
        out["not_played"] = json!(format!(
            "this kit has nothing on: {} (samples_put a sound there to hear them)",
            skipped.join(", ")
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Track;

    fn song() -> Project {
        let mut p = Project::new(48_000);
        p.key = Some("A minor".into());
        for (name, instrument) in [
            ("Keys", Instrument::Synth),
            ("Bass", Instrument::Synth),
            ("Drums", Instrument::Drums),
            ("Lead", Instrument::Synth),
        ] {
            p.tracks.push(Track {
                name: name.into(),
                instrument: Some(instrument),
                ..Default::default()
            });
        }
        p
    }

    #[test]
    fn a_song_is_written_from_its_chords() {
        let mut p = song();
        let out = set_chords(&mut p, Some("i9 | VImaj9 | III | VII"), None, Some(8)).unwrap();
        assert_eq!(out["chords"][1], "bar 2: Fmaj9");
        assert_eq!(p.chords.len(), 8);
        // A second pass over bars 5 to 6 replaces only those.
        set_chords(&mut p, Some("Dm7 | E7"), Some(4 * 3840), None).unwrap();
        let names: Vec<&str> = p.chords.iter().map(|c| c.chord.as_str()).collect();
        assert_eq!(names, ["Am9", "Fmaj9", "C", "G", "Dm7", "E7", "C", "G"]);

        let out = write_chords(
            &mut p,
            ChordPart {
                track: 0,
                start: None,
                bars: None,
                chords: None,
                voicing: None,
                rhythm: Some(ChordRhythm::Push),
                register: None,
                seed: None,
            },
        )
        .unwrap();
        assert_eq!(out["region"], json!([0, 0]));
        assert_eq!(p.tracks[0].regions[0].length, 8 * 3840);
        let drums = write_drums(
            &mut p,
            DrumPart {
                track: 2,
                start: None,
                bars: None,
                style: Some("trap".into()),
                fill_every: None,
                seed: Some(1),
            },
        )
        .unwrap();
        assert_eq!(drums["groove"], "trap");
        assert!(drums["fills"].is_string());
        // Trap's open hat is on the kit; nothing's skipped.
        assert!(drums["not_played"].is_null(), "{drums}");
        let out = write_bass(
            &mut p,
            BassPart {
                track: 1,
                start: None,
                bars: None,
                style: Some(BassStyle::Kick),
                register: None,
                seed: None,
                kick_track: None,
            },
        )
        .unwrap();
        assert!(out["notes"].as_u64().unwrap() >= 16);
        // Each 808 note starts with a kick, on its chord's root.
        let kicks: Vec<i64> = heard_notes(&p, 2)
            .iter()
            .filter(|n| n.pitch == 36)
            .map(|n| n.start)
            .collect();
        for n in heard_notes(&p, 1) {
            assert!(kicks.contains(&n.start));
            let chord = chord_track(&p, n.start as u64, n.start as u64 + 1)[0].chord;
            assert_eq!(n.pitch % 12, chord.root);
        }
        write_melody(
            &mut p,
            MelodyPart {
                track: 3,
                start: None,
                bars: None,
                style: None,
                register: None,
                density: None,
                seed: Some(4),
            },
        )
        .unwrap();
        assert!(!heard_notes(&p, 3).is_empty());
        // Written again, a part replaces itself.
        let again = write_chords(
            &mut p,
            ChordPart {
                track: 0,
                start: None,
                bars: None,
                chords: None,
                voicing: Some(Voicing::Spread),
                rhythm: None,
                register: None,
                seed: None,
            },
        )
        .unwrap();
        assert_eq!(again["replaced"], 1);
        assert_eq!(p.tracks[0].regions.len(), 1);
        // Drums don't go on a synth, nor chords on a kit.
        assert!(
            write_drums(
                &mut p,
                DrumPart {
                    track: 0,
                    start: None,
                    bars: None,
                    style: None,
                    fill_every: None,
                    seed: None
                }
            )
            .is_err()
        );
        assert!(
            write_chords(
                &mut p,
                ChordPart {
                    track: 2,
                    start: None,
                    bars: None,
                    chords: None,
                    voicing: None,
                    rhythm: None,
                    register: None,
                    seed: None
                }
            )
            .is_err()
        );
    }

    #[test]
    fn parts_are_commands() {
        use crate::commands::Command;
        let c: Command =
            serde_json::from_str(r#"{"op": "write_drums", "track": 2, "style": "trap"}"#).unwrap();
        assert!(matches!(c, Command::WriteDrums(DrumPart { track: 2, .. })));
        let c: Command =
            serde_json::from_str(r#"{"op": "write_bass", "track": 1, "style": "kick"}"#).unwrap();
        assert!(matches!(
            c,
            Command::WriteBass(BassPart {
                style: Some(BassStyle::Kick),
                ..
            })
        ));
        assert!(
            serde_json::from_str::<Command>(r#"{"op": "write_chords", "track": 0, "colour": 1}"#)
                .is_err()
        );
    }

    #[test]
    fn chords_are_heard_from_the_notes_without_a_chord_track() {
        let mut p = song();
        let notes = [57, 60, 64, 67]
            .iter()
            .map(|&pitch| Note {
                start: 0,
                length: 3840,
                pitch,
                velocity: 90,
            })
            .chain([53, 57, 60, 64].iter().map(|&pitch| Note {
                start: 3840,
                length: 3840,
                pitch,
                velocity: 90,
            }))
            .collect();
        p.tracks[0].regions.push(Region {
            start: 0,
            length: 7680,
            content: Content::Midi {
                notes,
                controls: Vec::new(),
            },
            ..Default::default()
        });
        let heard: Vec<String> = heard_chords(&p, 0, 7680)
            .iter()
            .map(|s| s.chord.name())
            .collect();
        assert_eq!(heard, ["Am7", "Fmaj7"]);
        let out = write_bass(
            &mut p,
            BassPart {
                track: 1,
                start: None,
                bars: Some(2),
                style: None,
                register: None,
                seed: None,
                kick_track: None,
            },
        )
        .unwrap();
        assert_eq!(out["notes"], 2);
    }
}
