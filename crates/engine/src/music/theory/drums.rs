//! Drum grooves by genre, and drums played from them.

use super::*;

/// A drum in a kit, on its General MIDI note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drum {
    Kick,
    Snare,
    Clap,
    Rim,
    HatClosed,
    HatOpen,
    Shaker,
    Perc,
    TomHigh,
    TomMid,
    TomLow,
    Crash,
    Ride,
}

impl Drum {
    pub fn note(self) -> u8 {
        match self {
            Drum::Kick => 36,
            Drum::Snare => 38,
            Drum::Clap => 39,
            Drum::Rim => 37,
            Drum::HatClosed => 42,
            Drum::HatOpen => 46,
            Drum::Shaker => 70,
            Drum::Perc => 56,
            Drum::TomHigh => 48,
            Drum::TomMid => 45,
            Drum::TomLow => 41,
            Drum::Crash => 49,
            Drum::Ride => 51,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Drum::Kick => "kick",
            Drum::Snare => "snare",
            Drum::Clap => "clap",
            Drum::Rim => "rim",
            Drum::HatClosed => "closed hat",
            Drum::HatOpen => "open hat",
            Drum::Shaker => "shaker",
            Drum::Perc => "perc",
            Drum::TomHigh => "high tom",
            Drum::TomMid => "mid tom",
            Drum::TomLow => "low tom",
            Drum::Crash => "crash",
            Drum::Ride => "ride",
        }
    }
}

