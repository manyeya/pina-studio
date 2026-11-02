//! A kit of parts: a MIDI file for each part of a song, chosen to go together. What makes parts go
//! together, most first: coming from one pack; playing the same roots bar by bar once they're in
//! one key; minor with minor; tempos a part can follow; loops that come round together. Harmony is
//! chosen first, then the rest to go with it, so it holds together as a sample kit does.

use super::*;

/// The parts of a kit, as they're shown, when none are asked for.
pub const KIT_PARTS: [&str; 8] = [
    "drums", "bass", "chords", "keys", "pad", "pluck", "lead", "arp",
];
/// The order they're chosen in: what sets the harmony, what follows it, and drums, which only
/// need the tempo.
const CHOSEN_IN: [&str; 8] = [
    "chords", "keys", "pad", "bass", "lead", "pluck", "arp", "drums",
];
/// Of these, only the first there is plays from the start: three kinds of chords at once is a mess.
const HARMONY: [&str; 3] = ["chords", "keys", "pad"];
const MELODY: [&str; 3] = ["lead", "pluck", "arp"];

/// What a kit is built from: a key and tempo to keep to, words for it, the parts it needs (all
/// there are if not said), how many alternatives each has (4 if not said), files to keep, as when
/// some are locked, and which of the kits that would do (0 the best: `New picks` counts up).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct MidiKitSeed {
    pub key: Option<String>,
    pub tempo: Option<f32>,
    pub words: String,
    pub parts: Vec<String>,
    pub alternatives: usize,
    /// The part each is kept as.
    pub keep: Vec<(String, PathBuf)>,
    pub variation: u64,
}

/// A kit, every part's `transpose` taking it into `key`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct MidiKit {
    pub key: Option<String>,
    pub slots: Vec<MidiKitSlot>,
}

/// A part: its pick, what else would do, and whether it plays from the start.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MidiKitSlot {
    pub role: String,
    pub pick: Option<MidiHit>,
    pub alternatives: Vec<MidiHit>,
    pub on: bool,
}

/// A file's bars' roots, round its loop, once `shift` semitones are added.
fn roots(m: &MidiSample, shift: i32) -> Vec<Option<u8>> {
    let bars = m.study.chords.len().min(m.study.loop_bars.max(1) as usize);
    let root = |c: &String| Chord::parse(c).map(|c| (c.root as i32 + shift).rem_euclid(12) as u8);
    m.study.chords[..bars].iter().map(root).collect()
}

/// How often two run of roots agree, 0 to 1, each going round; none if they never both sound.
fn agreement(a: &[Option<u8>], b: &[Option<u8>]) -> Option<f32> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let (mut same, mut both) = (0, 0);
    for bar in 0..a.len().max(b.len()) {
        if let (Some(x), Some(y)) = (a[bar % a.len()], b[bar % b.len()]) {
            both += 1;
            same += (x == y) as u32;
        }
    }
    (both > 0).then(|| same as f32 / both as f32)
}

fn own_key(m: &MidiSample) -> Option<Key> {
    m.study.key.as_deref().and_then(Key::parse)
}

/// The semitones taking a file into `key`: none for drums, or with no key to go on.
fn into_key(m: &MidiSample, key: Option<Key>) -> i32 {
    match (own_key(m), key) {
        (Some(from), Some(to)) if !m.study.all_drums() => transpose_to(&from, &to),
        _ => 0,
    }
}

/// How well `m` goes with what's been chosen, 0 to 1, and why.
fn goes_with(
    midi: &[MidiSample],
    m: &MidiSample,
    chosen: &[usize],
    key: Option<Key>,
) -> (f32, Vec<String>) {
    if chosen.is_empty() {
        return (0.0, Vec::new());
    }
    let (mut sum, mut pack, mut same_roots) = (0.0, false, false);
    for other in chosen.iter().map(|&c| &midi[c]) {
        let mut parts = vec![(((m.folder == other.folder) as u8) as f32, 3.0)];
        pack |= m.folder == other.folder;
        if let (Some(a), Some(b)) = (own_key(m), own_key(other)) {
            parts.push((((a.minor == b.minor) as u8) as f32, 1.0));
            let at = key.unwrap_or(b);
            let heard = (
                roots(m, into_key(m, Some(at))),
                roots(other, into_key(other, Some(at))),
            );
            if let Some(share) = agreement(&heard.0, &heard.1) {
                parts.push((share, 3.0));
                same_roots |= share >= 0.75;
            }
        }
        if let (Some(a), Some(b)) = (m.study.tempo, other.study.tempo) {
            parts.push(((1.0 - tempo_off(a, b) / NEAR_TEMPO).clamp(0.0, 1.0), 1.0));
        }
        let (a, b) = (m.study.loop_bars.max(1), other.study.loop_bars.max(1));
        let round = a.is_multiple_of(b) || b.is_multiple_of(a);
        parts.push((((round) as u8) as f32, 0.5));
        let weight: f32 = parts.iter().map(|p| p.1).sum();
        sum += parts.iter().map(|(s, w)| s * w).sum::<f32>() / weight;
    }
    let mut why = Vec::new();
    if pack {
        why.push("from the same pack".to_string());
    }
    if same_roots {
        why.push("plays the same roots".to_string());
    }
    (sum / chosen.len() as f32, why)
}

