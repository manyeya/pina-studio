//! Playing it from the window: four octaves of keys (lower on a key is louder), the bend and mod
//! wheels, the arpeggiator, and a demo of each kind of patch.

use std::time::Duration;

use super::*;
use pina_engine::{audio::engine::LiveNote, plugin::ControlKind};

pub(crate) const LOWEST: u8 = 36;
pub(crate) const HIGHEST: u8 = 84;
/// How far down the keys the black ones reach.
const BLACK_DEPTH: f32 = 0.62;

fn black(n: u8) -> bool {
    matches!(n % 12, 1 | 3 | 6 | 8 | 10)
}

/// The note under (x, y), each 0 to 1 across keys `lo` to `hi`: a black key where one reaches,
/// else the white.
pub(crate) fn key_at(x: f32, y: f32, lo: u8, hi: u8) -> Option<u8> {
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    let whites: Vec<u8> = (lo..=hi).filter(|n| !black(*n)).collect();
    let w = 1.0 / whites.len() as f32;
    let i = ((x / w) as usize).min(whites.len() - 1);
    let n = whites[i];
    if y < BLACK_DEPTH {
        let into = x / w - i as f32;
        if into > 0.7 && n < hi && black(n + 1) {
            return Some(n + 1);
        }
        if into < 0.3 && n > lo && black(n - 1) {
            return Some(n - 1);
        }
    }
    Some(n)
}

/// What a demo plays for each part: (beat, note, beats long).
fn phrase(part: &str) -> &'static [(f32, u8, f32)] {
    match part {
        "bass" => &[
            (0.0, 36, 0.7),
            (1.0, 36, 0.4),
            (1.5, 43, 0.4),
            (2.0, 46, 0.7),
            (3.0, 48, 0.4),
            (3.5, 46, 0.4),
            (4.0, 36, 0.7),
            (5.0, 39, 0.4),
            (5.5, 41, 0.4),
            (6.0, 43, 1.5),
        ],
        "pad" => &[
            (0.0, 57, 3.8),
            (0.0, 60, 3.8),
            (0.0, 64, 3.8),
            (0.0, 71, 3.8),
            (4.0, 53, 3.8),
            (4.0, 60, 3.8),
            (4.0, 64, 3.8),
            (4.0, 69, 3.8),
        ],
        "keys" => &[
            (0.0, 57, 0.9),
            (0.0, 60, 0.9),
            (0.0, 64, 0.9),
            (1.0, 72, 0.4),
            (1.5, 76, 0.4),
            (2.0, 53, 0.9),
            (2.0, 60, 0.9),
            (2.0, 69, 0.9),
            (3.0, 72, 0.4),
            (4.0, 55, 0.9),
            (4.0, 62, 0.9),
            (4.0, 71, 0.9),
            (5.0, 74, 0.9),
            (6.0, 52, 1.6),
            (6.0, 59, 1.6),
            (6.0, 67, 1.6),
        ],
        "pluck" => &[
            (0.0, 69, 0.2),
            (0.5, 72, 0.2),
            (1.0, 76, 0.2),
            (1.5, 81, 0.2),
            (2.0, 76, 0.2),
            (2.5, 72, 0.2),
            (3.0, 74, 0.2),
            (3.5, 77, 0.2),
            (4.0, 81, 0.2),
            (4.5, 84, 0.2),
            (5.0, 81, 0.2),
            (5.5, 77, 0.2),
            (6.0, 76, 1.0),
        ],
        "fx" => &[(0.0, 48, 3.8), (4.0, 60, 3.8)],
        _ => &[
            (0.0, 69, 0.9),
            (1.0, 72, 0.45),
            (1.5, 76, 0.9),
            (2.5, 74, 0.45),
            (3.0, 72, 0.9),
            (4.0, 71, 0.45),
            (4.5, 67, 0.9),
            (5.5, 69, 2.0),
        ],
    }
}

/// A demo's notes on and off in order (seconds, pitch, velocity), and a beat's length.
type Demo = (Vec<(f32, u8, u8)>, f32);

/// A held chord, for an arpeggio to play.
const CHORD: [(f32, u8, f32); 3] = [(0.0, 57, 7.9), (0.0, 60, 7.9), (0.0, 64, 7.9)];

