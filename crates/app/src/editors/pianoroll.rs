//! The piano roll: the selected MIDI region's notes against a keyboard, in the editor under the
//! tracks. It shares the tracks area's zoom and horizontal scroll,
//! so its grid lines up with the lanes above; ⌥-scroll zooms its rows.
//!
//! Notes are picked by clicking (shift adds), by a rubber band over the grid, or all with ⌘A;
//! dragged together, by either edge to resize, ⌥-dragged to copy; moved with the arrows (a
//! semitone, ⇧ an octave; left and right a grid step, ⌥ finer), copied, cut, pasted at the
//! playhead and duplicated after themselves (⌘C ⌘X ⌘V ⌘D), quantized with Q. Double-click (or
//! click, with the pencil) to write a note at the last length used, or a chord with a stamp; drag
//! the pencil on to paint more. With "In key" on, notes land on the song's scale. Under the notes,
//! a lane for their velocities (drag a dot, or draw across) or a controller's moves: the mod
//! wheel, pitch bend, expression or the sustain pedal, drawn freehand. Right-click for more.

use gpui::{
    App, Bounds, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    Point, ScrollDelta, ScrollWheelEvent, Window, canvas, div, fill, point, prelude::*, px, rgb,
    size,
};
use pina_engine::{
    music::{Key, NoteEdit},
    song::project::{At, Content, ControlPoint, ControlValue, Note, PPQ, Region},
};

use crate::{
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Drag, Edge, Slider, Tool, Workspace},
};

/// The height of one pitch, as it opens.
pub const KEY_H: f32 = 12.0;
/// The lane under the notes, and how tall a full velocity (or controller) stands in it.
pub const VELOCITY_LANE: f32 = 78.0;
pub const VELOCITY_H: f32 = 60.0;
const ROLL_RULER: f32 = 20.0;
const KEYS_W: f32 = 56.0;

/// The scroll that puts `pitch` near the top, at the opening row height.
pub fn scroll_to(pitch: u8) -> f32 {
    ((127 - pitch as i32) as f32 * KEY_H - KEY_H * 2.0).max(0.0)
}

fn black(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// Note names: middle C (60) is C3.
pub fn name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    format!("{}{}", NAMES[pitch as usize % 12], pitch as i32 / 12 - 2)
}

/// A pitch's name as a key spells it (B♭ in F major, A♯ in E major), middle C being C3;
/// with no key, sharps.
pub fn name_in(key: Option<Key>, pitch: u8) -> String {
    let Some(key) = key else {
        return name(pitch);
    };
    let (letter, octave, alter) = key.spell(pitch);
    let accidental = match alter {
        1 => "♯",
        -1 => "♭",
        _ => "",
    };
    let letter = ["C", "D", "E", "F", "G", "A", "B"][letter as usize];
    format!("{letter}{accidental}{}", octave - 1)
}

/// Where a note written at a tick goes: its track, the region it's in (if one takes it), else
/// the span of a new region round it.
type Target = (usize, Option<At>, Option<(u64, u64)>);

/// What a click writes: a note, or a chord stamped on it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Stamp {
    Note,
    Triad,
    Seventh,
    Ninth,
    Sus2,
    Sus4,
    Power,
    Octave,
}

impl Stamp {
    pub const ALL: [Stamp; 8] = [
        Stamp::Note,
        Stamp::Triad,
        Stamp::Seventh,
        Stamp::Ninth,
        Stamp::Sus2,
        Stamp::Sus4,
        Stamp::Power,
        Stamp::Octave,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Stamp::Note => "Note",
            Stamp::Triad => "Triad",
            Stamp::Seventh => "7th",
            Stamp::Ninth => "9th",
            Stamp::Sus2 => "Sus2",
            Stamp::Sus4 => "Sus4",
            Stamp::Power => "Power",
            Stamp::Octave => "Octave",
        }
    }

    /// The pitches it writes on `root`: stacked on the key's scale when the root's in it (so the
    /// chord stays in key: a triad on D in C major is D minor), else on a major chord.
    pub fn pitches(self, root: u8, key: Option<Key>) -> Vec<u8> {
        let degrees: &[usize] = match self {
            Stamp::Note => &[0],
            Stamp::Triad => &[0, 2, 4],
            Stamp::Seventh => &[0, 2, 4, 6],
            Stamp::Ninth => &[0, 2, 4, 6, 8],
            Stamp::Sus2 => &[0, 1, 4],
            Stamp::Sus4 => &[0, 3, 4],
            Stamp::Power => {
                return [root, root + 7, root + 12]
                    .into_iter()
                    .filter(|&p| p <= 127)
                    .collect();
            }
            Stamp::Octave => {
                return [root, root + 12]
                    .into_iter()
                    .filter(|&p| p <= 127)
                    .collect();
            }
        };
        let out: Vec<u8> = match key.filter(|k| k.contains(root)) {
            Some(k) => {
                // The scale upward from the root, as far as the widest stack needs.
                let scale: Vec<u8> = (root..=root.saturating_add(24))
                    .filter(|&p| k.contains(p))
                    .collect();
                degrees
                    .iter()
                    .filter_map(|&d| scale.get(d).copied())
                    .collect()
            }
            None => {
                const MAJOR: [u8; 9] = [0, 2, 4, 5, 7, 9, 11, 12, 14];
                degrees
                    .iter()
                    .map(|&d| root.saturating_add(MAJOR[d]))
                    .collect()
            }
        };
        out.into_iter().filter(|&p| p <= 127).collect()
    }
}

/// What the lane under the notes shows and edits.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lane {
    Velocity,
    Mod,
    Bend,
    Expression,
    Sustain,
}

impl Lane {
    pub const ALL: [Lane; 5] = [
        Lane::Velocity,
        Lane::Mod,
        Lane::Bend,
        Lane::Expression,
        Lane::Sustain,
    ];

    pub fn short(self) -> &'static str {
        match self {
            Lane::Velocity => "Vel",
            Lane::Mod => "Mod",
            Lane::Bend => "Bend",
            Lane::Expression => "Expr",
            Lane::Sustain => "Sus",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lane::Velocity => "Velocity",
            Lane::Mod => "Mod wheel",
            Lane::Bend => "Pitch bend",
            Lane::Expression => "Expression",
            Lane::Sustain => "Sustain",
        }
    }

    fn cc(self) -> Option<u8> {
        match self {
            Lane::Mod => Some(1),
            Lane::Expression => Some(11),
            Lane::Sustain => Some(64),
            _ => None,
        }
    }

    /// Whether a controller move is this lane's.
    fn holds(self, v: &ControlValue) -> bool {
        match (self, v) {
            (Lane::Bend, ControlValue::PitchBend { .. }) => true,
            (lane, ControlValue::Cc { number, .. }) => lane.cc() == Some(*number),
            _ => false,
        }
    }

    /// A move's height in the lane, 0 to 1 (pitch bend's middle, none, at a half).
    fn level(self, v: &ControlValue) -> f32 {
        match v {
            ControlValue::Cc { value, .. } => *value as f32 / 127.0,
            ControlValue::PitchBend { value } => (*value as f32 + 8192.0) / 16383.0,
            _ => 0.0,
        }
    }

    /// The move a height makes (the sustain pedal's down or up).
    fn make(self, level: f32) -> ControlValue {
        let level = level.clamp(0.0, 1.0);
        match self {
            Lane::Bend => {
                // Near the middle is no bend at all.
                let v = level * 16383.0 - 8192.0;
                ControlValue::PitchBend {
                    value: if v.abs() < 300.0 {
                        0
                    } else {
                        v.round().clamp(-8192.0, 8191.0) as i16
                    },
                }
            }
            Lane::Sustain => ControlValue::Cc {
                number: 64,
                value: if level >= 0.5 { 127 } else { 0 },
            },
            lane => ControlValue::Cc {
                number: lane.cc().unwrap_or(1),
                value: (level * 127.0).round() as u8,
            },
        }
    }

    /// Its value as words.
    fn say(self, v: &ControlValue) -> String {
        match v {
            ControlValue::PitchBend { value } => format!("{:+.2} st", *value as f32 / 8192.0 * 2.0),
            ControlValue::Cc { number: 64, value } => {
                if *value >= 64 {
                    "down".into()
                } else {
                    "up".into()
                }
            }
            ControlValue::Cc { value, .. } => value.to_string(),
            _ => String::new(),
        }
    }
}

/// Something done to the piano roll's notes: from a key, the context menu or the tools.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RollOp {
    SelectAll,
    /// Every note of a pitch.
    SelectPitch(u8),
    Deselect,
    Copy,
    Cut,
    Paste,
    Duplicate,
    Delete,
    /// Semitones; on the scale's notes with "In key" on, a semitone being a scale step.
    Transpose(i32),
    /// Quarters of the grid.
    Nudge(i32),
    Quantize,
    Edit(NoteEdit),
}

