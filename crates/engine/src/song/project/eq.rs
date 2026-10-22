//! The channel EQ: its bands (shape, slope, placement, dynamics) and how it processes.

use super::*;

/// What an EQ band does to the sound.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Bell,
    LowShelf,
    LowCut,
    HighShelf,
    HighCut,
    Notch,
    BandPass,
    /// A shelf that tilts round its frequency: half its gain down below, half up above.
    TiltShelf,
    /// A straight tilt across all that's heard (20 Hz to 20 kHz), level at its frequency: its
    /// gain the difference from end to end.
    FlatTilt,
    /// Shifts the phase round its frequency and leaves the level: to line up two takes.
    AllPass,
}

impl Shape {
    pub const ALL: [Shape; 10] = [
        Shape::Bell,
        Shape::LowShelf,
        Shape::LowCut,
        Shape::HighShelf,
        Shape::HighCut,
        Shape::Notch,
        Shape::BandPass,
        Shape::TiltShelf,
        Shape::FlatTilt,
        Shape::AllPass,
    ];

    /// The slope that's a brickwall: as steep as cuts go.
    pub const BRICKWALL: f32 = 144.0;

    pub fn name(self) -> &'static str {
        match self {
            Shape::Bell => "Bell",
            Shape::LowShelf => "Low Shelf",
            Shape::LowCut => "Low Cut",
            Shape::HighShelf => "High Shelf",
            Shape::HighCut => "High Cut",
            Shape::Notch => "Notch",
            Shape::BandPass => "Band Pass",
            Shape::TiltShelf => "Tilt Shelf",
            Shape::FlatTilt => "Flat Tilt",
            Shape::AllPass => "All Pass",
        }
    }

    /// Whether it boosts or cuts by a gain (the others only take away).
    pub fn has_gain(self) -> bool {
        matches!(
            self,
            Shape::Bell | Shape::LowShelf | Shape::HighShelf | Shape::TiltShelf | Shape::FlatTilt
        )
    }

    /// Whether it can move with the sound: a bell, shelf or tilt.
    pub fn can_be_dynamic(self) -> bool {
        self.has_gain()
    }

    /// The slopes it steps through, in dB an octave (its menu's, the wheel's); those between
    /// work too, as far as `slope_range` goes, but for a notch and an all pass, whose slopes go
    /// in twelves (their filters' order). None for a flat tilt.
    pub fn slopes(self) -> &'static [f32] {
        match self {
            Shape::LowCut | Shape::HighCut => &[
                6.0,
                12.0,
                18.0,
                24.0,
                30.0,
                36.0,
                48.0,
                72.0,
                96.0,
                Shape::BRICKWALL,
            ],
            Shape::LowShelf | Shape::HighShelf | Shape::TiltShelf | Shape::BandPass => {
                &[6.0, 12.0, 18.0, 24.0, 30.0, 36.0, 48.0, 72.0, 96.0]
            }
            Shape::Bell => &[12.0, 18.0, 24.0, 30.0, 36.0, 48.0, 72.0, 96.0],
            Shape::Notch | Shape::AllPass => &[12.0, 24.0, 36.0, 48.0, 72.0, 96.0],
            Shape::FlatTilt => &[],
        }
    }

    /// The slopes it takes, least and most, in dB an octave (a cut's brickwall besides): from 0
    /// for a cut or band pass (none at all), 12 for a bell, notch or all pass (a second-order
    /// filter's), 6 for the rest.
    pub fn slope_range(self) -> Option<(f32, f32)> {
        match self {
            Shape::LowCut | Shape::HighCut | Shape::BandPass => Some((0.0, 96.0)),
            Shape::LowShelf | Shape::HighShelf | Shape::TiltShelf => Some((6.0, 96.0)),
            Shape::Bell | Shape::Notch | Shape::AllPass => Some((12.0, 96.0)),
            Shape::FlatTilt => None,
        }
    }

    /// The slope it takes nearest `s`.
    pub fn fit_slope(self, s: f32) -> f32 {
        let Some((lo, hi)) = self.slope_range() else {
            return 12.0;
        };
        if matches!(self, Shape::LowCut | Shape::HighCut) && s >= Shape::BRICKWALL {
            return Shape::BRICKWALL;
        }
        let s = s.clamp(lo, hi);
        match self {
            Shape::Notch | Shape::AllPass => (s / 12.0).round() * 12.0,
            _ => (s * 100.0).round() / 100.0,
        }
    }

    /// The width it starts with: a cut or shelf with no bump, a bell an octave and a half.
    pub fn default_q(self) -> f32 {
        match self {
            Shape::Bell | Shape::BandPass => 1.0,
            Shape::Notch => 4.0,
            _ => std::f32::consts::FRAC_1_SQRT_2,
        }
    }
}

