use super::*;

fn heard(like: Category, attack: f32, sustain: f32, low: f32, traits: &[&str]) -> Heard {
    Heard {
        sounds_like: vec![(like, 0.6)],
        traits: traits.iter().map(|t| t.to_string()).collect(),
        attack_ms: attack,
        decay_ms: 400.0,
        sustain,
        centroid_hz: 800.0,
        bands_db: [low, low, -6.0, -8.0, -15.0, -20.0],
        lufs: -18.0,
        peak_db: -3.0,
        width_db: -10.0,
    }
}

#[test]
fn a_part_wants_its_own_kind_of_sound() {
    let pad = heard(Category::Pad, 600.0, 0.8, -12.0, &["soft", "wide"]);
    let bass = heard(Category::Bass, 8.0, 0.7, -2.0, &["dark", "sub"]);
    let pluck = heard(Category::Pluck, 5.0, 0.1, -14.0, &["bright"]);
    let none: Vec<String> = Vec::new();
    // A pad for the pad, a bass for the bass, not the other way round.
    let j = |part: &str, h: &Heard, asked: &[String]| judge(part, h, asked, &[]);
    assert!(j("pad", &pad, &none).0 > j("pad", &bass, &none).0);
    assert!(j("bass", &bass, &none).0 > j("bass", &pad, &none).0);
    assert!(j("arp", &pluck, &none).0 > j("arp", &pad, &none).0);
    // The character asked for counts.
    let dark = vec!["dark".to_string()];
    let bright_bass = heard(Category::Bass, 8.0, 0.7, -2.0, &["bright"]);
    assert!(j("bass", &bass, &dark).0 > j("bass", &bright_bass, &dark).0);
    // A slow swell is said, for a part that wants its notes on time.
    assert!(
        j("lead", &pad, &none)
            .1
            .iter()
            .any(|w| w.contains("attack"))
    );
    // Keys play chords more than a synth does; an instrument asked for by name, more still.
    let keys = heard(Category::Keys, 5.0, 0.4, -14.0, &[]);
    let synth = heard(Category::Synth, 5.0, 0.4, -14.0, &[]);
    assert!(j("chords", &keys, &none).0 > j("chords", &synth, &none).0);
    let brass = heard(Category::Brass, 20.0, 0.6, -14.0, &[]);
    let lead = heard(Category::Lead, 20.0, 0.6, -14.0, &[]);
    let asked = [Category::Brass];
    assert!(judge("lead", &brass, &none, &asked).0 > judge("lead", &lead, &none, &asked).0);
}
