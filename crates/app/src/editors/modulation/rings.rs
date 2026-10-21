//! Knobs that modulators move: their rings, and where each is now.

use super::*;

/// The knobs a modulator can be dropped on (track, setting), where each was drawn; the steps'
/// editors (track, modulator), where.
pub type DropTargets = std::rc::Rc<std::cell::RefCell<Vec<(usize, ModTarget, Bounds<Pixels>)>>>;

/// A modulator's range on a knob: its colour, the knob positions it moves between, and where it
/// has it now.
pub struct Ring {
    pub color: u32,
    pub lo: f32,
    pub hi: f32,
    pub live: f32,
}

/// The knob's angle (degrees, clockwise from three o'clock) for a position, as `knob` draws it.
pub(crate) fn angle(pos: f32) -> f32 {
    135.0 + 270.0 * pos.clamp(0.0, 1.0)
}

/// A knob's rings: each modulator's range just outside its arc, a step further out each, and a
/// dot where it has it now; a halo while a modulator is offered to it.
pub(crate) fn paint_rings(b: Bounds<Pixels>, rings: &[Ring], offering: bool, window: &mut Window) {
    let c = b.center();
    let base = f32::from(b.size.width.min(b.size.height)) / 2.0;
    let at = |deg: f32, r: f32| {
        let a = deg.to_radians();
        point(c.x + px(r * a.cos()), c.y + px(r * a.sin()))
    };
    if offering {
        window.paint_quad(
            fill(
                Bounds::centered_at(c, size(px(base * 2.0 + 8.0), px(base * 2.0 + 8.0))),
                alpha(ACCENT, 0.12),
            )
            .corner_radii(px(base + 4.0))
            .border_widths(px(1.))
            .border_color(alpha(ACCENT, 0.8)),
        );
    }
    for (k, ring) in rings.iter().enumerate() {
        let r = base + 3.0 + 3.0 * k as f32;
        let (from, to) = (angle(ring.lo), angle(ring.hi));
        if to - from > 0.5 {
            let mut p = PathBuilder::stroke(px(2.));
            let steps = ((to - from) / 6.0).ceil().max(2.0) as usize;
            for i in 0..=steps {
                let d = from + (to - from) * i as f32 / steps as f32;
                match i {
                    0 => p.move_to(at(d, r)),
                    _ => p.line_to(at(d, r)),
                }
            }
            if let Ok(p) = p.build() {
                window.paint_path(p, alpha(ring.color, 0.75));
            }
        }
        window.paint_quad(
            fill(
                Bounds::centered_at(at(angle(ring.live), r), size(px(5.), px(5.))),
                rgb(ring.color),
            )
            .corner_radii(px(2.5)),
        );
    }
}

impl Workspace {
    /// The track and setting a knob turns, if modulators can move it.
    pub fn mod_key(&self, target: Target) -> Option<(usize, ModTarget)> {
        let track = |ch: Channel| match ch {
            Channel::Track(t) => Some(t),
            _ => None,
        };
        Some(match target {
            Target::Param { track: id, param } => {
                let t = self
                    .session
                    .project
                    .tracks
                    .iter()
                    .position(|t| t.id == id)?;
                (t, ModTarget::Plugin(Which::Instrument, param))
            }
            Target::LowCut(ch) => (track(ch)?, ModTarget::LowCut),
            // A quick knob moves its band once it has one.
            Target::Quick(ch, q) => {
                let eq = self.strip_dsp(ch).0?;
                (track(ch)?, ModTarget::EqGain(q.find(&eq)? as u8))
            }
            Target::Comp(ch, CompField::Threshold) => (track(ch)?, ModTarget::CompThreshold),
            Target::Comp(ch, CompField::Makeup) => (track(ch)?, ModTarget::CompMakeup),
            Target::Comp(..) => return None,
            Target::Sat(ch, SatField::Drive) => (track(ch)?, ModTarget::SatDrive),
            Target::Sat(ch, SatField::Tone) => (track(ch)?, ModTarget::SatTone),
            Target::Sat(ch, SatField::Mix) => (track(ch)?, ModTarget::SatMix),
            Target::Sat(..) => return None,
            Target::Sampler { track, field } => (track, ModTarget::Sampler(param_of(field)?)),
        })
    }

