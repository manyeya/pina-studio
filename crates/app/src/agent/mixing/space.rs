//! Real space: the song's reverb and delay, Pina Studio's own, on auxes tracks send to.

use super::*;

/// An aux by name, made if there's none (sent to from `first` to make it); its index and bus.
pub(crate) fn aux(dir: &Path, first: usize, name: &str) -> Result<(usize, u32), String> {
    let p = cli::project(dir)?;
    if let Some(i) = p.auxes.iter().position(|a| a.name == name) {
        return Ok((i, p.auxes[i].bus));
    }
    let made = run(
        dir,
        json!({ "op": "add_send", "track": first, "level_db": -60 }),
    )?;
    let bus = made["bus"].as_u64().ok_or("no bus made")? as u32;
    let p = cli::project(dir)?;
    let i = p
        .auxes
        .iter()
        .position(|a| a.bus == bus)
        .ok_or("no aux made")?;
    run(
        dir,
        json!({ "op": "set_aux", "aux": i, "name": name, "volume_db": 0 }),
    )?;
    Ok((i, bus))
}

/// The song's reverb, "Verb": Pina Studio's own on an aux, fully wet. Its kind by genre (a
/// plate for voices in pop, R&B and trap, a warm chamber for lo-fi and jazz, a hall, a cathedral
/// for ambient), its pre-delay a sixty-fourth note, its decay by genre, its lows and highs cut so
/// it doesn't cloud the mix, and (where a voice or lead must stay clear) ducking under what's
/// sent to it. A reverb plugin an earlier mix put there makes way.
pub(crate) fn reverb(
    dir: &Path,
    first: usize,
    bpm: f64,
    genre: &str,
) -> Result<(u32, String), String> {
    let (i, bus) = aux(dir, first, "Verb")?;
    let ch = json!({ "aux": i });
    clear_plugins(dir, i, "reverb")?;
    let style = match genre {
        "ambient" => "cathedral",
        "cinematic" => "hall",
        "rnb" | "neo_soul" | "pop" | "gospel" | "trap" | "drill" | "dnb" => "plate",
        "lofi" | "boom_bap" | "jazz" | "soul" | "funk" => "chamber",
        "house" | "deep_house" | "techno" | "edm" | "amapiano" | "afrobeats" | "afro_house"
        | "reggaeton" | "dancehall" | "garage" => "room",
        _ => "hall",
    };
    let decay = match genre {
        "ambient" => 5.5,
        "cinematic" => 3.0,
        "rnb" | "neo_soul" | "pop" | "gospel" => 1.8,
        "lofi" | "boom_bap" => 1.6,
        "trap" | "drill" => 1.4,
        "dnb" => 1.2,
        "house" | "deep_house" | "techno" | "edm" | "amapiano" | "afrobeats" | "reggaeton" => 1.3,
        _ => 1.6,
    };
    // A sixty-fourth note, if that's a usual pre-delay at this tempo; else held to one.
    let sixty_fourth = 60_000.0 / bpm / 16.0;
    let predelay = match (8.0..=40.0).contains(&sixty_fourth) {
        true => json!({ "predelay_beats": 0.0625 }),
        false => json!({ "predelay_ms": sixty_fourth.clamp(8.0, 40.0).round() }),
    };
    let ducking = match genre {
        "rnb" | "neo_soul" | "pop" | "trap" | "drill" | "house" | "edm" => 0.3,
        _ => 0.0,
    };
    let mut reverb = json!({ "style": style, "decay_s": decay, "mix": 1, "low_cut_hz": 250,
        "high_cut_hz": 9000, "ducking": ducking });
    for (k, v) in predelay.as_object().into_iter().flatten() {
        reverb[k] = v.clone();
    }
    run(
        dir,
        json!({ "op": "set_reverb", "channel": ch, "reverb": reverb }),
    )?;
    Ok((
        bus,
        format!(
            "Pina Studio's {style} reverb, {:.0} ms pre-delay, {decay:.1} s decay{}",
            sixty_fourth.clamp(8.0, 40.0),
            if ducking > 0.0 {
                ", ducking under what's sent"
            } else {
                ""
            }
        ),
    ))
}

/// The song's delay, "Echo": Pina Studio's own on an aux, fully wet: a dotted eighth (an eighth
/// under 90 BPM), its repeats thin, dark and ducked under what's sent, so they sit behind what
/// they repeat; tape in lo-fi, bouncing in trap and dance music, dub in reggae.
pub(crate) fn echo(
    dir: &Path,
    first: usize,
    bpm: f64,
    genre: &str,
) -> Result<(u32, String), String> {
    let (i, bus) = aux(dir, first, "Echo")?;
    let ch = json!({ "aux": i });
    clear_plugins(dir, i, "delay")?;
    let beats = if bpm < 90.0 { 0.5 } else { 0.75 };
    let style = match genre {
        "lofi" | "boom_bap" | "neo_soul" | "jazz" => "tape",
        "trap" | "drill" | "house" | "deep_house" | "techno" | "edm" | "afro_house" => "ping_pong",
        "reggae" | "dancehall" | "dub" => "dub",
        _ => "digital",
    };
    run(
        dir,
        json!({ "op": "set_delay", "channel": ch, "delay": { "style": style, "sync": true,
            "beats": [beats, beats], "feedback": 0.3, "low_cut_hz": 300, "high_cut_hz": 6500,
            "ducking": 0.35, "mix": 1 } }),
    )?;
    let note = if beats == 0.5 {
        "an eighth"
    } else {
        "a dotted eighth"
    };
    Ok((
        bus,
        format!(
            "Pina Studio's {} delay, {note} ({:.0} ms), ducked under what's sent",
            style.replace('_', "-"),
            beats * 60_000.0 / bpm
        ),
    ))
}

/// Takes off an aux the plugins (of a `kind`: "reverb", "delay") an earlier mix put there, and
/// the EQ it shaped them with: the built-in does it now.
fn clear_plugins(dir: &Path, aux: usize, kind: &str) -> Result<(), String> {
    let p = cli::project(dir)?;
    let old: Vec<usize> = p.auxes[aux]
        .inserts
        .iter()
        .enumerate()
        .filter(|(_, r)| r.class.to_lowercase().contains(kind))
        .map(|(k, _)| k)
        .collect();
    for k in old.into_iter().rev() {
        run(
            dir,
            json!({ "op": "set_insert", "channel": { "aux": aux }, "index": k }),
        )?;
    }
    run(dir, json!({ "op": "set_eq", "channel": { "aux": aux } }))?;
    Ok(())
}

/// Sends `t` to `bus` at `db`, or sets the send it has there.
pub(crate) fn send(dir: &Path, t: usize, bus: u32, db: f32) -> Result<(), String> {
    let p = cli::project(dir)?;
    match p.tracks[t].sends.iter().position(|s| s.bus == bus) {
        Some(s) => run(
            dir,
            json!({ "op": "set_send", "track": t, "send": s, "level_db": db }),
        ),
        None => run(
            dir,
            json!({ "op": "add_send", "track": t, "bus": bus, "level_db": db }),
        ),
    }
    .map(drop)
}
