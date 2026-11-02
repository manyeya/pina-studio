//! The tools' knobs, as tables: each knob's name, where its setting sits (0 to 1), how it's set
//! from there, and how it's said. A window lays them out, and automation and modulators move them
//! by their place in the table; nothing else needs to know each one.

use super::*;

/// A knob's range.
#[derive(Clone, Copy)]
pub enum KnobScale {
    Lin(f32, f32),
    Log(f32, f32),
}

impl KnobScale {
    pub fn pos(self, v: f32) -> f32 {
        match self {
            KnobScale::Lin(lo, hi) => (v - lo) / (hi - lo),
            KnobScale::Log(lo, hi) => (v.max(lo) / lo).ln() / (hi / lo).ln(),
        }
        .clamp(0.0, 1.0)
    }

    pub fn value(self, p: f32) -> f32 {
        let p = p.clamp(0.0, 1.0);
        match self {
            KnobScale::Lin(lo, hi) => lo + (hi - lo) * p,
            KnobScale::Log(lo, hi) => lo * (hi / lo).powf(p),
        }
    }
}

pub struct ToolKnob {
    pub name: &'static str,
    pub scale: KnobScale,
    pub pos: fn(&Unit) -> f32,
    pub set: fn(&mut Unit, f32),
    pub say: fn(&Unit) -> String,
    /// Turns from a middle (0 there).
    pub bipolar: bool,
}

fn percent(v: f32) -> String {
    format!("{:.0} %", v * 100.0)
}

fn signed(v: f32) -> String {
    format!("{:+.0} %", v * 100.0)
}

fn hz(v: f32) -> String {
    match v >= 1000.0 {
        true => format!("{:.1} kHz", v / 1000.0),
        false => format!("{v:.0} Hz"),
    }
}

fn db(v: f32) -> String {
    format!("{v:+.1} dB")
}

fn ms(v: f32) -> String {
    format!("{v:.0} ms")
}

fn octaves(v: f32) -> String {
    format!("{v:+.1} oct")
}

fn plain(v: f32) -> String {
    format!("{v:.1}")
}

macro_rules! knob {
    ($name:literal, $v:ident, $f:ident $([$i:literal])?, $scale:expr, $say:expr) => {
        knob!($name, $v, $f $([$i])?, $scale, $say, false)
    };
    ($name:literal, $v:ident, $f:ident $([$i:literal])?, $scale:expr, $say:expr, $bipolar:expr) => {
        ToolKnob {
            name: $name,
            scale: $scale,
            pos: |u| match u {
                Unit::$v(x) => $scale.pos(x.$f$([$i])?),
                _ => 0.0,
            },
            set: |u, p| {
                if let Unit::$v(x) = u {
                    x.$f$([$i])? = $scale.value(p);
                }
            },
            say: |u| match u {
                Unit::$v(x) => $say(x.$f$([$i])?),
                _ => String::new(),
            },
            bipolar: $bipolar,
        }
    };
}

const THRESHOLD: KnobScale = KnobScale::Lin(-60.0, 0.0);
const RATIO: KnobScale = KnobScale::Log(1.0, 20.0);

fn ratio(v: f32) -> String {
    format!("{v:.1}:1")
}

fn beats(v: f32) -> String {
    format!("{v:.2} beats")
}

const OCTAVES: KnobScale = KnobScale::Lin(-6.0, 6.0);
const UNIT: KnobScale = KnobScale::Lin(0.0, 1.0);
const SWING: KnobScale = KnobScale::Lin(-1.0, 1.0);
const LEVEL: KnobScale = KnobScale::Lin(-24.0, 24.0);
const HZ: KnobScale = KnobScale::Log(20.0, 20_000.0);
const RATE: KnobScale = KnobScale::Log(0.02, 20.0);

