//! Smart Slice, in the Audio editor: an audio region's recording sliced as a producer would
//! (`pina_engine::smart`), in one click. It listens to what's in it (a beat, a voice in a beat, a
//! voice alone: by its sound) and slices it that way: a beat at its hits, each slice named for
//! what it sounds like; a voice into vocal chops, named by the words sung once they're heard
//! (they're heard at once); a song at its hits, voice and all (or, asked, without the voice), its
//! words a tab away. What it heard can be put right. The slices show beside the waveform, each to
//! hear, narrowed by kind, onto a Sampler; Flips flips them.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use gpui::{AppContext, Context, Div, FontWeight, MouseButton, canvas, div, prelude::*, px, rgb};
use pina_engine::{
    audio::engine::{AudioData, resample},
    samples::Category,
    smart::{self, Going, InIt, Slice},
    song::project::{At, Content, Timing},
};

use super::lyrics::{clock, hear_button, note, pad, progress, waiting};
use crate::{
    sounds::browser::primary,
    ui::{
        menu::{Menu, MenuAction},
        theme::*,
    },
    workspace::Workspace,
};

/// Which slices show: all, or one kind.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SliceFilter {
    Drums,
    Tonal,
    Voice,
    Named(String),
}

impl SliceFilter {
    fn keeps(&self, s: &Slice) -> bool {
        match self {
            SliceFilter::Drums => s.drum(),
            SliceFilter::Tonal => !s.drum() && s.category != Category::Vocal,
            SliceFilter::Voice => s.voice || s.category == Category::Vocal,
            SliceFilter::Named(n) => kind_of(s) == *n,
        }
    }

    fn name(&self) -> String {
        match self {
            SliceFilter::Drums => "Drums".into(),
            SliceFilter::Tonal => "Chords, notes & phrases".into(),
            SliceFilter::Voice => "With a voice".into(),
            SliceFilter::Named(n) => n.clone(),
        }
    }
}

/// A slice's kind, without its note: "Chord Am" is a Chord, a vocal chop a Vocal.
fn kind_of(s: &Slice) -> String {
    if s.category == Category::Vocal && s.voice {
        return "Vocal".into();
    }
    s.name.split(' ').next().map_or(String::new(), |w| match w {
        "Closed" | "Open" => s.name.split(' ').take(2).collect::<Vec<_>>().join(" "),
        w => w.to_string(),
    })
}

/// A recording sliced: its region, the region's file, the file cut (it, or it without its
/// voice), what's in it (as heard, or as put right; guessed when there was nothing to listen
/// with), its slices and audio, which show, and which is being heard.
pub struct Beat {
    pub at: At,
    pub file: PathBuf,
    pub cut: PathBuf,
    pub heard: InIt,
    pub guessed: bool,
    pub slices: Vec<Slice>,
    pub audio: Arc<(Vec<Vec<f32>>, f64)>,
    pub unvoiced: bool,
    pub filter: Option<SliceFilter>,
    pub playing: Option<(usize, Instant)>,
}

/// What Smart Slice is doing in the background.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Doing {
    Listening = 0,
    Separating = 1,
    Slicing = 2,
}

/// Slicing in the background: what it's doing and how far (0 to 1000), Stop, or what went wrong.
pub struct BeatWait {
    pub file: PathBuf,
    doing: Arc<AtomicU32>,
    going: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    started: Instant,
    pub failed: Option<String>,
    /// What to do again, if it failed: its way of slicing.
    again: (Option<InIt>, bool),
}

impl BeatWait {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The icon for a slice's kind.
fn icon_of(s: &Slice) -> Icon {
    match s.category {
        Category::Kick | Category::Bass808 => Icon::Kick,
        Category::Snare | Category::Rim | Category::Snap => Icon::Snare,
        Category::Clap => Icon::Clap,
        Category::HatClosed | Category::HatOpen | Category::Shaker | Category::Tambourine => {
            Icon::HiHat
        }
        Category::Ride | Category::Crash => Icon::Cymbal,
        Category::Tom => Icon::Tom,
        Category::Perc => Icon::Percussion,
        Category::Vocal => Icon::Text,
        c if !c.is_drum() && s.name.starts_with("Hit") => Icon::Pulse,
        c if !c.is_drum() && s.name.starts_with("Phrase") => Icon::Wave,
        _ => Icon::Note,
    }
}

/// Opens a menu of `items` where it's clicked.
fn menu(
    items: Vec<(String, MenuAction)>,
    cx: &mut Context<Workspace>,
) -> impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    cx.listener(move |ws, e: &gpui::MouseDownEvent, _, cx| {
        cx.stop_propagation();
        ws.menu = Some(Menu {
            at: e.position,
            items: items.clone(),
            ..Default::default()
        });
        cx.notify();
    })
}