/// Which part of a stereo sound a band works on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    #[default]
    Stereo,
    Left,
    Right,
    /// What's the same on both sides: the middle.
    Mid,
    /// What differs: the edges.
    Side,
}

impl Placement {
    pub const ALL: [Placement; 5] = [
        Placement::Stereo,
        Placement::Left,
        Placement::Right,
        Placement::Mid,
        Placement::Side,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Placement::Stereo => "Stereo",
            Placement::Left => "Left",
            Placement::Right => "Right",
            Placement::Mid => "Mid",
            Placement::Side => "Side",
        }
    }

    pub(crate) fn is_stereo(&self) -> bool {
        *self == Placement::Stereo
    }
}

/// A band that moves with the sound: as what it covers gets louder than its threshold, its gain
/// goes by up to `range_db` more (below 0 down, as a compressor would; above, up). Without a
/// threshold it sets its own, following what the band usually hears, so it moves on what
/// stands out; without attack and release, its own, by its frequency. It listens to what the
/// band covers, or between `trigger`'s two frequencies; with `external`, to the EQ's sidechain.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Dynamic {
    pub range_db: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_db: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_ms: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_ms: Option<f32>,
    /// What it listens to, low and high Hz, if not what the band covers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
    /// Kept but still.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
    /// Spectral: rather than the whole band moving, only what stands out in it, frequency by
    /// frequency (in linear phase, so with the EQ's latency). `density` 0 moves wide stretches
    /// together, 1 each frequency on its own; `tilt` has it hear the highs 3 dB an octave
    /// louder, as a mix's pink spectrum needs.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub spectral: bool,
    #[serde(skip_serializing_if = "is_half")]
    pub density: f32,
    #[serde(skip_serializing_if = "is_true")]
    pub tilt: bool,
}

pub(crate) fn is_half(v: &f32) -> bool {
    *v == 0.5
}

pub(crate) fn is_true(v: &bool) -> bool {
    *v
}

impl Default for Dynamic {
    fn default() -> Self {
        Dynamic {
            range_db: -6.0,
            threshold_db: None,
            attack_ms: None,
            release_ms: None,
            trigger: None,
            external: false,
            bypass: false,
            spectral: false,
            density: 0.5,
            tilt: true,
        }
    }
}

/// How an EQ's filters treat phase: as analog EQs do, with no latency; as an analog EQ's phase
/// too, all through what's heard (a little latency); or not at all, linear phase (more latency,
/// as `resolution` sets it, and its bands' timing kept together).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Processing {
    #[default]
    ZeroLatency,
    Natural,
    Linear,
}

impl Processing {
    pub const ALL: [Processing; 3] = [
        Processing::ZeroLatency,
        Processing::Natural,
        Processing::Linear,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Processing::ZeroLatency => "Zero Latency",
            Processing::Natural => "Natural Phase",
            Processing::Linear => "Linear Phase",
        }
    }

    pub(crate) fn is_zero(&self) -> bool {
        *self == Processing::ZeroLatency
    }
}

/// One band of an EQ: its shape at a frequency (Hz), its gain (dB), width (Q) and slope (dB an
/// octave, fractions too); the part of the sound it works on; and whether it moves with the
/// sound.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(from = "BandJson")]
pub struct EqBand {
    pub shape: Shape,
    pub freq: f32,
    #[serde(skip_serializing_if = "zero_f32")]
    pub gain_db: f32,
    pub q: f32,
    #[serde(skip_serializing_if = "is_twelve")]
    pub slope: f32,
    #[serde(skip_serializing_if = "Placement::is_stereo")]
    pub placement: Placement,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic: Option<Dynamic>,
    /// Kept but not heard.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub off: bool,
    /// Its width narrows as its gain grows, as an analog EQ's does.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub gain_q: bool,
    /// Which band it is while the song's open, so the engine keeps its sound through edits (a
    /// band taken out before it fades rather than clicking). Not saved.
    #[serde(skip)]
    pub id: u16,
}

