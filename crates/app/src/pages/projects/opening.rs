//! A song opening: its tracks' instruments and plugins loaded one a frame, its audio read on a
//! thread of its own, and a loading screen while it takes long enough to see: the mark's steps
//! playing, a bar filling, the song's tracks lighting up as each is ready.

use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use gpui::{AnyElement, BoxShadow, Hsla, deferred, linear_color_stop, linear_gradient, point};
use pina_engine::{
    audio::engine::ReadFile,
    audio::player::{MASTER_ID, Player},
};

use super::*;
use crate::arrange::tracks::instrument_name;

/// How long a song opens before the loading screen shows: quicker, it never does.
const QUIET: Duration = Duration::from_millis(140);
const FADE_IN: f32 = 0.22;
const FADE_OUT: f32 = 0.36;
/// The bar's width, and everything under it.
const COLUMN: f32 = 392.0;

/// A song being opened, kept aside until it's all loaded.
pub struct Opening {
    session: Session,
    player: Player,
    /// Its audio files as the reading thread finishes them.
    files: mpsc::Receiver<(PathBuf, Result<ReadFile, String>)>,
    /// Its channels' ids, in the order they load: tracks, auxes, master.
    ids: Vec<u64>,
    /// A step is asked for the next frame.
    stepping: bool,
    pub(crate) splash: Splash,
}

/// What the loading screen shows, kept to fade out over the song once it's open.
pub struct Splash {
    name: String,
    /// Each track's colour, and when it was ready.
    tracks: Vec<(u32, Option<Instant>)>,
    channels: (usize, usize),
    files: (usize, usize),
    /// What's loading now.
    step: String,
    started: Instant,
    /// When the song took over: fading out from then.
    ended: Option<Instant>,
    /// The bar as drawn, catching up with what's done, and when it last moved.
    bar: f32,
    drawn: Instant,
}

impl Opening {
    pub fn new(ws: &Workspace, session: Session, player: Player) -> Opening {
        let project = &session.project;
        let looks = ws.looks_of(&project.tracks, &session.dir);
        let ids: Vec<u64> = project
            .tracks
            .iter()
            .map(|t| t.id)
            .chain(project.auxes.iter().map(|a| a.id))
            .chain([MASTER_ID])
            .collect();
        // Its audio, read on a thread of its own while plugins load here.
        let unread = player.unread_files(project);
        let reader = player.file_reader();
        let (tx, files) = mpsc::channel();
        let count = unread.len();
        std::thread::spawn(move || {
            for f in unread {
                let read = reader.read(&f);
                if tx.send((f, read)).is_err() {
                    return;
                }
            }
        });
        let name = session
            .dir
            .file_name()
            .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());
        let now = Instant::now();
        let mut o = Opening {
            splash: Splash {
                name,
                tracks: looks.iter().map(|l| (l.color, None)).collect(),
                channels: (0, ids.len()),
                files: (0, count),
                step: String::new(),
                started: now,
                ended: None,
                bar: 0.0,
                drawn: now,
            },
            session,
            player,
            files,
            ids,
            stepping: false,
        };
        o.splash.step = o.says(0);
        o
    }

    /// What loading channel `k` (by load order) is, in words.
    fn says(&self, k: usize) -> String {
        let p = &self.session.project;
        let Some(&id) = self.ids.get(k) else {
            return match self.splash.files {
                (read, of) if read < of => "Reading audio".into(),
                _ => "Laying out the song".into(),
            };
        };
        if let Some(t) = p.tracks.iter().find(|t| t.id == id) {
            let what = match &t.instrument {
                None if t.inserts.is_empty() => return format!("Setting up {}", t.name),
                None => format!("{} effects", t.inserts.len()),
                i => instrument_name(i.as_ref()),
            };
            return format!("Loading {what} · {}", t.name);
        }
        match p.auxes.iter().find(|a| a.id == id) {
            Some(a) => format!("Loading {}", a.name),
            None => "Loading the master".into(),
        }
    }
}

