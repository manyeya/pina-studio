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
    eprintln!(
        "% of real time: EQ {:.3}, compressor {:.3}, saturator {:.3}, all {all:.3}",
        parts[0], parts[1], parts[2]
    );
    assert!(all < 1.0);
}
