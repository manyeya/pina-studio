//! Flips, beside an audio region's lyrics: the Studio Agent's engine (Claude Code or Codex)
//! reads the words sung and how long each lasts, and writes the hooks a producer would make of
//! them (`pina_engine::lyrics::flips`): words from anywhere in a new order on the grid, pitched
//! up as soul samples are or down and slowed, stuttered, answered, reversed into the beat. Each
//! shows as it's written: what it says now, its chops on the bars with their waveforms, to hear
//! looping or to use (a Sampler track playing it, opened in the Sampler to flip further).

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use futures::StreamExt;
use gpui::{
    Animation, AnimationExt, AppContext, Context, Div, Entity, FontWeight, MouseButton, canvas,
    div, prelude::*, px, relative, rgb,
};
use pina_engine::{
    audio::engine::{AudioData, resample},
    lyrics::flips::{self, Flip, Reading},
    song::project::{At, PPQ},
};

use crate::{
    agent::{Asked, Event, Kind, Tuning, ask_once, tuning},
    sounds::browser::primary,
    ui::{
        menu::{Menu, MenuAction},
        text::{TextEvent, TextInput},
        theme::*,
    },
    workspace::Workspace,
};

/// A recording's audio, as read: its sides and rate.
type Audio = Arc<(Vec<Vec<f32>>, f64)>;

/// How many flips are asked for at a time.
const ASK_FOR: usize = 6;

/// The chops' colours, in the order a flip first plays them: the Sampler's, so a chop is the
/// same colour here and where Use it puts it.
use crate::editors::sampler::HUES;

/// The Lyrics panel's two pages.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum LyricsTab {
    /// Its slices (Smart Slice), or its lines.
    #[default]
    Lines,
    /// The words sung in it, beside its slices.
    Words,
    Flips,
}

/// The flips found, how they're asked for, and what's being found now.
#[derive(Default)]
pub struct FlipsUi {
    pub tab: LyricsTab,
    /// The style asked for (an id of `flips::STYLES`); "any" when none is picked.
    pub style: Option<&'static str>,
    /// What the person wants, in their words.
    pub vibe: Option<Entity<TextInput>>,
    /// The engine asked: the Studio Agent's, unless another is picked here.
    pub kind: Option<Kind>,
    pub now: Option<Flips>,
    run: u32,
    /// The engine's command, when it isn't where it's installed (a test's stand-in).
    pub command: Option<PathBuf>,
    /// The model and effort for the engine, as last read.
    tuned: Option<(Kind, Tuning)>,
}

/// The flips for one file's words.
pub struct Flips {
    /// The file the words were heard in: its vocals stem, or the region's own.
    pub heard: PathBuf,
    pub ideas: Vec<Flip>,
    /// When each came in: a new one fades in, once.
    arrived: Vec<Instant>,
    pub asking: Option<Asking>,
    pub error: Option<String>,
    /// The flip being heard: from when, for how long, and its loop's length (seconds).
    pub playing: Option<(usize, Instant, f32, f32)>,
}

impl Flips {
    /// Flips shown for `heard`, and one being written (by `kind`, at `stage`, `secs` in), for
    /// pictures of them.
    #[cfg(test)]
    pub fn shown(heard: PathBuf, ideas: Vec<Flip>, finding: Option<(Kind, Stage, u64)>) -> Flips {
        let found = ideas.len();
        Flips {
            arrived: vec![Instant::now() - Duration::from_secs(9); found],
            heard,
            ideas,
            asking: finding.map(|(kind, stage, secs)| Asking {
                kind,
                started: Instant::now() - Duration::from_secs(secs),
                stage,
                asked: Asked::idle(),
                run: 0,
                words: 274,
                found,
            }),
            error: None,
            playing: None,
        }
    }
}

/// An engine finding flips.
pub struct Asking {
    pub kind: Kind,
    pub started: Instant,
    pub stage: Stage,
    asked: Asked,
    run: u32,
    pub words: usize,
    /// Found so far, this time.
    pub found: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Starting,
    Thinking,
    Writing,
}

/// Where flips' model and effort are kept, for an engine.
fn flips_key(kind: Kind) -> String {
    format!("flips:{}", kind.def().id)
}

/// The ways to flip what's flipped: a voice's words, or a beat's slices.
fn ways(beat: bool) -> &'static [(&'static str, &'static str, &'static str)] {
    match beat {
        true => flips::BEAT_STYLES,
        false => flips::STYLES,
    }
}

fn style_name(style: &str) -> &'static str {
    flips::STYLES
        .iter()
        .chain(flips::BEAT_STYLES)
        .find(|s| s.0 == style)
        .map_or("Flip", |s| match s.0 {
            "any" => "Flip",
            _ => s.1,
        })
}

