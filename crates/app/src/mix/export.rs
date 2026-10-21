//! The Bounce sheet (⌘B, or Export in the control bar), bouncing and exporting in one sheet:
//! what to bounce (the mix, each track as a stem, or each marked
//! section) over which range (the song, the cycle, any bars), on the left; how (formats, several
//! at once; bits, sample rate, the MP3's and AAC's bitrate; level: as it plays, to a peak, or to a
//! loudness under a ceiling; the tail; dither; stems through the master or not), on the right;
//! presets across the top; where it goes, what it'll be called and how much it makes, at the
//! foot. Bouncing shows each file's progress, then its loudness and peak, to play or show in the
//! Finder. Bounce in Place renders a track to audio on a track of its own, the original muted.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use gpui::{
    Context, Div, Entity, FontWeight, MouseButton, PathPromptOptions, div, prelude::*, px, rgb,
};
use pina_engine::{
    bounce::{self, Bits, Done, Format, Job, Level, Options},
    commands::{self, Command},
};

use crate::{
    ui::text::{TextEvent, TextInput},
    ui::theme::*,
    workspace::Workspace,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum What {
    Mix,
    Stems,
    Sections,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Span {
    Song,
    Cycle,
    Bars,
}

/// The formats, in the order they're offered, with their defaults.
const FORMATS: [Format; 7] = [
    Format::Wav,
    Format::Aiff,
    Format::Flac,
    Format::Mp3 { kbps: 320 },
    Format::Aac { kbps: 256 },
    Format::Alac,
    Format::Midi,
];

/// Loudness targets, as the places they're for want them.
const TARGETS: [(f32, &str); 4] = [
    (-9.0, "Club"),
    (-14.0, "Streaming"),
    (-16.0, "Apple"),
    (-23.0, "Broadcast"),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Preset {
    Master,
    Streaming,
    Cd,
    Stems,
    Demo,
    Midi,
}

impl Preset {
    const ALL: [Preset; 6] = [
        Preset::Master,
        Preset::Streaming,
        Preset::Cd,
        Preset::Stems,
        Preset::Demo,
        Preset::Midi,
    ];

    fn name(self) -> &'static str {
        match self {
            Preset::Master => "Master",
            Preset::Streaming => "Streaming",
            Preset::Cd => "CD",
            Preset::Stems => "Stems",
            Preset::Demo => "Demo MP3",
            Preset::Midi => "MIDI",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Preset::Master => "WAV 24-bit, as it plays",
            Preset::Streaming => "WAV + MP3, −14 LUFS",
            Preset::Cd => "WAV 16-bit 44.1, dithered",
            Preset::Stems => "Each track, 24-bit",
            Preset::Demo => "MP3 192, −14 LUFS",
            Preset::Midi => "The notes, a .mid",
        }
    }
}

/// What's come of each job, as it comes.
type Results = Vec<Option<Result<Done, String>>>;

/// A bounce running: each job's name and progress, and what's come of each.
pub struct Run {
    names: Vec<String>,
    progress: Arc<Mutex<Vec<f32>>>,
    results: Arc<Mutex<Results>>,
    cancel: Arc<AtomicBool>,
    pub finished: bool,
}

pub struct Export {
    pub what: What,
    pub span: Span,
    pub bars: (u64, u64),
    pub tracks: Vec<bool>,
    pub sections: Vec<bool>,
    pub formats: Vec<Format>,
    pub bits: Bits,
    pub rate: u32,
    pub level: Level,
    pub tail: bool,
    pub dither: bool,
    pub master_on_stems: bool,
    pub folder: PathBuf,
    pub name: Entity<TextInput>,
    pub run: Option<Run>,
    _subs: Vec<gpui::Subscription>,
}

/// Why a bounce came out quieter than its loudness target: its peaks met the ceiling first.
fn short_of(target: Option<(f32, f32)>, lufs: f32) -> Option<String> {
    let (want, ceiling) = target?;
    (lufs < want - 0.5).then(|| {
        format!("held at its {ceiling:.0} dB ceiling: a limiter on the master gets it louder")
    })
}

fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

impl Workspace {
    /// Opens the Bounce sheet, set as it was last (or for the mix, the whole song, WAV 24-bit).
    pub fn open_export(&mut self, cx: &mut Context<Self>) {
        if self
            .export
            .as_ref()
            .is_some_and(|e| e.run.as_ref().is_some_and(|r| !r.finished))
        {
            return;
        }
        let p = &self.session.project;
        let song = self
            .session
            .dir
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());
        let name = cx.new(|cx| {
            let mut t = TextInput::field("", cx);
            t.set_text(&song, cx);
            t
        });
        let sub = cx.subscribe(&name, |ws, _, e: &TextEvent, cx| {
            if let TextEvent::Confirm(_) = e {
                ws.start_export(cx);
            }
        });
        let with = bounce::tracks_with_regions(p);
        let bars = p.end().div_ceil(p.bar().max(1)).max(1);
        self.export = Some(Export {
            what: What::Mix,
            span: Span::Song,
            bars: (1, bars),
            tracks: (0..p.tracks.len()).map(|i| with.contains(&i)).collect(),
            sections: vec![true; bounce::sections(p).len()],
            formats: vec![Format::Wav],
            bits: Bits::I24,
            rate: p.sample_rate,
            level: Level::Off,
            tail: true,
            dither: true,
            master_on_stems: false,
            folder: self.session.dir.join("Bounces"),
            name,
            run: None,
            _subs: vec![sub],
        });
        cx.notify();
    }

    fn close_export(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.export.as_ref().and_then(|e| e.run.as_ref()) {
            r.cancel.store(true, Ordering::Relaxed);
        }
        self.export = None;
        self.refocus = true;
        cx.notify();
    }

    fn preset(&mut self, preset: Preset, cx: &mut Context<Self>) {
        let rate = self.session.project.sample_rate;
        let Some(e) = self.export.as_mut() else {
            return;
        };
        e.what = if preset == Preset::Stems {
            What::Stems
        } else {
            What::Mix
        };
        e.tail = true;
        e.master_on_stems = false;
        (e.formats, e.bits, e.rate, e.level, e.dither) = match preset {
            Preset::Master => (
                vec![Format::Wav],
                Bits::I24,
                rate.max(44_100),
                Level::Off,
                true,
            ),
            Preset::Streaming => (
                vec![Format::Wav, Format::Mp3 { kbps: 320 }],
                Bits::I24,
                48_000,
                Level::Loudness {
                    lufs: -14.0,
                    ceiling: -1.0,
                },
                true,
            ),
            Preset::Cd => (
                vec![Format::Wav],
                Bits::I16,
                44_100,
                Level::Peak { db: -0.3 },
                true,
            ),
            Preset::Stems => (
                vec![Format::Wav],
                Bits::I24,
                rate.max(44_100),
                Level::Off,
                true,
            ),
            Preset::Demo => (
                vec![Format::Mp3 { kbps: 192 }],
                Bits::I24,
                44_100,
                Level::Loudness {
                    lufs: -14.0,
                    ceiling: -1.0,
                },
                true,
            ),
            Preset::Midi => (vec![Format::Midi], e.bits, e.rate, e.level, e.dither),
        };
        cx.notify();
    }

    fn toggle_format(&mut self, f: Format, cx: &mut Context<Self>) {
        let Some(e) = self.export.as_mut() else {
            return;
        };
        match e
            .formats
            .iter()
            .position(|o| std::mem::discriminant(o) == std::mem::discriminant(&f))
        {
            Some(i) if e.formats.len() > 1 => drop(e.formats.remove(i)),
            Some(_) => {}
            None => e.formats.push(f),
        }
        cx.notify();
    }

    /// The range bounced, in ticks; None for the whole song.
    fn export_range(&self) -> Option<(u64, u64)> {
        let e = self.export.as_ref()?;
        let p = &self.session.project;
        match e.span {
            Span::Song => None,
            Span::Cycle => p.cycle.map(|c| (c.start, c.end)),
            Span::Bars => {
                let bar = p.bar();
                Some(((e.bars.0.max(1) - 1) * bar, e.bars.1.max(e.bars.0) * bar))
            }
        }
    }

    /// What the sheet would bounce: a job a file name.
    fn export_jobs(&self, cx: &gpui::App) -> Vec<Job> {
        let Some(e) = self.export.as_ref() else {
            return Vec::new();
        };
        let p = &self.session.project;
        let base = bounce::file_safe(e.name.read(cx).text());
        let base = if base.is_empty() {
            "Bounce".to_string()
        } else {
            base
        };
        let range = self.export_range();
        match e.what {
            What::Mix => vec![Job {
                name: base,
                track: None,
                range,
                master: true,
            }],
            What::Stems => p
                .tracks
                .iter()
                .enumerate()
                .filter(|(i, _)| e.tracks.get(*i).copied().unwrap_or(false))
                .map(|(i, t)| Job {
                    name: format!("{base} - {}", bounce::file_safe(&t.name)),
                    track: Some(i),
                    range,
                    master: e.master_on_stems,
                })
                .collect(),
            What::Sections => bounce::sections(p)
                .into_iter()
                .enumerate()
                .filter(|(k, _)| e.sections.get(*k).copied().unwrap_or(false))
                .map(|(k, (name, from, to))| Job {
                    name: format!("{base} - {:02} {}", k + 1, bounce::file_safe(&name)),
                    track: None,
                    range: Some((from, to)),
                    master: true,
                })
                .collect(),
        }
    }

    fn export_options(&self) -> Option<Options> {
        let e = self.export.as_ref()?;
        Some(Options {
            formats: e.formats.clone(),
            bits: e.bits,
            rate: e.rate,
            level: e.level,
            tail: e.tail,
            dither: e.dither,
        })
    }

    /// Bounces what the sheet says, in the background, a job at a time.
    pub(crate) fn start_export(&mut self, cx: &mut Context<Self>) {
        let jobs = self.export_jobs(cx);
        let Some(opts) = self.export_options() else {
            return;
        };
        if jobs.is_empty() {
            self.status = Some("Nothing to bounce: tick a track or a section.".into());
            cx.notify();
            return;
        }
        let mut project = self.session.project.clone();
        if let Err(e) = self.player.save_states(&mut project) {
            self.status = Some(e);
            cx.notify();
            return;
        }
        let Some(e) = self.export.as_mut() else {
            return;
        };
        let (dir, folder) = (self.session.dir.clone(), e.folder.clone());
        let progress = Arc::new(Mutex::new(vec![0f32; jobs.len()]));
        let results = Arc::new(Mutex::new(
            (0..jobs.len()).map(|_| None).collect::<Vec<_>>(),
        ));
        let cancel = Arc::new(AtomicBool::new(false));
        e.run = Some(Run {
            names: jobs.iter().map(|j| j.name.clone()).collect(),
            progress: progress.clone(),
            results: results.clone(),
            cancel: cancel.clone(),
            finished: false,
        });
        self.exporting = true;
        cx.notify();
        let work = cx.background_spawn(async move {
            for (k, job) in jobs.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let p = progress.clone();
                let done = bounce::bounce(&project, &dir, job, &opts, &folder, &move |f| {
                    if let Ok(mut v) = p.lock() {
                        v[k] = f;
                    }
                });
                if let Ok(mut r) = results.lock() {
                    r[k] = Some(done);
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let mut work = Some(work);
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let finished = work.as_ref().is_some_and(|w| w.is_ready());
                let more = this.update(cx, |ws, cx| {
                    if finished && let Some(r) = ws.export.as_mut().and_then(|e| e.run.as_mut()) {
                        r.finished = true;
                    }
                    if finished {
                        ws.exporting = false;
                        let made = ws
                            .export
                            .as_ref()
                            .and_then(|e| e.run.as_ref())
                            .map_or(0, |r| {
                                r.results
                                    .lock()
                                    .map_or(0, |v| v.iter().flatten().filter(|d| d.is_ok()).count())
                            });
                        ws.status = Some(format!("Bounced {made} of them"));
                    }
                    cx.notify();
                });
                if more.is_err() || finished {
                    if let Some(w) = work.take() {
                        w.await;
                    }
                    break;
                }
            }
        })
        .detach();
    }

    fn pick_export_folder(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Bounce Here".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dirs))) = picked.await else {
                return;
            };
            this.update(cx, |ws, cx| {
                if let (Some(e), Some(d)) = (ws.export.as_mut(), dirs.into_iter().next()) {
                    e.folder = d;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A track rendered to audio (through its own plugins, sends and fader, not the master's)
    /// on a new audio track under it, from its first region to its last's end and its tail; the
    /// track muted. One undo puts it back.
    pub fn bounce_in_place(&mut self, t: usize, cx: &mut Context<Self>) {
        self.bounce_range_in_place(t, None, cx);
    }

    /// The same over a range of ticks (a region's), or the whole track's regions.
    pub fn bounce_range_in_place(
        &mut self,
        t: usize,
        range: Option<(u64, u64)>,
        cx: &mut Context<Self>,
    ) {
        let p = &self.session.project;
        let Some(track) = p.tracks.get(t) else { return };
        let whole = (
            track.regions.iter().map(|r| r.start).min(),
            track.regions.iter().map(|r| r.start + r.length).max(),
        );
        let (Some(from), Some(to)) = range.map_or(whole, |(a, b)| (Some(a), Some(b))) else {
            self.status = Some(format!("{} has nothing to bounce.", track.name));
            cx.notify();
            return;
        };
        let mut project = p.clone();
        if let Err(e) = self.player.save_states(&mut project) {
            self.status = Some(e);
            cx.notify();
            return;
        }
        let name = format!("{} (bounced)", track.name);
        let job = Job {
            name: bounce::file_safe(&name),
            track: Some(t),
            range: Some((from, to)),
            master: false,
        };
        let (dir, rate) = (self.session.dir.clone(), p.sample_rate);
        self.status = Some(format!("Bouncing {} in place…", track.name));
        cx.notify();
        let work = cx.background_spawn(async move {
            let [l, r] = bounce::render(&project, &dir, &job, rate, true, &|_| {})?;
            let file = std::env::temp_dir().join(format!("{}.wav", job.name));
            pina_engine::analysis::write_wav(&file, &l, &r, rate)?;
            Ok::<_, String>(file)
        });
        cx.spawn(async move |this, cx| {
            let made = work.await;
            this.update(cx, |ws, cx| match made {
                Ok(file) => {
                    let dir = ws.session.dir.clone();
                    ws.session.checkpoint();
                    let at = t + 1;
                    let p = &mut ws.session.project;
                    let color = p.tracks[t].color.clone();
                    let added = commands::apply(
                        p,
                        &dir,
                        Command::AddTrack {
                            name: name.clone(),
                            instrument: None,
                            index: Some(at),
                        },
                    )
                    .and_then(|_| {
                        commands::apply(
                            p,
                            &dir,
                            Command::AddAudioRegion {
                                track: at,
                                file: file.clone(),
                                start: from,
                                offset: 0,
                                length: None,
                            },
                        )
                    });
                    let _ = std::fs::remove_file(&file);
                    match added {
                        Ok(_) => {
                            p.tracks[at].color = color;
                            p.tracks[t].mute = true;
                            ws.select_track(at);
                            ws.changed(cx);
                            ws.status =
                                Some(format!("{name}: the original muted (⌘Z puts it back)"));
                        }
                        Err(e) => {
                            ws.session.cancel();
                            ws.status = Some(e);
                            cx.notify();
                        }
                    }
                }
                Err(e) => {
                    ws.status = Some(format!("Couldn't bounce it: {e}"));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Opens the sheet on stems, with one track ticked.
    pub fn export_stem(&mut self, t: usize, cx: &mut Context<Self>) {
        self.open_export(cx);
        self.preset(Preset::Stems, cx);
        if let Some(e) = self.export.as_mut() {
            e.tracks = (0..e.tracks.len()).map(|i| i == t).collect();
        }
        cx.notify();
    }

    /// The sheet, if it's open.
    pub fn export_view(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        self.export.as_ref()?;
        let jobs = self.export_jobs(cx);
        let p = &self.session.project;
        let e = self.export.as_ref().unwrap();
        let song = self
            .session
            .dir
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());
        let range = self.export_range();
        let (from, to) = range.unwrap_or((0, p.end()));
        let seconds = p.samples(to.saturating_sub(from), 1.0) as f64;
        let bar = p.bar().max(1);
        let bars_label = format!(
            "bars {}–{} · {}",
            from / bar + 1,
            to.div_ceil(bar).max(from / bar + 1),
            clock(seconds)
        );
        let audio = e.formats.iter().filter(|f| **f != Format::Midi).count();
        let files = jobs.len() * e.formats.len();
        let bytes: f64 = jobs.len() as f64
            * e.formats
                .iter()
                .map(|f| {
                    f.bytes_a_second(e.bits, e.rate) * (seconds + if e.tail { 2.0 } else { 0.0 })
                })
                .sum::<f64>();
        let size = match bytes {
            b if b >= 1e9 => format!("{:.1} GB", b / 1e9),
            b if b >= 1e6 => format!("{:.0} MB", b / 1e6),
            b => format!("{:.0} KB", (b / 1e3).max(1.0)),
        };

        let seg = |label: String, on: bool| {
            div()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(rgb(RAISED))
                        .border_1()
                        .border_color(rgb(LINE_STRONG))
                        .text_color(rgb(TEXT))
                })
                .when(!on, |d| {
                    d.text_color(rgb(TEXT_3))
                        .hover(|d| d.text_color(rgb(TEXT_2)))
                })
                .child(label)
        };
        let segs = |items: Vec<Div>| {
            div()
                .flex()
                .p(px(2.))
                .gap(px(2.))
                .rounded(px(9.))
                .bg(rgb(WELL))
                .border_1()
                .border_color(rgb(LINE))
                .children(items)
        };
        let label = |s: &'static str| {
            div()
                .w(px(70.))
                .flex_none()
                .text_size(px(10.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_3))
                .child(s)
        };
        let row = |name: &'static str, body: Div| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(label(name))
                .child(body)
        };
        let check = |on: bool, color: u32| {
            div()
                .size(px(16.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(if on { rgb(color) } else { rgb(LINE_STRONG) })
                .bg(if on {
                    alpha(color, 0.9)
                } else {
                    rgb(WELL).into()
                })
                .when(on, |d| d.child(icon(Icon::Check, 11.).text_color(rgb(BG))))
        };

        // Presets.
        let presets = Preset::ALL.map(|pr| {
            div()
                .flex_1()
                .min_w(px(0.))
                .px(px(10.))
                .py(px(7.))
                .rounded(px(9.))
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(0x1e1e24))
                .cursor_pointer()
                .hover(|d| d.border_color(alpha(ACCENT, 0.6)))
                .debug_selector(move || format!("bounce-preset-{}", pr.name()))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(pr.name()),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_3))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(pr.about()),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.preset(pr, cx)),
                )
        });

        // What, over which range, and the list.
        let what = [
            (What::Mix, "Mix"),
            (What::Stems, "Stems"),
            (What::Sections, "Sections"),
        ]
        .map(|(w, l)| {
            seg(l.into(), e.what == w)
                .debug_selector(move || format!("bounce-what-{l}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if let Some(e) = ws.export.as_mut() {
                            e.what = w;
                        }
                        cx.notify();
                    }),
                )
        });
        let has_cycle = p.cycle.is_some();
        let spans = [
            (Span::Song, "Song"),
            (Span::Cycle, "Cycle"),
            (Span::Bars, "Bars"),
        ]
        .map(|(s, l)| {
            seg(l.into(), e.span == s)
                .when(s == Span::Cycle && !has_cycle, |d| d.opacity(0.4))
                .debug_selector(move || format!("bounce-span-{l}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if s == Span::Cycle && ws.session.project.cycle.is_none() {
                            ws.status = Some("There's no cycle: sweep one in the ruler.".into());
                        } else if let Some(e) = ws.export.as_mut() {
                            e.span = s;
                        }
                        cx.notify();
                    }),
                )
        });
        let stepper = |which: usize, value: u64, cx: &mut Context<Self>| {
            let step = move |by: i64, cx: &mut Context<Self>| {
                div()
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(RAISED)))
                    .child(
                        icon(if by < 0 { Icon::Minus } else { Icon::Plus }, 11.)
                            .text_color(rgb(TEXT_2)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            let last = ws
                                .session
                                .project
                                .end()
                                .div_ceil(ws.session.project.bar().max(1))
                                .max(1)
                                + 16;
                            if let Some(e) = ws.export.as_mut() {
                                let v = if which == 0 {
                                    &mut e.bars.0
                                } else {
                                    &mut e.bars.1
                                };
                                *v = (*v as i64 + by).clamp(1, last as i64) as u64;
                                if e.bars.1 < e.bars.0 {
                                    e.bars.1 = e.bars.0;
                                }
                            }
                            cx.notify();
                        }),
                    )
            };
            div()
                .flex()
                .items_center()
                .rounded(px(8.))
                .bg(rgb(WELL))
                .border_1()
                .border_color(rgb(LINE))
                .child(step(-1, cx))
                .child(
                    div()
                        .w(px(30.))
                        .flex()
                        .justify_center()
                        .font_family(MONO)
                        .text_size(px(12.))
                        .child(value.to_string()),
                )
                .child(step(1, cx))
        };
        let list: Vec<Div> = match e.what {
            What::Mix => vec![div().p(px(12.)).rounded(px(10.)).bg(rgb(WELL)).text_size(px(12.)).text_color(rgb(TEXT_2)).child(format!("Every track, through the master: one file of the song as you hear it ({} tracks).", p.tracks.len()))],
            What::Stems => p
                .tracks
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let on = e.tracks.get(i).copied().unwrap_or(false);
                    let color = self.color(i);
                    let empty = t.regions.is_empty();
                    div()
                        .h(px(30.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .rounded(px(7.))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(OFF)))
                        .debug_selector(move || format!("bounce-track-{i}"))
                        .child(check(on, ACCENT))
                        .child(div().size(px(8.)).rounded(px(2.)).bg(rgb(color)))
                        .child(div().flex_1().text_size(px(12.5)).text_color(rgb(if empty { TEXT_4 } else { TEXT })).child(t.name.clone()))
                        .child(div().text_size(px(10.5)).text_color(rgb(TEXT_4)).child(if empty { "empty" } else if t.mute { "muted" } else { "" }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                if let Some(v) = ws.export.as_mut().and_then(|e| e.tracks.get_mut(i)) {
                                    *v ^= true;
                                }
                                cx.notify();
                            }),
                        )
                })
                .collect(),
            What::Sections => {
                let found = bounce::sections(p);
                if found.is_empty() {
                    vec![div().p(px(12.)).rounded(px(10.)).bg(rgb(WELL)).text_size(px(12.)).text_color(rgb(TEXT_3)).child("No sections: add markers (double-click over the ruler) to bounce each part on its own.")]
                } else {
                    found
                        .into_iter()
                        .enumerate()
                        .map(|(k, (name, a, b))| {
                            let on = e.sections.get(k).copied().unwrap_or(false);
                            div()
                                .h(px(30.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .gap(px(10.))
                                .rounded(px(7.))
                                .cursor_pointer()
                                .hover(|d| d.bg(rgb(OFF)))
                                .debug_selector(move || format!("bounce-section-{k}"))
                                .child(check(on, ACCENT))
                                .child(div().flex_1().text_size(px(12.5)).child(name))
                                .child(div().font_family(MONO).text_size(px(10.5)).text_color(rgb(TEXT_3)).child(format!("bars {}–{}", a / bar + 1, b.div_ceil(bar))))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        if let Some(v) = ws.export.as_mut().and_then(|e| e.sections.get_mut(k)) {
                                            *v ^= true;
                                        }
                                        cx.notify();
                                    }),
                                )
                        })
                        .collect()
                }
            }
        };
        let left = div()
            .w(px(380.))
            .flex_none()
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(row("WHAT", segs(Vec::from(what))))
            .child(row("RANGE", segs(Vec::from(spans))))
            .when(e.span == Span::Bars, |d| {
                d.child(row(
                    "BARS",
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(stepper(0, e.bars.0, cx))
                        .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child("to"))
                        .child(stepper(1, e.bars.1, cx)),
                ))
            })
            .child(row(
                "",
                div()
                    .text_size(px(11.))
                    .font_family(MONO)
                    .text_color(rgb(TEXT_3))
                    .child(bars_label),
            ))
            .child(
                div()
                    .id("bounce-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(list),
            );

        // How.
        let formats = FORMATS.map(|f| {
            let on = e
                .formats
                .iter()
                .any(|o| std::mem::discriminant(o) == std::mem::discriminant(&f));
            div()
                .h(px(30.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(if on {
                    alpha(ACCENT, 0.7)
                } else {
                    rgb(LINE).into()
                })
                .bg(if on {
                    alpha(ACCENT, 0.12)
                } else {
                    rgb(WELL).into()
                })
                .cursor_pointer()
                .debug_selector(move || format!("bounce-format-{}", f.name()))
                .child(check(on, ACCENT))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(f.name()),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.toggle_format(f, cx)),
                )
        });
        let bits = [Bits::I16, Bits::I24, Bits::F32].map(|b| {
            seg(
                match b {
                    Bits::I16 => "16".into(),
                    Bits::I24 => "24".into(),
                    Bits::F32 => "32 float".into(),
                },
                e.bits == b,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    if let Some(e) = ws.export.as_mut() {
                        e.bits = b;
                    }
                    cx.notify();
                }),
            )
        });
        let rates = [44_100u32, 48_000, 88_200, 96_000].map(|r| {
            seg(format!("{}", r as f32 / 1000.0), e.rate == r)
                .debug_selector(move || format!("bounce-rate-{r}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if let Some(e) = ws.export.as_mut() {
                            e.rate = r;
                        }
                        cx.notify();
                    }),
                )
        });
        let mp3 = e.formats.iter().find_map(|f| match f {
            Format::Mp3 { kbps } => Some(*kbps),
            _ => None,
        });
        let aac = e.formats.iter().find_map(|f| match f {
            Format::Aac { kbps } => Some(*kbps),
            _ => None,
        });
        let kbps_segs =
            |now: u32, options: &[u32], aac: bool, cx: &mut Context<Self>| -> Vec<Div> {
                options
                    .iter()
                    .map(|&k| {
                        seg(k.to_string(), now == k).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                if let Some(e) = ws.export.as_mut() {
                                    for f in e.formats.iter_mut() {
                                        match f {
                                            Format::Mp3 { kbps } if !aac => *kbps = k,
                                            Format::Aac { kbps } if aac => *kbps = k,
                                            _ => {}
                                        }
                                    }
                                }
                                cx.notify();
                            }),
                        )
                    })
                    .collect()
            };
        let level_kind = match e.level {
            Level::Off => 0,
            Level::Peak { .. } => 1,
            Level::Loudness { .. } => 2,
        };
        let levels = [(0, "As it plays"), (1, "Peak −1 dB"), (2, "Loudness")].map(|(k, l)| {
            seg(l.into(), level_kind == k)
                .debug_selector(move || format!("bounce-level-{k}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if let Some(e) = ws.export.as_mut() {
                            e.level = match k {
                                0 => Level::Off,
                                1 => Level::Peak { db: -1.0 },
                                _ => Level::Loudness {
                                    lufs: -14.0,
                                    ceiling: -1.0,
                                },
                            };
                        }
                        cx.notify();
                    }),
                )
        });
        let targets = match e.level {
            Level::Loudness { lufs, .. } => Some(TARGETS.map(|(t, l)| {
                seg(format!("{t:.0} {l}"), (lufs - t).abs() < 0.1).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if let Some(e) = ws.export.as_mut() {
                            e.level = Level::Loudness {
                                lufs: t,
                                ceiling: -1.0,
                            };
                        }
                        cx.notify();
                    }),
                )
            })),
            _ => None,
        };
        let switch = |label: &'static str,
                      on: bool,
                      sel: &'static str,
                      f: fn(&mut Export),
                      cx: &mut Context<Self>| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .debug_selector(move || sel.into())
                .child(check(on, ACCENT))
                .child(div().text_size(px(12.)).child(label))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| {
                        if let Some(e) = ws.export.as_mut() {
                            f(e);
                        }
                        cx.notify();
                    }),
                )
        };
        let right = div()
            .flex_1()
            .min_w(px(0.))
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label("FORMATS"))
                    .child(div().flex().flex_wrap().gap(px(6.)).children(formats)),
            )
            .when(audio > 0, |d| {
                d.child(row("BITS", segs(Vec::from(bits))))
                    .child(row("RATE kHz", segs(Vec::from(rates))))
                    .when_some(mp3, |d, k| {
                        d.child(row(
                            "MP3 kbps",
                            segs(kbps_segs(k, &[128, 192, 256, 320], false, cx)),
                        ))
                    })
                    .when_some(aac, |d, k| {
                        d.child(row(
                            "AAC kbps",
                            segs(kbps_segs(k, &[128, 192, 256], true, cx)),
                        ))
                    })
                    .child(row("LEVEL", segs(Vec::from(levels))))
                    .when_some(targets, |d, t| d.child(row("TARGET", segs(Vec::from(t)))))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(18.))
                            .pl(px(80.))
                            .child(switch(
                                "Tail (reverb, delay)",
                                e.tail,
                                "bounce-tail",
                                |e| e.tail ^= true,
                                cx,
                            ))
                            .child(switch(
                                "Dither",
                                e.dither,
                                "bounce-dither",
                                |e| e.dither ^= true,
                                cx,
                            ))
                            .when(e.what == What::Stems, |d| {
                                d.child(switch(
                                    "Stems through the master",
                                    e.master_on_stems,
                                    "bounce-master",
                                    |e| e.master_on_stems ^= true,
                                    cx,
                                ))
                            }),
                    )
            });

        // Running, or done: each file.
        let target = match e.level {
            Level::Loudness { lufs, ceiling } => Some((lufs, ceiling)),
            _ => None,
        };
        let run_view = e.run.as_ref().map(|r| {
            let progress = r.progress.lock().map(|v| v.clone()).unwrap_or_default();
            let results: Vec<Option<Result<Done, String>>> =
                r.results.lock().map(|v| v.clone()).unwrap_or_default();
            div()
                .id("bounce-run")
                .flex_1()
                .min_h(px(0.))
                .p(px(18.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .overflow_y_scroll()
                .children(r.names.iter().enumerate().map(|(k, name)| {
                    let pr = progress.get(k).copied().unwrap_or(0.0);
                    let res = results.get(k).cloned().flatten();
                    let tail = match &res {
                        Some(Ok(d)) => {
                            let a = &d.analysis;
                            let shown = d.files.first().cloned();
                            let wav = d
                                .files
                                .iter()
                                .find(|f| {
                                    f.extension()
                                        .is_some_and(|x| x == "wav" || x == "aif" || x == "flac")
                                })
                                .cloned();
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .font_family(MONO)
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_2))
                                        .child(match d.seconds > 0.0 {
                                            true => format!(
                                                "{:.1} LUFS · peak {:.1} dB · {}",
                                                a.lufs,
                                                a.peak_db[0].max(a.peak_db[1]),
                                                clock(d.seconds)
                                            ),
                                            false => "MIDI".into(),
                                        }),
                                )
                                .when_some(short_of(target, a.lufs), |d, note| {
                                    d.child(
                                        div().text_size(px(11.)).text_color(rgb(AMBER)).child(note),
                                    )
                                })
                                .child(div().flex().gap(px(4.)).children(d.files.iter().map(|f| {
                                    div()
                                        .px(px(6.))
                                        .py(px(1.))
                                        .rounded(px(4.))
                                        .bg(rgb(RAISED))
                                        .text_size(px(10.))
                                        .text_color(rgb(TEXT_2))
                                        .child(f.extension().map_or(String::new(), |x| {
                                            x.to_string_lossy().to_uppercase()
                                        }))
                                })))
                                .child(div().flex_1())
                                .when_some(wav, |d, f| {
                                    d.child(button("Play", false).on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            if let Err(e) = ws.player.audition(&f, 0.0, 0.0) {
                                                ws.status = Some(e);
                                            }
                                            cx.notify();
                                        }),
                                    ))
                                })
                                .when_some(shown, |d, f| {
                                    d.child(button("Show", false).on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_, _, _, _| {
                                            let _ = std::process::Command::new("open")
                                                .arg("-R")
                                                .arg(&f)
                                                .spawn();
                                        }),
                                    ))
                                })
                        }
                        Some(Err(err)) => div()
                            .text_size(px(11.5))
                            .text_color(rgb(METER_HIGH))
                            .child(err.clone()),
                        None => div()
                            .h(px(6.))
                            .w_full()
                            .rounded(px(3.))
                            .bg(rgb(WELL))
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(pr.clamp(0.0, 1.0)))
                                    .rounded(px(3.))
                                    .bg(rgb(ACCENT)),
                            ),
                    };
                    div()
                        .p(px(12.))
                        .rounded(px(10.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .debug_selector(move || format!("bounce-job-{k}"))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(name.clone()),
                        )
                        .child(tail)
                }))
        });
        let running = e.run.as_ref().is_some_and(|r| !r.finished);
        let finished = e.run.as_ref().is_some_and(|r| r.finished);
        let folder = e.folder.clone();
        let shown_folder = folder
            .strip_prefix(&self.session.dir)
            .map_or(folder.display().to_string(), |f| f.display().to_string());
        let sheet = div()
            .id("bounce")
            .w(px(900.))
            .h(px(620.))
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
            .debug_selector(|| "bounce".into())
            .on_mouse_down(MouseButton::Left, |_, _, _| {})
            .child(
                div()
                    .h(px(52.))
                    .px(px(18.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(icon(Icon::Export, 16.).text_color(rgb(TEXT_2)))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Bounce"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(TEXT_3))
                            .child(format!("· {song}")),
                    )
                    .child(div().flex_1())
                    .child(
                        icon_seg(Icon::Close, false, 28.)
                            .debug_selector(|| "bounce-close".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.close_export(cx)),
                            ),
                    ),
            )
            .when(e.run.is_none(), |d| {
                d.child(
                    div()
                        .px(px(18.))
                        .pt(px(14.))
                        .pb(px(4.))
                        .flex()
                        .gap(px(8.))
                        .children(presets),
                )
                .child(div().flex_1().min_h(px(0.)).flex().child(left).child(right))
            })
            .children(run_view)
            .child(
                div()
                    .h(px(64.))
                    .px(px(18.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(0x141418))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(10.))
                            .h(px(32.))
                            .rounded(px(8.))
                            .bg(rgb(WELL))
                            .border_1()
                            .border_color(rgb(LINE))
                            .cursor_pointer()
                            .hover(|d| d.border_color(rgb(LINE_STRONG)))
                            .debug_selector(|| "bounce-folder".into())
                            .child(icon(Icon::Folder, 13.).text_color(rgb(TEXT_3)))
                            .child(
                                div()
                                    .max_w(px(160.))
                                    .text_size(px(12.))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(shown_folder),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.pick_export_folder(cx)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(200.))
                            .h(px(32.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .bg(rgb(FIELD))
                            .border_1()
                            .border_color(rgb(LINE_STRONG))
                            .text_size(px(12.5))
                            .debug_selector(|| "bounce-name".into())
                            .child(e.name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.5))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .child(format!(
                                "{files} file{} · {size} · {}",
                                if files == 1 { "" } else { "s" },
                                clock(seconds)
                            )),
                    )
                    .when(finished, |d| {
                        d.child(button("Show Folder", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, _, _, _| {
                                let _ = std::process::Command::new("open").arg(&folder).spawn();
                            }),
                        ))
                    })
                    .child(
                        button(
                            if running {
                                "Stop"
                            } else if finished {
                                "Done"
                            } else {
                                "Cancel"
                            },
                            false,
                        )
                        .debug_selector(|| "bounce-cancel".into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.close_export(cx)),
                        ),
                    )
                    .when(e.run.is_none(), |d| {
                        d.child(
                            div()
                                .h(px(34.))
                                .px(px(18.))
                                .flex()
                                .items_center()
                                .rounded(px(9.))
                                .bg(rgb(TEXT))
                                .text_color(rgb(BG))
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .hover(|d| d.bg(rgb(0xffffff)))
                                .debug_selector(|| "bounce-go".into())
                                .child(format!(
                                    "Bounce {} file{}",
                                    files,
                                    if files == 1 { "" } else { "s" }
                                ))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.start_export(cx)),
                                ),
                        )
                    }),
            );
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(alpha(0x000000, 0.55))
                    .child(sheet),
            )
            .with_priority(3),
        )
    }
}
