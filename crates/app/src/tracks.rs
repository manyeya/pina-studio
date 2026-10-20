//! The tracks area: tools and track headers on the left; markers, the ruler (with the cycle) and
//! the lanes with their regions on the right. Regions move by dragging their body, and trim by
//! dragging their edges; the tools add, cut and erase them.

use std::sync::Arc;

use gpui::{
    AnyElement, App, Bounds, BoxShadow, Context, Div, FontWeight, Hsla, MouseButton,
    MouseDownEvent, PathBuilder, Pixels, Point, SharedString, Window, canvas, div, fill,
    linear_color_stop, linear_gradient, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    peaks::Peaks,
    project::{At, Content, Instrument, PPQ, PanMode, Param, Region, comp_segments},
    stretch::Map,
};

use crate::{
    menu::{MARKER_NAMES, Menu, MenuAction},
    strip::{self, Channel},
    theme::*,
    workspace::{Bottom, Drag, Edge, Name, Snap, Tool, Workspace},
};

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
pub fn sampler_name(s: &pina_engine::project::Sampler) -> String {
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
fn multisample_name(s: &pina_engine::project::Sampler) -> Option<String> {
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
                    .h(px(TOP_H))
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
            .child({
                // The four always to hand, then the one other in hand (or the rest's menu): Text,
                // Glue, Solo, Mute, Zoom, Fade, Flex, as Logic's Tool menu (T).
                let mains: Vec<_> = Tool::ALL
                    .into_iter()
                    .filter(|t| t.main())
                    .map(|t| tool(t.icon(), t, self.tool == t, cx))
                    .collect();
                let other = (!self.tool.main()).then_some(self.tool);
                let more = div()
                    .h(px(24.))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(1.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(other.is_some(), |d| d.bg(rgb(RAISED)))
                    .when(other.is_none(), |d| d.hover(|d| d.bg(rgb(OFF))))
                    .debug_selector(|| "tool-more".into())
                    .child(
                        icon(other.map_or(Icon::More, |t| t.icon()), 14.)
                            .text_color(rgb(if other.is_some() { TEXT } else { TEXT_3 })),
                    )
                    .child(icon(Icon::Chevron, 10.).text_color(rgb(TEXT_3)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.tool_menu(e.position, cx);
                        }),
                    );
                tray().p(px(2.)).rounded(px(8.)).children(mains).child(more)
            })
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
            if self.show_automation {
                rows.extend(self.auto_headers(i, cx));
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
        // Where its automation has them, if it plays any.
        let (name, mute, solo, volume, pan, mode, spread) = (
            t.name.clone(),
            self.muted_now(i),
            t.solo,
            self.now_value(i, Param::Volume),
            self.now_value(i, Param::Pan),
            t.pan_mode,
            t.spread.unwrap_or(1.0),
        );
        let kind = match stack {
            Some(_) => instrument_kind(t.instrument.as_ref()),
            None => instrument_name(t.instrument.as_ref()),
        };
        let level = self.levels.get(&Some(t.id)).copied().unwrap_or_default();
        let shown = self.settings.clone();
        let roomy = lh >= 64.0 && shown.header_strip;
        let pos = strip::fader_pos(volume);
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
                PanMode::Binaural => crate::binaural::azimuth_label(pan),
            }
        };
        // The readout shows the level, or the pan while it's turned.
        let panning = matches!(
            self.drag,
            Some(Drag::Pan { channel: Channel::Track(t), .. } | Drag::Spread { track: t, .. }) if t == i
        );
        let readout_text = match panning {
            true => pan_text,
            false if volume <= strip::SILENT + 0.01 => "−∞".to_string(),
            false => format!("{volume:+.1}").replace('-', "−"),
        };
        // Automated, the readout lights in its mode's colour.
        let marked = match panning {
            true => self.auto_mark(i, Param::Pan),
            false => self.auto_mark(i, Param::Volume),
        };
        let lit = marked.unwrap_or(tint(color, 0.55));

        // The strip's buttons: dark until on, then lit in their colour.
        let button = |label: &'static str, on: bool, glow: u32, ink: u32| {
            div()
                .w(px(24.))
                .h(px(18.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .text_size(px(9.5))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(rgb(glow))
                        .border_color(rgb(glow))
                        .text_color(rgb(ink))
                        .shadow(vec![BoxShadow {
                            color: alpha(glow, 0.55),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(8.),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                })
                .when(!on, |d| {
                    d.bg(rgb(HEADERS))
                        .border_color(rgb(RAISED))
                        .text_color(rgb(0x6a6a75))
                        .hover(|s| s.text_color(rgb(TEXT_2)))
                })
                .child(label)
        };
        let buttons = div()
            .flex()
            .gap(px(4.))
            .child(
                button("M", mute, ACCENT, 0xffffff)
                    .debug_selector(move || format!("mute-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| {
                                let m = !ws.muted_now(i);
                                ws.session.project.tracks[i].mute = m;
                                ws.moved_once(i, Param::Mute, m as u8 as f32);
                            });
                        }),
                    ),
            )
            .child(
                button("S", solo, SOLO, BG)
                    .debug_selector(move || format!("solo-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.edit(cx, |ws| ws.session.project.tracks[i].solo ^= true);
                        }),
                    ),
            )
            .when(shown.header_record, |d| {
                d.child(
                    button("R", armed == Some(i), RECORD, 0xffffff)
                        .debug_selector(move || format!("arm-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.toggle_arm(i);
                                cx.notify();
                            }),
                        ),
                )
            });
        // The level, left over right, as LEDs: green to -7 dB, amber, then red at 0.
        let (now, held) = (level.now, level.held);
        let leds = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                const AT: [f32; 12] = [
                    -42.0, -36.0, -30.0, -24.0, -18.0, -14.0, -10.0, -7.0, -4.5, -2.5, -1.0, 0.0,
                ];
                let w = f32::from(b.size.width);
                let seg = (w - 11.0) / 12.0;
                for (row, (l, h)) in [(now[0], held[0]), (now[1], held[1])]
                    .into_iter()
                    .enumerate()
                {
                    let (db, peak) = (20.0 * l.max(1e-9).log10(), 20.0 * h.max(1e-9).log10());
                    let top = (AT.iter().rposition(|&a| peak >= a)).filter(|_| h > 1e-6);
                    for (k, &at) in AT.iter().enumerate() {
                        let on = db >= at || top == Some(k);
                        let ink = match (on, k) {
                            (false, _) => rgb(0x2a2a31),
                            (true, 0..=7) => rgb(METER_LOW),
                            (true, 8..=9) => rgb(METER_MID),
                            _ => rgb(METER_HIGH),
                        };
                        window.paint_quad(fill(
                            Bounds::new(
                                point(
                                    b.origin.x + px(k as f32 * (seg + 1.0)),
                                    b.origin.y + px(row as f32 * 5.0),
                                ),
                                size(px(seg), px(3.)),
                            ),
                            ink,
                        ));
                    }
                }
            },
        )
        .flex_1()
        .h(px(8.));

        // The fader: a groove filled in the track's colour, a scale under it, and a ridged
        // cap. Drag it (⇧ for fine), swipe sideways over it, double-click to type a level,
        // ⌥-click for 0 dB.
        const FADER_W: f32 = 98.0;
        let ticks = [strip::SILENT, -24.0, -12.0, -6.0, 0.0, 6.0].map(|db| {
            div()
                .absolute()
                .left(px(
                    (FADER_W * strip::fader_pos(db) - 0.5).clamp(0.0, FADER_W - 1.0)
                ))
                .top(px(16.))
                .w(px(1.))
                .h(px(4.))
                .bg(rgb(if db == 0.0 { TEXT_3 } else { 0x3a3a44 }))
        });
        let fader = div()
            .id(("fader", i))
            .w(px(FADER_W))
            .h(px(22.))
            .flex_none()
            .relative()
            .cursor_ew_resize()
            .debug_selector(move || format!("track-fader-{i}"))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(9.))
                    .h(px(3.))
                    .rounded(px(1.5))
                    .bg(rgb(0x0b0b0e)),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(9.))
                    .w(px(FADER_W * pos))
                    .h(px(3.))
                    .rounded(px(1.5))
                    .bg(alpha(color, if mute { 0.35 } else { 0.8 })),
            )
            .children(ticks)
            .child(self.modded_fader(i, FADER_W, 22.))
            // The cap, as the mixer's are, stood on end.
            .child(
                div()
                    .absolute()
                    .left(px((FADER_W * pos - 7.0).clamp(0.0, FADER_W - 14.0)))
                    .top(px(1.))
                    .w(px(14.))
                    .h(px(20.))
                    .rounded(px(4.))
                    .bg(rgb(0x3a3a44))
                    .border_1()
                    .border_color(rgb(0x4e4e5a))
                    .shadow_md()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().w(px(1.)).h(px(12.)).bg(rgb(TEXT))),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ws.select_track(i);
                    if e.modifiers.alt {
                        ws.edit(cx, |ws| {
                            ws.set_volume(Channel::Track(i), 0.0);
                            ws.moved_once(i, Param::Volume, 0.0);
                        });
                        return;
                    }
                    if e.click_count == 2 {
                        ws.start_naming(Name::Volume(i), window, cx);
                        return;
                    }
                    let pos = strip::fader_pos(ws.now_value(i, Param::Volume));
                    let x = f32::from(e.position.x);
                    ws.begin_drag(
                        Drag::HFader {
                            track: i,
                            x,
                            pos,
                            width: FADER_W,
                        },
                        cx,
                    );
                }),
            )
            .on_scroll_wheel(swipe(Name::Volume(i), cx));
        // The readout, an LCD: the level (double-click to type one), or the pan while it's
        // turned; a pan typed shows here too.
        let field = self
            .name_field(Name::Volume(i))
            .or_else(|| self.name_field(Name::Pan(i)));
        let readout = div()
            .w(px(40.))
            .h(px(18.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .bg(rgb(0x0b0b0e))
            .border_1()
            .border_color(match marked {
                Some(c) => alpha(c, 0.45),
                None => rgb(0x22222a).into(),
            })
            .font_family(MONO)
            .text_size(px(10.))
            .text_color(rgb(lit))
            .whitespace_nowrap()
            .overflow_hidden()
            .debug_selector(move || format!("track-db-{i}"))
            .child(match field {
                Some(field) => div().w_full().px(px(2.)).child(field),
                None => div().child(readout_text),
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    if e.click_count == 2 {
                        cx.stop_propagation();
                        ws.start_naming(Name::Volume(i), window, cx);
                    }
                }),
            );
        // The pan knob, its LED ring showing where: drag up or right to turn it right (⇧ fine;
        // ⌘ spreads a stereo pan), swipe sideways, double-click to type it (binaural: its
        // panner), ⌥-click for the middle, right-click for its mode and the pan law.
        let pan_knob = div()
            .id(("pan", i))
            .size(px(24.))
            .flex_none()
            .cursor_ns_resize()
            .debug_selector(move || format!("track-pan-{i}"))
            .child(self.modded(
                Some((i, pina_engine::modulate::ModTarget::Pan)),
                24.,
                pan_dial(mode, pan, spread, color),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ws.select_track(i);
                    let shown_pan = ws.now_value(i, Param::Pan);
                    let t = &ws.session.project.tracks[i];
                    if e.modifiers.alt {
                        ws.edit(cx, |ws| {
                            ws.set_pan(Channel::Track(i), 0.0);
                            ws.moved_once(i, Param::Pan, 0.0);
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
                            pan: shown_pan,
                        },
                    };
                    ws.begin_drag(drag, cx);
                    cx.notify();
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

        // The strip: a raised panel, its buttons and LEDs over its fader, readout and knob.
        let strip_panel = div()
            .w(px(190.))
            .h(px(if roomy { 60. } else { 28. }))
            .flex_none()
            .mr(px(8.))
            .px(px(8.))
            .py(px(if roomy { 6. } else { 4. }))
            .flex()
            .flex_col()
            .justify_between()
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(RAISED))
            .bg(linear_gradient(
                180.,
                linear_color_stop(rgb(0x1e1e24), 0.),
                linear_color_stop(rgb(0x17171c), 1.),
            ))
            .shadow(vec![BoxShadow {
                color: alpha(0xffffff, 0.05),
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            }])
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(buttons)
                    .when(shown.header_meter, |d| d.child(leds)),
            )
            .when(roomy, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(fader)
                        .child(readout)
                        .child(pan_knob),
                )
            });

        // Who it is: its icon (click for its colour and icon), number and name (double-click to
        // rename), and what it plays.
        let icon_size = if roomy { 36. } else { 26. };
        let identity = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(13.))
            .pr(px(8.))
            .when(mute, |d| d.opacity(0.55))
            .when(shown.header_icons, |d| {
                d.child(
                    div()
                        .id(("track-look", i))
                        .size(px(icon_size))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(icon_size / 4.0))
                        .bg(alpha(color, 0.14))
                        .border_1()
                        .border_color(alpha(color, if picking { 0.8 } else { 0.34 }))
                        .cursor_pointer()
                        .hover(|d| d.bg(alpha(color, 0.26)))
                        .child(crate::looks::glyph(glyph, icon_size * 0.52).text_color(rgb(color)))
                        .debug_selector(move || format!("track-look-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, e: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                ws.select_track(i);
                                let at = e.position + gpui::point(gpui::px(24.), gpui::px(-12.));
                                ws.open_picker(i, at, window, cx);
                            }),
                        ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(6.))
                            .min_w(px(0.))
                            .when(shown.header_numbers, |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .font_family(MONO)
                                        .text_size(px(10.))
                                        .text_color(rgb(if selected { TEXT_3 } else { TEXT_4 }))
                                        .child(format!("{:02}", i + 1)),
                                )
                            })
                            .child(match self.name_field(Name::Track(i)) {
                                Some(field) => div().flex_1().child(field),
                                None => div()
                                    .min_w(px(0.))
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(px(17.))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_color(rgb(if selected { 0xffffff } else { 0xe6e6ea }))
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
                            }),
                    )
                    .when(lh >= 52.0 && shown.header_kind, |d| {
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
            );

        let height = lh;
        div()
            .absolute()
            .top(px(top))
            .left_0()
            .w_full()
            .h(px(lh))
            .flex()
            .items_center()
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
            .child(identity)
            .child(strip_panel)
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
        let rows_at: Vec<(f32, f32, f32)> = (0..tracks)
            .map(|i| (self.row_top(i), self.lane_h(i), self.autos_h(i)))
            .collect();
        let auto_h = self.auto_h();
        let bounds = self.lanes_bounds.clone();
        let cycle = self
            .session
            .project
            .cycle
            .filter(|c| !c.off)
            .map(|c| (c.start as f32 * ppt - sx, c.end as f32 * ppt - sx));
        let (shading, beat_lines) = (self.settings.bar_shading, self.settings.beat_lines);
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
                while shading && (k as f32 * group - sx) < w {
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
                for (i, &(y, lh, ah)) in rows_at.iter().enumerate() {
                    if i == selected {
                        quad(window, 0.0, y, w, lh, alpha(0xffffff, 0.025));
                    }
                    let row_h = lh + ah;
                    if automation {
                        quad(window, 0.0, y + lh, w, ah, rgb(0x0b0b0e).into());
                        quad(window, 0.0, y + lh - 1.0, w, 1.0, rgb(LINE_SOFT).into());
                        let mut k = auto_h;
                        while k < ah - 1.0 {
                            quad(window, 0.0, y + lh + k - 1.0, w, 1.0, rgb(0x17171c).into());
                            k += auto_h;
                        }
                    }
                    quad(window, 0.0, y + row_h - 1.0, w, 1.0, rgb(LINE_SOFT).into());
                }
                let _ = sy;
                let beat_px = ppt * beat_ticks as f32;
                let first = (sx / beat_px) as u64;
                let last = ((sx + w) / beat_px) as u64 + 1;
                for beat in first..=last {
                    let bar = beat % per_bar == 0;
                    if !bar && (beat_px < 10.0 || !beat_lines) {
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
                            && self.session.project.tracks[i].regions.is_empty()
                            && self.settings.double_click_writes,
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
                            if ws.tool == Tool::Zoom {
                                return ws.zoom_press(e, cx);
                            }
                            // The pencil draws a region; so does a double-click, opening the
                            // piano roll on it to write in.
                            let x = f32::from(e.position.x);
                            if ws.tool == Tool::Pencil {
                                ws.draw_region(i, x, cx);
                            } else if e.click_count == 2
                                && ws.settings.double_click_writes
                                && ws.draw_region(i, x, cx).is_some()
                            {
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
            self.auto_views(cx)
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
            .children(regions)
            .children(take)
            .children(lines)
            .children(self.agent_marker())
            // The Zoom tool's drag: the bars it'll fill the window with.
            .children(self.zoom_shown().map(|(x, w)| {
                div()
                    .absolute()
                    .top_0()
                    .left(px(x))
                    .w(px(w))
                    .h_full()
                    .bg(alpha(ACCENT, 0.12))
                    .border_l_1()
                    .border_r_1()
                    .border_color(alpha(ACCENT, 0.7))
            }))
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
                .child("Pina Studio agent")
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
        let zone = self.settings.loop_zone;
        let handle = |edge: Edge, cx: &mut Context<Self>| {
            let h = div().absolute().w(px(6.)).cursor_ew_resize();
            let h = match (edge, zone) {
                (Edge::Start, _) | (_, false) => h.top_0().h_full(),
                _ => h.bottom_0().h(px(half)),
            };
            let h = match edge {
                Edge::Start => h.left_0(),
                _ => h.right_0(),
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
            .bg(alpha(
                color,
                match (selected, self.settings.region_bright) {
                    (true, true) => 0.45,
                    (false, true) => 0.33,
                    (true, false) => 0.25,
                    (false, false) => 0.13,
                },
            ))
            .border_1()
            .border_color(if selected {
                Hsla::from(rgb(0xffffff))
            } else {
                alpha(color, 0.33)
            })
            .when(track.mute, |d| d.opacity(0.4))
            .group(group.clone())
            .debug_selector(move || format!("region-{}-{}", at.0, at.1))
            .when(self.settings.region_names, |d| {
                d.child({
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
            })
            .when(self.settings.region_contents, |d| {
                d.child(
                    div()
                        .flex_1()
                        .relative()
                        .mx(px(1.))
                        .mb(px(4.))
                        .when(!self.settings.region_names, |d| d.mt(px(4.)))
                        .child(
                            canvas(
                                |_, _, _| {},
                                move |b, _, window, _| paint_inside(&inside, b, ppt, ink, window),
                            )
                            .absolute()
                            .size_full(),
                        ),
                )
            })
            // Its fades, as Logic draws them: the corners darkened above an equal-power curve.
            .when(region.fade_in > 0 || region.fade_out > 0, |d| {
                let (fade_in, fade_out, length) = (region.fade_in, region.fade_out, region.length);
                d.child(
                    canvas(
                        |_, _, _| {},
                        move |b, _, window, _| {
                            let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                            let at = |x: f32, y: f32| point(px(x0 + x), px(y0 + y));
                            let mut fade = |from: f32, span: f32, rising: bool| {
                                if span < 1.0 {
                                    return;
                                }
                                let n = 24;
                                let curve: Vec<(f32, f32)> = (0..=n)
                                    .map(|i| {
                                        let u = i as f32 / n as f32;
                                        let g = match rising {
                                            true => (u * std::f32::consts::FRAC_PI_2).sin(),
                                            false => {
                                                ((1.0 - u) * std::f32::consts::FRAC_PI_2).sin()
                                            }
                                        };
                                        (from + u * span, h * (1.0 - g))
                                    })
                                    .collect();
                                let mut shade = PathBuilder::fill();
                                shade.move_to(at(from, 0.0));
                                for &(x, y) in &curve {
                                    shade.line_to(at(x, y));
                                }
                                shade.line_to(at(from + span, 0.0));
                                shade.close();
                                if let Ok(p) = shade.build() {
                                    window.paint_path(p, alpha(0x000000, 0.45));
                                }
                                let mut line = PathBuilder::stroke(px(1.5));
                                for (i, &(x, y)) in curve.iter().enumerate() {
                                    match i {
                                        0 => line.move_to(at(x, y)),
                                        _ => line.line_to(at(x, y)),
                                    }
                                }
                                if let Ok(p) = line.build() {
                                    window.paint_path(p, alpha(0xffffff, 0.75));
                                }
                            };
                            let k = w / length.max(1) as f32;
                            fade(0.0, fade_in as f32 * k, true);
                            fade(w - fade_out as f32 * k, fade_out as f32 * k, false);
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            })
            .when(selected, |d| d.child(pill(true)).child(pill(false)))
            .child(handle(Edge::Start, cx))
            .child(handle(Edge::End, cx))
            .when(
                self.settings.loop_zone && (!region.looped || project.loop_end(at) <= end),
                |d| d.child(loop_tab(at, (start, end), half, color, group.clone(), cx)),
            )
            .when(selected, |d| d.children(self.warp_handles(at, cx)))
            .cursor(match self.tool {
                Tool::Pointer | Tool::Pencil | Tool::Flex => gpui::CursorStyle::OpenHand,
                Tool::Scissors | Tool::Text => gpui::CursorStyle::IBeam,
                Tool::Eraser => gpui::CursorStyle::OperationNotAllowed,
                Tool::Fade => gpui::CursorStyle::ResizeLeftRight,
                Tool::Zoom => gpui::CursorStyle::Crosshair,
                Tool::Glue | Tool::Solo | Tool::Mute => gpui::CursorStyle::PointingHand,
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
            Tool::Text => return ws.name_region(at, cx),
            Tool::Glue => return ws.glue(at, cx),
            Tool::Mute => {
                ws.edit(cx, |ws| {
                    let r = &mut ws.session.project.tracks[at.0].regions[at.1];
                    r.mute ^= true;
                });
                return;
            }
            Tool::Solo => {
                let tick = ws.tick_at(f32::from(e.position.x)) as u64;
                return ws.solo_hold(at, tick, cx);
            }
            Tool::Zoom => return ws.zoom_press(e, cx),
            Tool::Fade => return ws.fade_press(at, e, cx),
            Tool::Pointer | Tool::Pencil | Tool::Flex => {}
        }
        ws.auto_picked = None;
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
            (Edge::End, _) if e.modifiers.alt || ws.tool == Tool::Flex => Edge::Stretch,
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
            let r = f32::from(b.size.width.min(b.size.height)) / 2.0 - 1.5;
            let at = |deg: f32, rr: f32| {
                point(
                    c.x + px(rr * deg.to_radians().cos()),
                    c.y + px(rr * deg.to_radians().sin()),
                )
            };
            let arc = |from: f32, to: f32, ink: Hsla, window: &mut Window| {
                if (to - from).abs() < 0.5 {
                    return;
                }
                let mut p = gpui::PathBuilder::stroke(px(2.));
                let steps = ((to - from).abs() / 6.0).ceil().max(2.0) as usize;
                for k in 0..=steps {
                    let d = from + (to - from) * k as f32 / steps as f32;
                    match k {
                        0 => p.move_to(at(d, r)),
                        _ => p.line_to(at(d, r)),
                    }
                }
                if let Ok(p) = p.build() {
                    window.paint_path(p, ink);
                }
            };
            let dot = |p: Point<Pixels>, d: f32, ink: Hsla, window: &mut Window| {
                window.paint_quad(
                    fill(Bounds::centered_at(p, size(px(d), px(d))), ink).corner_radii(px(d / 2.0)),
                );
            };
            let body = r * 0.62;
            let knob = |window: &mut Window| {
                window.paint_quad(
                    fill(
                        Bounds::centered_at(c, size(px(body * 2.0), px(body * 2.0))),
                        rgb(0x2e2e36),
                    )
                    .corner_radii(px(body))
                    .border_widths(px(1.))
                    .border_color(rgb(0x0b0b0e)),
                );
            };
            // -1 to 1 round the top, 135° either way; binaural all the way round.
            let deg = |p: f32| 270.0 + p.clamp(-1.0, 1.0) * 135.0;
            let ink: Hsla = rgb(color).into();
            match mode {
                PanMode::Binaural => {
                    arc(0.0, 360.0, rgb(0x26262d).into(), window);
                    knob(window);
                    dot(c, body * 0.7, rgb(0x4a4a58).into(), window);
                    dot(
                        at(270.0 + pan.clamp(-1.0, 1.0) * 180.0, r),
                        5.0,
                        ink,
                        window,
                    );
                }
                PanMode::Stereo => {
                    arc(deg(-1.0), deg(1.0), rgb(0x26262d).into(), window);
                    let (a, z) = (deg(pan - spread.abs()), deg(pan + spread.abs()));
                    arc(a.min(z), a.max(z), ink, window);
                    knob(window);
                    for d in [deg(pan - spread), deg(pan + spread)] {
                        let mut p = gpui::PathBuilder::stroke(px(1.6));
                        p.move_to(at(d, body * 0.35));
                        p.line_to(at(d, body * 0.85));
                        if let Ok(p) = p.build() {
                            window.paint_path(p, Hsla::from(rgb(TEXT)));
                        }
                    }
                }
                PanMode::Balance => {
                    arc(deg(-1.0), deg(1.0), rgb(0x26262d).into(), window);
                    let (a, z) = (deg(0.0), deg(pan));
                    arc(a.min(z), a.max(z), ink, window);
                    // The middle, marked.
                    dot(
                        at(270.0, r + 0.2),
                        2.4,
                        if pan.abs() < 0.005 {
                            ink
                        } else {
                            rgb(0x3a3a44).into()
                        },
                        window,
                    );
                    knob(window);
                    let mut p = gpui::PathBuilder::stroke(px(1.6));
                    p.move_to(at(deg(pan), body * 0.3));
                    p.line_to(at(deg(pan), body * 0.85));
                    if let Ok(p) = p.build() {
                        window.paint_path(p, Hsla::from(rgb(TEXT)));
                    }
                }
            }
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samplers_are_named_for_what_they_play() {
        use pina_engine::project::{Sampler, Zone};
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
