//! The tracks area: tools and track headers on the left; markers, the ruler (with the cycle) and
//! the lanes with their regions on the right. Regions move by dragging their body, and trim by
//! dragging their edges; the tools add, cut and erase them.

use std::sync::Arc;

use gpui::{
    AnyElement, App, Bounds, Context, Div, FontWeight, Hsla, MouseButton, MouseDownEvent,
    PathBuilder, Pixels, Point, SharedString, Window, canvas, div, fill, point, prelude::*, px,
    rgb, size,
};
use tempo_engine::{
    peaks::Peaks,
    project::{
        At, AutoMode, AutoPoint, Content, Instrument, PPQ, PanMode, Param, Region, Which,
        comp_segments,
    },
    stretch::Map,
};

use crate::{
    menu::{MARKER_NAMES, Menu, MenuAction},
    strip::{self, Channel},
    theme::*,
    workspace::{Bottom, Drag, Edge, Name, Snap, Tool, Workspace},
};

/// Space kept free above and below an automation line.
const AUTO_PAD: f32 = 6.0;

/// Where a parameter value sits in an automation lane, from its top: volume on the fader's
/// scale, pan with left at the top, a plugin parameter with 1 at the top.
pub fn auto_to_y(param: Param, value: f32) -> f32 {
    let t = match param {
        Param::Volume => 1.0 - strip::fader_pos(value),
        Param::Pan => (value.clamp(-1.0, 1.0) + 1.0) / 2.0,
        Param::Plugin(..) => 1.0 - value.clamp(0.0, 1.0),
    };
    AUTO_PAD + t * (AUTO_H - 2.0 * AUTO_PAD)
}

pub fn auto_from_y(param: Param, y: f32) -> f32 {
    let t = ((y - AUTO_PAD) / (AUTO_H - 2.0 * AUTO_PAD)).clamp(0.0, 1.0);
    match param {
        Param::Volume => strip::fader_db(1.0 - t),
        Param::Pan => t * 2.0 - 1.0,
        Param::Plugin(..) => 1.0 - t,
    }
}

/// How each automation mode shows: Read green, Touch amber, Latch violet, Off grey.
pub fn mode_color(mode: AutoMode) -> u32 {
    match mode {
        AutoMode::Off => TEXT_3,
        AutoMode::Read => METER_LOW,
        AutoMode::Touch => AMBER,
        AutoMode::Latch => VIOLET,
    }
}

/// What a region shows inside.
#[derive(Default)]
pub struct Inside {
    /// Start (ticks from the region's start), length and pitch.
    notes: Vec<(i64, u64, u8)>,
    waves: Vec<Wave>,
}

/// A stretch of waveform: from `x0` to `x1` pixels into the region, `offset` the sample at `x0`.
struct Wave {
    x0: f32,
    x1: f32,
    peaks: Arc<Peaks>,
    offset: f32,
    samples_per_px: f32,
}

/// Where an audio region's file plays: its frames (at the project's rate) in the stretched,
/// warped audio, and those in ticks from where the region starts.
pub struct Warping {
    pub map: Map,
    pub hits: Arc<Vec<usize>>,
    pub warps: Vec<[u64; 2]>,
    /// The stretched frame the region starts at, and frames a tick.
    start: f64,
    per_tick: f64,
}

impl Warping {
    /// Ticks from the region's start where file frame `f` plays.
    pub fn tick(&self, f: usize) -> f64 {
        (self.map.at(f) as f64 - self.start) / self.per_tick
    }
}

/// What a region is called on screen.
fn region_name(track: &str, region: &Region) -> String {
    match &region.content {
        Content::Takes { takes, comp } => {
            match &comp_segments(comp, takes.len(), region.length)[..] {
                [(_, _, take)] => format!("{track} · Take {}", take + 1),
                _ => format!("{track} · Comp"),
            }
        }
        Content::Audio { stretch, .. } if *stretch != 1.0 => {
            format!("{track} · Flex ×{stretch:.2}")
        }
        Content::Audio { file, .. } => file
            .file_stem()
            .map_or(track.to_string(), |s| s.to_string_lossy().into_owned()),
        _ if !region.name.is_empty() => region.name.clone(),
        Content::Pattern { .. } => format!("{track} · Pattern"),
        Content::Midi { .. } => track.to_string(),
    }
}

/// A sampler's name: a kit of its sounds, or the sample it plays across the keys.
pub fn sampler_name(s: &tempo_engine::project::Sampler) -> String {
    match (s.zones.len(), s.zones.first()) {
        (0, _) => "Sampler".into(),
        (1, Some(z)) if z.high.is_some() => {
            let stem = z.file.file_stem().map(|n| n.to_string_lossy().into_owned());
            format!(
                "Sampler · {}",
                if z.name.is_empty() {
                    stem.unwrap_or_default()
                } else {
                    z.name.clone()
                }
            )
        }
        (n, _) if s.zones.iter().all(|z| z.high.is_none()) => format!("{n}-piece kit"),
        (n, _) => multisample_name(s).unwrap_or_else(|| format!("{n} zones")),
    }
}

