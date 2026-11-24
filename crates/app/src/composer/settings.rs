//! Settings › Composers: how Compose works, signing in for the models whose makers ask, each model
//! to download (or remove) with its licence, and where they run.

use std::sync::atomic::Ordering;

use gpui::{Context, Div, FontWeight, MouseButton, div, prelude::*, px, rgb};
use pina_engine::composer::{
    self,
    catalog::{KNOWN, Known},
    server,
};

use super::sheet::{Act, licence_short};
use crate::{
    pages::{
        settings::Section,
        studio::{card, row, row_with},
    },
    ui::{
        menu::MenuAction,
        text::{TextEvent, TextInput},
        theme::*,
    },
    workspace::{Bottom, Workspace},
};

impl Workspace {
    /// What a menu in Compose or its settings asked for.
    pub(crate) fn compose_act(&mut self, a: Act, cx: &mut Context<Self>) {
        match a {
            Act::Model(id) => self.composer.model = Some(id),
            Act::Part(id, part) => match part {
                Some(p) => {
                    self.composer.parts.insert(id, p);
                }
                None => {
                    self.composer.parts.remove(&id);
                }
            },
            Act::Steer(id, name, level) => {
                let s = self.composer.steer.entry(id).or_default();
                match level {
                    Some(l) => {
                        s.insert(name, l);
                    }
                    None => {
                        s.remove(&name);
                    }
                }
            }
            Act::Plays(id, p) => p.apply(self.composer.steer.entry(id).or_default()),
            Act::Takes(n) => self.composer.takes_n = n.clamp(1, 16),
            Act::OnCpu(on) => {
                self.settings.compose_on_cpu = on;
                self.settings.save();
            }
            Act::Settings => self.show_settings(Section::Composers, cx),
            Act::Open(t) => {
                if !self.is_picked(t) {
                    self.track = t;
                    self.tracks_picked.clear();
                }
                self.open_compose(cx);
            }
        }
        cx.notify();
    }

    /// Settings, open at `section`.
    pub(crate) fn show_settings(&mut self, section: Section, cx: &mut Context<Self>) {
        self.settings_section = section;
        if self.view != crate::workspace::View::Settings {
            self.open_settings(cx);
        }
        if section == Section::Composers {
            self.compose_check_up(cx);
        }
        cx.notify();
    }

    /// Compose open under the song.
    pub(crate) fn open_compose(&mut self, cx: &mut Context<Self>) {
        self.show_bottom(Bottom::Compose, cx);
    }

    /// The field the token's typed in: Enter keeps it.
    fn compose_token_field(&mut self, cx: &mut Context<Self>) -> gpui::Entity<TextInput> {
        if let Some(t) = &self.composer.token {
            return t.clone();
        }
        let t = cx.new(|cx| TextInput::form("", "hf_…", cx));
        cx.subscribe(&t, |ws, _, e: &TextEvent, cx| {
            if let TextEvent::Confirm(text) = e {
                ws.compose_save_token(&text.clone(), cx);
            }
        })
        .detach();
        self.composer.token = Some(t.clone());
        t
    }