pub(crate) fn zero_f32(v: &f32) -> bool {
    *v == 0.0
}

/// A band as JSON has it: anything left out as a 1 kHz bell has it, but its width, which is
/// its shape's (a cut's or shelf's with no bump).
#[derive(Deserialize)]
#[serde(default)]
pub(crate) struct BandJson {
    pub(crate) shape: Shape,
    pub(crate) freq: f32,
    pub(crate) gain_db: f32,
    pub(crate) q: Option<f32>,
    pub(crate) slope: f32,
    pub(crate) placement: Placement,
    pub(crate) dynamic: Option<Dynamic>,
    pub(crate) off: bool,
    pub(crate) gain_q: bool,
}

impl Default for BandJson {
    fn default() -> Self {
        let b = EqBand::default();
        BandJson {
            shape: b.shape,
            freq: b.freq,
            gain_db: b.gain_db,
            q: None,
            slope: b.slope,
            placement: b.placement,
            dynamic: None,
            off: false,
            gain_q: false,
        }
    }
}

impl From<BandJson> for EqBand {
    fn from(j: BandJson) -> Self {
        EqBand {
            shape: j.shape,
            freq: j.freq,
            gain_db: j.gain_db,
            q: j.q.unwrap_or(j.shape.default_q()),
            slope: j.slope,
            placement: j.placement,
            dynamic: j.dynamic,
            off: j.off,
            gain_q: j.gain_q,
            id: 0,
        }
    }
}

pub(crate) fn is_twelve(v: &f32) -> bool {
    *v == 12.0
}

impl Default for EqBand {
    fn default() -> Self {
        EqBand::new(Shape::Bell, 1_000.0, 0.0)
    }
}

impl EqBand {
    pub fn new(shape: Shape, freq: f32, gain_db: f32) -> Self {
        EqBand {
            shape,
            freq,
            gain_db,
            q: shape.default_q(),
            slope: 12.0,
            placement: Placement::Stereo,
            dynamic: None,
            off: false,
            gain_q: false,
            id: 0,
        }
    }

    /// Its gain as heard, with the EQ's gain scale: none for a shape that has none.
    pub fn gain(&self, scale: f32) -> f32 {
        match self.shape.has_gain() {
            true => self.gain_db * scale,
            false => 0.0,
        }
    }
}

/// An EQ's bands, at most `MAX_BANDS`, kept in place so an EQ copies without allocating (the
/// audio thread copies it to modulate it). In JSON, a list.
#[derive(Clone, Copy, PartialEq)]
pub struct Bands {
    pub(crate) n: u8,
    pub(crate) list: [EqBand; MAX_BANDS],
}

impl Default for Bands {
    fn default() -> Self {
        Bands {
            n: 0,
            list: [EqBand::default(); MAX_BANDS],
        }
    }
}

impl std::ops::Deref for Bands {
    type Target = [EqBand];

    fn deref(&self) -> &[EqBand] {
        &self.list[..self.n as usize]
    }
}

impl std::ops::DerefMut for Bands {
    fn deref_mut(&mut self) -> &mut [EqBand] {
        &mut self.list[..self.n as usize]
    }
}

impl std::fmt::Debug for Bands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl Bands {
    pub fn from_slice(bands: &[EqBand]) -> Self {
        let mut out = Bands::default();
        for b in bands {
            out.push(*b);
        }
        out
    }

    /// Adds a band at the end, giving it an id of its own if it hasn't one: where it went, or
    /// None if the EQ is full.
    pub fn push(&mut self, band: EqBand) -> Option<usize> {
        self.insert(self.n as usize, band)
    }

    pub fn insert(&mut self, at: usize, mut band: EqBand) -> Option<usize> {
        let n = self.n as usize;
        if n == MAX_BANDS {
            return None;
        }
        if band.id == 0 || self.iter().any(|b| b.id == band.id) {
            band.id = self.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        }
        let at = at.min(n);
        self.list.copy_within(at..n, at + 1);
        self.list[at] = band;
        self.n += 1;
        Some(at)
    }

    pub fn remove(&mut self, at: usize) -> Option<EqBand> {
        let n = self.n as usize;
        if at >= n {
            return None;
        }
        let band = self.list[at];
        self.list.copy_within(at + 1..n, at);
        self.n -= 1;
        Some(band)
    }

    /// Where the band with this id is.
    pub fn find(&self, id: u16) -> Option<usize> {
        self.iter().position(|b| b.id == id)
    }
}

