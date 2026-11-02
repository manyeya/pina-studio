//! The Browser's categories: their names, words, colours and icons.

use super::*;

/// What a device is for: effects by what they do to a sound, instruments by what they play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cat {
    Eq,
    Dynamics,
    Reverb,
    Delay,
    Distortion,
    Modulation,
    Filter,
    Pitch,
    Space,
    Utility,
    MultiFx,
    OtherFx,
    Synth,
    Sampler,
    Drums,
    Keys,
    Orchestral,
    OtherInstrument,
}

impl Cat {
    pub fn name(self) -> &'static str {
        match self {
            Cat::Eq => "EQ",
            Cat::Dynamics => "Dynamics",
            Cat::Reverb => "Reverb",
            Cat::Delay => "Delay",
            Cat::Distortion => "Distortion",
            Cat::Modulation => "Modulation",
            Cat::Filter => "Filter",
            Cat::Pitch => "Pitch",
            Cat::Space => "Stereo & space",
            Cat::Utility => "Utility",
            Cat::MultiFx => "Multi FX",
            Cat::OtherFx => "Effect",
            Cat::Synth => "Synths",
            Cat::Sampler => "Samplers",
            Cat::Drums => "Drums",
            Cat::Keys => "Keys",
            Cat::Orchestral => "Orchestral",
            Cat::OtherInstrument => "Instrument",
        }
    }

    pub fn instrument(self) -> bool {
        matches!(
            self,
            Cat::Synth
                | Cat::Sampler
                | Cat::Drums
                | Cat::Keys
                | Cat::Orchestral
                | Cat::OtherInstrument
        )
    }

    /// Words that mean it, in categories, names and searches: "echo" finds a delay, "space" a
    /// reverb, "glue" a compressor.
    pub(crate) fn words(self) -> &'static [&'static str] {
        match self {
            Cat::Eq => &[
                "eq",
                "equalizer",
                "equaliser",
                "tone",
                "shelf",
                "graphic",
                "parametric",
            ],
            Cat::Dynamics => &[
                "dynamics",
                "compressor",
                "compression",
                "comp",
                "limiter",
                "gate",
                "expander",
                "transient",
                "glue",
                "maximizer",
                "de-esser",
                "deesser",
            ],
            Cat::Reverb => &[
                "reverb", "verb", "room", "hall", "plate", "chamber", "space", "ambience", "early",
            ],
            Cat::Delay => &["delay", "echo", "tape echo", "repeat", "ping"],
            Cat::Distortion => &[
                "distortion",
                "saturation",
                "saturator",
                "overdrive",
                "drive",
                "fuzz",
                "amp",
                "bitcrush",
                "crusher",
                "lofi",
                "lo-fi",
                "warmth",
                "grit",
                "tape",
            ],
            Cat::Modulation => &[
                "modulation",
                "chorus",
                "flanger",
                "phaser",
                "tremolo",
                "vibrato",
                "rotary",
                "ensemble",
                "wobble",
            ],
            Cat::Filter => &[
                "filter",
                "lowpass",
                "highpass",
                "bandpass",
                "low pass",
                "high pass",
                "cutoff",
            ],
            Cat::Pitch => &[
                "pitch",
                "tuning",
                "tuner",
                "harmonizer",
                "pitch-shifter",
                "shifter",
                "autotune",
                "octave",
            ],
            Cat::Space => &[
                "stereo", "spatial", "surround", "imager", "panner", "width", "wide", "binaural",
            ],
            Cat::Utility => &[
                "utility",
                "tools",
                "analyzer",
                "analyser",
                "meter",
                "gain",
                "mastering",
                "restoration",
                "sound isolation",
                "network",
                "isolation",
            ],
            Cat::MultiFx => &[
                "multi-effects",
                "multi-fx",
                "multifx",
                "multi fx",
                "fx rack",
                "suite",
            ],
            Cat::OtherFx => &["fx", "effect", "audio-effect"],
            Cat::Synth => &[
                "synth",
                "synthesizer",
                "synthesiser",
                "analog",
                "wavetable",
                "fm",
                "modular",
                "poly",
            ],
            Cat::Sampler => &["sampler", "rompler", "sample"],
            Cat::Drums => &["drum", "drums", "drum-machine", "beat", "percussion", "kit"],
            Cat::Keys => &[
                "piano",
                "keys",
                "organ",
                "electric piano",
                "rhodes",
                "keyboard",
            ],
            Cat::Orchestral => &[
                "strings",
                "orchestra",
                "orchestral",
                "ensemble strings",
                "brass",
                "choir",
            ],
            Cat::OtherInstrument => &["instrument", "generator"],
        }
    }

    /// Its colour in the Browser.
    pub fn color(self) -> u32 {
        let h = match self {
            Cat::Eq => 165.0,
            Cat::Dynamics => 42.0,
            Cat::Reverb => 232.0,
            Cat::Delay => 202.0,
            Cat::Distortion => 8.0,
            Cat::Modulation => 268.0,
            Cat::Filter => 186.0,
            Cat::Pitch => 318.0,
            Cat::Space => 92.0,
            Cat::Utility | Cat::OtherFx => 220.0,
            Cat::MultiFx => 28.0,
            Cat::Synth => 202.0,
            Cat::Sampler => 30.0,
            Cat::Drums => 6.0,
            Cat::Keys => 163.0,
            Cat::Orchestral => 343.0,
            Cat::OtherInstrument => 256.0,
        };
        let s = if matches!(self, Cat::Utility | Cat::OtherFx) {
            0.22
        } else {
            0.78
        };
        hsl(h, s, 0.66)
    }

    /// Its icon: an instrument's as a track's, an effect's drawn for what it does.
    pub fn glyph(self, size: f32) -> Svg {
        let track = match self {
            Cat::Synth => Some("synth"),
            Cat::Sampler => Some("sampler"),
            Cat::Drums => Some("drums"),
            Cat::Keys => Some("keys"),
            Cat::Orchestral => Some("strings"),
            Cat::OtherInstrument => Some("midi"),
            Cat::OtherFx | Cat::MultiFx => Some("fx"),
            _ => None,
        };
        if let Some(id) = track {
            return glyph(id, size);
        }
        let id = match self {
            Cat::Eq => "eq",
            Cat::Dynamics => "dynamics",
            Cat::Reverb => "reverb",
            Cat::Delay => "delay",
            Cat::Distortion => "distortion",
            Cat::Modulation => "modulation",
            Cat::Filter => "filter",
            Cat::Pitch => "pitch",
            Cat::Space => "space",
            _ => "utility",
        };
        drawn(id, size)
    }
}

