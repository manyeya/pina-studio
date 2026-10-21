//! The control bar: the project, the transport and its display, song or pattern, the panels,
//! who's here (you, and an agent when one is editing), and export.

use gpui::{Context, Div, FontWeight, MouseButton, MouseDownEvent, div, prelude::*, px, rgb};
use pina_engine::{look::Family, music::Key, project::PPQ};

use crate::{
    ui::looks::family_shade,
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Bottom, Drag, View, Workspace, pt},
};

/// The narrowest the window goes.
pub const MIN_W: f32 = 1120.0;
/// Below these widths, the control bar tightens up to keep everything in it: first the
/// wordmark goes and the spacing closes up, then Export becomes an icon.
const ROOMY: f32 = 1520.0;
const SNUG: f32 = 1320.0;

/// The time signatures to pick from.
const SIGNATURES: [[u64; 2]; 12] = [
    [4, 4],
    [3, 4],
    [2, 4],
    [5, 4],
    [6, 4],
    [7, 4],
    [6, 8],
    [3, 8],
    [5, 8],
    [7, 8],
    [9, 8],
    [12, 8],
];

/// A field of the transport display: a value over its label.
fn field(value: impl IntoElement, label: &'static str, big: bool, pad: f32) -> Div {
    div()
        .px(px(pad))
        .flex()
        .flex_col()
        .gap(px(1.))
        .child(
            div()
                .text_size(px(if big { 17. } else { 14. }))
                .line_height(px(20.))
                .font_weight(FontWeight::MEDIUM)
                .child(value),
        )
        .child(
            div()
                .text_size(px(8.5))
                .line_height(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_3))
                .child(label),
        )
}

fn divider(h: f32) -> Div {
    div().w(px(1.)).h(px(h)).flex_none().bg(rgb(LINE))
}

