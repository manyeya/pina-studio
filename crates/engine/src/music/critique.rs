//! What a producer would say listening to a song, for an agent that can't: wrong notes and
//! clashes, a bass off its chords, kick and bass flamming, robotic parts, a song that never
//! changes, and (from a render) clipping, loudness, tonal balance against its genre, masking,
//! parts too loud for their role, and stereo trouble. Each with where, and the command that fixes
//! it.

use serde::Serialize;

use crate::{
    music::Key,
    music::analysis::Analysis,
    music::arrange::role,
    music::compose::{chords_at, heard_notes},
    music::playbook::{self, Playbook},
    music::theory::{Quality, Span},
    song::project::{Content, Instrument, Note, Project},
};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Issue {
    /// "fix": it sounds wrong. "improve": it sounds amateur. "note": worth knowing.
    pub level: &'static str,
    /// "setup", "sound", "harmony", "rhythm", "arrangement", "mix" or "master".
    pub area: &'static str,
    pub what: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// What to do, with Pina Studio's commands.
    pub fix: String,
}

/// What a render measured: the whole mix, each track alone (by index), each marked section.
pub struct Heard<'a> {
    pub mix: &'a Analysis,
    pub tracks: &'a [Analysis],
    pub sections: &'a [(String, Analysis)],
}

/// Mix targets when the song has no genre: the middle of released songs.
const GENERIC: [f32; 6] = [-5.0, -4.5, -7.0, -13.5, -18.0, -21.0];
const BANDS: [&str; 6] = [
    "sub (under 60 Hz)",
    "bass (60-250 Hz)",
    "low mids (250 Hz-1 kHz)",
    "mids (1-4 kHz)",
    "presence (4-8 kHz)",
    "air (over 8 kHz)",
];

fn issue(
    level: &'static str,
    area: &'static str,
    what: String,
    at: Option<String>,
    fix: String,
) -> Issue {
    Issue {
        level,
        area,
        what,
        at,
        fix,
    }
}

/// "bars 3, 5, 9-12": bar numbers (from 1), runs joined, the first few.
fn bars(list: &[u64]) -> String {
    let mut b: Vec<u64> = list.to_vec();
    b.sort_unstable();
    b.dedup();
    let mut runs: Vec<String> = Vec::new();
    let mut i = 0;
    while i < b.len() && runs.len() < 6 {
        let mut j = i;
        while j + 1 < b.len() && b[j + 1] == b[j] + 1 {
            j += 1;
        }
        runs.push(if i == j {
            format!("{}", b[i] + 1)
        } else {
            format!("{}-{}", b[i] + 1, b[j] + 1)
        });
        i = j + 1;
    }
    let more = if i < b.len() { ", ..." } else { "" };
    format!(
        "{} {}{more}",
        if b.len() == 1 { "bar" } else { "bars" },
        runs.join(", ")
    )
}

fn track_name(p: &Project, t: usize) -> String {
    format!("track {t} ({})", p.tracks[t].name)
}

fn chord_at(spans: &[Span], tick: i64) -> Option<&Span> {
    spans
        .iter()
        .find(|s| s.start as i64 <= tick && tick < s.end as i64)
}

/// A note that fits where it plays: in the key, in the chord, or a colour of the chord (its 9th,
/// 11th or 13th within the key).
fn fits(key: Option<&Key>, chord: Option<&Span>, pitch: u8) -> bool {
    key.is_none_or(|k| k.contains(pitch)) || chord.is_some_and(|s| s.chord.has(pitch))
}

