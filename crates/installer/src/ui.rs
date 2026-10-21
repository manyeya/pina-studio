//! The installer's window: Pina Studio on the left (its icon over a curve that moves, as its EQ's does),
//! and on the right what's about to happen, what is happening, and what's done. Install, update
//! or reinstall; the `pina` command and the example songs if wanted; Pina Studio opened at the end,
//! and the disk image put away. Or Pina Studio taken out again.
//!
//! Everything moves by the clock, a frame at a time: the curve draws itself in and breathes, the
//! icon lands, words and rows rise in turn; while it works the curve quickens, steps tick
//! themselves off and the bar glides; when it's done a check draws itself, a ring goes out from
//! the icon and notes float up off the curve.

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

pub fn alpha(c: u32, a: f32) -> Hsla {
    Hsla::from(rgb(c)).opacity(a)
}

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
                include_bytes!("../../app/icon/pina.svg").to_vec(),
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

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// A button: the main one filled, the others quiet.
fn button(label: impl Into<SharedString>, main: bool) -> Div {
    div()
        .h(px(38.))
        .px(px(if main { 22. } else { 16. }))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(9.))
        .cursor_pointer()
        .text_size(px(13.5))
        .font_weight(FontWeight::SEMIBOLD)
        .when(main, |d| {
            d.bg(rgb(ACCENT))
                .text_color(rgb(0xffffff))
                .hover(|d| d.bg(rgb(0x6d99ff)))
        })
        .when(!main, |d| {
            d.bg(rgb(FIELD))
                .text_color(rgb(TEXT_2))
                .hover(|d| d.bg(rgb(RAISED)).text_color(rgb(TEXT)))
        })
        .child(label.into())
}

/// An on/off switch.
fn switch(on: bool) -> Div {
    div()
        .relative()
        .w(px(34.))
        .h(px(20.))
        .flex_none()
        .rounded(px(10.))
        .bg(rgb(if on { ACCENT } else { RAISED }))
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(px(if on { 16. } else { 2. }))
                .size(px(16.))
                .rounded(px(8.))
                .bg(rgb(if on { 0xffffff } else { TEXT_3 })),
        )
}

/// Eased: quick, then settling.
fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