impl Serialize for Bands {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for Bands {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let list = Vec::<EqBand>::deserialize(d)?;
        if list.len() > MAX_BANDS {
            return Err(serde::de::Error::custom(format!(
                "an EQ has at most {MAX_BANDS} bands"
            )));
        }
        // Ids afresh: they're the song's while it's open, not the file's.
        let mut out = Bands::default();
        for b in list {
            out.push(EqBand { id: 0, ..b });
        }
        Ok(out)
    }
}

/// A channel's built-in EQ: up to 24 bands, each a bell, shelf, cut, notch, band pass or tilt
/// over the whole sound or a part of it (left, right, mid, side), still or moving with the sound;
/// then a gain scale for them all and the output level. Anything left out of its JSON is as
/// `Eq::default()` has it: no bands, flat.
///
/// It reads the EQ Pina Studio had before too (a low cut, two bells and a high shelf, by name), and
/// takes those names in `set_eq`: `low_cut` is its first low cut, `low_mid` and `high_mid` its
/// first and second bells, `high_shelf` its first high shelf, each added if missing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(from = "EqJson")]
pub struct Eq {
    pub bands: Bands,
    /// How much of each band's gain is heard: 1 as set, 0 none, 2 twice.
    #[serde(skip_serializing_if = "one_f32")]
    pub scale: f32,
    #[serde(skip_serializing_if = "zero_f32")]
    pub output_db: f32,
    /// Its output's balance, -1 to 1: left and right, or with `pan_ms` mid (-1) and side (1).
    #[serde(skip_serializing_if = "zero_f32")]
    pub pan: f32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pan_ms: bool,
    /// Its output turned upside down.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
    /// Makes up for how much louder or quieter its bands make the sound.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub auto_gain: bool,
    /// Clean, or with an analog EQ's saturation: subtle, or warm.
    #[serde(skip_serializing_if = "Character::is_clean")]
    pub character: Character,
    /// A track (by id) its dynamic bands can listen to instead of what goes through it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidechain: Option<u64>,
    /// How its filters treat phase, and (for linear phase and spectral bands) at what
    /// resolution: 0 low to 3 very high, the latency doubling each step.
    #[serde(skip_serializing_if = "Processing::is_zero")]
    pub processing: Processing,
    #[serde(skip_serializing_if = "is_one_u8")]
    pub resolution: u8,
    /// Kept but not heard.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

pub(crate) fn is_one_u8(v: &u8) -> bool {
    *v == 1
}

/// An EQ's character: clean (transparent), or the saturation of an analog one, subtle or warm.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Character {
    #[default]
    Clean,
    Subtle,
    Warm,
}

impl Character {
    pub const ALL: [Character; 3] = [Character::Clean, Character::Subtle, Character::Warm];

    pub(crate) fn is_clean(&self) -> bool {
        *self == Character::Clean
    }

    pub fn name(self) -> &'static str {
        match self {
            Character::Clean => "Clean",
            Character::Subtle => "Subtle",
            Character::Warm => "Warm",
        }
    }
}

pub(crate) fn one_f32(v: &f32) -> bool {
    *v == 1.0
}

impl Default for Eq {
    fn default() -> Self {
        Eq {
            bands: Bands::default(),
            scale: 1.0,
            output_db: 0.0,
            pan: 0.0,
            pan_ms: false,
            invert: false,
            auto_gain: false,
            character: Character::Clean,
            sidechain: None,
            processing: Processing::ZeroLatency,
            resolution: 1,
            bypass: false,
        }
    }
}

impl Eq {
    /// The EQ Pina Studio had before, flat: a low cut (off), bells at 400 Hz and 2.5 kHz and a high
    /// shelf at 10 kHz. Songs made with it open with these four, in this order, so what moved
    /// them by number (modulators) still does.
    pub fn classic() -> Self {
        let mut cut = EqBand::new(Shape::LowCut, 20.0, 0.0);
        cut.off = true;
        Eq {
            bands: Bands::from_slice(&[
                cut,
                EqBand::new(Shape::Bell, 400.0, 0.0),
                EqBand::new(Shape::Bell, 2_500.0, 0.0),
                EqBand::new(Shape::HighShelf, 10_000.0, 0.0),
            ]),
            ..Eq::default()
        }
    }

