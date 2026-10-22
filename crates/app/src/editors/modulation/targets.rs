//! What a modulator can move, by name, and the menus that pick it.

use super::*;

impl Workspace {
    /// A modulator's name on its track: LFO 1, Steps 2.
    pub fn mod_name(&self, t: usize, k: usize) -> String {
        let p = &self.session.project;
        let Some(m) = p.tracks.get(t).and_then(|t| t.modulators.get(k)) else {
            return String::new();
        };
        // One its stack shares by the stack's name, numbered among the stack's.
        let (list, at, from) = match m.kind {
            ModKind::Shared { modulator } => match p.stack_of(t) {
                Some(aux) if (modulator as usize) < aux.modulators.len() => (
                    &aux.modulators[..],
                    modulator as usize,
                    format!("{} · ", aux.name),
                ),
                _ => return "Shared".into(),
            },
            _ => (&p.tracks[t].modulators[..], k, String::new()),
        };
        let kind = list[at].kind.name();
        let n = list[..=at].iter().filter(|o| o.kind.name() == kind).count();
        format!("{from}{kind} {n}")
    }

    /// What a route moves, as people say it.
    pub fn target_name(&self, t: usize, mt: ModTarget) -> String {
        let track = self.session.project.tracks.get(t);
        match mt {
            ModTarget::Volume => "Volume".into(),
            ModTarget::Pan => "Pan".into(),
            ModTarget::Send(i) => {
                let aux = track
                    .and_then(|tr| tr.sends.get(i))
                    .and_then(|s| self.session.project.auxes.iter().find(|a| a.bus == s.bus));
                format!("Send to {}", aux.map_or("its aux", |a| a.name.as_str()))
            }
            ModTarget::LowCut => "Low cut".into(),
            ModTarget::EqGain(i) | ModTarget::EqFreq(i) => {
                let band = track
                    .and_then(|tr| tr.eq)
                    .and_then(|e| e.bands.get(i as usize).copied());
                let what = if matches!(mt, ModTarget::EqGain(_)) {
                    "gain"
                } else {
                    "frequency"
                };
                match band {
                    Some(b) => format!(
                        "EQ {} {what} ({} {})",
                        i + 1,
                        b.shape.name().to_lowercase(),
                        crate::mix::eq::hz_text(b.freq)
                    ),
                    None => format!("EQ band {} {what}", i + 1),
                }
            }
            ModTarget::CompThreshold => "Threshold".into(),
            ModTarget::CompMakeup => "Makeup".into(),
            ModTarget::SatDrive => "Drive".into(),
            ModTarget::SatTone => "Tone".into(),
            ModTarget::SatMix => "Saturation mix".into(),
            ModTarget::Sampler(p) => match p {
                SamplerParam::Cutoff => "Cutoff",
                SamplerParam::Resonance => "Resonance",
                SamplerParam::FilterEnv => "Filter envelope",
                SamplerParam::Transpose => "Transpose",
                SamplerParam::Volume => "Sampler level",
                SamplerParam::Vibrato => "Vibrato",
                SamplerParam::LfoFilter => "LFO to filter",
                SamplerParam::Tremolo => "Tremolo",
            }
            .into(),
            ModTarget::Plugin(which, id) => {
                let name = track
                    .and_then(|tr| {
                        let names = self.param_names(tr.id, which);
                        names.iter().find(|p| p.0 == id).map(|p| p.1.clone())
                    })
                    .unwrap_or_else(|| format!("Parameter {id}"));
                // An effect's, by its plugin: two can both have a Cutoff.
                match (which, track.and_then(|tr| tr.plugin(which))) {
                    (Which::Insert(_), Some(r)) => format!("{} · {name}", r.class),
                    _ => name,
                }
            }
            ModTarget::Sound(r, param) => {
                let sound = self
                    .sound_name(t, r)
                    .unwrap_or_else(|| format!("Sound {}", r + 1));
                let what = match param {
                    pina_engine::dsp::modulate::SoundParam::Velocity => "velocity",
                    pina_engine::dsp::modulate::SoundParam::Pitch => "pitch",
                    pina_engine::dsp::modulate::SoundParam::Chance => "chance",
                };
                format!("{sound} {what}")
            }
        }
    }

