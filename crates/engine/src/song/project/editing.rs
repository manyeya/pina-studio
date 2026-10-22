//! Edits to regions and takes: tracks added, regions moved, trimmed, split, aliased and comped.

use super::*;

impl Project {
    pub fn region(&self, at: At) -> &Region {
        &self.tracks[at.0].regions[at.1]
    }

    /// A MIDI region's notes, if it is one.
    pub fn notes_mut(&mut self, at: At) -> Option<&mut Vec<Note>> {
        match &mut self.tracks.get_mut(at.0)?.regions.get_mut(at.1)?.content {
            Content::Midi { notes, .. } => Some(notes),
            Content::Audio { .. } | Content::Takes { .. } | Content::Pattern { .. } => None,
        }
    }

    /// A MIDI region's controller moves, if it is one.
    pub fn controls_mut(&mut self, at: At) -> Option<&mut Vec<ControlPoint>> {
        match &mut self.tracks.get_mut(at.0)?.regions.get_mut(at.1)?.content {
            Content::Midi { controls, .. } => Some(controls),
            Content::Audio { .. } | Content::Takes { .. } | Content::Pattern { .. } => None,
        }
    }

    /// Puts what was just recorded on a track, from `start` to `end`: as a take, if it began
    /// inside a region of its kind there (which becomes a take folder, the new take playing), or
    /// else as a region of its own. Returns where it went.
    pub fn add_take(&mut self, track: usize, start: u64, end: u64, content: Content) -> At {
        let regions = &mut self.tracks[track].regions;
        let over = regions.iter().position(|r| {
            r.content.is_midi() == content.is_midi()
                && (r.start..r.start + r.length).contains(&start)
        });
        let Some(i) = over else {
            regions.push(Region {
                start,
                length: (end - start).max(1),
                content,
                ..Default::default()
            });
            return (track, regions.len() - 1);
        };
        let r = &mut regions[i];
        if !matches!(r.content, Content::Takes { .. }) {
            let old = std::mem::replace(
                &mut r.content,
                Content::Takes {
                    takes: Vec::new(),
                    comp: Vec::new(),
                },
            );
            if let Content::Takes { takes, .. } = &mut r.content {
                takes.push(Take {
                    start: 0,
                    content: old,
                });
            }
        }
        let Content::Takes { takes, comp } = &mut r.content else {
            unreachable!()
        };
        takes.push(Take {
            start: (start - r.start) as i64,
            content,
        });
        comp.clear();
        r.length = r.length.max(end - r.start);
        (track, i)
    }

    /// Flex: stretches a plain audio region to `length` ticks, its audio slower or faster and its
    /// pitch kept, as option-dragging its end does. From a quarter to four times as long
    /// as it was recorded.
    pub fn flex(&mut self, at: At, length: u64) -> Result<(), String> {
        let r = &mut self.tracks[at.0].regions[at.1];
        let old = r.length;
        // MIDI and patterns: their notes or steps, longer or shorter; an alias its own then.
        if let Some(c) = r
            .content
            .stretched(length.max(1) as f64 / old.max(1) as f64)
        {
            r.content = c;
            r.length = length.max(1);
            r.link = None;
            return Ok(());
        }
        let Content::Audio { stretch, warps, .. } = &mut r.content else {
            return Err("a take folder doesn't stretch: flatten it first".into());
        };
        // The stretch that fills `length`, within limits, and the length that gives; warp
        // markers stretch with it.
        let wanted = *stretch * length.max(1) as f64 / old as f64;
        let ratio = wanted.clamp(0.25, 4.0);
        for w in warps.iter_mut() {
            w[1] = (w[1] as f64 * ratio / *stretch).round() as u64;
        }
        *stretch = ratio;
        r.length = (length.max(1) as f64 * ratio / wanted).round() as u64;
        Ok(())
    }