/// A velocity's colour: soft notes cool, hard ones hot.
pub fn velocity_color(v: u8) -> u32 {
    const STOPS: [(f32, u32); 6] = [
        (0.0, 0x6366f1),
        (0.3, 0x38bdf8),
        (0.5, 0x3ddc97),
        (0.7, 0xffc53d),
        (0.86, 0xff7a59),
        (1.0, 0xff4d6d),
    ];
    let t = v as f32 / 127.0;
    let k = STOPS
        .iter()
        .position(|s| s.0 >= t)
        .unwrap_or(STOPS.len() - 1)
        .max(1);
    let (a, b) = (STOPS[k - 1], STOPS[k]);
    let f = ((t - a.0) / (b.0 - a.0).max(1e-6)).clamp(0.0, 1.0);
    let mix = |shift: u32| {
        let (x, y) = (
            ((a.1 >> shift) & 0xff) as f32,
            ((b.1 >> shift) & 0xff) as f32,
        );
        ((x + (y - x) * f).round() as u32) << shift
    };
    mix(16) | mix(8) | mix(0)
}

impl Workspace {
    /// A pitch's name in the song's key.
    pub fn note_name(&self, pitch: u8) -> String {
        name_in(self.session.project.song_key(), pitch)
    }

    /// Where a tick falls in the song: bar, beat, sixteenth.
    fn position(&self, tick: u64) -> String {
        let (bar, beat) = (
            self.session.project.bar().max(1),
            self.session.project.beat().max(1),
        );
        let sixteenth = (PPQ / 4).max(1);
        format!(
            "{}.{}.{}",
            tick / bar + 1,
            tick % bar / beat + 1,
            tick % beat / sixteenth + 1
        )
    }

