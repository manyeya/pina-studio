//! A synth's own effects rack: the built-in effects it offers by name (each set as that name
//! says: a tube drive, an ensemble, a hall), what each is called once it's in, and the rack
//! changed (an effect added, taken out, moved) with the synth's routes to its knobs kept on the
//! effect they rode.

use super::*;

/// What each of the rack's effects does, in a line, by the name it's offered under: what the
/// browser says of it, and what its search reads.
pub fn rack_about(name: &str) -> &'static str {
    match name {
        "Tape" => "Warm and rounded: even harmonics, the top softened",
        "Tube" => "Thick and sweet: leans on the second harmonic",
        "Transformer" => "The lows saturate first: a fuller, firmer bass",
        "Crunch" => "An amp pushed: bite and grit on every note",
        "Heavy" => "An amp flat out: saturated, compressed and loud",
        "Rectify" => "Folds the bottom half up: an octave-up fizz",
        "Foldback" => "Folds the wave over itself: metallic and vocal",
        "Lo-Fi" => "Fewer bits, a lower rate: crunchy and old",
        "Chorus" => "Two drifting voices: wider and softer",
        "Ensemble" => "Three voices: the lush, wide string-machine shimmer",
        "Flanger" => "A jet sweep of combs, fed back",
        "Phaser" => "Notches sweeping through: liquid and moving",
        "Vibrato" => "The pitch wavering, all wet",
        "Tremolo" => "The level pulsing, in time with the song",
        "Room" => "A small, close space: body without a tail",
        "Chamber" => "A bright, dense room: vocals and snares",
        "Hall" => "A big, smooth hall: long and wide",
        "Plate" => "A metal plate: bright, dense, no room at all",
        "Ambience" => "Early reflections only: space without a wash",
        "Cathedral" => "Stone and height: a tail for days",
        "Shimmer" => "A tail that climbs an octave as it rings: ethereal pads",
        "Digital Delay" => "Clean repeats, exactly in time",
        "Tape Delay" => "Repeats that wobble and darken as they fade",
        "Analog Delay" => "Warm, murky repeats, each a little darker",
        "Ping-Pong Delay" => "Repeats bouncing left and right",
        "Dub Delay" => "Long, saturated feedback that swells and runs away",
        "Slapback" => "One quick repeat: the rockabilly double",
        "Filter" => "A resonant filter, swept by its own LFO or envelope",
        "EQ" => "Bands to cut and boost, over the synth's own spectrum",
        "Compressor" => "Levels it out: punch and sustain",
        "Multiband" => "Three bands, each compressed on its own",
        "Width" => "Wider or narrower, the lows kept mono",
        "Transient Shaper" => "More or less attack, more or less tail",
        "Gate" => "Silence under a threshold: chops and cuts",
        "Limiter" => "A ceiling it never goes over",
        "Pitch" => "Shifted up or down, fed back for cascades",
        "Tape Stop" => "Slows to a halt, and back up to speed",
        _ => "",
    }
}

/// The rack's effects by what they do, each by the name it's offered under.
pub const RACK_OFFERS: [(&str, &[&str]); 4] = [
    (
        "Drive",
        &[
            "Tape",
            "Tube",
            "Transformer",
            "Crunch",
            "Heavy",
            "Rectify",
            "Foldback",
            "Lo-Fi",
        ],
    ),
    (
        "Motion",
        &[
            "Chorus", "Ensemble", "Flanger", "Phaser", "Vibrato", "Tremolo",
        ],
    ),
    (
        "Space",
        &[
            "Room",
            "Chamber",
            "Hall",
            "Plate",
            "Ambience",
            "Cathedral",
            "Shimmer",
            "Digital Delay",
            "Tape Delay",
            "Analog Delay",
            "Ping-Pong Delay",
            "Dub Delay",
            "Slapback",
        ],
    ),
    (
        "Shape",
        &[
            "Filter",
            "EQ",
            "Compressor",
            "Multiband",
            "Width",
            "Transient Shaper",
            "Gate",
            "Limiter",
            "Pitch",
            "Tape Stop",
        ],
    ),
];

