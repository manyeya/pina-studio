//! The synth's sounds as a library: the factory's, those saved, and packs of them, made here or
//! brought in. Kept in Pina's folder as `Sounds/<pack>/<sound>.pinasound` (a sound not in a
//! pack straight under `Sounds`), each pack's name, maker, words and cover in its `pack.json`.
//! A pack goes out and comes in as one `.pinapack` file: every sound in it and its cover.

use std::path::{Path, PathBuf};

use pina_engine::song::project::{POLY_PATCHES, Poly, poly_patch};
use serde::{Deserialize, Serialize};

/// What a sound can be for, as the library and Save offer them.
pub(crate) const PARTS: [&str; 9] = [
    "Lead", "Bass", "Pad", "Keys", "Pluck", "FX", "Arp", "Chord", "Drone",
];

/// What a sound can sound like: the words the library narrows by, and search reads.
pub(crate) const CHARACTER: [&str; 12] = [
    "Warm",
    "Bright",
    "Dark",
    "Wide",
    "Evolving",
    "Aggressive",
    "Soft",
    "Glassy",
    "Analog",
    "Digital",
    "Lo-fi",
    "Mono",
];

/// The factory's sounds, as a pack is named.
pub(crate) const FACTORY: &str = "Factory";

/// A sound saved: what it is, and the synth itself.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct SavedSound {
    pub name: String,
    pub part: String,
    #[serde(default)]
    pub words: Vec<String>,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub by: String,
    /// When it was saved, in seconds since 1970.
    #[serde(default)]
    pub made: u64,
    pub poly: Poly,
}

/// A pack: its name, who made it, what it's for, its cover (a seed it's drawn from), and the
/// order its sounds are stepped through in.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub(crate) struct PackInfo {
    pub name: String,
    pub by: String,
    pub about: String,
    pub cover: u32,
    pub made: u64,
    /// Brought in from a file, rather than made here.
    pub imported: bool,
    pub order: Vec<String>,
}

/// A pack as one file, to share.
#[derive(Serialize, Deserialize)]
pub(crate) struct PackFile {
    pub pack: PackInfo,
    pub sounds: Vec<SavedSound>,
}

/// A sound in the library: the factory's or saved, by its key ("Name" for the factory's,
/// "Pack/Name" for one in a pack, "/Name" for one in none).
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub key: String,
    pub sound: SavedSound,
    /// Its pack's name: the factory's, one of the packs, or None (saved in none).
    pub pack: Option<String>,
}

impl Entry {
    pub fn factory(&self) -> bool {
        self.pack.as_deref() == Some(FACTORY)
    }
}

/// Every sound there is, and every pack.
#[derive(Default)]
pub(crate) struct Library {
    pub dir: PathBuf,
    pub sounds: Vec<Entry>,
    pub packs: Vec<PackInfo>,
}

/// Seconds since 1970, now.
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A name safe as a file's: no slashes, nothing hidden.
fn file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '-'
            } else {
                c
            }
        })
        .collect();
    clean.trim().trim_start_matches('.').to_string()
}

/// The words a factory sound's line says it has.
fn words_in(about: &str) -> Vec<String> {
    let about = about.to_lowercase();
    CHARACTER
        .iter()
        .filter(|w| about.contains(&w.to_lowercase()))
        .map(|w| w.to_string())
        .collect()
}

pub(crate) fn capital(part: &str) -> String {
    match part {
        "fx" => "FX".into(),
        p => {
            let mut c = p.chars();
            c.next()
                .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        }
    }
}