impl Workspace {
    /// A note played live: to the synth whose window is open, unless its track is armed (then as
    /// any live note goes, so it records).
    pub(crate) fn play_note(&mut self, pitch: u8, velocity: u8) {
        let armed = self.armed();
        let to = self
            .poly_view
            .as_ref()
            .map(|v| v.track)
            .filter(|&t| armed != Some(t))
            .and_then(|t| self.session.project.tracks.get(t))
            .map(|t| t.id);
        if let Some(v) = self.poly_view.as_mut() {
            v.held.retain(|&p| p != pitch);
            if velocity > 0 {
                v.held.push(pitch);
            }
        }
        match to {
            Some(id) => self.player.preview(id, pitch, velocity),
            None => self.player.send_live(LiveNote { pitch, velocity }),
        }
    }

    /// The pointer on the keys: the note it's over now, the one before let go.
    pub(crate) fn poly_finger(&mut self, at: gpui::Point<f32>) {
        let Some(v) = self.poly_view.as_ref() else {
            return;
        };
        let b = v.keys.get();
        let x = (at.x - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0);
        let y = (at.y - f32::from(b.origin.y)) / f32::from(b.size.height).max(1.0);
        let now = key_at(x, y, LOWEST, HIGHEST);
        let was = v.finger;
        if now == was {
            return;
        }
        if let Some(p) = was {
            self.play_note(p, 0);
        }
        if let Some(p) = now {
            // Further down the key, harder.
            self.play_note(p, (50.0 + 77.0 * y.clamp(0.0, 1.0)).round() as u8);
        }
        if let Some(v) = self.poly_view.as_mut() {
            v.finger = now;
        }
    }

    /// The finger lifted off the keys.
    pub(crate) fn poly_lift(&mut self) {
        if let Some(p) = self.poly_view.as_mut().and_then(|v| v.finger.take()) {
            self.play_note(p, 0);
        }
    }

    /// A wheel moved to `at`: the mod wheel 0 to 1, bend -1 to 1.
    pub(crate) fn poly_wheel(&mut self, t: usize, bend: bool, top: f32, height: f32, y: f32) {
        let up = (1.0 - (y - top) / height.max(1.0)).clamp(0.0, 1.0);
        self.set_wheel(t, bend, if bend { up * 2.0 - 1.0 } else { up });
    }

    pub(crate) fn set_wheel(&mut self, t: usize, bend: bool, value: f32) {
        let Some(id) = self.session.project.tracks.get(t).map(|t| t.id) else {
            return;
        };
        let kind = match bend {
            true => ControlKind::PitchBend(value),
            false => ControlKind::Cc { number: 1, value },
        };
        self.player.control(id, kind);
        if let Some(v) = self.poly_view.as_mut() {
            match bend {
                true => v.bend = value,
                false => v.wheel = value,
            }
        }
    }

