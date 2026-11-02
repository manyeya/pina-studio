//! Kits as stacks: a kit track whose notes play its pads, each pad a track of its own (its own
//! sampler, channel and effects) playing its note, all summed into one stack for the kit's own
//! effects. A starter pattern on the kit track has a row a pad; its notes can be split out onto
//! the pads' own tracks.

use std::collections::HashMap;

use super::{
    Content, Instrument, Layering, Note, PPQ, Pad, Pattern, PatternRow, Project, Region, Sampler,
    Track, Zone,
};
use crate::song::project::patterns::VELOCITY;

/// How many sounds a pad can layer: as many as its sampler plays at once on a note.
pub const PAD_LAYERS: usize = 8;

/// A pad to make: its name, its sound (a sampler, its sample on the pad's note), its note, its
/// choke group, and the sixteenths of a bar a starter pattern has it on.
#[derive(Clone, Debug, Default)]
pub struct NewPad {
    pub name: String,
    pub sampler: Sampler,
    pub note: u8,
    pub choke: Option<u8>,
    pub steps: Vec<usize>,
}

/// What `add_kit` made: the stack's bus, the kit track, and each pad's track.
#[derive(Clone, Debug, PartialEq)]
pub struct KitMade {
    pub bus: u32,
    pub kit: usize,
    pub pads: Vec<usize>,
}

impl Project {
    /// A kit at track `index`: its kit track, a track per pad, all in a stack named `name`; with
    /// `pattern`, a starter pattern on the kit track at `at` (a bar, looping four), a row a pad.
    pub fn add_kit(
        &mut self,
        name: &str,
        pads: Vec<NewPad>,
        index: usize,
        at: u64,
        pattern: bool,
    ) -> KitMade {
        let index = index.min(self.tracks.len());
        let bar = self.bar();
        let rows: Vec<PatternRow> = pads
            .iter()
            .map(|p| {
                let mut row = PatternRow::note(p.note, 16);
                row.name = p.name.clone();
                for &s in p.steps.iter().filter(|&&s| s < 16) {
                    row.cells[s] = VELOCITY;
                }
                row
            })
            .collect();
        let mut kit = Track {
            name: name.to_string(),
            instrument: Some(Instrument::Kit),
            ..Default::default()
        };
        if pattern {
            kit.regions.push(Region {
                start: at,
                length: bar,
                looped: true,
                loop_length: Some(bar * 4),
                name: name.to_string(),
                content: Content::Pattern({
                    let mut p = Pattern {
                        step: PPQ / 4,
                        rows,
                        ..Default::default()
                    };
                    p.tidy();
                    p
                }),
                ..Default::default()
            });
        }
        self.tracks.insert(index, kit);
        self.assign_ids();
        let id = self.tracks[index].id;
        for (k, p) in pads.into_iter().enumerate() {
            self.tracks.insert(
                index + 1 + k,
                Track {
                    name: p.name,
                    instrument: Some(Instrument::Sampler(p.sampler)),
                    pad: Some(Pad {
                        kit: id,
                        note: p.note,
                        choke: p.choke,
                    }),
                    ..Default::default()
                },
            );
        }
        self.assign_ids();
        let n = self.tracks.len() - index;
        let members: Vec<usize> = (index..index + n)
            .filter(|&t| t == index || self.tracks[t].pad.is_some_and(|p| p.kit == id))
            .collect();
        let (bus, _) = self.add_stack(&members);
        if let Some(a) = self.auxes.iter_mut().find(|a| a.bus == bus) {
            a.name = name.to_string();
        }
        let kit = self.tracks.iter().position(|t| t.id == id).unwrap_or(index);
        KitMade {
            bus,
            kit,
            pads: self.pads_of(kit),
        }
    }

    /// The kit track a track belongs to: itself if it's one, else its pad's kit.
    pub fn kit_of(&self, t: usize) -> Option<usize> {
        let track = self.tracks.get(t)?;
        if track.instrument == Some(Instrument::Kit) {
            return Some(t);
        }
        let id = track.pad?.kit;
        self.tracks.iter().position(|k| k.id == id)
    }

    /// A kit's pad on `note`, by track index.
    pub fn pad_on(&self, kit: usize, note: u8) -> Option<usize> {
        self.pads_of(kit)
            .into_iter()
            .find(|&t| self.tracks[t].pad.is_some_and(|p| p.note == note))
    }

