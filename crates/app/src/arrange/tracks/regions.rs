//! Regions in their lanes: their names, what they show inside, their loops, and the handles to drag.

use super::*;

/// What a region shows inside.
#[derive(Default)]
pub struct Inside {
    /// Start (ticks from the region's start), length and pitch.
    notes: Vec<(i64, u64, u8)>,
    waves: Vec<Wave>,
}

/// A stretch of waveform: from `x0` to `x1` pixels into the region, `offset` the sample at `x0`.
pub(crate) struct Wave {
    x0: f32,
    x1: f32,
    peaks: Arc<Peaks>,
    offset: f32,
    samples_per_px: f32,
}

/// What a region is called on screen.
pub(crate) fn region_name(track: &str, region: &Region) -> String {
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
pub fn sampler_name(s: &pina_engine::song::project::Sampler) -> String {
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
pub(crate) fn multisample_name(s: &pina_engine::song::project::Sampler) -> Option<String> {
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

/// Starts dragging a region (`start` and `end` are its ticks when the drag starts), or does what
/// the tool does to it.
/// A sideways swipe over a header's fader or pan knob nudges it; up and down scroll the tracks.
pub(crate) fn swipe(
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
/// there. A loop sign shows on hover.
pub(crate) fn loop_tab(
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

pub(crate) fn grab(
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
        // Option-dragging a region's end time-stretches it: audio flexes, MIDI
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
        // Option-dragging a region drags a copy of it; with shift too, an alias.
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

impl Workspace {
    pub(crate) fn region_view(&self, at: At, cx: &mut Context<Self>) -> Div {
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
        // Its start trims; its end's bottom half trims (⌥ time-stretches), its top half loops.
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
                        // An alias: its name in italics, and a link.
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
            // Its fades: the corners darkened above an equal-power curve.
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
    pub(crate) fn loop_views(&self, at: At, cx: &mut Context<Self>) -> Vec<Div> {
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
