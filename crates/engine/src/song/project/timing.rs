//! How an audio region keeps time with the song.
//!
//! Played as recorded (stretched by hand, maybe), its file runs in seconds whatever the tempo
//! does. Following, its own tempo is known and each of its beats lands on one of the song's
//! through the tempo map, so a tempo change, or a tempo that changes as the song goes, bends it
//! along. Locked to its bars or its beats, the ones its analysis found are pinned to the song's
//! too, so a take that drifts keeps to the grid.
//!
//! Warp markers are pinned points between a frame of the file and where it plays: in the
//! stretched audio when it's played as recorded, in the file's own steady time (at its tempo)
//! when it follows, so a tempo change leaves them be.

use serde::{Deserialize, Deserializer, Serialize, de::Error};

use super::{Content, MAX_BPM, MIN_BPM, PPQ, Project, TempoChange};

/// How an audio region keeps time.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Follow {
    /// Played as recorded (or as stretched by hand), whatever the tempo.
    #[default]
    Off,
    /// Its own tempo kept to the song's: its beats on the song's beats.
    On,
    /// As On, with its bars' downbeats pinned to the song's bars.
    Bars,
    /// As On, with every beat pinned to the song's beats.
    Beats,
}

impl Follow {
    pub const ALL: [Follow; 4] = [Follow::Off, Follow::On, Follow::Bars, Follow::Beats];

    pub fn name(self) -> &'static str {
        match self {
            Follow::Off => "As recorded",
            Follow::On => "Follow",
            Follow::Bars => "Lock bars",
            Follow::Beats => "Lock beats",
        }
    }

    pub fn is_off(&self) -> bool {
        *self == Follow::Off
    }
}

/// Written `true` and `false` before it had more than two settings.
impl<'de> Deserialize<'de> for Follow {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Was {
            Bool(bool),
            Name(String),
        }
        Ok(match Was::deserialize(d)? {
            Was::Bool(true) => Follow::On,
            Was::Bool(false) => Follow::Off,
            Was::Name(n) => match n.as_str() {
                "off" => Follow::Off,
                "on" => Follow::On,
                "bars" => Follow::Bars,
                "beats" => Follow::Beats,
                other => {
                    return Err(D::Error::custom(format!(
                        "follow is off, on, bars or beats, not {other}"
                    )));
                }
            },
        })
    }
}

/// Where a recording's beats are, as listening found them: each beat's frame in the file (at the
/// project's rate), the beats to its bar, and which beat is its first downbeat.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Grid {
    pub beats: Vec<u64>,
    #[serde(default = "four")]
    pub meter: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub downbeat: u32,
}

fn four() -> u32 {
    4
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Grid {
    /// Its downbeats' frames.
    pub fn downbeats(&self) -> impl Iterator<Item = u64> + '_ {
        let meter = self.meter.max(1) as usize;
        self.beats
            .iter()
            .skip(self.downbeat as usize)
            .step_by(meter)
            .copied()
    }

    /// Its average tempo, from the first beat to the last, at `rate`.
    pub fn tempo(&self, rate: f64) -> Option<f64> {
        let (first, last) = (*self.beats.first()?, *self.beats.last()?);
        let n = self.beats.len().checked_sub(1).filter(|&n| n > 0)?;
        Some(60.0 * rate * n as f64 / (last - first).max(1) as f64)
    }
}

/// Straight lines between pinned points (x, y), carried on past the last at `slope` (and before
/// the first, back to (0, 0)).
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    points: Vec<(f64, f64)>,
    slope: f64,
}

impl Curve {
    /// Through (0, 0) and `pins` (those that keep both in order).
    pub fn new(pins: impl IntoIterator<Item = (f64, f64)>, slope: f64) -> Curve {
        let mut pins: Vec<(f64, f64)> = pins.into_iter().collect();
        pins.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut points = vec![(0.0, 0.0)];
        for (x, y) in pins {
            let last = *points.last().unwrap();
            if x > last.0 && y > last.1 {
                points.push((x, y));
            }
        }
        Curve {
            points,
            slope: slope.max(1e-6),
        }
    }

