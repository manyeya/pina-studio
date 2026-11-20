//! About Pina Studio (Settings › About Pina Studio, and the app menu's About): who made it, which
//! licence covers what, and the open-source software it's built on, credited. Pina Studio is
//! designed and written by Manyeya Khutso Siema. Every crate in it is listed (`credits.rs`, made
//! from Cargo.lock by scripts/credits.py), the projects it leans on most named first.

use std::sync::Arc;

use gpui::{
    Context, Div, FontWeight, Image, ImageFormat, MouseButton, div, img, prelude::*, px, rgb,
};

use crate::{credits::CRATES, ui::theme::*, workspace::Workspace};

/// Pina Studio's author.
pub const AUTHOR: &str = "Manyeya Khutso Siema";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COPYRIGHT: &str = "© 2026 Manyeya Khutso Siema";
const AUTHOR_URL: &str = "https://github.com/manyeya";
/// The author's own site.
const HOME: &str = "https://www.manyeya.dev";
const SITE: &str = "https://manyeya.github.io/pina-studio/";
const SOURCE: &str = "https://github.com/manyeya/pina-studio";
const FILE: &str = "https://github.com/manyeya/pina-studio/blob/main/";

/// What Pina Studio is built on, most of all: its name, what it does here, its licence, its home.
pub const BUILT_ON: &[(&str, &str, &str, &str)] = &[
    (
        "GPUI",
        "The whole interface, drawn on the GPU. By Zed Industries.",
        "Apache-2.0",
        "https://github.com/zed-industries/zed",
    ),
    (
        "Symphonia",
        "Reading MP3, AAC, FLAC, WAV, AIFF and Ogg. By Philip Deljanov and contributors.",
        "MPL-2.0",
        "https://github.com/pdeljanov/Symphonia",
    ),
    (
        "Rubber Band Library",
        "Time-stretching and pitch-shifting audio. By Chris Cannam, Particular Programs.",
        "GPL-2.0-or-later",
        "https://breakfastquay.com/rubberband/",
    ),
    (
        "ONNX Runtime and ort",
        "Running the stem separation models. By Microsoft, and pyke.",
        "MIT",
        "https://onnxruntime.ai",
    ),
    (
        "whisper.cpp",
        "Hearing the words sung, for lyrics. By Georgi Gerganov and the ggml authors.",
        "MIT",
        "https://github.com/ggml-org/whisper.cpp",
    ),
    (
        "LAME",
        "Bouncing to MP3.",
        "LGPL-3.0",
        "https://lame.sourceforge.io",
    ),
    (
        "VST 3 SDK and CLAP",
        "Hosting plugins. By Steinberg (VST is its registered trademark), and the CLAP team.",
        "MIT",
        "https://github.com/free-audio/clap",
    ),
    (
        "cpal and midir",
        "Sound in and out, and MIDI. By RustAudio, and Patrick Reisert.",
        "Apache-2.0, MIT",
        "https://github.com/RustAudio/cpal",
    ),
    (
        "RustFFT",
        "Fast Fourier transforms, for the song study and the spectra. By Elliott Mahler and contributors.",
        "MIT OR Apache-2.0",
        "https://github.com/ejmahler/RustFFT",
    ),
    (
        "Geist and Geist Mono",
        "The type. By Vercel.",
        "OFL-1.1",
        "https://vercel.com/font",
    ),
    ("Lucide", "The icons' designs.", "ISC", "https://lucide.dev"),
];