/// A quiet picker, as the Studio Agent's prompt box has them: text and a chevron, raised on
/// hover.
fn picker(sel: &'static str) -> Div {
    div()
        .flex_none()
        .h(px(22.))
        .px(px(5.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|d| d.bg(rgb(RAISED)))
        .text_size(px(11.))
        .text_color(rgb(TEXT_3))
        .whitespace_nowrap()
        .debug_selector(move || sel.into())
}

fn chevron() -> gpui::Svg {
    icon(Icon::Chevron, 10.).text_color(rgb(TEXT_4))
}

/// The Studio Agent at work: its dot, breathing.
fn working_dot(id: &'static str) -> gpui::AnyElement {
    div()
        .flex_none()
        .size(px(6.))
        .rounded(px(3.))
        .bg(rgb(AGENT))
        .with_animation(
            id,
            Animation::new(Duration::from_millis(1000))
                .repeat()
                .with_easing(gpui::pulsating_between(0.25, 1.0)),
            |d, t| d.opacity(t),
        )
        .into_any_element()
}

/// The two numbers a row's meta has, in the house's mono.
fn mono(s: impl Into<gpui::SharedString>) -> Div {
    div()
        .flex_none()
        .font_family(MONO)
        .text_size(px(10.5))
        .text_color(rgb(TEXT_3))
        .whitespace_nowrap()
        .child(s.into())
}

impl Workspace {
    /// What's flipped: a recording's slices (Smart Slice), else the words heard in it; the file
    /// they're in and its audio, whether it's a beat's.
    fn flip_source(&self) -> Option<(PathBuf, Audio, bool)> {
        if let Some(b) = self.beat.as_ref().filter(|b| !b.slices.is_empty()) {
            let words = self
                .lyrics
                .as_ref()
                .filter(|l| l.file == b.file && !l.lines.is_empty());
            return Some(match (b.heard, words) {
                (pina_engine::smart::InIt::Voice, Some(l)) => {
                    (l.heard.clone(), l.audio.clone(), false)
                }
                _ => (b.cut.clone(), b.audio.clone(), true),
            });
        }
        let l = self.lyrics.as_ref().filter(|l| !l.lines.is_empty())?;
        Some((l.heard.clone(), l.audio.clone(), false))
    }

    /// The engine flips are asked of.
    fn flips_kind(&self) -> Kind {
        self.flips.kind.unwrap_or(self.chat.kind)
    }

    /// The model and effort flips are asked with, kept for each engine apart from the Studio
    /// Agent's own (fast and light for flips, say, thorough for the conversation); at first, the
    /// Studio Agent's.
    pub(crate) fn flip_tuning(&mut self) -> Tuning {
        let kind = self.flips_kind();
        if let Some((k, t)) = &self.flips.tuned
            && *k == kind
        {
            return t.clone();
        }
        let kept = crate::agent::tuning_for(&flips_key(kind));
        let t = match kept == Tuning::default() {
            true => tuning(kind),
            false => kept,
        };
        self.flips.tuned = Some((kind, t.clone()));
        t
    }

    /// A model or effort picked for flips (an effort the new model doesn't take goes back to its
    /// own), kept.
    pub(crate) fn pick_flip_tuning(
        &mut self,
        model: Option<Option<String>>,
        effort: Option<Option<String>>,
    ) {
        let kind = self.flips_kind();
        let mut t = self.flip_tuning();
        if let Some(m) = model {
            t.model = m;
        }
        if let Some(e) = effort {
            t.effort = e;
        }
        if t.effort
            .as_ref()
            .is_some_and(|e| !kind.efforts(t.model.as_deref()).contains(e))
        {
            t.effort = None;
        }
        crate::agent::tune_for(&flips_key(kind), &t);
        self.flips.tuned = Some((kind, t));
    }

    /// Asks the engine for flips of the lyrics shown: new ones (the flips there are, kept,
    /// named so it makes others) when `more`. Each shows as it's written.
    pub(crate) fn find_flips(&mut self, more: bool, cx: &mut Context<Self>) {
        let Some((heard, _, beat)) = self.flip_source() else {
            return;
        };
        if let Some(a) = self.flips.now.as_ref().and_then(|f| f.asking.as_ref()) {
            a.asked.stop();
        }
        self.player.stop_audition();
        let p = &self.session.project;
        let bpb = (p.bar() / PPQ).max(1) as u32;
        let key = p.song_key().map(|k| k.name());
        let vibe = self
            .flips
            .vibe
            .as_ref()
            .map(|v| v.read(cx).text().to_string())
            .unwrap_or_default();
        let kept: Vec<Flip> = match self.flips.now.take() {
            Some(f) if more && f.heard == heard => f.ideas,
            _ => Vec::new(),
        };
        let made: Vec<String> = kept.iter().map(|f| f.name.clone()).collect();
        let style = self.flips.style.unwrap_or("any");
        let (words, prompt) = match (beat, &self.beat, &self.lyrics) {
            (false, _, Some(l)) => (
                flips::numbered(&l.lines),
                flips::prompt(
                    &l.lines,
                    p.tempo,
                    bpb,
                    key.as_deref(),
                    style,
                    &vibe,
                    ASK_FOR,
                    &made,
                ),
            ),
            (true, Some(b), _) => (
                flips::slice_words(&b.slices),
                flips::beat_prompt(
                    &b.slices,
                    p.tempo,
                    bpb,
                    key.as_deref(),
                    style,
                    &vibe,
                    ASK_FOR,
                    &made,
                ),
            ),
            (_, _, Some(l)) => (
                flips::numbered(&l.lines),
                flips::prompt(
                    &l.lines,
                    p.tempo,
                    bpb,
                    key.as_deref(),
                    style,
                    &vibe,
                    ASK_FOR,
                    &made,
                ),
            ),
            _ => return,
        };
        let kind = self.flips_kind();
        let tuned = self.flip_tuning();
        let name = kind.def().name;
        self.flips.run += 1;
        let run = self.flips.run;
        self.flips.tab = LyricsTab::Flips;
        let asked = self
            .flips
            .command
            .clone()
            .or_else(|| kind.command())
            .ok_or(format!(
                "{name} isn't installed on this computer. Install it and sign in, or pick the \
                 other engine above."
            ))
            .and_then(|cmd| ask_once(kind, &cmd, &tuned, flips::SYSTEM, &prompt));
        let (asked, mut events) = match asked {
            Ok(a) => a,
            Err(e) => {
                self.flips.now = Some(Flips {
                    arrived: vec![Instant::now() - Duration::from_secs(9); kept.len()],
                    heard,
                    ideas: kept,
                    asking: None,
                    error: Some(e),
                    playing: None,
                });
                return cx.notify();
            }
        };
        self.flips.now = Some(Flips {
            arrived: vec![Instant::now() - Duration::from_secs(9); kept.len()],
            heard,
            ideas: kept,
            asking: Some(Asking {
                kind,
                started: Instant::now(),
                stage: Stage::Starting,
                asked,
                run,
                words: words.len(),
                found: 0,
            }),
            error: None,
            playing: None,
        });
        self.status = Some(format!("{name} is finding flips in the words…"));
        // The time it's taking, counted as it goes.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let going = this.update(cx, |ws, cx| {
                    let on = ws.flips_asking().is_some_and(|a| a.run == run);
                    if on {
                        cx.notify();
                    }
                    on
                });
                if !matches!(going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let mut reading = Reading::default();
            while let Some(e) = events.next().await {
                let done = matches!(e, Event::Done { .. });
                let ours = this.update(cx, |ws, cx| {
                    ws.flips_said(run, e, &mut reading, &words, bpb, cx)
                });
                if done || !matches!(ours, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn flips_asking(&self) -> Option<&Asking> {
        self.flips.now.as_ref()?.asking.as_ref()
    }

    /// What the engine said, while it's still the one asked (`run`): each flip as its line
    /// ends; at the end, what went wrong if it found none.
    fn flips_said(
        &mut self,
        run: u32,
        e: Event,
        reading: &mut Reading,
        words: &[pina_engine::lyrics::Word],
        bpb: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(f) = self.flips.now.as_mut() else {
            return false;
        };
        let Some(a) = f.asking.as_mut().filter(|a| a.run == run) else {
            return false;
        };
        match e {
            Event::Thinking if a.stage == Stage::Starting => a.stage = Stage::Thinking,
            Event::Text { text } => {
                a.stage = Stage::Writing;
                let new = reading.add(&text, words, bpb);
                a.found += new.len();
                f.arrived.extend(new.iter().map(|_| Instant::now()));
                f.ideas.extend(new);
            }
            Event::Done { error, stopped, .. } => {
                let new = reading.end(words, bpb);
                a.found += new.len();
                f.arrived.extend(new.iter().map(|_| Instant::now()));
                f.ideas.extend(new);
                let (found, kind) = (a.found, a.kind);
                let name = kind.def().name;
                f.asking = None;
                f.error = match (error, stopped) {
                    (_, true) => None,
                    (Some(e), _) if found == 0 => Some(kind.explain(&e).unwrap_or(e)),
                    (None, _) if found == 0 => Some(format!(
                        "{name} answered, but not with flips Pina Studio could read. Try again."
                    )),
                    _ => None,
                };
                self.status = Some(match found {
                    0 => format!("{name} found no flips"),
                    n => format!("{name} found {n} flips: hear them, use the one that hits"),
                });
            }
            _ => {}
        }
        cx.notify();
        true
    }

    /// Stops finding: what's found is kept.
    fn stop_flips(&mut self, cx: &mut Context<Self>) {
        if let Some(f) = self.flips.now.as_mut()
            && let Some(a) = f.asking.take()
        {
            a.asked.stop();
            self.status = Some(format!("Stopped: {} flips kept", f.ideas.len()));
        }
        cx.notify();
    }

    /// Hears flip `i` looping (about 8 seconds of it), or stops it if it's playing.
    fn hear_flip(&mut self, i: usize, cx: &mut Context<Self>) {
        let (Some((heard, audio, _)), Some(f)) = (self.flip_source(), self.flips.now.as_mut())
        else {
            return;
        };
        let now = Instant::now();
        if let Some((j, from, long, _)) = f.playing.take()
            && j == i
            && now.duration_since(from).as_secs_f32() < long
        {
            self.player.stop_audition();
            return cx.notify();
        }
        let Some(flip) = f.ideas.get(i).filter(|_| f.heard == heard) else {
            return;
        };
        let p = &self.session.project;
        let bpb = (p.bar() / PPQ).max(1) as u32;
        let loop_secs = (flip.bars * bpb) as f32 * 60.0 / p.tempo as f32;
        let passes = ((8.0 / loop_secs).ceil() as usize).clamp(1, 4);
        let (sides, rate) = &*audio;
        let out = flips::render(flip, sides, *rate, p.tempo, bpb, passes);
        let to = self.player.rate();
        let channels: Vec<Vec<f32>> = out.iter().map(|s| resample(s, *rate, to)).collect();
        let _ = self
            .player
            .audition_audio(std::sync::Arc::new(AudioData::new(channels)), 0.0, 0.0);
        f.playing = Some((i, now, loop_secs * passes as f32, loop_secs));
        // Its playhead, moving.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                let going = this.update(cx, |ws, cx| {
                    let on = ws.flips.now.as_ref().and_then(|f| f.playing).is_some_and(
                        |(j, from, long, _)| {
                            j == i && from == now && from.elapsed().as_secs_f32() < long
                        },
                    );
                    cx.notify();
                    on
                });
                if !matches!(going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Flip `i` in the song: on a Sampler track of its own, looping from the playhead's bar,
    /// opened in the Sampler to flip further.
    pub(crate) fn use_flip(&mut self, at: At, i: usize, cx: &mut Context<Self>) {
        let (Some((_, audio, _)), Some(f)) = (self.flip_source(), self.flips.now.as_ref()) else {
            return;
        };
        let Some(flip) = f.ideas.get(i).cloned() else {
            return;
        };
        let (sides, rate) = &*audio;
        let seconds = sides.first().map_or(0, Vec::len) as f32 / *rate as f32;
        let heard = f.heard.clone();
        let bar = self.session.project.bar().max(1);
        let tick = self.playhead() / bar * bar;
        self.player.stop_audition();
        let mut made = None;
        self.edit(cx, |ws| {
            made = ws
                .session
                .project
                .flip_words(at, &heard, seconds, &flip, tick)
        });
        match made {
            Some(t) => {
                self.select_track(t);
                self.region = Some((t, 0));
                self.sampler_ui.zone = 0;
                self.sampler_ui.chop = None;
                self.show_bottom(crate::workspace::Bottom::Sampler, cx);
                self.status = Some(format!(
                    "“{}” is on its own Sampler, looping from bar {}: flip it further here",
                    flip.name,
                    tick / bar + 1
                ));
            }
            None => {
                self.session.cancel();
                self.status = Some("That flip plays nothing".into());
            }
        }
        cx.notify();
    }

    /// The Lyrics panel's tabs, in its header: its lines, and the flips (how many, the Studio
    /// Agent's dot while it's finding more).
    pub(crate) fn lyrics_tabs(
        &mut self,
        lines: &'static str,
        words: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let tab = self.flips.tab;
        let found = self.flips.now.as_ref().map_or(0, |f| f.ideas.len());
        let asking = self.flips_asking().is_some();
        let hearing = self
            .lyrics_wait
            .as_ref()
            .is_some_and(|w| w.hearing.is_some());
        tray()
            .p(px(2.))
            .flex_none()
            .child(
                seg(lines, tab == LyricsTab::Lines, 22.)
                    .debug_selector(|| "lyrics-tab-lines".into())
                    .on_mouse_down(MouseButton::Left, pick(LyricsTab::Lines, cx)),
            )
            .when(words, |d| {
                d.child(
                    seg("Words", tab == LyricsTab::Words, 22.)
                        .debug_selector(|| "lyrics-tab-words".into())
                        .when(hearing, |d| d.child(working_dot("words-tab-working")))
                        .on_mouse_down(MouseButton::Left, pick(LyricsTab::Words, cx)),
                )
            })
            .child(
                seg("Flips", tab == LyricsTab::Flips, 22.)
                    .debug_selector(|| "lyrics-tab-flips".into())
                    .when(found > 0, |d| {
                        d.child(
                            div()
                                .font_family(MONO)
                                .text_size(px(10.5))
                                .text_color(rgb(TEXT_3))
                                .child(found.to_string()),
                        )
                    })
                    .when(asking, |d| d.child(working_dot("flips-tab-working")))
                    .on_mouse_down(MouseButton::Left, pick(LyricsTab::Flips, cx)),
            )
    }

    /// The Flips page: what to ask for (in words, and the engine, its model and effort, and a
    /// style), Find (Stop while it works, in its place), and the flips, a row each.
    pub(crate) fn flips_page(&mut self, at: At, cx: &mut Context<Self>) -> Vec<Div> {
        let Some((heard, _, beat)) = self.flip_source() else {
            return Vec::new();
        };
        if self.flips.now.as_ref().is_some_and(|f| f.heard != heard) {
            self.flips.now = None;
        }
        let words = match beat {
            true => self.beat.as_ref().map_or(0, |b| b.slices.len()),
            false => self
                .lyrics
                .as_ref()
                .map_or(0, |l| flips::numbered(&l.lines).len()),
        };
        let vibe = self.flips_vibe(cx);
        let kind = self.flips_kind();
        let tuned = self.flip_tuning();
        let style = self.flips.style.unwrap_or("any");
        let asking = self
            .flips_asking()
            .map(|a| (a.kind, a.started, a.stage, a.found, a.words));
        let found = self.flips.now.as_ref().map_or(0, |f| f.ideas.len());
        let error = self.flips.now.as_ref().and_then(|f| f.error.clone());
        let tick = |on: bool| if on { "✓ " } else { "    " };
        let engines: Vec<(String, MenuAction)> = [Kind::ClaudeCode, Kind::Codex]
            .iter()
            .map(|&k| {
                let there = self.flips.command.is_some() || k.command().is_some();
                let label = match there {
                    true => k.def().name.to_string(),
                    false => format!("{} · not installed", k.def().name),
                };
                (
                    format!("{}{label}", tick(k == kind)),
                    MenuAction::FlipEngine(k),
                )
            })
            .collect();
        // The model, then the efforts it takes, as the Studio Agent's own menu has them.
        let mut tuning_items = vec![
            ("Model".to_string(), MenuAction::Nothing),
            (
                format!("{}Its own", tick(tuned.model.is_none())),
                MenuAction::FlipModel(None),
            ),
        ];
        tuning_items.extend(kind.models().into_iter().map(|m| {
            (
                format!("{}{}", tick(tuned.model.as_deref() == Some(&m.id)), m.name),
                MenuAction::FlipModel(Some(m.id)),
            )
        }));
        tuning_items.push((String::new(), MenuAction::Nothing));
        tuning_items.push(("Effort".to_string(), MenuAction::Nothing));
        tuning_items.push((
            format!("{}Its own", tick(tuned.effort.is_none())),
            MenuAction::FlipEffort(None),
        ));
        tuning_items.extend(kind.efforts(tuned.model.as_deref()).into_iter().map(|e| {
            (
                format!(
                    "{}{}",
                    tick(tuned.effort.as_deref() == Some(e.as_str())),
                    crate::agent::effort_name(&e)
                ),
                MenuAction::FlipEffort(Some(e)),
            )
        }));
        let tuning_label = {
            let model = match &tuned.model {
                Some(m) => crate::agent::model_name(kind, m),
                None => kind.default_model().map_or("Default model".into(), |m| {
                    crate::agent::model_name(kind, &m)
                }),
            };
            match &tuned.effort {
                Some(e) => format!("{model} · {}", crate::agent::effort_name(e)),
                None => model,
            }
        };
        let styles: Vec<(String, MenuAction)> = ways(beat)
            .iter()
            .map(|s| {
                (
                    format!("{}{}", tick(s.0 == style), s.1),
                    MenuAction::FlipStyle(s.0),
                )
            })
            .collect();
        let style_label = ways(beat)
            .iter()
            .find(|s| s.0 == style)
            .map_or("Any style", |s| s.1);
        // Find, or (working) Stop, in one place.
        let go = match asking {
            Some(_) => button("Stop", false)
                .h(px(28.))
                .flex_none()
                .debug_selector(|| "flips-stop".into())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.stop_flips(cx)),
                ),
            None => primary(match found {
                0 => "Find flips".to_string(),
                _ => format!("{ASK_FOR} more"),
            })
            .h(px(28.))
            .px(px(12.))
            .flex_none()
            .debug_selector(|| "flips-find".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| ws.find_flips(found > 0, cx)),
            ),
        };
        let mut out = vec![
            div()
                .px(px(12.))
                .pb(px(6.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .h(px(28.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(rgb(RAISED))
                                .bg(rgb(CARD))
                                .text_size(px(12.))
                                .debug_selector(|| "flips-vibe".into())
                                .child(div().flex_1().min_w(px(0.)).child(vibe)),
                        )
                        .child(go),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .child(
                            picker("flips-engine")
                                .child(crate::agent::chat::logo(kind, 11.))
                                .child(kind.def().name)
                                .child(chevron())
                                .on_mouse_down(MouseButton::Left, menu(engines, cx)),
                        )
                        .child(
                            picker("flips-tuning")
                                .min_w(px(0.))
                                .flex_shrink(1.)
                                .child(
                                    div()
                                        .min_w(px(0.))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(tuning_label),
                                )
                                .child(chevron())
                                .on_mouse_down(MouseButton::Left, menu(tuning_items, cx)),
                        )
                        .child(
                            picker("flips-style")
                                .child(style_label)
                                .child(chevron())
                                .on_mouse_down(MouseButton::Left, menu(styles, cx)),
                        ),
                ),
        ];
        // The list's header: how many, and (working) what it's doing, how long it's taken and
        // how far, on a line under it.
        if found > 0 || asking.is_some() {
            let doing = asking.map(|(k, started, stage, got, words)| {
                let what = match stage {
                    Stage::Starting => format!("Starting {}", k.def().name),
                    Stage::Thinking => {
                        format!("Reading {words} {}", if beat { "slices" } else { "words" })
                    }
                    Stage::Writing if got < ASK_FOR => format!("Writing {} of {ASK_FOR}", got + 1),
                    Stage::Writing => "Finishing".to_string(),
                };
                format!("{what} · {}s", started.elapsed().as_secs())
            });
            out.push(
                div()
                    .px(px(12.))
                    .pt(px(4.))
                    .pb(px(5.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_3))
                            .child(format!("{found} FLIP{}", if found == 1 { "" } else { "S" })),
                    )
                    .children(doing.map(|d| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .debug_selector(|| "flips-finding".into())
                            .child(working_dot("flips-working"))
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.5))
                                    .text_color(rgb(AGENT_TEXT))
                                    .child(d),
                            )
                    }))
                    .when(asking.is_none(), |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(TEXT_3))
                                .cursor_pointer()
                                .hover(|s| s.text_color(rgb(TEXT)))
                                .debug_selector(|| "flips-again".into())
                                .child("Start over")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _, _, cx| ws.find_flips(false, cx)),
                                ),
                        )
                    }),
            );
            out.push(progress_line(asking.map(|a| (a.2, a.3))));
        }
        if let Some(e) = error {
            out.extend(self.flips_failed(e, kind, cx));
        } else if found == 0 && asking.is_none() {
            out.push(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(match beat {
                        true => format!(
                            "The Studio Agent reads the {words} slices, what each sounds like \
                             and where it falls, and flips them as on an MPC: a new groove from \
                             the hits, chords re-sequenced, rolls, half-time, reversed into the \
                             beat. Nothing changes in the song until you use one."
                        ),
                        false => format!(
                            "The Studio Agent reads the {words} words and when each is sung, \
                             and flips them into hooks: words in a new order on the grid, \
                             pitched up or down, stuttered, answered, reversed into the beat. \
                             Nothing changes in the song until you use one."
                        ),
                    }),
            );
        }
        out.extend(self.flip_rows(at, cx));
        // The next, still being written: one row in its shape.
        if asking.is_some_and(|a| a.3 < ASK_FOR) {
            out.push(ghost());
        }
        out
    }

    /// The field for what the person wants: Enter does what the button does (more, keeping
    /// those there are; Start over is the one way to clear them).
    fn flips_vibe(&mut self, cx: &mut Context<Self>) -> Entity<TextInput> {
        if let Some(v) = &self.flips.vibe {
            return v.clone();
        }
        let v = cx.new(|cx| {
            TextInput::form("", "What should it make? “darker”, “say never let go”…", cx)
        });
        cx.subscribe(&v, |ws, _, e: &TextEvent, cx| {
            if matches!(e, TextEvent::Confirm(t) if !t.trim().is_empty())
                && ws.flips_asking().is_none()
            {
                let more = ws.flips.now.as_ref().is_some_and(|f| !f.ideas.is_empty());
                ws.find_flips(more, cx);
            }
        })
        .detach();
        self.flips.vibe = Some(v.clone());
        v
    }

    /// It didn't find any: why, in plain words, and what to do.
    fn flips_failed(&mut self, e: String, kind: Kind, cx: &mut Context<Self>) -> Vec<Div> {
        let other = match kind {
            Kind::ClaudeCode => Kind::Codex,
            Kind::Codex => Kind::ClaudeCode,
        };
        vec![
            super::lyrics::note("No flips this time", e).debug_selector(|| "flips-error".into()),
            div()
                .px(px(12.))
                .pb(px(10.))
                .flex()
                .gap(px(6.))
                .child(button("Try again", false).h(px(26.)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _, _, cx| ws.find_flips(false, cx)),
                ))
                .when(other.command().is_some(), |d| {
                    d.child(
                        button(format!("Ask {} instead", other.def().name), false)
                            .h(px(26.))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.flips.kind = Some(other);
                                    ws.find_flips(false, cx)
                                }),
                            ),
                    )
                }),
        ]
    }

    /// A row a flip, as the Lines tab's rows: what it says, its bars and pitch, hear and use;
    /// its chops on the bars (each its number, colour and waveform, as the Sampler will have
    /// them; reversed and pitched ones marked; a playhead while it's heard); its name and
    /// style, how many chops and hits; while heard, why it works. Click a row to hear it. New
    /// ones fade in as they're written.
    fn flip_rows(&mut self, at: At, cx: &mut Context<Self>) -> Vec<Div> {
        let Some(heard) = self.flips.now.as_ref().map(|f| f.heard.clone()) else {
            return Vec::new();
        };
        let wave = self.wave(&heard, cx);
        let seconds = wave.as_ref().map_or(1.0, |w| w.seconds()).max(0.001);
        let p = &self.session.project;
        let bpb = (p.bar() / PPQ).max(1) as u32;
        let tempo = p.tempo as f32;
        let Some(f) = self.flips.now.as_ref() else {
            return Vec::new();
        };
        let playing = f
            .playing
            .filter(|(_, from, long, _)| from.elapsed().as_secs_f32() < *long);
        let mut out = Vec::new();
        for (i, flip) in f.ideas.iter().enumerate() {
            let on = playing.is_some_and(|p| p.0 == i);
            let beats = (flip.bars * bpb) as f32;
            // A chop as the Sampler makes it: its words, which way and how high.
            let mut chops: Vec<(u32, u32, bool, i8)> = Vec::new();
            let blocks: Vec<Div> = flip
                .hits
                .iter()
                .enumerate()
                .map(|(k, h)| {
                    let key = (h.from.to_bits(), h.to.to_bits(), h.reverse, h.pitch);
                    let c = chops.iter().position(|x| *x == key).unwrap_or_else(|| {
                        chops.push(key);
                        chops.len() - 1
                    });
                    let tone = HUES[c % HUES.len()];
                    let next = flip.hits.get(k + 1).map_or(beats, |n| n.beat);
                    let lasts = h.beats.min(next - h.beat).max(0.05);
                    // What of the chop sounds in its place: sped by its pitch.
                    let sung = (lasts * 60.0 / tempo * flip.speed(h)).min(h.to - h.from);
                    let view = match h.reverse {
                        true => ((h.to - sung) / seconds, h.to / seconds),
                        false => (h.from / seconds, (h.from + sung) / seconds),
                    };
                    // Room for its number (the lane's about 370 wide).
                    let roomy = lasts / beats * 370.0 >= 16.0;
                    let loading = wave.is_none();
                    let wave = wave.clone();
                    div()
                        .absolute()
                        .top(px(2.))
                        .bottom(px(2.))
                        .left(relative(h.beat / beats))
                        .w(relative(lasts / beats))
                        .rounded(px(2.))
                        .overflow_hidden()
                        .bg(alpha(tone, 0.16))
                        .border_l_2()
                        .border_color(rgb(tone))
                        .when(loading, |d| {
                            d.child(super::lyrics::waiting(tone, format!("flip-{i}-{k}")))
                        })
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
                                            |_| (alpha(tone, 0.4), alpha(tone, 0.9)),
                                        );
                                    }
                                },
                            )
                            .absolute()
                            .size_full(),
                        )
                        .when(roomy, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .top(px(2.))
                                    .left(px(3.))
                                    .px(px(2.))
                                    .rounded(px(2.))
                                    .bg(alpha(0x000000, 0.6))
                                    .flex()
                                    .items_center()
                                    .gap(px(2.))
                                    .font_family(MONO)
                                    .text_size(px(10.))
                                    .line_height(px(11.))
                                    .text_color(rgb(tone))
                                    .child((c + 1).to_string())
                                    .when(h.reverse, |d| {
                                        d.child(icon(Icon::ArrowLeft, 9.).text_color(rgb(tone)))
                                    })
                                    .when(h.pitch != 0, |d| d.child(format!("{:+}", h.pitch))),
                            )
                        })
                })
                .collect();
            let grid: Vec<Div> = (1..flip.bars * bpb)
                .map(|b| {
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(relative(b as f32 / beats))
                        .w(px(1.))
                        .bg(alpha(0xffffff, if b % bpb == 0 { 0.18 } else { 0.06 }))
                })
                .collect();
            let head = playing.filter(|p| p.0 == i).map(|(_, from, _, looped)| {
                let x = (from.elapsed().as_secs_f32() % looped) / looped;
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(relative(x))
                    .w(px(2.))
                    .bg(rgb(TEXT))
            });
            let bars = format!("{} bar{}", flip.bars, if flip.bars == 1 { "" } else { "s" });
            let pitch = match flip.pitch {
                0 => String::new(),
                p => format!("  {p:+} st"),
            };
            let title = match flip.says.is_empty() {
                true => flip.name.clone(),
                false => flip.says.clone(),
            };
            let row = div()
                .relative()
                .px(px(12.))
                .py(px(8.))
                .flex()
                .flex_col()
                .gap(px(5.))
                .border_t_1()
                .border_color(rgb(LINE_SOFT))
                .cursor_pointer()
                .when(on, |d| d.bg(rgb(FIELD)))
                .when(!on, |d| d.hover(|s| s.bg(rgb(OFF))))
                .debug_selector(move || format!("flip-{i}"))
                // Heard: an edge in the accent, as what's playing has.
                .when(on, |d| {
                    d.child(
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
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(TEXT))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(title),
                        )
                        .child(mono(format!("{bars}{pitch}")))
                        .child(
                            super::lyrics::hear_button(on, format!("flip-hear-{i}")).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.hear_flip(i, cx)
                                }),
                            ),
                        )
                        .child(
                            button("Use it", false)
                                .h(px(24.))
                                .flex_none()
                                .debug_selector(move || format!("flip-use-{i}"))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.use_flip(at, i, cx)
                                    }),
                                ),
                        ),
                )
                .child(
                    div()
                        .relative()
                        .h(px(26.))
                        .rounded(px(4.))
                        .overflow_hidden()
                        .bg(rgb(WELL))
                        .children(grid)
                        .children(blocks)
                        .children(head),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .gap(px(4.))
                                .text_size(px(11.))
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(div().text_color(rgb(TEXT_2)).child(flip.name.clone()))
                                .child(div().text_color(rgb(TEXT_4)).child("·"))
                                .child(
                                    div().text_color(rgb(TEXT_3)).child(style_name(&flip.style)),
                                ),
                        )
                        .child(mono(format!(
                            "{} chop{} · {} hits",
                            chops.len(),
                            if chops.len() == 1 { "" } else { "s" },
                            flip.hits.len()
                        ))),
                )
                // Why it works, while it's heard.
                .when(on && !flip.why.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(flip.why.clone()),
                    )
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _, _, cx| ws.hear_flip(i, cx)),
                );
            // Just written: it fades in, rising into place.
            let new = f
                .arrived
                .get(i)
                .is_some_and(|a| a.elapsed() < Duration::from_millis(400));
            out.push(match new {
                true => div().child(row.with_animation(
                    gpui::ElementId::Name(format!("flip-in-{i}-{}", flip.name).into()),
                    Animation::new(Duration::from_millis(320)).with_easing(gpui::ease_out_quint()),
                    |d, t| d.opacity(t).mt(px((1.0 - t) * 8.0)),
                )),
                false => row,
            });
        }
        out
    }
}

