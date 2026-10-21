use super::*;

#[test]
fn knobs_typed_values_and_the_display_map_both_ways() {
    for k in [
        EqKnob::Freq,
        EqKnob::Q,
        EqKnob::Gain,
        EqKnob::Threshold,
        EqKnob::Scale,
        EqKnob::Output,
    ] {
        for v in [0.2f32, 0.5, 0.8] {
            let back = k.pos(k.value(v));
            assert!((back - v).abs() < 0.01, "{k:?} {v} {back}");
        }
    }
    let geo = Geo {
        w: 800.0,
        h: 400.0,
        lo: LOW,
        hi: HIGH,
        range: 12.0,
    };
    assert!((geo.freq(geo.x(1_000.0)) - 1_000.0).abs() < 0.01);
    assert!((geo.db(geo.y(6.0)) - 6.0).abs() < 1e-4);
    assert_eq!(hz_text(850.0), "850 Hz");
    assert_eq!(hz_text(1_200.0), "1.20 kHz");
    assert_eq!(note_text(440.0), "A4");
    // Typed: frequencies as Hz, kHz or notes (and cents); gains as dB or a ratio.
    let f = |t: &str| EqKnob::Freq.parse(t).unwrap();
    assert_eq!(f("100"), 100.0);
    assert_eq!(f("2k"), 2_000.0);
    assert_eq!(f("1.5 kHz"), 1_500.0);
    assert!((f("A4") - 440.0).abs() < 0.01);
    assert!((f("C#2+13") - 69.82).abs() < 0.1, "{}", f("C#2+13"));
    assert!((EqKnob::Gain.parse("2x").unwrap() - 6.02).abs() < 0.01);
    assert_eq!(EqKnob::Gain.parse("-3 dB"), Some(-3.0));
    assert_eq!(EqKnob::Scale.parse("50%"), Some(0.5));
    assert_eq!(EqKnob::Freq.parse("loud"), None);
    // A click makes a cut at either end, a notch low down, a bell between; pulled, shelves.
    assert_eq!(band_at(&geo, 10.0, 200.0, false).shape, Shape::LowCut);
    assert_eq!(band_at(&geo, 10.0, 200.0, true).shape, Shape::LowShelf);
    assert_eq!(band_at(&geo, 795.0, 200.0, false).shape, Shape::HighCut);
    assert_eq!(band_at(&geo, 400.0, 395.0, false).shape, Shape::Notch);
    let bell = band_at(&geo, 400.0, 100.0, false);
    assert!(bell.shape == Shape::Bell && bell.gain_db > 5.0, "{bell:?}");
    // The quick knobs find their bands, and add them.
    let mut eq = Eq::classic();
    assert_eq!(Quick::Presence.find(&eq), Some(2));
    assert_eq!(Quick::Air.find(&eq), Some(3));
    let mut empty = Eq::default();
    assert_eq!(Quick::Air.band(&mut empty), Some(0));
    set_low_cut(&mut eq, 90.0);
    assert_eq!(low_cut(&eq), 90.0);
    set_low_cut(&mut eq, 0.0);
    assert_eq!((low_cut(&eq), eq.bands.len()), (0.0, 4));
    // Pina Studio's presets are all valid EQs.
    for (name, eq) in factory() {
        assert!(!eq.bands.is_empty(), "{name}");
    }
}
