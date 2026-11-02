use super::*;

/// How long a channel strip takes, all three effects on, as a share of real time: run with
/// `cargo test --release -p pina-engine strip_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn strip_cost() {
    let rate = 48_000.0;
    let mut strip = Strip::new(rate);
    let (eq, comp, sat) = (
        Eq {
            bands: crate::song::project::Bands::from_slice(&[
                crate::song::project::EqBand::new(crate::song::project::Shape::LowCut, 80.0, 0.0),
                crate::song::project::EqBand::new(crate::song::project::Shape::Bell, 300.0, -3.0),
                crate::song::project::EqBand::new(crate::song::project::Shape::Bell, 3_000.0, 2.0),
                crate::song::project::EqBand::new(
                    crate::song::project::Shape::HighShelf,
                    10_000.0,
                    2.0,
                ),
            ]),
            ..Eq::default()
        },
        Comp::default(),
        Saturator::default(),
    );
    let seconds = 10;
    let mut l: Vec<f32> = (0..MAX_BLOCK)
        .map(|i| (i as f32 * 0.05).sin() * 0.5)
        .collect();
    let mut r = l.clone();
    let mut time = |e: Option<&Eq>, c: Option<&Comp>, s: Option<&Saturator>| {
        let began = std::time::Instant::now();
        for _ in 0..(seconds * 48_000) / MAX_BLOCK {
            strip.process(e, None, c, s, &mut l, &mut r, None, None);
        }
        began.elapsed().as_secs_f64() / seconds as f64 * 100.0
    };
    let (e, c, s) = (Some(&eq), Some(&comp), Some(&sat));
    let parts = [
        time(e, None, None),
        time(None, c, None),
        time(None, None, s),
    ];
    let all = time(e, c, s);
    // The saturator at its most: six bands, each eight times over.
    let mut most = Saturator {
        bands: crate::song::project::SatBands::from_slice(
            &[crate::song::project::SatBand {
                style: crate::song::project::SatStyle::WarmTube,
                ..Default::default()
            }; 6],
        ),
        crossovers: crate::song::project::sat_crossovers(6),
        quality: 8,
        ..Saturator::default()
    };
    most.slope = 48;
    let heaviest = time(None, None, Some(&most));
    let spectral = Comp {
        spectral: 1.0,
        ..comp
    };
    let spectral = time(None, Some(&spectral), None);
    // The limiter, metered, plain and at its heaviest (multiband, clipping at 8x).
    let mut limit = |lim: &Limiter| {
        let began = std::time::Instant::now();
        for _ in 0..(seconds * 48_000) / MAX_BLOCK {
            strip.limit(Some(lim), &mut l, &mut r, true);
        }
        began.elapsed().as_secs_f64() / seconds as f64 * 100.0
    };
    let plain = limit(&Limiter {
        gain_db: 12.0,
        ..Limiter::default()
    });
    let limit_most = limit(&Limiter {
        gain_db: 12.0,
        style: crate::song::project::LimitStyle::Multiband,
        clip: 0.5,
        oversample: 8,
        ..Limiter::default()
    });
    eprintln!(
        "% of real time: EQ {:.3}, compressor {:.3}, saturator {:.3}, all {all:.3}; saturator \
         six bands at 8x {heaviest:.3}; spectral compressor {spectral:.3}; limiter {plain:.3}, \
         multiband clipping at 8x {limit_most:.3}",
        parts[0], parts[1], parts[2]
    );
    assert!(all < 1.0);
}
