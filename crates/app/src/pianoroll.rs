//! The piano roll: the selected MIDI region's notes against a keyboard, in the editor under the
//! tracks. It shares the tracks area's zoom and horizontal scroll, so its grid lines up with the
//! lanes above. Rows out of the song's key are darker; another track's notes can show faintly
//! behind; velocities stand in a lane underneath. Double-click to add a note (a click, with the
//! pencil); drag a note to move it, its right edge to resize it.

use gpui::{
    App, Bounds, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent, ScrollDelta,
    ScrollWheelEvent, Window, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use tempo_engine::{
    music::{Key, NoteEdit},
    project::{At, Content, Note, PPQ},
};

use crate::{
    menu::{Menu, MenuAction},
    theme::*,
    workspace::{Drag, Edge, Slider, Tool, Workspace},
};

/// The height of one pitch.
pub const KEY_H: f32 = 12.0;
/// The velocity lane, and how tall a full velocity stands in it.
pub const VELOCITY_LANE: f32 = 56.0;
pub const VELOCITY_H: f32 = 42.0;
const ROLL_RULER: f32 = 20.0;
const KEYS_W: f32 = 56.0;

/// The scroll that puts `pitch` near the top.
pub fn scroll_to(pitch: u8) -> f32 {
    ((127 - pitch as i32) as f32 * KEY_H - KEY_H * 2.0).max(0.0)
}

fn black(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// Logic's names: middle C (60) is C3.
pub fn name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
    ];
    format!("{}{}", NAMES[pitch as usize % 12], pitch as i32 / 12 - 2)
}

/// A pitch's name as a key spells it (B♭ in F major, A♯ in E major), middle C being C3 as in
/// Logic; with no key, sharps.
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

impl Workspace {
    /// A pitch's name in the song's key.
    pub fn note_name(&self, pitch: u8) -> String {
        name_in(self.session.project.song_key(), pitch)
    }
}

