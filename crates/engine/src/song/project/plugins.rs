//! Plugins as a song holds them: which one, its parameters, its presets.

use super::*;

/// A plugin format.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Vst3,
    Clap,
    Vst2,
    Au,
}

impl Format {
    pub(crate) fn is_vst3(&self) -> bool {
        *self == Format::Vst3
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Vst3 => "VST3",
            Format::Clap => "CLAP",
            Format::Vst2 => "VST2",
            Format::Au => "Audio Unit",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct PluginRef {
    /// Its format: VST3 unless it says.
    #[serde(default, skip_serializing_if = "Format::is_vst3")]
    pub format: Format,
    /// The bundle: a `.vst3`, `.clap`, `.vst` or `.component` (an Audio Unit may have none).
    #[serde(default)]
    pub path: PathBuf,
    /// Its name, which picks it out of a bundle with more than one.
    pub class: String,
    /// What its format knows it by, where a name isn't enough: an Audio Unit's type, subtype
    /// and manufacturer ("aumu samp appl"), a CLAP plugin's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    /// Its settings, saved under `Plugin States/` in the project folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<PathBuf>,
    /// The preset Pina Studio last loaded into it: loaded over its settings, and undone like any edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<Preset>,
    /// Parameters set from Pina Studio (its Smart Controls, or an agent), by id, 0 to 1, in the order
    /// they were set: applied over its preset and settings, and undone like any edit.
    #[serde(default, skip_serializing_if = "Params::is_empty")]
    pub params: Params,
    /// For a plugin with a sidechain input: the track (by id) it hears there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidechain: Option<u64>,
}

impl PluginRef {
    /// Which plugin this is, settings aside.
    pub fn same_plugin(&self, other: &PluginRef) -> bool {
        self.format == other.format
            && self.path == other.path
            && self.class == other.class
            && self.uid == other.uid
    }
}

/// Something a plugin can load: one of its own programs, or a preset file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    #[serde(flatten)]
    pub source: PresetSource,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum PresetSource {
    /// A program the plugin has built in: its place in the plugin's list `list` (VST3 has
    /// several lists, the others one, 0).
    Program {
        #[serde(default)]
        list: i32,
        program: u32,
    },
    /// A preset file: `.vstpreset`, `.clap-preset` or whatever the plugin keeps, `.fxp`,
    /// `.aupreset`.
    File { file: PathBuf },
}

/// Parameter values by id, in the order they were set: some parameters (an effect's type, a
/// mode) reset others, so order matters. Written as a JSON object, in that order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Params(Vec<(u32, f64)>);

impl Params {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, id: &u32) -> Option<&f64> {
        self.0.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    }

    pub fn contains_key(&self, id: &u32) -> bool {
        self.get(id).is_some()
    }

    /// Sets a parameter, as the latest set. Returns what it was.
    pub fn insert(&mut self, id: u32, value: f64) -> Option<f64> {
        let was = self.remove(&id);
        self.0.push((id, value));
        was
    }

    pub fn remove(&mut self, id: &u32) -> Option<f64> {
        let i = self.0.iter().position(|(k, _)| k == id)?;
        Some(self.0.remove(i).1)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u32, &f64)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&u32, &mut f64)> {
        self.0.iter_mut().map(|(k, v)| (&*k, v))
    }
}

impl FromIterator<(u32, f64)> for Params {
    fn from_iter<I: IntoIterator<Item = (u32, f64)>>(iter: I) -> Self {
        let mut p = Params::default();
        for (k, v) in iter {
            p.insert(k, v);
        }
        p
    }
}

impl Serialize for Params {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(&k.to_string(), v)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Params {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = Params;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("parameter values by id")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut m: A) -> Result<Params, A::Error> {
                let mut p = Params::default();
                while let Some((k, v)) = m.next_entry::<String, f64>()? {
                    let id = k.parse().map_err(serde::de::Error::custom)?;
                    p.insert(id, v);
                }
                Ok(p)
            }
        }
        d.deserialize_map(Visit)
    }
}

impl<'a> IntoIterator for &'a Params {
    type Item = (&'a u32, &'a f64);
    type IntoIter =
        std::iter::Map<std::slice::Iter<'a, (u32, f64)>, fn(&'a (u32, f64)) -> (&'a u32, &'a f64)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(k, v)| (k, v))
    }
}
