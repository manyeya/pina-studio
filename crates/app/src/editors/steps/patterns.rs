//! Pattern regions: found, made, turned to and from MIDI, their length and rate.

use super::*;

impl Workspace {
    /// The pattern region the pane shows: the selected one, else the selected track's under the
    /// playhead, else its first.
    pub fn pattern(&self) -> Option<At> {
        let p = &self.session.project;
        let is = |at: At| matches!(p.region(at).content, Content::Pattern { .. });
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
        // A drum kit starts with kick, snare and hats; anything else with the key's chord.
        let instrument = tracks[self.track].instrument.as_ref();
        let starting: Vec<u8> = match (instrument, kit_sounds(instrument)) {
            (Some(Instrument::Drums), _) => STARTING_ROWS.to_vec(),
            // A sampler kit: its first few sounds.
            (_, Some(sounds)) => sounds.iter().take(6).map(|s| s.0).collect(),
            _ => {
                let key = self.session.project.song_key().unwrap_or(Key {
                    root: 0,
                    minor: false,
                });
                let root = 60 + key.root;
                let third = if key.minor { 3 } else { 4 };
                vec![root + 12, root + 7, root + third, root]
            }
        };
        let steps = (bar / (PPQ / 4)).max(1) as usize;
        let rows = starting
            .iter()
            .map(|&pitch| PatternRow {
                pitch,
                cells: vec![0; steps],
                mute: false,
                ..Default::default()
            })
            .collect();
        let t = self.track;
        self.edit(cx, |ws| {
            let regions = &mut ws.session.project.tracks[t].regions;
            regions.push(Region {
                start,
                length: bar,
                content: Content::Pattern {
                    step: PPQ / 4,
                    rows,
                    swing: 50.0,
                },
                ..Default::default()
            });
            ws.region = Some((t, regions.len() - 1));
            ws.bottom = Some(Bottom::Steps);
            ws.steps_row = 0;
        });
    }

    /// A MIDI region made a pattern region: a row a pitch, each note on its nearest sixteenth,
    /// its velocity kept; up to four bars long, looped over the region.
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
        let steps = ((bar * bars) / step) as usize;
        let rows = to_pattern(notes, step, steps);
        if rows.is_empty() {
            self.status = Some("That region has no notes to make steps of.".into());
            cx.notify();
            return;
        }
        self.edit(cx, |ws| {
            ws.session.project.tracks[at.0].regions[at.1].content = Content::Pattern {
                step,
                rows,
                swing: 50.0,
            };
            ws.region = Some(at);
            ws.bottom = Some(Bottom::Steps);
            ws.steps_row = 0;
        });
        self.status = Some(format!("A pattern now: {steps} steps"));
    }

    /// A pattern region made a MIDI region, to edit in the piano roll.
    pub fn pattern_to_midi(&mut self, at: At, cx: &mut Context<Self>) {
        let r = self.session.project.region(at).clone();
        let Content::Pattern { step, rows, swing } = &r.content else {
            return;
        };
        let notes = pattern_notes(*step, rows, r.length, *swing);
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

    /// The pattern's length in steps: every row taken to it, a row's steps repeated when it
    /// grows (so a bar's groove fills two), cut when it shrinks; the region at least a pass long.
    pub(crate) fn set_pattern_steps(&mut self, at: At, steps: usize, cx: &mut Context<Self>) {
        self.edit(cx, |ws| {
            let r = &mut ws.session.project.tracks[at.0].regions[at.1];
            if let Content::Pattern { rows, step, .. } = &mut r.content {
                for row in rows.iter_mut() {
                    let had = row.cells.len().max(1);
                    row.cells = (0..steps)
                        .map(|k| row.cells.get(k % had).copied().unwrap_or(0))
                        .collect();
                }
                r.length = r.length.max(*step * steps as u64);
            }
        });
    }

    pub(crate) fn set_pattern_rate(&mut self, at: At, rate: u64, cx: &mut Context<Self>) {
        self.edit(cx, |ws| {
            if let Content::Pattern { step, .. } =
                &mut ws.session.project.tracks[at.0].regions[at.1].content
            {
                *step = rate;
            }
        });
    }
}
