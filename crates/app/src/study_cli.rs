//! Studying a song from its file, for the command line, agents and the Song Study page: the
//! engine's study, kept (so it's made once a file), and, online, what the file doesn't say:
//! its lyrics (LRCLIB, timed where it can) and its year and genre (MusicBrainz). Only its title,
//! artist and length leave this Mac; its audio never does.

use std::path::{Path, PathBuf};

use pina_engine::study::{self, Study};
use serde_json::{Value, json};

/// Where studies are kept (`PINA_STUDIES`, for tests).
fn cache_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PINA_STUDIES") {
        return PathBuf::from(d);
    }
    pina_engine::paths::caches().join("studies")
}

/// Every song studied, the latest first: its online study where there is one.
pub fn studied() -> Vec<Study> {
    let Ok(entries) = std::fs::read_dir(cache_dir()) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, bool, Study)> = entries
        .flatten()
        .filter_map(|e| {
            let when = e.metadata().ok()?.modified().ok()?;
            let online = e.file_name().to_string_lossy().ends_with("-online.json");
            let s: Study = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
            (s.version == study::VERSION && s.file.is_file()).then_some((when, online, s))
        })
        .collect();
    // Online first, then the latest: the first of each file is the one kept.
    found.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    let mut out: Vec<(std::time::SystemTime, Study)> = Vec::new();
    for (when, _, s) in found {
        match out.iter_mut().find(|(_, o)| o.file == s.file) {
            Some(o) => o.0 = o.0.max(when),
            None => out.push((when, s)),
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.0));
    out.into_iter().map(|(_, s)| s).collect()
}

/// A study forgotten: its kept files gone (the song's file stays).
pub fn forget(path: &Path) {
    for online in [false, true] {
        if let Some(f) = cache_file(path, online) {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// The file a study of `path` is kept in: by its path, size and when it last changed.
fn cache_file(path: &Path, online: bool) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let full = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in full
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .chain(&meta.len().to_le_bytes())
        .chain(&modified.to_le_bytes())
    {
        h = (h ^ *b as u64).wrapping_mul(0x100_0000_01b3);
    }
    Some(cache_dir().join(format!(
        "{h:016x}-{}{}.json",
        study::VERSION,
        if online { "-online" } else { "" }
    )))
}

/// A study of the song in `path`, made or kept; `online` adds its lyrics, year and genre.
pub fn study(path: &Path, online: bool) -> Result<Study, String> {
    if !path.is_file() {
        return Err(format!("{} isn't an audio file", path.display()));
    }
    let kept = |online: bool| -> Option<Study> {
        let b = std::fs::read(cache_file(path, online)?).ok()?;
        serde_json::from_slice::<Study>(&b)
            .ok()
            .filter(|s| s.version == study::VERSION)
    };
    let keep = |s: &Study, online: bool| {
        if let Some(f) = cache_file(path, online) {
            let _ = std::fs::create_dir_all(cache_dir());
            let _ = std::fs::write(f, serde_json::to_vec(s).unwrap_or_default());
        }
    };
    if online && let Some(s) = kept(true) {
        return Ok(s);
    }
    let mut s = match kept(false) {
        Some(s) => s,
        None => {
            let s = study::study(path)?;
            keep(&s, false);
            s
        }
    };
    // Kept online only once the lyrics service answered: offline or busy, it's asked again
    // next time.
    if online && look_up(&mut s) {
        keep(&s, true);
    }
    Ok(s)
}

/// A URL's JSON, by curl (macOS has it), within a few seconds: `Err` if the server couldn't be
/// reached or was busy, `Ok(None)` if it answered with nothing (a 404).
fn fetch(url: &str) -> Result<Option<Value>, ()> {
    // A busy server is asked once more, a moment later.
    fetch_once(url).or_else(|_| {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        fetch_once(url)
    })
}

fn fetch_once(url: &str) -> Result<Option<Value>, ()> {
    let out = std::process::Command::new("curl")
        .args([
            "-s",
            "-m",
            "15",
            "-w",
            "\n%{http_code}",
            "-A",
            "PinaStudio/0.1 (song study; https://pina.studio)",
            url,
        ])
        .output()
        .map_err(|_| ())?;
    // Cut off (a timeout) or unreachable: not an answer.
    if !out.status.success() {
        return Err(());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, code) = text.rsplit_once('\n').ok_or(())?;
    match code.trim().parse::<u16>().unwrap_or(0) {
        200..=299 => serde_json::from_str(body).map(Some).map_err(|_| ()),
        400..=499 => Ok(None),
        _ => Err(()),
    }
}

fn get(url: &str) -> Option<Value> {
    fetch(url).ok().flatten()
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b => format!("%{b:02X}"),
        })
        .collect()
}