    /// Starts or stops the demo: a phrase for the patch's part, round and round.
    pub(crate) fn poly_demo(&mut self, cx: &mut Context<Self>) {
        let Some(v) = self.poly_view.as_mut() else {
            return;
        };
        if v.demo.is_some() {
            self.poly_all_off();
            cx.notify();
            return;
        }
        v.turns += 1;
        let turn = v.turns;
        v.demo = Some(turn);
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(Some((notes, beat))) = this.update(cx, |ws, _| ws.demo_notes(turn)) else {
                    return;
                };
                let mut at = 0.0;
                for (when, pitch, velocity) in notes {
                    let wait = (when - at).max(0.0);
                    at = when;
                    cx.background_executor()
                        .timer(Duration::from_secs_f32(wait))
                        .await;
                    let go = this.update(cx, |ws, cx| {
                        let on = ws.poly_view.as_ref().and_then(|v| v.demo) == Some(turn);
                        if on {
                            ws.play_note(pitch, velocity);
                            cx.notify();
                        }
                        on
                    });
                    if !go.unwrap_or(false) {
                        return;
                    }
                }
                cx.background_executor()
                    .timer(Duration::from_secs_f32((beat * 8.0 - at).max(0.0)))
                    .await;
            }
        })
        .detach();
        cx.notify();
    }

    /// The demo's notes for the synth as it is now, on and off in order (seconds, pitch,
    /// velocity), and a beat's length; none once it's stopped.
    fn demo_notes(&self, turn: u64) -> Option<Demo> {
        let v = self.poly_view.as_ref().filter(|v| v.demo == Some(turn))?;
        let p = self.poly(v.track)?;
        let part = POLY_PATCHES
            .iter()
            .find(|q| q.0 == p.patch)
            .map_or("lead", |q| q.1);
        let beat = 60.0 / self.session.project.tempo.max(1.0) as f32;
        let list: &[(f32, u8, f32)] = match p.arp.mode {
            ArpMode::Off => phrase(part),
            _ => &CHORD,
        };
        let mut notes: Vec<(f32, u8, u8)> = list
            .iter()
            .flat_map(|&(at, pitch, len)| [(at * beat, pitch, 96), ((at + len) * beat, pitch, 0)])
            .collect();
        notes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
        Some((notes, beat))
    }

    /// The bottom of the window: wheels, keys, arpeggiator.
    pub(crate) fn poly_keys_row(&self, t: usize, p: &Poly, cx: &mut Context<Self>) -> Div {
        let Some(v) = self.poly_view.as_ref() else {
            return div();
        };
        let held = v.held.clone();
        let typed = self
            .typing
            .map(|o| (60 + 12 * o as i32).clamp(0, 127) as u8);
        let bounds = v.keys.clone();
        let (wheel, bend) = (v.wheel, v.bend);
        let keys = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                let whites: Vec<u8> = (LOWEST..=HIGHEST).filter(|n| !black(*n)).collect();
                let w = f32::from(b.size.width) / whites.len() as f32;
                let h = f32::from(b.size.height);
                let at = |x: f32, top: f32, wide: f32, tall: f32| {
                    Bounds::new(
                        point(b.origin.x + px(x), b.origin.y + px(top)),
                        gpui::size(px(wide), px(tall)),
                    )
                };
                for (i, &n) in whites.iter().enumerate() {
                    let down = held.contains(&n);
                    window.paint_quad(
                        gpui::fill(
                            at(i as f32 * w, 0.0, w - 1.0, h),
                            rgb(if down { 0x9a9aa5 } else { 0xdfdfe4 }),
                        )
                        .corner_radii(gpui::Corners {
                            bottom_left: px(3.),
                            bottom_right: px(3.),
                            ..Default::default()
                        }),
                    );
                    if Some(n) == typed {
                        window.paint_quad(
                            gpui::fill(
                                at(i as f32 * w + w / 2.0 - 3.0, h - 22.0, 6.0, 6.0),
                                rgb(ACCENT),
                            )
                            .corner_radii(px(3.)),
                        );
                    }
                }
                for (i, &n) in whites.iter().enumerate() {
                    if n < HIGHEST && black(n + 1) {
                        let down = held.contains(&(n + 1));
                        window.paint_quad(
                            gpui::fill(
                                at((i as f32 + 0.7) * w, 0.0, w * 0.6, h * BLACK_DEPTH),
                                rgb(if down { 0x6a6a75 } else { 0x17171b }),
                            )
                            .corner_radii(gpui::Corners {
                                bottom_left: px(2.),
                                bottom_right: px(2.),
                                ..Default::default()
                            }),
                        );
                    }
                }
            },
        )
        .size_full();
        // The C's names along the bottom, in place.
        let whites = (LOWEST..=HIGHEST).filter(|n| !black(*n)).count() as f32;
        let names = (LOWEST..=HIGHEST).filter(|n| n % 12 == 0).map(move |n| {
            let i = (LOWEST..n).filter(|m| !black(*m)).count() as f32;
            div()
                .absolute()
                .bottom(px(3.))
                .left(gpui::relative(i / whites))
                .w(gpui::relative(1.0 / whites))
                .flex()
                .justify_center()
                .font_family(MONO)
                .text_size(px(9.5))
                .text_color(rgb(0x55555f))
                .child(format!("C{}", n as i32 / 12 - 1))
        });
        let wheel_strip = |bend: bool, value: f32, cx: &mut Context<Self>| {
            let fill = if bend { (value + 1.0) / 2.0 } else { value };
            div()
                .relative()
                .w(px(18.))
                .h_full()
                .rounded(px(4.))
                .bg(rgb(WELL))
                .overflow_hidden()
                .cursor_ns_resize()
                .debug_selector(move || if bend { "poly-bend" } else { "poly-wheel" }.into())
                .when(!bend, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .h(gpui::relative(fill))
                            .bg(rgb(LINE_STRONG)),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .left(px(2.))
                        .right(px(2.))
                        .h(px(4.))
                        .bottom(gpui::relative(fill))
                        .mb(px(-2.))
                        .rounded(px(2.))
                        .bg(rgb(TEXT)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let Some(b) = ws.poly_view.as_ref().map(|v| v.keys.get()) else {
                            return;
                        };
                        let (top, height) = (f32::from(b.origin.y), f32::from(b.size.height));
                        ws.begin_drag(
                            Drag::PolyWheel {
                                track: t,
                                bend,
                                top,
                                height,
                            },
                            cx,
                        );
                        ws.poly_wheel(t, bend, top, height, pt(e.position).y);
                        cx.notify();
                    }),
                )
        };
        let modes = [
            ("Off", ArpMode::Off),
            ("Up", ArpMode::Up),
            ("Down", ArpMode::Down),
            ("Up and down", ArpMode::UpDown),
            ("As played", ArpMode::Played),
            ("Random", ArpMode::Random),
            ("Chord", ArpMode::Chord),
        ];
        let mode = p.arp.mode;
        let arp = self.poly_menu(
            "poly-arp".into(),
            match mode {
                ArpMode::Off => "Arp off".to_string(),
                m => modes
                    .iter()
                    .find(|x| x.1 == m)
                    .map_or("Arp", |x| x.0)
                    .to_string(),
            },
            move |_| {
                modes
                    .iter()
                    .map(|&(name, m)| (name.to_string(), MenuAction::Poly(t, PolyOp::ArpMode(m))))
                    .collect()
            },
            false,
            cx,
        );
        let rates = [("1/32", 0.125), ("1/16", 0.25), ("1/8", 0.5), ("1/4", 1.0)];
        let beats = p.arp.beats;
        let rate = self.poly_menu(
            "poly-arp-rate".into(),
            rates
                .iter()
                .find(|r| r.1 == beats)
                .map_or_else(|| format!("{beats} beats"), |r| r.0.to_string()),
            move |_| {
                rates
                    .iter()
                    .map(|&(n, b)| (n.to_string(), MenuAction::Poly(t, PolyOp::ArpBeats(b))))
                    .collect()
            },
            false,
            cx,
        );
        div()
            .flex_none()
            .h(px(KEYS_H))
            .px(px(12.))
            .py(px(8.))
            .flex()
            .gap(px(10.))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .h_full()
                    .flex()
                    .gap(px(4.))
                    .child(wheel_strip(true, bend, cx))
                    .child(wheel_strip(false, wheel, cx)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .debug_selector(|| "poly-keys".into())
                    .child(keys)
                    .children(names)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.begin_drag(Drag::PolyKeys, cx);
                            ws.poly_finger(pt(e.position));
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(OUTPUT_W + 112.0))
                    .flex()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .w(px(104.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(arp)
                            .child(rate)
                            .child(label("Type A to K to play").text_size(px(10.))),
                    )
                    .child(self.poly_amount(t, PolyAmount::Octaves, p, cx))
                    .child(self.poly_amount(t, PolyAmount::Gate, p, cx))
                    .child(self.poly_amount(t, PolyAmount::Swing, p, cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_found_where_it_is_drawn() {
        // C3 to C4: eight white keys.
        let (lo, hi) = (48, 60);
        let w = 1.0 / 8.0;
        assert_eq!(key_at(0.5 * w, 0.9, lo, hi), Some(48), "C, low on the key");
        assert_eq!(
            key_at(0.95 * w, 0.3, lo, hi),
            Some(49),
            "C sharp, high between C and D"
        );
        assert_eq!(key_at(0.95 * w, 0.9, lo, hi), Some(48), "under it, C again");
        assert_eq!(key_at(2.95 * w, 0.3, lo, hi), Some(52), "E has no sharp");
        assert_eq!(key_at(7.5 * w, 0.3, lo, hi), Some(60), "the top C");
        assert_eq!(key_at(1.2, 0.5, lo, hi), None);
    }
}
