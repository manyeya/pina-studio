use super::*;

const RATE: f64 = 44_100.0;

/// Hits of decaying noise at `times` (seconds), `seconds` long.
fn drums(times: &[f64], seconds: f64) -> Vec<Vec<f32>> {
    let n = (seconds * RATE) as usize;
    let mut v = vec![0f32; n];
    let mut seed = 1u32;
    for &t in times {
        let at = (t * RATE) as usize;
        for j in 0..(0.12 * RATE) as usize {
            if at + j >= n {
                break;
            }
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 9) as f32 / (1 << 23) as f32 * 2.0 - 1.0;
            v[at + j] += noise * 0.8 * (-(j as f32) / (0.02 * RATE as f32)).exp();
        }
    }
    vec![v.clone(), v]
}

/// A C minor chord that swells in over a quarter second and holds.
fn pad(seconds: f64) -> Vec<Vec<f32>> {
    let n = (seconds * RATE) as usize;
    let v: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / RATE;
            let swell = (t / 0.25).min(1.0) as f32;
            // C loudest, then Eb and G.
            [(261.63, 0.3), (311.13, 0.1), (392.0, 0.1)]
                .iter()
                .map(|(f, a)| (std::f64::consts::TAU * f * t).sin() as f32 * a)
                .sum::<f32>()
                * swell
        })
        .collect();
    vec![v]
}