/// What a multisample's files are named for, as "Moog Bass" for Moog_Bass_C1.wav,
/// Moog_Bass_C#1.wav...: the start their names share.
fn multisample_name(s: &tempo_engine::project::Sampler) -> Option<String> {
    let stems: Vec<String> = s
        .zones
        .iter()
        .map(|z| z.file.file_stem().map(|n| n.to_string_lossy().into_owned()))
        .collect::<Option<_>>()?;
    let first = stems.first()?;
    let shared = stems.iter().fold(first.len(), |n, stem| {
        first
            .chars()
            .zip(stem.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum::<usize>()
            .min(n)
    });
    let name = first[..shared]
        .trim_end_matches(['_', '-', ' ', '.'])
        .replace('_', " ");
    (name.chars().count() >= 3).then_some(name)
}

/// The instrument's kind, in a word or two: beside a stack's chip, where a name won't fit.
fn instrument_kind(i: Option<&Instrument>) -> String {
    match i {
        Some(i @ Instrument::Sampler(_)) if i.is_kit() => "Kit".into(),
        Some(Instrument::Sampler(_)) => "Sampler".into(),
        _ => instrument_name(i),
    }
}

/// The instrument's name, under a track's.
pub fn instrument_name(i: Option<&Instrument>) -> String {
    match i {
        None => "Audio".into(),
        Some(Instrument::Synth) => "Sine Synth".into(),
        Some(Instrument::Drums) => "Drum Kit".into(),
        Some(Instrument::Sampler(s)) => sampler_name(s),
        Some(Instrument::Plugin(r)) => r.class.clone(),
    }
}

impl Workspace {
    pub fn tracks_area(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(MARKERS_H + RULER_H))
                    .flex_none()
                    .flex()
                    .child(self.toolbar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(self.markers(cx))
                            .child(self.ruler(cx)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(self.headers(cx))
                    .child(self.lanes(cx)),
            )
    }

    /// Over the track headers: the tools, the snap, and adding a track.
    fn toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let tool = |i: Icon, t: Tool, on: bool, cx: &mut Context<Self>| {
            icon_seg(i, on, 24.)
                .debug_selector(move || format!("tool-{t:?}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        ws.tool = t;
                        cx.notify();
                    }),
                )
        };
        let snap = match self.snap {
            Snap::Smart => match self.grid() {
                g if g == PPQ / 4 => "1/16",
                g if g == PPQ => "Beat",
                _ => "Bar",
            },
            s => s.label(),
        };
        div()
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(10.))
            .pr(px(8.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                tray()
                    .p(px(2.))
                    .rounded(px(8.))
                    .child(tool(
                        Icon::Pointer,
                        Tool::Pointer,
                        self.tool == Tool::Pointer,
                        cx,
                    ))
                    .child(tool(
                        Icon::Pencil,
                        Tool::Pencil,
                        self.tool == Tool::Pencil,
                        cx,
                    ))
                    .child(tool(
                        Icon::Scissors,
                        Tool::Scissors,
                        self.tool == Tool::Scissors,
                        cx,
                    ))
                    .child(tool(
                        Icon::Eraser,
                        Tool::Eraser,
                        self.tool == Tool::Eraser,
                        cx,
                    )),
            )
            .child(
                div()
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(OFF)))
                    .font_family(MONO)
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT_2))
                    .child(icon(Icon::Magnet, 13.).text_color(rgb(TEXT_3)))
                    .child(snap)
                    .debug_selector(|| "snap".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            let items = Snap::ALL
                                .iter()
                                .map(|&s| {
                                    let label = if s == Snap::Smart {
                                        "Smart (follows zoom)".to_string()
                                    } else {
                                        s.label().to_string()
                                    };
                                    (label, MenuAction::Snap(s))
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
                div()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(LINE))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(OFF)))
                    .text_color(rgb(TEXT_2))
                    .child(icon(Icon::Plus, 14.).text_color(rgb(TEXT_2)))
                    .debug_selector(|| "new-track".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, window, cx| {
                            // The New Track palette, opening down from the button.
                            let at = e.position + gpui::point(px(-14.), px(18.));
                            ws.open_adder(at, window, cx);
                        }),
                    ),
            )
    }

    /// The song's sections: each marker runs to the next.
    fn markers(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let markers = &self.session.project.markers;
        let playhead = self.playhead();
        let bar = self.session.project.bar();
        let chips: Vec<Div> = markers
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let end = markers
                    .get(i + 1)
                    .map_or(m.tick + bar * 8, |n| n.tick.max(m.tick + 1));
                let current = (m.tick..end).contains(&playhead);
                let (x, w) = (m.tick as f32 * ppt - sx, (end - m.tick) as f32 * ppt);
                let tick = m.tick;
                div()
                    .absolute()
                    .left(px(x + 1.))
                    .top(px(3.))
                    .w(px((w - 2.).max(4.)))
                    .h(px(15.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(rgb(if current { 0x23232b } else { 0x18181e }))
                    .text_color(rgb(if current { TEXT } else { TEXT_3 }))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .cursor_grab()
                    .child(match self.name_field(Name::Marker(i)) {
                        Some(field) => div().w_full().child(field),
                        None => div().child(m.name.clone()),
                    })
                    .debug_selector(move || format!("marker-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            // Double-click a marker to rename it.
                            if e.click_count == 2 {
                                ws.start_naming(Name::Marker(i), window, cx);
                                return;
                            }
                            let x = f32::from(e.position.x);
                            ws.begin_drag(
                                Drag::Marker {
                                    index: i,
                                    x,
                                    tick,
                                    moved: false,
                                },
                                cx,
                            );
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let mut items =
                                vec![("Rename…".to_string(), MenuAction::Rename(Name::Marker(i)))];
                            items.extend(MARKER_NAMES.iter().map(|n| {
                                (
                                    format!("Call it {n}"),
                                    MenuAction::NameMarker(i, n.to_string()),
                                )
                            }));
                            items.push(("Delete Marker".into(), MenuAction::DeleteMarker(i)));
                            ws.menu = Some(Menu {
                                at: e.position,
                                items,
                                ..Default::default()
                            });
                            cx.notify();
                        }),
                    )
            })
            .collect();
        let empty = markers.is_empty();
        div()
            .h(px(MARKERS_H))
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(rgb(WELL))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .children(chips)
            .when(empty, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(8.))
                        .top(px(4.))
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_4))
                        .child("Double-click to add a marker"),
                )
            })
            .debug_selector(|| "markers".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, window, cx| {
                    if e.click_count != 2 {
                        return;
                    }
                    let bar = ws.session.project.bar() as f64;
                    let tick = ((ws.tick_at(f32::from(e.position.x)) / bar).floor() * bar) as u64;
                    let n = ws.session.project.markers.len();
                    let name = MARKER_NAMES
                        .get(n)
                        .map_or(format!("Marker {}", n + 1), |s| s.to_string());
                    ws.choose(MenuAction::AddMarker(tick, name), cx);
                    // Then name it.
                    let markers = &ws.session.project.markers;
                    if let Some(i) = markers.iter().rposition(|m| m.tick == tick) {
                        ws.start_naming(Name::Marker(i), window, cx);
                    }
                }),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    fn ruler(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, scroll) = (self.px_per_tick(), self.scroll_x);
        let (beat, per_bar) = (
            self.session.project.beat(),
            self.session.project.beats_per_bar(),
        );
        let bar_px = ppt * (beat * per_bar) as f32;
        let width = f32::from(self.lanes_bounds.get().size.width).max(2000.0);
        // Label every bar, or fewer when they'd crowd.
        let every = (40.0 / bar_px).ceil().max(1.0) as u64;
        let first = (scroll / bar_px) as u64;
        let last = ((scroll + width) / bar_px) as u64 + 1;
        let labels = (first..=last).filter(|b| b % every == 0).map(|b| {
            div()
                .absolute()
                .left(px(b as f32 * bar_px - scroll + 4.))
                .top(px(11.))
                .font_family(MONO)
                .text_size(px(10.))
                .line_height(px(12.))
                .text_color(rgb(TEXT_3))
                .child(format!("{}", b + 1))
        });
        let playhead = self.playhead() as f32 * ppt - scroll;
        let cycle = self.session.project.cycle.map(|c| {
            let (x0, x1) = (c.start as f32 * ppt - scroll, c.end as f32 * ppt - scroll);
            div()
                .absolute()
                .left(px(x0))
                .top(px(2.))
                .w(px((x1 - x0).max(2.)))
                .h(px(8.))
                .rounded(px(3.))
                .bg(if c.off {
                    alpha(TEXT_4, 0.6)
                } else {
                    alpha(ACCENT, 0.9)
                })
        });
        div()
            .h(px(RULER_H))
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(rgb(WELL))
            .border_b_1()
            .border_color(rgb(LINE))
            .children(cycle)
            .child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let beat_px = ppt * beat as f32;
                        let first = (scroll / beat_px) as u64;
                        let last = ((scroll + f32::from(b.size.width)) / beat_px) as u64 + 1;
                        for beat in first..=last {
                            let bar = beat % per_bar == 0;
                            if !bar && beat_px < 8.0 {
                                continue;
                            }
                            let x = b.origin.x + px((beat as f32 * beat_px - scroll).round());
                            let (top, color) = if bar {
                                (px(11.), LINE_STRONG)
                            } else {
                                (px(20.), LINE)
                            };
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(x, b.origin.y + top),
                                    size(px(1.), b.size.height - top),
                                ),
                                rgb(color),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .children(labels)
            .child(
                gpui::svg()
                    .data(PLAYHEAD)
                    .absolute()
                    .left(px(playhead - 5.5))
                    .top(px(13.))
                    .w(px(11.))
                    .h(px(12.))
                    .text_color(rgb(TEXT)),
            )
            .debug_selector(|| "ruler".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    // The ruler's top half is the cycle's: sweep it to loop a stretch, click it
                    // to turn the cycle on or off. The rest moves the playhead.
                    let top = f32::from(ws.lanes_bounds.get().origin.y) - RULER_H;
                    if f32::from(e.position.y) < top + RULER_H / 2.0 {
                        let anchor = ws.snap(ws.tick_at(f32::from(e.position.x)));
                        ws.begin_drag(
                            Drag::Cycle {
                                anchor,
                                moved: false,
                            },
                            cx,
                        );
                    } else {
                        ws.begin_drag(Drag::Locate, cx);
                        ws.locate_at(f32::from(e.position.x), cx);
                    }
                }),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    fn headers(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let armed = self.armed();
        let mut rows: Vec<Div> = Vec::new();
        for i in 0..self.session.project.tracks.len() {
            let t = &self.session.project.tracks[i];
            let selected = i == self.track;
            let color = self.color(i);
            let glyph = self.shown(i).look.icon;
            let picking = self.picker.as_ref().is_some_and(|p| p.track == i);
            // The summing stack it plays into, if it's in one.
            let stack = t.output.and_then(|b| {
                let aux = self
                    .session
                    .project
                    .auxes
                    .iter()
                    .find(|a| a.bus == b && a.stack)?;
                Some(aux.name.clone())
            });
            let top = self.row_top(i);
            rows.push(self.header_row(i, top, selected, color, glyph, picking, stack, armed, cx));
            let t = &self.session.project.tracks[i];
            if self.show_automation {
                let param = self.lane_param(i);
                let mode = t.automation.mode;
                let (label, plugin) = match param {
                    Param::Volume => ("Volume".to_string(), None),
                    Param::Pan => ("Pan".to_string(), None),
                    // The instrument's is plain (the header names it); an insert's says whose.
                    Param::Plugin(Which::Instrument, _) => (self.param_name(i, param), None),
                    Param::Plugin(which, _) => {
                        (self.param_name(i, param), Some(self.plugin_name(i, which)))
                    }
                };
                rows.push(
                    div()
                        .absolute()
                        .top(px(top + self.lane_h(i)))
                        .left_0()
                        .w_full()
                        .h(px(AUTO_H))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .pl(px(12.))
                        .pr(px(10.))
                        .bg(rgb(0x16161b))
                        .border_b_1()
                        .border_color(rgb(LINE_SOFT))
                        .child(
                            dropdown(label, 24.)
                                .min_w(px(0.))
                                .max_w(px(160.))
                                .flex_shrink_1()
                                .overflow_hidden()
                                .debug_selector(move || format!("auto-param-{i}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        ws.menu = Some(Menu::search(e.position, ws.auto_params(i)));
                                        cx.notify();
                                    }),
                                ),
                        )
                        .children(plugin.map(|name| {
                            div()
                                .min_w(px(0.))
                                .flex_shrink_1()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .text_ellipsis()
                                .child(name)
                        }))
                        .child(div().flex_1())
                        .child(
                            chip(mode.name(), mode_color(mode))
                                .cursor_pointer()
                                .debug_selector(move || format!("auto-mode-{i}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                        cx.stop_propagation();
                                        let items = AutoMode::ALL
                                            .iter()
                                            .map(|&m| {
                                                (m.name().to_string(), MenuAction::AutoMode(i, m))
                                            })
                                            .collect();
                                        ws.menu = Some(Menu {
                                            at: e.position,
                                            items,
                                            ..Menu::default()
                                        });
                                        cx.notify();
                                    }),
                                ),
                        ),
                );
            }
        }
        let width = self.header_w;
        div()
            .w(px(self.header_w))
            .flex_none()
            .h_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(HEADERS))
            .border_r_1()
            .border_color(rgb(LINE))
            .children(rows)
            // Its right edge: drag the column wider or narrower; double-click for its usual width.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(-1.))
                    .w(px(6.))
                    .h_full()
                    .cursor_col_resize()
                    .hover(|d| d.bg(alpha(ACCENT, 0.35)))
                    .when(
                        matches!(self.drag, Some(Drag::Width { side: false, .. })),
                        |d| d.bg(alpha(ACCENT, 0.6)),
                    )
                    .debug_selector(|| "headers-edge".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.header_w = HEADER_W;
                                cx.notify();
                                return;
                            }
                            ws.begin_drag(
                                Drag::Width {
                                    side: false,
                                    x: f32::from(e.position.x),
                                    width,
                                },
                                cx,
                            );
                        }),
                    ),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    /// A track's header, as a channel strip: its colour down its edge and its number; its icon
    /// (click for its colour and icon) and name (double-click to rename) over what it plays;
    /// mute, solo and record; and, tall enough, its fader with its meter running inside it, its
    /// level, and its pan knob, drawn as its mode pans (balance, a spread, around the head):
    /// right-click it for the modes and the pan law. Right-click the header for everything else;
    /// drag its bottom edge to make the track taller or shorter.
    #[allow(clippy::too_many_arguments)]
    fn header_row(
        &mut self,
        i: usize,
        top: f32,
        selected: bool,
        color: u32,
        glyph: &'static str,
        picking: bool,
        stack: Option<String>,
        armed: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Div {
        let lh = self.lane_h(i);
        let t = &self.session.project.tracks[i];
        let (name, mute, solo, volume, pan, mode, spread) = (
            t.name.clone(),
            t.mute,
            t.solo,
            t.volume_db,
            t.pan,
            t.pan_mode,
            t.spread.unwrap_or(1.0),
        );
        let kind = match stack {
            Some(_) => instrument_kind(t.instrument.as_ref()),
            None => instrument_name(t.instrument.as_ref()),
        };
        let level = self.levels.get(&Some(t.id)).copied().unwrap_or_default();
        let roomy = lh >= 64.0;
        let fader_w = (self.header_w - 196.0).max(50.0);
        let pos = strip::fader_pos(volume);
        let db_text = if volume <= strip::SILENT + 0.01 {
            "−∞".to_string()
        } else {
            format!("{volume:+.1}").replace('-', "−")
        };
        let pan_text = {
            let side = strip::pan_label;
            match mode {
                PanMode::Balance => side(pan),
                PanMode::Stereo => format!(
                    "{}↔{:.0}",
                    if pan.abs() < 0.005 {
                        String::new()
                    } else {
                        side(pan)
                    },
                    spread * 100.0
                ),
                PanMode::Binaural => format!("3D {}", crate::binaural::azimuth_label(pan)),
            }
        };
        // The meter, in the fader's own scale: where each side's level would put the fader.
        let meter = |l: f32| strip::fader_pos(20.0 * l.max(1e-9).log10());
        let (ml, mr) = (meter(level.now[0]), meter(level.now[1]));
        let hot = |l: f32| {
            if l > 1.0 {
                METER_HIGH
            } else if l > 0.5 {
                METER_MID
            } else {
                METER_LOW
            }
        };
        // The fader: a rail filled in the track's colour up to a round cap, 0 dB marked, and the
        // meter, left over right, under it. Drag it (⇧ for fine), swipe sideways over it,
        // double-click to type a level, ⌥-click for 0 dB.
        let rail = fader_w - 14.0;
        let fader = div()
            .id(("fader", i))
            .w(px(fader_w))
            .h(px(22.))
            .flex_none()
            .relative()
            .cursor_ew_resize()
            .debug_selector(move || format!("track-fader-{i}"))
            .child(
                div()
                    .absolute()
                    .left(px(7.))
                    .top(px(6.))
                    .w(px(rail))
                    .h(px(5.))
                    .rounded(px(2.5))
                    .bg(rgb(0x26262e)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.))
                    .top(px(6.))
                    .w(px(rail * pos))
                    .h(px(5.))
                    .rounded(px(2.5))
                    .bg(rgb(color)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.0 + rail * strip::fader_pos(0.0) - 0.5))
                    .top(px(2.))
                    .h(px(13.))
                    .w(px(1.))
                    .bg(alpha(0xffffff, 0.28)),
            )
            .children(
                [(15.0, ml, level.now[0]), (18.5, mr, level.now[1])].map(|(top, m, l)| {
                    div()
                        .absolute()
                        .left(px(7.))
                        .top(px(top))
                        .h(px(2.))
                        .w(px(rail * m))
                        .rounded(px(1.))
                        .bg(rgb(hot(l)))
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(rail * pos))
                    .top(px(1.5))
                    .size(px(14.))
                    .rounded(px(7.))
                    .bg(rgb(0xf4f4f7))
                    .border_1()
                    .border_color(alpha(0x000000, 0.45))
                    .shadow_sm(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ws.select_track(i);
                    if e.modifiers.alt {
                        ws.edit(cx, |ws| ws.set_volume(Channel::Track(i), 0.0));
                        return;
                    }
                    if e.click_count == 2 {
                        ws.start_naming(Name::Volume(i), window, cx);
                        return;
                    }
                    let pos = strip::fader_pos(ws.session.project.tracks[i].volume_db);
                    let x = f32::from(e.position.x);
                    ws.begin_drag(
                        Drag::HFader {
                            track: i,
                            x,
                            pos,
                            width: rail,
                        },
                        cx,
                    );
                }),
            )
            .on_scroll_wheel(swipe(Name::Volume(i), cx));
        let readout = match self.name_field(Name::Volume(i)) {
            Some(field) => div().w(px(44.)).flex_none().child(field),
            None => div()
                .w(px(44.))
                .flex_none()
                .font_family(MONO)
                .text_size(px(10.5))
                .text_color(rgb(TEXT_2))
                .whitespace_nowrap()
                .debug_selector(move || format!("track-db-{i}"))
                .child(db_text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                        if e.click_count == 2 {
                            cx.stop_propagation();
                            ws.start_naming(Name::Volume(i), window, cx);
                        }
                    }),
                ),
        };
        // The pan knob: drag up or right to turn it right (⇧ fine; on Stereo Pan ⌘ spreads it),
        // swipe sideways, double-click to type it, ⌥-click for the middle, right-click for its
        // mode and the pan law.
        let pan_knob = div()
            .id(("pan", i))
            .flex()
            .items_center()
            .gap(px(5.))
            .cursor_ns_resize()
            .debug_selector(move || format!("track-pan-{i}"))
            .child(
                div()
                    .size(px(30.))
                    .flex_none()
                    .child(pan_dial(mode, pan, spread, color)),
            )
            .child(match self.name_field(Name::Pan(i)) {
                Some(field) => div().w(px(44.)).child(field),
                // Binaural: a button for the panner.
                None if mode == PanMode::Binaural => div()
                    .px(px(5.))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .bg(alpha(color, 0.16))
                    .border_1()
                    .border_color(alpha(color, 0.35))
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT))
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .hover(|s| s.bg(alpha(color, 0.3)))
                    .debug_selector(move || format!("track-3d-{i}"))
                    .child(pan_text)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.select_track(i);
                            ws.open_panner(i, cx);
                        }),
                    ),
                None => div()
                    .w(px(44.))
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_2))
                    .whitespace_nowrap()
                    .child(pan_text),
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ws.select_track(i);
                    let t = &ws.session.project.tracks[i];
                    if e.modifiers.alt {
                        ws.edit(cx, |ws| {
                            ws.set_pan(Channel::Track(i), 0.0);
                            let t = &mut ws.session.project.tracks[i];
                            (t.spread, t.elevation, t.distance) = (None, None, None);
                        });
                        return;
                    }
                    if e.click_count == 2 {
                        match t.pan_mode {
                            PanMode::Binaural => ws.open_panner(i, cx),
                            _ => ws.start_naming(Name::Pan(i), window, cx),
                        }
                        return;
                    }
                    let (x, y) = (f32::from(e.position.x), f32::from(e.position.y));
                    let drag = match (e.modifiers.platform, t.pan_mode) {
                        (true, PanMode::Stereo) => Drag::Spread {
                            track: i,
                            x,
                            y,
                            spread: t.spread.unwrap_or(1.0),
                        },
                        _ => Drag::Pan {
                            channel: Channel::Track(i),
                            x,
                            y,
                            pan: t.pan,
                        },
                    };
                    ws.begin_drag(drag, cx);
                }),
            )
            .on_scroll_wheel(swipe(Name::Pan(i), cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.pan_menu(i, e.position, cx);
                }),
            );
        let height = lh;
        div()
            .absolute()
            .top(px(top))
            .left_0()
            .w_full()
            .h(px(lh))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(6.))
            .pl(px(10.))
            .pr(px(10.))
            .bg(rgb(if selected { HEADER_SELECTED } else { HEADERS }))
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .debug_selector(move || format!("header-{i}"))
            // Its colour down its edge.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(3.))
                    .bg(alpha(color, if selected { 0.95 } else { 0.5 })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(16.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .font_family(MONO)
                            .text_size(px(10.5))
                            .text_color(rgb(if selected { TEXT_2 } else { TEXT_4 }))
                            .child((i + 1).to_string()),
                    )
                    // Its icon, in its colour: click to choose them.
                    .child(
                        div()
                            .id(("track-look", i))
                            .size(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .bg(alpha(color, 0.15))
                            .border_1()
                            .border_color(alpha(color, if picking { 0.8 } else { 0.28 }))
                            .cursor_pointer()
                            .hover(|d| d.bg(alpha(color, 0.26)))
                            .child(crate::looks::glyph(glyph, 16.).text_color(rgb(color)))
                            .debug_selector(move || format!("track-look-{i}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    ws.select_track(i);
                                    let at =
                                        e.position + gpui::point(gpui::px(24.), gpui::px(-12.));
                                    ws.open_picker(i, at, window, cx);
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(match self.name_field(Name::Track(i)) {
                                Some(field) => div().child(field),
                                None => div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(px(16.))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_color(rgb(if mute {
                                        TEXT_3
                                    } else if selected {
                                        0xffffff
                                    } else {
                                        0xe6e6ea
                                    }))
                                    .child(name)
                                    .debug_selector(move || format!("track-name-{i}"))
                                    // Double-click the name to rename the track.
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                                            if e.click_count == 2 {
                                                cx.stop_propagation();
                                                ws.select_track(i);
                                                ws.start_naming(Name::Track(i), window, cx);
                                            }
                                        }),
                                    ),
                            })
                            .when(lh >= 52.0, |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4.))
                                        .text_size(px(10.5))
                                        .line_height(px(13.))
                                        .text_color(rgb(TEXT_3))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .min_w(px(0.))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .child(kind),
                                        )
                                        .when_some(stack, |d, name| {
                                            d.child(div().flex_none().child(chip(name, VIOLET)))
                                        }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(3.))
                            .child(
                                toggle("M", mute, TEXT, BG, 20.)
                                    .debug_selector(move || format!("mute-{i}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.edit(cx, |ws| {
                                                ws.session.project.tracks[i].mute ^= true
                                            });
                                        }),
                                    ),
                            )
                            .child(
                                toggle("S", solo, SOLO, BG, 20.)
                                    .debug_selector(move || format!("solo-{i}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.edit(cx, |ws| {
                                                ws.session.project.tracks[i].solo ^= true
                                            });
                                        }),
                                    ),
                            )
                            .child(
                                toggle("R", armed == Some(i), RECORD, 0xffffff, 20.)
                                    .debug_selector(move || format!("arm-{i}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.toggle_arm(i);
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    ),
            )
            .when(roomy, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .pl(px(24.))
                        .child(fader)
                        .child(readout)
                        .child(pan_knob),
                )
            })
            // The bottom edge: drag the track taller or shorter; double-click for its usual size.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(-2.))
                    .h(px(6.))
                    .cursor_row_resize()
                    .hover(|d| d.bg(alpha(ACCENT, 0.35)))
                    .debug_selector(move || format!("track-edge-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count == 2 {
                                ws.session.project.tracks[i].height = None;
                                ws.session.edited = true;
                                cx.notify();
                                return;
                            }
                            ws.begin_drag(
                                Drag::LaneHeight {
                                    track: i,
                                    y: f32::from(e.position.y),
                                    height,
                                },
                                cx,
                            );
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.select_track(i);
                    ws.track_menu(i, e.position, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    ws.select_track(i);
                    ws.region = None;
                    cx.notify();
                }),
            )
    }

    fn lanes(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (ppt, sx, sy) = (self.px_per_tick(), self.scroll_x, self.scroll_y);
        let p = &self.session.project;
        let (beat_ticks, per_bar, bar_ticks) = (p.beat(), p.beats_per_bar(), p.bar());
        let tracks = self.session.project.tracks.len();
        let selected = self.track;
        let automation = self.show_automation;
        // Each row's top and its lane's height, for the grid.
        let rows_at: Vec<(f32, f32)> = (0..tracks)
            .map(|i| (self.row_top(i), self.lane_h(i)))
            .collect();
        let bounds = self.lanes_bounds.clone();
        let cycle = self
            .session
            .project
            .cycle
            .filter(|c| !c.off)
            .map(|c| (c.start as f32 * ppt - sx, c.end as f32 * ppt - sx));
        let grid = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                let w = f32::from(b.size.width);
                let quad = |window: &mut Window, x: f32, y: f32, ww: f32, hh: f32, c: Hsla| {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x + px(x), b.origin.y + px(y)),
                            size(px(ww), px(hh)),
                        ),
                        c,
                    ));
                };
                // Every other four bars a shade lighter, as FL Studio does.
                let group = ppt * (bar_ticks * 4) as f32;
                let mut k = (sx / group).floor() as i64;
                while (k as f32 * group - sx) < w {
                    if k % 2 == 1 {
                        quad(
                            window,
                            k as f32 * group - sx,
                            0.0,
                            group,
                            f32::from(b.size.height),
                            rgb(LANE_ALT).into(),
                        );
                    }
                    k += 1;
                }
                if let Some((x0, x1)) = cycle {
                    quad(
                        window,
                        x0,
                        0.0,
                        x1 - x0,
                        f32::from(b.size.height),
                        alpha(ACCENT, 0.04),
                    );
                }
                for (i, &(y, lh)) in rows_at.iter().enumerate() {
                    if i == selected {
                        quad(window, 0.0, y, w, lh, alpha(0xffffff, 0.025));
                    }
                    let row_h = lh + if automation { AUTO_H } else { 0.0 };
                    if automation {
                        quad(window, 0.0, y + lh, w, AUTO_H, rgb(0x0b0b0e).into());
                        quad(window, 0.0, y + lh - 1.0, w, 1.0, rgb(LINE_SOFT).into());
                    }
                    quad(window, 0.0, y + row_h - 1.0, w, 1.0, rgb(LINE_SOFT).into());
                }
                let _ = sy;
                let beat_px = ppt * beat_ticks as f32;
                let first = (sx / beat_px) as u64;
                let last = ((sx + w) / beat_px) as u64 + 1;
                for beat in first..=last {
                    let bar = beat % per_bar == 0;
                    if !bar && beat_px < 10.0 {
                        continue;
                    }
                    let color = if beat % (per_bar * 4) == 0 {
                        0x2a2a32
                    } else if bar {
                        0x202027
                    } else {
                        0x16161b
                    };
                    let x = (beat as f32 * beat_px - sx).round();
                    quad(
                        window,
                        x,
                        0.0,
                        1.0,
                        f32::from(b.size.height),
                        rgb(color).into(),
                    );
                }
            },
        )
        .absolute()
        .size_full();

        // Clicking an empty lane selects its track; the pencil draws a region there.
        let rows: Vec<Div> = (0..tracks)
            .map(|i| {
                div()
                    .absolute()
                    .top(px(self.row_top(i)))
                    .left_0()
                    .w_full()
                    .h(px(self.lane_h(i)))
                    .when(self.tool == Tool::Pencil, |d| d.cursor_crosshair())
                    .debug_selector(move || format!("lane-{i}"))
                    // An empty instrument track says how to start.
                    .when(
                        self.session.project.tracks[i].instrument.is_some()
                            && self.session.project.tracks[i].regions.is_empty(),
                        |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .left(px(12.))
                                    .top_0()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_4))
                                    .child("Double-click to write notes, or play your keyboard"),
                            )
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            ws.select_track(i);
                            ws.region = None;
                            // The pencil draws a region; so does a double-click, opening the
                            // piano roll on it to write in.
                            let x = f32::from(e.position.x);
                            if ws.tool == Tool::Pencil {
                                ws.draw_region(i, x, cx);
                            } else if e.click_count == 2 && ws.draw_region(i, x, cx).is_some() {
                                ws.show_bottom(Bottom::PianoRoll, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            ws.select_track(i);
                            ws.region = None;
                            let tick = ws.snap(ws.tick_at(f32::from(e.position.x)));
                            ws.lane_menu(i, tick, e.position, cx);
                        }),
                    )
            })
            .collect();
        let auto_rows: Vec<Div> = if automation {
            (0..tracks)
                .map(|i| {
                    div()
                        .absolute()
                        .top(px(self.row_top(i) + self.lane_h(i)))
                        .left_0()
                        .w_full()
                        .h(px(AUTO_H))
                        .cursor_crosshair()
                        .debug_selector(move || format!("auto-lane-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                ws.select_track(i);
                                ws.add_auto_point(i, e.position.map(f32::from), cx);
                            }),
                        )
                })
                .collect()
        } else {
            Vec::new()
        };

        let width = f32::from(self.lanes_bounds.get().size.width).max(2000.0);
        let mut regions = Vec::new();
        for (t, track) in self.session.project.tracks.iter().enumerate() {
            for r in 0..track.regions.len() {
                let region = &track.regions[r];
                let x = region.start as f32 * ppt - sx;
                let w = region.length as f32 * ppt;
                if x + w >= 0.0 && x <= width {
                    regions.push(self.region_view((t, r), cx));
                }
                regions.extend(self.loop_views((t, r), cx));
            }
        }

        let playhead = self.playhead() as f32 * ppt - sx;
        let lines = if automation {
            self.automation(cx)
        } else {
            Vec::new()
        };
        // The take being recorded, growing with the playhead.
        let take = self.recording.as_ref().and_then(|rec| {
            let t = self
                .session
                .project
                .tracks
                .iter()
                .position(|t| t.id == rec.track())?;
            let x = rec.start() as f32 * ppt - sx;
            Some(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(self.row_top(t) + 4.))
                    .w(px((playhead - x).max(2.)))
                    .h(px(self.lane_h(t) - 8.))
                    .rounded(px(6.))
                    .bg(alpha(RECORD, 0.3))
                    .border_1()
                    .border_color(rgb(RECORD)),
            )
        });
        div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(LANE))
            .child(grid)
            .children(rows)
            .children(auto_rows)
            .children(regions)
            .children(take)
            .children(lines)
            .children(self.agent_marker())
            .children(self.sample_drop_view())
            .children(self.file_drop_view(cx))
            .when(tracks == 0, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(40.))
                        .left(px(40.))
                        .text_color(rgb(TEXT_3))
                        .child("No tracks yet. Add one with + or ⌥⌘N."),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(px(playhead))
                    .top_0()
                    .w(px(1.))
                    .h_full()
                    .bg(rgb(TEXT)),
            )
            .on_scroll_wheel(self.on_scroll(cx))
    }

    /// Where the agent is working, while it is: an outline round what it touched, and its cursor.
    fn agent_marker(&self) -> Vec<AnyElement> {
        let Some(a) = self.agent_presence() else {
            return Vec::new();
        };
        let Some(t) = a.track.filter(|&t| t < self.session.project.tracks.len()) else {
            return Vec::new();
        };
        let ppt = self.px_per_tick();
        let top = self.row_top(t);
        let width = f32::from(self.lanes_bounds.get().size.width).max(200.0);
        let (x, w, y, h) = match a
            .region
            .and_then(|r| self.session.project.tracks[t].regions.get(r))
        {
            Some(r) => (
                r.start as f32 * ppt - self.scroll_x - 1.0,
                r.length as f32 * ppt + 2.0,
                top + 2.0,
                self.lane_h(t) - 4.0,
            ),
            None => (0.0, width, top, self.lane_h(t)),
        };
        let fade =
            1.0 - (a.at.elapsed().as_secs_f32() / crate::workspace::PRESENCE.as_secs_f32()).powi(3);
        let cx_ = (x + w - 34.0).clamp(4.0, width - 110.0);
        let cy = y + h / 2.0;
        vec![
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
                .rounded(px(8.))
                .border_2()
                .border_color(alpha(AGENT, fade))
                .into_any_element(),
            gpui::svg()
                .data(CURSOR)
                .absolute()
                .left(px(cx_))
                .top(px(cy - 4.))
                .size(px(16.))
                .text_color(alpha(AGENT, fade))
                .into_any_element(),
            div()
                .absolute()
                .left(px(cx_ + 12.))
                .top(px(cy + 10.))
                .px(px(8.))
                .py(px(3.))
                .rounded(px(7.))
                .rounded_tl(px(2.))
                .bg(alpha(AGENT, fade))
                .text_color(rgb(BG))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .child("tempo agent")
                .into_any_element(),
        ]
    }

    /// The pencil's click on an empty lane: a MIDI region over the bar clicked, on an instrument
    /// track.
    /// Where a new MIDI region for `tick` on `track` goes: the cycle's stretch if it's on and
    /// the tick is in it, else four bars from the tick's bar; kept clear of the track's other
    /// regions. None if one's there already.
    pub fn new_region_span(&self, track: usize, tick: u64) -> Option<(u64, u64)> {
        let p = &self.session.project;
        let bar = p.bar();
        let regions = &p.tracks.get(track)?.regions;
        if regions
            .iter()
            .any(|r| r.start <= tick && tick < r.start + r.length)
        {
            return None;
        }
        let (start, end) = match p
            .cycle
            .filter(|c| !c.off && c.start <= tick && tick < c.end)
        {
            Some(c) => (c.start, c.end),
            None => (tick / bar * bar, tick / bar * bar + 4 * bar),
        };
        let start = regions
            .iter()
            .map(|r| r.start + r.length)
            .filter(|&e| e <= tick)
            .fold(start, u64::max);
        let end = regions
            .iter()
            .map(|r| r.start)
            .filter(|&s| s > tick)
            .fold(end, u64::min);
        (end > start).then_some((start, end - start))
    }

    /// A new, empty MIDI region at `x` on an instrument track (see `new_region_span`),
    /// selected.
    pub fn draw_region(&mut self, track: usize, x: f32, cx: &mut Context<Self>) -> Option<At> {
        if self.session.project.tracks[track].instrument.is_none() {
            self.status = Some(
                "Audio tracks take recordings and audio files: drag one in from the Finder or the browser.".into(),
            );
            return None;
        }
        let (start, length) = self.new_region_span(track, self.tick_at(x) as u64)?;
        self.edit(cx, |ws| {
            let regions = &mut ws.session.project.tracks[track].regions;
            regions.push(Region {
                start,
                length,
                content: Content::Midi {
                    notes: Vec::new(),
                    controls: Vec::new(),
                },
                ..Default::default()
            });
            ws.region = Some((track, regions.len() - 1));
        });
        self.region
    }

    /// An audio region's warp geometry, once its file is loaded.
    pub fn warping(&self, at: At) -> Option<Warping> {
        let p = &self.session.project;
        let Content::Audio {
            file,
            offset,
            stretch,
            warps,
            ..
        } = &p.region(at).content
        else {
            return None;
        };
        let (hits, n) = self.player.hits(file, p.sample_rate as f64)?;
        let len = (n as f64 * stretch).round() as usize;
        let pins: Vec<(usize, usize)> = warps
            .iter()
            .map(|w| (w[0] as usize, w[1] as usize))
            .collect();
        let map = Map::new(n, len, &pins);
        let start = match pins.is_empty() {
            true => *offset as f64 * stretch,
            false => map.at(*offset as usize) as f64,
        };
        Some(Warping {
            map,
            hits,
            warps: warps.clone(),
            start,
            per_tick: p.sample_rate as f64 * 60.0 / (p.tempo * PPQ as f64),
        })
    }

    /// A hit (or a warp marker) of an audio region dragged to `x`: a warp marker there, on the
    /// sixteenth, the hits either side pinned where they play (as Logic's flex markers do), and
    /// kept between its neighbours so the audio stays in order.
    pub fn drag_warp(&mut self, at: At, from: u64, x: f32) {
        let Some(w) = self.warping(at) else {
            return;
        };
        let r = self.session.project.region(at);
        let (start, length) = (r.start as f64, r.length as f64);
        let g = self.grid().min(PPQ / 4) as f64;
        let tick = (self.tick_at(x) - start).clamp(0.0, length);
        let to = (w.start + (tick / g).round() * g * w.per_tick).round() as u64;
        let i = w.hits.iter().position(|&h| h as u64 == from);
        let pins: Vec<[u64; 2]> = [i.and_then(|i| i.checked_sub(1)), i.map(|i| i + 1)]
            .into_iter()
            .flatten()
            .filter_map(|j| w.hits.get(j))
            .map(|&h| [h as u64, w.map.at(h) as u64])
            .collect();
        let Content::Audio { warps, .. } =
            &mut self.session.project.tracks[at.0].regions[at.1].content
        else {
            return;
        };
        for p in pins {
            if !warps.iter().any(|m| m[0] == p[0]) {
                warps.push(p);
            }
        }
        warps.retain(|m| m[0] != from);
        let below = warps.iter().filter(|m| m[0] < from).map(|m| m[1] + 1).max();
        let above = warps
            .iter()
            .filter(|m| m[0] > from)
            .map(|m| m[1].saturating_sub(1))
            .min();
        let to = to.max(below.unwrap_or(0)).min(above.unwrap_or(u64::MAX));
        warps.push([from, to]);
        warps.sort();
    }

    /// The selected audio region's hits, as handles in its title strip to drag, and its warp
    /// markers, as lines down it (a double-click takes one out).
    fn warp_handles(&self, at: At, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(w) = self.warping(at) else {
            return Vec::new();
        };
        let r = self.session.project.region(at);
        let ppt = self.px_per_tick();
        let width = r.length as f64;
        let mut out = Vec::new();
        let marked: Vec<u64> = w.warps.iter().map(|m| m[0]).collect();
        let handle = |from: u64, marker: bool, x: f32, cx: &mut Context<Self>| {
            div()
                .absolute()
                .left(px(x - 5.))
                .top_0()
                .w(px(11.))
                .h(px(17.))
                .flex()
                .justify_center()
                .cursor_ew_resize()
                .debug_selector(move || format!("warp-{}-{}-{from}", at.0, at.1))
                .child(match marker {
                    true => div()
                        .mt(px(3.))
                        .size(px(9.))
                        .rounded(px(2.))
                        .bg(rgb(ACCENT)),
                    false => div()
                        .mt(px(5.))
                        .w(px(1.5))
                        .h(px(9.))
                        .bg(alpha(0xffffff, 0.6)),
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if marker && e.click_count == 2 {
                            ws.edit(cx, |ws| {
                                let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                                if let Content::Audio { warps, .. } = &mut r.content {
                                    warps.retain(|m| m[0] != from);
                                }
                            });
                            return;
                        }
                        ws.begin_drag(Drag::Warp { at, from }, cx);
                    }),
                )
                .into_any_element()
        };
        for m in &w.warps {
            let tick = w.tick(m[0] as usize);
            if (0.0..=width).contains(&tick) {
                let x = (tick * ppt as f64) as f32;
                out.push(
                    div()
                        .absolute()
                        .left(px(x - 1.))
                        .top_0()
                        .w(px(2.))
                        .h_full()
                        .bg(alpha(ACCENT, 0.8))
                        .into_any_element(),
                );
                out.push(handle(m[0], true, x, cx));
            }
        }
        for &h in w.hits.iter().filter(|&&h| !marked.contains(&(h as u64))) {
            let tick = w.tick(h);
            if (0.0..=width).contains(&tick) {
                out.push(handle(h as u64, false, (tick * ppt as f64) as f32, cx));
            }
        }
        out
    }

    fn region_view(&self, at: At, cx: &mut Context<Self>) -> Div {
        let project = &self.session.project;
        let track = &project.tracks[at.0];
        let region = &track.regions[at.1];
        let ppt = self.px_per_tick();
        let x = region.start as f32 * ppt - self.scroll_x;
        let w = (region.length as f32 * ppt).max(3.0);
        let y = self.row_top(at.0) + 4.0;
        let lh = self.lane_h(at.0);
        let color = self.color(at.0);
        let selected = self.region == Some(at);
        let muted = region.mute;
        let inside = self.inside(&region.content, region.length);
        let (start, end) = (region.start, region.start + region.length);
        let ink = Hsla::from(rgb(color));
        // Its start trims; its end's bottom half trims (⌥ time-stretches), its top half loops,
        // as Logic's region edges do.
        let half = (lh - 8.0) / 2.0;
        let handle = |edge: Edge, cx: &mut Context<Self>| {
            let h = div().absolute().w(px(6.)).cursor_ew_resize();
            let h = match edge {
                Edge::Start => h.left_0().top_0().h_full(),
                _ => h.right_0().bottom_0().h(px(half)),
            };
            h.on_mouse_down(MouseButton::Left, grab(at, edge, start, end, cx))
        };
        let group: SharedString = format!("region-{}-{}", at.0, at.1).into();
        let pill = |left: bool| {
            div()
                .absolute()
                .top(px((lh - 8.0) / 2.0 - 8.0))
                .when(left, |d| d.left(px(-1.)))
                .when(!left, |d| d.right(px(-1.)))
                .w(px(3.))
                .h(px(16.))
                .rounded(px(2.))
                .bg(rgb(0xffffff))
        };

        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(lh - 8.))
            .rounded(px(6.))
            .overflow_hidden()
            .when(muted, |d| d.opacity(0.38))
            .flex()
            .flex_col()
            .bg(alpha(color, if selected { 0.25 } else { 0.13 }))
            .border_1()
            .border_color(if selected {
                Hsla::from(rgb(0xffffff))
            } else {
                alpha(color, 0.33)
            })
            .when(track.mute, |d| d.opacity(0.4))
            .group(group.clone())
            .debug_selector(move || format!("region-{}-{}", at.0, at.1))
            .child({
                let ink = if selected {
                    0xffffff
                } else {
                    tint(color, 0.55)
                };
                div()
                    .h(px(17.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(7.))
                    .pt(px(2.))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_color(rgb(ink))
                    // An alias, as Logic shows one: its name in italics, and a link.
                    .when(region.link.is_some(), |d| {
                        d.italic().child(icon(Icon::Link, 11.).text_color(rgb(ink)))
                    })
                    .when(region.looped, |d| {
                        d.child(icon(Icon::Loop, 11.).text_color(rgb(ink)))
                    })
                    .child(region_name(&track.name, region))
            })
            .child(
                div().flex_1().relative().mx(px(1.)).mb(px(4.)).child(
                    canvas(
                        |_, _, _| {},
                        move |b, _, window, _| paint_inside(&inside, b, ppt, ink, window),
                    )
                    .absolute()
                    .size_full(),
                ),
            )
            .when(selected, |d| d.child(pill(true)).child(pill(false)))
            .child(handle(Edge::Start, cx))
            .child(handle(Edge::End, cx))
            .when(!region.looped || project.loop_end(at) <= end, |d| {
                d.child(loop_tab(at, (start, end), half, color, group.clone(), cx))
            })
            .when(selected, |d| d.children(self.warp_handles(at, cx)))
            .cursor(match self.tool {
                Tool::Pointer | Tool::Pencil => gpui::CursorStyle::OpenHand,
                Tool::Scissors => gpui::CursorStyle::IBeam,
                Tool::Eraser => gpui::CursorStyle::OperationNotAllowed,
            })
            .on_mouse_down(MouseButton::Left, grab(at, Edge::Move, start, end, cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.region = Some(at);
                    ws.select_track(at.0);
                    ws.region_menu(at, e.position, cx);
                }),
            )
    }

    /// A looped region's repeats, after it to where its loop stops: its content again, fainter.
    /// Clicking one selects the region.
    fn loop_views(&self, at: At, cx: &mut Context<Self>) -> Vec<Div> {
        let project = &self.session.project;
        let region = project.region(at);
        if !region.looped || region.length == 0 {
            return Vec::new();
        }
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let width = f32::from(self.lanes_bounds.get().size.width).max(2000.0);
        let until = project.loop_end(at);
        let color = self.color(at.0);
        let ink = alpha(color, 0.55);
        let y = self.row_top(at.0) + 4.0;
        let lh = self.lane_h(at.0);
        let mut out = Vec::new();
        let mut start = region.start + region.length;
        while start < until {
            let length = region.length.min(until - start);
            let (x, w) = (start as f32 * ppt - sx, length as f32 * ppt);
            if x > width {
                break;
            }
            if x + w >= 0.0 {
                let inside = self.inside(&region.content, length);
                out.push(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(w.max(3.0)))
                        .h(px(lh - 8.))
                        .rounded(px(6.))
                        .overflow_hidden()
                        .bg(alpha(color, 0.06))
                        .border_1()
                        .border_dashed()
                        .border_color(alpha(color, 0.3))
                        .debug_selector(move || format!("loop-{}-{}-{start}", at.0, at.1))
                        .child(
                            div()
                                .absolute()
                                .top(px(17.))
                                .left(px(1.))
                                .right(px(1.))
                                .bottom(px(4.))
                                .child(
                                    canvas(
                                        |_, _, _| {},
                                        move |b, _, window, _| {
                                            paint_inside(&inside, b, ppt, ink, window)
                                        },
                                    )
                                    .size_full(),
                                ),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.region = Some(at);
                                ws.select_track(at.0);
                                cx.notify();
                            }),
                        ),
                );
            }
            start += region.length;
        }
        // The last repeat's top right drags the loops out or back.
        let group: SharedString = format!("loops-{}-{}", at.0, at.1).into();
        if let Some(last) = out.pop() {
            let ends = (region.start, region.start + region.length);
            out.push(last.group(group.clone()).child(loop_tab(
                at,
                ends,
                (lh - 8.0) / 2.0,
                color,
                group,
                cx,
            )));
        }
        out
    }

    /// Each track's automation for its chosen parameter, in the lane under it: a line over a
    /// shaded area, and points to drag.
    fn automation(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let mut out = Vec::new();
        for (t, track) in self.session.project.tracks.iter().enumerate() {
            let param = self.lane_param(t);
            let points: Vec<AutoPoint> = track.automation.points(param).to_vec();
            let level = match param {
                Param::Volume => track.volume_db,
                Param::Pan => track.pan,
                Param::Plugin(which, id) => self.player.param_value(track.id, which, id) as f32,
            };
            let off = track.automation.mode == AutoMode::Off;
            let color = self.color(t);
            let top = self.row_top(t) + self.lane_h(t);
            let shown = points.clone();
            out.push(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let y = |v: f32| b.origin.y + px(top + auto_to_y(param, v));
                        let x = |tick: u64| b.origin.x + px(tick as f32 * ppt - sx);
                        let (first, last) = match (shown.first(), shown.last()) {
                            (Some(f), Some(l)) => (f.value, l.value),
                            _ => (level, level),
                        };
                        let right = b.origin.x + b.size.width;
                        let bottom = b.origin.y + px(top + AUTO_H - 1.0);
                        let mut area = PathBuilder::fill();
                        area.move_to(point(b.origin.x, bottom));
                        area.line_to(point(b.origin.x, y(first)));
                        for p in &shown {
                            area.line_to(point(x(p.tick), y(p.value)));
                        }
                        area.line_to(point(right, y(last)));
                        area.line_to(point(right, bottom));
                        area.close();
                        let faint = if shown.is_empty() || off { 0.35 } else { 1.0 };
                        if let Ok(p) = area.build() {
                            window.paint_path(p, alpha(color, 0.12 * faint));
                        }
                        let mut path = PathBuilder::stroke(px(1.5));
                        path.move_to(point(b.origin.x, y(first)));
                        for p in &shown {
                            path.line_to(point(x(p.tick), y(p.value)));
                        }
                        path.line_to(point(right, y(last)));
                        if let Ok(path) = path.build() {
                            window.paint_path(path, alpha(color, faint));
                        }
                    },
                )
                .absolute()
                .size_full()
                .into_any_element(),
            );
            for (i, p) in points.iter().enumerate() {
                let x = p.tick as f32 * ppt - sx;
                out.push(
                    div()
                        .absolute()
                        .left(px(x - 4.5))
                        .top(px(top + auto_to_y(param, p.value) - 4.5))
                        .size(px(9.))
                        .rounded(px(5.))
                        .bg(rgb(0x0b0b0e))
                        .border_2()
                        .border_color(rgb(color))
                        .cursor_move()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                ws.select_track(t);
                                if e.modifiers.alt {
                                    ws.edit(cx, |ws| {
                                        let a = &mut ws.session.project.tracks[t].automation;
                                        a.points_mut(param).remove(i);
                                        a.tidy();
                                    });
                                } else {
                                    let drag = Drag::Auto {
                                        track: t,
                                        param,
                                        index: i,
                                        moved: false,
                                    };
                                    ws.begin_drag(drag, cx);
                                }
                            }),
                        )
                        .into_any_element(),
                );
            }
        }
        out
    }

    /// Adds an automation point where the pointer is, and starts dragging it.
    pub fn add_auto_point(&mut self, track: usize, p: Point<f32>, cx: &mut Context<Self>) {
        let param = self.lane_param(track);
        let point = AutoPoint {
            tick: self.snap(self.tick_at(p.x)),
            value: self.auto_value(track, param, p.y),
        };
        self.session.checkpoint();
        let name = self.param_name(track, param);
        let automation = &mut self.session.project.tracks[track].automation;
        let index = automation.add(param, point);
        automation.name(param, &name);
        self.drag = Some(Drag::Auto {
            track,
            param,
            index,
            moved: true,
        });
        self.changed(cx);
    }
}

