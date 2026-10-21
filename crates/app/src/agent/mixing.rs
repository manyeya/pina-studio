//! Mixing by measurement. Each track's own sound is rendered and measured first (its spectrum,
//! loudness, width, its lowest note), and its channel set from that: a low cut under its lowest
//! note, mud and harshness cut only as far as it has them, air where it lacks it, its low end
//! kept mono. Parts make room for each other: the bass ducks at the kick's own frequency as it
//! hits, the chords and pads dip their presence while the lead plays (dynamic EQ bands hearing
//! the other track), and in genres that pump, the music ducks under the kick. Compression is set
//! from each part's level, not a fixed threshold. A reverb (Dragonfly's, if it's here) with its
//! pre-delay and decay set from the tempo and genre, a delay in time with the song, the drums
//! glued on a bus of their own, then each part's level against the drums.

use std::path::Path;

use pina_engine::{
    music::analysis,
    music::arrange::role,
    plugin::scan,
    song::commands::{AutoParam, Command, PointIn},
    song::project::{Content, Format, PluginRef, Project},
};
use serde_json::{Value, json};

use crate::cli;

/// An edit given as JSON, as an agent would send it.
fn run(dir: &Path, v: Value) -> Result<Value, String> {
    let c: Command = serde_json::from_value(v).map_err(|e| e.to_string())?;
    cli::edit_with(dir, c)
}

/// What a track's own sound measured, before anything of the mix's touches it.
struct Source {
    lufs: f32,
    /// Sub, bass, low mid, mid, presence, air: dB under the whole.
    bands: [f32; 6],
    width_db: f32,
    /// Its lowest note's fundamental, for a part played from notes.
    lowest_hz: Option<f32>,
    render: [Vec<f32>; 2],
}

/// A dynamic band hearing another track, its threshold set once the mix is: `band` of
/// `track`'s EQ, hearing `source` between `listens`' two frequencies with its attack and
/// release (`times`), letting go between hits or not, moving by `range`.
struct Hearing {
    track: usize,
    band: usize,
    source: usize,
    listens: [f32; 2],
    times: (f32, f32),
    lets_go: bool,
    range: f32,
}

/// The lowest note a track plays, in Hz.
fn lowest_hz(p: &Project, t: usize) -> Option<f32> {
    let low = p.tracks[t]
        .regions
        .iter()
        .filter_map(|r| match &r.content {
            Content::Midi { notes, .. } => notes.iter().map(|n| n.pitch).min(),
            Content::Pattern { rows, .. } => rows.iter().map(|r| r.pitch).min(),
            _ => None,
        })
        .min()?;
    Some(440.0 * 2f32.powf((low as f32 - 69.0) / 12.0))
}

/// A second-order Butterworth section, high- or low-pass at `hz` (the bilinear transform): what
/// a dynamic band's detector filters with.
fn butterworth(x: &[f32], rate: f32, hz: f32, high: bool) -> Vec<f32> {
    let k = (std::f32::consts::PI * hz.min(rate * 0.45) / rate).tan();
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let n = 1.0 / (1.0 + k / q + k * k);
    let b = match high {
        true => [n, -2.0 * n, n],
        false => [k * k * n, 2.0 * k * k * n, k * k * n],
    };
    let a = [2.0 * (k * k - 1.0) * n, (1.0 - k / q + k * k) * n];
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    x.iter()
        .map(|&v| {
            let y = b[0] * v + b[1] * x1 + b[2] * x2 - a[0] * y1 - a[1] * y2;
            (x2, x1, y2, y1) = (x1, v, y1, y);
            y
        })
        .collect()
}

/// What a dynamic band listening between `lo` and `hi` Hz hears of a track, as its detector
/// does: the louder side, filtered there, followed with its attack and release.
fn heard_level(
    render: &[Vec<f32>; 2],
    rate: f32,
    [lo, hi]: [f32; 2],
    attack_ms: f32,
    release_ms: f32,
) -> Vec<f32> {
    let side = |x: &[f32]| butterworth(&butterworth(x, rate, lo, true), rate, hi, false);
    let (l, r) = (side(&render[0]), side(&render[1]));
    let coef = |ms: f32| 1.0 - (-1.0 / (ms.max(0.05) * 1e-3 * rate)).exp();
    let (up, down) = (coef(attack_ms), coef(release_ms));
    let mut env = 0f32;
    l.iter()
        .zip(&r)
        .map(|(a, b)| {
            let v = a.abs().max(b.abs());
            env += (v - env) * if v > env { up } else { down };
            env
        })
        .collect()
}

