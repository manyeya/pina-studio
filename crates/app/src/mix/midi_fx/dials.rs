//! The MIDI effects' numbers as dials: each one's range, how it reads, and where it sits.

use pina_engine::song::project::{ExprShape, MidiFx};

/// A number of a MIDI effect's, turned with a dial.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dial {
    ArpOctaves,
    ArpGate,
    Semitones,
    VelScale,
    VelAdd,
    VelMin,
    VelMax,
    RepCount,
    RepDecay,
    RepTranspose,
    RandPitch,
    RandVelocity,
    RandChance,
    ModFrom,
    ModTo,
    ModScale,
    MpeBend,
    MpeRound,
    SlideCurve,
    SlideLow,
    SlideHigh,
    PressCurve,
    PressLow,
    PressHigh,
}

impl Dial {
    /// Its lowest and highest, and the step it moves in.
    pub fn range(self) -> (f32, f32, f32) {
        match self {
            Dial::ArpOctaves => (1.0, 4.0, 1.0),
            Dial::ArpGate | Dial::RepDecay => (0.1, 1.0, 0.01),
            Dial::Semitones => (-48.0, 48.0, 1.0),
            Dial::VelScale | Dial::ModScale => (0.0, 200.0, 1.0),
            Dial::VelAdd => (-127.0, 127.0, 1.0),
            Dial::VelMin | Dial::VelMax => (1.0, 127.0, 1.0),
            Dial::RepCount => (1.0, 16.0, 1.0),
            Dial::RepTranspose => (-24.0, 24.0, 1.0),
            Dial::RandPitch => (0.0, 24.0, 1.0),
            Dial::RandVelocity => (0.0, 127.0, 1.0),
            Dial::RandChance | Dial::MpeRound => (0.0, 100.0, 1.0),
            Dial::ModFrom | Dial::ModTo => (0.0, 127.0, 1.0),
            Dial::MpeBend => (0.0, 400.0, 5.0),
            Dial::SlideCurve | Dial::PressCurve => (-100.0, 100.0, 1.0),
            Dial::SlideLow | Dial::SlideHigh | Dial::PressLow | Dial::PressHigh => {
                (0.0, 100.0, 1.0)
            }
        }
    }

    /// Either way of a middle: its arc grows from the top.
    pub fn bipolar(self) -> bool {
        matches!(
            self,
            Dial::Semitones
                | Dial::VelAdd
                | Dial::RepTranspose
                | Dial::SlideCurve
                | Dial::PressCurve
        )
    }