    /// A pad added to a kit on its note: a track after its last pad (in the kit's stack), and a
    /// row in the kit's patterns. Returns the pad's track.
    pub fn add_pad(&mut self, kit: usize, pad: NewPad) -> usize {
        let id = self.tracks[kit].id;
        let at = self.pads_of(kit).last().map_or(kit + 1, |&t| t + 1);
        let mut sampler = pad.sampler;
        for z in sampler.zones.iter_mut().filter(|z| z.high.is_none()) {
            z.note = pad.note;
        }
        let track = Track {
            name: pad.name.clone(),
            instrument: Some(Instrument::Sampler(sampler)),
            pad: Some(Pad {
                kit: id,
                note: pad.note,
                choke: pad.choke,
            }),
            output: self.tracks[kit].output,
            ..Default::default()
        };
        self.tracks.insert(at, track);
        self.assign_ids();
        for r in &mut self.tracks[kit].regions {
            let Content::Pattern(p) = &mut r.content else {
                continue;
            };
            if p.rows
                .iter()
                .any(|r| !r.is_automation() && r.pitch == pad.note)
            {
                continue;
            }
            let steps = p.rows.first().map_or(16, |r| r.cells.len());
            let mut row = PatternRow::note(pad.note, steps);
            row.name = pad.name.clone();
            p.rows.push(row);
        }
        at
    }

    /// Two of a kit's notes trade pads: each sound (its track, shaping, channel and all) moves to
    /// the other's note, and the kit's patterns and notes follow it, so the beat plays as it did,
    /// only laid out anew. A pad dropped on an empty note just moves there.
    pub fn swap_pads(&mut self, kit: usize, a: u8, b: u8) {
        if a == b {
            return;
        }
        let moves = [(self.pad_on(kit, a), b), (self.pad_on(kit, b), a)];
        for (t, to) in moves {
            let Some(track) = t.map(|t| &mut self.tracks[t]) else {
                continue;
            };
            let from = track.pad.map_or(to, |p| p.note);
            if let Some(p) = &mut track.pad {
                p.note = to;
            }
            if let Some(Instrument::Sampler(s)) = &mut track.instrument {
                let shift = |n: u8| (n as i32 + to as i32 - from as i32).clamp(0, 127) as u8;
                for z in &mut s.zones {
                    (z.note, z.high, z.root) =
                        (shift(z.note), z.high.map(shift), z.root.map(shift));
                }
            }
        }
        let swap = |n: u8| match n {
            n if n == a => b,
            n if n == b => a,
            n => n,
        };
        let track = &mut self.tracks[kit];
        let cells = track.cells.iter_mut().map(|c| &mut c.region);
        for r in track.regions.iter_mut().chain(cells) {
            match &mut r.content {
                Content::Pattern(p) => {
                    for row in p.rows.iter_mut().filter(|r| !r.is_automation()) {
                        row.pitch = swap(row.pitch);
                    }
                }
                Content::Midi { notes, .. } => {
                    for n in notes {
                        n.pitch = swap(n.pitch);
                    }
                }
                _ => {}
            }
        }
    }

    /// A pad's sound (its `layer`'s) replaced by `zone`'s sample, on the pad's note; what's the
    /// pad's rather than the sample's (its level, pan, reverse, choke and how it layers, its
    /// sampler's shaping, its channel and effects) kept. Its first layer names it `name`, and its
    /// row in the kit's patterns too.
    pub fn set_pad_sound(&mut self, pad: usize, layer: usize, mut zone: Zone, name: &str) {
        let track = &mut self.tracks[pad];
        let Some(p) = track.pad else {
            return;
        };
        let Some(Instrument::Sampler(s)) = &mut track.instrument else {
            return;
        };
        let layer = layer.min(s.zones.len().saturating_sub(1));
        let old = s.zones.get(layer).cloned().unwrap_or_default();
        (zone.note, zone.high, zone.root) = (p.note, None, None);
        (zone.gain_db, zone.pan, zone.reverse, zone.choke) =
            (old.gain_db, old.pan, old.reverse, old.choke);
        (zone.layer, zone.velocities, zone.velocity_fade) =
            (old.layer, old.velocities, old.velocity_fade);
        zone.gate = false;
        if layer > 0 {
            s.zones[layer] = zone;
            return;
        }
        zone.name = name.to_string();
        match s.zones.first_mut() {
            Some(z) => *z = zone,
            None => s.zones.push(zone),
        }
        let was = std::mem::replace(&mut track.name, name.to_string());
        if let Some(kit) = self.kit_of(pad) {
            for r in &mut self.tracks[kit].regions {
                let Content::Pattern(pt) = &mut r.content else {
                    continue;
                };
                for row in pt
                    .rows
                    .iter_mut()
                    .filter(|r| r.pitch == p.note && r.name == was)
                {
                    row.name = name.to_string();
                }
            }
        }
    }

