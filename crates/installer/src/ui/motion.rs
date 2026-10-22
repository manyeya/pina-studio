//! How the installer moves: easing, and the marks it draws as it goes.

use super::*;

/// Eased: quick, then settling.
pub(crate) fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

/// Eased past where it's going, and back: a landing.
pub(crate) fn ease_back(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let (c1, c3) = (1.70158, 2.70158);
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

/// How far in something's entrance is, `since` seconds on, starting `delay` in, over `dur`.
pub(crate) fn appear(since: f32, delay: f32, dur: f32) -> f32 {
    ease_out((since - delay) / dur)
}

/// Rising in: faded and a little lower until it's there.
pub(crate) fn rise(el: Div, p: f32) -> Div {
    el.opacity(p).relative().top(px((1.0 - p) * 14.0))
}

/// A number from 0 to 1, the same every time for `i`: where a note starts, how fast it goes.
pub(crate) fn hashed(i: usize, k: f32) -> f32 {
    let v = ((i as f32 + 1.0) * 12.9898 + k * 78.233).sin() * 43_758.547;
    v - v.floor()
}

/// What the curve's doing: how far it's drawn in, how lively, how calm (settling flat when
/// it's over), and the seconds since Pina Studio was ready (notes float up off it then).
#[derive(Clone, Copy)]
pub(crate) struct Motion {
    pub(crate) phase: f32,
    pub(crate) reveal: f32,
    pub(crate) energy: f32,
    pub(crate) calm: f32,
    pub(crate) party: Option<f32>,
}

/// The curve under Pina Studio's icon: a few bells and shelves breathing over a moving spectrum, as an
/// EQ's curve over a song does.
pub(crate) fn curve(m: Motion) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let o = b.origin;
            let at = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
            for k in 1..6 {
                let x = w * k as f32 / 6.0;
                let mut l = PathBuilder::stroke(px(1.));
                l.move_to(at(x, 0.0));
                l.line_to(at(x, h));
                if let Ok(p) = l.build() {
                    window.paint_path(p, alpha(0xffffff, 0.035 * m.reveal));
                }
            }
            let t = m.phase;
            let amp = h * 0.22 * m.energy * (1.0 - 0.75 * m.calm);
            let y = |x: f32| {
                let u = x / w;
                let bell = |c: f32, width: f32, g: f32| g * (-((u - c) / width).powi(2)).exp();
                h * 0.55
                    - amp
                        * (bell(0.22 + 0.04 * (t * 0.7).sin(), 0.09, 0.9)
                            + bell(0.58, 0.12, -0.6 + 0.25 * (t * 0.9).sin())
                            + bell(0.86 + 0.03 * t.cos(), 0.07, 0.7))
                    - amp * 0.35 * (1.0 / (1.0 + (-(u - 0.95) * 30.0).exp()))
            };
            let n = 120;
            let drawn = ((n as f32) * m.reveal).round() as usize;
            // The spectrum under it, moving faster, rising in.
            let mut fill = PathBuilder::fill();
            fill.move_to(at(0.0, h));
            for i in 0..=n {
                let x = w * i as f32 / n as f32;
                let u = x / w;
                let lift = (1.0 - m.reveal) * h * 0.4;
                let s = h * (0.72 - 0.18 * (1.0 - (u - 0.35).abs() * 1.6).max(0.0))
                    + 6.0 * m.energy * ((u * 31.0 + t * 2.3).sin() * (u * 17.0 - t * 1.7).cos())
                    + lift;
                fill.line_to(at(x, s.min(h)));
            }
            fill.line_to(at(w, h));
            fill.close();
            if let Ok(p) = fill.build() {
                window.paint_path(p, alpha(ACCENT, 0.10 * m.reveal));
            }
            if drawn >= 1 {
                // A glow under the line, then the line, drawing itself in from the left.
                for (width, a) in [(7.0, 0.10), (2.0, 1.0)] {
                    let mut line = PathBuilder::stroke(px(width));
                    for i in 0..=drawn {
                        let x = w * i as f32 / n as f32;
                        let p = at(x, y(x));
                        if i == 0 {
                            line.move_to(p);
                        } else {
                            line.line_to(p);
                        }
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, alpha(CURVE, a));
                    }
                }
                // Its leading point while it draws in.
                if m.reveal < 1.0 {
                    let x = w * drawn as f32 / n as f32;
                    if let Some(p) = dot(at(x, y(x)), 3.5) {
                        window.paint_path(p, rgb(0xffffff));
                    }
                }
            }
            // Ready: sparks and notes float up off the curve.
            if let Some(c) = m.party {
                for i in 0..34 {
                    let age = c - hashed(i, 1.0) * 0.9;
                    let life = 2.2 + hashed(i, 2.0) * 0.8;
                    if !(0.0..life).contains(&age) {
                        continue;
                    }
                    let x0 = w * hashed(i, 3.0);
                    let rise = age * (46.0 + hashed(i, 4.0) * 80.0);
                    let x = x0 + (age * 2.6 + i as f32).sin() * 9.0;
                    let fade = (1.0 - age / life) * (age * 6.0).min(1.0);
                    let color = [CURVE, MINT, ACCENT, 0xffffff][i % 4];
                    if let Some(p) = dot(at(x, y(x0) - rise), 1.4 + hashed(i, 5.0) * 2.4) {
                        window.paint_path(p, alpha(color, fade));
                    }
                }
            }
        },
    )
    .size_full()
}

