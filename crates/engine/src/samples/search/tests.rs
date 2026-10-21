use super::*;
use crate::samples::{Analysis, KeyGuess, Pitch};

fn sample(name: &str, folder: &str, a: Analysis) -> Sample {
    Sample {
        path: PathBuf::from(format!("/lib/{folder}/{name}.wav")),
        name: name.into(),
        folder: folder.into(),
        modified: 0,
        size: 0,
        analysis: a,
    }
}

fn drum(category: Category, traits: &[&str], embedding: [f32; 2]) -> Analysis {
    Analysis {
        category,
        confidence: 0.9,
        traits: traits.iter().map(|t| t.to_string()).collect(),
        embedding: vec![embedding[0], embedding[1]],
        seconds: 0.4,
        ..Default::default()
    }
}

/// The chroma of a key's triad, a little of every note under it.
fn chroma_of(key: &str) -> [f32; 12] {
    let k = Key::parse(key).unwrap();
    let third = if k.minor { 3 } else { 4 };
    let mut c = [0.02; 12];
    for step in [0, third, 7] {
        c[(k.root as usize + step) % 12] += 0.3;
    }
    c
}

fn melodic(key: &str, kind: Kind, bpm: Option<f32>) -> Analysis {
    Analysis {
        category: Category::Keys,
        kind,
        bpm,
        tonal: 0.9,
        confidence: 0.8,
        chroma: chroma_of(key),
        key: Some(KeyGuess {
            tonic: Key::parse(key).unwrap().root,
            minor: Key::parse(key).unwrap().minor,
            name: Key::parse(key).unwrap().name(),
            confidence: 0.8,
        }),
        embedding: vec![0.5, 0.5],
        ..Default::default()
    }
}

fn library() -> Vec<Sample> {
    vec![
        sample(
            "Kick_01",
            "Pack A/Drums",
            drum(Category::Kick, &["punchy", "dark"], [1.0, 0.0]),
        ),
        sample(
            "Kick_02",
            "Pack B/Drums",
            drum(Category::Kick, &["soft"], [0.9, 0.2]),
        ),
        sample(
            "Snare_01",
            "Pack A/Drums",
            drum(Category::Snare, &["snappy"], [0.0, 1.0]),
        ),
        sample(
            "Snare_02",
            "Pack B/Drums",
            drum(Category::Snare, &["snappy", "lo-fi"], [0.1, 1.0]),
        ),
        sample(
            "Hat_01",
            "Pack A/Drums",
            drum(Category::HatClosed, &["bright"], [-1.0, 0.0]),
        ),
        sample(
            "Keys_Cm",
            "Pack A/Loops",
            melodic("C minor", Kind::Loop, Some(95.0)),
        ),
        sample(
            "Keys_Dm",
            "Pack A/Loops",
            melodic("D minor", Kind::Loop, Some(140.0)),
        ),
        sample(
            "Keys_Fs",
            "Pack B/Loops",
            melodic("F# major", Kind::Loop, Some(96.0)),
        ),
        sample(
            "Sub_01",
            "Pack A/Drums",
            Analysis {
                category: Category::Bass808,
                tonal: 0.9,
                confidence: 0.9,
                pitch: Some(Pitch {
                    note: 31,
                    cents: 40.0,
                    hz: 50.0,
                    confidence: 0.9,
                }),
                embedding: vec![0.7, -0.7],
                ..Default::default()
            },
        ),
    ]
}

fn names(lib: &[Sample], hits: &[Hit]) -> Vec<String> {
    hits.iter().map(|h| lib[h.index].name.clone()).collect()
}

#[test]
fn words_say_categories_traits_keys_and_tempos() {
    let w = understand("Dusty Keys loop in C minor 95 bpm");
    assert_eq!(w.categories, [Category::Keys]);
    assert_eq!(w.kind, Some(Kind::Loop));
    assert_eq!(w.traits, ["lo-fi"]);
    assert_eq!(w.said, ["dusty"]);
    assert_eq!(w.key, Key::parse("C minor"));
    assert_eq!(w.bpm, Some(95.0));
    let w = understand("open hat Am 120bpm vinyl");
    assert_eq!(w.categories, [Category::HatOpen, Category::Texture]);
    assert_eq!(w.key, Key::parse("A minor"));
    assert_eq!(w.bpm, Some(120.0));
    assert_eq!(understand("kick").key, None);
}

