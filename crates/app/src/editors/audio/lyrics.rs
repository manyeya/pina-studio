//! An audio region's lyrics, beside it in the Audio editor: the lines sung in its file, as a
//! Whisper model the person downloaded hears them (`pina_engine::lyrics`), each when it's sung
//! and how often (a hook): to hear, or to make a loop on the bars on a track of lyric loops.
//! What's heard is kept beside the song's audio, so a file's lyrics are heard once.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};

use gpui::{
    AnimationExt, AppContext, Context, Div, FontWeight, MouseButton, canvas, div, prelude::*, px,
    rgb,
};
use pina_engine::{
    audio::engine::{AudioData, resample},
    lyrics::{self, Line},
    song::project::{At, Content},
};

use crate::{sounds::browser::primary, ui::theme::*, workspace::Workspace};

/// A file's lyrics, as heard.
pub struct Lyrics {
    /// The region's file, and the one heard: its vocals stem, if the vocals were separated
    /// first (loops play that), else the same.
    pub file: PathBuf,
    pub heard: PathBuf,
    pub model: &'static str,
    pub lines: Vec<Line>,
    /// The heard file's audio (its channels and rate), to hear a line at once.
    pub(crate) audio: Arc<(Vec<Vec<f32>>, f64)>,
}

/// Lyrics asked for and not shown yet: for the file `file` (the region at `at`'s).
pub struct LyricsWait {
    pub file: PathBuf,
    pub at: At,
    /// Being heard, by this Whisper model: how far taking the vocals out has got, in
    /// thousandths (past 1000, hearing the words). None: waiting for Whisper turbo, or failed.
    pub hearing: Option<(&'static str, Arc<AtomicU32>)>,
    pub failed: Option<String>,
}

#[cfg(test)]
impl Lyrics {
    /// Lyrics as if heard, to show (nothing to hear a line from).
    pub fn shown(file: PathBuf, model: &'static str, lines: Vec<Line>) -> Lyrics {
        Lyrics {
            heard: file.clone(),
            file,
            model,
            lines,
            audio: Arc::new((Vec::new(), 44_100.0)),
        }
    }
}

/// Seconds as m:ss.
pub(crate) fn clock(s: f32) -> String {
    let s = s.max(0.0);
    // Under a minute (a sample, a loop): tenths, so lines a second apart don't read the same.
    if s < 60.0 {
        return format!("{s:.1}s");
    }
    let s = s as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

impl Workspace {
    /// The lines sung in the audio region at `at`'s file, in the background: a song's vocals
    /// separated first (`lyrics::hear_file`), how far it's got in the panel; read back if they
    /// were heard before. Without a Whisper model, the panel offers turbo to download (and hears
    /// it once it's down).
    pub(crate) fn hear_lyrics(&mut self, at: At, cx: &mut Context<Self>) {
        if self
            .lyrics_wait
            .as_ref()
            .is_some_and(|w| w.hearing.is_some())
        {
            return;
        }
        let Content::Audio { file, .. } = &self.session.project.region(at).content else {
            return;
        };
        let file = file.clone();
        self.lyrics = None;
        let Some(model) = lyrics::installed(&lyrics::folder()).first().copied() else {
            self.lyrics_wait = Some(LyricsWait {
                file,
                at,
                hearing: None,
                failed: None,
            });
            self.status = Some("Lyrics needs Whisper: download it in the panel".into());
            return cx.notify();
        };
        // Known to be a voice alone (Smart Slice heard it): heard as it is, not separated again.
        let alone = self
            .beat
            .as_ref()
            .is_some_and(|b| b.file == file && b.heard == pina_engine::smart::InIt::Voice);
        let dir = self.session.dir.clone();
        let done = Arc::<AtomicU32>::default();
        self.lyrics_wait = Some(LyricsWait {
            file: file.clone(),
            at,
            hearing: Some((model.name, done.clone())),
            failed: None,
        });
        self.status = Some(format!("Hearing the lyrics with {}…", model.name));
        cx.notify();
        let job = cx.background_spawn({
            let (file, done) = (file.clone(), done.clone());
            async move {
                let progress = |p: f32| done.store(1 + (p * 1000.0) as u32, Ordering::Relaxed);
                let heard =
                    lyrics::hear_file(&dir, &file, model, alone, progress, &AtomicBool::new(false));
                done.store(1001, Ordering::Relaxed);
                heard
            }
        });
        // The panel redrawn as it goes.
        cx.spawn({
            let done = done.clone();
            async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(300))
                        .await;
                    let going = this.update(cx, |ws, cx| {
                        let ours = ws.lyrics_wait.as_ref().is_some_and(|w| {
                            w.hearing.as_ref().is_some_and(|h| Arc::ptr_eq(&h.1, &done))
                        });
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
        cx.spawn(async move |this, cx| {
            let heard = job.await;
            let _ = this.update(cx, |ws, cx| {
                match heard {
                    Ok(h) => {
                        ws.lyrics_wait = None;
                        let hooks = h.lines.iter().filter(|l| l.sung > 1).count();
                        ws.status = Some(match h.lines.len() {
                            0 => format!("{} heard no words in it", model.name),
                            n => format!(
                                "{n} lines heard, {hooks} sung more than once: loop any, loop the \
                                 hooks, or drag a line onto a track"
                            ),
                        });
                        // A voice alone sliced (Smart Slice): its chops named by these words.
                        if let Some(b) = ws.beat.as_mut().filter(|b| {
                            b.file == file
                                && b.cut == h.file
                                && b.heard == pina_engine::smart::InIt::Voice
                        }) {
                            let words: Vec<lyrics::Word> = h
                                .lines
                                .iter()
                                .flat_map(|l| match l.words.is_empty() {
                                    true => vec![lyrics::Word {
                                        text: l.text.clone(),
                                        start: l.start,
                                        end: l.end,
                                    }],
                                    false => l.words.clone(),
                                })
                                .collect();
                            pina_engine::smart::name_by_words(&mut b.slices, &words);
                        }
                        ws.lyrics = Some(Lyrics {
                            file,
                            heard: h.file,
                            model: model.name,
                            lines: h.lines,
                            audio: Arc::new(h.audio),
                        });
                    }
                    Err(e) => {
                        ws.status = Some(e.clone());
                        ws.lyrics_wait = Some(LyricsWait {
                            file,
                            at,
                            hearing: None,
                            failed: Some(e),
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Whisper turbo downloaded for the lyrics waiting for it, then heard.
    fn get_whisper_for_lyrics(&mut self, cx: &mut Context<Self>) {
        self.download_whisper(0, cx);
    }

    /// Every line sung more than once (each once) as a loop, one after another on the region's
    /// lyrics track from the playhead's bar: one undo.
    fn loop_hooks(&mut self, at: At, cx: &mut Context<Self>) {
        let Some(l) = &self.lyrics else {
            return;
        };
        let key = lyrics::flips::line_key;
        let mut seen = std::collections::HashSet::new();
        let hooks: Vec<Line> = l
            .lines
            .iter()
            .filter(|x| x.sung > 1 && seen.insert(key(&x.text)))
            .cloned()
            .collect();
        let stem = self.lyrics_stem();
        let bar = self.session.project.bar().max(1);
        let here = self.playhead() / bar * bar;
        let mut made = 0;
        let mut lane = 0;
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            lane = p.lyrics_track(at.0);
            for line in &hooks {
                let Some(bars) = p.lyric_bars(at, line.start, line.end) else {
                    continue;
                };
                let after = p.tracks[lane]
                    .regions
                    .iter()
                    .map(|r| (r.start + r.length).div_ceil(bar) * bar)
                    .max()
                    .unwrap_or(0);
                p.lyric_loop(at, bars, &line.text, lane, here.max(after), stem.as_deref());
                made += 1;
            }
        });
        let track = self.session.project.tracks[lane].name.clone();
        self.status = Some(format!(
            "{made} hooks as loops on {track}, one after another"
        ));
        cx.notify();
    }

    /// The file loops of the lyrics play, if not their region's own: its vocals stem.
    fn lyrics_stem(&self) -> Option<PathBuf> {
        self.lyrics
            .as_ref()
            .filter(|l| l.heard != l.file)
            .map(|l| l.heard.clone())
    }

    /// The `i`th line heard, now (a moment past its end, its last syllable whole).
    fn hear_line(&mut self, i: usize) {
        let Some(l) = &self.lyrics else {
            return;
        };
        let Some(line) = l.lines.get(i) else {
            return;
        };
        let (sides, rate) = &*l.audio;
        let a = (line.start as f64 * rate) as usize;
        let b = ((line.end as f64 + 0.15) * rate) as usize;
        let to = self.player.rate();
        let channels: Vec<Vec<f32>> = sides
            .iter()
            .map(|s| resample(&s[a.min(s.len())..b.min(s.len())], *rate, to))
            .collect();
        if channels.iter().all(Vec::is_empty) {
            return;
        }
        let length = Duration::from_secs_f32(line.end - line.start + 0.15);
        let _ = self
            .player
            .audition_audio(Arc::new(AudioData::new(channels)), 0.0, 0.0);
        self.lyric_playing = Some((i, std::time::Instant::now() + length));
    }

    /// The `i`th line as a loop on the bars (`Project::lyric_bars`): on `to` (a track and a
    /// tick) if it was dragged there, else on its track's lyrics track, at the playhead's bar or
    /// after the loops already there.
    fn loop_line(&mut self, at: At, i: usize, to: Option<(usize, u64)>, cx: &mut Context<Self>) {
        let Some(line) = self.lyrics.as_ref().and_then(|l| l.lines.get(i)).cloned() else {
            return;
        };
        let p = &self.session.project;
        let Some(bars) = p.lyric_bars(at, line.start, line.end) else {
            self.status = Some("The region doesn't play this line".into());
            return cx.notify();
        };
        let bar = p.bar().max(1);
        let here = self.playhead() / bar * bar;
        let stem = self.lyrics_stem();
        let mut made = None;
        self.edit(cx, |ws| {
            let p = &mut ws.session.project;
            if let Some((track, tick)) = to {
                let stem = stem.as_deref();
                made = Some(p.lyric_loop(at, bars, &line.text, track, tick / bar * bar, stem));
                return;
            }
            let track = p.lyrics_track(at.0);
            let after = p.tracks[track]
                .regions
                .iter()
                .map(|r| (r.start + r.length).div_ceil(bar) * bar)
                .max()
                .unwrap_or(0);
            made = Some(p.lyric_loop(
                at,
                bars,
                &line.text,
                track,
                here.max(after),
                stem.as_deref(),
            ));
        });
        if let Some(m) = made {
            let track = self.session.project.tracks[m.0].name.clone();
            self.status = Some(format!(
                "“{}”: a {}-bar loop on {track}",
                line.text,
                bars.1 / bar
            ));
            cx.notify();
        }
    }

    /// The lyrics of `file` (the region at `at`'s), as a column beside the waveform: waiting
    /// for Whisper (to download it), being heard (how far), heard nothing (why, and what next),
    /// or each line sung, to hear, loop or drag, the hooks to loop at once.
    pub(crate) fn lyrics_panel(
        &mut self,
        at: At,
        file: &std::path::Path,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        // Sliced as a beat (Smart Slice): its slices and Flips, or how the slicing's going.
        if self.beat.as_ref().is_some_and(|b| b.file == file)
            || self.beat_wait.as_ref().is_some_and(|w| w.file == file)
        {
            let sliced = self.beat.as_ref().filter(|b| b.file == file);
            let words = sliced.is_some_and(|b| b.heard != pina_engine::smart::InIt::Beat);
            if !words && self.flips.tab == crate::editors::audio::LyricsTab::Words {
                self.flips.tab = crate::editors::audio::LyricsTab::Lines;
            }
            let tabs = sliced
                .is_some_and(|b| !b.slices.is_empty())
                .then(|| self.lyrics_tabs("Slices", words, cx))
                .map(|t| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(t)
                        .children(self.heard_as_picker(cx))
                });
            let (title, body) = match self.beat.as_ref().filter(|b| b.file == file) {
                Some(b) if b.slices.is_empty() => (
                    "SMART SLICE".to_string(),
                    vec![note(
                        "No slices",
                        "It's silent where the region plays, or has no hits to cut at.",
                    )],
                ),
                Some(_) => {
                    let body = match self.flips.tab {
                        crate::editors::audio::LyricsTab::Flips => self.flips_page(at, cx),
                        crate::editors::audio::LyricsTab::Lines => self.slices_page(at, cx),
                        crate::editors::audio::LyricsTab::Words => self.words_page(at, file, cx),
                    };
                    (String::new(), body)
                }
                None => ("SMART SLICE".to_string(), self.beat_waiting(at, cx)),
            };
            return Some(self.smart_panel(tabs, title, body, cx));
        }
        // Lines heard: the tabs, Lines and Flips, in the header, beside who heard them.
        let tabs = self
            .lyrics
            .as_ref()
            .is_some_and(|l| l.file == file && !l.lines.is_empty())
            .then(|| self.lyrics_tabs("Lines", false, cx));
        let heard = self.lyrics.as_ref().filter(|l| l.file == file);
        let wait = self.lyrics_wait.as_ref().filter(|w| w.file == file);
        let title = match (heard, wait) {
            (Some(l), _) if tabs.is_some() => l.model.to_string(),
            (Some(l), _) => format!("LYRICS · {}", l.model),
            (
                None,
                Some(LyricsWait {
                    hearing: Some((m, _)),
                    ..
                }),
            ) => format!("LYRICS · {m}"),
            (None, Some(_)) => "LYRICS".to_string(),
            (None, None) => return None,
        };
        let body: Vec<Div> = match (heard, wait) {
            (Some(l), _) if l.lines.is_empty() => {
                let model = l.model;
                self.lyrics_nothing(at, model, cx)
            }
            (Some(_), _) => match self.flips.tab {
                crate::editors::audio::LyricsTab::Flips => self.flips_page(at, cx),
                _ => self.lyrics_lines(at, cx),
            },
            (None, Some(w)) => {
                let (hearing, failed) = (w.hearing.clone(), w.failed.clone());
                self.lyrics_waiting(at, hearing, failed, cx)
            }
            (None, None) => Vec::new(),
        };
        Some(self.smart_panel(tabs, title, body, cx))
    }

    /// The panel beside the waveform: its tabs and title, Close, and what it shows.
    fn smart_panel(
        &mut self,
        tabs: Option<Div>,
        title: String,
        body: Vec<Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        {
            div()
                .id("lyrics")
                .w(px(420.))
                .flex_none()
                .flex()
                .flex_col()
                .rounded(px(10.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .overflow_y_scroll()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(10.))
                        .py(px(8.))
                        .children(tabs)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT_2))
                                .child(title),
                        )
                        .child(
                            button("Close", false)
                                .h(px(22.))
                                .debug_selector(|| "lyrics-close".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| {
                                        ws.lyrics = None;
                                        ws.lyrics_wait = None;
                                        ws.beat = None;
                                        if let Some(w) = ws.beat_wait.take() {
                                            w.stop();
                                        }
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
                .children(body)
        }
    }

    /// The Words tab beside a recording's slices: its lines, how they're being heard, or (not
    /// yet) Hear the words.
    fn words_page(&mut self, at: At, file: &std::path::Path, cx: &mut Context<Self>) -> Vec<Div> {
        if self.lyrics.as_ref().is_some_and(|l| l.file == file) {
            let empty = self.lyrics.as_ref().is_some_and(|l| l.lines.is_empty());
            return match empty {
                true => {
                    let model = self.lyrics.as_ref().map_or("Whisper", |l| l.model);
                    self.lyrics_nothing(at, model, cx)
                }
                false => self.lyrics_lines(at, cx),
            };
        }
        if let Some(w) = self.lyrics_wait.as_ref().filter(|w| w.file == file) {
            let (hearing, failed) = (w.hearing.clone(), w.failed.clone());
            return self.lyrics_waiting(at, hearing, failed, cx);
        }
        vec![
            note(
                "The words sung in it",
                "Each line and word, heard by Whisper: loop a line on the bars, chop it at its \
                 words, or flip them.",
            ),
            pad().child(
                primary("Hear the words")
                    .h(px(28.))
                    .debug_selector(|| "words-hear".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.hear_lyrics(at, cx)),
                    ),
            ),
        ]
    }

    /// Waiting: Whisper to download (offered), the vocals being taken out, the words heard; or
    /// what went wrong.
    fn lyrics_waiting(
        &mut self,
        at: At,
        hearing: Option<(&'static str, Arc<AtomicU32>)>,
        failed: Option<String>,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        if let Some(e) = failed {
            return vec![
                note("Couldn't hear it", e),
                pad().child(button("Try again", false).h(px(28.)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.hear_lyrics(at, cx)),
                )),
            ];
        }
        match hearing {
            Some((model, done)) => {
                let at = done.load(Ordering::Relaxed);
                match at {
                    0 => vec![note("Reading it…", String::new())],
                    1..=1000 => vec![
                        note(
                            "Taking the vocals out first",
                            format!(
                                "So {model} hears the singing, not the beat. A few minutes for a \
                                 whole song, once: it's kept."
                            ),
                        ),
                        progress((at - 1) as f32 / 1000.0),
                    ],
                    _ => vec![
                        note(
                            "Hearing the words",
                            format!("{model} is listening: a minute or two for a whole song."),
                        ),
                        progress(-1.0),
                    ],
                }
            }
            None => {
                let turbo = &lyrics::WHISPERS[0];
                let going = self
                    .models_ui
                    .downloads
                    .get(turbo.file)
                    .map(|d| d.0.load(Ordering::Relaxed));
                vec![
                    note("Lyrics needs Whisper", String::new()),
                    match going {
                        Some(b) => progress(b as f32 / turbo.bytes as f32),
                        None => pad().child(
                            primary(format!(
                                "Download {} · {} MB",
                                turbo.name,
                                turbo.bytes / 1_000_000
                            ))
                            .h(px(30.))
                            .debug_selector(|| "lyrics-get-whisper".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _, _, cx| ws.get_whisper_for_lyrics(cx)),
                            ),
                        ),
                    },
                    small(
                        "It hears the words sung over music, in 99 languages (MIT licence). It \
                         downloads once; then this is heard.",
                    ),
                ]
            }
        }
    }

    /// Heard nothing: why, and what to try.
    fn lyrics_nothing(&mut self, at: At, model: &'static str, cx: &mut Context<Self>) -> Vec<Div> {
        let turbo = &lyrics::WHISPERS[0];
        if model != turbo.name {
            return vec![
                note("No words heard", String::new()),
                pad().child(
                    primary(format!("Get {} and hear again", turbo.name))
                        .h(px(30.))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                let installed = lyrics::installed(&lyrics::folder())
                                    .iter()
                                    .any(|w| w.file == lyrics::WHISPERS[0].file);
                                match installed {
                                    true => ws.hear_lyrics(at, cx),
                                    false => {
                                        if let Some(l) = ws.lyrics.take() {
                                            ws.lyrics_wait = Some(LyricsWait {
                                                file: l.file,
                                                at,
                                                hearing: None,
                                                failed: None,
                                            });
                                        }
                                        ws.get_whisper_for_lyrics(cx);
                                    }
                                }
                            }),
                        ),
                ),
                small(format!(
                    "{model} misses a lot sung over music; {} hears far more.",
                    turbo.name
                )),
            ];
        }
        let vocals = lyrics::vocals_model(&pina_engine::separate::folder()).is_some();
        let why = match vocals {
            true => "Whisper heard no singing in it, its vocals taken out first: it may have none."
                .to_string(),
            false => "If its singing is under a beat, a separation model with vocals takes it out \
                      first so Whisper can hear it: get Mel-Band RoFormer vocals in Settings, \
                      then press Lyrics again."
                .to_string(),
        };
        let mut out = vec![note("No words heard", why)];
        if !vocals {
            out.push(
                pad().child(
                    button("Stems & lyrics settings", false)
                        .h(px(28.))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.show_models(cx)),
                        ),
                ),
            );
        }
        out
    }

    /// Each line heard: when, how often, to hear, loop or drag; the hooks to loop at once.
    fn lyrics_lines(&mut self, at: At, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(heard) = self.lyrics.as_ref().map(|l| l.heard.clone()) else {
            return Vec::new();
        };
        let wave = self.wave(&heard, cx);
        let Some(l) = self.lyrics.as_ref() else {
            return Vec::new();
        };
        let p = &self.session.project;
        let bar = p.bar().max(1);
        let seconds = wave.as_ref().map_or(1.0, |w| w.seconds()).max(0.001);
        let playing = self
            .lyric_playing
            .filter(|(_, until)| std::time::Instant::now() < *until)
            .map(|(i, _)| i);
        let from_vocals = l.heard != l.file;
        // Each hook once, the most sung first: the first time it's sung.
        let key = lyrics::flips::line_key;
        let mut seen = std::collections::HashSet::new();
        let mut hooks: Vec<usize> = (0..l.lines.len())
            .filter(|&i| l.lines[i].sung > 1 && seen.insert(key(&l.lines[i].text)))
            .collect();
        hooks.sort_by_key(|&i| std::cmp::Reverse(l.lines[i].sung));
        let lines = l.lines.clone();
        // A line's own stretch of the wave, a tick where each word starts: where Chop cuts.
        let wave_of = |line: &Line, hue: u32, id: String| {
            let (from, to) = (line.start, line.end + 0.1);
            let view = (from / seconds, to / seconds);
            let loading = wave.is_none();
            let wave = wave.clone();
            div()
                .absolute()
                .size_full()
                // Still being read: a breathing tint where it'll be.
                .when(loading, |d| d.child(waiting(hue, id)))
                .child(
                    canvas(
                        |_, _, _| {},
                        move |b, _, window, _| {
                            if let Some(w) = &wave {
                                crate::editors::sampler::paint_wave(
                                    window,
                                    w,
                                    b,
                                    view,
                                    1.0,
                                    |_| (alpha(hue, 0.45), alpha(hue, 0.95)),
                                );
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
                .children(line.words.iter().skip(1).map(|w| {
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(gpui::relative((w.start - from) / (to - from)))
                        .w(px(1.))
                        .bg(alpha(0xffffff, 0.35))
                }))
        };
        // The actions every line has: hear it, loop it on the bars, chop it at its words.
        let actions = |i: usize, line: &Line, of: &'static str, cx: &mut Context<Self>| {
            let bars = p.lyric_bars(at, line.start, line.end);
            div()
                .flex()
                .flex_none()
                .gap(px(4.))
                .child(
                    hear_button(playing == Some(i), format!("{of}-hear-{i}")).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.hear_line(i);
                            cx.notify();
                        }),
                    ),
                )
                .children(bars.map(|(_, length)| {
                    let bars = length / bar;
                    button(
                        format!("Loop · {bars} bar{}", if bars == 1 { "" } else { "s" }),
                        false,
                    )
                    .h(px(24.))
                    .debug_selector(move || format!("{of}-loop-{i}"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.loop_line(at, i, None, cx)
                        }),
                    )
                }))
                .child(
                    button("Chop", false)
                        .h(px(24.))
                        .debug_selector(move || format!("{of}-chop-{i}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.chop_lyric(at, i, cx)
                            }),
                        ),
                )
        };
        let plural = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
        let mut out = vec![
            pad()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_3))
                        .child(format!(
                            "{} · {}{}",
                            plural(lines.len(), "line"),
                            plural(hooks.len(), "hook"),
                            if from_vocals {
                                " · heard in its vocals"
                            } else {
                                ""
                            }
                        )),
                )
                .when(!hooks.is_empty(), |d| {
                    d.child(
                        primary("Loop the hooks")
                            .h(px(26.))
                            .px(px(12.))
                            .debug_selector(|| "lyrics-loop-hooks".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| ws.loop_hooks(at, cx)),
                            ),
                    )
                }),
        ];
        // A line's row: when it's sung (or, a hook, how often), its words over its stretch of the
        // wave (a tick where each word starts), to hear, loop or chop; drag it onto a track.
        let row = |i: usize, line: &Line, of: &'static str, cx: &mut Context<Self>| {
            let on = playing == Some(i);
            let hook = of == "hook";
            div()
                .relative()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(12.))
                .py(px(6.))
                .border_t_1()
                .border_color(rgb(LINE_SOFT))
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
                .child(
                    div()
                        .w(px(34.))
                        .flex_none()
                        .font_family(MONO)
                        .text_size(px(10.5))
                        .text_color(rgb(if hook { AMBER } else { TEXT_3 }))
                        .child(match hook {
                            true => format!("×{}", line.sung),
                            false => clock(line.start),
                        }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .cursor_grab()
                        .debug_selector(move || match hook {
                            true => format!("lyric-hook-{i}"),
                            false => format!("lyric-line-{i}"),
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .min_w(px(0.))
                                        .text_size(px(12.5))
                                        .when(hook, |d| d.font_weight(FontWeight::MEDIUM))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(line.text.clone()),
                                )
                                .when(!hook && line.sung > 1, |d| {
                                    d.child(
                                        div()
                                            .flex_none()
                                            .font_family(MONO)
                                            .text_size(px(10.5))
                                            .text_color(rgb(AMBER))
                                            .child(format!("×{}", line.sung)),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .relative()
                                .h(px(22.))
                                .rounded(px(4.))
                                .overflow_hidden()
                                .bg(rgb(WELL))
                                .child(wave_of(
                                    line,
                                    if on { ACCENT } else { SLATE },
                                    format!("{of}-{i}"),
                                )),
                        )
                        .on_mouse_down(MouseButton::Left, drag_from(at, i, cx)),
                )
                .child(actions(i, line, of, cx))
        };
        if !hooks.is_empty() {
            out.push(pad().pt(px(4.)).child(heading("HOOKS")));
            for &i in &hooks {
                out.push(row(i, &lines[i], "hook", cx));
            }
        }
        out.push(
            pad()
                .pt(px(10.))
                .child(heading("ALL LINES · drag one onto a track")),
        );
        let rows: Vec<Div> = lines
            .iter()
            .enumerate()
            .map(|(i, line)| row(i, line, "lyric", cx))
            .collect();
        out.extend(rows);
        out
    }

    /// The `i`th line chopped at its words onto a Sampler track of its own (its vocals stem's,
    /// if it was heard there), opened in the Sampler to flip: a word a chop.
    fn chop_lyric(&mut self, at: At, i: usize, cx: &mut Context<Self>) {
        let Some(l) = self.lyrics.as_ref() else {
            return;
        };
        let Some(line) = l.lines.get(i).cloned() else {
            return;
        };
        let stem = (l.heard != l.file).then(|| l.heard.clone());
        let (sides, rate) = &*l.audio;
        let p = &self.session.project;
        let frames =
            (sides.first().map_or(0, Vec::len) as f64 * p.sample_rate as f64 / rate) as u64;
        let mut cuts: Vec<f32> = line.words.iter().map(|w| w.start).collect();
        if cuts.is_empty() {
            cuts.push(line.start);
        }
        cuts[0] = line.start;
        let bar = p.bar().max(1);
        let tick = self.playhead() / bar * bar;
        let mut made = None;
        self.edit(cx, |ws| {
            made = ws.session.project.chop_line(
                at,
                stem.as_deref(),
                &cuts,
                line.end,
                &line.text,
                tick,
                frames,
            );
        });
        match made {
            Some(t) => {
                self.select_track(t);
                self.region = Some((t, 0));
                self.sampler_ui.zone = 0;
                self.sampler_ui.chop = None;
                self.show_bottom(crate::workspace::Bottom::Sampler, cx);
                self.status = Some(format!(
                    "“{}” chopped into {} on a Sampler: flip them, swap them, stutter them",
                    line.text,
                    match cuts.len() {
                        1 => "one chop".to_string(),
                        n => format!("{n} words"),
                    }
                ));
            }
            None => {
                self.session.cancel();
                self.status = Some("The region doesn't play this line".into());
            }
        }
        cx.notify();
    }
}

/// A message in the panel: what's happening, and why.
pub(crate) fn note(title: &'static str, text: impl Into<gpui::SharedString>) -> Div {
    let text = text.into();
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .px(px(12.))
        .py(px(10.))
        .border_t_1()
        .border_color(rgb(LINE_SOFT))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .when(!text.is_empty(), |d| {
            d.child(div().text_size(px(12.)).text_color(rgb(TEXT_3)).child(text))
        })
}

pub(crate) fn pad() -> Div {
    div().px(px(12.)).pb(px(10.))
}

/// Starts dragging line `i` of `at`'s lyrics, to drop it on a track.
fn drag_from(
    at: At,
    i: usize,
    cx: &mut Context<Workspace>,
) -> impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    cx.listener(move |ws, e: &gpui::MouseDownEvent, _, cx| {
        cx.stop_propagation();
        let p = crate::workspace::pt(e.position);
        ws.begin_drag(
            crate::workspace::Drag::Lyric {
                region: at,
                line: i,
                from: p,
                at: p,
                moved: false,
            },
            cx,
        );
    })
}

