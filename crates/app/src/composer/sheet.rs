//! The Compose tab: the parts to write (the tracks picked) and what each is heard as and steered
//! by; the model and how it writes; and the takes, each to hear in the song before one's kept.
//! On the lanes, the bars being written, and each take's notes as they're written.

use gpui::{
    AnyElement, Bounds, Context, Div, FontWeight, MouseButton, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};
use pina_engine::{
    composer::catalog::{KNOWN, Known},
    song::project::Note,
};

use super::{Stage, Target, elapsed};
use crate::{
    pages::settings::Section,
    sounds::browser::widgets::primary,
    ui::{menu::MenuAction, theme::*},
    workspace::{Slider, Workspace},
};

/// What a menu in the sheet sets.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Model(&'static str),
    /// A track (by id) heard as a part, or as it sounds.
    Part(u64, Option<pina_engine::composer::score::Part>),
    /// A track's control (by its name) at a level, or left to the model.
    Steer(u64, String, Option<i64>),
    Takes(u32),
    OnCpu(bool),
    /// Settings › Composers, to download another.
    Settings,
    /// Compose opened from a track's menu: it, or the tracks picked with it.
    Open(usize),
}

/// The sheet's columns side by side when the editor's this wide; narrower, the model's under
/// the parts.
const WIDE: f32 = 980.0;

/// A control's name as the sheet says it.
pub fn control_name(name: &str) -> String {
    match name {
        "note_density" => "Density".into(),
        "min_polyphony" => "Fewest at once".into(),
        "max_polyphony" => "Most at once".into(),
        "min_note_duration" => "Shortest notes".into(),
        "max_note_duration" => "Longest notes".into(),
        "silence_proportion" => "Silence".into(),
        "pitch_range" => "Pitch range".into(),
        _ => {
            let s = name.replace('_', " ");
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        }
    }
}

/// A small caption over a column: WRITE, TAKES.
fn caption(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(s.into())
}

fn small(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .text_size(px(11.5))
        .text_color(rgb(TEXT_3))
        .child(s.into())
}

fn menu(
    items: Vec<(String, MenuAction)>,
    cx: &mut Context<Workspace>,
) -> impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    cx.listener(move |ws, e: &gpui::MouseDownEvent, _, cx| {
        cx.stop_propagation();
        ws.open_menu(e.position, items.clone(), cx);
    })
}

/// A thin bar of how far, 0 to 1.
fn bar(at: f32) -> Div {
    div()
        .h(px(4.))
        .flex_1()
        .rounded(px(2.))
        .bg(rgb(RAISED))
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .rounded(px(2.))
                .bg(rgb(ACCENT))
                .w(gpui::relative(at.clamp(0.02, 1.0))),
        )
}

/// Notes drawn small, a strip a part, between ticks `a` and `b` (bars every `bar`).
pub fn strips(parts: Vec<(Vec<Note>, u32)>, a: u64, b: u64, bar: u64, h: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.paint_quad(fill(bounds, rgb(WELL)));
            let (w, all_h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let span = (b.saturating_sub(a)).max(1) as f32;
            let mut at = a + bar;
            while at < b {
                let x = (at - a) as f32 / span * w;
                window.paint_quad(fill(
                    Bounds::new(
                        point(bounds.origin.x + px(x), bounds.origin.y),
                        size(px(1.), bounds.size.height),
                    ),
                    rgb(LINE_SOFT),
                ));
                at += bar.max(1);
            }
            let rh = all_h / parts.len().max(1) as f32;
            for (i, (notes, color)) in parts.iter().enumerate() {
                let (Some(lo), Some(hi)) = (
                    notes.iter().map(|n| n.pitch).min(),
                    notes.iter().map(|n| n.pitch).max(),
                ) else {
                    continue;
                };
                let range = (hi - lo).max(1) as f32;
                for n in notes {
                    let x = (n.start - a as i64) as f32 / span * w;
                    let nw = (n.length as f32 / span * w - 0.8).max(1.5);
                    let y =
                        i as f32 * rh + 3.0 + (1.0 - (n.pitch - lo) as f32 / range) * (rh - 8.0);
                    window.paint_quad(fill(
                        Bounds::new(
                            point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
                            size(px(nw.min(w - x)), px(2.)),
                        ),
                        rgb(*color),
                    ));
                }
            }
        },
    )
    .w_full()
    .h(px(h))
}