/// A genre's groove: its steps to a bar, swing, and each drum's bars, one character a step:
/// X accent, x hit, o soft, g ghost, . rest. Rows of several bars alternate.
pub struct Groove {
    pub id: &'static str,
    pub steps: u8,
    pub swing: f32,
    pub rows: &'static [(Drum, &'static [&'static str])],
    /// Hats rolled at the end of every other bar (trap, drill).
    pub rolls: bool,
}

pub const GROOVES: &[Groove] = &[
    Groove {
        id: "boom_bap",
        steps: 16,
        swing: 58.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x......x..x.....", "x.........x..x.."]),
            (Drum::Snare, &["....X.......X...", "....X.......X..g"]),
            (Drum::HatClosed, &["x.o.x.o.x.o.x.o.", "x.o.x.o.x.o.x.oo"]),
            (Drum::HatOpen, &["................", "..............o."]),
        ],
    },
    Groove {
        id: "lofi",
        steps: 16,
        swing: 60.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x......o..x.....", "x.........x....."]),
            (Drum::Snare, &["....x.......x...", "....x.......x..g"]),
            (Drum::HatClosed, &["o.g.o.g.o.g.o.g.", "o.g.o.g.o.g.o.gg"]),
            (Drum::Shaker, &["................", "................"]),
        ],
    },
    Groove {
        id: "trap",
        steps: 16,
        swing: 50.0,
        rolls: true,
        rows: &[
            (Drum::Kick, &["x.....x...x.....", "x.........x..x.."]),
            (Drum::Clap, &["........X.......", "........X......."]),
            (Drum::Snare, &["........x.......", "........x......."]),
            (Drum::HatClosed, &["x.x.x.x.x.x.x.x.", "x.x.x.x.x.x.x.x."]),
            (Drum::HatOpen, &["................", "..............o."]),
        ],
    },
    Groove {
        id: "drill",
        steps: 16,
        swing: 50.0,
        rolls: true,
        rows: &[
            (Drum::Kick, &["x.....x.....x...", "..x.....x......."]),
            (Drum::Snare, &["........X....x..", "........X......."]),
            (Drum::HatClosed, &["x..x..x.x..x..x.", "x..x..x.x..x.x.x"]),
            (Drum::Perc, &["...o......o.....", "...o......o....."]),
        ],
    },
    Groove {
        id: "house",
        steps: 16,
        swing: 54.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["X...X...X...X..."]),
            (Drum::Clap, &["....x.......x..."]),
            (Drum::HatOpen, &["..x...x...x...x."]),
            (Drum::HatClosed, &["g.g.g.g.g.g.g.g."]),
            (Drum::Shaker, &["gogogogogogogogo"]),
        ],
    },
    Groove {
        id: "techno",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["X...X...X...X..."]),
            (Drum::HatOpen, &["..x...x...x...x."]),
            (Drum::HatClosed, &["o.o.o.o.o.o.o.o."]),
            (Drum::Rim, &["......o.......o.", "......o....o..o."]),
            (Drum::Clap, &["....o.......o..."]),
        ],
    },
    Groove {
        id: "afrobeats",
        steps: 16,
        swing: 55.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x..x...x..x.....", "x..x...x..x...x."]),
            (Drum::Rim, &["...x..x...x..x..", "...x..x...x..x.."]),
            (Drum::Clap, &["....x.......x..."]),
            (Drum::Shaker, &["xoxoxoxoxoxoxoxo"]),
            (Drum::Perc, &["..o.....o..o....", "..o.....o...o.o."]),
        ],
    },
    Groove {
        id: "amapiano",
        steps: 16,
        swing: 54.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x.......x.......", "x.......x......."]),
            (Drum::Clap, &["....x.......x..."]),
            (Drum::Shaker, &["xgxgxgxgxgxgxgxg"]),
            (Drum::HatOpen, &["..o...o...o...o."]),
            (Drum::Perc, &["..x..x....x..x..", "..x..x..x...x..x"]),
        ],
    },
    Groove {
        id: "rnb",
        steps: 16,
        swing: 60.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x......x..x.x...", "x.x.......x....."]),
            (Drum::Snare, &["....x.......x...", "....x..g....x..g"]),
            (Drum::HatClosed, &["x.xgx.xgx.xgx.x.", "x.xgx.xgx.xgx.xx"]),
        ],
    },
    Groove {
        id: "pop",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x.......x.x.....", "x.......x......."]),
            (Drum::Snare, &["....x.......x..."]),
            (Drum::Clap, &["....o.......o..."]),
            (Drum::HatClosed, &["x.o.x.o.x.o.x.o."]),
        ],
    },
    Groove {
        id: "reggaeton",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x...x...x...x..."]),
            (Drum::Snare, &["...x..x....x..x."]),
            (Drum::HatClosed, &["x.o.x.o.x.o.x.o."]),
        ],
    },
    Groove {
        id: "dnb",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x.........x.....", "x.......x.x....."]),
            (Drum::Snare, &["....X.......X...", "....X..g....X..."]),
            (Drum::HatClosed, &["x.x.x.x.x.x.x.x."]),
            (Drum::Ride, &["................", "..............o."]),
        ],
    },
    Groove {
        id: "rock",
        steps: 16,
        swing: 50.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x.......x.x.....", "x.....x.x......."]),
            (Drum::Snare, &["....X.......X..."]),
            (Drum::HatClosed, &["x.x.x.x.x.x.x.x."]),
        ],
    },
    Groove {
        id: "neo_soul",
        steps: 16,
        swing: 62.0,
        rolls: false,
        rows: &[
            (Drum::Kick, &["x......x...x....", "x.x.......x..x.."]),
            (Drum::Rim, &["....x..g....x...", "....x..g....x.gg"]),
            (Drum::HatClosed, &["x.xgx.xgx.xgx.xg"]),
        ],
    },
];

/// The groove for a genre or style word, else boom bap.
pub fn groove(style: &str) -> &'static Groove {
    let s = style.to_lowercase().replace(['-', ' '], "_");
    let alias = match s.as_str() {
        "hip_hop" | "hiphop" | "boombap" => "boom_bap",
        "lo_fi" | "lofi_hip_hop" | "chill" => "lofi",
        "uk_drill" => "drill",
        "deep_house" | "afro_house" | "garage" => "house",
        "r&b" | "randb" | "soul" => "rnb",
        "gospel" | "jazz" => "neo_soul",
        "drum_and_bass" | "jungle" | "liquid" => "dnb",
        "edm" | "future_bass" => "pop",
        "dancehall" | "latin" => "reggaeton",
        other => other,
    };
    GROOVES
        .iter()
        .find(|g| g.id == alias)
        .unwrap_or(&GROOVES[0])
}