    /// Comps a take folder: `take` plays from `from` to `to` (ticks from the region's start).
    pub fn comp(&mut self, at: At, from: u64, to: u64, take: usize) -> Result<(), String> {
        let r = &mut self.tracks[at.0].regions[at.1];
        let length = r.length;
        let Content::Takes { takes, comp } = &mut r.content else {
            return Err("that region isn't a take folder".into());
        };
        if take >= takes.len() {
            return Err(format!(
                "the folder has no take {take}: it has {}",
                takes.len()
            ));
        }
        let (from, to) = (from.min(length), to.min(length));
        if from >= to {
            return Err("comp a range: `from` before `to`, inside the region".into());
        }
        let now = comp_segments(comp, takes.len(), length);
        let left = now
            .iter()
            .filter(|s| s.0 < from)
            .map(|&(a, b, t)| (a, b.min(from), t));
        let right = now
            .iter()
            .filter(|s| s.1 > to)
            .map(|&(a, b, t)| (a.max(to), b, t));
        let mut all: Vec<_> = left.chain([(from, to, take)]).chain(right).collect();
        all.sort_by_key(|s| s.0);
        *comp = pieces(all);
        Ok(())
    }

    /// Replaces a take folder with what its comp plays: one MIDI region, or an audio region per
    /// piece. Returns where they went.
    pub fn flatten(&mut self, at: At) -> Vec<At> {
        let r = self.region(at).clone();
        let Content::Takes { takes, comp } = &r.content else {
            return vec![at];
        };
        let rate = self.sample_rate as f64;
        let segments = comp_segments(comp, takes.len(), r.length);
        let mut out = Vec::new();
        if r.content.is_midi() {
            let (mut notes, mut controls) = (Vec::new(), Vec::new());
            for &(a, b, t) in &segments {
                let take = &takes[t];
                if let Some(n) = take.content.notes((b as i64 - take.start).max(0) as u64) {
                    let inside = |n: &&Note| (a as i64..b as i64).contains(&(take.start + n.start));
                    notes.extend(n.iter().filter(inside).map(|n| Note {
                        start: take.start + n.start,
                        ..*n
                    }));
                }
                // Each piece's controllers, as they stand where it begins.
                if let Content::Midi { controls: c, .. } = &take.content {
                    let moved = c.iter().map(|c| ControlPoint {
                        start: take.start + c.start,
                        ..*c
                    });
                    controls.extend(chase(moved, a as i64, b as i64));
                }
            }
            out.push(Region {
                start: r.start,
                length: r.length,
                content: Content::Midi { notes, controls },
                ..Default::default()
            });
        } else {
            for &(a, b, t) in &segments {
                let take = &takes[t];
                let from = (a as i64).max(take.start);
                if let (
                    Content::Audio {
                        offset, stretch, ..
                    },
                    true,
                ) = (&take.content, from < b as i64)
                {
                    let skip = self.samples((from - take.start) as u64, rate);
                    let mut content = take.content.clone();
                    if let Content::Audio { offset: o, .. } = &mut content {
                        *o = offset + (skip as f64 / stretch) as u64;
                    }
                    out.push(Region {
                        start: r.start + from as u64,
                        length: b - from as u64,
                        content,
                        ..Default::default()
                    });
                }
            }
        }
        let regions = &mut self.tracks[at.0].regions;
        regions.remove(at.1);
        let first = regions.len();
        regions.extend(out);
        (first..regions.len()).map(|i| (at.0, i)).collect()
    }

    /// Adds a track at `index` and returns its id.
    pub fn add_track(&mut self, index: usize, mut track: Track) -> u64 {
        track.id = next_id();
        let id = track.id;
        self.tracks.insert(index.min(self.tracks.len()), track);
        id
    }