/// An effect for the rack, as offered under `name`.
pub fn rack_effect(name: &str) -> Option<Unit> {
    let drive = |style: SatStyle, drive_db: f32| {
        let mut s = Saturator::default();
        if let Some(b) = s.bands.first_mut() {
            b.style = style;
            b.drive_db = drive_db;
        }
        Unit::Sat(s)
    };
    let tool = |kind: Fx| fresh_tool(kind, false);
    Some(match name {
        "Tape" => drive(SatStyle::WarmTape, 6.0),
        "Tube" => drive(SatStyle::WarmTube, 8.0),
        "Transformer" => drive(SatStyle::Transformer, 6.0),
        "Crunch" => drive(SatStyle::Crunch, 10.0),
        "Heavy" => drive(SatStyle::Heavy, 12.0),
        "Rectify" => drive(SatStyle::Rectify, 8.0),
        "Foldback" => drive(SatStyle::Foldback, 10.0),
        "Lo-Fi" => drive(SatStyle::Lofi, 8.0),
        "Chorus" => Unit::Motion(MotionStyle::Chorus.motion()),
        "Ensemble" => Unit::Motion(MotionStyle::Ensemble.motion()),
        "Flanger" => Unit::Motion(MotionStyle::Flanger.motion()),
        "Phaser" => Unit::Motion(MotionStyle::Phaser.motion()),
        "Vibrato" => Unit::Motion(MotionStyle::Vibrato.motion()),
        "Tremolo" => Unit::Motion(MotionStyle::Tremolo.motion()),
        "Room" => Unit::Reverb(ReverbStyle::Room.reverb()),
        "Chamber" => Unit::Reverb(ReverbStyle::Chamber.reverb()),
        "Hall" => Unit::Reverb(ReverbStyle::Hall.reverb()),
        "Plate" => Unit::Reverb(ReverbStyle::Plate.reverb()),
        "Ambience" => Unit::Reverb(ReverbStyle::Ambience.reverb()),
        "Cathedral" => Unit::Reverb(ReverbStyle::Cathedral.reverb()),
        "Shimmer" => Unit::Reverb(ReverbStyle::Shimmer.reverb()),
        "Digital Delay" => Unit::Delay(DelayStyle::Digital.delay()),
        "Tape Delay" => Unit::Delay(DelayStyle::Tape.delay()),
        "Analog Delay" => Unit::Delay(DelayStyle::Analog.delay()),
        "Ping-Pong Delay" => Unit::Delay(DelayStyle::PingPong.delay()),
        "Dub Delay" => Unit::Delay(DelayStyle::Dub.delay()),
        "Slapback" => Unit::Delay(DelayStyle::Slapback.delay()),
        "Filter" => tool(Fx::Filter)?,
        "EQ" => Unit::Eq(Eq::default()),
        "Compressor" => Unit::Comp(Comp::default()),
        "Multiband" => tool(Fx::Multiband)?,
        "Width" => tool(Fx::Stereo)?,
        "Transient Shaper" => tool(Fx::Shaper)?,
        "Gate" => tool(Fx::Gate)?,
        "Limiter" => Unit::Limit(Limiter::default()),
        "Pitch" => tool(Fx::Pitch)?,
        "Tape Stop" => tool(Fx::TapeStop)?,
        _ => return None,
    })
}

/// What an effect in the rack is called: its style where it has one ("Hall Reverb", "Tube
/// Drive", "Ensemble").
pub fn rack_name(u: &Unit) -> String {
    match u {
        Unit::Reverb(r) => format!("{} Reverb", r.style.name()),
        Unit::Delay(d) => format!("{} Delay", d.style.name()),
        Unit::Motion(m) => m.style.name().to_string(),
        Unit::Sat(s) => format!(
            "{} Drive",
            s.bands.first().map_or("Tape", |b| b.style.name())
        ),
        Unit::Eq(_) => "EQ".into(),
        Unit::Comp(_) => "Compressor".into(),
        Unit::Limit(_) => "Limiter".into(),
        Unit::Filter(_) => "Filter".into(),
        Unit::Shaper(_) => "Transient Shaper".into(),
        Unit::Gate(_) => "Gate".into(),
        Unit::DeEsser(_) => "De-Esser".into(),
        Unit::Stereo(_) => "Width".into(),
        Unit::Multiband(_) => "Multiband".into(),
        Unit::Pitch(_) => "Pitch".into(),
        Unit::TapeStop(_) => "Tape Stop".into(),
        Unit::Tune(_) => "Tune".into(),
    }
}

impl Poly {
    /// An effect into the rack, last; false if the rack's full.
    pub fn add_fx(&mut self, u: Unit) -> bool {
        if self.fx.len() >= RACK {
            return false;
        }
        self.fx.push(u);
        true
    }

    /// The rack's effect at `k` taken out, with the routes riding it; those on the effects after
    /// it follow them down a place.
    pub fn remove_fx(&mut self, k: usize) {
        if k >= self.fx.len() {
            return;
        }
        self.fx.remove(k);
        let k = k as u8;
        self.routes
            .retain(|r| !matches!(r.dest, PolyDest::Rack(at, _) if at == k));
        for r in &mut self.routes {
            if let PolyDest::Rack(at, knob) = r.dest
                && at > k
            {
                r.dest = PolyDest::Rack(at - 1, knob);
            }
        }
    }

    /// The rack's effect at `from` moved to `to`, the routes riding each effect following it.
    pub fn move_fx(&mut self, from: usize, to: usize) {
        let n = self.fx.len();
        if from >= n || to >= n || from == to {
            return;
        }
        let u = self.fx.remove(from);
        self.fx.insert(to, u);
        for r in &mut self.routes {
            if let PolyDest::Rack(at, knob) = r.dest {
                r.dest = PolyDest::Rack(moved_to(at as usize, from, to) as u8, knob);
            }
        }
    }
}

/// Where the effect at `at` goes when the one at `from` moves to `to`.
fn moved_to(at: usize, from: usize, to: usize) -> usize {
    match at {
        a if a == from => to,
        a if from < to && a > from && a <= to => a - 1,
        a if to < from && a >= to && a < from => a + 1,
        a => a,
    }
}