    /// A sound layered on a pad (up to `PAD_LAYERS`), on its note, played as its others are
    /// (stacked, in turn or by velocity): which layer it is.
    pub fn add_pad_layer(&mut self, pad: usize, mut zone: Zone) -> Option<usize> {
        let note = self.tracks[pad].pad?.note;
        let Some(Instrument::Sampler(s)) = &mut self.tracks[pad].instrument else {
            return None;
        };
        if s.zones.len() >= PAD_LAYERS {
            return None;
        }
        let layering = s.layering().unwrap_or(Layering::Stack);
        (zone.note, zone.high, zone.root, zone.gate) = (note, None, None, false);
        s.zones.push(zone);
        s.set_layering(layering);
        Some(s.zones.len() - 1)
    }

    /// A pad's layer taken out, if it has others; the rest play as they did.
    pub fn remove_pad_layer(&mut self, pad: usize, layer: usize) -> bool {
        let Some(Instrument::Sampler(s)) = &mut self.tracks[pad].instrument else {
            return false;
        };
        if s.zones.len() < 2 || layer >= s.zones.len() {
            return false;
        }
        let layering = s.layering();
        s.zones.remove(layer);
        if let Some(l) = layering {
            s.set_layering(l);
        }
        true
    }

    /// A kit track's pads, by track index, in their order.
    pub fn pads_of(&self, kit: usize) -> Vec<usize> {
        let Some(id) = self.tracks.get(kit).map(|t| t.id) else {
            return Vec::new();
        };
        (0..self.tracks.len())
            .filter(|&t| self.tracks[t].pad.is_some_and(|p| p.kit == id))
            .collect()
    }

    /// A kit track's notes moved onto its pads: each pad's in a region of its own on its track,
    /// where they were (a pattern's row a pattern of its own); what's left stays on the kit track.
    /// Returns how many regions it made.
    pub fn split_kit(&mut self, kit: usize) -> usize {
        let notes: HashMap<u8, usize> = self
            .pads_of(kit)
            .into_iter()
            .filter_map(|t| Some((self.tracks[t].pad?.note, t)))
            .collect();
        let regions = std::mem::take(&mut self.tracks[kit].regions);
        let mut made = 0;
        let mut kept = Vec::new();
        for r in regions {
            match &r.content {
                Content::Pattern(p) => {
                    let mut left = p.clone();
                    left.rows.clear();
                    for row in &p.rows {
                        match notes.get(&row.pitch).filter(|_| !row.is_automation()) {
                            Some(&t) => {
                                let mut one = p.clone();
                                one.rows = vec![row.clone()];
                                self.tracks[t].regions.push(Region {
                                    content: Content::Pattern(one),
                                    link: None,
                                    ..r.clone()
                                });
                                made += 1;
                            }
                            None => left.rows.push(row.clone()),
                        }
                    }
                    if !left.rows.is_empty() {
                        kept.push(Region {
                            content: Content::Pattern(left),
                            ..r.clone()
                        });
                    }
                }
                Content::Midi {
                    notes: all,
                    controls,
                } => {
                    let mut by: HashMap<usize, Vec<Note>> = HashMap::new();
                    let mut left = Vec::new();
                    for n in all {
                        match notes.get(&n.pitch) {
                            Some(&t) => by.entry(t).or_default().push(*n),
                            None => left.push(*n),
                        }
                    }
                    for (t, mine) in by {
                        self.tracks[t].regions.push(Region {
                            content: Content::Midi {
                                notes: mine,
                                controls: Vec::new(),
                            },
                            link: None,
                            ..r.clone()
                        });
                        made += 1;
                    }
                    if !left.is_empty() {
                        kept.push(Region {
                            content: Content::Midi {
                                notes: left,
                                controls: controls.clone(),
                            },
                            ..r.clone()
                        });
                    }
                }
                _ => kept.push(r.clone()),
            }
        }
        self.tracks[kit].regions = kept;
        for t in &mut self.tracks {
            t.regions.sort_by_key(|r| r.start);
        }
        made
    }
}