/// Shows the Lyrics panel's page `t`.
fn pick(
    t: LyricsTab,
    cx: &mut Context<Workspace>,
) -> impl Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    cx.listener(move |ws, _: &gpui::MouseDownEvent, _, cx| {
        ws.flips.tab = t;
        cx.notify();
    })
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

/// Under the list's header while the engine works: how many of the flips are written, or (not
/// writing yet) a light going across. A hairline otherwise.
fn progress_line(working: Option<(Stage, usize)>) -> Div {
    let line = div()
        .relative()
        .h(px(2.))
        .overflow_hidden()
        .bg(rgb(LINE_SOFT));
    match working {
        None => line,
        Some((Stage::Writing, got)) => line.child(
            div()
                .h_full()
                .bg(rgb(AGENT))
                .w(relative((got as f32 + 0.5) / ASK_FOR as f32)),
        ),
        Some(_) => line.child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(relative(0.3))
                .bg(rgb(AGENT))
                .with_animation(
                    "flips-going",
                    Animation::new(Duration::from_millis(1300)).repeat(),
                    |d, t| d.left(relative(t * 1.3 - 0.3)),
                ),
        ),
    }
}

/// The next flip, still being written: a row in its shape, breathing.
fn ghost() -> Div {
    let bar = |w: f32, h: f32| {
        div()
            .h(px(h))
            .w(relative(w))
            .rounded(px(3.))
            .bg(rgb(RAISED))
    };
    div().child(
        div()
            .px(px(12.))
            .py(px(8.))
            .flex()
            .flex_col()
            .gap(px(7.))
            .border_t_1()
            .border_color(rgb(LINE_SOFT))
            .child(bar(0.55, 10.))
            .child(div().h(px(26.)).rounded(px(4.)).bg(rgb(WELL)))
            .child(bar(0.35, 8.))
            .with_animation(
                "flip-ghost",
                Animation::new(Duration::from_millis(1400))
                    .repeat()
                    .with_easing(gpui::pulsating_between(0.35, 0.8)),
                |d, t| d.opacity(t),
            ),
    )
}