/// A waveform still being read: its place, breathing in `hue`.
pub(crate) fn waiting(hue: u32, id: String) -> gpui::AnyElement {
    div()
        .absolute()
        .size_full()
        .bg(alpha(hue, 0.16))
        .with_animation(
            gpui::ElementId::Name(format!("wave-waiting-{id}").into()),
            gpui::Animation::new(Duration::from_millis(1200))
                .repeat()
                .with_easing(gpui::pulsating_between(0.3, 1.0)),
            |d, t| d.opacity(t),
        )
        .into_any_element()
}

/// Hear, or (heard) stop: an icon, as the transport's.
pub(crate) fn hear_button(on: bool, sel: String) -> Div {
    div()
        .flex_none()
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(RAISED))
        .bg(rgb(if on { RAISED } else { CARD }))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(RAISED)))
        .debug_selector(move || sel.clone())
        .child(
            icon(if on { Icon::Stop } else { Icon::Play }, 12.).text_color(rgb(if on {
                TEXT
            } else {
                TEXT_2
            })),
        )
}

/// A section's heading.
pub(crate) fn heading(s: &'static str) -> Div {
    div()
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(s)
}

/// The why, under a message's action.
pub(crate) fn small(text: impl Into<gpui::SharedString>) -> Div {
    pad()
        .text_size(px(11.5))
        .text_color(rgb(TEXT_3))
        .child(text.into())
}

