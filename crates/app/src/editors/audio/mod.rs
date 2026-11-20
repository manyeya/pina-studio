//! The audio editor: an audio region's file laid out big, the part the region plays lit in its
//! track's colour. Its start and end drag to trim it (or set from the bar), its fades drag
//! from their handles; under it, its level, pitch, direction and how it keeps time. The region
//! made a Sampler track, whole or sliced at its hits (they're marked along the top).

mod flips;
mod lyrics;
mod smart;
mod stems;
#[cfg(test)]
#[cfg(all(test, unix))]
pub use flips::{Flips, Stage};
pub use flips::{FlipsUi, LyricsTab};
pub use lyrics::{Lyrics, LyricsWait};
pub use smart::{Beat, BeatWait, SliceFilter};
pub use stems::{Separating, StemSheet};

use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use gpui::{
    Bounds, Context, Div, FontWeight, MouseButton, MouseDownEvent, PathBuilder, Pixels,
    ScrollWheelEvent, canvas, div, fill, point, prelude::*, px, rgb, size,
};
use pina_engine::{
    audio::engine::read_audio,
    music::analysis::write_wav,
    song::project::{At, Content, Follow, Region, SAMPLER_ROOT, StretchMode, Timing},
};

use crate::{
    arrange::tempo::TempoOp,
    editors::{
        pianoroll::name_in,
        sampler::{Wave, captioned, paint_wave, pill, value_field, zoomed},
    },
    ui::menu::{Menu, MenuAction},
    ui::theme::*,
    workspace::{Bottom, Drag, Help, Target, Workspace},
};

/// A setting of an audio region the editor's fields turn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipField {
    Start,
    End,
    FadeIn,
    FadeOut,
    Gain,
    Transpose,
}

/// What's dragged on the waveform: where the region starts or ends, or where its fades do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipMark {
    Start,
    End,
    FadeIn,
    FadeOut,
}

/// How the editor shows the file.
#[derive(Default)]
pub struct AudioUi {
    /// The part of the file shown, as shares of it (all of it: none).
    pub view: Option<(f32, f32)>,
    /// Where the waveform was drawn, for its handles and the wheel.
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
}

/// Where a region is in its file: the file's length, and where the region starts and ends in it
/// (frames, as its offset counts them).
#[derive(Clone, Copy)]
struct Span {
    total: f64,
    from: f64,
    to: f64,
}

/// Where a region's slices are cut: at its hits, or on the song's grid every so many ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliceBy {
    Hits,
    Every(u64),
}

impl SliceBy {
    /// Its choices, for a song whose bar is `bar` ticks.
    pub fn all(bar: u64) -> [SliceBy; 5] {
        use pina_engine::song::project::PPQ;
        [
            SliceBy::Hits,
            SliceBy::Every(PPQ / 4),
            SliceBy::Every(PPQ / 2),
            SliceBy::Every(PPQ),
            SliceBy::Every(bar),
        ]
    }

    pub fn name(self, bar: u64) -> String {
        use pina_engine::song::project::PPQ;
        match self {
            SliceBy::Hits => "At its hits".into(),
            SliceBy::Every(t) if t == bar => "Every bar".into(),
            SliceBy::Every(t) => match PPQ * 4 / t.max(1) {
                4 => "Every 1/4 note".into(),
                n => format!("Every 1/{n} note"),
            },
        }
    }
}

/// The ways to slice the region at `at`, as menu items.
pub(crate) fn slice_items(at: At, bar: u64) -> Vec<(String, MenuAction)> {
    SliceBy::all(bar)
        .into_iter()
        .map(|how| {
            (
                how.name(bar),
                MenuAction::Region(at, crate::workspace::context::RegionOp::SliceToSampler(how)),
            )
        })
        .collect()
}

/// How far a region's gain goes, either way (dB), and its transpose (semitones).
const GAIN_DB: f32 = 30.0;
const SEMIS: f32 = 24.0;

/// A reversed copy's name ends so, with a mark of the file it was made from: "Loop (reversed
/// 1a2b3c4d)". Reversing one goes back to that file.
const REVERSED: &str = " (reversed";

/// A mark of a file's path, the same in every session, build and system (FNV-1a), so two files
/// of one name in different folders never share a reversed copy.
fn mark(file: &Path) -> String {
    let path = file.to_string_lossy().replace('\\', "/");
    let h = path.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    });
    format!("{h:08x}")
}

/// A reversed copy's name taken apart: the name it was made from, and its mark (none on a copy
/// made before they were marked).
fn reversed_base(stem: &str) -> Option<(&str, Option<&str>)> {
    let (base, rest) = stem.rsplit_once(REVERSED)?;
    let rest = rest.strip_suffix(')')?;
    Some((base, rest.strip_prefix(' ')))
}

