//! The channel strip's compressor and saturator.

use super::*;

/// How a compressor behaves: its detector, knee, release and colour.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompStyle {
    /// Transparent: a 6 dB knee on the peak (or RMS) level, as Pina Studio's compressor was.
    #[default]
    Clean,
    /// Smooth: an RMS ear, a wide knee and a release that follows the music, for vocals and
    /// anything that should just sit.
    Smooth,
    /// Punchy: a fast peak ear, a tight knee and a little saturation, for drums and bass.
    Punchy,
    /// A console bus compressor: ratios 2, 4 and 10, attacks 0.1 to 30 ms,
    /// releases 0.1 to 1.2 s or Auto, both sides one, for the glue on a bus or the mix.
    Bus,
}

impl CompStyle {
    pub const ALL: [CompStyle; 4] = [
        CompStyle::Clean,
        CompStyle::Smooth,
        CompStyle::Punchy,
        CompStyle::Bus,
    ];

    pub fn name(self) -> &'static str {
        match self {
            CompStyle::Clean => "Clean",
            CompStyle::Smooth => "Smooth",
            CompStyle::Punchy => "Punchy",
            CompStyle::Bus => "Bus",
        }
    }

    /// The steps a bus compressor's ratio, attack (ms) and release (ms) take.
    pub const BUS_RATIOS: [f32; 3] = [2.0, 4.0, 10.0];
    pub const BUS_ATTACKS: [f32; 6] = [0.1, 0.3, 1.0, 3.0, 10.0, 30.0];
    pub const BUS_RELEASES: [f32; 4] = [100.0, 300.0, 600.0, 1200.0];
}

/// The step of `steps` nearest `v` (by ratio, as they're spread).
pub fn nearest_step(v: f32, steps: &[f32]) -> f32 {
    steps
        .iter()
        .copied()
        .min_by(|a, b| (v / a).ln().abs().total_cmp(&(v / b).ln().abs()))
        .unwrap_or(v)
}

/// A channel's built-in compressor, after its EQ.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Comp {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    #[serde(default)]
    pub makeup_db: f32,
    /// Its level is an RMS average over 15 ms rather than the peak: gentler, more like the ear.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rms: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
    /// Its behaviour; Clean for one saved before it had styles.
    #[serde(default, skip_serializing_if = "is_clean")]
    pub style: CompStyle,
    /// How much of the compressed sound is heard, 0 to 1; the rest is the dry sound (parallel
    /// compression).
    #[serde(default = "full", skip_serializing_if = "is_full")]
    pub mix: f32,
    /// Its release follows the music: quick after a hit, slow after a long squeeze (a bus
    /// compressor's Auto).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_release: bool,
    /// What it hears goes through a high pass at this frequency (0: none), so the lows don't
    /// pump it.
    #[serde(default, skip_serializing_if = "is_off")]
    pub sidechain_hz: f32,
    /// How much its turning down goes to the part of the spectrum that made it, 0 to 1: at 1 a
    /// kick turns down the lows, not the hats over it (less pumping); at 0 all of it alike.
    #[serde(default, skip_serializing_if = "is_off")]
    pub spectral: f32,
}

fn is_clean(s: &CompStyle) -> bool {
    *s == CompStyle::Clean
}

fn full() -> f32 {
    1.0
}

fn is_full(v: &f32) -> bool {
    *v == 1.0
}

fn is_off(v: &f32) -> bool {
    *v == 0.0
}

impl Default for Comp {
    fn default() -> Self {
        Comp {
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            makeup_db: 0.0,
            rms: false,
            bypass: false,
            style: CompStyle::Clean,
            mix: 1.0,
            auto_release: false,
            sidechain_hz: 0.0,
            spectral: 0.0,
        }
    }
}

/// The most bands a saturator splits the sound into.
pub const MAX_SAT_BANDS: usize = 6;

/// How a saturator band bends the sound (see `dsp::sat` for each curve).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SatStyle {
    /// Tape driven warm: a biased soft clip, its even harmonics and a rounded top. Pina Studio's
    /// first, and its sound for a saturator saved before there were styles.
    #[default]
    WarmTape,
    /// Tape, clean: a symmetric soft clip, odd harmonics, gentle.
    CleanTape,
    /// A tube driven warm: each half clips its own way, strong even harmonics.
    WarmTube,
    /// A tube, clean: the gentlest curve, mostly the third harmonic.
    CleanTube,
    /// A transformer: the lows saturate first, thickening them.
    Transformer,
    /// A guitar amp, crunchy: mids forward, a harder clip, a speaker's top.
    Crunch,
    /// A guitar amp, heavy: two gain stages, tight lows, dark top.
    Heavy,
    /// Rectified: the lower half folded up, an octave above.
    Rectify,
    /// A wavefolder: past full scale it folds back, bright and metallic.
    Foldback,
    /// Lo-fi: fewer bits, the steps heard as grit.
    Lofi,
}