impl Library {
    /// The library kept in `dir`, read now.
    pub fn read(dir: &Path) -> Library {
        let mut lib = Library {
            dir: dir.to_path_buf(),
            ..Default::default()
        };
        for (name, part, about) in POLY_PATCHES {
            if let Some(poly) = poly_patch(name) {
                lib.sounds.push(Entry {
                    key: name.to_string(),
                    sound: SavedSound {
                        name: name.to_string(),
                        part: capital(part),
                        words: words_in(about),
                        about: about.to_string(),
                        by: "Pina".into(),
                        made: 0,
                        poly,
                    },
                    pack: Some(FACTORY.into()),
                });
            }
        }
        lib.read_sounds(dir, None);
        let Ok(dirs) = std::fs::read_dir(dir) else {
            return lib;
        };
        let mut packs: Vec<PathBuf> = dirs
            .flatten()
            .map(|e| e.path())
            // Hidden ones (what's been taken out) aren't packs.
            .filter(|p| {
                p.is_dir()
                    && !p
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            })
            .collect();
        packs.sort();
        for p in packs {
            let folder = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let mut info: PackInfo = std::fs::read(p.join("pack.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            info.name = folder.clone();
            lib.read_sounds(&p, Some(&folder));
            lib.packs.push(info);
        }
        lib
    }

    fn read_sounds(&mut self, dir: &Path, pack: Option<&str>) {
        let Ok(files) = std::fs::read_dir(dir) else {
            return;
        };
        let mut found: Vec<Entry> = files
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "pinasound"))
            .filter_map(|p| {
                let sound: SavedSound = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
                Some(Entry {
                    key: format!("{}/{}", pack.unwrap_or_default(), sound.name),
                    sound,
                    pack: pack.map(str::to_string),
                })
            })
            .collect();
        found.sort_by(|a, b| {
            a.sound
                .name
                .to_lowercase()
                .cmp(&b.sound.name.to_lowercase())
        });
        self.sounds.extend(found);
    }

    pub fn find(&self, key: &str) -> Option<&Entry> {
        self.sounds.iter().find(|e| e.key == key)
    }

    pub fn pack(&self, name: &str) -> Option<&PackInfo> {
        self.packs.iter().find(|p| p.name == name)
    }

    /// How many sounds a pack has.
    pub fn count(&self, pack: &str) -> usize {
        self.sounds
            .iter()
            .filter(|e| e.pack.as_deref() == Some(pack))
            .count()
    }

    fn folder(&self, pack: Option<&str>) -> PathBuf {
        match pack {
            Some(p) => self.dir.join(file_name(p)),
            None => self.dir.clone(),
        }
    }

    /// Saves `sound` into `pack` (None: in none), over one of its name there: its key.
    pub fn save(&mut self, sound: SavedSound, pack: Option<&str>) -> Result<String, String> {
        if pack == Some(FACTORY) {
            return Err("the factory's sounds stay as they are".into());
        }
        let dir = self.folder(pack);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(&sound).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.pinasound", file_name(&sound.name)));
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        let key = format!("{}/{}", pack.unwrap_or_default(), sound.name);
        self.sounds.retain(|e| e.key != key);
        self.sounds.push(Entry {
            key: key.clone(),
            sound,
            pack: pack.map(str::to_string),
        });
        Ok(key)
    }

    /// Takes a saved sound out: its file put aside in the library's hidden `.taken out`
    /// folder, not deleted, so it can be brought back.
    pub fn remove(&mut self, key: &str) -> Result<Entry, String> {
        let at = self
            .sounds
            .iter()
            .position(|e| e.key == key && !e.factory())
            .ok_or("only a saved sound can be taken out")?;
        let file = format!("{}.pinasound", file_name(&self.sounds[at].sound.name));
        let pack = self.sounds[at].pack.clone();
        let from = self.folder(pack.as_deref()).join(&file);
        let aside = self
            .dir
            .join(".taken out")
            .join(file_name(pack.as_deref().unwrap_or("No pack")));
        std::fs::create_dir_all(&aside).map_err(|e| e.to_string())?;
        std::fs::rename(&from, aside.join(&file)).map_err(|e| e.to_string())?;
        Ok(self.sounds.remove(at))
    }

    /// A sound taken out, back where it was.
    pub fn bring_back(&mut self, e: Entry) -> Result<String, String> {
        self.save(e.sound, e.pack.as_deref())
    }

