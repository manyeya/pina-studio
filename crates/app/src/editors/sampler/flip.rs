//! Flip: a chopped sample rearranged, as a beatmaker does on an MPC. Shown in the Sampler for a
//! sampler sliced into chops (Slice to Sampler, or Slice in the Sounds browser) with a MIDI
//! region playing them. Along the top, its chops, each its waveform and number: click one to hear
//! it, drag it onto the sequence to put it there. Under them, the sequence: the region's notes as
//! the chops they play, each its waveform, in time: drag one onto another to swap them; click one
//! to hear and pick it, then stutter it, play it backwards or silence it. Flip makes a new
//! arrangement every press (the one on the one, chops said again and answered); the whole turns
//! round, goes half or double time, or back to how it was. Every change is an edit, undone like
//! any.

use super::*;

use super::modules::module;
use pina_engine::song::project::{At, Content, chop_notes, cuts_of};

/// The colours chops go by, in turn.
pub(crate) const HUES: [u32; 8] = [CORAL, AMBER, MINT, SKY, VIOLET, PINK, TEAL, SLATE];

/// The sequence's height.
const LANE_H: f32 = 72.0;

/// What's being dragged: a chop in the sequence (its place), or one of the chops (its note).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChopFrom {
    Slot(usize),
    Chop(u8),
}

/// A chop's waveform, drawn into whatever holds it, in its colour.
fn chop_wave(wave: Option<Arc<Wave>>, view: (f32, f32), hue: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            if let Some(w) = &wave {
                paint_wave(window, w, b, view, 1.0, |_| {
                    (alpha(hue, 0.45), alpha(hue, 0.95))
                });
            }
        },
    )
    .absolute()
    .size_full()
}

impl Workspace {
    /// The MIDI region a flip works on: the picked region on track `t` if it's MIDI, else its
    /// first.
    pub(crate) fn flip_region(&self, t: usize) -> Option<At> {
        let track = self.session.project.tracks.get(t)?;
        let midi = |r: usize| matches!(track.regions[r].content, Content::Midi { .. });
        match self.region {
            Some((rt, r)) if rt == t && r < track.regions.len() && midi(r) => Some((t, r)),
            _ => (0..track.regions.len()).find(|&r| midi(r)).map(|r| (t, r)),
        }
    }

    /// A chop heard through the track's own sampler, as it's set.
    fn hear_chop(&mut self, t: usize, note: u8) {
        let id = self.session.project.tracks[t].id;
        self.player.preview(id, note, 110);
        // Chops are one-shots: let go, they play out.
        self.player.preview(id, note, 0);
    }

    /// The `i`th chop of the sequence, in its place and what it plays.
    fn chop_at(&self, region: At, i: usize) -> Option<(i64, u64, u8)> {
        let k = *self.session.project.chop_order(region).get(i)?;
        match &self.session.project.region(region).content {
            Content::Midi { notes, .. } => notes.get(k).map(|n| (n.start, n.length, n.pitch)),
            _ => None,
        }
    }

    /// The place in the sequence under `p` (a window position), if it's on a chop.
    fn chop_slot_at(&self, region: At, p: Point<f32>) -> Option<usize> {
        let b = self.sampler_ui.flip_lane.get();
        let (left, top) = (f32::from(b.origin.x), f32::from(b.origin.y));
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if w <= 0.0 || p.x < left || p.x > left + w || p.y < top - 16.0 || p.y > top + h + 16.0 {
            return None;
        }
        let length = self.session.project.region(region).length.max(1) as f32;
        let tick = ((p.x - left) / w * length) as i64;
        let order = self.session.project.chop_order(region);
        (0..order.len()).find(|&i| {
            self.chop_at(region, i)
                .is_some_and(|(start, len, _)| tick >= start && tick < start + len as i64)
        })
    }