impl Workspace {
    pub fn control_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let playing = self.player.playing();
        let recording = self.recording.is_some();
        let project = &self.session.project;
        let ticks = self.playhead();
        let [bar, beat, sixteenth, _] = project.bbt(ticks);
        let dot = || div().text_color(rgb(TEXT_4)).child(".");
        let position = div()
            .flex()
            .child(format!("{}", bar + 1))
            .child(dot())
            .child(format!("{}", beat + 1))
            .child(dot())
            .child(format!("{}", sixteenth + 1))
            .child(dot())
            .child(format!("{:03}", ticks % (PPQ / 4) + 1));
        let seconds = self.player.position() as f64 / self.player.rate();
        let time = format!("{}:{:06.3}", (seconds / 60.0) as u64, seconds % 60.0);
        let key = project.song_key();
        let key_label = key.map_or("—".to_string(), |k| k.short());
        let cycle = project.cycle.is_some_and(|c| !c.off);
        let metronome = self.player.metronome();
        let name = self
            .session
            .dir
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());

        let transport = |i: Icon, on_bg: Option<u32>, color: u32, w: f32| {
            div()
                .w(px(w))
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .cursor_pointer()
                .bg(match on_bg {
                    Some(c) => alpha(c, 1.0),
                    None => alpha(BG, 1.0),
                })
                .text_color(rgb(color))
                .when(on_bg.is_none(), |d| d.hover(|s| s.bg(rgb(OFF))))
                .child(icon(i, 16.).text_color(rgb(color)))
        };
        let toggle_on = |on: bool| on.then_some(0x1d2a4a);
        // Tighter below a roomy width, so every field still fits.
        let roomy = self.viewport_w >= ROOMY;
        let snug = !roomy && self.viewport_w >= SNUG;
        let pad = match (roomy, snug) {
            (true, _) => 12.,
            (_, true) => 10.,
            _ => 8.,
        };
        let [beats, unit] = project.time_signature;

        div()
            .h(px(CONTROL_BAR_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(match (roomy, snug) {
                (true, _) => 10.,
                (_, true) => 8.,
                _ => 6.,
            }))
            .pl(px(14.))
            .pr(px(12.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(logo())
                    .when(roomy, |d| {
                        d.child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Pina Studio"),
                        )
                    }),
            )
            .child(divider(20.))
            .child(
                div()
                    .h(px(30.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(OFF)))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .child(
                        div()
                            .max_w(px(if roomy { 220. } else { 120. }))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .child(icon(Icon::Chevron, 14.).text_color(rgb(TEXT_3)))
                    .debug_selector(|| "project-menu".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            ws.refresh_songs(cx);
                            ws.menu = Some(ws.song_menu(e.position));
                            cx.notify();
                        }),
                    ),
            )
            .child(div().flex_1())
            .child(
                tray()
                    .child(transport(Icon::ToStart, None, TEXT_2, 32.).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.go_to_start(cx)),
                    ))
                    .child(
                        transport(Icon::Stop, None, TEXT_2, 32.).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.stop(cx)),
                        ),
                    )
                    .child(
                        transport(
                            Icon::Play,
                            playing.then_some(ACCENT),
                            if playing { 0xffffff } else { TEXT },
                            40.,
                        )
                        .debug_selector(|| "play".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.player.play();
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        transport(
                            Icon::Record,
                            recording.then_some(RECORD),
                            if recording { 0xffffff } else { RECORD },
                            32.,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.record(cx)),
                        ),
                    )
                    .child(divider(18.).mx(px(3.)))
                    .child(
                        transport(
                            Icon::Cycle,
                            toggle_on(cycle),
                            if cycle { ACCENT_TEXT } else { TEXT_2 },
                            32.,
                        )
                        .debug_selector(|| "cycle".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.toggle_cycle(cx)),
                        ),
                    )
                    .child(
                        transport(
                            Icon::Metronome,
                            toggle_on(metronome),
                            if metronome { ACCENT_TEXT } else { TEXT_2 },
                            32.,
                        )
                        .debug_selector(|| "metronome".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.toggle_metronome(cx)),
                        ),
                    ),
            )
            .child(
                div()
                    .h(px(40.))
                    .px(px(2.))
                    .flex()
                    .items_center()
                    .rounded(px(10.))
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(LINE))
                    .font_family(MONO)
                    .child(field(position, "BAR · BEAT · DIV · TICK", true, pad))
                    .child(divider(24.))
                    .child(field(
                        div().text_color(rgb(TEXT_2)).child(time),
                        "TIME",
                        false,
                        pad,
                    ))
                    .child(divider(24.))
                    .child(
                        field(format!("{:.2}", project.tempo), "BPM", false, pad)
                            .cursor_ns_resize()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    let bpm = ws.session.project.tempo;
                                    ws.begin_drag(
                                        Drag::Tempo {
                                            y: pt(e.position).y,
                                            bpm,
                                        },
                                        cx,
                                    );
                                }),
                            ),
                    )
                    .child(divider(24.))
                    .child(
                        field(format!("{beats}/{unit}"), "TIME SIG", false, pad)
                            .cursor_pointer()
                            .debug_selector(|| "time-signature".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    let now = ws.session.project.time_signature;
                                    let items = SIGNATURES
                                        .into_iter()
                                        .map(|s| {
                                            let mark = if s == now { "✓ " } else { "    " };
                                            let label = format!("{mark}{}/{}", s[0], s[1]);
                                            (label, MenuAction::TimeSignature(s))
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
                    .child(divider(24.))
                    .child(
                        field(key_label, "KEY", false, pad)
                            .cursor_pointer()
                            .debug_selector(|| "key".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                                    ws.menu = Some(ws.key_menu(e.position));
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(
                tray()
                    .child(seg("Song", !self.pattern_mode, 30.).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.set_pattern_mode(false, cx)),
                    ))
                    .child(
                        seg("Pattern", self.pattern_mode, 30.)
                            .debug_selector(|| "pattern-mode".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.set_pattern_mode(true, cx)),
                            ),
                    ),
            )
            .child(div().flex_1())
            .child(
                tray()
                    .child(
                        icon_seg(
                            Icon::PanelLeft,
                            self.show_inspector && self.view == View::Arrange,
                            30.,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.view = View::Arrange;
                                ws.show_inspector = !ws.show_inspector;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        icon_seg(
                            Icon::PanelBottom,
                            self.bottom.is_some_and(|b| b != Bottom::Loops)
                                && self.view == View::Arrange,
                            30.,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.view = View::Arrange;
                                ws.bottom = match ws.bottom {
                                    Some(b) if b != Bottom::Loops => None,
                                    _ => Some(Bottom::PianoRoll),
                                };
                                ws.focus_roll();
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        icon_seg(Icon::Mixer, self.view == View::Mixer, 30.)
                            .debug_selector(|| "mixer".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.toggle_mixer(cx)),
                            ),
                    )
                    .child(
                        icon_seg(
                            Icon::Grid,
                            self.bottom == Some(Bottom::Loops) && self.view == View::Arrange,
                            30.,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.toggle_bottom(Bottom::Loops, cx)),
                        ),
                    )
                    .child(
                        icon_seg(
                            Icon::Samples,
                            self.view == View::Samples
                                || (self.view == View::Arrange && self.show_browser),
                            30.,
                        )
                        .debug_selector(|| "samples".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.toggle_samples(cx)),
                        ),
                    )
                    .child(
                        icon_seg(Icon::Settings, self.view == View::Settings, 30.)
                            .debug_selector(|| "settings".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.open_settings(cx)),
                            ),
                    ),
            )
            .child(self.presence(cx))
            .child(
                div()
                    .h(px(32.))
                    .px(px(if roomy || snug { 14. } else { 9. }))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .bg(rgb(if self.exporting { RAISED } else { TEXT }))
                    .text_color(rgb(if self.exporting { TEXT_2 } else { BG }))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .hover(|s| s.bg(rgb(0xffffff)))
                    .when(!roomy && !snug, |d| {
                        let fg = if self.exporting { TEXT_2 } else { BG };
                        d.child(icon(Icon::Export, 15.).text_color(rgb(fg)))
                    })
                    .when(roomy || snug || self.exporting, |d| {
                        d.child(if self.exporting {
                            "Exporting…"
                        } else {
                            "Export"
                        })
                    })
                    .debug_selector(|| "export".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.open_export(cx)),
                    ),
            )
    }

    /// The agent's mark (the one picked in the Agent panel): ringed while it, or any agent, is
    /// working on the song, raised while its panel is open; clicking it opens the panel.
    fn presence(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.chat.busy || self.agent_presence().is_some();
        let open = self.show_agent && !self.show_browser && self.view == View::Arrange;
        div()
            .size(px(30.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(15.))
            .cursor_pointer()
            .bg(rgb(if open { RAISED } else { FIELD }))
            .border_2()
            .border_color(rgb(if busy {
                AGENT
            } else if open {
                LINE_STRONG
            } else {
                FIELD
            }))
            .hover(|d| d.bg(rgb(RAISED)))
            .child(crate::agent::chat::logo(self.chat.kind, 15.))
            .debug_selector(|| "agent".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _, window, cx| ws.toggle_agent(window, cx)),
            )
    }

    /// The keys to choose from, and going back to working it out.
    pub fn key_menu(&self, at: gpui::Point<gpui::Pixels>) -> Menu {
        let detected = {
            let mut p = self.session.project.clone();
            p.key = None;
            p.song_key()
        };
        let mut items = vec![(
            match detected {
                Some(k) => format!("From the notes ({})", k.name()),
                None => "From the notes".into(),
            },
            MenuAction::Key(None),
        )];
        // Majors in one column, minors in the other, the key now ticked.
        let now = self.session.project.key.as_deref().and_then(Key::parse);
        items.extend(Key::all().map(|k| {
            let label = match now == Some(k) {
                true => format!("✓ {}", k.name()),
                false => k.name(),
            };
            (label, MenuAction::Key(Some(k.name())))
        }));
        // Across the bottom: whether choosing one moves the music into it.
        let tail = (
            match self.key_moves_notes {
                true => "✓ Move the notes into the new key".to_string(),
                false => "    Move the notes into the new key".to_string(),
            },
            MenuAction::KeyMovesNotes(at),
        );
        Menu {
            at,
            items,
            columns: 2,
            lead: 1,
            tail: Some(tail),
            ..Default::default()
        }
    }
}

/// Pina Studio's mark, as its app icon (scripts/icon.py): steps spelling p, a row a track, each in
/// its family's colour.
fn logo() -> impl IntoElement {
    const P: [[bool; 4]; 5] = [
        [true, true, true, true],
        [true, false, false, true],
        [true, true, true, true],
        [true, false, false, false],
        [true, false, false, false],
    ];
    let rows = [
        Family::Drums,
        Family::Percussion,
        Family::Bass,
        Family::Keys,
        Family::Synth,
    ];
    div()
        .size(px(24.))
        .flex_none()
        .rounded(px(7.))
        .bg(rgb(0x0c0c0f))
        .border_1()
        .border_color(rgb(LINE_STRONG))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(1.))
        .children(P.iter().zip(rows).map(|(steps, family)| {
            div()
                .flex()
                .gap(px(1.))
                .children(steps.iter().map(move |&on| {
                    div().size(px(3.)).rounded(px(0.75)).bg(rgb(if on {
                        family_shade(family, 0)
                    } else {
                        0x1c1c23
                    }))
                }))
        }))
}
