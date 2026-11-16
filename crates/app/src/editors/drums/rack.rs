//! The Drum Rack, down the left: its title (every pad's sounds swapped, those locked kept; a
//! kit hot-swapped in; its options), its pads (sixteen of the 128 notes beside an overview of
//! them all, sized to the pane's height within limits), and the picked pad's strip under them.
//! Beside it, its panel: the picked pad's Drum Sampler, its effects, the chain list, the macros
//! and their variations, or hot-swap, one at a time by its tabs.

use gpui::MouseMoveEvent;
use pina_engine::dsp::modulate::{MAX_MODS, ModKind};

use super::*;
use crate::editors::modulation::ModField;
use crate::workspace::Name;

/// The gap between pads; the overview's cells' width and gap (4 across, 32 down).
const PAD_GAP: f32 = 4.0;
const CELL_W: f32 = 8.0;
const CELL_GAP: f32 = 1.0;
/// The picked pad's strip under the pads.
const STRIP_H: f32 = 52.0;

/// A little square button on a pad or a chain: its glyph, lit when on.
pub(crate) fn mini(label: &'static str, on: bool, lit: u32, size: f32) -> Div {
    div()
        .w(px(size))
        .h(px(size - 1.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.))
        .text_size(px(size * 0.62))
        .font_weight(FontWeight::BOLD)
        .cursor_pointer()
        .map(|d| match on {
            true => d.bg(rgb(lit)).text_color(rgb(BG)),
            false => d
                .bg(alpha(0x000000, 0.28))
                .text_color(rgb(TEXT_3))
                .hover(|d| d.bg(alpha(0x000000, 0.5)).text_color(rgb(TEXT))),
        })
        .child(label)
}

/// A change to a kit's macro variations, given the macros' values now.
type VariationEdit<'a> = dyn Fn(&mut Vec<Vec<f32>>, &[f32]) + 'a;

