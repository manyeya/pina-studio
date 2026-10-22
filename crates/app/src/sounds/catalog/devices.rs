//! Every device that can go on a track: Pina Studio's own and every plugin installed, each categorized and ranked.

use super::*;

/// The formats, best first: CLAP, then VST3, Audio Units, VST2.
pub(crate) fn rank(f: Format) -> u8 {
    match f {
        Format::Clap => 0,
        Format::Vst3 => 1,
        Format::Au => 2,
        Format::Vst2 => 3,
    }
}

/// An Audio Unit's type and subtype codes, from its uid ("aufx dely appl").
pub(crate) fn au_codes(uid: Option<&str>) -> (String, String) {
    let mut parts = uid.unwrap_or_default().split(' ');
    (
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
    )
}

/// What a plugin is for: from what it says (its categories, or an Audio Unit's codes), else its
/// name.
pub fn categorize(p: &PluginInfo) -> Cat {
    let (kind, sub) = au_codes(p.uid.as_deref());
    if p.format == Format::Au {
        let by_code = match sub.as_str() {
            "dely" | "sdly" => Some(Cat::Delay),
            "rvb2" | "mrev" => Some(Cat::Reverb),
            "pmeq" | "nbeq" | "greq" | "hshf" | "lshf" => Some(Cat::Eq),
            "lpas" | "hpas" | "bpas" | "filt" => Some(Cat::Filter),
            "dcmp" | "mcmp" | "lmtr" => Some(Cat::Dynamics),
            "dist" => Some(Cat::Distortion),
            "tmpt" | "nutp" => Some(Cat::Pitch),
            "raac" | "nsnd" | "vois" | "rogr" => Some(Cat::Utility),
            "samp" => Some(Cat::Sampler),
            _ => None,
        };
        if let Some(c) = by_code {
            return c;
        }
        let _ = kind;
    }
    let said = p.categories.join(" ").to_lowercase();
    let name = format!("{} {}", p.class, p.vendor).to_lowercase();
    let has = |text: &str, c: Cat| {
        c.words().iter().any(|w| {
            text.split(|ch: char| !ch.is_alphanumeric() && ch != '-')
                .any(|t| t == *w)
                || (w.contains(' ') && text.contains(w))
        })
    };
    let order: &[Cat] = match p.instrument {
        true => &[
            Cat::Drums,
            Cat::Sampler,
            Cat::Keys,
            Cat::Orchestral,
            Cat::Synth,
        ],
        false => &[
            Cat::MultiFx,
            Cat::Reverb,
            Cat::Delay,
            Cat::Eq,
            Cat::Dynamics,
            Cat::Distortion,
            Cat::Modulation,
            Cat::Filter,
            Cat::Pitch,
            Cat::Space,
            Cat::Utility,
        ],
    };
    order
        .iter()
        .copied()
        .find(|c| has(&said, *c))
        .or_else(|| order.iter().copied().find(|c| has(&name, *c)))
        .unwrap_or(if p.instrument {
            Cat::Synth
        } else {
            Cat::OtherFx
        })
}

/// Every device: Pina Studio's own, then each plugin once, its formats together.
pub fn devices(plugins: &[PluginInfo]) -> Vec<Device> {
    let own = |key: &str, name: &str, cat: Cat, own: Own, about: &str| Device {
        key: format!("own:{key}"),
        name: name.into(),
        vendor: "Pina Studio".into(),
        cat,
        own: Some(own),
        formats: Vec::new(),
        hay: format!(
            "{name} pina studio built in {about} {}",
            cat.words().join(" ")
        )
        .to_lowercase(),
    };
    let mut out = vec![
        own("synth", "Sine Synth", Cat::Synth, Own::Synth, "sine simple"),
        own(
            "drums",
            "Drum Kit",
            Cat::Drums,
            Own::Drums,
            "808 kick snare hat",
        ),
        own(
            "sampler",
            "Sampler",
            Cat::Sampler,
            Own::Sampler,
            "samples zones pads",
        ),
        own("eq", "Channel EQ", Cat::Eq, Own::Eq, "low cut bell shelf"),
        own(
            "comp",
            "Compressor",
            Cat::Dynamics,
            Own::Comp,
            "soft knee glue",
        ),
        own(
            "sat",
            "Saturator",
            Cat::Distortion,
            Own::Sat,
            "drive tone warmth tape",
        ),
    ];
    let mut groups: Vec<(String, Vec<PluginInfo>)> = Vec::new();
    for p in plugins {
        let key = format!(
            "plugin:{}/{}",
            p.vendor.to_lowercase(),
            p.class.to_lowercase()
        );
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, g)) => g.push(p.clone()),
            None => groups.push((key, vec![p.clone()])),
        }
    }
    for (key, mut formats) in groups {
        formats.sort_by_key(|p| rank(p.format));
        let first = &formats[0];
        let cat = categorize(first);
        let hay = format!(
            "{} {} {} {} {}",
            first.class,
            first.vendor,
            cat.name(),
            cat.words().join(" "),
            first.categories.join(" ")
        )
        .to_lowercase();
        out.push(Device {
            key,
            name: first.class.clone(),
            vendor: first.vendor.clone(),
            cat,
            own: None,
            formats,
            hay,
        });
    }
    out
}

/// How well a device matches a search: every word somewhere, its name counting most. None if a
/// word is missing.
pub fn score(d: &Device, query: &str) -> Option<u32> {
    let name = d.name.to_lowercase();
    let mut total = 0;
    for w in query.split_whitespace() {
        total += match () {
            _ if name.starts_with(w) => 30,
            _ if name.split_whitespace().any(|t| t.starts_with(w)) => 20,
            _ if name.contains(w) => 12,
            _ if d.cat.words().iter().any(|c| c.starts_with(w)) => 10,
            _ if d.hay.contains(w) => 5,
            _ => return None,
        };
    }
    Some(total)
}

/// The instrument a device is.
pub(crate) fn instrument_of(d: &Device) -> Option<Instrument> {
    match d.own {
        Some(Own::Synth) => Some(Instrument::Synth),
        Some(Own::Drums) => Some(Instrument::Drums),
        Some(Own::Sampler) => Some(Instrument::Sampler(Default::default())),
        Some(_) => None,
        None => d
            .formats
            .first()
            .map(|p| Instrument::Plugin(p.plugin_ref())),
    }
}
