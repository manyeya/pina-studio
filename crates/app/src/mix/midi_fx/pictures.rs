//! What each MIDI effect does, drawn: the notes an arpeggiator plays, the chord one note makes,
//! a velocity curve, repeats fading out, notes varied by chance, MPE shapes.

use gpui::{
    Bounds, Div, Hsla, PathBuilder, Pixels, Window, canvas, div, fill, point, prelude::*, px, rgb,
    size,
};
use pina_engine::song::project::{ArpOrder, ExprShape, MidiFx};

use crate::ui::theme::*;

#[allow(clippy::too_many_arguments)]
/// A filled box, `r` round.
fn rect(window: &mut Window, b: Bounds<Pixels>, x: f32, y: f32, w: f32, h: f32, r: f32, c: Hsla) {
    let at = point(b.origin.x + px(x), b.origin.y + px(y));
    window.paint_quad(
        fill(Bounds::new(at, size(px(w.max(0.5)), px(h.max(0.5)))), c).corner_radii(px(r)),
    );
}

/// A line through the points, `w` thick.
fn line(window: &mut Window, b: Bounds<Pixels>, pts: &[(f32, f32)], w: f32, c: Hsla) {
    let mut path = PathBuilder::stroke(px(w));
    for (i, &(x, y)) in pts.iter().enumerate() {
        let p = point(b.origin.x + px(x), b.origin.y + px(y));
        if i == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    if let Ok(p) = path.build() {
        window.paint_path(p, c);
    }
}

fn is_black(p: u8) -> bool {
    matches!(p % 12, 1 | 3 | 6 | 8 | 10)
}

/// A keyboard from `lo` to `hi`, each key lit as `lit` says (none: as it is).
pub fn paint_keys(
    window: &mut Window,
    b: Bounds<Pixels>,
    lo: u8,
    hi: u8,
    lit: &dyn Fn(u8) -> Option<Hsla>,
) {
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let whites: Vec<u8> = (lo..=hi).filter(|&p| !is_black(p)).collect();
    let kw = w / whites.len().max(1) as f32;
    for (i, &p) in whites.iter().enumerate() {
        let c = lit(p).unwrap_or(rgb(0xe8e8ec).into());
        rect(window, b, i as f32 * kw + 1.0, 0.0, kw - 2.0, h, 4.0, c);
    }
    for p in (lo..=hi).filter(|&p| is_black(p)) {
        let k = whites.iter().position(|&q| q > p).unwrap_or(whites.len());
        let x = k as f32 * kw - kw * 0.3;
        let c = lit(p).unwrap_or(rgb(0x1a1b21).into());
        rect(window, b, x, 0.0, kw * 0.6, h * 0.6, 3.0, c);
    }
}

/// An arpeggiator's notes over two bars, from `held` (in the order they went down).
pub fn arp_steps(held: &[u8], order: ArpOrder, octaves: u8, n: usize) -> Vec<Vec<u8>> {
    let mut sorted = held.to_vec();
    sorted.sort_unstable();
    let over = |notes: &[u8]| -> Vec<u8> {
        (0..octaves.max(1))
            .flat_map(|o| notes.iter().map(move |&p| p.saturating_add(12 * o)))
            .collect()
    };
    let up = over(&sorted);
    let run: Vec<u8> = match order {
        ArpOrder::Up => up,
        ArpOrder::Down => up.into_iter().rev().collect(),
        ArpOrder::UpDown => {
            let mut r = up.clone();
            if up.len() > 2 {
                r.extend(up[1..up.len() - 1].iter().rev());
            }
            r
        }
        ArpOrder::Played => over(held),
        ArpOrder::Random => {
            // The same each time it's drawn: a scramble of the notes.
            let mut r = up;
            let mut s = 0x9e37_79b9u32;
            for i in (1..r.len()).rev() {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                r.swap(i, (s >> 8) as usize % (i + 1));
            }
            r
        }
        ArpOrder::Chord => return vec![over(&sorted); n],
    };
    (0..n)
        .map(|i| {
            run.get(i % run.len().max(1))
                .map(|&p| vec![p])
                .unwrap_or_default()
        })
        .collect()
}

/// The arpeggiator's two bars: a lane a pitch, each step's note as long as its gate.
pub fn arp(fx: MidiFx, held: Vec<u8>, color: u32) -> impl IntoElement {
    let MidiFx::Arpeggiator {
        rate,
        order,
        octaves,
        gate,
        ..
    } = fx
    else {
        return div();
    };
    let n = ((8.0 / rate.max(0.05)).round() as usize).clamp(2, 64);
    let steps = arp_steps(&held, order, octaves, n);
    let beat = (1.0 / rate).round().max(1.0) as usize;
    div().size_full().child(
        canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let all: Vec<u8> = steps.iter().flatten().copied().collect();
                let (lo, hi) = (
                    all.iter().copied().min().unwrap_or(60),
                    all.iter().copied().max().unwrap_or(72),
                );
                let rows = (hi - lo + 1) as f32;
                let (pad, sw) = (8.0, w / n as f32);
                let rh = ((h - pad * 2.0) / rows).min(18.0);
                let top = (h - rh * rows) / 2.0;
                // Each pitch's lane, the beats down the grid.
                for r in 0..rows as u8 {
                    if !is_black(lo + r) {
                        let y = top + (hi - lo - r) as f32 * rh;
                        rect(window, b, 0.0, y, w, rh, 0.0, rgb(0x121318).into());
                    }
                }
                for i in (0..=n).step_by(beat) {
                    let x = i as f32 * sw;
                    let c = if i % (beat * 4) == 0 {
                        0x2a2b33
                    } else {
                        0x1c1d23
                    };
                    rect(window, b, x, 0.0, 1.0, h, 0.0, rgb(c).into());
                }
                for (i, notes) in steps.iter().enumerate() {
                    for &p in notes {
                        let y = top + (hi - p) as f32 * rh;
                        let c = match i == 0 {
                            true => rgb(tint(color, 0.25)).into(),
                            false => alpha(color, 0.9),
                        };
                        let len = (sw * gate - 2.0).max(2.0);
                        rect(
                            window,
                            b,
                            i as f32 * sw + 1.0,
                            y + 1.0,
                            len,
                            rh - 2.0,
                            3.0,
                            c,
                        );
                    }
                }
            },
        )
        .size_full(),
    )
}

