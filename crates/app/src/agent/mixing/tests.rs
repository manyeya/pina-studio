use super::*;

fn sine(hz: f32, rate: f32, seconds: f32) -> Vec<f32> {
    (0..(rate * seconds) as usize)
        .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / rate).sin())
        .collect()
}

#[test]
fn a_band_hears_its_own_frequencies() {
    let rate = 48_000.0;
    let low = sine(60.0, rate, 1.0);
    let high = sine(3000.0, rate, 1.0);
    let r = |x: &Vec<f32>| [x.clone(), x.clone()];
    let beat = 24_000;
    // A kick's band hears 60 Hz and not 3 kHz; a lead's, the other way round.
    let kick = |x: &Vec<f32>| band_peak_db(&r(x), rate, beat, [30.0, 110.0], (3.0, 100.0));
    let lead = |x: &Vec<f32>| band_peak_db(&r(x), rate, beat, [1500.0, 4500.0], (15.0, 200.0));
    let (kick_low, kick_high) = (kick(&low).unwrap().0, kick(&high).unwrap().0);
    assert!(kick_low - kick_high > 20.0, "{kick_low} {kick_high}");
    let (lead_high, lead_low) = (lead(&high).unwrap().0, lead(&low).unwrap().0);
    assert!(lead_high - lead_low > 20.0, "{lead_high} {lead_low}");
}

#[test]
fn a_duck_lets_go_before_the_next_hit() {
    // A kick that falls 25 dB by the next beat: the whole range, from where it lets go.
    let t = under(-10.0, Some(-35.0), 8.0);
    assert!((t - (-10.0 - 8.0 / 0.75 - 5.0)).abs() < 1e-3, "{t}");
    // One that rings on, 6 dB down by then: above its ring, so it lets go between hits.
    assert_eq!(under(-10.0, Some(-16.0), 8.0), -13.0);
    // A lead's dip holds while it plays: no floor.
    assert!(under(-10.0, None, 3.0) < -18.0);
}

#[test]
fn a_kicks_thump_is_found_where_it_is() {
    let rate = 48_000.0;
    let x = sine(55.0, rate, 1.0);
    let hz = kick_hz(&[x.clone(), x], rate);
    assert!((hz - 55.0).abs() <= 5.0, "{hz}");
}
