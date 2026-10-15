//! A channel strip, as in Logic's inspector and mixer: plugin slots, sends, pan, level, fader and
//! meter, mute and solo, name; for a track, an aux, or the master (Stereo Out).
//!
//! Clicking a plugin opens its editor; clicking an empty slot, or right-clicking any, picks a
//! plugin. Sends drag up and down for level; right-click one to remove it.

use gpui::{
    Context, Div, Hsla, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, canvas, div, fill,
    point, prelude::*, px, rgb, size,
};
use tempo_engine::{
    player::Which,
    project::{Instrument, PluginRef},
};

use crate::{
    menu::{Menu, MenuAction},
    theme::*,
    workspace::{Drag, Workspace},
};

/// Quieter than anything audible: the fader's floor.
pub const SILENT: f32 = -144.0;
/// Plugin rows: the instrument (or input) and three inserts.
const SLOT_ROWS: usize = 4;
/// Send rows: two sends and the one that adds a send.
const SEND_ROWS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Channel {
    Track(usize),
    Aux(usize),
    Master,
}

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

fn db_text(db: f32) -> String {
    if db <= -100.0 {
        "-∞".to_string()
    } else {
        format!("{db:.1}")
    }
}

impl Workspace {
    pub fn volume(&self, ch: Channel) -> f32 {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].volume_db,
            Channel::Aux(i) => p.auxes[i].volume_db,
            Channel::Master => p.master_volume_db,
        }
    }

    pub fn set_volume(&mut self, ch: Channel, db: f32) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].volume_db = db,
            Channel::Aux(i) => p.auxes[i].volume_db = db,
            Channel::Master => p.master_volume_db = db,
        }
    }

    pub fn set_pan(&mut self, ch: Channel, pan: f32) {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => p.tracks[i].pan = pan,
            Channel::Aux(i) => p.auxes[i].pan = pan,
            Channel::Master => {}
        }
    }

    /// The inserts of a track or aux.
    pub fn inserts_mut(&mut self, ch: Channel) -> Option<&mut Vec<PluginRef>> {
        let p = &mut self.session.project;
        match ch {
            Channel::Track(i) => Some(&mut p.tracks[i].inserts),
            Channel::Aux(i) => Some(&mut p.auxes[i].inserts),
            Channel::Master => None,
        }
    }

    fn channel_id(&self, ch: Channel) -> Option<u64> {
        let p = &self.session.project;
        match ch {
            Channel::Track(i) => Some(p.tracks[i].id),
            Channel::Aux(i) => Some(p.auxes[i].id),
            Channel::Master => None,
        }
    }

    /// Opens a plugin's editor, saying why not if it can't.
    fn open_editor(&mut self, ch: Channel, which: Which, cx: &mut Context<Self>) {
        let Some(id) = self.channel_id(ch) else {
            return;
        };
        if let Err(e) = self.player.open_editor(id, which) {
            self.status = Some(e);
            cx.notify();
        }
    }

    /// The plugins that can go in a slot: instruments for the instrument slot, effects for
    /// inserts, by name.
    fn plugin_menu(&self, ch: Channel, which: Which, at: Point<Pixels>) -> Menu {
        let instrument = which == Which::Instrument;
        let mut items = Vec::new();
        match (ch, which) {
            (Channel::Track(track), Which::Instrument) => {
                let action = MenuAction::Instrument {
                    track,
                    instrument: Instrument::Synth,
                };
                items.push(("Sine Synth".to_string(), action));
            }
            (_, Which::Insert(index)) => {
                let action = MenuAction::Insert {
                    channel: ch,
                    index,
                    plugin: None,
                };
                items.push(("No Plug-in".to_string(), action));
            }
            _ => {}
        }
        for p in self
            .plugins
            .iter()
            .flatten()
            .filter(|p| p.instrument == instrument)
        {
            let plugin = PluginRef {
                path: p.path.clone(),
                class: p.class.clone(),
                state: None,
            };
            let action = match (ch, which) {
                (Channel::Track(track), Which::Instrument) => MenuAction::Instrument {
                    track,
                    instrument: Instrument::Vst3(plugin),
                },
                (_, Which::Insert(index)) => MenuAction::Insert {
                    channel: ch,
                    index,
                    plugin: Some(plugin),
                },
                _ => continue,
            };
            items.push((format!("{} — {}", p.class, p.vendor), action));
        }
        if self.plugins.is_none() {
            items.push(("Looking for plugins…".to_string(), MenuAction::Nothing));
        }
        Menu { at, items }
    }

    /// Where a track can play: the master, or any aux or stack.
    fn output_menu(&self, track: usize, at: Point<Pixels>) -> Menu {
        let mut items = vec![(
            "Stereo Out".to_string(),
            MenuAction::Output { track, bus: None },
        )];
        items.extend(self.session.project.auxes.iter().map(|a| {
            (
                format!("Bus {} ({})", a.bus, a.name),
                MenuAction::Output {
                    track,
                    bus: Some(a.bus),
                },
            )
        }));
        Menu { at, items }
    }

    /// The buses a track can send to: every aux, and a new one.
    fn send_menu(&self, track: usize, at: Point<Pixels>) -> Menu {
        let mut items: Vec<_> = self
            .session
            .project
            .auxes
            .iter()
            .map(|a| {
                (
                    format!("Bus {} ({})", a.bus, a.name),
                    MenuAction::Send {
                        track,
                        bus: Some(a.bus),
                    },
                )
            })
            .collect();
        items.push(("New Aux".to_string(), MenuAction::Send { track, bus: None }));
        Menu { at, items }
    }
}