impl Track {
    /// Its synth's rack effect at `k` taken out: the synth's routes to it, its automation and the
    /// track's modulators on it go with it; those on the effects after it follow them down.
    pub fn remove_rack_fx(&mut self, k: usize) {
        let Some(Instrument::Poly(p)) = &mut self.instrument else {
            return;
        };
        if k >= p.fx.len() {
            return;
        }
        p.remove_fx(k);
        self.rack_moved(|at| match at {
            a if a == k => None,
            a if a > k => Some(a - 1),
            a => Some(a),
        });
    }

    /// Its synth's rack effect at `from` moved to `to`, what rides each effect following it.
    pub fn move_rack_fx(&mut self, from: usize, to: usize) {
        let Some(Instrument::Poly(p)) = &mut self.instrument else {
            return;
        };
        let n = p.fx.len();
        if from >= n || to >= n || from == to {
            return;
        }
        p.move_fx(from, to);
        self.rack_moved(|at| Some(moved_to(at, from, to)));
    }

    /// The track's automation and modulators on its synth's rack, each effect where `place` says
    /// it went (None: gone).
    fn rack_moved(&mut self, place: impl Fn(usize) -> Option<usize>) {
        use crate::dsp::modulate::ModTarget;
        self.automation
            .rack
            .retain_mut(|a| match place(a.place as usize) {
                Some(now) => {
                    a.place = now as u8;
                    true
                }
                None => false,
            });
        for m in &mut self.modulators {
            m.routes.retain_mut(|r| {
                let ModTarget::Rack(k, knob) = r.target else {
                    return true;
                };
                match place(k as usize) {
                    Some(now) => {
                        r.target = ModTarget::Rack(now as u8, knob);
                        true
                    }
                    None => false,
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_on_offer_is_made_and_named() {
        for (_, names) in RACK_OFFERS {
            for name in names {
                let u = rack_effect(name).unwrap_or_else(|| panic!("{name}"));
                assert!(!rack_name(&u).is_empty());
            }
        }
        assert_eq!(rack_name(&rack_effect("Hall").unwrap()), "Hall Reverb");
        assert_eq!(rack_name(&rack_effect("Tube").unwrap()), "Warm Tube Drive");
        assert_eq!(rack_name(&rack_effect("Ensemble").unwrap()), "Ensemble");
    }

    #[test]
    fn routes_follow_their_effect_as_the_rack_changes() {
        let mut p = Poly::default();
        for name in ["Tube", "Chorus", "Hall"] {
            assert!(p.add_fx(rack_effect(name).unwrap()));
        }
        let route = |k: u8| PolyRoute {
            source: PolySource::Lfo(0),
            dest: PolyDest::Rack(k, 0),
            depth: 0.2,
            by: None,
        };
        p.routes = vec![route(0), route(1), route(2)];
        // The hall to the front: its route goes with it, the others a place along.
        p.move_fx(2, 0);
        assert_eq!(rack_name(&p.fx[0]), "Hall Reverb");
        let at: Vec<PolyDest> = p.routes.iter().map(|r| r.dest).collect();
        assert_eq!(
            at,
            [
                PolyDest::Rack(1, 0),
                PolyDest::Rack(2, 0),
                PolyDest::Rack(0, 0)
            ]
        );
        // The drive out (now second): its route goes, the chorus's comes down a place.
        p.remove_fx(1);
        let at: Vec<PolyDest> = p.routes.iter().map(|r| r.dest).collect();
        assert_eq!(at, [PolyDest::Rack(1, 0), PolyDest::Rack(0, 0)]);
        // Full at eight.
        while p.add_fx(rack_effect("EQ").unwrap()) {}
        assert_eq!(p.fx.len(), RACK);
    }

    #[test]
    fn a_tracks_automation_and_modulators_follow_its_synths_rack() {
        use crate::dsp::modulate::{ModTarget, Modulator};
        let mut p = Poly::default();
        for name in ["Tube", "Chorus", "Hall"] {
            p.add_fx(rack_effect(name).unwrap());
        }
        let mut t = Track {
            instrument: Some(Instrument::Poly(Box::new(p))),
            ..Default::default()
        };
        t.automation
            .points_mut(Param::Rack(2, 0))
            .push(AutoPoint::new(0, 0.5));
        t.automation
            .points_mut(Param::Rack(0, 1))
            .push(AutoPoint::new(0, 0.5));
        let mut lfo = Modulator {
            kind: crate::dsp::modulate::ModKind::lfo(),
            routes: Vec::new(),
        };
        lfo.routes.push(crate::dsp::modulate::ModRoute {
            target: ModTarget::Rack(2, 3),
            depth: 0.2,
        });
        t.modulators.push(lfo);
        // The hall to the front, then the drive (now second) out.
        t.move_rack_fx(2, 0);
        t.remove_rack_fx(1);
        let auto: Vec<Param> = t.automation.params();
        assert_eq!(auto, [Param::Rack(0, 0)]);
        assert_eq!(t.modulators[0].routes[0].target, ModTarget::Rack(0, 3));
    }
}
