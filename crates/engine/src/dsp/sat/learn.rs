//! A saturator set by listening. It measures a channel's sound: where its energy sits, how
//! bright it is, how many harmonics it has of its own, and how sharply and how far its peaks
//! stand out. Those measurements, with the part it plays and the song's genre, decide where the
//! saturator splits, each band's style, drive, tone and dynamics, and any ringing tuned to the
//! song's key. Running the saturator over the sound sets its output, so it comes out as loud as
//! it went in.

use super::Saturation;
use crate::{
    dsp::comp::learn::{levels, percentile},
    music::{Key, analysis::analyze},
    song::project::{MAX_SAT_BANDS, SatBand, SatBands, SatStyle, Saturator},
};

/// The colour a genre's saturation leans to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flavour {
    /// Tube and tape, rounded and thick: soul, R&B, jazz, gospel, hip-hop, Afrobeats.
    Warm,
    /// Transformers pushing the lows, tape on the rest: house, techno, amapiano, EDM.
    Club,
    /// Amps driven hard: trap, drill, drum & bass, rock.
    Hard,
    /// Clean tape and tube, barely there: pop, ambient, cinematic.
    Clean,
    /// Worn tape and fewer bits: lo-fi.
    Lofi,
}

impl Flavour {
    /// A genre (as songs and playbooks name it) to its saturation's colour.
    pub fn of_genre(genre: &str) -> Flavour {
        match genre {
            "lofi" | "chill" => Flavour::Lofi,
            "trap" | "drill" | "dnb" | "rock" | "metal" | "dubstep" | "phonk" => Flavour::Hard,
            "house" | "deep_house" | "afro_house" | "techno" | "edm" | "trance" | "garage"
            | "amapiano" | "reggaeton" | "dancehall" => Flavour::Club,
            "neo_soul" | "rnb" | "soul" | "jazz" | "gospel" | "funk" | "hip_hop" | "boom_bap"
            | "afrobeats" => Flavour::Warm,
            _ => Flavour::Clean,
        }
    }
}

/// What it's listening for: the part the channel plays (as `music::arrange::role` names it),
/// the song's genre, the lowest note it plays (Hz) and the song's key.
#[derive(Clone, Copy, Debug)]
pub struct Ear<'a> {
    pub part: &'a str,
    pub genre: &'a str,
    pub lowest_hz: Option<f32>,
    pub key: Option<Key>,
}

/// What was heard: energy in the six bands (dB of the whole), how much is over its lowest
/// octaves, how sharply it hits and how far its peaks stand over its body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heard {
    pub sub: f32,
    pub bass: f32,
    pub low_mid: f32,
    pub mid: f32,
    pub presence: f32,
    pub air: f32,
    /// The highs (over 4 kHz) against the whole, dB.
    pub top: f32,
    /// Everything over 250 Hz against the whole, dB: little means few harmonics (a sine).
    pub upper: f32,
    /// How far its sharpest fronts jump in 50 ms, dB.
    pub sharp: f32,
    /// Its loud peaks over its loud body, dB.
    pub crest: f32,
}

impl Heard {
    /// A sound with nothing heard of it (a bus, which isn't heard alone): its part and the
    /// genre decide on their own.
    pub fn flat() -> Heard {
        Heard {
            sub: -14.0,
            bass: -8.0,
            low_mid: -6.0,
            mid: -7.0,
            presence: -14.0,
            air: -20.0,
            top: -13.0,
            upper: -3.0,
            sharp: 6.0,
            crest: 12.0,
        }
    }
}

fn sum_db(v: &[f32]) -> f32 {
    10.0 * v
        .iter()
        .map(|d| 10f32.powf(d / 10.0))
        .sum::<f32>()
        .max(1e-12)
        .log10()
}

/// Measures `render` (a channel alone, at `rate`). None when nothing plays.
pub fn hear(render: &[Vec<f32>; 2], rate: f64) -> Option<Heard> {
    let l = levels(render, rate as f32);
    if l.rms.len() < 10 {
        return None;
    }
    let s = analyze(&render[0], &render[1], rate).spectrum_db;
    Some(Heard {
        sub: s.sub,
        bass: s.bass,
        low_mid: s.low_mid,
        mid: s.mid,
        presence: s.presence,
        air: s.air,
        top: sum_db(&[s.presence, s.air]),
        upper: sum_db(&[s.low_mid, s.mid, s.presence, s.air]),
        sharp: percentile(&l.rise, 0.98),
        crest: percentile(&l.peak, 0.95) - percentile(&l.rms, 0.7),
    })
}

fn band(style: SatStyle, drive_db: f32, tone: f32) -> SatBand {
    SatBand {
        style,
        drive_db: drive_db.clamp(0.0, 24.0),
        tone: tone.clamp(-1.0, 1.0),
        ..SatBand::default()
    }
}

