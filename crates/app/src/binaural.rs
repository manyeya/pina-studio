//! The Binaural Panner: a binaural track's sound round the listener's head, in 3D.
//! The floor's rings are a metre apart; the head faces FRONT. Drag the source round the head at
//! its height, ⌘-drag it up or down, drag the space round it to look from another side; or set
//! the way round, the height and the distance with the sliders, or a place with a click. Its
//! pulse is the track's level.

use gpui::{
    Bounds, Context, Hsla, MouseButton, MouseDownEvent, Pixels, Point, Window, anchored, canvas,
    deferred, div, fill, point, prelude::*, px, rgb, size,
};
use pina_engine::project::{PanMode, Place};

use crate::{
    theme::*,
    workspace::{Drag, Slider, Workspace},
};

/// The view's size.
const W: f32 = 380.0;
const H: f32 = 280.0;
/// Where the view looks from by default: degrees round from behind, and down.
pub const ORBIT: (f32, f32) = (28.0, 24.0);
const HEAD: f32 = 0.27;
const SOURCE: f32 = 0.13;

/// The view's camera: where it is, which way is ahead, right and up, and its lens.
#[derive(Clone, Copy)]
pub struct Camera {
    eye: [f32; 3],
    ahead: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    focal: f32,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

impl Camera {
    /// Looking at the head from `yaw` degrees round from behind it (right is positive) and
    /// `pitch` down.
    pub fn new((yaw, pitch): (f32, f32)) -> Camera {
        let (y, p) = (yaw.to_radians(), pitch.to_radians());
        let target = [0.0, 0.45, 0.0];
        let back = 7.5;
        let eye = [
            target[0] + back * y.sin() * p.cos(),
            target[1] + back * p.sin(),
            target[2] - back * y.cos() * p.cos(),
        ];
        let d = sub(target, eye);
        let n = dot(d, d).sqrt();
        let ahead = [d[0] / n, d[1] / n, d[2] / n];
        // up × ahead, then ahead × right.
        let r = [ahead[2], 0.0, -ahead[0]];
        let rn = dot(r, r).sqrt();
        let right = [r[0] / rn, 0.0, r[2] / rn];
        let up = [
            ahead[1] * right[2] - ahead[2] * right[1],
            ahead[2] * right[0] - ahead[0] * right[2],
            ahead[0] * right[1] - ahead[1] * right[0],
        ];
        Camera {
            eye,
            ahead,
            right,
            up,
            focal: W * 1.08,
        }
    }

    /// Where a point shows in the view (from its top left), and how many pixels a metre is
    /// there; none behind the camera.
    pub fn project(&self, p: [f32; 3]) -> Option<(f32, f32, f32)> {
        let v = sub(p, self.eye);
        let depth = dot(v, self.ahead);
        if depth < 0.1 {
            return None;
        }
        let scale = self.focal / depth;
        Some((
            W / 2.0 + dot(v, self.right) * scale,
            H / 2.0 - dot(v, self.up) * scale,
            scale,
        ))
    }

    fn depth(&self, p: [f32; 3]) -> f32 {
        dot(sub(p, self.eye), self.ahead)
    }

