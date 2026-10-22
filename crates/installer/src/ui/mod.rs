//! The installer's window: Pina Studio on the left (its icon over a curve that moves, as its EQ's does),
//! and on the right what's about to happen, what is happening, and what's done. Install, update
//! or reinstall; the `pina` command and the example songs if wanted; Pina Studio opened at the end,
//! and the disk image put away. Or Pina Studio taken out again.
//!
//! Everything moves by the clock, a frame at a time: the curve draws itself in and breathes, the
//! icon lands, words and rows rise in turn; while it works the curve quickens, steps tick
//! themselves off and the bar glides; when it's done a check draws itself, a ring goes out from
//! the icon and notes float up off the curve.

mod motion;
mod screens;
mod widgets;

pub(crate) use motion::*;
pub use widgets::*;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    Context, Div, FontWeight, Hsla, Image, ImageFormat, IntoElement, MouseButton, ParentElement,
    PathBuilder, Render, SharedString, Styled, Window, canvas, div, img, point, prelude::*, px,
    rgb,
};

use crate::install::{self, Over, Plan};

pub const BG: u32 = 0x0c0c0f;
pub const SURFACE: u32 = 0x131317;
pub const CARD: u32 = 0x1a1a1f;
pub const FIELD: u32 = 0x24242b;
pub const RAISED: u32 = 0x2b2b33;
pub const LINE: u32 = 0x26262d;
pub const TEXT: u32 = 0xf4f4f6;
pub const TEXT_2: u32 = 0xd4d4da;
pub const TEXT_3: u32 = 0x8a8a95;
pub const ACCENT: u32 = 0x5b8cff;
pub const MINT: u32 = 0x3ddc97;
pub const RECORD: u32 = 0xff6b6b;
pub const CURVE: u32 = 0xffd166;
pub const SANS: &str = "Geist";
pub const MONO: &str = "Geist Mono";

/// Where the installer is.
#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    /// What's about to happen, and the button that does it.
    Ready,
    /// Pina Studio is open: it has to close first (and ask to save its song).
    Waiting,
    Working,
    Done,
    Failed(String),
    /// Taking it out: asked first, then done.
    Removing,
    Removed,
}

/// One thing the installer does, and how it went.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Waiting,
    Running,
    Done(String),
    Skipped(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Job {
    Copy,
    Register,
    Command,
    Examples,
}

impl Job {
    fn title(self) -> &'static str {
        match self {
            Job::Copy => "Copy Pina Studio into Applications",
            Job::Register => "Let the Finder, Spotlight and Dock know",
            Job::Command => "The pina command",
            Job::Examples => "Example songs",
        }
    }
}

pub struct Installer {
    pub plan: Plan,
    pub version: String,
    pub archs: String,
    pub over: Over,
    pub want_command: bool,
    pub want_examples: bool,
    pub and_settings: bool,
    pub stage: Stage,
    pub steps: Vec<(Job, State)>,
    icon: Arc<Image>,
    /// When the window opened, the stage began, and each step finished: what the animations
    /// count from. The bar's width as shown, and how lively the curve is, each easing to where
    /// they're going.
    started: Instant,
    stage_at: Instant,
    step_done: Vec<Option<Instant>>,
    shown: f32,
    energy: f32,
    calm: f32,
    phase: f32,
    last: f32,
}

/// What the payload runs on: "Apple silicon and Intel", or one of them.
fn archs(app: &std::path::Path) -> String {
    let out = std::process::Command::new("lipo")
        .arg("-archs")
        .arg(app.join("Contents/MacOS/pina"))
        .output();
    let archs = out
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    match (archs.contains("arm64"), archs.contains("x86_64")) {
        (true, true) => "Apple silicon and Intel".into(),
        (true, false) => "Apple silicon".into(),
        (false, true) => "Intel".into(),
        _ => "this Mac".into(),
    }
}