    /// A step pattern's sound on a track, by its row: the pattern shown, else the track's first.
    pub(crate) fn sound_name(&self, t: usize, r: usize) -> Option<String> {
        let track = self.session.project.tracks.get(t)?;
        let shown = self.pattern().filter(|at| at.0 == t).map(|at| at.1);
        let regions = shown.into_iter().chain(0..track.regions.len());
        let pitch = regions
            .filter_map(|i| match &track.regions.get(i)?.content {
                pina_engine::song::project::Content::Pattern(p) => {
                    p.rows.get(r).map(|row| row.pitch)
                }
                _ => None,
            })
            .next()?;
        Some(crate::editors::steps::row_name(
            track.instrument.as_ref(),
            self.session.project.song_key(),
            pitch,
        ))
    }

    /// Everything a modulator on track `t` can move, as a menu (the step pattern's sounds too).
    pub fn target_menu(&self, t: usize, k: usize) -> Vec<(String, MenuAction)> {
        let Some(track) = self.session.project.tracks.get(t) else {
            return Vec::new();
        };
        let ahead = !matches!(
            self.session.project.mod_kind(t, k),
            Some(ModKind::Envelope { .. } | ModKind::Follower { .. })
        );
        let mut items: Vec<(String, MenuAction)> = Vec::new();
        let group =
            |items: &mut Vec<(String, MenuAction)>, title: &str, targets: Vec<ModTarget>| {
                if targets.is_empty() {
                    return;
                }
                if !items.is_empty() {
                    items.push((String::new(), MenuAction::Nothing));
                }
                items.push((title.to_string(), MenuAction::Nothing));
                for mt in targets {
                    items.push((self.target_name(t, mt), MenuAction::Map(t, k, mt)));
                }
            };
        // The pattern's sounds first: what a step sequencer modulates.
        let rows = self
            .pattern()
            .filter(|at| at.0 == t)
            .and_then(|at| match &track.regions[at.1].content {
                pina_engine::song::project::Content::Pattern(p) => Some(p.rows.len()),
                _ => None,
            })
            .unwrap_or(0);
        if ahead {
            use pina_engine::dsp::modulate::SoundParam;
            let sounds = (0..rows)
                .flat_map(|r| {
                    [SoundParam::Velocity, SoundParam::Pitch, SoundParam::Chance]
                        .map(|p| ModTarget::Sound(r, p))
                })
                .collect();
            group(&mut items, "Its pattern's sounds", sounds);
        }
        let mut mix = vec![ModTarget::Volume, ModTarget::Pan];
        mix.extend((0..track.sends.len()).map(ModTarget::Send));
        group(&mut items, "Mix", mix);
        group(
            &mut items,
            "Channel EQ, compressor, saturator",
            std::iter::once(ModTarget::LowCut)
                .chain(track.eq.iter().flat_map(|e| {
                    e.bands.iter().enumerate().flat_map(|(i, b)| {
                        let gain = b.shape.has_gain().then_some(ModTarget::EqGain(i as u8));
                        gain.into_iter().chain([ModTarget::EqFreq(i as u8)])
                    })
                }))
                .chain([
                    ModTarget::CompThreshold,
                    ModTarget::CompMakeup,
                    ModTarget::SatDrive,
                    ModTarget::SatTone,
                    ModTarget::SatMix,
                ])
                .collect(),
        );
        if matches!(track.instrument, Some(Instrument::Sampler(_))) {
            group(
                &mut items,
                "Sampler",
                SamplerParam::ALL.map(ModTarget::Sampler).to_vec(),
            );
        }
        // Every parameter of its plugins: the instrument's, then each insert's (type to find).
        for which in self.plugin_slots(t) {
            let names = self.param_names(track.id, which);
            if names.is_empty() {
                continue;
            }
            items.push((String::new(), MenuAction::Nothing));
            items.push((
                match which {
                    Which::Instrument => "Instrument".to_string(),
                    Which::Insert(i) => format!(
                        "Insert {}: {}",
                        i + 1,
                        track.inserts.get(i).map_or("", |r| r.class.as_str())
                    ),
                },
                MenuAction::Nothing,
            ));
            // Named from the one list (asking a plugin for one parameter's name asks it for
            // all); an effect's by its plugin too, as `target_name` says them.
            let class = match which {
                Which::Insert(i) => track.inserts.get(i).map(|r| r.class.as_str()),
                Which::Instrument => None,
            };
            items.extend(names.iter().map(|(id, name)| {
                let label = match class {
                    Some(c) => format!("{c} · {name}"),
                    None => name.clone(),
                };
                (label, MenuAction::Map(t, k, ModTarget::Plugin(which, *id)))
            }));
        }
        items
    }

