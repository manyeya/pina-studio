//! Stacks and auxes: made, gathered, flattened, and the modulators a stack shares.

use super::*;

impl Project {
    /// Adds an aux on the next free bus, as a send picking a new bus makes; returns the
    /// bus.
    /// Makes a summing stack of these tracks: a stack channel they all play into, and the tracks
    /// moved together under the first of them. Returns its bus, and where each
    /// track is now, by where it was.
    pub fn add_stack(&mut self, tracks: &[usize]) -> (u32, Vec<usize>) {
        let bus = self.add_aux();
        let n = self.auxes.iter().filter(|a| a.stack).count() + 1;
        let stack = self.auxes.last_mut().unwrap();
        stack.stack = true;
        stack.name = format!("Stack {n}");
        for &t in tracks {
            self.tracks[t].output = Some(bus);
        }
        (bus, self.gather(bus))
    }

    /// Moves the tracks playing into `bus` together, under the first of them, in their order;
    /// returns where each track is now, by where it was.
    pub fn gather(&mut self, bus: u32) -> Vec<usize> {
        let n = self.tracks.len();
        let members: Vec<usize> = (0..n)
            .filter(|&i| self.tracks[i].output == Some(bus))
            .collect();
        let order: Vec<usize> = (0..n)
            .flat_map(|i| match members.first() {
                Some(&first) if i == first => members.clone(),
                _ if members.contains(&i) => Vec::new(),
                _ => vec![i],
            })
            .collect();
        let mut old: Vec<Option<Track>> = std::mem::take(&mut self.tracks)
            .into_iter()
            .map(Some)
            .collect();
        self.tracks = order.iter().map(|&i| old[i].take().unwrap()).collect();
        let mut moved = vec![0; n];
        for (to, &from) in order.iter().enumerate() {
            moved[from] = to;
        }
        moved
    }

    /// Undoes a stack (Flatten Stack): its tracks play into the master again, and the
    /// stack goes, with the modulators it shared.
    pub fn flatten_stack(&mut self, bus: u32) {
        while self
            .auxes
            .iter()
            .any(|a| a.bus == bus && !a.modulators.is_empty())
        {
            self.remove_shared(bus, 0);
        }
        for t in &mut self.tracks {
            if t.output == Some(bus) {
                t.output = None;
            }
            t.sends.retain(|s| s.bus != bus);
        }
        self.auxes.retain(|a| a.bus != bus);
    }

    /// The stack a track plays into, if it plays into one.
    pub fn stack_of(&self, t: usize) -> Option<&Aux> {
        let bus = self.tracks.get(t)?.output?;
        self.auxes.iter().find(|a| a.bus == bus && a.stack)
    }

    /// A track's modulator's kind; for one its stack shares, the stack's (none if the track has
    /// left the stack, or the stack the modulator).
    pub fn mod_kind(&self, t: usize, k: usize) -> Option<&ModKind> {
        match &self.tracks.get(t)?.modulators.get(k)?.kind {
            ModKind::Shared { modulator } => {
                Some(&self.stack_of(t)?.modulators.get(*modulator as usize)?.kind)
            }
            kind => Some(kind),
        }
    }

    pub fn mod_kind_mut(&mut self, t: usize, k: usize) -> Option<&mut ModKind> {
        let shared = match self.tracks.get(t)?.modulators.get(k)?.kind {
            ModKind::Shared { modulator } => Some(modulator as usize),
            _ => None,
        };
        match shared {
            Some(s) => {
                let bus = self.stack_of(t)?.bus;
                let aux = self.auxes.iter_mut().find(|a| a.bus == bus)?;
                Some(&mut aux.modulators.get_mut(s)?.kind)
            }
            None => Some(&mut self.tracks[t].modulators[k].kind),
        }
    }

    /// A track's modulators as they move what's worked out ahead (a pattern's sounds): any its
    /// stack shares as the stack's, with the track's own routes.
    pub fn mods_of(&self, t: usize) -> Vec<Modulator> {
        let Some(track) = self.tracks.get(t) else {
            return Vec::new();
        };
        (0..track.modulators.len())
            .map(|k| Modulator {
                kind: self
                    .mod_kind(t, k)
                    .unwrap_or(&track.modulators[k].kind)
                    .clone(),
                routes: track.modulators[k].routes.clone(),
            })
            .collect()
    }

    /// A modulator shared by the stack on `bus`, which every track in it gets (to route as it
    /// likes); returns its place on the stack.
    pub fn add_shared(&mut self, bus: u32, kind: ModKind) -> Option<usize> {
        let aux = self.auxes.iter_mut().find(|a| a.bus == bus && a.stack)?;
        if aux.modulators.len() >= MAX_MODS {
            return None;
        }
        aux.modulators.push(Modulator {
            kind,
            routes: Vec::new(),
        });
        let s = aux.modulators.len() - 1;
        for t in self.tracks.iter_mut().filter(|t| t.output == Some(bus)) {
            if t.modulators.len() < MAX_MODS {
                t.modulators.push(Modulator {
                    kind: ModKind::Shared { modulator: s as u8 },
                    routes: Vec::new(),
                });
            }
        }
        Some(s)
    }

    /// Takes a shared modulator off its stack, and off every track sharing it (their lanes for it
    /// too); those after it move down a place.
    pub fn remove_shared(&mut self, bus: u32, s: usize) {
        let Some(aux) = self.auxes.iter_mut().find(|a| a.bus == bus) else {
            return;
        };
        if s >= aux.modulators.len() {
            return;
        }
        aux.modulators.remove(s);
        for t in self.tracks.iter_mut().filter(|t| t.output == Some(bus)) {
            let mut k = 0;
            while k < t.modulators.len() {
                match &mut t.modulators[k].kind {
                    ModKind::Shared { modulator } if *modulator as usize == s => {
                        t.modulators.remove(k);
                        t.automation.modulator_removed(k);
                        continue;
                    }
                    ModKind::Shared { modulator } if *modulator as usize > s => *modulator -= 1,
                    _ => {}
                }
                k += 1;
            }
        }
    }

    pub fn add_aux(&mut self) -> u32 {
        let bus = self.auxes.iter().map(|a| a.bus).max().unwrap_or(0) + 1;
        self.auxes.push(Aux {
            id: next_id(),
            bus,
            name: format!("Aux {bus}"),
            ..Default::default()
        });
        bus
    }
}
