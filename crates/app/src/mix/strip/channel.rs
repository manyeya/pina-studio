//! A channel's settings, read and set: volume, pan, inserts, saturator, limiter, and which
//! channel a strip shows.

use super::*;

impl Workspace {
    /// A channel's level: for a track whose volume is automated, where the automation has it.
    pub fn volume(&self, ch: Channel) -> f32 {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => self
                .automated(i, Param::Volume)
                .unwrap_or(p.tracks[i].volume_db),
            Channel::Aux(i) => p.auxes[i].volume_db,
            Channel::Master => p.master_volume_db,
        }
    }

    /// A channel's built-in saturator.
    pub fn sat(&self, ch: Channel) -> Option<Saturator> {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].sat,
            Channel::Aux(i) => p.auxes[i].sat,
            Channel::Master => p.master_sat,
        }
    }

    pub fn sat_mut(&mut self, ch: Channel) -> &mut Option<Saturator> {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => &mut p.tracks[i].sat,
            Channel::Aux(i) => &mut p.auxes[i].sat,
            Channel::Master => &mut p.master_sat,
        }
    }

    /// A channel's built-in limiter.
    pub fn limiter(&self, ch: Channel) -> Option<Limiter> {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].limit,
            Channel::Aux(i) => p.auxes[i].limit,
            Channel::Master => p.master_limit,
        }
    }

    /// A channel's limiter as it's heard now: its settings where their automation has them.
    pub fn limiter_heard(&self, ch: Channel) -> Option<Limiter> {
        let mut l = self.limiter(ch)?;
        if let Channel::Track(t) = ch {
            for field in pina_engine::song::project::LimitParam::ALL {
                if let Some(v) = self.automated(t, Param::Limit(field)) {
                    field.set(&mut l, v);
                }
            }
        }
        Some(l)
    }

    /// A stacked effect as it's heard now: its knobs where their automation has them.
    pub fn tool_heard(&self, ch: Channel, k: usize) -> Option<Unit> {
        let u = *self.more(ch).get(k)?;
        Some(self.as_heard(ch, k as u8, u))
    }

    /// An effect at `place` as it's heard now.
    fn as_heard(&self, ch: Channel, place: u8, mut u: Unit) -> Unit {
        if let Channel::Track(t) = ch {
            for (i, knob) in knobs(u.kind()).iter().enumerate() {
                if let Some(v) = self.automated(t, Param::Tool(place, i as u8)) {
                    (knob.set)(&mut u, v);
                }
            }
        }
        u
    }

    pub fn limiter_mut(&mut self, ch: Channel) -> &mut Option<Limiter> {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => &mut p.tracks[i].limit,
            Channel::Aux(i) => &mut p.auxes[i].limit,
            Channel::Master => &mut p.master_limit,
        }
    }

    pub fn set_volume(&mut self, ch: Channel, db: f32) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].volume_db = db,
            Channel::Aux(i) => p.auxes[i].volume_db = db,
            Channel::Master => p.master_volume_db = db,
        }
    }

    pub fn set_pan(&mut self, ch: Channel, pan: f32) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].pan = pan,
            Channel::Aux(i) => p.auxes[i].pan = pan,
            Channel::Master => {}
        }
    }

    pub fn inserts(&self, ch: Channel) -> &[PluginRef] {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => &p.tracks[i].inserts,
            Channel::Aux(i) => &p.auxes[i].inserts,
            Channel::Master => &p.master_inserts,
        }
    }

    pub fn channel_id(&self, ch: Channel) -> Option<u64> {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => Some(p.tracks[i].id),
            Channel::Aux(i) => Some(p.auxes[i].id),
            Channel::Master => None,
        }
    }

    /// A channel's name and color.
    pub fn channel_look(&self, ch: Channel) -> (String, u32) {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => (p.tracks[i].name.clone(), self.color(i)),
            Channel::Aux(i) => (p.auxes[i].name.clone(), BUS),
            Channel::Master => ("Stereo Out".into(), MASTER),
        }
    }

    /// The id a channel's plugins go by in the player.
    pub fn owner_id(&self, ch: Channel) -> u64 {
        self.channel_id(ch).unwrap_or(MASTER_ID)
    }

    pub(crate) fn has_builtin_instrument(&self, ch: Channel) -> bool {
        matches!(ch, Channel::Track(i) if matches!(self.session.project.tracks[i].instrument, Some(Instrument::Synth | Instrument::Drums | Instrument::Sampler(_))))
    }

    /// Shows a channel in the mixer, its effects in the side panel.
    #[cfg(test)]
    pub fn show_channel(&mut self, ch: Channel, cx: &mut Context<Self>) {
        self.channel = ch;
        if let Channel::Track(i) = ch {
            self.track = i;
        }
        self.view = crate::workspace::View::Mixer;
        cx.notify();
    }
}

