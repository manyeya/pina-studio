use super::*;

#[test]
fn a_sine_lands_in_its_bin_at_about_full_power() {
    let rate = 44_100.0;
    let size = 2048;
    let k = 100;
    let f = k as f32 * rate / size as f32;
    let x: Vec<f32> = (0..size)
        .map(|i| (std::f32::consts::TAU * f * i as f32 / rate).sin())
        .collect();
    let mut s = Spectra::new(size);
    let mut p = vec![0.0; size / 2 + 1];
    s.power(&x, 0, &mut p);
    let top = (0..p.len()).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap();
    assert_eq!(top, k);
    // A Hann window's peak gain is half; scaled by 2/N that's 1/2 in amplitude, 1/4 in power.
    assert!((p[k] - 0.25).abs() < 0.01, "{}", p[k]);
}

#[test]
fn white_noise_reads_level_across_mel_bands() {
    let size = 2048;
    let mel = Mel::new(40, size, 44_100.0, 20.0, 16_000.0);
    let flat = vec![1.0; size / 2 + 1];
    let mut out = vec![0.0; 40];
    mel.apply(&flat, &mut out);
    let bin = 44_100.0 / size as f32;
    // Unit-area triangles over a flat density of 1 per bin: each band reads 1 / bin.
    for &o in &out[8..] {
        assert!((o * bin - 1.0).abs() < 0.15, "{out:?}");
    }
}

#[test]
fn loudness_matches_bs1770_for_a_sine() {
    // A 0 dBFS 997 Hz sine in one channel of two reads -3.01 LUFS.
    let rate = 48_000.0;
    let tone: Vec<f32> = (0..(rate as usize * 3))
        .map(|i| (std::f64::consts::TAU * 997.0 * i as f64 / rate).sin() as f32)
        .collect();
    let silence = vec![0.0; tone.len()];
    let l = loudness(&[tone.clone(), silence], rate, false);
    assert!((l + 3.01).abs() < 0.05, "{l}");
    // The loudest moment of a short burst: the same tone for 400 ms reads the same.
    let burst: Vec<f32> = tone[..(0.4 * rate) as usize].to_vec();
    let l = loudness(&[burst.clone(), vec![0.0; burst.len()]], rate, true);
    assert!((l + 3.01).abs() < 0.1, "{l}");
}

#[test]
fn dct_of_a_constant_is_all_in_the_first_coefficient() {
    let c = dct(&[2.0; 8], 4);
    assert!((c[0] - 2.0 * 8f32.sqrt()).abs() < 1e-4);
    assert!(c[1..].iter().all(|v| v.abs() < 1e-4));
}