    /// The point at `height` under a spot in the view, if the spot looks that way.
    pub fn at_height(&self, (x, y): (f32, f32), height: f32) -> Option<[f32; 3]> {
        let (sx, sy) = ((x - W / 2.0) / self.focal, -(y - H / 2.0) / self.focal);
        let ray: [f32; 3] =
            std::array::from_fn(|k| self.ahead[k] + self.right[k] * sx + self.up[k] * sy);
        if ray[1].abs() < 1e-4 {
            return None;
        }
        let t = (height - self.eye[1]) / ray[1];
        (t > 0.0).then(|| std::array::from_fn(|k| self.eye[k] + ray[k] * t))
    }
}

/// A pan's way round, as the panner and the knob print it: 0°, R90°, L45°, 180°.
pub fn azimuth_label(pan: f32) -> String {
    match (pan.clamp(-1.0, 1.0) * 180.0).round() as i32 {
        0 => "0°".into(),
        180 | -180 => "180°".into(),
        a if a < 0 => format!("L{}°", -a),
        a => format!("R{a}°"),
    }
}

/// Places a click puts the source: name, way round (pan), height (degrees).
const PLACES: [(&str, f32, f32); 6] = [
    ("Front", 0.0, 0.0),
    ("Left", -0.5, 0.0),
    ("Right", 0.5, 0.0),
    ("Behind", 1.0, 0.0),
    ("Above", 0.0, 75.0),
    ("Below", 0.0, -45.0),
];

impl Workspace {
    /// Opens the panner beside a track's header.
    pub fn open_panner(&mut self, t: usize, cx: &mut Context<Self>) {
        let lanes = self.lanes_bounds.get();
        let y = f32::from(lanes.origin.y) + self.row_top(t).max(0.0);
        self.panner = Some((t, point(lanes.origin.x + px(12.), px(y))));
        cx.notify();
    }

    /// Where a binaural track's sound is, as set.
    pub fn place_of(&self, t: usize) -> Option<Place> {
        let track = self.session.project.tracks.get(t)?;
        Some(Place::of(track, track.pan))
    }

    /// Moves a track's sound to a place.
    pub fn set_place(&mut self, t: usize, place: Place) {
        let Some(track) = self.session.project.tracks.get_mut(t) else {
            return;
        };
        // To a tenth of a degree and a centimetre: what the ear could tell, and tidy to save.
        track.pan = ((place.azimuth / 180.0).clamp(-1.0, 1.0) * 1800.0).round() / 1800.0;
        let elevation = (place.elevation.clamp(-90.0, 90.0) * 10.0).round() / 10.0;
        let distance = (place.distance * 100.0).round() / 100.0;
        track.elevation = Some(elevation).filter(|&e| e != 0.0);
        track.distance = Some(distance).filter(|&d| d != 1.0);
        let pan = track.pan;
        self.moved(t, pina_engine::project::Param::Pan, pan);
    }

    /// A drag in the view: the source (⌘ raises it) or, anywhere else, the view round.
    fn press_panner(&mut self, t: usize, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let (Some(place), b) = (self.place_of(t), self.panner_bounds.get()) else {
            return;
        };
        let at = (
            f32::from(e.position.x - b.origin.x),
            f32::from(e.position.y - b.origin.y),
        );
        let camera = Camera::new(self.orbit);
        let on_source = camera.project(place.xyz()).is_some_and(|(x, y, scale)| {
            (x - at.0).hypot(y - at.1) <= (SOURCE * scale).max(8.0) + 6.0
        });
        let (x, y) = (f32::from(e.position.x), f32::from(e.position.y));
        let drag = match (on_source, e.modifiers.platform) {
            (true, true) => Drag::Raise {
                track: t,
                y,
                height: place.xyz()[1],
            },
            (true, false) => Drag::Source {
                track: t,
                height: place.xyz()[1],
            },
            _ => Drag::Orbit {
                x,
                y,
                yaw: self.orbit.0,
                pitch: self.orbit.1,
            },
        };
        self.begin_drag(drag, cx);
    }

    /// The source dragged round the head at its height: under the pointer.
    pub fn drag_source(&mut self, t: usize, height: f32, p: Point<f32>) {
        let b = self.panner_bounds.get();
        let at = (p.x - f32::from(b.origin.x), p.y - f32::from(b.origin.y));
        if let Some([x, _, z]) = Camera::new(self.orbit).at_height(at, height) {
            self.set_place(t, Place::from_xyz([x, height, z]));
        }
    }