    /// Where `x` goes.
    pub fn at(&self, x: f64) -> f64 {
        let p = &self.points;
        let k = p.partition_point(|q| q.0 <= x).clamp(1, p.len());
        match p.get(k) {
            Some(&b) => {
                let a = p[k - 1];
                a.1 + (x - a.0) * (b.1 - a.1) / (b.0 - a.0)
            }
            None => {
                let a = p[p.len() - 1];
                a.1 + (x - a.0) * self.slope
            }
        }
    }

    /// What goes to `y`.
    pub fn back(&self, y: f64) -> f64 {
        let p = &self.points;
        let k = p.partition_point(|q| q.1 <= y).clamp(1, p.len());
        match p.get(k) {
            Some(&b) => {
                let a = p[k - 1];
                a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1)
            }
            None => {
                let a = p[p.len() - 1];
                a.0 + (y - a.1) / self.slope
            }
        }
    }
}

/// An audio region's timing, worked out once: how its file's frames (at the project's rate) map
/// to its own time, and, following, the frames of that time to a quarter note.
pub struct Timing {
    curve: Curve,
    offset: f64,
    /// Following: frames a quarter note at its own tempo. Played as recorded: none.
    quarter: Option<f64>,
}

impl Timing {
    /// For audio content; none for anything else.
    pub fn of(content: &Content, rate: f64) -> Option<Timing> {
        let Content::Audio {
            offset,
            stretch,
            follow,
            tempo,
            grid,
            warps,
            ..
        } = content
        else {
            return None;
        };
        let warps = warps.iter().map(|w| (w[0] as f64, w[1] as f64));
        let quarter = tempo
            .filter(|t| *t > 0.0 && !follow.is_off())
            .map(|t| rate * 60.0 / t);
        let curve = match quarter {
            None => Curve::new(warps, *stretch),
            // Locked: its downbeats (or beats) pinned to its own steady time, counted from the
            // first, unless a warp marker of its own is near.
            Some(q) => {
                let pins: Vec<(f64, f64)> = match (follow, grid) {
                    (Follow::Bars | Follow::Beats, Some(g)) if !g.beats.is_empty() => {
                        let every = match follow {
                            Follow::Bars => g.meter.max(1) as usize,
                            _ => 1,
                        };
                        let first = g.beats[(g.downbeat as usize).min(g.beats.len() - 1)] as f64;
                        g.beats
                            .iter()
                            .skip(g.downbeat as usize)
                            .step_by(every)
                            .enumerate()
                            .map(|(k, &b)| (b as f64, first + (k * every) as f64 * q))
                            .filter(|&(x, _)| !warps_near(content, x, q / 4.0))
                            .collect()
                    }
                    _ => Vec::new(),
                };
                Curve::new(warps.chain(pins), 1.0)
            }
        };
        Some(Timing {
            curve,
            offset: *offset as f64,
            quarter,
        })
    }

    pub fn follows(&self) -> bool {
        self.quarter.is_some()
    }

    /// Where a frame of the file is in its warp time: the second of a warp marker on it.
    pub fn warp_time(&self, frame: f64) -> f64 {
        self.curve.at(frame)
    }
}

fn warps_near(content: &Content, x: f64, within: f64) -> bool {
    let Content::Audio { warps, .. } = content else {
        return false;
    };
    warps.iter().any(|w| (w[0] as f64 - x).abs() < within)
}

/// How the stretcher fits a file so it plays in time: how many times as long it is, its points
/// pinned (input, output; frames at the engine's rate), and the song's seconds where its first
/// frame plays.
pub struct AudioFit {
    pub ratio: f64,
    pub warps: Vec<(usize, usize)>,
    pub zero: f64,
}

impl Project {
    /// The tick where frame `frame` of audio content plays, its `offset` playing at `start`.
    pub fn audio_tick(&self, t: &Timing, start: f64, frame: f64) -> f64 {
        let along = t.curve.at(frame) - t.curve.at(t.offset);
        match t.quarter {
            Some(q) => start + along / q * PPQ as f64,
            None => self.tick_at_seconds(self.seconds_at(start) + along / self.sample_rate as f64),
        }
    }

