//! Parameters by name: plugins' (asked of them now and then), and any automatable one.

use super::*;

/// A plugin's parameters: their ids and names.
pub type ParamNames = Rc<Vec<(u32, String)>>;

impl Workspace {
    /// After an edit (or undo): keeps the selection valid and brings the engine up to date.
    /// A plugin's parameters' ids and names, asked of it now and then (an Audio Unit answers
    /// one call a parameter): kept until an edit, or two seconds, for a plugin swapped or loaded.
    pub fn param_names(&self, track: u64, which: Which) -> ParamNames {
        if let Some((at, names)) = self.param_names.borrow().get(&(track, which))
            && at.elapsed() < std::time::Duration::from_secs(2)
        {
            return names.clone();
        }
        let names: ParamNames = Rc::new(
            self.player
                .params(track, which)
                .into_iter()
                .map(|p| (p.id, p.name))
                .collect(),
        );
        // A plugin still loading has none yet: asked again next time.
        if !names.is_empty() {
            self.param_names
                .borrow_mut()
                .insert((track, which), (std::time::Instant::now(), names.clone()));
        }
        names
    }

    /// A plugin's name on a track, from its bundle.
    pub fn plugin_name(&self, track: usize, which: Which) -> String {
        self.session.project.tracks[track]
            .plugin(which)
            .and_then(|r| r.path.file_stem())
            .map_or_else(|| "Plugin".into(), |s| s.to_string_lossy().into_owned())
    }

    /// What a parameter is called: Volume, Pan, or the plugin's name for it.
    pub fn param_name(&self, track: usize, param: Param) -> String {
        let t = &self.session.project.tracks[track];
        match param {
            Param::Volume => "Volume".into(),
            Param::Pan => "Pan".into(),
            Param::Mute => "Mute".into(),
            Param::Send(bus) => {
                let aux = self.session.project.auxes.iter().find(|a| a.bus == bus);
                format!("Send to {}", aux.map_or("a bus", |a| a.name.as_str()))
            }
            Param::Eq(band, field) => {
                let what = match field {
                    pina_engine::song::project::EqField::Freq => "Frequency",
                    pina_engine::song::project::EqField::Gain => "Gain",
                    pina_engine::song::project::EqField::Q => "Q",
                };
                let shape =
                    t.eq.and_then(|e| e.bands.get(band as usize).map(|b| b.shape.name()))
                        .unwrap_or("Band");
                format!("EQ {} {shape}: {what}", band + 1)
            }
            Param::Mod(k, setting) => {
                format!("{}: {}", self.mod_name(track, k as usize), setting.name())
            }
            Param::Plugin(which, id) => t
                .automation
                .plugins
                .iter()
                .find(|a| a.plugin == which && a.param == id && !a.name.is_empty())
                .map(|a| a.name.clone())
                .or_else(|| {
                    let names = self.param_names(t.id, which);
                    names.iter().find(|p| p.0 == id).map(|p| p.1.clone())
                })
                .unwrap_or_else(|| format!("Parameter {id}")),
        }
    }

    /// Shows automated plugin parameters' values in their plugins (and Smart Controls) as the
    /// playhead moves: the engine plays them, the controllers only show them.
    pub(crate) fn show_automation_values(&self) {
        let tick = self.playhead() as f64;
        for t in &self.session.project.tracks {
            if t.automation.mode == AutoMode::Off {
                continue;
            }
            for a in &t.automation.plugins {
                if let Some(v) = value_at(&a.points, tick) {
                    self.player.show_param(t.id, a.plugin, a.param, v as f64);
                }
            }
        }
    }
}