static FILTER: [ToolKnob; 11] = [
    knob!("Cutoff", Filter, cutoff_hz, HZ, hz),
    knob!("Resonance", Filter, resonance, UNIT, percent),
    knob!("Drive", Filter, drive, UNIT, percent),
    knob!("Mix", Filter, mix, UNIT, percent),
    knob!("Output", Filter, output_db, LEVEL, db, true),
    knob!(
        "LFO depth",
        Filter,
        lfo_depth,
        KnobScale::Lin(0.0, 6.0),
        octaves
    ),
    knob!("LFO rate", Filter, lfo_hz, RATE, |v| format!("{v:.2} Hz")),
    knob!("LFO stereo", Filter, lfo_stereo, UNIT, percent),
    knob!("Env depth", Filter, env_depth, OCTAVES, octaves, true),
    knob!(
        "Env attack",
        Filter,
        env_attack_ms,
        KnobScale::Log(0.5, 200.0),
        ms
    ),
    knob!(
        "Env release",
        Filter,
        env_release_ms,
        KnobScale::Log(5.0, 2000.0),
        ms
    ),
];

static MOTION: [ToolKnob; 8] = [
    knob!("Rate", Motion, rate_hz, RATE, |v| format!("{v:.2} Hz")),
    knob!("Depth", Motion, depth, UNIT, percent),
    knob!(
        "Feedback",
        Motion,
        feedback,
        KnobScale::Lin(-0.95, 0.95),
        signed,
        true
    ),
    knob!(
        "Delay",
        Motion,
        delay_ms,
        KnobScale::Log(0.5, 40.0),
        |v| format!("{v:.1} ms")
    ),
    knob!("Spread", Motion, spread, UNIT, percent),
    ToolKnob {
        name: "Stages",
        scale: KnobScale::Lin(2.0, 12.0),
        pos: |u| match u {
            Unit::Motion(m) => (m.stages as f32 - 2.0) / 10.0,
            _ => 0.0,
        },
        set: |u, p| {
            if let Unit::Motion(m) = u {
                m.stages = ((p * 5.0).round() as u8) * 2 + 2;
            }
        },
        say: |u| match u {
            Unit::Motion(m) => m.stages.to_string(),
            _ => String::new(),
        },
        bipolar: false,
    },
    knob!("Mix", Motion, mix, UNIT, percent),
    knob!("Output", Motion, output_db, LEVEL, db, true),
];

static SHAPER: [ToolKnob; 5] = [
    knob!("Attack", Shaper, attack, SWING, signed, true),
    knob!("Sustain", Shaper, sustain, SWING, signed, true),
    knob!("Speed", Shaper, speed_ms, KnobScale::Log(5.0, 500.0), ms),
    knob!("Mix", Shaper, mix, UNIT, percent),
    knob!("Output", Shaper, output_db, LEVEL, db, true),
];

static GATE: [ToolKnob; 9] = [
    knob!(
        "Threshold",
        Gate,
        threshold_db,
        KnobScale::Lin(-80.0, 0.0),
        db
    ),
    knob!(
        "Range",
        Gate,
        range_db,
        KnobScale::Lin(0.0, 90.0),
        |v| format!("{v:.0} dB")
    ),
    knob!(
        "Ratio",
        Gate,
        ratio,
        KnobScale::Log(1.0, 100.0),
        |v| format!("{v:.1}:1")
    ),
    knob!(
        "Attack",
        Gate,
        attack_ms,
        KnobScale::Log(0.05, 100.0),
        |v| format!("{v:.1} ms")
    ),
    knob!("Hold", Gate, hold_ms, KnobScale::Lin(0.0, 500.0), ms),
    knob!("Release", Gate, release_ms, KnobScale::Log(5.0, 2000.0), ms),
    knob!("Ear low cut", Gate, low_cut_hz, HZ, hz),
    knob!("Ear high cut", Gate, high_cut_hz, HZ, hz),
    knob!("Output", Gate, output_db, LEVEL, db, true),
];

static DE_ESSER: [ToolKnob; 4] = [
    knob!(
        "Frequency",
        DeEsser,
        freq_hz,
        KnobScale::Log(1000.0, 16_000.0),
        hz
    ),
    knob!(
        "Width",
        DeEsser,
        width,
        KnobScale::Lin(0.2, 4.0),
        |v| format!("{v:.1} oct")
    ),
    knob!(
        "Threshold",
        DeEsser,
        threshold_db,
        KnobScale::Lin(-70.0, 0.0),
        db
    ),
    knob!(
        "Range",
        DeEsser,
        range_db,
        KnobScale::Lin(0.0, 40.0),
        |v| format!("{v:.0} dB")
    ),
];

