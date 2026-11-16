//! The sounds column: the library's sounds for the pad picked, as the browser knows them. Those
//! that sound like it (nearest first), its pack's (its kind first), or every one of its kind (in
//! the song's key), narrowed by words. A click puts one on the pad, heard there as the beat
//! plays; up and down step through them; + layers one on; a row drags onto any pad; Back to puts
//! back the sound it had when the list was made.

use std::ops::Range;

use gpui::{ScrollStrategy, uniform_list};
use pina_engine::samples::{Kind, search};

use super::*;
use crate::sounds::browser::wave;

/// A row's height.
const ROW_H: f32 = 46.0;
/// The most any list holds.
const MOST: usize = 400;

/// Where the sounds column's sounds come from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// Those that sound like the pad's, nearest first.
    Like,
    /// Its pack's: the folder its sound is from, its kind first.
    Pack,
    /// Every one of its kind, in the song's key.
    Kind,
}

/// What the column found, and what for: the pad and layer, where from and the words (and how
/// big the library was), so it's found again only when one changes; and the sound the pad had
/// when its list was first made, for Back to.
pub(crate) struct SoundList {
    key: (u8, usize, Source, String, usize),
    pub(crate) anchor: Option<usize>,
    pub(crate) hits: Vec<usize>,
}

impl Workspace {
    /// The library sample on a pad's layer, by index.
    pub(crate) fn layer_index(&self, kit: usize, note: u8, layer: usize) -> Option<usize> {
        let from = self.layer_from(kit, note, layer)?;
        self.browser
            .library
            .samples
            .iter()
            .position(|s| s.path == from)
    }

    /// What a pad is, or (no pad on the note) what its note plays in the drum map.
    fn sound_role(&self, kit: usize, note: u8) -> Category {
        match self.drum_pad(kit, note) {
            Some(_) => self.pad_role(kit, note),
            None => gm_role(note),
        }
    }

    /// The words the column is narrowed by.
    fn sound_words(&self, cx: &Context<Self>) -> String {
        self.drums
            .find
            .as_ref()
            .map(|f| f.read(cx).text().trim().to_string())
            .unwrap_or_default()
    }

    /// The pad picked's sounds, found again only when what they're for changes.
    pub(crate) fn sound_list(&mut self, kit: usize, cx: &Context<Self>) -> &SoundList {
        let note = self.drums.pad;
        let layer = self.pad_layer(kit, note);
        let source = self.drums.source;
        let words = self.sound_words(cx);
        let samples = &self.browser.library.samples;
        let key = (note, layer, source, words.clone(), samples.len());
        if self.drums.list.as_ref().is_some_and(|l| l.key == key) {
            return self.drums.list.as_ref().unwrap();
        }
        let current = self.layer_index(kit, note, layer);
        let role = self.sound_role(kit, note);
        let samples = &self.browser.library.samples;
        let of_kind = |role: Category| match role {
            Category::Other => Vec::new(),
            r => vec![r],
        };
        let kind_query = || search::Query {
            text: words.clone(),
            categories: of_kind(role),
            kind: Some(Kind::OneShot),
            key: self.sample_key().map(|k| k.name()),
            limit: MOST,
            ..Default::default()
        };
        let current_path = current.and_then(|i| samples.get(i)).map(|s| s.path.clone());
        let query = match (source, &current_path) {
            (Source::Like, Some(path)) => search::Query {
                text: words.clone(),
                categories: of_kind(role).into_iter().filter(|r| r.is_drum()).collect(),
                kind: Some(Kind::OneShot),
                like: Some(path.clone()),
                limit: MOST,
                ..Default::default()
            },
            (Source::Pack, Some(path)) => {
                let folder = samples
                    .iter()
                    .find(|s| &s.path == path)
                    .map(|s| s.folder.clone());
                search::Query {
                    text: words.clone(),
                    kind: Some(Kind::OneShot),
                    folder,
                    limit: MOST,
                    ..Default::default()
                }
            }
            _ => kind_query(),
        };
        let mut hits: Vec<usize> = search::find(samples, &query)
            .into_iter()
            .map(|h| h.index)
            .collect();
        // Its pack's: its own kind first, the rest as found.
        if source == Source::Pack {
            hits.sort_by_key(|&i| samples.get(i).map(|s| s.analysis.category != role));
        }
        // Like it: the pad's own sound first, the nearest after.
        if source == Source::Like
            && words.is_empty()
            && let Some(me) = current
        {
            hits.retain(|&i| i != me);
            hits.insert(0, me);
        }
        // Back to: the sound the pad had when this pad's (or layer's) list was first made.
        let anchor = match &self.drums.list {
            Some(l) if (l.key.0, l.key.1) == (note, layer) => l.anchor,
            _ => current,
        };
        self.drums.list = Some(SoundList { key, anchor, hits });
        self.drums.list.as_ref().unwrap()
    }