/// How far, 0 to 1 (under 0: going, not knowing how far).
pub(crate) fn progress(at: f32) -> Div {
    let known = at >= 0.0;
    pad()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_1()
                .h(px(6.))
                .rounded(px(3.))
                .bg(rgb(WELL))
                .relative()
                .overflow_hidden()
                .child(match known {
                    true => div()
                        .h_full()
                        .rounded(px(3.))
                        .bg(rgb(ACCENT))
                        .w(gpui::relative(at.clamp(0.02, 1.0)))
                        .into_any_element(),
                    // Not knowing how far: a light going across, so it's plainly at work.
                    false => div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w(gpui::relative(0.3))
                        .rounded(px(3.))
                        .bg(rgb(ACCENT))
                        .with_animation(
                            "lyrics-going",
                            gpui::Animation::new(Duration::from_millis(1300)).repeat(),
                            |d, t| d.left(gpui::relative(t * 1.3 - 0.3)),
                        )
                        .into_any_element(),
                }),
        )
        .when(known, |d| {
            d.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(format!("{}%", (at * 100.0) as u32)),
            )
        })
}

impl Workspace {
    /// Where a line dragged to `p` (a window position) would go: an audio track under it, at
    /// the bar there; or its region's lyrics track, at the bar there; none off the tracks.
    fn lyric_target(&self, region: At, p: gpui::Point<f32>) -> Option<(Option<usize>, u64)> {
        let b = self.lanes_bounds.get();
        let (left, top) = (f32::from(b.origin.x), f32::from(b.origin.y));
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        if p.x < left || p.y < top || p.x > left + w || p.y > top + h {
            return None;
        }
        let bar = self.session.project.bar().max(1);
        let tick = self.tick_at(p.x).max(0.0) as u64 / bar * bar;
        let row = self.track_at_y(p.y - top);
        let audio = row.filter(|&t| {
            self.session
                .project
                .tracks
                .get(t)
                .is_some_and(|t| t.instrument.is_none() && t.pad.is_none())
                && t != region.0
        });
        Some((audio, tick))
    }