/// Eased past where it's going, and back: a landing.
fn ease_back(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let (c1, c3) = (1.70158, 2.70158);
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

/// How far in something's entrance is, `since` seconds on, starting `delay` in, over `dur`.
fn appear(since: f32, delay: f32, dur: f32) -> f32 {
    ease_out((since - delay) / dur)
}

/// Rising in: faded and a little lower until it's there.
fn rise(el: Div, p: f32) -> Div {
    el.opacity(p).relative().top(px((1.0 - p) * 14.0))
}

/// A number from 0 to 1, the same every time for `i`: where a note starts, how fast it goes.
fn hashed(i: usize, k: f32) -> f32 {
    let v = ((i as f32 + 1.0) * 12.9898 + k * 78.233).sin() * 43_758.547;
    v - v.floor()
}

/// What the curve's doing: how far it's drawn in, how lively, how calm (settling flat when
/// it's over), and the seconds since Pina Studio was ready (notes float up off it then).
#[derive(Clone, Copy)]
struct Motion {
    phase: f32,
    reveal: f32,
    energy: f32,
    calm: f32,
    party: Option<f32>,
}

/// The curve under Pina Studio's icon: a few bells and shelves breathing over a moving spectrum, as an
/// EQ's curve over a song does.
fn curve(m: Motion) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
            let o = b.origin;
            let at = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
            for k in 1..6 {
                let x = w * k as f32 / 6.0;
                let mut l = PathBuilder::stroke(px(1.));
                l.move_to(at(x, 0.0));
                l.line_to(at(x, h));
                if let Ok(p) = l.build() {
                    window.paint_path(p, alpha(0xffffff, 0.035 * m.reveal));
                }
            }
            let t = m.phase;
            let amp = h * 0.22 * m.energy * (1.0 - 0.75 * m.calm);
            let y = |x: f32| {
                let u = x / w;
                let bell = |c: f32, width: f32, g: f32| g * (-((u - c) / width).powi(2)).exp();
                h * 0.55
                    - amp
                        * (bell(0.22 + 0.04 * (t * 0.7).sin(), 0.09, 0.9)
                            + bell(0.58, 0.12, -0.6 + 0.25 * (t * 0.9).sin())
                            + bell(0.86 + 0.03 * t.cos(), 0.07, 0.7))
                    - amp * 0.35 * (1.0 / (1.0 + (-(u - 0.95) * 30.0).exp()))
            };
            let n = 120;
            let drawn = ((n as f32) * m.reveal).round() as usize;
            // The spectrum under it, moving faster, rising in.
            let mut fill = PathBuilder::fill();
            fill.move_to(at(0.0, h));
            for i in 0..=n {
                let x = w * i as f32 / n as f32;
                let u = x / w;
                let lift = (1.0 - m.reveal) * h * 0.4;
                let s = h * (0.72 - 0.18 * (1.0 - (u - 0.35).abs() * 1.6).max(0.0))
                    + 6.0 * m.energy * ((u * 31.0 + t * 2.3).sin() * (u * 17.0 - t * 1.7).cos())
                    + lift;
                fill.line_to(at(x, s.min(h)));
            }
            fill.line_to(at(w, h));
            fill.close();
            if let Ok(p) = fill.build() {
                window.paint_path(p, alpha(ACCENT, 0.10 * m.reveal));
            }
            if drawn >= 1 {
                // A glow under the line, then the line, drawing itself in from the left.
                for (width, a) in [(7.0, 0.10), (2.0, 1.0)] {
                    let mut line = PathBuilder::stroke(px(width));
                    for i in 0..=drawn {
                        let x = w * i as f32 / n as f32;
                        let p = at(x, y(x));
                        if i == 0 {
                            line.move_to(p);
                        } else {
                            line.line_to(p);
                        }
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, alpha(CURVE, a));
                    }
                }
                // Its leading point while it draws in.
                if m.reveal < 1.0 {
                    let x = w * drawn as f32 / n as f32;
                    if let Some(p) = dot(at(x, y(x)), 3.5) {
                        window.paint_path(p, rgb(0xffffff));
                    }
                }
            }
            // Ready: sparks and notes float up off the curve.
            if let Some(c) = m.party {
                for i in 0..34 {
                    let age = c - hashed(i, 1.0) * 0.9;
                    let life = 2.2 + hashed(i, 2.0) * 0.8;
                    if !(0.0..life).contains(&age) {
                        continue;
                    }
                    let x0 = w * hashed(i, 3.0);
                    let rise = age * (46.0 + hashed(i, 4.0) * 80.0);
                    let x = x0 + (age * 2.6 + i as f32).sin() * 9.0;
                    let fade = (1.0 - age / life) * (age * 6.0).min(1.0);
                    let color = [CURVE, MINT, ACCENT, 0xffffff][i % 4];
                    if let Some(p) = dot(at(x, y(x0) - rise), 1.4 + hashed(i, 5.0) * 2.4) {
                        window.paint_path(p, alpha(color, fade));
                    }
                }
            }
        },
    )
    .size_full()
}

/// A filled circle.
fn dot(c: gpui::Point<gpui::Pixels>, r: f32) -> Option<gpui::Path<gpui::Pixels>> {
    let mut p = PathBuilder::fill();
    p.move_to(point(c.x + px(r), c.y));
    p.arc_to(
        point(px(r), px(r)),
        px(0.),
        false,
        false,
        point(c.x - px(r), c.y),
    );
    p.arc_to(
        point(px(r), px(r)),
        px(0.),
        false,
        false,
        point(c.x + px(r), c.y),
    );
    p.close();
    p.build().ok()
}