    /// The pad's next sound in the column (or the one before, `by` -1): put on it, heard, and
    /// scrolled to.
    pub(crate) fn step_sound(&mut self, kit: usize, note: u8, by: i32, cx: &mut Context<Self>) {
        self.drums.pad = note;
        let layer = self.pad_layer(kit, note);
        let current = self.layer_index(kit, note, layer);
        let hits = self.sound_list(kit, cx).hits.clone();
        if hits.is_empty() {
            self.status =
                Some("Nothing in the library to step to: the column beside it is empty.".into());
            cx.notify();
            return;
        }
        let at = current.and_then(|c| hits.iter().position(|&i| i == c));
        let to = match at {
            Some(at) => (at as i32 + by).rem_euclid(hits.len() as i32) as usize,
            None if by > 0 => 0,
            None => hits.len() - 1,
        };
        self.drums.scroll.scroll_to_item(to, ScrollStrategy::Center);
        self.set_pad_sample(kit, note, layer, hits[to], cx);
        cx.notify();
    }

    /// The column: what it's for, where its sounds come from, the words, the sounds.
    pub(crate) fn pad_sounds(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        if self.drums.find.is_none() {
            let field = cx.new(|cx| TextInput::search("Search sounds", cx));
            let mut subs = vec![cx.observe(&field, |_, _, cx| cx.notify())];
            subs.push(cx.subscribe(
                &field,
                |ws, field, e: &crate::ui::text::TextEvent, cx| match e {
                    // Enter: the first found, on the pad.
                    crate::ui::text::TextEvent::Confirm(_) => {
                        if let Some(kit) = ws.drum_kit() {
                            let note = ws.drums.pad;
                            let layer = ws.pad_layer(kit, note);
                            if let Some(&first) = ws.sound_list(kit, cx).hits.first() {
                                ws.set_pad_sample(kit, note, layer, first, cx);
                            }
                        }
                    }
                    crate::ui::text::TextEvent::Cancel => {
                        field.update(cx, |f, cx| f.set_text("", cx));
                    }
                },
            ));
            self.drums.find = Some(field);
            self.drums.find_subs = subs;
        }
        let note = self.drums.pad;
        let layer = self.pad_layer(kit, note);
        let role = self.sound_role(kit, note);
        let color = category_color(role);
        let current = self.layer_index(kit, note, layer);
        let (count, anchor) = {
            let l = self.sound_list(kit, cx);
            (l.hits.len(), l.anchor)
        };
        let samples = &self.browser.library.samples;
        let back = anchor
            .filter(|&a| Some(a) != current)
            .and_then(|a| Some((a, samples.get(a)?.name.clone())));
        let pad_name = self
            .session
            .project
            .pad_on(kit, note)
            .map(|t| self.session.project.tracks[t].name.clone())
            .unwrap_or_else(|| format!("a new {}", role.name().to_lowercase()));
        let source = self.drums.source;
        let sources = tray().p(px(2.)).rounded(px(8.)).w_full().children(
            [
                (Source::Like, "Like it".to_string(), "sounds-like"),
                (Source::Pack, "Its pack".to_string(), "sounds-pack"),
                (
                    Source::Kind,
                    format!("Every {}", role.name().to_lowercase()),
                    "sounds-kind",
                ),
            ]
            .map(|(s, label, sel)| {
                seg(label, source == s, 24.)
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .debug_selector(move || sel.into())
                    .help(
                        match s {
                            Source::Like => "Sounds like the pad's, nearest first",
                            Source::Pack => "The pack the pad's sound is from, its kind first",
                            Source::Kind => {
                                "Every one of its kind in the library, in the song's key"
                            }
                        },
                        cx,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drums.source = s;
                            ws.drums.scroll.scroll_to_item(0, ScrollStrategy::Top);
                            cx.notify();
                        }),
                    )
            }),
        );
        let field = self.drums.find.clone().unwrap();
        let list = match count {
            0 => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .px(px(20.))
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_2))
                        .child("Nothing here"),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .text_center()
                        .child(match self.browser.library.samples.is_empty() {
                            true => "Add a folder of samples on the Samples page to fill this.",
                            false => "Try other words, or another of the three above.",
                        }),
                )
                .into_any_element(),
            n => uniform_list(
                "pad-sounds",
                n,
                cx.processor(|ws, range: Range<usize>, _, cx| ws.pad_sound_rows(range, cx)),
            )
            .track_scroll(&self.drums.scroll)
            .flex_1()
            .into_any_element(),
        };
        div()
            .w(px(316.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(LINE))
            .bg(rgb(HEADERS))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .px(px(12.))
                    .pt(px(10.))
                    .pb(px(10.))
                    .border_b_1()
                    .border_color(rgb(LINE_SOFT))
                    .child(
                        div()
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(div().size(px(8.)).rounded(px(4.)).bg(rgb(color)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(TEXT_2))
                                            .child(format!(
                                                "HOT-SWAP · {}",
                                                pad_name.to_uppercase()
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(count.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .h(px(32.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(8.))
                            .bg(rgb(BG))
                            .border_1()
                            .border_color(rgb(LINE))
                            .text_size(px(12.5))
                            .debug_selector(|| "sounds-search".into())
                            .child(icon(Icon::Search, 13.).text_color(rgb(TEXT_3)))
                            .child(div().flex_1().min_w(px(0.)).child(field)),
                    )
                    .child(sources)
                    .children(back.map(|(a, name)| {
                        div()
                            .h(px(26.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .rounded(px(7.))
                            .bg(rgb(FIELD))
                            .hover(|d| d.bg(rgb(RAISED)))
                            .cursor_pointer()
                            .debug_selector(|| "sounds-back".into())
                            .child(icon(Icon::RotateLeft, 12.).text_color(rgb(TEXT_2)))
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_3))
                                    .child("Back to"),
                            )
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .text_size(px(11.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(name),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.set_pad_sample(kit, note, layer, a, cx);
                                }),
                            )
                    })),
            )
            .child(list)
            .child(
                div()
                    .h(px(28.))
                    .flex_none()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(rgb(LINE_SOFT))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child("Click: on the pad  ·  ↑↓ step  ·  + layer  ·  drag onto a pad"),
            )
    }

    /// Rows of the column, as its list draws them.
    fn pad_sound_rows(
        &mut self,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        let Some(kit) = self.drum_kit() else {
            return Vec::new();
        };
        let hits = self.sound_list(kit, cx).hits.clone();
        range
            .filter_map(|pos| Some(self.pad_sound_row(kit, pos, *hits.get(pos)?, cx)))
            .collect()
    }

    /// A row: hear it on its own, its outline, its name, pack and length; on the pad, said so;
    /// else + to layer it on. A click puts it on the pad; a drag takes it to any pad.
    fn pad_sound_row(
        &self,
        kit: usize,
        pos: usize,
        index: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let note = self.drums.pad;
        let layer = self.pad_layer(kit, note);
        let s = &self.browser.library.samples[index];
        let a = &s.analysis;
        let color = category_color(a.category);
        let on = self.layer_index(kit, note, layer) == Some(index);
        let heard = self.heard(index);
        let pack = s.folder.rsplit('/').next().unwrap_or(&s.folder).to_string();
        let group = format!("pad-sound-row-{pos}");
        let has_pad = self.drum_pad(kit, note).is_some();
        div()
            .id(("pad-sound-row", pos))
            .group(group.clone())
            .w_full()
            .h(px(ROW_H))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(9.))
            .relative()
            .border_b_1()
            .border_color(rgb(LINE_SOFT))
            .bg(match on {
                true => alpha(color, 0.14),
                false => gpui::transparent_black(),
            })
            .when(!on, |d| d.hover(|s| s.bg(rgb(0x1a1a20))))
            .cursor_pointer()
            .debug_selector(move || format!("pad-sound-{pos}"))
            .when(on, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(3.))
                        .h_full()
                        .bg(rgb(color)),
                )
            })
            .child(self.sample_play(
                index,
                heard.is_some(),
                color,
                20.,
                format!("pad-sound-play-{pos}"),
                cx,
            ))
            .child(div().w(px(48.)).h(px(24.)).flex_none().child(wave(
                a.waveform.clone(),
                color,
                if on { 1.0 } else { 0.75 },
            )))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(if on {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::MEDIUM
                            })
                            .text_color(rgb(if on { TEXT } else { TEXT_2 }))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(s.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(format!(
                                "{} · {} · {}",
                                a.category.name(),
                                crate::sounds::browser::seconds_text(a.seconds),
                                pack
                            )),
                    ),
            )
            .child(match on {
                true => chip("On pad", color).into_any_element(),
                false if has_pad => icon_seg(Icon::Plus, false, 24.)
                    .invisible()
                    .group_hover(group, |d| d.visible())
                    .debug_selector(move || format!("pad-sound-layer-{pos}"))
                    .help("Layer it on the pad, with what's there", cx)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.drum_op(DrumOp::Layer { note, index }, cx);
                        }),
                    )
                    .into_any_element(),
                false => div().into_any_element(),
            })
            // Pressed: picked up, to drag onto a pad; let go where it was, it's on the pad.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.drums.pressed = Some((note, layer, index));
                    let at = crate::workspace::pt(e.position);
                    ws.begin_drag(
                        Drag::Sample {
                            index,
                            from: at,
                            at,
                            moved: false,
                        },
                        cx,
                    );
                }),
            )
    }
}

