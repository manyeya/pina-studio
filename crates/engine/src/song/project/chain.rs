//! The order a channel's effects run in: its built-ins (EQ, compressor, saturator, limiter), any
//! more of them stacked (a second EQ after the saturator, a compressor before and after the
//! plugins), and its plugins, in any order. Saved as a list of `Fx`, each `Plugin` the next of
//! its inserts and each `More` the next of its other built-ins; an empty list is the built-ins
//! first, then the plugins, then the others, as it was before the order could change.

use super::*;

/// An effect in a channel's chain: one of its built-ins, the next of its plugins, or the next
/// of its other built-ins.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Fx {
    Eq,
    Comp,
    Sat,
    Plugin,
    Limit,
    More,
    /// What the stacked-only built-ins are: they've no own place on a channel, only `More`s.
    Reverb,
    Delay,
    Filter,
    Motion,
    Shaper,
    Gate,
    DeEsser,
    Stereo,
    Multiband,
    Pitch,
    TapeStop,
    Tune,
}

/// A built-in effect's settings, as one of a channel's others.
pub trait AsUnit: Copy {
    const KIND: Fx;
    fn of(u: &Unit) -> Option<Self>;
    fn of_mut(u: &mut Unit) -> Option<&mut Self>;
    fn unit(self) -> Unit;
}

macro_rules! units {
    ($($v:ident($t:ty)),* $(,)?) => {
        /// One more built-in effect on a channel, past its own EQ, compressor, saturator and
        /// limiter: any number of each, each where `Fx::More` puts it in the chain.
        // A few on a channel, kept in place and Copy as the channel's own are.
        #[allow(clippy::large_enum_variant)]
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
        #[serde(rename_all = "snake_case")]
        pub enum Unit {
            $($v($t)),*
        }

        impl Unit {
            /// Which built-in it is, as the chain names a channel's own.
            pub fn kind(&self) -> Fx {
                match self {
                    $(Unit::$v(_) => Fx::$v),*
                }
            }

            pub fn bypassed(&self) -> bool {
                match self {
                    $(Unit::$v(x) => x.bypass),*
                }
            }

            pub fn bypass_mut(&mut self) -> &mut bool {
                match self {
                    $(Unit::$v(x) => &mut x.bypass),*
                }
            }
        }

        $(impl AsUnit for $t {
            const KIND: Fx = Fx::$v;
            fn of(u: &Unit) -> Option<Self> {
                match u {
                    Unit::$v(x) => Some(*x),
                    _ => None,
                }
            }
            fn of_mut(u: &mut Unit) -> Option<&mut Self> {
                match u {
                    Unit::$v(x) => Some(x),
                    _ => None,
                }
            }
            fn unit(self) -> Unit {
                Unit::$v(self)
            }
        })*
    };
}

units!(
    Eq(Eq),
    Comp(Comp),
    Sat(Saturator),
    Limit(Limiter),
    Reverb(Reverb),
    Delay(Delay),
    Filter(FilterFx),
    Motion(Motion),
    Shaper(Shaper),
    Gate(Gate),
    DeEsser(DeEsser),
    Stereo(Stereo),
    Multiband(Multiband),
    Pitch(PitchShift),
    TapeStop(TapeStop),
    Tune(Tune),
);

const BUILT_INS: [Fx; 4] = [Fx::Eq, Fx::Comp, Fx::Sat, Fx::Limit];

/// The chain as it runs: `order` with every built-in in it once (one it hasn't goes where it
/// usually does: the EQ, compressor and saturator first, the limiter last), a `Plugin` for each
/// of `plugins` and a `More` for each of `more` (any more than it has go before a limiter
/// that's last, else last: the plugins, then the others).
pub fn chain(order: &[Fx], plugins: usize, more: usize) -> Vec<Fx> {
    let mut out = Vec::with_capacity(4 + plugins + more);
    let mut seen = [false; 4];
    let (mut n, mut m) = (0, 0);
    for &fx in order {
        match BUILT_INS.iter().position(|&b| b == fx) {
            Some(k) if !seen[k] => {
                seen[k] = true;
                out.push(fx);
            }
            Some(_) => {}
            None if fx == Fx::More => {
                if m < more {
                    m += 1;
                    out.push(fx);
                }
            }
            None if fx == Fx::Plugin && n < plugins => {
                n += 1;
                out.push(fx);
            }
            None => {}
        }
    }
    for k in (0..3).rev().filter(|&k| !seen[k]) {
        out.insert(0, BUILT_INS[k]);
    }
    let at = match out.last() {
        Some(Fx::Limit) => out.len() - 1,
        _ => out.len(),
    };
    out.splice(
        at..at,
        std::iter::repeat_n(Fx::Plugin, plugins - n).chain(std::iter::repeat_n(Fx::More, more - m)),
    );
    if !seen[3] {
        out.push(Fx::Limit);
    }
    out
}