    /// A new pack, empty: its name, made unique.
    pub fn make_pack(&mut self, mut info: PackInfo) -> Result<String, String> {
        let base = file_name(&info.name);
        if base.is_empty() || base == FACTORY {
            return Err("a pack needs a name of its own".into());
        }
        let mut name = base.clone();
        let mut n = 2;
        while self.pack(&name).is_some() {
            name = format!("{base} {n}");
            n += 1;
        }
        info.name = name.clone();
        if info.made == 0 {
            info.made = now();
        }
        self.write_pack(&info)?;
        self.packs.push(info);
        self.packs.sort_by_key(|p| p.name.to_lowercase());
        Ok(name)
    }

    fn write_pack(&self, info: &PackInfo) -> Result<(), String> {
        let dir = self.folder(Some(&info.name));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(info).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("pack.json"), json).map_err(|e| e.to_string())
    }

    /// A pack's name, words or cover changed.
    pub fn set_pack(&mut self, info: PackInfo) -> Result<(), String> {
        self.write_pack(&info)?;
        if let Some(p) = self.packs.iter_mut().find(|p| p.name == info.name) {
            *p = info;
        }
        Ok(())
    }

    /// A pack renamed: its folder, and every sound's key in it. Its new name.
    pub fn rename_pack(&mut self, old: &str, new: &str) -> Result<String, String> {
        let new = file_name(new);
        if new.is_empty() || new == FACTORY || self.pack(&new).is_some() {
            return Err("that name's taken: a pack needs one of its own".into());
        }
        std::fs::rename(self.folder(Some(old)), self.folder(Some(&new)))
            .map_err(|e| e.to_string())?;
        for p in self.packs.iter_mut().filter(|p| p.name == old) {
            p.name = new.clone();
        }
        for e in self
            .sounds
            .iter_mut()
            .filter(|e| e.pack.as_deref() == Some(old))
        {
            e.pack = Some(new.clone());
            e.key = format!("{new}/{}", e.sound.name);
        }
        Ok(new)
    }

    /// A saved sound moved into another pack (None: in none): its new key.
    pub fn move_sound(&mut self, key: &str, to: Option<&str>) -> Result<String, String> {
        let sound = self
            .find(key)
            .filter(|e| !e.factory())
            .ok_or("only a saved sound moves")?
            .sound
            .clone();
        let new = self.save(sound, to)?;
        if new != key {
            self.remove(key)?;
        }
        Ok(new)
    }

    /// A pack as one file, to share: every sound in it.
    pub fn export(&self, pack: &str) -> Result<Vec<u8>, String> {
        let info = self.pack(pack).cloned().ok_or("no such pack")?;
        let sounds = self
            .sounds
            .iter()
            .filter(|e| e.pack.as_deref() == Some(pack))
            .map(|e| e.sound.clone())
            .collect();
        serde_json::to_vec_pretty(&PackFile { pack: info, sounds }).map_err(|e| e.to_string())
    }

    /// A pack file brought in, as a pack of its own (renamed if one has its name): its name,
    /// and how many sounds came in.
    pub fn import(&mut self, bytes: &[u8]) -> Result<(String, usize), String> {
        let file: PackFile =
            serde_json::from_slice(bytes).map_err(|_| "that isn't a pack of sounds".to_string())?;
        let mut info = file.pack;
        info.imported = true;
        info.order.clear();
        let name = self.make_pack(info)?;
        let n = file.sounds.len();
        for s in file.sounds {
            self.save(s, Some(&name))?;
        }
        Ok((name, n))
    }
}

impl Library {
    /// Every rack kept: those saved here (newest names first), then each factory sound's.
    pub fn racks(&self) -> Vec<(String, Vec<pina_engine::song::project::Unit>)> {
        let mut saved: Vec<(String, Vec<pina_engine::song::project::Unit>)> =
            std::fs::read_dir(self.dir.join(".racks"))
                .map(|d| {
                    d.flatten()
                        .filter_map(|e| {
                            let p = e.path();
                            let name = p.file_stem()?.to_str()?.to_string();
                            let units = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
                            Some((name, units))
                        })
                        .collect()
                })
                .unwrap_or_default();
        saved.sort_by_key(|r| r.0.to_lowercase());
        let factory = self
            .sounds
            .iter()
            .filter(|e| e.factory() && !e.sound.poly.fx.is_empty())
            .map(|e| (format!("{}'s", e.sound.name), e.sound.poly.fx.clone()));
        saved.into_iter().chain(factory).collect()
    }