    /// The MIDI region the piano roll shows: the one selected, else the selected track's under
    /// the playhead, else its first.
    pub fn roll_region(&self) -> Option<At> {
        let p = &self.session.project;
        let midi = |at: At| matches!(p.region(at).content, Content::Midi { .. });
        self.region.filter(|&at| midi(at)).or_else(|| {
            let track = p.tracks.get(self.track)?;
            let tick = self.playhead();
            let mine: Vec<At> = (0..track.regions.len())
                .map(|r| (self.track, r))
                .filter(|&at| midi(at))
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

    /// A MIDI region on `track` that `tick` is inside.
    pub fn midi_region_at(&self, track: usize, tick: u64) -> Option<At> {
        let t = self.session.project.tracks.get(track)?;
        t.regions
            .iter()
            .position(|r| {
                r.start <= tick
                    && tick < r.start + r.length
                    && matches!(r.content, Content::Midi { .. })
            })
            .map(|r| (track, r))
    }

    /// Grows a MIDI region to the end of the bar its last note starts in, when a note starts
    /// past its end, as far as the next region on its track allows.
    pub fn grow_to_notes(&mut self, at: At) {
        let p = &mut self.session.project;
        let bar = p.bar();
        let r = p.region(at);
        let Content::Midi { notes, .. } = &r.content else {
            return;
        };
        let last = notes.iter().map(|n| n.start).max().unwrap_or(0);
        if last < r.length as i64 {
            return;
        }
        let next = p.tracks[at.0]
            .regions
            .iter()
            .map(|o| o.start)
            .filter(|&s| s > r.start)
            .min();
        let want = (last as u64 / bar + 1) * bar;
        let room = next.map_or(u64::MAX, |n| n - r.start);
        let length = want.min(room).max(r.length);
        p.tracks[at.0].regions[at.1].length = length;
    }

    /// Scrolls the piano roll to the selected region's notes, bringing the region into view if
    /// it's off the side.
    pub fn focus_roll(&mut self) {
        if let Some(at) = self.roll_region() {
            let r = self.session.project.region(at);
            let (x0, x1) = (
                r.start as f32 * self.px_per_tick(),
                (r.start + r.length) as f32 * self.px_per_tick(),
            );
            let width = f32::from(self.lanes_bounds.get().size.width);
            if x0 < self.scroll_x || (width > 0.0 && x1 > self.scroll_x + width) {
                self.scroll_x = (x0 - 40.0).max(0.0);
            }
        }
        if let Some(at) = self.roll_region()
            && let Some(notes) = self.session.project.notes_mut(at)
            && let Some(top) = notes.iter().map(|n| n.pitch).max()
        {
            let kh = self.roll.key_h;
            self.roll_scroll_y =
                ((127 - top.saturating_add(4).min(127) as i32) as f32 * kh - kh * 2.0).max(0.0);
        }
    }

    /// Fits the shown notes' pitches to the piano roll's height.
    fn fit_roll(&mut self) {
        let Some(at) = self.roll_region() else { return };
        let Some(notes) = self.session.project.notes_mut(at) else {
            return;
        };
        let (Some(lo), Some(hi)) = (
            notes.iter().map(|n| n.pitch).min(),
            notes.iter().map(|n| n.pitch).max(),
        ) else {
            return;
        };
        let height = f32::from(self.roll_bounds.get().size.height).max(120.0);
        let rows = (hi - lo) as f32 + 7.0;
        self.roll.key_h = (height / rows).clamp(6.0, 26.0);
        self.roll_scroll_y =
            ((127 - hi.saturating_add(3).min(127)) as f32 * self.roll.key_h).max(0.0);
        self.focus_roll_x();
    }

    fn focus_roll_x(&mut self) {
        if let Some(at) = self.roll_region() {
            let r = self.session.project.region(at);
            self.scroll_x = (r.start as f32 * self.px_per_tick() - 40.0).max(0.0);
        }
    }

    /// The window y of a pitch's row in the piano roll.
    #[cfg(test)]
    pub fn pitch_y(&self, pitch: u8) -> f32 {
        let top = f32::from(self.roll_bounds.get().origin.y);
        top + (127 - pitch) as f32 * self.roll.key_h - self.roll_scroll_y
    }

    fn pitch_at(&self, y: f32) -> u8 {
        let top = f32::from(self.roll_bounds.get().origin.y);
        let row = ((y - top + self.roll_scroll_y) / self.roll.key_h).floor();
        (127.0 - row).clamp(0.0, 127.0) as u8
    }

    /// The pitch a note goes to: itself, or with "In key" on, the scale's nearest (the way it's
    /// moving, if it is).
    fn in_key(&self, pitch: u8, toward: i32) -> u8 {
        let Some(key) = self.session.project.song_key().filter(|_| self.roll.in_key) else {
            return pitch;
        };
        if key.contains(pitch) {
            return pitch;
        }
        let step = |d: i32| (pitch as i32 + d).clamp(0, 127) as u8;
        match toward.signum() {
            1 => step(1),
            -1 => step(-1),
            _ if key.contains(step(-1)) => step(-1),
            _ => step(1),
        }
    }

    /// The length a new note is written at: the last picked or stretched, else the grid.
    fn new_length(&self) -> u64 {
        match self.roll.length {
            0 => self.grid(),
            l => l,
        }
    }

    fn roll_scroll(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => p.map(f32::from),
            ScrollDelta::Lines(l) => l.map(|v| v * 24.0),
        };
        let b = self.roll_bounds.get();
        if e.modifiers.platform {
            let x = f32::from(e.position.x) - f32::from(b.origin.x);
            self.zoom_around(self.px_per_beat * (1.0 + d.y * 0.01), x);
        } else if e.modifiers.alt {
            // Taller or shorter rows, about the pitch under the mouse.
            let y = f32::from(e.position.y) - f32::from(b.origin.y);
            let row = (y + self.roll_scroll_y) / self.roll.key_h;
            self.roll.key_h = (self.roll.key_h * (1.0 + d.y * 0.01)).clamp(6.0, 28.0);
            self.roll_scroll_y = (row * self.roll.key_h - y).max(0.0);
        } else {
            let height = f32::from(b.size.height);
            let most = (128.0 * self.roll.key_h - height).max(0.0);
            self.roll_scroll_y = (self.roll_scroll_y - d.y).clamp(0.0, most);
            self.scroll_x = (self.scroll_x - d.x).max(0.0);
        }
        cx.notify();
    }

    /// The region a note at `tick` on the shown track goes in: one it's inside, or the shown
    /// one (grown to take it); else, with none there, a new region round it: drawing is all it
    /// takes to start writing. None if it can't (an audio track, or a region in the way).
    fn region_for(&mut self, tick: u64, cx: &mut Context<Self>) -> Option<Target> {
        let shown = self.roll_region();
        let track = shown.map_or(self.track, |at| at.0);
        let t = self.session.project.tracks.get(track)?;
        if t.instrument.is_none() {
            self.status = Some(format!(
                "{} plays audio: notes go on an instrument track (⌥⌘N, or an instrument from the Browser).",
                t.name
            ));
            cx.notify();
            return None;
        }
        let project = &self.session.project;
        let grows = shown.filter(|&at| {
            let r = project.region(at);
            let next = project.tracks[at.0]
                .regions
                .iter()
                .map(|o| o.start)
                .filter(|&s| s > r.start)
                .min()
                .unwrap_or(u64::MAX);
            tick >= r.start && tick < next
        });
        let target = self.midi_region_at(track, tick).or(grows);
        let span = match target {
            Some(_) => None,
            None => match self.new_region_span(track, tick) {
                Some(span) => Some(span),
                None => {
                    self.status = Some("A region's there already: click inside it.".into());
                    cx.notify();
                    return None;
                }
            },
        };
        Some((track, target, span))
    }

    /// A click on the grid. With the pointer: a rubber band, or with a double-click a note. With
    /// the pencil: a note (or chord), and painting on as it's dragged.
    fn roll_click(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let p = e.position.map(f32::from);
        let adds = e.click_count == 2 || self.tool == Tool::Pencil;
        if !adds {
            let base = if e.modifiers.shift {
                self.picked.clone()
            } else {
                Vec::new()
            };
            self.picked = base.clone();
            self.held_picks = base;
            self.begin_drag(Drag::Marquee { from: p, to: p }, cx);
            return;
        }
        let tick = self.snap_down(self.tick_at(p.x));
        let pitch = self.in_key(self.pitch_at(p.y), 0);
        let Some((track, target, span)) = self.region_for(tick, cx) else {
            return;
        };
        let length = self.new_length();
        let pitches = self
            .roll
            .stamp
            .pitches(pitch, self.session.project.song_key());
        let id = self.session.project.tracks[track].id;
        let mut made = None;
        self.edit(cx, |ws| {
            let at = match (target, span) {
                (Some(at), _) => at,
                (None, Some((start, len))) => {
                    let regions = &mut ws.session.project.tracks[track].regions;
                    regions.push(Region {
                        start,
                        length: len,
                        content: Content::Midi {
                            notes: Vec::new(),
                            controls: Vec::new(),
                        },
                        ..Default::default()
                    });
                    (track, regions.len() - 1)
                }
                (None, None) => return,
            };
            let start = ws.session.project.region(at).start;
            let Some(notes) = ws.session.project.notes_mut(at) else {
                return;
            };
            let first = notes.len();
            for &pitch in &pitches {
                notes.push(Note {
                    start: (tick - start) as i64,
                    length,
                    pitch,
                    velocity: 100,
                });
            }
            ws.picked = (first..notes.len()).collect();
            ws.region = Some(at);
            ws.grow_to_notes(at);
            made = Some((at, (tick - start) as i64));
        });
        let Some((at, start)) = made else { return };
        for &pitch in &pitches {
            self.player.preview(id, pitch, 100);
        }
        self.painting = pitches;
        self.drag = Some(Drag::Paint { at, last: start });
        cx.notify();
    }

    /// The rubber band moved: the notes it touches picked (with those picked before, with shift).
    pub(crate) fn drag_marquee(&mut self, to: Point<f32>) {
        let Some(Drag::Marquee { from, .. }) = self.drag else {
            return;
        };
        let base = self.held_picks.clone();
        if let Some(Drag::Marquee { to: t, .. }) = &mut self.drag {
            *t = to;
        }
        let Some(at) = self.roll_region() else { return };
        let r = self.session.project.region(at).clone();
        let (t0, t1) = (
            self.tick_at(from.x.min(to.x)),
            self.tick_at(from.x.max(to.x)),
        );
        let (hi, lo) = (
            self.pitch_at(from.y.min(to.y)),
            self.pitch_at(from.y.max(to.y)),
        );
        let Content::Midi { notes, .. } = &r.content else {
            return;
        };
        let mut picked = base;
        for (i, n) in notes.iter().enumerate() {
            let s = r.start as f64 + n.start as f64;
            let touches = s < t1 && s + n.length as f64 > t0 && (lo..=hi).contains(&n.pitch);
            if touches && !picked.contains(&i) {
                picked.push(i);
            }
        }
        self.picked = picked;
    }

    /// The pencil dragged on: a note (or chord) at each note length it passes, at the pitch under
    /// it, painting.
    pub(crate) fn drag_paint(&mut self, p: Point<f32>) {
        let Some(Drag::Paint { at, last }) = self.drag else {
            return;
        };
        let sounding = self.painting.clone();
        if self.tool != Tool::Pencil {
            return;
        }
        let length = self.new_length().max(1) as i64;
        let start = self.session.project.region(at).start as i64;
        let tick = self.tick_at(p.x) as i64 - start;
        let steps = (tick - last).div_euclid(length);
        if steps < 1 {
            return;
        }
        let pitch = self.in_key(self.pitch_at(p.y), 0);
        let pitches = self
            .roll
            .stamp
            .pitches(pitch, self.session.project.song_key());
        let id = self.session.project.tracks[at.0].id;
        let Some(notes) = self.session.project.notes_mut(at) else {
            return;
        };
        for k in 1..=steps {
            let first = notes.len();
            for &pitch in &pitches {
                notes.push(Note {
                    start: last + k * length,
                    length: length as u64,
                    pitch,
                    velocity: 100,
                });
            }
            self.picked.extend(first..notes.len());
        }
        self.grow_to_notes(at);
        if sounding != pitches {
            for &p in &sounding {
                self.player.preview(id, p, 0);
            }
            for &p in &pitches {
                self.player.preview(id, p, 100);
            }
        }
        self.painting = pitches;
        self.drag = Some(Drag::Paint {
            at,
            last: last + steps * length,
        });
    }

    /// The picked notes dragged: moved (the grabbed one snapping, the rest keeping their
    /// distance), or stretched by either edge.
    pub(crate) fn drag_notes(&mut self, p: Point<f32>) {
        let Some(Drag::Notes {
            at,
            lead,
            edge,
            from,
            ..
        }) = self.drag
        else {
            return;
        };
        let was = self.held.clone();
        let Some(&(_, first)) = was.iter().find(|(i, _)| *i == lead) else {
            return;
        };
        let d = p - from;
        let dt = (d.x / self.px_per_tick()) as f64;
        let region_start = self.session.project.region(at).start as i64;
        let grid = self.grid() as i64;
        let snapped = |t: f64| self.snap(t.max(0.0)) as i64;
        let track = self.session.project.tracks[at.0].id;
        match edge {
            Edge::Move => {
                let earliest = was.iter().map(|(_, n)| n.start).min().unwrap_or(0);
                let shift = (snapped((region_start + first.start) as f64 + dt)
                    - region_start
                    - first.start)
                    .max(-earliest - region_start);
                let rows = (d.y / self.roll.key_h).round() as i32;
                let lo = was.iter().map(|(_, n)| n.pitch as i32).min().unwrap_or(0);
                let hi = was.iter().map(|(_, n)| n.pitch as i32).max().unwrap_or(127);
                let by = (-rows).clamp(-lo, 127 - hi);
                let keyed: Vec<(usize, u8)> = was
                    .iter()
                    .map(|&(i, n)| (i, self.in_key((n.pitch as i32 + by) as u8, by)))
                    .collect();
                let Some(notes) = self.session.project.notes_mut(at) else {
                    return;
                };
                let before = notes.get(lead).map(|n| n.pitch);
                for (&(i, n), &(_, pitch)) in was.iter().zip(&keyed) {
                    if let Some(note) = notes.get_mut(i) {
                        note.start = n.start + shift;
                        note.pitch = pitch;
                    }
                }
                let after = notes.get(lead).map(|n| n.pitch);
                if let (Some(old), Some(new)) = (before, after)
                    && old != new
                {
                    self.player.preview(track, old, 0);
                    self.player.preview(track, new, 100);
                }
            }
            Edge::End | Edge::Stretch | Edge::Loop => {
                let end = first.start + first.length as i64;
                let by = snapped((region_start + end) as f64 + dt) - region_start - end;
                let Some(notes) = self.session.project.notes_mut(at) else {
                    return;
                };
                for &(i, n) in &was {
                    if let Some(note) = notes.get_mut(i) {
                        let least = grid.min(n.length as i64).max(1);
                        note.length = (n.length as i64 + by).max(least) as u64;
                    }
                }
                if let Some(n) = notes.get(lead) {
                    self.roll.length = n.length;
                }
            }
            Edge::Start => {
                let by =
                    snapped((region_start + first.start) as f64 + dt) - region_start - first.start;
                // No note shorter than a grid step (or than it was), nor before the region.
                let most = was
                    .iter()
                    .map(|(_, n)| n.length as i64 - grid.min(n.length as i64).max(1))
                    .min()
                    .unwrap_or(0);
                let least = -was
                    .iter()
                    .map(|(_, n)| n.start + region_start)
                    .min()
                    .unwrap_or(0);
                let by = by.clamp(least, most);
                let Some(notes) = self.session.project.notes_mut(at) else {
                    return;
                };
                for &(i, n) in &was {
                    if let Some(note) = notes.get_mut(i) {
                        note.start = n.start + by;
                        note.length = (n.length as i64 - by) as u64;
                    }
                }
                if let Some(n) = notes.get(lead) {
                    self.roll.length = n.length;
                }
            }
        }
    }

    /// Velocities drawn across the lane: each note starting between where the mouse was and is
    /// takes the height it's at (the picked notes only, if some are).
    pub(crate) fn drag_velocity_line(&mut self, p: Point<f32>) {
        let Some(Drag::VelocityLine { at, x }) = self.drag else {
            return;
        };
        let lane = self.lane_bounds.get();
        let bottom = f32::from(lane.origin.y) + f32::from(lane.size.height) - 5.0;
        let v = (((bottom - p.y) / VELOCITY_H) * 127.0)
            .round()
            .clamp(1.0, 127.0) as u8;
        let (t0, t1) = (self.tick_at(x.min(p.x)), self.tick_at(x.max(p.x)) + 1.0);
        let start = self.session.project.region(at).start as f64;
        let only = self.picked.clone();
        if let Some(notes) = self.session.project.notes_mut(at) {
            for (i, n) in notes.iter_mut().enumerate() {
                let s = start + n.start as f64;
                if (t0 - 2.0..=t1).contains(&s) && (only.is_empty() || only.contains(&i)) {
                    n.velocity = v;
                }
            }
        }
        self.drag = Some(Drag::VelocityLine { at, x: p.x });
    }

    /// A controller's moves drawn freehand: one a grid step (finer than the notes'), replacing
    /// what was there; or with ⌥, rubbed out.
    pub(crate) fn drag_control(&mut self, p: Point<f32>) {
        let Some(Drag::Control { at, last, erase }) = self.drag else {
            return;
        };
        let lane = self.roll.lane;
        let b = self.lane_bounds.get();
        let bottom = f32::from(b.origin.y) + f32::from(b.size.height) - 5.0;
        let level = (bottom - p.y) / VELOCITY_H;
        let start = self.session.project.region(at).start as i64;
        let step = (self.grid() / 2).max(PPQ / 16) as i64;
        let tick = ((self.tick_at(p.x) as i64 - start) / step * step).max(0);
        let (from, to) = match last {
            Some(l) => (l.min(tick), l.max(tick)),
            None => (tick, tick),
        };
        let r = &mut self.session.project.tracks[at.0].regions[at.1];
        let Content::Midi { controls, .. } = &mut r.content else {
            return;
        };
        controls.retain(|c| !(lane.holds(&c.kind) && c.start >= from && c.start <= to));
        if !erase {
            let mut t = from;
            while t <= to {
                controls.push(ControlPoint {
                    start: t,
                    kind: lane.make(level),
                });
                t += step;
            }
            controls.sort_by_key(|c| c.start);
        }
        self.drag = Some(Drag::Control {
            at,
            last: Some(tick),
            erase,
        });
    }

    pub fn delete_notes(&mut self, at: At, cx: &mut Context<Self>) {
        let mut gone = self.picked.clone();
        gone.sort_unstable_by(|a, b| b.cmp(a));
        gone.dedup();
        self.edit(cx, |ws| {
            if let Some(notes) = ws.session.project.notes_mut(at) {
                for i in gone {
                    if i < notes.len() {
                        notes.remove(i);
                    }
                }
            }
            ws.picked.clear();
        });
    }

    /// Does something to the shown region's notes: the picked ones, or for a tool with none
    /// picked, all of them.
    pub fn roll_op(&mut self, op: RollOp, cx: &mut Context<Self>) {
        let Some(at) = self.roll_region() else { return };
        let count = self
            .session
            .project
            .region(at)
            .content
            .notes(0)
            .map_or(0, |n| n.len());
        let picked = self.picked.clone();
        let notes_of = |ws: &Self| -> Vec<Note> {
            match &ws.session.project.region(at).content {
                Content::Midi { notes, .. } => notes.clone(),
                _ => Vec::new(),
            }
        };
        match op {
            RollOp::SelectAll => {
                self.region = Some(at);
                self.picked = (0..count).collect();
            }
            RollOp::SelectPitch(pitch) => {
                self.region = Some(at);
                self.picked = notes_of(self)
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.pitch == pitch)
                    .map(|(i, _)| i)
                    .collect();
            }
            RollOp::Deselect => self.picked.clear(),
            RollOp::Copy | RollOp::Cut => {
                let notes = notes_of(self);
                let mut clip: Vec<Note> = picked
                    .iter()
                    .filter_map(|&i| notes.get(i).copied())
                    .collect();
                let first = clip.iter().map(|n| n.start).min().unwrap_or(0);
                for n in &mut clip {
                    n.start -= first;
                }
                if !clip.is_empty() {
                    self.status = Some(format!(
                        "Copied {} note{}",
                        clip.len(),
                        if clip.len() == 1 { "" } else { "s" }
                    ));
                    self.clip = clip;
                }
                if op == RollOp::Cut && !picked.is_empty() {
                    self.delete_notes(at, cx);
                }
            }
            RollOp::Paste => {
                if self.clip.is_empty() {
                    return;
                }
                let r = self.session.project.region(at).clone();
                // At the playhead, if it's in the region; else where the region starts.
                let tick = self.playhead();
                let to = match (r.start..r.start + r.length).contains(&tick) {
                    true => self.snap_down(tick as f64) - r.start,
                    false => 0,
                } as i64;
                let clip = self.clip.clone();
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    let first = notes.len();
                    notes.extend(clip.iter().map(|n| Note {
                        start: n.start + to,
                        ..*n
                    }));
                    ws.picked = (first..notes.len()).collect();
                    ws.region = Some(at);
                    ws.grow_to_notes(at);
                });
            }
            RollOp::Duplicate => {
                let notes = notes_of(self);
                let chosen: Vec<Note> = picked
                    .iter()
                    .filter_map(|&i| notes.get(i).copied())
                    .collect();
                let (Some(from), Some(to)) = (
                    chosen.iter().map(|n| n.start).min(),
                    chosen.iter().map(|n| n.start + n.length as i64).max(),
                ) else {
                    return;
                };
                // After itself, its span rounded up to the beat (a bar's worth, a bar on).
                let beat = self.session.project.beat() as i64;
                let span = ((to - from) + beat - 1) / beat * beat;
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    let first = notes.len();
                    notes.extend(chosen.iter().map(|n| Note {
                        start: n.start + span,
                        ..*n
                    }));
                    ws.picked = (first..notes.len()).collect();
                    ws.grow_to_notes(at);
                });
            }
            RollOp::Delete => {
                if !picked.is_empty() {
                    self.delete_notes(at, cx);
                }
            }
            RollOp::Transpose(by) => {
                if picked.is_empty() {
                    return;
                }
                let key = self
                    .session
                    .project
                    .song_key()
                    .filter(|_| self.roll.in_key && by.abs() == 1);
                let track = self.session.project.tracks[at.0].id;
                let mut heard = None;
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    for &i in &picked {
                        let Some(n) = notes.get_mut(i) else { continue };
                        let mut p = (n.pitch as i32 + by).clamp(0, 127);
                        // In key, a step is the scale's next note.
                        if let Some(k) = key {
                            while !k.contains(p as u8) && (1..127).contains(&p) {
                                p += by;
                            }
                        }
                        n.pitch = p as u8;
                        heard = Some((n.pitch, n.velocity));
                    }
                });
                if let Some((pitch, v)) = heard {
                    self.player.preview(track, pitch, v);
                    self.player.preview(track, pitch, 0);
                }
            }
            RollOp::Nudge(quarters) => {
                if picked.is_empty() {
                    return;
                }
                // A grid step, or a quarter of one (a tick at least).
                let by = match self.grid() as i64 * quarters as i64 / 4 {
                    0 => quarters.signum() as i64,
                    by => by,
                };
                let region_start = self.session.project.region(at).start as i64;
                self.edit(cx, |ws| {
                    let Some(notes) = ws.session.project.notes_mut(at) else {
                        return;
                    };
                    for &i in &picked {
                        if let Some(n) = notes.get_mut(i) {
                            n.start = (n.start + by).max(-region_start);
                        }
                    }
                });
            }
            RollOp::Quantize => {
                let edit = NoteEdit::Quantize {
                    grid: self.roll.grid,
                    strength: self.roll.strength,
                    swing: self.roll.swing,
                };
                self.edit_notes(at, edit, cx);
            }
            RollOp::Edit(edit) => self.edit_notes(at, edit, cx),
        }
        cx.notify();
    }

    /// The menu a right-click on the piano roll opens: on a note, or on the grid.
    fn roll_menu(&mut self, e: &MouseDownEvent, on: Option<usize>, cx: &mut Context<Self>) {
        let Some(at) = self.roll_region() else { return };
        if let Some(i) = on
            && !self.picked.contains(&i)
        {
            self.picked = vec![i];
        }
        let pitch = self.pitch_at(f32::from(e.position.y));
        let some = !self.picked.is_empty();
        let mut items: Vec<(String, MenuAction)> = Vec::new();
        let mut add = |label: String, op: RollOp, on: bool| {
            items.push((
                label,
                if on {
                    MenuAction::Roll(op)
                } else {
                    MenuAction::Nothing
                },
            ))
        };
        add("Select All    ⌘A".into(), RollOp::SelectAll, true);
        add(
            format!("Select Every {}", self.note_name(pitch)),
            RollOp::SelectPitch(pitch),
            true,
        );
        add("Copy    ⌘C".into(), RollOp::Copy, some);
        add("Cut    ⌘X".into(), RollOp::Cut, some);
        add(
            "Paste at Playhead    ⌘V".into(),
            RollOp::Paste,
            !self.clip.is_empty(),
        );
        add("Duplicate    ⌘D".into(), RollOp::Duplicate, some);
        add("Delete    ⌫".into(), RollOp::Delete, some);
        add("Quantize    Q".into(), RollOp::Quantize, true);
        add("Up an Octave    ⇧↑".into(), RollOp::Transpose(12), some);
        add("Down an Octave    ⇧↓".into(), RollOp::Transpose(-12), some);
        add("Legato".into(), RollOp::Edit(NoteEdit::Legato), true);
        add(
            "Strum".into(),
            RollOp::Edit(NoteEdit::Strum { ticks: PPQ / 32 }),
            true,
        );
        add("Glue".into(), RollOp::Edit(NoteEdit::Glue), true);
        add("Reverse".into(), RollOp::Edit(NoteEdit::Reverse), true);
        add(
            "Flip Upside Down".into(),
            RollOp::Edit(NoteEdit::Flip),
            true,
        );
        let _ = at;
        self.menu = Some(Menu {
            at: e.position,
            items,
            ..Default::default()
        });
        cx.notify();
    }

    /// A press in the lane under the notes: drawing velocities or a controller's moves.
    fn lane_down(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(at) = self.roll_region() else { return };
        let p = e.position.map(f32::from);
        match self.roll.lane {
            Lane::Velocity => {
                self.begin_drag(Drag::VelocityLine { at, x: p.x }, cx);
                self.drag_velocity_line(p);
            }
            _ => {
                self.begin_drag(
                    Drag::Control {
                        at,
                        last: None,
                        erase: e.modifiers.alt,
                    },
                    cx,
                );
                self.drag_control(p);
            }
        }
        self.changed(cx);
    }

    /// The quantize grid, strength, swing and one-click tools, left of the keyboard.
    fn roll_tools(&mut self, at: Option<At>, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let grid_seg = |label: &'static str, g: u64, on: bool, cx: &mut Context<Self>| {
            seg(label, on, 20.)
                .flex_1()
                .px(px(0.))
                .font_family(MONO)
                .text_size(px(10.5))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.roll.grid = g;
                        cx.notify();
                    }),
                )
        };
        let g = self.roll.grid;
        let slider =
            |ws: &Self, label: &'static str, s: Slider, text: String, cx: &mut Context<Self>| {
                div()
                    .h(px(18.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(div().w(px(46.)).child(label))
                    .child(ws.slider_bar(s, cx).flex_1().min_w(px(0.)))
                    .child(
                        div()
                            .w(px(30.))
                            .flex()
                            .justify_end()
                            .font_family(MONO)
                            .text_color(rgb(TEXT_2))
                            .child(text),
                    )
            };
        // The tools work on the picked notes (or all of them); with no region, greyed out.
        let tool = |label: &'static str, edit: NoteEdit, cx: &mut Context<Self>| {
            let b = button(label, false)
                .h(px(24.))
                .text_size(px(11.))
                .debug_selector(move || format!("roll-{label}"));
            match at {
                Some(at) => b.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.edit_notes(at, edit, cx)),
                ),
                None => b.opacity(0.4).cursor_default(),
            }
        };
        let picked = self.picked.len();
        div()
            .id("roll-tools")
            .flex_1()
            .min_w(px(0.))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child("Quantize"),
                    )
                    .child(
                        tray()
                            .p(px(2.))
                            .rounded(px(7.))
                            .child(grid_seg("1/4", PPQ, g == PPQ, cx))
                            .child(grid_seg("1/8", PPQ / 2, g == PPQ / 2, cx))
                            .child(grid_seg("1/16", PPQ / 4, g == PPQ / 4, cx))
                            .child(grid_seg("1/32", PPQ / 8, g == PPQ / 8, cx)),
                    ),
            )
            .child(slider(
                self,
                "Strength",
                Slider::Strength,
                format!("{:.0}%", self.roll.strength * 100.0),
                cx,
            ))
            .child(slider(
                self,
                "Swing",
                Slider::Swing,
                format!("{:.0}%", self.roll.swing),
                cx,
            ))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child(match picked {
                        0 => "Tools · on every note".to_string(),
                        1 => "Tools · on the picked note".to_string(),
                        n => format!("Tools · on {n} picked notes"),
                    }),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(4.))
                    .child(tool(
                        "Humanize",
                        NoteEdit::Humanize {
                            timing: 20,
                            velocity: 8,
                            seed: fresh_seed(),
                        },
                        cx,
                    ))
                    .child(tool("Legato", NoteEdit::Legato, cx))
                    .child(tool("Chop", NoteEdit::Chop { grid: g }, cx))
                    .child(tool("Arpeggiate", NoteEdit::Arpeggiate { grid: g }, cx))
                    .child(tool("Strum", NoteEdit::Strum { ticks: PPQ / 32 }, cx))
                    .child(tool("Glue", NoteEdit::Glue, cx))
                    .child(tool("Reverse", NoteEdit::Reverse, cx))
                    .child(tool("Flip", NoteEdit::Flip, cx))
                    .child(tool(
                        "Softer",
                        NoteEdit::Dynamics {
                            percent: 100,
                            by: -10,
                        },
                        cx,
                    ))
                    .child(tool(
                        "Harder",
                        NoteEdit::Dynamics {
                            percent: 100,
                            by: 10,
                        },
                        cx,
                    )),
            )
    }

    pub fn roll_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx, sy, kh) = (
            self.px_per_tick(),
            self.scroll_x,
            self.roll_scroll_y,
            self.roll.key_h,
        );
        let (beat_ticks, bar_ticks) = (self.session.project.beat(), self.session.project.bar());
        let at = self.roll_region();
        let color = at.map_or(ACCENT, |at| self.color(at.0));
        let project = &self.session.project;
        let span = at.map(|at| {
            let r = project.region(at);
            (r.start, r.start + r.length)
        });
        let track = at.map(|at| project.tracks[at.0].id);
        let key = self.roll.scale.then(|| project.song_key()).flatten();
        // Names spelled by the key, scale shown or not.
        let spelling = project.song_key();
        let hover = self.roll_hover;
        let by_velocity = self.roll.by_velocity;

        // Pitches sounding now: the region's notes under the playhead, and a key held down.
        let tick = self.playhead();
        let mut sounding = [false; 128];
        if let (Some(at), Some((start, _))) = (at, span)
            && self.player.playing()
            && let Content::Midi { notes, .. } = &project.region(at).content
        {
            for n in notes {
                let on = start as i64 + n.start;
                if (on..on + n.length as i64).contains(&(tick as i64)) {
                    sounding[n.pitch as usize] = true;
                }
            }
        }
        match &self.drag {
            Some(Drag::Key { pitch, .. }) => sounding[*pitch as usize] = true,
            Some(Drag::Paint { .. }) => self
                .painting
                .iter()
                .for_each(|&p| sounding[p as usize] = true),
            _ => {}
        }

        let bounds = self.roll_bounds.clone();
        let grid = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                let w = f32::from(b.size.width);
                let quad = |window: &mut Window, x: f32, y: f32, ww: f32, hh: f32, c: Hsla| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        c,
                    ));
                };
                let first = (sy / kh) as i32;
                let last = ((sy + f32::from(b.size.height)) / kh) as i32 + 1;
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    let y = row as f32 * kh - sy;
                    let bg = match key {
                        Some(k) if k.is_root(pitch) => Hsla::from(rgb(0x15202c)),
                        Some(k) if k.contains(pitch) => rgb(0x131318).into(),
                        Some(_) => rgb(0x0d0d10).into(),
                        None if black(pitch) => rgb(0x0f0f12).into(),
                        None => rgb(0x141418).into(),
                    };
                    quad(window, 0.0, y, w, kh, bg);
                    if hover == Some(pitch) {
                        quad(window, 0.0, y, w, kh, alpha(0xffffff, 0.035));
                    }
                    let line = if pitch.is_multiple_of(12) {
                        0x24242b
                    } else {
                        0x17171c
                    };
                    quad(window, 0.0, y + kh - 1.0, w, 1.0, rgb(line).into());
                }
                let sixteenth = ppt * (PPQ / 4) as f32;
                let fine = sixteenth >= 6.0;
                let step = if fine { PPQ / 4 } else { beat_ticks };
                let step_px = ppt * step as f32;
                let first = (sx / step_px) as u64;
                let last = ((sx + w) / step_px) as u64 + 1;
                for k in first..=last {
                    let t = k * step;
                    let color = if t % bar_ticks == 0 {
                        0x34343d
                    } else if t % beat_ticks == 0 {
                        0x24242b
                    } else {
                        0x17171c
                    };
                    let x = (t as f32 * ppt - sx).round();
                    quad(
                        window,
                        x,
                        0.0,
                        1.0,
                        f32::from(b.size.height),
                        rgb(color).into(),
                    );
                }
                // Shade what's outside the region.
                if let Some((start, end)) = span {
                    let (x0, x1) = (start as f32 * ppt - sx, end as f32 * ppt - sx);
                    let shade = alpha(0x000000, 0.4);
                    if x0 > 0.0 {
                        quad(window, 0.0, 0.0, x0, f32::from(b.size.height), shade);
                    }
                    if x1 < w {
                        quad(
                            window,
                            x1.max(0.0),
                            0.0,
                            w - x1.max(0.0),
                            f32::from(b.size.height),
                            shade,
                        );
                    }
                }
            },
        )
        .absolute()
        .size_full();

        // Another track's notes, faintly behind.
        let mut ghosts = Vec::new();
        if let Some(ghost) = self.roll.ghost.filter(|&g| Some(g) != track)
            && let Some(t) = project.tracks.iter().find(|t| t.id == ghost)
        {
            let width = f32::from(self.roll_bounds.get().size.width).max(400.0);
            for r in &t.regions {
                let Some(notes) = r.content.notes(r.length) else {
                    continue;
                };
                for n in notes.iter() {
                    let x = (r.start as i64 + n.start) as f32 * ppt - sx;
                    let w = (n.length as f32 * ppt).max(3.0);
                    if x + w < 0.0 || x > width {
                        continue;
                    }
                    ghosts.push(
                        div()
                            .absolute()
                            .left(px(x))
                            .top(px((127 - n.pitch) as f32 * kh - sy + 1.))
                            .w(px(w - 1.0))
                            .h(px(kh - 2.))
                            .rounded(px(3.))
                            .border_1()
                            .border_dashed()
                            .border_color(alpha(TEXT, 0.22))
                            .bg(alpha(TEXT, 0.035)),
                    );
                }
            }
        }

        let mut notes = Vec::new();
        let mut stems = Vec::new();
        let mut tip = None;
        let lane = self.roll.lane;
        let mut curve: Vec<(f32, f32)> = Vec::new();
        let mut curve_now = None;
        if let (Some(at), Some((start, end))) = (at, span) {
            let Content::Midi {
                notes: list,
                controls,
            } = &project.region(at).content
            else {
                unreachable!()
            };
            for (i, n) in list.iter().enumerate() {
                let x = (start as i64 + n.start) as f32 * ppt - sx;
                let w = (n.length as f32 * ppt).max(3.0);
                let y = (127 - n.pitch) as f32 * kh - sy;
                let picked = self.picked.contains(&i);
                let n = *n;
                let ink = if by_velocity {
                    velocity_color(n.velocity)
                } else {
                    color
                };
                let strength = if by_velocity {
                    0.92
                } else {
                    0.55 + 0.45 * n.velocity as f32 / 127.0
                };
                let label = w >= 26.0 && kh >= 10.0;
                notes.push(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y + 1.))
                        .w(px(w - 1.0))
                        .h(px(kh - 2.))
                        .pl(px(5.))
                        .rounded(px(3.))
                        .overflow_hidden()
                        .bg(if picked {
                            Hsla::from(rgb(tint(ink, 0.55)))
                        } else {
                            alpha(ink, strength)
                        })
                        .when(picked, |d| d.border_1().border_color(rgb(0xffffff)))
                        .font_family(MONO)
                        .text_size(px(8.5))
                        .line_height(px(kh - 2.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(BG))
                        .whitespace_nowrap()
                        .cursor(match self.tool {
                            Tool::Eraser => gpui::CursorStyle::OperationNotAllowed,
                            Tool::Scissors => gpui::CursorStyle::IBeam,
                            _ => gpui::CursorStyle::OpenHand,
                        })
                        .when(label, |d| d.child(name_in(spelling, n.pitch)))
                        // Its velocity, as a line along its foot as long as it's hard.
                        .when(kh >= 9.0 && w > 8.0, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .bottom_0()
                                    .h(px(2.))
                                    .w(px((w - 1.0) * n.velocity as f32 / 127.0))
                                    .bg(alpha(0x000000, 0.35)),
                            )
                        })
                        .debug_selector(move || format!("note-{i}"))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .w(px(4.))
                                .h_full()
                                .cursor_ew_resize()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    grab_note(at, i, Edge::Start, cx),
                                ),
                        )
                        .child(
                            div()
                                .absolute()
                                .right_0()
                                .top_0()
                                .w(px(5.))
                                .h_full()
                                .cursor_ew_resize()
                                .on_mouse_down(MouseButton::Left, grab_note(at, i, Edge::End, cx)),
                        )
                        .on_mouse_down(MouseButton::Left, grab_note(at, i, Edge::Move, cx))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.roll_menu(e, Some(i), cx);
                            }),
                        ),
                );
                if lane == Lane::Velocity {
                    let h = n.velocity as f32 / 127.0 * VELOCITY_H;
                    let dot = if picked {
                        Hsla::from(rgb(0xffffff))
                    } else {
                        rgb(ink).into()
                    };
                    stems.push(
                        div()
                            .absolute()
                            .left(px(x))
                            .bottom(px(5.))
                            .w(px(2.))
                            .h(px(h))
                            .bg(dot),
                    );
                    stems.push(
                        div()
                            .absolute()
                            .left(px(x - 3.))
                            .bottom(px(5.0 + h - 4.0))
                            .size(px(8.))
                            .rounded(px(4.))
                            .bg(dot)
                            .cursor_ns_resize()
                            .debug_selector(move || format!("velocity-{i}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    // The dot's note and, if it's picked, every picked note.
                                    if !ws.picked.contains(&i) {
                                        ws.picked = vec![i];
                                    }
                                    let notes = ws
                                        .session
                                        .project
                                        .region(at)
                                        .content
                                        .notes(0)
                                        .map(|n| n.to_vec())
                                        .unwrap_or_default();
                                    ws.held = ws
                                        .picked
                                        .iter()
                                        .filter_map(|&k| notes.get(k).map(|n| (k, *n)))
                                        .collect();
                                    let y = f32::from(e.position.y);
                                    ws.begin_drag(Drag::Velocity { at, y }, cx);
                                }),
                            ),
                    );
                }
            }
            // The tooltip, for the note grabbed (or picked alone).
            let lead = match &self.drag {
                Some(Drag::Notes { lead, .. }) => Some(*lead),
                _ if self.picked.len() == 1 => self.picked.first().copied(),
                _ => None,
            };
            if let Some(n) = lead.and_then(|i| list.get(i)) {
                let x = (start as i64 + n.start) as f32 * ppt - sx;
                let w = (n.length as f32 * ppt).max(3.0);
                let y = (127 - n.pitch) as f32 * kh - sy;
                let at_tick = (start as i64 + n.start).max(0) as u64;
                tip = Some(
                    div()
                        .absolute()
                        .left(px(x + w + 6.))
                        .top(px(y - 2.))
                        .px(px(6.))
                        .py(px(1.))
                        .rounded(px(4.))
                        .bg(rgb(TEXT))
                        .text_color(rgb(BG))
                        .font_family(MONO)
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .child(format!(
                            "{} · {} · {} · vel {}",
                            name_in(spelling, n.pitch),
                            self.position(at_tick),
                            length_name(n.length),
                            n.velocity
                        )),
                );
            }
            // The lane's controller, as a held line.
            if lane != Lane::Velocity {
                let mut points: Vec<&ControlPoint> =
                    controls.iter().filter(|c| lane.holds(&c.kind)).collect();
                points.sort_by_key(|c| c.start);
                for c in &points {
                    curve.push((
                        (start as i64 + c.start) as f32 * ppt - sx,
                        lane.level(&c.kind),
                    ));
                }
                let now = tick as i64 - start as i64;
                curve_now = points
                    .iter()
                    .rev()
                    .find(|c| c.start <= now)
                    .map(|c| lane.say(&c.kind));
                let _ = end;
            }
        }

        // The rubber band.
        let band = match &self.drag {
            Some(Drag::Marquee { from, to, .. }) => {
                let o = self.roll_bounds.get().origin;
                let (x0, y0) = (
                    from.x.min(to.x) - f32::from(o.x),
                    from.y.min(to.y) - f32::from(o.y),
                );
                let (w, h) = ((from.x - to.x).abs(), (from.y - to.y).abs());
                Some(
                    div()
                        .absolute()
                        .left(px(x0))
                        .top(px(y0))
                        .w(px(w))
                        .h(px(h))
                        .rounded(px(2.))
                        .border_1()
                        .border_color(alpha(ACCENT, 0.9))
                        .bg(alpha(ACCENT, 0.12)),
                )
            }
            _ => None,
        };

        // The keyboard: click a key to hear it.
        let keys = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let w = f32::from(b.size.width);
                let first = (sy / kh) as i32;
                let last = ((sy + f32::from(b.size.height)) / kh) as i32 + 1;
                window.paint_quad(fill(b, rgb(0xd9d9df)));
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    let y = b.origin.y + px(row as f32 * kh - sy);
                    let line = |window: &mut Window, at: f32| {
                        window.paint_quad(fill(
                            Bounds::new(point(b.origin.x, y + px(at)), size(px(w), px(1.))),
                            rgb(0xb0b0ba),
                        ));
                    };
                    if black(pitch) {
                        line(window, kh / 2.0);
                    } else {
                        if sounding[pitch as usize] {
                            window.paint_quad(fill(
                                Bounds::new(point(b.origin.x, y), size(px(w), px(kh))),
                                rgb(color),
                            ));
                        } else if hover == Some(pitch) {
                            window.paint_quad(fill(
                                Bounds::new(point(b.origin.x, y), size(px(w), px(kh))),
                                rgb(0xc8c8d0),
                            ));
                        }
                        if pitch > 0 && !black(pitch - 1) {
                            line(window, kh - 0.5);
                        }
                    }
                }
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    // The song's scale down the keys' edge: its notes marked, its tonic bright.
                    if let Some(k) = key.filter(|k| k.contains(pitch)) {
                        let y = b.origin.y + px(row as f32 * kh - sy + 1.0);
                        let mark = Bounds::new(
                            point(b.origin.x + px(w - 4.0), y),
                            size(px(4.), px(kh - 2.)),
                        );
                        let c = match k.is_root(pitch) {
                            true => Hsla::from(rgb(ACCENT)),
                            false => alpha(ACCENT, 0.45),
                        };
                        window.paint_quad(fill(mark, c));
                    }
                    if black(pitch) {
                        let y = b.origin.y + px(row as f32 * kh - sy + 1.0);
                        let key = Bounds::new(point(b.origin.x, y), size(px(w * 0.6), px(kh - 2.)));
                        let c = match (sounding[pitch as usize], hover == Some(pitch)) {
                            (true, _) => rgb(color),
                            (_, true) => rgb(0x34343c),
                            _ => rgb(0x18181c),
                        };
                        window.paint_quad(fill(key, c).corner_radii(gpui::Corners {
                            top_left: px(0.),
                            bottom_left: px(0.),
                            top_right: px(3.),
                            bottom_right: px(3.),
                        }));
                    }
                }
            },
        )
        .absolute()
        .size_full();
        // Every C, and the song's tonic, named; and the pitch under the mouse.
        let tonic = |p: u8| key.is_some_and(|k| k.is_root(p));
        let labels = (0..=127u8)
            .filter(|&p| (p % 12 == 0 || (tonic(p) && !black(p)) || hover == Some(p)) && kh >= 9.0)
            .map(|p| {
                div()
                    .absolute()
                    .right(px(7.))
                    .top(px((127 - p) as f32 * kh - sy + (kh - 10.0) / 2.0))
                    .font_family(MONO)
                    .text_size(px(9.))
                    .line_height(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(match (tonic(p), hover == Some(p)) {
                        (true, _) => ACCENT,
                        (_, true) if black(p) => TEXT,
                        _ => 0x5a5a64,
                    }))
                    .child(name_in(spelling, p))
            });
        let keyboard = div()
            .w(px(KEYS_W))
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(ROLL_RULER))
                    .flex_none()
                    .bg(rgb(WELL))
                    .border_b_1()
                    .border_color(rgb(LINE)),
            )
            .child(
                div()
                    .flex_1()
                    .relative()
                    .overflow_hidden()
                    .child(keys)
                    .children(labels)
                    .when_some(track, |d, track| {
                        d.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                let pitch = ws.pitch_at(f32::from(e.position.y));
                                // ⌘-click picks every note of the key's pitch.
                                if e.modifiers.platform {
                                    ws.roll_op(RollOp::SelectPitch(pitch), cx);
                                    return;
                                }
                                ws.player.preview(track, pitch, 100);
                                ws.begin_drag(Drag::Key { track, pitch }, cx);
                            }),
                        )
                    })
                    .on_mouse_move(
                        cx.listener(|ws, e: &MouseMoveEvent, _, cx| ws.hover_roll(e, cx)),
                    )
                    .on_scroll_wheel(cx.listener(|ws, e, _, cx| ws.roll_scroll(e, cx))),
            );

        let bar_px = ppt * bar_ticks as f32;
        let width = f32::from(self.roll_bounds.get().size.width).max(400.0);
        let every = (40.0 / bar_px).ceil().max(1.0) as u64;
        let bars = ((sx / bar_px) as u64..=((sx + width) / bar_px) as u64 + 1)
            .filter(|b| b % every == 0)
            .map(|b| {
                div()
                    .absolute()
                    .left(px(b as f32 * bar_px - sx + 5.))
                    .top(px(4.))
                    .font_family(MONO)
                    .text_size(px(10.))
                    .line_height(px(12.))
                    .text_color(rgb(TEXT_2))
                    .child(format!("{}", b + 1))
            });
        let playhead = tick as f32 * ppt - sx;
        let tools = self.roll_tools(at, cx);

        // The lane: a tab for each thing it can show.
        let lane_tabs = Lane::ALL.map(|l| {
            seg(l.short(), lane == l, 20.)
                .flex_1()
                .px(px(0.))
                .text_size(px(10.5))
                .debug_selector(move || format!("lane-{}", l.short()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.roll.lane = l;
                        cx.notify();
                    }),
                )
        });
        let lane_bounds = self.lane_bounds.clone();
        let lane_view = canvas(
            move |b, _, _| lane_bounds.set(b),
            move |b, _, window, _| {
                if lane == Lane::Velocity {
                    return;
                }
                let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let floor = y0 + h - 5.0;
                let y_of = |level: f32| floor - level * VELOCITY_H;
                if lane == Lane::Bend {
                    window.paint_quad(fill(
                        Bounds::new(point(px(x0), px(y_of(0.5))), size(px(w), px(1.))),
                        alpha(0xffffff, 0.12),
                    ));
                }
                for (k, &(x, level)) in curve.iter().enumerate() {
                    let next = curve.get(k + 1).map_or(w, |c| c.0).min(w);
                    let (left, right) = (x.max(0.0), next);
                    if right <= left {
                        continue;
                    }
                    let base = if lane == Lane::Bend { y_of(0.5) } else { floor };
                    let top = y_of(level);
                    let (a, bb) = (top.min(base), top.max(base));
                    window.paint_quad(fill(
                        Bounds::new(
                            point(px(x0 + left), px(a)),
                            size(px(right - left), px((bb - a).max(1.0))),
                        ),
                        alpha(color, 0.22),
                    ));
                    window.paint_quad(fill(
                        Bounds::new(
                            point(px(x0 + left), px(top - 1.0)),
                            size(px(right - left), px(2.)),
                        ),
                        rgb(color),
                    ));
                    if x >= 0.0 {
                        window.paint_quad(
                            fill(
                                Bounds::new(
                                    point(px(x0 + x - 2.0), px(top - 3.0)),
                                    size(px(5.), px(5.)),
                                ),
                                rgb(0xffffff),
                            )
                            .corner_radii(gpui::Corners::all(px(2.5))),
                        );
                    }
                }
            },
        )
        .absolute()
        .size_full();

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .child(
                div()
                    .w(px(self.header_w))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(rgb(SURFACE))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(div().flex_1().min_h(px(0.)).flex().child(tools).child(keyboard))
                    .child(
                        div()
                            .h(px(VELOCITY_LANE))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap(px(6.))
                            .px(px(12.))
                            .border_t_1()
                            .border_color(rgb(LINE))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(div().font_weight(FontWeight::MEDIUM).child(lane.name()))
                                    .child(div().flex_1().text_size(px(11.)).text_color(rgb(TEXT_3)).whitespace_nowrap().overflow_hidden().child(
                                        match (lane, &curve_now) {
                                            (Lane::Velocity, _) => "drag a dot, or draw across".to_string(),
                                            (_, Some(v)) => format!("now {v} · draw, ⌥ to erase"),
                                            _ => "draw it, ⌥ to erase".to_string(),
                                        },
                                    )),
                            )
                            .child(tray().p(px(2.)).rounded(px(7.)).children(lane_tabs)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .relative()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(ROLL_RULER))
                            .flex_none()
                            .relative()
                            .overflow_hidden()
                            .bg(rgb(WELL))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .cursor_pointer()
                            .children(bars)
                            .debug_selector(|| "roll-ruler".into())
                            // Click the ruler to move the playhead there.
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    let x = f32::from(e.position.x) - f32::from(ws.roll_bounds.get().origin.x);
                                    let tick = ((x + ws.scroll_x) / ws.px_per_tick()).max(0.0);
                                    let tick = ws.snap(tick as f64);
                                    ws.locate_tick(tick, cx);
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.))
                            .relative()
                            .overflow_hidden()
                            .debug_selector(|| "roll-grid".into())
                            .child(grid)
                            .children(ghosts)
                            .children(notes)
                            .children(band)
                            .children(tip)
                            .when(at.is_none(), |d| {
                                d.child(
                                    div()
                                        .absolute()
                                        .top(px(20.))
                                        .left(px(20.))
                                        .text_color(rgb(TEXT_3))
                                        .child("Select a MIDI region to edit its notes, or draw one with the pencil."),
                                )
                            })
                            .on_mouse_down(MouseButton::Left, cx.listener(|ws, e, _, cx| ws.roll_click(e, cx)))
                            .on_mouse_down(MouseButton::Right, cx.listener(|ws, e: &MouseDownEvent, _, cx| ws.roll_menu(e, None, cx)))
                            .on_mouse_move(cx.listener(|ws, e: &MouseMoveEvent, _, cx| ws.hover_roll(e, cx)))
                            .on_scroll_wheel(cx.listener(|ws, e, _, cx| ws.roll_scroll(e, cx))),
                    )
                    .child(
                        div()
                            .h(px(VELOCITY_LANE))
                            .flex_none()
                            .relative()
                            .overflow_hidden()
                            .bg(rgb(WELL))
                            .border_t_1()
                            .border_color(rgb(LINE))
                            .debug_selector(|| "roll-lane".into())
                            .child(lane_view)
                            .children(stems)
                            .on_mouse_down(MouseButton::Left, cx.listener(|ws, e, _, cx| ws.lane_down(e, cx))),
                    )
                    .child(div().absolute().left(px(playhead)).top_0().w(px(1.)).h_full().bg(rgb(TEXT))),
            )
    }

    fn hover_roll(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        let pitch = Some(self.pitch_at(f32::from(e.position.y)));
        if self.roll_hover != pitch {
            self.roll_hover = pitch;
            cx.notify();
        }
    }

    /// The piano roll's own controls in the editor's bar: scale, writing in key, chord stamps,
    /// ghost notes, colour by velocity, quantize, fit and zoom.
    pub fn roll_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let key = self.session.project.song_key();
        let scale_label = key.map_or("Scale".to_string(), |k| k.short());
        let ghost = self
            .roll
            .ghost
            .and_then(|g| self.session.project.tracks.iter().find(|t| t.id == g))
            .map_or("Ghost".to_string(), |t| t.name.clone());
        let at = self.roll_region();
        let grid = self.roll.grid;
        let stamp = self.roll.stamp;
        div()
            .flex()
            .items_center()
            .gap(px(5.))
            .min_w(px(0.))
            .overflow_hidden()
            .child(
                chip_button(Icon::Scale, scale_label, self.roll.scale)
                    .debug_selector(|| "roll-scale".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.roll.scale ^= true;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                chip_text("In key", self.roll.in_key)
                    .debug_selector(|| "roll-in-key".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.roll.in_key ^= true;
                            if ws.roll.in_key && ws.session.project.song_key().is_none() {
                                ws.status = Some(
                                    "The song has no key yet: set one in the control bar.".into(),
                                );
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                chip_text(
                    if stamp == Stamp::Note {
                        "Chords"
                    } else {
                        stamp.name()
                    },
                    stamp != Stamp::Note,
                )
                .debug_selector(|| "roll-stamp".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                        let items = Stamp::ALL
                            .iter()
                            .map(|&s| (s.name().to_string(), MenuAction::Stamp(s)))
                            .collect();
                        ws.menu = Some(Menu {
                            at: e.position,
                            items,
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                ),
            )
            .child(
                chip_button(Icon::Ghost, ghost, self.roll.ghost.is_some())
                    .debug_selector(|| "roll-ghost".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            let mut items = vec![("Off".to_string(), MenuAction::Ghost(None))];
                            let shown = ws
                                .roll_region()
                                .map(|at| ws.session.project.tracks[at.0].id);
                            items.extend(
                                ws.session
                                    .project
                                    .tracks
                                    .iter()
                                    .filter(|t| t.instrument.is_some() && Some(t.id) != shown)
                                    .map(|t| (t.name.clone(), MenuAction::Ghost(Some(t.id)))),
                            );
                            ws.menu = Some(Menu {
                                at: e.position,
                                items,
                                ..Default::default()
                            });
                            cx.notify();
                        }),
                    ),
            )
            .child(
                chip(self.roll.by_velocity)
                    // A swatch of the colours velocities take.
                    .child(
                        div().flex().rounded(px(2.)).overflow_hidden().children(
                            [20u8, 64, 96, 127]
                                .map(|v| div().w(px(3.)).h(px(10.)).bg(rgb(velocity_color(v)))),
                        ),
                    )
                    .child("Velocity")
                    .debug_selector(|| "roll-colours".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.roll.by_velocity ^= true;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                button(
                    format!(
                        "Q {}",
                        crate::workspace::inspector::grid_name(grid).trim_end_matches(" note")
                    ),
                    false,
                )
                .debug_selector(|| "roll-quantize".into())
                .when(at.is_none(), |d| d.opacity(0.4).cursor_default())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.roll_op(RollOp::Quantize, cx)),
                ),
            )
            .child(
                button("Fit", false)
                    .debug_selector(|| "roll-fit".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.fit_roll();
                            cx.notify();
                        }),
                    ),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(rgb(LINE)))
            .child(
                div()
                    .w(px(84.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(self.slider_bar(Slider::Zoom, cx).flex_1()),
            )
    }
}

