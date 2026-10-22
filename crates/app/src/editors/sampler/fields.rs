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

/// Where a setting sits on its knob, 0 to 1.
pub fn field_pos(s: &Sampler, f: Field) -> f32 {
    let v = &s.voice;
    let z = |i: usize| s.zones.get(i).cloned().unwrap_or_default();
    match f {
        Field::Attack => ms_pos(v.attack_ms),
        Field::Decay => ms_pos(v.decay_ms),
        Field::Sustain => v.sustain,
        Field::Release => ms_pos(v.release_ms),
        Field::Cutoff => (v.cutoff_hz / 20.0).max(1.0).log10() / 3.0,
        Field::Resonance => v.resonance,
        Field::FilterEnv => (v.filter_env + 6.0) / 12.0,
        Field::FilterAttack => ms_pos(v.filter_attack_ms),
        Field::FilterDecay => ms_pos(v.filter_decay_ms),
        Field::FilterSustain => v.filter_sustain,
        Field::FilterRelease => ms_pos(v.filter_release_ms),
        Field::KeyTrack => v.key_track,
        Field::Velocity => v.velocity,
        Field::Glide => ms_pos(v.glide_ms),
        Field::LfoRate => (v.lfo_hz / 0.05).max(1.0).log10() / 400f32.log10(),
        Field::Vibrato => v.vibrato_cents / 100.0,
        Field::LfoFilter => v.lfo_filter / 4.0,
        Field::Tremolo => v.tremolo,
        Field::Transpose => (v.transpose + 24.0) / 48.0,
        Field::Volume => (v.gain_db + 24.0) / 36.0,
        Field::Note(i) => z(i).note as f32 / 127.0,
        Field::High(i) => z(i).high.unwrap_or(z(i).note) as f32 / 127.0,
        Field::Root(i) => z(i).root.unwrap_or(z(i).note) as f32 / 127.0,
        Field::Tune(i) => (z(i).tune_cents + 100.0) / 200.0,
        Field::Gain(i) => (z(i).gain_db + 24.0) / 36.0,
        Field::Pan(i) => (z(i).pan + 1.0) / 2.0,
        Field::VelLow(i) => z(i).velocities.unwrap_or((1, 127)).0 as f32 / 127.0,
        Field::VelHigh(i) => z(i).velocities.unwrap_or((1, 127)).1 as f32 / 127.0,
    }
    .clamp(0.0, 1.0)
}

/// Turns a setting to `v` (0 to 1) on its knob.
pub fn field_set(s: &mut Sampler, f: Field, v: f32) {
    let v = v.clamp(0.0, 1.0);
    let note = |v: f32| (v * 127.0).round() as u8;
    let voice = &mut s.voice;
    match f {
        Field::Attack => voice.attack_ms = ms_of(v),
        Field::Decay => voice.decay_ms = ms_of(v),
        Field::Sustain => voice.sustain = v,
        Field::Release => voice.release_ms = ms_of(v),
        Field::Cutoff => voice.cutoff_hz = 20.0 * 1000f32.powf(v),
        Field::Resonance => voice.resonance = v,
        Field::FilterEnv => voice.filter_env = ((v * 12.0 - 6.0) * 10.0).round() / 10.0,
        Field::FilterAttack => voice.filter_attack_ms = ms_of(v),
        Field::FilterDecay => voice.filter_decay_ms = ms_of(v),
        Field::FilterSustain => voice.filter_sustain = v,
        Field::FilterRelease => voice.filter_release_ms = ms_of(v),
        Field::KeyTrack => voice.key_track = v,
        Field::Velocity => voice.velocity = v,
        Field::Glide => voice.glide_ms = ms_of(v).min(4000.0),
        Field::LfoRate => voice.lfo_hz = 0.05 * 400f32.powf(v),
        Field::Vibrato => voice.vibrato_cents = (v * 100.0).round(),
        Field::LfoFilter => voice.lfo_filter = v * 4.0,
        Field::Tremolo => voice.tremolo = v,
        Field::Transpose => voice.transpose = (v * 48.0 - 24.0).round(),
        Field::Volume => voice.gain_db = ((v * 36.0 - 24.0) * 2.0).round() / 2.0,
        Field::Note(i)
        | Field::High(i)
        | Field::Root(i)
        | Field::Tune(i)
        | Field::Gain(i)
        | Field::Pan(i)
        | Field::VelLow(i)
        | Field::VelHigh(i) => {
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
                _ => {
                    let (lo, _) = z.velocities.unwrap_or((1, 127));
                    let hi = (v * 127.0).round().clamp(lo as f32, 127.0) as u8;
                    z.velocities = Some((lo, hi)).filter(|&r| r != (1, 127));
                }
            }
        }
    }
}