#[test]
fn chips_lit_by_words_turn_off_by_taking_the_words_out() {
    assert_eq!(
        named("dusty punchy kick"),
        (
            vec![Category::Kick],
            vec!["lo-fi".to_string(), "punchy".to_string()]
        )
    );
    assert_eq!(
        unsay("dusty punchy kick", Some(Category::Kick), None),
        "dusty punchy"
    );
    assert_eq!(
        unsay("dusty punchy kick", None, Some("lo-fi")),
        "punchy kick"
    );
    assert_eq!(
        unsay("open hat loop", Some(Category::HatOpen), None),
        "loop"
    );
}

#[test]
fn a_search_in_words_finds_what_it_says() {
    let lib = library();
    let q = |text: &str| Query {
        text: text.into(),
        ..Default::default()
    };
    // How it sounds ranks: the punchy kick, then the other.
    assert_eq!(
        names(&lib, &find(&lib, &q("punchy kick"))),
        ["Kick_01", "Kick_02"]
    );
    assert_eq!(names(&lib, &find(&lib, &q("snare pack b"))), ["Snare_02"]);
    // A word with an ending finds the name it's made from, as "jazzy" finds Jazz.
    assert_eq!(names(&lib, &find(&lib, &q("snare packed b"))), ["Snare_02"]);
    // A word for a trait counts in a name too: Kick_02, called dusty, is dusty.
    let mut named = lib.clone();
    named[1].name = "Dusty Kick 02".into();
    assert_eq!(
        names(&named, &find(&named, &q("dusty kick"))),
        ["Dusty Kick 02", "Kick_01"]
    );
    // A key and tempo: the C minor loop at 95 fits as it is; F# major at 96 fits two
    // semitones up (A flat major is C minor's sixth), so it comes after, saying so; D minor at
    // 140 is too far in tempo.
    let hits = find(&lib, &q("keys loop C minor 95 bpm"));
    assert_eq!(names(&lib, &hits), ["Keys_Cm", "Keys_Fs"]);
    assert!(
        hits[0].reasons.iter().any(|r| r.contains("in C minor")),
        "{:?}",
        hits[0].reasons
    );
    assert_eq!((hits[0].transpose, hits[1].transpose), (None, Some(2)));
}

#[test]
fn a_loop_found_in_the_key_ranks_above_one_moved_into_it() {
    // A busy riff, its notes all over, that analysis is sure is in C minor; and D minor
    // chords, which fit two semitones down.
    let mut riff = melodic("C minor", Kind::Loop, Some(95.0));
    riff.chroma = chroma_of("C minor").map(|c| c + 0.1);
    let lib = vec![
        sample("Riff_Cm", "A", riff),
        sample("Keys_Dm", "A", melodic("D minor", Kind::Loop, Some(95.0))),
    ];
    let hits = find(
        &lib,
        &Query {
            text: "keys loop in C minor".into(),
            ..Default::default()
        },
    );
    assert_eq!(names(&lib, &hits), ["Riff_Cm", "Keys_Dm"]);
    assert_eq!((hits[0].transpose, hits[1].transpose), (None, Some(-2)));
}

#[test]
fn words_for_character_rank_rather_than_rule_out() {
    let mut dusty = melodic("C minor", Kind::Loop, Some(95.0));
    dusty.traits = vec!["lo-fi".into()];
    let lib = vec![
        sample(
            "Keys_Clean",
            "A",
            melodic("C minor", Kind::Loop, Some(95.0)),
        ),
        sample("Keys_Dusty", "A", dusty),
    ];
    let q = |text: &str, traits: Vec<String>| Query {
        text: text.into(),
        traits,
        ..Default::default()
    };
    let hits = find(&lib, &q("dusty keys loop", Vec::new()));
    assert_eq!(names(&lib, &hits), ["Keys_Dusty", "Keys_Clean"]);
    // A trait picked as a filter keeps to it.
    let hits = find(&lib, &q("keys loop", vec!["lo-fi".into()]));
    assert_eq!(names(&lib, &hits), ["Keys_Dusty"]);
}