    /// A rack kept by `name`, over one of that name.
    pub fn save_rack(
        &self,
        name: &str,
        units: &[pina_engine::song::project::Unit],
    ) -> Result<(), String> {
        let dir = self.dir.join(".racks");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(units).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(format!("{}.json", file_name(name))), json)
            .map_err(|e| e.to_string())
    }
}

/// Where the library lives: Pina's folder, or (in tests, which never touch the person's) a
/// folder of the test's own.
pub(crate) fn sounds_dir() -> PathBuf {
    match cfg!(test) {
        true => {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static N: AtomicUsize = AtomicUsize::new(0);
            std::env::temp_dir().join(format!(
                "pina-test-sounds-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ))
        }
        false => pina_engine::paths::support().join("Sounds"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound(name: &str) -> SavedSound {
        SavedSound {
            name: name.into(),
            part: "Pluck".into(),
            words: vec!["Glassy".into()],
            about: "short glassy plucks".into(),
            by: "you".into(),
            made: 1,
            poly: poly_patch("Init").unwrap(),
        }
    }

    #[test]
    fn sounds_are_saved_packed_shared_and_brought_back() {
        let dir = sounds_dir();
        let mut lib = Library::read(&dir);
        let factory = lib.sounds.len();
        assert_eq!(factory, POLY_PATCHES.len());
        assert!(lib.find("Evolving Pad").is_some_and(|e| e.factory()));
        // Saved in no pack, then into a pack made for it.
        let key = lib.save(sound("Neon Arp Glass"), None).unwrap();
        assert_eq!(key, "/Neon Arp Glass");
        let pack = lib
            .make_pack(PackInfo {
                name: "Night Drive".into(),
                by: "you".into(),
                ..Default::default()
            })
            .unwrap();
        let moved = lib.move_sound(&key, Some(&pack)).unwrap();
        assert_eq!(moved, "Night Drive/Neon Arp Glass");
        lib.save(sound("Sky Lead"), Some(&pack)).unwrap();
        // The factory's can't be written over; a pack's name is its own.
        assert!(lib.save(sound("x"), Some(FACTORY)).is_err());
        assert_eq!(
            lib.make_pack(PackInfo {
                name: "Night Drive".into(),
                ..Default::default()
            })
            .unwrap(),
            "Night Drive 2"
        );
        // Read again from the folder: all there.
        let again = Library::read(&dir);
        assert_eq!(again.count("Night Drive"), 2);
        assert!(again.find("/Neon Arp Glass").is_none());
        // Shared as one file and brought into another library: a pack of its own, imported.
        let file = again.export("Night Drive").unwrap();
        let other_dir = sounds_dir();
        let mut other = Library::read(&other_dir);
        assert_eq!(other.import(&file).unwrap(), ("Night Drive".into(), 2));
        assert!(other.pack("Night Drive").unwrap().imported);
        assert_eq!(
            other.find("Night Drive/Sky Lead").unwrap().sound.part,
            "Pluck"
        );
        assert!(other.import(b"not a pack").is_err());
        // Taken out, put aside (not a pack), and brought back.
        let gone = other.remove("Night Drive/Sky Lead").unwrap();
        assert_eq!(Library::read(&other_dir).count("Night Drive"), 1);
        assert_eq!(Library::read(&other_dir).packs.len(), 1);
        other.bring_back(gone).unwrap();
        assert_eq!(Library::read(&other_dir).count("Night Drive"), 2);
        assert!(other.remove("Evolving Pad").is_err());
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(other_dir);
    }
}