/// A setting as it reads under its knob.
pub(crate) fn field_text(s: &Sampler, f: Field, key: Option<pina_engine::music::Key>) -> String {
    let v = &s.voice;
    let z = |i: usize| s.zones.get(i).cloned().unwrap_or_default();
    match f {
        Field::Attack => ms_text(v.attack_ms),
        Field::Decay => ms_text(v.decay_ms),
        Field::Sustain | Field::FilterSustain => format!(
            "{:.0}%",
            if f == Field::Sustain {
                v.sustain
            } else {
                v.filter_sustain
            } * 100.0
        ),
        Field::Release => ms_text(v.release_ms),
        Field::Cutoff => match v.cutoff_hz {
            c if c >= 1000.0 => format!("{:.1} kHz", c / 1000.0),
            c => format!("{c:.0} Hz"),
        },
        Field::Resonance => format!("{:.0}%", v.resonance * 100.0),
        Field::FilterEnv => format!("{:+.1} oct", v.filter_env),
        Field::FilterAttack => ms_text(v.filter_attack_ms),
        Field::FilterDecay => ms_text(v.filter_decay_ms),
        Field::FilterRelease => ms_text(v.filter_release_ms),
        Field::KeyTrack => format!("{:.0}%", v.key_track * 100.0),
        Field::Velocity => format!("{:.0}%", v.velocity * 100.0),
        Field::Glide => ms_text(v.glide_ms),
        Field::LfoRate => format!("{:.2} Hz", v.lfo_hz),
        Field::Vibrato => format!("{:.0} ct", v.vibrato_cents),
        Field::LfoFilter => format!("{:.1} oct", v.lfo_filter),
        Field::Tremolo => format!("{:.0}%", v.tremolo * 100.0),
        Field::Transpose => format!("{:+.0} st", v.transpose),
        Field::Volume | Field::Gain(_) => {
            let db = if f == Field::Volume {
                v.gain_db
            } else {
                z(match f {
                    Field::Gain(i) => i,
                    _ => 0,
                })
                .gain_db
            };
            format!("{db:+.1} dB").replace('-', "−")
        }
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
    }
}

/// An envelope's shape, drawn: rising, falling to its level, holding, letting go.
pub(crate) fn envelope(attack: f32, decay: f32, sustain: f32, release: f32, color: u32) -> Div {
    let sustain = sustain.clamp(0.0, 1.0);
    // Each stage's width by the log of its time, so short and long both show.
    let w = |ms: f32| 0.15 + (ms.max(0.0) + 1.0).log10();
    let (a, d, r) = (w(attack), w(decay), w(release));
    let hold = 1.2;
    let total = a + d + hold + r;
    div().h(px(20.)).w_full().child(
        canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let (bw, bh) = (f32::from(b.size.width), f32::from(b.size.height));
                let x = |t: f32| x0 + t / total * bw;
                let y = |level: f32| y0 + bh - 2.0 - level * (bh - 4.0);
                let pts = [
                    (x(0.0), y(0.0)),
                    (x(a), y(1.0)),
                    (x(a + d), y(sustain)),
                    (x(a + d + hold), y(sustain)),
                    (x(total), y(0.0)),
                ];
                let mut area = gpui::PathBuilder::fill();
                area.move_to(point(px(pts[0].0), px(y(0.0))));
                for (px_, py) in pts {
                    area.line_to(point(px(px_), px(py)));
                }
                area.close();
                if let Ok(p) = area.build() {
                    window.paint_path(p, alpha(color, 0.15));
                }
                let mut line = gpui::PathBuilder::stroke(px(1.5));
                line.move_to(point(px(pts[0].0), px(pts[0].1)));
                for (px_, py) in &pts[1..] {
                    line.line_to(point(px(*px_), px(*py)));
                }
                if let Ok(p) = line.build() {
                    window.paint_path(p, rgb(color));
                }
            },
        )
        .size_full(),
    )
}