    /// The song's seconds where frame `frame` plays.
    pub fn audio_seconds(&self, t: &Timing, start: f64, frame: f64) -> f64 {
        let along = t.curve.at(frame) - t.curve.at(t.offset);
        match t.quarter {
            Some(q) => self.seconds_at(start + along / q * PPQ as f64),
            None => self.seconds_at(start) + along / self.sample_rate as f64,
        }
    }

    /// The frame of the file playing at `tick`.
    pub fn audio_frame(&self, t: &Timing, start: f64, tick: f64) -> f64 {
        let along = match t.quarter {
            Some(q) => (tick - start) / PPQ as f64 * q,
            None => (self.seconds_at(tick) - self.seconds_at(start)) * self.sample_rate as f64,
        };
        t.curve.back(t.curve.at(t.offset) + along)
    }

    /// An audio region's hits (frames of its file, in order) moved onto the nearest `grid` line by
    /// warp markers, those it had over them replaced; of two near one line, the nearer goes. Where
    /// it starts stays put, and so does a hit on its first line. Returns how many it placed.
    pub fn quantize_hits(&mut self, at: super::At, hits: &[usize], grid: u64) -> usize {
        let r = self.region(at);
        let Some(t) = Timing::of(&r.content, self.sample_rate as f64) else {
            return 0;
        };
        let (start, end) = (r.start as f64, (r.start + r.length) as f64);
        let g = grid.max(1) as f64;
        // Each grid line's nearest hit, as the region plays now.
        let mut lines: Vec<(f64, usize, f64)> = Vec::new();
        for &h in hits {
            let now = self.audio_tick(&t, start, h as f64);
            if now < start || now >= end {
                continue;
            }
            let line = (now / g).round() * g;
            if line <= start {
                continue;
            }
            match lines.last_mut() {
                Some(l) if l.0 == line => {
                    if (now - line).abs() < (l.2 - line).abs() {
                        *l = (line, h, now);
                    }
                }
                _ => lines.push((line, h, now)),
            }
        }
        let first = self.audio_frame(&t, start, start).max(0.0);
        let last = self.audio_frame(&t, start, end);
        let mut placed: Vec<[u64; 2]> = lines
            .iter()
            .map(|&(line, h, _)| {
                [
                    h as u64,
                    self.warp_at(&t, start, line).round().max(0.0) as u64,
                ]
            })
            .collect();
        // Where it starts stays put.
        if first > 0.0 {
            placed.push([first.round() as u64, t.warp_time(first).round() as u64]);
        }
        let n = lines.len();
        let Content::Audio { warps, .. } = &mut self.tracks[at.0].regions[at.1].content else {
            return 0;
        };
        warps.retain(|w| (w[0] as f64) < first || (w[0] as f64) > last);
        warps.extend(placed);
        warps.sort();
        warps.dedup_by_key(|w| w[0]);
        n
    }

    /// Where a warp marker goes for its frame to play at `tick`: the second of [frame, where].
    pub fn warp_at(&self, t: &Timing, start: f64, tick: f64) -> f64 {
        let along = match t.quarter {
            Some(q) => (tick - start) / PPQ as f64 * q,
            None => (self.seconds_at(tick) - self.seconds_at(start)) * self.sample_rate as f64,
        };
        t.curve.at(t.offset) + along
    }

    /// The frames of a file `n` long where its line from frames to ticks bends: its warp and
    /// lock points, and where the song's tempo changes while it plays; with its ends.
    pub fn audio_corners(&self, t: &Timing, start: f64, n: f64) -> Vec<f64> {
        let mut v: Vec<f64> = t.curve.points[1..].iter().map(|p| p.0).collect();
        let (first, last) = (self.audio_tick(t, start, 0.0), self.audio_tick(t, start, n));
        v.extend(
            self.tempos
                .iter()
                .map(|c| c.tick as f64)
                .filter(|&c| c > first && c < last)
                .map(|c| self.audio_frame(t, start, c)),
        );
        v.retain(|&f| f > 0.0 && f < n);
        v.extend([0.0, n]);
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        v
    }