/// A filled circle.
pub(crate) fn dot(c: gpui::Point<gpui::Pixels>, r: f32) -> Option<gpui::Path<gpui::Pixels>> {
    let mut p = PathBuilder::fill();
    p.move_to(point(c.x + px(r), c.y));
    p.arc_to(
        point(px(r), px(r)),
        px(0.),
        false,
        false,
        point(c.x - px(r), c.y),
    );
    p.arc_to(
        point(px(r), px(r)),
        px(0.),
        false,
        false,
        point(c.x + px(r), c.y),
    );
    p.close();
    p.build().ok()
}

/// An arc round `c`, from one angle to another (degrees, clockwise from the right).
pub(crate) fn arc(
    c: gpui::Point<gpui::Pixels>,
    r: f32,
    from: f32,
    to: f32,
    width: f32,
) -> Option<gpui::Path<gpui::Pixels>> {
    if (to - from).abs() < 0.5 {
        return None;
    }
    let mut p = PathBuilder::stroke(px(width));
    let steps = (((to - from).abs() / 8.0).ceil() as usize).max(2);
    for k in 0..=steps {
        let a = (from + (to - from) * k as f32 / steps as f32).to_radians();
        let q = point(c.x + px(r * a.cos()), c.y + px(r * a.sin()));
        if k == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p.build().ok()
}

/// A tick, drawn as far as `p` (0 to 1) along its two strokes.
pub(crate) fn tick(
    c: gpui::Point<gpui::Pixels>,
    size: f32,
    width: f32,
    p: f32,
) -> Option<gpui::Path<gpui::Pixels>> {
    if p <= 0.0 {
        return None;
    }
    let pts = [(-0.45, 0.05), (-0.15, 0.35), (0.45, -0.35)].map(|(x, y)| (x * size, y * size));
    let lens = [
        ((pts[1].0 - pts[0].0).powi(2) + (pts[1].1 - pts[0].1).powi(2)).sqrt(),
        ((pts[2].0 - pts[1].0).powi(2) + (pts[2].1 - pts[1].1).powi(2)).sqrt(),
    ];
    let mut left = p.min(1.0) * (lens[0] + lens[1]);
    let at = |q: (f32, f32)| point(c.x + px(q.0), c.y + px(q.1));
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(at(pts[0]));
    for k in 0..2 {
        let f = (left / lens[k]).min(1.0);
        let to = (
            pts[k].0 + (pts[k + 1].0 - pts[k].0) * f,
            pts[k].1 + (pts[k + 1].1 - pts[k].1) * f,
        );
        path.line_to(at(to));
        left -= lens[k];
        if left <= 0.0 {
            break;
        }
    }
    path.build().ok()
}

/// A step's mark: a faint ring waiting, a turning arc, a tick drawing itself (with its ring
/// going round), or a dash.
pub(crate) fn mark(state: &State, t: f32, since: Option<f32>) -> impl IntoElement {
    let state = state.clone();
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let r = 8.0;
            let s = since.unwrap_or(10.0);
            match &state {
                State::Waiting => {
                    if let Some(p) = arc(c, r, 0.0, 360.0, 1.5) {
                        window.paint_path(p, rgb(RAISED));
                    }
                }
                State::Running => {
                    if let Some(p) = arc(c, r, 0.0, 360.0, 2.0) {
                        window.paint_path(p, rgb(RAISED));
                    }
                    let a = t * 400.0;
                    let len = 70.0 + 50.0 * (t * 3.0).sin();
                    if let Some(p) = arc(c, r, a, a + len, 2.0) {
                        window.paint_path(p, rgb(ACCENT));
                    }
                }
                State::Done(_) => {
                    let ring = ease_out(s / 0.35);
                    if let Some(p) = dot(c, r * ease_back(s / 0.4).max(0.0)) {
                        window.paint_path(p, alpha(MINT, 0.18));
                    }
                    if let Some(p) = arc(c, r, -90.0, -90.0 + 360.0 * ring, 2.0) {
                        window.paint_path(p, rgb(MINT));
                    }
                    if let Some(p) = tick(c, 10.0, 2.0, ease_out((s - 0.15) / 0.3)) {
                        window.paint_path(p, rgb(MINT));
                    }
                }
                State::Skipped(_) => {
                    let p = ease_out(s / 0.3);
                    let mut dash = PathBuilder::stroke(px(2.));
                    dash.move_to(point(c.x - px(3.5 * p), c.y));
                    dash.line_to(point(c.x + px(3.5 * p), c.y));
                    if let Some(q) = arc(c, r, 0.0, 360.0, 1.5) {
                        window.paint_path(q, rgb(TEXT_3));
                    }
                    if let Ok(q) = dash.build() {
                        window.paint_path(q, rgb(TEXT_3));
                    }
                }
            }
        },
    )
    .size(px(20.))
    .flex_none()
}