impl Installer {
    pub fn new(plan: Plan) -> Installer {
        let version = install::version(&plan.payload).unwrap_or_else(|| "?".into());
        let over = install::over(install::version(&plan.app()), &version);
        Installer {
            archs: archs(&plan.payload),
            want_examples: plan.examples.is_some() && !plan.songs.exists(),
            want_command: true,
            and_settings: false,
            stage: Stage::Ready,
            steps: Vec::new(),
            icon: Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../../../app/icon/pina.svg").to_vec(),
            )),
            started: Instant::now(),
            stage_at: Instant::now(),
            step_done: Vec::new(),
            shown: 0.0,
            energy: 1.0,
            calm: 0.0,
            phase: 0.0,
            last: 0.0,
            version,
            over,
            plan,
        }
    }

    /// For pictures: the window open a while, this stage `seconds` in, its steps finished a
    /// moment ago, the bar where it's going.
    #[cfg(test)]
    pub fn wind(&mut self, seconds: f32) {
        let ago = |s: f32| Instant::now() - Duration::from_secs_f32(s);
        self.started = ago(6.0);
        self.stage_at = ago(seconds);
        self.last = 6.0;
        self.step_done = self
            .steps
            .iter()
            .map(|s| matches!(s.1, State::Done(_) | State::Skipped(_)).then(|| ago(2.0)))
            .collect();
        let done = self.step_done.iter().flatten().count() as f32;
        self.shown = match self.stage {
            Stage::Done => 1.0,
            _ => (done + 0.35) / self.steps.len().max(1) as f32,
        };
        (self.energy, self.calm) = match self.stage {
            Stage::Working => (1.55, 0.0),
            Stage::Done | Stage::Removed => (1.0, 1.0),
            _ => (1.0, 0.0),
        };
        self.phase = 2.4;
    }

    /// A new stage: its animations start over.
    pub fn set_stage(&mut self, stage: Stage) {
        if stage != self.stage {
            self.stage = stage;
            self.stage_at = Instant::now();
        }
    }

    /// A step's state; finished, when (its tick draws itself from then).
    pub fn set_step(&mut self, i: usize, state: State) {
        if self.step_done.len() < self.steps.len() {
            self.step_done.resize(self.steps.len(), None);
        }
        if matches!(state, State::Done(_) | State::Skipped(_)) {
            self.step_done[i] = Some(Instant::now());
        }
        self.steps[i].1 = state;
    }

    /// Install: once Pina Studio's closed (if it's open), each step in turn, off this thread.
    fn start(&mut self, cx: &mut Context<Self>) {
        let mut jobs = vec![Job::Copy, Job::Register];
        if self.want_command {
            jobs.push(Job::Command);
        }
        if self.want_examples && self.plan.examples.is_some() {
            jobs.push(Job::Examples);
        }
        self.steps = jobs.into_iter().map(|j| (j, State::Waiting)).collect();
        self.step_done = vec![None; self.steps.len()];
        self.shown = 0.0;
        self.set_stage(match install::running(&self.plan.app()) {
            true => Stage::Waiting,
            false => Stage::Working,
        });
        cx.notify();
        let plan = self.plan.clone();
        cx.spawn(async move |this, cx| {
            // Pina Studio open: wait for it to close (it asks to save its song first).
            loop {
                let app = plan.app();
                let open = cx
                    .background_executor()
                    .spawn(async move { install::running(&app) })
                    .await;
                if !open {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(600))
                    .await;
            }
            let jobs: Vec<Job> = this
                .update(cx, |v, cx| {
                    v.set_stage(Stage::Working);
                    cx.notify();
                    v.steps.iter().map(|s| s.0).collect()
                })
                .unwrap_or_default();
            for (i, job) in jobs.into_iter().enumerate() {
                let _ = this.update(cx, |v, cx| {
                    v.set_step(i, State::Running);
                    cx.notify();
                });
                let plan = plan.clone();
                let (state, fatal) = cx
                    .background_executor()
                    .spawn(async move { run(job, &plan) })
                    .await;
                let go_on = this
                    .update(cx, |v, cx| {
                        v.set_step(i, state);
                        if let Some(why) = fatal.clone() {
                            v.set_stage(Stage::Failed(why));
                        }
                        cx.notify();
                        fatal.is_none()
                    })
                    .unwrap_or(false);
                if !go_on {
                    return;
                }
                // Each step seen ticking off, not a blur.
                cx.background_executor()
                    .timer(Duration::from_millis(420))
                    .await;
            }
            let _ = this.update(cx, |v, cx| {
                v.set_stage(Stage::Done);
                cx.notify();
            });
        })
        .detach();
    }

    fn remove(&mut self, cx: &mut Context<Self>) {
        let plan = self.plan.clone();
        let and_settings = self.and_settings;
        self.set_stage(Stage::Working);
        self.steps = vec![(Job::Copy, State::Running)];
        self.step_done = vec![None];
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { install::uninstall(&plan, and_settings) })
                .await;
            let _ = this.update(cx, |v, cx| {
                v.set_stage(match result {
                    Ok(()) => Stage::Removed,
                    Err(e) => Stage::Failed(e),
                });
                cx.notify();
            });
        })
        .detach();
    }
}

