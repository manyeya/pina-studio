//! A line of lyrics as a loop: from the bar line it starts after (its pickup with it) for the
//! bars it takes, a copy of its audio region playing just that, on a track of lyric loops.

use super::*;

impl Project {
    /// Where the line sung from `from` to `to` (seconds into the file of the audio region at
    /// `at`) plays in the song as a loop: the bar line at or before where it starts, and how
    /// long: 1, 2, 4 or 8 bars (past 8, whole bars). None if the region doesn't play it.
    pub fn lyric_bars(&self, at: At, from: f32, to: f32) -> Option<(u64, u64)> {
        let r = self.region(at);
        let t = Timing::of(&r.content, self.sample_rate as f64)?;
        let rate = self.sample_rate as f64;
        let tick = |s: f32| self.audio_tick(&t, r.start as f64, s as f64 * rate);
        let (a, b) = (tick(from), tick(to));
        if b <= r.start as f64 || a >= (r.start + r.length) as f64 {
            return None;
        }
        let bar = self.bar().max(1);
        let first = (a.max(0.0) as u64 / bar) * bar;
        let bars = ((b - first as f64) / bar as f64).ceil().max(1.0) as u64;
        let bars = match bars <= 8 {
            true => bars.next_power_of_two(),
            false => bars,
        };
        Some((first, bars * bar))
    }

    /// The audio track lyric loops from track `of` go on: the one after it named so, else a new
    /// one there.
    pub fn lyrics_track(&mut self, of: usize) -> usize {
        let name = format!("{} Lyrics", self.tracks[of].name);
        if let Some(t) = self.tracks.iter().position(|t| t.name == name) {
            return t;
        }
        let track = Track {
            name,
            ..Default::default()
        };
        self.add_track(of + 1, track);
        of + 1
    }

    /// A line's loop: a copy of the audio region at `at` playing `bars` (as `lyric_bars` gives
    /// them), named `name`, at `to` on `track`, ready to loop (drag its loop out); playing `file`
    /// instead of the region's, if given (its vocals stem: sample for sample the same). Where it
    /// is.
    pub fn lyric_loop(
        &mut self,
        at: At,
        (first, length): (u64, u64),
        name: &str,
        track: usize,
        to: u64,
        file: Option<&std::path::Path>,
    ) -> At {
        let mut r = Region {
            name: name.into(),
            link: None,
            mute: false,
            looped: true,
            loop_length: Some(length),
            ..self.region(at).clone()
        };
        if let (Some(stem), Content::Audio { file, .. }) = (file, &mut r.content) {
            *file = stem.to_path_buf();
        }
        self.tracks[at.0].regions.push(r);
        let copy = (at.0, self.tracks[at.0].regions.len() - 1);
        // Trimmed on its own track (what plays stays where it is), then moved; a file starting
        // after the bar line starts the loop as far after `to`, so its beats stay on the bars.
        self.trim_start(copy, first);
        self.trim_end(copy, first + length);
        let shift = self.region(copy).start.saturating_sub(first);
        self.tracks[copy.0].regions[copy.1].loop_length = Some(length - shift);
        self.move_region(copy, track, to + shift)
    }
}

impl Project {
    /// A line sung in the audio region at `at` (playing `file` instead, its vocals stem, if
    /// given) chopped at its words, ready to flip: from the first word (`cuts[0]`, seconds into
    /// the file) to `to`, cut at each of `cuts`, on a new Sampler track after the region's track
    /// named `name`, a chop a note from C1, a MIDI region playing them as they were sung put at
    /// `tick` (each word as far into its bar as it was). `frames` is the file's length (frames at
    /// the song's rate). Where the Sampler track is.
    #[allow(clippy::too_many_arguments)]
    pub fn chop_line(
        &mut self,
        at: At,
        file: Option<&std::path::Path>,
        cuts: &[f32],
        to: f32,
        name: &str,
        tick: u64,
        frames: u64,
    ) -> Option<usize> {
        let from = *cuts.first()?;
        let (first, _) = self.lyric_bars(at, from, to)?;
        let rate = self.sample_rate as f64;
        let r = self.region(at);
        let t = Timing::of(&r.content, rate)?;
        let tick_of = |s: f32| {
            self.audio_tick(&t, r.start as f64, s as f64 * rate)
                .max(0.0) as u64
        };
        let (start, end) = (tick_of(from), tick_of(to).max(tick_of(from) + 1));
        // A copy of the region, just the line, playing its stem: sliced as any region is.
        let mut copy = Region {
            name: name.into(),
            link: None,
            mute: false,
            looped: false,
            loop_length: None,
            ..r.clone()
        };
        if let (Some(stem), Content::Audio { file: f, .. }) = (file, &mut copy.content) {
            *f = stem.to_path_buf();
        }
        self.tracks[at.0].regions.push(copy);
        let temp = (at.0, self.tracks[at.0].regions.len() - 1);
        self.trim_start(temp, start);
        self.trim_end(temp, end);
        let cut_frames: Vec<u64> = cuts[1..]
            .iter()
            .map(|&s| (s as f64 * rate).round() as u64)
            .collect();
        let made = self.sampler_from_audio(temp, frames, Some(&cut_frames));
        self.tracks[at.0].regions.remove(temp.1);
        let made = made?;
        // Its words as far into the bar as they were sung, the bar where it's put.
        let into = self.region((made, 0)).start.saturating_sub(first);
        self.tracks[made].regions[0].start = tick + into;
        self.tracks[made].name = name.into();
        Some(made)
    }
}
