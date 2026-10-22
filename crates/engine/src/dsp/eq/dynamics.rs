//! Dynamic bands: what they hear, how fast they move, their character.

use super::*;

/// Bands playing at once: the EQ's, and as many again fading out.
pub(crate) const LIVE: usize = MAX_BANDS * 2;

/// A dynamic band's gain moves this much for each dB over its threshold (4:1), up to its range.
pub(crate) const DYNAMIC_RATIO: f64 = 0.75;

/// What a dynamic band listens to: what's round its frequency (a shelf's side of it), or what
/// lies between its trigger's two frequencies. Up to two sections, and how many.
pub(crate) fn detector(band: &EqBand, rate: f64) -> ([Section; 2], usize) {
    let r2 = FRAC_1_SQRT_2;
    let at = |hz: f64| 2.0 * PI * hz.clamp(10.0, 0.49 * rate);
    if let Some([lo, hi]) = band.dynamic.and_then(|d| d.trigger) {
        let (a, b) = (at(lo as f64), at(hi as f64));
        return (
            [
                matched2(a, r2, a, rate, &highpass2(a, r2)),
                matched2(b, r2, b, rate, &lowpass2(b, r2)),
            ],
            2,
        );
    }
    let w = at(band.freq as f64);
    let q = (band.q as f64).clamp(0.3, 40.0);
    let one = match band.shape {
        Shape::LowShelf => matched2(w, r2, w, rate, &lowpass2(w, r2)),
        Shape::HighShelf | Shape::TiltShelf => matched2(w, r2, w, rate, &highpass2(w, r2)),
        Shape::FlatTilt => Section {
            b0: 1.0,
            ..Section::default()
        },
        _ => matched2(w, q, w, rate, &bandpass2(w, q)),
    };
    ([one, Section::default()], 1)
}

/// A dynamic band's own attack and release, by its frequency (ms): quick up top, slower down
/// low, where a cycle takes longer.
pub fn auto_times(freq: f32) -> (f64, f64) {
    let attack = (4_000.0 / freq.max(10.0) as f64).clamp(0.5, 40.0);
    (attack, (attack * 10.0).clamp(30.0, 500.0))
}

/// The analog warmth of the subtle and warm characters, on what a band adds or takes away: soft
/// above a whisper; the warm one lopsided, for even harmonics.
#[inline]
pub(crate) fn colour(d: f64, character: Character) -> f64 {
    match character {
        Character::Clean => d,
        Character::Subtle => (1.2 * d).tanh() / 1.2,
        Character::Warm => {
            const BIAS: f64 = 0.12;
            ((1.8 * d + BIAS).tanh() - BIAS.tanh()) / 1.8
        }
    }
}

/// A band as it plays.
#[derive(Clone, Copy)]
pub(crate) struct Live {
    /// The band's id; 0 a free place.
    pub(crate) id: u16,
    /// Taken out of the EQ: fading to nothing, then freed. A ghost is what a band was before its
    /// filters were made another way, fading out as they fade in.
    pub(crate) gone: bool,
    pub(crate) ghost: bool,
    pub(crate) band: EqBand,
    /// Where it is on its glide to `band`: log2 Hz, dB, log2 Q.
    pub(crate) at: [f64; 3],
    /// How much of it is heard, 0 to 1: it fades in and out.
    pub(crate) mix: f64,
    pub(crate) sections: [Section; MAX_SECTIONS],
    pub(crate) n: usize,
    /// Each lane's sections' memory.
    pub(crate) state: [[[f64; 2]; MAX_SECTIONS]; 2],
    /// Its sections need working out again.
    pub(crate) stale: bool,
    /// A dynamic band's ear and its memory; its level now (linear) and usually (dB, for its
    /// own threshold); how far it's moved now (dB).
    pub(crate) det: [Section; 2],
    pub(crate) det_n: usize,
    pub(crate) det_state: [[[f64; 2]; 2]; 2],
    pub(crate) env: f64,
    pub(crate) usual: f64,
    pub(crate) moved: f64,
}

impl Live {
    pub(crate) const FREE: Live = Live {
        id: 0,
        gone: false,
        ghost: false,
        band: EqBand {
            shape: Shape::Bell,
            freq: 1_000.0,
            gain_db: 0.0,
            q: 1.0,
            slope: 12.0,
            placement: Placement::Stereo,
            dynamic: None,
            off: false,
            gain_q: false,
            id: 0,
        },
        at: [0.0; 3],
        mix: 0.0,
        sections: [Section {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }; MAX_SECTIONS],
        n: 0,
        state: [[[0.0; 2]; MAX_SECTIONS]; 2],
        stale: true,
        det: [Section {
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }; 2],
        det_n: 0,
        det_state: [[[0.0; 2]; 2]; 2],
        env: 0.0,
        usual: -120.0,
        moved: 0.0,
    };

    pub(crate) fn target(band: &EqBand) -> [f64; 3] {
        [
            (band.freq as f64).max(1.0).log2(),
            band.gain_db as f64,
            (band.q as f64).max(0.001).log2(),
        ]
    }

    pub(crate) fn start(band: EqBand, mix: f64, rate: f64) -> Live {
        let (det, det_n) = detector(&band, rate);
        Live {
            id: band.id,
            band,
            at: Live::target(&band),
            mix,
            det,
            det_n,
            ..Live::FREE
        }
    }

    /// The band as it is on its glide.
    pub(crate) fn now(&self) -> EqBand {
        EqBand {
            freq: self.at[0].exp2() as f32,
            gain_db: self.at[1] as f32,
            q: self.at[2].exp2() as f32,
            ..self.band
        }
    }

    /// Takes new settings: a new shape or placement starts its filters afresh; a new slope
    /// keeps their memory, unless it takes another number of sections.
    pub(crate) fn set(&mut self, band: EqBand, rate: f64) {
        let o = &self.band;
        if (o.shape, o.placement) != (band.shape, band.placement) {
            self.state = [[[0.0; 2]; MAX_SECTIONS]; 2];
            self.det_state = [[[0.0; 2]; 2]; 2];
            self.at = Live::target(&band);
            self.stale = true;
        }
        if o.slope != band.slope {
            self.stale = true;
        }
        let trigger = |b: &EqBand| b.dynamic.and_then(|d| d.trigger);
        if (o.freq, o.q, o.shape, trigger(o)) != (band.freq, band.q, band.shape, trigger(&band)) {
            (self.det, self.det_n) = detector(&band, rate);
        }
        if o.gain_q != band.gain_q {
            self.stale = true;
        }
        self.band = band;
    }
}