/// The key's tonic moved by octaves into `lo`..`hi` Hz: where a band rings in tune.
fn tonic_in(key: Key, lo: f32, hi: f32) -> f32 {
    let mut hz = 440.0 * 2f32.powf((key.root as f32 - 9.0) / 12.0);
    while hz < lo {
        hz *= 2.0;
    }
    while hz > hi {
        hz /= 2.0;
    }
    hz
}

/// The saturator for what was heard, and why, in words. Its output is left at 0 dB: `learn`
/// finds it by running the saturator.
pub fn plan(ear: &Ear, h: &Heard) -> (Saturator, Vec<String>) {
    use SatStyle::*;
    let f = Flavour::of_genre(ear.genre);
    let mut why = Vec::new();
    // A dull sound is brightened by what's added, a bright one darkened: 0 at -16 dB of highs.
    let tilt = ((-16.0 - h.top) / 12.0).clamp(-0.4, 0.4);
    match tilt {
        t if t > 0.15 => why.push(format!(
            "dull ({:.0} dB of highs): what it adds is brighter",
            h.top
        )),
        t if t < -0.15 => why.push(format!(
            "bright ({:.0} dB of highs): what it adds is darker",
            h.top
        )),
        _ => {}
    }
    // Squashed already, it's driven less; peaky, the peaks are what it takes off. (A tone with
    // nothing over 4 kHz, a sine, peaks 3 dB over its body however it's played: not squashed.)
    let dense = match h.crest {
        c if c < 7.0 && h.top > -45.0 => {
            why.push(format!("already dense ({c:.0} dB crest): driven gently"));
            -3.0
        }
        c if c > 16.0 => 2.0,
        _ => 0.0,
    };
    let (mut bands, crossovers, mix): (Vec<SatBand>, Vec<f32>, f32) = match ear.part {
        "drums" => {
            let low = match f {
                Flavour::Club => band(Transformer, 4.0 + dense, 0.0),
                Flavour::Lofi => band(WarmTape, 4.0 + dense, -0.2),
                _ => band(CleanTape, 3.0 + dense, 0.0),
            };
            let body = match f {
                Flavour::Hard => band(Crunch, 6.0 + dense, tilt),
                Flavour::Club => band(Transformer, 5.0 + dense, tilt),
                Flavour::Warm => band(WarmTape, 5.0 + dense, tilt),
                Flavour::Clean => band(CleanTape, 3.0 + dense, tilt),
                Flavour::Lofi => SatBand {
                    mix: 0.6,
                    ..band(Lofi, 5.0, tilt - 0.2)
                },
            };
            let top = match f {
                Flavour::Clean | Flavour::Hard => band(CleanTape, 2.0, tilt.min(0.0)),
                _ => band(WarmTape, 3.0, tilt.min(0.0) - 0.1),
            };
            why.push(format!(
                "drums in three bands: lows {} for weight, kept punchy; body {} for its {}; \
                 top {} to round the hats",
                low.style.name(),
                body.style.name(),
                match f {
                    Flavour::Hard => "bite",
                    Flavour::Lofi => "grit",
                    _ => "thickness",
                },
                top.style.name()
            ));
            (
                vec![
                    SatBand {
                        dynamics: -0.25,
                        ..low
                    },
                    body,
                    top,
                ],
                vec![120.0, 4500.0],
                0.8,
            )
        }
        "perc" => {
            let b = match f {
                Flavour::Hard => band(Crunch, 3.0, tilt),
                Flavour::Club => band(Transformer, 3.0, tilt),
                Flavour::Lofi => band(Lofi, 3.0, tilt - 0.2),
                Flavour::Warm => band(WarmTape, 3.0, tilt),
                Flavour::Clean => band(CleanTape, 2.0, tilt),
            };
            why.push(format!("percussion: {} for a little edge", b.style.name()));
            (
                vec![SatBand {
                    dynamics: -0.2,
                    ..b
                }],
                vec![],
                0.6,
            )
        }
        "bass" => {
            // The fundamental clean and steady; the harmonics over it are what's driven, so it's
            // heard on small speakers and not only felt.
            let split = match ear.lowest_hz {
                Some(hz) => (hz * 2.2).clamp(80.0, 200.0),
                None if h.sub > h.bass => 100.0,
                None => 150.0,
            };
            let pure = h.upper < -25.0;
            let rich = h.upper > -8.0;
            let more = match (pure, rich) {
                (true, _) => {
                    why.push(format!(
                        "nearly a sine ({:.0} dB over 250 Hz): driven harder for harmonics",
                        h.upper
                    ));
                    4.0
                }
                (_, true) => {
                    why.push("rich in harmonics already: driven less".into());
                    -4.0
                }
                _ => 0.0,
            };
            let low = match f {
                Flavour::Hard => band(CleanTape, 3.0, 0.0),
                Flavour::Club => band(Transformer, 3.0, 0.0),
                _ => band(CleanTube, 2.0, 0.0),
            };
            let high = match f {
                Flavour::Hard => band(Heavy, 12.0 + more + dense, tilt),
                Flavour::Club => band(Transformer, 9.0 + more + dense, tilt),
                Flavour::Warm => band(WarmTube, 9.0 + more + dense, tilt),
                Flavour::Clean => band(WarmTape, 6.0 + more + dense, tilt),
                Flavour::Lofi => band(WarmTape, 8.0 + more + dense, tilt - 0.3),
            };
            why.push(format!(
                "bass split at {split:.0} Hz: under it {} keeps the fundamental clean, over it {} \
                 at {:.0} dB for the harmonics",
                low.style.name(),
                high.style.name(),
                high.drive_db
            ));
            (
                vec![
                    SatBand {
                        dynamics: 0.1,
                        ..low
                    },
                    SatBand { mix: 0.85, ..high },
                ],
                vec![split],
                1.0,
            )
        }
        "chords" | "pad" => {
            let held = ear.part == "pad";
            let body = match f {
                Flavour::Clean => band(CleanTape, 1.5, 0.0),
                _ => band(WarmTape, 2.0, 0.0),
            };
            let mid = match f {
                Flavour::Warm | Flavour::Hard => band(WarmTube, if held { 3.0 } else { 4.0 }, tilt),
                Flavour::Club => band(WarmTape, 3.0, tilt),
                Flavour::Clean => band(CleanTube, 2.0, tilt),
                Flavour::Lofi => band(WarmTape, 5.0, tilt - 0.2),
            };
            let top = match f {
                Flavour::Lofi => SatBand {
                    mix: 0.4,
                    ..band(Lofi, 4.0, -0.3)
                },
                _ => band(CleanTape, 1.0, tilt.min(0.0) - if held { 0.2 } else { 0.0 }),
            };
            why.push(format!(
                "{} in three bands: body {}, the middle {} for {}, top {}",
                ear.part,
                body.style.name(),
                mid.style.name(),
                if held {
                    "warmth that swells with it"
                } else {
                    "harmonics that bind the voicing"
                },
                top.style.name()
            ));
            (
                vec![
                    body,
                    SatBand {
                        dynamics: if held { 0.2 } else { 0.0 },
                        ..mid
                    },
                    top,
                ],
                vec![400.0, 5000.0],
                if held { 0.5 } else { 0.6 },
            )
        }
        "lead" => {
            let low = match f {
                Flavour::Clean => band(CleanTube, 2.0, 0.0),
                _ => band(WarmTube, 3.0, 0.0),
            };
            let mut high = match f {
                Flavour::Hard => band(Crunch, 7.0 + dense, tilt),
                Flavour::Club | Flavour::Warm => band(WarmTube, 5.0 + dense, tilt),
                Flavour::Clean => band(CleanTube, 3.0 + dense, tilt),
                Flavour::Lofi => band(WarmTape, 5.0 + dense, tilt - 0.2),
            };
            // Electronic leads ring a little on the key's tonic: resonance in tune.
            if let (Some(k), Flavour::Club | Flavour::Hard) = (ear.key, f) {
                high.feedback = 0.15;
                high.feedback_hz = tonic_in(k, 1200.0, 2400.0);
                why.push(format!(
                    "rings a touch at {:.0} Hz, the key's tonic",
                    high.feedback_hz
                ));
            }
            why.push(format!(
                "lead split at 1.2 kHz: {} under for body, {} over for presence",
                low.style.name(),
                high.style.name()
            ));
            (vec![low, high], vec![1200.0], 0.7)
        }
        "arp" | "counter" | "pluck" | "keys" => {
            let b = match f {
                Flavour::Hard => SatBand {
                    mix: 0.6,
                    ..band(Crunch, 4.0, tilt)
                },
                Flavour::Club => band(WarmTape, 4.0, tilt),
                Flavour::Warm => band(WarmTube, 3.0, tilt),
                Flavour::Clean => band(CleanTape, 2.0, tilt),
                Flavour::Lofi => SatBand {
                    mix: 0.5,
                    ..band(Lofi, 3.0, tilt - 0.2)
                },
            };
            why.push(format!("{}: {} for edge", ear.part, b.style.name()));
            (vec![b], vec![], 0.6)
        }
        "vocals" => {
            let mid = match f {
                Flavour::Warm | Flavour::Lofi => band(WarmTube, 4.0, tilt),
                Flavour::Hard => band(WarmTube, 5.0, tilt),
                _ => band(CleanTube, 3.0, tilt),
            };
            why.push(format!(
                "voice in three bands: chest clean, {} through the middle, the esses softened",
                mid.style.name()
            ));
            (
                vec![
                    band(CleanTube, 1.0, 0.0),
                    SatBand {
                        dynamics: 0.2,
                        ..mid
                    },
                    band(CleanTape, 1.0, tilt.min(0.0) - 0.3),
                ],
                vec![250.0, 3500.0],
                0.7,
            )
        }
        "texture" => {
            let b = match f {
                Flavour::Lofi | Flavour::Warm => band(Lofi, 6.0, -0.3),
                _ => band(WarmTape, 4.0, -0.2),
            };
            why.push(format!("texture: {} to sit it back", b.style.name()));
            (vec![b], vec![], 0.5)
        }
        "fx" => {
            let mut b = match f {
                Flavour::Hard => band(Foldback, 6.0, 0.2),
                Flavour::Club => band(Rectify, 5.0, 0.0),
                _ => band(WarmTape, 3.0, 0.0),
            };
            if let Some(k) = ear.key {
                b.feedback = 0.25;
                b.feedback_hz = tonic_in(k, 400.0, 800.0);
                why.push(format!("rings at {:.0} Hz, the key's tonic", b.feedback_hz));
            }
            why.push(format!("effects: {} for colour", b.style.name()));
            (vec![b], vec![], 0.35)
        }
        // A bus or the whole mix: glue in three bands, gently, half of it heard.
        "bus" => {
            let low = match f {
                Flavour::Club => band(Transformer, 2.0, 0.0),
                Flavour::Warm | Flavour::Lofi => band(WarmTape, 2.0, 0.0),
                _ => band(CleanTape, 1.5, 0.0),
            };
            let mid = match f {
                Flavour::Warm => band(WarmTube, 2.0, tilt),
                Flavour::Lofi => band(WarmTape, 3.0, tilt - 0.2),
                Flavour::Hard => band(CleanTape, 2.5, tilt),
                _ => band(CleanTape, 1.5, tilt),
            };
            why.push(format!(
                "glue in three bands: lows {}, middle {}, top clean tape",
                low.style.name(),
                mid.style.name()
            ));
            (
                vec![low, mid, band(CleanTape, 1.0, tilt.min(0.0))],
                vec![150.0, 5000.0],
                0.5,
            )
        }
        _ => {
            let b = band(WarmTape, 3.0, tilt);
            why.push("warm tape, gently".into());
            (vec![b], vec![], 0.5)
        }
    };
    bands.truncate(MAX_SAT_BANDS);
    let mut cross = [20_000.0; MAX_SAT_BANDS - 1];
    cross[..crossovers.len()].copy_from_slice(&crossovers);
    // The harder curves alias, run at more times the rate.
    let quality = match bands.iter().map(|b| b.style).max_by_key(|s| match s {
        Rectify | Foldback => 3,
        Crunch | Heavy | Lofi => 2,
        _ => 1,
    }) {
        Some(Rectify | Foldback) => 8,
        Some(Crunch | Heavy) => 4,
        _ => 2,
    };
    (
        Saturator {
            bands: SatBands::from_slice(&bands),
            crossovers: cross,
            slope: 24,
            quality,
            mix,
            output_db: 0.0,
            bypass: false,
        },
        why,
    )
}