    /// A line being dragged, at `p`.
    pub(crate) fn drag_lyric(&mut self, p: gpui::Point<f32>, cx: &mut Context<Self>) {
        if let Some(crate::workspace::Drag::Lyric {
            from, at, moved, ..
        }) = &mut self.drag
        {
            *at = p;
            *moved |= (p.x - from.x).hypot(p.y - from.y) > 4.0;
        }
        cx.notify();
    }

    /// A line let go of over the tracks: a loop there (`loop_line`).
    pub(crate) fn drop_lyric(
        &mut self,
        region: At,
        line: usize,
        p: gpui::Point<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some((track, tick)) = self.lyric_target(region, p) else {
            return cx.notify();
        };
        let track = match track {
            Some(t) => t,
            // Not over an audio track: its lyrics track, made if it isn't there.
            None => {
                let mut made = 0;
                let exists = {
                    let p = &self.session.project;
                    let name = format!("{} Lyrics", p.tracks[region.0].name);
                    p.tracks.iter().position(|t| t.name == name)
                };
                match exists {
                    Some(t) => t,
                    None => {
                        self.edit(cx, |ws| made = ws.session.project.lyrics_track(region.0));
                        // Its own undo step folded into the loop's.
                        self.session.cancel();
                        made
                    }
                }
            }
        };
        self.loop_line(region, line, Some((track, tick)), cx);
    }