const PLAYHEAD: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 11 12"><path fill="#000" d="M1.5 0h8a1.5 1.5 0 0 1 1.5 1.5v4.6a1.5 1.5 0 0 1-.5 1.1L5.5 12 .5 7.2A1.5 1.5 0 0 1 0 6.1V1.5A1.5 1.5 0 0 1 1.5 0z"/></svg>"##;
const CURSOR: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path fill="#000" d="M1.5 1.2l12.8 7.4-5.6 1.5-2.6 5.4z"/></svg>"##;

/// Starts dragging a region (`start` and `end` are its ticks when the drag starts), or does what
/// the tool does to it.
/// A sideways swipe over a header's fader or pan knob nudges it; up and down scroll the tracks.
fn swipe(
    what: Name,
    cx: &mut Context<Workspace>,
) -> impl Fn(&gpui::ScrollWheelEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |ws, e: &gpui::ScrollWheelEvent, _, cx| {
        let d = e.delta.pixel_delta(px(16.));
        let (dx, dy) = (f32::from(d.x), f32::from(d.y));
        if dx.abs() > dy.abs() {
            cx.stop_propagation();
            ws.swipe(what, dx, cx);
        }
    })
}

/// The top half of a region's end (or of its last repeat's): drag it to loop the region out to
/// there, as Logic's loop tool does. A loop sign shows on hover.
fn loop_tab(
    at: At,
    (start, end): (u64, u64),
    half: f32,
    color: u32,
    group: SharedString,
    cx: &mut Context<Workspace>,
) -> Div {
    div()
        .absolute()
        .top_0()
        .right_0()
        .w(px(18.))
        .h(px(half))
        .flex()
        .justify_end()
        .pt(px(2.))
        .pr(px(2.))
        .cursor(gpui::CursorStyle::ResizeRight)
        .debug_selector(move || format!("loop-tab-{}-{}", at.0, at.1))
        .child(
            div()
                .size(px(14.))
                .rounded(px(4.))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(color))
                .opacity(0.)
                .group_hover(group, |s| s.opacity(1.))
                .child(icon(Icon::Loop, 10.).text_color(rgb(0xffffff))),
        )
        .on_mouse_down(MouseButton::Left, grab(at, Edge::Loop, start, end, cx))
}