impl Workspace {
    /// Smart Slice on the audio region at `at`, in one click: what's in it heard (kept from
    /// before, or listened for), then sliced that way. Already sliced, its slices.
    pub(crate) fn smart_slice(&mut self, at: At, cx: &mut Context<Self>) {
        let Content::Audio { file, .. } = &self.session.project.region(at).content else {
            return;
        };
        if self.beat.as_ref().is_some_and(|b| &b.file == file)
            || self
                .beat_wait
                .as_ref()
                .is_some_and(|w| &w.file == file && w.failed.is_none())
        {
            return cx.notify();
        }
        self.slice_as(at, None, false, cx);
    }

    /// The region at `at` sliced, as `heard` says is in it (listened for, if none), `unvoiced`
    /// (its voice taken out first) or not: the stretch it plays, in the background, what it's
    /// doing and how far in the panel; kept, so it's instant the next time.
    pub(crate) fn slice_as(
        &mut self,
        at: At,
        heard: Option<InIt>,
        unvoiced: bool,
        cx: &mut Context<Self>,
    ) {
        let p = &self.session.project;
        let r = p.region(at);
        let Content::Audio { file, .. } = &r.content else {
            return;
        };
        let rate = p.sample_rate as f64;
        let stretch = Timing::of(&r.content, rate).map(|t| {
            let frame = |tick: u64| p.audio_frame(&t, r.start as f64, tick as f64) / rate;
            (frame(r.start) as f32, frame(r.start + r.length) as f32)
        });
        let file = file.clone();
        if let Some(w) = self.beat_wait.take() {
            w.stop();
        }
        // What it was put right to, kept on: a re-slice keeps the tab.
        let keep_tab = self.beat.as_ref().is_some_and(|b| b.file == file);
        self.beat = None;
        if self.lyrics.as_ref().is_some_and(|l| l.file != file) {
            self.lyrics = None;
        }
        if !keep_tab {
            self.flips.tab = crate::editors::audio::LyricsTab::Lines;
        }
        let doing = Arc::new(AtomicU32::new(match heard {
            None => Doing::Listening as u32,
            Some(_) if unvoiced => Doing::Separating as u32,
            Some(_) => Doing::Slicing as u32,
        }));
        let (going, stop) = (Arc::<AtomicU32>::default(), Arc::<AtomicBool>::default());
        self.beat_wait = Some(BeatWait {
            file: file.clone(),
            doing: doing.clone(),
            going: going.clone(),
            stop: stop.clone(),
            started: Instant::now(),
            failed: None,
            again: (heard, unvoiced),
        });
        self.status = Some("Smart Slice: listening to what's in it…".into());
        let dir = self.session.dir.clone();
        let job = cx.background_spawn({
            let (file, doing, going, stop) =
                (file.clone(), doing.clone(), going.clone(), stop.clone());
            async move {
                let tell = |d: Doing, p: f32| {
                    doing.store(d as u32, Ordering::Relaxed);
                    going.store((p.clamp(0.0, 1.0) * 1000.0) as u32, Ordering::Relaxed);
                };
                let (heard, sure) = match heard {
                    Some(h) => (h, true),
                    None => smart::hear(&dir, &file, |p| tell(Doing::Listening, p), &stop)?,
                };
                let cut = smart::slice_file(
                    &dir,
                    &file,
                    stretch,
                    unvoiced,
                    heard == InIt::Voice,
                    |g| match g {
                        Going::Separating(p) => tell(Doing::Separating, p),
                        Going::Slicing(p) => tell(Doing::Slicing, p),
                    },
                    &stop,
                )?;
                Ok::<_, String>((heard, sure, cut))
            }
        });
        // The panel redrawn as it goes.
        cx.spawn({
            let stop = stop.clone();
            async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(200))
                        .await;
                    let on = this.update(cx, |ws, cx| {
                        let ours = ws
                            .beat_wait
                            .as_ref()
                            .is_some_and(|w| Arc::ptr_eq(&w.stop, &stop) && w.failed.is_none());
                        if ours {
                            cx.notify();
                        }
                        ours
                    });
                    if !matches!(on, Ok(true)) {
                        break;
                    }
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let done = job.await;
            let _ = this.update(cx, |ws, cx| {
                if !ws
                    .beat_wait
                    .as_ref()
                    .is_some_and(|w| Arc::ptr_eq(&w.stop, &stop))
                {
                    return;
                }
                match done {
                    Ok((heard, sure, c)) => {
                        ws.beat_wait = None;
                        let n = c.slices.len();
                        ws.status = Some(match heard {
                            InIt::Voice => format!(
                                "A voice alone: {n} vocal chops, named by their words as they're \
                                 heard"
                            ),
                            InIt::Mixed => format!(
                                "A voice in a beat: {n} slices, voice and all; its words are under \
                                 Words"
                            ),
                            InIt::Beat => format!("{n} slices at its hits"),
                        });
                        ws.beat = Some(Beat {
                            at,
                            file,
                            cut: c.file,
                            heard,
                            guessed: !sure,
                            slices: c.slices,
                            audio: Arc::new(c.audio),
                            unvoiced,
                            filter: None,
                            playing: None,
                        });
                        // A voice alone: its words heard now, to name its chops by.
                        let named = ws
                            .beat
                            .as_ref()
                            .is_some_and(|b| b.slices.iter().any(|s| !s.name.starts_with("Vocal")));
                        if heard == InIt::Voice
                            && !named
                            && !pina_engine::lyrics::installed(&pina_engine::lyrics::folder())
                                .is_empty()
                        {
                            ws.hear_lyrics(at, cx);
                        }
                    }
                    Err(_) if stop.load(Ordering::Relaxed) => {
                        ws.beat_wait = None;
                        ws.status = Some("Smart Slice stopped".into());
                    }
                    Err(e) => {
                        ws.status = Some(e.clone());
                        if let Some(w) = ws.beat_wait.as_mut() {
                            w.failed = Some(e);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// What's in it put right (from the panel): sliced again that way.
    pub(crate) fn slice_heard_as(&mut self, heard: InIt, cx: &mut Context<Self>) {
        if let Some(b) = self.beat.as_ref() {
            let at = b.at;
            let unvoiced = b.unvoiced && heard == InIt::Mixed;
            let _ = smart::put_right(&self.session.dir, &b.file, heard);
            self.slice_as(at, Some(heard), unvoiced, cx);
        }
    }

    /// A song's slices with its voice in them, or (`unvoiced`) without it.
    pub(crate) fn slice_unvoiced(&mut self, unvoiced: bool, cx: &mut Context<Self>) {
        if let Some(b) = self.beat.as_ref().filter(|b| b.unvoiced != unvoiced) {
            let (at, heard) = (b.at, b.heard);
            self.slice_as(at, Some(heard), unvoiced, cx);
        }
    }

    fn stop_slicing(&mut self, cx: &mut Context<Self>) {
        if let Some(w) = self.beat_wait.as_ref() {
            w.stop();
        }
        cx.notify();
    }

    /// Slice `i` heard; heard again, stopped.
    fn hear_slice(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(b) = self.beat.as_mut() else {
            return;
        };
        let now = Instant::now();
        if b.playing
            .take()
            .is_some_and(|(j, until)| j == i && now < until)
        {
            self.player.stop_audition();
            return cx.notify();
        }
        let Some(s) = b.slices.get(i) else {
            return;
        };
        let (sides, rate) = &*b.audio;
        let (a, z) = (
            (s.start as f64 * rate) as usize,
            (s.end as f64 * rate) as usize,
        );
        let to = self.player.rate();
        let channels: Vec<Vec<f32>> = sides
            .iter()
            .map(|x| resample(&x[a.min(x.len())..z.min(x.len())], *rate, to))
            .collect();
        if channels.iter().all(Vec::is_empty) {
            return;
        }
        let _ = self
            .player
            .audition_audio(Arc::new(AudioData::new(channels)), 0.0, 0.0);
        b.playing = Some((i, now + Duration::from_secs_f32(s.end - s.start)));
        // Lit until it's played.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs_f32(0.05))
                .await;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let on = this.update(cx, |ws, cx| {
                    let on = ws
                        .beat
                        .as_ref()
                        .and_then(|b| b.playing)
                        .is_some_and(|(j, until)| j == i && Instant::now() < until);
                    cx.notify();
                    on
                });
                if !matches!(on, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// The slices shown (all, or the kind picked) onto a Sampler track of their own, a zone
    /// each, played where they play now; opened in the Sampler.
    pub(crate) fn slices_to_sampler(&mut self, at: At, cx: &mut Context<Self>) {
        let Some(b) = self.beat.as_ref() else {
            return;
        };
        let shown: Vec<(f32, f32, String)> = b
            .slices
            .iter()
            .filter(|s| b.filter.as_ref().is_none_or(|f| f.keeps(s)))
            .map(|s| (s.start, s.end, s.name.clone()))
            .collect();
        let (sides, rate) = &*b.audio;
        let seconds = sides.first().map_or(0, Vec::len) as f32 / *rate as f32;
        let stem = b
            .file
            .file_stem()
            .map_or("Audio".into(), |s| s.to_string_lossy().into_owned());
        let name = match &b.filter {
            Some(f) => format!("{stem} {}", f.name()),
            None => format!("{stem} Slices"),
        };
        let cut = b.cut.clone();
        let bar = self.session.project.bar().max(1);
        let tick = self.playhead() / bar * bar;
        let mut made = None;
        self.edit(cx, |ws| {
            made = ws
                .session
                .project
                .slices_to_sampler(at, &cut, seconds, &shown, &name, tick)
        });
        match made {
            Some(t) => {
                self.select_track(t);
                self.region = Some((t, 0));
                self.sampler_ui.zone = 0;
                self.sampler_ui.chop = None;
                self.show_bottom(crate::workspace::Bottom::Sampler, cx);
                let most = (127 - pina_engine::song::project::FIRST_SLICE) as usize + 1;
                self.status = Some(format!(
                    "{} slices on {name}, a pad each, played where they were{}",
                    shown.len().min(most),
                    match shown.len() > most {
                        true => format!(" (the first {most}: narrow them by kind for the rest)"),
                        false => String::new(),
                    }
                ));
            }
            None => {
                self.session.cancel();
                self.status = Some("No slices to put on a Sampler".into());
            }
        }
        cx.notify();
    }

    /// The panel while Smart Slice works: what it's doing (listening, taking the voice out,
    /// cutting) and how far, with Stop; or what went wrong.
    pub(crate) fn beat_waiting(&mut self, at: At, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(w) = self.beat_wait.as_ref() else {
            return Vec::new();
        };
        let (heard, unvoiced) = w.again;
        if let Some(e) = w.failed.clone() {
            return vec![
                note("Couldn't slice it", e),
                pad().child(button("Try again", false).h(px(26.)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.slice_as(at, heard, unvoiced, cx)),
                )),
            ];
        }
        let going = w.going.load(Ordering::Relaxed) as f32 / 1000.0;
        let secs = w.started.elapsed().as_secs();
        let (what, how, far) = match w.doing.load(Ordering::Relaxed) {
            0 => (
                "Listening to what's in it",
                "A beat, a voice in a beat, or a voice alone: it slices each its own way.",
                if going > 0.0 { going } else { -1.0 },
            ),
            1 => (
                "Taking the voice out",
                "Once for this file: it's kept.",
                going,
            ),
            _ => (
                "Cutting it",
                "Each slice named for what it sounds like.",
                going,
            ),
        };
        vec![
            div()
                .px(px(12.))
                .pt(px(4.))
                .pb(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .debug_selector(|| "slices-going".into())
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .child(what),
                        )
                        .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(how)),
                )
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(rgb(TEXT_3))
                        .child(format!("{secs}s")),
                )
                .child(
                    button("Stop", false)
                        .h(px(24.))
                        .debug_selector(|| "slices-stop".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.stop_slicing(cx)),
                        ),
                ),
            progress(far),
        ]
    }

    /// What it heard in it, to put right: a dropdown in the panel's header.
    pub(crate) fn heard_as_picker(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let b = self.beat.as_ref()?;
        let items: Vec<(String, MenuAction)> = [InIt::Beat, InIt::Mixed, InIt::Voice]
            .iter()
            .map(|&h| {
                (
                    format!("{}{}", if h == b.heard { "✓ " } else { "    " }, h.name()),
                    MenuAction::SliceHeardAs(h),
                )
            })
            .collect();
        Some(
            dropdown(b.heard.name(), 22.)
                .flex_none()
                .debug_selector(|| "smart-heard-as".into())
                .on_mouse_down(MouseButton::Left, menu(items, cx)),
        )
    }

    /// The slices: how many of each kind, which show (a dropdown), all onto a Sampler; a row a
    /// slice: where it is, its kind's icon and name (a voice in it said), its waveform, to hear
    /// (click the row).
    pub(crate) fn slices_page(&mut self, at: At, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(cut) = self.beat.as_ref().map(|b| b.cut.clone()) else {
            return Vec::new();
        };
        let wave = self.wave(&cut, cx);
        let seconds = wave.as_ref().map_or(1.0, |w| w.seconds()).max(0.001);
        let Some(b) = self.beat.as_ref() else {
            return Vec::new();
        };
        let playing = b
            .playing
            .filter(|(_, until)| Instant::now() < *until)
            .map(|(i, _)| i);
        // Kinds, the most first.
        let mut kinds: Vec<(String, usize)> = Vec::new();
        for s in &b.slices {
            let k = kind_of(s);
            match kinds.iter_mut().find(|(n, _)| *n == k) {
                Some(e) => e.1 += 1,
                None => kinds.push((k, 1)),
            }
        }
        kinds.sort_by_key(|k| std::cmp::Reverse(k.1));
        let alone = b.heard == InIt::Voice;
        let voiced = match alone {
            true => 0,
            false => b.slices.iter().filter(|s| s.voice).count(),
        };
        let summary = match alone {
            true => format!("{} vocal chops", b.slices.len()),
            false => kinds
                .iter()
                .take(3)
                .map(|(n, c)| format!("{c} {}", n.to_lowercase()))
                .chain((voiced > 0).then(|| format!("{voiced} with the voice")))
                .collect::<Vec<_>>()
                .join(" · "),
        };
        let tick = |on: bool| if on { "✓ " } else { "    " };
        let mut filters: Vec<(String, MenuAction)> = vec![(
            format!("{}All {}", tick(b.filter.is_none()), b.slices.len()),
            MenuAction::SliceFilter(None),
        )];
        let add = |f: SliceFilter, filters: &mut Vec<(String, MenuAction)>| {
            let n = b.slices.iter().filter(|s| f.keeps(s)).count();
            if n > 0 {
                filters.push((
                    format!("{}{} {n}", tick(b.filter.as_ref() == Some(&f)), f.name()),
                    MenuAction::SliceFilter(Some(f)),
                ));
            }
        };
        add(SliceFilter::Drums, &mut filters);
        add(SliceFilter::Tonal, &mut filters);
        add(SliceFilter::Voice, &mut filters);
        filters.push((String::new(), MenuAction::Nothing));
        for (k, _) in &kinds {
            add(SliceFilter::Named(k.clone()), &mut filters);
        }
        let shown: Vec<usize> = (0..b.slices.len())
            .filter(|&i| b.filter.as_ref().is_none_or(|f| f.keeps(&b.slices[i])))
            .collect();
        // A screenful at a time: the kinds narrow a song's hundreds.
        const MOST: usize = 200;
        let more = shown.len().saturating_sub(MOST);
        let filter_name = b
            .filter
            .as_ref()
            .map_or(format!("All {}", b.slices.len()), |f| {
                format!("{} {}", f.name(), shown.len())
            });
        let first = b.slices.first().map_or(0.0, |s| s.start);
        let (heard, guessed, unvoiced) = (b.heard, b.guessed, b.unvoiced);
        let can_unvoice =
            pina_engine::lyrics::vocals_model(&pina_engine::separate::folder()).is_some();
        // A voice in a beat: kept in the slices, or taken out first.
        let voice_items: Vec<(String, MenuAction)> = vec![
            (
                format!("{}Keep the voice in", if unvoiced { "    " } else { "✓ " }),
                MenuAction::SliceUnvoiced(false),
            ),
            (
                match can_unvoice {
                    true => format!("{}Take the voice out", if unvoiced { "✓ " } else { "    " }),
                    false => "    Take the voice out (needs a separation model with vocals)".into(),
                },
                match can_unvoice {
                    true => MenuAction::SliceUnvoiced(true),
                    false => MenuAction::Nothing,
                },
            ),
        ];
        let mut out = vec![
            pad()
                .flex()
                .flex_col()
                .gap(px(6.))
                .when(guessed, |d| {
                    d.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(format!(
                                "Heard as {} without a separation model to listen with: if it's \
                                 wrong, change it above.",
                                heard.name().to_lowercase()
                            )),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .child(summary),
                        )
                        .when(heard == InIt::Mixed, |d| {
                            d.child(
                                dropdown(
                                    if unvoiced {
                                        "Voice taken out"
                                    } else {
                                        "Voice kept in"
                                    },
                                    22.,
                                )
                                .flex_none()
                                .debug_selector(|| "slices-voice".into())
                                .on_mouse_down(MouseButton::Left, menu(voice_items, cx)),
                            )
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            dropdown(filter_name, 26.)
                                .flex_1()
                                .debug_selector(|| "slices-filter".into())
                                .on_mouse_down(MouseButton::Left, menu(filters, cx)),
                        )
                        .child(
                            primary("To Sampler")
                                .h(px(26.))
                                .px(px(12.))
                                .debug_selector(|| "slices-to-sampler".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| ws.slices_to_sampler(at, cx)),
                                ),
                        ),
                ),
        ];
        let rows: Vec<Div> = shown
            .iter()
            .take(MOST)
            .map(|&i| {
                let s = &b.slices[i];
                let on = playing == Some(i);
                let view = (s.start / seconds, s.end / seconds);
                let loading = wave.is_none();
                let wave = wave.clone();
                let tone = if on { ACCENT } else { SLATE };
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(12.))
                    .py(px(5.))
                    .border_t_1()
                    .border_color(rgb(LINE_SOFT))
                    .cursor_pointer()
                    .when(on, |d| {
                        d.bg(rgb(FIELD)).child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(2.))
                                .bg(rgb(ACCENT)),
                        )
                    })
                    .when(!on, |d| d.hover(|h| h.bg(rgb(OFF))))
                    .debug_selector(move || format!("slice-{i}"))
                    .child(
                        div()
                            .w(px(40.))
                            .flex_none()
                            .font_family(MONO)
                            .text_size(px(10.5))
                            .text_color(rgb(TEXT_3))
                            .child(clock(s.start - first)),
                    )
                    .child(icon(icon_of(s), 13.).flex_none().text_color(rgb(if on {
                        TEXT
                    } else {
                        TEXT_2
                    })))
                    .child(
                        div()
                            .w(px(104.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(s.name.clone()),
                            )
                            .when(s.voice && !alone, |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .font_family(MONO)
                                        .text_size(px(10.))
                                        .text_color(rgb(TEXT_3))
                                        .child("voice"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .relative()
                            .h(px(20.))
                            .rounded(px(3.))
                            .overflow_hidden()
                            .bg(rgb(WELL))
                            .when(loading, |d| d.child(waiting(tone, format!("slice-{i}"))))
                            .child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _, window, _| {
                                        if let Some(w) = &wave {
                                            crate::editors::sampler::paint_wave(
                                                window,
                                                w,
                                                bounds,
                                                view,
                                                1.0,
                                                |_| (alpha(tone, 0.45), alpha(tone, 0.95)),
                                            );
                                        }
                                    },
                                )
                                .absolute()
                                .size_full(),
                            ),
                    )
                    .child(hear_button(on, format!("slice-hear-{i}")).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.hear_slice(i, cx)
                        }),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.hear_slice(i, cx)),
                    )
            })
            .collect();
        out.extend(rows);
        if more > 0 {
            out.push(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .border_t_1()
                    .border_color(rgb(LINE_SOFT))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(format!(
                        "{more} more: narrow them by kind above, or trim the region to the part to \
                         slice"
                    )),
            );
        }
        out
    }
}