static STEREO: [ToolKnob; 10] = [
    knob!("Lows", Stereo, width[0], KnobScale::Lin(0.0, 2.0), percent),
    knob!(
        "Middle",
        Stereo,
        width[1],
        KnobScale::Lin(0.0, 2.0),
        percent
    ),
    knob!("Highs", Stereo, width[2], KnobScale::Lin(0.0, 2.0), percent),
    knob!(
        "Mono under",
        Stereo,
        mono_hz,
        KnobScale::Lin(0.0, 500.0),
        |v: f32| match v {
            v if v < 1.0 => "Off".to_string(),
            v => hz(v),
        }
    ),
    knob!("Low split", Stereo, crossovers[0], HZ, hz),
    knob!("High split", Stereo, crossovers[1], HZ, hz),
    knob!("Mid", Stereo, mid_db, LEVEL, db, true),
    knob!("Side", Stereo, side_db, LEVEL, db, true),
    knob!("Balance", Stereo, balance, SWING, plain, true),
    knob!("Output", Stereo, output_db, LEVEL, db, true),
];

static MULTIBAND: [ToolKnob; 16] = [
    knob!("Low threshold", Multiband, threshold_db[0], THRESHOLD, db),
    knob!(
        "Middle threshold",
        Multiband,
        threshold_db[1],
        THRESHOLD,
        db
    ),
    knob!("High threshold", Multiband, threshold_db[2], THRESHOLD, db),
    knob!("Low ratio", Multiband, ratio[0], RATIO, ratio),
    knob!("Middle ratio", Multiband, ratio[1], RATIO, ratio),
    knob!("High ratio", Multiband, ratio[2], RATIO, ratio),
    knob!("Low lift", Multiband, lift[0], UNIT, percent),
    knob!("Middle lift", Multiband, lift[1], UNIT, percent),
    knob!("High lift", Multiband, lift[2], UNIT, percent),
    knob!("Low gain", Multiband, gain_db[0], LEVEL, db, true),
    knob!("Middle gain", Multiband, gain_db[1], LEVEL, db, true),
    knob!("High gain", Multiband, gain_db[2], LEVEL, db, true),
    knob!(
        "Attack",
        Multiband,
        attack_ms,
        KnobScale::Log(0.1, 200.0),
        ms
    ),
    knob!(
        "Release",
        Multiband,
        release_ms,
        KnobScale::Log(5.0, 2000.0),
        ms
    ),
    knob!("Mix", Multiband, mix, UNIT, percent),
    knob!("Output", Multiband, output_db, LEVEL, db, true),
];

static PITCH: [ToolKnob; 5] = [
    knob!(
        "Semitones",
        Pitch,
        semitones,
        KnobScale::Lin(-24.0, 24.0),
        |v: f32| { format!("{v:+.1} st") },
        true
    ),
    knob!(
        "Cents",
        Pitch,
        cents,
        KnobScale::Lin(-100.0, 100.0),
        |v: f32| { format!("{v:+.0} ct") },
        true
    ),
    knob!(
        "Feedback",
        Pitch,
        feedback,
        KnobScale::Lin(0.0, 0.9),
        percent
    ),
    knob!("Mix", Pitch, mix, UNIT, percent),
    knob!("Output", Pitch, output_db, LEVEL, db, true),
];

static TAPE_STOP: [ToolKnob; 6] = [
    knob!("Stop", TapeStop, stop, UNIT, percent),
    knob!(
        "Stop time",
        TapeStop,
        stop_beats,
        KnobScale::Log(0.0625, 4.0),
        beats
    ),
    knob!(
        "Start time",
        TapeStop,
        start_beats,
        KnobScale::Log(0.0625, 4.0),
        beats
    ),
    knob!("Curve", TapeStop, curve, UNIT, percent),
    knob!("Mix", TapeStop, mix, UNIT, percent),
    knob!("Output", TapeStop, output_db, LEVEL, db, true),
];

static TUNE: [ToolKnob; 5] = [
    knob!("Speed", Tune, speed_ms, KnobScale::Lin(0.0, 400.0), ms),
    knob!("Amount", Tune, amount, UNIT, percent),
    knob!("Low", Tune, low_hz, KnobScale::Log(60.0, 500.0), hz),
    knob!("High", Tune, high_hz, KnobScale::Log(200.0, 2000.0), hz),
    knob!("Output", Tune, output_db, LEVEL, db, true),
];