impl Workspace {
    /// Hot-swap on the kit as a whole: drums fitted to the song, or a folder of the library's,
    /// into this kit (its pads given the folder's sounds on their notes).
    pub(crate) fn kit_swap(&mut self, kit: usize, cx: &mut Context<Self>) -> Div {
        let folders = self.drum_folders();
        let row = |label: String, sub: String| {
            div()
                .h(px(34.))
                .px(px(12.))
                .flex()
                .flex_col()
                .justify_center()
                .cursor_pointer()
                .hover(|d| d.bg(rgb(FIELD)))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(TEXT))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(label),
                )
                .child(div().text_size(px(10.5)).text_color(rgb(TEXT_3)).child(sub))
        };
        let new = row(
            "Fit the song".into(),
            "Drums picked to go together, in its key and tempo".into(),
        )
        .debug_selector(|| "kit-swap-fit".into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|ws, _, _, cx| {
                cx.stop_propagation();
                ws.drum_op(DrumOp::Fit, cx);
            }),
        );
        let list: Vec<Div> = folders
            .into_iter()
            .map(|(folder, n)| {
                let name = folder.rsplit('/').next().unwrap_or(&folder).to_string();
                let into = Some(kit);
                row(name, format!("{n} sounds")).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.drum_op(
                            DrumOp::Folder {
                                folder: folder.clone(),
                                into,
                            },
                            cx,
                        );
                    }),
                )
            })
            .collect();
        div()
            .w(px(316.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(LINE))
            .bg(rgb(HEADERS))
            .debug_selector(|| "kit-swap".into())
            .child(
                div()
                    .h(px(38.))
                    .flex_none()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(LINE_SOFT))
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(TEXT_2))
                    .child("HOT-SWAP · KIT"),
            )
            .child(
                div()
                    .id("kit-swap-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(new)
                    .children(list),
            )
            .child(
                div()
                    .h(px(28.))
                    .flex_none()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(rgb(LINE_SOFT))
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_4))
                    .child("D: hot-swap the picked pad instead"),
            )
    }
}