    /// Moves a region to `start` on `track`, or keeps it on its own track if `track` can't play
    /// it (MIDI goes on instrument tracks, audio on audio tracks). Returns where it is now.
    pub fn move_region(&mut self, at: At, track: usize, start: u64) -> At {
        let midi = self.region(at).content.is_midi();
        let track = if self.tracks[track].instrument.is_some() == midi {
            track
        } else {
            at.0
        };
        let mut r = self.tracks[at.0].regions.remove(at.1);
        r.start = start;
        self.tracks[track].regions.push(r);
        (track, self.tracks[track].regions.len() - 1)
    }

    /// Moves a region's start to `start`, leaving its content where it is on the timeline. An
    /// audio region can't start before its file does.
    pub fn trim_start(&mut self, at: At, start: u64) {
        let r = self.region(at);
        let end = r.start + r.length;
        let mut start = start.min(end - 1);
        if let Content::Audio {
            offset, stretch, ..
        } = r.content
        {
            start = start.max(
                r.start
                    .saturating_sub(self.ticks((offset as f64 * stretch) as u64)),
            );
        }
        let (old, rate) = (r.start, self.sample_rate as f64);
        let moved = if start >= old {
            self.samples(start - old, rate) as i64
        } else {
            -(self.samples(old - start, rate) as i64)
        };
        let r = &mut self.tracks[at.0].regions[at.1];
        match &mut r.content {
            // Its notes move, so an alias trimmed at the start becomes a region of its own.
            Content::Midi { notes, controls } => {
                r.link = None;
                for n in notes {
                    n.start -= start as i64 - old as i64;
                }
                for c in controls {
                    c.start -= start as i64 - old as i64;
                }
            }
            Content::Audio {
                offset, stretch, ..
            } => *offset = (*offset as f64 + moved as f64 / *stretch).max(0.0) as u64,
            // A pattern starts over from the region's start.
            Content::Pattern { .. } => {}
            // Takes keep their place on the timeline; the comp keeps its, from the new start.
            Content::Takes { takes, comp } => {
                let delta = start as i64 - old as i64;
                let segments = comp_segments(comp, takes.len(), end - old);
                for t in takes {
                    t.start -= delta;
                }
                let shifted =
                    segments
                        .into_iter()
                        .filter(|s| s.1 as i64 > delta)
                        .map(|(a, b, t)| {
                            (
                                (a as i64 - delta).max(0) as u64,
                                (b as i64 - delta) as u64,
                                t,
                            )
                        });
                *comp = pieces(shifted);
            }
        }
        // Its loops stop where they did.
        if let Some(l) = &mut r.loop_length {
            *l = (old + *l).saturating_sub(start);
        }
        r.start = start;
        r.length = end - start;
    }

    /// Moves a region's end to `end`.
    pub fn trim_end(&mut self, at: At, end: u64) {
        let r = &mut self.tracks[at.0].regions[at.1];
        r.length = end.max(r.start + 1) - r.start;
    }

    /// Makes an alias of a MIDI or pattern region at `start` on `track`: a region linked to it,
    /// so editing either's notes or steps edits both. Returns where it went.
    pub fn alias(&mut self, at: At, track: usize, start: u64) -> Result<At, String> {
        let r = self.region(at);
        if !matches!(r.content, Content::Midi { .. } | Content::Pattern { .. }) {
            return Err("only MIDI and pattern regions have aliases".into());
        }
        if self.tracks[track].instrument.is_none() {
            return Err("an alias goes on an instrument track".into());
        }
        let link = match r.link {
            Some(l) => l,
            None => {
                let next = self
                    .tracks
                    .iter()
                    .flat_map(|t| &t.regions)
                    .filter_map(|r| r.link)
                    .max()
                    .map_or(1, |l| l + 1);
                self.tracks[at.0].regions[at.1].link = Some(next);
                next
            }
        };
        let alias = Region {
            start,
            link: Some(link),
            ..self.region(at).clone()
        };
        self.tracks[track].regions.push(alias);
        Ok((track, self.tracks[track].regions.len() - 1))
    }

    /// Makes an alias a region of its own, its notes copied.
    pub fn unlink(&mut self, at: At) {
        self.tracks[at.0].regions[at.1].link = None;
    }