impl Workspace {
    /// The MIDI region the piano roll shows: the selected one, if it's MIDI.
    pub fn roll_region(&self) -> Option<At> {
        self.region.filter(|&at| {
            matches!(
                self.session.project.region(at).content,
                Content::Midi { .. }
            )
        })
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
            self.roll_scroll_y = scroll_to(top.saturating_add(4).min(127));
        }
    }

    /// The window y of a pitch's row in the piano roll.
    #[cfg(test)]
    pub fn pitch_y(&self, pitch: u8) -> f32 {
        let top = f32::from(self.roll_bounds.get().origin.y);
        top + (127 - pitch) as f32 * KEY_H - self.roll_scroll_y
    }

    fn pitch_at(&self, y: f32) -> u8 {
        let top = f32::from(self.roll_bounds.get().origin.y);
        let row = ((y - top + self.roll_scroll_y) / KEY_H).floor();
        (127.0 - row).clamp(0.0, 127.0) as u8
    }

    fn roll_scroll(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => p.map(f32::from),
            ScrollDelta::Lines(l) => l.map(|v| v * 24.0),
        };
        if e.modifiers.platform {
            let x = f32::from(e.position.x) - f32::from(self.roll_bounds.get().origin.x);
            self.zoom_around(self.px_per_beat * (1.0 + d.y * 0.01), x);
        } else {
            let height = f32::from(self.roll_bounds.get().size.height);
            let most = (128.0 * KEY_H - height).max(0.0);
            self.roll_scroll_y = (self.roll_scroll_y - d.y).clamp(0.0, most);
            self.scroll_x = (self.scroll_x - d.x).max(0.0);
        }
        cx.notify();
    }

    /// A double-click on empty grid (or a click, with the pencil) adds a note there, a grid step
    /// long; a single click deselects.
    fn roll_click(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        self.note = None;
        let adds = e.click_count == 2 || self.tool == Tool::Pencil;
        let Some(at) = self.roll_region().filter(|_| adds) else {
            cx.notify();
            return;
        };
        let p = e.position.map(f32::from);
        let tick = self.snap_down(self.tick_at(p.x));
        let region = self.session.project.region(at);
        if tick < region.start || tick >= region.start + region.length {
            return;
        }
        let note = Note {
            start: (tick - region.start) as i64,
            length: self.grid(),
            pitch: self.pitch_at(p.y),
            velocity: 100,
        };
        let track = self.session.project.tracks[at.0].id;
        self.edit(cx, |ws| {
            let notes = ws.session.project.notes_mut(at).unwrap();
            notes.push(note);
            ws.note = Some(notes.len() - 1);
        });
        self.player.preview(track, note.pitch, note.velocity);
        self.begin_drag(
            Drag::Key {
                track,
                pitch: note.pitch,
            },
            cx,
        );
    }

    /// The quantize grid, strength, swing and one-click tools, left of the keyboard.
    fn roll_tools(&mut self, at: Option<At>, cx: &mut Context<Self>) -> Div {
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
        // The tools work on a MIDI region's notes; without one they're greyed out.
        let tool = |label: &'static str, edit: NoteEdit, cx: &mut Context<Self>| {
            let b = button(label, false).debug_selector(move || format!("roll-{label}"));
            match at {
                Some(at) => b.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.edit_notes(at, edit, cx)),
                ),
                None => b.opacity(0.4).cursor_default(),
            }
        };
        div()
            .flex_1()
            .min_w(px(0.))
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
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
                    .mt(px(2.))
                    .grid()
                    .grid_cols(2)
                    .gap(px(5.))
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
                    .child(tool("Arpeggiate", NoteEdit::Arpeggiate { grid: g }, cx)),
            )
    }

    pub fn roll_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx, sy) = (self.px_per_tick(), self.scroll_x, self.roll_scroll_y);
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

        // Pitches sounding now: the region's notes under the playhead, and a key held down.
        let tick = self.playhead();
        let mut sounding = [false; 128];
        if let (Some(at), Some((start, _))) = (at, span)
            && self.player.playing()
            && let Content::Midi { notes } = &project.region(at).content
        {
            for n in notes {
                let on = start as i64 + n.start;
                if (on..on + n.length as i64).contains(&(tick as i64)) {
                    sounding[n.pitch as usize] = true;
                }
            }
        }
        if let Some(Drag::Key { pitch, .. }) = self.drag {
            sounding[pitch as usize] = true;
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
                let first = (sy / KEY_H) as i32;
                let last = ((sy + f32::from(b.size.height)) / KEY_H) as i32 + 1;
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    let y = row as f32 * KEY_H - sy;
                    let bg = match key {
                        Some(k) if k.is_root(pitch) => Hsla::from(rgb(0x15202c)),
                        Some(k) if k.contains(pitch) => rgb(0x131318).into(),
                        Some(_) => rgb(0x0d0d10).into(),
                        None if black(pitch) => rgb(0x0f0f12).into(),
                        None => rgb(0x141418).into(),
                    };
                    quad(window, 0.0, y, w, KEY_H, bg);
                    let line = if pitch.is_multiple_of(12) {
                        0x24242b
                    } else {
                        0x17171c
                    };
                    quad(window, 0.0, y + KEY_H - 1.0, w, 1.0, rgb(line).into());
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
                            .top(px((127 - n.pitch) as f32 * KEY_H - sy + 1.))
                            .w(px(w - 1.0))
                            .h(px(KEY_H - 2.))
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
        if let (Some(at), Some((start, _))) = (at, span) {
            let Content::Midi { notes: list } = &project.region(at).content else {
                unreachable!()
            };
            for (i, n) in list.iter().enumerate() {
                let x = (start as i64 + n.start) as f32 * ppt - sx;
                let w = (n.length as f32 * ppt).max(3.0);
                let y = (127 - n.pitch) as f32 * KEY_H - sy;
                let selected = self.note == Some(i);
                let n = *n;
                let strength = 0.55 + 0.45 * n.velocity as f32 / 127.0;
                notes.push(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y + 1.))
                        .w(px(w - 1.0))
                        .h(px(KEY_H - 2.))
                        .pl(px(4.))
                        .rounded(px(3.))
                        .overflow_hidden()
                        .bg(if selected {
                            Hsla::from(rgb(tint(color, 0.6)))
                        } else {
                            alpha(color, strength)
                        })
                        .when(selected, |d| d.border_1().border_color(rgb(0xffffff)))
                        .font_family(MONO)
                        .text_size(px(8.5))
                        .line_height(px(KEY_H - 2.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(BG))
                        .whitespace_nowrap()
                        .cursor(match self.tool {
                            Tool::Eraser => gpui::CursorStyle::OperationNotAllowed,
                            Tool::Scissors => gpui::CursorStyle::IBeam,
                            _ => gpui::CursorStyle::OpenHand,
                        })
                        .when(w >= 26.0, |d| d.child(name_in(spelling, n.pitch)))
                        .debug_selector(move || format!("note-{i}"))
                        .child(
                            div()
                                .absolute()
                                .right_0()
                                .top_0()
                                .w(px(5.))
                                .h_full()
                                .cursor_ew_resize()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    grab_note(at, i, n, Edge::End, cx),
                                ),
                        )
                        .on_mouse_down(MouseButton::Left, grab_note(at, i, n, Edge::Move, cx)),
                );
                let h = n.velocity as f32 / 127.0 * VELOCITY_H;
                let ink = if selected {
                    Hsla::from(rgb(0xffffff))
                } else {
                    rgb(color).into()
                };
                stems.push(
                    div()
                        .absolute()
                        .left(px(x))
                        .bottom(px(5.))
                        .w(px(2.))
                        .h(px(h))
                        .bg(ink),
                );
                let velocity = n.velocity;
                stems.push(
                    div()
                        .absolute()
                        .left(px(x - 3.))
                        .bottom(px(5.0 + h - 4.0))
                        .size(px(8.))
                        .rounded(px(4.))
                        .bg(ink)
                        .cursor_ns_resize()
                        .debug_selector(move || format!("velocity-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.note = Some(i);
                                let y = f32::from(e.position.y);
                                ws.begin_drag(
                                    Drag::Velocity {
                                        at,
                                        index: i,
                                        y,
                                        velocity,
                                    },
                                    cx,
                                );
                            }),
                        ),
                );
                if selected {
                    tip = Some(
                        div()
                            .absolute()
                            .left(px(x + w + 6.))
                            .top(px(y - 1.))
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
                                "{} · vel {}",
                                name_in(spelling, n.pitch),
                                n.velocity
                            )),
                    );
                }
            }
        }

        // The keyboard: click a key to hear it.
        let keys = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let w = f32::from(b.size.width);
                let first = (sy / KEY_H) as i32;
                let last = ((sy + f32::from(b.size.height)) / KEY_H) as i32 + 1;
                window.paint_quad(fill(b, rgb(0xd9d9df)));
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    let y = b.origin.y + px(row as f32 * KEY_H - sy);
                    let line = |window: &mut Window, at: f32| {
                        window.paint_quad(fill(
                            Bounds::new(point(b.origin.x, y + px(at)), size(px(w), px(1.))),
                            rgb(0xb0b0ba),
                        ));
                    };
                    if black(pitch) {
                        line(window, KEY_H / 2.0);
                    } else {
                        if sounding[pitch as usize] {
                            window.paint_quad(fill(
                                Bounds::new(point(b.origin.x, y), size(px(w), px(KEY_H))),
                                rgb(color),
                            ));
                        }
                        if pitch > 0 && !black(pitch - 1) {
                            line(window, KEY_H - 0.5);
                        }
                    }
                }
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    // The song's scale down the keys' edge: its notes marked, its tonic bright.
                    if let Some(k) = key.filter(|k| k.contains(pitch)) {
                        let y = b.origin.y + px(row as f32 * KEY_H - sy + 1.0);
                        let mark = Bounds::new(
                            point(b.origin.x + px(w - 4.0), y),
                            size(px(4.), px(KEY_H - 2.)),
                        );
                        let c = match k.is_root(pitch) {
                            true => Hsla::from(rgb(ACCENT)),
                            false => alpha(ACCENT, 0.45),
                        };
                        window.paint_quad(fill(mark, c));
                    }
                    if black(pitch) {
                        let y = b.origin.y + px(row as f32 * KEY_H - sy + 1.0);
                        let key =
                            Bounds::new(point(b.origin.x, y), size(px(w * 0.6), px(KEY_H - 2.)));
                        let c = if sounding[pitch as usize] {
                            rgb(color)
                        } else {
                            rgb(0x18181c)
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
        // Every C, and the song's tonic, named.
        let tonic = |p: u8| key.is_some_and(|k| k.is_root(p));
        let labels = (0..=127u8)
            .filter(|&p| p % 12 == 0 || (tonic(p) && !black(p)))
            .map(|p| {
                div()
                    .absolute()
                    .right(px(7.))
                    .top(px((127 - p) as f32 * KEY_H - sy + 1.))
                    .font_family(MONO)
                    .text_size(px(9.))
                    .line_height(px(10.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(if tonic(p) { ACCENT } else { 0x5a5a64 }))
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
                                ws.player.preview(track, pitch, 100);
                                ws.begin_drag(Drag::Key { track, pitch }, cx);
                            }),
                        )
                    })
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

        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .child(
                div()
                    .w(px(HEADER_W))
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
                            .items_center()
                            .gap(px(8.))
                            .px(px(12.))
                            .border_t_1()
                            .border_color(rgb(LINE))
                            .child(div().font_weight(FontWeight::MEDIUM).child("Velocity"))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child("drag a dot"),
                            ),
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
                                    let x = f32::from(e.position.x)
                                        - f32::from(ws.roll_bounds.get().origin.x);
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
                            .child(grid)
                            .children(ghosts)
                            .children(notes)
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
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e, _, cx| ws.roll_click(e, cx)),
                            )
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
                            .children(stems),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(playhead))
                            .top_0()
                            .w(px(1.))
                            .h_full()
                            .bg(rgb(TEXT)),
                    ),
            )
    }

    /// The piano roll's own controls in the editor's bar: scale, ghost notes, quantize, zoom.
    pub fn roll_controls(&mut self, cx: &mut Context<Self>) -> Div {
        let key = self.session.project.song_key();
        let scale_label = key.map_or("Scale".to_string(), |k| format!("Scale · {}", k.name()));
        let ghost = self
            .roll
            .ghost
            .and_then(|g| self.session.project.tracks.iter().find(|t| t.id == g))
            .map_or("Ghost · Off".to_string(), |t| {
                format!("Ghost · {}", t.name)
            });
        let at = self.roll_region();
        let grid = self.roll.grid;
        div()
            .flex()
            .items_center()
            .gap(px(8.))
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
                button(
                    format!(
                        "Q {}",
                        crate::inspector::grid_name(grid).trim_end_matches(" note")
                    ),
                    false,
                )
                .debug_selector(|| "roll-quantize".into())
                .when(at.is_none(), |d| d.opacity(0.4).cursor_default())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if let Some(at) = at {
                            let edit = NoteEdit::Quantize {
                                grid: ws.roll.grid,
                                strength: ws.roll.strength,
                                swing: ws.roll.swing,
                            };
                            ws.edit_notes(at, edit, cx);
                        }
                    }),
                ),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(rgb(LINE)))
            .child(
                div()
                    .w(px(130.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child("Zoom")
                    .child(self.slider_bar(Slider::Zoom, cx).flex_1()),
            )
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
    div()
        .h(px(26.))
        .px(px(10.))
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
        .child(icon(i, 13.).text_color(rgb(if on { ACCENT_TEXT } else { TEXT_2 })))
        .child(label)
}

/// Starts dragging note `i` of a region, sounding it; or erases or cuts it, with those tools.
fn grab_note(
    at: At,
    i: usize,
    n: Note,
    edge: Edge,
    cx: &mut Context<Workspace>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
        cx.stop_propagation();
        match ws.tool {
            Tool::Eraser => {
                ws.edit(cx, |ws| {
                    if let Some(notes) = ws.session.project.notes_mut(at) {
                        notes.remove(i);
                    }
                    ws.note = None;
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
            Tool::Pointer | Tool::Pencil => {}
        }
        ws.note = Some(i);
        let track = ws.session.project.tracks[at.0].id;
        ws.player.preview(track, n.pitch, n.velocity);
        let from = e.position.map(f32::from);
        let (start, pitch, length) = (n.start, n.pitch, n.length);
        let drag = Drag::Note {
            at,
            index: i,
            edge,
            from,
            start,
            pitch,
            length,
            moved: false,
        };
        ws.begin_drag(drag, cx);
    })
}

#[cfg(test)]
mod spelling {
    use super::*;

    #[test]
    fn notes_are_named_as_the_key_spells_them() {
        let key = |s: &str| Key::parse(s);
        assert_eq!(name_in(None, 60), "C3"); // middle C, as Logic numbers it
        assert_eq!(name_in(None, 70), "A♯3");
        assert_eq!(name_in(key("Eb major"), 70), "B♭3");
        assert_eq!(name_in(key("Eb major"), 63), "E♭3");
        assert_eq!(name_in(key("E major"), 70), "A♯3");
        assert_eq!(name_in(key("C major"), 72), "C4");
    }
}