/// How hard a track hits between `lo` and `hi` Hz as a dynamic band's detector hears it (with
/// its attack and release), and how far that's fallen by the end of each beat: each beat's
/// peak, and its least over the beat's last quarter; of each, the loudest quarter's least (a
/// sparse kick leaves many beats without one). dBFS.
fn band_peak_db(
    render: &[Vec<f32>; 2],
    rate: f32,
    beat: usize,
    band: [f32; 2],
    times: (f32, f32),
) -> Option<(f32, f32)> {
    let env = heard_level(render, rate, band, times.0, times.1);
    let most = |c: &[f32]| c.iter().fold(0f32, |m, v| m.max(*v));
    let least = |c: &[f32]| c.iter().fold(f32::MAX, |m, v| m.min(*v));
    let (mut peaks, mut ends): (Vec<f32>, Vec<f32>) = env
        .chunks(beat.max(1))
        .filter(|c| c.len() == beat.max(1))
        .map(|c| (most(c), least(&c[c.len() * 3 / 4..])))
        .filter(|(p, _)| *p > 1e-4)
        .unzip();
    if peaks.is_empty() {
        return None;
    }
    peaks.sort_by(f32::total_cmp);
    ends.sort_by(f32::total_cmp);
    let db = |v: f32| 20.0 * v.max(1e-6).log10();
    Some((db(peaks[peaks.len() * 3 / 4]), db(ends[ends.len() * 3 / 4])))
}

/// Where a kick's thump is strongest, 40 to 100 Hz (its fundamental): Goertzel's energy at each
/// frequency over its loudest beats.
fn kick_hz(render: &[Vec<f32>; 2], rate: f32) -> f32 {
    let mono: Vec<f32> = render[0]
        .iter()
        .zip(&render[1])
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    let energy = |f: f32| {
        let w = 2.0 * std::f32::consts::PI * f / rate;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0f32, 0f32);
        for &x in mono.iter().step_by(4) {
            let s = x + c * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        s1 * s1 + s2 * s2 - c * s1 * s2
    };
    // Sampled every fourth sample: to Goertzel, every frequency is four times as high.
    (40..=100)
        .step_by(5)
        .map(|f| f as f32)
        .max_by(|a, b| energy(*a * 4.0).total_cmp(&energy(*b * 4.0)))
        .unwrap_or(55.0)
}

/// A part's ear for mud (250 Hz-1 kHz) and bite (4-8 kHz): how much of its sound may be there
/// before it's cut, dB of the whole.
fn limits(part: &str) -> (f32, f32) {
    match part {
        "chords" => (-4.5, -16.0),
        "pad" => (-5.0, -18.0),
        "lead" => (-7.0, -14.0),
        "arp" | "counter" => (-7.0, -14.0),
        "vocals" => (-7.0, -13.0),
        "perc" => (-9.0, -9.0),
        "drums" => (-8.0, -11.0),
        "bass" => (-10.0, -24.0),
        "texture" => (-6.0, -16.0),
        _ => (-6.0, -14.0),
    }
}

/// A part's low cut, before its lowest note has its say.
fn low_cut(part: &str) -> f32 {
    match part {
        "bass" | "drums" => 25.0,
        "vocals" => 90.0,
        "fx" => 60.0,
        "perc" | "chords" => 150.0,
        "lead" => 180.0,
        _ => 200.0,
    }
}

