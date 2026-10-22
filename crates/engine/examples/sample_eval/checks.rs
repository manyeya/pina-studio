//! Timing a scan, and checking key and tempo alone.

use super::*;

/// How long analysing takes on one core, file by file (read, then analyse).
pub(crate) fn time_check(files: &[PathBuf]) {
    let (mut shots, mut loops) = (Vec::new(), Vec::new());
    for path in files {
        let start = Instant::now();
        let Ok(a) = pina_engine::samples::analyze::analyze_file(path) else {
            continue;
        };
        let ms = start.elapsed().as_secs_f32() * 1000.0;
        match a.kind {
            Kind::OneShot if a.seconds < 4.0 => shots.push(ms),
            Kind::Loop => loops.push(ms * 8.0 / a.seconds.max(1.0)),
            _ => {}
        }
    }
    let stats = |v: &mut Vec<f32>| {
        v.sort_by(|a, b| a.total_cmp(b));
        if v.is_empty() {
            return "-".to_string();
        }
        format!(
            "median {:.1} ms, 95th {:.1} ms, max {:.1} ms ({} files)",
            v[v.len() / 2],
            v[v.len() * 95 / 100],
            v[v.len() - 1],
            v.len()
        )
    };
    println!(
        "one core: one-shots {}; loops per 8 s {}",
        stats(&mut shots),
        stats(&mut loops)
    );
}

/// Key alone, from the audio, for every file whose name gives a root: how often the tonic is
/// right, for a range of weights on the bass line and its start.
pub(crate) fn key_check(files: &[PathBuf]) {
    let labelled: Vec<(&PathBuf, Truth)> = files
        .iter()
        .map(|f| (f, truth(f)))
        .filter(|(_, t)| t.root.is_some())
        .collect();
    let next = AtomicUsize::new(0);
    let out = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..std::thread::available_parallelism().map_or(4, |n| n.get()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((path, t)) = labelled.get(i) else {
                        break;
                    };
                    let Ok((c, rate)) = pina_engine::audio::engine::read_audio(path) else {
                        continue;
                    };
                    let f = features::extract(&c, rate, None);
                    out.lock().unwrap().push((t.clone(), f.tonal));
                }
            });
        }
    });
    let out = out.into_inner().unwrap();
    // How far off the tonic is, when it is, and where the chroma's strongest class sits.
    let mut packs: Vec<String> = out.iter().map(|o| o.0.pack.clone()).collect();
    packs.sort();
    packs.dedup();
    for pack in &packs {
        let mut off = [0usize; 12];
        let mut top = [0usize; 12];
        let mut bass = [0usize; 12];
        for (t, tonal) in out.iter().filter(|o| &o.0.pack == pack) {
            let root = t.root.unwrap() as usize;
            if let Some((tonic, _, _)) = pina_engine::samples::tonal::key(tonal) {
                off[(tonic as usize + 12 - root) % 12] += 1;
            }
            let strongest = (0..12)
                .max_by(|&a, &b| tonal.chroma[a].total_cmp(&tonal.chroma[b]))
                .unwrap();
            top[(strongest + 12 - root) % 12] += 1;
            let low = (0..12)
                .max_by(|&a, &b| tonal.bass[a].total_cmp(&tonal.bass[b]))
                .unwrap();
            bass[(low + 12 - root) % 12] += 1;
        }
        println!(
            "{pack}: key-root {off:?}\n    strongest class-root {top:?}\n    strongest bass-root {bass:?}"
        );
    }
    let chords = out.iter().filter(|(t, _)| t.kind == Some(Kind::OneShot));
    let (n, right) = chords.fold((0, 0), |(n, r), (t, tonal)| {
        (
            n + 1,
            r + (pina_engine::samples::tonal::chord(tonal).map(|c| c.0) == t.root) as usize,
        )
    });
    println!("chord roots of one-shots: {}", pct(right, n));
    for (bass, first, root) in [
        (1.0, 0.5, 0.0),
        (1.0, 0.5, 0.5),
        (1.0, 0.5, 1.0),
        (1.0, 0.5, 2.0),
        (0.5, 0.5, 1.0),
        (0.0, 0.0, 1.0),
        (0.0, 0.0, 2.0),
        (1.0, 1.0, 1.0),
    ] {
        {
            let mut by: HashMap<String, (usize, usize, usize)> = HashMap::new();
            for (t, tonal) in &out {
                let e = by.entry(t.pack.clone()).or_default();
                e.2 += 1;
                if let Some((tonic, minor, _)) =
                    pina_engine::samples::tonal::key_weighted(tonal, bass, first, root)
                    && Some(tonic) == t.root
                {
                    e.0 += 1;
                    if t.minor == Some(minor) {
                        e.1 += 1;
                    }
                }
            }
            let mut packs: Vec<_> = by.into_iter().collect();
            packs.sort();
            let all = packs
                .iter()
                .fold((0, 0), |a, (_, e)| (a.0 + e.0, a.1 + e.2));
            let parts: Vec<String> = packs
                .iter()
                .map(|(p, e)| format!("{p} {:.0}%", 100.0 * e.0 as f32 / e.2 as f32))
                .collect();
            println!(
                "bass {bass} first {first} root {root}: {}  [{}]",
                pct(all.0, all.1),
                parts.join(", ")
            );
        }
    }
}

/// Tempo alone, from the audio, for every loop whose name gives one: right, half or double, or
/// other, pack by pack.
pub(crate) fn tempo_check(files: &[PathBuf]) {
    let loops: Vec<(&PathBuf, Truth)> = files
        .iter()
        .map(|f| (f, truth(f)))
        .filter(|(_, t)| t.bpm.is_some() && t.kind == Some(Kind::Loop))
        .collect();
    let next = AtomicUsize::new(0);
    let out = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..std::thread::available_parallelism().map_or(4, |n| n.get()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((path, t)) = loops.get(i) else { break };
                    let Ok((c, rate)) = pina_engine::audio::engine::read_audio(path) else {
                        continue;
                    };
                    let f = features::extract(&c, rate, None);
                    out.lock().unwrap().push((
                        path.to_string_lossy().to_string(),
                        t.pack.clone(),
                        t.bpm.unwrap(),
                        f.tempo.map(|t| t.bpm),
                        f.seconds,
                    ));
                }
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    let mut by: HashMap<String, [usize; 4]> = HashMap::new();
    for (path, pack, want, got, seconds) in &out {
        let e = by.entry(pack.clone()).or_default();
        e[3] += 1;
        let near = |k: f32| got.is_some_and(|g| ((g - want * k) / (want * k)).abs() < 0.02);
        if near(1.0) {
            e[0] += 1;
        } else if near(0.5) || near(2.0) {
            e[1] += 1;
            println!("  half/double {want} -> {got:?} ({seconds:.2}s) {path}");
        } else {
            e[2] += 1;
            println!("  wrong {want} -> {got:?} ({seconds:.2}s) {path}");
        }
    }
    let mut packs: Vec<_> = by.into_iter().collect();
    packs.sort();
    let mut all = [0usize; 4];
    for (pack, e) in packs {
        println!(
            "{pack:<16} right {:>16}  half/double {:>14}  other {}",
            pct(e[0], e[3]),
            pct(e[1], e[3]),
            e[2]
        );
        for i in 0..4 {
            all[i] += e[i];
        }
    }
    println!(
        "{:<16} right {:>16}  half/double {:>14}  other {}",
        "all",
        pct(all[0], all[3]),
        pct(all[1], all[3]),
        all[2]
    );
}
