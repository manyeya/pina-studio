//! The score editor, as in Logic's linear view: the selected MIDI region as notation on a grand
//! staff (middle C and up on the treble, below on the bass), in the pane under the tracks and in
//! time with them. Click a note to select (and hear) it; delete deletes it.

use gpui::{
    Bounds, Context, Hsla, MouseButton, PathBuilder, Pixels, Point, Window, canvas, div, fill,
    point, prelude::*, px, rgb, size,
};
use tempo_engine::project::{BEATS_PER_BAR, Content, PPQ};

use crate::{theme::*, workspace::Workspace};

/// The space between staff lines.
const GAP: f32 = 9.0;
/// Where each staff's bottom line sits in the pane.
const TREBLE_BOTTOM: f32 = 90.0;
const BASS_BOTTOM: f32 = 200.0;
/// The diatonic steps of the bottom lines: E4 and G2.
const TREBLE_LINE: i32 = 30;
const BASS_LINE: i32 = 18;

/// A note's place on the staff: its diatonic step (C4 is 28, D4 29), and whether it's sharp.
pub fn step(pitch: u8) -> (i32, bool) {
    const LETTER: [i32; 12] = [0, 0, 1, 1, 2, 3, 3, 4, 4, 5, 5, 6];
    const SHARP: [bool; 12] = [
        false, true, false, true, false, false, true, false, true, false, true, false,
    ];
    let (octave, pc) = (pitch as i32 / 12 - 1, pitch as usize % 12);
    (octave * 7 + LETTER[pc], SHARP[pc])
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
}

/// The note value nearest a length in ticks, by ratio: a slightly short quarter (played
/// detached) is still a quarter.
pub fn value(length: u64) -> Value {
    const VALUES: [(Value, u64); 5] = [
        (Value::Whole, PPQ * 4),
        (Value::Half, PPQ * 2),
        (Value::Quarter, PPQ),
        (Value::Eighth, PPQ / 2),
        (Value::Sixteenth, PPQ / 4),
    ];
    let off = |ticks: u64| (length.max(1) as f64 / ticks as f64).ln().abs();
    VALUES
        .iter()
        .min_by(|a, b| off(a.1).total_cmp(&off(b.1)))
        .unwrap()
        .0
}

/// Where a note goes: the y of its head, the staff's bottom-line step and y, and its step.
fn place(pitch: u8) -> (f32, i32, f32, i32) {
    let (d, _) = step(pitch);
    let (line, bottom) = if pitch >= 60 {
        (TREBLE_LINE, TREBLE_BOTTOM)
    } else {
        (BASS_LINE, BASS_BOTTOM)
    };
    (bottom - (d - line) as f32 * GAP / 2.0, line, bottom, d)
}

fn line(window: &mut Window, from: Point<Pixels>, to: Point<Pixels>, width: f32, color: Hsla) {
    let mut p = PathBuilder::stroke(px(width));
    p.move_to(from);
    p.line_to(to);
    if let Ok(p) = p.build() {
        window.paint_path(p, color);
    }
}

fn head(window: &mut Window, c: Point<Pixels>, hollow: bool, color: Hsla) {
    let (rx, ry) = (px(GAP * 0.65), px(GAP * 0.5));
    let mut p = if hollow {
        PathBuilder::stroke(px(1.5))
    } else {
        PathBuilder::fill()
    };
    p.move_to(point(c.x + rx, c.y));
    p.arc_to(point(rx, ry), px(0.), false, false, point(c.x - rx, c.y));
    p.arc_to(point(rx, ry), px(0.), false, false, point(c.x + rx, c.y));
    p.close();
    if let Ok(p) = p.build() {
        window.paint_path(p, color);
    }
}

