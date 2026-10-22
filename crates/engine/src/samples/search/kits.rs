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