/// A built-in effect, as a channel has it: its own (`at` None), or another of its stacked ones
/// (`at` its place among them).
pub trait Stackable: AsUnit {
    fn own(ws: &Workspace, ch: Channel) -> Option<Self>;
    /// Where its own goes on a channel: None for those that only stack (a reverb, a delay).
    fn own_mut(ws: &mut Workspace, ch: Channel) -> Option<&mut Option<Self>>;
}

impl Stackable for Eq {
    fn own(ws: &Workspace, ch: Channel) -> Option<Self> {
        ws.strip_dsp(ch).0
    }
    fn own_mut(ws: &mut Workspace, ch: Channel) -> Option<&mut Option<Self>> {
        Some(ws.strip_dsp_mut(ch).0)
    }
}

impl Stackable for Comp {
    fn own(ws: &Workspace, ch: Channel) -> Option<Self> {
        ws.strip_dsp(ch).1
    }
    fn own_mut(ws: &mut Workspace, ch: Channel) -> Option<&mut Option<Self>> {
        Some(ws.strip_dsp_mut(ch).1)
    }
}

impl Stackable for Saturator {
    fn own(ws: &Workspace, ch: Channel) -> Option<Self> {
        ws.sat(ch)
    }
    fn own_mut(ws: &mut Workspace, ch: Channel) -> Option<&mut Option<Self>> {
        Some(ws.sat_mut(ch))
    }
}

impl Stackable for Limiter {
    fn own(ws: &Workspace, ch: Channel) -> Option<Self> {
        ws.limiter(ch)
    }
    fn own_mut(ws: &mut Workspace, ch: Channel) -> Option<&mut Option<Self>> {
        Some(ws.limiter_mut(ch))
    }
}

macro_rules! only_stacked {
    ($($t:ty),*) => {
        $(impl Stackable for $t {
            fn own(_: &Workspace, _: Channel) -> Option<Self> {
                None
            }
            fn own_mut(_: &mut Workspace, _: Channel) -> Option<&mut Option<Self>> {
                None
            }
        })*
    };
}
only_stacked!(
    Reverb, Delay, FilterFx, Motion, Shaper, Gate, DeEsser, Stereo, Multiband, PitchShift,
    TapeStop, Tune
);

impl Workspace {
    /// A channel's other built-ins, stacked past its own of each kind, in the order they run.
    pub fn more(&self, ch: Channel) -> &[Unit] {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => &p.tracks[i].more,
            Channel::Aux(i) => &p.auxes[i].more,
            Channel::Master => &p.master_more,
        }
    }

    /// A channel's built-in of a kind as it's heard now: its own (`at` None), or the one at `at`
    /// among its others.
    pub fn built_in<T: Stackable>(&self, ch: Channel, at: Option<usize>) -> Option<T> {
        match at {
            None => {
                let own = T::own(self, ch)?.unit();
                let place = match own {
                    Unit::Comp(_) => OWN_COMP,
                    Unit::Sat(_) => OWN_SAT,
                    _ => return T::own(self, ch),
                };
                T::of(&self.as_heard(ch, place, own))
            }
            Some(k) => T::of(&self.tool_heard(ch, k)?),
        }
    }

    /// A channel's built-in (as `built_in` finds it), to change in place: no edit of its own
    /// (inside one, or a drag's).
    pub fn unit_mut<T: Stackable>(&mut self, ch: Channel, at: Option<usize>) -> Option<&mut T> {
        match at {
            None => T::own_mut(self, ch)?.as_mut(),
            Some(k) => self.chain_mut(ch).more.get_mut(k).and_then(T::of_mut),
        }
    }

    /// The built-in the effect window shows (its channel's own, or a stacked one), to change.
    pub fn fx_unit_mut<T: Stackable>(&mut self, ch: Channel) -> Option<&mut T> {
        let at = self.fx_stacked();
        self.unit_mut(ch, at)
    }

    /// Which built-in the effect window shows: its channel's own (None), or a stacked one.
    pub fn fx_stacked(&self) -> Option<usize> {
        self.fx_view.as_ref().and_then(|v| v.stacked)
    }

    /// Sets a channel's built-in (as `built_in` finds it), or takes it out with None: one of its
    /// others goes from the chain, its place with it.
    pub fn set_built_in<T: Stackable>(&mut self, ch: Channel, at: Option<usize>, v: Option<T>) {
        match (at, v) {
            (None, v) => {
                if let Some(own) = T::own_mut(self, ch) {
                    *own = v;
                }
            }
            (Some(k), Some(v)) => {
                if let Some(u) = self.chain_mut(ch).more.get_mut(k) {
                    *u = v.unit();
                }
            }
            (Some(k), None) => drop(self.chain_mut(ch).remove_more(k)),
        }
    }

    /// Another of a kind stacked on a channel, last before a limiter that's last (else last):
    /// its place among the others.
    pub fn stack_built_in<T: Stackable>(&mut self, ch: Channel, v: T) -> usize {
        self.chain_mut(ch).add_more(v.unit())
    }
}