    /// The line being dragged, by the pointer, saying where it would go.
    pub fn lyric_ghost(&self) -> Option<impl IntoElement + use<>> {
        let Some(crate::workspace::Drag::Lyric {
            region,
            line,
            at,
            moved: true,
            ..
        }) = self.drag
        else {
            return None;
        };
        let text = self.lyrics.as_ref()?.lines.get(line)?.text.clone();
        let p = &self.session.project;
        let bar = p.bar().max(1);
        let hint = match self.lyric_target(region, at) {
            Some((Some(t), tick)) => {
                format!("A loop on {}, bar {}", p.tracks[t].name, tick / bar + 1)
            }
            Some((None, tick)) => format!(
                "A loop on {} Lyrics, bar {}",
                p.tracks[region.0].name,
                tick / bar + 1
            ),
            None => "Drop it on the tracks".to_string(),
        };
        Some(
            div()
                .absolute()
                .left(px(at.x + 14.))
                .top(px(at.y + 10.))
                .w(px(240.))
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .rounded(px(9.))
                .bg(rgb(0x1c1c22))
                .border_1()
                .border_color(rgb(AMBER))
                .shadow_lg()
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("“{text}”")),
                )
                .child(div().text_size(px(11.)).text_color(rgb(TEXT_3)).child(hint)),
        )
    }
}