/// A note's length as a fraction of a bar of 4/4: "1/16", "3/8", "1/1".
fn length_name(ticks: u64) -> String {
    let whole = PPQ * 4;
    let gcd = |mut a: u64, mut b: u64| {
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    let g = gcd(ticks, whole).max(1);
    match whole / g {
        d if d <= 64 => format!("{}/{d}", ticks / g),
        _ => format!("{ticks} ticks"),
    }
}

/// A different seed each time, so humanizing twice isn't the same nudge twice.
fn fresh_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

/// A toggle with an icon, tinted with the accent while on.
fn chip_button(i: Icon, label: String, on: bool) -> Div {
    chip(on)
        .child(icon(i, 13.).text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 })))
        .child(label)
}

/// A toggle with words alone.
fn chip_text(label: &str, on: bool) -> Div {
    chip(on).child(label.to_string())
}

fn chip(on: bool) -> Div {
    div()
        .h(px(26.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(7.))
        .cursor_pointer()
        .whitespace_nowrap()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .bg(if on {
            alpha(ACCENT, 0.16)
        } else {
            rgb(0x23232a).into()
        })
        .text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 }))
        .hover(|s| s.bg(rgb(RAISED)))
}

/// Grabs note `i` of a region: picking it (shift adds or takes it away), ⌥ copying the picked
/// notes to drag the copies, and sounding it; or erases or cuts it, with those tools.
fn grab_note(
    at: At,
    i: usize,
    edge: Edge,
    cx: &mut Context<Workspace>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
        cx.stop_propagation();
        let Some(n) = ws
            .session
            .project
            .region(at)
            .content
            .notes(0)
            .and_then(|n| n.get(i).copied())
        else {
            return;
        };
        match ws.tool {
            Tool::Eraser => {
                ws.edit(cx, |ws| {
                    if let Some(notes) = ws.session.project.notes_mut(at) {
                        notes.remove(i);
                    }
                    ws.picked.clear();
                });
                return;
            }
            Tool::Scissors => {
                let start = ws.session.project.region(at).start as i64;
                let cut = ws.snap(ws.tick_at(f32::from(e.position.x))) as i64 - start;
                if cut > n.start && cut < n.start + n.length as i64 {
                    ws.edit(cx, |ws| {
                        if let Some(notes) = ws.session.project.notes_mut(at) {
                            let first = (cut - n.start) as u64;
                            notes[i].length = first;
                            notes.push(Note {
                                start: cut,
                                length: n.length - first,
                                ..n
                            });
                        }
                    });
                }
                return;
            }
            _ => {}
        }
        if e.modifiers.shift {
            match ws.picked.iter().position(|&k| k == i) {
                Some(k) => {
                    ws.picked.remove(k);
                    cx.notify();
                    return;
                }
                None => ws.picked.push(i),
            }
        } else if !ws.picked.contains(&i) {
            ws.picked = vec![i];
        }
        ws.region = Some(at);
        // The length new notes are written at: this one's.
        ws.roll.length = n.length;
        let track = ws.session.project.tracks[at.0].id;
        ws.player.preview(track, n.pitch, n.velocity);
        let from = e.position.map(f32::from);
        ws.session.checkpoint();
        let mut lead = i;
        // ⌥: the picked notes copied, and the copies dragged.
        if e.modifiers.alt && edge == Edge::Move {
            let Some(notes) = ws.session.project.notes_mut(at) else {
                return;
            };
            let first = notes.len();
            let copies: Vec<Note> = ws
                .picked
                .iter()
                .filter_map(|&k| notes.get(k).copied())
                .collect();
            lead = first + ws.picked.iter().position(|&k| k == i).unwrap_or(0);
            notes.extend(copies);
            ws.picked = (first..notes.len()).collect();
        }
        let notes = ws
            .session
            .project
            .region(at)
            .content
            .notes(0)
            .map(|n| n.to_vec())
            .unwrap_or_default();
        ws.held = ws
            .picked
            .iter()
            .filter_map(|&k| notes.get(k).map(|n| (k, *n)))
            .collect();
        ws.drag = Some(Drag::Notes {
            at,
            lead,
            edge,
            from,
            moved: false,
        });
        cx.notify();
    })
}

#[cfg(test)]
mod spelling;