    /// For audio content that starts at `start` (its `offset` there): the frame playing at `tick`,
    /// rounded; any other content none.
    pub fn frame_at(&self, content: &Content, start: f64, tick: f64) -> Option<u64> {
        let t = Timing::of(content, self.sample_rate as f64)?;
        Some(self.audio_frame(&t, start, tick).round().max(0.0) as u64)
    }

    /// How a file's `frames` frames last from `start`, in ticks, played as recorded.
    pub fn ticks_for(&self, start: u64, frames: u64) -> u64 {
        let s = self.seconds_at(start as f64) + frames as f64 / self.sample_rate as f64;
        (self.tick_at_seconds(s) - start as f64).round().max(0.0) as u64
    }

    /// For the stretcher: how a file `n` frames long (at the project's rate) is fitted so it
    /// plays in time from `start`, at `rate`: how many times as long it is, its points pinned
    /// (input, output; frames at `rate`), and the song's seconds where its first frame plays.
    pub fn audio_fit(&self, content: &Content, start: f64, n: u64, rate: f64) -> Option<AudioFit> {
        let t = Timing::of(content, self.sample_rate as f64)?;
        let scale = rate / self.sample_rate as f64;
        let zero = self.audio_seconds(&t, start, 0.0);
        let out = |frame: f64| ((self.audio_seconds(&t, start, frame) - zero) * rate).round();
        let len = out(n as f64).max(1.0);
        // Following, where the song's tempo changes (inside the file) is a corner too; played as
        // recorded, only its warps bend it in seconds.
        let pins: Vec<f64> = match t.follows() {
            true => self.audio_corners(&t, start, n as f64),
            false => t.curve.points[1..].iter().map(|p| p.0).collect(),
        };
        let warps = pins
            .into_iter()
            .filter(|&f| f > 0.0 && f < n as f64)
            .map(|f| ((f * scale).round() as usize, out(f) as usize))
            .collect();
        // Exactly 1 when it plays as recorded, so it isn't stretched for nothing.
        let ratio = len / (n as f64 * scale).max(1.0);
        let ratio = if (ratio - 1.0).abs() < 1e-9 {
            1.0
        } else {
            ratio
        };
        Some(AudioFit { ratio, warps, zero })
    }
}

/// Audio content set to keep time as `follow` says. Following, its own tempo (what its stretch
/// made it at a song tempo of `song`, if it had none) and its warps in its own time. Played as
/// recorded again, the stretch it had before it followed (kept all along, so switching following
/// on and off gives back the audio it had) and its warps in the stretched audio.
fn refollow(content: &mut Content, follow: Follow, song: f64) {
    let Content::Audio {
        stretch,
        follow: was,
        tempo,
        warps,
        ..
    } = content
    else {
        return;
    };
    match (was.is_off(), follow.is_off()) {
        (true, false) => {
            tempo.get_or_insert(*stretch * song);
            for w in warps.iter_mut() {
                w[1] = (w[1] as f64 / *stretch).round() as u64;
            }
        }
        (false, true) => {
            for w in warps.iter_mut() {
                w[1] = (w[1] as f64 * *stretch).round() as u64;
            }
        }
        _ => {}
    }
    *was = follow;
}

impl Project {
    /// Changes how an audio region keeps time, its length following so it still ends on the same
    /// frame of its file: a loop keeps whole beats, a take all of itself. Anything else just
    /// changes.
    fn retime(&mut self, at: super::At, change: impl FnOnce(&mut Content)) {
        let rate = self.sample_rate as f64;
        let r = self.region(at);
        let start = r.start as f64;
        let end = Timing::of(&r.content, rate)
            .map(|t| self.audio_frame(&t, start, start + r.length as f64));
        change(&mut self.tracks[at.0].regions[at.1].content);
        if let (Some(end), Some(t)) = (end, Timing::of(&self.region(at).content, rate)) {
            let length = (self.audio_tick(&t, start, end) - start).round().max(1.0);
            self.tracks[at.0].regions[at.1].length = length as u64;
        }
    }

