//! The synth's patches: what each is called, the part it's for, how it sounds, and how it's made.

use super::*;

/// What each patch is called, the part it's for, and how it sounds.
pub const POLY_PATCHES: [(&str, &str, &str); 30] = [
    ("Init", "lead", "one saw, the filter open"),
    (
        "Sub Bass",
        "bass",
        "a sine and a little of its octave, round and deep",
    ),
    (
        "Saw Bass",
        "bass",
        "two saws through the ladder, a quick pluck of filter",
    ),
    (
        "Analog Bass",
        "bass",
        "fat, warm, driven hard through the ladder",
    ),
    ("Reese", "bass", "detuned saws, moving and growling"),
    (
        "Acid",
        "bass",
        "a square through a screaming diode ladder, sliding",
    ),
    (
        "Growl",
        "bass",
        "a folded formant table, its position swept by the LFO",
    ),
    (
        "Wobble",
        "bass",
        "a filter thrown open and shut in time with the song",
    ),
    ("Supersaw", "lead", "seven detuned saws, wide and bright"),
    (
        "Trance Lead",
        "lead",
        "two stacked unisons, bright, a touch of glide",
    ),
    (
        "Square Lead",
        "lead",
        "a hollow square, vibrato on the wheel, a little glide",
    ),
    ("Soft Lead", "lead", "a triangle and sine, gentle"),
    (
        "Ladder Lead",
        "lead",
        "a singing, overdriven mono lead, the classic solo",
    ),
    (
        "Sync Lead",
        "lead",
        "a hard-synced saw, its sweep on the envelope",
    ),
    ("Warm Pad", "pad", "saws slowly opening, wide, long release"),
    ("Glass Pad", "pad", "glassy partials, slow, shimmering"),
    ("Choir", "pad", "breathing voices through a formant filter"),
    (
        "Evolving Pad",
        "pad",
        "tables drifting through their frames on slow LFOs",
    ),
    (
        "Strings",
        "pad",
        "detuned saws and pulses, soft attack, warm",
    ),
    ("Brass", "lead", "saws swelling open, a brass section's"),
    ("Pluck", "pluck", "a bright saw, the filter snapping shut"),
    ("House Stab", "pluck", "a short, wide chord stab, resonant"),
    ("Arp Pulse", "pluck", "a pulse arpeggiated in sixteenths"),
    ("Bell", "keys", "FM partials, a long ring"),
    (
        "FM Keys",
        "keys",
        "a soft electric piano from two operators",
    ),
    ("Organ", "keys", "drawbars, a little rotary wobble"),
    ("Metal Hit", "fx", "clashing partials through a comb"),
    (
        "Noise Sweep",
        "fx",
        "noise through a resonant filter, opening over the bar",
    ),
    ("Drone", "pad", "deep, moving, never quite repeating"),
    ("Tape Keys", "keys", "a worn, wandering felt-keys tone"),
];

struct Make(Poly);

impl Make {
    fn new() -> Make {
        let mut p = Poly::default();
        p.params[POLY_MASTER] = -6.0;
        Make(p)
    }

    fn at(&mut self, i: usize, v: f32) -> &mut Make {
        self.0.params[i] = v;
        self
    }

    /// Oscillator `i`: its table, how loud, and where through the table.
    fn osc(&mut self, i: usize, table: &str, level: f32, position: f32) -> &mut Make {
        self.0.tables[i] = table.into();
        self.at(osc_param(i, OscField::Level), level);
        self.at(osc_param(i, OscField::Position), position)
    }

    fn tune(&mut self, i: usize, octave: f32, semi: f32, fine: f32) -> &mut Make {
        self.at(osc_param(i, OscField::Octave), octave);
        self.at(osc_param(i, OscField::Semi), semi);
        self.at(osc_param(i, OscField::Fine), fine)
    }

    fn unison(&mut self, i: usize, voices: f32, detune: f32) -> &mut Make {
        self.at(osc_param(i, OscField::Unison), voices);
        self.at(osc_param(i, OscField::Detune), detune)
    }

