//! Pattern regions: found, made, turned to and from MIDI, their length and rate.

use super::*;

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

    /// A MIDI region made a pattern region: a row a pitch, each note on its nearest sixteenth,
    /// its velocity kept, how far off the step as its offset, how long as ties and a gate; up to
    /// four bars (64 steps) long, looped over the region.
    pub fn midi_to_pattern(&mut self, at: At, cx: &mut Context<Self>) {
        let r = self.session.project.region(at).clone();
        let Content::Midi { notes, .. } = &r.content else {
            return;
        };
        let bar = self.session.project.bar();
        let step = PPQ / 4;
        // As long as it takes to hold what it plays: one, two or four bars.
        let span = notes
            .iter()
            .map(|n| n.start.max(0) as u64 + 1)
            .max()
            .unwrap_or(bar);
        let bars = match span.div_ceil(bar) {
            0 | 1 => 1,
            2 => 2,
            _ => 4,
        };
        let steps = ((bar * bars) / step).min(64) as usize;
        let rows = to_pattern(notes, step, steps);
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
        self.status = Some(format!("A pattern now: {steps} steps"));
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