#[test]
fn out_of_key_loops_say_how_far_to_move() {
    let lib = library();
    let hits = find(
        &lib,
        &Query {
            key: Some("C minor".into()),
            categories: vec![Category::Keys],
            ..Default::default()
        },
    );
    // D minor fits C minor two semitones down.
    let dm = hits
        .iter()
        .find(|h| lib[h.index].name == "Keys_Dm")
        .expect("Keys_Dm");
    assert_eq!(dm.transpose, Some(-2));
    // Drums and the 808 have nothing to clash.
    let all = find(
        &lib,
        &Query {
            key: Some("F# major".into()),
            ..Default::default()
        },
    );
    assert!(names(&lib, &all).contains(&"Sub_01".to_string()));
    assert!(names(&lib, &all).contains(&"Kick_01".to_string()));
}

#[test]
fn likeness_finds_neighbours_and_kits_hold_together() {
    let lib = library();
    let near = similar(&lib, &lib[0].path, Aspects::default(), 3);
    assert_eq!(lib[near[0].index].name, "Kick_02");
    // A kit from pack A, in C minor at 95: everything from pack A that fits.
    let k = kit(
        &lib,
        &KitSeed {
            key: Some("C minor".into()),
            bpm: Some(95.0),
            text: "punchy".into(),
            roles: vec![
                Category::Kick,
                Category::Snare,
                Category::Bass808,
                Category::Keys,
            ],
            ..Default::default()
        },
    );
    let picks: Vec<String> = k
        .slots
        .iter()
        .map(|s| {
            s.pick
                .as_ref()
                .map_or("-".into(), |p| lib[p.index].name.clone())
        })
        .collect();
    assert_eq!(picks, ["Kick_01", "Snare_01", "Sub_01", "Keys_Cm"]);
    // Words nothing in a role is called don't leave it empty; how they say it sounds ranks.
    let jazzy = kit(
        &lib,
        &KitSeed {
            text: "jazzy punchy".into(),
            roles: vec![Category::Kick],
            ..Default::default()
        },
    );
    let kick = jazzy.slots[0]
        .pick
        .as_ref()
        .map(|p| lib[p.index].name.as_str());
    assert_eq!(kick, Some("Kick_01"));
    let snare = k.slots[1].pick.as_ref().unwrap();
    assert!(
        snare.reasons.iter().any(|r| r.contains("same pack")),
        "{:?}",
        snare.reasons
    );
}

#[test]
fn a_pitched_one_shot_plays_in_tune_across_the_keys() {
    let lib = library();
    let z = zone(&lib[8], None);
    assert_eq!(
        (z.root, z.high, z.tune_cents, z.gate),
        (Some(31), Some(127), -40.0, true)
    );
    let k = zone(&lib[0], None);
    assert_eq!((k.note, k.high), (36, None));
    assert_eq!(zone(&lib[4], None).choke, Some(1));
    // A clean low tone the pitch tracker is only half sure of still plays in tune.
    let mut low = lib[8].clone();
    low.analysis.tonal = 0.99;
    low.analysis.pitch.as_mut().unwrap().confidence = 0.45;
    assert_eq!(zone(&low, None).root, Some(31));
    low.analysis.tonal = 0.6;
    assert_eq!(zone(&low, None).root, None);
}

#[test]
fn the_map_gathers_alike_sounds_into_clusters() {
    // Three kinds of sound, twenty each, near their own kind in the embedding.
    let mut lib = Vec::new();
    for (c, centre) in [[1.0f32, 0.0], [-0.5, 0.86], [-0.5, -0.86]]
        .iter()
        .enumerate()
    {
        for i in 0..20 {
            let wobble = ((i * 37 + c * 11) % 17) as f32 / 17.0 * 0.3 - 0.15;
            let mut e = vec![0.0; 8];
            e[0] = centre[0] + wobble;
            e[1] = centre[1] - wobble * 0.5;
            e[2 + c] = 0.5;
            lib.push(sample(
                &format!("S{c}_{i}"),
                "A",
                Analysis {
                    embedding: e,
                    ..drum(Category::Kick, &[], [0.0, 0.0])
                },
            ));
        }
    }
    let m = map(&lib);
    let d = |a: usize, b: usize| ((m[a][0] - m[b][0]).powi(2) + (m[a][1] - m[b][1]).powi(2)).sqrt();
    let mean = |pairs: &mut dyn Iterator<Item = (usize, usize)>| {
        let v: Vec<f32> = pairs.map(|(a, b)| d(a, b)).collect();
        v.iter().sum::<f32>() / v.len() as f32
    };
    let within = mean(&mut (0..60).flat_map(|a| {
        (0..60)
            .filter(move |b| a / 20 == b / 20 && a != *b)
            .map(move |b| (a, b))
    }));
    let between = mean(&mut (0..60).flat_map(|a| {
        (0..60)
            .filter(move |b| a / 20 != b / 20)
            .map(move |b| (a, b))
    }));
    assert!(between > within * 3.0, "within {within}, between {between}");
}

