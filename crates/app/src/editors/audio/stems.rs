//! An audio region parted into its stems by a separation model the person has added (Pina
//! Studio comes with none; see `pina_engine::separate`), from the Separate Stems sheet: the
//! models there are to pick from (what each gives, how well, its licence, how long this file
//! will take, or that it's done already), more to download, which stems to keep and whether the
//! region's muted; then how it's going (reading, separating with its time left, putting the
//! stems in), to stop or hide. Each stem goes on a track of its own, in a stack, playing as the
//! region did. What a model made for a file is kept, so asking again is instant.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use gpui::{AppContext, Context, Div, FontWeight, MouseButton, div, prelude::*, px, rgb};
use pina_engine::{
    separate::{self, Model, catalog::Known},
    song::project::{At, Content},
};

use crate::{
    sounds::browser::primary,
    ui::{menu::MenuAction, theme::*},
    workspace::{Workspace, context::RegionOp},
};

/// A separation running.
pub struct Separating {
    pub model: String,
    pub at: At,
    /// How far it's got, in thousandths (1000: writing the stems).
    pub done: Arc<AtomicU32>,
    pub stop: Arc<AtomicBool>,
    pub started: Instant,
    /// Made before, by this model: only put in.
    pub kept: bool,
}

/// The Separate Stems sheet, for the region at `at` (its file `file`, `seconds` long).
pub struct StemSheet {
    pub at: At,
    pub file: PathBuf,
    pub seconds: f32,
    /// The model picked, by name.
    pub model: Option<String>,
    /// Its stems left out.
    pub without: Vec<String>,
    /// The region muted once its stems are in.
    pub mute: bool,
    /// What went wrong the last time.
    pub failed: Option<String>,
}

/// The models there are, the tested ones first in their order (the best first).
fn installed() -> Vec<Model> {
    let (mut models, _) = separate::models(&separate::folder());
    models.sort_by_key(|m| {
        separate::catalog::KNOWN
            .iter()
            .position(|k| k.about().name == m.about.name)
            .unwrap_or(usize::MAX)
    });
    models
}

/// A model's stems, its rest after them.
fn stems_of(m: &Model) -> Vec<String> {
    m.about.stems.iter().chain(&m.about.rest).cloned().collect()
}

/// Whether `file`'s stems by `m` are made already.
fn kept(dir: &std::path::Path, file: &std::path::Path, m: &Model) -> bool {
    let folder = dir.join(separate::stems_folder(file, &m.about.name));
    stems_of(m)
        .iter()
        .all(|s| folder.join(format!("{s}.wav")).is_file())
}

/// A span of time as people say it.
fn about(seconds: f32) -> String {
    match seconds {
        s if s < 50.0 => format!("{} s", (s / 5.0).ceil().max(1.0) * 5.0),
        s if s < 90.0 * 60.0 => format!("{} min", (s / 60.0).round().max(1.0)),
        s => format!("{:.1} h", s / 3600.0),
    }
}