    /// Settings › Composers.
    pub(crate) fn composers_page(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let step = |n: &'static str, title: &'static str, what: &'static str| {
            let number = div()
                .size(px(22.))
                .flex_none()
                .rounded(px(11.))
                .bg(rgb(ACCENT))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0xffffff))
                .child(n);
            row_with(Some(number), title, None, what, div())
        };
        let how = card(
            "How it works",
            None,
            vec![
                step(
                    "1",
                    "Accept a model's terms where its makers host it, and sign in below",
                    "MIDI-GPT's makers ask everyone who downloads it to. Which you use, and on what terms, is up to you",
                ),
                step(
                    "2",
                    "Download a model",
                    "The first write brings its own Python with it (uv, about 690 MB, once), apart from anything else on this computer",
                ),
                step(
                    "3",
                    "Pick the tracks in the headers and loop the bars, then Compose › Write",
                    "The parts are written together, each hearing the others and the rest of the song, in its key",
                ),
                step(
                    "4",
                    "Hear each take in the song, and keep one",
                    "Picked, a take plays in place of what's there. One undo takes a kept one back",
                ),
            ],
        );
        // Signing in.
        let signed = self.composer.signed_in();
        let field = self.compose_token_field(cx);
        let hf = match signed {
            true => card(
                "Hugging Face",
                Some("for models whose makers ask you to sign in"),
                vec![row(
                    "Signed in",
                    "With the token kept where hf auth login keeps it (~/.cache/huggingface/token), for every Hugging Face tool",
                    button("Use another token", false)
                        .debug_selector(|| "composers-token-again".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.composer.signed_in = Some(false);
                                cx.notify();
                            }),
                        ),
                )],
            ),
            false => card(
                "Hugging Face",
                Some("for models whose makers ask you to sign in"),
                vec![
                    row(
                        "Not signed in",
                        "Paste an access token (read is enough) and press Enter. It's kept where hf auth login keeps it",
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .w(px(220.))
                                    .h(px(28.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.))
                                    .bg(rgb(FIELD))
                                    .font_family(MONO)
                                    .text_size(px(12.))
                                    .debug_selector(|| "composers-token".into())
                                    .child(field.clone()),
                            )
                            .child(
                                button("Save", false)
                                    .debug_selector(|| "composers-token-save".into())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            let t = field.read(cx).text().to_string();
                                            ws.compose_save_token(&t, cx);
                                        }),
                                    ),
                            ),
                    ),
                    row(
                        "Their terms",
                        "Accept them on the model's page with the same account first",
                        button("Open MIDI-GPT's page", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, _, cx| cx.open_url(KNOWN[0].page)),
                        ),
                    ),
                    row(
                        "A token",
                        "Made in your Hugging Face settings, under Access Tokens",
                        button("Open Access Tokens", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, _, cx| {
                                cx.open_url("https://huggingface.co/settings/tokens")
                            }),
                        ),
                    ),
                ],
            ),
        };
        // The models.
        let dir = composer::folder();
        let have = composer::installed(&dir);
        let downloads: Vec<Div> = KNOWN
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let k: &'static Known = k;
                let going = self.composer.downloads.get(k.id).cloned();
                let right = match going {
                    Some((done, stop)) => div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .min_w(px(34.))
                                .font_family(MONO)
                                .text_size(px(11.5))
                                .child(format!(
                                    "{}%",
                                    done.load(Ordering::Relaxed) * 100 / k.bytes.max(1)
                                )),
                        )
                        .child(
                            button("Stop", false)
                                .debug_selector(move || format!("composer-stop-{i}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |_, _, _, _| {
                                        stop.store(true, Ordering::Relaxed)
                                    }),
                                ),
                        ),
                    None if have.iter().any(|h| h.known.id == k.id) => div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(rgb(TEXT_2))
                                .child("Downloaded ✓"),
                        )
                        .child(
                            button("Remove", false)
                                .debug_selector(move || format!("composer-remove-{i}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.compose_remove(k, cx)),
                                ),
                        ),
                    None => div().child(
                        button(
                            format!("Download · {} MB", (k.bytes as f64 / 1e6).round()),
                            false,
                        )
                        .debug_selector(move || format!("composer-download-{i}"))
                        .when(!k.gated || signed, |d| {
                            d.on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.compose_download(k, cx)),
                            )
                        })
                        .when(k.gated && !signed, |d| d.opacity(0.5)),
                    ),
                };
                let licence = div()
                    .flex_none()
                    .px(px(6.))
                    .py(px(1.))
                    .rounded(px(5.))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(AMBER))
                    .bg(rgb(WELL))
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, {
                        let url = k.licence_url;
                        cx.listener(move |_, _, _, cx| cx.open_url(url))
                    })
                    .child(licence_short(k));
                row_with(None, k.name, Some(licence), k.about, right)
            })
            .collect();
        let get = card(
            "Download a model",
            Some("from where its makers host it"),
            downloads,
        );
        // Where they run.
        let family = KNOWN[0].family;
        let python = server::ready(family, &dir);
        let on_cpu = self.settings.compose_on_cpu;
        let device_items = vec![
            (
                format!(
                    "{}Auto: the GPU where there is one",
                    if on_cpu { "" } else { "✓ " }
                ),
                MenuAction::Compose(Act::OnCpu(false)),
            ),
            (
                format!("{}The CPU", if on_cpu { "✓ " } else { "" }),
                MenuAction::Compose(Act::OnCpu(true)),
            ),
        ];
        let up = self.composer.up.clone();
        let up_name = up
            .as_deref()
            .and_then(|id| Known::named(id))
            .map(|k| k.name);
        let running = card(
            "Running",
            None,
            vec![
                row(
                    format!("{}'s Python", family.name()),
                    match python {
                        true => format!("Installed: {} · uv's Python 3.12", family.package()),
                        false => "Comes with the first write, made by uv apart from anything else on this computer".into(),
                    },
                    div().text_size(px(11.5)).text_color(rgb(TEXT_2)).child(match python {
                        true => "Installed ✓",
                        false => "Not yet",
                    }),
                ),
                row(
                    "Runs on",
                    "A model loads in a few seconds and stays up between takes; it stops itself after ten idle minutes",
                    dropdown(
                        match on_cpu {
                            true => "The CPU",
                            false => "Auto",
                        },
                        26.,
                    )
                    .w(px(150.))
                    .debug_selector(|| "composers-device".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, e: &gpui::MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            ws.open_menu(e.position, device_items.clone(), cx);
                        }),
                    ),
                ),
                row(
                    match up_name {
                        Some(n) => format!("{n} is up"),
                        None => "Nothing running".into(),
                    },
                    match up_name {
                        Some(_) => "On this computer only, waiting for the next write".to_string(),
                        None => "Started when you write, on this computer only".to_string(),
                    },
                    match up_name {
                        Some(_) => div().child(
                            button("Stop it", false)
                                .debug_selector(|| "composers-stop-server".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.compose_stop_server(cx)),
                                ),
                        ),
                        None => div(),
                    },
                ),
                row(
                    "Folder",
                    dir.display().to_string(),
                    button("Show", false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _, _, _| {
                            let dir = composer::folder();
                            let _ = std::fs::create_dir_all(&dir);
                            crate::platform::reveal(&dir);
                        }),
                    ),
                ),
            ],
        );
        let mut out = vec![how, hf, get, running];
        if let Some(e) = &self.composer.error {
            out.insert(
                0,
                div()
                    .px(px(14.))
                    .py(px(10.))
                    .rounded(px(10.))
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(LINE))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_2))
                    .child(e.clone()),
            );
        }
        out
    }
}
