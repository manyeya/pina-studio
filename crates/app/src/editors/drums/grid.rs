//! The pads: four by four, the lowest note at the bottom left, as drum machines lay them out;
//! beside them every note a kit can use, the sixteen shown framed.

use super::*;

/// The grid's gaps, and the note map's width.
const GAP: f32 = 6.0;
const MAP_W: f32 = 14.0;

impl Workspace {
    pub(crate) fn drum_grid(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let from = self.drums.from.min(128 - PADS);
        let w = self.header_w;
        let pad_w = ((w - 24.0 - MAP_W - 8.0 - GAP * 3.0) / 4.0).floor();
        // Top row first: the bank's last four notes.
        let rows: Vec<Div> = (0..4u8)
            .rev()
            .map(|row| {
                div().flex().gap(px(GAP)).children(
                    (0..4u8).map(|col| self.drum_cell(kit, from + row * 4 + col, pad_w, cx)),
                )
            })
            .collect();
        let shift = |label: &'static str, i: Icon, by: i32, cx: &mut Context<Self>| {
            icon_seg(i, false, 22.)
                .debug_selector(move || format!("drums-bank-{label}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        let to = ws.drums.from as i32 + by;
                        ws.drums.from = to.clamp(0, (128 - PADS) as i32) as u8;
                        cx.notify();
                    }),
                )
        };
        // The kit's pads that aren't shown (an 808 on its own note, below C1): how many, either
        // side, the nearest a click away.
        let notes: Vec<u8> = {
            let p = &self.session.project;
            p.pads_of(kit)
                .into_iter()
                .filter_map(|t| p.tracks[t].pad.map(|p| p.note))
                .collect()
        };
        let out = |dir: &'static str, cx: &mut Context<Self>| {
            let (them, glyph, help): (Vec<u8>, _, _) = match dir {
                "below" => (
                    notes.iter().copied().filter(|&n| n < from).collect(),
                    Icon::Chevron,
                    "Pads below these: click for the nearest",
                ),
                _ => (
                    notes
                        .iter()
                        .copied()
                        .filter(|&n| n >= from + PADS)
                        .collect(),
                    Icon::ChevronUp,
                    "Pads above these: click for the nearest",
                ),
            };
            let n = match dir {
                "below" => them.iter().max(),
                _ => them.iter().min(),
            }
            .copied()?;
            // The bank with it on its edge row, nearest the one shown.
            let nearest = match dir {
                "below" => n / 4 * 4,
                _ => (n / 4 * 4).saturating_sub(PADS - 4),
            };
            let color = self.pad_color(kit, n);
            Some(
                div()
                    .h(px(20.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(5.))
                    .bg(rgb(FIELD))
                    .hover(|d| d.bg(rgb(RAISED)))
                    .cursor_pointer()
                    .font_family(MONO)
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_2))
                    .child(div().size(px(5.)).rounded(px(3.)).bg(rgb(color)))
                    .child(them.len().to_string())
                    .child(icon(glyph, 8.).text_color(rgb(TEXT_3)))
                    .debug_selector(move || format!("drums-out-{dir}"))
                    .help(help, cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.drums.from = nearest.min(128 - PADS);
                            cx.notify();
                        }),
                    ),
            )
        };
        let (below, above) = (out("below", cx), out("above", cx));
        let head = div()
            .h(px(24.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(10.5))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(TEXT_3))
                    .child("PADS"),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_2))
                    .child(format!(
                        "{} – {}",
                        note_name(from),
                        note_name(from + PADS - 1)
                    )),
            )
            .children(below)
            .children(above)
            .child(div().flex_1())
            .child(shift("down", Icon::Minus, -(PADS as i32), cx))
            .child(shift("up", Icon::Plus, PADS as i32, cx));
        div()
            .w(px(w))
            .flex_none()
            .h_full()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .border_r_1()
            .border_color(rgb(LINE))
            .bg(rgb(HEADERS))
            .child(head)
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(self.note_map(kit, from, cx))
                    .child(div().flex().flex_col().gap(px(GAP)).children(rows)),
            )
    }

    /// Every note's place, four a row, those with pads lit in their colour and the sixteen shown
    /// framed: click it to show the notes there.
    fn note_map(&self, kit: usize, from: u8, cx: &mut Context<Self>) -> impl IntoElement {
        let p = &self.session.project;
        let lit: Vec<(u8, u32)> = p
            .pads_of(kit)
            .into_iter()
            .filter_map(|t| p.tracks[t].pad.map(|pad| pad.note))
            .map(|n| (n, self.pad_color(kit, n)))
            .collect();
        let map = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let row = h / 32.0;
                let cell = (w - 3.0) / 4.0;
                for &(n, color) in &lit {
                    let (r, c) = ((n / 4) as f32, (n % 4) as f32);
                    window.paint_quad(fill(
                        Bounds::new(
                            point(
                                b.origin.x + px(c * (cell + 1.0)),
                                b.origin.y + px(h - (r + 1.0) * row + 0.5),
                            ),
                            size(px(cell), px((row - 1.0).max(1.0))),
                        ),
                        alpha(color, 0.9),
                    ));
                }
                let top = h - (from as f32 / 4.0 + 4.0) * row;
                for (y, hh) in [(top, 1.0), (top + 4.0 * row - 1.0, 1.0)] {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(b.origin.x - px(2.), b.origin.y + px(y)),
                            size(px(w + 4.0), px(hh)),
                        ),
                        rgb(TEXT_2),
                    ));
                }
            },
        )
        .size_full();
        div()
            .w(px(MAP_W))
            .flex_none()
            .h(px(4.0 * self.pad_height() + 3.0 * GAP))
            .rounded(px(3.))
            .bg(rgb(BG))
            .cursor_pointer()
            .debug_selector(|| "drums-map".into())
            .child(map)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    let Some(top) = ws.drums.map_top.get() else {
                        return;
                    };
                    let h = 4.0 * ws.pad_height() + 3.0 * GAP;
                    let y = (f32::from(e.position.y) - top).clamp(0.0, h);
                    let row = ((h - y) / h * 32.0).floor() as i32 - 2;
                    ws.drums.from = (row * 4).clamp(0, (128 - PADS) as i32) as u8;
                    cx.notify();
                }),
            )
            .child(
                canvas(
                    {
                        let cell = self.drums.map_top.clone();
                        move |b, _, _| cell.set(Some(f32::from(b.origin.y)))
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }

    /// How tall a pad is: the grid fills the pane under its head.
    pub(crate) fn pad_height(&self) -> f32 {
        let room = self.editor_height() - EDITOR_BAR_H - 24.0 - 8.0 - 24.0;
        ((room - 3.0 * GAP) / 4.0).clamp(40.0, 72.0).floor()
    }

    /// One pad: its name, note and outline, lit as it plays; empty, a place to drop a sound.
    fn drum_cell(&mut self, kit: usize, note: u8, w: f32, cx: &mut Context<Self>) -> Div {
        let h = self.pad_height();
        let picked = self.drums.pad == note;
        let pad = self
            .drum_pad(kit, note)
            .map(|(t, s)| (t, s.zones.first().cloned(), s.zones.len()));
        let (cells, frame) = (self.drums.cells.clone(), self.drums.frame);
        let place = canvas(
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
        .size_full();
        // What's being dragged onto it: a pad from elsewhere, or a sample.
        let target = match self.drag {
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
        };
        let lifted = matches!(self.drag, Some(Drag::Pad { note: n, moved: true, .. }) if n == note);
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
            if ws.drum_pad(kit, note).is_some() {
                ws.hit_pad(kit, note, velocity);
            }
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
        let base = div()
            .relative()
            .w(px(w))
            .h(px(h))
            .flex_none()
            .rounded(px(8.))
            .overflow_hidden()
            .cursor_pointer()
            .debug_selector(move || format!("pad-{note}"))
            .child(place);
        let Some((t, zone, layers)) = pad else {
            return base
                .border_1()
                .border_dashed()
                .border_color(match target {
                    Some(c) => rgb(c).into(),
                    None if picked => alpha(TEXT_2, 0.8),
                    None => alpha(0xffffff, 0.12),
                })
                .bg(match target {
                    Some(c) => alpha(c, 0.12),
                    None => alpha(0xffffff, 0.015),
                })
                .hover(|d| d.bg(alpha(0xffffff, 0.04)))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(Icon::Plus, 14.).text_color(rgb(TEXT_4))),
                )
                .child(
                    div()
                        .absolute()
                        .right(px(6.))
                        .bottom(px(4.))
                        .font_family(MONO)
                        .text_size(px(9.))
                        .text_color(rgb(TEXT_4))
                        .child(note_name(note)),
                )
                .on_mouse_down(MouseButton::Left, press);
        };
        let track = &self.session.project.tracks[t];
        let (name, muted, solo, choke) = (
            track.name.clone(),
            track.mute,
            track.solo,
            track.pad.and_then(|p| p.choke),
        );
        let color = self.pad_color(kit, note);
        // Lit as it plays, and flashed as it's struck by hand.
        let level = self
            .levels
            .get(&Some(track.id))
            .map_or(0.0, |l| l.now[0].max(l.now[1]));
        let playing = ((20.0 * level.max(1e-5).log10() + 40.0) / 40.0).clamp(0.0, 1.0);
        let flash = self.drums.struck.get(&note).map_or(0.0, |(at, v)| {
            v * (1.0 - at.elapsed().as_secs_f32() / 0.25).max(0.0)
        });
        let glow = playing.max(flash);
        let outline = zone
            .as_ref()
            .and_then(|z| {
                let file = z.file.clone();
                self.wave(&file, cx)
            })
            .map(|w| w.outline(28));
        let wave = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let Some(o) = &outline else {
                    return;
                };
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let step = w / o.len() as f32;
                for (i, v) in o.iter().enumerate() {
                    let bar = (v * h).max(1.0);
                    window.paint_quad(fill(
                        Bounds::new(
                            point(
                                b.origin.x + px(i as f32 * step),
                                b.origin.y + px((h - bar) / 2.0),
                            ),
                            size(px((step - 1.0).max(1.0)), px(bar)),
                        ),
                        alpha(color, 0.55),
                    ));
                }
            },
        )
        .absolute()
        .left(px(8.))
        .right(px(8.))
        .bottom(px(16.))
        .h(px((h * 0.32).clamp(12.0, 22.0)));
        base.bg(rgb(0x15151a))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(alpha(color, 0.10 + 0.45 * glow)),
            )
            .border_1()
            .border_color(match (target, picked) {
                (Some(c), _) => rgb(c).into(),
                (None, true) => rgb(color).into(),
                (None, false) => alpha(color, 0.28),
            })
            .when_some(target, |d, c| {
                d.child(div().absolute().inset_0().bg(alpha(c, 0.22)))
            })
            .when(picked, |d| d.shadow_md())
            .when(muted || lifted, |d| d.opacity(0.45))
            .hover(|d| d.border_color(alpha(color, 0.7)))
            .child(wave)
            .child(
                div()
                    .absolute()
                    .top(px(6.))
                    .left(px(8.))
                    .right(px(6.))
                    .flex()
                    .items_start()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(11.))
                            .line_height(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .max_h(px(26.))
                            .overflow_hidden()
                            .child(name),
                    )
                    .when(solo, |d| {
                        d.child(
                            div()
                                .text_size(px(9.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(SOLO))
                                .child("S"),
                        )
                    }),
            )
            // Its choke group, a little numbered tag; how many sounds it layers; and its note.
            .child(
                div()
                    .absolute()
                    .left(px(7.))
                    .bottom(px(4.))
                    .flex()
                    .gap(px(3.))
                    .children(choke.map(|c| tag(c.to_string(), color)))
                    .when(layers > 1, |d| {
                        d.child(
                            tag(layers.to_string(), color)
                                .gap(px(2.))
                                .child(icon(Icon::Stack, 7.).text_color(rgb(TEXT_3)))
                                .flex_row_reverse(),
                        )
                    }),
            )
            .child(
                div()
                    .absolute()
                    .right(px(6.))
                    .bottom(px(3.))
                    .font_family(MONO)
                    .text_size(px(9.))
                    .text_color(rgb(TEXT_3))
                    .child(note_name(note)),
            )
            .on_mouse_down(MouseButton::Left, press)
    }
}

/// A little tag on a pad: its choke group, its layers.
fn tag(label: String, color: u32) -> Div {
    div()
        .h(px(13.))
        .min_w(px(13.))
        .px(px(3.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .border_1()
        .border_color(alpha(color, 0.6))
        .font_family(MONO)
        .text_size(px(8.5))
        .text_color(rgb(TEXT_2))
        .child(label)
}