/// A row in the strip's plugin or send area.
fn row(label: String, used: bool) -> Div {
    div()
        .flex_none()
        .w(px(STRIP_W - 8.))
        .h(px(17.))
        .px_1()
        .rounded(px(3.))
        .flex()
        .items_center()
        .text_size(px(10.))
        .whitespace_nowrap()
        .overflow_hidden()
        .bg(rgb(if used { 0x3e4a5a } else { 0x1c1c1e }))
        .text_color(rgb(if used { TEXT } else { TEXT_DIM }))
        .child(label)
}

/// A plugin slot: left-click opens the plugin (or picks one, if empty), right-click picks one.
fn plugin_slot(
    label: String,
    used: bool,
    ch: Channel,
    which: Which,
    cx: &mut Context<Workspace>,
) -> Div {
    row(label, used)
        .debug_selector(move || format!("slot-{ch:?}-{which:?}"))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                if used && !(which == Which::Instrument && ws.has_builtin_instrument(ch)) {
                    ws.open_editor(ch, which, cx);
                } else {
                    ws.menu = Some(ws.plugin_menu(ch, which, e.position));
                    cx.notify();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ws.menu = Some(ws.plugin_menu(ch, which, e.position));
                cx.notify();
            }),
        )
}

impl Workspace {
    fn has_builtin_instrument(&self, ch: Channel) -> bool {
        matches!(ch, Channel::Track(i) if self.session.project.tracks[i].instrument == Some(Instrument::Synth))
    }
}