/// A drum's step velocity, from its character.
pub(crate) fn hit(c: char) -> u8 {
    match c {
        'X' => 120,
        'x' => 100,
        'o' => 72,
        'g' => 42,
        _ => 0,
    }
}

/// A groove written out for `bars` bars: each drum's cells (a velocity a step), its rows' bars
/// alternating, small changes by `seed`, a fill every `fill_every` bars (its last bar), and a
/// crash after each fill. Trap and drill hats get rolls, on a 32nd grid. Answers the steps per
/// bar and the rows by note.
pub fn drums(g: &Groove, bars: u64, fill_every: u64, seed: u64) -> (u8, Vec<(u8, Vec<u8>)>) {
    let mut rng = Rng::new(seed);
    // Rolls need 32nds: every 16th step becomes two.
    let fine = if g.rolls { 2usize } else { 1 };
    let per_bar = g.steps as usize * fine;
    let total = per_bar * bars as usize;
    let mut rows: Vec<(Drum, Vec<u8>)> = Vec::new();
    for (drum, patterns) in g.rows {
        let mut cells = vec![0u8; total];
        for b in 0..bars as usize {
            let pattern: Vec<char> = patterns[b % patterns.len()].chars().collect();
            for (i, &c) in pattern.iter().enumerate().take(g.steps as usize) {
                let mut v = hit(c);
                if v > 0 {
                    // Humans don't hit twice the same.
                    v = (v as i32 + rng.below(9) as i32 - 4).clamp(1, 127) as u8;
                }
                cells[b * per_bar + i * fine] = v;
            }
        }
        rows.push((*drum, cells));
    }
    let row = |rows: &mut Vec<(Drum, Vec<u8>)>, d: Drum| -> usize {
        match rows.iter().position(|(x, _)| *x == d) {
            Some(i) => i,
            None => {
                rows.push((d, vec![0; total]));
                rows.len() - 1
            }
        }
    };
    // Rolls: the last beat of every other bar's hats in 32nds, rising.
    if g.rolls {
        let h = row(&mut rows, Drum::HatClosed);
        for b in (1..bars as usize).step_by(2) {
            let start = b * per_bar + per_bar - 8;
            let kind = rng.below(3);
            for k in 0..8 {
                let on = match kind {
                    0 => true,
                    1 => k % 2 == 0 || k >= 4,
                    _ => k >= 2,
                };
                rows[h].1[start + k] = if on { 60 + (k as u8) * 6 } else { 0 };
            }
        }
    }
    // Fills: the last bar of each run of `fill_every`, its back half a snare (or tom) run.
    if fill_every > 0 {
        let s = row(
            &mut rows,
            if g.rows.iter().any(|r| r.0 == Drum::Snare) {
                Drum::Snare
            } else {
                Drum::Clap
            },
        );
        let toms = [Drum::TomHigh, Drum::TomMid, Drum::TomLow];
        let use_toms = rng.chance(0.5);
        let mut b = fill_every as usize;
        while b <= bars as usize {
            let bar0 = (b - 1) * per_bar;
            let from = bar0 + per_bar / 2;
            // Clear what played there, then the run.
            for (_, cells) in rows.iter_mut() {
                for c in cells[from..bar0 + per_bar].iter_mut() {
                    *c = (*c).min(30);
                }
            }
            let n = (per_bar / 2) / fine;
            for k in 0..n {
                let at = from + k * fine;
                let vel = 70 + (k * 50 / n.max(1)) as u8;
                if use_toms && k >= n / 2 {
                    let t = toms[(k - n / 2) * 3 / (n - n / 2).max(1)];
                    let r = row(&mut rows, t);
                    rows[r].1[at] = vel;
                } else {
                    rows[s].1[at] = vel;
                }
            }
            // A crash on the bar after.
            let after = b * per_bar;
            if after < total {
                let c = row(&mut rows, Drum::Crash);
                rows[c].1[after] = 110;
                let k = row(&mut rows, Drum::Kick);
                rows[k].1[after] = rows[k].1[after].max(110);
            }
            b += fill_every as usize;
        }
    }
    (
        per_bar as u8,
        rows.into_iter()
            .map(|(d, cells)| (d.note(), cells))
            .collect(),
    )
}