    pub fn name(self) -> &'static str {
        match self {
            Dial::ArpOctaves => "Octaves",
            Dial::ArpGate => "Gate",
            Dial::Semitones => "Semitones",
            Dial::VelScale => "Scale",
            Dial::VelAdd => "Add",
            Dial::VelMin => "Lowest",
            Dial::VelMax => "Highest",
            Dial::RepCount => "Repeats",
            Dial::RepDecay => "Each",
            Dial::RepTranspose => "Moves",
            Dial::RandPitch => "Pitch",
            Dial::RandVelocity => "Velocity",
            Dial::RandChance => "Chance",
            Dial::ModFrom => "From",
            Dial::ModTo => "To",
            Dial::ModScale => "Scale",
            Dial::MpeBend => "Bend",
            Dial::MpeRound => "Round",
            Dial::SlideCurve => "Slide curve",
            Dial::SlideLow => "Slide low",
            Dial::SlideHigh => "Slide high",
            Dial::PressCurve => "Press curve",
            Dial::PressLow => "Press low",
            Dial::PressHigh => "Press high",
        }
    }

    /// Its value in the effect, if it's this effect's.
    pub fn get(self, fx: &MidiFx) -> Option<f32> {
        let shape = |s: &ExprShape, part: u8| match part {
            0 => s.curve as f32,
            1 => s.low as f32,
            _ => s.high as f32,
        };
        Some(match (self, fx) {
            (Dial::ArpOctaves, MidiFx::Arpeggiator { octaves, .. }) => *octaves as f32,
            (Dial::ArpGate, MidiFx::Arpeggiator { gate, .. }) => *gate,
            (Dial::Semitones, MidiFx::Transposer { semitones }) => *semitones as f32,
            (Dial::VelScale, MidiFx::Velocity { scale, .. }) => *scale as f32,
            (Dial::VelAdd, MidiFx::Velocity { add, .. }) => *add as f32,
            (Dial::VelMin, MidiFx::Velocity { min, .. }) => *min as f32,
            (Dial::VelMax, MidiFx::Velocity { max, .. }) => *max as f32,
            (Dial::RepCount, MidiFx::NoteRepeater { count, .. }) => *count as f32,
            (Dial::RepDecay, MidiFx::NoteRepeater { decay, .. }) => *decay,
            (Dial::RepTranspose, MidiFx::NoteRepeater { transpose, .. }) => *transpose as f32,
            (Dial::RandPitch, MidiFx::Randomizer { pitch, .. }) => *pitch as f32,
            (Dial::RandVelocity, MidiFx::Randomizer { velocity, .. }) => *velocity as f32,
            (Dial::RandChance, MidiFx::Randomizer { chance, .. }) => *chance as f32,
            (Dial::ModFrom, MidiFx::Modifier { from, .. }) => *from as f32,
            (Dial::ModTo, MidiFx::Modifier { to, .. }) => *to as f32,
            (Dial::ModScale, MidiFx::Modifier { scale, .. }) => *scale as f32,
            (Dial::MpeBend, MidiFx::MpeShaper { bend, .. }) => *bend as f32,
            (Dial::MpeRound, MidiFx::MpeShaper { round, .. }) => *round as f32,
            (Dial::SlideCurve, MidiFx::MpeShaper { slide, .. }) => shape(slide, 0),
            (Dial::SlideLow, MidiFx::MpeShaper { slide, .. }) => shape(slide, 1),
            (Dial::SlideHigh, MidiFx::MpeShaper { slide, .. }) => shape(slide, 2),
            (Dial::PressCurve, MidiFx::MpeShaper { press, .. }) => shape(press, 0),
            (Dial::PressLow, MidiFx::MpeShaper { press, .. }) => shape(press, 1),
            (Dial::PressHigh, MidiFx::MpeShaper { press, .. }) => shape(press, 2),
            _ => return None,
        })
    }

    /// Sets it, on its step and in its range (a lowest no higher than its highest).
    pub fn set(self, fx: &mut MidiFx, v: f32) {
        let (lo, hi, step) = self.range();
        let v = ((v / step).round() * step).clamp(lo, hi);
        let shape = |s: &mut ExprShape, part: u8| match part {
            0 => s.curve = v as i8,
            1 => s.low = (v as u8).min(s.high),
            _ => s.high = (v as u8).max(s.low),
        };
        match (self, fx) {
            (Dial::ArpOctaves, MidiFx::Arpeggiator { octaves, .. }) => *octaves = v as u8,
            (Dial::ArpGate, MidiFx::Arpeggiator { gate, .. }) => *gate = v,
            (Dial::Semitones, MidiFx::Transposer { semitones }) => *semitones = v as i8,
            (Dial::VelScale, MidiFx::Velocity { scale, .. }) => *scale = v as u8,
            (Dial::VelAdd, MidiFx::Velocity { add, .. }) => *add = v as i8,
            (Dial::VelMin, MidiFx::Velocity { min, max, .. }) => *min = (v as u8).min(*max),
            (Dial::VelMax, MidiFx::Velocity { min, max, .. }) => *max = (v as u8).max(*min),
            (Dial::RepCount, MidiFx::NoteRepeater { count, .. }) => *count = v as u8,
            (Dial::RepDecay, MidiFx::NoteRepeater { decay, .. }) => *decay = v,
            (Dial::RepTranspose, MidiFx::NoteRepeater { transpose, .. }) => *transpose = v as i8,
            (Dial::RandPitch, MidiFx::Randomizer { pitch, .. }) => *pitch = v as u8,
            (Dial::RandVelocity, MidiFx::Randomizer { velocity, .. }) => *velocity = v as u8,
            (Dial::RandChance, MidiFx::Randomizer { chance, .. }) => *chance = v as u8,
            (Dial::ModFrom, MidiFx::Modifier { from, .. }) => *from = v as u8,
            (Dial::ModTo, MidiFx::Modifier { to, .. }) => *to = v as u8,
            (Dial::ModScale, MidiFx::Modifier { scale, .. }) => *scale = v as u8,
            (Dial::MpeBend, MidiFx::MpeShaper { bend, .. }) => *bend = v as u16,
            (Dial::MpeRound, MidiFx::MpeShaper { round, .. }) => *round = v as u8,
            (Dial::SlideCurve, MidiFx::MpeShaper { slide, .. }) => shape(slide, 0),
            (Dial::SlideLow, MidiFx::MpeShaper { slide, .. }) => shape(slide, 1),
            (Dial::SlideHigh, MidiFx::MpeShaper { slide, .. }) => shape(slide, 2),
            (Dial::PressCurve, MidiFx::MpeShaper { press, .. }) => shape(press, 0),
            (Dial::PressLow, MidiFx::MpeShaper { press, .. }) => shape(press, 1),
            (Dial::PressHigh, MidiFx::MpeShaper { press, .. }) => shape(press, 2),
            _ => {}
        }
    }

    /// Where it sits on its dial, 0 to 1.
    pub fn pos(self, fx: &MidiFx) -> f32 {
        let (lo, hi, _) = self.range();
        self.get(fx).map_or(0.0, |v| (v - lo) / (hi - lo))
    }

    /// Set from a dial's place.
    pub fn set_pos(self, fx: &mut MidiFx, pos: f32) {
        let (lo, hi, _) = self.range();
        self.set(fx, lo + pos.clamp(0.0, 1.0) * (hi - lo));
    }

    /// How it reads under its dial: the number, a space, the unit.
    pub fn text(self, fx: &MidiFx) -> String {
        let Some(v) = self.get(fx) else {
            return String::new();
        };
        let signed = |v: f32| match v {
            v if v > 0.0 => format!("+{v:.0}"),
            v if v < 0.0 => format!("−{:.0}", -v),
            _ => "0".to_string(),
        };
        match self {
            Dial::ArpOctaves => format!("{v:.0}"),
            Dial::ArpGate | Dial::RepDecay => format!("{:.0} %", v * 100.0),
            Dial::Semitones | Dial::RepTranspose => format!("{} st", signed(v)),
            Dial::VelAdd => signed(v),
            Dial::VelScale | Dial::ModScale | Dial::RandChance | Dial::MpeBend => {
                format!("{v:.0} %")
            }
            Dial::MpeRound
            | Dial::SlideLow
            | Dial::SlideHigh
            | Dial::PressLow
            | Dial::PressHigh => format!("{v:.0} %"),
            Dial::RandPitch => format!("±{v:.0} st"),
            Dial::RandVelocity => format!("±{v:.0}"),
            Dial::VelMin | Dial::VelMax | Dial::RepCount => format!("{v:.0}"),
            Dial::ModFrom => format!("CC {v:.0}"),
            Dial::ModTo if v == 0.0 => "Off".to_string(),
            Dial::ModTo => format!("CC {v:.0}"),
            Dial::SlideCurve | Dial::PressCurve => match v {
                v if v > 0.0 => format!("{v:.0} early"),
                v if v < 0.0 => format!("{:.0} late", -v),
                _ => "Even".to_string(),
            },
        }
    }

    /// The dials an effect shows, in order.
    pub fn of(fx: &MidiFx) -> &'static [Dial] {
        match fx {
            MidiFx::Arpeggiator { .. } => &[Dial::ArpOctaves, Dial::ArpGate],
            MidiFx::ChordTrigger { .. } | MidiFx::ScaleQuantizer { .. } | MidiFx::Plugin { .. } => {
                &[]
            }
            MidiFx::Transposer { .. } => &[Dial::Semitones],
            MidiFx::Velocity { .. } => &[Dial::VelScale, Dial::VelAdd, Dial::VelMin, Dial::VelMax],
            MidiFx::NoteRepeater { .. } => &[Dial::RepCount, Dial::RepDecay, Dial::RepTranspose],
            MidiFx::Randomizer { .. } => &[Dial::RandPitch, Dial::RandVelocity, Dial::RandChance],
            MidiFx::Modifier { .. } => &[Dial::ModFrom, Dial::ModTo, Dial::ModScale],
            MidiFx::MpeShaper { .. } => &[
                Dial::MpeBend,
                Dial::MpeRound,
                Dial::SlideCurve,
                Dial::SlideLow,
                Dial::SlideHigh,
                Dial::PressCurve,
                Dial::PressLow,
                Dial::PressHigh,
            ],
        }
    }
}