impl SatStyle {
    pub const ALL: [SatStyle; 10] = [
        SatStyle::WarmTape,
        SatStyle::CleanTape,
        SatStyle::WarmTube,
        SatStyle::CleanTube,
        SatStyle::Transformer,
        SatStyle::Crunch,
        SatStyle::Heavy,
        SatStyle::Rectify,
        SatStyle::Foldback,
        SatStyle::Lofi,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SatStyle::WarmTape => "Warm Tape",
            SatStyle::CleanTape => "Clean Tape",
            SatStyle::WarmTube => "Warm Tube",
            SatStyle::CleanTube => "Clean Tube",
            SatStyle::Transformer => "Transformer",
            SatStyle::Crunch => "Crunch Amp",
            SatStyle::Heavy => "Heavy Amp",
            SatStyle::Rectify => "Rectify",
            SatStyle::Foldback => "Foldback",
            SatStyle::Lofi => "Lo-Fi",
        }
    }
}

/// One band of a saturator: the frequencies between two crossovers, saturated its own way.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct SatBand {
    pub style: SatStyle,
    /// How hard it's driven, in dB: 0 to 24.
    pub drive_db: f32,
    /// What it adds, darker (-1) to brighter (1).
    pub tone: f32,
    /// How its level moves with how loud it is, -1 to 1: over 0 the loud parts come down (it
    /// compresses), under 0 they come up (it expands, for punch).
    #[serde(skip_serializing_if = "is_zero_f32")]
    pub dynamics: f32,
    /// How much of what it makes is fed back in, 0 to 1, ringing at `feedback_hz`.
    #[serde(skip_serializing_if = "is_zero_f32")]
    pub feedback: f32,
    pub feedback_hz: f32,
    /// How much of the saturated band is heard, 0 to 1; the rest is the band dry.
    pub mix: f32,
    pub level_db: f32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mute: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub solo: bool,
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

impl Default for SatBand {
    fn default() -> Self {
        SatBand {
            style: SatStyle::WarmTape,
            drive_db: 6.0,
            tone: 0.0,
            dynamics: 0.0,
            feedback: 0.0,
            feedback_hz: 1000.0,
            mix: 1.0,
            level_db: 0.0,
            mute: false,
            solo: false,
        }
    }
}

/// A saturator's bands, kept in place so a saturator copies without allocating (the audio
/// thread copies it to modulate it). In JSON, a list.
#[derive(Clone, Copy, PartialEq)]
pub struct SatBands {
    n: u8,
    list: [SatBand; MAX_SAT_BANDS],
}

impl SatBands {
    /// Up to `MAX_SAT_BANDS` of `bands`; at least one.
    pub fn from_slice(bands: &[SatBand]) -> Self {
        let mut list = [SatBand::default(); MAX_SAT_BANDS];
        let n = bands.len().clamp(1, MAX_SAT_BANDS);
        list[..bands.len().min(n)].copy_from_slice(&bands[..bands.len().min(n)]);
        SatBands { n: n as u8, list }
    }
}

impl std::ops::Deref for SatBands {
    type Target = [SatBand];

    fn deref(&self) -> &[SatBand] {
        &self.list[..self.n as usize]
    }
}

impl std::ops::DerefMut for SatBands {
    fn deref_mut(&mut self) -> &mut [SatBand] {
        &mut self.list[..self.n as usize]
    }
}

impl std::fmt::Debug for SatBands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// A channel's built-in saturator, after its compressor: the sound split into up to six bands,
/// each driven into a curve of its own (tape, tube, transformer, amp, or the stranger ones),
/// with its own tone, dynamics, feedback, mix and level; run several times over the sample rate
/// so what it adds doesn't fold back down as aliasing. Quiet sounds keep their level; loud ones
/// take on the style's character.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "SaturatorJson", into = "SaturatorJson")]
pub struct Saturator {
    pub bands: SatBands,
    /// Where the bands split, low to high, Hz: one fewer than the bands (the rest unused).
    pub crossovers: [f32; MAX_SAT_BANDS - 1],
    /// How steep the splits are, dB an octave: 12, 24 or 48.
    pub slope: u8,
    /// How many times the sample rate it runs at: 2, 4 or 8. More costs more and aliases less,
    /// which the harder styles want.
    pub quality: u8,
    /// How much of the saturated sound is heard, 0 to 1; the rest is the dry sound.
    pub mix: f32,
    pub output_db: f32,
    pub bypass: bool,
}

