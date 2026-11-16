//! Flipping a chopped sample, as a beatmaker does on an MPC: the MIDI region that plays a
//! sliced sampler's chops (a note a chop, its pitch which), rearranged. Chops swapped, one put in
//! another's place, stuttered, silenced, played backwards; the whole flipped into a new order,
//! turned round, played at half or double time, or put back as it was.

use super::*;

/// A sliced sampler's chops: its zones' notes, in order.
pub fn chop_notes(s: &Sampler) -> Vec<u8> {
    let mut notes: Vec<u8> = s.zones.iter().map(|z| z.note).collect();
    notes.sort_unstable();
    notes.dedup();
    notes
}

/// A small, steady random number source: each flip from its seed, the same every time.
fn next(seed: &mut u64) -> f64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed >> 11) as f64 / (1u64 << 53) as f64
}

impl Project {
    /// The MIDI region at `at`'s notes, in the order they play (indices into them).
    pub fn chop_order(&self, at: At) -> Vec<usize> {
        let Content::Midi { notes, .. } = &self.region(at).content else {
            return Vec::new();
        };
        let mut order: Vec<usize> = (0..notes.len()).collect();
        order.sort_by_key(|&i| (notes[i].start, notes[i].pitch));
        order
    }

    fn chop_notes_mut(&mut self, at: At) -> Option<&mut Vec<Note>> {
        match &mut self.tracks[at.0].regions[at.1].content {
            Content::Midi { notes, .. } => Some(notes),
            _ => None,
        }
    }

    /// The `a`th and `b`th chops (in the order they play) trade places: each plays the other's
    /// sound where it is.
    pub fn swap_chops(&mut self, at: At, a: usize, b: usize) {
        let order = self.chop_order(at);
        let (Some(&a), Some(&b)) = (order.get(a), order.get(b)) else {
            return;
        };
        if let Some(notes) = self.chop_notes_mut(at) {
            let (pa, pb) = (notes[a].pitch, notes[b].pitch);
            (notes[a].pitch, notes[b].pitch) = (pb, pa);
        }
    }

    /// The `i`th chop plays the chop on `note` instead.
    pub fn set_chop(&mut self, at: At, i: usize, note: u8) {
        let order = self.chop_order(at);
        if let (Some(&k), Some(notes)) = (order.get(i), self.chop_notes_mut(at)) {
            notes[k].pitch = note;
        }
    }

    /// The `i`th chop played `times` times in its place, each as long: a stutter.
    pub fn stutter_chop(&mut self, at: At, i: usize, times: u64) {
        let order = self.chop_order(at);
        let (Some(&k), Some(notes)) = (order.get(i), self.chop_notes_mut(at)) else {
            return;
        };
        let n = notes[k].clone();
        let times = times.clamp(1, n.length.max(1));
        let each = (n.length / times).max(1);
        notes[k].length = each;
        for t in 1..times {
            notes.push(Note {
                start: n.start + (t * each) as i64,
                length: each,
                ..n.clone()
            });
        }
    }

    /// The `i`th chop taken out: silence in its place.
    pub fn silence_chop(&mut self, at: At, i: usize) {
        let order = self.chop_order(at);
        if let (Some(&k), Some(notes)) = (order.get(i), self.chop_notes_mut(at)) {
            notes.remove(k);
        }
    }

    /// The `i`th chop played backwards: its sound, reversed, on a note of its own in the
    /// region's sampler (the next free one; the same one, if it was reversed before), and the
    /// chop on it. The note it's on now.
    pub fn reverse_chop(&mut self, at: At, i: usize) -> Option<u8> {
        let order = self.chop_order(at);
        let k = *order.get(i)?;
        let note = match &self.region(at).content {
            Content::Midi { notes, .. } => notes[k].pitch,
            _ => return None,
        };
        let Some(Instrument::Sampler(s)) = &mut self.tracks[at.0].instrument else {
            return None;
        };
        let zone = s.zones.iter().find(|z| z.note == note)?.clone();
        let flipped = Zone {
            reverse: !zone.reverse,
            ..zone.clone()
        };
        let same = |z: &Zone| {
            z.file == flipped.file
                && z.start == flipped.start
                && z.end == flipped.end
                && z.reverse == flipped.reverse
        };
        let to = match s.zones.iter().find(|z| same(z)) {
            Some(z) => z.note,
            None => {
                let used = chop_notes(s);
                let free = (0..=127u8).find(|n| *n > note && !used.contains(n))?;
                s.zones.push(Zone {
                    note: free,
                    high: None,
                    name: format!("{} reversed", zone.name),
                    ..flipped
                });
                free
            }
        };
        if let Some(notes) = self.chop_notes_mut(at) {
            notes[k].pitch = to;
        }
        Some(to)
    }