fn grab(
    at: At,
    edge: Edge,
    start: u64,
    end: u64,
    cx: &mut Context<Workspace>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
        cx.stop_propagation();
        match ws.tool {
            Tool::Eraser => {
                ws.edit(cx, |ws| {
                    ws.session.project.tracks[at.0].regions.remove(at.1);
                    ws.region = None;
                });
                return;
            }
            Tool::Scissors => {
                let tick = ws.snap(ws.tick_at(f32::from(e.position.x)));
                ws.select_track(at.0);
                ws.split_at(at, tick, cx);
                return;
            }
            Tool::Pointer | Tool::Pencil => {}
        }
        if ws.region != Some(at) {
            ws.picked.clear();
        }
        ws.region = Some(at);
        ws.select_track(at.0);
        // Double-clicking a region opens it: MIDI in the piano roll, a pattern in the step
        // sequencer, a take folder its takes.
        if e.click_count == 2 && edge == Edge::Move {
            let pane = match ws.session.project.region(at).content {
                Content::Takes { .. } => Some(Bottom::Takes),
                Content::Pattern { .. } => Some(Bottom::Steps),
                Content::Midi { .. } => Some(Bottom::PianoRoll),
                Content::Audio { .. } => None,
            };
            if let Some(pane) = pane {
                ws.show_bottom(pane, cx);
                return;
            }
        }
        // Option-dragging a region's end time-stretches it, as in Logic: audio flexes, MIDI
        // and patterns spread their notes or steps. A take folder only trims.
        let content = &ws.session.project.region(at).content;
        let edge = match (edge, content) {
            (Edge::End, Content::Takes { .. }) => Edge::End,
            (Edge::End, _) if e.modifiers.alt => Edge::Stretch,
            _ => edge,
        };
        ws.stretching = match (edge, content) {
            (Edge::Stretch, Content::Midi { .. } | Content::Pattern { .. }) => {
                Some(content.clone())
            }
            _ => None,
        };
        let from = e.position.map(f32::from);
        // Option-dragging a region drags a copy of it; with shift too, an alias, as in Logic.
        let copy = edge == Edge::Move && e.modifiers.alt;
        let drag = Drag::Region {
            at,
            edge,
            from,
            start,
            end,
            track: at.0,
            moved: false,
            copy,
        };
        ws.begin_drag(drag, cx);
        if copy {
            let project = &mut ws.session.project;
            let made = match e.modifiers.shift {
                true => project.alias(at, at.0, start),
                false => {
                    let mut r = project.region(at).clone();
                    r.link = None;
                    project.tracks[at.0].regions.push(r);
                    Ok((at.0, project.tracks[at.0].regions.len() - 1))
                }
            };
            match made {
                Ok(copy) => {
                    if let Some(Drag::Region { at, .. }) = &mut ws.drag {
                        *at = copy;
                    }
                    ws.region = Some(copy);
                    ws.changed(cx);
                }
                Err(e) => {
                    ws.drag = None;
                    ws.session.cancel();
                    ws.status = Some(e);
                    cx.notify();
                }
            }
        }
    })
}