    /// Sets how an audio region (or each take of a take folder) keeps time. Following, it plays
    /// at the song's tempo; played as recorded again, as it did before it followed.
    pub fn set_follow(&mut self, at: super::At, follow: Follow) {
        let song = self.tempo_at(self.region(at).start);
        self.retime(at, |c| match c {
            Content::Takes { takes, .. } => {
                for t in takes {
                    refollow(&mut t.content, follow, song);
                }
            }
            c => refollow(c, follow, song),
        });
    }

    /// An audio region played as it was recorded: not following, not stretched (its warp markers
    /// kept, in the audio as recorded).
    pub fn unstretch(&mut self, at: super::At) {
        self.set_follow(at, Follow::Off);
        self.retime(at, |c| {
            if let Content::Audio { stretch, warps, .. } = c {
                for w in warps.iter_mut() {
                    w[1] = (w[1] as f64 / *stretch).round() as u64;
                }
                *stretch = 1.0;
            }
        });
    }

    /// Sets an audio region's own tempo: following, it plays at the song's from that (its length
    /// following, so it keeps the same audio).
    pub fn set_audio_tempo(&mut self, at: super::At, bpm: f64) {
        self.retime(at, |c| {
            if let Content::Audio { tempo, .. } = c {
                *tempo = Some(bpm.clamp(super::MIN_BPM, super::MAX_BPM));
            }
        });
    }

    /// Songs from before the tempo map: audio that followed the tempo had a stretch for the
    /// song's; now it has its own tempo, and its warps are in its own time.
    pub(crate) fn upgrade_audio(&mut self) {
        let song = self.tempo;
        let fix = |c: &mut Content| {
            if let Content::Audio {
                follow,
                tempo: None,
                ..
            } = c
                && !follow.is_off()
            {
                let mode = *follow;
                *follow = Follow::Off;
                refollow(c, mode, song);
            }
        };
        for t in &mut self.tracks {
            for r in t
                .regions
                .iter_mut()
                .chain(t.cells.iter_mut().map(|c| &mut c.region))
            {
                match &mut r.content {
                    Content::Takes { takes, .. } => {
                        takes.iter_mut().for_each(|k| fix(&mut k.content))
                    }
                    c => fix(c),
                }
            }
        }
    }
}

impl Project {
    /// Whether the song has something else keeping time where audio at `at` would play, from
    /// `start` to `end`: any other region there. Auto adapts the tempo only without one.
    pub fn keeps_time(&self, at: super::At, start: u64, end: u64) -> bool {
        self.tracks.iter().enumerate().any(|(t, track)| {
            track
                .regions
                .iter()
                .enumerate()
                .any(|(i, r)| (t, i) != at && r.start < end && r.start + r.length > start)
        })
    }