/// A stem's name, as a heading: "drums" → "Drums".
fn title(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

impl Workspace {
    /// An audio region's menu item for its stems: the sheet, or how far it's got.
    pub(crate) fn stems_items(&self, at: At) -> Vec<(String, MenuAction)> {
        if !separate::available() {
            return vec![(
                "Separate Stems (needs Apple silicon)".into(),
                MenuAction::Nothing,
            )];
        }
        let label = match &self.separating {
            Some(s) => format!(
                "Separating Stems… {}%",
                s.done.load(Ordering::Relaxed).min(1000) / 10
            ),
            None => "Separate Stems…".into(),
        };
        vec![(label, MenuAction::Region(at, RegionOp::Stems))]
    }

    /// Settings' Stems & lyrics page, to download a model or add one.
    pub(crate) fn show_models(&mut self, cx: &mut Context<Self>) {
        self.stems_sheet = None;
        self.settings_section = crate::pages::settings::Section::Models;
        self.open_settings(cx);
    }

    /// The Separate Stems sheet, for the audio region at `at` (or the one separating).
    pub(crate) fn open_stems(&mut self, at: At, cx: &mut Context<Self>) {
        let at = self.separating.as_ref().map_or(at, |s| s.at);
        let p = &self.session.project;
        let Some(r) = p.tracks.get(at.0).and_then(|t| t.regions.get(at.1)) else {
            return;
        };
        let Content::Audio { file, .. } = &r.content else {
            return;
        };
        let rate = p.sample_rate as f64;
        let seconds = self.player.hits(file, rate).map_or(
            (p.seconds_at((r.start + r.length) as f64) - p.seconds_at(r.start as f64)) as f32,
            |(_, frames)| (frames as f64 / rate) as f32,
        );
        let model = installed().first().map(|m| m.about.name.clone());
        self.stems_sheet = Some(StemSheet {
            at,
            file: file.clone(),
            seconds,
            model,
            without: Vec::new(),
            mute: true,
            failed: None,
        });
        cx.notify();
    }

    /// The sheet's region parted by its picked model, in the background.
    pub(crate) fn start_stems(&mut self, cx: &mut Context<Self>) {
        if self.separating.is_some() {
            return;
        }
        let Some(sheet) = &mut self.stems_sheet else {
            return;
        };
        sheet.failed = None;
        let (at, file, without, mute) = (
            sheet.at,
            sheet.file.clone(),
            sheet.without.clone(),
            sheet.mute,
        );
        let Some(model) = installed()
            .into_iter()
            .find(|m| Some(&m.about.name) == sheet.model.as_ref())
        else {
            return;
        };
        let dir = self.session.dir.clone();
        // Beside the song's audio, a folder a file and model (as Lyrics keeps them too).
        let folder = separate::stems_folder(&file, &model.about.name);
        let stems: Vec<(String, PathBuf)> = stems_of(&model)
            .into_iter()
            .map(|s| {
                let f = folder.join(format!("{s}.wav"));
                (s, f)
            })
            .collect();
        let (done, stop) = (Arc::<AtomicU32>::default(), Arc::<AtomicBool>::default());
        let made_before = kept(&dir, &file, &model);
        self.separating = Some(Separating {
            model: model.about.name.clone(),
            at,
            done: done.clone(),
            stop: stop.clone(),
            started: Instant::now(),
            kept: made_before,
        });
        let job = cx.background_spawn({
            let (done, stop, out) = (done.clone(), stop.clone(), dir.join(&folder));
            let source = dir.join(&file);
            async move {
                if !made_before {
                    // Under 1000 while separating; 1000 once it's writing the stems.
                    let progress = |p: f32| done.store(1 + (p * 998.0) as u32, Ordering::Relaxed);
                    separate::separate(&source, &model, &out, progress, &stop)?;
                }
                done.store(1000, Ordering::Relaxed);
                Ok::<(), String>(())
            }
        });
        // The sheet (and the status line, the sheet hidden) as it goes.
        let name = self
            .separating
            .as_ref()
            .map(|s| s.model.clone())
            .unwrap_or_default();
        cx.spawn({
            let done = done.clone();
            async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(300))
                        .await;
                    let going = this.update(cx, |ws, cx| {
                        let ours = ws
                            .separating
                            .as_ref()
                            .is_some_and(|s| Arc::ptr_eq(&s.done, &done));
                        if ours && ws.stems_sheet.is_none() {
                            let pct = done.load(Ordering::Relaxed).min(1000) / 10;
                            ws.status = Some(format!("Separating stems with {name}: {pct}%"));
                        }
                        if ours {
                            cx.notify();
                        }
                        ours
                    });
                    if !matches!(going, Ok(true)) {
                        break;
                    }
                }
            }
        })
        .detach();
        let keep: Vec<(String, PathBuf)> = stems
            .into_iter()
            .filter(|(s, _)| !without.contains(s))
            .collect();
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |ws, cx| {
                ws.separated(at, &file, &keep, mute, result, cx)
            });
        })
        .detach();
        cx.notify();
    }

    /// A separation done: its stems in, as tracks (the sheet closed), or why not (on the sheet).
    fn separated(
        &mut self,
        at: At,
        file: &std::path::Path,
        stems: &[(String, PathBuf)],
        mute: bool,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = self.separating.take() else {
            return;
        };
        let mut fail = |ws: &mut Self, e: String| {
            ws.status = Some(e.clone());
            if let Some(sheet) = ws.stems_sheet.as_mut().filter(|s| s.at == at) {
                sheet.failed = Some(e);
            }
            cx.notify();
        };
        match result {
            Err(e) if e == "Stopped" => {
                self.status = Some("Stopped separating stems".into());
                return cx.notify();
            }
            Err(e) => return fail(self, e),
            Ok(()) => {}
        }
        let same = self
            .session
            .project
            .tracks
            .get(at.0)
            .and_then(|t| t.regions.get(at.1))
            .is_some_and(|r| matches!(&r.content, Content::Audio { file: f, .. } if f == file));
        if !same {
            return fail(
                self,
                "The region moved while its stems were made: separate it again (they're kept, \
                 so it's instant)"
                    .into(),
            );
        }
        let mut made = Vec::new();
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            made = p.stems_from_audio(at, stems);
            if !mute {
                p.tracks[at.0].regions[at.1].mute = false;
            }
        });
        if let Some(&first) = made.first() {
            self.select_track(first);
            self.region = Some((first, 0));
        }
        self.stems_sheet = None;
        self.status = Some(format!(
            "{} stems by {} on tracks of their own, in a stack{}",
            made.len(),
            job.model,
            if mute { "; the region is muted" } else { "" }
        ));
        cx.notify();
    }

    /// The Separate Stems sheet, over everything, if it's open.
    pub fn stems_sheet_view(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let sheet = self.stems_sheet.as_ref()?;
        let name = sheet
            .file
            .file_stem()
            .map_or("audio".into(), |s| s.to_string_lossy().into_owned());
        let length = sheet.seconds;
        let running = self
            .separating
            .as_ref()
            .filter(|s| s.at == sheet.at)
            .map(|s| {
                (
                    s.model.clone(),
                    s.done.load(Ordering::Relaxed),
                    s.started.elapsed().as_secs_f32(),
                    s.kept,
                )
            });
        let (body, foot) = match running {
            Some((model, done, elapsed, kept)) => (
                self.stems_running(&name, &model, done, elapsed, kept),
                self.stems_running_foot(cx),
            ),
            None => (self.stems_choosing(cx), self.stems_choosing_foot(cx)),
        };
        let card = div()
            .id("stems-sheet")
            .w(px(640.))
            .max_h(px(640.))
            .flex()
            .flex_col()
            .rounded(px(16.))
            .bg(rgb(0x17171c))
            .border_1()
            .border_color(rgb(LINE_STRONG))
            .shadow_lg()
            .overflow_hidden()
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .debug_selector(|| "stems-sheet".into())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(px(56.))
                    .flex_none()
                    .px(px(18.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(icon(Icon::Stack, 16.).text_color(rgb(TEXT_2)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Separate stems"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{name} · {}", clock(length))),
                            ),
                    ),
            )
            .child(
                div()
                    .id("stems-body")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(body),
            )
            .child(foot);
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(alpha(0x000000, 0.55))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            // Off the sheet: put away (what's running carries on).
                            ws.stems_sheet = None;
                            cx.notify();
                        }),
                    )
                    .child(card),
            )
            .with_priority(3),
        )
    }

    /// Picking: the models there are, more to get, the stems to keep, the region muted or not.
    fn stems_choosing(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(sheet) = self.stems_sheet.as_ref() else {
            return div();
        };
        let models = installed();
        let dir = self.session.dir.clone();
        let (file, seconds, picked, failed) = (
            sheet.file.clone(),
            sheet.seconds,
            sheet.model.clone(),
            sheet.failed.clone(),
        );
        let heading = |s: &'static str| {
            div()
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_3))
                .child(s)
        };
        let chip = |s: String, tone: u32| {
            div()
                .px(px(6.))
                .py(px(1.))
                .rounded(px(5.))
                .bg(rgb(WELL))
                .text_size(px(10.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(tone))
                .child(s)
        };
        // The model: a dropdown of those there are, each with how long it'll take (or that it's
        // ready); what the picked one gives, how well and on what terms, under it.
        let time_of = |m: &Model| match (kept(&dir, &file, m), Known::named(&m.about.name)) {
            (true, _) => "ready: made before".to_string(),
            (false, Some(k)) => format!("about {}", about(k.speed * seconds)),
            (false, None) => String::new(),
        };
        let items: Vec<(String, MenuAction)> = models
            .iter()
            .map(|m| {
                let t = time_of(m);
                let label = match t.is_empty() {
                    true => m.about.name.clone(),
                    false => format!("{} · {t}", m.about.name),
                };
                (label, MenuAction::StemModel(m.about.name.clone()))
            })
            .collect();
        let picker = dropdown(picked.clone().unwrap_or_else(|| "Pick a model".into()), 32.)
            .w_full()
            .debug_selector(|| "stems-model".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &gpui::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.menu = Some(crate::ui::menu::Menu {
                        at: e.position,
                        items: items.clone(),
                        ..Default::default()
                    });
                    cx.notify();
                }),
            );
        let summary = models
            .iter()
            .find(|m| picked.as_ref() == Some(&m.about.name))
            .map(|m| {
                let known = Known::named(&m.about.name);
                let ready = kept(&dir, &file, m);
                let licence = match known {
                    Some(k) => chip(k.licence.into(), if k.open { MINT } else { AMBER }),
                    None if !m.about.licence.is_empty() => chip(m.about.licence.clone(), TEXT_3),
                    None => div(),
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.))
                            .child(licence)
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(if ready { MINT } else { TEXT_2 }))
                                    .child(time_of(m)),
                            ),
                    )
                    .children(known.map(|k| {
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .child(k.heard.split(" · about").next().unwrap_or("").to_string())
                    }))
            });
        // The tested ones not here yet, to get now.
        let more: Vec<Div> = separate::catalog::KNOWN
            .iter()
            .enumerate()
            .filter(|(_, k)| !models.iter().any(|m| m.about.name == k.about().name))
            .map(|(i, k)| {
                let going = self
                    .models_ui
                    .downloads
                    .get(k.id)
                    .map(|d| d.0.load(Ordering::Relaxed));
                let right = match going {
                    Some(b) => div()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_2))
                        .child(format!("Downloading {}%", b * 100 / k.bytes.max(1))),
                    None => button(format!("Download · {} MB", k.bytes / 1_000_000), false)
                        .h(px(26.))
                        .debug_selector(move || format!("stems-get-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| ws.download_model(i, cx)),
                        ),
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .py(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(k.about().name),
                                    )
                                    .child(chip(
                                        k.licence.into(),
                                        if k.open { MINT } else { AMBER },
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT_3))
                                    .child(k.heard.split(" · ").next().unwrap_or("").to_string()),
                            ),
                    )
                    .child(right)
            })
            .collect();
        let picked_model = models
            .iter()
            .find(|m| picked.as_ref() == Some(&m.about.name));
        let without = sheet.without.clone();
        let mute = sheet.mute;
        let stem_toggles: Vec<Div> = picked_model.map_or(Vec::new(), |m| {
            stems_of(m)
                .into_iter()
                .map(|s| {
                    let on = !without.contains(&s);
                    let name = s.clone();
                    div()
                        .h(px(28.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(if on { rgb(LINE_STRONG) } else { rgb(LINE) })
                        .bg(if on { rgb(RAISED) } else { rgb(WELL) })
                        .text_size(px(12.))
                        .text_color(rgb(if on { TEXT } else { TEXT_4 }))
                        .cursor_pointer()
                        .debug_selector(move || format!("stems-keep-{s}"))
                        .child(
                            icon(if on { Icon::Check } else { Icon::Minus }, 11.)
                                .text_color(rgb(if on { MINT } else { TEXT_4 })),
                        )
                        .child(title(&name))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                if let Some(sh) = ws.stems_sheet.as_mut() {
                                    match sh.without.iter().position(|w| *w == name) {
                                        Some(k) => drop(sh.without.remove(k)),
                                        None => sh.without.push(name.clone()),
                                    }
                                }
                                cx.notify();
                            }),
                        )
                })
                .collect()
        });
        let mut body = div().flex().flex_col().gap(px(14.)).p(px(18.));
        if let Some(e) = failed {
            body = body.child(
                div()
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(alpha(RECORD, 0.12))
                    .border_1()
                    .border_color(alpha(RECORD, 0.5))
                    .text_size(px(12.))
                    .child(format!("It didn't work: {e}")),
            );
        }
        body = match models.is_empty() {
            true => body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Get a model to separate with"),
                    )
                    .child(div().text_size(px(12.)).text_color(rgb(TEXT_3)).child(
                        "Pina Studio comes with none: pick one below. BS-RoFormer SW \
                                 gives every stem; Mel-Band RoFormer vocals (MIT) is the best \
                                 for vocals.",
                    )),
            ),
            false => body.child(heading("MODEL")).child(picker).children(summary),
        };
        if !more.is_empty() {
            body = body
                .child(heading(if models.is_empty() {
                    "MODELS"
                } else {
                    "MORE MODELS"
                }))
                .child(div().flex().flex_col().children(more));
        }
        if !stem_toggles.is_empty() {
            body = body
                .child(heading("STEMS TO KEEP"))
                .child(div().flex().flex_wrap().gap(px(6.)).children(stem_toggles))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            switch(mute)
                                .debug_selector(|| "stems-mute".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        if let Some(s) = ws.stems_sheet.as_mut() {
                                            s.mute ^= true;
                                        }
                                        cx.notify();
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .child("Mute the region once its stems are in"),
                        ),
                );
        }
        body
    }

    /// The foot while picking: what it'll make and how long, Cancel and Separate.
    fn stems_choosing_foot(&mut self, cx: &mut Context<Self>) -> Div {
        let sheet = self.stems_sheet.as_ref();
        let models = installed();
        let picked = sheet.and_then(|s| {
            models
                .iter()
                .find(|m| s.model.as_ref() == Some(&m.about.name))
        });
        let count = picked.map_or(0, |m| {
            stems_of(m)
                .iter()
                .filter(|st| !sheet.is_some_and(|s| s.without.contains(st)))
                .count()
        });
        let ready =
            picked.is_some_and(|m| sheet.is_some_and(|s| kept(&self.session.dir, &s.file, m)));
        let what = match (picked, sheet) {
            (Some(m), Some(s)) => {
                let time = match (ready, Known::named(&m.about.name)) {
                    (true, _) => "made before: instant".to_string(),
                    (false, Some(k)) => format!("about {} on this Mac", about(k.speed * s.seconds)),
                    (false, None) => "how long depends on the model".to_string(),
                };
                format!("{count} stem{} · {time}", if count == 1 { "" } else { "s" })
            }
            _ => "Pick a model".to_string(),
        };
        let can = count > 0;
        foot()
            .child(
                div()
                    .flex_1()
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT_3))
                    .child(what),
            )
            .child(
                button("Cancel", false)
                    .debug_selector(|| "stems-cancel".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.stems_sheet = None;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                primary(if ready { "Add the stems" } else { "Separate" })
                    .h(px(34.))
                    .px(px(18.))
                    .when(!can, |d| d.opacity(0.4))
                    .debug_selector(|| "stems-go".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            if can {
                                ws.start_stems(cx);
                            }
                        }),
                    ),
            )
    }

    /// Going: what it's doing, how far, how long left.
    fn stems_running(&self, name: &str, model: &str, done: u32, elapsed: f32, kept: bool) -> Div {
        let share = (done.min(1000) as f32 / 1000.0).clamp(0.0, 1.0);
        let (what, why) = match (kept, done) {
            (true, _) | (_, 1000..) => (
                "Putting the stems in".to_string(),
                "Each on a track of its own, in a stack".to_string(),
            ),
            (_, 0) => (format!("Reading {name}…"), String::new()),
            _ => {
                let left = match share > 0.03 {
                    true => format!("about {} left", about(elapsed * (1.0 - share) / share)),
                    false => "working out how long…".to_string(),
                };
                (
                    format!("Separating with {model}"),
                    format!("{}% · {left}", (share * 100.0) as u32),
                )
            }
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(24.))
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(what),
            )
            .child(
                div()
                    .h(px(8.))
                    .rounded(px(4.))
                    .bg(rgb(WELL))
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .rounded(px(4.))
                            .bg(rgb(ACCENT))
                            .w(gpui::relative(share.max(0.02))),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_3))
                    .debug_selector(|| "stems-progress".into())
                    .child(why),
            )
            .child(div().text_size(px(11.5)).text_color(rgb(TEXT_4)).child(
                "It runs on this Mac's processor. Hide this and keep working: the status \
                         line says how far it's got.",
            ))
    }

    /// The foot while going: Hide, and Stop.
    fn stems_running_foot(&mut self, cx: &mut Context<Self>) -> Div {
        foot()
            .child(div().flex_1())
            .child(
                button("Hide", false)
                    .debug_selector(|| "stems-hide".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            ws.stems_sheet = None;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                button("Stop", false)
                    .debug_selector(|| "stems-stop".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| {
                            if let Some(s) = &ws.separating {
                                s.stop.store(true, Ordering::Relaxed);
                            }
                            cx.notify();
                        }),
                    ),
            )
    }
}

/// The sheet's foot, its actions on the right.
fn foot() -> Div {
    div()
        .h(px(60.))
        .flex_none()
        .px(px(18.))
        .flex()
        .items_center()
        .gap(px(10.))
        .border_t_1()
        .border_color(rgb(LINE))
}

/// Seconds as m:ss.
fn clock(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}
