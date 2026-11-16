//! The Drum Rack's chain list: a row a pad, lowest note first. Each row: its name (double-click
//! to rename), on, solo and hot-swap; with I/O, the note it receives, the note it plays, its
//! choke group, and a preview; its volume and pan; with sends, how much it sends to each of the
//! kit's returns. Under them, with returns shown, the return chains and a way to add one.

use gpui::Window;

use super::*;
use crate::mix::strip::{self, Channel};
use crate::workspace::{Name, pt};

const ROW_H: f32 = 20.0;
const NAME_W: f32 = 104.0;
const NOTE_W: f32 = 40.0;
const CHOKE_W: f32 = 34.0;
const VOL_W: f32 = 62.0;
const PAN_W: f32 = 32.0;
const SEND_W: f32 = 34.0;

/// A value in a cell of the list: text in a well, to click or drag.
fn cell(w: f32, text: impl Into<gpui::SharedString>, lit: bool) -> Div {
    div()
        .w(px(w))
        .h(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.))
        .bg(rgb(WELL))
        .font_family(MONO)
        .text_size(px(9.5))
        .text_color(rgb(if lit { TEXT } else { TEXT_3 }))
        .whitespace_nowrap()
        .overflow_hidden()
        .cursor_pointer()
        .hover(|d| d.bg(rgb(FIELD)))
        .child(text.into())
}