    fn warp(&mut self, i: usize, w: Warp, amount: f32) -> &mut Make {
        self.0.warps[i][0] = w;
        self.at(osc_param(i, OscField::WarpA), amount)
    }

    fn filter(&mut self, i: usize, kind: PolyFilter, hz: f32, res: f32, drive: f32) -> &mut Make {
        self.0.filters[i] = kind;
        self.at(filter_param(i, FilterField::Cutoff), hz);
        self.at(filter_param(i, FilterField::Resonance), res);
        self.at(filter_param(i, FilterField::Drive), drive)
    }

    fn env(&mut self, i: usize, a: f32, d: f32, s: f32, r: f32) -> &mut Make {
        self.at(env_param(i, EnvField::Attack), a);
        self.at(env_param(i, EnvField::Decay), d);
        self.at(env_param(i, EnvField::Sustain), s);
        self.at(env_param(i, EnvField::Release), r)
    }

    fn route(&mut self, source: PolySource, dest: usize, depth: f32) -> &mut Make {
        self.0.routes.push(PolyRoute {
            source,
            dest: PolyDest::Param(dest as u16),
            depth,
            by: None,
        });
        self
    }

    /// Envelope 2 on filter 1's cutoff.
    fn sweep(&mut self, depth: f32) -> &mut Make {
        let cut = filter_param(0, FilterField::Cutoff);
        self.route(PolySource::Env(1), cut, depth)
    }

    fn lfo(&mut self, i: usize, wave: LfoWave, hz: f32, beats: f32) -> &mut Make {
        self.0.lfos[i].wave = wave;
        self.0.lfos[i].beats = beats;
        self.at(lfo_param(i, LfoField::Rate), hz)
    }

    fn mono(&mut self, legato: bool, glide_ms: f32) -> &mut Make {
        self.0.voicing.mono = true;
        self.0.voicing.legato = legato;
        self.at(POLY_GLIDE, glide_ms)
    }
}

const POSITION: fn(usize) -> usize = |i| osc_param(i, OscField::Position);

