//! Pattern regions: found, made, turned to and from MIDI, their length and rate.

use super::*;
use pina_engine::song::project::Note;

impl Workspace {
    /// The pattern region the pane shows: the selected one, else the selected track's under the
    /// playhead, else its first.
    pub fn pattern(&self) -> Option<At> {
        let p = &self.session.project;
        let is = |at: At| matches!(p.region(at).content, Content::Pattern(_));
        self.region.filter(|&at| is(at)).or_else(|| {
            let track = p.tracks.get(self.track)?;
            let tick = self.playhead();
            let mine: Vec<At> = (0..track.regions.len())
                .map(|r| (self.track, r))
                .filter(|&at| is(at))
                .collect();
            mine.iter()
                .copied()
                .find(|&at| {
                    let r = p.region(at);
                    r.start <= tick && tick < r.start + r.length
                })
                .or(mine.first().copied())
        })
    }

    /// Where a new region of `length` can go on a track: the bar the playhead is in, or the
    /// first bar after it clear of the regions there.
    pub(crate) fn free_bar(&self, track: usize, length: u64) -> u64 {
        let bar = self.session.project.bar().max(1);
        let mut start = self.playhead() / bar * bar;
        let regions = &self.session.project.tracks[track].regions;
        while let Some(r) = regions
            .iter()
            .find(|r| r.start < start + length && start < r.start + r.length)
        {
            start = (r.start + r.length).div_ceil(bar) * bar;
        }
        start
    }

    /// A new empty pattern region, a bar long, at the first free bar from the playhead on the
    /// selected instrument track; and opens it.
    pub fn new_pattern(&mut self, cx: &mut Context<Self>) {
        self.new_pattern_at(None, cx);
    }

    /// The same at a tick (snapped), from a lane's menu.
    pub fn new_pattern_at(&mut self, at: Option<u64>, cx: &mut Context<Self>) {
        let tracks = &self.session.project.tracks;
        if !tracks
            .get(self.track)
            .is_some_and(|t| t.instrument.is_some())
        {
            self.status = Some("Select an instrument track for the pattern.".into());
            cx.notify();
            return;
        }
        let bar = self.session.project.bar();
        let start = at.map_or_else(|| self.free_bar(self.track, bar), |t| self.snap(t as f64));
        let t = self.track;
        let pattern = self.starting_pattern(t);
        let length = (pattern.step * pattern.steps() as u64).max(1);
        self.edit(cx, |ws| {
            let regions = &mut ws.session.project.tracks[t].regions;
            regions.push(Region {
                start,
                length,
                content: Content::Pattern(pattern),
                ..Default::default()
            });
            ws.region = Some((t, regions.len() - 1));
            ws.bottom = Some(Bottom::Steps);
            ws.steps.row = 0;
        });
    }

    /// A MIDI region made a pattern region, as exactly as steps can hold it: a row a pitch, on
    /// the step its notes sit on (1/16, or finer, or triplets), each velocity kept and how long
    /// it holds as ties and a gate. As long as the region, or the shortest whole bars that loop
    /// through it unchanged.
    pub fn midi_to_pattern(&mut self, at: At, cx: &mut Context<Self>) {
        let r = self.session.project.region(at).clone();
        let Content::Midi { notes, .. } = &r.content else {
            return;
        };
        let bar = self.session.project.bar();
        let notes: Vec<_> = notes
            .iter()
            .filter(|n| n.start >= 0 && (n.start as u64) < r.length)
            .cloned()
            .collect();
        let (step, span) = pattern_fit(&notes, r.length, bar);
        let steps = ((span / step) as usize).clamp(1, MOST_STEPS);
        let rows = to_pattern(&notes, step, steps);
        if rows.is_empty() {
            self.status = Some("That region has no notes to make steps of.".into());
            cx.notify();
            return;
        }
        let mut pattern = Pattern::new(step, rows, 50.0);
        pattern.length = steps;
        pattern.tidy();
        self.edit(cx, |ws| {
            ws.session.project.tracks[at.0].regions[at.1].content = Content::Pattern(pattern);
            ws.region = Some(at);
            ws.bottom = Some(Bottom::Steps);
            ws.steps.row = 0;
        });
        let rate = match step {
            s if s == PPQ / 4 => "1/16".to_string(),
            s if s == PPQ / 6 => "1/16 triplets".to_string(),
            s if s == PPQ / 8 => "1/32".to_string(),
            s if s == PPQ / 12 => "1/32 triplets".to_string(),
            s => format!("{s} ticks"),
        };
        let bars = span / bar;
        self.status = Some(
            match steps * step as usize >= r.length as usize || span < r.length {
                true => format!(
                    "A pattern now: {steps} steps of {rate}, {bars} bar{}{}",
                    if bars == 1 { "" } else { "s" },
                    if span < r.length { ", looping" } else { "" }
                ),
                false => format!(
                    "A pattern now: its first {steps} steps of {rate} (the most a pattern holds)"
                ),
            },
        );
    }

    /// A pattern region made a MIDI region, to edit in the piano roll.
    pub fn pattern_to_midi(&mut self, at: At, cx: &mut Context<Self>) {
        let r = self.session.project.region(at).clone();
        let Content::Pattern(p) = &r.content else {
            return;
        };
        let notes = pina_engine::song::project::pattern_notes(p, r.length);
        self.edit(cx, |ws| {
            ws.session.project.tracks[at.0].regions[at.1].content = Content::Midi {
                notes,
                controls: Vec::new(),
            };
            ws.region = Some(at);
            ws.picked.clear();
        });
        self.show_bottom(Bottom::PianoRoll, cx);
    }
}

/// The most steps a converted pattern takes: sixteen bars of sixteenths.
const MOST_STEPS: usize = 256;

/// The step a region's notes sit on, and how long a pattern holds them: the coarsest of 1/16,
/// 1/16 triplets, 1/32 and 1/32 triplets every note starts on (1/16, with nudges, if none fit);
/// the fewest whole bars (1, 2, 4, 8) that repeat unchanged through the region, else all of it.
pub(crate) fn pattern_fit(notes: &[Note], length: u64, bar: u64) -> (u64, u64) {
    let step = [PPQ / 4, PPQ / 6, PPQ / 8, PPQ / 12]
        .into_iter()
        .find(|&s| notes.iter().all(|n| n.start.rem_euclid(s as i64) == 0))
        .unwrap_or(PPQ / 4);
    let total = length.div_ceil(bar).max(1) * bar;
    let key = |n: &Note, span: u64| (n.start as u64 % span, n.pitch, n.velocity, n.length);
    let repeats = |span: u64| {
        let mut first: Vec<_> = notes
            .iter()
            .filter(|n| (n.start as u64) < span)
            .map(|n| key(n, span))
            .collect();
        first.sort_unstable();
        (1..total / span).all(|k| {
            let mut again: Vec<_> = notes
                .iter()
                .filter(|n| (n.start as u64) / span == k)
                .map(|n| key(n, span))
                .collect();
            again.sort_unstable();
            again == first
        })
    };
    let span = [1, 2, 4, 8]
        .into_iter()
        .map(|b| b * bar)
        .find(|&s| s < total && total.is_multiple_of(s) && repeats(s))
        .unwrap_or(total);
    (step, span)
}