/// A band's knob, on an effect whose bands come and go.
macro_rules! band {
    ($name:literal, $v:ident, $b:literal, $f:ident, $scale:expr, $say:expr) => {
        band!($name, $v, $b, $f, $scale, $say, false)
    };
    ($name:literal, $v:ident, $b:literal, $f:ident, $scale:expr, $say:expr, $bipolar:expr) => {
        ToolKnob {
            name: $name,
            scale: $scale,
            pos: |u| match u {
                Unit::$v(x) => x.bands.get($b).map_or(0.0, |b| $scale.pos(b.$f)),
                _ => 0.0,
            },
            set: |u, p| {
                if let Unit::$v(x) = u
                    && let Some(b) = x.bands.get_mut($b)
                {
                    b.$f = $scale.value(p);
                }
            },
            say: |u| match u {
                Unit::$v(x) => x.bands.get($b).map_or_else(String::new, |b| $say(b.$f)),
                _ => String::new(),
            },
            bipolar: $bipolar,
        }
    };
}

/// A track's own compressor and saturator, where an effect's place among the stacked ones would
/// go: automation and modulators reach them the same way.
pub const OWN_COMP: u8 = 254;
pub const OWN_SAT: u8 = 255;

/// How many knobs each of a saturator's bands has.
pub const SAT_BAND_KNOBS: usize = 7;

static COMP: [ToolKnob; 8] = [
    knob!("Threshold", Comp, threshold_db, THRESHOLD, db),
    knob!("Ratio", Comp, ratio, RATIO, ratio),
    knob!("Attack", Comp, attack_ms, KnobScale::Log(0.1, 100.0), ms),
    knob!(
        "Release",
        Comp,
        release_ms,
        KnobScale::Log(10.0, 2000.0),
        ms
    ),
    knob!("Makeup", Comp, makeup_db, KnobScale::Lin(0.0, 24.0), db),
    knob!("Mix", Comp, mix, UNIT, percent),
    knob!(
        "Sidechain",
        Comp,
        sidechain_hz,
        KnobScale::Log(20.0, 500.0),
        hz
    ),
    knob!("Spectral", Comp, spectral, UNIT, percent),
];