    /// A change to the flip, as one edit.
    fn flip_edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Workspace)) {
        self.edit(cx, f);
    }

    /// The Flip module, for track `t`'s sampler `s`, if it's chopped and has a region to flip.
    pub(crate) fn flip_module(
        &mut self,
        t: usize,
        s: &Sampler,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let chops = chop_notes(s);
        let sliced = chops.len() >= 2 && s.zones.iter().skip(1).any(|z| z.file == s.zones[0].file);
        if !sliced {
            return None;
        }
        let region = self.flip_region(t)?;
        let p = &self.session.project;
        let r = p.region(region).clone();
        let length = r.length.max(1) as f32;
        let (beat, bar) = (p.beat().max(1) as f32, p.bar().max(1) as f32);
        let order = p.chop_order(region);
        let sequence: Vec<(i64, u64, u8)> = (0..order.len())
            .filter_map(|i| self.chop_at(region, i))
            .collect();
        let picked = self.sampler_ui.chop.filter(|&i| i < sequence.len());
        let hue = |note: u8| {
            let k = chops.iter().position(|&c| c == note).unwrap_or(0);
            HUES[k % HUES.len()]
        };
        let number = |note: u8| chops.iter().position(|&c| c == note).map_or(0, |k| k + 1);
        let zone_of = |note: u8| s.zones.iter().find(|z| z.note == note).cloned();
        let mut waves: HashMap<PathBuf, Option<Arc<Wave>>> = HashMap::new();
        for z in &s.zones {
            if !waves.contains_key(&z.file) {
                let w = self.wave(&z.file, cx);
                waves.insert(z.file.clone(), w);
            }
        }
        let view_of = |z: &Zone| (z.start, z.end.unwrap_or(1.0));
        let wave_of = |z: &Zone| waves.get(&z.file).cloned().flatten();

        // The chops, to hear and to drag in.
        let tiles: Vec<Div> = chops
            .iter()
            .map(|&note| {
                let z = zone_of(note).unwrap_or_default();
                let tone = hue(note);
                div()
                    .relative()
                    .w(px(64.))
                    .h(px(46.))
                    .flex_none()
                    .rounded(px(8.))
                    .overflow_hidden()
                    .bg(alpha(tone, 0.12))
                    .border_1()
                    .border_color(alpha(tone, 0.45))
                    .cursor_grab()
                    .debug_selector(move || format!("chop-{note}"))
                    .child(chop_wave(wave_of(&z), view_of(&z), tone))
                    .child(
                        div()
                            .absolute()
                            .top(px(3.))
                            .left(px(4.))
                            .px(px(4.))
                            .rounded(px(4.))
                            .bg(alpha(0x000000, 0.6))
                            .text_size(px(10.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(tone))
                            .child(match z.reverse {
                                true => format!("{} ◀", number(note)),
                                false => number(note).to_string(),
                            }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let p = pt(e.position);
                            ws.begin_drag(
                                Drag::Chop {
                                    track: t,
                                    region,
                                    from: ChopFrom::Chop(note),
                                    start: p,
                                    at: p,
                                    moved: false,
                                },
                                cx,
                            );
                        }),
                    )
            })
            .collect();

        // The sequence: the beats and bars behind, each chop in its place.
        let lines: Vec<Div> = (1..)
            .map(|k| k as f32 * beat)
            .take_while(|&tick| tick < length)
            .map(|tick| {
                let on_bar = (tick % bar).abs() < 0.5;
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(gpui::relative(tick / length))
                    .w(px(1.))
                    .bg(alpha(0xffffff, if on_bar { 0.16 } else { 0.06 }))
            })
            .collect();
        let blocks: Vec<Div> = sequence
            .iter()
            .enumerate()
            .map(|(i, &(start, len, note))| {
                let z = zone_of(note).unwrap_or_default();
                let tone = hue(note);
                let on = picked == Some(i);
                div()
                    .absolute()
                    .top(px(4.))
                    .bottom(px(4.))
                    .left(gpui::relative(start as f32 / length))
                    .w(gpui::relative(len as f32 / length))
                    .px(px(1.))
                    .child(
                        div()
                            .relative()
                            .size_full()
                            .rounded(px(6.))
                            .overflow_hidden()
                            .bg(alpha(tone, if on { 0.3 } else { 0.16 }))
                            .border_1()
                            .border_color(if on {
                                alpha(0xffffff, 1.0)
                            } else {
                                alpha(tone, 0.6)
                            })
                            .cursor_grab()
                            .debug_selector(move || format!("slot-{i}"))
                            .child(chop_wave(wave_of(&z), view_of(&z), tone))
                            .child(
                                div()
                                    .absolute()
                                    .top(px(3.))
                                    .left(px(3.))
                                    .px(px(4.))
                                    .rounded(px(4.))
                                    .bg(alpha(0x000000, 0.6))
                                    .text_size(px(10.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(if on { 0xffffff } else { tone }))
                                    .child(match z.reverse {
                                        true => format!("{} ◀", number(note)),
                                        false => number(note).to_string(),
                                    }),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    let p = pt(e.position);
                                    ws.begin_drag(
                                        Drag::Chop {
                                            track: t,
                                            region,
                                            from: ChopFrom::Slot(i),
                                            start: p,
                                            at: p,
                                            moved: false,
                                        },
                                        cx,
                                    );
                                }),
                            ),
                    )
            })
            .collect();
        // Where it's playing, on the sequence (round its loops).
        let head = {
            let (now, from) = (self.playhead() as i64, r.start as i64);
            (self.player.playing() && now >= from && (r.looped || now < from + length as i64))
                .then(|| ((now - from) as f32 % length) / length)
        };
        let lane_bounds = self.sampler_ui.flip_lane.clone();
        let lane = div()
            .relative()
            .w_full()
            .h(px(LANE_H))
            .rounded(px(10.))
            .bg(rgb(0x0a0a0d))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .overflow_hidden()
            .debug_selector(|| "flip-lane".into())
            .child(
                canvas(move |b, _, _| lane_bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .children(lines)
            .children(blocks)
            .children(head.map(|x| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(gpui::relative(x))
                    .w(px(2.))
                    .bg(rgb(0xffffff))
            }));

        let cuts = self.cuts_lane(t, s, cx);
        // Stopped after Play it was heard playing (the song came to its end, say): the solo let
        // go. Not before it's heard: the audio thread starts a moment after the click. Stopping
        // it lets go too (`stop`), even before a frame saw it play.
        match (&mut self.sampler_ui.flip_solo, self.player.playing()) {
            (Some((_, heard)), true) => *heard = true,
            (Some((_, true)), false) => self.unsolo_flip(cx),
            _ => {}
        }

        // What to do to it all, and to the chop picked.
        let act = |label: &'static str,
                   sel: &'static str,
                   f: fn(&mut Workspace, At, &mut Context<Self>)| {
            button(label, false)
                .h(px(26.))
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        f(ws, region, cx)
                    }),
                )
        };
        let originals: Vec<u8> = {
            let mut v: Vec<u8> = s
                .zones
                .iter()
                .filter(|z| !z.reverse)
                .map(|z| z.note)
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let flip = crate::sounds::browser::primary("Flip")
            .h(px(28.))
            .px(px(16.))
            .debug_selector(|| "flip".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.sampler_ui.flip_seed += 1;
                    let seed = ws.sampler_ui.flip_seed;
                    ws.flip_edit(cx, |ws| ws.session.project.flip_chops(region, seed));
                    ws.status = Some(format!("Flip {seed}: press again for another"));
                }),
            );
        let playing = self.player.playing();
        let span = (r.start, r.start + r.length);
        let play = button(if playing { "Stop" } else { "Play it" }, playing)
            .h(px(26.))
            .debug_selector(|| "flip-play".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    if ws.player.playing() {
                        ws.play_stop(cx);
                        return ws.unsolo_flip(cx);
                    }
                    // Round and round the region, from its start, alone.
                    ws.edit(cx, |ws| {
                        ws.session.project.cycle = Some(pina_engine::song::project::Cycle {
                            start: span.0,
                            end: span.1,
                            off: false,
                        })
                    });
                    if !ws.session.project.tracks[t].solo {
                        ws.session.project.tracks[t].solo = true;
                        ws.sampler_ui.flip_solo = Some((t, false));
                        ws.changed(cx);
                    }
                    ws.locate_tick(span.0, cx);
                    ws.play_stop(cx);
                }),
            );
        let tools = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .child(play)
            .child(flip)
            .child(act("Turn round", "flip-reverse", |ws, at, cx| {
                ws.flip_edit(cx, |ws| ws.session.project.reverse_chop_order(at))
            }))
            .child(act("Half time", "flip-half", |ws, at, cx| {
                ws.flip_edit(cx, |ws| ws.session.project.time_chops(at, true))
            }))
            .child(act("Double time", "flip-double", |ws, at, cx| {
                ws.flip_edit(cx, |ws| ws.session.project.time_chops(at, false))
            }))
            .child(
                button("As it was", false)
                    .h(px(26.))
                    .debug_selector(|| "flip-original".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            let chops = originals.clone();
                            ws.flip_edit(cx, |ws| {
                                ws.session.project.chops_in_order(region, &chops)
                            });
                        }),
                    ),
            );
        let on_picked = picked.map(|i| {
            let stutter = |times: u64, label: &'static str| {
                button(label, false)
                    .h(px(24.))
                    .debug_selector(move || format!("chop-stutter-{times}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.flip_edit(cx, |ws| {
                                ws.session.project.stutter_chop(region, i, times)
                            });
                        }),
                    )
            };
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_2))
                        .child(format!("Chop {} of {}", i + 1, sequence.len())),
                )
                .child(stutter(2, "Stutter ×2"))
                .child(stutter(3, "×3"))
                .child(stutter(4, "×4"))
                .child(
                    button("Backwards", false)
                        .h(px(24.))
                        .debug_selector(|| "chop-reverse".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.flip_edit(cx, |ws| {
                                    ws.session.project.reverse_chop(region, i);
                                });
                            }),
                        ),
                )
                .child(
                    button("Silence", false)
                        .h(px(24.))
                        .debug_selector(|| "chop-silence".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.flip_edit(cx, |ws| ws.session.project.silence_chop(region, i));
                                ws.sampler_ui.chop = None;
                            }),
                        ),
                )
        });
        let heading = |s: &'static str| {
            div()
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_3))
                .child(s)
        };
        Some(
            module("FLIP", AMBER, div())
                .h_auto()
                .w_full()
                .debug_selector(|| "flip-module".into())
                .child(tools)
                .child(heading(
                    "CUTS · click the sample to cut it there, drag a cut to move it, double-click one to take it out",
                ))
                .child(cuts)
                .child(heading("CHOPS · click to hear, drag onto the sequence"))
                .child(
                    div()
                        .id("flip-chops")
                        .flex()
                        .gap(px(6.))
                        .overflow_x_scroll()
                        .children(tiles),
                )
                .child(heading("SEQUENCE · drag a chop onto another to swap them"))
                .child(lane)
                .children(on_picked),
        )
    }

    /// A chop being dragged, at `p`.
    pub(crate) fn drag_chop(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        if let Some(Drag::Chop {
            start, at, moved, ..
        }) = &mut self.drag
        {
            *at = p;
            *moved |= (p.x - start.x).hypot(p.y - start.y) > 4.0;
        }
        cx.notify();
    }

    /// A chop let go of: on a place in the sequence, swapped there or put there; not dragged, a
    /// click: heard (and, in the sequence, picked).
    pub(crate) fn drop_chop(&mut self, drag: Option<Drag>, cx: &mut Context<Self>) {
        let Some(Drag::Chop {
            track,
            region,
            from,
            at,
            moved,
            ..
        }) = drag
        else {
            return;
        };
        if !moved {
            let note = match from {
                ChopFrom::Slot(i) => {
                    self.sampler_ui.chop = Some(i);
                    self.chop_at(region, i).map(|c| c.2)
                }
                ChopFrom::Chop(n) => Some(n),
            };
            if let Some(n) = note {
                self.hear_chop(track, n);
            }
            return cx.notify();
        }
        let Some(slot) = self.chop_slot_at(region, at) else {
            return cx.notify();
        };
        match from {
            ChopFrom::Slot(i) if i != slot => {
                self.flip_edit(cx, |ws| ws.session.project.swap_chops(region, i, slot));
            }
            ChopFrom::Chop(n) => {
                self.flip_edit(cx, |ws| ws.session.project.set_chop(region, slot, n));
            }
            _ => {}
        }
        self.sampler_ui.chop = Some(slot);
        cx.notify();
    }

    /// The chop being dragged, by the pointer: its number in its colour, and what letting go
    /// will do.
    pub fn chop_ghost(&self) -> Option<impl IntoElement + use<>> {
        let Some(Drag::Chop {
            track,
            region,
            from,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let s = self.sampler_of(track)?;
        let chops = chop_notes(s);
        let note = match from {
            ChopFrom::Slot(i) => self.chop_at(region, i)?.2,
            ChopFrom::Chop(n) => n,
        };
        let k = chops.iter().position(|&c| c == note)?;
        let tone = HUES[k % HUES.len()];
        let what = match (from, self.chop_slot_at(region, at)) {
            (_, None) => "Drop it on the sequence".to_string(),
            (ChopFrom::Slot(i), Some(to)) if i == to => "Where it is".to_string(),
            (ChopFrom::Slot(_), Some(to)) => format!("Swap with chop {}", to + 1),
            (ChopFrom::Chop(_), Some(to)) => format!("Play it at chop {}", to + 1),
        };
        Some(
            div()
                .absolute()
                .left(px(at.x + 12.))
                .top(px(at.y + 10.))
                .px(px(8.))
                .py(px(5.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(8.))
                .bg(rgb(0x1c1c22))
                .border_1()
                .border_color(rgb(tone))
                .shadow_lg()
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(tone))
                        .child(format!("{}", k + 1)),
                )
                .child(div().text_size(px(11.)).text_color(rgb(TEXT_2)).child(what)),
        )
    }
}

