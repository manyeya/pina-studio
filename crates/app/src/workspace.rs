//! The main window, over one open project: control bar, inspector, tracks area and mixer. Every
//! edit checkpoints the session (for undo), changes the project, and syncs the player.

use std::{cell::Cell, collections::HashMap, rc::Rc};

use gpui::{
    App, Bounds, Context, DispatchPhase, Div, FocusHandle, FontWeight, KeyBinding, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, PromptLevel, ScrollDelta,
    ScrollWheelEvent, Window, actions, canvas, div, prelude::*, px, rgb,
};
use tempo_engine::{
    player::Player,
    project::{At, BEATS_PER_BAR, Instrument, PPQ, Track},
    session::Session,
};

use crate::{
    strip,
    theme::{self, *},
};

actions!(
    tempo,
    [
        PlayStop,
        GoToStart,
        Undo,
        Redo,
        Delete,
        DeleteTrack,
        Split,
        ToggleMixer,
        ToggleInspector,
        Save,
        NewTrack,
        ZoomIn,
        ZoomOut,
        Quit,
    ]
);

/// Logic's shortcuts, where it has one.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("space", PlayStop, None),
        KeyBinding::new("enter", GoToStart, None),
        KeyBinding::new("cmd-z", Undo, None),
        KeyBinding::new("cmd-shift-z", Redo, None),
        KeyBinding::new("backspace", Delete, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("cmd-backspace", DeleteTrack, None),
        KeyBinding::new("cmd-t", Split, None),
        KeyBinding::new("x", ToggleMixer, None),
        KeyBinding::new("i", ToggleInspector, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-alt-n", NewTrack, None),
        KeyBinding::new("cmd-right", ZoomIn, None),
        KeyBinding::new("cmd-left", ZoomOut, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-w", Quit, None),
    ]);
}

#[derive(Clone, Copy, PartialEq)]
pub enum Edge {
    Move,
    Start,
    End,
}

/// A mouse drag in progress. Edits checkpoint when they start, so a drag is one undo step.
#[derive(Clone, Copy)]
pub enum Drag {
    Region {
        at: At,
        edge: Edge,
        from: Point<f32>,
        start: u64,
        end: u64,
        track: usize,
        moved: bool,
    },
    Fader {
        track: Option<usize>,
        y: f32,
        pos: f32,
    },
    Pan {
        track: usize,
        y: f32,
        pan: f32,
    },
    Tempo {
        y: f32,
        bpm: f64,
    },
    Locate,
}

pub struct Workspace {
    pub session: Session,
    pub player: Player,
    focus: FocusHandle,
    /// The selected track, and region.
    pub track: usize,
    pub region: Option<At>,
    /// Horizontal zoom, and scroll in pixels.
    pub px_per_beat: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    show_mixer: bool,
    show_inspector: bool,
    drag: Option<Drag>,
    /// Meter readings, falling back slowly, by track id.
    pub levels: HashMap<u64, f32>,
    pub master_level: f32,
    status: Option<String>,
    /// Where the lanes were drawn last frame, for turning pointer positions into ticks.
    pub lanes_bounds: Rc<Cell<Bounds<Pixels>>>,
    title: String,
}

fn pt(p: Point<Pixels>) -> Point<f32> {
    p.map(f32::from)
}