/// The strongest frequency, by a slow scan of the middle of `v`, in Hz.
fn peak_hz(v: &[f32]) -> f64 {
    let mid = &v[v.len() / 4..v.len() * 3 / 4];
    (200..500)
        .map(|f| {
            let f = f as f64;
            let (mut re, mut im) = (0.0, 0.0);
            for (i, s) in mid.iter().enumerate() {
                let a = std::f64::consts::TAU * f * i as f64 / RATE;
                re += *s as f64 * a.cos();
                im += *s as f64 * a.sin();
            }
            (f, re * re + im * im)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap()
        .0
}

#[test]
fn hits_are_found_to_the_millisecond_and_swells_are_not_hits() {
    let times = [0.1, 0.35, 0.6, 0.85];
    let found = onsets(&drums(&times, 1.0), RATE);
    assert_eq!(found.len(), 4, "{found:?}");
    for (f, t) in found.iter().zip(times) {
        let ms = (*f as f64 / RATE - t) * 1000.0;
        assert!((-2.0..=0.5).contains(&ms), "{ms} ms off at {t}");
    }
    assert!(onsets(&pad(1.0), RATE).is_empty());
}

#[test]
fn polyphonic_keeps_hits_on_time_and_sharp() {
    let times = [0.1, 0.35, 0.6, 0.85];
    let input = AudioData {
        channels: drums(&times, 1.0),
    };
    for ratio in [0.8, 1.25] {
        let out = render(&input, &Fit::ratio(ratio), RATE);
        assert_eq!(out.channels[0].len(), out_len(44_100, ratio));
        let found = onsets(&out.channels, RATE);
        assert_eq!(found.len(), 4, "{ratio}: {found:?}");
        for (f, t) in found.iter().zip(times) {
            let ms = (*f as f64 / RATE - t * ratio) * 1000.0;
            assert!(ms.abs() <= 2.0, "{ratio}: {ms} ms off at {t}");
        }
        // The attacks are the recording's, peak for peak, with nothing smeared ahead of them.
        let peak = |v: &[f32], at: usize| {
            v[at..at + (0.01 * RATE) as usize]
                .iter()
                .fold(0f32, |m, s| m.max(s.abs()))
        };
        let a = peak(&input.channels[0], (0.35 * RATE) as usize);
        let b = peak(&out.channels[0], (0.35 * ratio * RATE) as usize);
        assert!((a - b).abs() < 0.02, "{ratio}: {a} {b}");
        let before = |v: &[f32], at: f64| {
            let at = (at * RATE) as usize;
            let part = &v[at - (0.02 * RATE) as usize..at - (0.002 * RATE) as usize];
            (part.iter().map(|s| s * s).sum::<f32>() / part.len() as f32).sqrt()
        };
        for t in &times[1..] {
            let (was, now) = (
                before(&input.channels[0], *t),
                before(&out.channels[0], t * ratio),
            );
            assert!(
                now <= was * 1.5 + 1e-3,
                "{ratio}: {now} before {t}, {was} recorded"
            );
        }
    }
}

#[test]
fn polyphonic_keeps_a_chord_in_tune_and_moves_it_when_asked() {
    let input = AudioData { channels: pad(1.0) };
    let out = render(&input, &Fit::ratio(1.3), RATE);
    assert_eq!(out.channels[0].len(), out_len(44_100, 1.3));
    assert!((peak_hz(&out.channels[0]) - 261.0).abs() < 3.0);
    // Up three semitones, as long as it was: C becomes Eb.
    let up = render(
        &input,
        &Fit {
            semitones: 3.0,
            ..Fit::ratio(1.0)
        },
        RATE,
    );
    assert_eq!(up.channels[0].len(), 44_100);
    assert!((peak_hz(&up.channels[0]) - 311.1).abs() < 4.0);
}

#[test]
fn slicing_plays_each_hit_as_recorded_at_its_new_time() {
    let times = [0.1, 0.35, 0.6, 0.85];
    let input = AudioData {
        channels: drums(&times, 1.0),
    };
    let out = render(
        &input,
        &Fit {
            flex: Flex::Slicing,
            ..Fit::ratio(1.5)
        },
        RATE,
    );
    let src = &input.channels[0];
    let found = onsets(&input.channels, RATE);
    for &h in &found {
        let o = out_len(h, 1.5);
        assert_eq!(&out.channels[0][o..o + 1000], &src[h..h + 1000]);
    }
    // After a slice, its slot is silent.
    let gap = out_len(found[1], 1.5) - 10;
    assert_eq!(out.channels[0][gap], 0.0);
}

#[test]
fn transposed_hits_stay_sharp_and_nothing_comes_before_them() {
    let times = [0.1, 0.35, 0.6, 0.85];
    let input = AudioData {
        channels: drums(&times, 1.0),
    };
    let fit = Fit {
        semitones: 4.0,
        ..Fit::ratio(1.25)
    };
    let out = render(&input, &fit, RATE);
    let rms = |v: &[f32], a: f64, b: f64| {
        let part = &v[(a * RATE) as usize..(b * RATE) as usize];
        (part.iter().map(|s| s * s).sum::<f32>() / part.len() as f32).sqrt()
    };
    for t in &times[1..] {
        let at = t * 1.25;
        // Loud at the hit, quiet in the 20 ms before it, as recorded.
        let (hit, before) = (
            rms(&out.channels[0], at, at + 0.01),
            rms(&out.channels[0], at - 0.02, at - 0.002),
        );
        let was = rms(&input.channels[0], t - 0.02, t - 0.002);
        assert!(hit > 0.2, "{t}: {hit}");
        assert!(
            before <= was * 1.5 + 2e-3,
            "{t}: {before} before, {was} recorded"
        );
    }
}

#[test]
fn warp_markers_move_a_hit_and_fit_the_rest_every_way() {
    let times = [0.1, 0.35, 0.6, 0.85];
    let input = AudioData {
        channels: drums(&times, 1.0),
    };
    let hits = onsets(&input.channels, RATE);
    // The third hit 60 ms late, the hits either side pinned where they are: the audio between them stretches and squeezes to fit.
    let moved = hits[2] + (0.06 * RATE) as usize;
    for flex in Flex::ALL {
        let fit = Fit {
            flex,
            warps: vec![(hits[1], hits[1]), (hits[2], moved), (hits[3], hits[3])],
            ..Fit::ratio(1.0)
        };
        let out = render(&input, &fit, RATE);
        assert_eq!(out.channels[0].len(), 44_100, "{flex:?}");
        let found = onsets(&out.channels, RATE);
        let near = |at: usize| found.iter().any(|&f| (f as i64 - at as i64).abs() < 90);
        assert!(near(moved), "{flex:?}: {found:?}, wanted {moved}");
        assert!(
            near(hits[0]) && near(hits[1]) && near(hits[3]),
            "{flex:?}: {found:?}"
        );
    }
}

#[test]
fn a_voice_keeps_its_formants_when_asked() {
    let input = AudioData { channels: pad(1.0) };
    let up = |formants| {
        let fit = Fit {
            semitones: 5.0,
            formants,
            ..Fit::ratio(1.0)
        };
        render(&input, &fit, RATE).channels[0].clone()
    };
    let (kept, moved) = (up(true), up(false));
    assert_eq!(kept.len(), moved.len());
    assert!(kept.iter().zip(&moved).any(|(a, b)| (a - b).abs() > 1e-3));
}

#[test]
fn speed_is_a_turntable() {
    let input = AudioData { channels: pad(1.0) };
    let out = render(
        &input,
        &Fit {
            semitones: 7.0,
            flex: Flex::Speed,
            ..Fit::ratio(1.25)
        },
        RATE,
    );
    assert_eq!(out.channels[0].len(), out_len(44_100, 1.25));
    // A quarter slower: lower by that, the transposition ignored.
    assert!((peak_hz(&out.channels[0]) - 261.63 / 1.25).abs() < 3.0);
}