    /// The song keeps time with an audio region: from its first downbeat (on the nearest bar,
    /// with `align`) the tempo is the take's own, bar by bar, or with `steady` its average
    /// throughout (the whole song's, when the tempo doesn't change elsewhere). The region then
    /// follows the song, locked to its beats (a steady one simply follows), so it plays as it
    /// was recorded and keeps doing so through later tempo changes.
    pub fn adapt_to(&mut self, at: super::At, steady: bool, align: bool) -> Result<(), String> {
        let rate = self.sample_rate as f64;
        let r = self.region(at).clone();
        let Content::Audio { grid, .. } = &r.content else {
            return Err("that isn't an audio region".into());
        };
        let grid = grid
            .clone()
            .filter(|g| g.beats.len() >= 2)
            .ok_or("its beats aren't known yet: analyze_tempo it first")?;
        let first = (grid.downbeat as usize).min(grid.beats.len() - 1);
        let beats = &grid.beats[first..];
        if beats.len() < 2 {
            return Err("it has too few beats after its first downbeat to keep time with".into());
        }
        let now = Timing::of(&r.content, rate).ok_or("that isn't an audio region")?;
        let bar = self.bar() as f64;
        let mut down = self.audio_tick(&now, r.start as f64, beats[0] as f64);
        if align {
            down = (down / bar).round() * bar;
        }
        let down = down.max(0.0).round() as u64;
        // A beat of the song is so many quarter notes; a take's beat lasts its frames.
        let beat = self.beat();
        let qn = beat as f64 / PPQ as f64;
        let bpm =
            |frames: u64, beats: usize| qn * beats as f64 * 60.0 * rate / frames.max(1) as f64;
        let n = beats.len() - 1;
        let average = bpm(beats[n] - beats[0], n);
        let end = down + n as u64 * beat;
        if steady && self.tempos.is_empty() {
            self.set_tempo(average);
        } else {
            let meter = grid.meter.max(1) as usize;
            let changes: Vec<TempoChange> = match steady {
                true => vec![TempoChange {
                    tick: down,
                    bpm: average,
                }],
                false => (0..n)
                    .step_by(meter)
                    .map(|k| {
                        let to = (k + meter).min(n);
                        TempoChange {
                            tick: down + k as u64 * beat,
                            bpm: bpm(beats[to] - beats[k], to - k),
                        }
                    })
                    .collect(),
            };
            // With nothing before it, the song starts at the take's tempo too.
            if down <= self.bar() && !self.keeps_time(at, 0, down) {
                self.tempo = changes[0].bpm.clamp(MIN_BPM, MAX_BPM);
            }
            self.replace_tempos(down, end, &changes);
        }
        // Its own tempo is its average; it follows now, locked unless it's steady.
        let follow = match steady {
            true => Follow::On,
            false => Follow::Beats,
        };
        {
            let Content::Audio {
                tempo, follow: f, ..
            } = &mut self.tracks[at.0].regions[at.1].content
            else {
                unreachable!()
            };
            *tempo = Some(average);
            *f = follow;
        }
        // Where it starts, so its first downbeat is on `down`.
        let r = self.region(at).clone();
        let t = Timing::of(&r.content, rate).ok_or("that isn't an audio region")?;
        let shift = self.audio_tick(&t, 0.0, beats[0] as f64);
        let start = down as f64 - shift;
        let region = &mut self.tracks[at.0].regions[at.1];
        if start >= 0.0 {
            region.start = start.round() as u64;
        } else {
            // It would start before the song does: what plays from the song's start on.
            let frame = self.audio_frame(&t, start, 0.0).round().max(0.0) as u64;
            let region = &mut self.tracks[at.0].regions[at.1];
            if let Content::Audio { offset, .. } = &mut region.content {
                *offset = frame;
            }
            region.length = region.length.saturating_sub((-start).round() as u64).max(1);
            region.start = 0;
        }
        Ok(())
    }

    /// The tempo set so an audio region, as it plays now, fills from `from` to `to`: the song's
    /// tempo, when it doesn't change elsewhere, or that stretch's. The region moves there,
    /// played as recorded.
    pub fn fit_tempo(&mut self, at: super::At, from: u64, to: u64) -> Result<f64, String> {
        if to <= from {
            return Err("there's no stretch to fit it to".into());
        }
        if !matches!(self.region(at).content, Content::Audio { .. }) {
            return Err("that isn't an audio region".into());
        }
        // Played as recorded, as long as its audio lasts that way.
        self.set_follow(at, Follow::Off);
        let r = self.region(at);
        let seconds =
            self.seconds_at((r.start + r.length) as f64) - self.seconds_at(r.start as f64);
        let bpm =
            ((to - from) as f64 / PPQ as f64 * 60.0 / seconds.max(1e-3)).clamp(MIN_BPM, MAX_BPM);
        match self.tempos.is_empty() {
            true => self.set_tempo(bpm),
            false => self.replace_tempos(from, to, &[TempoChange { tick: from, bpm }]),
        }
        let length = self.ticks_for(from, (seconds * self.sample_rate as f64).round() as u64);
        let region = &mut self.tracks[at.0].regions[at.1];
        (region.start, region.length) = (from, length.max(1));
        Ok(bpm)
    }
}