#[test]
fn the_map_spreads_samples_by_sound() {
    let lib = library();
    let m = map(&lib);
    assert_eq!(m.len(), lib.len());
    assert!(m.iter().all(|p| p[0].abs() <= 1.0 && p[1].abs() <= 1.0));
    // The two kicks sit nearer each other than the kick and the hat.
    let d = |a: usize, b: usize| ((m[a][0] - m[b][0]).powi(2) + (m[a][1] - m[b][1]).powi(2)).sqrt();
    assert!(d(0, 1) < d(0, 4), "{m:?}");
}

#[test]
fn a_drum_is_judged_as_its_kind_of_drum() {
    let hat = |tail: f32, traits: &[&str]| {
        let mut a = drum(Category::HatClosed, traits, [0.5, 0.5]);
        a.envelope.decay_ms = 40.0;
        a.envelope.tail_ms = tail;
        a
    };
    // A tight hat beats one that rings on in reverb, and the reverb is said.
    let tight = profile(&hat(180.0, &["snappy", "dry"])).unwrap();
    let washy = profile(&hat(1100.0, &["wet"])).unwrap();
    assert!(
        tight.0 > washy.0 && washy.1.is_some(),
        "{tight:?} {washy:?}"
    );
    // An 808 with no note can't play a bassline.
    let mut toneless = drum(Category::Bass808, &["sub"], [0.5, 0.5]);
    toneless.envelope.decay_ms = 900.0;
    assert!(profile(&toneless).unwrap().1.unwrap().contains("note"));
    // A loop isn't judged as a hit.
    let mut beat = hat(180.0, &[]);
    beat.kind = Kind::Loop;
    assert!(profile(&beat).is_none());
}

#[test]
fn a_dark_kit_keeps_its_hats_bright_and_its_kick_low() {
    assert!(!suits(Category::HatClosed, "dark"));
    assert!(suits(Category::Kick, "dark"));
    assert!(!suits(Category::Kick, "bright"));
    assert!(suits(Category::HatOpen, "bright"));
    assert!(!suits(Category::Snare, "boomy"));
}

#[test]
fn a_pitched_drum_is_tuned_onto_the_key_by_the_nearer_way() {
    let mut a = drum(Category::Kick, &["punchy", "sub"], [0.5, 0.5]);
    a.tonal = 0.8;
    // Ab1 a little flat: two and a half semitones above F1, the key's root.
    a.pitch = Some(Pitch {
        note: 32,
        cents: -43.0,
        hz: 50.7,
        confidence: 0.9,
    });
    let s = sample("Kick", "Pack", a);
    let key = Key::parse("F minor").unwrap();
    let mut z = zone(&s, None);
    let why = tune_to(&mut z, &s, key).unwrap();
    // Down three semitones, back up 43 cents: on F.
    assert_eq!((z.note, z.root, z.tune_cents), (36, Some(39), 43.0));
    assert!(why.contains("onto F"), "{why}");
    // A snare has no note to tune; a kick already on the key's fifth stays.
    let mut on_fifth = s.clone();
    on_fifth.analysis.pitch = Some(Pitch {
        note: 36,
        cents: 0.0,
        hz: 65.4,
        confidence: 0.9,
    });
    let mut z = zone(&on_fifth, None);
    assert!(tune_to(&mut z, &on_fifth, key).is_none());
    let snare = sample("Snare", "Pack", drum(Category::Snare, &[], [0.5, 0.5]));
    let mut z = zone(&snare, None);
    assert!(tune_to(&mut z, &snare, key).is_none());
}