/// How many of the parts there are a pack has, 0 to 1.
fn reach_of_pack(midi: &[MidiSample], folder: &str) -> f32 {
    let plays = |m: &&MidiSample, role: &str| m.study.parts.iter().any(|p| p.role == role);
    let there = |of: &dyn Fn(&&MidiSample) -> bool, role: &&str| {
        midi.iter().filter(of).any(|m| plays(&m, role))
    };
    let all = KIT_PARTS.iter().filter(|r| there(&|_| true, r)).count();
    let own = KIT_PARTS
        .iter()
        .filter(|r| there(&|m| m.folder == folder, r))
        .count();
    own as f32 / all.max(1) as f32
}

/// The files for a part, best first: those that fit if any do, and go with what's chosen.
fn ranked(
    midi: &[MidiSample],
    role: &str,
    seed: &MidiKitSeed,
    key: Option<Key>,
    chosen: &[usize],
) -> Vec<MidiHit> {
    let mut hits = find(
        midi,
        &MidiQuery {
            words: seed.words.clone(),
            role: Some(role.to_string()),
            key: seed.key.clone(),
            tempo: seed.tempo,
            limit: Some(usize::MAX),
        },
    );
    hits.retain(|h| !chosen.contains(&h.index));
    let fit = |h: &MidiHit| fits(&midi[h.index], key, seed.tempo, true);
    if hits.iter().any(fit) {
        hits.retain(fit);
    }
    for h in &mut hits {
        let m = &midi[h.index];
        // A file of just this part, not one that has it among others.
        if m.study.role == role && m.study.parts.len() <= 1 {
            h.score += 2.0;
        }
        let (with, why) = goes_with(midi, m, chosen, key);
        h.score += 6.0 * with;
        h.why.extend(why);
        // The first starts the kit: best from a pack that can fill the other parts too.
        if chosen.is_empty() {
            h.score += 3.0 * reach_of_pack(midi, &m.folder);
        }
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits
}

/// Builds a kit from `seed`: each part in turn, the best fit that goes with those before it.
pub fn kit(midi: &[MidiSample], seed: &MidiKitSeed) -> MidiKit {
    let alternatives = match seed.alternatives {
        0 => 4,
        n => n,
    };
    let wanted = seed.key.as_deref().and_then(Key::parse);
    let index_of = |p: &PathBuf| midi.iter().position(|m| &m.path == p);
    let kept: Vec<(&str, usize)> = seed
        .keep
        .iter()
        .filter_map(|(role, p)| Some((&role[..], index_of(p)?)))
        .collect();
    let mut chosen: Vec<usize> = kept.iter().map(|k| k.1).collect();
    let mut made: Vec<MidiKitSlot> = Vec::new();
    let mut varied = false;
    let mut key = wanted.or(chosen.iter().find_map(|&c| own_key(&midi[c])));
    for role in CHOSEN_IN {
        if !seed.parts.is_empty() && !seed.parts.iter().any(|p| p == role) {
            continue;
        }
        let has = |m: &MidiSample| m.study.parts.iter().any(|p| p.role == role);
        if !midi.iter().any(has) {
            continue;
        }
        let kept = kept.iter().find(|k| k.0 == role).map(|k| k.1);
        let mut hits = ranked(midi, role, seed, key, &chosen);
        let pick = match kept {
            Some(index) => Some(MidiHit {
                index,
                score: 1.0,
                why: vec!["kept".into()],
                transpose: 0,
            }),
            None if hits.is_empty() => None,
            // New picks: the first part chosen takes another of its best, the rest follow it.
            // Only of its best: it mustn't walk away from what's been kept.
            None => {
                let best = hits[0].score;
                let good = hits.iter().filter(|h| h.score >= best - 1.5).count();
                let reach = good.min(alternatives + 1) as u64;
                let at = if varied { 0 } else { seed.variation % reach };
                varied = true;
                Some(hits.remove(at as usize))
            }
        };
        if let Some(p) = pick.as_ref().filter(|_| kept.is_none()) {
            chosen.push(p.index);
            key = key.or(own_key(&midi[p.index]));
        }
        hits.truncate(alternatives);
        made.push(MidiKitSlot {
            role: role.to_string(),
            pick,
            alternatives: hits,
            on: true,
        });
    }
    for slot in &mut made {
        for h in slot.pick.iter_mut().chain(&mut slot.alternatives) {
            h.transpose = into_key(&midi[h.index], key);
        }
    }
    let first_of = |family: &[&str], made: &[MidiKitSlot]| {
        let there = |r: &&str| made.iter().any(|s| s.role == *r && s.pick.is_some());
        family.iter().copied().find(there).map(str::to_string)
    };
    let (harmony, melody) = (first_of(&HARMONY, &made), first_of(&MELODY, &made));
    for slot in &mut made {
        let family = HARMONY.contains(&&*slot.role) || MELODY.contains(&&*slot.role);
        let leads = [&harmony, &melody]
            .iter()
            .any(|f| f.as_ref() == Some(&slot.role));
        slot.on = slot.pick.is_some() && (!family || leads);
    }
    let place = |s: &MidiKitSlot| KIT_PARTS.iter().position(|p| *p == s.role);
    made.sort_by_key(place);
    MidiKit {
        key: key.map(|k| k.name()),
        slots: made,
    }
}
