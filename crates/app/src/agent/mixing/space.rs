//! Real space: the song's reverb and delay, on auxes, from what's installed.

use super::*;

/// A plugin installed, by its name, in the format it plays best in here.
pub(crate) fn installed(plugins: &[scan::PluginInfo], class: &str) -> Option<PluginRef> {
    let rank = |f: Format| match f {
        Format::Clap => 0,
        Format::Vst3 => 1,
        Format::Au => 2,
        Format::Vst2 => 3,
    };
    plugins
        .iter()
        .filter(|p| p.class == class)
        .min_by_key(|p| rank(p.format))
        .map(|p| p.plugin_ref())
}

pub(crate) fn au(class: &str, uid: &str) -> PluginRef {
    PluginRef {
        format: Format::Au,
        class: class.into(),
        uid: Some(uid.into()),
        ..Default::default()
    }
}

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

/// The song's reverb, "Verb": Dragonfly's plate (its hall for big, slow genres) if it's here,
/// else on macOS Apple's; fully wet, its pre-delay a sixty-fourth note, its decay by genre, its
/// lows and highs cut so it doesn't cloud the mix. None where there's no reverb to use.
pub(crate) fn reverb(
    dir: &Path,
    first: usize,
    plugins: &[scan::PluginInfo],
    bpm: f64,
    genre: &str,
) -> Result<Option<(u32, String)>, String> {
    let big = matches!(genre, "cinematic" | "ambient");
    let hall = installed(plugins, "Dragonfly Hall Reverb").filter(|_| big);
    let plate = installed(plugins, "Dragonfly Plate Reverb");
    if hall.is_none() && plate.is_none() && !cfg!(target_os = "macos") {
        return Ok(None);
    }
    let (i, bus) = aux(dir, first, "Verb")?;
    let ch = json!({ "aux": i });
    let predelay = (60_000.0 / bpm / 16.0).clamp(8.0, 40.0);
    let decay = match genre {
        "cinematic" | "ambient" => 3.0,
        "rnb" | "neo_soul" | "pop" => 1.8,
        "lofi" | "boom_bap" => 1.6,
        "trap" | "drill" => 1.4,
        "dnb" => 1.2,
        "house" | "deep_house" | "techno" | "edm" | "amapiano" | "afrobeats" | "reggaeton" => 1.3,
        _ => 1.6,
    };
    let set = |name: &str, text: String| {
        run(
            dir,
            json!({ "op": "set_plugin_param", "channel": ch, "plugin": { "insert": 0 }, "param": name, "text": text }),
        )
    };
    let which = if let Some(h) = hall {
        run(
            dir,
            json!({ "op": "set_insert", "channel": ch, "index": 0, "plugin": h }),
        )?;
        set("Dry Level", "0".into())?;
        set("Early Level", "15".into())?;
        set("Late Level", "100".into())?;
        set("Size", "40".into())?;
        set("Predelay", format!("{predelay:.0}"))?;
        set("Decay", format!("{decay:.1}"))?;
        set("Low Cut", "200".into())?;
        set("High Cut", "8000".into())?;
        "Dragonfly Hall Reverb"
    } else if let Some(pl) = plate {
        run(
            dir,
            json!({ "op": "set_insert", "channel": ch, "index": 0, "plugin": pl }),
        )?;
        set("Dry Level", "0".into())?;
        set("Wet Level", "100".into())?;
        set("Predelay", format!("{predelay:.0}"))?;
        set("Decay", format!("{decay:.1}"))?;
        // Its low cut stops at 200 Hz; the aux's EQ cuts the rest.
        set("Low Cut", "200".into())?;
        set("High Cut", "9000".into())?;
        set("Dampen", "6000".into())?;
        "Dragonfly Plate Reverb"
    } else {
        run(
            dir,
            json!({ "op": "set_insert", "channel": ch, "index": 0, "plugin": au("AUReverb2", "aufx rvb2 appl") }),
        )?;
        set("Dry/Wet Mix", "100".into())?;
        set("Low Freq Decay Time", format!("{:.1}", decay * 1.1))?;
        set("High Freq Decay Time", format!("{:.1}", decay * 0.6))?;
        "AUReverb2"
    };
    run(
        dir,
        json!({ "op": "set_eq", "channel": ch, "eq": { "bands": [
            { "shape": "low_cut", "freq": 250, "slope": 24 },
            { "shape": "high_shelf", "freq": 8000, "gain_db": -3 },
        ] } }),
    )?;
    Ok(Some((
        bus,
        format!("{which}, {predelay:.0} ms pre-delay, {decay:.1} s decay"),
    )))
}

/// The song's delay, "Echo": Apple's, a dotted eighth (an eighth under 90 BPM), its repeats dark
/// and thin so they sit behind what they repeat. None away from macOS, where there's no Apple's.
pub(crate) fn echo(dir: &Path, first: usize, bpm: f64) -> Result<Option<(u32, String)>, String> {
    if !cfg!(target_os = "macos") {
        return Ok(None);
    }
    let (i, bus) = aux(dir, first, "Echo")?;
    let ch = json!({ "aux": i });
    let beats = if bpm < 90.0 { 0.5 } else { 0.75 };
    let seconds = beats * 60.0 / bpm;
    run(
        dir,
        json!({ "op": "set_insert", "channel": ch, "index": 0, "plugin": au("AUDelay", "aufx dely appl") }),
    )?;
    for (name, text) in [
        ("Dry/Wet Mix", "100".to_string()),
        ("Delay Time", format!("{seconds:.3}")),
        ("Feedback", "30".to_string()),
        ("Lowpass Cutoff Frequency", "6000".to_string()),
    ] {
        run(
            dir,
            json!({ "op": "set_plugin_param", "channel": ch, "plugin": { "insert": 0 }, "param": name, "text": text }),
        )?;
    }
    run(
        dir,
        json!({ "op": "set_eq", "channel": ch, "eq": { "bands": [
            { "shape": "low_cut", "freq": 300, "slope": 24 },
            { "shape": "high_cut", "freq": 7000 },
        ] } }),
    )?;
    let note = if beats == 0.5 {
        "an eighth"
    } else {
        "a dotted eighth"
    };
    Ok(Some((
        bus,
        format!("Apple's delay, {note} ({:.0} ms)", seconds * 1000.0),
    )))
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