/// A keyboard with `notes` lit (the first, the one played, brighter).
pub fn chord(notes: Vec<u8>, lo: u8, hi: u8, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let lit = |p: u8| match notes.first() == Some(&p) {
                true => Some(rgb(tint(color, 0.3)).into()),
                false if notes.contains(&p) => Some(alpha(color, 0.85)),
                false => None,
            };
            paint_keys(window, b, lo, hi, &lit);
        },
    )
    .size_full()
}

/// A transposer: the chord played, as outlines, and where it goes.
pub fn moved(from: Vec<u8>, by: i32, color: u32) -> impl IntoElement {
    let to: Vec<u8> = from
        .iter()
        .map(|&p| (p as i32 + by).clamp(0, 127) as u8)
        .collect();
    let lo = from.iter().chain(&to).min().copied().unwrap_or(48) / 12 * 12;
    let hi = (from.iter().chain(&to).max().copied().unwrap_or(72) / 12 * 12 + 11).max(lo + 23);
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let lit = |p: u8| match (to.contains(&p), from.contains(&p)) {
                (true, _) => Some(alpha(color, 0.9)),
                (false, true) => Some(rgb(0x6a6b75).into()),
                _ => None,
            };
            paint_keys(window, b, lo, hi.min(127), &lit);
        },
    )
    .size_full()
}

/// Velocities in, along; out, up: the line the effect draws them through, the even line under
/// it, and the range they're kept in.
pub fn velocity(fx: MidiFx, color: u32) -> impl IntoElement {
    let MidiFx::Velocity {
        scale,
        add,
        min,
        max,
    } = fx
    else {
        return div().into_any_element();
    };
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let x = |v: f32| v / 127.0 * w;
            let y = |v: f32| h - v / 127.0 * h;
            for k in 1..4 {
                let f = k as f32 / 4.0;
                rect(window, b, f * w, 0.0, 1.0, h, 0.0, rgb(0x1c1d23).into());
                rect(window, b, 0.0, f * h, w, 1.0, 0.0, rgb(0x1c1d23).into());
            }
            // Outside the range kept: shaded.
            rect(
                window,
                b,
                0.0,
                y(max as f32),
                w,
                y(min as f32) - y(max as f32),
                0.0,
                alpha(color, 0.07),
            );
            line(window, b, &[(0.0, h), (w, 0.0)], 1.0, rgb(0x3a3b44).into());
            let pts: Vec<(f32, f32)> = (0..=64)
                .map(|i| {
                    let v = i as f32 * 127.0 / 64.0;
                    let out = (v * scale as f32 / 100.0 + add as f32).clamp(min as f32, max as f32);
                    (x(v), y(out))
                })
                .collect();
            line(window, b, &pts, 2.5, rgb(color).into());
        },
    )
    .size_full()
    .into_any_element()
}