impl Workspace {
    pub fn new(
        session: Session,
        mut player: Player,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let status = player.sync(&session.project).err();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |ws, cx| ws.should_close(window, cx))
                .unwrap_or(true)
        });
        Workspace {
            session,
            player,
            focus,
            track: 0,
            region: None,
            px_per_beat: 48.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            show_mixer: false,
            show_inspector: true,
            drag: None,
            levels: HashMap::new(),
            master_level: 0.0,
            status,
            lanes_bounds: Rc::new(Cell::new(Bounds::default())),
            title: String::new(),
        }
    }

    pub fn px_per_tick(&self) -> f32 {
        self.px_per_beat / PPQ as f32
    }

    /// The tick under a window x position, in the lanes.
    pub fn tick_at(&self, x: f32) -> f64 {
        let left = f32::from(self.lanes_bounds.get().origin.x);
        ((x - left + self.scroll_x) / self.px_per_tick()).max(0.0) as f64
    }

    /// The editing grid, in ticks: finer as you zoom in.
    pub fn grid(&self) -> u64 {
        match self.px_per_beat {
            z if z >= 96.0 => PPQ / 4,
            z if z >= 20.0 => PPQ,
            _ => PPQ * BEATS_PER_BAR,
        }
    }

    pub fn snap(&self, tick: f64) -> u64 {
        let g = self.grid() as f64;
        ((tick / g).round() * g).max(0.0) as u64
    }

    pub fn playhead(&self) -> u64 {
        self.session.project.ticks(self.player.position())
    }

    /// After an edit (or undo): keeps the selection valid and brings the engine up to date.
    pub fn changed(&mut self, cx: &mut Context<Self>) {
        let tracks = &self.session.project.tracks;
        self.track = self.track.min(tracks.len().saturating_sub(1));
        if let Some((t, r)) = self.region
            && tracks.get(t).is_none_or(|t| r >= t.regions.len())
        {
            self.region = None;
        }
        self.status = self.player.sync(&self.session.project).err();
        cx.notify();
    }

    /// One undoable edit.
    pub fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Self)) {
        self.session.checkpoint();
        f(self);
        self.changed(cx);
    }

    pub fn begin_drag(&mut self, drag: Drag, cx: &mut Context<Self>) {
        if !matches!(drag, Drag::Locate) {
            self.session.checkpoint();
        }
        self.drag = Some(drag);
        cx.notify();
    }

    fn drag_to(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        let ppt = self.px_per_tick();
        let tracks = self.session.project.tracks.len();
        if let Some(Drag::Region { moved, .. }) = &mut self.drag {
            *moved = true;
        }
        match self.drag {
            Some(Drag::Region {
                at,
                edge,
                from,
                start,
                end,
                track,
                ..
            }) => {
                let dt = ((p.x - from.x) / ppt) as f64;
                match edge {
                    Edge::Move => {
                        let start = self.snap(start as f64 + dt);
                        let rows = ((p.y - from.y) / LANE_H).round() as isize;
                        let to = (track as isize + rows).clamp(0, tracks as isize - 1) as usize;
                        let now = self.session.project.move_region(at, to, start);
                        if let Some(Drag::Region { at, .. }) = &mut self.drag {
                            *at = now;
                        }
                        self.region = Some(now);
                        self.track = now.0;
                    }
                    Edge::Start => {
                        let start = self.snap(start as f64 + dt);
                        self.session.project.trim_start(at, start);
                    }
                    Edge::End => {
                        let end = self.snap(end as f64 + dt);
                        self.session.project.trim_end(at, end);
                    }
                }
            }
            Some(Drag::Fader { track, y, pos }) => {
                let db = strip::fader_db((pos - (p.y - y) / FADER_H).clamp(0.0, 1.0));
                match track {
                    Some(t) => self.session.project.tracks[t].volume_db = db,
                    None => self.session.project.master_volume_db = db,
                }
            }
            Some(Drag::Pan { track, y, pan }) => {
                self.session.project.tracks[track].pan = (pan - (p.y - y) / 100.0).clamp(-1.0, 1.0);
            }
            Some(Drag::Tempo { y, bpm }) => {
                self.session.project.tempo =
                    (bpm + ((y - p.y) / 4.0) as f64).round().clamp(20.0, 400.0);
            }
            Some(Drag::Locate) => self.locate_at(p.x, cx),
            None => return,
        }
        self.changed(cx);
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(Drag::Region { moved: false, .. }) = self.drag {
            self.session.cancel();
        }
        self.drag = None;
        cx.notify();
    }

    /// Moves the playhead to the grid line nearest a window x position.
    pub fn locate_at(&mut self, x: f32, cx: &mut Context<Self>) {
        let project = &self.session.project;
        let tick = self.snap(self.tick_at(x));
        self.player
            .locate(project.samples(tick, self.player.rate()));
        cx.notify();
    }

    fn scroll(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => p.map(f32::from),
            ScrollDelta::Lines(l) => l.map(|v| v * 24.0),
        };
        if e.modifiers.platform {
            // Zoom around the pointer.
            let x = f32::from(e.position.x) - f32::from(self.lanes_bounds.get().origin.x);
            let tick = (x + self.scroll_x) / self.px_per_tick();
            self.px_per_beat = (self.px_per_beat * (1.0 + d.y * 0.01)).clamp(4.0, 800.0);
            self.scroll_x = (tick * self.px_per_tick() - x).max(0.0);
        } else {
            let height = f32::from(self.lanes_bounds.get().size.height);
            let content = self.session.project.tracks.len() as f32 * LANE_H;
            self.scroll_x = (self.scroll_x - d.x).max(0.0);
            self.scroll_y = (self.scroll_y - d.y).clamp(0.0, (content - height + LANE_H).max(0.0));
        }
        cx.notify();
    }

    pub fn on_scroll(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ScrollWheelEvent, &mut Window, &mut App) + 'static {
        cx.listener(|this, e, _, cx| this.scroll(e, cx))
    }

    fn play_stop(&mut self, cx: &mut Context<Self>) {
        if self.player.playing() {
            self.player.stop();
        } else {
            self.player.play();
        }
        cx.notify();
    }

    fn go_to_start(&mut self, cx: &mut Context<Self>) {
        self.player.locate(0);
        self.scroll_x = 0.0;
        cx.notify();
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        if self.session.undo() {
            self.changed(cx);
        }
    }

    fn redo(&mut self, cx: &mut Context<Self>) {
        if self.session.redo() {
            self.changed(cx);
        }
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        let Some((t, r)) = self.region else { return };
        self.edit(cx, |ws| {
            ws.session.project.tracks[t].regions.remove(r);
            ws.region = None;
        });
    }

    fn delete_track(&mut self, cx: &mut Context<Self>) {
        if self.session.project.tracks.is_empty() {
            return;
        }
        let t = self.track;
        self.edit(cx, |ws| {
            ws.session.project.tracks.remove(t);
            ws.region = None;
        });
    }

    /// Splits the selected region at the playhead, as Logic's ⌘T does.
    fn split(&mut self, cx: &mut Context<Self>) {
        let (Some(at), tick) = (self.region, self.playhead()) else {
            return;
        };
        self.session.checkpoint();
        match self.session.project.split_region(at, tick) {
            Some(second) => {
                self.region = Some(second);
                self.changed(cx);
            }
            None => self.session.cancel(),
        }
    }

    pub fn new_track(&mut self, cx: &mut Context<Self>) {
        let n = self.session.project.tracks.len();
        let at = if n == 0 { 0 } else { self.track + 1 };
        self.edit(cx, |ws| {
            let track = Track {
                name: format!("Inst {}", n + 1),
                instrument: Some(Instrument::Synth),
                ..Default::default()
            };
            ws.session.project.add_track(at, track);
            ws.track = at;
            ws.region = None;
        });
    }

    fn zoom(&mut self, by: f32, cx: &mut Context<Self>) {
        self.px_per_beat = (self.px_per_beat * by).clamp(4.0, 800.0);
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) -> bool {
        let result = self.session.save();
        self.status = Some(match &result {
            Ok(()) => format!("Saved {}", self.session.dir.display()),
            Err(e) => e.clone(),
        });
        cx.notify();
        result.is_ok()
    }

    /// Quits, asking first if there are unsaved changes.
    fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.edited {
            cx.quit();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "Save changes to this project before quitting?",
            None,
            &["Save", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(choice) = answer.await else { return };
            this.update(cx, |this, cx| match choice {
                0 if this.save(cx) => cx.quit(),
                1 => cx.quit(),
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.session.edited {
            self.quit(window, cx);
        }
        !self.session.edited
    }

    /// Catches pointer moves and releases anywhere in the window while dragging.
    fn drag_listener(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let t = this.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        t.update(cx, |ws, cx| ws.drag_to(pt(e.position), cx)).ok();
                    }
                });
                let t = this.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        t.update(cx, |ws, cx| ws.end_drag(cx)).ok();
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }

    /// Reads the meters, letting them fall back gently.
    fn read_meters(&mut self) {
        for t in &self.session.project.tracks {
            let l = self.levels.entry(t.id).or_default();
            *l = self.player.meter(t.id).max(*l * 0.85);
        }
        self.master_level = self.player.master_meter().max(self.master_level * 0.85);
    }

    /// Keeps the playhead on screen while playing, a page at a time, as Logic does.
    fn follow_playhead(&mut self) {
        let width = f32::from(self.lanes_bounds.get().size.width);
        let x = self.playhead() as f32 * self.px_per_tick() - self.scroll_x;
        if width > 0.0 && (x < 0.0 || x > width - 20.0) {
            self.scroll_x = (x + self.scroll_x - 20.0).max(0.0);
        }
    }

    fn control_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let playing = self.player.playing();
        let project = &self.session.project;
        let ticks = self.playhead();
        let beat = ticks / PPQ;
        let position = format!(
            "{:>3} {} {} {:>3}",
            beat / BEATS_PER_BAR + 1,
            beat % BEATS_PER_BAR + 1,
            ticks % PPQ / (PPQ / 4) + 1,
            ticks % (PPQ / 4) + 1
        );
        let transport = |icon: Icon, on: bool| {
            div()
                .w(px(34.))
                .h(px(26.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(if on { PLAYING } else { BUTTON }))
                .child(theme::icon(icon, rgb(if on { 0x10220f } else { TEXT })))
        };
        let lcd_field = |value: String, label: &'static str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .child(div().text_size(px(19.)).child(value))
                .child(
                    div()
                        .text_size(px(8.))
                        .text_color(rgb(TEXT_DIM))
                        .child(label),
                )
        };
        let name = self
            .session
            .dir
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());

        div()
            .h(px(CONTROL_BAR_H))
            .flex_none()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(button("Inspector", self.show_inspector).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.show_inspector = !ws.show_inspector;
                            cx.notify();
                        }),
                    ))
                    .child(button("Mixer", self.show_mixer).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.show_mixer = !ws.show_mixer;
                            cx.notify();
                        }),
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(transport(Icon::ToStart, false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.go_to_start(cx)),
                    ))
                    .child(transport(Icon::Stop, false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.player.stop();
                            cx.notify();
                        }),
                    ))
                    .child(transport(Icon::Play, playing).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.player.play();
                            cx.notify();
                        }),
                    ))
                    .child(transport(Icon::Record, false).opacity(0.4)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_5()
                    .px_4()
                    .py_1()
                    .rounded(px(6.))
                    .bg(rgb(LCD))
                    .font_family(MONO)
                    .text_color(rgb(LCD_TEXT))
                    .child(lcd_field(position, "BAR  BEAT  DIV  TICK"))
                    .child(
                        lcd_field(format!("{:.0}", project.tempo), "TEMPO")
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
                    .child(lcd_field("4/4".into(), "TIME")),
            )
            .child(div().flex_1())
            .child(div().text_size(px(12.)).text_color(rgb(TEXT_DIM)).child(
                if self.session.edited {
                    format!("{name} — Edited")
                } else {
                    name
                },
            ))
    }

    fn inspector(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let label = |s: &'static str| {
            div()
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(TEXT_DIM))
                .child(s)
        };
        let has_track = !self.session.project.tracks.is_empty();
        div()
            .w(px(INSPECTOR_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .bg(rgb(PANEL_DARK))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(label("CHANNEL STRIPS"))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .when(has_track, |d| {
                        d.child(strip::strip(self, Some(self.track), cx))
                    })
                    .child(strip::strip(self, None, cx)),
            )
    }

    fn mixer(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let strips: Vec<Div> = (0..self.session.project.tracks.len())
            .map(|i| strip::strip(self, Some(i), cx))
            .collect();
        div()
            .h(px(MIXER_H))
            .flex_none()
            .flex()
            .bg(rgb(PANEL_DARK))
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .id("mixer")
                    .flex_1()
                    .flex()
                    .gap_1()
                    .p_1()
                    .overflow_x_scroll()
                    .children(strips),
            )
            .child(
                div()
                    .p_1()
                    .border_l_1()
                    .border_color(rgb(BORDER))
                    .child(strip::strip(self, None, cx)),
            )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.player.collect();
        self.read_meters();
        let playing = self.player.playing();
        if playing {
            self.follow_playhead();
        }
        // ponytail: redraws every frame while anything moves; fine until the UI gets heavy.
        let busy = playing || self.master_level > 1e-4 || self.levels.values().any(|&l| l > 1e-4);
        if busy || self.drag.is_some() {
            window.request_animation_frame();
        }
        let dir = self
            .session
            .dir
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy());
        let title = format!(
            "{dir}{}",
            if self.session.edited {
                " — Edited"
            } else {
                ""
            }
        );
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }

        let status = self.status.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .track_focus(&self.focus)
            .on_action(cx.listener(|ws, _: &PlayStop, _, cx| ws.play_stop(cx)))
            .on_action(cx.listener(|ws, _: &GoToStart, _, cx| ws.go_to_start(cx)))
            .on_action(cx.listener(|ws, _: &Undo, _, cx| ws.undo(cx)))
            .on_action(cx.listener(|ws, _: &Redo, _, cx| ws.redo(cx)))
            .on_action(cx.listener(|ws, _: &Delete, _, cx| ws.delete(cx)))
            .on_action(cx.listener(|ws, _: &DeleteTrack, _, cx| ws.delete_track(cx)))
            .on_action(cx.listener(|ws, _: &Split, _, cx| ws.split(cx)))
            .on_action(cx.listener(|ws, _: &NewTrack, _, cx| ws.new_track(cx)))
            .on_action(cx.listener(|ws, _: &ZoomIn, _, cx| ws.zoom(1.25, cx)))
            .on_action(cx.listener(|ws, _: &ZoomOut, _, cx| ws.zoom(0.8, cx)))
            .on_action(cx.listener(|ws, _: &Save, _, cx| {
                ws.save(cx);
            }))
            .on_action(cx.listener(|ws, _: &Quit, window, cx| ws.quit(window, cx)))
            .on_action(cx.listener(|ws, _: &ToggleMixer, _, cx| {
                ws.show_mixer = !ws.show_mixer;
                cx.notify();
            }))
            .on_action(cx.listener(|ws, _: &ToggleInspector, _, cx| {
                ws.show_inspector = !ws.show_inspector;
                cx.notify();
            }))
            .child(self.control_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .when(self.show_inspector, |d| d.child(self.inspector(cx)))
                    .child(self.tracks_area(cx)),
            )
            .when(self.show_mixer, |d| d.child(self.mixer(cx)))
            .when_some(status, |d, s| {
                d.child(
                    div()
                        .h(px(22.))
                        .flex_none()
                        .px_3()
                        .flex()
                        .items_center()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_DIM))
                        .bg(rgb(PANEL))
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(s),
                )
            })
            .when(self.drag.is_some(), |d| d.child(self.drag_listener(cx)))
    }
}