impl Workspace {
    /// What a region's content shows, `length` ticks of it.
    pub fn inside(&self, content: &Content, length: u64) -> Inside {
        let mut inside = Inside::default();
        match content {
            Content::Takes { takes, comp } => {
                for (from, to, t) in comp_segments(comp, takes.len(), length) {
                    self.add_inside(&mut inside, &takes[t].content, takes[t].start, from, to);
                }
            }
            content => self.add_inside(&mut inside, content, 0, 0, length),
        }
        inside
    }

    /// Adds what `content`, beginning `at` ticks into a region, shows from `from` to `to` ticks
    /// into it.
    pub fn add_inside(&self, inside: &mut Inside, content: &Content, at: i64, from: u64, to: u64) {
        let project = &self.session.project;
        let ppt = self.px_per_tick();
        match content {
            Content::Midi { .. } | Content::Pattern { .. } => {
                let notes = content.notes((to as i64 - at).max(0) as u64).unwrap();
                let shown = notes
                    .iter()
                    .filter(|n| (from as i64..to as i64).contains(&(at + n.start)));
                inside
                    .notes
                    .extend(shown.map(|n| (at + n.start, n.length, n.pitch)));
            }
            Content::Audio {
                file,
                offset,
                stretch,
                warps,
                ..
            } => {
                let begin = (from as i64).max(at);
                let Some(peaks) = self.player.peaks(file).filter(|_| begin < to as i64) else {
                    return;
                };
                let per_tick = project.sample_rate as f64 * 60.0 / (project.tempo * PPQ as f64);
                // Warped, a piece of waveform for each stretch of it between the markers.
                let n = peaks.len_samples();
                let pins: Vec<(usize, usize)> = warps
                    .iter()
                    .map(|w| (w[0] as usize, w[1] as usize))
                    .collect();
                let map = Map::new(n, (n as f64 * stretch).round() as usize, &pins);
                let start = match pins.is_empty() {
                    true => *offset as f64 * stretch,
                    false => map.at(*offset as usize) as f64,
                };
                for w in map.points.windows(2) {
                    let ((i0, o0), (i1, o1)) = (w[0], w[1]);
                    let tick = |o: usize| at as f64 + (o as f64 - start) / per_tick;
                    let (t0, t1) = (tick(o0), tick(o1));
                    let (a, b) = (t0.max(begin as f64), t1.min(to as f64));
                    if a >= b {
                        continue;
                    }
                    // In the file's own samples: flexed audio covers fewer per tick.
                    let samples_per_tick = (i1 - i0) as f64 / (t1 - t0).max(1e-9);
                    inside.waves.push(Wave {
                        x0: a as f32 * ppt,
                        x1: b as f32 * ppt,
                        peaks: peaks.clone(),
                        offset: (i0 as f64 + (a - t0) * samples_per_tick) as f32,
                        samples_per_px: (samples_per_tick / ppt as f64) as f32,
                    });
                }
            }
            Content::Takes { .. } => {}
        }
    }
}

