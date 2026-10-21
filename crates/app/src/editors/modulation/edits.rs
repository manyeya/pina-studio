//! Modulators added, shared, taken off, mapped and unmapped, and their depths dragged.

use super::*;

impl Workspace {
    /// Edits a track's modulators as one undo step, and the engine hears.
    pub(crate) fn mods_edit(
        &mut self,
        t: usize,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Vec<Modulator>),
    ) {
        if t >= self.session.project.tracks.len() {
            return;
        }
        self.edit(cx, |ws| f(&mut ws.session.project.tracks[t].modulators));
    }

    pub fn add_modulator(&mut self, t: usize, kind: ModKind, cx: &mut Context<Self>) {
        let Some(track) = self.session.project.tracks.get(t) else {
            return;
        };
        if track.modulators.len() >= MAX_MODS {
            self.status = Some(format!("A track has {MAX_MODS} modulators at most."));
            cx.notify();
            return;
        }
        let k = track.modulators.len();
        self.mods_edit(t, cx, |mods| {
            mods.push(Modulator {
                kind,
                routes: Vec::new(),
            })
        });
        self.status = Some(format!(
            "{}: drag its ↗ onto a knob to have it move it",
            self.mod_name(t, k)
        ));
    }

    /// A modulator for every track in `t`'s stack: the stack's, each track routing it as it
    /// likes.
    pub fn share_modulator(&mut self, t: usize, kind: ModKind, cx: &mut Context<Self>) {
        let Some(aux) = self.session.project.stack_of(t) else {
            return;
        };
        if aux.modulators.len() >= MAX_MODS {
            self.status = Some(format!("A stack shares {MAX_MODS} modulators at most."));
            cx.notify();
            return;
        }
        let (bus, stack) = (aux.bus, aux.name.clone());
        self.edit(cx, |ws| {
            ws.session.project.add_shared(bus, kind);
        });
        let k = self.session.project.tracks[t]
            .modulators
            .len()
            .saturating_sub(1);
        self.status = Some(format!(
            "{}: every track in {stack} has it; drag its ↗ onto a knob of each to have it move it",
            self.mod_name(t, k)
        ));
    }

    pub fn remove_modulator(&mut self, t: usize, k: usize, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let Some(m) = p.tracks.get(t).and_then(|tr| tr.modulators.get(k)) else {
            return;
        };
        // One the stack shares goes from the stack and every track in it; else, this one.
        let shared = match m.kind {
            ModKind::Shared { modulator } if p.mod_kind(t, k).is_some() => {
                p.stack_of(t).map(|a| (a.bus, modulator as usize))
            }
            _ => None,
        };
        let gone: Vec<(usize, usize)> = match shared {
            Some((bus, s)) => p
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, tr)| tr.output == Some(bus))
                .filter_map(|(i, tr)| {
                    let at = tr.modulators.iter().position(
                        |m| matches!(m.kind, ModKind::Shared { modulator } if modulator as usize == s),
                    )?;
                    Some((i, at))
                })
                .collect(),
            None => vec![(t, k)],
        };
        let routes: Vec<(usize, ModRoute)> = gone
            .iter()
            .flat_map(|&(i, at)| {
                p.tracks[i].modulators[at]
                    .routes
                    .iter()
                    .map(move |r| (i, *r))
            })
            .collect();
        self.edit(cx, |ws| match shared {
            Some((bus, s)) => ws.session.project.remove_shared(bus, s),
            None => {
                let track = &mut ws.session.project.tracks[t];
                track.modulators.remove(k);
                // Its settings' automation goes with it; the ones after it move up a place.
                track.automation.modulator_removed(k);
            }
        });
        for (i, k) in gone {
            if let Some(id) = self.session.project.tracks.get(i).map(|tr| tr.id)
                && let Some(lanes) = self.auto_lanes.get_mut(&id)
            {
                lanes.retain(|p| !matches!(p, Param::Mod(m, _) if *m as usize == k));
                for p in lanes.iter_mut() {
                    if let Param::Mod(m, s) = *p
                        && m as usize > k
                    {
                        *p = Param::Mod(m - 1, s);
                    }
                }
            }
        }
        for (i, r) in routes {
            self.settle(i, r.target);
        }
        self.mod_edit = None;
    }

    /// A plugin parameter no longer moved: back where it's set.
    pub(crate) fn settle(&mut self, t: usize, mt: ModTarget) {
        if let (ModTarget::Plugin(which, id), Some(track)) =
            (mt, self.session.project.tracks.get(t))
        {
            let still = track
                .modulators
                .iter()
                .any(|m| m.routes.iter().any(|r| r.target == mt));
            if !still {
                let set = track.plugin(which).and_then(|r| r.params.get(&id).copied());
                if let Some(v) = set {
                    self.player.set_param(track.id, which, id, v);
                }
            }
        }
    }

    /// Modulator `k` of track `t` moving `mt`, by its usual depth (kept if it already does).
    pub fn map_modulator(&mut self, t: usize, k: usize, mt: ModTarget, cx: &mut Context<Self>) {
        let Some(track) = self.session.project.tracks.get(t) else {
            return;
        };
        // A pattern's steps are worked out ahead: an envelope or follower can't be known then.
        if let (ModTarget::Sound(..), Some(ModKind::Envelope { .. } | ModKind::Follower { .. })) =
            (mt, self.session.project.mod_kind(t, k))
        {
            self.status = Some(
                "A sound's steps are worked out ahead: an LFO, steps, random or a macro can move them, not an envelope or a follower.".into(),
            );
            cx.notify();
            return;
        }
        if track
            .modulators
            .get(k)
            .is_none_or(|m| m.routes.iter().any(|r| r.target == mt))
        {
            return;
        }
        // A plugin parameter moves about where it's set: kept in the song for the engine.
        let base = match mt {
            ModTarget::Plugin(which, id) => {
                Some((which, id, self.player.param_value(track.id, which, id)))
            }
            _ => None,
        };
        self.edit(cx, |ws| {
            let track = &mut ws.session.project.tracks[t];
            if let Some((which, id, v)) = base
                && let Some(r) = track.plugin_mut(which)
                && !r.params.contains_key(&id)
            {
                r.params.insert(id, v);
            }
            track.modulators[k].routes.push(ModRoute {
                target: mt,
                depth: modulate::default_depth(mt),
            });
        });
        self.status = Some(format!(
            "{} → {}: drag the route in its card, or pick the card and drag the knob, for how far",
            self.mod_name(t, k),
            self.target_name(t, mt)
        ));
    }

    pub fn unmap(&mut self, t: usize, k: usize, mt: ModTarget, cx: &mut Context<Self>) {
        self.mods_edit(t, cx, |mods| {
            if let Some(m) = mods.get_mut(k) {
                m.routes.retain(|r| r.target != mt);
            }
        });
        self.settle(t, mt);
    }

    /// A modulator dropped: onto the knob under the pointer, if it's one its track has.
    pub fn drop_modulator(&mut self, t: usize, k: usize, at: Point<f32>, cx: &mut Context<Self>) {
        let at = point(px(at.x), px(at.y));
        let hit = self
            .mod_targets
            .borrow()
            .iter()
            .rev()
            .find(|(_, _, b)| b.contains(&at))
            .map(|&(tt, mt, _)| (tt, mt));
        match hit {
            Some((tt, mt)) if tt == t => self.map_modulator(t, k, mt, cx),
            Some(_) => {
                self.status = Some("A modulator moves its own track's settings.".into());
                cx.notify();
            }
            None => {}
        }
    }

    /// A knob dragged while a modulator that moves it is picked: its depth instead.
    pub fn depth_drag(&self, drag: Drag) -> Drag {
        let Some((t, k)) = self.mod_edit else {
            return drag;
        };
        let (key, x, y) = match drag {
            Drag::Knob { target, y, .. } => (self.mod_key(target), 0.0, y),
            Drag::HFader { track, x, .. } => (Some((track, ModTarget::Volume)), x, 0.0),
            Drag::Pan {
                channel: Channel::Track(track),
                x,
                y,
                ..
            } => (Some((track, ModTarget::Pan)), x, y),
            _ => return drag,
        };
        let Some((tt, mt)) = key.filter(|&(tt, _)| tt == t) else {
            return drag;
        };
        let depth = self
            .session
            .project
            .tracks
            .get(tt)
            .and_then(|tr| tr.modulators.get(k))
            .and_then(|m| m.routes.iter().find(|r| r.target == mt))
            .map(|r| r.depth);
        match depth {
            Some(depth) => Drag::ModDepth {
                track: t,
                index: k,
                target: mt,
                x,
                y,
                depth,
            },
            None => drag,
        }
    }

    /// A route's depth dragged: up or right more, ⇧ finer; as far as its target goes either way.
    pub fn drag_depth(
        &mut self,
        (t, k, mt): (usize, usize, ModTarget),
        from: (f32, f32),
        depth: f32,
        p: Point<f32>,
    ) {
        let range = modulate::depth_range(mt);
        let (dx, dy) = (p.x - from.0, from.1 - p.y);
        let by = if dx.abs() > dy.abs() { dx } else { dy };
        let fine = if self.fine { 0.2 } else { 1.0 };
        let to = (depth + by / 160.0 * range * fine).clamp(-range, range);
        if let Some(r) = self
            .session
            .project
            .tracks
            .get_mut(t)
            .and_then(|tr| tr.modulators.get_mut(k))
            .and_then(|m| m.routes.iter_mut().find(|r| r.target == mt))
        {
            r.depth = to;
        }
        self.status = Some(format!(
            "{} → {}  {}",
            self.mod_name(t, k),
            self.target_name(t, mt),
            modulate::depth_text(mt, to)
        ));
    }

    /// For a track in a stack: a modulator to share with every track in it, or one the stack
    /// shares that this track hasn't.
    pub fn shared_menu(&self, t: usize) -> Option<Vec<(String, MenuAction)>> {
        let p = &self.session.project;
        let aux = p.stack_of(t)?;
        let mut items = vec![(format!("Shared by {}", aux.name), MenuAction::Nothing)];
        // A follower starts on the kick, or else the first track outside the stack.
        let source = p
            .tracks
            .iter()
            .find(|o| o.name.to_lowercase().contains("kick"))
            .or_else(|| p.tracks.iter().find(|o| o.output != Some(aux.bus)))
            .map_or(p.tracks[t].id, |o| o.id);
        // An envelope hears notes, and a stack plays none of its own.
        for kind in [
            ModKind::lfo(),
            ModKind::steps(),
            ModKind::follower(source),
            ModKind::random(),
            ModKind::mac(),
        ] {
            items.push((
                format!("New {}", kind.name()),
                MenuAction::ShareMod(t, kind),
            ));
        }
        let has = |s: usize| {
            p.tracks[t]
                .modulators
                .iter()
                .any(|m| matches!(m.kind, ModKind::Shared { modulator } if modulator as usize == s))
        };
        let missing: Vec<usize> = (0..aux.modulators.len()).filter(|&s| !has(s)).collect();
        if !missing.is_empty() {
            items.push((String::new(), MenuAction::Nothing));
        }
        for s in missing {
            let kind = aux.modulators[s].kind.name();
            let n = aux.modulators[..=s]
                .iter()
                .filter(|o| o.kind.name() == kind)
                .count();
            items.push((
                format!("{} · {kind} {n}", aux.name),
                MenuAction::LinkMod(t, s as u8),
            ));
        }
        Some(items)
    }
}