static SAT: [ToolKnob; 44] = [
    band!(
        "Band 1 drive",
        Sat,
        0,
        drive_db,
        KnobScale::Lin(0.0, 24.0),
        db
    ),
    band!("Band 1 tone", Sat, 0, tone, SWING, plain, true),
    band!("Band 1 dynamics", Sat, 0, dynamics, SWING, plain, true),
    band!("Band 1 feedback", Sat, 0, feedback, UNIT, percent),
    band!(
        "Band 1 feedback pitch",
        Sat,
        0,
        feedback_hz,
        KnobScale::Log(20.0, 18_000.0),
        hz
    ),
    band!("Band 1 mix", Sat, 0, mix, UNIT, percent),
    band!("Band 1 level", Sat, 0, level_db, LEVEL, db, true),
    band!(
        "Band 2 drive",
        Sat,
        1,
        drive_db,
        KnobScale::Lin(0.0, 24.0),
        db
    ),
    band!("Band 2 tone", Sat, 1, tone, SWING, plain, true),
    band!("Band 2 dynamics", Sat, 1, dynamics, SWING, plain, true),
    band!("Band 2 feedback", Sat, 1, feedback, UNIT, percent),
    band!(
        "Band 2 feedback pitch",
        Sat,
        1,
        feedback_hz,
        KnobScale::Log(20.0, 18_000.0),
        hz
    ),
    band!("Band 2 mix", Sat, 1, mix, UNIT, percent),
    band!("Band 2 level", Sat, 1, level_db, LEVEL, db, true),
    band!(
        "Band 3 drive",
        Sat,
        2,
        drive_db,
        KnobScale::Lin(0.0, 24.0),
        db
    ),
    band!("Band 3 tone", Sat, 2, tone, SWING, plain, true),
    band!("Band 3 dynamics", Sat, 2, dynamics, SWING, plain, true),
    band!("Band 3 feedback", Sat, 2, feedback, UNIT, percent),
    band!(
        "Band 3 feedback pitch",
        Sat,
        2,
        feedback_hz,
        KnobScale::Log(20.0, 18_000.0),
        hz
    ),
    band!("Band 3 mix", Sat, 2, mix, UNIT, percent),
    band!("Band 3 level", Sat, 2, level_db, LEVEL, db, true),
    band!(
        "Band 4 drive",
        Sat,
        3,
        drive_db,
        KnobScale::Lin(0.0, 24.0),
        db
    ),
    band!("Band 4 tone", Sat, 3, tone, SWING, plain, true),
    band!("Band 4 dynamics", Sat, 3, dynamics, SWING, plain, true),
    band!("Band 4 feedback", Sat, 3, feedback, UNIT, percent),
    band!(
        "Band 4 feedback pitch",
        Sat,
        3,
        feedback_hz,
        KnobScale::Log(20.0, 18_000.0),
        hz
    ),
    band!("Band 4 mix", Sat, 3, mix, UNIT, percent),
    band!("Band 4 level", Sat, 3, level_db, LEVEL, db, true),
    band!(
        "Band 5 drive",
        Sat,
        4,
        drive_db,
        KnobScale::Lin(0.0, 24.0),
        db
    ),
    band!("Band 5 tone", Sat, 4, tone, SWING, plain, true),
    band!("Band 5 dynamics", Sat, 4, dynamics, SWING, plain, true),
    band!("Band 5 feedback", Sat, 4, feedback, UNIT, percent),
    band!(
        "Band 5 feedback pitch",
        Sat,
        4,
        feedback_hz,
        KnobScale::Log(20.0, 18_000.0),
        hz
    ),
    band!("Band 5 mix", Sat, 4, mix, UNIT, percent),
    band!("Band 5 level", Sat, 4, level_db, LEVEL, db, true),
    band!(
        "Band 6 drive",
        Sat,
        5,
        drive_db,
        KnobScale::Lin(0.0, 24.0),
        db
    ),
    band!("Band 6 tone", Sat, 5, tone, SWING, plain, true),
    band!("Band 6 dynamics", Sat, 5, dynamics, SWING, plain, true),
    band!("Band 6 feedback", Sat, 5, feedback, UNIT, percent),
    band!(
        "Band 6 feedback pitch",
        Sat,
        5,
        feedback_hz,
        KnobScale::Log(20.0, 18_000.0),
        hz
    ),
    band!("Band 6 mix", Sat, 5, mix, UNIT, percent),
    band!("Band 6 level", Sat, 5, level_db, LEVEL, db, true),
    knob!("Mix", Sat, mix, UNIT, percent),
    knob!(
        "Output",
        Sat,
        output_db,
        KnobScale::Lin(-12.0, 12.0),
        db,
        true
    ),
];

static LIMIT: [ToolKnob; 4] = [
    knob!("Gain", Limit, gain_db, KnobScale::Lin(0.0, 30.0), db),
    knob!("Ceiling", Limit, ceiling_db, KnobScale::Lin(-30.0, 0.0), db),
    knob!(
        "Release",
        Limit,
        release_ms,
        KnobScale::Log(1.0, 2000.0),
        ms
    ),
    knob!("Clip", Limit, clip, UNIT, percent),
];