/// A slice's kind, for a test's eyes.
#[cfg(test)]
pub(crate) fn kinds(slices: &[Slice]) -> Vec<String> {
    slices.iter().map(kind_of).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(name: &str, category: Category, voice: bool) -> Slice {
        Slice {
            start: 0.0,
            end: 0.1,
            name: name.into(),
            category,
            confidence: 0.9,
            voice,
        }
    }

    #[test]
    fn slices_are_narrowed_by_kind_their_notes_and_voice_aside() {
        let s = [
            slice("Kick", Category::Kick, false),
            slice("Closed Hat", Category::HatClosed, false),
            slice("Chord Am", Category::Chord, true),
            slice("Chord C", Category::Chord, false),
            slice("Phrase Dm", Category::Fx, false),
            slice("letting go", Category::Vocal, true),
        ];
        assert_eq!(
            kinds(&s),
            ["Kick", "Closed Hat", "Chord", "Chord", "Phrase", "Vocal"]
        );
        let keeps = |f: SliceFilter| s.iter().filter(|x| f.keeps(x)).count();
        assert_eq!(keeps(SliceFilter::Drums), 2);
        assert_eq!(keeps(SliceFilter::Tonal), 3);
        assert_eq!(keeps(SliceFilter::Voice), 2);
        assert_eq!(keeps(SliceFilter::Named("Chord".into())), 2);
        assert_eq!(keeps(SliceFilter::Named("Vocal".into())), 1);
    }
}