/// Everything a song's notes and layout say, without hearing it.
pub fn music(p: &Project) -> Vec<Issue> {
    let mut out = Vec::new();
    let bar = p.bar();
    let beat = p.beat();
    let found = p.genre.as_deref().and_then(playbook::find);
    let book = found.as_ref();
    let key = p.song_key();
    let end = p.end();
    let song_bars = end.div_ceil(bar);
    let roles: Vec<&str> = p.tracks.iter().map(role).collect();
    let notes: Vec<Vec<Note>> = (0..p.tracks.len())
        .map(|t| {
            if p.tracks[t].mute {
                Vec::new()
            } else {
                heard_notes(p, t)
            }
        })
        .collect();
    let pitched: Vec<usize> = (0..p.tracks.len())
        .filter(|&t| {
            p.tracks[t].instrument.as_ref().is_some_and(|i| !i.is_kit()) && !notes[t].is_empty()
        })
        .collect();
    let spans = chords_at(p, 0, end.max(bar));

    // Setup.
    match book {
        None => out.push(issue("note", "setup", "the song has no genre".into(), None, format!(
            "set_genre (one of {}) so its tempo, swing, parts, layout and mix are checked against the genre's",
            playbook::ids().join(", ")
        ))),
        Some(b) if p.tempo < b.tempo.0 - 3.0 || p.tempo > b.tempo.1 + 3.0 => out.push(issue("improve", "setup",
            format!("{} bpm is outside {}'s {}-{}", p.tempo, b.name, b.tempo.0, b.tempo.1), None,
            format!("set_tempo to {}", ((b.tempo.0 + b.tempo.1) / 2.0).round()))),
        _ => {}
    }
    if key.is_none() && !pitched.is_empty() {
        out.push(issue(
            "note",
            "setup",
            "no key set, and the notes don't say one clearly".into(),
            None,
            "set_key".into(),
        ));
    }
    if p.chords.is_empty() && !pitched.is_empty() {
        out.push(issue(
            "note",
            "setup",
            "no chord track: the chords were read from the notes".into(),
            None,
            "set_chords, so parts written later follow them and the harmony checks are exact"
                .into(),
        ));
    }

    // Sound.
    for (t, tr) in p.tracks.iter().enumerate() {
        if notes[t].is_empty() {
            continue;
        }
        match &tr.instrument {
            Some(Instrument::Synth) => out.push(issue("improve", "sound",
                "plays the built-in sine synth, a placeholder that sounds like a test tone".into(),
                Some(track_name(p, t)),
                format!("give it a real instrument: sounds role \"{}\" lists every instrument's presets that suit it, and samples", roles[t]))),
            Some(Instrument::Drums) => out.push(issue("improve", "sound",
                "plays the built-in synthesized kit".into(), Some(track_name(p, t)),
                "samples_kit for a real kit that goes together, then write_drums on it".into())),
            _ => {}
        }
    }

    // Harmony.
    for &t in &pitched {
        let wrong: Vec<u64> = notes[t]
            .iter()
            .filter(|n| !fits(key.as_ref(), chord_at(&spans, n.start), n.pitch))
            .map(|n| n.start.max(0) as u64 / bar)
            .collect();
        if wrong.len() > 2 && wrong.len() * 100 > notes[t].len() * 3 {
            out.push(issue("fix", "harmony",
                format!("{} notes outside the key ({}) and the chord", wrong.len(), key.map_or("?".into(), |k| k.name())),
                Some(format!("{}, {}", track_name(p, t), bars(&wrong))),
                "move them into the key (edit_notes transpose, or set_notes), or rewrite with write_chords / write_melody".into()));
        }
    }
    // Clashes: two parts a semitone apart at once.
    let mut clashes: Vec<(u64, usize, usize)> = Vec::new();
    for (i, &a) in pitched.iter().enumerate() {
        for &b in &pitched[i + 1..] {
            for x in notes[a].iter().filter(|n| n.length >= beat / 2) {
                for y in notes[b].iter().filter(|n| n.length >= beat / 2) {
                    let overlap = (x.start + x.length as i64).min(y.start + y.length as i64)
                        - x.start.max(y.start);
                    if overlap < (beat / 2) as i64 {
                        continue;
                    }
                    let d = (x.pitch as i32 - y.pitch as i32).unsigned_abs();
                    let b9 = chord_at(&spans, x.start.max(y.start))
                        .is_some_and(|s| s.chord.quality == Quality::Dom7b9);
                    if d == 1 || d == 13 && !b9 {
                        clashes.push((x.start.max(y.start) as u64 / bar, a, b));
                    }
                }
            }
        }
    }
    if clashes.len() > 2 {
        let (a, b) = (clashes[0].1, clashes[0].2);
        let where_: Vec<u64> = clashes.iter().map(|c| c.0).collect();
        out.push(issue("fix", "harmony",
            format!("{} times two parts play a semitone apart (a rub that sounds like a wrong note)", clashes.len()),
            Some(format!("{} against {}, {}", track_name(p, a), track_name(p, b), bars(&where_))),
            "move one part's register (write_chords register, or edit_notes transpose 12), or use a rootless voicing".into()));
    }
    // The bass on its chords.
    for &t in pitched.iter().filter(|&&t| roles[t] == "bass") {
        let mut off: Vec<u64> = Vec::new();
        for s in &spans {
            if let Some(n) = notes[t]
                .iter()
                .find(|n| (n.start - s.start as i64).abs() <= (beat / 2) as i64)
            {
                let root = s.chord.bass.unwrap_or(s.chord.root);
                if n.pitch % 12 != root && !s.chord.has(n.pitch) {
                    off.push(s.start / bar);
                }
            }
        }
        if off.len() > 1 {
            out.push(issue("fix", "harmony", format!("the bass plays off the chord {} times where chords change", off.len()),
                Some(format!("{}, {}", track_name(p, t), bars(&off))),
                format!("write_bass on track {t} (it plays each chord's root), or move those notes to the root")));
        }
    }

    // Rhythm.
    let kicks: Vec<i64> = (0..p.tracks.len())
        .filter(|&t| {
            p.tracks[t]
                .instrument
                .as_ref()
                .is_some_and(Instrument::is_kit)
        })
        .flat_map(|t| {
            notes[t]
                .iter()
                .filter(|n| matches!(n.pitch, 35 | 36))
                .map(|n| n.start)
                .collect::<Vec<_>>()
        })
        .collect();
    for &t in pitched.iter().filter(|&&t| roles[t] == "bass") {
        let flams: Vec<u64> = notes[t]
            .iter()
            .filter(|n| {
                kicks
                    .iter()
                    .any(|&k| (12..=150).contains(&(n.start - k).abs()))
            })
            .map(|n| n.start.max(0) as u64 / bar)
            .collect();
        if flams.len() > 2 {
            out.push(issue("fix", "rhythm", format!("{} bass notes land just off a kick: the two flam and the low end smears", flams.len()),
                Some(format!("{}, {}", track_name(p, t), bars(&flams))),
                format!("put them on the kick: write_bass style kick on track {t}, or edit_notes quantize")));
        }
    }
    for (t, list) in notes.iter().enumerate() {
        if list.len() < 16 {
            continue;
        }
        let mean = list.iter().map(|n| n.velocity as f32).sum::<f32>() / list.len() as f32;
        let spread = (list
            .iter()
            .map(|n| (n.velocity as f32 - mean).powi(2))
            .sum::<f32>()
            / list.len() as f32)
            .sqrt();
        let accents_written = p.tracks[t].regions.iter().any(|r| matches!(&r.content, Content::Pattern { rows, .. } if rows.iter().any(|row| row.cells.iter().any(|&c| c > 0 && c < 60))));
        if spread < 3.0 && !accents_written {
            out.push(issue("improve", "rhythm", format!("every note at velocity {mean:.0}: it sounds programmed"),
                Some(track_name(p, t)),
                "edit_notes humanize (velocity 10, timing 8), or accents and ghost notes in the pattern".into()));
        }
    }
    if let Some(b) = book.filter(|b| b.swing > 53.0) {
        let straight: Vec<usize> = (0..p.tracks.len())
            .filter(|&t| {
                p.tracks[t]
                    .regions
                    .iter()
                    .any(|r| matches!(&r.content, Content::Pattern { swing, .. } if *swing < 52.0))
            })
            .collect();
        if let Some(&t) = straight.first() {
            out.push(issue(
                "improve",
                "rhythm",
                format!("the drums are straight; {} swings ({})", b.name, b.swing),
                Some(track_name(p, t)),
                format!("set_pattern swing {} on its pattern regions", b.swing),
            ));
        }
    }
    for &t in pitched.iter().filter(|&&t| roles[t] == "lead") {
        let busy = notes[t].len() as f32 / song_bars.max(1) as f32;
        if busy > 12.0 {
            out.push(issue(
                "improve",
                "rhythm",
                format!("the lead plays {busy:.0} notes a bar: too busy to hum"),
                Some(track_name(p, t)),
                "write_melody with density 0.4, or thin it: a hook leaves space".into(),
            ));
        }
    }

    // Arrangement.
    let active = |t: usize, from: u64, to: u64| -> bool {
        notes[t]
            .iter()
            .any(|n| n.start >= from as i64 && n.start < to as i64)
            || p.tracks[t].instrument.is_none()
                && !p.tracks[t].mute
                && p.tracks[t]
                    .regions
                    .iter()
                    .any(|r| r.start < to && r.start + r.length > from)
    };
    let sounding: Vec<usize> = (0..p.tracks.len()).filter(|&t| active(t, 0, end)).collect();
    if song_bars < 24 && sounding.len() > 2 {
        out.push(issue(
            "note",
            "arrangement",
            format!("{song_bars} bars: a loop, not a song yet"),
            None,
            "when the loop works, arrange (sections from the genre's playbook, or your own)".into(),
        ));
    } else if song_bars >= 24 {
        if p.markers.is_empty() {
            out.push(issue(
                "improve",
                "arrangement",
                "no sections marked".into(),
                None,
                "add_marker at each section, or arrange".into(),
            ));
        }
        let blocks: Vec<Vec<usize>> = (0..song_bars.div_ceil(4))
            .map(|k| {
                (0..p.tracks.len())
                    .filter(|&t| active(t, k * 4 * bar, (k + 1) * 4 * bar))
                    .collect()
            })
            .collect();
        let full = blocks.iter().filter(|b| b.len() == sounding.len()).count();
        if full * 10 >= blocks.len() * 9 && sounding.len() > 1 {
            out.push(issue("fix", "arrangement",
                format!("all {} parts play in nearly every bar: nothing comes or goes, so nothing builds", sounding.len()),
                None, "arrange so the intro and verses drop parts and the chorus has them all; transitions between".into()));
        }
        if blocks.first().is_some_and(|b| b.len() == sounding.len()) && sounding.len() > 2 {
            out.push(issue(
                "improve",
                "arrangement",
                "the song starts with everything at once".into(),
                Some("bars 1-4".into()),
                "start with one or two parts (chords, or drums alone) and bring the rest in".into(),
            ));
        }
        // The hook hits harder than what's before it.
        for (i, m) in p.markers.iter().enumerate() {
            let lower = m.name.to_lowercase();
            if i == 0 || !["chorus", "hook", "drop"].iter().any(|w| lower.contains(w)) {
                continue;
            }
            let prev = &p.markers[i - 1];
            let next = p.markers.get(i + 1).map_or(end, |n| n.tick);
            let here = (0..p.tracks.len())
                .filter(|&t| active(t, m.tick, next))
                .count();
            let before = (0..p.tracks.len())
                .filter(|&t| active(t, prev.tick, m.tick))
                .count();
            if here < before {
                out.push(issue(
                    "improve",
                    "arrangement",
                    format!(
                        "{} has fewer parts ({here}) than {} before it ({before})",
                        m.name, prev.name
                    ),
                    Some(bars(&[m.tick / bar])),
                    format!(
                        "bring every part into {}, or take some out of {}",
                        m.name, prev.name
                    ),
                ));
            }
        }
        let fx = (0..p.tracks.len()).any(|t| roles[t] == "fx" && !p.tracks[t].regions.is_empty())
            || p.tracks
                .iter()
                .any(|t| t.regions.iter().any(|r| r.name == "Fill"));
        if p.markers.len() > 2 && !fx {
            out.push(issue(
                "note",
                "arrangement",
                "no transitions into the sections".into(),
                None,
                "transition kind build into each chorus or drop, fill or reverse into verses"
                    .into(),
            ));
        }
    }
    out
}