static EQ: [ToolKnob; 24] = [
    band!(
        "Band 1 frequency",
        Eq,
        0,
        freq,
        KnobScale::Log(10.0, 30_000.0),
        hz
    ),
    band!(
        "Band 1 gain",
        Eq,
        0,
        gain_db,
        KnobScale::Lin(-30.0, 30.0),
        db,
        true
    ),
    band!("Band 1 width", Eq, 0, q, KnobScale::Log(0.025, 40.0), plain),
    band!(
        "Band 2 frequency",
        Eq,
        1,
        freq,
        KnobScale::Log(10.0, 30_000.0),
        hz
    ),
    band!(
        "Band 2 gain",
        Eq,
        1,
        gain_db,
        KnobScale::Lin(-30.0, 30.0),
        db,
        true
    ),
    band!("Band 2 width", Eq, 1, q, KnobScale::Log(0.025, 40.0), plain),
    band!(
        "Band 3 frequency",
        Eq,
        2,
        freq,
        KnobScale::Log(10.0, 30_000.0),
        hz
    ),
    band!(
        "Band 3 gain",
        Eq,
        2,
        gain_db,
        KnobScale::Lin(-30.0, 30.0),
        db,
        true
    ),
    band!("Band 3 width", Eq, 2, q, KnobScale::Log(0.025, 40.0), plain),
    band!(
        "Band 4 frequency",
        Eq,
        3,
        freq,
        KnobScale::Log(10.0, 30_000.0),
        hz
    ),
    band!(
        "Band 4 gain",
        Eq,
        3,
        gain_db,
        KnobScale::Lin(-30.0, 30.0),
        db,
        true
    ),
    band!("Band 4 width", Eq, 3, q, KnobScale::Log(0.025, 40.0), plain),
    band!(
        "Band 5 frequency",
        Eq,
        4,
        freq,
        KnobScale::Log(10.0, 30_000.0),
        hz
    ),
    band!(
        "Band 5 gain",
        Eq,
        4,
        gain_db,
        KnobScale::Lin(-30.0, 30.0),
        db,
        true
    ),
    band!("Band 5 width", Eq, 4, q, KnobScale::Log(0.025, 40.0), plain),
    band!(
        "Band 6 frequency",
        Eq,
        5,
        freq,
        KnobScale::Log(10.0, 30_000.0),
        hz
    ),
    band!(
        "Band 6 gain",
        Eq,
        5,
        gain_db,
        KnobScale::Lin(-30.0, 30.0),
        db,
        true
    ),
    band!("Band 6 width", Eq, 5, q, KnobScale::Log(0.025, 40.0), plain),
    band!(
        "Band 7 frequency",
        Eq,
        6,
        freq,
        KnobScale::Log(10.0, 30_000.0),
        hz
    ),
    band!(
        "Band 7 gain",
        Eq,
        6,
        gain_db,
        KnobScale::Lin(-30.0, 30.0),
        db,
        true
    ),
    band!("Band 7 width", Eq, 6, q, KnobScale::Log(0.025, 40.0), plain),
    band!(
        "Band 8 frequency",
        Eq,
        7,
        freq,
        KnobScale::Log(10.0, 30_000.0),
        hz
    ),
    band!(
        "Band 8 gain",
        Eq,
        7,
        gain_db,
        KnobScale::Lin(-30.0, 30.0),
        db,
        true
    ),
    band!("Band 8 width", Eq, 7, q, KnobScale::Log(0.025, 40.0), plain),
];

/// Whether a knob's there on this effect: not a band it hasn't got.
pub fn knob_shows(u: &Unit, i: usize) -> bool {
    match u {
        Unit::Eq(e) => i / 3 < e.bands.len(),
        Unit::Sat(s) => i / SAT_BAND_KNOBS < s.bands.len() || i >= 6 * SAT_BAND_KNOBS,
        _ => true,
    }
}

/// A setting of the Sampler's voice, for automation: as an effect's knob, on the voice.
pub struct VoiceKnob {
    pub name: &'static str,
    pub scale: KnobScale,
    pub pos: fn(&SamplerVoice) -> f32,
    pub set: fn(&mut SamplerVoice, f32),
    pub say: fn(&SamplerVoice) -> String,
}

macro_rules! voice {
    ($name:literal, $f:ident, $scale:expr, $say:expr) => {
        VoiceKnob {
            name: $name,
            scale: $scale,
            pos: |v| $scale.pos(v.$f),
            set: |v, p| v.$f = $scale.value(p),
            say: |v| $say(v.$f),
        }
    };
}

const TIME_MS: KnobScale = KnobScale::Log(0.5, 10_000.0);

