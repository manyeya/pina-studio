//! The sampler's settings on their knobs: positions, values and text.

use super::*;

/// Milliseconds on a knob: 0 at the bottom, then 1 ms to 10 s, evenly by ratio.
pub(crate) fn ms_of(v: f32) -> f32 {
    if v < 0.01 {
        0.0
    } else {
        10f32.powf(4.0 * (v - 0.01) / 0.99)
    }
}

pub(crate) fn ms_pos(ms: f32) -> f32 {
    if ms <= 0.0 {
        0.0
    } else {
        0.01 + 0.99 * (ms.max(1.0).log10() / 4.0).clamp(0.0, 1.0)
    }
}

pub(crate) fn ms_text(ms: f32) -> String {
    match ms {
        m if m <= 0.0 => "0".into(),
        m if m < 10.0 => format!("{m:.1} ms"),
        m if m < 1000.0 => format!("{m:.0} ms"),
        m => format!("{:.2} s", m / 1000.0),
    }
}

fn log_pos(v: f32, lo: f32, hi: f32) -> f32 {
    (v.max(lo) / lo).ln() / (hi / lo).ln()
}

fn log_of(p: f32, lo: f32, hi: f32) -> f32 {
    lo * (hi / lo).powf(p)
}

fn percent(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

/// The Length knob's top before Full: below it the decay, from nothing to 10 s.
const LENGTH_TOP: f32 = 0.96;

/// An LFO's lengths in time with the song, in beats: a 64th note to four bars.
pub(crate) const BEATS: [f32; 9] = [0.0625, 0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

/// A length in beats as a note value: "1/16", "2 bars".
pub(crate) fn beats_text(b: f32) -> String {
    match b {
        b if b >= 4.0 => match (b / 4.0) as u32 {
            1 => "1 bar".into(),
            n => format!("{n} bars"),
        },
        b => format!("1/{:.0}", 4.0 / b),
    }
}

fn beats_pos(b: f32) -> f32 {
    let i = BEATS
        .iter()
        .position(|&x| x >= b - 1e-4)
        .unwrap_or(BEATS.len() - 1);
    i as f32 / (BEATS.len() - 1) as f32
}

fn beats_of(v: f32) -> f32 {
    BEATS[(v * (BEATS.len() - 1) as f32).round() as usize]
}

/// The LFO's rate on its knob, in Hz or, in time with the song, in beats.
fn rate_pos(hz: f32, beats: f32) -> f32 {
    match beats > 0.0 {
        true => beats_pos(beats),
        false => (hz / 0.05).max(1.0).log10() / 400f32.log10(),
    }
}

fn rate_text(hz: f32, beats: f32) -> String {
    match beats > 0.0 {
        true => beats_text(beats),
        false => format!("{hz:.2} Hz"),
    }
}

/// Where a setting sits on its knob, 0 to 1.
pub fn field_pos(s: &Sampler, f: Field) -> f32 {
    let v = &s.voice;
    let z = |i: usize| s.zones.get(i).cloned().unwrap_or_default();
    match f {
        Field::Attack => ms_pos(v.attack_ms),
        Field::Decay => ms_pos(v.decay_ms),
        Field::Length if v.sustain >= 0.999 => 1.0,
        Field::Length => ms_pos(v.decay_ms) * LENGTH_TOP,
        Field::Sustain => v.sustain,
        Field::Release => ms_pos(v.release_ms),
        Field::Cutoff => (v.cutoff_hz / 20.0).max(1.0).log10() / 3.0,
        Field::Resonance => v.resonance,
        Field::Drive => v.drive,
        Field::FilterEnv => (v.filter_env + 6.0) / 12.0,
        Field::FilterAttack => ms_pos(v.filter_attack_ms),
        Field::FilterDecay => ms_pos(v.filter_decay_ms),
        Field::FilterSustain => v.filter_sustain,
        Field::FilterRelease => ms_pos(v.filter_release_ms),
        Field::KeyTrack => v.key_track,
        Field::Velocity => v.velocity,
        Field::Glide => ms_pos(v.glide_ms),
        Field::LfoRate => rate_pos(v.lfo_hz, v.lfo_beats),
        Field::Vibrato => v.vibrato_cents / 100.0,
        Field::LfoFilter => v.lfo_filter / 4.0,
        Field::Tremolo => v.tremolo,
        Field::Lfo2Rate => rate_pos(v.lfo2.hz, v.lfo2.beats),
        Field::ModAttack => ms_pos(v.mod_env.attack_ms),
        Field::ModDecay => ms_pos(v.mod_env.decay_ms),
        Field::ModSustain => v.mod_env.sustain,
        Field::ModRelease => ms_pos(v.mod_env.release_ms),
        Field::Transpose => (v.transpose + 24.0) / 48.0,
        Field::Volume => (v.gain_db + 24.0) / 36.0,
        Field::Speed => (v.speed + 2.0) / 4.0,
        Field::Position => v.position,
        Field::GrainSize => log_pos(v.grain_ms, 5.0, 2_000.0),
        Field::Density => log_pos(v.density, 1.0, 200.0),
        Field::Spray => v.spray,
        Field::GrainPitch => v.grain_pitch / 1200.0,
        Field::GrainReverse => v.grain_reverse,
        Field::GrainSpread => v.grain_spread,
        Field::GrainShape => v.grain_shape,
        Field::Unison => (v.unison.max(1) - 1) as f32 / 7.0,
        Field::Detune => v.detune_cents / 100.0,
        Field::Spread => v.spread,
        Field::PitchEnv => (v.pitch_env + 48.0) / 96.0,
        Field::PitchDecay => ms_pos(v.pitch_decay_ms),
        Field::Sub => v.sub,
        Field::Noise => v.noise,
        Field::NoiseColor => (v.noise_color + 1.0) / 2.0,
        Field::Fm => v.fm,
        Field::Ring => v.ring,
        Field::OscRatio => log_pos(v.osc_ratio, 0.25, 16.0),
        Field::Punch => v.punch,
        Field::Color => v.color,
        Field::CrushKhz => log_pos(v.crush_khz, 4.0, 48.0),
        Field::Bits => (v.bits.clamp(2, 16) - 2) as f32 / 14.0,
        Field::Route(i) => (v.matrix.slots()[i.min(7)].amount + 1.0) / 2.0,
        Field::Note(i) => z(i).note as f32 / 127.0,
        Field::High(i) => z(i).high.unwrap_or(z(i).note) as f32 / 127.0,
        Field::Root(i) => z(i).root.unwrap_or(z(i).note) as f32 / 127.0,
        Field::Tune(i) => (z(i).tune_cents + 100.0) / 200.0,
        Field::Gain(i) => (z(i).gain_db + 24.0) / 36.0,
        Field::Pan(i) => (z(i).pan + 1.0) / 2.0,
        Field::VelLow(i) => z(i).velocities.unwrap_or((1, 127)).0 as f32 / 127.0,
        Field::VelHigh(i) => z(i).velocities.unwrap_or((1, 127)).1 as f32 / 127.0,
        Field::VelFade(i) => z(i).velocity_fade as f32 / 64.0,
        Field::LoopFade(i) => z(i).loop_fade * 2.0,
    }
    .clamp(0.0, 1.0)
}

/// Turns a setting to `v` (0 to 1) on its knob.
pub fn field_set(s: &mut Sampler, f: Field, v: f32) {
    let v = v.clamp(0.0, 1.0);
    let note = |v: f32| (v * 127.0).round() as u8;
    let hundredths = |v: f32| (v * 100.0).round() / 100.0;
    let voice = &mut s.voice;
    match f {
        Field::Attack => voice.attack_ms = ms_of(v),
        Field::Decay => voice.decay_ms = ms_of(v),
        Field::Length if v > LENGTH_TOP + 0.01 => voice.sustain = 1.0,
        Field::Length => {
            voice.sustain = 0.0;
            voice.decay_ms = ms_of(v / LENGTH_TOP).max(5.0).round();
        }
        Field::Sustain => voice.sustain = v,
        Field::Release => voice.release_ms = ms_of(v),
        Field::Cutoff => voice.cutoff_hz = 20.0 * 1000f32.powf(v),
        Field::Resonance => voice.resonance = v,
        Field::Drive => voice.drive = hundredths(v),
        Field::FilterEnv => voice.filter_env = ((v * 12.0 - 6.0) * 10.0).round() / 10.0,
        Field::FilterAttack => voice.filter_attack_ms = ms_of(v),
        Field::FilterDecay => voice.filter_decay_ms = ms_of(v),
        Field::FilterSustain => voice.filter_sustain = v,
        Field::FilterRelease => voice.filter_release_ms = ms_of(v),
        Field::KeyTrack => voice.key_track = v,
        Field::Velocity => voice.velocity = v,
        Field::Glide => voice.glide_ms = ms_of(v).min(4000.0),
        Field::LfoRate => match voice.lfo_beats > 0.0 {
            true => voice.lfo_beats = beats_of(v),
            false => voice.lfo_hz = 0.05 * 400f32.powf(v),
        },
        Field::Vibrato => voice.vibrato_cents = (v * 100.0).round(),
        Field::LfoFilter => voice.lfo_filter = v * 4.0,
        Field::Tremolo => voice.tremolo = v,
        Field::Lfo2Rate => match voice.lfo2.beats > 0.0 {
            true => voice.lfo2.beats = beats_of(v),
            false => voice.lfo2.hz = 0.05 * 400f32.powf(v),
        },
        Field::ModAttack => voice.mod_env.attack_ms = ms_of(v),
        Field::ModDecay => voice.mod_env.decay_ms = ms_of(v),
        Field::ModSustain => voice.mod_env.sustain = v,
        Field::ModRelease => voice.mod_env.release_ms = ms_of(v),
        Field::Transpose => voice.transpose = (v * 48.0 - 24.0).round(),
        Field::Volume => voice.gain_db = ((v * 36.0 - 24.0) * 2.0).round() / 2.0,
        Field::Speed => voice.speed = ((v * 4.0 - 2.0) * 100.0).round() / 100.0,
        Field::Position => voice.position = (v * 1000.0).round() / 1000.0,
        Field::GrainSize => voice.grain_ms = log_of(v, 5.0, 2_000.0).round(),
        Field::Density => voice.density = (log_of(v, 1.0, 200.0) * 10.0).round() / 10.0,
        Field::Spray => voice.spray = hundredths(v),
        Field::GrainPitch => voice.grain_pitch = (v * 1200.0).round(),
        Field::GrainReverse => voice.grain_reverse = hundredths(v),
        Field::GrainSpread => voice.grain_spread = hundredths(v),
        Field::GrainShape => voice.grain_shape = hundredths(v),
        Field::Unison => voice.unison = (v * 7.0).round() as u8 + 1,
        Field::Detune => voice.detune_cents = (v * 100.0).round(),
        Field::Spread => voice.spread = hundredths(v),
        Field::PitchEnv => voice.pitch_env = (v * 96.0 - 48.0).round(),
        Field::PitchDecay => voice.pitch_decay_ms = ms_of(v).max(1.0),
        Field::Sub => voice.sub = hundredths(v),
        Field::Noise => voice.noise = hundredths(v),
        Field::NoiseColor => voice.noise_color = hundredths(v * 2.0 - 1.0),
        Field::Fm => voice.fm = hundredths(v),
        Field::Ring => voice.ring = hundredths(v),
        Field::OscRatio => voice.osc_ratio = (log_of(v, 0.25, 16.0) * 4.0).round() / 4.0,
        Field::Punch => voice.punch = hundredths(v),
        Field::Color => voice.color = hundredths(v),
        Field::CrushKhz => voice.crush_khz = (log_of(v, 4.0, 48.0) * 10.0).round() / 10.0,
        Field::Bits => voice.bits = (v * 14.0).round() as u8 + 2,
        Field::Route(i) => voice.matrix.slots_mut()[i.min(7)].amount = hundredths(v * 2.0 - 1.0),
        Field::Note(i)
        | Field::High(i)
        | Field::Root(i)
        | Field::Tune(i)
        | Field::Gain(i)
        | Field::Pan(i)
        | Field::VelLow(i)
        | Field::VelHigh(i)
        | Field::VelFade(i)
        | Field::LoopFade(i) => {
            let Some(z) = s.zones.get_mut(i) else { return };
            match f {
                Field::Note(_) => {
                    let n = note(v);
                    // A range keeps its width; a kit's sound just moves.
                    if let Some(h) = z.high {
                        let width = h.saturating_sub(z.note);
                        z.high = Some(n.saturating_add(width).min(127));
                        z.root = z
                            .root
                            .map(|r| (r as i32 + n as i32 - z.note as i32).clamp(0, 127) as u8);
                    }
                    z.note = n;
                }
                Field::High(_) => {
                    let h = note(v).max(z.note);
                    z.high = (h > z.note).then_some(h);
                }
                Field::Root(_) => z.root = Some(note(v)),
                Field::Tune(_) => z.tune_cents = (v * 200.0 - 100.0).round(),
                Field::Gain(_) => z.gain_db = ((v * 36.0 - 24.0) * 2.0).round() / 2.0,
                Field::Pan(_) => z.pan = ((v * 2.0 - 1.0) * 50.0).round() / 50.0,
                Field::VelLow(_) => {
                    let (_, hi) = z.velocities.unwrap_or((1, 127));
                    let lo = (v * 127.0).round().clamp(1.0, hi as f32) as u8;
                    z.velocities = Some((lo, hi)).filter(|&r| r != (1, 127));
                }
                Field::VelHigh(_) => {
                    let (lo, _) = z.velocities.unwrap_or((1, 127));
                    let hi = (v * 127.0).round().clamp(lo as f32, 127.0) as u8;
                    z.velocities = Some((lo, hi)).filter(|&r| r != (1, 127));
                }
                Field::VelFade(_) => z.velocity_fade = (v * 64.0).round() as u8,
                _ => z.loop_fade = (v * 50.0).round() / 100.0,
            }
        }
    }
}

/// A setting as it reads under its knob.
pub(crate) fn field_text(s: &Sampler, f: Field, key: Option<pina_engine::music::Key>) -> String {
    let v = &s.voice;
    let z = |i: usize| s.zones.get(i).cloned().unwrap_or_default();
    let db = |d: f32| format!("{d:+.1} dB").replace('-', "−");
    match f {
        Field::Attack => ms_text(v.attack_ms),
        Field::Decay => ms_text(v.decay_ms),
        Field::Length if v.sustain >= 0.999 => "Full".into(),
        Field::Length => ms_text(v.decay_ms),
        Field::Sustain => percent(v.sustain),
        Field::FilterSustain => percent(v.filter_sustain),
        Field::Release => ms_text(v.release_ms),
        Field::Cutoff => match v.cutoff_hz {
            c if c >= 1000.0 => format!("{:.1} kHz", c / 1000.0),
            c => format!("{c:.0} Hz"),
        },
        Field::Resonance => percent(v.resonance),
        Field::Drive => percent(v.drive),
        Field::FilterEnv => format!("{:+.1} oct", v.filter_env),
        Field::FilterAttack => ms_text(v.filter_attack_ms),
        Field::FilterDecay => ms_text(v.filter_decay_ms),
        Field::FilterRelease => ms_text(v.filter_release_ms),
        Field::KeyTrack => percent(v.key_track),
        Field::Velocity => percent(v.velocity),
        Field::Glide => ms_text(v.glide_ms),
        Field::LfoRate => rate_text(v.lfo_hz, v.lfo_beats),
        Field::Vibrato => format!("{:.0} ct", v.vibrato_cents),
        Field::LfoFilter => format!("{:.1} oct", v.lfo_filter),
        Field::Tremolo => percent(v.tremolo),
        Field::Lfo2Rate => rate_text(v.lfo2.hz, v.lfo2.beats),
        Field::ModAttack => ms_text(v.mod_env.attack_ms),
        Field::ModDecay => ms_text(v.mod_env.decay_ms),
        Field::ModSustain => percent(v.mod_env.sustain),
        Field::ModRelease => ms_text(v.mod_env.release_ms),
        Field::Transpose => format!("{:+.0} st", v.transpose),
        Field::Volume => db(v.gain_db),
        Field::Speed => match v.speed {
            s if s.abs() < 0.005 => "Still".into(),
            s => format!("{s:.2}×").replace('-', "−"),
        },
        Field::Position => percent(v.position),
        Field::GrainSize => ms_text(v.grain_ms),
        Field::Density => format!("{:.0}/s", v.density),
        Field::Spray => ms_text(v.spray * 1000.0),
        Field::GrainPitch => format!("±{:.0} ct", v.grain_pitch),
        Field::GrainReverse => percent(v.grain_reverse),
        Field::GrainSpread => percent(v.grain_spread),
        Field::GrainShape => match v.grain_shape {
            s if s < 0.05 => "Smooth".into(),
            s if s > 0.95 => "Hard".into(),
            s => percent(s),
        },
        Field::Unison => match v.unison.max(1) {
            1 => "Off".into(),
            n => format!("{n} voices"),
        },
        Field::Detune => format!("{:.0} ct", v.detune_cents),
        Field::Spread => percent(v.spread),
        Field::PitchEnv => format!("{:+.0} st", v.pitch_env),
        Field::PitchDecay => ms_text(v.pitch_decay_ms),
        Field::Sub => percent(v.sub),
        Field::Noise => percent(v.noise),
        Field::NoiseColor => match v.noise_color {
            c if c.abs() < 0.05 => "White".into(),
            c if c < 0.0 => format!("Dark {:.0}", -c * 100.0),
            c => format!("Bright {:.0}", c * 100.0),
        },
        Field::Fm => percent(v.fm),
        Field::Ring => percent(v.ring),
        Field::OscRatio => format!("{:.2}×", v.osc_ratio),
        Field::Punch => percent(v.punch),
        Field::Color => percent(v.color),
        Field::CrushKhz => format!("{:.1} kHz", v.crush_khz),
        Field::Bits => format!("{}-bit", v.bits),
        Field::Route(i) => {
            format!("{:+.0}%", v.matrix.slots()[i.min(7)].amount * 100.0).replace('-', "−")
        }
        Field::Gain(i) => db(z(i).gain_db),
        Field::Note(i) => name_in(key, z(i).note),
        Field::High(i) => name_in(key, z(i).high.unwrap_or(z(i).note)),
        Field::Root(i) => name_in(key, z(i).root.unwrap_or(z(i).note)),
        Field::Tune(i) => format!("{:+.0} ct", z(i).tune_cents),
        Field::Pan(i) => match z(i).pan {
            p if p.abs() < 0.01 => "C".into(),
            p if p < 0.0 => format!("L{:.0}", -p * 100.0),
            p => format!("R{:.0}", p * 100.0),
        },
        Field::VelLow(i) => z(i).velocities.unwrap_or((1, 127)).0.to_string(),
        Field::VelHigh(i) => z(i).velocities.unwrap_or((1, 127)).1.to_string(),
        Field::VelFade(i) => match z(i).velocity_fade {
            0 => "Hard".into(),
            n => n.to_string(),
        },
        Field::LoopFade(i) => percent(z(i).loop_fade),
    }
}

/// Whether a setting's knob turns from a middle (0 there).
pub(crate) fn bipolar(f: Field) -> bool {
    matches!(
        f,
        Field::FilterEnv
            | Field::Transpose
            | Field::Tune(_)
            | Field::Pan(_)
            | Field::Speed
            | Field::PitchEnv
            | Field::NoiseColor
            | Field::Route(_)
    )
}

/// What automation calls a setting of the voice, if it can move it.
pub(crate) fn voice_name(f: Field) -> Option<&'static str> {
    Some(match f {
        Field::Cutoff => "Cutoff",
        Field::Resonance => "Resonance",
        Field::Drive => "Filter drive",
        Field::FilterEnv => "Filter envelope",
        Field::FilterAttack => "Filter attack",
        Field::FilterDecay => "Filter decay",
        Field::FilterSustain => "Filter sustain",
        Field::FilterRelease => "Filter release",
        Field::KeyTrack => "Key track",
        Field::Attack => "Attack",
        Field::Decay => "Decay",
        Field::Sustain => "Sustain",
        Field::Release => "Release",
        Field::Velocity => "Velocity",
        Field::Glide => "Glide",
        Field::Transpose => "Transpose",
        Field::Volume => "Volume",
        Field::LfoRate => "LFO rate",
        Field::Vibrato => "Vibrato",
        Field::LfoFilter => "LFO to filter",
        Field::Tremolo => "Tremolo",
        Field::Detune => "Detune",
        Field::Spread => "Spread",
        Field::PitchEnv => "Pitch envelope",
        Field::PitchDecay => "Pitch decay",
        Field::Sub => "Sub",
        Field::Noise => "Noise",
        Field::Fm => "FM",
        Field::Ring => "Ring",
        Field::Punch => "Punch",
        Field::Position => "Grain position",
        Field::Density => "Grain density",
        _ => return None,
    })
}