/// Where the mix's bands should sit, and how far off is too far.
fn targets(book: Option<&Playbook>) -> [f32; 6] {
    book.map_or(GENERIC, |b| b.spectrum)
}

fn spectrum(a: &Analysis) -> [f32; 6] {
    let s = &a.spectrum_db;
    [s.sub, s.bass, s.low_mid, s.mid, s.presence, s.air]
}

/// What a render of the song says.
pub fn mix(p: &Project, heard: &Heard) -> Vec<Issue> {
    let mut out = Vec::new();
    let found = p.genre.as_deref().and_then(playbook::find);
    let book = found.as_ref();
    let m = heard.mix;
    let roles: Vec<&str> = p.tracks.iter().map(role).collect();
    let peak = m.peak_db[0].max(m.peak_db[1]);
    let limited = p
        .master_inserts
        .iter()
        .any(|i| i.class.to_lowercase().contains("limit"));
    // Before a master, peaks over 0 are headroom the limiter takes back; after one, clipping.
    if (m.clipped > 0 || peak > -0.1) && limited {
        out.push(issue("fix", "master", format!("the master clips: peaks at {peak:.1} dBFS, {} samples over", m.clipped), None,
            "master again (it sets the limiter's gain and the ceiling), or set_master volume_db lower".into()));
    } else if peak > -0.1 {
        out.push(issue(
            "note",
            "master",
            format!("peaks at {peak:.1} dBFS: it would clip exported as it is"),
            None,
            "master it (a limiter catches the peaks), or set_master volume_db lower".into(),
        ));
    }
    if m.lufs < -60.0 {
        out.push(issue(
            "fix",
            "mix",
            "the song is silent".into(),
            None,
            "check the tracks have instruments and aren't muted".into(),
        ));
        return out;
    }
    let reference = p.reference.as_ref();
    let target = reference.map_or(book.map_or(-10.0, |b| b.lufs), |r| r.lufs);
    let against = match reference {
        Some(r) => format!(
            "the reference ({})",
            r.file
                .file_stem()
                .map_or("".into(), |s| s.to_string_lossy().to_string())
        ),
        None => format!("finished {} songs", book.map_or("", |b| b.name.as_str())),
    };
    if m.lufs < target - 3.0 {
        out.push(issue(
            if limited { "improve" } else { "note" },
            "master",
            format!(
                "{:.1} LUFS: quiet next to {against} (about {target})",
                m.lufs
            ),
            None,
            format!("master with lufs {target} once the mix is right"),
        ));
    } else if m.lufs > target + 2.0 {
        out.push(issue(
            "improve",
            "master",
            format!(
                "{:.1} LUFS: louder than {against} ({target}), squashing it",
                m.lufs
            ),
            None,
            format!("master with lufs {target}"),
        ));
    }
    // Unmastered, a mix's peaks stand well over its average; mastered, they shouldn't.
    if m.crest_db > if limited { 14.0 } else { 22.0 } {
        out.push(issue("improve", "mix", format!("peaks stand {:.0} dB over the average: hits poke out and the rest sounds small", m.crest_db), None,
            "compress the loudest parts (set_comp on drums, threshold -18, ratio 4, attack 10), or mix_role them".into()));
    } else if m.crest_db < 6.0 {
        out.push(issue(
            "improve",
            "mix",
            format!(
                "only {:.1} dB between peaks and average: over-compressed, lifeless",
                m.crest_db
            ),
            None,
            "ease off compression and the limiter".into(),
        ));
    }
    // Tonal balance.
    let want = reference.map_or(targets(book), |r| r.spectrum);
    let have = spectrum(m);
    for i in 0..6 {
        let off = have[i] - want[i];
        let room = if i == 0 { 5.0 } else { 4.0 };
        if off.abs() <= room {
            continue;
        }
        // Who fills that band most (alone, against their own total and loudness).
        let culprit = heard
            .tracks
            .iter()
            .enumerate()
            .filter(|(t, a)| a.lufs > -60.0 && *t < p.tracks.len())
            .max_by(|a, b| {
                let score = |x: &Analysis| spectrum(x)[i] + x.lufs;
                score(a.1).total_cmp(&score(b.1))
            })
            .map(|(t, _)| t);
        let (what, fix) = match (i, off > 0.0) {
            (0, false) => ("thin: little sub", "a sub or 808 with weight below 60 Hz, louder; no low cut on the bass".to_string()),
            (0, true) => ("too much sub: it'll swamp small speakers and eat headroom", "a low cut at 30 Hz on the bass (set_eq_band), turn the sub down".to_string()),
            (1, false) => ("no low-end body", "on the bass: a bell +3 dB at 100 Hz (set_eq_band) and set_saturator drive 9 (its harmonics fill 60-250 Hz); a kick with body; mix_role does both".to_string()),
            (1, true) => ("boomy", "a bell -3 dB at 120-200 Hz on the bass or kick (set_eq_band), a low cut at 120 Hz on everything else".to_string()),
            (2, true) => ("muddy: the low mids pile up", "a bell -3 dB at 300-400 Hz, q 1, on the chords and pads (set_eq_band); a low cut at 150-200 Hz on them".to_string()),
            (2, false) => ("hollow: thin low mids", "less low cut on the chords".to_string()),
            (3, true) => ("honky, forward mids", "a bell -2 dB at 1-2 kHz on the loudest part (set_eq_band)".to_string()),
            (3, false) => ("the mids are scooped: parts lose definition", "a bell +2 dB at 2 kHz on the lead (set_eq_band)".to_string()),
            (4, true) => ("harsh: too much 4-8 kHz", "a dynamic bell at 4-5 kHz, q 1.5, range -4 dB, on the brightest part (set_eq_band with dynamic): it only cuts when it bites".to_string()),
            (4, false) => ("dull: no bite", "a bell +2 dB at 5 kHz on the drums and lead (set_eq_band)".to_string()),
            (5, true) => ("fizzy, too much air", "a high shelf -3 dB at 10 kHz (set_eq_band)".to_string()),
            _ => ("dark: no air", "a high shelf +2 dB at 10 kHz on the hats and lead (set_eq_band; lo-fi wants it dark: fine there)".to_string()),
        };
        out.push(issue(
            if off.abs() > room + 3.0 {
                "fix"
            } else {
                "improve"
            },
            "mix",
            format!(
                "{what}: {} at {:.1} dB, {against} about {:.1}",
                BANDS[i], have[i], want[i]
            ),
            culprit
                .filter(|_| off > 0.0)
                .map(|t| format!("most from {}", track_name(p, t))),
            fix,
        ));
    }
    // Parts outside their role's place.
    let level = |t: usize| heard.tracks.get(t).map_or(-100.0, |a| a.lufs);
    let core: Vec<usize> = (0..p.tracks.len())
        .filter(|&t| matches!(roles[t], "drums" | "bass" | "lead" | "vocals"))
        .collect();
    let loudest_core = core.iter().map(|&t| level(t)).fold(-100.0f32, f32::max);
    for (t, &role) in roles.iter().enumerate() {
        if heard.tracks.get(t).is_none() || p.tracks[t].mute {
            continue;
        }
        let has_regions = !p.tracks[t].regions.is_empty();
        if has_regions && level(t) < -60.0 {
            out.push(issue(
                "fix",
                "mix",
                "makes no sound".into(),
                Some(track_name(p, t)),
                "give it an instrument that has a sound on its notes (sounds), or delete it".into(),
            ));
            continue;
        }
        if matches!(role, "pad" | "texture" | "counter" | "arp" | "fx")
            && loudest_core > -60.0
            && level(t) > loudest_core + 1.0
        {
            out.push(issue(
                "improve",
                "mix",
                format!(
                    "a {} louder ({:.1} LUFS) than the drums, bass and lead",
                    role,
                    level(t)
                ),
                Some(track_name(p, t)),
                format!(
                    "set_track volume_db down {:.0} dB, or mix_role it",
                    level(t) - loudest_core + 4.0
                ),
            ));
        }
        // Low end in parts that aren't the low end.
        if !matches!(role, "bass" | "drums") && level(t) > -40.0 {
            let s = spectrum(&heard.tracks[t]);
            if s[0].max(s[1]) > -4.0 {
                out.push(issue(
                    "improve",
                    "mix",
                    format!(
                        "a {} with most of its energy under 250 Hz, under the bass",
                        role
                    ),
                    Some(track_name(p, t)),
                    format!("a low cut at 150 Hz on track {t} (set_eq_band; mix_role does it)"),
                ));
            }
        }
    }
    if m.stereo.correlation < 0.2 {
        out.push(issue(
            "fix",
            "mix",
            format!(
                "left and right fight (correlation {:.2}): it thins out, or vanishes, in mono",
                m.stereo.correlation
            ),
            None,
            "find the wide part (a stereo widener, or a sound out of phase) and narrow it".into(),
        ));
    } else if m.stereo.width_db < -20.0 && p.tracks.len() > 2 {
        out.push(issue("improve", "mix", "nearly mono: everything in the middle".into(), None,
            "pan the counter, perc and doubled parts (set_track pan ±0.3-0.6), keep the kick, bass and lead centred".into()));
    }
    // Sections: the hook's the loudest.
    for w in heard.sections.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let hook = ["chorus", "hook", "drop"]
            .iter()
            .any(|x| b.0.to_lowercase().contains(x));
        if hook && b.1.lufs + 0.5 < a.1.lufs {
            out.push(issue(
                "improve",
                "arrangement",
                format!(
                    "{} ({:.1} LUFS) is quieter than {} before it ({:.1})",
                    b.0, b.1.lufs, a.0, a.1.lufs
                ),
                None,
                format!("{} should hit hardest: more parts, louder, wider", b.0),
            ));
        }
    }
    out
}

/// Both, the worst first.
pub fn critique(p: &Project, heard: Option<&Heard>) -> Vec<Issue> {
    let mut all = music(p);
    if let Some(h) = heard {
        all.extend(mix(p, h));
    }
    let rank = |l: &str| match l {
        "fix" => 0,
        "improve" => 1,
        _ => 2,
    };
    all.sort_by_key(|i| rank(i.level));
    all
}

#[cfg(test)]
mod tests;
