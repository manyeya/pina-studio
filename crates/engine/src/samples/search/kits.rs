//! Kits: a sample for each role, chosen to go together.

use super::*;

/// What a kit is built from: a key and tempo to fit, words and traits for its sound, a sample
/// it should sound like, a pack to keep to, and the roles it needs (a hip-hop kit by default).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct KitSeed {
    pub key: Option<String>,
    pub bpm: Option<f32>,
    pub text: String,
    pub traits: Vec<String>,
    pub like: Option<PathBuf>,
    pub folder: Option<String>,
    pub roles: Vec<Category>,
    /// Alternatives to offer for each role (4 if not said).
    pub alternatives: usize,
    /// Samples to keep (a role they fill isn't picked again), as when some are locked.
    pub keep: Vec<PathBuf>,
}

/// A kit: for each role, its pick and what else would do.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Kit {
    pub slots: Vec<KitSlot>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KitSlot {
    pub role: Category,
    pub pick: Option<Hit>,
    pub alternatives: Vec<Hit>,
}

/// The roles of a kit when none are asked for: what a beat needs.
pub const KIT_ROLES: [Category; 8] = [
    Category::Kick,
    Category::Snare,
    Category::Clap,
    Category::HatClosed,
    Category::HatOpen,
    Category::Perc,
    Category::Bass808,
    Category::Keys,
];

/// Whether a kit's character suits a role: a dark kit's hats, shakers and cymbals stay bright, its
/// kick and 808 aren't made thin and bright, its snare and clap aren't made boomy.
pub(crate) fn suits(role: Category, t: &str) -> bool {
    use Category::*;
    match role {
        HatClosed | HatOpen | Shaker | Tambourine | Ride | Crash => {
            !matches!(t, "dark" | "sub" | "boomy" | "warm")
        }
        Kick | Bass808 | Tom => !matches!(t, "bright" | "airy" | "thin"),
        Snare | Clap | Rim | Snap => !matches!(t, "sub" | "boomy"),
        _ => true,
    }
}