/// What each effect does, in a line.
pub fn about(fx: &MidiFx) -> &'static str {
    match fx {
        MidiFx::Arpeggiator { .. } => "Plays what's held one note at a time, in time",
        MidiFx::ChordTrigger { .. } => "Makes each note a chord",
        MidiFx::ScaleQuantizer { .. } => "Keeps every note in a key",
        MidiFx::Transposer { .. } => "Moves every note up or down",
        MidiFx::Velocity { .. } => "Makes notes harder, softer or more even",
        MidiFx::NoteRepeater { .. } => "Plays each note again, fading as it goes",
        MidiFx::Randomizer { .. } => "Varies pitch and velocity, and leaves notes out by chance",
        MidiFx::Modifier { .. } => "Sends one controller's moves as another's",
        MidiFx::MpeShaper { .. } => "Reshapes each note's bend, slide and pressure",
        MidiFx::Plugin { .. } => "A plugin's: what it plays goes on down the chain",
    }
}

/// A controller's usual name, for the common ones.
pub fn cc_name(cc: u8) -> &'static str {
    match cc {
        0 => "Off",
        1 => "Mod wheel",
        2 => "Breath",
        4 => "Foot",
        7 => "Volume",
        10 => "Pan",
        11 => "Expression",
        64 => "Sustain",
        71 => "Resonance",
        72 => "Release",
        73 => "Attack",
        74 => "Brightness",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dial_reads_sets_and_keeps_to_its_range() {
        for kind in MidiFx::KINDS {
            let mut fx = MidiFx::fresh(kind).unwrap();
            for &d in Dial::of(&fx) {
                let (lo, hi, _) = d.range();
                d.set_pos(&mut fx, 1.0);
                assert!(d.get(&fx).unwrap() <= hi, "{kind} {d:?}");
                d.set_pos(&mut fx, 0.0);
                assert!(d.get(&fx).unwrap() >= lo, "{kind} {d:?}");
                assert!(!d.text(&fx).is_empty());
            }
        }
        // A lowest stays under its highest.
        let mut v = MidiFx::fresh("Velocity Processor").unwrap();
        Dial::VelMax.set(&mut v, 40.0);
        Dial::VelMin.set(&mut v, 90.0);
        assert_eq!(
            (Dial::VelMin.get(&v), Dial::VelMax.get(&v)),
            (Some(40.0), Some(40.0))
        );
        assert_eq!(
            Dial::Semitones.text(&MidiFx::Transposer { semitones: -7 }),
            "−7 st"
        );
    }
}
