//! Measures the song study's tempo and key against what loops' file names say: "(128BPM)",
//! "_90bpm_", "in A minor", "Cmaj". Loops of six seconds or more, from the folders given (the
//! sample library's by default), every `--every`th of them.
//!
//!     cargo run --release -p pina-engine --example study_eval -- [--every N] [--list] [folders]

use std::path::{Path, PathBuf};

use pina_engine::{engine::read_audio, study::study_audio};

fn bpm_in(s: &str) -> Option<f32> {
    let lower = s.to_lowercase();
    let i = lower.find("bpm")?;
    let digits: String = lower[..i]
        .trim_end_matches([' ', '_', '-'])
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits
        .parse()
        .ok()
        .filter(|b: &f32| (50.0..=220.0).contains(b))
}

/// A key a name spells out: a note, then minor/min/m or major/maj.
fn key_in(s: &str) -> Option<String> {
    let names = [
        "C", "C#", "Db", "D", "D#", "Eb", "E", "F", "F#", "Gb", "G", "G#", "Ab", "A", "A#", "Bb",
        "B",
    ];
    let tokens: Vec<&str> = s
        .split([' ', '_', '-', '(', ')', '.'])
        .filter(|t| !t.is_empty())
        .collect();
    for (i, t) in tokens.iter().enumerate() {
        for n in names {
            let rest = match t.strip_prefix(n) {
                Some(r) => r,
                None => continue,
            };
            let quality = match rest.to_lowercase().as_str() {
                "min" | "minor" | "m" => Some("minor"),
                "maj" | "major" => Some("major"),
                "" => match tokens.get(i + 1).map(|x| x.to_lowercase()).as_deref() {
                    Some("minor" | "min") => Some("minor"),
                    Some("major" | "maj") => Some("major"),
                    _ => None,
                },
                _ => None,
            };
            if let Some(q) = quality {
                return pina_engine::music::Key::parse(&format!("{n} {q}")).map(|k| k.name());
            }
        }
    }
    None
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|e| {
            e.eq_ignore_ascii_case("wav")
                || e.eq_ignore_ascii_case("aif")
                || e.eq_ignore_ascii_case("aiff")
        }) {
            out.push(p);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let every: usize = args
        .iter()
        .position(|a| a == "--every")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let list = args.iter().any(|a| a == "--list");
    let mut dirs: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--") && a.parse::<usize>().is_err())
        .map(PathBuf::from)
        .collect();
    if dirs.is_empty() {
        dirs.push(pina_engine::paths::songs().join("Samples"));
    }
    let mut all = Vec::new();
    for d in &dirs {
        files(d, &mut all);
    }
    let labelled: Vec<PathBuf> = all
        .into_iter()
        .filter(|p| bpm_in(&p.to_string_lossy()).is_some())
        .step_by(every.max(1))
        .collect();
    let (mut n, mut exact, mut octave, mut keyed, mut key_right, mut key_relative) =
        (0, 0, 0, 0, 0, 0);
    let (mut roots, mut root_right) = (0, 0);
    let started = std::time::Instant::now();
    for p in &labelled {
        let Ok((mut sides, rate)) = read_audio(p) else {
            continue;
        };
        let l = sides.remove(0);
        let r = sides.pop().unwrap_or_else(|| l.clone());
        if (l.len() as f64) < rate * 4.0 {
            continue;
        }
        let Ok(s) = study_audio(&l, &r, rate as f32) else {
            continue;
        };
        let truth = bpm_in(&p.to_string_lossy()).unwrap();
        let close = |b: f32| (b - truth).abs() / truth < 0.03;
        n += 1;
        let ok = if close(s.tempo.bpm) {
            exact += 1;
            "ok"
        } else if close(s.tempo.bpm * 2.0)
            || close(s.tempo.bpm / 2.0)
            || close(s.tempo.bpm * 1.5)
            || close(s.tempo.bpm / 1.5)
        {
            octave += 1;
            "octave"
        } else {
            "WRONG"
        };
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let mut key_note = String::new();
        // A root alone after "]-" ("MSC_Neos[110]-D"): the tonic, mode unsaid.
        if let Some(root) = name
            .rsplit_once("]-")
            .map(|(_, r)| r.trim_end_matches(".wav").to_string())
            && let Some(want) = pina_engine::music::Key::parse(&format!("{root} major"))
        {
            let mine = pina_engine::music::Key::parse(&s.key.name).unwrap();
            roots += 1;
            // The tonic itself, or (a mode unsaid) the relative minor's.
            if mine.root == want.root
                || (mine.minor && (mine.root + 3) % 12 == want.root)
                || (!mine.minor && (want.root + 3) % 12 == mine.root)
            {
                root_right += 1;
            }
            key_note = format!(" key {} vs root {root}", s.key.name);
        }
        if let Some(k) = key_in(&name) {
            keyed += 1;
            let mine = pina_engine::music::Key::parse(&s.key.name).unwrap();
            let theirs = pina_engine::music::Key::parse(&k).unwrap();
            let relative = (mine.minor != theirs.minor)
                && (if mine.minor {
                    (mine.root + 3) % 12 == theirs.root
                } else {
                    (theirs.root + 3) % 12 == mine.root
                });
            if mine == theirs {
                key_right += 1;
            } else if relative {
                key_relative += 1;
            }
            key_note = format!(" key {} vs {k}", s.key.name);
        }
        if list || ok == "WRONG" {
            println!(
                "{ok:6} {:6.1} vs {truth:5.0}{key_note}  {name}",
                s.tempo.bpm
            );
        }
    }
    let pct = |a: usize, b: usize| {
        if b == 0 {
            0.0
        } else {
            100.0 * a as f32 / b as f32
        }
    };
    if roots > 0 {
        println!(
            "roots: {:.0}% of {roots} (the tonic, or its relative)",
            pct(root_right, roots)
        );
    }
    println!(
        "\n{n} loops: tempo {:.0}% exact, {:.0}% exact or half/double/thirds; key {:.0}% of {keyed} (and {:.0}% its relative); {:.1}s",
        pct(exact, n),
        pct(exact + octave, n),
        pct(key_right, keyed),
        pct(key_relative, keyed),
        started.elapsed().as_secs_f32()
    );
}