    /// The source raised or lowered (⌘-drag), staying the same way round and as far out.
    pub fn raise_source(&mut self, t: usize, height: f32) {
        let Some(place) = self.place_of(t) else {
            return;
        };
        let [x, _, z] = place.xyz();
        let height = height.clamp(-Place::FARTHEST, Place::FARTHEST);
        let mut moved = Place::from_xyz([x, height, z]);
        // Straight up or down keeps its way round.
        if x.hypot(z) < 1e-3 {
            moved.azimuth = place.azimuth;
        }
        self.set_place(t, moved);
    }

    pub fn panner_view(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let (t, at) = self.panner?;
        let track = self.session.project.tracks.get(t)?;
        if track.pan_mode != PanMode::Binaural {
            return None;
        }
        let place = Place::of(track, track.pan);
        let color = self.color(t);
        let name = track.name.clone();
        let level = self
            .levels
            .get(&Some(track.id))
            .map_or(0.0, |l| l.now[0].max(l.now[1]));
        let camera = Camera::new(self.orbit);
        let bounds = self.panner_bounds.clone();
        // The compass round the floor, as words where it says.
        let labels = [
            ("FRONT", [0.0, 0.0, 3.35]),
            ("BACK", [0.0, 0.0, -3.35]),
            ("L", [-3.35, 0.0, 0.0]),
            ("R", [3.35, 0.0, 0.0]),
        ]
        .into_iter()
        .filter_map(|(word, p)| {
            let (x, y, _) = camera.project(p)?;
            Some(
                div()
                    .absolute()
                    .left(px(x - 20.0))
                    .top(px(y - 6.0))
                    .w(px(40.))
                    .flex()
                    .justify_center()
                    .font_family(MONO)
                    .text_size(px(9.))
                    .text_color(rgb(if word == "FRONT" { ACCENT } else { TEXT_4 }))
                    .child(word),
            )
        })
        .collect::<Vec<_>>();
        let view = div()
            .id("panner-view")
            .relative()
            .w(px(W))
            .h(px(H))
            .rounded(px(10.))
            .overflow_hidden()
            .bg(rgb(0x0b0b10))
            .border_1()
            .border_color(rgb(LINE))
            .cursor(gpui::CursorStyle::OpenHand)
            .debug_selector(|| "panner-view".into())
            .child(
                canvas(
                    move |b, _, _| bounds.set(b),
                    move |b, _, window, _| paint_scene(b, camera, place, color, level, window),
                )
                .absolute()
                .size_full(),
            )
            .children(labels)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if e.click_count == 2 {
                        ws.orbit = ORBIT;
                        cx.notify();
                        return;
                    }
                    ws.press_panner(t, e, cx);
                }),
            );
        let readout = |label: &str, value: String, slider: Slider, cx: &mut Context<Self>| {
            self.slider_row(label.to_string(), value, slider, cx)
        };
        let height = match place.elevation.round() as i32 {
            0 => "level".to_string(),
            e if e > 0 => format!("{e}° up"),
            e => format!("{}° down", -e),
        };
        let rows = div()
            .flex()
            .flex_col()
            .child(readout(
                "Round",
                azimuth_label(place.azimuth / 180.0),
                Slider::Azimuth(t),
                cx,
            ))
            .child(readout("Height", height, Slider::Elevation(t), cx))
            .child(readout(
                "Distance",
                format!("{:.2} m", place.distance),
                Slider::Distance(t),
                cx,
            ));
        let places = div()
            .flex()
            .gap(px(4.))
            .children(PLACES.map(|(word, pan, elevation)| {
                let here = (place.azimuth / 180.0 - pan).abs() < 0.01
                    && (place.elevation - elevation).abs() < 0.5;
                div()
                    .flex_1()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .text_size(px(11.))
                    .cursor_pointer()
                    .bg(rgb(if here { RAISED } else { FIELD }))
                    .text_color(rgb(if here { TEXT } else { TEXT_2 }))
                    .hover(|s| s.bg(rgb(RAISED)))
                    .debug_selector(move || format!("panner-{}", word.to_lowercase()))
                    .child(word)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            let distance = ws.place_of(t).map_or(1.0, |p| p.distance);
                            ws.edit(cx, |ws| {
                                ws.set_place(
                                    t,
                                    Place {
                                        azimuth: pan * 180.0,
                                        elevation,
                                        distance,
                                    },
                                )
                            });
                        }),
                    )
            }));
        let panel = div()
            .id("panner")
            .occlude()
            .w(px(W + 24.))
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .rounded(px(14.))
            .bg(rgb(0x1a1a20))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(color)))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(format!("Binaural Panner · {name}")),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .text_color(rgb(TEXT_3))
                            .hover(|s| s.bg(rgb(RAISED)).text_color(rgb(TEXT)))
                            .debug_selector(|| "panner-close".into())
                            .child("✕")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.panner = None;
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(view)
            .child(places)
            .child(rows)
            .child(div().text_size(px(10.5)).text_color(rgb(TEXT_4)).child(
                "Drag the sound round the head · ⌘-drag it up or down · drag the space \
                         to look round (double-click: as it was) · for headphones",
            ))
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                if ws.drag.is_none() {
                    ws.panner = None;
                    cx.notify();
                }
            }));
        Some(deferred(anchored().position(at).snap_to_window().child(panel)).with_priority(1))
    }
}