impl Workspace {
    /// The track Play it soloed, unsoloed.
    pub(crate) fn unsolo_flip(&mut self, cx: &mut Context<Self>) {
        if let Some((t, _)) = self.sampler_ui.flip_solo.take()
            && let Some(track) = self.session.project.tracks.get_mut(t)
        {
            track.solo = false;
            self.changed(cx);
        }
    }

    /// The sample, whole, its chops in their colours and the cuts between them: click to cut,
    /// drag a cut to move it, double-click one to take it out.
    fn cuts_lane(&mut self, t: usize, s: &Sampler, cx: &mut Context<Self>) -> Div {
        let chops = cuts_of(s);
        let order = chop_notes(s);
        let wave = s.zones.first().and_then(|z| self.wave(&z.file, cx));
        let tones: Vec<(f32, f32, u32)> = chops
            .iter()
            .map(|c| {
                let k = order.iter().position(|&n| n == c.1).unwrap_or(0);
                (c.2, c.3, HUES[k % HUES.len()])
            })
            .collect();
        let bounds = self.sampler_ui.cut_lane.clone();
        let drawn = canvas(
            move |b, _, _| bounds.set(b),
            move |b, _, window, _| {
                if let Some(w) = &wave {
                    paint_wave(window, w, b, (0.0, 1.0), 1.0, |fa| {
                        match tones.iter().find(|c| fa >= c.0 && fa < c.1) {
                            Some(c) => (alpha(c.2, 0.4), alpha(c.2, 0.95)),
                            None => (alpha(0xffffff, 0.08), alpha(0xffffff, 0.18)),
                        }
                    });
                }
            },
        )
        .absolute()
        .size_full();
        let markers: Vec<Div> = chops
            .iter()
            .enumerate()
            .skip(1)
            .map(|(k, c)| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(gpui::relative(c.2))
                    .w(px(12.))
                    .ml(px(-6.))
                    .flex()
                    .justify_center()
                    .cursor_ew_resize()
                    .debug_selector(move || format!("cut-{k}"))
                    .child(div().w(px(2.)).h_full().bg(alpha(0xffffff, 0.85)))
                    .child(
                        div()
                            .absolute()
                            .top(px(2.))
                            .size(px(9.))
                            .rounded(px(5.))
                            .bg(rgb(0xffffff)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if e.click_count >= 2 {
                                ws.drag = None;
                                ws.edit(cx, |ws| ws.session.project.join_chops(t, k));
                                ws.status = Some("Cut taken out: its chops joined".into());
                                return;
                            }
                            let p = pt(e.position);
                            ws.begin_drag(
                                Drag::Cut {
                                    track: t,
                                    k: Some(k),
                                    start: p,
                                    moved: false,
                                },
                                cx,
                            );
                        }),
                    )
            })
            .collect();
        div()
            .relative()
            .w_full()
            .h(px(64.))
            .rounded(px(10.))
            .bg(rgb(0x0a0a0d))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .overflow_hidden()
            .cursor_crosshair()
            .debug_selector(|| "cut-lane".into())
            .child(drawn)
            .children(markers)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    let p = pt(e.position);
                    ws.begin_drag(
                        Drag::Cut {
                            track: t,
                            k: None,
                            start: p,
                            moved: false,
                        },
                        cx,
                    );
                }),
            )
    }

    /// Where `p` (a window position) is along the sample, as a share of it.
    fn cut_share(&self, p: Point<f32>) -> f32 {
        let b = self.sampler_ui.cut_lane.get();
        let (left, w) = (f32::from(b.origin.x), f32::from(b.size.width).max(1.0));
        ((p.x - left) / w).clamp(0.0, 1.0)
    }

    /// A cut being dragged: moved as it goes.
    pub(crate) fn drag_cut(&mut self, p: Point<f32>, cx: &mut Context<Self>) {
        let Some(Drag::Cut {
            track,
            k,
            start,
            moved,
        }) = &mut self.drag
        else {
            return;
        };
        *moved |= (p.x - start.x).abs() > 3.0;
        if let (Some(k), true) = (*k, *moved) {
            let (t, share) = (*track, self.cut_share(p));
            self.session.project.move_cut(t, k, share);
            self.changed(cx);
        }
        cx.notify();
    }

    /// A cut let go of; or, a click on the sample, a cut made there.
    pub(crate) fn drop_cut(&mut self, drag: Option<Drag>, cx: &mut Context<Self>) {
        let Some(Drag::Cut {
            track,
            k,
            start,
            moved,
        }) = drag
        else {
            return;
        };
        if k.is_none() && !moved {
            let share = self.cut_share(start);
            match self.session.project.cut_chop(track, share) {
                Some(_) => {
                    self.status = Some("Cut: a chop more, the sequence playing as it did".into());
                    self.changed(cx);
                }
                None => self.session.cancel(),
            }
        } else if !moved || self.session.unchanged() {
            self.session.cancel();
        }
        cx.notify();
    }
}