/// A note and its repeats, each `decay` as loud and `transpose` from the last.
pub fn repeats(fx: MidiFx, color: u32) -> impl IntoElement {
    let MidiFx::NoteRepeater {
        count,
        decay,
        transpose,
        ..
    } = fx
    else {
        return div().into_any_element();
    };
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let n = count as usize + 1;
            // Spread across, however few: room for eight at least.
            let sw = (w - 16.0) / n.max(8) as f32;
            let mid = h * 0.5;
            let step = (transpose as f32 * 3.0).clamp(-h / (2.0 * n as f32), h / (2.0 * n as f32));
            let mut level = 1.0f32;
            for i in 0..n {
                let bar = (h * 0.42 * level).max(3.0);
                let cy = mid - step * i as f32;
                let c = match i {
                    0 => rgb(tint(color, 0.25)).into(),
                    _ => alpha(color, 0.25 + 0.7 * level),
                };
                rect(
                    window,
                    b,
                    8.0 + i as f32 * sw,
                    cy - bar / 2.0,
                    sw * 0.62,
                    bar,
                    4.0,
                    c,
                );
                level *= decay;
            }
        },
    )
    .size_full()
    .into_any_element()
}

/// Sixteen notes as the randomizer plays them: moved up or down, louder or softer, some left
/// out (drawn hollow).
pub fn random(fx: MidiFx, color: u32) -> impl IntoElement {
    let MidiFx::Randomizer {
        pitch,
        velocity,
        chance,
    } = fx
    else {
        return div().into_any_element();
    };
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let sw = w / 16.0;
            let mut s = 0x2545_f491u32;
            let mut next = move || {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (s >> 8) as f32 / (1u32 << 24) as f32
            };
            let row = (h - 16.0) / 25.0;
            rect(window, b, 0.0, h / 2.0, w, 1.0, 0.0, rgb(0x2a2b33).into());
            for i in 0..16 {
                let dp = ((next() * 2.0 - 1.0) * pitch as f32).round();
                let dv = (next() * 2.0 - 1.0) * velocity as f32 / 127.0;
                let played = next() * 100.0 < chance as f32;
                let y = h / 2.0 - dp * row - row / 2.0;
                let x = i as f32 * sw + 2.0;
                let level = (0.75 + dv).clamp(0.15, 1.0);
                match played {
                    true => rect(
                        window,
                        b,
                        x,
                        y,
                        sw - 4.0,
                        row.max(4.0),
                        3.0,
                        alpha(color, level),
                    ),
                    false => {
                        let o = alpha(color, 0.35);
                        rect(window, b, x, y, sw - 4.0, 1.5, 0.0, o);
                        rect(window, b, x, y + row.max(4.0) - 1.5, sw - 4.0, 1.5, 0.0, o);
                    }
                }
            }
        },
    )
    .size_full()
    .into_any_element()
}

/// One MPE shape: how far it's pushed (along) to what it plays (up).
pub fn shape(s: ExprShape, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            for k in 1..4 {
                let f = k as f32 / 4.0;
                rect(window, b, f * w, 0.0, 1.0, h, 0.0, rgb(0x1c1d23).into());
                rect(window, b, 0.0, f * h, w, 1.0, 0.0, rgb(0x1c1d23).into());
            }
            line(window, b, &[(0.0, h), (w, 0.0)], 1.0, rgb(0x3a3b44).into());
            let pts: Vec<(f32, f32)> = (0..=48)
                .map(|i| {
                    let v = i as f32 / 48.0;
                    (v * w, h - s.apply(v) * h)
                })
                .collect();
            line(window, b, &pts, 2.5, rgb(color).into());
        },
    )
    .size_full()
}

/// A picture's frame: dark, rounded, with a caption in its corner.
pub fn frame(caption: String, picture: impl IntoElement) -> Div {
    div()
        .relative()
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .p(px(12.))
        .pt(px(30.))
        .rounded(px(10.))
        .bg(rgb(0x0b0c10))
        .border_1()
        .border_color(rgb(0x1f2027))
        .child(picture)
        .child(
            div()
                .absolute()
                .top(px(9.))
                .left(px(12.))
                .font_family(MONO)
                .text_size(px(11.))
                .text_color(rgb(TEXT_3))
                .child(caption),
        )
}