/// An arc round `c`, from one angle to another (degrees, clockwise from the right).
fn arc(
    c: gpui::Point<gpui::Pixels>,
    r: f32,
    from: f32,
    to: f32,
    width: f32,
) -> Option<gpui::Path<gpui::Pixels>> {
    if (to - from).abs() < 0.5 {
        return None;
    }
    let mut p = PathBuilder::stroke(px(width));
    let steps = (((to - from).abs() / 8.0).ceil() as usize).max(2);
    for k in 0..=steps {
        let a = (from + (to - from) * k as f32 / steps as f32).to_radians();
        let q = point(c.x + px(r * a.cos()), c.y + px(r * a.sin()));
        if k == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p.build().ok()
}

/// A tick, drawn as far as `p` (0 to 1) along its two strokes.
fn tick(
    c: gpui::Point<gpui::Pixels>,
    size: f32,
    width: f32,
    p: f32,
) -> Option<gpui::Path<gpui::Pixels>> {
    if p <= 0.0 {
        return None;
    }
    let pts = [(-0.45, 0.05), (-0.15, 0.35), (0.45, -0.35)].map(|(x, y)| (x * size, y * size));
    let lens = [
        ((pts[1].0 - pts[0].0).powi(2) + (pts[1].1 - pts[0].1).powi(2)).sqrt(),
        ((pts[2].0 - pts[1].0).powi(2) + (pts[2].1 - pts[1].1).powi(2)).sqrt(),
    ];
    let mut left = p.min(1.0) * (lens[0] + lens[1]);
    let at = |q: (f32, f32)| point(c.x + px(q.0), c.y + px(q.1));
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(at(pts[0]));
    for k in 0..2 {
        let f = (left / lens[k]).min(1.0);
        let to = (
            pts[k].0 + (pts[k + 1].0 - pts[k].0) * f,
            pts[k].1 + (pts[k + 1].1 - pts[k].1) * f,
        );
        path.line_to(at(to));
        left -= lens[k];
        if left <= 0.0 {
            break;
        }
    }
    path.build().ok()
}

/// A step's mark: a faint ring waiting, a turning arc, a tick drawing itself (with its ring
/// going round), or a dash.
fn mark(state: &State, t: f32, since: Option<f32>) -> impl IntoElement {
    let state = state.clone();
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let r = 8.0;
            let s = since.unwrap_or(10.0);
            match &state {
                State::Waiting => {
                    if let Some(p) = arc(c, r, 0.0, 360.0, 1.5) {
                        window.paint_path(p, rgb(RAISED));
                    }
                }
                State::Running => {
                    if let Some(p) = arc(c, r, 0.0, 360.0, 2.0) {
                        window.paint_path(p, rgb(RAISED));
                    }
                    let a = t * 400.0;
                    let len = 70.0 + 50.0 * (t * 3.0).sin();
                    if let Some(p) = arc(c, r, a, a + len, 2.0) {
                        window.paint_path(p, rgb(ACCENT));
                    }
                }
                State::Done(_) => {
                    let ring = ease_out(s / 0.35);
                    if let Some(p) = dot(c, r * ease_back(s / 0.4).max(0.0)) {
                        window.paint_path(p, alpha(MINT, 0.18));
                    }
                    if let Some(p) = arc(c, r, -90.0, -90.0 + 360.0 * ring, 2.0) {
                        window.paint_path(p, rgb(MINT));
                    }
                    if let Some(p) = tick(c, 10.0, 2.0, ease_out((s - 0.15) / 0.3)) {
                        window.paint_path(p, rgb(MINT));
                    }
                }
                State::Skipped(_) => {
                    let p = ease_out(s / 0.3);
                    let mut dash = PathBuilder::stroke(px(2.));
                    dash.move_to(point(c.x - px(3.5 * p), c.y));
                    dash.line_to(point(c.x + px(3.5 * p), c.y));
                    if let Some(q) = arc(c, r, 0.0, 360.0, 1.5) {
                        window.paint_path(q, rgb(TEXT_3));
                    }
                    if let Ok(q) = dash.build() {
                        window.paint_path(q, rgb(TEXT_3));
                    }
                }
            }
        },
    )
    .size(px(20.))
    .flex_none()
}

/// The big check when it's done: its ring going round, filling, the tick drawn last, a pop.
fn big_check(since: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let pop =
                1.0 + 0.08 * (std::f32::consts::PI * ((since - 0.55) / 0.35).clamp(0.0, 1.0)).sin();
            let r = 22.0 * pop;
            if let Some(p) = dot(c, r * ease_out(since / 0.5)) {
                window.paint_path(p, alpha(MINT, 0.16));
            }
            if let Some(p) = arc(c, r, -90.0, -90.0 + 360.0 * ease_out(since / 0.5), 2.5) {
                window.paint_path(p, rgb(MINT));
            }
            if let Some(p) = tick(c, 24.0 * pop, 3.0, ease_out((since - 0.3) / 0.35)) {
                window.paint_path(p, rgb(MINT));
            }
        },
    )
    .size(px(56.))
    .flex_none()
}