impl Workspace {
    /// The Compose tab's body.
    pub(crate) fn compose_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let wide = f32::from(self.lanes_bounds.get().size.width) + self.header_w >= WIDE;
        let mut parts = self.compose_parts(cx);
        let model = self.compose_model_column(cx);
        let takes = self.compose_takes(cx);
        // Narrow, the model and how it writes go under the parts.
        let model = match wide {
            true => model,
            false => {
                parts.push(div().h(px(1.)).bg(rgb(LINE_SOFT)).into_any_element());
                parts.extend(model);
                Vec::new()
            }
        };
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .bg(rgb(SURFACE))
            .child(
                div()
                    .id("compose-parts")
                    .w(px(self.header_w.max(290.)))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .gap(px(8.))
                    .px(px(14.))
                    .py(px(12.))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .children(parts),
            )
            .when(wide, |d| {
                d.child(
                    div()
                        .w(px(300.))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .px(px(14.))
                        .py(px(12.))
                        .border_r_1()
                        .border_color(rgb(LINE))
                        .overflow_hidden()
                        .children(model),
                )
            })
            .child(takes)
    }

    /// The parts: each track picked, what it's heard as, and its own steering; what's heard.
    fn compose_parts(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let ((b0, b1), from) = self.compose_bars();
        let tracks = self.compose_tracks();
        let busy = self.composer.job.is_some();
        let info = self
            .compose_model()
            .and_then(|m| self.compose_info(m.known));
        let mut out: Vec<AnyElement> = vec![
            caption(format!("WRITE · BARS {}–{}", b0 + 1, b1)).into_any_element(),
            small(match from {
                "the loop" => {
                    "From the loop: sweep the ruler's top half to change them".to_string()
                }
                "the region" => {
                    "The region's bars · sweep the ruler's top half to pick others".to_string()
                }
                _ => "Four bars from the playhead · sweep the ruler's top half to pick".to_string(),
            })
            .into_any_element(),
        ];
        if tracks.is_empty() {
            out.push(
                small("Pick a track that plays notes in the headers, or ⌘-click several to write them together")
                    .text_color(rgb(TEXT_2))
                    .into_any_element(),
            );
        }
        let p = &self.session.project;
        for &t in &tracks {
            let tr = &p.tracks[t];
            let id = tr.id;
            let drum = tr.instrument.as_ref().is_some_and(|i| i.is_kit());
            let heard = self.composer.parts.get(&id).copied();
            let mut items: Vec<(String, MenuAction)> = vec![(
                format!("{}As it sounds", if heard.is_none() { "✓ " } else { "" }),
                MenuAction::Compose(Act::Part(id, None)),
            )];
            items.extend(pina_engine::composer::score::Part::ALL.iter().map(|&part| {
                (
                    format!(
                        "{}{}",
                        if heard == Some(part) { "✓ " } else { "" },
                        part.name()
                    ),
                    MenuAction::Compose(Act::Part(id, Some(part))),
                )
            }));
            let shown = heard.map_or("As it sounds", |p| p.name());
            let row = div()
                .flex()
                .items_center()
                .gap(px(10.))
                .pt(px(6.))
                .border_t_1()
                .border_color(rgb(LINE_SOFT))
                .child(
                    div()
                        .size(px(9.))
                        .flex_none()
                        .rounded(px(3.))
                        .bg(rgb(self.color(t))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(tr.name.clone()),
                )
                .child(
                    dropdown(shown, 24.)
                        .w(px(112.))
                        .flex_none()
                        .debug_selector(move || format!("compose-part-{id}"))
                        .when(!busy, |d| {
                            d.on_mouse_down(MouseButton::Left, menu(items, cx))
                        }),
                );
            out.push(row.into_any_element());
            // The model's own controls that fit this part: each a dropdown of its levels.
            let controls: Vec<(String, i64)> = info
                .as_ref()
                .and_then(|i| i["attributes"].as_object().cloned())
                .map(|a| {
                    a.into_iter()
                        .filter_map(|(name, levels)| Some((name, levels.as_i64()?)))
                        .filter(|(name, _)| {
                            match info
                                .as_ref()
                                .map(|i| i["attribute_track_types"][name].clone())
                            {
                                Some(v) if v == "drum" => drum,
                                Some(v) if v == "melodic" => !drum,
                                _ => true,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if controls.is_empty() {
                continue;
            }
            let chips = controls.into_iter().map(|(name, levels)| {
                let set = self
                    .composer
                    .steer
                    .get(&id)
                    .and_then(|s| s.get(&name))
                    .copied();
                let mut items = vec![(
                    format!(
                        "{}The model's choice",
                        if set.is_none() { "✓ " } else { "" }
                    ),
                    MenuAction::Compose(Act::Steer(id, name.clone(), None)),
                )];
                items.extend((0..levels).map(|l| {
                    (
                        format!(
                            "{}{} of {levels}",
                            if set == Some(l) { "✓ " } else { "" },
                            l + 1
                        ),
                        MenuAction::Compose(Act::Steer(id, name.clone(), Some(l))),
                    )
                }));
                let label = match set {
                    Some(l) => format!("{}: {}", control_name(&name), l + 1),
                    None => control_name(&name),
                };
                dropdown(label, 22.)
                    .flex_none()
                    .debug_selector({
                        let name = name.clone();
                        move || format!("compose-steer-{id}-{name}")
                    })
                    .when(!busy, |d| {
                        d.on_mouse_down(MouseButton::Left, menu(items, cx))
                    })
            });
            out.push(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .pl(px(19.))
                    .children(chips)
                    .into_any_element(),
            );
        }
        if !tracks.is_empty() {
            let heard: Vec<String> = p
                .tracks
                .iter()
                .enumerate()
                .filter(|(i, t)| !tracks.contains(i) && t.instrument.is_some())
                .map(|(_, t)| t.name.clone())
                .collect();
            out.push(div().h(px(1.)).bg(rgb(LINE_SOFT)).into_any_element());
            out.push(caption("HEARS").into_any_element());
            out.push(
                small(match heard.is_empty() {
                    true => "Nothing else plays notes: it writes on its own".to_string(),
                    false => format!("{} · audio isn't heard", heard.join(", ")),
                })
                .text_color(rgb(TEXT_2))
                .into_any_element(),
            );
        }
        out
    }

    /// The model, set up if it has to be, and how it writes.
    fn compose_model_column(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out: Vec<AnyElement> = Vec::new();
        let have = super::installed();
        let picked = self.compose_model();
        let busy = self.composer.job.is_some();
        // The model: each downloaded, then those to download in Settings.
        let mut items: Vec<(String, MenuAction)> = have
            .iter()
            .map(|h| {
                (
                    format!(
                        "{}{}",
                        if picked.as_ref().map(|p| p.known.id) == Some(h.known.id) {
                            "✓ "
                        } else {
                            ""
                        },
                        h.known.name
                    ),
                    MenuAction::Compose(Act::Model(h.known.id)),
                )
            })
            .collect();
        items.push((
            "Download others… (Settings › Composers)".into(),
            MenuAction::Compose(Act::Settings),
        ));
        let (name, sub, dot) = match (&picked, &self.composer.job) {
            (None, _) => (
                "No model yet".to_string(),
                "none downloaded yet".to_string(),
                TEXT_4,
            ),
            (Some(m), Some(j)) => (
                m.known.name.to_string(),
                match &j.stage {
                    Stage::Python(_) => "getting its Python".to_string(),
                    Stage::Starting => "starting".to_string(),
                    Stage::Writing { .. } => "running".to_string(),
                },
                MINT,
            ),
            (Some(m), None) => (
                m.known.name.to_string(),
                format!(
                    "ready · hears {} bars",
                    m.known
                        .bars
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                MINT,
            ),
        };
        out.push(
            div()
                .h(px(40.))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(10.))
                .rounded(px(7.))
                .bg(rgb(FIELD))
                .cursor_pointer()
                .debug_selector(|| "compose-model".into())
                .child(div().size(px(7.)).flex_none().rounded(px(4.)).bg(rgb(dot)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(name),
                        )
                        .child(small(sub).text_size(px(11.)).whitespace_nowrap()),
                )
                .child(icon(Icon::Chevron, 12.).text_color(rgb(TEXT_3)))
                .when(!busy, |d| {
                    d.on_mouse_down(MouseButton::Left, menu(items, cx))
                })
                .into_any_element(),
        );
        // None yet: the steps to get one show where the takes will be.
        if picked.is_none() {
            return out;
        }
        let dim = |d: Div| d.when(busy, |d| d.opacity(0.5));
        let in_key = self.session.project.song_key();
        out.push(
            dim(row_ctl("In key").child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(switch(self.composer.in_key && in_key.is_some()))
                    .child(small(match in_key {
                        Some(k) => k.name(),
                        None => "the song has no key yet".into(),
                    })),
            ))
            .debug_selector(|| "compose-in-key".into())
            .when(!busy && in_key.is_some(), |d| {
                d.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| {
                        ws.composer.in_key = !ws.composer.in_key;
                        cx.notify();
                    }),
                )
            })
            .into_any_element(),
        );
        out.push(
            dim(row_ctl("Temperature").child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(self.slider_bar(Slider::Temperature, cx).flex_1())
                    .child(
                        div()
                            .w(px(32.))
                            .font_family(MONO)
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_2))
                            .child(format!("{:.2}", self.composer.temperature)),
                    ),
            ))
            .into_any_element(),
        );
        out.push(
            div()
                .pl(px(94.))
                .mt(px(-6.))
                .flex()
                .justify_between()
                .child(small("safer").text_size(px(10.5)).text_color(rgb(TEXT_4)))
                .child(
                    small("wilder")
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_4))
                        .mr(px(40.)),
                )
                .into_any_element(),
        );
        let n = self.composer.takes_n;
        let takes: Vec<(String, MenuAction)> = [1, 2, 3, 4, 6, 8]
            .iter()
            .map(|&k| {
                (
                    format!("{}{k}", if k == n { "✓ " } else { "" }),
                    MenuAction::Compose(Act::Takes(k)),
                )
            })
            .collect();
        out.push(
            dim(row_ctl("Takes").child(
                dropdown(format!("{n} at a time"), 26.)
                    .w(px(130.))
                    .debug_selector(|| "compose-takes".into())
                    .when(!busy, |d| {
                        d.on_mouse_down(MouseButton::Left, menu(takes, cx))
                    }),
            ))
            .into_any_element(),
        );
        if let Some(m) = &picked {
            out.push(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .mt(px(2.))
                    .child(chip(licence_short(m.known), AMBER))
                    .child(
                        small("Licence ↗")
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .on_mouse_down(MouseButton::Left, {
                                let url = m.known.licence_url;
                                cx.listener(move |_, _, _, cx| cx.open_url(url))
                            }),
                    )
                    .into_any_element(),
            );
        }
        out
    }

    /// First run, in place: signing in, the download, and what comes with the first write.
    fn compose_setup(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let k: &'static Known = &KNOWN[0];
        let signed = self.composer.signed_in();
        let going = self.composer.downloads.get(k.id).cloned();
        let step = |n: &'static str, done: bool, title: String, what: String| {
            div()
                .flex()
                .gap(px(9.))
                .py(px(6.))
                .border_t_1()
                .border_color(rgb(LINE_SOFT))
                .child(
                    div()
                        .size(px(18.))
                        .flex_none()
                        .rounded(px(9.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .bg(match done {
                            true => alpha(MINT, 0.16),
                            false => rgb(RAISED).into(),
                        })
                        .text_color(rgb(if done { MINT } else { TEXT_3 }))
                        .child(if done { "✓" } else { n }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .child(title),
                        )
                        .child(small(what)),
                )
        };
        let mut out = vec![
            step(
                "1",
                signed,
                match signed {
                    true => "Signed in to Hugging Face".into(),
                    false => "Sign in at Hugging Face".into(),
                },
                match signed {
                    true => "MIDI-GPT's makers ask for their terms to be accepted with the same account".into(),
                    false => "Accept MIDI-GPT's terms there, then give Pina Studio a token in Settings › Composers".into(),
                },
            )
            .when(!signed, |d| {
                d.child(
                    button("Sign in", false)
                        .debug_selector(|| "compose-sign-in".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.show_settings(Section::Composers, cx)),
                        ),
                )
            })
            .into_any_element(),
        ];
        let downloading = going.as_ref().map(|(done, _)| {
            done.load(std::sync::atomic::Ordering::Relaxed) as f32 / k.bytes as f32
        });
        let mut get = step(
            "2",
            false,
            format!("Download {}", k.name.trim_start_matches("MIDI-GPT ")),
            format!(
                "{} MB, from its makers' page, on their terms",
                (k.bytes as f64 / 1e6).round()
            ),
        );
        get = match (downloading, going) {
            (Some(at), Some((_, stop))) => get.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .w(px(34.))
                            .font_family(MONO)
                            .text_size(px(11.))
                            .child(format!("{:.0}%", at * 100.0)),
                    )
                    .child(button("Stop", false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _, _, _| {
                            stop.store(true, std::sync::atomic::Ordering::Relaxed)
                        }),
                    )),
            ),
            _ => get.child(
                button("Download", false)
                    .debug_selector(|| "compose-download".into())
                    .when(signed, |d| {
                        d.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| ws.compose_download(k, cx)),
                        )
                    })
                    .when(!signed, |d| d.opacity(0.5)),
            ),
        };
        out.push(get.into_any_element());
        if let Some(at) = downloading {
            out.push(div().flex().child(bar(at)).into_any_element());
        }
        out.push(
            step(
                "3",
                false,
                "Its own Python, by uv".into(),
                "Comes with the first write, apart from anything else on this computer · about 690 MB, once for every MIDI-GPT model".into(),
            )
            .into_any_element(),
        );
        out.push(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(chip(licence_short(k), AMBER))
                .child(
                    small("Licence ↗")
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, {
                            let url = k.licence_url;
                            cx.listener(move |_, _, _, cx| cx.open_url(url))
                        }),
                )
                .into_any_element(),
        );
        out
    }

    /// The takes: each a row to pick (and hear in the song), the one being written as it goes,
    /// and the main action at the foot.
    fn compose_takes(&mut self, cx: &mut Context<Self>) -> Div {
        let bar = self.session.project.bar().max(1);
        let c = &self.composer;
        let target = c.target.clone().or_else(|| {
            let p = &self.session.project;
            Some(Target {
                tracks: self
                    .compose_tracks()
                    .iter()
                    .map(|&t| p.tracks[t].id)
                    .collect(),
                bars: self.compose_bars().0,
            })
        });
        let (a, b) = target.as_ref().map_or((0, bar * 4), |t| {
            (t.bars.0 as u64 * bar, t.bars.1 as u64 * bar)
        });
        let colors: Vec<(u64, u32)> = target
            .as_ref()
            .map(|t| {
                t.tracks
                    .iter()
                    .filter_map(|id| {
                        let i = self
                            .session
                            .project
                            .tracks
                            .iter()
                            .position(|tr| tr.id == *id)?;
                        Some((*id, self.color(i)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let strip_h = 21.0 * colors.len().clamp(1, 3) as f32;
        let parts_of = |notes: &dyn Fn(u64) -> Vec<Note>| -> Vec<(Vec<Note>, u32)> {
            colors.iter().map(|&(id, col)| (notes(id), col)).collect()
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        for (k, take) in c.takes.iter().enumerate() {
            let on = c.picked == Some(k);
            let parts = parts_of(&|id| {
                take.notes
                    .iter()
                    .find(|(t, _)| *t == id)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default()
            });
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .mx(px(8.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(7.))
                    .relative()
                    .cursor_pointer()
                    .when(on, |d| {
                        d.bg(rgb(HEADER_SELECTED)).child(
                            div()
                                .absolute()
                                .left(px(-6.))
                                .top(px(8.))
                                .bottom(px(8.))
                                .w(px(2.))
                                .rounded(px(1.))
                                .bg(rgb(ACCENT)),
                        )
                    })
                    .when(!on, |d| d.hover(|s| s.bg(rgb(STRIP))))
                    .debug_selector(move || format!("compose-take-{k}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.compose_pick(k, cx)),
                    )
                    .child(
                        div()
                            .w(px(16.))
                            .flex_none()
                            .font_family(MONO)
                            .text_size(px(12.))
                            .text_color(rgb(TEXT_3))
                            .child(format!("{}", k + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(strips(parts, a, b, bar, strip_h)),
                    )
                    .child(
                        div()
                            .w(px(84.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .items_end()
                            .gap(px(2.))
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{} notes", take.count())),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_4))
                                    .child(match c.kept == Some(k) {
                                        true => "kept".to_string(),
                                        false => format!("seed {}", take.seed),
                                    }),
                            ),
                    )
                    .child(
                        icon_seg(Icon::Play, false, 24.)
                            .debug_selector(move || format!("compose-hear-{k}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.compose_hear(k, cx)
                                }),
                            ),
                    )
                    .into_any_element(),
            );
        }
        if let Some(j) = &c.job
            && let Stage::Writing { take, of } = j.stage
        {
            let live = parts_of(&|id| {
                j.live
                    .iter()
                    .filter(|(t, _)| *t == id)
                    .map(|(_, n)| n.clone())
                    .collect()
            });
            let at = j
                .live
                .iter()
                .map(|(_, n)| n.start)
                .max()
                .unwrap_or(a as i64);
            rows.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .mx(px(8.))
                    .px(px(8.))
                    .py(px(6.))
                    .debug_selector(|| "compose-writing".into())
                    .child(
                        div()
                            .w(px(16.))
                            .font_family(MONO)
                            .text_size(px(12.))
                            .text_color(rgb(TEXT_3))
                            .child(format!("{}", c.takes.len() + 1)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(strips(live, a, b, bar, strip_h)),
                    )
                    .child(
                        div()
                            .w(px(108.))
                            .flex_none()
                            .text_right()
                            .font_family(MONO)
                            .text_size(px(11.))
                            .text_color(rgb(ACCENT_TEXT))
                            .child(format!("writing · bar {}", at.max(0) as u64 / bar + 1)),
                    )
                    .into_any_element(),
            );
            for k in take..of {
                rows.push(
                    div()
                        .mx(px(16.))
                        .my(px(3.))
                        .h(px(strip_h))
                        .rounded(px(5.))
                        .bg(rgb(WELL))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_4))
                        .child(format!("take {} · waiting", k + 1))
                        .into_any_element(),
                );
            }
        }
        let empty = rows.is_empty();
        let job = c
            .job
            .as_ref()
            .map(|j| (j.stage.clone(), j.started, j.tokens));
        let foot = self.compose_foot(job, cx);
        let setup = match empty && self.compose_model().is_none() {
            true => Some(self.compose_setup(cx)),
            false => None,
        };
        let ready = self.compose_model().is_some();
        let c = &self.composer;
        div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px(px(16.))
                    .pt(px(12.))
                    .pb(px(6.))
                    .child(caption("TAKES"))
                    .child(small(match (c.takes.is_empty(), c.picked) {
                        (false, Some(_)) => "the one picked plays in the song · Space",
                        _ => "",
                    })),
            )
            .child(
                div()
                    .id("compose-takes")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(rows)
                    .when_some(setup, |d, steps| {
                        d.child(
                            div()
                                .px(px(16.))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .max_w(px(520.))
                                .children(steps),
                        )
                    })
                    .when(empty && ready, |d| {
                        d.child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(6.))
                                .px(px(24.))
                                .text_size(px(12.5))
                                .text_color(rgb(TEXT_3))
                                .child(match &c.error {
                                    Some(e) => div().text_color(rgb(TEXT_2)).child(e.clone()),
                                    None => div().child(
                                        "Each take writes the parts together, hearing the rest of the song",
                                    ),
                                })
                                .when(c.error.is_none(), |d| {
                                    d.child(small("Picked, a take plays in the song in place of what's there"))
                                }),
                        )
                    })
                    .when(!empty && c.error.is_some(), |d| {
                        d.child(
                            div()
                                .mx(px(16.))
                                .my(px(6.))
                                .text_size(px(12.))
                                .text_color(rgb(TEXT_2))
                                .child(c.error.clone().unwrap_or_default()),
                        )
                    }),
            )
            .child(foot)
    }

    /// The foot: Write, or Stop while it's writing, or Keep the take picked.
    fn compose_foot(
        &mut self,
        job: Option<(Stage, std::time::Instant, u64)>,
        cx: &mut Context<Self>,
    ) -> Div {
        let c = &self.composer;
        let n = c.takes_n;
        let ready = self.compose_model().is_some() && !self.compose_tracks().is_empty();
        let foot = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.))
            .px(px(16.))
            .py(px(10.))
            .border_t_1()
            .border_color(rgb(LINE));
        if let Some((stage, started, tokens)) = job {
            let doing = match stage {
                Stage::Python(line) => line,
                Stage::Starting => "Starting the model".into(),
                Stage::Writing { take, of } => format!("Take {take} of {of} · {tokens} tokens"),
            };
            return foot
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_2))
                        .debug_selector(|| "compose-stage".into())
                        .child(doing),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .child(elapsed(started)),
                )
                .child(
                    button("Stop", false)
                        .debug_selector(|| "compose-stop".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.compose_stop(cx)),
                        ),
                );
        }
        let write = |label: String| {
            primary(label)
                .debug_selector(|| "compose-write".into())
                .when(!ready, |d| d.opacity(0.45))
                .when(ready, |d| {
                    d.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.compose_write(cx)),
                    )
                })
        };
        let takes = |n: u32| match n {
            1 => "a take".to_string(),
            n => format!("{n} takes"),
        };
        match (c.picked, c.kept) {
            (Some(k), _) => foot
                .child(
                    button("Clear", false)
                        .h(px(30.))
                        .debug_selector(|| "compose-clear".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.compose_clear(cx)),
                        ),
                )
                .child(div().flex_1())
                .child(
                    button(format!("{} more", n), false)
                        .h(px(30.))
                        .debug_selector(|| "compose-more".into())
                        .when(ready, |d| {
                            d.on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.compose_write(cx)),
                            )
                        }),
                )
                .child(
                    primary(format!("Keep take {}", k + 1))
                        .debug_selector(|| "compose-keep".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.compose_keep(cx)),
                        ),
                ),
            (None, Some(k)) => foot
                .child(
                    small(format!("Take {} kept · ⌘Z takes it back", k + 1))
                        .flex_1()
                        .text_color(rgb(TEXT_2)),
                )
                .child(write(format!("Write {}", takes(n)))),
            (None, None) => foot.child(write(format!("Write {}", takes(n)))),
        }
    }

    /// On the lanes, while Compose is open: the bars to write on each track picked, the take
    /// picked (or the one being written) drawn in them.
    pub(crate) fn compose_overlay(&self) -> Vec<AnyElement> {
        if self.bottom != Some(crate::workspace::Bottom::Compose) {
            return Vec::new();
        }
        let p = &self.session.project;
        let c = &self.composer;
        let target = c.target.clone().unwrap_or_else(|| Target {
            tracks: self
                .compose_tracks()
                .iter()
                .map(|&t| p.tracks[t].id)
                .collect(),
            bars: self.compose_bars().0,
        });
        let bar = p.bar().max(1);
        let (a, b) = (target.bars.0 as u64 * bar, target.bars.1 as u64 * bar);
        let (ppt, sx) = (self.px_per_tick(), self.scroll_x);
        let (x, w) = (a as f32 * ppt - sx, (b - a) as f32 * ppt);
        let writing = c
            .job
            .as_ref()
            .is_some_and(|j| matches!(j.stage, Stage::Writing { .. }));
        let shown = c.picked.and_then(|k| c.takes.get(k).map(|t| (k, t)));
        let mut out = Vec::new();
        for id in &target.tracks {
            let Some(t) = p.tracks.iter().position(|tr| tr.id == *id) else {
                continue;
            };
            let h = self.lane_h(t);
            if h < 12.0 {
                continue;
            }
            let color = self.color(t);
            let notes: Vec<Note> = match (writing, shown) {
                (true, _) => c
                    .job
                    .iter()
                    .flat_map(|j| j.live.iter())
                    .filter(|(tr, _)| tr == id)
                    .map(|(_, n)| n.clone())
                    .collect(),
                (false, Some((_, take))) => take
                    .notes
                    .iter()
                    .find(|(tr, _)| tr == id)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let label = match (writing, shown) {
                (true, _) => "Writing…".to_string(),
                (false, Some((k, _))) => format!("Take {}", k + 1),
                _ => "Compose".to_string(),
            };
            let ink = match notes.is_empty() && !writing {
                true => ACCENT,
                false => color,
            };
            let span = (b - a).max(1);
            out.push(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(self.row_top(t) + 3.))
                    .w(px(w.max(4.)))
                    .h(px(h - 6.))
                    .rounded(px(6.))
                    .border_1()
                    .border_dashed()
                    .border_color(alpha(ink, 0.85))
                    .bg(alpha(ink, 0.08))
                    .overflow_hidden()
                    .debug_selector(move || format!("compose-lane-{t}"))
                    .child(
                        div()
                            .absolute()
                            .left(px(7.))
                            .top(px(4.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(tint(ink, 0.2)))
                            .whitespace_nowrap()
                            .child(label),
                    )
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let (Some(lo), Some(hi)) = (
                                    notes.iter().map(|n| n.pitch).min(),
                                    notes.iter().map(|n| n.pitch).max(),
                                ) else {
                                    return;
                                };
                                let (bw, bh) =
                                    (f32::from(bounds.size.width), f32::from(bounds.size.height));
                                let top = 20.0_f32.min(bh * 0.4);
                                for n in &notes {
                                    let nx = (n.start - a as i64) as f32 / span as f32 * bw;
                                    let nw = (n.length as f32 / span as f32 * bw - 1.0).max(1.5);
                                    let ny = top
                                        + (1.0 - (n.pitch - lo) as f32 / (hi - lo).max(1) as f32)
                                            * (bh - top - 6.0);
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(
                                                bounds.origin.x + px(nx),
                                                bounds.origin.y + px(ny),
                                            ),
                                            size(px(nw), px(2.)),
                                        ),
                                        rgb(color),
                                    ));
                                }
                            },
                        )
                        .absolute()
                        .inset_0(),
                    )
                    .into_any_element(),
            );
        }
        out
    }
}

/// A control's row: its name, then the control.
fn row_ctl(label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .min_h(px(30.))
        .child(
            div()
                .w(px(86.))
                .flex_none()
                .text_size(px(12.))
                .text_color(rgb(TEXT_3))
                .child(label),
        )
}

/// A licence as a chip says it: what its makers allow, short.
pub fn licence_short(k: &Known) -> &'static str {
    match k.licence.contains("Non-Commercial") {
        true => "Non-commercial research only",
        false => "See its licence",
    }
}
