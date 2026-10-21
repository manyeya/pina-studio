//! The score editor, a linear view: the selected MIDI region as notation on a grand
//! staff (middle C and up on the treble, below on the bass), in the pane under the tracks and in
//! time with them, in the song's key: its signature at the start of the staves, and notes spelled
//! by it, with accidentals where they leave it. Click a note to select (and hear) it; delete deletes
//! it.

use gpui::{
    Bounds, Context, FontWeight, Hsla, MouseButton, PathBuilder, Pixels, Point, Window, canvas,
    div, fill, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    music::Key,
    song::project::{Content, PPQ},
};

use crate::{
    ui::theme::*,
    workspace::{Drag, Workspace},
};

/// The space between staff lines.
const GAP: f32 = 9.0;
/// Where each staff's bottom line sits in the pane.
const TREBLE_BOTTOM: f32 = 90.0;
const BASS_BOTTOM: f32 = 200.0;
/// The diatonic steps of the bottom lines: E4 and G2.
const TREBLE_LINE: i32 = 30;
const BASS_LINE: i32 = 18;

/// A note's place on the staff in a key: its diatonic step (C4 is 28, D4 29), and the accidental
/// it needs (1 a sharp, -1 a flat, 0 a natural), if the key signature doesn't already say it.
pub fn step(pitch: u8, key: &Key) -> (i32, Option<i8>) {
    let (letter, octave, alter) = key.spell(pitch);
    let shown = (alter != key.signature_on(letter)).then_some(alter);
    (octave * 7 + letter, shown)
}

