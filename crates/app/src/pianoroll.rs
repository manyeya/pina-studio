//! The piano roll: the selected MIDI region's notes against a keyboard, in the pane under the
//! tracks. It shares the tracks area's zoom and horizontal scroll, so its grid lines up with the
//! lanes above. Double-click to add a note; drag a note to move it, its right edge to resize it.

use gpui::{
    App, Bounds, Context, Hsla, MouseButton, MouseDownEvent, ScrollDelta, ScrollWheelEvent, Window,
    canvas, div, fill, hsla, point, prelude::*, px, rgb, size,
};
use tempo_engine::project::{At, BEATS_PER_BAR, Content, Note, PPQ};

use crate::{
    theme::*,
    workspace::{Drag, Edge, Workspace},
};

/// The height of one pitch.
pub const KEY_H: f32 = 12.0;

/// The scroll that puts `pitch` near the top.
pub fn scroll_to(pitch: u8) -> f32 {
    ((127 - pitch as i32) as f32 * KEY_H - KEY_H * 2.0).max(0.0)
}

fn black(pitch: u8) -> bool {
    matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
}

/// Logic's names: middle C (60) is C3.
fn name(pitch: u8) -> String {
    format!("C{}", pitch as i32 / 12 - 2)
}

/// Quiet notes green, loud ones red.
fn velocity_color(velocity: u8) -> Hsla {
    hsla(0.33 * (1.0 - velocity as f32 / 127.0), 0.7, 0.5, 1.0)
}

impl Workspace {
    /// The MIDI region the piano roll shows: the selected one, if it's MIDI.
    fn roll_region(&self) -> Option<At> {
        self.region.filter(|&at| {
            matches!(
                self.session.project.region(at).content,
                Content::Midi { .. }
            )
        })
    }

    /// Scrolls the piano roll to the selected region's notes.
    pub fn focus_roll(&mut self) {
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
        let height = f32::from(self.roll_bounds.get().size.height);
        let most = (128.0 * KEY_H - height).max(0.0);
        self.roll_scroll_y = (self.roll_scroll_y - d.y).clamp(0.0, most);
        self.scroll_x = (self.scroll_x - d.x).max(0.0);
        cx.notify();
    }

