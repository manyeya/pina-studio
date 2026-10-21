use super::*;

const RATE: f64 = 44_100.0;

fn decaying(hz: f32, seconds: f32, decay_per_second_db: f32) -> Vec<f32> {
    (0..(seconds * RATE as f32) as usize)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            (std::f32::consts::TAU * hz * t).sin() * 10f32.powf(-decay_per_second_db * t / 20.0)
        })
        .collect()
}

#[test]
fn an_envelope_times_its_decay_and_tail() {
    // 100 dB a second: 20 dB down at 200 ms, 60 dB at 600 ms.
    let x = decaying(200.0, 1.0, 100.0);
    let f = extract(&[x], RATE, None);
    let e = f.envelope;
    assert!(e.attack_ms < 5.0, "{e:?}");
    assert!((e.decay_ms - 200.0).abs() < 15.0, "{e:?}");
    assert!((e.tail_ms - 600.0).abs() < 25.0, "{e:?}");
    assert!(e.sustain < 0.2, "{e:?}");
}

#[test]
fn spectrum_and_width_say_where_the_energy_is() {
    let x = decaying(40.0, 1.0, 20.0);
    let f = extract(&[x.clone(), x.clone()], RATE, None);
    assert!(f.spectrum.bands_db[0] > -1.0, "{:?}", f.spectrum);
    assert!(f.spectrum.flatness < 0.05, "{:?}", f.spectrum);
    assert_eq!(f.width, 0.0);
    let p = f.pitch.unwrap();
    assert!((p.hz - 40.0).abs() < 0.5, "{p:?}");
    let flipped: Vec<f32> = x.iter().map(|v| -v).collect();
    assert!(extract(&[x, flipped], RATE, None).width > 0.99);
}

#[test]
fn noise_is_flat() {
    let mut seed = 3u32;
    let x: Vec<f32> = (0..22_050)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        })
        .collect();
    let f = extract(&[x], RATE, None);
    assert!(f.spectrum.flatness > 0.8, "{:?}", f.spectrum);
}