/// The big check when it's done: its ring going round, filling, the tick drawn last, a pop.
pub(crate) fn big_check(since: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let pop =
                1.0 + 0.08 * (std::f32::consts::PI * ((since - 0.55) / 0.35).clamp(0.0, 1.0)).sin();
            let r = 22.0 * pop;
            if let Some(p) = dot(c, r * ease_out(since / 0.5)) {
                window.paint_path(p, alpha(MINT, 0.16));
            }
            if let Some(p) = arc(c, r, -90.0, -90.0 + 360.0 * ease_out(since / 0.5), 2.5) {
                window.paint_path(p, rgb(MINT));
            }
            if let Some(p) = tick(c, 24.0 * pop, 3.0, ease_out((since - 0.3) / 0.35)) {
                window.paint_path(p, rgb(MINT));
            }
        },
    )
    .size(px(56.))
    .flex_none()
}

/// Behind the icon: a soft glow that breathes; when it's done, rings going out from it.
pub(crate) fn halo(t: f32, open: f32, party: Option<f32>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let breathe = 1.0 + 0.05 * (t * 1.3).sin();
            for k in 0..7 {
                let r = (54.0 + k as f32 * 13.0) * breathe * (0.6 + 0.4 * open);
                if let Some(p) = dot(c, r) {
                    window.paint_path(p, alpha(ACCENT, 0.035 * open));
                }
            }
            if let Some(c0) = party {
                for k in 0..3 {
                    let age = c0 - k as f32 * 0.28;
                    if !(0.0..1.4).contains(&age) {
                        continue;
                    }
                    let r = 72.0 + ease_out(age / 1.4) * 130.0;
                    if let Some(p) = arc(c, r, 0.0, 360.0, 2.0) {
                        window.paint_path(p, alpha(MINT, 0.5 * (1.0 - age / 1.4)));
                    }
                }
            }
        },
    )
    .size_full()
}
