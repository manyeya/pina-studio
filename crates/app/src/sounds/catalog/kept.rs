//! What the Browser keeps between runs: favourites and what was used lately.

use super::*;

/// What's remembered: favourites and what's been used lately, newest first.
#[derive(Serialize, Deserialize, Default)]
pub(crate) struct Kept {
    #[serde(default)]
    pub(crate) favourites: Vec<String>,
    #[serde(default)]
    pub(crate) recent: Vec<String>,
}

pub(crate) fn kept_file() -> PathBuf {
    pina_engine::paths::support().join("browser.json")
}

pub struct Catalog {
    pub devices: Vec<Device>,
    pub search: Entity<TextInput>,
    pub scope: Scope,
    pub cat: Option<Cat>,
    /// The row picked with the keyboard, by its place in what's shown.
    pub picked: Option<usize>,
    /// The device whose presets are shown, and them, once looked for.
    pub open: Option<String>,
    pub(crate) presets: HashMap<String, Vec<Preset>>,
    pub favourites: Vec<String>,
    pub recent: Vec<String>,
    pub(crate) _typed: Subscription,
    pub(crate) _done: Subscription,
}

impl Catalog {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let search = cx.new(|cx| TextInput::search("Search devices, presets, sounds, songs", cx));
        let typed = cx.observe(&search, |ws, _, cx| {
            ws.catalog.picked = (!ws.catalog_query(cx).is_empty()).then_some(0);
            cx.notify();
        });
        // Enter puts the picked row in; Escape clears the search.
        let done = cx.subscribe(&search, |ws, input, e: &TextEvent, cx| match e {
            TextEvent::Confirm(_) => ws.use_picked(false, cx),
            TextEvent::Cancel => input.update(cx, |i, cx| i.set_text("", cx)),
        });
        let kept: Kept = match cfg!(test) {
            true => Kept::default(),
            false => std::fs::read(kept_file())
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default(),
        };
        Catalog {
            devices: devices(&[]),
            search,
            scope: Scope::All,
            cat: None,
            picked: None,
            open: None,
            presets: HashMap::new(),
            favourites: kept.favourites,
            recent: kept.recent,
            _typed: typed,
            _done: done,
        }
    }

    pub(crate) fn save(&self) {
        if cfg!(test) {
            return;
        }
        let kept = Kept {
            favourites: self.favourites.clone(),
            recent: self.recent.clone(),
        };
        let file = kept_file();
        let _ = std::fs::create_dir_all(file.parent().unwrap());
        let _ = std::fs::write(file, serde_json::to_vec_pretty(&kept).unwrap());
    }

    pub fn device(&self, key: &str) -> Option<usize> {
        self.devices.iter().position(|d| d.key == key)
    }

    /// A plugin's preset files, found the first time they're asked for.
    pub fn presets(&mut self, d: usize) -> &[Preset] {
        let device = &self.devices[d];
        self.presets.entry(device.key.clone()).or_insert_with(|| {
            let ext = |f: Format| match f {
                Format::Vst3 => "vstpreset",
                Format::Au => "aupreset",
                Format::Vst2 => "fxp",
                Format::Clap => "clap-preset",
            };
            let mut found = Vec::new();
            for p in &device.formats {
                let dirs = pina_engine::plugin::preset_dirs(&p.vendor, &p.class);
                found.extend(pina_engine::plugin::preset_files(&dirs, ext(p.format)));
            }
            found.dedup_by(|a, b| a.name == b.name);
            found
        })
    }
}