    /// Where the `nth` band of a shape is.
    pub fn nth(&self, shape: Shape, nth: usize) -> Option<usize> {
        self.bands
            .iter()
            .enumerate()
            .filter(|(_, b)| b.shape == shape)
            .nth(nth)
            .map(|(i, _)| i)
    }

    /// The `nth` band of a shape, added (as `new`) if there are fewer.
    pub(crate) fn nth_or_add(&mut self, shape: Shape, nth: usize, new: EqBand) -> Option<usize> {
        match self.nth(shape, nth) {
            Some(i) => Some(i),
            None => self.bands.push(new),
        }
    }

    /// The old EQ's names onto this one's bands.
    pub(crate) fn apply_classic(&mut self, j: &EqJson) {
        if let Some(hz) = j.low_cut {
            let on = hz > 20.0;
            match self.nth(Shape::LowCut, 0) {
                Some(i) => {
                    let b = &mut self.bands[i];
                    (b.freq, b.off) = (hz.max(20.0), !on);
                }
                None if on => {
                    self.bands.insert(0, EqBand::new(Shape::LowCut, hz, 0.0));
                }
                None => {}
            }
        }
        let named = [
            (&j.low_mid, Shape::Bell, 0, 400.0),
            (&j.high_mid, Shape::Bell, 1, 2_500.0),
            (&j.high_shelf, Shape::HighShelf, 0, 10_000.0),
        ];
        for (band, shape, nth, freq) in named {
            let Some(c) = band else { continue };
            let Some(i) = self.nth_or_add(shape, nth, EqBand::new(shape, freq, 0.0)) else {
                continue;
            };
            let b = &mut self.bands[i];
            b.freq = c.freq.unwrap_or(b.freq);
            b.gain_db = c.gain_db.unwrap_or(b.gain_db);
            b.q = c.q.unwrap_or(b.q);
        }
    }
}

/// An EQ as JSON has it: its bands, or the old EQ's names, or both (`set_eq` with the old names
/// on an EQ that has bands).
#[derive(Deserialize)]
pub(crate) struct EqJson {
    pub(crate) bands: Option<Bands>,
    #[serde(default = "unity")]
    pub(crate) scale: f32,
    #[serde(default)]
    pub(crate) output_db: f32,
    #[serde(default)]
    pub(crate) pan: f32,
    #[serde(default)]
    pub(crate) pan_ms: bool,
    #[serde(default)]
    pub(crate) invert: bool,
    #[serde(default)]
    pub(crate) auto_gain: bool,
    #[serde(default)]
    pub(crate) character: Character,
    #[serde(default)]
    pub(crate) sidechain: Option<u64>,
    #[serde(default)]
    pub(crate) processing: Processing,
    #[serde(default = "one_u8")]
    pub(crate) resolution: u8,
    #[serde(default)]
    pub(crate) bypass: bool,
    pub(crate) low_cut: Option<f32>,
    pub(crate) low_mid: Option<ClassicBand>,
    pub(crate) high_mid: Option<ClassicBand>,
    pub(crate) high_shelf: Option<ClassicBand>,
}

pub(crate) fn unity() -> f32 {
    1.0
}

pub(crate) fn one_u8() -> u8 {
    1
}

/// One of the old EQ's bands, as much of it as was given.
#[derive(Deserialize)]
pub(crate) struct ClassicBand {
    pub(crate) freq: Option<f32>,
    pub(crate) gain_db: Option<f32>,
    pub(crate) q: Option<f32>,
}

impl From<EqJson> for Eq {
    fn from(j: EqJson) -> Self {
        let classic = j.low_cut.is_some()
            || j.low_mid.is_some()
            || j.high_mid.is_some()
            || j.high_shelf.is_some();
        let mut eq = match (j.bands, classic) {
            (Some(bands), _) => Eq {
                bands,
                ..Eq::default()
            },
            (None, true) => Eq::classic(),
            (None, false) => Eq::default(),
        };
        eq.apply_classic(&j);
        eq.scale = j.scale.clamp(0.0, 2.0);
        eq.output_db = j.output_db;
        eq.pan = j.pan.clamp(-1.0, 1.0);
        eq.pan_ms = j.pan_ms;
        eq.invert = j.invert;
        eq.auto_gain = j.auto_gain;
        eq.character = j.character;
        eq.sidechain = j.sidechain;
        eq.processing = j.processing;
        eq.resolution = j.resolution.min(3);
        eq.bypass = j.bypass;
        eq
    }
}