impl Workspace {
    pub(crate) fn about_page(&mut self, cx: &mut Context<Self>) -> Vec<Div> {
        let card = || {
            div()
                .rounded(px(12.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
        };
        let caption = |t: &'static str| {
            div()
                .pt(px(8.))
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_4))
                .child(t)
        };
        let link = |label: &'static str, url: String, cx: &mut Context<Self>| {
            button(label, false).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, _, cx| cx.open_url(&url)),
            )
        };
        // A row that opens its project's home.
        let row = |url: &'static str, cx: &mut Context<Self>| {
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .px(px(14.))
                .border_b_1()
                .border_color(rgb(LINE_SOFT))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(OFF)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |_, _, _, cx| cx.open_url(url)),
                )
        };

        let head = card().p(px(18.)).child(
            div()
                .flex()
                .items_center()
                .gap(px(16.))
                .child(
                    img(Arc::new(Image::from_bytes(
                        ImageFormat::Svg,
                        include_bytes!("../../icon/pina.svg").to_vec(),
                    )))
                    .size(px(72.))
                    .flex_none(),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(px(22.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Pina Studio"),
                        )
                        .child(div().text_color(rgb(TEXT_3)).child(format!(
                            "Version {VERSION}  ·  A DAW where agents work in your song with you"
                        ))),
                ),
        );

        let made_by = card()
            .p(px(18.))
            .gap(px(8.))
            .child(caption("MADE BY"))
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(AUTHOR),
            )
            .child(div().text_color(rgb(TEXT_2)).child(
                "Pina Studio is designed and written by Manyeya Khutso Siema: the arrangement, the \
                 editors, the mixer and its effects, the synths and samplers, the song study, the \
                 Studio Agent and the engine under all of it. Manyeya Khutso Siema made Pina Live \
                 too, Pina Studio's sister app for DJs, which plays through the same engine.",
            ))
            .child(
                div()
                    .pt(px(4.))
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(link("manyeya.dev", HOME.into(), cx))
                    .child(link("github.com/manyeya", AUTHOR_URL.into(), cx))
                    .child(link("Pina Studio's website", SITE.into(), cx))
                    .child(link("The source", SOURCE.into(), cx)),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .child(COPYRIGHT),
            );

        let licence = card()
            .p(px(18.))
            .gap(px(8.))
            .child(caption("LICENCE"))
            .child(div().text_color(rgb(TEXT_2)).child(
                "Pina Studio's source code is free software under the MIT License. This build \
                 includes the Rubber Band Library, which is under the GNU GPL, so the build as a whole \
                 is distributed under the GNU General Public License, version 3 or later; its source \
                 is Pina Studio's repository. Every other open-source component is listed below and \
                 in the third-party notices, each with its licence.",
            ))
            .child(
                div()
                    .pt(px(4.))
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(link("The MIT License", format!("{FILE}LICENSE"), cx))
                    .child(link("The GNU GPL v3", format!("{FILE}COPYING"), cx))
                    .child(link("Which covers what", format!("{FILE}NOTICE"), cx))
                    .child(link(
                        "Third-party notices",
                        format!("{FILE}THIRD-PARTY-NOTICES.md"),
                        cx,
                    )),
            );

        let built_on = BUILT_ON
            .iter()
            .fold(card(), |c, &(name, what, license, url)| {
                c.child(
                    row(url, cx)
                        .py(px(10.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(name),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(rgb(TEXT_3))
                                        .child(what),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font_family(MONO)
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .child(license),
                        ),
                )
            });

        // Every crate: how many under each licence, then (asked for) each one.
        let mut tally: Vec<(&str, usize)> = Vec::new();
        for c in CRATES {
            match tally.iter_mut().find(|(l, _)| *l == c.license) {
                Some((_, n)) => *n += 1,
                None => tally.push((c.license, 1)),
            }
        }
        tally.sort_by_key(|t| std::cmp::Reverse(t.1));
        let tally = tally
            .iter()
            .map(|(l, n)| format!("{l} {n}"))
            .collect::<Vec<_>>()
            .join("  ·  ");
        let all = self.about_all;
        let mut every = card().child(
            div()
                .p(px(14.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .border_b_1()
                .border_color(rgb(LINE_SOFT))
                .child(div().text_color(rgb(TEXT_2)).child(format!(
                    "Pina Studio is built from {} open-source crates. Thank you to everyone who \
                         wrote them.",
                    CRATES.len()
                )))
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child(tally),
                )
                .child(
                    div().flex().child(
                        button(
                            match all {
                                true => "Hide the list".to_string(),
                                false => format!("Show all {}", CRATES.len()),
                            },
                            all,
                        )
                        .debug_selector(|| "about-all".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                ws.about_all = !ws.about_all;
                                cx.notify();
                            }),
                        ),
                    ),
                ),
        );
        if all {
            for c in CRATES {
                every = every.child(
                    row(c.url, cx)
                        .h(px(28.))
                        .text_size(px(11.5))
                        .child(
                            div()
                                .w(px(180.))
                                .flex_none()
                                .text_color(rgb(TEXT_2))
                                .truncate()
                                .child(c.name),
                        )
                        .child(
                            div()
                                .w(px(96.))
                                .flex_none()
                                .font_family(MONO)
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_4))
                                .truncate()
                                .child(c.version),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_color(rgb(TEXT_3))
                                .truncate()
                                .child(c.by),
                        )
                        .child(
                            div()
                                .flex_none()
                                .font_family(MONO)
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .child(c.license),
                        ),
                );
            }
        }

        vec![
            head,
            made_by,
            licence,
            caption("BUILT ON"),
            built_on,
            caption("EVERY OPEN-SOURCE COMPONENT"),
            every,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The credits are Cargo.lock's: every crate listed is there at that version (run
    /// scripts/credits.py again after a dependency changes).
    #[test]
    fn credits_match_cargo_lock() {
        // Checked out on Windows with CRLF line ends: compared as LF.
        let lock = include_str!("../../../../Cargo.lock").replace("\r\n", "\n");
        for c in CRATES {
            let entry = format!("name = \"{}\"\nversion = \"{}\"", c.name, c.version);
            assert!(
                lock.contains(&entry),
                "{} {} isn't in Cargo.lock",
                c.name,
                c.version
            );
        }
        for name in ["gpui-pre", "symphonia", "rubberband", "whisper-rs", "ort"] {
            assert!(
                CRATES.iter().any(|c| c.name == name),
                "{name} isn't credited"
            );
        }
    }
}
