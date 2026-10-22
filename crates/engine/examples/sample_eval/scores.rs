//! Every file analysed, and how often the analysis agrees with the labels: categories, kinds, pitches, tempos, keys, neighbours, duplicates, speed.

use super::*;

/// One file's results.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Row {
    pub(crate) path: PathBuf,
    pub(crate) truth: Truth,
    pub(crate) named: Analysis,
    pub(crate) blind: Analysis,
    pub(crate) millis: f32,
    pub(crate) feats: Option<(Vec<f32>, [f32; 10])>,
}

/// Analyses every file on every core, with its name and without.
pub(crate) fn run(files: &[PathBuf], fit: bool) -> Vec<Row> {
    let next = AtomicUsize::new(0);
    let rows = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = files.get(i) else { break };
                    let start = Instant::now();
                    let Ok((channels, rate)) = pina_engine::audio::engine::read_audio(path) else {
                        eprintln!("can't read {}", path.display());
                        continue;
                    };
                    let hint = path.to_string_lossy().to_lowercase();
                    let named = analyze(&channels, rate, &hint);
                    let millis = start.elapsed().as_secs_f32() * 1000.0;
                    let blind = analyze(&channels, rate, "");
                    let feats = fit.then(|| {
                        let f = features::extract(&channels, rate, None);
                        let loud = if blind.kind == Kind::OneShot {
                            f.loudness_short
                        } else {
                            f.loudness_long
                        };
                        (classify::vector(&f), trait_inputs(&f, loud))
                    });
                    let t = truth(path);
                    rows.lock().unwrap().push(Row {
                        path: path.clone(),
                        truth: t,
                        named,
                        blind,
                        millis,
                        feats,
                    });
                }
            });
        }
    });
    let mut rows = rows.into_inner().unwrap();
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    rows
}

pub(crate) fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "-".into()
    } else {
        format!("{:.1}% ({n}/{d})", 100.0 * n as f32 / d as f32)
    }
}

/// Category accuracy, overall and per category, and what each was taken for.
pub(crate) fn categories(rows: &[&Row], blind: bool, title: &str) {
    let mut confusion: HashMap<Category, HashMap<Category, usize>> = HashMap::new();
    let (mut right, mut total, mut drum_right, mut drum_total) = (0, 0, 0, 0);
    for r in rows {
        let Some(&first) = r.truth.cats.first() else {
            continue;
        };
        let got = if blind {
            r.blind.category
        } else {
            r.named.category
        };
        let ok = r.truth.cats.contains(&got);
        total += 1;
        right += ok as usize;
        if first.is_drum() && first != Category::Drums && r.truth.kind == Some(Kind::OneShot) {
            drum_total += 1;
            drum_right += ok as usize;
        }
        *confusion.entry(first).or_default().entry(got).or_default() += 1;
    }
    println!(
        "\n{title}: {}   drum one-shots: {}",
        pct(right, total),
        pct(drum_right, drum_total)
    );
    let mut cats: Vec<&Category> = confusion.keys().collect();
    cats.sort_by_key(|c| Category::ALL.iter().position(|x| x == *c));
    for c in cats {
        let row = &confusion[c];
        let n: usize = row.values().sum();
        let accepted: usize = rows
            .iter()
            .filter(|r| r.truth.cats.first() == Some(c))
            .filter(|r| {
                r.truth.cats.contains(&if blind {
                    r.blind.category
                } else {
                    r.named.category
                })
            })
            .count();
        let mut taken: Vec<(&Category, &usize)> = row.iter().collect();
        taken.sort_by(|a, b| b.1.cmp(a.1));
        let taken: Vec<String> = taken
            .iter()
            .map(|(c, n)| format!("{} {n}", c.name()))
            .collect();
        println!(
            "  {:<11} {:>14}   as: {}",
            c.name(),
            pct(accepted, n),
            taken.join(", ")
        );
    }
}