/// A part's compressor: (dB over its own loudness, ratio, attack, release).
fn comp(part: &str) -> Option<(f32, f32, f32, f32)> {
    match part {
        "drums" => Some((6.0, 4.0, 8.0, 90.0)),
        "perc" => Some((6.0, 2.5, 10.0, 80.0)),
        "bass" => Some((4.0, 4.0, 10.0, 120.0)),
        "chords" => Some((6.0, 2.5, 20.0, 150.0)),
        "pad" => Some((8.0, 2.0, 30.0, 200.0)),
        "lead" => Some((5.0, 3.0, 5.0, 100.0)),
        "arp" | "counter" => Some((6.0, 2.5, 10.0, 120.0)),
        "vocals" => Some((4.0, 4.0, 3.0, 80.0)),
        "fx" | "texture" => None,
        _ => Some((6.0, 2.5, 15.0, 120.0)),
    }
}

/// A part's saturation: (drive, tone, mix).
fn sat(part: &str) -> Option<(f32, f32, f32)> {
    match part {
        // Soft-clips the hits a compressor lets through.
        "drums" => Some((6.0, 0.0, 0.7)),
        // Harmonics, so it's heard on small speakers and not only felt.
        "bass" => Some((9.0, 0.15, 0.55)),
        "chords" => Some((3.0, -0.1, 0.35)),
        "lead" => Some((2.0, 0.1, 0.25)),
        "vocals" => Some((2.0, 0.1, 0.2)),
        _ => None,
    }
}

/// Where a part sits against the drums alone, LUFS (a bass by its RMS, as LUFS hardly hears
/// sub); the drums at -20 leave the master room.
fn level(part: &str) -> f32 {
    match part {
        "drums" => -20.0,
        "perc" => -28.0,
        "bass" => -21.0,
        "chords" => -25.0,
        "pad" => -29.0,
        "lead" => -24.5,
        "counter" | "arp" => -28.0,
        "vocals" => -21.5,
        "texture" => -36.0,
        _ => -28.0,
    }
}

/// Its sends: (reverb, delay), dB, if any.
fn sends(part: &str, genre: &str) -> (Option<f32>, Option<f32>) {
    let wet = match genre {
        "lofi" | "cinematic" | "ambient" | "neo_soul" => 3.0,
        "trap" | "drill" | "techno" => -2.0,
        _ => 0.0,
    };
    let verb = match part {
        "lead" => Some(-14.0),
        "vocals" => Some(-13.0),
        "chords" => Some(-17.0),
        "pad" => Some(-11.0),
        "arp" => Some(-13.0),
        "counter" => Some(-14.0),
        "perc" => Some(-18.0),
        "drums" if !matches!(genre, "trap" | "drill") => Some(-22.0),
        "fx" => Some(-12.0),
        _ => None,
    };
    let echo = match part {
        "lead" => Some(-17.0),
        "vocals" => Some(-16.0),
        "arp" | "counter" => Some(-20.0),
        "perc" => Some(-24.0),
        _ => None,
    };
    (verb.map(|v| v + wet), echo.map(|v| v + wet * 0.5))
}

/// Where a dynamic band hearing another track should start, for it to move its whole `range` as
/// that track hits at `peak` (the band moves by 4:1 over its threshold, through a 6 dB knee) and
/// let go again as it falls to `floor` before its next hit: a kick that rings on, too little
/// apart, gets a shallower duck that lets go, rather than one that never does.
fn under(peak: f32, floor: Option<f32>, range: f32) -> f32 {
    let full = peak - range.abs() / 0.75 - 5.0;
    let free = floor.map_or(-80.0, |f| f + 3.0);
    full.max(free).clamp(-80.0, 0.0)
}

/// Genres whose music breathes with the kick: the bass and pads ducking on every hit.
fn pumps(genre: &str) -> bool {
    matches!(
        genre,
        "house" | "deep_house" | "techno" | "edm" | "amapiano" | "pop"
    )
}