/// What the file doesn't say, looked up by its title and artist: its lyrics, and its year and
/// genre if its tags don't give them. Whether the lyrics service answered.
fn look_up(s: &mut Study) -> bool {
    // Asked by the title alone: "(feat. …)", "(Remix)" and the like aren't in every listing.
    let title = s
        .identity
        .title
        .split(['(', '['])
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let artist = s.identity.artist.clone();
    if title.is_empty() {
        return true;
    }
    // Lyrics: LRCLIB, the exact match by length first, else the nearest found.
    let duration = s.identity.seconds.round() as i64;
    let exact = match artist.is_empty() {
        true => Ok(None),
        false => fetch(&format!(
            "https://lrclib.net/api/get?artist_name={}&track_name={}&duration={duration}",
            encode(&artist),
            encode(&title)
        )),
    };
    let found = match exact {
        Ok(Some(r)) => Some(r),
        _ => {
            let q = if artist.is_empty() {
                title.clone()
            } else {
                format!("{artist} {title}")
            };
            let Ok(results) = fetch(&format!("https://lrclib.net/api/search?q={}", encode(&q)))
            else {
                return false;
            };
            // Its title, by the same artist (when known), of the nearest length: a song of the
            // same name by someone else, or another of theirs, isn't this one.
            let same = |r: &Value| {
                let (theirs, mine) = (
                    r["artistName"].as_str().unwrap_or("").to_lowercase(),
                    artist.to_lowercase(),
                );
                let named = r["trackName"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&title.to_lowercase());
                named
                    && (mine.is_empty()
                        || (!theirs.is_empty()
                            && (theirs.contains(&mine) || mine.contains(&theirs))))
            };
            results.and_then(|v| {
                v.as_array()?
                    .iter()
                    .filter(|r| !r["instrumental"].as_bool().unwrap_or(false) && same(r))
                    // A snippet (a line or two someone posted) only if there's nothing whole.
                    .min_by_key(|r| {
                        let lines = r["plainLyrics"]
                            .as_str()
                            .map_or(0, |t| t.lines().filter(|l| !l.trim().is_empty()).count());
                        (r["duration"].as_f64().unwrap_or(0.0) as i64 - duration).abs()
                            + if lines < 8 { 600 } else { 0 }
                    })
                    .cloned()
            })
        }
    };
    if let Some(r) = found {
        // Timed for another cut of the song (a radio edit, an extended mix), the times would land
        // in the wrong sections: the words alone are kept.
        let close = (r["duration"].as_f64().unwrap_or(0.0) as i64 - duration).abs() <= 4;
        let synced = r["syncedLyrics"].as_str().filter(|_| close);
        let lines = study::words::parse(synced, r["plainLyrics"].as_str());
        if !lines.is_empty() {
            let timed = lines.iter().any(|l| l.0.is_some());
            let source = format!("LRCLIB{}", if timed { ", timed" } else { "" });
            study::attach_lyrics(s, &lines, &source);
        }
        if s.identity.album.is_empty() {
            s.identity.album = r["albumName"].as_str().unwrap_or("").to_string();
        }
    }
    // Year and genre: MusicBrainz, when the tags don't say.
    if (s.identity.year.is_empty() || s.identity.genre.is_empty()) && !artist.is_empty() {
        let q = format!("recording:\"{title}\" AND artist:\"{artist}\"");
        if let Some(v) = get(&format!(
            "https://musicbrainz.org/ws/2/recording/?query={}&fmt=json&limit=3",
            encode(&q)
        )) && let Some(rec) = v["recordings"].as_array().and_then(|a| a.first())
        {
            if s.identity.year.is_empty() {
                s.identity.year = rec["first-release-date"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(4)
                    .collect();
            }
            if s.identity.genre.is_empty() {
                let mut tags: Vec<(i64, String)> = rec["tags"]
                    .as_array()
                    .map(|t| {
                        t.iter()
                            .filter_map(|t| {
                                Some((
                                    t["count"].as_i64().unwrap_or(0),
                                    t["name"].as_str()?.to_string(),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                tags.sort_by_key(|t| std::cmp::Reverse(t.0));
                s.identity.genre = tags
                    .into_iter()
                    .take(3)
                    .map(|t| t.1)
                    .collect::<Vec<_>>()
                    .join(", ");
            }
        }
    }
    true
}

/// A study as JSON for an agent: everything, its long lists (every beat, every bar's loudness)
/// cut to what's useful, and a summary to read first.
pub fn for_agent(s: &Study) -> Value {
    let mut v = serde_json::to_value(s).unwrap_or(Value::Null);
    if let Some(t) = v.get_mut("tempo") {
        t["beats"] = json!(s.tempo.beats.len());
        t["bars"] = json!(s.tempo.bars.len());
    }
    v["energy"] = json!(
        s.energy
            .iter()
            .map(|e| (e * 100.0).round() / 100.0)
            .collect::<Vec<_>>()
    );
    v["summary"] = json!(summary(s));
    v
}

/// The study in a paragraph or two.
pub fn summary(s: &Study) -> String {
    let id = &s.identity;
    let who = match (id.title.is_empty(), id.artist.is_empty()) {
        (false, false) => format!("\"{}\" by {}", id.title, id.artist),
        (false, true) => format!("\"{}\"", id.title),
        _ => id.file_name.clone(),
    };
    let year = if id.year.is_empty() {
        String::new()
    } else {
        format!(" ({})", id.year)
    };
    let form: Vec<String> = s
        .sections
        .iter()
        .map(|x| format!("{} ({} bars)", x.name, x.bars))
        .collect();
    let chorus = s.sections.iter().find(|x| x.name.starts_with("Chorus"));
    let mut out = format!(
        "{who}{year}: {:.0} BPM in {}/{}, {} (fit {:.2}), {:.0}:{:02.0} long. Form: {}.",
        s.tempo.bpm,
        s.tempo.time_signature[0],
        s.tempo.time_signature[1],
        s.key.name,
        s.key.fit,
        (id.seconds / 60.0).floor(),
        id.seconds % 60.0,
        form.join(", ")
    );
    if let Some(c) = chorus {
        out.push_str(&format!(
            " Chorus goes round {} ({}).",
            c.chords, c.numerals
        ));
    }
    out.push_str(&format!(
        " Groove: kick {}, snare {}, hats {}, swing {:.0} on the {}, nearest Pina Studio groove {}.",
        s.groove.kick,
        s.groove.snare,
        s.groove.hats,
        s.groove.swing,
        s.groove.swing_on,
        s.groove.nearest
    ));
    if let Some(g) = s.genres.first() {
        out.push_str(&format!(" Nearest playbook: {}.", g.name));
    }
    let m = &s.mix.overall;
    out.push_str(&format!(
        " Mix: {:.1} LUFS, peaks {:.1} dBFS, crest {:.1} dB, loudness range {:.1} dB.",
        m.lufs,
        m.peak_db[0].max(m.peak_db[1]),
        m.crest_db,
        s.mix.range_db
    ));
    if let Some(l) = &s.lyrics {
        out.push_str(&format!(
            " Lyrics ({}): {} lines, {}.",
            l.source,
            l.lines.len(),
            l.voice
        ));
        if let Some(h) = l.hooks.first() {
            out.push_str(&format!(" Hook: \"{h}\"."));
        }
    }
    out
}
