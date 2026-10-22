//! The mix, stage by stage, each measured as it goes.

use super::*;

/// Mixes tracks by measuring them (all with regions, or `only`), `as_role` for the part if
/// given; with `levels`, each one's fader to where its part sits against the drums.
pub fn mix(
    dir: &Path,
    only: Option<usize>,
    as_role: Option<&str>,
    levels: bool,
) -> Result<Value, String> {
    let p = cli::project(dir)?;
    let tracks: Vec<usize> = match only {
        Some(t) if t < p.tracks.len() => vec![t],
        Some(t) => return Err(format!("no track {t}")),
        None => (0..p.tracks.len())
            .filter(|&t| !p.tracks[t].regions.is_empty())
            .collect(),
    };
    if tracks.is_empty() {
        return Err("no tracks with anything on them to mix".into());
    }
    let genre = p.genre.clone().unwrap_or_default();
    let lofi = matches!(genre.as_str(), "lofi" | "boom_bap" | "neo_soul");
    let sub_led = matches!(genre.as_str(), "trap" | "drill" | "amapiano" | "reggaeton");
    let rate = p.sample_rate as f32;
    let beat = (rate as f64 * 60.0 / p.tempo) as usize;
    let part_of = |t: usize| as_role.unwrap_or_else(|| role(&p.tracks[t]));
    // Each track's own sound: its channel's EQ, compressor, saturator and sends taken off (they're
    // what this sets), its plugins kept.
    // Measured at its fader's 0 dB, past no master, as its compressor and anything hearing it
    // as a sidechain hear it (the fader is added back where it counts).
    let mut bare = p.clone();
    for t in &mut bare.tracks {
        (t.eq, t.comp, t.sat) = (None, None, None);
        t.sends.clear();
        (t.volume_db, t.pan) = (0.0, 0.0);
        t.automation.volume.clear();
    }
    bare.master_inserts.clear();
    (bare.master_eq, bare.master_comp, bare.master_sat) = (None, None, None);
    bare.master_volume_db = 0.0;
    let all: Vec<usize> = (0..p.tracks.len())
        .filter(|&t| !p.tracks[t].regions.is_empty())
        .collect();
    let mut sources: Vec<Option<Source>> = (0..p.tracks.len()).map(|_| None).collect();
    for &t in &all {
        let Some(range) = crate::agent::listen::span(&p, t) else {
            continue;
        };
        let render = cli::rendered(&cli::alone(&bare, t), dir, Some(range))?;
        let a = analysis::analyze(&render[0], &render[1], rate as f64);
        let s = &a.spectrum_db;
        sources[t] = Some(Source {
            lufs: a.lufs,
            bands: [s.sub, s.bass, s.low_mid, s.mid, s.presence, s.air],
            width_db: a.stereo.width_db,
            lowest_hz: lowest_hz(&p, t),
            render,
        });
    }
    // The kick: of the drums and percussion, what thumps hardest down low (a kick, a taiko, not
    // a snare), if anything does; where its thump is and how hard it hits.
    let kick = all
        .iter()
        .copied()
        .filter(|&t| matches!(part_of(t), "drums" | "perc"))
        .filter_map(|d| {
            let s = sources[d].as_ref()?;
            Some((
                d,
                band_peak_db(&s.render, rate, beat, [30.0, 110.0], (3.0, 100.0))?,
            ))
        })
        .filter(|(_, (peak, _))| *peak > -30.0)
        .max_by(|a, b| a.1.0.total_cmp(&b.1.0))
        .map(|(d, (peak, floor))| {
            (
                d,
                kick_hz(&sources[d].as_ref().unwrap().render, rate),
                peak,
                floor,
            )
        });
    // The focal part: the first lead or vocal, and how hard it hits in the presence range.
    let lead = all
        .iter()
        .copied()
        .find(|&t| matches!(part_of(t), "vocals" | "lead"))
        .and_then(|l| {
            let s = sources[l].as_ref()?;
            Some((
                l,
                band_peak_db(&s.render, rate, beat, [1500.0, 4500.0], (15.0, 200.0))?.0,
            ))
        });
    let has_bass = all.iter().any(|&t| part_of(t) == "bass");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let plugins = scan::scan(&exe, &scan::everything(), &scan::cache_file());
    let first = tracks[0];
    let (verb_bus, verb_said) = reverb(dir, first, &plugins, p.tempo, &genre)?;
    let wants_echo = tracks
        .iter()
        .any(|&t| sends(part_of(t), &genre).1.is_some());
    let echo = match wants_echo {
        true => Some(echo(dir, first, p.tempo)?),
        false => None,
    };
    let mut done = Vec::new();
    let mut side = 1.0f32;
    // Bands hearing another track: (track, band, source, what they listen to, whether they let
    // go between its hits, range). Their thresholds are set last, from the source as they hear
    // it: through its own channel (a kick's compressor flattens it) and fader.
    let mut hearing: Vec<Hearing> = Vec::new();
    for &t in &tracks {
        let part = part_of(t);
        let Some(src) = sources[t].as_ref() else {
            done.push(
                json!({ "track": t, "name": p.tracks[t].name, "part": part, "skipped": "silent" }),
            );
            continue;
        };
        let mut bands = Vec::new();
        let mut why: Vec<String> = Vec::new();
        // Low cut: the part's, never into its lowest note.
        // What thumps hardest down low (a taiko, as much as a kick) keeps its lows.
        let thump = kick.is_some_and(|k| k.0 == t);
        let mut cut = if thump { 25.0 } else { low_cut(part) };
        if let Some(hz) = src
            .lowest_hz
            .filter(|_| !matches!(part, "bass" | "drums") && !thump)
        {
            // Room under the rest for a bass, if there's one (and more where an 808 carries the
            // song); without one, low strings or a low piano are the bass.
            let floor: f32 = match (has_bass, sub_led) {
                (true, true) => 120.0,
                (true, false) => 80.0,
                (false, _) => 30.0,
            };
            cut = cut.min(hz * 0.7).max(floor.min(cut));
        }
        bands.push(json!({ "shape": "low_cut", "freq": cut.round(), "slope": if part == "bass" { 24 } else { 12 } }));
        why.push(format!("low cut {cut:.0} Hz"));
        // The low end mono: below where it's felt, the sides cut.
        match part {
            "bass" => {
                bands.push(
                    json!({ "shape": "low_cut", "freq": 200, "slope": 24, "placement": "side" }),
                );
                why.push("mono under 200 Hz".into());
            }
            "drums" | "perc" | "fx" => {}
            _ if src.width_db > -25.0 => {
                bands.push(
                    json!({ "shape": "low_cut", "freq": 150, "slope": 12, "placement": "side" }),
                );
                why.push("mono under 150 Hz".into());
            }
            _ => {}
        }
        let (mud_at, bite_at) = limits(part);
        let [sub, low, mud, mid, bite, air] = src.bands;
        if mud > mud_at {
            let cut = ((mud - mud_at) * 0.8).clamp(1.5, 4.0);
            bands.push(json!({ "shape": "bell", "freq": 300, "gain_db": -cut, "q": 1.0 }));
            why.push(format!(
                "mud -{cut:.1} dB at 300 Hz ({mud:.0} dB of it there)"
            ));
        }
        if bite > bite_at + 2.0 {
            let range = ((bite - bite_at) * 0.8).clamp(2.0, 5.0);
            let spectral = matches!(part, "lead" | "vocals");
            bands.push(
                json!({ "shape": "bell", "freq": 5000, "gain_db": 0, "q": 1.2,
                "dynamic": { "range_db": -range, "spectral": spectral } }),
            );
            why.push(format!(
                "{} harshness by up to {range:.1} dB at 5 kHz, when it's there",
                if spectral { "tames" } else { "dips" }
            ));
        }
        match part {
            "lead" | "vocals" | "chords" | "arp" if air < -30.0 => {
                bands.push(json!({ "shape": "high_shelf", "freq": 10000, "gain_db": 1.5 }));
                why.push("air +1.5 dB".into());
            }
            "pad" | "texture" | "bass" if air > -20.0 => {
                bands.push(json!({ "shape": "high_shelf", "freq": 9000, "gain_db": -2 }));
                why.push("top -2 dB, out of the lead's way".into());
            }
            _ => {}
        }
        if matches!(part, "lead" | "vocals") && mid < -10.0 {
            bands.push(json!({ "shape": "bell", "freq": 3000, "gain_db": 1.5, "q": 0.8 }));
            why.push("presence +1.5 dB".into());
        }
        if part == "bass" && sub > -3.0 && low < -6.0 {
            bands.push(json!({ "shape": "bell", "freq": 100, "gain_db": 2, "q": 0.8 }));
            why.push("body +2 dB at 100 Hz: it's nearly all sub".into());
        }
        // Room for the others: what this track's dynamic bands hear (one track per EQ).
        let mut sidechain = None;
        let pump = pumps(&genre) && matches!(part, "bass" | "pad" | "chords" | "arp" | "counter");
        if let Some((d, hz, peak, floor)) = kick.filter(|k| k.0 != t) {
            // Let go within the beat: a quarter of it for the pump, an eighth for the carve.
            let quarter = (60_000.0 / p.tempo * 0.25).round() as f32;
            if pump {
                let depth = match part {
                    "bass" => 6.0,
                    "pad" => 8.0,
                    "chords" => 5.0,
                    _ => 4.0,
                };
                hearing.push(Hearing {
                    track: t,
                    band: bands.len(),
                    source: d,
                    listens: [30.0, 110.0],
                    times: (3.0, quarter),
                    lets_go: true,
                    range: depth,
                });
                bands.push(json!({ "shape": "low_shelf", "freq": 16000, "gain_db": 0,
                    "dynamic": { "range_db": -depth, "threshold_db": under(peak, Some(floor), depth), "external": true,
                        "trigger": [30, 110], "attack_ms": 3, "release_ms": quarter } }));
                why.push(format!(
                    "ducks {depth:.0} dB under the kick ({} pumps)",
                    genre
                ));
                sidechain = Some(d);
            } else if part == "bass" {
                let eighth = (quarter * 0.5).round();
                hearing.push(Hearing {
                    track: t,
                    band: bands.len(),
                    source: d,
                    listens: [hz * 0.7, hz * 1.4],
                    times: (2.0, eighth),
                    lets_go: true,
                    range: 6.0,
                });
                bands.push(json!({ "shape": "bell", "freq": hz, "gain_db": 0, "q": 1.0,
                    "dynamic": { "range_db": -6, "threshold_db": under(peak, Some(floor), 6.0), "external": true,
                        "trigger": [hz * 0.7, hz * 1.4], "attack_ms": 2, "release_ms": eighth } }));
                why.push(format!(
                    "makes room for the kick: -6 dB at {hz:.0} Hz as it hits"
                ));
                sidechain = Some(d);
            }
        }
        if sidechain.is_none()
            && let Some((l, peak)) = lead.filter(|l| l.0 != t)
            && matches!(part, "chords" | "pad" | "arp" | "counter")
        {
            hearing.push(Hearing {
                track: t,
                band: bands.len(),
                source: l,
                listens: [1500.0, 4500.0],
                times: (15.0, 200.0),
                lets_go: false,
                range: 3.0,
            });
            bands.push(json!({ "shape": "bell", "freq": 2500, "gain_db": 0, "q": 0.9,
                "dynamic": { "range_db": -3, "threshold_db": under(peak, None, 3.0), "external": true,
                    "trigger": [1500, 4500], "attack_ms": 15, "release_ms": 200 } }));
            why.push(format!(
                "dips 3 dB at 2.5 kHz while {} plays",
                p.tracks[l].name
            ));
            sidechain = Some(l);
        }
        let mut eq = json!({ "bands": bands });
        if let Some(s) = sidechain {
            eq["sidechain"] = json!(s);
        }
        run(
            dir,
            json!({ "op": "set_eq", "channel": { "track": t }, "eq": eq }),
        )?;
        // Compression from where it plays, so a quiet part is compressed as much as a loud one.
        let comp = comp(part).map(|(over, ratio, attack, release)| {
            json!({ "threshold_db": (src.lufs + over).clamp(-60.0, 0.0).round(), "ratio": ratio,
                "attack_ms": attack, "release_ms": release, "makeup_db": 0, "rms": part == "pad" })
        });
        run(
            dir,
            json!({ "op": "set_comp", "channel": { "track": t }, "comp": comp }),
        )?;
        let sat = sat(part).map(|(drive, tone, mix)| {
            json!({ "drive_db": if lofi { drive + 3.0 } else { drive }, "tone": tone, "mix": mix })
        });
        run(
            dir,
            json!({ "op": "set_saturator", "channel": { "track": t }, "sat": sat }),
        )?;
        // Side by side: the parts that aren't the middle's take turns left and right.
        let pan = match part {
            "perc" => 0.35,
            "counter" | "arp" => 0.35,
            _ => 0.0,
        };
        let pan = if pan != 0.0 {
            side = -side;
            pan * side
        } else {
            0.0
        };
        run(dir, json!({ "op": "set_track", "track": t, "pan": pan }))?;
        let (verb, delay) = sends(part, &genre);
        if let Some(db) = verb {
            send(dir, t, verb_bus, db)?;
        }
        if let (Some(db), Some((bus, _))) = (delay, &echo) {
            send(dir, t, *bus, db)?;
        }
        done.push(
            json!({ "track": t, "name": p.tracks[t].name, "part": part, "eq": why,
            "verb_db": verb, "echo_db": delay.filter(|_| echo.is_some()) }),
        );
    }
    // The drums on a bus of their own, glued.
    let kit: Vec<usize> = tracks
        .iter()
        .copied()
        .filter(|&t| matches!(part_of(t), "drums" | "perc"))
        .collect();
    let mut bus_said = None;
    if kit.len() >= 2 {
        let now = cli::project(dir)?;
        let stack = match now.auxes.iter().position(|a| a.name == "Drums" && a.stack) {
            Some(i) => {
                for &t in &kit {
                    run(
                        dir,
                        json!({ "op": "set_output", "track": t, "bus": now.auxes[i].bus }),
                    )?;
                }
                i
            }
            None => {
                let made = run(
                    dir,
                    json!({ "op": "add_stack", "tracks": kit, "name": "Drums" }),
                )?;
                // The kit's tracks were moved together: what's been said about each, to where
                // it is now.
                let moved: Vec<usize> = serde_json::from_value(made["moved"].clone())
                    .map_err(|e| format!("add_stack: {e}"))?;
                for d in done.iter_mut() {
                    if let Some(t) = d["track"].as_u64().and_then(|t| moved.get(t as usize)) {
                        d["track"] = json!(t);
                    }
                }
                let bus = made["bus"].as_u64().ok_or("no stack made")? as u32;
                cli::project(dir)?
                    .auxes
                    .iter()
                    .position(|a| a.bus == bus)
                    .ok_or("no stack made")?
            }
        };
        let loudest = kit
            .iter()
            .filter_map(|&t| sources[t].as_ref().map(|s| s.lufs))
            .fold(-70f32, f32::max);
        run(
            dir,
            json!({ "op": "set_comp", "channel": { "aux": stack }, "comp": {
            "threshold_db": (loudest + 6.0).clamp(-60.0, 0.0).round(), "ratio": 2, "attack_ms": 30,
            "release_ms": 120, "makeup_db": 0 } }),
        )?;
        bus_said = Some(format!(
            "{} drum tracks on a Drums bus, glued (2:1, 30 ms attack)",
            kit.len()
        ));
    }
    // Levels: each part alone, through what this set, moved to its part's loudness.
    if levels {
        let p = cli::project(dir)?;
        let mut unmastered = p.clone();
        unmastered.master_inserts.clear();
        (
            unmastered.master_eq,
            unmastered.master_comp,
            unmastered.master_sat,
        ) = (None, None, None);
        unmastered.master_volume_db = 0.0;
        let report = cli::report(&unmastered, dir, None, true)?;
        for d in done.iter_mut() {
            let Some(t) = d["track"].as_u64().map(|t| t as usize) else {
                continue;
            };
            if d.get("skipped").is_some() {
                continue;
            }
            // As the song is now (a stack may have moved it).
            let part = as_role.unwrap_or_else(|| role(&p.tracks[t]));
            let a = &report.tracks[t];
            let measured = if part == "bass" {
                (a.rms_db[0] + a.rms_db[1]) / 2.0
            } else {
                a.lufs
            };
            if measured < -60.0 {
                d["level"] = json!("silent: no level set");
                continue;
            }
            let under = if sub_led && matches!(part, "lead" | "chords" | "pad" | "counter" | "arp")
            {
                3.0
            } else {
                0.0
            };
            let wanted = p.tracks[t].volume_db + level(part) - under - measured;
            let volume = (wanted.clamp(-30.0, 12.0) as f64 * 10.0).round() as f32 / 10.0;
            run(
                dir,
                json!({ "op": "set_track", "track": t, "volume_db": volume }),
            )?;
            // Its level automation (a section's ups and downs) moves with it: it's what plays.
            let moved = volume - p.tracks[t].volume_db;
            let auto = &p.tracks[t].automation.volume;
            if !auto.is_empty() && moved.abs() > 0.05 {
                let points = auto
                    .iter()
                    .map(|x| PointIn {
                        tick: x.tick,
                        value: Some(x.value + moved),
                        text: None,
                        curve: Some(x.curve),
                        step: Some(x.step),
                        ease: Some(x.ease),
                    })
                    .collect();
                cli::edit_with(
                    dir,
                    Command::SetAutomation {
                        track: t,
                        param: AutoParam::Volume,
                        send: None,
                        plugin: None,
                        id: None,
                        name: None,
                        modulator: None,
                        setting: None,
                        points,
                    },
                )?;
            }
            d["volume_db"] = json!((volume as f64 * 10.0).round() / 10.0);
            if wanted > 12.0 {
                d["level"] = json!(format!(
                    "needed {wanted:.0} dB: its sound is too quiet; turn it up at its source and mix again"
                ));
            }
        }
    }
    // What each band hearing another track hears: that track's own output as it's now set
    // (through its channel and fader, not its sends, bus or the master).
    let now = cli::project(dir)?;
    let mut tap = now.clone();
    for t in &mut tap.tracks {
        t.sends.clear();
        t.output = None;
    }
    tap.master_inserts.clear();
    (tap.master_eq, tap.master_comp, tap.master_sat) = (None, None, None);
    tap.master_volume_db = 0.0;
    let mut heard: Vec<(usize, [Vec<f32>; 2])> = Vec::new();
    for Hearing {
        track: t,
        band,
        source,
        listens,
        times,
        lets_go,
        range,
    } in hearing
    {
        if !heard.iter().any(|(s, _)| *s == source) {
            let Some(span) = crate::agent::listen::span(&tap, source) else {
                continue;
            };
            heard.push((
                source,
                cli::rendered(&cli::alone(&tap, source), dir, Some(span))?,
            ));
        }
        let render = &heard.iter().find(|(s, _)| *s == source).unwrap().1;
        let Some((peak, floor)) = band_peak_db(render, rate, beat, listens, times) else {
            continue;
        };
        let threshold = under(peak, lets_go.then_some(floor), range);
        run(
            dir,
            json!({ "op": "set_eq_band", "channel": { "track": t }, "band": band,
                "dynamic": { "threshold_db": (threshold as f64 * 10.0).round() / 10.0 } }),
        )?;
    }
    Ok(json!({
        "mixed": done,
        "reverb": verb_said,
        "delay": echo.map(|e| e.1),
        "drum_bus": bus_said,
        "kick": kick.map(|(d, hz, peak, floor)| json!({ "track": d, "hz": hz,
            "peak_db": (peak as f64 * 10.0).round() / 10.0, "before_next_db": (floor as f64 * 10.0).round() / 10.0 })),
        "next": "listen and critique to check what's left; master when it's right",
    }))
}