/// Draws a region's notes and waveforms in its track's color.
pub fn paint_inside(
    inside: &Inside,
    b: Bounds<Pixels>,
    ppt: f32,
    color: Hsla,
    window: &mut Window,
) {
    let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
    let mut bar = |x: f32, y: f32, bw: f32, bh: f32, c: Hsla| {
        let at = point(b.origin.x + px(x), b.origin.y + px(y));
        window.paint_quad(fill(Bounds::new(at, size(px(bw), px(bh))), c).corner_radii(px(1.)));
    };
    if let Some(lo) = inside.notes.iter().map(|n| n.2).min() {
        let hi = inside.notes.iter().map(|n| n.2).max().unwrap();
        // At least an octave tall, centered on the notes.
        let span = (hi - lo).max(12) as f32 + 1.0;
        let top = (lo as f32 + hi as f32) / 2.0 + span / 2.0;
        let row = (h - 2.0) / span;
        for &(start, len, pitch) in &inside.notes {
            let (x, nw) = (start as f32 * ppt, (len as f32 * ppt - 1.0).max(1.5));
            if x + nw > 0.0 && x < w {
                let y = 1.0 + (top - pitch as f32 - 0.5) * row;
                bar(
                    x.max(0.0),
                    y,
                    nw.min(w - x.max(0.0)),
                    row.clamp(2.0, 3.0),
                    color,
                );
            }
        }
    }
    let mid = h / 2.0;
    let wave = color.opacity(0.85);
    for wv in &inside.waves {
        for x in wv.x0.max(0.0) as usize..wv.x1.min(w) as usize {
            let from = wv.offset + (x as f32 - wv.x0) * wv.samples_per_px;
            if from as usize >= wv.peaks.len_samples() {
                break;
            }
            let (lo, hi) = wv
                .peaks
                .range(from as usize, (from + wv.samples_per_px) as usize);
            let (y1, y2) = (mid - hi * mid, mid - lo * mid);
            bar(x as f32, y1, 1.0, (y2 - y1).max(1.0), wave);
        }
    }
}