/// Where `n` bands split by default, spread over the spectrum.
pub fn sat_crossovers(n: usize) -> [f32; MAX_SAT_BANDS - 1] {
    let mut out = [20_000.0; MAX_SAT_BANDS - 1];
    let (lo, hi) = (120f32, 8_000f32);
    for (k, c) in out.iter_mut().take(n.saturating_sub(1)).enumerate() {
        let t = match n {
            2 => 0.5,
            _ => k as f32 / (n - 2) as f32,
        };
        *c = lo * (hi / lo).powf(t);
    }
    out
}

impl Saturator {
    /// The band `hz` falls in.
    pub fn band_at(&self, hz: f32) -> usize {
        let n = self.bands.len();
        self.crossovers[..n - 1]
            .iter()
            .filter(|&&c| hz >= c)
            .count()
    }

    /// Splits the band `hz` falls in there, the new band above it a copy of it; which that is.
    /// None when it has as many bands as it can, or `hz` is too close to a split already.
    pub fn split(&mut self, hz: f32) -> Option<usize> {
        let n = self.bands.len();
        let near = self.crossovers[..n - 1]
            .iter()
            .any(|&c| (hz / c).log2().abs() < 1.0 / 3.0);
        if n >= MAX_SAT_BANDS || near || !(20.0..=20_000.0).contains(&hz) {
            return None;
        }
        let i = self.band_at(hz);
        let mut bands = self.bands.to_vec();
        bands.insert(
            i + 1,
            SatBand {
                solo: false,
                ..bands[i]
            },
        );
        self.bands = SatBands::from_slice(&bands);
        let mut splits = self.crossovers[..n - 1].to_vec();
        splits.insert(i, hz);
        self.crossovers[..n].copy_from_slice(&splits);
        Some(i + 1)
    }

    /// Takes out band `i`, its neighbour taking its range (the last band can't go).
    pub fn remove(&mut self, i: usize) {
        let n = self.bands.len();
        if n < 2 || i >= n {
            return;
        }
        let mut bands = self.bands.to_vec();
        bands.remove(i);
        self.bands = SatBands::from_slice(&bands);
        let mut splits = self.crossovers[..n - 1].to_vec();
        splits.remove(i.saturating_sub(1));
        splits.push(20_000.0);
        self.crossovers[..n - 1].copy_from_slice(&splits);
    }
}

impl Default for Saturator {
    fn default() -> Self {
        Saturator {
            bands: SatBands::from_slice(&[SatBand::default()]),
            crossovers: sat_crossovers(1),
            slope: 24,
            quality: 2,
            mix: 1.0,
            output_db: 0.0,
            bypass: false,
        }
    }
}

/// How a limiter goes about it: the character of its gain moves, from the most transparent to
/// the loudest, and a multiband one that limits the lows, mids and highs apart before its
/// brickwall.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LimitStyle {
    /// Clean above all: slow, smooth moves and a soft knee.
    #[default]
    Transparent,
    /// Transients kept: it grabs late and lets go quickly, the very top of a hit clipped.
    Punchy,
    /// Short peaks let go quickly, a long squeeze slowly: the song's dynamics kept.
    Dynamic,
    /// Balanced, for most things.
    Allround,
    /// As loud as it gets: fast moves and clipping.
    Aggressive,
    /// Dense and loud yet clean: a smooth clipper before a careful limiter.
    Modern,
    /// Glue: slow, a soft knee and a little warmth.
    Bus,
    /// Never distorts: its whole lookahead to move, true peaks always caught, no clipping.
    Safe,
    /// The classic digital brickwall: a release that adapts to the music.
    Classic,
    /// Lows, mids and highs limited apart first, so a kick doesn't duck the cymbals.
    Multiband,
}

impl LimitStyle {
    pub const ALL: [LimitStyle; 10] = [
        LimitStyle::Transparent,
        LimitStyle::Punchy,
        LimitStyle::Dynamic,
        LimitStyle::Allround,
        LimitStyle::Aggressive,
        LimitStyle::Modern,
        LimitStyle::Bus,
        LimitStyle::Safe,
        LimitStyle::Classic,
        LimitStyle::Multiband,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LimitStyle::Transparent => "Transparent",
            LimitStyle::Punchy => "Punchy",
            LimitStyle::Dynamic => "Dynamic",
            LimitStyle::Allround => "Allround",
            LimitStyle::Aggressive => "Aggressive",
            LimitStyle::Modern => "Modern",
            LimitStyle::Bus => "Bus",
            LimitStyle::Safe => "Safe",
            LimitStyle::Classic => "Classic",
            LimitStyle::Multiband => "Multiband",
        }
    }

    /// What it's like, in a line.
    pub fn about(self) -> &'static str {
        match self {
            LimitStyle::Transparent => "Clean above all: slow, smooth moves",
            LimitStyle::Punchy => "Keeps the hits: grabs late, lets go fast",
            LimitStyle::Dynamic => "Short peaks quick, long squeezes slow",
            LimitStyle::Allround => "Balanced, for most things",
            LimitStyle::Aggressive => "As loud as it gets",
            LimitStyle::Modern => "Dense and loud, still clean",
            LimitStyle::Bus => "Glue: slow, soft, a little warm",
            LimitStyle::Safe => "Never distorts",
            LimitStyle::Classic => "The classic digital brickwall",
            LimitStyle::Multiband => "Lows, mids and highs apart",
        }
    }
}