/// Where a key signature's sharps or flats go on the treble staff, in order, as diatonic steps;
/// the bass's are two octaves down.
const SIGNATURE_SHARPS: [i32; 7] = [38, 35, 39, 36, 33, 37, 34]; // F5 C5 G5 D5 A4 E5 B4
const SIGNATURE_FLATS: [i32; 7] = [34, 37, 33, 36, 32, 35, 31]; // B4 E5 A4 D5 G4 C5 F4

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
fn place(pitch: u8, key: &Key) -> (f32, i32, f32, i32) {
    let (d, _) = step(pitch, key);
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

/// An accidental to the left of a point: a sharp (two uprights, two rising bars), a flat (an
/// upright and a bowl) or a natural.
fn accidental(window: &mut Window, c: Point<Pixels>, alter: i8, color: Hsla) {
    let x = c.x;
    match alter {
        1 => {
            for dx in [0.0, GAP * 0.45] {
                let x = x + px(dx);
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
                let (a, b) = (
                    point(x - px(3.), y + px(1.)),
                    point(x + px(GAP * 0.45 + 3.), y - px(1.)),
                );
                line(window, a, b, 1.5, color);
            }
        }
        -1 => {
            line(
                window,
                point(x, c.y - px(GAP * 1.4)),
                point(x, c.y + px(GAP * 0.5)),
                1.2,
                color,
            );
            let mut bowl = PathBuilder::stroke(px(1.4));
            bowl.move_to(point(x, c.y - px(GAP * 0.1)));
            bowl.curve_to(
                point(x, c.y + px(GAP * 0.5)),
                point(x + px(GAP * 0.9), c.y - px(GAP * 0.5)),
            );
            if let Ok(p) = bowl.build() {
                window.paint_path(p, color);
            }
        }
        _ => {
            let r = x + px(GAP * 0.5);
            line(
                window,
                point(x, c.y - px(GAP)),
                point(x, c.y + px(GAP * 0.4)),
                1.0,
                color,
            );
            line(
                window,
                point(r, c.y - px(GAP * 0.4)),
                point(r, c.y + px(GAP)),
                1.0,
                color,
            );
            for dy in [-GAP * 0.3, GAP * 0.3] {
                let y = c.y + px(dy);
                line(
                    window,
                    point(x, y + px(1.)),
                    point(r, y - px(1.)),
                    1.6,
                    color,
                );
            }
        }
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
    pub fn score_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let bar_ticks = self.session.project.bar();
        let key = self.session.project.song_key().unwrap_or(Key {
            root: 0,
            minor: false,
        });
        let [beats, unit] = self.session.project.time_signature;
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
            if let Content::Midi { notes: list, .. } = &r.content {
                notes = list
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| (0..r.length as i64).contains(&n.start))
                    .map(|(i, n)| (i, r.start + n.start as u64, n.length, n.pitch))
                    .collect();
            }
        }
        let selected = self.picked.last().copied();
        let ink = Hsla::from(rgb(0xe8e8ec));
        let faint = Hsla::from(rgb(0x3a3a44));
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
                let bar = bar_ticks as f32 * ppt;
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
                        Hsla::from(rgb(ACCENT_TEXT))
                    } else {
                        ink
                    };
                    let (y, base, bottom, d) = place(pitch, &key);
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
                    if let (_, Some(alter)) = step(pitch, &key) {
                        accidental(window, point(c.x - px(GAP * 1.9), c.y), alter, color);
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
            let (y, ..) = place(pitch, &key);
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
                        ws.picked = vec![i];
                        // It sounds until the mouse comes up, as a piano roll key does.
                        if let Some(at) = ws.region {
                            let track = ws.session.project.tracks[at.0].id;
                            ws.player.preview(track, pitch, 100);
                            ws.begin_drag(Drag::Key { track, pitch }, cx);
                        }
                        cx.notify();
                    }),
                )
        });
        // The start of the staves, in the left column: their lines, and the key's sharps or flats.
        let sig = key.signature();
        let signature = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let ink = Hsla::from(rgb(0xe8e8ec));
                let faint = Hsla::from(rgb(0x3a3a44));
                let x0 = b.origin.x + px(70.);
                for bottom in [TREBLE_BOTTOM, BASS_BOTTOM] {
                    for k in 0..5 {
                        let y = b.origin.y + px(bottom - k as f32 * GAP);
                        let width = b.size.width - px(70.);
                        window.paint_quad(fill(
                            Bounds::new(point(x0, y), size(width, px(1.))),
                            faint,
                        ));
                    }
                }
                let (steps, alter) = match sig {
                    s if s > 0 => (&SIGNATURE_SHARPS[..s as usize], 1),
                    s => (&SIGNATURE_FLATS[..(-s) as usize], -1),
                };
                for (bottom, line_step, octaves) in [
                    (TREBLE_BOTTOM, TREBLE_LINE, 0),
                    (BASS_BOTTOM, BASS_LINE, 14),
                ] {
                    for (i, &d) in steps.iter().enumerate() {
                        let y = bottom - (d - octaves - line_step) as f32 * GAP / 2.0;
                        let at = point(x0 + px(16. + i as f32 * GAP * 1.1), b.origin.y + px(y));
                        accidental(window, at, alter, ink);
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
            .bg(rgb(LANE))
            .child(
                div()
                    .w(px(self.header_w))
                    .flex_none()
                    .bg(rgb(SURFACE))
                    .h_full()
                    .relative()
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(signature)
                    .children([("Treble", TREBLE_BOTTOM), ("Bass", BASS_BOTTOM)].map(
                        |(name, bottom)| {
                            div()
                                .absolute()
                                .left(px(12.))
                                .top(px(bottom - 3.0 * GAP))
                                .child(name)
                        },
                    ))
                    .children([TREBLE_BOTTOM, BASS_BOTTOM].map(|bottom| {
                        // The time signature, stacked, after the key's.
                        div()
                            .absolute()
                            .right(px(14.))
                            .top(px(bottom - 4.0 * GAP - 2.0))
                            .flex()
                            .flex_col()
                            .items_center()
                            .font_family(MONO)
                            .text_size(px(13.))
                            .line_height(px(GAP * 2.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xe8e8ec))
                            .child(format!("{beats}"))
                            .child(format!("{unit}"))
                    })),
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
                                .text_color(rgb(TEXT_3))
                                .child("Select a MIDI region to see it as notation."),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.picked.clear();
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
        let c = Key {
            root: 0,
            minor: false,
        };
        assert_eq!(step(60, &c), (28, None)); // middle C, a ledger line under the treble
        assert_eq!(step(64, &c).0, TREBLE_LINE); // E4, the treble's bottom line
        assert_eq!(step(43, &c).0, BASS_LINE); // G2, the bass's bottom line
        assert_eq!(step(61, &c), (28, Some(1))); // C#4
        assert_eq!(step(72, &c).0 - step(60, &c).0, 7); // an octave is seven steps
        // In F major, Bb needs no flat of its own; B natural needs a natural.
        let f = Key::parse("F major").unwrap();
        assert_eq!(step(70, &f), (34, None));
        assert_eq!(step(71, &f), (34, Some(0)));
        // In Eb major, Gb is a G with a flat.
        let eb = Key::parse("Eb major").unwrap();
        assert_eq!(step(66, &eb), (32, Some(-1)));
        assert_eq!(value(PPQ), Value::Quarter);
        assert_eq!(value(PPQ * 7 / 8), Value::Quarter); // detached, still a quarter
        assert_eq!(value(PPQ / 2 + 60), Value::Eighth);
        assert_eq!(value(PPQ * 4), Value::Whole);
    }
}
