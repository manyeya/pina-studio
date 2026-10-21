use super::*;

#[test]
fn samplers_are_named_for_what_they_play() {
    use pina_engine::song::project::{Sampler, Zone};
    let zone = |file: &str, high| Zone {
        file: file.into(),
        note: 36,
        high,
        ..Zone::default()
    };
    let multi = Sampler {
        zones: vec![
            zone("Moog_Bass_C1.wav", Some(36)),
            zone("Moog_Bass_A#2.wav", Some(46)),
        ],
        ..Default::default()
    };
    assert_eq!(sampler_name(&multi), "Moog Bass");
    let mixed = Sampler {
        zones: vec![zone("Sub.wav", Some(48)), zone("Bass.wav", Some(60))],
        ..Default::default()
    };
    assert_eq!(sampler_name(&mixed), "2 zones");
    let kit = Sampler {
        zones: vec![zone("Kick.wav", None), zone("Snare.wav", None)],
        ..Default::default()
    };
    assert_eq!(sampler_name(&kit), "2-piece kit");
    let kit = Some(Instrument::Sampler(kit));
    assert_eq!(instrument_kind(kit.as_ref()), "Kit");
}