/// One step, done: how it went, and whether the rest can't go on.
fn run(job: Job, plan: &Plan) -> (State, Option<String>) {
    match job {
        Job::Copy => match install::copy_app(plan) {
            Ok(app) => (State::Done(app.display().to_string()), None),
            Err(e) => (State::Skipped(e.clone()), Some(e)),
        },
        Job::Register => {
            install::register(&plan.app());
            (State::Done("Ready to open from anywhere".into()), None)
        }
        Job::Command => match install::command_line(&plan.app(), &plan.bin) {
            Ok(link) => (State::Done(format!("{}", link.display())), None),
            Err(e) => (State::Skipped(e), None),
        },
        Job::Examples => match plan
            .examples
            .as_deref()
            .map(|from| install::examples(from, &plan.songs))
        {
            Some(Ok(0)) => (State::Skipped("Already there".into()), None),
            Some(Ok(n)) => (
                State::Done(format!(
                    "{n} in {}",
                    plan.songs.display().to_string().replace(&home(), "~")
                )),
                None,
            ),
            Some(Err(e)) => (State::Skipped(e), None),
            None => (State::Skipped("None in this installer".into()), None),
        },
    }
}

impl Render for Installer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = self.started.elapsed().as_secs_f32();
        let dt = (now - self.last).clamp(0.0, 0.1);
        self.last = now;
        // One frame after another while the window's up: everything here moves by the clock.
        window.request_animation_frame();
        // The curve quickens while it works, settles when it's over; the bar glides on.
        let working = matches!(self.stage, Stage::Working | Stage::Waiting);
        let (energy, calm) = match self.stage {
            Stage::Working => (1.55, 0.0),
            Stage::Waiting => (0.8, 0.0),
            Stage::Done | Stage::Removed => (1.0, 1.0),
            _ => (1.0, 0.0),
        };
        let k = (dt * 4.0).min(1.0);
        self.energy += (energy - self.energy) * k;
        self.calm += (calm - self.calm) * (dt * 1.6).min(1.0);
        self.phase += dt * (0.6 + 0.9 * self.energy);
        let finished = self
            .steps
            .iter()
            .filter(|s| matches!(s.1, State::Done(_) | State::Skipped(_)))
            .count() as f32;
        let target = match self.stage {
            Stage::Done => 1.0,
            _ if self.steps.is_empty() => 0.0,
            // Partway into the step that's running, so the bar never stands still.
            _ => (finished + if working { 0.35 } else { 0.0 }) / self.steps.len() as f32,
        };
        self.shown += (target - self.shown) * (dt * 5.0).min(1.0);
        let party = (self.stage == Stage::Done).then(|| self.stage_at.elapsed().as_secs_f32());
        let gone = (self.stage == Stage::Removed).then(|| self.stage_at.elapsed().as_secs_f32());
        // Pina Studio's entrance: the curve draws in, the icon lands, the words rise.
        let reveal = appear(now, 0.0, 1.1);
        let landing = ease_back(((now - 0.12) / 0.75).clamp(0.0, 1.0));
        let pop = party.map_or(0.0, |c| {
            0.07 * (std::f32::consts::PI * (c / 0.45).clamp(0.0, 1.0)).sin()
        });
        let fade = gone.map_or(1.0, |g| 1.0 - 0.65 * ease_out(g / 0.6));
        let icon = 132.0 * (0.78 + 0.22 * landing) * (1.0 + pop) * (1.0 - 0.1 * (1.0 - fade));
        let motion = Motion {
            phase: self.phase,
            reveal,
            energy: self.energy,
            calm: self.calm,
            party,
        };
        let right = match self.stage {
            Stage::Ready => self.ready(cx),
            Stage::Removing | Stage::Removed => self.removing(cx),
            _ => self.progress(now, cx),
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(BG))
            .font_family(SANS)
            .text_color(rgb(TEXT))
            .child(
                div()
                    .w(px(300.))
                    .h_full()
                    .flex_none()
                    .relative()
                    .overflow_hidden()
                    .bg(rgb(SURFACE))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .absolute()
                            .left(px(0.))
                            .right(px(0.))
                            .bottom(px(0.))
                            .h(px(190.))
                            .child(curve(motion)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(0.))
                            .right(px(0.))
                            .top(px(14.))
                            .h(px(280.))
                            .child(halo(now, reveal * fade, party)),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .pt(px(78.))
                            .gap(px(10.))
                            .child(
                                div()
                                    .size(px(132.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        img(self.icon.clone())
                                            .size(px(icon))
                                            .opacity(appear(now, 0.05, 0.5) * fade),
                                    ),
                            )
                            .child(rise(
                                div()
                                    .text_size(px(28.))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Pina Studio"),
                                appear(now, 0.35, 0.6),
                            ))
                            .child(rise(
                                div()
                                    .px(px(36.))
                                    .text_center()
                                    .text_size(px(12.5))
                                    .line_height(px(18.))
                                    .text_color(rgb(TEXT_3))
                                    .child(
                                        "Arrange, mix and produce, with agents as collaborators.",
                                    ),
                                appear(now, 0.48, 0.6),
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .pt(px(52.))
                    .px(px(36.))
                    .pb(px(30.))
                    .child(right.min_w(px(0.))),
            )
    }
}