/// A value from 0 to 1 for a number, spread evenly: the same number, the same value.
fn scatter(n: u64) -> f32 {
    let mut x = n ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

impl Workspace {
    /// The pads' size: as tall as the pane lets four rows and the picked pad's strip be,
    /// between 52 and 84, a little wider than tall.
    fn fit_pads(&mut self) -> (f32, f32) {
        let body = self.editor_height() - EDITOR_BAR_H;
        // The pane's padding, the rack's title and border, its own padding, the strip.
        let room = body - 16.0 - TITLE_H - 2.0 - 20.0 - STRIP_H - 8.0;
        let h = ((room - 3.0 * PAD_GAP) / 4.0).clamp(52.0, 84.0).floor();
        let w = (h * 1.08).clamp(64.0, 92.0).floor();
        self.drums.pad_size = (w, h);
        (w, h)
    }

    pub(crate) fn drum_rack(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let (pw, _) = self.fit_pads();
        let p = &self.session.project;
        let locked = p
            .pads_of(kit)
            .into_iter()
            .filter(|&t| p.tracks[t].pad.is_some_and(|p| p.lock))
            .count();
        let step = |label: &'static str, i: Icon, by: i32, cx: &mut Context<Self>| {
            icon_seg(i, false, 20.)
                .debug_selector(move || format!("drums-kit-{label}"))
                .help(
                    match by {
                        1 => "Swap every pad to the next sound like it (locked pads stay)",
                        _ => "Swap every pad back to the sound before (locked pads stay)",
                    },
                    cx,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.step_kit(by, cx);
                    }),
                )
        };
        let views = self.drums.views;
        let buttons = div()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .mr(px(2.))
                    .whitespace_nowrap()
                    .child(match locked {
                        0 => "Swap all".to_string(),
                        n => format!("Swap all · {n} locked"),
                    }),
            )
            .child(step("back", Icon::Back, -1, cx))
            .child(step("next", Icon::Forward, 1, cx))
            .child(div().w(px(1.)).h(px(14.)).bg(rgb(LINE)).mx(px(3.)))
            .child(
                icon_seg(Icon::Shuffle, false, 20.)
                    .debug_selector(|| "drums-rack-swap".into())
                    .help(
                        "Hot-swap the kit: a new one fitted to the song, or a folder's",
                        cx,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drums.tab = RackTab::Swap;
                            ws.drums.swap_rack = true;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                icon_seg(Icon::More, false, 20.)
                    .debug_selector(|| "drums-rack-options".into())
                    .help("The rack's options", cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let check = |on: bool, label: &str| match on {
                                true => format!("✓ {label}"),
                                false => label.to_string(),
                            };
                            let items = vec![
                                (
                                    check(views.auto, "Auto Select"),
                                    MenuAction::Drums(DrumOp::Option(RackOption::Auto)),
                                ),
                                (
                                    check(views.swaps, "Swap Buttons on Pads"),
                                    MenuAction::Drums(DrumOp::Option(RackOption::Swaps)),
                                ),
                            ];
                            ws.menu = Some(Menu {
                                at: e.position,
                                items,
                                ..Default::default()
                            });
                            cx.notify();
                        }),
                    ),
            );
        let grid_w = 4.0 * pw + 3.0 * PAD_GAP;
        let body = div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .child(self.pad_view(kit, cx))
            .child(self.picked_strip(kit, cx));
        device(device_name("Drum Rack", CORAL), buttons, body)
            .w(px(grid_w + 4.0 * (CELL_W + CELL_GAP) + 10.0 + 22.0))
            .h_full()
            .debug_selector(|| "drum-rack".into())
    }

    /// The picked pad's strip: its colour and name (double-click to rename); the note it
    /// receives and its choke group; mute, solo and lock; the sounds like it either side. On an
    /// empty pad: its note, and what the drum map has there.
    fn picked_strip(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let note = self.drums.pad;
        let strip = div()
            .h(px(STRIP_H))
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(4.))
            .px(px(8.))
            .rounded(px(6.))
            .bg(rgb(WELL))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .debug_selector(|| "rack-picked".into());
        let Some(t) = self.session.project.pad_on(kit, note) else {
            return strip
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_2))
                        .child(match gm_name(note) {
                            Some(n) => format!("{} · {n}", note_name(note)),
                            None => note_name(note),
                        }),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child("Empty: drop a sample on it, or pick one in Hot-swap"),
                );
        };
        let color = self.pad_color(kit, note);
        let renaming = self.name_field(Name::Track(t));
        let track = &self.session.project.tracks[t];
        let (name, muted, solo) = (track.name.clone(), track.mute, track.solo);
        let pad = track
            .pad
            .unwrap_or(pina_engine::song::project::Pad::new(0, note, None));
        let small = |label: String, sel: &'static str| {
            dropdown(label, 20.)
                .font_family(MONO)
                .debug_selector(move || sel.into())
        };
        let edit_track = move |what: &'static str| {
            move |ws: &mut Workspace,
                  _: &MouseDownEvent,
                  _: &mut gpui::Window,
                  cx: &mut Context<Workspace>| {
                cx.stop_propagation();
                ws.edit(cx, |ws| {
                    let track = &mut ws.session.project.tracks[t];
                    match what {
                        "M" => track.mute ^= true,
                        "S" => track.solo ^= true,
                        _ => {
                            if let Some(p) = &mut track.pad {
                                p.lock ^= true;
                            }
                        }
                    }
                });
            }
        };
        let top = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .flex_none()
                    .size(px(8.))
                    .rounded(px(2.))
                    .bg(rgb(color)),
            )
            .child(match renaming {
                Some(field) => div().flex_1().min_w(px(0.)).child(field),
                None => div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(name)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                            if e.click_count == 2 {
                                cx.stop_propagation();
                                ws.start_naming(Name::Track(t), window, cx);
                            }
                        }),
                    ),
            })
            .child(
                mini("M", muted, TEXT_2, 18.)
                    .debug_selector(|| "rack-picked-m".into())
                    .on_mouse_down(MouseButton::Left, cx.listener(edit_track("M"))),
            )
            .child(
                mini("S", solo, SOLO, 18.)
                    .debug_selector(|| "rack-picked-s".into())
                    .on_mouse_down(MouseButton::Left, cx.listener(edit_track("S"))),
            )
            .child(
                div()
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.))
                    .cursor_pointer()
                    .map(|d| match pad.lock {
                        true => d.bg(alpha(0xffffff, 0.85)),
                        false => d
                            .bg(alpha(0x000000, 0.28))
                            .hover(|d| d.bg(alpha(0x000000, 0.5))),
                    })
                    .child(
                        icon(if pad.lock { Icon::Lock } else { Icon::Unlock }, 10.)
                            .text_color(rgb(if pad.lock { BG } else { TEXT_3 })),
                    )
                    .debug_selector(|| "rack-picked-lock".into())
                    .on_mouse_down(MouseButton::Left, cx.listener(edit_track("L"))),
            );
        let bottom = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_3))
                    .child("Note"),
            )
            .child(small(note_name(note), "rack-picked-note").on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.pad_note_menu(kit, note, e.position, cx);
                }),
            ))
            .child(
                div()
                    .ml(px(4.))
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_3))
                    .child("Choke"),
            )
            .child(
                small(
                    pad.choke.map_or("None".into(), |g| g.to_string()),
                    "rack-picked-choke",
                )
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
                ),
            )
            .child(div().flex_1())
            .child(
                icon_seg(Icon::Back, false, 20.)
                    .debug_selector(|| "rack-picked-back".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.step_pad(note, -1, cx);
                        }),
                    ),
            )
            .child(
                icon_seg(Icon::Forward, false, 20.)
                    .debug_selector(|| "rack-picked-next".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.step_pad(note, 1, cx);
                        }),
                    ),
            );
        strip.child(top).child(bottom)
    }

    /// The rack's panel: its tabs, and what the picked one shows (scrolling down if it's taller
    /// than the pane).
    pub(crate) fn rack_panel(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let tab = self.drums.tab;
        let note = self.drums.pad;
        let pad_t = self.session.project.pad_on(kit, note);
        let effects = pad_t.map_or(0, |t| {
            self.shown_effects(crate::mix::strip::Channel::Track(t))
                .len()
        });
        let chains = self.session.project.pads_of(kit).len();
        let tabs = tray().flex_wrap().p(px(2.)).rounded(px(7.)).children(
            [
                (RackTab::Sampler, "Sampler".to_string(), "rack-tab-sampler"),
                (
                    RackTab::Effects,
                    match effects {
                        0 => "Effects".to_string(),
                        n => format!("Effects {n}"),
                    },
                    "rack-tab-effects",
                ),
                (
                    RackTab::Chains,
                    format!("Chains {chains}"),
                    "rack-tab-chains",
                ),
                (RackTab::Macros, "Macros".to_string(), "rack-tab-macros"),
                (RackTab::Swap, "Hot-swap".to_string(), "rack-tab-swap"),
            ]
            .map(|(t, label, sel)| {
                seg(label, tab == t, 22.)
                    .px(px(6.))
                    .debug_selector(move || sel.into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drums.tab = t;
                            if t == RackTab::Swap {
                                ws.drums.swap_rack = false;
                            }
                            cx.notify();
                        }),
                    )
            }),
        );
        // What goes on the right of the tabs: the picked pad's layers and its full Sampler; the
        // chain list's columns; what hot-swap swaps.
        let views = self.drums.views;
        let side: Div = match tab {
            RackTab::Sampler => div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(self.layer_tabs(kit, cx))
                .children(pad_t.map(|t| {
                    icon_seg(Icon::Expand, false, 22.)
                        .debug_selector(|| "pad-sampler".into())
                        .help("Open the pad in the full Sampler: every setting it has", cx)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.select_track(t);
                                ws.show_bottom(Bottom::Sampler, cx);
                            }),
                        )
                })),
            RackTab::Chains => div().flex().items_center().gap(px(2.)).children(
                [
                    (RackOption::Io, "I/O", views.io),
                    (RackOption::Sends, "Sends", views.sends),
                    (RackOption::Returns, "Returns", views.returns),
                ]
                .map(|(o, label, on)| {
                    seg(label, on, 22.)
                        .debug_selector(move || format!("rack-option-{}", label.to_lowercase()))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.drum_op(DrumOp::Option(o), cx);
                            }),
                        )
                }),
            ),
            RackTab::Swap => {
                let rack = self.drums.swap_rack;
                tray().p(px(2.)).rounded(px(7.)).children(
                    [("The pad", false), ("The kit", true)].map(|(label, k)| {
                        seg(label, rack == k, 22.)
                            .debug_selector(move || format!("swap-target-{k}"))
                            .help("D switches between the pad and the kit", cx)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.drums.swap_rack = k;
                                    cx.notify();
                                }),
                            )
                    }),
                )
            }
            _ => div(),
        };
        let body: Div = match tab {
            RackTab::Sampler => self.sampler_view(kit, cx).p(px(10.)),
            RackTab::Effects => match pad_t {
                Some(t) => self.effects_view(t, cx),
                None => div()
                    .p(px(16.))
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .child("Pick a pad with a sound to see its effects."),
            },
            RackTab::Chains => div().child(self.chain_list(kit, cx)),
            RackTab::Macros => div()
                .size_full()
                .flex()
                .child(self.rack_macros(kit, cx))
                .child(self.rack_variations(kit, cx)),
            RackTab::Swap => match self.drums.swap_rack {
                true => self.kit_swap(kit, cx),
                false => self.pad_sounds(kit, cx),
            }
            .w_full()
            .h_full()
            .border_0()
            .bg(rgb(SURFACE)),
        };
        // Hot-swap and the macros fill the panel (the list scrolls itself); the rest scroll down
        // when they're taller.
        let fills = matches!(tab, RackTab::Swap | RackTab::Macros);
        let content = match fills {
            true => div()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .child(body.flex_1().min_h(px(0.))),
            false => div()
                .relative()
                .flex_1()
                .min_h(px(0.))
                .child(
                    div()
                        .id("rack-panel-body")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.drums.devices_scroll)
                        .child(body.pr(px(12.))),
                )
                .child(scroll_bar(
                    self.drums.devices_scroll.clone(),
                    "rack-panel-bar",
                    cx,
                )),
        };
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(LINE))
            .overflow_hidden()
            .debug_selector(|| "rack-panel".into())
            // Narrow, what's on the right of the tabs goes under them: nothing cut off.
            .child(
                div()
                    .min_h(px(TITLE_H + 6.0))
                    .flex_none()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(4.))
                    .px(px(6.))
                    .py(px(1.))
                    .bg(rgb(HEADERS))
                    .border_b_1()
                    .border_color(rgb(LINE_SOFT))
                    .child(tabs)
                    .child(div().flex_1())
                    .child(side),
            )
            .child(content)
    }

    /// The kit's macros: knobs it shares with every pad (Map, then turn a pad's knob to set how
    /// far one moves it), as many as asked for, randomised at once.
    fn rack_macros(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let p = &self.session.project;
        // The kit track's modulators that are the stack's macros, by the stack's index.
        let macros: Vec<(usize, usize, f32)> = p.tracks[kit]
            .modulators
            .iter()
            .enumerate()
            .filter_map(|(k, m)| match m.kind {
                ModKind::Shared { modulator } => match p.mod_kind(kit, k) {
                    Some(ModKind::Macro { value }) => Some((k, modulator as usize, *value)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let count = self.drums.macros.clamp(1, 16);
        let mapping = self.mod_edit;
        let knob = |i: usize, ws: &Self, cx: &mut Context<Self>| {
            let found = macros.get(i).copied();
            let value = found.map_or(0.0, |m| m.2);
            let mapped = found.is_some_and(|(_, s, _)| ws.macro_mapping(kit) == Some(s));
            div()
                .w(px(76.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .py(px(6.))
                .rounded(px(6.))
                .when(mapped, |d| d.bg(alpha(ACCENT, 0.2)))
                .cursor_ns_resize()
                .debug_selector(move || format!("rack-macro-{i}"))
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .child(format!("Macro {}", i + 1)),
                )
                .child(crate::mix::eq::dial(
                    value,
                    40.,
                    4.5,
                    if found.is_some() { CORAL } else { TEXT_4 },
                    false,
                ))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_2))
                        .child(format!("{:.0}", value * 127.0)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let k = match found {
                            Some((k, _, _)) => k,
                            None => match ws.add_macro(kit, cx) {
                                Some(k) => k,
                                None => return,
                            },
                        };
                        if ws.drums.mapping {
                            ws.map_macro(kit, k, cx);
                            return;
                        }
                        ws.begin_drag(
                            Drag::ModValue {
                                track: kit,
                                index: k,
                                field: ModField::Value,
                                y: crate::workspace::pt(e.position).y,
                                value,
                            },
                            cx,
                        );
                    }),
                )
        };
        // Two rows: four across for up to eight, eight across past that.
        let across = if count <= 8 { 4 } else { 8 };
        let mut knobs: Vec<Div> = (0..count).map(|i| knob(i, self, cx)).collect();
        let mut rows = Vec::new();
        while !knobs.is_empty() {
            let rest = knobs.split_off(across.min(knobs.len()));
            rows.push(div().flex().gap(px(6.)).children(knobs));
            knobs = rest;
        }
        let map_on = self.drums.mapping;
        let head = div()
            .flex()
            .items_center()
            .gap(px(3.))
            .child(
                seg("Map", map_on, 18.)
                    .debug_selector(|| "rack-map".into())
                    .help(
                        "Map: pick a macro, then turn a knob on the pad's devices to set how far the macro moves it",
                        cx,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drums.mapping ^= true;
                            if !ws.drums.mapping {
                                ws.mod_edit = None;
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                seg("Rand", false, 18.)
                    .debug_selector(|| "rack-rand".into())
                    .help("Every macro to a value of its own, at random", cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.randomize_macros(kit, cx);
                        }),
                    ),
            )
            .child(div().flex_1())
            .child(
                icon_seg(Icon::Minus, false, 18.)
                    .help("Fewer macros", cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drums.macros = ws.drums.macros.saturating_sub(1).max(1);
                            cx.notify();
                        }),
                    ),
            )
            .child(
                icon_seg(Icon::Plus, false, 18.)
                    .help("More macros (up to 16)", cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drums.macros = (ws.drums.macros + 1).min(16);
                            cx.notify();
                        }),
                    ),
            );
        let _ = mapping;
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .child(head)
            .child(
                div()
                    .id("rack-macros")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(10.))
                    .children(rows),
            )
    }

    /// The macros' values now, in their order on the kit's stack.
    fn macro_values(&self, kit: usize) -> Vec<f32> {
        let p = &self.session.project;
        let bus = p.tracks[kit].output;
        p.auxes
            .iter()
            .find(|a| Some(a.bus) == bus && a.stack)
            .map(|a| {
                a.modulators
                    .iter()
                    .filter_map(|m| match m.kind {
                        ModKind::Macro { value } => Some(value),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The kit's macro variations: New keeps the macros' values; click one to bring them back,
    /// ↻ to keep the values now in it instead, × to take it out.
    fn rack_variations(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let p = &self.session.project;
        let bus = p.tracks[kit].output;
        let kept: Vec<Vec<f32>> = p
            .auxes
            .iter()
            .find(|a| Some(a.bus) == bus && a.stack)
            .map(|a| a.variations.clone())
            .unwrap_or_default();
        let now = self.macro_values(kit);
        let edit_variations =
            move |ws: &mut Workspace, cx: &mut Context<Workspace>, f: &VariationEdit<'_>| {
                let now = ws.macro_values(kit);
                let bus = ws.session.project.tracks[kit].output;
                ws.edit(cx, |ws| {
                    if let Some(a) = ws
                        .session
                        .project
                        .auxes
                        .iter_mut()
                        .find(|a| Some(a.bus) == bus && a.stack)
                    {
                        f(&mut a.variations, &now);
                    }
                });
            };
        let rows: Vec<Div> = kept
            .iter()
            .enumerate()
            .map(|(i, values)| {
                let on = *values == now;
                let launch = values.clone();
                let group: gpui::SharedString = format!("variation-{i}").into();
                div()
                    .group(group.clone())
                    .h(px(20.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(alpha(CORAL, 0.18)))
                    .hover(|d| d.bg(rgb(FIELD)))
                    .debug_selector(move || format!("rack-variation-{i}"))
                    .child(icon(Icon::Play, 8.).text_color(rgb(if on { CORAL } else { TEXT_3 })))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_2))
                            .child(format!("Variation {}", i + 1)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(2.))
                            .opacity(0.)
                            .group_hover(group, |s| s.opacity(1.))
                            .child(
                                icon_seg(Icon::RotateLeft, false, 16.)
                                    .debug_selector(move || format!("rack-variation-{i}-keep"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            edit_variations(ws, cx, &|v, now| {
                                                if let Some(slot) = v.get_mut(i) {
                                                    *slot = now.to_vec();
                                                }
                                            });
                                        }),
                                    ),
                            )
                            .child(
                                icon_seg(Icon::Close, false, 16.)
                                    .debug_selector(move || format!("rack-variation-{i}-delete"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            edit_variations(ws, cx, &|v, _| {
                                                if i < v.len() {
                                                    v.remove(i);
                                                }
                                            });
                                        }),
                                    ),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.launch_variation(kit, &launch, cx);
                        }),
                    )
            })
            .collect();
        div()
            .w(px(220.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(3.))
            .p(px(12.))
            .border_l_1()
            .border_color(rgb(LINE_SOFT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(9.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(TEXT_3))
                            .child("VARIATIONS"),
                    )
                    .child(
                        seg("New", false, 18.)
                            .debug_selector(|| "rack-variation-new".into())
                            .help("Keep the macros' values now as a variation", cx)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    edit_variations(ws, cx, &|v, now| v.push(now.to_vec()));
                                }),
                            ),
                    ),
            )
            .child(
                div()
                    .id("rack-variations")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .children(rows)
                    .when(kept.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(TEXT_4))
                                .child("Set the macros, then New."),
                        )
                    }),
            )
    }

    /// A variation's values back on the kit's macros, in their order.
    fn launch_variation(&mut self, kit: usize, values: &[f32], cx: &mut Context<Self>) {
        let bus = self.session.project.tracks[kit].output;
        self.edit(cx, |ws| {
            if let Some(a) = ws
                .session
                .project
                .auxes
                .iter_mut()
                .find(|a| Some(a.bus) == bus && a.stack)
            {
                let macros = a.modulators.iter_mut().filter_map(|m| match &mut m.kind {
                    ModKind::Macro { value } => Some(value),
                    _ => None,
                });
                for (v, &to) in macros.zip(values) {
                    *v = to;
                }
            }
        });
    }

    /// The stack's macro picked to map, if Map is on and one is: by the stack's index.
    fn macro_mapping(&self, kit: usize) -> Option<usize> {
        let (t, k) = self.mod_edit?;
        let p = &self.session.project;
        match p.tracks.get(t)?.modulators.get(k)?.kind {
            ModKind::Shared { modulator } if p.kit_of(t) == Some(kit) => Some(modulator as usize),
            _ => None,
        }
    }

    /// A new macro on the kit: a modulator the stack shares. Returns the kit track's.
    fn add_macro(&mut self, kit: usize, cx: &mut Context<Self>) -> Option<usize> {
        let bus = self.session.project.tracks[kit].output?;
        let mut k = None;
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            if let Some(s) = p.add_shared(bus, ModKind::Macro { value: 0.0 }) {
                k = p.tracks[kit]
                    .modulators
                    .iter()
                    .position(|m| m.kind == ModKind::Shared { modulator: s as u8 });
            }
        });
        if k.is_none() {
            self.status = Some(format!(
                "A kit has at most {MAX_MODS} macros and modulators."
            ));
        }
        k
    }

    /// Map: the macro (the kit track's modulator `k`) picked; the picked pad's knobs then set
    /// how far it moves them.
    fn map_macro(&mut self, kit: usize, k: usize, cx: &mut Context<Self>) {
        let p = &self.session.project;
        let shared = p.tracks[kit].modulators.get(k).map(|m| m.kind.clone());
        let pad = p.pad_on(kit, self.drums.pad);
        let target = pad.and_then(|t| {
            p.tracks[t]
                .modulators
                .iter()
                .position(|m| Some(&m.kind) == shared.as_ref())
                .map(|pk| (t, pk))
        });
        self.mod_edit = target;
        self.status = Some(match target {
            Some(_) => {
                "Turn a knob on the pad's Drum Sampler: that's how far the macro moves it.".into()
            }
            None => "Pick a pad to map the macro to its knobs.".into(),
        });
        cx.notify();
    }

    /// Every macro of the kit to a value of its own at random.
    fn randomize_macros(&mut self, kit: usize, cx: &mut Context<Self>) {
        let Some(bus) = self.session.project.tracks[kit].output else {
            return;
        };
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64)
            ^ self.drums.frame;
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            if let Some(a) = p.auxes.iter_mut().find(|a| a.bus == bus && a.stack) {
                for (i, m) in a.modulators.iter_mut().enumerate() {
                    if let ModKind::Macro { value } = &mut m.kind {
                        let r = scatter(seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                        *value = (r * 100.0).round() / 100.0;
                    }
                }
            }
        });
    }

    /// The pads: the overview of all 128 notes (the sixteen shown framed; drag it), and the
    /// sixteen, the lowest at the bottom left.
    fn pad_view(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let view = self.drums.view;
        let (pw, ph) = self.drums.pad_size;
        let rows: Vec<Div> = (0..4u8)
            .rev()
            .map(|r| {
                div().flex().gap(px(PAD_GAP)).children((0..4u8).map(|c| {
                    let note = view + r * 4 + c;
                    match self.drum_pad(kit, note).map(|(t, _)| t) {
                        Some(t) => self.rack_pad(kit, note, t, (pw, ph), cx),
                        None => self.empty_pad_cell(kit, note, (pw, ph), cx),
                    }
                }))
            })
            .collect();
        div()
            .flex_none()
            .flex()
            .items_start()
            .gap(px(10.))
            .child(self.pad_overview(kit, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(PAD_GAP))
                    .debug_selector(|| "rack-pads".into())
                    .children(rows),
            )
    }

    /// Every note's pad, tiny: those with chains lit, the sixteen shown framed. Click or drag to
    /// show others.
    /// The overview's rows' height: its 32 as tall as the four rows of pads beside it.
    fn overview_cell_h(&self) -> f32 {
        let (_, ph) = self.drums.pad_size;
        ((4.0 * ph + 3.0 * PAD_GAP) - 31.0 * CELL_GAP) / 32.0
    }

    fn pad_overview(&mut self, kit: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let cell_h = self.overview_cell_h();
        let lit: Vec<(u8, u32)> = self
            .kit_notes(kit)
            .into_iter()
            .map(|n| (n, self.pad_color(kit, n)))
            .collect();
        let (view, picked) = (self.drums.view, self.drums.pad);
        let bounds = self.drums.overview.clone();
        let w = 4.0 * (CELL_W + CELL_GAP) - CELL_GAP;
        let h = 32.0 * (cell_h + CELL_GAP) - CELL_GAP;
        let drawn = canvas(
            move |b, _, _| bounds.set(Some(b)),
            move |b, _, window, _| {
                for note in 0..128u8 {
                    let (row, col) = ((note / 4) as f32, (note % 4) as f32);
                    let at = point(
                        b.origin.x + px(col * (CELL_W + CELL_GAP)),
                        b.origin.y + px((31.0 - row) * (cell_h + CELL_GAP)),
                    );
                    let color = match lit.iter().find(|l| l.0 == note) {
                        Some(&(_, c)) if note == picked => rgb(c).into(),
                        Some(&(_, c)) => alpha(c, 0.7),
                        None => rgb(FIELD).into(),
                    };
                    window.paint_quad(fill(Bounds::new(at, size(px(CELL_W), px(cell_h))), color));
                }
                // The sixteen shown, framed.
                let top = 31.0 - (view / 4 + 3) as f32;
                let frame = Bounds::new(
                    point(
                        b.origin.x - px(1.5),
                        b.origin.y + px(top * (cell_h + CELL_GAP) - 1.5),
                    ),
                    size(px(w + 3.0), px(4.0 * (cell_h + CELL_GAP) + 2.0)),
                );
                window.paint_quad(gpui::quad(
                    frame,
                    px(2.),
                    gpui::transparent_black(),
                    px(1.),
                    rgb(TEXT_2),
                    Default::default(),
                ));
            },
        )
        .w(px(w))
        .h(px(h));
        div()
            .flex_none()
            .pt(px(2.))
            .cursor_ns_resize()
            .debug_selector(|| "rack-overview".into())
            .help(
                "Every note's pad: drag to show others (↑ ↓ a bank, ⌥ a row)",
                cx,
            )
            .child(drawn)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.overview_at(f32::from(e.position.y), cx);
                }),
            )
            .on_mouse_move(cx.listener(|ws, e: &MouseMoveEvent, _, cx| {
                if e.pressed_button == Some(MouseButton::Left) {
                    ws.overview_at(f32::from(e.position.y), cx);
                }
            }))
    }

    /// The pads shown moved to the overview's row under `y` (it among the four).
    fn overview_at(&mut self, y: f32, cx: &mut Context<Self>) {
        let cell_h = self.overview_cell_h();
        let Some(b) = self.drums.overview.get() else {
            return;
        };
        let from_top = ((y - f32::from(b.origin.y)) / (cell_h + CELL_GAP)).floor() as i32;
        let row = (31 - from_top).clamp(0, 31);
        let first = (row - 1).clamp(0, 28) as u8;
        if self.drums.view != first * 4 {
            self.drums.view = first * 4;
            cx.notify();
        }
    }

    /// Where a pad was drawn, for dropping onto.
    fn place(&self, note: u8) -> impl IntoElement {
        let (cells, frame) = (self.drums.cells.clone(), self.drums.frame);
        canvas(
            move |b, _, _| {
                let mut c = cells.borrow_mut();
                if c.0 != frame {
                    *c = (frame, Vec::new());
                }
                c.1.push((note, b));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    /// What's being dragged over the pad on `note`: a pad from elsewhere, or a sample.
    fn drop_color(&self, note: u8) -> Option<u32> {
        match self.drag {
            Some(Drag::Pad {
                note: from,
                at,
                moved: true,
                ..
            }) => (from != note && self.pad_under(at) == Some(note)).then_some(TEXT),
            Some(Drag::Sample {
                at, moved: true, ..
            }) => (self.pad_under(at) == Some(note)).then_some(ACCENT),
            _ => None,
        }
    }

    /// A pad with no chain: its note, and what the drum map has there; a sample dropped on it
    /// makes one.
    fn empty_pad_cell(
        &mut self,
        _kit: usize,
        note: u8,
        (w, h): (f32, f32),
        cx: &mut Context<Self>,
    ) -> Div {
        let target = self.drop_color(note);
        let picked = self.drums.pad == note;
        div()
            .relative()
            .w(px(w))
            .h(px(h))
            .flex_none()
            .rounded(px(5.))
            .border_1()
            .border_color(match (target, picked) {
                (Some(c), _) => rgb(c).into(),
                (None, true) => alpha(TEXT_2, 0.7),
                _ => rgb(LINE_SOFT).into(),
            })
            .bg(match target {
                Some(c) => alpha(c, 0.16),
                None => rgb(WELL).into(),
            })
            .hover(|d| d.bg(rgb(OFF)))
            .cursor_pointer()
            .debug_selector(move || format!("pad-{note}"))
            .child(self.place(note))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(1.))
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(10.))
                            .text_color(rgb(TEXT_4))
                            .child(note_name(note)),
                    )
                    .children(gm_name(note).map(|n| {
                        div()
                            .px(px(3.))
                            .text_size(px(8.5))
                            .text_color(rgb(if picked { TEXT_3 } else { TEXT_4 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .max_w(px(w - 4.0))
                            .child(n)
                    })),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.drums.pad = note;
                    cx.notify();
                }),
            )
    }

    /// A pad with a chain: its name; mute, preview and solo; with the swap buttons shown, the
    /// sounds like it either side and its lock. Lit as it plays, flashed as it's struck; click
    /// it to hear it (higher, harder) and pick it, drag it onto another to trade.
    fn rack_pad(
        &mut self,
        kit: usize,
        note: u8,
        t: usize,
        (w, h): (f32, f32),
        cx: &mut Context<Self>,
    ) -> Div {
        let picked = self.drums.pad == note;
        let target = self.drop_color(note);
        let lifted = matches!(self.drag, Some(Drag::Pad { note: n, moved: true, .. }) if n == note);
        let track = &self.session.project.tracks[t];
        let (name, muted, solo, lock) = (
            track.name.clone(),
            track.mute,
            track.solo,
            track.pad.is_some_and(|p| p.lock),
        );
        let layers = match &track.instrument {
            Some(Instrument::Sampler(s)) => s.zones.len(),
            _ => 1,
        };
        let color = self.pad_color(kit, note);
        let level = self
            .levels
            .get(&Some(track.id))
            .map_or(0.0, |l| l.now[0].max(l.now[1]));
        let playing = ((20.0 * level.max(1e-5).log10() + 40.0) / 40.0).clamp(0.0, 1.0);
        let flash = self.drums.struck.get(&note).map_or(0.0, |(at, v)| {
            v * (1.0 - at.elapsed().as_secs_f32() / 0.25).max(0.0)
        });
        let glow = playing.max(flash);
        let swaps = self.drums.views.swaps;
        let group: gpui::SharedString = format!("rack-pad-{note}").into();
        let press = cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            ws.drums.pad = note;
            // Higher on the pad, harder.
            let velocity = match ws.drums.cells.borrow().1.iter().find(|c| c.0 == note) {
                Some((_, b)) => {
                    let y = (f32::from(e.position.y) - f32::from(b.origin.y))
                        / f32::from(b.size.height).max(1.0);
                    (127.0 - y.clamp(0.0, 1.0) * 87.0) as u8
                }
                None => 100,
            };
            ws.hit_pad(kit, note, velocity);
            let at = crate::workspace::pt(e.position);
            ws.begin_drag(
                Drag::Pad {
                    note,
                    from: at,
                    at,
                    moved: false,
                },
                cx,
            );
        });
        let edit_track = move |label: &'static str| {
            move |ws: &mut Workspace,
                  _: &MouseDownEvent,
                  _: &mut gpui::Window,
                  cx: &mut Context<Workspace>| {
                cx.stop_propagation();
                match label {
                    "P" => {
                        ws.drums.pad = note;
                        ws.hit_pad(kit, note, 100);
                        cx.notify();
                    }
                    _ => ws.edit(cx, |ws| {
                        let track = &mut ws.session.project.tracks[t];
                        match label {
                            "M" => track.mute ^= true,
                            _ => track.solo ^= true,
                        }
                    }),
                }
            }
        };
        let arrow = |i: Icon, by: i32, cx: &mut Context<Self>| {
            div()
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.))
                .bg(alpha(0x000000, 0.4))
                .hover(|d| d.bg(alpha(0x000000, 0.7)))
                .cursor_pointer()
                .child(icon(i, 9.).text_color(rgb(TEXT)))
                .debug_selector(move || format!("pad-{note}-swap-{by}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.drums.pad = note;
                        ws.step_pad(note, by, cx);
                    }),
                )
        };
        let lock_button =
            div()
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.))
                .cursor_pointer()
                .map(|d| match lock {
                    true => d.bg(alpha(0xffffff, 0.85)),
                    false => d
                        .bg(alpha(0x000000, 0.4))
                        .hover(|d| d.bg(alpha(0x000000, 0.7))),
                })
                .child(
                    icon(if lock { Icon::Lock } else { Icon::Unlock }, 8.5)
                        .text_color(rgb(if lock { BG } else { TEXT_2 })),
                )
                .debug_selector(move || format!("pad-{note}-lock"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.edit(cx, |ws| {
                            if let Some(p) = &mut ws.session.project.tracks[t].pad {
                                p.lock ^= true;
                            }
                        });
                    }),
                );
        div()
            .relative()
            .group(group.clone())
            .w(px(w))
            .h(px(h))
            .flex_none()
            .rounded(px(5.))
            .overflow_hidden()
            .cursor_pointer()
            .debug_selector(move || format!("pad-{note}"))
            .child(self.place(note))
            .bg(rgb(RAISED))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(alpha(color, 0.16 + 0.5 * glow)),
            )
            .border_1()
            .border_color(match (target, picked) {
                (Some(c), _) => rgb(c).into(),
                (None, true) => rgb(TEXT).into(),
                (None, false) => alpha(color, 0.35),
            })
            .when_some(target, |d, c| {
                d.child(div().absolute().inset_0().bg(alpha(c, 0.25)))
            })
            .when(muted || lifted, |d| d.opacity(0.5))
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .left(px(4.))
                    .right(px(4.))
                    .text_size(px(10.))
                    .line_height(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .max_h(px(24.))
                    .overflow_hidden()
                    .child(match layers {
                        1 => name,
                        _ => format!("{name} ×{layers}"),
                    }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(3.))
                    .bottom(px(3.))
                    .flex()
                    .gap(px(1.))
                    .child(
                        mini("M", muted, TEXT_2, 14.)
                            .debug_selector(move || format!("pad-{note}-m"))
                            .on_mouse_down(MouseButton::Left, cx.listener(edit_track("M"))),
                    )
                    .child(
                        mini("▶", false, TEXT_2, 14.)
                            .debug_selector(move || format!("pad-{note}-play"))
                            .on_mouse_down(MouseButton::Left, cx.listener(edit_track("P"))),
                    )
                    .child(
                        mini("S", solo, SOLO, 14.)
                            .debug_selector(move || format!("pad-{note}-s"))
                            .on_mouse_down(MouseButton::Left, cx.listener(edit_track("S"))),
                    ),
            )
            .when(swaps, |d| {
                d.child(
                    div()
                        .absolute()
                        .right(px(3.))
                        .bottom(px(3.))
                        .flex()
                        .gap(px(1.))
                        .child(
                            div()
                                .flex()
                                .gap(px(1.))
                                .opacity(0.)
                                .group_hover(group.clone(), |s| s.opacity(1.))
                                .child(arrow(Icon::Back, -1, cx))
                                .child(arrow(Icon::Forward, 1, cx)),
                        )
                        .child(
                            div()
                                .when(!lock, |d| {
                                    d.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.))
                                })
                                .child(lock_button),
                        ),
                )
            })
            .on_mouse_down(MouseButton::Left, press)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.pad_menu(note, t, e.position, cx);
                }),
            )
    }

    /// A pad's (or its chain's) menu: rename, duplicate, lock, extract, delete.
    pub(crate) fn pad_menu(
        &mut self,
        note: u8,
        t: usize,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.pick_pad(note);
        let locked = self.session.project.tracks[t].pad.is_some_and(|p| p.lock);
        let items = vec![
            (
                "Rename".to_string(),
                MenuAction::Rename(crate::workspace::Name::Track(t)),
            ),
            (
                "Duplicate".to_string(),
                MenuAction::Drums(DrumOp::Duplicate { note }),
            ),
            (
                match locked {
                    true => "Unlock".to_string(),
                    false => "Lock".to_string(),
                },
                MenuAction::Drums(DrumOp::Lock { note }),
            ),
            (
                "Show Similar Files".to_string(),
                MenuAction::Drums(DrumOp::Similar { note }),
            ),
            (
                "Extract Chain".to_string(),
                MenuAction::Drums(DrumOp::Extract { note }),
            ),
            (
                "Delete".to_string(),
                MenuAction::Drums(DrumOp::Remove { note }),
            ),
        ];
        self.menu = Some(Menu {
            at,
            items,
            ..Default::default()
        });
        cx.notify();
    }
}