/// A channel's limiter, last in its chain by default: brought up by `gain_db` into a ceiling no
/// sample (or, with `true_peak`, nothing between samples) goes over, reading `lookahead_ms`
/// ahead so it's turned down before a peak arrives.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Limiter {
    pub style: LimitStyle,
    /// How far it's driven in, dB (0 to 30).
    pub gain_db: f32,
    /// What nothing goes over, dBFS (-12 to 0).
    pub ceiling_db: f32,
    /// How fast it lets go (ms, 1 to 2000); with `auto_release`, as the music needs, around it.
    pub release_ms: f32,
    pub auto_release: bool,
    /// How far ahead it reads (ms, 0.1 to 10), and how much of that it takes to come down (ms):
    /// shorter is punchier, longer cleaner.
    pub lookahead_ms: f32,
    pub attack_ms: f32,
    /// How much the sides are turned down together (0 apart, 1 as one).
    pub link: f32,
    /// How hard peaks are rounded off before it limits, 0 to 1.
    pub clip: f32,
    pub true_peak: bool,
    /// How many times the sample rate it runs at: 1, 2, 4 or 8.
    pub oversample: u8,
    /// A DC offset taken out first.
    pub dc: bool,
    /// For listening, never in a bounce: heard at the level it came in (its gain taken back
    /// off), or only what it takes away.
    pub unity: bool,
    pub delta: bool,
    /// The loudness Learn aimed at, LUFS, shown on its meter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_lufs: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bypass: bool,
}

impl Default for Limiter {
    fn default() -> Self {
        Limiter {
            style: LimitStyle::Transparent,
            gain_db: 0.0,
            ceiling_db: -1.0,
            release_ms: 120.0,
            auto_release: true,
            lookahead_ms: 2.0,
            attack_ms: 1.5,
            link: 1.0,
            clip: 0.0,
            true_peak: true,
            oversample: 1,
            dc: false,
            unity: false,
            delta: false,
            target_lufs: None,
            bypass: false,
        }
    }
}

/// A saturator as JSON: its bands as a list; and, read but no longer written, a drive and tone
/// for every band at once (how one was saved before it had bands: then one warm tape band).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct SaturatorJson {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    bands: Vec<SatBand>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    crossovers: Vec<f32>,
    slope: u8,
    quality: u8,
    mix: f32,
    output_db: f32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    bypass: bool,
    #[serde(skip_serializing)]
    drive_db: Option<f32>,
    #[serde(skip_serializing)]
    tone: Option<f32>,
}

impl Default for SaturatorJson {
    fn default() -> Self {
        let s = Saturator::default();
        SaturatorJson {
            bands: Vec::new(),
            crossovers: Vec::new(),
            slope: s.slope,
            quality: s.quality,
            mix: s.mix,
            output_db: s.output_db,
            bypass: false,
            drive_db: None,
            tone: None,
        }
    }
}

impl From<SaturatorJson> for Saturator {
    fn from(j: SaturatorJson) -> Self {
        let mut bands = match j.bands.is_empty() {
            true => vec![SatBand::default()],
            false => j.bands,
        };
        for b in &mut bands {
            b.drive_db = j.drive_db.unwrap_or(b.drive_db);
            b.tone = j.tone.unwrap_or(b.tone);
        }
        let bands = SatBands::from_slice(&bands);
        let mut crossovers = sat_crossovers(bands.len());
        for (c, &given) in crossovers.iter_mut().zip(&j.crossovers) {
            *c = given.clamp(20.0, 20_000.0);
        }
        Saturator {
            bands,
            crossovers,
            slope: match j.slope {
                12 | 48 => j.slope,
                _ => 24,
            },
            quality: match j.quality {
                4 | 8 => j.quality,
                _ => 2,
            },
            mix: j.mix,
            output_db: j.output_db,
            bypass: j.bypass,
        }
    }
}

impl From<Saturator> for SaturatorJson {
    fn from(s: Saturator) -> Self {
        let n = s.bands.len();
        SaturatorJson {
            bands: s.bands.to_vec(),
            crossovers: s.crossovers[..n - 1].to_vec(),
            slope: s.slope,
            quality: s.quality,
            mix: s.mix,
            output_db: s.output_db,
            bypass: s.bypass,
            drive_db: None,
            tone: None,
        }
    }
}
