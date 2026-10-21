//! Which lanes a track shows, and what each can show.

use super::*;

impl Workspace {
    /// How tall each automation lane is.
    pub fn auto_h(&self) -> f32 {
        self.settings.auto_height
    }

    /// The parameters a track's lanes show, top to bottom: those opened, else its first
    /// automated one, else its volume.
    pub fn auto_lanes(&self, track: usize) -> Vec<Param> {
        let t = &self.session.project.tracks[track];
        match self.auto_lanes.get(&t.id) {
            Some(lanes) if !lanes.is_empty() => lanes.clone(),
            _ => vec![t.automation.params().first().copied().unwrap_or_default()],
        }
    }

    /// The parameter a track's first lane shows.
    pub fn lane_param(&self, track: usize) -> Param {
        self.auto_lanes(track)[0]
    }

    /// How much of a track's row its lanes take, while they show.
    pub fn autos_h(&self, track: usize) -> f32 {
        match self.show_automation {
            true => self.auto_lanes(track).len() as f32 * self.auto_h(),
            false => 0.0,
        }
    }

    /// A lane's top, in the lanes.
    pub fn lane_top(&self, track: usize, k: usize) -> f32 {
        self.row_top(track) + self.lane_h(track) + k as f32 * self.auto_h()
    }

    /// Shows `param` in a track's `k`th lane (or a new one under them); if it showed in another,
    /// they swap.
    pub(crate) fn set_lane(&mut self, track: usize, k: usize, param: Param) {
        let mut lanes = self.auto_lanes(track);
        match lanes.iter().position(|&p| p == param) {
            Some(j) if k < lanes.len() => lanes.swap(j, k),
            Some(_) => {}
            None if k < lanes.len() => lanes[k] = param,
            None => lanes.push(param),
        }
        let id = self.session.project.tracks[track].id;
        self.auto_lanes.insert(id, lanes);
        self.show_automation = true;
    }

    /// Makes sure a parameter shows, as it records: in place of a first lane with nothing on
    /// it, else under the others.
    pub(crate) fn show_lane(&mut self, track: usize, param: Param) {
        let lanes = self.auto_lanes(track);
        if lanes.contains(&param) {
            return;
        }
        let t = &self.session.project.tracks[track];
        let empty = lanes.len() == 1 && t.automation.points(lanes[0]).is_empty();
        self.set_lane(track, if empty { 0 } else { lanes.len() }, param);
    }

    /// A lane more under a track's: the next automated parameter not showing, else the next of
    /// volume, pan, mute and its sends.
    pub(crate) fn add_lane(&mut self, track: usize) {
        let lanes = self.auto_lanes(track);
        let t = &self.session.project.tracks[track];
        let next = t
            .automation
            .params()
            .into_iter()
            .chain([Param::Volume, Param::Pan, Param::Mute])
            .chain(t.sends.iter().map(|s| Param::Send(s.bus)))
            .find(|p| !lanes.contains(p));
        let param = next.unwrap_or(Param::Volume);
        self.set_lane(track, lanes.len(), param);
    }

    pub(crate) fn remove_lane(&mut self, track: usize, k: usize) {
        let mut lanes = self.auto_lanes(track);
        if lanes.len() > 1 && k < lanes.len() {
            lanes.remove(k);
        }
        let id = self.session.project.tracks[track].id;
        self.auto_lanes.insert(id, lanes);
    }

    /// What a track's lanes can show: volume, pan, mute, its sends, and every parameter of its
    /// plugins, those with automation marked; picking one shows it in lane `k`.
    pub fn auto_params(&self, track: usize, k: usize) -> Vec<(String, MenuAction)> {
        let t = &self.session.project.tracks[track];
        let id = t.id;
        let mark = |p: Param| match t.automation.points(p).is_empty() {
            true => "",
            false => "● ",
        };
        let item = |p: Param, label: String| {
            (
                format!("{}{label}", mark(p)),
                MenuAction::Auto(id, AutoOp::Lane(k, p)),
            )
        };
        let mut items: Vec<(String, MenuAction)> = [Param::Volume, Param::Pan, Param::Mute]
            .into_iter()
            .map(|p| item(p, self.param_name(track, p)))
            .collect();
        for s in &t.sends {
            let p = Param::Send(s.bus);
            items.push(item(p, self.param_name(track, p)));
        }
        let plugins = std::iter::once(Which::Instrument)
            .chain((0..t.inserts.len()).map(Which::Insert))
            .filter(|&w| t.plugin(w).is_some());
        for which in plugins {
            let name = self.plugin_name(track, which);
            items.push((name.clone(), MenuAction::Nothing));
            for p in self.player.params(id, which) {
                let param = Param::Plugin(which, p.id);
                items.push(item(param, format!("{name}: {}", p.name)));
            }
        }
        // Its modulators' settings: a macro turned, an LFO's phase, an envelope's times.
        let project = &self.session.project;
        let settings: Vec<Param> = (0..t.modulators.len())
            .filter_map(|k| Some((k, project.mod_kind(track, k)?)))
            .flat_map(|(k, kind)| {
                pina_engine::dsp::modulate::ModParam::of(kind)
                    .into_iter()
                    .map(move |s| Param::Mod(k as u8, s))
            })
            .collect();
        if !settings.is_empty() {
            items.push(("Modulators".into(), MenuAction::Nothing));
            items.extend(
                settings
                    .into_iter()
                    .map(|p| item(p, self.param_name(track, p))),
            );
        }
        // The EQ's bands: each one's frequency, gain (if it has one) and width.
        if let Some(eq) = t.eq.filter(|e| !e.bands.is_empty()) {
            items.push(("Channel EQ".into(), MenuAction::Nothing));
            for (b, band) in eq.bands.iter().enumerate() {
                let fields = [EqField::Freq, EqField::Gain, EqField::Q];
                for field in fields
                    .into_iter()
                    .filter(|&f| f != EqField::Gain || band.shape.has_gain())
                {
                    let p = Param::Eq(b as u8, field);
                    items.push(item(p, self.param_name(track, p)));
                }
            }
        }
        items
    }

    /// A region moved (or copied) with "Automation moves with regions": the automation under
    /// it goes with it.
    pub fn carry_automation(&mut self, from: (usize, u64, u64), to: (usize, u64), copy: bool) {
        let (t0, start, end) = from;
        let (t1, at) = to;
        if (t0 == t1 && start == at) || end <= start {
            return;
        }
        let clip = self.session.project.tracks[t0].automation.take(start, end);
        if clip.lanes.is_empty() {
            return;
        }
        if !copy {
            self.session.project.tracks[t0].automation.erase(start, end);
        }
        self.session.project.tracks[t1].automation.put(at, &clip);
    }
}
