use super::*;
use crate::song::project::{Note, Region, Sampler, Zone};

fn track(name: &str, instrument: Option<Instrument>) -> Track {
    Track {
        name: name.into(),
        instrument,
        ..Default::default()
    }
}

#[test]
fn a_track_looks_like_what_it_plays() {
    let none = |_: &Path| None;
    // A sampler of drum hits is a kit, whatever it's called.
    let kit = track(
        "Untitled",
        Some(Instrument::Sampler(Sampler {
            zones: ["k.wav", "s.wav", "h.wav"]
                .iter()
                .map(|f| Zone {
                    file: f.into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })),
    );
    let heard = |p: &Path| match p.to_str() {
        Some("k.wav") => Some(Category::Kick),
        Some("s.wav") => Some(Category::Snare),
        _ => Some(Category::HatClosed),
    };
    let l = look(&kit, &heard);
    assert_eq!(
        (l.icon, l.family, l.source),
        ("drums", Family::Drums, Source::Sound)
    );
    // Audio: what its regions hold, over what it's called ("Chords" of sung pads is vocals).
    let mut sung = track("Chords", None);
    sung.regions.push(Region {
        length: 3840,
        content: Content::Audio {
            file: "pad.wav".into(),
            offset: 0,
            stretch: 1.0,
            flex: Default::default(),
            follow: false,
            formants: false,
            warps: Vec::new(),
        },
        ..Default::default()
    });
    let vocal = |_: &Path| Some(Category::Vocal);
    assert_eq!(look(&sung, &vocal).icon, "vocals");
    // Its name, within the family its sound says, is more exact.
    sung.name = "Vocal Chop".into();
    assert_eq!(look(&sung, &vocal).icon, "chop");
    sung.name = "Chords".into();
    assert_eq!(look(&sung, &none).icon, "chords");
    // A synth playing low single notes is bass; its name, when it says, comes first.
    let mut low = track("Synth 2", Some(Instrument::Synth));
    low.regions.push(Region {
        length: 3840,
        content: Content::Midi {
            notes: vec![Note {
                start: 0,
                length: 960,
                pitch: 36,
                velocity: 100,
            }],
            controls: Vec::new(),
        },
        ..Default::default()
    });
    let l = look(&low, &none);
    assert_eq!((l.icon, l.source), ("bass", Source::Notes));
    low.name = "Lead Vox".into();
    assert_eq!(look(&low, &none).icon, "vocals");
    // Chosen wins; a mark keeps the family its sound says.
    low.icon = Some("strings".into());
    assert_eq!(look(&low, &none).family, Family::Strings);
    low.icon = Some("star".into());
    let l = look(&low, &none);
    assert_eq!((l.icon, l.family), ("star", Family::Vocals));
}

#[test]
fn every_icon_is_findable_and_colours_read() {
    assert!(ICONS.iter().all(|i| icon(i.id).is_some()));
    assert_eq!(parse_color("#d97757"), Some(0xd97757));
    assert_eq!(parse_color("d97757"), None);
    assert_eq!(named("Kick 02", NAMED), Some("kick"));
    assert_eq!(named("Kickstart", NAMED), None);
    assert_eq!(named("Serum 2", PLUGINS), Some("synth"));
}