/// Every setting of the Sampler's voice that moves smoothly.
pub static VOICE: [VoiceKnob; 32] = [
    voice!("Cutoff", cutoff_hz, HZ, hz),
    voice!("Resonance", resonance, UNIT, percent),
    voice!("Filter drive", drive, UNIT, percent),
    voice!(
        "Filter envelope",
        filter_env,
        KnobScale::Lin(-8.0, 8.0),
        octaves
    ),
    voice!("Filter attack", filter_attack_ms, TIME_MS, ms),
    voice!("Filter decay", filter_decay_ms, TIME_MS, ms),
    voice!("Filter sustain", filter_sustain, UNIT, percent),
    voice!("Filter release", filter_release_ms, TIME_MS, ms),
    voice!("Key track", key_track, UNIT, percent),
    voice!("Attack", attack_ms, TIME_MS, ms),
    voice!("Decay", decay_ms, TIME_MS, ms),
    voice!("Sustain", sustain, UNIT, percent),
    voice!("Release", release_ms, TIME_MS, ms),
    voice!("Velocity", velocity, UNIT, percent),
    voice!("Glide", glide_ms, KnobScale::Lin(0.0, 2000.0), ms),
    voice!("Transpose", transpose, KnobScale::Lin(-24.0, 24.0), plain),
    voice!("Volume", gain_db, KnobScale::Lin(-48.0, 12.0), db),
    voice!("LFO rate", lfo_hz, RATE, hz),
    voice!("Vibrato", vibrato_cents, KnobScale::Lin(0.0, 1200.0), plain),
    voice!(
        "LFO to filter",
        lfo_filter,
        KnobScale::Lin(0.0, 6.0),
        octaves
    ),
    voice!("Tremolo", tremolo, UNIT, percent),
    voice!("Detune", detune_cents, KnobScale::Lin(0.0, 100.0), plain),
    voice!("Spread", spread, UNIT, percent),
    voice!(
        "Pitch envelope",
        pitch_env,
        KnobScale::Lin(-48.0, 48.0),
        plain
    ),
    voice!("Pitch decay", pitch_decay_ms, TIME_MS, ms),
    voice!("Sub", sub, UNIT, percent),
    voice!("Noise", noise, UNIT, percent),
    voice!("FM", fm, UNIT, percent),
    voice!("Ring", ring, UNIT, percent),
    voice!("Punch", punch, UNIT, percent),
    voice!("Grain position", position, UNIT, percent),
    voice!("Grain density", density, KnobScale::Lin(1.0, 100.0), plain),
];

static REVERB: [ToolKnob; 15] = [
    knob!(
        "Pre-delay",
        Reverb,
        predelay_ms,
        KnobScale::Lin(0.0, 500.0),
        ms
    ),
    knob!("Size", Reverb, size, UNIT, percent),
    knob!(
        "Decay",
        Reverb,
        decay_s,
        KnobScale::Log(0.1, 30.0),
        |v| format!("{v:.1} s")
    ),
    knob!("Early", Reverb, early, UNIT, percent),
    knob!("Diffusion", Reverb, diffusion, UNIT, percent),
    knob!(
        "Damping",
        Reverb,
        damping_hz,
        KnobScale::Log(500.0, 20_000.0),
        hz
    ),
    knob!("Bass", Reverb, bass, KnobScale::Log(0.25, 4.0), plain),
    knob!("Modulation", Reverb, modulation, UNIT, percent),
    knob!(
        "Low cut",
        Reverb,
        low_cut_hz,
        KnobScale::Log(20.0, 2000.0),
        hz
    ),
    knob!(
        "High cut",
        Reverb,
        high_cut_hz,
        KnobScale::Log(1000.0, 20_000.0),
        hz
    ),
    knob!("Width", Reverb, width, KnobScale::Lin(0.0, 2.0), percent),
    knob!("Ducking", Reverb, ducking, UNIT, percent),
    knob!("Shimmer", Reverb, shimmer, UNIT, percent),
    knob!("Mix", Reverb, mix, UNIT, percent),
    knob!("Output", Reverb, output_db, LEVEL, db, true),
];

static DELAY: [ToolKnob; 14] = [
    knob!("Left time", Delay, ms[0], KnobScale::Log(1.0, 5000.0), ms),
    knob!("Right time", Delay, ms[1], KnobScale::Log(1.0, 5000.0), ms),
    knob!(
        "Feedback",
        Delay,
        feedback,
        KnobScale::Lin(0.0, 1.1),
        percent
    ),
    knob!("Cross", Delay, cross, UNIT, percent),
    knob!(
        "Low cut",
        Delay,
        low_cut_hz,
        KnobScale::Log(20.0, 2000.0),
        hz
    ),
    knob!(
        "High cut",
        Delay,
        high_cut_hz,
        KnobScale::Log(500.0, 20_000.0),
        hz
    ),
    knob!("Drive", Delay, drive, UNIT, percent),
    knob!("Wow", Delay, wow, UNIT, percent),
    knob!("Flutter", Delay, flutter, UNIT, percent),
    knob!("Diffusion", Delay, diffusion, UNIT, percent),
    knob!("Ducking", Delay, ducking, UNIT, percent),
    knob!("Width", Delay, width, KnobScale::Lin(0.0, 2.0), percent),
    knob!("Mix", Delay, mix, UNIT, percent),
    knob!("Output", Delay, output_db, LEVEL, db, true),
];