    /// What each group of aliases plays now, for `sync_links`.
    pub fn link_contents(&self) -> HashMap<u64, Content> {
        let mut out = HashMap::new();
        for r in self.tracks.iter().flat_map(|t| &t.regions) {
            if let Some(l) = r.link {
                out.entry(l).or_insert_with(|| r.content.clone());
            }
        }
        out
    }

    /// Keeps aliases alike after an edit: in each group, the region whose content changed since
    /// `last` (the groups' contents after the previous call) is copied to the others. A region
    /// left on its own is no longer an alias.
    pub fn sync_links(&mut self, last: &mut HashMap<u64, Content>) {
        let mut groups: BTreeMap<u64, Vec<At>> = BTreeMap::new();
        for (t, track) in self.tracks.iter().enumerate() {
            for (r, region) in track.regions.iter().enumerate() {
                if let Some(l) = region.link {
                    groups.entry(l).or_default().push((t, r));
                }
            }
        }
        last.retain(|l, _| groups.contains_key(l));
        for (link, members) in groups {
            if members.len() == 1 {
                let (t, r) = members[0];
                self.tracks[t].regions[r].link = None;
                last.remove(&link);
                continue;
            }
            let changed = last
                .get(&link)
                .and_then(|c| {
                    members
                        .iter()
                        .copied()
                        .find(|&at| self.region(at).content != *c)
                })
                .unwrap_or(members[0]);
            let content = self.region(changed).content.clone();
            for &(t, r) in members.iter().filter(|&&at| at != changed) {
                let other = &mut self.tracks[t].regions[r].content;
                if *other != content {
                    *other = content.clone();
                }
            }
            last.insert(link, content);
        }
    }

    /// Splits a region at `tick`, if that's inside it, and returns where the second half went.
    /// Notes stay with the half they start in.
    pub fn split_region(&mut self, at: At, tick: u64) -> Option<At> {
        let r = self.region(at);
        if tick <= r.start || tick >= r.start + r.length {
            return None;
        }
        let first = tick - r.start;
        let skip = self.samples(first, self.sample_rate as f64);
        let r = &mut self.tracks[at.0].regions[at.1];
        let mut second = r.clone();
        second.start = tick;
        second.length = r.length - first;
        r.length = first;
        match (&mut r.content, &mut second.content) {
            // Each half keeps only its notes, so neither is an alias any more.
            (
                Content::Midi {
                    notes: a,
                    controls: ca,
                },
                Content::Midi {
                    notes: b,
                    controls: cb,
                },
            ) => {
                r.link = None;
                second.link = None;
                a.retain(|n| n.start < first as i64);
                b.retain(|n| n.start >= first as i64);
                for n in b {
                    n.start -= first as i64;
                }
                // The second half starts with each controller where the first left it.
                ca.retain(|c| c.start < first as i64);
                *cb = chase(cb.iter().copied(), first as i64, i64::MAX)
                    .map(|c| ControlPoint {
                        start: c.start - first as i64,
                        ..c
                    })
                    .collect();
            }
            (
                _,
                Content::Audio {
                    offset, stretch, ..
                },
            ) => *offset += (skip as f64 / *stretch) as u64,
            (Content::Pattern { .. }, _) => {}
            (
                Content::Takes { takes, comp: left },
                Content::Takes {
                    takes: moved,
                    comp: right,
                },
            ) => {
                let segments = comp_segments(left, takes.len(), first + second.length);
                *left = pieces(segments.iter().copied().filter(|s| s.0 < first));
                let after = segments.into_iter().filter(|s| s.1 > first);
                *right = pieces(after.map(|(a, b, t)| (a.max(first) - first, b - first, t)));
                for t in moved {
                    t.start -= first as i64;
                }
            }
            _ => unreachable!(),
        }
        self.tracks[at.0].regions.push(second);
        Some((at.0, self.tracks[at.0].regions.len() - 1))
    }
}