pub(crate) fn kinds(rows: &[&Row]) {
    let (mut named, mut blind, mut total) = (0, 0, 0);
    let mut wrong = Vec::new();
    for r in rows {
        let Some(k) = r.truth.kind else { continue };
        total += 1;
        named += (r.named.kind == k) as usize;
        blind += (r.blind.kind == k) as usize;
        if r.blind.kind != k {
            wrong.push(format!(
                "{} ({:.1}s)",
                r.path.file_name().unwrap().to_string_lossy(),
                r.blind.seconds
            ));
        }
    }
    println!(
        "\nLoop or one-shot: with names {}, audio only {}",
        pct(named, total),
        pct(blind, total)
    );
    if !wrong.is_empty() {
        println!(
            "  audio-only misses: {}",
            wrong
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

pub(crate) fn pitches(rows: &[&Row]) {
    let (mut within, mut octave, mut total, mut classes, mut class_total) = (0, 0, 0, 0, 0);
    let mut misses = Vec::new();
    let mut by_pack: HashMap<String, (usize, usize)> = HashMap::new();
    for r in rows {
        let p = r.blind.pitch.as_ref();
        if let Some(want) = r.truth.note {
            total += 1;
            let entry = by_pack.entry(r.truth.pack.clone()).or_default();
            entry.1 += 1;
            match p {
                Some(p) => {
                    let off = (p.note as f32 + p.cents / 100.0) - want as f32;
                    if off.abs() <= 0.5 {
                        within += 1;
                        entry.0 += 1;
                    } else if ((off.abs() - 12.0).abs() <= 0.5) || ((off.abs() - 24.0).abs() <= 0.5)
                    {
                        octave += 1;
                        misses.push(format!(
                            "{} heard {} {:+.0}c",
                            r.path.file_stem().unwrap().to_string_lossy(),
                            note_name(p.note),
                            p.cents
                        ));
                    } else {
                        misses.push(format!(
                            "{} heard {} {:+.0}c",
                            r.path.file_stem().unwrap().to_string_lossy(),
                            note_name(p.note),
                            p.cents
                        ));
                    }
                }
                None => misses.push(format!(
                    "{}: none",
                    r.path.file_stem().unwrap().to_string_lossy()
                )),
            }
        }
        if let Some(want) = r.truth.class {
            class_total += 1;
            if p.is_some_and(|p| p.note % 12 == want) {
                classes += 1;
            }
        }
    }
    println!(
        "\nPitch (audio only): notes within ±50 cents {}, an octave or two off {}; pitch classes {}",
        pct(within, total),
        pct(octave, total),
        pct(classes, class_total)
    );
    for (pack, (n, d)) in &by_pack {
        println!("  {pack}: {}", pct(*n, *d));
    }
    if !misses.is_empty() {
        println!(
            "  misses: {}",
            misses
                .iter()
                .take(30)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

pub(crate) fn tempos(rows: &[&Row]) {
    let mut by_pack: HashMap<String, [usize; 5]> = HashMap::new();
    let mut misses = Vec::new();
    for r in rows {
        let Some(want) = r.truth.bpm else { continue };
        if r.truth.kind != Some(Kind::Loop) {
            continue;
        }
        let e = by_pack.entry(r.truth.pack.clone()).or_default();
        e[4] += 1;
        let got = r.blind.bpm.or_else(|| {
            // Called a one-shot: what tempo would it have had?
            let (c, rate) = pina_engine::audio::engine::read_audio(&r.path).ok()?;
            let f = features::extract(&c, rate, None);
            f.tempo.map(|t| t.bpm)
        });
        let Some(got) = got else {
            e[3] += 1;
            misses.push(format!(
                "{}: none",
                r.path.file_name().unwrap().to_string_lossy()
            ));
            continue;
        };
        let near = |k: f32| ((got - want * k) / (want * k)).abs() < 0.02;
        if near(1.0) {
            e[0] += 1;
        } else if near(0.5) || near(2.0) {
            e[1] += 1;
            misses.push(format!(
                "{}: {got:.1} (half/double)",
                r.path.file_name().unwrap().to_string_lossy()
            ));
        } else {
            e[2] += 1;
            misses.push(format!(
                "{}: {got:.1}",
                r.path.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    println!("\nTempo of loops named with one (audio only, within 2%):");
    let mut packs: Vec<_> = by_pack.iter().collect();
    packs.sort_by_key(|p| p.0.clone());
    let mut sum = [0usize; 5];
    for (pack, e) in packs {
        println!(
            "  {pack:<16} right {:>16}  half/double {:>14}  other {:>3}  none {:>3}",
            pct(e[0], e[4]),
            pct(e[1], e[4]),
            e[2],
            e[3]
        );
        for i in 0..5 {
            sum[i] += e[i];
        }
    }
    println!(
        "  {:<16} right {:>16}  half/double {:>14}  other {:>3}  none {:>3}",
        "all",
        pct(sum[0], sum[4]),
        pct(sum[1], sum[4]),
        sum[2],
        sum[3]
    );
    if !misses.is_empty() {
        println!(
            "  misses: {}",
            misses
                .iter()
                .take(40)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

pub(crate) fn keys(rows: &[&Row]) {
    let (mut right, mut total, mut relative, mut mode_right, mut mode_total) = (0, 0, 0, 0, 0);
    let mut by_pack: HashMap<String, (usize, usize)> = HashMap::new();
    for r in rows {
        let Some(root) = r.truth.root else { continue };
        let e = by_pack.entry(r.truth.pack.clone()).or_default();
        total += 1;
        e.1 += 1;
        let Some(k) = &r.blind.key else { continue };
        if k.tonic == root {
            right += 1;
            e.0 += 1;
            if let Some(m) = r.truth.minor {
                mode_total += 1;
                mode_right += (k.minor == m) as usize;
            }
        } else if (k.minor && (k.tonic + 3) % 12 == root)
            || (!k.minor && (k.tonic + 9) % 12 == root)
        {
            relative += 1;
        }
    }
    println!(
        "\nKey (audio only) where the name gives a root: tonic right {}, its relative {}; mode right where the tonic is {}",
        pct(right, total),
        pct(relative, total),
        pct(mode_right, mode_total)
    );
    for (pack, (n, d)) in &by_pack {
        println!("  {pack}: {}", pct(*n, *d));
    }
    println!("\nKeys of the 90s pack's melodic loops (audio only):");
    for r in rows {
        if r.truth.pack != "90s"
            || r.blind.kind != Kind::Loop
            || r.truth.cats.first().is_none_or(|c| c.is_drum())
        {
            continue;
        }
        let chroma: Vec<String> = {
            let mut order: Vec<usize> = (0..12).collect();
            order.sort_by(|&a, &b| r.blind.chroma[b].total_cmp(&r.blind.chroma[a]));
            order
                .iter()
                .take(4)
                .map(|&i| format!("{} {:.0}%", NOTES[i], r.blind.chroma[i] * 100.0))
                .collect()
        };
        println!(
            "  {:<34} {:<10} {:<9} fit {:>5}  tonal {:.2}  top: {}",
            r.path.file_stem().unwrap().to_string_lossy(),
            r.blind.category.name(),
            r.blind
                .key
                .as_ref()
                .map_or("-".to_string(), |k| k.name.clone()),
            r.blind
                .key
                .as_ref()
                .map_or("-".to_string(), |k| format!("{:.2}", k.confidence)),
            r.blind.tonal,
            chroma.join(", ")
        );
    }
}

/// How often a one-shot's five nearest neighbours (by embedding) share its category family.
pub(crate) fn neighbours(rows: &[&Row]) {
    let family = |c: Category| match c {
        Category::HatOpen => Category::HatClosed,
        Category::Ride => Category::Crash,
        c => c,
    };
    let shots: Vec<&&Row> = rows
        .iter()
        .filter(|r| r.truth.kind == Some(Kind::OneShot) && !r.truth.cats.is_empty())
        .collect();
    let (mut shared, mut total) = (0, 0);
    let mut per: HashMap<Category, (usize, usize)> = HashMap::new();
    for a in &shots {
        let mut near: Vec<(f32, &&&Row)> = shots
            .iter()
            .filter(|b| b.path != a.path)
            .map(|b| {
                (
                    a.blind
                        .embedding
                        .iter()
                        .zip(&b.blind.embedding)
                        .map(|(x, y)| x * y)
                        .sum::<f32>(),
                    b,
                )
            })
            .collect();
        near.sort_by(|x, y| y.0.total_cmp(&x.0));
        let mine = family(a.truth.cats[0]);
        let e = per.entry(mine).or_default();
        for (_, b) in near.iter().take(5) {
            total += 1;
            e.1 += 1;
            if family(b.truth.cats[0]) == mine {
                shared += 1;
                e.0 += 1;
            }
        }
    }
    println!(
        "\nEmbedding: one-shots' 5 nearest neighbours sharing their category: {}",
        pct(shared, total)
    );
    let mut cats: Vec<_> = per.iter().collect();
    cats.sort_by_key(|c| Category::ALL.iter().position(|x| x == c.0));
    let parts: Vec<String> = cats
        .iter()
        .map(|(c, (n, d))| format!("{} {:.0}%", c.name(), 100.0 * *n as f32 / *d as f32))
        .collect();
    println!("  {}", parts.join(", "));
}

pub(crate) fn duplicates(rows: &[&Row]) {
    let mut by: HashMap<u64, Vec<&PathBuf>> = HashMap::new();
    for r in rows {
        by.entry(r.blind.fingerprint).or_default().push(&r.path);
    }
    let groups: Vec<&Vec<&PathBuf>> = by.values().filter(|g| g.len() > 1).collect();
    let mut same = 0;
    for g in &groups {
        let read = |p: &PathBuf| pina_engine::audio::engine::read_audio(p).ok().map(|a| a.0);
        let first = read(g[0]);
        if g[1..].iter().all(|p| read(p) == first) {
            same += 1;
        }
    }
    println!(
        "\nFingerprints: {} files, {} shared by more than one file ({} of them identical audio)",
        rows.len(),
        groups.len(),
        same
    );
    for g in groups.iter().take(10) {
        let names: Vec<String> = g
            .iter()
            .map(|p| {
                p.strip_prefix(p.ancestors().nth(3).unwrap())
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        println!("  {}", names.join(" = "));
    }
}

pub(crate) fn speed(rows: &[&Row]) {
    let mut shots: Vec<f32> = rows
        .iter()
        .filter(|r| r.blind.kind == Kind::OneShot && r.blind.seconds < 4.0)
        .map(|r| r.millis)
        .collect();
    let mut loops: Vec<f32> = rows
        .iter()
        .filter(|r| r.blind.kind == Kind::Loop)
        .map(|r| r.millis * 8.0 / r.blind.seconds.max(1.0))
        .collect();
    let stats = |v: &mut Vec<f32>| {
        v.sort_by(|a, b| a.total_cmp(b));
        if v.is_empty() {
            return "-".to_string();
        }
        format!(
            "median {:.1} ms, 95th {:.1} ms, max {:.1} ms",
            v[v.len() / 2],
            v[v.len() * 95 / 100],
            v[v.len() - 1]
        )
    };
    println!(
        "\nSpeed (reading and analysing, every core busy): one-shots {}; loops per 8 s {}",
        stats(&mut shots),
        stats(&mut loops)
    );
}

pub(crate) fn list(rows: &[&Row]) {
    for r in rows {
        let a = &r.blind;
        println!(
            "{:<60} truth {:<22} named {:<11} blind {:<11} {:.2} {:?} bpm {:?} bars {:?} pitch {} key {} tonal {:.2} traits {}",
            r.path
                .to_string_lossy()
                .rsplit('/')
                .take(2)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("/"),
            r.truth
                .cats
                .iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
                .join("|"),
            r.named.category.name(),
            a.category.name(),
            a.confidence,
            a.kind,
            a.bpm,
            a.bars,
            a.pitch.map_or("-".into(), |p| format!(
                "{}{:+.0}c({:.2})",
                note_name(p.note),
                p.cents,
                p.confidence
            )),
            a.key.as_ref().map_or("-".into(), |k| k.name.clone()),
            a.tonal,
            a.traits.join(",")
        );
    }
}
