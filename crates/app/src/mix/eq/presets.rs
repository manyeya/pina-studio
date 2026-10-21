//! The EQ's presets, factory and saved, and its first low cut.

use super::*;

/// The EQ's first low cut, if it's on: its frequency (else 0).
pub fn low_cut(eq: &Eq) -> f32 {
    eq.nth(Shape::LowCut, 0)
        .map(|i| eq.bands[i])
        .filter(|b| !b.off)
        .map_or(0.0, |b| b.freq)
}

/// Sets the EQ's first low cut (adding one at the front if it has none): 20 Hz or less is off.
pub fn set_low_cut(eq: &mut Eq, hz: f32) {
    let on = hz > 20.0;
    match eq.nth(Shape::LowCut, 0) {
        Some(i) => {
            let b = &mut eq.bands[i];
            (b.freq, b.off) = (hz.max(20.0), !on);
        }
        None if on => {
            eq.bands.insert(0, EqBand::new(Shape::LowCut, hz, 0.0));
        }
        None => {}
    }
}

/// The band a click (or, `drag`ged, a pull of the curve) would make at a place on the plot.
pub(crate) fn band_at(geo: &Geo, x: f32, y: f32, drag: bool) -> EqBand {
    let freq = geo.freq(x).clamp(10.0, 30_000.0);
    let edge = x / geo.w;
    let gain = (geo.db(y) * 2.0).round() / 2.0;
    let shape = match () {
        _ if edge < 0.04 => {
            if drag {
                Shape::LowShelf
            } else {
                Shape::LowCut
            }
        }
        _ if edge > 0.96 => {
            if drag {
                Shape::HighShelf
            } else {
                Shape::HighCut
            }
        }
        _ if !drag && gain <= -geo.range * 0.85 => Shape::Notch,
        _ => Shape::Bell,
    };
    let gain = match shape.has_gain() && !drag {
        true => gain.clamp(-30.0, 30.0),
        false => 0.0,
    };
    EqBand::new(shape, freq.round() as f32, gain)
}

/// Pina Studio's own presets: starting points, by what they're for.
pub fn factory() -> Vec<(&'static str, Eq)> {
    let b = |shape: Shape, freq: f32, gain: f32, q: Option<f32>, slope: u8| {
        let mut band = EqBand::new(shape, freq, gain);
        band.q = q.unwrap_or(band.q);
        band.slope = slope as f32;
        band
    };
    let eq = |bands: &[EqBand]| Eq {
        bands: Bands::from_slice(bands),
        ..Eq::default()
    };
    let dynamic = |mut band: EqBand, range: f32, threshold: f32| {
        band.dynamic = Some(Dynamic {
            range_db: range,
            threshold_db: Some(threshold),
            ..Dynamic::default()
        });
        band
    };
    vec![
        (
            "Vocal: clear and present",
            eq(&[
                b(Shape::LowCut, 90.0, 0.0, None, 24),
                b(Shape::Bell, 300.0, -2.5, Some(1.2), 12),
                dynamic(b(Shape::Bell, 5_500.0, 0.0, Some(3.0), 12), -4.0, -30.0),
                b(Shape::Bell, 3_000.0, 2.0, Some(1.0), 12),
                b(Shape::HighShelf, 11_000.0, 2.5, None, 12),
            ]),
        ),
        (
            "Kick: tight and punchy",
            eq(&[
                b(Shape::LowCut, 28.0, 0.0, None, 24),
                b(Shape::Bell, 60.0, 3.0, Some(1.4), 12),
                b(Shape::Bell, 350.0, -4.0, Some(1.5), 12),
                b(Shape::Bell, 4_000.0, 2.5, Some(1.2), 12),
            ]),
        ),
        (
            "Bass: clean low end",
            eq(&[
                b(Shape::LowCut, 32.0, 0.0, None, 24),
                b(Shape::Bell, 90.0, 2.0, Some(1.0), 12),
                dynamic(b(Shape::Bell, 220.0, 0.0, Some(1.5), 12), -4.0, -28.0),
                b(Shape::HighCut, 7_000.0, 0.0, None, 12),
            ]),
        ),
        (
            "Drum bus: snap",
            eq(&[
                b(Shape::LowCut, 30.0, 0.0, None, 12),
                b(Shape::Bell, 450.0, -2.0, Some(1.0), 12),
                b(Shape::Bell, 5_000.0, 2.0, Some(0.8), 12),
                b(Shape::HighShelf, 12_000.0, 1.5, None, 12),
            ]),
        ),
        (
            "Pads: out of the way",
            eq(&[
                b(Shape::LowCut, 200.0, 0.0, None, 24),
                b(Shape::Bell, 2_500.0, -2.0, Some(0.8), 12),
                b(Shape::HighShelf, 9_000.0, -2.0, None, 12),
            ]),
        ),
        (
            "Mix: smile",
            eq(&[
                b(Shape::LowShelf, 90.0, 1.5, None, 12),
                b(Shape::Bell, 400.0, -1.0, Some(0.7), 12),
                b(Shape::HighShelf, 10_000.0, 1.5, None, 12),
            ]),
        ),
        (
            "Mastering: mono lows",
            eq(&[{
                let mut cut = b(Shape::LowCut, 120.0, 0.0, None, 12);
                cut.placement = Placement::Side;
                cut
            }]),
        ),
        (
            "Telephone",
            eq(&[
                b(Shape::LowCut, 400.0, 0.0, None, 24),
                b(Shape::Bell, 1_500.0, 4.0, Some(1.0), 12),
                b(Shape::HighCut, 3_400.0, 0.0, None, 24),
            ]),
        ),
    ]
}

/// Where saved presets go (a folder of their own while testing).
pub(crate) fn presets_dir() -> std::path::PathBuf {
    match cfg!(test) {
        true => std::env::temp_dir().join(format!("pina-test-eq-presets-{}", std::process::id())),
        false => pina_engine::paths::support().join("EQ Presets"),
    }
}

/// The presets saved, by name.
pub fn saved_presets() -> Vec<(String, std::path::PathBuf)> {
    let mut list: Vec<(String, std::path::PathBuf)> = std::fs::read_dir(presets_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| {
            let name = p
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (name, p)
        })
        .collect();
    list.sort();
    list
}