/// A pan knob, drawn as its mode pans: balance a dot round the dial; stereo pan the arc its two
/// sides span; binaural the dial with a head at its middle.
fn pan_dial(mode: PanMode, pan: f32, spread: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let r = f32::from(b.size.width.min(b.size.height)) / 2.0 - 2.0;
            let at = |deg: f32, rr: f32| {
                point(
                    c.x + px(rr * deg.to_radians().cos()),
                    c.y + px(rr * deg.to_radians().sin()),
                )
            };
            // -1 to 1 round the dial's top, 135° either way.
            let deg = |p: f32| 270.0 + p.clamp(-1.0, 1.0) * 135.0;
            let arc = |from: f32, to: f32, width: f32, window: &mut Window, ink: Hsla| {
                if (to - from).abs() < 0.5 {
                    return;
                }
                let mut p = gpui::PathBuilder::stroke(px(width));
                let steps = ((to - from).abs() / 6.0).ceil().max(2.0) as usize;
                for k in 0..=steps {
                    let d = from + (to - from) * k as f32 / steps as f32;
                    if k == 0 {
                        p.move_to(at(d, r));
                    } else {
                        p.line_to(at(d, r));
                    }
                }
                if let Ok(p) = p.build() {
                    window.paint_path(p, ink);
                }
            };
            // Binaural: from above, the head in the middle facing up, the sound round it.
            if mode == PanMode::Binaural {
                let ring = |rr: f32, ink: Hsla, width: f32, window: &mut Window| {
                    let mut p = gpui::PathBuilder::stroke(px(width));
                    for k in 0..=48 {
                        let d = k as f32 * 7.5;
                        match k {
                            0 => p.move_to(at(d, rr)),
                            _ => p.line_to(at(d, rr)),
                        }
                    }
                    if let Ok(p) = p.build() {
                        window.paint_path(p, ink);
                    }
                };
                ring(r, rgb(RAISED).into(), 2.0, window);
                ring(r * 0.62, alpha(0xffffff, 0.07), 1.0, window);
                let head = r * 0.3;
                window.paint_quad(
                    fill(
                        Bounds::centered_at(c, size(px(head * 2.0), px(head * 2.0))),
                        rgb(0x3b3b48),
                    )
                    .corner_radii(px(head)),
                );
                window.paint_quad(
                    fill(
                        Bounds::centered_at(at(270.0, head), size(px(3.), px(3.))),
                        alpha(ACCENT, 0.9),
                    )
                    .corner_radii(px(1.5)),
                );
                let d = deg(pan) + (pan.clamp(-1.0, 1.0) * 45.0);
                let dot = at(d, r);
                let mut beam = gpui::PathBuilder::stroke(px(1.));
                beam.move_to(c);
                beam.line_to(dot);
                if let Ok(p) = beam.build() {
                    window.paint_path(p, alpha(color, 0.5));
                }
                window.paint_quad(
                    fill(Bounds::centered_at(dot, size(px(7.), px(7.))), rgb(color))
                        .corner_radii(px(3.5)),
                );
                return;
            }
            arc(deg(-1.0), deg(1.0), 2.5, window, rgb(RAISED).into());
            match mode {
                PanMode::Stereo => {
                    let (a, z) = (deg(pan - spread.abs()), deg(pan + spread.abs()));
                    arc(a.min(z), a.max(z), 2.5, window, rgb(color).into());
                    for d in [deg(pan - spread), deg(pan + spread)] {
                        window.paint_quad(
                            fill(
                                Bounds::centered_at(at(d, r), size(px(5.), px(5.))),
                                rgb(0xffffff),
                            )
                            .corner_radii(px(2.5)),
                        );
                    }
                }
                _ => {
                    let (a, z) = (deg(0.0), deg(pan));
                    arc(a.min(z), a.max(z), 2.5, window, rgb(color).into());
                    window.paint_quad(
                        fill(
                            Bounds::centered_at(at(deg(pan), r), size(px(5.), px(5.))),
                            rgb(0xffffff),
                        )
                        .corner_radii(px(2.5)),
                    );
                }
            }
            // The middle: a disc.
            let body = r * 0.55;
            window.paint_quad(
                fill(
                    Bounds::centered_at(c, size(px(body * 2.0), px(body * 2.0))),
                    rgb(0x2a2a32),
                )
                .corner_radii(px(body)),
            );
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_values_and_lane_heights_round_trip() {
        for (param, v) in [
            (Param::Volume, -12.0),
            (Param::Volume, 6.0),
            (Param::Pan, -0.5),
        ] {
            assert!((auto_from_y(param, auto_to_y(param, v)) - v).abs() < 1e-3);
        }
        assert_eq!(auto_to_y(Param::Pan, -1.0), AUTO_PAD); // left at the top
    }

    #[test]
    fn samplers_are_named_for_what_they_play() {
        use tempo_engine::project::{Sampler, Zone};
        let zone = |file: &str, high| Zone {
            file: file.into(),
            note: 36,
            high,
            ..Zone::default()
        };
        let multi = Sampler {
            zones: vec![
                zone("Moog_Bass_C1.wav", Some(36)),
                zone("Moog_Bass_A#2.wav", Some(46)),
            ],
            ..Default::default()
        };
        assert_eq!(sampler_name(&multi), "Moog Bass");
        let mixed = Sampler {
            zones: vec![zone("Sub.wav", Some(48)), zone("Bass.wav", Some(60))],
            ..Default::default()
        };
        assert_eq!(sampler_name(&mixed), "2 zones");
        let kit = Sampler {
            zones: vec![zone("Kick.wav", None), zone("Snare.wav", None)],
            ..Default::default()
        };
        assert_eq!(sampler_name(&kit), "2-piece kit");
        let kit = Some(Instrument::Sampler(kit));
        assert_eq!(instrument_kind(kit.as_ref()), "Kit");
    }
}
