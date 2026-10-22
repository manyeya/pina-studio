//! A part's channel from what was measured: its low cut, compressor, saturator, level and sends.

/// A part's ear for mud (250 Hz-1 kHz) and bite (4-8 kHz): how much of its sound may be there
/// before it's cut, dB of the whole.
pub(crate) fn limits(part: &str) -> (f32, f32) {
    match part {
        "chords" => (-4.5, -16.0),
        "pad" => (-5.0, -18.0),
        "lead" => (-7.0, -14.0),
        "arp" | "counter" => (-7.0, -14.0),
        "vocals" => (-7.0, -13.0),
        "perc" => (-9.0, -9.0),
        "drums" => (-8.0, -11.0),
        "bass" => (-10.0, -24.0),
        "texture" => (-6.0, -16.0),
        _ => (-6.0, -14.0),
    }
}

/// A part's low cut, before its lowest note has its say.
pub(crate) fn low_cut(part: &str) -> f32 {
    match part {
        "bass" | "drums" => 25.0,
        "vocals" => 90.0,
        "fx" => 60.0,
        "perc" | "chords" => 150.0,
        "lead" => 180.0,
        _ => 200.0,
    }
}

/// A part's compressor: (dB over its own loudness, ratio, attack, release).
pub(crate) fn comp(part: &str) -> Option<(f32, f32, f32, f32)> {
    match part {
        "drums" => Some((6.0, 4.0, 8.0, 90.0)),
        "perc" => Some((6.0, 2.5, 10.0, 80.0)),
        "bass" => Some((4.0, 4.0, 10.0, 120.0)),
        "chords" => Some((6.0, 2.5, 20.0, 150.0)),
        "pad" => Some((8.0, 2.0, 30.0, 200.0)),
        "lead" => Some((5.0, 3.0, 5.0, 100.0)),
        "arp" | "counter" => Some((6.0, 2.5, 10.0, 120.0)),
        "vocals" => Some((4.0, 4.0, 3.0, 80.0)),
        "fx" | "texture" => None,
        _ => Some((6.0, 2.5, 15.0, 120.0)),
    }
}

/// A part's saturation: (drive, tone, mix).
pub(crate) fn sat(part: &str) -> Option<(f32, f32, f32)> {
    match part {
        // Soft-clips the hits a compressor lets through.
        "drums" => Some((6.0, 0.0, 0.7)),
        // Harmonics, so it's heard on small speakers and not only felt.
        "bass" => Some((9.0, 0.15, 0.55)),
        "chords" => Some((3.0, -0.1, 0.35)),
        "lead" => Some((2.0, 0.1, 0.25)),
        "vocals" => Some((2.0, 0.1, 0.2)),
        _ => None,
    }
}

/// Where a part sits against the drums alone, LUFS (a bass by its RMS, as LUFS hardly hears
/// sub); the drums at -20 leave the master room.
pub(crate) fn level(part: &str) -> f32 {
    match part {
        "drums" => -20.0,
        "perc" => -28.0,
        "bass" => -21.0,
        "chords" => -25.0,
        "pad" => -29.0,
        "lead" => -24.5,
        "counter" | "arp" => -28.0,
        "vocals" => -21.5,
        "texture" => -36.0,
        _ => -28.0,
    }
}

/// Its sends: (reverb, delay), dB, if any.
pub(crate) fn sends(part: &str, genre: &str) -> (Option<f32>, Option<f32>) {
    let wet = match genre {
        "lofi" | "cinematic" | "ambient" | "neo_soul" => 3.0,
        "trap" | "drill" | "techno" => -2.0,
        _ => 0.0,
    };
    let verb = match part {
        "lead" => Some(-14.0),
        "vocals" => Some(-13.0),
        "chords" => Some(-17.0),
        "pad" => Some(-11.0),
        "arp" => Some(-13.0),
        "counter" => Some(-14.0),
        "perc" => Some(-18.0),
        "drums" if !matches!(genre, "trap" | "drill") => Some(-22.0),
        "fx" => Some(-12.0),
        _ => None,
    };
    let echo = match part {
        "lead" => Some(-17.0),
        "vocals" => Some(-16.0),
        "arp" | "counter" => Some(-20.0),
        "perc" => Some(-24.0),
        _ => None,
    };
    (verb.map(|v| v + wet), echo.map(|v| v + wet * 0.5))
}

/// Where a dynamic band hearing another track should start, for it to move its whole `range` as
/// that track hits at `peak` (the band moves by 4:1 over its threshold, through a 6 dB knee) and
/// let go again as it falls to `floor` before its next hit: a kick that rings on, too little
/// apart, gets a shallower duck that lets go, rather than one that never does.
pub(crate) fn under(peak: f32, floor: Option<f32>, range: f32) -> f32 {
    let full = peak - range.abs() / 0.75 - 5.0;
    let free = floor.map_or(-80.0, |f| f + 3.0);
    full.max(free).clamp(-80.0, 0.0)
}

/// Genres whose music breathes with the kick: the bass and pads ducking on every hit.
pub(crate) fn pumps(genre: &str) -> bool {
    matches!(
        genre,
        "house" | "deep_house" | "techno" | "edm" | "amapiano" | "pop"
    )
}