/// A plugin installed, by its name, in the format it plays best in here.
fn installed(plugins: &[scan::PluginInfo], class: &str) -> Option<PluginRef> {
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

fn au(class: &str, uid: &str) -> PluginRef {
    PluginRef {
        format: Format::Au,
        class: class.into(),
        uid: Some(uid.into()),
        ..Default::default()
    }
}

/// An aux by name, made if there's none (sent to from `first` to make it); its index and bus.
fn aux(dir: &Path, first: usize, name: &str) -> Result<(usize, u32), String> {
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
/// else Apple's; fully wet, its pre-delay a sixty-fourth note, its decay by genre, its lows and
/// highs cut so it doesn't cloud the mix.
fn reverb(
    dir: &Path,
    first: usize,
    plugins: &[scan::PluginInfo],
    bpm: f64,
    genre: &str,
) -> Result<(u32, String), String> {
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
    let big = matches!(genre, "cinematic" | "ambient");
    let set = |name: &str, text: String| {
        run(
            dir,
            json!({ "op": "set_plugin_param", "channel": ch, "plugin": { "insert": 0 }, "param": name, "text": text }),
        )
    };
    let hall = installed(plugins, "Dragonfly Hall Reverb").filter(|_| big);
    let plate = installed(plugins, "Dragonfly Plate Reverb");
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
    Ok((
        bus,
        format!("{which}, {predelay:.0} ms pre-delay, {decay:.1} s decay"),
    ))
}

/// The song's delay, "Echo": Apple's, a dotted eighth (an eighth under 90 BPM), its repeats dark
/// and thin so they sit behind what they repeat.
fn echo(dir: &Path, first: usize, bpm: f64) -> Result<(u32, String), String> {
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
    Ok((
        bus,
        format!("Apple's delay, {note} ({:.0} ms)", seconds * 1000.0),
    ))
}

/// Sends `t` to `bus` at `db`, or sets the send it has there.
fn send(dir: &Path, t: usize, bus: u32, db: f32) -> Result<(), String> {
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

/// The master's EQ set to bring the mix's tonal balance halfway to the reference's (else the
/// genre's): a broad move in each of the six bands, sub to air, at most 2.5 dB, none under
/// half a dB. What it measured and did; nothing when there's nothing to aim at.
pub fn tonal_balance(dir: &Path) -> Result<Value, String> {
    let p = cli::project(dir)?;
    let target = p
        .reference
        .as_ref()
        .map(|r| (r.spectrum, "the reference"))
        .or_else(|| {
            let g = p.genre.as_deref()?;
            pina_engine::music::playbook::find(g).map(|b| (b.spectrum, "the genre"))
        });
    let Some((target, aim)) = target else {
        return Ok(Value::Null);
    };
    let mut unmastered = p.clone();
    unmastered.master_inserts.clear();
    (
        unmastered.master_eq,
        unmastered.master_comp,
        unmastered.master_sat,
    ) = (None, None, None);
    unmastered.master_volume_db = 0.0;
    let s = cli::report(&unmastered, dir, None, false)?.mix.spectrum_db;
    let measured = [s.sub, s.bass, s.low_mid, s.mid, s.presence, s.air];
    const NAMES: [&str; 6] = ["sub", "bass", "low mid", "mid", "presence", "air"];
    let mut bands = Vec::new();
    let mut said = Vec::new();
    for (i, (want, got)) in target.iter().zip(measured).enumerate() {
        let gain = ((want - got) * 0.5).clamp(-2.5, 2.5);
        if gain.abs() < 0.5 {
            continue;
        }
        let gain = (gain as f64 * 10.0).round() / 10.0;
        bands.push(match i {
            0 => json!({ "shape": "low_shelf", "freq": 60, "gain_db": gain }),
            1 => json!({ "shape": "bell", "freq": 120, "gain_db": gain, "q": 0.9 }),
            2 => json!({ "shape": "bell", "freq": 500, "gain_db": gain, "q": 0.8 }),
            3 => json!({ "shape": "bell", "freq": 2500, "gain_db": gain, "q": 0.8 }),
            4 => json!({ "shape": "bell", "freq": 6000, "gain_db": gain, "q": 0.9 }),
            _ => json!({ "shape": "high_shelf", "freq": 10000, "gain_db": gain }),
        });
        said.push(
            json!({ "band": NAMES[i], "measured_db": (got as f64 * 10.0).round() / 10.0,
            "target_db": *want, "eq_db": gain }),
        );
    }
    let eq = match bands.is_empty() {
        true => Value::Null,
        false => json!({ "bands": bands }),
    };
    run(
        dir,
        json!({ "op": "set_eq", "channel": "master", "eq": eq }),
    )?;
    Ok(json!({ "toward": aim, "moves": said }))
}

#[cfg(test)]
mod tests;
