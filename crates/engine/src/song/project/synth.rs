//! Songs from when the synth was a sampler whose zones were oscillators: each opens as the synth
//! it became, the closest it can be, nothing lost.

use super::*;

/// What an oscillator zone played.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OscWave {
    Sine,
    Triangle,
    Saw,
    Square,
    Pulse,
    Organ,
    Hollow,
    Vocal,
    Glass,
    Bright,
}

impl OscWave {
    /// The wavetable and the place in it that's closest.
    fn table(self) -> (&'static str, f32) {
        match self {
            OscWave::Sine => ("Basic", 0.0),
            OscWave::Triangle => ("Basic", 1.0 / 3.0),
            OscWave::Saw => ("Basic", 2.0 / 3.0),
            OscWave::Square => ("Basic", 1.0),
            OscWave::Pulse => ("Analog", 2.0 / 3.0),
            OscWave::Organ => ("Organ", 0.55),
            OscWave::Hollow => ("Odd", 0.3),
            OscWave::Vocal => ("Formant", 0.3),
            OscWave::Glass => ("Glass", 0.4),
            OscWave::Bright => ("Soft Saw", 0.0),
        }
    }
}

impl Sampler {
    /// Whether it's the old synth: every zone an oscillator.
    pub fn is_synth(&self) -> bool {
        !self.zones.is_empty() && self.zones.iter().all(|z| z.wave.is_some())
    }

    /// The synth it became.
    fn as_poly(&self) -> Poly {
        let mut p = Poly::default();
        let v = &self.voice;
        for (o, z) in self.zones.iter().take(OSCS).enumerate() {
            let (table, position) = z.wave.map_or(("Basic", 2.0 / 3.0), OscWave::table);
            p.tables[o] = table.into();
            let tune = z.tune_cents / 100.0;
            let octave = (tune / 12.0).round().clamp(-4.0, 4.0);
            let semi = (tune - octave * 12.0).round().clamp(-12.0, 12.0);
            for (f, value) in [
                (OscField::Level, 0.75 * 10f32.powf(z.gain_db / 20.0)),
                (OscField::Pan, z.pan),
                (OscField::Position, position),
                (OscField::Octave, octave),
                (OscField::Semi, semi),
                (OscField::Fine, (tune - octave * 12.0 - semi) * 100.0),
                (OscField::Unison, v.unison.clamp(1, 16) as f32),
                (OscField::Detune, v.detune_cents.min(100.0)),
                (OscField::Width, v.spread),
            ] {
                p.params[osc_param(o, f)] = value;
            }
        }
        p.filters[0] = match v.filter {
            FilterMode::Off => PolyFilter::None,
            FilterMode::HighPass => PolyFilter::High12,
            FilterMode::BandPass => PolyFilter::Band12,
            FilterMode::Notch => PolyFilter::Notch,
            FilterMode::Ladder => PolyFilter::Ladder,
            FilterMode::Comb => PolyFilter::CombUp,
            FilterMode::Formant => PolyFilter::Formant,
            _ => PolyFilter::Low12,
        };
        let cut = filter_param(0, FilterField::Cutoff);
        for (i, value) in [
            (cut, v.cutoff_hz),
            (filter_param(0, FilterField::Resonance), v.resonance),
            (filter_param(0, FilterField::Drive), v.drive),
            (filter_param(0, FilterField::KeyTrack), v.key_track),
            (env_param(0, EnvField::Attack), v.attack_ms.max(0.1)),
            (env_param(0, EnvField::Decay), v.decay_ms.max(0.1)),
            (env_param(0, EnvField::Sustain), v.sustain),
            (env_param(0, EnvField::Release), v.release_ms.max(0.1)),
            (env_param(1, EnvField::Attack), v.filter_attack_ms.max(0.1)),
            (env_param(1, EnvField::Decay), v.filter_decay_ms.max(0.1)),
            (env_param(1, EnvField::Sustain), v.filter_sustain),
            (
                env_param(1, EnvField::Release),
                v.filter_release_ms.max(0.1),
            ),
            (lfo_param(0, LfoField::Rate), v.lfo_hz.max(0.01)),
            (SUB_LEVEL, v.sub),
            (NOISE_LEVEL, v.noise),
            (POLY_GLIDE, v.glide_ms.max(0.1)),
        ] {
            p.params[i] = value;
        }
        // Octaves over the ten of the cutoff's knob.
        let mut route = |source, dest: usize, depth: f32| {
            if depth != 0.0 {
                p.routes.push(PolyRoute {
                    source,
                    dest: PolyDest::Param(dest as u16),
                    depth,
                    by: None,
                });
            }
        };
        route(PolySource::Env(1), cut, v.filter_env / 10.0);
        route(PolySource::Lfo(0), cut, v.lfo_filter / 10.0);
        route(
            PolySource::Lfo(0),
            osc_param(0, OscField::Fine),
            v.vibrato_cents / 200.0,
        );
        p.voicing.mono = v.mono;
        p.voicing.legato = v.legato;
        p
    }
}

impl Project {
    /// Old synths, as they are now.
    pub(crate) fn upgrade_synths(&mut self) {
        for t in &mut self.tracks {
            if let Some(Instrument::Sampler(s)) = &t.instrument
                && s.is_synth()
            {
                t.instrument = Some(Instrument::Poly(Box::new(s.as_poly())));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_synth_opens_as_the_new_one() {
        let zone = |wave, tune_cents, gain_db| Zone {
            wave: Some(wave),
            tune_cents,
            gain_db,
            ..Zone::default()
        };
        let old = Sampler {
            zones: vec![
                zone(OscWave::Saw, -1207.0, 0.0),
                zone(OscWave::Glass, 700.0, -6.0),
            ],
            voice: SamplerVoice {
                filter: FilterMode::Ladder,
                cutoff_hz: 900.0,
                filter_env: 3.0,
                attack_ms: 20.0,
                mono: true,
                ..SamplerVoice::default()
            },
        };
        let mut p = Project::new(48_000);
        p.tracks.push(Track {
            instrument: Some(Instrument::Sampler(old)),
            ..Track::default()
        });
        p.upgrade_synths();
        let Some(Instrument::Poly(s)) = &p.tracks[0].instrument else {
            panic!("{:?}", p.tracks[0].instrument);
        };
        assert_eq!(
            (s.tables[0].as_str(), s.tables[1].as_str()),
            ("Basic", "Glass")
        );
        assert_eq!(s.params[osc_param(0, OscField::Octave)], -1.0);
        assert!((s.params[osc_param(0, OscField::Fine)] + 7.0).abs() < 0.01);
        let second = s.params[osc_param(1, OscField::Octave)] * 12.0
            + s.params[osc_param(1, OscField::Semi)];
        assert_eq!(second, 7.0);
        assert!(s.params[osc_param(1, OscField::Level)] < 0.4);
        assert_eq!(s.filters[0], PolyFilter::Ladder);
        assert_eq!(s.params[env_param(0, EnvField::Attack)], 20.0);
        assert_eq!(s.routes.len(), 1);
        assert!(s.voicing.mono);
        // It's saved as what it is now, and a real sampler's left alone.
        assert!(serde_json::to_string(&p).unwrap().contains("\"poly\""));
        let mut q = Project::new(48_000);
        q.tracks.push(Track {
            instrument: Some(Instrument::Sampler(Sampler::default())),
            ..Track::default()
        });
        q.upgrade_synths();
        assert!(matches!(
            q.tracks[0].instrument,
            Some(Instrument::Sampler(_))
        ));
    }
}