impl Workspace {
    pub fn score_pane(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let at = self.region.filter(|&at| {
            matches!(
                self.session.project.region(at).content,
                Content::Midi { .. }
            )
        });
        let mut notes = Vec::new();
        let mut span = None;
        if let Some(at) = at {
            let r = self.session.project.region(at);
            span = Some((r.start, r.start + r.length));
            if let Content::Midi { notes: list } = &r.content {
                notes = list
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| (0..r.length as i64).contains(&n.start))
                    .map(|(i, n)| (i, r.start + n.start as u64, n.length, n.pitch))
                    .collect();
            }
        }
        let selected = self.note;
        let ink = Hsla::from(rgb(0xe8e8ec));
        let faint = Hsla::from(rgb(0x6a6a72));
        let shown = notes.clone();
        let staff = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let x0 = b.origin.x;
                let w = b.size.width;
                for bottom in [TREBLE_BOTTOM, BASS_BOTTOM] {
                    for k in 0..5 {
                        let y = b.origin.y + px(bottom - k as f32 * GAP);
                        window.paint_quad(fill(Bounds::new(point(x0, y), size(w, px(1.))), faint));
                    }
                }
                // Bar lines across both staves.
                let bar = (PPQ * BEATS_PER_BAR) as f32 * ppt;
                let first = (sx / bar) as u64;
                let last = ((sx + f32::from(w)) / bar) as u64 + 1;
                for k in first..=last {
                    let x = x0 + px(k as f32 * bar - sx);
                    let top = b.origin.y + px(TREBLE_BOTTOM - 4.0 * GAP);
                    let h = px(BASS_BOTTOM - TREBLE_BOTTOM + 4.0 * GAP + 1.0);
                    window.paint_quad(fill(Bounds::new(point(x, top), size(px(1.), h)), faint));
                }
                // Shade what's outside the region.
                if let Some((start, end)) = span {
                    let shade = Hsla::from(rgb(0x000000)).opacity(0.3);
                    let (a, z) = (start as f32 * ppt - sx, end as f32 * ppt - sx);
                    if a > 0.0 {
                        window.paint_quad(fill(
                            Bounds::new(b.origin, size(px(a), b.size.height)),
                            shade,
                        ));
                    }
                    if z < f32::from(w) {
                        let from = point(x0 + px(z.max(0.0)), b.origin.y);
                        window.paint_quad(fill(
                            Bounds::new(from, size(w - px(z.max(0.0)), b.size.height)),
                            shade,
                        ));
                    }
                }
                for &(i, start, length, pitch) in &shown {
                    let color = if selected == Some(i) {
                        Hsla::from(rgb(0x5fb3ff))
                    } else {
                        ink
                    };
                    let (y, base, bottom, d) = place(pitch);
                    let x = start as f32 * ppt - sx + GAP;
                    if x < -GAP * 3.0 || x > f32::from(w) + GAP {
                        continue;
                    }
                    let c = point(x0 + px(x), b.origin.y + px(y));
                    // Ledger lines, above the staff or below it, out to the note.
                    let ledgers = (base + 10..=d)
                        .step_by(2)
                        .chain((d..base).filter(|s| (base - s) % 2 == 0));
                    for s in ledgers {
                        let ly = b.origin.y + px(bottom - (s - base) as f32 * GAP / 2.0);
                        let from = point(c.x - px(GAP * 1.1), ly);
                        window.paint_quad(fill(
                            Bounds::new(from, size(px(GAP * 2.2), px(1.))),
                            faint,
                        ));
                    }
                    let value = value(length);
                    head(
                        window,
                        c,
                        matches!(value, Value::Whole | Value::Half),
                        color,
                    );
                    if step(pitch).1 {
                        // A sharp: two uprights and two rising bars.
                        let sx0 = c.x - px(GAP * 1.9);
                        for dx in [0.0, GAP * 0.45] {
                            let x = sx0 + px(dx);
                            line(
                                window,
                                point(x, c.y - px(GAP)),
                                point(x, c.y + px(GAP)),
                                1.0,
                                color,
                            );
                        }
                        for dy in [-GAP * 0.35, GAP * 0.35] {
                            let y = c.y + px(dy);
                            line(
                                window,
                                point(sx0 - px(3.), y + px(1.)),
                                point(sx0 + px(GAP * 0.45 + 3.), y - px(1.)),
                                1.5,
                                color,
                            );
                        }
                    }
                    if value == Value::Whole {
                        continue;
                    }
                    // Stems go up from notes under the middle line, down from those on or over it.
                    let up = d < base + 4;
                    let (sx1, dir) = if up {
                        (c.x + px(GAP * 0.6), -1.0)
                    } else {
                        (c.x - px(GAP * 0.6), 1.0)
                    };
                    let tip = c.y + px(dir * GAP * 3.5);
                    line(window, point(sx1, c.y), point(sx1, tip), 1.2, color);
                    let flags = match value {
                        Value::Eighth => 1,
                        Value::Sixteenth => 2,
                        _ => 0,
                    };
                    for f in 0..flags {
                        let y = tip - px(dir * f as f32 * GAP * 0.8);
                        line(
                            window,
                            point(sx1, y),
                            point(sx1 + px(GAP * 0.9), y - px(dir * GAP * 1.2)),
                            1.5,
                            color,
                        );
                    }
                }
            },
        )
        .absolute()
        .size_full();
        // Click targets over the heads.
        let targets = notes.iter().map(|&(i, start, _, pitch)| {
            let (y, ..) = place(pitch);
            let x = start as f32 * ppt - sx + GAP;
            div()
                .absolute()
                .left(px(x - GAP))
                .top(px(y - GAP * 0.7))
                .w(px(GAP * 2.0))
                .h(px(GAP * 1.4))
                .debug_selector(move || format!("score-note-{i}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.note = Some(i);
                        if let Some(at) = ws.region {
                            let track = ws.session.project.tracks[at.0].id;
                            ws.player.preview(track, pitch, 100);
                            ws.player.preview(track, pitch, 0);
                        }
                        cx.notify();
                    }),
                )
        });
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
                        .child("SCORE"),
                )
            })
            .child(
                div()
                    .w(px(HEADER_W))
                    .flex_none()
                    .h_full()
                    .relative()
                    .border_r_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_DIM))
                    .child(
                        div()
                            .absolute()
                            .right(px(8.))
                            .top(px(TREBLE_BOTTOM - 3.0 * GAP))
                            .child("Treble"),
                    )
                    .child(
                        div()
                            .absolute()
                            .right(px(8.))
                            .top(px(BASS_BOTTOM - 3.0 * GAP))
                            .child("Bass"),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .relative()
                    .overflow_hidden()
                    .child(staff)
                    .children(targets)
                    .when(at.is_none(), |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(20.))
                                .left(px(20.))
                                .text_color(rgb(TEXT_DIM))
                                .child("Select a MIDI region to see it as notation."),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.note = None;
                            cx.notify();
                        }),
                    )
                    .on_scroll_wheel(self.on_scroll(cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_find_their_line_and_value() {
        assert_eq!(step(60), (28, false)); // middle C, a ledger line under the treble
        assert_eq!(step(64).0, TREBLE_LINE); // E4, the treble's bottom line
        assert_eq!(step(43).0, BASS_LINE); // G2, the bass's bottom line
        assert_eq!(step(61), (28, true)); // C#4
        assert_eq!(step(72).0 - step(60).0, 7); // an octave is seven steps
        assert_eq!(value(PPQ), Value::Quarter);
        assert_eq!(value(PPQ * 7 / 8), Value::Quarter); // detached, still a quarter
        assert_eq!(value(PPQ / 2 + 60), Value::Eighth);
        assert_eq!(value(PPQ * 4), Value::Whole);
    }
}