    /// Where a knob for `mt` on track `t` would sit, moved `by` in its units.
    pub fn mod_pos(&self, t: usize, mt: ModTarget, by: f32) -> f32 {
        let Some(track) = self.session.project.tracks.get(t) else {
            return 0.0;
        };
        let eq = track.eq.unwrap_or_default();
        let comp = track.comp.unwrap_or_default();
        let sat = track.sat.unwrap_or_default();
        match mt {
            ModTarget::Volume => strip::fader_pos(modulate::apply_volume(track.volume_db, by)),
            ModTarget::Pan => (modulate::apply_pan(track.pan, by) + 1.0) / 2.0,
            ModTarget::Send(i) => track.sends.get(i).map_or(0.0, |s| {
                strip::fader_pos(modulate::apply_volume(s.level_db, by))
            }),
            ModTarget::LowCut | ModTarget::EqGain(_) | ModTarget::EqFreq(_) => {
                let mut e = eq;
                modulate::apply_eq(&mut e, mt, by);
                match mt {
                    ModTarget::LowCut => strip::low_cut_pos(crate::mix::eq::low_cut(match by {
                        0.0 => &eq,
                        _ => &e,
                    })),
                    ModTarget::EqGain(i) => {
                        strip::eq_gain_pos(e.bands.get(i as usize).map_or(0.0, |b| b.gain_db))
                    }
                    ModTarget::EqFreq(i) => e.bands.get(i as usize).map_or(0.0, |b| {
                        ((b.freq / 10.0).ln() / 3000f32.ln()).clamp(0.0, 1.0)
                    }),
                    _ => 0.0,
                }
            }
            ModTarget::CompThreshold | ModTarget::CompMakeup => {
                let mut c = comp;
                modulate::apply_comp(&mut c, mt, by);
                let field = match mt {
                    ModTarget::CompThreshold => CompField::Threshold,
                    _ => CompField::Makeup,
                };
                strip::comp_pos(field, &c)
            }
            ModTarget::SatDrive | ModTarget::SatTone | ModTarget::SatMix => {
                let mut s = sat;
                modulate::apply_sat(&mut s, mt, by);
                let field = match mt {
                    ModTarget::SatDrive => SatField::Drive,
                    ModTarget::SatTone => SatField::Tone,
                    _ => SatField::Mix,
                };
                strip::sat_pos(field, &s)
            }
            ModTarget::Sampler(p) => {
                let Some(Instrument::Sampler(s)) = &track.instrument else {
                    return 0.0;
                };
                let mut voice = s.voice;
                modulate::apply_voice(&mut voice, p, by);
                field_pos(
                    &Sampler {
                        voice,
                        ..Default::default()
                    },
                    field_of(p),
                )
            }
            ModTarget::Plugin(which, id) => {
                (self.player.param_value(track.id, which, id) as f32 + by).clamp(0.0, 1.0)
            }
            // Drawn in the step sequencer's lane, not on a knob.
            ModTarget::Sound(..) => 0.0,
        }
    }

    /// The modulators moving `mt` on track `t`, as rings: their ranges and where they are now.
    pub fn mod_rings(&self, t: usize, mt: ModTarget) -> Vec<Ring> {
        let Some(track) = self.session.project.tracks.get(t) else {
            return Vec::new();
        };
        let values = self.mod_values.get(&track.id).copied().unwrap_or_default();
        track
            .modulators
            .iter()
            .enumerate()
            .flat_map(|(k, m)| {
                let bipolar = self
                    .session
                    .project
                    .mod_kind(t, k)
                    .is_some_and(|k| k.bipolar());
                let low = if bipolar { -1.0 } else { 0.0 };
                m.routes
                    .iter()
                    .filter(move |r| r.target == mt)
                    .map(move |r| (k, low, r.depth))
            })
            .map(|(k, low, depth)| {
                let (a, b) = (self.mod_pos(t, mt, depth * low), self.mod_pos(t, mt, depth));
                Ring {
                    color: MOD_COLORS[k % MAX_MODS],
                    lo: a.min(b),
                    hi: a.max(b),
                    live: self.mod_pos(t, mt, depth * values[k]),
                }
            })
            .collect()
    }

    /// A knob that modulators can move: `inner` (a `size` knob) with its rings round it, a drop
    /// target for a modulator's ↗, lit while one is dragged.
    pub fn modded(
        &self,
        key: Option<(usize, ModTarget)>,
        size: f32,
        inner: impl IntoElement,
    ) -> Div {
        let wrap = div().relative().size(px(size)).flex_none().child(inner);
        let Some((t, mt)) = key else {
            return wrap;
        };
        let rings = self.mod_rings(t, mt);
        let offering = matches!(self.drag, Some(Drag::Modulator { track, .. }) if track == t);
        let targets = self.mod_targets.clone();
        wrap.child(
            canvas(
                move |b, _, _| targets.borrow_mut().push((t, mt, b)),
                move |b, _, window, _| paint_rings(b, &rings, offering, window),
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }

    /// The same for a fader that slides sideways (the header's), `w` wide: its rings as bars
    /// over it.
    pub fn modded_fader(&self, t: usize, w: f32, h: f32) -> Div {
        let rings = self.mod_rings(t, ModTarget::Volume);
        let offering = matches!(self.drag, Some(Drag::Modulator { track, .. }) if track == t);
        let targets = self.mod_targets.clone();
        div().absolute().top_0().left_0().w(px(w)).h(px(h)).child(
            canvas(
                move |b, _, _| targets.borrow_mut().push((t, ModTarget::Volume, b)),
                move |b, _, window, _| {
                    let (x0, w) = (b.origin.x + px(7.), f32::from(b.size.width) - 14.0);
                    if offering {
                        window.paint_quad(
                            fill(b, alpha(ACCENT, 0.12))
                                .corner_radii(px(5.))
                                .border_widths(px(1.))
                                .border_color(alpha(ACCENT, 0.7)),
                        );
                    }
                    for (k, ring) in rings.iter().enumerate() {
                        let y = b.origin.y + px(1.0 + 2.5 * k as f32);
                        let (a, z) = (x0 + px(w * ring.lo), x0 + px(w * ring.hi));
                        window.paint_quad(fill(
                            Bounds::new(point(a, y), size(z - a + px(1.), px(2.))),
                            alpha(ring.color, 0.6),
                        ));
                        window.paint_quad(
                            fill(
                                Bounds::centered_at(
                                    point(x0 + px(w * ring.live), y + px(1.)),
                                    size(px(5.), px(5.)),
                                ),
                                rgb(ring.color),
                            )
                            .corner_radii(px(2.5)),
                        );
                    }
                },
            )
            .size_full(),
        )
    }
}
