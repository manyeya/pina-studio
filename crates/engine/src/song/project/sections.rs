//! The song's sections, and time itself: a marker's section runs to the next marker (the last to
//! the song's end), and moving, copying, swapping or deleting one takes everything in it along
//! (every track's regions, cut at its edges, their automation, the markers and chords in it, the
//! tempo it's at), the rest of the song moving up to make room or close the gap.

use super::{AutoClip, ChordSpan, Content, Marker, Project, Region, TempoChange};
use crate::music::arrange::clip;

/// A stretch of the song lifted out: each track's regions (starts from its start), each track's
/// automation, its markers, chords and tempo changes (from its start), the tempo it starts at, and
/// how long it is.
#[derive(Clone, Debug)]
pub struct Span {
    pub length: u64,
    regions: Vec<Vec<Region>>,
    autos: Vec<AutoClip>,
    markers: Vec<Marker>,
    chords: Vec<ChordSpan>,
    tempos: Vec<TempoChange>,
    tempo: f64,
}

impl Project {
    /// Each section: from its marker to the next (the last to the song's end, a bar at least).
    pub fn sections(&self) -> Vec<(u64, u64)> {
        let bar = self.bar();
        let end = self.end().div_ceil(bar) * bar;
        self.markers
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let to = match (self.markers.get(i + 1), m.length) {
                    (Some(n), _) => n.tick,
                    (None, Some(l)) => m.tick + l,
                    (None, None) => end.max(m.tick + bar),
                };
                (m.tick, to.max(m.tick + 1))
            })
            .collect()
    }

    /// A looped region that plays on across `tick` becomes its loops, each a region, so it can be
    /// cut there; the loops of one that doesn't stop where they did.
    fn unloop_across(&mut self, tick: u64) {
        for t in 0..self.tracks.len() {
            for i in 0..self.tracks[t].regions.len() {
                let r = &self.tracks[t].regions[i];
                if !r.looped {
                    continue;
                }
                let end = self.loop_end((t, i));
                let r = self.tracks[t].regions[i].clone();
                if !(r.start < tick && end > tick) {
                    // Its loops stop where they do now, wherever its neighbours go.
                    self.tracks[t].regions[i].loop_length = Some(end - r.start);
                    continue;
                }
                self.tracks[t].regions[i].looped = false;
                self.tracks[t].regions[i].loop_length = None;
                let mut at = r.start + r.length;
                while at < end {
                    let copy = Region {
                        start: at,
                        looped: false,
                        loop_length: None,
                        ..r.clone()
                    };
                    // The last loop as far as it played.
                    if let Some(c) = clip(self, &copy, at, end) {
                        self.tracks[t].regions.push(c);
                    }
                    at += r.length.max(1);
                }
            }
        }
    }

    /// Every region across `tick` cut there (a take folder stays whole).
    fn cut_at(&mut self, tick: u64) {
        self.unloop_across(tick);
        for t in 0..self.tracks.len() {
            let regions = std::mem::take(&mut self.tracks[t].regions);
            let mut kept = Vec::new();
            for r in regions {
                let across = r.start < tick && r.start + r.length > tick;
                if !across || matches!(r.content, Content::Takes { .. }) {
                    kept.push(r);
                    continue;
                }
                kept.extend(clip(self, &r, r.start, tick));
                kept.extend(clip(self, &r, tick, r.start + r.length));
            }
            kept.sort_by_key(|r| r.start);
            self.tracks[t].regions = kept;
        }
    }

    /// A copy of `from..to`, everything in it.
    pub fn lift(&mut self, from: u64, to: u64) -> Span {
        self.cut_at(from);
        self.cut_at(to);
        let rel = |tick: u64| tick - from;
        Span {
            length: to - from,
            regions: self
                .tracks
                .iter()
                .map(|t| {
                    t.regions
                        .iter()
                        .filter(|r| r.start >= from && r.start < to)
                        .map(|r| Region {
                            start: rel(r.start),
                            ..r.clone()
                        })
                        .collect()
                })
                .collect(),
            autos: self
                .tracks
                .iter()
                .map(|t| t.automation.take(from, to))
                .collect(),
            markers: self
                .markers
                .iter()
                .filter(|m| m.tick >= from && m.tick < to)
                .map(|m| Marker {
                    tick: rel(m.tick),
                    ..m.clone()
                })
                .collect(),
            chords: self
                .chords
                .iter()
                .filter(|c| c.start < to && c.start + c.length > from)
                .map(|c| {
                    let start = c.start.max(from);
                    ChordSpan {
                        start: rel(start),
                        length: (c.start + c.length).min(to) - start,
                        chord: c.chord.clone(),
                    }
                })
                .collect(),
            tempos: self
                .tempos
                .iter()
                .filter(|c| c.tick > from && c.tick < to)
                .map(|c| TempoChange {
                    tick: rel(c.tick),
                    bpm: c.bpm,
                })
                .collect(),
            tempo: self.tempo_at(from),
        }
    }

    /// `length` ticks of nothing at `at`: everything from there on moves later (regions across
    /// it cut there first).
    pub fn insert_time(&mut self, at: u64, length: u64) {
        if length == 0 {
            return;
        }
        self.cut_at(at);
        let later = |tick: &mut u64| {
            if *tick >= at {
                *tick += length;
            }
        };
        for t in &mut self.tracks {
            t.regions.iter_mut().for_each(|r| later(&mut r.start));
            t.automation.shift(at, length as i64);
        }
        self.markers.iter_mut().for_each(|m| later(&mut m.tick));
        for c in &mut self.chords {
            later(&mut c.start);
        }
        // The tempo there carries on through the gap; what came after moves along.
        let held = self.tempo_at(at);
        self.tempos.iter_mut().for_each(|c| later(&mut c.tick));
        let after = self.tempo_at(at + length);
        if at > 0 && (after - held).abs() > 1e-9 {
            self.set_tempo_at(at + length, after);
        }
        if let Some(c) = &mut self.cycle {
            later(&mut c.start);
            later(&mut c.end);
        }
    }

    /// `from..to` taken out of the song, everything after it moving up to close the gap.
    pub fn remove_time(&mut self, from: u64, to: u64) {
        if to <= from {
            return;
        }
        self.cut_at(from);
        self.cut_at(to);
        let length = to - from;
        let inside = |tick: u64| tick >= from && tick < to;
        let earlier = |tick: &mut u64| {
            if *tick >= to {
                *tick -= length;
            }
        };
        let after = self.tempo_at(to);
        for t in &mut self.tracks {
            t.regions.retain(|r| !inside(r.start));
            t.regions.iter_mut().for_each(|r| earlier(&mut r.start));
            t.automation.erase(from, to);
            t.automation.shift(to, -(length as i64));
        }
        self.markers.retain(|m| !inside(m.tick));
        self.markers.iter_mut().for_each(|m| earlier(&mut m.tick));
        self.chords
            .retain(|c| !(c.start >= from && c.start + c.length <= to));
        for c in &mut self.chords {
            if c.start < from && c.start + c.length > from {
                c.length = (from - c.start) + (c.start + c.length).saturating_sub(to);
            } else if inside(c.start) {
                c.length = (c.start + c.length).saturating_sub(to);
                c.start = from;
            } else {
                earlier(&mut c.start);
            }
        }
        self.chords.retain(|c| c.length > 0);
        self.tempos.retain(|c| !inside(c.tick));
        self.tempos.iter_mut().for_each(|c| earlier(&mut c.tick));
        if (self.tempo_at(from) - after).abs() > 1e-9 {
            self.set_tempo_at(from, after);
        }
        if let Some(c) = &mut self.cycle {
            for t in [&mut c.start, &mut c.end] {
                if inside(*t) {
                    *t = from;
                } else {
                    earlier(t);
                }
            }
        }
    }

    /// A lifted stretch laid down at `at` (where there's room for it: inserted time).
    fn lay(&mut self, at: u64, span: &Span) {
        for (t, regions) in span.regions.iter().enumerate() {
            if let Some(track) = self.tracks.get_mut(t) {
                track.regions.extend(regions.iter().map(|r| Region {
                    start: r.start + at,
                    ..r.clone()
                }));
                track.regions.sort_by_key(|r| r.start);
            }
        }
        for (t, auto) in span.autos.iter().enumerate() {
            if let Some(track) = self.tracks.get_mut(t) {
                track.automation.put(at, auto);
            }
        }
        self.markers.extend(span.markers.iter().map(|m| Marker {
            tick: m.tick + at,
            ..m.clone()
        }));
        self.markers.sort_by_key(|m| m.tick);
        self.chords.extend(span.chords.iter().map(|c| ChordSpan {
            start: c.start + at,
            ..c.clone()
        }));
        self.chords.sort_by_key(|c| c.start);
        // Its tempo inside it; the tempo after it as it was.
        let mut changes = vec![TempoChange {
            tick: at,
            bpm: span.tempo,
        }];
        changes.extend(span.tempos.iter().map(|c| TempoChange {
            tick: c.tick + at,
            bpm: c.bpm,
        }));
        self.replace_tempos(at, at + span.length, &changes);
    }

    /// Section `i` moved to before section `before` (the number of sections: after the last),
    /// everything in it along, the song closing up behind it and making room for it.
    pub fn move_section(&mut self, i: usize, before: usize) -> Result<(), String> {
        let sections = self.sections();
        let &(from, to) = sections.get(i).ok_or("there's no such section")?;
        if before == i || before == i + 1 {
            return Ok(());
        }
        let dest = section_start(&sections, before)?;
        let span = self.lift(from, to);
        self.remove_time(from, to);
        let dest = if dest > from {
            dest - (to - from)
        } else {
            dest
        };
        self.insert_time(dest, span.length);
        self.lay(dest, &span);
        Ok(())
    }

    /// A copy of section `i` before section `before`, the song making room for it.
    pub fn copy_section(&mut self, i: usize, before: usize) -> Result<(), String> {
        let sections = self.sections();
        let &(from, to) = sections.get(i).ok_or("there's no such section")?;
        let dest = section_start(&sections, before)?;
        let span = self.lift(from, to);
        self.insert_time(dest, span.length);
        self.lay(dest, &span);
        Ok(())
    }

    /// Two sections change places.
    pub fn swap_sections(&mut self, a: usize, b: usize) -> Result<(), String> {
        let (a, b) = (a.min(b), a.max(b));
        if a == b {
            return Ok(());
        }
        // The later one before the earlier, then the earlier where the later was.
        self.move_section(b, a)?;
        self.move_section(a + 1, b + 1)
    }

    /// Section `i` taken out, everything in it, the song closing the gap.
    pub fn delete_section(&mut self, i: usize) -> Result<(), String> {
        let &(from, to) = self.sections().get(i).ok_or("there's no such section")?;
        self.remove_time(from, to);
        Ok(())
    }

    /// Section `i` emptied, its time kept: its regions and automation gone, its marker kept.
    pub fn clear_section(&mut self, i: usize) -> Result<(), String> {
        let &(from, to) = self.sections().get(i).ok_or("there's no such section")?;
        self.cut_at(from);
        self.cut_at(to);
        for t in &mut self.tracks {
            t.regions.retain(|r| !(r.start >= from && r.start < to));
            t.automation.erase(from, to);
        }
        Ok(())
    }

    /// A section named `name` after the last (or at the start), `bars` long.
    pub fn add_section(&mut self, name: &str, bars: u64) -> u64 {
        let tick = self.sections().last().map_or(0, |s| s.1);
        self.markers.push(Marker {
            tick,
            name: name.to_string(),
            length: Some(bars.max(1) * self.bar()),
        });
        self.markers.sort_by_key(|m| m.tick);
        tick
    }
}

/// Where section `before` starts (one past the last: where the last ends).
fn section_start(sections: &[(u64, u64)], before: usize) -> Result<u64, String> {
    match sections.get(before) {
        Some(s) => Ok(s.0),
        None if before == sections.len() => Ok(sections.last().map_or(0, |s| s.1)),
        None => Err("there's no such place among the sections".into()),
    }
}