fn power_db(x: &[Vec<f32>; 2]) -> f32 {
    let n = x[0].len().max(1) as f64;
    let p: f64 = x
        .iter()
        .flat_map(|c| c.iter())
        .map(|&s| (s as f64).powi(2))
        .sum::<f64>()
        / (2.0 * n);
    (10.0 * p.max(1e-20).log10()) as f32
}

/// Sets a saturator for `render` (a channel alone, at `rate`), as `ear` says it's heard: the
/// plan for what's measured, and its output set so it measures as loud as it went in. None when
/// nothing plays.
pub fn learn(render: &[Vec<f32>; 2], rate: f64, ear: &Ear) -> Option<(Saturator, Vec<String>)> {
    let heard = hear(render, rate)?;
    let (mut sat, why) = plan(ear, &heard);
    // shortcut: levels the first 30 s, enough for a part's loop; a long song's level is the same.
    let n = render[0]
        .len()
        .min(render[1].len())
        .min((rate * 30.0) as usize);
    let mut out = [render[0][..n].to_vec(), render[1][..n].to_vec()];
    let mut s = Saturation::new(rate);
    let [l, r] = &mut out;
    s.process(&sat, l, r);
    let dry = power_db(&[render[0][..n].to_vec(), render[1][..n].to_vec()]);
    sat.output_db = ((dry - power_db(&out)) * 10.0).round() / 10.0;
    sat.output_db = sat.output_db.clamp(-12.0, 12.0);
    Some((sat, why))
}

#[cfg(test)]
mod tests;
