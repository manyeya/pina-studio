//! A song studied: everything Pina Studio can tell about an existing song from its file. Who it is
//! (its tags), its tempo and bars, its key and chords (as numerals, and the loops each section
//! goes round), its form (intro, verse, pre-chorus, chorus, bridge, outro, found by where it
//! changes and what repeats), its groove (kick, snare and hats on a grid, swing, syncopation),
//! what plays in it (as far as can be measured), its mix and dynamics, section by section, the
//! genres it's nearest, and (given its lyrics) their rhymes, hooks and words. What it means is
//! for a reader: an agent, given this.

pub mod beats;
pub mod features;
pub mod form;
pub mod groove;
pub mod harmony;
pub mod words;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    analysis::{self, Analysis},
    music::Key,
    playbook::{self, Part, Playbook, Section},
    theory::Chord,
};

pub use groove::Groove;
pub use harmony::ChordAt;
pub use words::Lyrics;

/// Bumped when what a study holds changes, so older ones are made again.
pub const VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Identity {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: String,
    /// The genre its tags give.
    pub genre: String,
    pub seconds: f32,
    pub file_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Tempo {
    pub bpm: f32,
    /// How steadily the song keeps it, 0 to 1.
    pub confidence: f32,
    pub time_signature: [u32; 2],
    /// Each beat's time and each bar's start, seconds.
    pub beats: Vec<f32>,
    pub bars: Vec<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KeyInfo {
    pub name: String,
    /// How well the song's notes fit it, -1 to 1 (above 0.6 clear).
    pub fit: f32,
    /// Sections in another key: (section index, key).
    pub changes: Vec<(usize, String)>,
    /// How far its notes sit off A440, in cents (a song tuned to 432 Hz is -32).
    #[serde(default)]
    pub tuning: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SectionInfo {
    pub name: String,
    /// The same letter for the same music.
    pub label: String,
    pub start: f32,
    pub end: f32,
    /// Bars, from 0, and how many.
    pub bar: usize,
    pub bars: usize,
    /// How hard it plays against the rest, 0 to 1.
    pub energy: f32,
    pub lufs: f32,
    /// The chords it goes round, bar by bar, and as numerals.
    pub chords: String,
    pub numerals: String,
}

/// Something heard playing, as far as can be measured, and how sure.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Element {
    pub name: String,
    /// The part it plays, as playbooks name parts: drums, bass, chords, pad, lead, vocals...
    pub role: String,
    pub confidence: f32,
    pub about: String,
    /// The sections it's heard in.
    pub sections: Vec<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Mix {
    pub overall: Analysis,
    /// How far its loudness moves between quiet and loud bars, dB (loudness range).
    pub range_db: f32,
    pub sections: Vec<Analysis>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GenreMatch {
    pub id: String,
    pub name: String,
    pub score: f32,
    pub why: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Study {
    pub version: u32,
    pub file: PathBuf,
    pub identity: Identity,
    pub tempo: Tempo,
    pub key: KeyInfo,
    pub sections: Vec<SectionInfo>,
    pub chords: Vec<ChordAt>,
    pub groove: Groove,
    pub instruments: Vec<Element>,
    pub mix: Mix,
    pub genres: Vec<GenreMatch>,
    pub lyrics: Option<Lyrics>,
    /// Each bar's loudness, 0 to 1 against the song's range: its shape, for drawing.
    pub energy: Vec<f32>,
}

/// The tags a file carries: title, artist, album, year, genre; else what its name says
/// ("Artist - Title").
pub fn identity(path: &Path) -> Identity {
    let mut id = Identity {
        file_name: path
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
        ..Default::default()
    };
    if let Some(tags) = tags(path) {
        for (k, v) in tags {
            let slot = match k {
                "title" => &mut id.title,
                "artist" => &mut id.artist,
                "album" => &mut id.album,
                "year" => &mut id.year,
                "genre" => &mut id.genre,
                _ => continue,
            };
            if slot.is_empty() {
                *slot = v;
            }
        }
    }
    if id.year.len() > 4 {
        id.year.truncate(4);
    }
    if id.title.is_empty() {
        let stem = path
            .file_stem()
            .map_or(String::new(), |s| s.to_string_lossy().into_owned());
        // "01 - Artist_017 - Title [-2st]": a track number, a catalogue number and an edit's
        // note aren't its name.
        let digits = stem.len() - stem.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let stem = match &stem[digits..] {
            rest if (1..=3).contains(&digits)
                && (rest.starts_with(". ") || rest.starts_with(" - ")) =>
            {
                rest.trim_start_matches(['.', ' ', '-'])
            }
            _ => stem.as_str(),
        };
        let stem = match (stem.find('['), stem.ends_with(']')) {
            (Some(i), true) if i > 0 => stem[..i].trim(),
            _ => stem.trim(),
        };
        match stem.split_once(" - ") {
            Some((a, t)) if id.artist.is_empty() => {
                let a = a.trim();
                id.artist = match a.rsplit_once('_') {
                    Some((name, n))
                        if !name.is_empty() && n.chars().all(|c| c.is_ascii_digit()) =>
                    {
                        name.to_string()
                    }
                    _ => a.to_string(),
                };
                id.title = t.trim().to_string();
            }
            _ => id.title = stem.to_string(),
        }
    }
    id
}

/// The file's tags, by Symphonia.
fn tags(path: &Path) -> Option<Vec<(&'static str, String)>> {
    use symphonia::core::{
        formats::{FormatOptions, probe::Hint},
        io::MediaSourceStream,
        meta::{MetadataOptions, StandardTag},
    };
    let file = std::fs::File::open(path).ok()?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;
    let mut out = Vec::new();
    let mut meta = format.metadata();
    let revision = meta.skip_to_latest()?;
    for tag in &revision.media.tags {
        let (k, v) = match &tag.std {
            Some(StandardTag::TrackTitle(v)) => ("title", v),
            Some(StandardTag::Artist(v)) => ("artist", v),
            Some(StandardTag::AlbumArtist(v)) => ("artist", v),
            Some(StandardTag::Album(v)) => ("album", v),
            Some(
                StandardTag::ReleaseDate(v)
                | StandardTag::RecordingDate(v)
                | StandardTag::OriginalReleaseDate(v),
            ) => ("year", v),
            Some(StandardTag::Genre(v)) => ("genre", v),
            _ => continue,
        };
        out.push((k, v.to_string()));
    }
    Some(out)
}

fn db(power: f32) -> f32 {
    10.0 * power.max(1e-12).log10()
}

/// Studies the song in an audio file (WAV, AIFF, FLAC, MP3).
pub fn study(path: &Path) -> Result<Study, String> {
    let (mut sides, rate) = crate::engine::read_audio(path)?;
    let l = sides.remove(0);
    let r = sides.pop().unwrap_or_else(|| l.clone());
    let mut s = study_audio(&l, &r, rate as f32)?;
    s.identity = Identity {
        seconds: s.identity.seconds,
        ..identity(path)
    };
    s.file = path.to_path_buf();
    Ok(s)
}

/// Studies a song's audio.
pub fn study_audio(l: &[f32], r: &[f32], rate: f32) -> Result<Study, String> {
    if l.len() < (rate * 4.0) as usize {
        return Err("too short to study: under four seconds".into());
    }
    let seconds = l.len() as f32 / rate;
    let mono: Vec<f32> = l.iter().zip(r).map(|(a, b)| 0.5 * (a + b)).collect();
    let f = features::frames(&mono, rate);
    let b = beats::beats(&f);
    let mut bars = b.bars();
    if bars.is_empty() {
        bars.push(0.0);
    }
    let end = seconds;
    let span = |i: usize, starts: &[f32]| (starts[i], starts.get(i + 1).copied().unwrap_or(end));
    // Each beat's pitch classes.
    let beat_chroma: Vec<[f32; 12]> = (0..b.beats.len())
        .map(|i| {
            let (a, z) = span(i, &b.beats);
            features::mean(&f.chroma, f.at(a), f.at(z).max(f.at(a) + 1))
        })
        .collect();
    let beat_bass: Vec<[f32; 12]> = (0..b.beats.len())
        .map(|i| {
            let (a, z) = span(i, &b.beats);
            features::mean(&f.bass, f.at(a), f.at(z).max(f.at(a) + 1))
        })
        .collect();
    // The key, from all its notes (louder frames counting more).
    let weighted = |rows: &[[f32; 12]]| -> [f32; 12] {
        let mut acc = [0f32; 12];
        for (i, row) in rows.iter().enumerate() {
            let w = 10f32.powf(f.level[i] / 20.0);
            let total: f32 = row.iter().sum::<f32>().max(1e-9);
            for k in 0..12 {
                acc[k] += w * row[k] / total;
            }
        }
        acc
    };
    let scored = harmony::keys(&weighted(&f.chroma), &weighted(&f.bass));
    // Chords, beat by beat, then as spans.
    let per_beat = harmony::chords(&beat_chroma, &beat_bass);
    // Keys that share most of their notes (a relative, a fifth either side): the chords say
    // which it is.
    let phrases: Vec<usize> = (b.first..b.beats.len())
        .step_by(b.per_bar.max(1) * 4)
        .collect();
    let (key, fit) = harmony::by_chords(&scored, &per_beat, &phrases);
    let mut chords: Vec<ChordAt> = Vec::new();
    for (i, c) in per_beat.iter().enumerate() {
        let (a, z) = span(i, &b.beats);
        let name = c.map_or("N".to_string(), |c| c.name_in(Some(&key)));
        match chords.last_mut() {
            Some(last) if last.chord == name => last.end = z,
            _ => chords.push(ChordAt {
                start: a,
                end: z,
                numeral: c.map_or(String::new(), |c| harmony::numeral(&c, &key)),
                chord: name,
            }),
        }
    }
    // The bars, described for the form.
    let bar_rows: Vec<form::Bar> = (0..bars.len())
        .map(|i| {
            let (a, z) = span(i, &bars);
            let (fa, fz) = (f.at(a), f.at(z).max(f.at(a) + 1));
            let mut chroma = features::mean(&f.chroma, fa, fz);
            let total: f32 = chroma.iter().sum::<f32>().max(1e-9);
            chroma.iter_mut().for_each(|v| *v /= total);
            let timbre = features::mean(&f.timbre, fa, fz).to_vec();
            let power = f.level[fa..fz.min(f.level.len())]
                .iter()
                .map(|v| 10f32.powf(v / 10.0))
                .sum::<f32>()
                / (fz - fa) as f32;
            form::Bar {
                chroma,
                timbre,
                level: db(power),
            }
        })
        .collect();
    let parts = form::form(&bar_rows);
    // Each bar's chord: the one held longest in it.
    let bar_chords: Vec<String> = (0..bars.len())
        .map(|i| {
            let (a, z) = span(i, &bars);
            chords
                .iter()
                .map(|c| (c, (c.end.min(z) - c.start.max(a)).max(0.0)))
                .filter(|(_, held)| *held > 0.0)
                .max_by(|x, y| x.1.total_cmp(&y.1))
                .map_or("N".into(), |(c, _)| c.chord.clone())
        })
        .collect();
    let sections: Vec<SectionInfo> = parts
        .iter()
        .map(|p| {
            let start = if p.from == 0 { 0.0 } else { bars[p.from] };
            let end_t = bars.get(p.to).copied().unwrap_or(end);
            let looped = harmony::loop_of(&bar_chords[p.from..p.to.min(bar_chords.len())]);
            let numerals: Vec<String> = looped
                .iter()
                .map(|c| Chord::parse(c).map_or(c.clone(), |c| harmony::numeral(&c, &key)))
                .collect();
            SectionInfo {
                name: p.name.clone(),
                label: p.label.to_string(),
                start,
                end: end_t,
                bar: p.from,
                bars: p.to - p.from,
                energy: p.energy,
                lufs: 0.0,
                chords: looped.join(" | "),
                numerals: numerals.join(" | "),
            }
        })
        .collect();
    // The mix: the whole song, then each section.
    let cut = |a: f32| ((a * rate) as usize).min(l.len());
    let overall = analysis::analyze(l, r, rate as f64);
    let section_mix: Vec<Analysis> = sections
        .iter()
        .map(|s| {
            let (a, z) = (cut(s.start), cut(s.end));
            analysis::analyze(&l[a..z.max(a + 1)], &r[a..z.max(a + 1)], rate as f64)
        })
        .collect();
    let mut sections = sections;
    for (s, m) in sections.iter_mut().zip(&section_mix) {
        s.lufs = m.lufs;
    }
    // Key changes: a section whose own notes and chords fit another key (not just the song's
    // relative) clearly better than the song's.
    let mut changes = Vec::new();
    for (i, s) in sections.iter().enumerate() {
        let (fa, fz) = (
            f.at(s.start),
            f.at(s.end).max(f.at(s.start) + 1).min(f.chroma.len()),
        );
        let scored = harmony::keys(&weighted(&f.chroma[fa..fz]), &weighted(&f.bass[fa..fz]));
        let beats: Vec<_> = b
            .beats
            .iter()
            .zip(&per_beat)
            .filter(|(t, _)| **t >= s.start && **t < s.end)
            .map(|(_, c)| *c)
            .collect();
        let (k, kf) = harmony::by_chords(&scored, &beats, &[]);
        let here = scored.iter().find(|x| x.0 == key).map_or(-1.0, |x| x.2);
        let relative =
            k.minor != key.minor && (k.root + if k.minor { 3 } else { 9 }) % 12 == key.root;
        if k != key && !relative && kf > here + 0.15 && kf > 0.65 && s.bars >= 4 {
            changes.push((i, k.name()));
        }
    }
    // Loudness bar by bar: its shape, and its range.
    let levels: Vec<f32> = bar_rows.iter().map(|b| b.level).collect();
    let top = levels.iter().copied().fold(f32::MIN, f32::max);
    let heard: Vec<f32> = levels.iter().copied().filter(|&v| v > top - 40.0).collect();
    let pct = |q: f32| {
        let mut v = heard.clone();
        v.sort_by(f32::total_cmp);
        v.get(((v.len().max(1) - 1) as f32 * q) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    let range_db = pct(0.95) - pct(0.10);
    let floor = top - 30.0;
    let energy: Vec<f32> = levels
        .iter()
        .map(|&v| ((v - floor) / 30.0).clamp(0.0, 1.0))
        .collect();
    // The groove, over the bars loud enough to have the beat in them.
    let loud: Vec<bool> = levels.iter().map(|&v| v > top - 9.0).collect();
    let groove = groove::groove(&f, &b.beats, &bars, b.per_bar, &loud);
    let instruments = instruments(&groove, &overall, &section_mix, &f, &sections);
    let tempo = Tempo {
        bpm: (b.bpm * 10.0).round() / 10.0,
        confidence: b.confidence,
        time_signature: [b.per_bar as u32, 4],
        beats: b.beats.clone(),
        bars: bars.clone(),
    };
    let genres = genres(&tempo, &key, &groove, &overall);
    Ok(Study {
        version: VERSION,
        file: PathBuf::new(),
        identity: Identity {
            seconds,
            ..Default::default()
        },
        tempo,
        key: KeyInfo {
            name: key.name(),
            fit,
            changes,
            tuning: f.tuning.round(),
        },
        sections,
        chords,
        groove,
        instruments,
        mix: Mix {
            overall,
            range_db,
            sections: section_mix,
        },
        genres,
        lyrics: None,
        energy,
    })
}

/// What plays, as far as the audio says: drums from the groove, low end and harmony from the
/// spectrum and the chroma, each with the sections it's heard in.
fn instruments(
    g: &Groove,
    overall: &Analysis,
    sections: &[Analysis],
    f: &features::Frames,
    info: &[SectionInfo],
) -> Vec<Element> {
    let mut out = Vec::new();
    let hits = |p: &str| p.chars().filter(|&c| c != '.' && c != 'g').count();
    let in_sections = |test: &dyn Fn(&Analysis) -> bool| -> Vec<usize> {
        sections
            .iter()
            .enumerate()
            .filter(|(_, a)| a.lufs > -50.0 && test(a))
            .map(|(i, _)| i)
            .collect()
    };
    let (k, s, h) = (hits(&g.kick), hits(&g.snare), hits(&g.hats));
    if k + s + h >= 4 {
        let mut parts = Vec::new();
        if k > 0 {
            parts.push("kick");
        }
        if s > 0 {
            parts.push("snare or clap");
        }
        if h > 0 {
            parts.push(if h >= 8 { "busy hats" } else { "hats" });
        }
        let busy = (k + s + h) as f32 / 16.0;
        out.push(Element {
            name: "Drums".into(),
            role: "drums".into(),
            confidence: (0.55 + busy * 0.6).min(0.95),
            about: format!(
                "{} on a {}-step bar, nearest Pina Studio {} groove",
                parts.join(", "),
                16,
                g.nearest.replace('_', " ")
            ),
            sections: in_sections(&|a: &Analysis| {
                a.spectrum_db.presence > -24.0 || a.spectrum_db.bass > -10.0
            }),
        });
    }
    let sub = overall.spectrum_db.sub;
    if sub > -7.0 {
        out.push(Element {
            name: if sub > -3.5 {
                "808 / sub bass".into()
            } else {
                "Sub bass".into()
            },
            role: "bass".into(),
            confidence: ((sub + 9.0) / 8.0).clamp(0.4, 0.95),
            about: format!("{sub:.1} dB of the mix under 60 Hz: the low end carries it"),
            sections: in_sections(&|a: &Analysis| a.spectrum_db.sub > -9.0),
        });
    } else if overall.spectrum_db.bass > -9.0 {
        out.push(Element {
            name: "Bass".into(),
            role: "bass".into(),
            confidence: 0.7,
            about: format!("{:.1} dB of the mix in 60-250 Hz", overall.spectrum_db.bass),
            sections: in_sections(&|a: &Analysis| a.spectrum_db.bass > -11.0),
        });
    }
    // Harmony: chords held still (pads, strings, organ) or struck (piano, guitar, plucks), by
    // how much the mids start against how steady the pitch classes are.
    let tonal = overall.notes.key_fit;
    if tonal > 0.3 {
        let starts: f32 = f.snare.iter().sum::<f32>() / f.len().max(1) as f32;
        let held = starts < 0.08;
        out.push(Element {
            name: if held {
                "Sustained chords (pads, keys, strings)".into()
            } else {
                "Struck or plucked chords (piano, guitar, plucks)".into()
            },
            role: if held { "pad".into() } else { "chords".into() },
            confidence: (0.4 + tonal * 0.4).min(0.8),
            about: format!("clear harmony: its notes fit a key with r = {tonal:.2}"),
            sections: (0..info.len())
                .filter(|&i| !info[i].chords.is_empty() && info[i].chords != "N")
                .collect(),
        });
    }
    if overall.spectrum_db.mid > -14.0 {
        out.push(Element {
            name: "A lead in the mids (voice or melody)".into(),
            role: "lead".into(),
            confidence: ((overall.spectrum_db.mid + 18.0) / 10.0).clamp(0.35, 0.75),
            about: format!(
                "{:.1} dB of the mix in 1-4 kHz, where voices and leads sit",
                overall.spectrum_db.mid
            ),
            sections: in_sections(&|a: &Analysis| a.spectrum_db.mid > -15.0),
        });
    }
    if h >= 8 || overall.spectrum_db.air > -20.0 {
        out.push(Element {
            name: "Hats, shakers or air".into(),
            role: "perc".into(),
            confidence: 0.6,
            about: format!("{:.1} dB over 8 kHz", overall.spectrum_db.air),
            sections: in_sections(&|a: &Analysis| a.spectrum_db.air > -24.0),
        });
    }
    out
}

/// The playbooks it's nearest: its tempo in their range (or at half or double it), its swing,
/// key, groove and the balance of its mix.
fn genres(t: &Tempo, key: &Key, g: &Groove, mix: &Analysis) -> Vec<GenreMatch> {
    let (books, _) = playbook::load();
    let s = &mix.spectrum_db;
    let mine = [s.sub, s.bass, s.low_mid, s.mid, s.presence, s.air];
    let mut out: Vec<GenreMatch> = books
        .iter()
        .map(|b| {
            let in_range =
                |bpm: f32| bpm as f64 >= b.tempo.0 - 2.0 && bpm as f64 <= b.tempo.1 + 2.0;
            let tempo = if in_range(t.bpm) {
                1.0
            } else if in_range(t.bpm * 2.0) || in_range(t.bpm / 2.0) {
                0.7
            } else {
                let mid = ((b.tempo.0 + b.tempo.1) / 2.0) as f32;
                (1.0 - (t.bpm - mid).abs() / 40.0).max(0.0)
            };
            let swing = (1.0 - (g.swing - b.swing).abs() / 15.0).max(0.0);
            let mode = if b.minor == key.minor { 1.0 } else { 0.4 };
            let groove = if b.groove == g.nearest { 1.0 } else { 0.3 };
            let dist: f32 = (0..6).map(|i| (mine[i] - b.spectrum[i]).abs()).sum::<f32>() / 6.0;
            let balance = (1.0 - dist / 10.0).max(0.0);
            let score = 0.32 * tempo + 0.12 * swing + 0.1 * mode + 0.22 * groove + 0.24 * balance;
            let why = format!(
                "{} BPM {} its {:.0}-{:.0}; groove {}; mix {:.1} dB off its balance",
                t.bpm.round(),
                if tempo >= 1.0 {
                    "in"
                } else if tempo >= 0.7 {
                    "at half or double"
                } else {
                    "outside"
                },
                b.tempo.0,
                b.tempo.1,
                if groove >= 1.0 {
                    "like its"
                } else {
                    "unlike its"
                },
                dist
            );
            GenreMatch {
                id: b.id.clone(),
                name: b.name.clone(),
                score,
                why,
            }
        })
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out.truncate(4);
    out
}

/// Takes in the song's lyrics: read for their rhymes, hooks and words, each line in its
/// section; and if a section the lyrics come back to most isn't the one called the chorus, it
/// is now.
pub fn attach_lyrics(s: &mut Study, lines: &[(Option<f32>, String)], source: &str) {
    let spans: Vec<(f32, f32)> = s.sections.iter().map(|x| (x.start, x.end)).collect();
    let lyrics = words::read(lines, &spans, source);
    // The letter whose sections hold the most hook lines.
    let hooks: Vec<&str> = lyrics.hooks.iter().map(String::as_str).collect();
    let mut by_label: Vec<(String, usize)> = Vec::new();
    for l in lyrics
        .lines
        .iter()
        .filter(|l| hooks.contains(&l.text.as_str()))
    {
        if let Some(sec) = l.section.and_then(|i| s.sections.get(i)) {
            match by_label.iter_mut().find(|(k, _)| *k == sec.label) {
                Some(e) => e.1 += 1,
                None => by_label.push((sec.label.clone(), 1)),
            }
        }
    }
    if let Some((label, n)) = by_label.into_iter().max_by_key(|e| e.1)
        && n >= 2
    {
        let spans: Vec<(usize, usize)> =
            s.sections.iter().map(|x| (x.bar, x.bar + x.bars)).collect();
        let labels: Vec<char> = s
            .sections
            .iter()
            .map(|x| x.label.chars().next().unwrap_or('A'))
            .collect();
        let energy: Vec<f32> = s.sections.iter().map(|x| x.energy).collect();
        let total = s.sections.last().map_or(0, |x| x.bar + x.bars);
        let names = form::name_with(&spans, &labels, &energy, total, label.chars().next());
        for (x, n) in s.sections.iter_mut().zip(names) {
            x.name = n;
        }
    }
    if !lyrics.lines.is_empty() && !s.instruments.iter().any(|e| e.role == "vocals") {
        let sung: Vec<usize> = {
            let mut v: Vec<usize> = lyrics.lines.iter().filter_map(|l| l.section).collect();
            v.sort_unstable();
            v.dedup();
            // Untimed lyrics don't say where: where a lead's heard in the mids, then.
            if v.is_empty() {
                v = s
                    .instruments
                    .iter()
                    .find(|e| e.role == "lead")
                    .map_or(Vec::new(), |e| e.sections.clone());
            }
            v
        };
        s.instruments.insert(
            0,
            Element {
                name: "Vocals".into(),
                role: "vocals".into(),
                confidence: 0.95,
                about: format!("{} lines of lyrics, {}", lyrics.lines.len(), lyrics.voice),
                sections: sung,
            },
        );
    }
    s.lyrics = Some(lyrics);
}

/// A playbook made from the song: its tempo, swing, key, groove, sections (their bars, energy
/// and who plays), its parts, mix and what stands out about it, to make songs like it.
pub fn to_playbook(s: &Study, taken: &[String]) -> Playbook {
    let name = match (s.identity.title.is_empty(), s.identity.artist.is_empty()) {
        (false, false) => format!("Like {} ({})", s.identity.title, s.identity.artist),
        (false, true) => format!("Like {}", s.identity.title),
        _ => "Like this song".to_string(),
    };
    let nearest = s.genres.first().and_then(|g| playbook::find(&g.id));
    let minor = s.key.name.ends_with("minor");
    let mut parts: Vec<Part> = Vec::new();
    for e in &s.instruments {
        if !parts.iter().any(|p| p.role == e.role) {
            let write = nearest
                .as_ref()
                .and_then(|b| b.parts.iter().find(|p| p.role == e.role))
                .map_or(String::new(), |p| p.write.clone());
            parts.push(Part {
                role: e.role.clone(),
                sound: e.name.to_lowercase(),
                write,
            });
        }
    }
    let sections = s
        .sections
        .iter()
        .enumerate()
        .map(|(i, x)| Section {
            name: x.name.clone(),
            bars: (x.bars.div_ceil(4) * 4).max(4) as u64,
            parts: s
                .instruments
                .iter()
                .filter(|e| e.sections.contains(&i))
                .map(|e| e.role.clone())
                .fold(Vec::new(), |mut v, r| {
                    if !v.contains(&r) {
                        v.push(r);
                    }
                    v
                }),
            energy: (1.0 + (x.energy * 4.0).round()) as u8,
        })
        .collect();
    let sp = &s.mix.overall.spectrum_db;
    let chorus = s.sections.iter().find(|x| x.name.starts_with("Chorus"));
    let mut notes = vec![
        format!(
            "Studied from {}: {:.0} BPM, {}, swing {:.0} on the {}.",
            s.identity.file_name, s.tempo.bpm, s.key.name, s.groove.swing, s.groove.swing_on
        ),
        format!(
            "Groove: kick {}, snare {}, hats {}.",
            s.groove.kick, s.groove.snare, s.groove.hats
        ),
    ];
    if let Some(c) = chorus {
        notes.push(format!(
            "Its chorus goes round {} ({}), {} bars.",
            c.chords, c.numerals, c.bars
        ));
    }
    if let Some(l) = &s.lyrics
        && let Some(h) = l.hooks.first()
    {
        notes.push(format!("Its hook: \"{h}\"."));
    }
    let bpm = s.tempo.bpm as f64;
    Playbook {
        id: playbook::new_id(&name, taken),
        name,
        tempo: ((bpm - 4.0).round(), (bpm + 4.0).round()),
        minor,
        groove: s.groove.nearest.clone(),
        swing: s.groove.swing.round(),
        harmony: nearest
            .as_ref()
            .map_or(if minor { "dark".into() } else { "warm".into() }, |b| {
                b.harmony.clone()
            }),
        parts,
        sections,
        lufs: (s.mix.overall.lufs * 2.0).round() / 2.0,
        spectrum: [sp.sub, sp.bass, sp.low_mid, sp.mid, sp.presence, sp.air]
            .map(|v| (v * 2.0).round() / 2.0),
        notes,
    }
}

#[cfg(test)]
mod tests;