/// A channel's effects, to change: its order, its plugins, its other built-ins and (a track's)
/// automation, which follows its plugins.
pub struct ChainMut<'a> {
    pub order: &'a mut Vec<Fx>,
    pub inserts: &'a mut Vec<PluginRef>,
    pub more: &'a mut Vec<Unit>,
    pub automation: Option<&'a mut Automation>,
    pub modulators: Option<&'a mut Vec<crate::dsp::modulate::Modulator>>,
}

impl ChainMut<'_> {
    /// What points at the stacked effects follows them: `place` says where each now is.
    fn stacked_moved(&mut self, place: impl Fn(u8) -> Option<u8> + Copy) {
        use crate::dsp::modulate::ModTarget;
        // A track's own compressor and saturator don't move with them.
        let place = |p: u8| if p >= OWN_COMP { Some(p) } else { place(p) };
        if let Some(a) = self.automation.as_deref_mut() {
            a.stacked_moved(place);
        }
        for m in self.modulators.iter_mut().flat_map(|m| m.iter_mut()) {
            m.routes.retain_mut(|r| {
                let ModTarget::Tool(k, knob) = r.target else {
                    return true;
                };
                let now = place(k);
                r.target = ModTarget::Tool(now.unwrap_or(k), knob);
                now.is_some()
            });
        }
    }

    /// Puts a plugin in insert slot `index` (past the end adds one, last in the chain), or takes
    /// it out with None.
    pub fn set_insert(&mut self, index: usize, plugin: Option<PluginRef>) -> Result<(), String> {
        match plugin {
            Some(mut p) if index < self.inserts.len() => {
                let was = &self.inserts[index];
                // The same plugin with other settings is the one running; another is new.
                match was.same_plugin(&p) {
                    true => p.instance = was.instance,
                    false => {
                        p.instance = next_id();
                        if let Some(a) = self.automation.as_deref_mut() {
                            a.forget(Which::Insert(index));
                        }
                    }
                }
                self.inserts[index] = p;
            }
            Some(mut p) => {
                if p.instance == 0 {
                    p.instance = next_id();
                }
                self.inserts.push(p);
            }
            None if index < self.inserts.len() => {
                if !self.order.is_empty() {
                    let mut c = chain(self.order, self.inserts.len(), self.more.len());
                    if let Some(at) = c
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| **f == Fx::Plugin)
                        .nth(index)
                        .map(|(at, _)| at)
                    {
                        c.remove(at);
                    }
                    *self.order = c;
                }
                self.inserts.remove(index);
                if let Some(a) = self.automation.as_deref_mut() {
                    a.insert_removed(index);
                }
            }
            None => return Err(format!("no insert {index} to remove")),
        }
        Ok(())
    }

    /// The place in `more` of the `nth` (from 0) other built-in of `kind`, in the order they run.
    pub fn nth_more(&self, kind: Fx, nth: usize) -> Option<usize> {
        self.more
            .iter()
            .enumerate()
            .filter(|(_, u)| u.kind() == kind)
            .nth(nth)
            .map(|(k, _)| k)
    }

    /// One more built-in, last before a limiter that's last (else last); its place in `more`.
    pub fn add_more(&mut self, unit: Unit) -> usize {
        if !self.order.is_empty() {
            let mut c = chain(self.order, self.inserts.len(), self.more.len());
            let at = match c.last() {
                Some(Fx::Limit) => c.len() - 1,
                _ => c.len(),
            };
            c.insert(at, Fx::More);
            *self.order = c;
        }
        self.more.push(unit);
        self.more.len() - 1
    }

    /// Takes out the other built-in at `k` in `more`, and its place in the chain.
    pub fn remove_more(&mut self, k: usize) -> Result<(), String> {
        if k >= self.more.len() {
            return Err(format!("no other effect {k}"));
        }
        if !self.order.is_empty() {
            let mut c = chain(self.order, self.inserts.len(), self.more.len());
            if let Some(at) = c
                .iter()
                .enumerate()
                .filter(|(_, f)| **f == Fx::More)
                .nth(k)
                .map(|(at, _)| at)
            {
                c.remove(at);
            }
            *self.order = c;
        }
        self.more.remove(k);
        let k = k as u8;
        self.stacked_moved(|p| match p.cmp(&k) {
            std::cmp::Ordering::Less => Some(p),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(p - 1),
        });
        Ok(())
    }

    /// Moves the effect at `from` in the chain to `to` (both places in `chain`); its plugins
    /// keep their settings and automation, its other built-ins their settings.
    pub fn move_fx(&mut self, from: usize, to: usize) -> Result<(), String> {
        let (n, m) = (self.inserts.len(), self.more.len());
        // Each place: what's there, and which insert (or other built-in) it is.
        let (mut k, mut j) = (0, 0);
        let mut c: Vec<(Fx, usize)> = chain(self.order, n, m)
            .into_iter()
            .map(|fx| match fx {
                Fx::Plugin => {
                    k += 1;
                    (fx, k - 1)
                }
                Fx::More => {
                    j += 1;
                    (fx, j - 1)
                }
                _ => (fx, usize::MAX),
            })
            .collect();
        if from >= c.len() || to >= c.len() {
            return Err(format!("the chain has {} effects", c.len()));
        }
        let moved = c.remove(from);
        c.insert(to, moved);
        // Which insert each place now holds, old index by new.
        let was: Vec<usize> = c
            .iter()
            .filter(|(f, _)| *f == Fx::Plugin)
            .map(|&(_, i)| i)
            .collect();
        let old = std::mem::take(self.inserts);
        *self.inserts = was.iter().map(|&i| old[i].clone()).collect();
        if let Some(a) = self.automation.as_deref_mut() {
            a.inserts_moved(&was);
        }
        let others: Vec<usize> = c
            .iter()
            .filter(|(f, _)| *f == Fx::More)
            .map(|&(_, i)| i)
            .collect();
        let old = std::mem::take(self.more);
        *self.more = others.iter().map(|&i| old[i]).collect();
        let now = |p: u8| {
            others
                .iter()
                .position(|&o| o == p as usize)
                .map(|n| n as u8)
        };
        let moved: Vec<Option<u8>> = (0..others.len() as u8).map(now).collect();
        let moved = &moved;
        self.stacked_moved(|p| moved.get(p as usize).copied().flatten());
        *self.order = c.into_iter().map(|(f, _)| f).collect();
        // The usual order is saved as none at all.
        if *self.order == chain(&[], n, m) {
            self.order.clear();
        }
        Ok(())
    }
}

