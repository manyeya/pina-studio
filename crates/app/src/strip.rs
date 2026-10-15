//! A channel strip, as in Logic's inspector and mixer: instrument and inserts, pan, level, fader
//! and meter, mute and solo, name. Track `None` is the master, Stereo Out.

use gpui::{
    Context, Div, Hsla, MouseButton, MouseDownEvent, PathBuilder, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};
use tempo_engine::project::{Instrument, Track};

use crate::{
    theme::*,
    workspace::{Drag, Workspace},
};

/// Quieter than anything audible: the fader's floor.
pub const SILENT: f32 = -144.0;
const INSERT_ROWS: usize = 3;

/// Fader position (0 to 1) for a gain in dB: an audio taper, 0 dB at about 0.7, +6 dB at the top.
pub fn fader_pos(db: f32) -> f32 {
    10f32.powf((db - 6.0) / 40.0).clamp(0.0, 1.0)
}

pub fn fader_db(pos: f32) -> f32 {
    if pos <= 1e-4 {
        SILENT
    } else {
        (40.0 * pos.log10() + 6.0).max(SILENT)
    }
}

/// Meter height (0 to 1) for a peak level: -60 dB to +6 dB.
fn meter_pos(level: f32) -> f32 {
    ((20.0 * level.max(1e-9).log10() + 60.0) / 66.0).clamp(0.0, 1.0)
}

fn slots(t: &Track) -> Vec<(String, bool)> {
    let instrument = match &t.instrument {
        None => ("Audio".to_string(), false),
        Some(Instrument::Synth) => ("Sine Synth".to_string(), true),
        Some(Instrument::Vst3(r)) => (r.class.clone(), true),
    };
    let mut rows = vec![instrument];
    rows.extend(t.inserts.iter().map(|r| (r.class.clone(), true)));
    rows
}

fn set_volume(ws: &mut Workspace, track: Option<usize>, db: f32) {
    match track {
        Some(t) => ws.session.project.tracks[t].volume_db = db,
        None => ws.session.project.master_volume_db = db,
    }
}