/// Where reversing `file` leads: back to the file a reversed copy was made from (beside it, and
/// that very file by its mark), else a reversed copy of it in the song's Audio Files; and
/// whether that has to be written first.
/// A file's name, as it's shown.
fn file_name(f: &Path) -> String {
    f.file_name().map_or_else(
        || f.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn reversed_of(dir: &Path, file: &Path) -> (PathBuf, bool) {
    let stem = file
        .file_stem()
        .map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let back = reversed_base(&stem)
        .map(|(base, m)| (file.with_file_name(format!("{base}.wav")), m))
        .filter(|(f, m)| dir.join(f).is_file() && m.is_none_or(|m| m == mark(f)));
    if let Some((f, _)) = back {
        return (f, false);
    }
    let to = PathBuf::from("Audio Files").join(format!("{stem}{REVERSED} {}).wav", mark(file)));
    let write = !dir.join(&to).is_file();
    (to, write)
}

fn is_reversed(file: &Path) -> bool {
    file.file_stem()
        .is_some_and(|s| reversed_base(&s.to_string_lossy()).is_some())
}

impl Workspace {
    /// The audio region the editor shows: the selected region, if it's audio.
    pub fn audio_region(&self) -> Option<At> {
        let at = self.region?;
        let r = self.session.project.tracks.get(at.0)?.regions.get(at.1)?;
        matches!(r.content, Content::Audio { .. }).then_some(at)
    }

    /// Where the region at `at` is in its file, once its file's been read.
    fn clip_span(&self, at: At) -> Option<Span> {
        let p = &self.session.project;
        let r = p.tracks.get(at.0)?.regions.get(at.1)?;
        let Content::Audio { file, .. } = &r.content else {
            return None;
        };
        let rate = p.sample_rate as f64;
        let (_, frames) = self.player.hits(file, rate)?;
        let t = Timing::of(&r.content, rate)?;
        let start = r.start as f64;
        let total = (frames as f64).max(1.0);
        Some(Span {
            total,
            from: p.audio_frame(&t, start, start).clamp(0.0, total),
            to: p
                .audio_frame(&t, start, (r.start + r.length) as f64)
                .clamp(0.0, total),
        })
    }

    /// A field's place, 0 to 1, as a knob has it.
    pub fn clip_pos(&self, at: At, f: ClipField) -> f32 {
        let Some(r) = self
            .session
            .project
            .tracks
            .get(at.0)
            .and_then(|t| t.regions.get(at.1))
        else {
            return 0.0;
        };
        let len = r.length.max(1) as f32;
        match f {
            ClipField::Gain => (r.gain_db + GAIN_DB) / (2.0 * GAIN_DB),
            ClipField::Transpose => (r.transpose as f32 + SEMIS) / (2.0 * SEMIS),
            ClipField::FadeIn => r.fade_in as f32 / len,
            ClipField::FadeOut => r.fade_out as f32 / len,
            ClipField::Start => self
                .clip_span(at)
                .map_or(0.0, |s| (s.from / s.total) as f32),
            ClipField::End => self.clip_span(at).map_or(1.0, |s| (s.to / s.total) as f32),
        }
    }

    /// A field set from its place (0 to 1). Its start and end trim the region: what it plays
    /// stays where it is on the timeline.
    pub fn clip_set(&mut self, at: At, f: ClipField, v: f32) {
        let v = v.clamp(0.0, 1.0);
        if let ClipField::Start | ClipField::End = f {
            if let Some(s) = self.clip_span(at) {
                let frame = v as f64 * s.total;
                match f {
                    ClipField::Start => self.trim_clip(at, false, frame.min(s.to - 1.0)),
                    _ => self.trim_clip(at, true, frame.max(s.from + 1.0)),
                }
            }
            return;
        }
        let Some(r) = self
            .session
            .project
            .tracks
            .get_mut(at.0)
            .and_then(|t| t.regions.get_mut(at.1))
        else {
            return;
        };
        let len = r.length as f32;
        match f {
            ClipField::Gain => r.gain_db = ((v * 2.0 - 1.0) * GAIN_DB * 10.0).round() / 10.0,
            ClipField::Transpose => r.transpose = ((v * 2.0 - 1.0) * SEMIS).round() as i8,
            ClipField::FadeIn => {
                r.fade_in = ((v * len) as u64).min(r.length.saturating_sub(r.fade_out))
            }
            ClipField::FadeOut => {
                r.fade_out = ((v * len) as u64).min(r.length.saturating_sub(r.fade_in))
            }
            ClipField::Start | ClipField::End => {}
        }
    }

    /// The region's start (or end) moved to frame `frame` of its file: no further than its file
    /// goes.
    fn trim_clip(&mut self, at: At, end: bool, frame: f64) {
        let p = &self.session.project;
        let r = p.region(at);
        let Some(t) = Timing::of(&r.content, p.sample_rate as f64) else {
            return;
        };
        let tick = p.audio_tick(&t, r.start as f64, frame).round().max(0.0) as u64;
        match end {
            false => self.session.project.trim_start(at, tick),
            true => {
                let last = self.audio_end(at).unwrap_or(u64::MAX);
                let start = self.session.project.region(at).start;
                self.session
                    .project
                    .trim_end(at, tick.min(last).max(start + 1));
            }
        }
    }

    fn clip_text(&self, at: At, f: ClipField) -> String {
        let p = &self.session.project;
        let r = p.region(at);
        let ms = |a: u64, b: u64| (p.seconds_at(b as f64) - p.seconds_at(a as f64)) * 1000.0;
        let secs = |frames: f64| format!("{:.3} s", frames / p.sample_rate as f64);
        let end = r.start + r.length;
        match f {
            ClipField::Gain if r.gain_db.abs() < 0.05 => "0.0 dB".into(),
            ClipField::Gain => format!("{:+.1} dB", r.gain_db).replace('-', "−"),
            ClipField::Transpose => format!("{:+} st", r.transpose).replace('-', "−"),
            ClipField::FadeIn if r.fade_in == 0 => "Off".into(),
            ClipField::FadeIn => format!("{:.0} ms", ms(r.start, r.start + r.fade_in)),
            ClipField::FadeOut if r.fade_out == 0 => "Off".into(),
            ClipField::FadeOut => format!("{:.0} ms", ms(end.saturating_sub(r.fade_out), end)),
            ClipField::Start => self.clip_span(at).map_or("…".into(), |s| secs(s.from)),
            ClipField::End => self.clip_span(at).map_or("…".into(), |s| secs(s.to)),
        }
    }

    fn clip_value(
        &self,
        at: At,
        f: ClipField,
        label: &'static str,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        value_field(
            label,
            self.clip_text(at, f),
            self.clip_pos(at, f),
            color,
            Target::Clip { at, field: f },
            format!("clip-{f:?}"),
            cx,
        )
    }

    /// A handle on the waveform dragged: the region's start or end (trimming it), or where its
    /// fades end.
    pub(crate) fn drag_clip(&mut self, p: gpui::Point<f32>) {
        let Some(Drag::Clip {
            at,
            mark,
            left,
            width,
        }) = self.drag
        else {
            return;
        };
        let Some(s) = self.clip_span(at) else { return };
        let (a, b) = self.audio_ui.view.unwrap_or((0.0, 1.0));
        let share = (a + (p.x - left) / width.max(1.0) * (b - a)).clamp(0.0, 1.0) as f64;
        let frame = share * s.total;
        match mark {
            ClipMark::Start => self.trim_clip(at, false, frame.min(s.to - 1.0)),
            ClipMark::End => self.trim_clip(at, true, frame.max(s.from + 1.0)),
            ClipMark::FadeIn | ClipMark::FadeOut => {
                let pj = &self.session.project;
                let r = pj.region(at);
                let Some(t) = Timing::of(&r.content, pj.sample_rate as f64) else {
                    return;
                };
                let tick = pj.audio_tick(&t, r.start as f64, frame).round().max(0.0) as u64;
                let (start, end) = (r.start, r.start + r.length);
                let r = &mut self.session.project.tracks[at.0].regions[at.1];
                match mark {
                    ClipMark::FadeIn => {
                        r.fade_in = tick
                            .saturating_sub(start)
                            .min(r.length.saturating_sub(r.fade_out))
                    }
                    _ => {
                        r.fade_out = end
                            .saturating_sub(tick)
                            .min(r.length.saturating_sub(r.fade_in))
                    }
                }
            }
        }
    }

    /// The region played backwards: a reversed copy of its file in the song's Audio Files (made
    /// once; reversing that again goes back to its file), the region on the same part of it.
    /// Its warp markers and beats go, being the wrong way round now.
    pub(crate) fn reverse_clip(&mut self, at: At, cx: &mut Context<Self>) {
        let Some(s) = self.clip_span(at) else {
            self.status = Some("Still reading the audio: try again in a moment".into());
            cx.notify();
            return;
        };
        let Content::Audio { file, .. } = &self.session.project.region(at).content else {
            return;
        };
        let file = file.clone();
        let dir = self.session.dir.clone();
        let (to, write) = reversed_of(&dir, &file);
        let offset = (s.total - s.to).round().max(0.0) as u64;
        let (from_path, to_path) = (dir.join(&file), dir.join(&to));
        let job = cx.background_spawn(async move {
            if !write {
                return Ok(());
            }
            let (sides, rate) = read_audio(&from_path)?;
            let backwards = |s: &Vec<f32>| s.iter().rev().copied().collect::<Vec<f32>>();
            let l = backwards(&sides[0]);
            let r = sides.get(1).map_or_else(|| l.clone(), backwards);
            if let Some(folder) = to_path.parent() {
                std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
            }
            write_wav(&to_path, &l, &r, rate.round() as u32)
        });
        cx.spawn(async move |this, cx| {
            let done = job.await;
            let _ = this.update(cx, |ws, cx| {
                if let Err(e) = done {
                    ws.status = Some(format!("Couldn't reverse it: {e}"));
                    return cx.notify();
                }
                let same = ws
                    .session
                    .project
                    .tracks
                    .get(at.0)
                    .and_then(|t| t.regions.get(at.1))
                    .is_some_and(
                        |r| matches!(&r.content, Content::Audio { file: f, .. } if *f == file),
                    );
                if !same {
                    ws.status =
                        Some("The region changed while it was reversed: reverse it again".into());
                    return cx.notify();
                }
                ws.edit(cx, |ws| {
                    if let Content::Audio {
                        file,
                        offset: o,
                        warps,
                        grid,
                        ..
                    } = &mut ws.session.project.tracks[at.0].regions[at.1].content
                    {
                        (*file, *o) = (to.clone(), offset);
                        warps.clear();
                        *grid = None;
                    }
                });
                ws.status = Some("Reversed".into());
            });
        })
        .detach();
    }

    /// The region made a Sampler track playing it where it was, whole (`slice` none) or sliced,
    /// opened in the Sampler; the region muted.
    /// Does `then` once `file`'s audio is read (its hits known), the status bar saying `what`
    /// meanwhile; after a minute, says it couldn't be read.
    pub(crate) fn once_read(
        &mut self,
        file: PathBuf,
        what: String,
        then: impl FnOnce(&mut Workspace, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let since = std::time::Instant::now();
        self.busy = Some((what.clone(), since));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let mut then = Some(then);
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let done = this.update(cx, |ws, cx| {
                    let rate = ws.session.project.sample_rate as f64;
                    let read = ws.player.hits(&file, rate).is_some();
                    let late = since.elapsed().as_secs() >= 60;
                    if (read || late) && ws.busy.as_ref().is_some_and(|b| b.1 == since) {
                        ws.busy = None;
                    }
                    match (read, late) {
                        (true, _) => {
                            if let Some(then) = then.take() {
                                then(ws, cx);
                            }
                        }
                        (false, true) => {
                            ws.status = Some(format!("Couldn't read {}", file_name(&file)));
                        }
                        _ => {}
                    }
                    cx.notify();
                    read || late
                });
                if !matches!(done, Ok(false)) {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn clip_to_sampler(
        &mut self,
        at: At,
        slice: Option<SliceBy>,
        cx: &mut Context<Self>,
    ) {
        let p = &self.session.project;
        let r = p.region(at);
        let Content::Audio { file, .. } = &r.content else {
            return;
        };
        let Some((hits, frames)) = self.player.hits(file, p.sample_rate as f64) else {
            // Not read yet: it's sliced as soon as it is.
            let (file, name) = (file.clone(), file_name(file));
            let what = match slice {
                Some(_) => format!("Reading {name} to slice it"),
                None => format!("Reading {name} for the Sampler"),
            };
            return self.once_read(
                file.clone(),
                what,
                move |ws, cx| {
                    let same = matches!(&ws.session.project.tracks.get(at.0)
                        .and_then(|t| t.regions.get(at.1)).map(|r| &r.content),
                        Some(Content::Audio { file: f, .. }) if *f == file);
                    if same {
                        ws.clip_to_sampler(at, slice, cx);
                    }
                },
                cx,
            );
        };
        let cuts: Vec<u64> = match slice {
            None => Vec::new(),
            Some(SliceBy::Hits) => hits.iter().map(|&h| h as u64).collect(),
            // On the song's grid: each line inside the region, where it falls in the file.
            Some(SliceBy::Every(step)) => {
                let Some(t) = Timing::of(&r.content, p.sample_rate as f64) else {
                    return;
                };
                let step = step.max(1);
                let first = r.start.div_ceil(step) * step;
                (first..r.start + r.length)
                    .step_by(step as usize)
                    .map(|tick| {
                        p.audio_frame(&t, r.start as f64, tick as f64)
                            .round()
                            .max(0.0) as u64
                    })
                    .collect()
            }
        };
        let mut made = None;
        self.edit(cx, |ws| {
            made = ws.session.project.sampler_from_audio(
                at,
                frames as u64,
                slice.is_some().then_some(cuts.as_slice()),
            );
        });
        let Some(t) = made else { return };
        let zones = self.sampler_of(t).map_or(0, |s| s.zones.len());
        self.select_track(t);
        self.region = Some((t, 0));
        self.sampler_ui.zone = 0;
        self.sampler_ui.view = None;
        self.show_bottom(Bottom::Sampler, cx);
        let first = name_in(None, pina_engine::song::project::FIRST_SLICE);
        self.status = Some(match slice {
            Some(how) => format!(
                "Sliced {} into {zones} sounds, from {first}; the audio region is muted",
                how.name(self.session.project.bar()).to_lowercase()
            ),
            None => format!(
                "On a Sampler track across the keys ({} plays it as recorded); the audio region \
                 is muted",
                name_in(None, SAMPLER_ROOT)
            ),
        });
    }

    /// The bar's own: the region made a Sampler track, whole, or sliced (a menu of how).
    pub fn audio_controls(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let at = self.audio_region()?;
        let bar = self.session.project.bar();
        let whole = button("To Sampler", false)
            .h(px(28.))
            .debug_selector(|| "clip-to-sampler".into())
            .help(
                "A Sampler track playing it where it was: the sound across the keys",
                cx,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.clip_to_sampler(at, None, cx);
                }),
            );
        let sliced = dropdown("Slice to Sampler", 28.)
            .debug_selector(|| "clip-slice".into())
            .help(
                "A Sampler track playing it where it was: a slice at each of its hits, or on \
                 the grid, a note each",
                cx,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    ws.menu = Some(Menu {
                        at: e.position,
                        items: slice_items(at, bar),
                        ..Default::default()
                    });
                    cx.notify();
                }),
            );
        let busy = self
            .lyrics_wait
            .as_ref()
            .is_some_and(|w| w.hearing.is_some())
            || self.beat_wait.as_ref().is_some_and(|w| w.failed.is_none());
        let words = button(if busy { "Slicing…" } else { "Smart Slice" }, false)
            .h(px(28.))
            .debug_selector(|| "clip-smart".into())
            .child(icon(Icon::Scissors, 13.).text_color(rgb(TEXT_3)))
            .help(
                "Slices it as a producer would: at its hits (each named: kick, snare, a chord), its \
                 words, or the beat without the voice; to hear, put on a Sampler, or flip",
                cx,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.smart_slice(at, cx);
                }),
            );
        let stems = button("Stems…", false)
            .h(px(28.))
            .debug_selector(|| "clip-stems".into())
            .help(
                "Parts it into vocals, drums, bass and the rest: pick the model, the stems to keep",
                cx,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.open_stems(at, cx);
                }),
            );
        Some(
            div()
                .flex()
                .gap(px(6.))
                .child(whole)
                .child(sliced)
                .child(stems)
                .child(words),
        )
    }

    pub fn audio_body(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(at) = self.audio_region() else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(LANE))
                .text_size(px(12.))
                .text_color(rgb(TEXT_3))
                .child("Double-click an audio region to edit it here.")
                .into_any_element();
        };
        let r = self.session.project.region(at).clone();
        let Content::Audio {
            file,
            stretch_mode,
            follow,
            formants,
            ..
        } = r.content.clone()
        else {
            return div().into_any_element();
        };
        let color = self.color(at.0);
        let lyrics = self.lyrics_panel(at, &file, cx);
        let wave = self.wave(&file, cx);
        let waveform = self.clip_wave(at, &r, wave, color, cx);
        let k =
            |ws: &Self, f, label, cx: &mut Context<Self>| ws.clip_value(at, f, label, color, cx);
        let tray_of = || tray().p(px(2.)).gap(px(1.)).rounded(px(8.));
        let reverse = pill("Reverse", is_reversed(&file), color)
            .debug_selector(|| "clip-reverse".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.reverse_clip(at, cx);
                }),
            );
        let formant = pill("Keep formants", formants, color)
            .debug_selector(|| "clip-formants".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.edit(cx, |ws| {
                        if let Content::Audio { formants, .. } =
                            &mut ws.session.project.tracks[at.0].regions[at.1].content
                        {
                            *formants ^= true;
                        }
                    })
                }),
            );
        let menu = |label: String, sel: &'static str, items: Vec<(String, MenuAction)>| {
            dropdown(label, 28.)
                .debug_selector(move || sel.into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        ws.menu = Some(Menu {
                            at: e.position,
                            items: items.clone(),
                            ..Default::default()
                        });
                        cx.notify();
                    }),
                )
        };
        let stretch = menu(
            stretch_mode.name().to_string(),
            "clip-stretch",
            StretchMode::ALL
                .into_iter()
                .map(|m| (m.name().to_string(), MenuAction::RegionStretchMode(at, m)))
                .collect(),
        );
        let time = menu(
            follow.name().to_string(),
            "clip-follow",
            Follow::ALL
                .into_iter()
                .map(|f| {
                    (
                        f.name().to_string(),
                        MenuAction::Tempo(TempoOp::Follow(at, f)),
                    )
                })
                .collect(),
        );
        let row = || div().flex().items_center().gap(px(4.));
        let bar = div()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(12.))
            .child(captioned(
                "TRIM",
                row()
                    .child(k(self, ClipField::Start, "Start", cx))
                    .child(k(self, ClipField::End, "End", cx))
                    .child(tray_of().child(reverse)),
            ))
            .child(captioned(
                "FADES",
                row().child(k(self, ClipField::FadeIn, "In", cx)).child(k(
                    self,
                    ClipField::FadeOut,
                    "Out",
                    cx,
                )),
            ))
            .child(captioned("LEVEL", k(self, ClipField::Gain, "Gain", cx)))
            .child(captioned(
                "PITCH",
                row()
                    .child(k(self, ClipField::Transpose, "Transpose", cx))
                    .child(tray_of().child(formant)),
            ))
            .child(captioned("STRETCH", stretch))
            .child(captioned("TIME", time));
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .bg(rgb(LANE))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(waveform),
                    )
                    .children(lyrics),
            )
            .child(bar)
            .into_any_element()
    }

    /// The file, big: what the region plays lit, the rest shaded; its fades drawn, its hits
    /// marked along the top, where it's playing; its start and end and its fades to drag. The
    /// wheel zooms where it points (sideways scrolls); double-click shows it all.
    fn clip_wave(
        &mut self,
        at: At,
        r: &Region,
        wave: Option<Arc<Wave>>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = self.audio_ui.view.unwrap_or((0.0, 1.0));
        let span = self.clip_span(at);
        let p = &self.session.project;
        let rate = p.sample_rate as f64;
        let timing = Timing::of(&r.content, rate);
        // A tick of the song as a place in the file (0 to 1).
        let share = |tick: u64| -> Option<f32> {
            let (t, s) = (timing.as_ref()?, span?);
            Some((p.audio_frame(t, r.start as f64, tick as f64) / s.total) as f32)
        };
        let (from, to) = span.map_or((0.0, 1.0), |s| {
            ((s.from / s.total) as f32, (s.to / s.total) as f32)
        });
        let end = r.start + r.length;
        let fade_in = share(r.start + r.fade_in.min(r.length)).unwrap_or(from);
        let fade_out = share(end.saturating_sub(r.fade_out.min(r.length))).unwrap_or(to);
        let hits: Vec<f32> = match (&r.content, span) {
            (Content::Audio { file, .. }, Some(s)) => self
                .player
                .hits(file, rate)
                .map(|(h, _)| {
                    h.iter()
                        .map(|&f| (f as f64 / s.total) as f32)
                        .filter(|&x| x > from && x < to)
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let now = self.playhead();
        let head = (self.player.playing() && (r.start..end).contains(&now))
            .then(|| share(now))
            .flatten();
        let seconds = wave.as_ref().map_or(1.0, |w| w.seconds());
        let gain = 10f32.powf(r.gain_db / 20.0);
        let across = move |at: f32| (at - view.0) / (view.1 - view.0).max(1e-6);
        let drawn = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (x0, y0) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let x_of = |at: f32| x0 + across(at) * w;
                let quad = |window: &mut gpui::Window, x: f32, y: f32, ww: f32, hh: f32, c| {
                    window.paint_quad(fill(
                        Bounds::new(point(px(x), px(y)), size(px(ww), px(hh))),
                        c,
                    ));
                };
                quad(window, x0, y0 + h / 2.0, w, 1.0, alpha(0xffffff, 0.06));
                if let Some(wave) = &wave {
                    paint_wave(window, wave, b, view, gain, |fa| {
                        match fa >= from && fa < to {
                            true => (alpha(color, 0.4), alpha(color, 0.95)),
                            false => (alpha(0xffffff, 0.1), alpha(0xffffff, 0.2)),
                        }
                    });
                }
                // Outside what plays, shaded.
                let shade = alpha(0x000000, 0.5);
                if x_of(from) > x0 {
                    quad(window, x0, y0, x_of(from) - x0, h, shade);
                }
                if x_of(to) < x0 + w {
                    quad(window, x_of(to), y0, x0 + w - x_of(to), h, shade);
                }
                // The fades: the level rising to the top over the fade in, falling over the
                // fade out; above the line, shaded.
                for (a, bb, rising) in [(from, fade_in, true), (fade_out, to, false)] {
                    let (xa, xb) = (x_of(a), x_of(bb));
                    if xb - xa < 1.0 {
                        continue;
                    }
                    let mut path = PathBuilder::fill();
                    match rising {
                        true => {
                            path.move_to(point(px(xa), px(y0)));
                            path.line_to(point(px(xb), px(y0)));
                            path.line_to(point(px(xa), px(y0 + h)));
                        }
                        false => {
                            path.move_to(point(px(xa), px(y0)));
                            path.line_to(point(px(xb), px(y0)));
                            path.line_to(point(px(xb), px(y0 + h)));
                        }
                    }
                    path.close();
                    if let Ok(path) = path.build() {
                        window.paint_path(path, alpha(0x000000, 0.35));
                    }
                }
                // Its hits, where slicing cuts it.
                for &x in &hits {
                    quad(window, x_of(x), y0, 1.0, 7.0, alpha(0xffffff, 0.45));
                }
                if let Some(x) = head {
                    for (width, a) in [(10.0, 0.12), (2.0, 1.0)] {
                        quad(window, x_of(x) - width / 2.0, y0, width, h, alpha(color, a));
                    }
                }
            },
        )
        .absolute()
        .size_full();
        let bounds = self.audio_ui.bounds.clone();
        let mark = |share: f32, which: ClipMark, cx: &mut Context<Self>| {
            let x = across(share);
            let trim = matches!(which, ClipMark::Start | ClipMark::End);
            let inward = matches!(which, ClipMark::End | ClipMark::FadeOut);
            let tone = match which {
                ClipMark::Start => MINT,
                ClipMark::End => CORAL,
                _ => TEXT,
            };
            // A trim's handle runs its full height; a fade's is just its dot, over the trim's.
            div()
                .absolute()
                .left(gpui::relative(x))
                .when(trim, |d| d.top_0().bottom_0())
                .when(!trim, |d| d.top(px(18.)).h(px(15.)))
                .w(px(15.))
                .ml(px(-7.))
                .cursor_ew_resize()
                .debug_selector(move || format!("clip-mark-{which:?}"))
                .when(!(-0.01..=1.01).contains(&x), |d| d.invisible())
                .when(trim, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left(px(7.))
                            .top_0()
                            .bottom_0()
                            .w(px(2.))
                            .bg(rgb(tone)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(4.))
                            .top(gpui::relative(0.5))
                            .mt(px(-14.))
                            .w(px(8.))
                            .h(px(28.))
                            .rounded(px(4.))
                            .bg(rgb(tone))
                            .border_1()
                            .border_color(rgb(BG)),
                    )
                    .child(
                        div()
                            .absolute()
                            .when(!inward, |d| d.left(px(7.)).rounded_r(px(4.)))
                            .when(inward, |d| d.right(px(7.)).rounded_l(px(4.)))
                            .top_0()
                            .h(px(15.))
                            .px(px(4.))
                            .flex()
                            .items_center()
                            .bg(rgb(tone))
                            .text_size(px(9.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(BG))
                            .child(match which {
                                ClipMark::Start => "START",
                                _ => "END",
                            }),
                    )
                })
                // A fade's handle: a dot on the top edge where it ends.
                .when(!trim, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left(px(2.))
                            .top(px(2.))
                            .size(px(11.))
                            .rounded(px(6.))
                            .bg(rgb(tone))
                            .border_2()
                            .border_color(rgb(color)),
                    )
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        let b = ws.audio_ui.bounds.get();
                        ws.begin_drag(
                            Drag::Clip {
                                at,
                                mark: which,
                                left: f32::from(b.origin.x),
                                width: f32::from(b.size.width),
                            },
                            cx,
                        );
                    }),
                )
        };
        // Seconds along the bottom, at a spacing that suits the zoom.
        let shown = seconds * (view.1 - view.0);
        let step = [
            0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0,
        ]
        .into_iter()
        .find(|&s| shown / s <= 10.0)
        .unwrap_or(120.0);
        let first = (seconds * view.0 / step).ceil() as i64;
        let ticks: Vec<(f32, String)> = (first..)
            .map(|k| k as f32 * step)
            .take_while(|&sec| sec <= seconds * view.1)
            .map(|sec| {
                let label = match step < 1.0 {
                    true => format!("{sec:.2}s"),
                    false => format!("{sec:.0}s"),
                };
                (across(sec / seconds), label)
            })
            .collect();
        let zoomed_in = view != (0.0, 1.0);
        // The words sung (Lyrics), each where it's sung, as far as the next one: a line's first
        // marked, a hook's in amber. Without words, each line as one.
        let sung: Vec<(f32, f32, String, bool, bool)> = match (&r.content, &self.lyrics) {
            // Sliced (Smart Slice): each slice's name where it starts, a voice's in amber.
            (Content::Audio { file, .. }, _)
                if self.beat.as_ref().is_some_and(|b| &b.file == file)
                    && !(self.flips.tab == crate::editors::audio::LyricsTab::Words
                        && self.lyrics.as_ref().is_some_and(|l| &l.file == file)) =>
            {
                self.beat
                    .as_ref()
                    .map(|b| {
                        b.slices
                            .iter()
                            .map(|s| (s.start, s.end, s.name.clone(), true, s.voice))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            (Content::Audio { file, .. }, Some(l)) if &l.file == file => l
                .lines
                .iter()
                .flat_map(|line| {
                    // More than 40 s across (a whole song): each line as one, the words unreadable.
                    let wide = (view.1 - view.0) * seconds > 40.0;
                    let words: Vec<(f32, f32, String)> = match line.words.is_empty() || wide {
                        true => vec![(line.start, line.end, line.text.clone())],
                        false => line
                            .words
                            .iter()
                            .map(|w| (w.start, w.end, w.text.clone()))
                            .collect(),
                    };
                    let (start, hook) = (line.start, line.sung > 1);
                    words.into_iter().enumerate().map(move |(k, (a, b, t))| {
                        (if k == 0 { start } else { a }, b, t, k == 0, hook)
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        // Too many in view to read (a long file, a dense beat): faint ticks, no names.
        let shown = sung
            .iter()
            .filter(|(a, _, _, _, _)| (0.0..=1.0).contains(&across(a / seconds)))
            .count();
        let dense = shown > 64;
        let words = sung
            .iter()
            .enumerate()
            .filter_map(|(i, (a, b, text, first, hook))| {
                let next = sung.get(i + 1).map_or(*b, |n| n.0).max(*a);
                let (x, x2) = (across(a / seconds), across(next / seconds));
                if x2 < 0.0 || x > 1.0 {
                    return None;
                }
                let tone = if *hook { AMBER } else { 0xffffff };
                Some(
                    div()
                        .absolute()
                        .left(gpui::relative(x))
                        .w(gpui::relative((x2 - x).max(0.0)))
                        .top(px(26.))
                        .h(px(15.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .pl(px(3.))
                        .border_l_1()
                        .border_color(alpha(
                            tone,
                            match (dense, *first) {
                                (true, _) => 0.18,
                                (false, true) => 0.85,
                                (false, false) => 0.25,
                            },
                        ))
                        .text_size(px(10.))
                        .text_color(rgb(if *hook { AMBER } else { TEXT_2 }))
                        .when(!dense, |d| d.child(text.clone())),
                )
            });
        div()
            .flex_1()
            .min_h(px(120.))
            .relative()
            .rounded(px(12.))
            .bg(rgb(0x0a0a0d))
            .border_1()
            .border_color(rgb(LINE_SOFT))
            .overflow_hidden()
            .debug_selector(|| "audio-wave".into())
            .child(
                canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(drawn)
            .children(ticks.into_iter().map(|(x, label)| {
                div()
                    .absolute()
                    .left(gpui::relative(x))
                    .bottom(px(2.))
                    .pl(px(3.))
                    .border_l_1()
                    .border_color(alpha(0xffffff, 0.12))
                    .font_family(MONO)
                    .text_size(px(8.5))
                    .text_color(rgb(TEXT_4))
                    .child(label)
            }))
            .children(words)
            .child(mark(from, ClipMark::Start, cx))
            .child(mark(to, ClipMark::End, cx))
            .child(mark(fade_in, ClipMark::FadeIn, cx))
            .child(mark(fade_out, ClipMark::FadeOut, cx))
            .when(zoomed_in, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(gpui::relative(view.0))
                        .w(gpui::relative(view.1 - view.0))
                        .top(px(2.))
                        .h(px(3.))
                        .rounded(px(2.))
                        .bg(alpha(0xffffff, 0.3)),
                )
            })
            .on_scroll_wheel(cx.listener(|ws, e: &ScrollWheelEvent, _, cx| {
                let b = ws.audio_ui.bounds.get();
                let x = f32::from(e.position.x - b.origin.x) / f32::from(b.size.width).max(1.0);
                let d = e.delta.pixel_delta(px(16.));
                ws.audio_ui.view = zoomed(
                    ws.audio_ui.view,
                    x.clamp(0.0, 1.0),
                    f32::from(d.y),
                    f32::from(d.x),
                );
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, e: &MouseDownEvent, _, cx| {
                    if e.click_count >= 2 {
                        ws.audio_ui.view = None;
                        cx.notify();
                    }
                }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_files_of_one_name_get_their_own_reversed_copies_and_each_goes_back_to_its_own() {
        let dir = std::env::temp_dir().join(format!("pina-reversed-{}", std::process::id()));
        let audio = dir.join("Audio Files");
        std::fs::create_dir_all(&audio).unwrap();
        let (a, b) = (
            Path::new("Audio Files/Loop.wav"),
            Path::new("Pack/Loop.wav"),
        );
        std::fs::write(dir.join(a), b"").unwrap();
        let (ra, write) = reversed_of(&dir, a);
        assert!(write && is_reversed(&ra) && !is_reversed(a), "{ra:?}");
        std::fs::write(dir.join(&ra), b"").unwrap();
        // The other folder's Loop isn't given the first one's copy.
        let (rb, write) = reversed_of(&dir, b);
        assert!(write && rb != ra, "{rb:?}");
        // Back: each copy to the file it was made from, when that's beside it.
        assert_eq!(reversed_of(&dir, &ra), (a.to_path_buf(), false));
        // A copy made before copies were marked still goes back.
        let old = Path::new("Audio Files/Loop (reversed).wav");
        assert!(is_reversed(old));
        assert_eq!(reversed_of(&dir, old), (a.to_path_buf(), false));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