    /// A new flip: each chop's place given a chop from the sampler's, as a beatmaker would: the
    /// first chop (the one) back on most downbeats, chops said again and answered, the rest
    /// picked anew. The same `seed`, the same flip.
    pub fn flip_chops(&mut self, at: At, seed: u64) {
        let Some(Instrument::Sampler(s)) = &self.tracks[at.0].instrument else {
            return;
        };
        let chops = chop_notes(s);
        let (Some(&one), true) = (chops.first(), chops.len() > 1) else {
            return;
        };
        let order = self.chop_order(at);
        let bar = self.bar().max(1) as i64;
        let beat = self.beat().max(1) as i64;
        let start = self.region(at).start as i64;
        let mut seed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let Some(notes) = self.chop_notes_mut(at) else {
            return;
        };
        let mut picked: Vec<u8> = Vec::with_capacity(order.len());
        for &k in &order {
            let r = next(&mut seed);
            let any = chops[(next(&mut seed) * chops.len() as f64) as usize % chops.len()];
            let at_bar = (start + notes[k].start).rem_euclid(bar) == 0;
            let on_beat = (start + notes[k].start).rem_euclid(beat) == 0;
            let chop = match picked.len() {
                0 => one,
                _ if at_bar && r < 0.7 => one,
                n if r < 0.25 => picked[n - 1],
                n if n >= 2 && r < 0.45 => picked[n - 2],
                _ if on_beat && r < 0.55 => chops[1.min(chops.len() - 1)],
                _ => any,
            };
            picked.push(chop);
            notes[k].pitch = chop;
        }
    }

    /// The chops turned round: the last played first.
    pub fn reverse_chop_order(&mut self, at: At) {
        let order = self.chop_order(at);
        let Some(notes) = self.chop_notes_mut(at) else {
            return;
        };
        let pitches: Vec<u8> = order.iter().map(|&k| notes[k].pitch).collect();
        for (&k, &p) in order.iter().zip(pitches.iter().rev()) {
            notes[k].pitch = p;
        }
    }

    /// The chops in their first order: the `n`th to play, the `n`th chop of `chops`.
    pub fn chops_in_order(&mut self, at: At, chops: &[u8]) {
        let order = self.chop_order(at);
        let Some(notes) = self.chop_notes_mut(at) else {
            return;
        };
        for (n, &k) in order.iter().enumerate() {
            if let Some(&c) = chops.get(n % chops.len().max(1)) {
                notes[k].pitch = c;
            }
        }
    }

    /// The chops at half time (`slower`), each twice as long and as far apart, what goes past
    /// the region's end left out; or double time, each half as long, the whole played twice to
    /// fill the region.
    pub fn time_chops(&mut self, at: At, slower: bool) {
        let length = self.region(at).length as i64;
        let Some(notes) = self.chop_notes_mut(at) else {
            return;
        };
        match slower {
            true => {
                for n in notes.iter_mut() {
                    n.start *= 2;
                    n.length *= 2;
                }
                notes.retain(|n| n.start < length);
            }
            false => {
                for n in notes.iter_mut() {
                    n.start /= 2;
                    n.length = (n.length / 2).max(1);
                }
                let again: Vec<Note> = notes
                    .iter()
                    .map(|n| Note {
                        start: n.start + length / 2,
                        ..n.clone()
                    })
                    .collect();
                notes.extend(again);
            }
        }
    }
}

/// A sliced sampler's chops in the order they run through its sample: each its zone's index,
/// note, and where it starts and ends (shares of the sample). Only the first zone's file's, not
/// reversed.
pub fn cuts_of(s: &Sampler) -> Vec<(usize, u8, f32, f32)> {
    let Some(file) = s.zones.first().map(|z| &z.file) else {
        return Vec::new();
    };
    let mut chops: Vec<(usize, u8, f32, f32)> = s
        .zones
        .iter()
        .enumerate()
        .filter(|(_, z)| &z.file == file && !z.reverse)
        .map(|(i, z)| (i, z.note, z.start, z.end.unwrap_or(1.0)))
        .collect();
    chops.sort_by(|a, b| a.2.total_cmp(&b.2));
    chops
}