pub fn strip(ws: &Workspace, track: Option<usize>, cx: &mut Context<Workspace>) -> Div {
    let project = &ws.session.project;
    let t = track.map(|i| &project.tracks[i]);
    let volume = t.map_or(project.master_volume_db, |t| t.volume_db);
    let level = t.map_or(ws.master_level, |t| {
        ws.levels.get(&t.id).copied().unwrap_or(0.0)
    });
    let color = t.map_or(0x6b6b72, |t| track_color(t.instrument.is_some()));
    let name = t.map_or("Stereo Out".to_string(), |t| t.name.clone());
    let rows = t.map(slots).unwrap_or_default();
    let pos = fader_pos(volume);

    let slot = |text: Option<&(String, bool)>| {
        let (label, used) = text.cloned().unwrap_or_default();
        div()
            .w(px(STRIP_W - 8.))
            .h(px(17.))
            .px_1()
            .rounded(px(3.))
            .text_size(px(10.))
            .whitespace_nowrap()
            .overflow_hidden()
            .bg(rgb(if used { 0x3e4a5a } else { 0x1c1c1e }))
            .text_color(rgb(if used { TEXT } else { TEXT_DIM }))
            .child(label)
    };

    let fader = div()
        .relative()
        .w(px(34.))
        .h(px(FADER_H))
        .cursor_ns_resize()
        .child(
            div()
                .absolute()
                .left(px(15.))
                .top_0()
                .w(px(4.))
                .h_full()
                .rounded(px(2.))
                .bg(rgb(0x121214)),
        )
        .child(
            div()
                .absolute()
                .left(px(3.))
                .top(px((1.0 - pos) * FADER_H - 7.0))
                .w(px(28.))
                .h(px(14.))
                .rounded(px(3.))
                .bg(rgb(0xa2a2aa))
                .border_1()
                .border_color(rgb(0x5a5a62))
                .flex()
                .items_center()
                .child(div().w_full().h(px(1.)).bg(rgb(0x2a2a2e))),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                if e.modifiers.alt {
                    ws.edit(cx, |ws| set_volume(ws, track, 0.0));
                } else {
                    let y = f32::from(e.position.y);
                    ws.begin_drag(Drag::Fader { track, y, pos }, cx);
                }
            }),
        );

    let m = meter_pos(level);
    let meter = div().w(px(6.)).h(px(FADER_H)).child(
        canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                window.paint_quad(fill(b, rgb(0x121214)));
                let (h, bottom) = (f32::from(b.size.height), b.origin.y + b.size.height);
                let segments = [
                    (0.0, 54.0 / 66.0, METER_LOW),
                    (54.0 / 66.0, 60.0 / 66.0, METER_MID),
                    (60.0 / 66.0, 1.0, METER_HIGH),
                ];
                for (from, to, color) in segments {
                    let top = m.min(to);
                    if top > from {
                        let y = bottom - px(top * h);
                        let hh = px((top - from) * h);
                        window.paint_quad(fill(
                            gpui::Bounds::new(point(b.origin.x, y), size(b.size.width, hh)),
                            rgb(color),
                        ));
                    }
                }
            },
        )
        .size_full(),
    );

    let level_text = if volume <= -100.0 {
        "-∞".to_string()
    } else {
        format!("{volume:.1}")
    };

    div()
        .w(px(STRIP_W))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .py_1()
        .rounded(px(4.))
        .bg(rgb(if track.is_some() && track == Some(ws.track) {
            0x36363c
        } else {
            PANEL
        }))
        .when_some(track, |d, i| {
            d.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.track = i;
                    cx.notify();
                }),
            )
        })
        .children((0..=INSERT_ROWS).map(|i| slot(rows.get(i))))
        .child(match t {
            Some(t) => {
                let i = track.unwrap();
                let pan = t.pan;
                div().cursor_ns_resize().child(knob(pan)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if e.modifiers.alt {
                            ws.edit(cx, |ws| ws.session.project.tracks[i].pan = 0.0);
                        } else {
                            let y = f32::from(e.position.y);
                            ws.begin_drag(Drag::Pan { track: i, y, pan }, cx);
                        }
                    }),
                )
            }
            None => div().h(px(28.)),
        })
        .child(div().font_family(MONO).text_size(px(10.)).child(level_text))
        .child(div().flex().gap_1().child(fader).child(meter))
        .child(match track {
            Some(i) => {
                let (mute, solo) = (t.unwrap().mute, t.unwrap().solo);
                div()
                    .flex()
                    .gap_1()
                    .child(toggle("M", mute, MUTE).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| ws.session.project.tracks[i].mute ^= true);
                        }),
                    ))
                    .child(toggle("S", solo, SOLO).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| ws.session.project.tracks[i].solo ^= true);
                        }),
                    ))
            }
            None => div().h(px(16.)),
        })
        .child(
            div()
                .w(px(STRIP_W - 8.))
                .h(px(18.))
                .px_1()
                .rounded(px(3.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.))
                .whitespace_nowrap()
                .overflow_hidden()
                .bg(Hsla::from(rgb(color)).opacity(0.9))
                .text_color(rgb(0x101010))
                .child(name),
        )
}

/// A pan knob, its pointer turned up to 135° either way.
fn knob(pan: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let r = b.size.width.min(b.size.height) / 2.0 - px(1.);
            let mut disc = PathBuilder::fill();
            disc.move_to(point(c.x + r, c.y));
            disc.arc_to(point(r, r), px(0.), false, false, point(c.x - r, c.y));
            disc.arc_to(point(r, r), px(0.), false, false, point(c.x + r, c.y));
            disc.close();
            if let Ok(p) = disc.build() {
                window.paint_path(p, rgb(0x46464d));
            }
            let a = pan * 135f32.to_radians();
            let mut pointer = PathBuilder::stroke(px(2.));
            pointer.move_to(c);
            pointer.line_to(point(c.x + r * a.sin(), c.y - r * a.cos()));
            if let Ok(p) = pointer.build() {
                window.paint_path(p, rgb(0xf0f0f0));
            }
        },
    )
    .size(px(28.))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fader_taper_round_trips() {
        for db in [-60.0, -12.0, 0.0, 6.0] {
            assert!((fader_db(fader_pos(db)) - db).abs() < 1e-3);
        }
        assert!((fader_pos(0.0) - 0.708).abs() < 1e-3);
        assert_eq!(fader_db(0.0), SILENT);
    }
}