#[cfg(test)]
impl Opening {
    /// `n` channels loaded, for a picture: no more load till it's taken.
    pub(crate) fn steps_for_shot(&mut self, n: usize) {
        for _ in 0..n {
            if let Some(id) = self.player.load_next(&self.session.project)
                && let Some(t) = self.session.project.tracks.iter().position(|t| t.id == id)
            {
                self.splash.tracks[t].1 = Some(Instant::now());
                self.splash.channels.0 += 1;
            }
        }
        self.splash.step = self.says(self.splash.channels.0);
    }

    /// As if it began `ago` before now, its bar caught up, for a picture of it.
    pub(crate) fn began(&mut self, ago: Duration) {
        let s = &mut self.splash;
        s.started -= ago;
        let total = (s.channels.1 + s.files.1).max(1) as f32;
        s.bar = (s.channels.0 + s.files.0) as f32 / total;
        self.stepping = true;
    }
}

impl Workspace {
    /// The next piece of the song opening: the files read since, and one channel; once it's all
    /// in, the song takes over.
    pub(crate) fn open_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(o) = self.loading.as_mut() else {
            return;
        };
        o.stepping = false;
        while let Ok((file, read)) = o.files.try_recv() {
            // One that can't be read is said when the song plays it.
            if let Ok(read) = read {
                o.player.keep_file(file, read);
            }
            o.splash.files.0 += 1;
        }
        let project = &o.session.project;
        if let Some(id) = o.player.load_next(project) {
            if let Some(t) = project.tracks.iter().position(|t| t.id == id) {
                o.splash.tracks[t].1 = Some(Instant::now());
            }
            o.splash.channels.0 += 1;
        } else {
            o.splash.channels.0 = o.splash.channels.1;
        }
        o.splash.step = o.says(o.splash.channels.0);
        let (done, of) = (o.splash.channels, o.splash.files);
        if done.0 == done.1 && of.0 == of.1 {
            let o = self.loading.take().unwrap();
            let mut splash = o.splash;
            // Seen: it fades out over the song.
            if splash.started.elapsed() > QUIET {
                splash.ended = Some(Instant::now());
                self.splash = Some(splash);
            }
            self.finish_opening(o.session, o.player, window, cx);
        }
        cx.notify();
    }

    /// Each frame a song is opening: a step asked for the next, and the screen kept moving.
    pub(crate) fn opening_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(o) = self.loading.as_mut()
            && !o.stepping
        {
            o.stepping = true;
            cx.on_next_frame(window, |ws, window, cx| ws.open_step(window, cx));
        }
        if self.splash.as_ref().is_some_and(|s| {
            s.ended
                .is_some_and(|e| e.elapsed().as_secs_f32() > FADE_OUT)
        }) {
            self.splash = None;
        }
        if self.loading.is_some() || self.splash.is_some() {
            window.request_animation_frame();
        }
    }

    /// The loading screen, over everything: while a song opens (once it's taken a moment), and
    /// fading out over it once it has.
    pub(crate) fn splash_view(&mut self) -> Option<AnyElement> {
        let s = match (self.loading.as_mut(), self.splash.as_mut()) {
            (Some(o), _) if o.splash.started.elapsed() > QUIET => &mut o.splash,
            (None, Some(s)) => s,
            _ => return None,
        };
        let now = Instant::now();
        let t = s.started.elapsed().as_secs_f32();
        // The bar eases after what's done, however long a frame took.
        let total = (s.channels.1 + s.files.1).max(1) as f32;
        let target = match s.ended {
            Some(_) => 1.0,
            None => (s.channels.0 + s.files.0) as f32 / total,
        };
        let dt = now.duration_since(s.drawn).as_secs_f32().min(0.25);
        s.bar += (target - s.bar) * (1.0 - (-dt * 7.0).exp());
        s.drawn = now;
        let shown = (t - QUIET.as_secs_f32()) / FADE_IN;
        let gone = s
            .ended
            .map_or(0.0, |e| e.elapsed().as_secs_f32() / FADE_OUT);
        let fade = ease_out(shown.clamp(0.0, 1.0)) * (1.0 - ease_in(gone.clamp(0.0, 1.0)));
        // Rising into place as it comes, drifting up as it goes.
        let rise = 10.0 * (1.0 - ease_out(shown.clamp(0.0, 1.0))) - 8.0 * ease_in(gone.min(1.0));
        let s = &*s;
        let percent = (s.bar * 100.0).round().min(100.0) as u32;
        let counts = match s.files.1 {
            0 => format!("{} of {} tracks", ready(s), s.tracks.len()),
            n => format!(
                "{} of {} tracks · {} of {n} audio files",
                ready(s),
                s.tracks.len(),
                s.files.0
            ),
        };
        let screen = div()
            .id("opening")
            .debug_selector(|| "opening".into())
            .occlude()
            .absolute()
            .inset_0()
            .opacity(fade)
            .bg(rgb(BG))
            .flex()
            .items_center()
            .justify_center()
            .child(
                // A soft glow the mark sits in, breathing.
                div()
                    .absolute()
                    .size(px(40.))
                    .rounded_full()
                    .shadow(vec![BoxShadow {
                        color: alpha(ACCENT, 0.07 + 0.025 * (t * 1.3).sin()),
                        offset: point(px(0.), px(-90.)),
                        blur_radius: px(320.),
                        spread_radius: px(150.),
                        inset: false,
                    }]),
            )
            .child(
                div()
                    .relative()
                    .top(px(rise))
                    .w(px(COLUMN))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(mark(t))
                    .child(
                        div()
                            .mt(px(30.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_4))
                            .child("OPENING"),
                    )
                    .child(
                        div()
                            .mt(px(6.))
                            .max_w(px(COLUMN))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(px(26.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(s.name.clone()),
                    )
                    .child(div().mt(px(26.)).child(bar(s.bar, t)))
                    .child(
                        div()
                            .mt(px(10.))
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_size(px(12.5))
                                    .text_color(rgb(TEXT_2))
                                    .child(s.step.clone()),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(12.))
                                    .text_color(rgb(TEXT_3))
                                    .child(format!("{percent}%")),
                            ),
                    )
                    .child(div().mt(px(22.)).child(cells(s, now, t)))
                    .child(
                        div()
                            .mt(px(12.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_4))
                            .child(counts),
                    ),
            );
        Some(deferred(screen).with_priority(5).into_any_element())
    }
}

