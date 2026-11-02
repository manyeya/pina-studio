//! The song's chord track (`set_chords`), and what a song's notes and chords are where it plays.
//! The parts themselves are the agent's to write, note by note.

use serde_json::{Value, json};

use crate::{
    music::Key,
    music::theory::{self, Chord, Span},
    song::project::{ChordSpan, Note, Project},
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
        let stop = p.plays_to((t, r));
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

#[cfg(test)]
mod tests;