    /// A double-click on empty grid adds a note there, a grid step long; a single click
    /// deselects.
    fn roll_click(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        self.note = None;
        let Some(at) = self.roll_region().filter(|_| e.click_count == 2) else {
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

    pub fn piano_roll(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx, sy) = (self.px_per_tick(), self.scroll_x, self.roll_scroll_y);
        let at = self.roll_region();
        let project = &self.session.project;
        let span = at.map(|at| {
            let r = project.region(at);
            (r.start, r.start + r.length)
        });
        let track = at.map(|at| project.tracks[at.0].id);

        let bounds = self.roll_bounds.clone();
        let grid = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                let first = (sy / KEY_H) as i32;
                let last = ((sy + f32::from(b.size.height)) / KEY_H) as i32 + 1;
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    let y = b.origin.y + px(row as f32 * KEY_H - sy);
                    let bg = if black(pitch) { 0x222225 } else { 0x2a2a2e };
                    let r = Bounds::new(point(b.origin.x, y), size(b.size.width, px(KEY_H)));
                    window.paint_quad(fill(r, rgb(bg)));
                    if pitch.is_multiple_of(12) {
                        let line = Bounds::new(
                            point(b.origin.x, y + px(KEY_H - 1.)),
                            size(b.size.width, px(1.)),
                        );
                        window.paint_quad(fill(line, rgb(BORDER)));
                    }
                }
                let beat_px = ppt * PPQ as f32;
                let first = (sx / beat_px) as u64;
                let last = ((sx + f32::from(b.size.width)) / beat_px) as u64 + 1;
                for beat in first..=last {
                    let bar = beat % BEATS_PER_BAR == 0;
                    let x = b.origin.x + px(beat as f32 * beat_px - sx);
                    let color = if bar { GRID_BAR } else { GRID_BEAT };
                    window.paint_quad(fill(
                        Bounds::new(point(x, b.origin.y), size(px(1.), b.size.height)),
                        rgb(color),
                    ));
                }
                // Shade what's outside the region.
                if let Some((start, end)) = span {
                    let (x0, x1) = (start as f32 * ppt - sx, end as f32 * ppt - sx);
                    let shade = Hsla::from(rgb(0x000000)).opacity(0.35);
                    let w = f32::from(b.size.width);
                    if x0 > 0.0 {
                        window.paint_quad(fill(
                            Bounds::new(b.origin, size(px(x0), b.size.height)),
                            shade,
                        ));
                    }
                    if x1 < w {
                        let at = point(b.origin.x + px(x1.max(0.0)), b.origin.y);
                        window.paint_quad(fill(
                            Bounds::new(at, size(px(w - x1.max(0.0)), b.size.height)),
                            shade,
                        ));
                    }
                }
            },
        )
        .absolute()
        .size_full();

        let mut notes = Vec::new();
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
                notes.push(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y + 1.))
                        .w(px(w))
                        .h(px(KEY_H - 2.))
                        .rounded(px(2.))
                        .bg(velocity_color(n.velocity))
                        .border_1()
                        .border_color(if selected {
                            gpui::white()
                        } else {
                            Hsla::from(rgb(0x101010)).opacity(0.6)
                        })
                        .cursor_grab()
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
            }
        }

        // The keyboard: click a key to hear it.
        let keys = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let first = (sy / KEY_H) as i32;
                let last = ((sy + f32::from(b.size.height)) / KEY_H) as i32 + 1;
                for row in first..=last.min(127) {
                    let pitch = (127 - row) as u8;
                    let y = b.origin.y + px(row as f32 * KEY_H - sy);
                    let (color, w) = if black(pitch) {
                        (0x1a1a1c, 0.6)
                    } else {
                        (0xe8e8ec, 1.0)
                    };
                    let key =
                        Bounds::new(point(b.origin.x, y), size(b.size.width * w, px(KEY_H - 1.)));
                    window.paint_quad(fill(key, rgb(color)));
                }
            },
        )
        .absolute()
        .size_full();
        let labels = (0..=127u8).filter(|p| p % 12 == 0).map(|p| {
            div()
                .absolute()
                .right(px(4.))
                .top(px((127 - p) as f32 * KEY_H - sy))
                .text_size(px(9.))
                .text_color(rgb(0x404044))
                .child(name(p))
        });
        let keyboard = div()
            .w(px(HEADER_W))
            .flex_none()
            .h_full()
            .relative()
            .overflow_hidden()
            .border_r_1()
            .border_color(rgb(BORDER))
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
            .on_scroll_wheel(cx.listener(|ws, e, _, cx| ws.roll_scroll(e, cx)));

        let playhead = self.playhead() as f32 * ppt - sx;
        div()
            .h(px(ROLL_H))
            .flex_none()
            .flex()
            .bg(rgb(PANEL_DARK))
            .border_t_1()
            .border_color(rgb(BORDER))
            .when(self.show_inspector, |d| {
                d.child(
                    div()
                        .w(px(INSPECTOR_W))
                        .flex_none()
                        .p_2()
                        .border_r_1()
                        .border_color(rgb(BORDER))
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_DIM))
                        .child("PIANO ROLL"),
                )
            })
            .child(keyboard)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .relative()
                    .overflow_hidden()
                    .child(grid)
                    .children(notes)
                    .child(
                        div()
                            .absolute()
                            .left(px(playhead))
                            .top_0()
                            .w(px(1.))
                            .h_full()
                            .bg(rgb(0xffffff)),
                    )
                    .when(at.is_none(), |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(20.))
                                .left(px(20.))
                                .text_color(rgb(TEXT_DIM))
                                .child("Select a MIDI region to edit its notes."),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e, _, cx| ws.roll_click(e, cx)),
                    )
                    .on_scroll_wheel(cx.listener(|ws, e, _, cx| ws.roll_scroll(e, cx))),
            )
    }
}

/// Starts dragging note `i` of a region, sounding it.
fn grab_note(
    at: At,
    i: usize,
    n: Note,
    edge: Edge,
    cx: &mut Context<Workspace>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
        cx.stop_propagation();
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