pub fn strip(ws: &Workspace, ch: Channel, cx: &mut Context<Workspace>) -> Div {
    let project = &ws.session.project;
    let (name, color, pan, mute, solo, id) = match ch {
        Channel::Track(i) => {
            let t = &project.tracks[i];
            let color = track_color(t.instrument.is_some());
            (
                t.name.clone(),
                color,
                Some(t.pan),
                Some(t.mute),
                Some(t.solo),
                Some(t.id),
            )
        }
        Channel::Aux(i) => {
            let a = &project.auxes[i];
            (
                a.name.clone(),
                AUX,
                Some(a.pan),
                Some(a.mute),
                None,
                Some(a.id),
            )
        }
        Channel::Master => ("Stereo Out".to_string(), 0x6b6b72, None, None, None, None),
    };
    let volume = ws.volume(ch);
    let level = id.map_or(ws.master_level, |id| {
        ws.levels.get(&id).copied().unwrap_or(0.0)
    });
    let pos = fader_pos(volume);

    // Plugin rows: the instrument (or what feeds the strip), the inserts, and one empty slot.
    let mut slots = Vec::new();
    match ch {
        Channel::Track(i) => {
            let t = &project.tracks[i];
            slots.push(match &t.instrument {
                None => row("Audio In".into(), false),
                Some(Instrument::Synth) => {
                    plugin_slot("Sine Synth".into(), true, ch, Which::Instrument, cx)
                }
                Some(Instrument::Vst3(r)) => {
                    plugin_slot(r.class.clone(), true, ch, Which::Instrument, cx)
                }
            });
        }
        Channel::Aux(i) => slots.push(row(format!("Bus {}", project.auxes[i].bus), false)),
        Channel::Master => {}
    }
    let inserts = match ch {
        Channel::Track(i) => &project.tracks[i].inserts[..],
        Channel::Aux(i) => &project.auxes[i].inserts[..],
        Channel::Master => &[][..],
    };
    for (n, r) in inserts.iter().enumerate() {
        slots.push(plugin_slot(r.class.clone(), true, ch, Which::Insert(n), cx));
    }
    if ch != Channel::Master {
        slots.push(plugin_slot(
            String::new(),
            false,
            ch,
            Which::Insert(inserts.len()),
            cx,
        ));
    }
    slots.truncate(SLOT_ROWS);
    while slots.len() < SLOT_ROWS {
        slots.push(row(String::new(), false).opacity(0.0));
    }

    // Send rows, for tracks.
    let mut sends = Vec::new();
    if let Channel::Track(track) = ch {
        let list = &project.tracks[track].sends;
        for (index, s) in list.iter().enumerate().take(SEND_ROWS - 1) {
            let level = s.level_db;
            sends.push(
                row(format!("Bus {}  {}", s.bus, db_text(level)), true)
                    .debug_selector(move || format!("send-{track}-{index}"))
                    .cursor_ns_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.modifiers.alt {
                                ws.edit(cx, |ws| {
                                    ws.session.project.tracks[track].sends[index].level_db = 0.0
                                });
                            } else {
                                let y = f32::from(e.position.y);
                                ws.begin_drag(
                                    Drag::Send {
                                        track,
                                        index,
                                        y,
                                        level,
                                    },
                                    cx,
                                );
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| {
                                ws.session.project.tracks[track].sends.remove(index);
                            });
                        }),
                    ),
            );
        }
        sends.push(
            row("+ Send".into(), false)
                .debug_selector(move || format!("add-send-{track}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.menu = Some(ws.send_menu(track, e.position));
                        cx.notify();
                    }),
                ),
        );
    }
    while sends.len() < SEND_ROWS {
        sends.push(row(String::new(), false).opacity(0.0));
    }
    // Where it plays.
    let output = match ch {
        Channel::Track(track) => {
            let bus = project.tracks[track].output;
            let to = bus.and_then(|b| project.auxes.iter().find(|a| a.bus == b));
            let label = to.map_or("Stereo Out".to_string(), |a| a.name.clone());
            row(format!("→ {label}"), bus.is_some())
                .debug_selector(move || format!("output-{track}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.menu = Some(ws.output_menu(track, e.position));
                        cx.notify();
                    }),
                )
        }
        _ => row(String::new(), false).opacity(0.0),
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
                    ws.edit(cx, |ws| ws.set_volume(ch, 0.0));
                } else {
                    let y = f32::from(e.position.y);
                    ws.begin_drag(
                        Drag::Fader {
                            channel: ch,
                            y,
                            pos,
                        },
                        cx,
                    );
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

    let selected = matches!(ch, Channel::Track(i) if i == ws.track);
    div()
        .debug_selector(move || format!("strip-{ch:?}"))
        .w(px(STRIP_W))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .py_1()
        .rounded(px(4.))
        .bg(rgb(if selected { 0x36363c } else { PANEL }))
        .when_some(
            if let Channel::Track(i) = ch {
                Some(i)
            } else {
                None
            },
            |d, i| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.track = i;
                        cx.notify();
                    }),
                )
            },
        )
        .children(slots)
        .child(div().h(px(4.)))
        .children(sends)
        .child(output)
        .child(match pan {
            Some(pan) => div().cursor_ns_resize().child(knob(pan)).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if e.modifiers.alt {
                        ws.edit(cx, |ws| ws.set_pan(ch, 0.0));
                    } else {
                        let y = f32::from(e.position.y);
                        ws.begin_drag(
                            Drag::Pan {
                                channel: ch,
                                y,
                                pan,
                            },
                            cx,
                        );
                    }
                }),
            ),
            None => div().h(px(28.)),
        })
        .child(
            div()
                .font_family(MONO)
                .text_size(px(10.))
                .child(db_text(volume)),
        )
        .child(div().flex().gap_1().child(fader).child(meter))
        .child(
            div()
                .flex()
                .gap_1()
                .h(px(16.))
                .when_some(mute, |d, on| {
                    d.child(toggle("M", on, MUTE).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| match ch {
                                Channel::Track(i) => ws.session.project.tracks[i].mute ^= true,
                                Channel::Aux(i) => ws.session.project.auxes[i].mute ^= true,
                                Channel::Master => {}
                            });
                        }),
                    ))
                })
                .when_some(
                    solo.zip(if let Channel::Track(i) = ch {
                        Some(i)
                    } else {
                        None
                    }),
                    |d, (on, i)| {
                        d.child(toggle("S", on, SOLO).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.edit(cx, |ws| ws.session.project.tracks[i].solo ^= true);
                            }),
                        ))
                    },
                ),
        )
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