/// What each built-in's icon is, by its device's key: those that aren't a kind's own, so that no
/// two of them look alike.
pub(crate) fn own_icon(key: &str) -> Option<&'static str> {
    Some(match key.strip_prefix("own:")? {
        "poly" => "poly",
        "limit" => "limit",
        "shaper" => "shaper",
        "gate" => "gate",
        "de_esser" => "de_esser",
        "multiband" => "multiband",
        "tune" => "tune",
        "tape_stop" => "tape_stop",
        _ => return None,
    })
}

/// A drawn icon, by name.
pub(crate) fn drawn(id: &str, size: f32) -> Svg {
    static CACHE: OnceLock<HashMap<&'static str, Vec<u8>>> = OnceLock::new();
    let shapes = CACHE.get_or_init(|| {
            [
                ("eq", r##"<path d="M3 16c2.6 0 3.4-8 6.5-8S12.6 17 15.6 17 18.4 12 21 12"/><path d="M3 20.5h18"/>"##),
                ("dynamics", r##"<path d="M3.5 3.5v17h17"/><path d="M3.5 20.5 12 12l8.5-4"/><path d="M12 12h.01"/>"##),
                ("reverb", r##"<path d="M2.5 19a9.5 9.5 0 0 1 19 0M6 19a6 6 0 0 1 12 0M9.5 19a2.5 2.5 0 0 1 5 0"/>"##),
                ("delay", r##"<circle cx="5" cy="12" r="2.6"/><circle cx="12" cy="12" r="2"/><circle cx="18" cy="12" r="1.4"/><path d="M21.5 12h.01"/>"##),
                ("distortion", r##"<path d="M2.5 12c1.2-4 2-5.5 3.5-5.5h3c1.3 0 1.6 11 3 11h3c1.3 0 1.7-5.5 3.2-5.5h3.3"/>"##),
                ("modulation", r##"<path d="M2.5 9c3-4.5 6.3 4.5 9.5 0s6.5-4.5 9.5 0M2.5 15c3 4.5 6.3-4.5 9.5 0s6.5 4.5 9.5 0"/>"##),
                ("filter", r##"<path d="M2.5 8h9c3.5 0 4.5 2.6 5.6 6.2L18.8 20"/><path d="M2.5 20.5h19"/>"##),
                ("pitch", r##"<path d="M7.5 20V4M3.5 8l4-4 4 4M16.5 4v16M12.5 16l4 4 4-4"/>"##),
                ("space", r##"<circle cx="8.8" cy="12" r="5.8"/><circle cx="15.2" cy="12" r="5.8"/>"##),
                ("utility", r##"<path d="M5 3.5v17M12 3.5v17M19 3.5v17"/><circle cx="5" cy="14" r="2"/><circle cx="12" cy="8" r="2"/><circle cx="19" cy="16" r="2"/>"##),
                ("poly", r##"<path d="M2.5 16c2.4-6.5 4.4-6.5 6.8 0s4.4 6.5 6.8 0"/><path d="M7.5 9c2.4-6.5 4.4-6.5 6.8 0s4.4 6.5 6.8 0"/>"##),
                ("limit", r##"<path d="M3 4.5h18"/><path d="M3 20c3-1 3.6-11 5.6-15.5h6.8C17.4 9 18 19 21 20"/>"##),
                ("shaper", r##"<path d="M2.5 20h3.2l2-16.5 3 11 2.6-3.2L21.5 20"/>"##),
                ("gate", r##"<path d="M2.5 20h5.5V5.5h8V20h5.5"/><path d="M12 5.5V20"/>"##),
                ("de_esser", r##"<path d="M16.5 7.2c-1.1-2.5-8-3.2-8.8 0-.9 3.7 8.8 4.2 8.8 8.2 0 3.6-7.6 3.7-9.3 1"/><path d="M3 12h18"/>"##),
                ("multiband", r##"<path d="M3 20.5v-7h5v7M9.5 20.5V6h5v14.5M16 20.5v-10h5v10"/>"##),
                ("tune", r##"<path d="M9 18V5.5l11-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="17" cy="16.5" r="3"/>"##),
                ("tape_stop", r##"<circle cx="7" cy="10" r="4.2"/><circle cx="17" cy="10" r="4.2"/><path d="M7 14.2h10"/><path d="M7 10h.01M17 10h.01"/><path d="M3 20.5h18"/>"##),
            ]
            .into_iter()
            .map(|(id, shape)| {
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">{shape}</svg>"##
                );
                (id, svg.into_bytes())
            })
            .collect()
        });
    svg().data(&shapes[id][..]).size(px(size)).flex_none()
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn no_two_built_ins_of_a_kind_share_an_icon() {
        use std::collections::HashMap;
        // What each one is drawn with: its own, or its kind's.
        let mut seen: HashMap<(bool, String), String> = HashMap::new();
        for d in crate::sounds::catalog::devices::devices(&[]) {
            if d.own.is_none() {
                continue;
            }
            let icon = match own_icon(&d.key) {
                Some(id) => id.to_string(),
                None => format!("{:?}", d.cat),
            };
            let same = seen.insert((d.instrument(), icon.clone()), d.name.clone());
            // Only the sine synth and the sampler's family keep their kind's on purpose.
            assert!(
                same.is_none() || d.instrument(),
                "{} and {} look the same",
                d.name,
                same.unwrap_or_default()
            );
        }
    }
}