fn ready(s: &Splash) -> usize {
    s.tracks.iter().filter(|c| c.1.is_some()).count()
}

fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}

fn ease_in(x: f32) -> f32 {
    x * x * x
}

/// How a step lights when it plays `ago` seconds ago: a flash that dies away.
fn flash(ago: f32) -> f32 {
    (-ago / 0.22).exp()
}

/// Pina Studio's mark (steps spelling p, a row a track, each its family's colour), big, its steps
/// playing: a playhead crosses them in sixteenths at 124 BPM, each lit step flashing as it's hit.
fn mark(t: f32) -> impl IntoElement {
    const P: [[bool; 4]; 5] = [
        [true, true, true, true],
        [true, false, false, true],
        [true, true, true, true],
        [true, false, false, false],
        [true, false, false, false],
    ];
    let rows = [
        Family::Drums,
        Family::Percussion,
        Family::Bass,
        Family::Keys,
        Family::Synth,
    ];
    let step = 60.0 / 124.0 / 4.0;
    let n = (t / step).floor();
    let head = n as usize % 4;
    // When column `c` was last hit.
    let ago = |c: usize| {
        let back = (head + 4 - c) % 4;
        t - (n - back as f32) * step
    };
    let breathe = 0.5 + 0.5 * (t * 2.1).sin();
    div()
        .size(px(116.))
        .rounded(px(30.))
        .bg(rgb(0x0c0c0f))
        .border_1()
        .border_color(rgb(LINE_STRONG))
        .shadow(vec![
            BoxShadow {
                color: alpha(ACCENT, 0.10 + 0.08 * breathe),
                offset: point(px(0.), px(0.)),
                blur_radius: px(36. + 10. * breathe),
                spread_radius: px(0.),
                inset: false,
            },
            BoxShadow {
                color: Hsla::black().opacity(0.6),
                offset: point(px(0.), px(14.)),
                blur_radius: px(30.),
                spread_radius: px(-6.),
                inset: false,
            },
        ])
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .children(P.iter().zip(rows).map(move |(steps, family)| {
            let color = family_shade(family, 0);
            div()
                .flex()
                .gap(px(4.))
                .children(steps.iter().enumerate().map(move |(c, &on)| {
                    let f = flash(ago(c));
                    // A step a little bigger as it's hit, in a cell that stays put.
                    let size = 13.0 + 3.0 * f * on as u8 as f32;
                    let cell = div().size(px(size)).rounded(px(3.5));
                    let cell = match on {
                        true => cell
                            .bg(mix(color, 0xffffff, 0.35 * f))
                            .opacity(0.7 + 0.3 * f)
                            .shadow(vec![BoxShadow {
                                color: alpha(color, 0.55 * f),
                                offset: point(px(0.), px(0.)),
                                blur_radius: px(10.),
                                spread_radius: px(0.),
                                inset: false,
                            }]),
                        false => cell.bg(mix(0x1c1c23, 0x34343d, f)),
                    };
                    div()
                        .size(px(16.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(cell)
                }))
        }))
}

/// How far the song has loaded: a bar filling, a sheen running along it.
fn bar(done: f32, t: f32) -> impl IntoElement {
    let fill = (done.clamp(0.0, 1.0) * COLUMN).max(6.0);
    let sheen = 90.0;
    let x = (t % 1.5) / 1.5 * (fill + sheen) - sheen;
    div()
        .w(px(COLUMN))
        .h(px(6.))
        .rounded(px(3.))
        .bg(rgb(0x1c1c23))
        .child(
            div()
                .relative()
                .w(px(fill))
                .h_full()
                .rounded(px(3.))
                .overflow_hidden()
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(rgb(ACCENT), 0.),
                    linear_color_stop(rgb(VIOLET), 1.),
                ))
                .shadow(vec![BoxShadow {
                    color: alpha(ACCENT, 0.45),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(12.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(x))
                        .h_full()
                        .flex()
                        .children([(0.0, 0.5), (0.5, 0.0)].map(|(a, b)| {
                            div().w(px(sheen / 2.0)).h_full().bg(linear_gradient(
                                90.,
                                linear_color_stop(alpha(0xffffff, a), 0.),
                                linear_color_stop(alpha(0xffffff, b), 1.),
                            ))
                        })),
                ),
        )
}

/// The song's tracks, a cell each in its colour: dim till ready, then popping in; the one
/// loading now pulses.
fn cells(s: &Splash, now: Instant, t: f32) -> impl IntoElement {
    let next = s.tracks.iter().position(|c| c.1.is_none());
    let pulse = 0.5 + 0.5 * (t * 6.0).sin();
    div()
        .w(px(COLUMN))
        .flex()
        .flex_wrap()
        .justify_center()
        .gap(px(5.))
        .children(s.tracks.iter().enumerate().map(move |(i, &(color, at))| {
            let cell = div().size(px(10.)).rounded(px(3.));
            match at {
                Some(at) => {
                    let f = flash(now.duration_since(at).as_secs_f32());
                    cell.bg(mix(color, 0xffffff, 0.5 * f))
                        .shadow(vec![BoxShadow {
                            color: alpha(color, 0.35 + 0.4 * f),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(6. + 6. * f),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                }
                None if Some(i) == next => cell
                    .bg(alpha(color, 0.2 + 0.25 * pulse))
                    .border_1()
                    .border_color(alpha(color, 0.5 + 0.5 * pulse)),
                None => cell.bg(alpha(color, 0.14)),
            }
        }))
}

/// `a` toward `b` by `k`.
fn mix(a: u32, b: u32, k: f32) -> Hsla {
    let k = k.clamp(0.0, 1.0);
    let ch = |c: u32, s: u32| ((c >> s) & 0xff) as f32;
    let at = |s: u32| (ch(a, s) + (ch(b, s) - ch(a, s)) * k).round() as u32;
    rgb((at(16) << 16) | (at(8) << 8) | at(0)).into()
}