/// Behind the icon: a soft glow that breathes; when it's done, rings going out from it.
fn halo(t: f32, open: f32, party: Option<f32>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let breathe = 1.0 + 0.05 * (t * 1.3).sin();
            for k in 0..7 {
                let r = (54.0 + k as f32 * 13.0) * breathe * (0.6 + 0.4 * open);
                if let Some(p) = dot(c, r) {
                    window.paint_path(p, alpha(ACCENT, 0.035 * open));
                }
            }
            if let Some(c0) = party {
                for k in 0..3 {
                    let age = c0 - k as f32 * 0.28;
                    if !(0.0..1.4).contains(&age) {
                        continue;
                    }
                    let r = 72.0 + ease_out(age / 1.4) * 130.0;
                    if let Some(p) = arc(c, r, 0.0, 360.0, 2.0) {
                        window.paint_path(p, alpha(MINT, 0.5 * (1.0 - age / 1.4)));
                    }
                }
            }
        },
    )
    .size_full()
}

impl Installer {
    /// The right side before anything's done: what installing means here, and the button.
    fn ready(&self, cx: &mut Context<Self>) -> Div {
        let (title, action, note) = match &self.over {
            Over::Nothing => (
                "Install Pina Studio".to_string(),
                "Install".to_string(),
                None,
            ),
            Over::Older(old) => (
                "Update Pina Studio".to_string(),
                format!("Update to {}", self.version),
                Some(format!("Replaces {old}. Your songs and settings stay.")),
            ),
            Over::Same => (
                "Pina Studio is installed".to_string(),
                "Reinstall".to_string(),
                Some(format!(
                    "{} is here already. Your songs and settings stay.",
                    self.version
                )),
            ),
            Over::Newer(newer) => (
                "A newer Pina Studio is installed".to_string(),
                format!("Replace with {}", self.version),
                Some(format!("{newer} is here: this one's older.")),
            ),
        };
        let installed = self.over != Over::Nothing;
        let where_to = self.plan.apps.display().to_string().replace(&home(), "~");
        // In turn: the words, each row, the button. The first time, after Pina Studio's own entrance.
        let since = self.stage_at.elapsed().as_secs_f32();
        let lead = if self.started == self.stage_at {
            0.45
        } else {
            0.0
        };
        let at = |k: f32| appear(since, lead + k, 0.55);
        let row = |title: &'static str, about: String, on: Option<bool>, sel: &'static str| {
            div()
                .min_h(px(58.))
                .px(px(14.))
                .py(px(10.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(10.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(LINE))
                .debug_selector(move || sel.into())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(px(13.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(16.))
                                .text_color(rgb(TEXT_3))
                                .overflow_hidden()
                                .child(about),
                        ),
                )
                .child(match on {
                    Some(on) => switch(on),
                    None => div()
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .child("Always"),
                })
                .when(on.is_some(), |d| {
                    d.cursor_pointer().hover(|d| d.bg(rgb(0x1f1f25)))
                })
        };
        let examples = self
            .plan
            .examples
            .as_ref()
            .and_then(|e| std::fs::read_dir(e).ok())
            .map(|d| {
                d.flatten()
                    .filter(|e| e.path().join("project.json").is_file())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|s| !s.is_empty());
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(rise(
                        div()
                            .text_size(px(24.))
                            .font_weight(FontWeight::BOLD)
                            .child(title),
                        at(0.0),
                    ))
                    .child(rise(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(TEXT_3))
                            .child(match note {
                                Some(n) => n,
                                None => format!(
                                    "Version {} · for {}",
                                    self.version, self.archs
                                ),
                            }),
                        at(0.07),
                    )),
            )
            .child(
                div()
                    .mt(px(6.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(rise(
                        row("Pina Studio", format!("Into {where_to}"), None, "install-app"),
                        at(0.16),
                    ))
                    .child(rise(
                        row(
                            "The pina command",
                            "Use Pina Studio from Terminal and scripts: pina new, pina bounce… May ask for your password.".into(),
                            Some(self.want_command),
                            "install-command",
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _, _, cx| {
                                v.want_command = !v.want_command;
                                cx.notify();
                            }),
                        ),
                        at(0.23),
                    ))
                    .when_some(examples, |d, names| {
                        d.child(rise(
                            row(
                                "Example songs",
                                format!("{names}, to open and take apart."),
                                Some(self.want_examples),
                                "install-examples",
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|v, _, _, cx| {
                                    v.want_examples = !v.want_examples;
                                    cx.notify();
                                }),
                            ),
                            at(0.30),
                        ))
                    }),
            )
            .child(div().flex_1())
            .child(rise(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .when(installed, |d| {
                        d.child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgb(TEXT_3))
                                .cursor_pointer()
                                .hover(|d| d.text_color(rgb(TEXT_2)))
                                .debug_selector(|| "install-uninstall".into())
                                .child("Uninstall…")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|v, _, _, cx| {
                                        v.set_stage(Stage::Removing);
                                        cx.notify();
                                    }),
                                ),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        button(action, true)
                            .debug_selector(|| "install-go".into())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|v, _, _, cx| v.start(cx)),
                            ),
                    ),
                at(0.40),
            ))
    }

    /// The right side while it works, and once it's done or stuck.
    fn progress(&self, t: f32, cx: &mut Context<Self>) -> Div {
        let (title, sub) = match &self.stage {
            Stage::Waiting => (
                "Quit Pina Studio to go on".to_string(),
                "It's open. Quit it (it asks to save your song first); installing goes on by itself."
                    .to_string(),
            ),
            Stage::Working => ("Installing…".to_string(), format!("Pina Studio {}", self.version)),
            Stage::Done => (
                "Pina Studio is ready".to_string(),
                "Everything's in place. Open it, and start a song.".to_string(),
            ),
            Stage::Failed(why) => ("That didn't work".to_string(), why.clone()),
            _ => (String::new(), String::new()),
        };
        let done = self.stage == Stage::Done;
        let working = matches!(self.stage, Stage::Working | Stage::Waiting);
        let since = self.stage_at.elapsed().as_secs_f32();
        let at = |k: f32| appear(since, k, 0.5);
        let steps = self.steps.iter().enumerate().map(|(i, (job, state))| {
            let detail = match state {
                State::Done(d) | State::Skipped(d) => Some(d.clone()),
                _ => None,
            };
            let finished = self
                .step_done
                .get(i)
                .copied()
                .flatten()
                .map(|at| at.elapsed().as_secs_f32());
            // The step running glows a little; its details rise in when it's done.
            let running = *state == State::Running;
            rise(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .py(px(7.))
                    .px(px(8.))
                    .rounded(px(8.))
                    .when(running, |d| {
                        d.bg(alpha(ACCENT, 0.06 + 0.04 * (t * 3.0).sin().abs()))
                    })
                    .child(div().pt(px(1.)).child(mark(state, t, finished)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .text_color(rgb(match state {
                                        State::Waiting => TEXT_3,
                                        _ => TEXT,
                                    }))
                                    .child(job.title()),
                            )
                            .when_some(detail, |d, text| {
                                let p = appear(finished.unwrap_or(1.0), 0.1, 0.35);
                                d.child(
                                    div()
                                        .opacity(p)
                                        .text_size(px(11.5))
                                        .font_family(MONO)
                                        .text_color(rgb(TEXT_3))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(text),
                                )
                            }),
                    ),
                at(0.12 + i as f32 * 0.07),
            )
        });
        let bar_w = 404.0;
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .when(done, |d| d.child(big_check(since)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(rise(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .text_size(px(24.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(title)
                                    .when(self.stage == Stage::Waiting, |d| {
                                        d.child(
                                            div()
                                                .size(px(8.))
                                                .rounded(px(4.))
                                                .bg(rgb(CURVE))
                                                .opacity(0.35 + 0.65 * (t * 3.0).sin().abs()),
                                        )
                                    }),
                                at(if done { 0.35 } else { 0.0 }),
                            ))
                            .child(rise(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(18.))
                                    .text_color(rgb(if matches!(self.stage, Stage::Failed(_)) {
                                        RECORD
                                    } else {
                                        TEXT_3
                                    }))
                                    .child(sub),
                                at(if done { 0.45 } else { 0.06 }),
                            )),
                    ),
            )
            // The bar: gliding to where it's got to, a shine going along it while it works.
            .child(
                div()
                    .relative()
                    .h(px(4.))
                    .rounded(px(2.))
                    .overflow_hidden()
                    .bg(rgb(RAISED))
                    .child(
                        div()
                            .h(px(4.))
                            .rounded(px(2.))
                            .bg(rgb(if done { MINT } else { ACCENT }))
                            .w(gpui::relative(self.shown.clamp(0.0, 1.0))),
                    )
                    .when(working, |d| {
                        let x = ((t * 0.7) % 1.3 - 0.15) * bar_w;
                        d.child(
                            div()
                                .absolute()
                                .top(px(0.))
                                .left(px(x))
                                .w(px(70.))
                                .h(px(4.))
                                .rounded(px(2.))
                                .bg(alpha(0xffffff, 0.35)),
                        )
                    }),
            )
            .child(div().flex().flex_col().gap(px(2.)).children(steps))
            .child(div().flex_1())
            .child(rise(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(10.))
                    .when(done || matches!(self.stage, Stage::Failed(_)), |d| {
                        d.child(
                            button("Close", false)
                                .debug_selector(|| "install-close".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|_, _, _, cx| cx.quit()),
                                ),
                        )
                    })
                    .when(matches!(self.stage, Stage::Failed(_)), |d| {
                        d.child(button("Try again", true).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _, _, cx| v.start(cx)),
                        ))
                    })
                    .when(done, |d| {
                        d.child(
                            button("Open Pina Studio", true)
                                .debug_selector(|| "install-open".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|v, _, _, cx| {
                                        install::launch(&v.plan.app());
                                        install::eject_later();
                                        cx.quit();
                                    }),
                                ),
                        )
                    }),
                if working { 1.0 } else { at(0.6) },
            ))
    }

    /// Taking Pina Studio out: what goes and what stays, asked first; then done.
    fn removing(&self, cx: &mut Context<Self>) -> Div {
        let removed = self.stage == Stage::Removed;
        let since = self.stage_at.elapsed().as_secs_f32();
        let at = |k: f32| appear(since, k, 0.5);
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(rise(
                div()
                    .text_size(px(24.))
                    .font_weight(FontWeight::BOLD)
                    .child(if removed {
                        "Pina Studio is uninstalled"
                    } else {
                        "Uninstall Pina Studio?"
                    }),
                at(0.0),
            ))
            .child(rise(
                div()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_color(rgb(TEXT_3))
                    .child(match removed {
                        true => "It's in the Trash. Your songs are where they were.",
                        false => "Pina Studio goes to the Trash, and its pina command with it. Your songs stay in ~/Music/Pina Studio.",
                    }),
                at(0.07),
            ))
            .when(!removed, |d| {
                d.child(rise(
                    div()
                        .mt(px(4.))
                        .min_h(px(58.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .rounded(px(10.))
                        .bg(rgb(CARD))
                        .border_1()
                        .border_color(rgb(LINE))
                        .cursor_pointer()
                        .debug_selector(|| "uninstall-settings".into())
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(3.))
                                .child(
                                    div()
                                        .text_size(px(13.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Its settings and caches too"),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(rgb(TEXT_3))
                                        .child("Agents, plugins found, studies kept: to the Trash as well."),
                                ),
                        )
                        .child(switch(self.and_settings))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _, _, cx| {
                                v.and_settings = !v.and_settings;
                                cx.notify();
                            }),
                        ),
                    at(0.16),
                ))
            })
            .child(div().flex_1())
            .child(rise(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(10.))
                    .when(removed, |d| {
                        d.child(button("Close", true).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, _, cx| cx.quit()),
                        ))
                    })
                    .when(!removed, |d| {
                        d.child(button("Cancel", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _, _, cx| {
                                v.set_stage(Stage::Ready);
                                cx.notify();
                            }),
                        ))
                        .child(
                            button("Move to Trash", true)
                                .bg(rgb(0xc93c3c))
                                .debug_selector(|| "uninstall-go".into())
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|v, _, _, cx| v.remove(cx)),
                                ),
                        )
                    }),
                at(0.3),
            ))
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