impl Track {
    pub fn chain_mut(&mut self) -> ChainMut<'_> {
        ChainMut {
            order: &mut self.chain,
            inserts: &mut self.inserts,
            more: &mut self.more,
            automation: Some(&mut self.automation),
            modulators: Some(&mut self.modulators),
        }
    }
}

impl Aux {
    pub fn chain_mut(&mut self) -> ChainMut<'_> {
        ChainMut {
            order: &mut self.chain,
            inserts: &mut self.inserts,
            more: &mut self.more,
            automation: None,
            modulators: None,
        }
    }
}

impl Project {
    pub fn master_chain_mut(&mut self) -> ChainMut<'_> {
        ChainMut {
            order: &mut self.master_chain,
            inserts: &mut self.master_inserts,
            more: &mut self.master_more,
            automation: None,
            modulators: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(name: &str) -> PluginRef {
        PluginRef {
            path: format!("/{name}.vst3").into(),
            class: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_stacked_effects_lanes_and_routes_follow_it_and_go_with_it() {
        use crate::dsp::modulate::{ModKind, ModRoute, ModTarget, Modulator};
        let mut t = Track {
            more: vec![
                Unit::Gate(Gate::default()),
                Unit::Filter(FilterFx::default()),
                Unit::Shaper(Shaper::default()),
            ],
            ..Track::default()
        };
        for k in 0..3 {
            t.automation
                .points_mut(Param::Tool(k, 1))
                .push(AutoPoint::new(0, k as f32));
        }
        let route = |k| ModRoute {
            target: ModTarget::Tool(k, 1),
            depth: 0.5,
        };
        t.modulators = vec![Modulator {
            kind: ModKind::Macro { value: 0.0 },
            routes: vec![route(0), route(1), route(2)],
        }];
        let value = |t: &Track, k| {
            t.automation
                .points(Param::Tool(k, 1))
                .first()
                .map(|p| p.value)
        };
        // The filter to the front: gate and filter trade places, the shaper stays.
        let at: Vec<usize> = chain(&t.chain, 0, 3)
            .iter()
            .enumerate()
            .filter(|(_, f)| **f == Fx::More)
            .map(|(i, _)| i)
            .collect();
        t.chain_mut().move_fx(at[1], at[0]).unwrap();
        assert_eq!(t.more[0].kind(), Fx::Filter);
        assert_eq!((value(&t, 0), value(&t, 1)), (Some(1.0), Some(0.0)));
        assert_eq!(t.modulators[0].routes[0], route(1));
        // The filter out: its lane and route go, those after it come down a place.
        t.chain_mut().remove_more(0).unwrap();
        assert_eq!(
            (value(&t, 0), value(&t, 1), value(&t, 2)),
            (Some(0.0), Some(2.0), None)
        );
        assert_eq!(t.modulators[0].routes.len(), 2);
        assert_eq!(t.modulators[0].routes[0], route(0));
    }

    #[test]
    fn effects_move_anywhere_and_plugins_take_their_automation() {
        let mut t = Track::default();
        t.set_insert(0, Some(plugin("a"))).unwrap();
        t.set_insert(1, Some(plugin("b"))).unwrap();
        t.automation.plugins.push(PluginAuto {
            plugin: Which::Insert(1),
            param: 0,
            name: String::new(),
            points: Vec::new(),
        });
        assert_eq!(
            chain(&t.chain, 2, 0),
            [Fx::Eq, Fx::Comp, Fx::Sat, Fx::Plugin, Fx::Plugin, Fx::Limit]
        );
        // b to the front, then the compressor after it.
        t.chain_mut().move_fx(4, 0).unwrap();
        t.chain_mut().move_fx(2, 1).unwrap();
        assert_eq!(
            t.chain,
            [Fx::Plugin, Fx::Comp, Fx::Eq, Fx::Sat, Fx::Plugin, Fx::Limit]
        );
        assert_eq!(
            (t.inserts[0].class.as_str(), t.inserts[1].class.as_str()),
            ("b", "a")
        );
        assert_eq!(t.automation.plugins[0].plugin, Which::Insert(0));
        // Taking a plugin out takes its place in the chain with it.
        t.set_insert(0, None).unwrap();
        assert_eq!(t.chain, [Fx::Comp, Fx::Eq, Fx::Sat, Fx::Plugin, Fx::Limit]);
        assert!(t.automation.plugins.is_empty());
        // Another goes last; and back to the usual order saves none.
        t.set_insert(1, Some(plugin("c"))).unwrap();
        assert_eq!(chain(&t.chain, 2, 0)[4..], [Fx::Plugin, Fx::Limit]);
        t.chain_mut().move_fx(0, 1).unwrap();
        assert!(t.chain.is_empty());
    }

    #[test]
    fn built_ins_stack_anywhere_and_move_with_their_settings() {
        let mut t = Track::default();
        t.set_insert(0, Some(plugin("a"))).unwrap();
        // Another EQ and another compressor: after the plugin, before the limiter.
        let tight = Comp {
            ratio: 8.0,
            ..Comp::default()
        };
        t.chain_mut().add_more(Unit::Eq(Eq::default()));
        t.chain_mut().add_more(Unit::Comp(tight));
        assert_eq!(
            chain(&t.chain, 1, 2),
            [
                Fx::Eq,
                Fx::Comp,
                Fx::Sat,
                Fx::Plugin,
                Fx::More,
                Fx::More,
                Fx::Limit
            ]
        );
        assert_eq!(t.chain_mut().nth_more(Fx::Comp, 0), Some(1));
        assert_eq!(t.chain_mut().nth_more(Fx::Comp, 1), None);
        // The second compressor to the front: it goes first in the list too.
        t.chain_mut().move_fx(5, 0).unwrap();
        assert_eq!(t.more[0], Unit::Comp(tight));
        assert_eq!(
            chain(&t.chain, 1, 2),
            [
                Fx::More,
                Fx::Eq,
                Fx::Comp,
                Fx::Sat,
                Fx::Plugin,
                Fx::More,
                Fx::Limit
            ]
        );
        // Taken out, its place goes with it.
        t.chain_mut().remove_more(0).unwrap();
        assert_eq!(t.more, [Unit::Eq(Eq::default())]);
        assert_eq!(
            chain(&t.chain, 1, 1),
            [Fx::Eq, Fx::Comp, Fx::Sat, Fx::Plugin, Fx::More, Fx::Limit]
        );
        // Saved and read back as it was.
        let back: Track = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back.more, t.more);
    }

    #[test]
    fn a_chain_missing_or_repeating_effects_is_made_whole() {
        assert_eq!(
            chain(&[Fx::Sat, Fx::Plugin, Fx::Sat, Fx::Plugin], 1, 0),
            [Fx::Eq, Fx::Comp, Fx::Sat, Fx::Plugin, Fx::Limit]
        );
        // A chain saved before limiters: the limiter last, after its plugins.
        assert_eq!(
            chain(&[Fx::Plugin, Fx::Eq, Fx::Comp, Fx::Sat], 2, 0),
            [Fx::Plugin, Fx::Eq, Fx::Comp, Fx::Sat, Fx::Plugin, Fx::Limit]
        );
    }
}