    /// The plugins on track `t`: its instrument, if it's one, and its inserts.
    pub(crate) fn plugin_slots(&self, t: usize) -> Vec<Which> {
        let Some(track) = self.session.project.tracks.get(t) else {
            return Vec::new();
        };
        let instrument =
            matches!(track.instrument, Some(Instrument::Plugin(_))).then_some(Which::Instrument);
        instrument
            .into_iter()
            .chain((0..track.inserts.len()).map(Which::Insert))
            .collect()
    }

    /// What a modulator moves to move an automation lane's parameter, if one can.
    pub fn mod_target_of(&self, t: usize, param: Param) -> Option<ModTarget> {
        use pina_engine::song::project::EqField;
        Some(match param {
            Param::Volume => ModTarget::Volume,
            Param::Pan => ModTarget::Pan,
            Param::Send(bus) => {
                let sends = &self.session.project.tracks.get(t)?.sends;
                ModTarget::Send(sends.iter().position(|s| s.bus == bus)?)
            }
            Param::Plugin(which, id) => ModTarget::Plugin(which, id),
            Param::Eq(band, EqField::Gain) => ModTarget::EqGain(band),
            Param::Eq(band, EqField::Freq) => ModTarget::EqFreq(band),
            _ => return None,
        })
    }

    /// An automation lane's menu of modulators: each of the track's, ticked if it moves `mt`
    /// (picking it maps it, or lets it go), and new ones that do.
    pub fn lane_mod_menu(
        &self,
        t: usize,
        mt: ModTarget,
        title: String,
    ) -> Vec<(String, MenuAction)> {
        let Some(track) = self.session.project.tracks.get(t) else {
            return Vec::new();
        };
        let mut items = vec![(format!("Modulate {title}"), MenuAction::Nothing)];
        for (k, m) in track.modulators.iter().enumerate() {
            let name = self.mod_name(t, k);
            items.push(match m.routes.iter().any(|r| r.target == mt) {
                true => (format!("✓ {name}"), MenuAction::Unmap(t, k, mt)),
                false => (name, MenuAction::Map(t, k, mt)),
            });
        }
        if track.modulators.len() < MAX_MODS {
            if !track.modulators.is_empty() {
                items.push((String::new(), MenuAction::Nothing));
            }
            for kind in [
                ModKind::lfo(),
                ModKind::steps(),
                ModKind::random(),
                ModKind::mac(),
                ModKind::envelope(),
            ] {
                items.push((
                    format!("New {}", kind.name()),
                    MenuAction::NewModFor(t, kind, mt),
                ));
            }
        }
        items
    }

    /// A new modulator on track `t`, moving `mt`.
    pub fn new_mod_for(&mut self, t: usize, kind: ModKind, mt: ModTarget, cx: &mut Context<Self>) {
        let Some(k) = self
            .session
            .project
            .tracks
            .get(t)
            .map(|tr| tr.modulators.len())
        else {
            return;
        };
        self.add_modulator(t, kind, cx);
        if self.session.project.tracks[t].modulators.len() > k {
            self.map_modulator(t, k, mt, cx);
        }
    }
}