/// A column's name over its cells.
fn head(w: f32, label: &'static str) -> Div {
    div()
        .w(px(w))
        .flex_none()
        .text_size(px(9.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(TEXT_4))
        .text_center()
        .child(label)
}

impl Workspace {
    /// The kit's return chains: auxes its pads send to, by bus and name.
    pub(crate) fn kit_returns(&self, kit: usize) -> Vec<(u32, String)> {
        let id = self.session.project.tracks[kit].id;
        self.session
            .project
            .auxes
            .iter()
            .filter(|a| a.kit == Some(id))
            .map(|a| (a.bus, a.name.clone()))
            .collect()
    }

    pub(crate) fn chain_list(&mut self, kit: usize, cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        let views = self.drums.views;
        let returns = self.kit_returns(kit);
        let sends = if views.sends {
            returns.clone()
        } else {
            Vec::new()
        };
        let p = &self.session.project;
        let mut rows: Vec<(u8, usize)> = p
            .pads_of(kit)
            .into_iter()
            .filter_map(|t| p.tracks[t].pad.map(|pad| (pad.note, t)))
            .collect();
        rows.sort_unstable();
        let header = div()
            .h(px(18.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(3.))
            .px(px(6.))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .child(head(NAME_W + 3.0 + 3.0 * 17.0, "Chain").text_left())
            .when(views.io, |d| {
                d.child(head(NOTE_W, "Receive"))
                    .child(head(NOTE_W, "Play"))
                    .child(head(CHOKE_W, "Choke"))
                    .child(head(16.0, ""))
            })
            .child(head(VOL_W, "Volume"))
            .child(head(PAN_W, "Pan"))
            .children(sends.iter().enumerate().map(|(i, _)| {
                head(
                    SEND_W,
                    ["A", "B", "C", "D", "E", "F"]
                        .get(i)
                        .copied()
                        .unwrap_or("·"),
                )
            }));
        let list: Vec<Div> = rows
            .iter()
            .map(|&(note, t)| self.chain_row(kit, note, t, &sends, cx))
            .collect();
        let returns_view = views.returns.then(|| self.return_chains(kit, &returns, cx));
        let width = NAME_W
            + 3.0
            + 3.0 * 17.0
            + if views.io {
                2.0 * NOTE_W + CHOKE_W + 16.0 + 12.0
            } else {
                0.0
            }
            + VOL_W
            + PAN_W
            + sends.len() as f32 * (SEND_W + 3.0)
            + 30.0;
        // Every row, under the pads (the rack scrolls down), as wide as its columns; past the
        // rack's width it scrolls across, never cut off.
        div()
            .id("rack-chains")
            .flex_shrink(1.)
            .min_w(px(0.))
            .max_w(gpui::relative(1.))
            .overflow_x_scroll()
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .w(px(width))
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(div().py(px(2.)).children(list).children(returns_view)),
            )
    }

    /// One pad's row.
    fn chain_row(
        &mut self,
        kit: usize,
        note: u8,
        t: usize,
        sends: &[(u32, String)],
        cx: &mut Context<Self>,
    ) -> Div {
        let views = self.drums.views;
        let picked = self.drums.pad == note;
        let color = self.pad_color(kit, note);
        let renaming = self.name_field(Name::Track(t));
        let track = &self.session.project.tracks[t];
        let (name, muted, solo, vol, pan) = (
            track.name.clone(),
            track.mute,
            track.solo,
            track.volume_db,
            track.pan,
        );
        let pad = track
            .pad
            .unwrap_or(pina_engine::song::project::Pad::new(0, note, None));
        let levels: Vec<Option<f32>> = sends
            .iter()
            .map(|(bus, _)| {
                track
                    .sends
                    .iter()
                    .find(|s| s.bus == *bus)
                    .map(|s| s.level_db)
            })
            .collect();
        let name_cell = div()
            .w(px(NAME_W))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(
                div()
                    .flex_none()
                    .w(px(3.))
                    .h(px(12.))
                    .rounded(px(1.))
                    .bg(rgb(color)),
            )
            .child(match renaming {
                Some(field) => div().flex_1().min_w(px(0.)).child(field),
                None => div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(11.))
                    .text_color(rgb(if picked { TEXT } else { TEXT_2 }))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(name),
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    if e.click_count == 2 {
                        cx.stop_propagation();
                        ws.start_naming(Name::Track(t), window, cx);
                    }
                }),
            );
        let edit_track = move |what: &'static str| {
            move |ws: &mut Workspace,
                  _: &MouseDownEvent,
                  _: &mut Window,
                  cx: &mut Context<Workspace>| {
                cx.stop_propagation();
                ws.edit(cx, |ws| {
                    let track = &mut ws.session.project.tracks[t];
                    match what {
                        "on" => track.mute ^= true,
                        _ => track.solo ^= true,
                    }
                });
            }
        };
        let activator = div()
            .size(px(16.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(3.))
            .cursor_pointer()
            .map(|d| match muted {
                false => d.bg(alpha(AMBER, 0.85)),
                true => d.bg(rgb(FIELD)),
            })
            .child(icon(Icon::Speaker, 10.).text_color(rgb(if muted { TEXT_4 } else { BG })))
            .debug_selector(move || format!("chain-{note}-on"))
            .on_mouse_down(MouseButton::Left, cx.listener(edit_track("on")));
        let solo_b = rack_mini("S", solo, SOLO)
            .debug_selector(move || format!("chain-{note}-solo"))
            .on_mouse_down(MouseButton::Left, cx.listener(edit_track("solo")));
        let hot = rack_mini_icon(Icon::Shuffle, self.drums.tab == RackTab::Swap && picked)
            .debug_selector(move || format!("chain-{note}-swap"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.pick_pad(note);
                    ws.drums.tab = RackTab::Swap;
                    ws.drums.swap_rack = false;
                    cx.notify();
                }),
            );
        let io = views.io.then(|| {
            let receive = cell(NOTE_W, note_name(note), true)
                .debug_selector(move || format!("chain-{note}-receive"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.pad_note_menu(kit, note, e.position, cx);
                    }),
                );
            let play = cell(
                NOTE_W,
                note_name(pad.play.unwrap_or(note)),
                pad.play.is_some(),
            )
            .debug_selector(move || format!("chain-{note}-play"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.play_note_menu(note, e.position, cx);
                }),
            );
            let choke = cell(
                CHOKE_W,
                pad.choke.map_or("—".to_string(), |g| g.to_string()),
                pad.choke.is_some(),
            )
            .debug_selector(move || format!("chain-{note}-choke"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let items = std::iter::once((None, "None".to_string()))
                        .chain((1..=16u8).map(|g| (Some(g), g.to_string())))
                        .map(|(group, label)| {
                            (label, MenuAction::Drums(DrumOp::Choke { note, group }))
                        })
                        .collect();
                    ws.menu = Some(Menu {
                        at: e.position,
                        items,
                        ..Default::default()
                    });
                    cx.notify();
                }),
            );
            let preview = rack_mini("▶", false, TEXT_2)
                .debug_selector(move || format!("chain-{note}-preview"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.hit_pad(kit, note, 100);
                        cx.notify();
                    }),
                );
            div()
                .flex()
                .items_center()
                .gap(px(3.))
                .child(receive)
                .child(play)
                .child(choke)
                .child(preview)
        });
        let fill = strip::fader_pos(vol);
        let volume = div()
            .relative()
            .w(px(VOL_W))
            .h(px(16.))
            .flex_none()
            .rounded(px(3.))
            .bg(rgb(WELL))
            .overflow_hidden()
            .cursor_ew_resize()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(VOL_W * fill))
                    .bg(alpha(color, if muted { 0.25 } else { 0.55 })),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(MONO)
                    .text_size(px(9.5))
                    .text_color(rgb(TEXT))
                    .child(match vol {
                        v if v <= strip::SILENT => "-inf".to_string(),
                        v => format!("{v:.1} dB"),
                    }),
            )
            .debug_selector(move || format!("chain-{note}-volume"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if e.modifiers.alt {
                        ws.edit(cx, |ws| ws.set_volume(Channel::Track(t), 0.0));
                        return;
                    }
                    let pos = strip::fader_pos(ws.session.project.tracks[t].volume_db);
                    ws.begin_drag(
                        Drag::HFader {
                            channel: Channel::Track(t),
                            x: f32::from(e.position.x),
                            pos,
                            width: VOL_W,
                        },
                        cx,
                    );
                }),
            );
        let pan_cell = cell(
            PAN_W,
            match pan {
                p if p.abs() < 0.01 => "C".to_string(),
                p if p < 0.0 => format!("{:.0}L", -p * 50.0),
                p => format!("{:.0}R", p * 50.0),
            },
            pan.abs() >= 0.01,
        )
        .cursor_ew_resize()
        .debug_selector(move || format!("chain-{note}-pan"))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                if e.modifiers.alt {
                    ws.edit(cx, |ws| ws.set_pan(Channel::Track(t), 0.0));
                    return;
                }
                let at = pt(e.position);
                ws.begin_drag(
                    Drag::Pan {
                        channel: Channel::Track(t),
                        x: at.x,
                        y: at.y,
                        pan: ws.session.project.tracks[t].pan,
                    },
                    cx,
                );
            }),
        );
        let send_cells: Vec<Div> = sends
            .iter()
            .zip(levels)
            .map(|(&(bus, _), level)| {
                cell(
                    SEND_W,
                    match level {
                        Some(l) if l > strip::SILENT => format!("{l:.0}"),
                        _ => "-inf".to_string(),
                    },
                    level.is_some_and(|l| l > strip::SILENT),
                )
                .cursor_ns_resize()
                .debug_selector(move || format!("chain-{note}-send-{bus}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let index = ws.pad_send(t, bus, cx);
                        let level = ws.session.project.tracks[t].sends[index].level_db;
                        ws.begin_drag(
                            Drag::Send {
                                track: t,
                                index,
                                y: f32::from(e.position.y),
                                level,
                            },
                            cx,
                        );
                    }),
                )
            })
            .collect();
        div()
            .h(px(ROW_H))
            .flex()
            .items_center()
            .gap(px(3.))
            .px(px(6.))
            .when(picked, |d| d.bg(alpha(ACCENT, 0.14)))
            .hover(|d| d.bg(alpha(0xffffff, 0.03)))
            .debug_selector(move || format!("chain-{note}"))
            .child(name_cell)
            .child(activator)
            .child(solo_b)
            .child(hot)
            .children(io.map(|d| d.ml(px(6.))))
            .child(volume.ml(px(6.)))
            .child(pan_cell)
            .children(send_cells)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.pick_pad(note);
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.pad_menu(note, t, e.position, cx);
                }),
            )
    }

    /// The index of a pad's send to a bus: made (silent) if it hasn't one.
    fn pad_send(&mut self, t: usize, bus: u32, cx: &mut Context<Self>) -> usize {
        if let Some(i) = self.session.project.tracks[t]
            .sends
            .iter()
            .position(|s| s.bus == bus)
        {
            return i;
        }
        self.edit(cx, |ws| {
            ws.session.project.tracks[t]
                .sends
                .push(pina_engine::song::project::AuxSend {
                    bus,
                    level_db: strip::SILENT,
                    pre: false,
                });
        });
        self.session.project.tracks[t].sends.len() - 1
    }

    /// The kit's return chains: each named, its volume; and one more, added.
    fn return_chains(
        &mut self,
        kit: usize,
        returns: &[(u32, String)],
        cx: &mut Context<Self>,
    ) -> Div {
        let rows: Vec<Div> = returns
            .iter()
            .enumerate()
            .map(|(i, (bus, name))| {
                let bus = *bus;
                let a = self.session.project.auxes.iter().position(|a| a.bus == bus);
                let vol = a.map_or(0.0, |a| self.session.project.auxes[a].volume_db);
                div()
                    .h(px(ROW_H))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .px(px(6.))
                    .child(
                        div()
                            .w(px(14.))
                            .text_size(px(9.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(SKY))
                            .child(
                                ["A", "B", "C", "D", "E", "F"]
                                    .get(i)
                                    .copied()
                                    .unwrap_or("·"),
                            ),
                    )
                    .child(
                        div()
                            .w(px(NAME_W))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_2))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name.clone()),
                    )
                    .child(cell(56.0, format!("{vol:.1} dB"), true))
                    .child(
                        div()
                            .text_size(px(9.5))
                            .text_color(rgb(TEXT_4))
                            .child("Audio To Master"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            // In the mixer, its effects in the side panel.
                            if let Some(a) = a {
                                ws.channel = Channel::Aux(a);
                                ws.view = View::Mixer;
                                cx.notify();
                            }
                        }),
                    )
            })
            .collect();
        let add = (returns.len() < 6).then(|| {
            div()
                .h(px(ROW_H))
                .mx(px(6.))
                .flex()
                .items_center()
                .gap(px(5.))
                .text_size(px(10.5))
                .text_color(rgb(TEXT_3))
                .cursor_pointer()
                .hover(|d| d.text_color(rgb(TEXT)))
                .child(icon(Icon::Plus, 10.))
                .child("Return chain")
                .debug_selector(|| "rack-add-return".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.add_kit_return(kit, cx);
                    }),
                )
        });
        div()
            .mt(px(4.))
            .pt(px(2.))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .h(px(16.))
                    .px(px(6.))
                    .text_size(px(9.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(TEXT_4))
                    .child("RETURNS"),
            )
            .children(rows)
            .children(add)
    }

    /// A return chain for the kit: an aux its pads can send to.
    fn add_kit_return(&mut self, kit: usize, cx: &mut Context<Self>) {
        let id = self.session.project.tracks[kit].id;
        let n = self.kit_returns(kit).len();
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            let bus = p.add_aux();
            if let Some(a) = p.auxes.iter_mut().find(|a| a.bus == bus) {
                a.kit = Some(id);
                a.name = format!("Return {}", ["A", "B", "C", "D", "E", "F"][n.min(5)]);
            }
        });
        self.drums.views.sends = true;
    }

    /// A menu of the notes a pad's sampler can be played: its own (none moved), or another.
    pub(crate) fn play_note_menu(
        &mut self,
        note: u8,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let items: Vec<(String, MenuAction)> = (0..128u8)
            .rev()
            .map(|n| {
                let label = match n == note {
                    true => format!("{}  ·  its own", note_name(n)),
                    false => note_name(n),
                };
                let play = (n != note).then_some(n);
                (label, MenuAction::Drums(DrumOp::Play { note, play }))
            })
            .collect();
        self.menu = Some(Menu {
            at,
            items,
            searchable: true,
            ..Default::default()
        });
        cx.notify();
    }
}

/// A small switch in a chain's row.
fn rack_mini(label: &'static str, on: bool, lit: u32) -> Div {
    div()
        .size(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.))
        .text_size(px(9.))
        .font_weight(FontWeight::BOLD)
        .cursor_pointer()
        .map(|d| match on {
            true => d.bg(rgb(lit)).text_color(rgb(BG)),
            false => d
                .bg(rgb(FIELD))
                .text_color(rgb(TEXT_3))
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT))),
        })
        .child(label)
}

fn rack_mini_icon(i: Icon, on: bool) -> Div {
    div()
        .size(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.))
        .cursor_pointer()
        .map(|d| match on {
            true => d.bg(alpha(ACCENT, 0.3)),
            false => d.bg(rgb(FIELD)).hover(|d| d.bg(rgb(RAISED))),
        })
        .child(icon(i, 9.).text_color(rgb(if on { ACCENT_TEXT } else { TEXT_3 })))
}