impl Project {
    /// Every MIDI region on track `t`'s notes, changed by `f`.
    fn each_note(&mut self, t: usize, mut f: impl FnMut(&mut Vec<Note>)) {
        for r in &mut self.tracks[t].regions {
            if let Content::Midi { notes, .. } = &mut r.content {
                f(notes);
            }
        }
    }

    /// Track `t`'s sliced sampler cut at `share` of its sample, by hand: the chop there in two,
    /// the second on the note after the first's (the chops over it each a note up), and each
    /// note that played the chop in two as well, so what plays doesn't change. The new chop's
    /// note.
    pub fn cut_chop(&mut self, t: usize, share: f32) -> Option<u8> {
        let Some(Instrument::Sampler(s)) = &self.tracks.get(t)?.instrument else {
            return None;
        };
        let chops = cuts_of(s);
        let &(zone, note, a, b) = chops
            .iter()
            .find(|c| share > c.2 + 0.002 && share < c.3 - 0.002)?;
        if s.zones.iter().any(|z| z.note == 127) {
            return None;
        }
        let Some(Instrument::Sampler(s)) = &mut self.tracks[t].instrument else {
            return None;
        };
        for z in &mut s.zones {
            if z.note > note {
                z.note += 1;
            }
        }
        let second = Zone {
            note: note + 1,
            name: format!("Slice {}", s.zones.len() + 1),
            start: share,
            end: (b < 1.0).then_some(b),
            ..s.zones[zone].clone()
        };
        s.zones[zone].end = Some(share);
        s.zones.insert(zone + 1, second);
        let part = (share - a) / (b - a).max(1e-6);
        self.each_note(t, |notes| {
            let mut more = Vec::new();
            for n in notes.iter_mut() {
                if n.pitch > note {
                    n.pitch += 1;
                } else if n.pitch == note {
                    let first = ((n.length as f32 * part).round() as u64).clamp(1, n.length);
                    if first < n.length {
                        more.push(Note {
                            start: n.start + first as i64,
                            length: n.length - first,
                            pitch: note + 1,
                            ..n.clone()
                        });
                        n.length = first;
                    }
                }
            }
            notes.extend(more);
        });
        Some(note + 1)
    }

    /// Track `t`'s sliced sampler's `k`th cut (where its `k`th chop starts, from 1) moved to
    /// `share` of its sample, no further than its neighbours.
    pub fn move_cut(&mut self, t: usize, k: usize, share: f32) {
        let Some(Instrument::Sampler(s)) = &mut self.tracks[t].instrument else {
            return;
        };
        let chops = cuts_of(s);
        let (Some(&(before, ..)), Some(&(after, _, _, b))) =
            (chops.get(k.wrapping_sub(1)), chops.get(k))
        else {
            return;
        };
        let lo = chops[k - 1].2 + 0.002;
        let at = share.clamp(lo, (b - 0.002).max(lo));
        s.zones[before].end = Some(at);
        s.zones[after].start = at;
    }

    /// Track `t`'s sliced sampler's `k`th cut taken out: its `k`th chop joined to the one
    /// before (the chops over it each a note down), what played it playing that.
    pub fn join_chops(&mut self, t: usize, k: usize) {
        let Some(Instrument::Sampler(s)) = &mut self.tracks[t].instrument else {
            return;
        };
        let chops = cuts_of(s);
        let (Some(&(before, keep, ..)), Some(&(gone, note, _, b))) =
            (chops.get(k.wrapping_sub(1)), chops.get(k))
        else {
            return;
        };
        s.zones[before].end = (b < 1.0).then_some(b);
        s.zones.remove(gone);
        for z in &mut s.zones {
            if z.note > note {
                z.note -= 1;
            }
        }
        let keep = if keep > note { keep - 1 } else { keep };
        self.each_note(t, |notes| {
            for n in notes.iter_mut() {
                if n.pitch == note {
                    n.pitch = keep;
                } else if n.pitch > note {
                    n.pitch -= 1;
                }
            }
        });
    }
}