/// The scene: the floor's rings and spokes, the source's height arc, shadow and stem, the line
/// to the head, the head (its nose ahead, its ears), and the source pulsing with the level;
/// nearer things over further ones.
fn paint_scene(
    b: Bounds<Pixels>,
    camera: Camera,
    place: Place,
    color: u32,
    level: f32,
    window: &mut Window,
) {
    let o = b.origin;
    let at = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
    let line = |points: &[[f32; 3]], width: f32, ink: Hsla, window: &mut Window| {
        let mut path = gpui::PathBuilder::stroke(px(width));
        let mut started = false;
        for p in points {
            match camera.project(*p) {
                Some((x, y, _)) if started => path.line_to(at(x, y)),
                Some((x, y, _)) => {
                    path.move_to(at(x, y));
                    started = true;
                }
                None => started = false,
            }
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, ink);
        }
    };
    let ring = |r: f32, y: f32| -> Vec<[f32; 3]> {
        (0..=72)
            .map(|k| {
                let a = (k as f32 * 5.0).to_radians();
                [r * a.sin(), y, r * a.cos()]
            })
            .collect()
    };
    let disc = |p: [f32; 3], r: f32, ink: Hsla, window: &mut Window| {
        if let Some((x, y, scale)) = camera.project(p) {
            let d = (r * scale).max(1.0);
            window.paint_quad(
                fill(
                    Bounds::centered_at(at(x, y), size(px(d * 2.0), px(d * 2.0))),
                    ink,
                )
                .corner_radii(px(d)),
            );
        }
    };
    let white = |a: f32| alpha(0xffffff, a);
    let tint = |a: f32| alpha(color, a);

    // The floor.
    for r in [1.0, 2.0, 3.0] {
        line(
            &ring(r, 0.0),
            1.0,
            white(if r == 1.0 { 0.16 } else { 0.08 }),
            window,
        );
    }
    for k in 0..12 {
        let a = (k as f32 * 30.0).to_radians();
        let ink = if k == 0 {
            alpha(ACCENT, 0.5)
        } else {
            white(0.05)
        };
        line(
            &[
                [0.35 * a.sin(), 0.0, 0.35 * a.cos()],
                [3.0 * a.sin(), 0.0, 3.0 * a.cos()],
            ],
            1.0,
            ink,
            window,
        );
    }
    let [x, y, z] = place.xyz();
    let a = place.azimuth.to_radians();
    let flat = x.hypot(z);
    // Its height: the arc from the floor up over the head, and the part it's risen.
    let arc = |from: f32, to: f32| -> Vec<[f32; 3]> {
        (0..=24)
            .map(|k| {
                let e = (from + (to - from) * k as f32 / 24.0).to_radians();
                let d = place.distance;
                [d * a.sin() * e.cos(), d * e.sin(), d * a.cos() * e.cos()]
            })
            .collect()
    };
    line(&arc(-90.0, 90.0), 1.0, white(0.06), window);
    line(&arc(0.0, place.elevation), 2.0, tint(0.55), window);
    line(&ring(flat, 0.0), 1.0, tint(0.18), window);
    // The circle it'd go round at its height.
    if y.abs() > 0.02 {
        line(&ring(flat, y), 1.0, tint(0.3), window);
    }
    // Its shadow on the floor, and the stem down to it.
    let shadow: Vec<[f32; 3]> = (0..=24)
        .map(|k| {
            let t = (k as f32 * 15.0).to_radians();
            [x + 0.16 * t.sin(), 0.0, z + 0.16 * t.cos()]
        })
        .collect();
    let mut path = gpui::PathBuilder::fill();
    for (k, p) in shadow.iter().filter_map(|p| camera.project(*p)).enumerate() {
        match k {
            0 => path.move_to(at(p.0, p.1)),
            _ => path.line_to(at(p.0, p.1)),
        }
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, alpha(0x000000, 0.55));
    }
    for k in 0..12 {
        let (y0, y1) = (y * k as f32 / 12.0, y * (k as f32 + 0.5) / 12.0);
        line(&[[x, y0, z], [x, y1, z]], 1.0, tint(0.5), window);
    }
    // The line it travels to the head.
    line(&[[x, y, z], [0.0, 0.0, 0.0]], 1.0, tint(0.3), window);

    let head = |window: &mut Window| {
        disc([0.0, 0.0, 0.0], HEAD, rgb(0x3b3b48).into(), window);
        disc(
            [-0.04, 0.06, -0.02],
            HEAD * 0.7,
            rgb(0x4a4a58).into(),
            window,
        );
        for side in [-1.0, 1.0] {
            disc([side * HEAD, 0.0, 0.0], 0.05, rgb(0x5a5a6c).into(), window);
        }
        // Its nose, ahead.
        disc([0.0, 0.0, HEAD], 0.05, alpha(ACCENT, 0.9), window);
    };
    let source = |window: &mut Window| {
        let p = [x, y, z];
        for k in 1..=3 {
            let grow = 1.0 + k as f32 * (0.6 + 2.2 * level.min(1.5));
            disc(p, SOURCE * grow, tint(0.16 / k as f32), window);
        }
        disc(p, SOURCE, rgb(color).into(), window);
        disc([x - 0.03, y + 0.04, z], SOURCE * 0.45, white(0.55), window);
    };
    if camera.depth([x, y, z]) > camera.depth([0.0, 0.0, 0.0]) {
        source(window);
        head(window);
    } else {
        head(window);
        source(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spot_in_the_view_finds_the_point_that_shows_there() {
        let camera = Camera::new(ORBIT);
        let p = [1.2, 0.4, -0.7];
        let (x, y, _) = camera.project(p).unwrap();
        let back = camera.at_height((x, y), 0.4).unwrap();
        assert!(
            back.iter().zip(p).all(|(a, b)| (a - b).abs() < 1e-3),
            "{back:?}"
        );
        // Seen from behind, the head's right is the view's right and front is further up.
        let (rx, ..) = camera.project([1.0, 0.0, 0.0]).unwrap();
        let (_, fy, _) = camera.project([0.0, 0.0, 1.0]).unwrap();
        let (cx, cy, _) = camera.project([0.0, 0.0, 0.0]).unwrap();
        assert!(rx > cx && fy < cy);
        assert_eq!(azimuth_label(-0.25), "L45°");
    }
}