/// A patch of the synth by name (any case), or None.
pub fn poly_patch(name: &str) -> Option<Poly> {
    let name = POLY_PATCHES
        .iter()
        .find(|p| p.0.eq_ignore_ascii_case(name))?
        .0;
    const SAW: f32 = 2.0 / 3.0;
    const SQUARE: f32 = 1.0;
    const TRIANGLE: f32 = 1.0 / 3.0;
    let cut = filter_param(0, FilterField::Cutoff);
    let res = filter_param(0, FilterField::Resonance);
    let mut m = Make::new();
    match name {
        "Init" => {
            m.osc(0, "Basic", 0.75, SAW).env(0, 1.0, 300.0, 1.0, 120.0);
        }
        "Sub Bass" => {
            m.osc(0, "Basic", 0.8, 0.0)
                .osc(1, "Basic", 0.12, TRIANGLE)
                .tune(1, 1.0, 0.0, 0.0)
                .env(0, 2.0, 300.0, 1.0, 80.0)
                .mono(true, 40.0)
                .filter(0, PolyFilter::Ladder2, 900.0, 0.05, 0.0);
        }
        "Saw Bass" => {
            m.osc(0, "Basic", 0.7, SAW)
                .osc(1, "Basic", 0.5, SAW)
                .tune(1, -1.0, 0.0, 0.0)
                .filter(0, PolyFilter::Ladder, 320.0, 0.25, 0.3)
                .env(0, 1.0, 300.0, 0.7, 90.0)
                .env(1, 1.0, 220.0, 0.1, 100.0)
                .sweep(0.32)
                .mono(false, 0.1);
        }
        "Analog Bass" => {
            m.osc(0, "Analog", 0.8, 0.0)
                .osc(1, "Analog", 0.6, 1.0 / 3.0)
                .tune(1, -1.0, 0.0, 6.0)
                .filter(0, PolyFilter::Ladder, 260.0, 0.3, 0.65)
                .env(0, 1.0, 400.0, 0.8, 120.0)
                .env(1, 1.0, 180.0, 0.05, 100.0)
                .sweep(0.34)
                .at(POLY_DRIFT, 0.35)
                .at(filter_param(0, FilterField::Bass), 0.6)
                .at(filter_param(0, FilterField::KeyTrack), 0.4)
                .mono(false, 0.1);
        }
        "Reese" => {
            m.osc(0, "Basic", 0.7, SAW)
                .unison(0, 4.0, 28.0)
                .osc(1, "Basic", 0.5, SAW)
                .tune(1, -1.0, 0.0, 0.0)
                .unison(1, 2.0, 14.0)
                .filter(0, PolyFilter::Ladder, 700.0, 0.2, 0.3)
                .env(0, 1.0, 300.0, 1.0, 120.0)
                .lfo(0, LfoWave::Sine, 0.3, 0.0)
                .route(PolySource::Lfo(0), cut, 0.06)
                .mono(true, 60.0);
        }
        "Acid" => {
            m.osc(0, "Basic", 0.75, SQUARE)
                .filter(0, PolyFilter::Diode, 380.0, 0.78, 0.5)
                .env(0, 1.0, 250.0, 0.6, 60.0)
                .env(1, 1.0, 180.0, 0.0, 60.0)
                .sweep(0.36)
                .at(filter_param(0, FilterField::KeyTrack), 0.3)
                .mono(true, 70.0);
        }
        "Growl" => {
            m.osc(0, "Growl", 0.75, 0.3)
                .unison(0, 3.0, 10.0)
                .osc(1, "Basic", 0.5, 0.0)
                .tune(1, -1.0, 0.0, 0.0)
                .filter(0, PolyFilter::Snarl, 1800.0, 0.35, 0.5)
                .env(0, 1.0, 300.0, 1.0, 100.0)
                .lfo(0, LfoWave::Triangle, 2.0, 1.0)
                .route(PolySource::Lfo(0), POSITION(0), 0.4)
                .route(PolySource::Lfo(0), cut, 0.08)
                .mono(true, 30.0);
        }
        "Wobble" => {
            m.osc(0, "Basic", 0.7, SAW)
                .unison(0, 3.0, 12.0)
                .osc(1, "Basic", 0.5, SQUARE)
                .tune(1, -1.0, 0.0, 0.0)
                .filter(0, PolyFilter::Ladder, 500.0, 0.45, 0.45)
                .env(0, 1.0, 300.0, 1.0, 80.0)
                .lfo(0, LfoWave::Sine, 2.0, 0.5)
                .route(PolySource::Lfo(0), cut, 0.3)
                .mono(true, 25.0);
        }
        "Supersaw" => {
            m.osc(0, "Basic", 0.8, SAW)
                .unison(0, 7.0, 32.0)
                .osc(1, "Soft Saw", 0.3, 0.0)
                .tune(1, 1.0, 0.0, 0.0)
                .unison(1, 5.0, 24.0)
                .filter(0, PolyFilter::Low12, 7000.0, 0.1, 0.0)
                .env(0, 4.0, 300.0, 1.0, 300.0);
        }
        "Trance Lead" => {
            m.osc(0, "Analog", 0.75, 0.0)
                .unison(0, 8.0, 22.0)
                .osc(1, "Analog", 0.6, 0.0)
                .tune(1, 1.0, 0.0, 0.0)
                .unison(1, 8.0, 26.0)
                .filter(0, PolyFilter::Ladder2, 5200.0, 0.15, 0.25)
                .env(0, 1.0, 300.0, 1.0, 220.0)
                .env(1, 1.0, 500.0, 0.3, 200.0)
                .sweep(0.14)
                .at(POLY_DRIFT, 0.2);
        }
        "Square Lead" => {
            m.osc(0, "Basic", 0.7, SQUARE)
                .osc(1, "Analog", 0.25, 2.0 / 3.0)
                .tune(1, 1.0, 0.0, 0.0)
                .filter(0, PolyFilter::Low24, 4200.0, 0.2, 0.0)
                .env(0, 3.0, 300.0, 1.0, 150.0)
                .lfo(0, LfoWave::Sine, 5.5, 0.0)
                .route(PolySource::Wheel, osc_param(0, OscField::Fine), 0.0)
                .mono(true, 35.0);
            let fine = osc_param(0, OscField::Fine);
            m.0.routes[0] = PolyRoute {
                source: PolySource::Lfo(0),
                dest: PolyDest::Param(fine as u16),
                depth: 0.15,
                by: Some(PolySource::Wheel),
            };
        }
        "Soft Lead" => {
            m.osc(0, "Basic", 0.7, TRIANGLE)
                .osc(1, "Basic", 0.3, 0.0)
                .tune(1, 1.0, 0.0, 0.0)
                .filter(0, PolyFilter::Low12, 3500.0, 0.0, 0.0)
                .env(0, 12.0, 300.0, 1.0, 220.0);
        }
        "Ladder Lead" => {
            m.osc(0, "Analog", 0.75, 0.0)
                .osc(1, "Analog", 0.7, 0.0)
                .tune(1, 0.0, 0.0, 9.0)
                .osc(2, "Analog", 0.5, 1.0 / 3.0)
                .tune(2, -1.0, 0.0, -5.0)
                .filter(0, PolyFilter::Ladder, 1100.0, 0.42, 0.7)
                .env(0, 2.0, 400.0, 0.9, 180.0)
                .env(1, 2.0, 600.0, 0.35, 200.0)
                .sweep(0.22)
                .at(POLY_DRIFT, 0.4)
                .at(filter_param(0, FilterField::KeyTrack), 0.7)
                .mono(true, 50.0);
        }
        "Sync Lead" => {
            m.osc(0, "Basic", 0.75, SAW)
                .warp(0, Warp::Sync, 0.15)
                .osc(1, "Basic", 0.25, SAW)
                .tune(1, -1.0, 0.0, 0.0)
                .env(0, 1.0, 300.0, 1.0, 180.0)
                .env(1, 1.0, 700.0, 0.2, 300.0)
                .route(PolySource::Env(1), osc_param(0, OscField::WarpA), 0.65)
                .mono(false, 15.0);
        }
        "Warm Pad" => {
            m.osc(0, "Analog", 0.6, 0.0)
                .unison(0, 3.0, 14.0)
                .osc(1, "Analog", 0.5, 0.0)
                .tune(1, 0.0, 0.0, 7.0)
                .unison(1, 3.0, 14.0)
                .filter(0, PolyFilter::Ladder2, 1400.0, 0.15, 0.1)
                .env(0, 900.0, 1200.0, 0.85, 1800.0)
                .env(1, 1600.0, 2000.0, 0.4, 1500.0)
                .sweep(0.13)
                .lfo(0, LfoWave::Sine, 0.25, 0.0)
                .route(PolySource::Lfo(0), cut, 0.03)
                .at(POLY_DRIFT, 0.3);
        }
        "Glass Pad" => {
            m.osc(0, "Glass", 0.6, 0.3)
                .unison(0, 3.0, 9.0)
                .osc(1, "Glass", 0.4, 0.7)
                .tune(1, 1.0, 0.0, 0.0)
                .unison(1, 3.0, 9.0)
                .env(0, 1200.0, 500.0, 1.0, 2200.0)
                .lfo(0, LfoWave::Sine, 0.13, 0.0)
                .route(PolySource::Lfo(0), POSITION(0), 0.25)
                .route(PolySource::Lfo(0), POSITION(1), -0.2);
        }
        "Choir" => {
            m.osc(0, "Choir", 0.7, 0.4)
                .unison(0, 4.0, 11.0)
                .osc(1, "Formant", 0.4, 0.3)
                .tune(1, -1.0, 0.0, 0.0)
                .filter(0, PolyFilter::Formant, 900.0, 0.4, 0.0)
                .env(0, 450.0, 400.0, 1.0, 900.0)
                .lfo(0, LfoWave::Sine, 0.2, 0.0)
                .route(PolySource::Lfo(0), cut, 0.08)
                .at(filter_param(0, FilterField::Mix), 0.6);
        }
        "Evolving Pad" => {
            m.osc(0, "Harmonics", 0.55, 0.2)
                .unison(0, 4.0, 12.0)
                .osc(1, "Metal", 0.3, 0.4)
                .tune(1, 1.0, 7.0, 0.0)
                .unison(1, 2.0, 8.0)
                .osc(2, "Air", 0.25, 0.5)
                .tune(2, 2.0, 0.0, 0.0)
                .filter(0, PolyFilter::Cascade, 2400.0, 0.25, 0.1)
                .env(0, 1500.0, 2000.0, 0.8, 3000.0)
                .lfo(0, LfoWave::Smooth, 0.11, 0.0)
                .lfo(1, LfoWave::Rossler, 0.2, 0.0)
                .route(PolySource::Lfo(0), POSITION(0), 0.35)
                .route(PolySource::Lfo(1), POSITION(1), 0.35)
                .route(PolySource::Lfo(1), cut, 0.08);
        }
        "Strings" => {
            m.osc(0, "Analog", 0.6, 0.0)
                .unison(0, 4.0, 12.0)
                .osc(1, "Analog", 0.35, 2.0 / 3.0)
                .tune(1, 0.0, 0.0, 4.0)
                .unison(1, 3.0, 10.0)
                .filter(0, PolyFilter::Low12, 3200.0, 0.08, 0.0)
                .env(0, 280.0, 500.0, 1.0, 700.0);
        }
        "Brass" => {
            m.osc(0, "Analog", 0.7, 0.0)
                .unison(0, 3.0, 10.0)
                .osc(1, "Analog", 0.5, 0.0)
                .tune(1, 0.0, 0.0, -5.0)
                .filter(0, PolyFilter::Ladder, 900.0, 0.15, 0.2)
                .env(0, 40.0, 300.0, 1.0, 180.0)
                .env(1, 110.0, 500.0, 0.55, 200.0)
                .sweep(0.22)
                .at(filter_param(0, FilterField::KeyTrack), 0.5)
                .at(POLY_DRIFT, 0.25);
        }
        "Pluck" => {
            m.osc(0, "Analog", 0.75, 0.0)
                .unison(0, 2.0, 8.0)
                .osc(1, "Basic", 0.4, SAW)
                .tune(1, 1.0, 0.0, 0.0)
                .filter(0, PolyFilter::Ladder2, 600.0, 0.3, 0.1)
                .env(0, 1.0, 380.0, 0.0, 220.0)
                .env(1, 1.0, 160.0, 0.0, 160.0)
                .sweep(0.4)
                .at(filter_param(0, FilterField::KeyTrack), 0.6);
        }
        "House Stab" => {
            m.osc(0, "Basic", 0.7, SAW)
                .unison(0, 5.0, 16.0)
                .filter(0, PolyFilter::Ladder, 900.0, 0.5, 0.2)
                .env(0, 1.0, 240.0, 0.0, 120.0)
                .env(1, 1.0, 120.0, 0.0, 100.0)
                .sweep(0.28);
        }
        "Arp Pulse" => {
            m.osc(0, "PWM", 0.75, 0.4)
                .filter(0, PolyFilter::Ladder, 1500.0, 0.35, 0.15)
                .env(0, 1.0, 180.0, 0.0, 80.0)
                .env(1, 1.0, 120.0, 0.0, 80.0)
                .sweep(0.2);
            m.0.arp = Arp {
                mode: ArpMode::UpDown,
                beats: 0.25,
                octaves: 2,
                gate: 0.7,
                swing: 0.0,
            };
        }
        "Bell" => {
            m.osc(0, "FM 7:2", 0.7, 0.45)
                .osc(1, "Bell", 0.3, 0.8)
                .tune(1, 1.0, 7.0, 0.0)
                .env(0, 1.0, 2400.0, 0.0, 1500.0)
                .env(1, 1.0, 900.0, 0.0, 900.0)
                .route(PolySource::Env(1), POSITION(0), 0.4);
        }
        "FM Keys" => {
            m.osc(0, "FM 2:1", 0.7, 0.3)
                .env(0, 2.0, 1800.0, 0.2, 400.0)
                .env(1, 1.0, 500.0, 0.05, 300.0)
                .route(PolySource::Env(1), POSITION(0), 0.5)
                .route(PolySource::Velocity, POSITION(0), 0.2);
        }
        "Organ" => {
            m.osc(0, "Organ", 0.8, 0.55)
                .env(0, 4.0, 200.0, 1.0, 60.0)
                .lfo(0, LfoWave::Sine, 6.4, 0.0)
                .route(PolySource::Lfo(0), osc_param(0, OscField::Fine), 0.03)
                .route(PolySource::Lfo(0), osc_param(0, OscField::Level), 0.06);
        }
        "Metal Hit" => {
            m.osc(0, "Metal", 0.75, 0.6)
                .filter(0, PolyFilter::CombUp, 330.0, 0.85, 0.0)
                .env(0, 1.0, 900.0, 0.0, 500.0);
        }
        "Noise Sweep" => {
            m.at(osc_param(0, OscField::Level), 0.0)
                .at(NOISE_LEVEL, 0.8)
                .filter(0, PolyFilter::Band12, 200.0, 0.6, 0.0)
                .env(0, 100.0, 400.0, 1.0, 900.0)
                .env(1, 6000.0, 200.0, 1.0, 900.0)
                .sweep(0.7);
        }
        "Drone" => {
            m.osc(0, "Soft Saw", 0.6, 0.5)
                .unison(0, 5.0, 18.0)
                .osc(1, "Comb", 0.4, 0.2)
                .tune(1, -1.0, 0.0, 0.0)
                .filter(0, PolyFilter::Ladder, 800.0, 0.35, 0.25)
                .env(0, 2500.0, 2000.0, 1.0, 4000.0)
                .lfo(0, LfoWave::Lorenz, 0.2, 0.0)
                .lfo(1, LfoWave::Smooth, 0.07, 0.0)
                .route(PolySource::Lfo(0), cut, 0.1)
                .route(PolySource::Lfo(1), res, 0.15)
                .route(PolySource::Lfo(1), POSITION(1), 0.5);
        }
        "Tape Keys" => {
            m.osc(0, "Basic", 0.6, TRIANGLE)
                .osc(1, "FM 3:1", 0.2, 0.2)
                .tune(1, 1.0, 0.0, 0.0)
                .filter(0, PolyFilter::Low12, 2600.0, 0.05, 0.1)
                .env(0, 3.0, 1200.0, 0.3, 500.0)
                .at(POLY_DRIFT, 0.8)
                .lfo(0, LfoWave::Smooth, 0.6, 0.0)
                .route(PolySource::Lfo(0), osc_param(0, OscField::Fine), 0.03);
        }
        _ => {}
    }
    let mut p = m.0;
    p.patch = name.to_string();
    // A few voices over a stack, about as loud as one.
    p.params[POLY_MASTER] -= 2.0;
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_patch_is_made() {
        for (name, ..) in POLY_PATCHES {
            let p = poly_patch(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(p.patch, name);
            let plays = (0..OSCS).any(|o| p.params[osc_param(o, OscField::Level)] > 0.0)
                || p.params[NOISE_LEVEL] > 0.0;
            assert!(plays, "{name}");
            for (t, q) in p.params.0.iter().zip(POLY_PARAMS.iter()) {
                let (lo, hi) = match q.scale {
                    KnobScale::Lin(a, b) | KnobScale::Log(a, b) => (a, b),
                };
                assert!((lo..=hi).contains(t), "{name}: {} is {t}", q.id);
            }
        }
        assert!(poly_patch("reese").is_some());
        assert!(poly_patch("nothing").is_none());
    }
}