/// An effect's knobs, by what the chain calls it.
pub fn knobs(kind: Fx) -> &'static [ToolKnob] {
    match kind {
        Fx::Comp => &COMP,
        Fx::Sat => &SAT,
        Fx::Limit => &LIMIT,
        Fx::Eq => &EQ,
        Fx::Reverb => &REVERB,
        Fx::Delay => &DELAY,
        Fx::Filter => &FILTER,
        Fx::Motion => &MOTION,
        Fx::Shaper => &SHAPER,
        Fx::Gate => &GATE,
        Fx::DeEsser => &DE_ESSER,
        Fx::Stereo => &STEREO,
        Fx::Multiband => &MULTIBAND,
        Fx::Pitch => &PITCH,
        Fx::TapeStop => &TAPE_STOP,
        Fx::Tune => &TUNE,
        _ => &[],
    }
}

/// A new tool of `kind`, as it comes; `wet`, on a bus: a reverb or delay all wet.
pub fn fresh_tool(kind: Fx, wet: bool) -> Option<Unit> {
    let mix = |usual: f32| if wet { 1.0 } else { usual };
    Some(match kind {
        Fx::Reverb => Unit::Reverb(Reverb {
            mix: mix(Reverb::default().mix),
            ..Reverb::default()
        }),
        Fx::Delay => Unit::Delay(Delay {
            mix: mix(Delay::default().mix),
            ..Delay::default()
        }),
        Fx::Filter => Unit::Filter(FilterFx::default()),
        Fx::Motion => Unit::Motion(MotionStyle::Chorus.motion()),
        Fx::Shaper => Unit::Shaper(Shaper::default()),
        Fx::Gate => Unit::Gate(Gate::default()),
        Fx::DeEsser => Unit::DeEsser(DeEsser::default()),
        Fx::Stereo => Unit::Stereo(Stereo::default()),
        Fx::Multiband => Unit::Multiband(Multiband::default()),
        Fx::Pitch => Unit::Pitch(PitchShift::default()),
        Fx::TapeStop => Unit::TapeStop(TapeStop::default()),
        Fx::Tune => Unit::Tune(Tune::default()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_knob_goes_round_and_back() {
        for kind in [
            Fx::Comp,
            Fx::Limit,
            Fx::Reverb,
            Fx::Delay,
            Fx::Filter,
            Fx::Motion,
            Fx::Shaper,
            Fx::Gate,
            Fx::DeEsser,
            Fx::Stereo,
            Fx::Multiband,
            Fx::Pitch,
            Fx::TapeStop,
            Fx::Tune,
        ] {
            let mut u = match kind {
                Fx::Comp => Unit::Comp(Comp::default()),
                Fx::Limit => Unit::Limit(Limiter::default()),
                _ => fresh_tool(kind, false).unwrap(),
            };
            for k in knobs(kind) {
                for p in [0.0, 0.3, 1.0] {
                    (k.set)(&mut u, p);
                    let back = (k.pos)(&u);
                    assert!((back - p).abs() < 0.11, "{}: {p} came back {back}", k.name);
                    assert!(!(k.say)(&u).is_empty(), "{}", k.name);
                }
            }
        }
        // Bands it has go round too; those it hasn't do nothing.
        let mut s = Unit::Sat(Saturator::default());
        let drive = &knobs(Fx::Sat)[0];
        (drive.set)(&mut s, 0.5);
        assert_eq!((drive.pos)(&s), 0.5);
        assert!(knob_shows(&s, 0) && !knob_shows(&s, SAT_BAND_KNOBS));
        assert!(knob_shows(&s, 6 * SAT_BAND_KNOBS));
        let absent = &knobs(Fx::Sat)[SAT_BAND_KNOBS];
        (absent.set)(&mut s, 0.5);
        assert_eq!((absent.pos)(&s), 0.0);
        let mut v = SamplerVoice::default();
        for k in &VOICE {
            (k.set)(&mut v, 0.25);
            assert!(((k.pos)(&v) - 0.25).abs() < 1e-3, "{}", k.name);
            assert!(!(k.say)(&v).is_empty(), "{}", k.name);
        }
    }
}