/// Builds a kit from `seed`: each role in turn, the best fit for the words, key and tempo that
/// also goes with what's been picked so far (no clashes, one sound), so it holds together.
pub fn kit(samples: &[Sample], seed: &KitSeed) -> Kit {
    let roles = match seed.roles.is_empty() {
        true => KIT_ROLES.to_vec(),
        false => seed.roles.clone(),
    };
    let alternatives = match seed.alternatives {
        0 => 4,
        n => n,
    };
    let words = understand(&seed.text);
    // The seed's words, less any category in them (each role is its own). How they say it should
    // sound ranks, as in a search; traits picked are kept to.
    let rest = words.rest.join(" ");
    let traits = seed.traits.clone();
    let key = seed.key.clone().or(words.key.map(|k| k.name()));
    let bpm = seed.bpm.or(words.bpm);
    let mut chosen: Vec<PathBuf> = seed.keep.clone();
    chosen.extend(seed.like.iter().cloned());
    let slots = roles
        .into_iter()
        .map(|role| {
            // A kept sample in this role stays.
            if let Some((index, _)) = samples
                .iter()
                .enumerate()
                .find(|(_, s)| seed.keep.contains(&s.path) && s.analysis.category == role)
            {
                return KitSlot {
                    role,
                    pick: Some(Hit {
                        index,
                        score: 1.0,
                        reasons: vec!["kept".into()],
                        transpose: None,
                    }),
                    alternatives: Vec::new(),
                };
            }
            // A drum, or an 808 to play the bassline on, is a one-shot unless a loop's asked for.
            let shot = role.drum_note().is_some() || role == Category::Bass808;
            let kind = words.kind.or(shot.then_some(Kind::OneShot));
            // How the kit should sound, as it suits this role: a dark kit's hats aren't dark,
            // its kick isn't bright.
            let feel: String = words
                .said
                .iter()
                .zip(&words.traits)
                .filter(|(_, t)| suits(role, t))
                .map(|(w, _)| w.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let ask = |rest: &str, traits: Vec<String>, folder: Option<String>| Query {
                text: format!("{rest} {feel}"),
                categories: vec![role],
                kind,
                traits,
                key: key.clone(),
                bpm,
                folder,
                with: chosen.clone(),
                limit: alternatives + 1,
                ..Default::default()
            };
            // Loosen what was asked until something's there: the pack, the words names are
            // looked for by (a "jazzy" kit's kick needn't be called Jazz), then the traits.
            let mut hits = find(samples, &ask(&rest, traits.clone(), seed.folder.clone()));
            if hits.is_empty() && seed.folder.is_some() {
                hits = find(samples, &ask(&rest, traits.clone(), None));
            }
            if hits.is_empty() && !rest.is_empty() {
                hits = find(samples, &ask("", traits.clone(), None));
            }
            if hits.is_empty() && !traits.is_empty() {
                hits = find(samples, &ask("", Vec::new(), None));
            }
            let mut hits = hits.into_iter();
            let pick = hits.next();
            if let Some(p) = &pick {
                chosen.push(samples[p.index].path.clone());
            }
            KitSlot {
                role,
                pick,
                alternatives: hits.collect(),
            }
        })
        .collect();
    Kit { slots }
}

/// The order a folder's sounds take their pads in: the drums a beat leans on first.
const PAD_ORDER: [Category; 14] = [
    Category::Kick,
    Category::Snare,
    Category::Clap,
    Category::HatClosed,
    Category::HatOpen,
    Category::Rim,
    Category::Snap,
    Category::Tom,
    Category::Perc,
    Category::Shaker,
    Category::Tambourine,
    Category::Ride,
    Category::Crash,
    Category::Bass808,
];

/// A bank of pads.
pub const BANK: usize = 16;

/// Whether a sample is in `folder` (a library folder, as `Sample::folder` has it), or under it.
fn in_folder(s: &Sample, folder: &str) -> bool {
    s.folder == folder
        || s.folder
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// A kit from one folder's one-shots: a bank's worth of pads at most. Sixteen or fewer, each its
/// own pad; more, a pad a kind of sound (kicks, snares, hats…, a beat's drums first), then the
/// kinds there are most of again, till the bank's full. Each pad's alternatives are the folder's
/// other sounds of its kind, to step through.
pub fn folder_kit(samples: &[Sample], folder: &str) -> Kit {
    let mut by: Vec<(Category, Vec<usize>)> = Vec::new();
    for (i, s) in samples.iter().enumerate() {
        if s.analysis.kind != Kind::OneShot || !in_folder(s, folder) {
            continue;
        }
        let role = s.analysis.category;
        match by.iter_mut().find(|(r, _)| *r == role) {
            Some((_, list)) => list.push(i),
            None => by.push((role, vec![i])),
        }
    }
    // A beat's drums first, the rest after in the library's order; each kind's sounds by name.
    let rank = |r: &Category| {
        PAD_ORDER
            .iter()
            .position(|o| o == r)
            .unwrap_or(PAD_ORDER.len())
    };
    by.sort_by_key(|(r, _)| (rank(r), *r as usize));
    for (_, list) in &mut by {
        list.sort_by(|&a, &b| samples[a].name.cmp(&samples[b].name));
    }
    let hit = |index: usize| Hit {
        index,
        score: 1.0,
        reasons: vec!["in the folder".into()],
        transpose: None,
    };
    let slot = |role: Category, list: &[usize], k: usize| KitSlot {
        role,
        pick: Some(hit(list[k])),
        alternatives: list
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != k)
            .map(|(_, &i)| hit(i))
            .collect(),
    };
    let total: usize = by.iter().map(|(_, l)| l.len()).sum();
    let mut slots = Vec::new();
    match total <= BANK {
        true => {
            for (role, list) in &by {
                slots.extend((0..list.len()).map(|k| slot(*role, list, k)));
            }
        }
        false => {
            // A pad a kind, then round again for the kinds with more, most first.
            let mut taken = vec![0usize; by.len()];
            'fill: loop {
                let before = slots.len();
                for (n, (role, list)) in by.iter().enumerate() {
                    if slots.len() == BANK {
                        break 'fill;
                    }
                    if taken[n] < list.len() && (taken[n] == 0 || list.len() > taken[n] * 3) {
                        slots.push(slot(*role, list, taken[n]));
                        taken[n] += 1;
                    }
                }
                if slots.len() == before {
                    break;
                }
            }
        }
    }
    // Pads of a kind together, as they'll sit on the grid.
    slots.sort_by_key(|s| (rank(&s.role), s.role as usize));
    Kit { slots }
}

/// The library's folders that make a kit: those with four drum hits or more, by name, each
/// with how many one-shots it has.
pub fn drum_folders(samples: &[Sample]) -> Vec<(String, usize)> {
    let mut count: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for s in samples.iter().filter(|s| s.analysis.kind == Kind::OneShot) {
        let c = count.entry(s.folder.as_str()).or_default();
        c.0 += 1;
        c.1 += s.analysis.category.is_drum() as usize;
    }
    count
        .into_iter()
        .filter(|(_, (_, drums))| *drums >= 4)
        .map(|(f, (all, _))| (f.to_string(), all))
        .collect()
}

/// Other sounds a pad playing the library's `path` could play instead: its folder's others of its
/// kind first, then the library's that sound most like it. At most `limit`.
pub fn variations(samples: &[Sample], path: &std::path::Path, limit: usize) -> Vec<usize> {
    let Some(of) = samples.iter().position(|s| s.path == path) else {
        return Vec::new();
    };
    let me = &samples[of];
    let mut out: Vec<usize> = samples
        .iter()
        .enumerate()
        .filter(|&(i, s)| {
            i != of
                && s.folder == me.folder
                && s.analysis.kind == Kind::OneShot
                && s.analysis.category == me.analysis.category
        })
        .map(|(i, _)| i)
        .collect();
    out.sort_by(|&a, &b| samples[a].name.cmp(&samples[b].name));
    for h in similar(samples, path, Aspects::default(), limit + 1) {
        if h.index != of && !out.contains(&h.index) {
            out.push(h.index);
        }
    }
    out.truncate(limit);
    out
}
