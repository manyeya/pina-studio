//! The study as lanes: what the file is over one timeline of everything found in it, one
//! playhead over all of it, the rest docked below, and what's picked read out at the side. Only
//! what's in the file gets a lane: a piano loop has chords and no drums.

use std::{sync::Arc, time::Instant};

use gpui::{Hsla, ObjectFit, SharedString, Stateful, img, relative, size};
use pina_engine::{
    audio::engine::{AudioData, load_audio},
    study::{
        Makeup, Sure,
        reference::{Blueprint, HIGH_HZ, LOW_HZ},
    },
};

use super::like_view::{act, facts_view};
use super::*;

/// How wide a lane's name is.
const NAME_W: f32 = 92.;
/// Columns a lane drawn from its frames is drawn in, at most.
const COLUMNS: usize = 480;

/// What's picked: read out at the side.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Pick {
    /// What the file is.
    #[default]
    File,
    Tempo,
    Key,
    Length,
    Loudness,
    Swing,
    Chord(usize),
    Section(usize),
}

/// What's open under the lanes.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Dock {
    #[default]
    Harmony,
    Groove,
    Parts,
    Mix,
    Words,
    Known,
    Books,
    Compare,
}

/// Which studies the rail shows.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Only {
    #[default]
    All,
    Songs,
    Loops,
}

pub(crate) fn title_of(s: &Study) -> String {
    match s.identity.title.is_empty() {
        true => file_name(&s.file),
        false => s.identity.title.clone(),
    }
}

fn sure_dot(sure: Sure) -> Div {
    let d = div().size(px(7.)).flex_none().rounded(px(4.));
    match sure {
        Sure::Solid => d.bg(rgb(MINT)),
        Sure::Likely => d.border_1().border_color(rgb(AMBER)),
        Sure::Unsure => d.border_1().border_color(rgb(TEXT_3)),
    }
}

fn sure_word(sure: Sure) -> &'static str {
    match sure {
        Sure::Solid => "Solid",
        Sure::Likely => "Likely",
        Sure::Unsure => "Unsure",
    }
}

/// Bars a study lasts: a loop's are whole, however its tail runs on.
fn bars_of(s: &Study) -> usize {
    let per_bar = s.tempo.time_signature[0].max(1) as f32;
    match s.nature.song {
        true => s.tempo.bars.len(),
        false => (s.identity.seconds * s.tempo.bpm / (60.0 * per_bar)).round() as usize,
    }
}

struct Fact {
    name: &'static str,
    value: String,
    unit: &'static str,
    sure: Sure,
    pick: Pick,
}

/// What's said of it at the top. Nothing about what it can't have.
fn facts_of(s: &Study) -> Vec<Fact> {
    let mut out = vec![Fact {
        name: "Tempo",
        value: match s.tempo.bpm.fract().abs() < 0.05 {
            true => format!("{:.0}", s.tempo.bpm),
            false => format!("{:.1}", s.tempo.bpm),
        },
        unit: "BPM",
        sure: s.tempo.sure,
        pick: Pick::Tempo,
    }];
    if !s.key.name.is_empty() {
        out.push(Fact {
            name: "Key",
            value: match &s.key.other {
                Some(o) => format!("{} or {o}", s.key.name),
                None => s.key.name.clone(),
            },
            unit: "",
            sure: s.key.sure,
            pick: Pick::Key,
        });
    }
    out.push(Fact {
        name: "Length",
        value: match s.nature.song {
            true => clock(s.identity.seconds),
            false => bars_of(s).to_string(),
        },
        unit: if s.nature.song { "" } else { "bars" },
        sure: Sure::Solid,
        pick: Pick::Length,
    });
    out.push(Fact {
        name: "Loudness",
        value: format!("{:.1}", s.mix.overall.lufs),
        unit: "LUFS",
        sure: Sure::Solid,
        pick: Pick::Loudness,
    });
    if !s.groove.is_none() {
        out.push(Fact {
            name: "Swing",
            value: match s.groove.swing < 52.0 {
                true => "Straight".to_string(),
                false => format!("{:.0}", s.groove.swing),
            },
            unit: "",
            sure: Sure::Likely,
            pick: Pick::Swing,
        });
    }
    out
}

pub(crate) fn rect(window: &mut gpui::Window, x: f32, y: f32, w: f32, h: f32, color: Hsla) {
    window.paint_quad(fill(
        Bounds::new(point(px(x), px(y)), size(px(w), px(h))),
        color,
    ));
}

pub(crate) fn origin(b: Bounds<Pixels>) -> (f32, f32, f32, f32) {
    (
        f32::from(b.origin.x),
        f32::from(b.origin.y),
        f32::from(b.size.width),
        f32::from(b.size.height),
    )
}

/// A part's name, short enough for a lane.
fn part_name(e: &study::Element) -> String {
    match e.role.as_str() {
        "chords" => "Chords".into(),
        "pad" => "Pads".into(),
        "lead" => "Lead".into(),
        "perc" => "Hats and air".into(),
        _ => e.name.clone(),
    }
}

type Seek = Box<dyn Fn(&MouseDownEvent, &mut gpui::Window, &mut App)>;

/// What's read out at the side: its kind, its name, a line under it, rows, lines, how sure, and
/// where to hear it from.
type Reading = (
    String,
    String,
    String,
    Vec<(&'static str, String)>,
    Vec<String>,
    Option<Sure>,
    Option<f32>,
);

/// A click on a lane moves the playhead there.
fn seek(seconds: f32, cx: &mut Context<Workspace>) -> Seek {
    Box::new(
        cx.listener(move |ws: &mut Workspace, e: &MouseDownEvent, _, cx| {
            if let Some(b) = ws.studies.track.get() {
                let x = f32::from(e.position.x - b.origin.x) / f32::from(b.size.width);
                ws.study_seek(x.clamp(0.0, 1.0) * seconds, cx);
            }
        }),
    )
}

/// A lane: its name, and what's on it, a fixed height or the rest of the height.
pub(crate) fn lane(
    name: impl Into<SharedString>,
    height: Option<f32>,
    color: Option<u32>,
) -> (Div, Div) {
    let name = name.into();
    let row = div()
        .flex()
        .min_h(px(0.))
        .border_b_1()
        .border_color(rgb(LINE_SOFT));
    let row = match height {
        Some(h) => row.flex_none().h(px(h)),
        None => row.flex_1(),
    };
    let tag = div()
        .w(px(NAME_W))
        .flex_none()
        .h_full()
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(10.))
        .bg(rgb(SURFACE))
        .border_r_1()
        .border_color(rgb(LINE_SOFT))
        .text_size(px(11.))
        .text_color(rgb(TEXT_3))
        .whitespace_nowrap()
        .overflow_hidden()
        .when_some(color, |d, c| {
            d.child(div().size(px(7.)).flex_none().rounded(px(2.)).bg(rgb(c)))
        })
        .child(name);
    (
        row.child(tag),
        div()
            .relative()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .overflow_hidden(),
    )
}

impl Workspace {
    /// Where the playhead is, seconds.
    pub(crate) fn study_now(&self) -> f32 {
        match self.studies.playing {
            Some((since, from)) => from + since.elapsed().as_secs_f32(),
            None => self.studies.at,
        }
    }

    /// Plays the open study from `from`, its audio read off the UI thread the first time.
    pub(crate) fn study_play(&mut self, from: f32, cx: &mut Context<Self>) {
        let Some(file) = self.studies.open.clone() else {
            return;
        };
        self.studies.at = from;
        if let Some((f, data)) = &self.studies.audio
            && *f == file
        {
            match self.player.audition_from(data.clone(), from as f64, 0.0) {
                Ok(()) => self.studies.playing = Some((Instant::now(), from)),
                Err(e) => self.status = Some(e),
            }
            cx.notify();
            return;
        }
        if self.studies.reading.as_ref() == Some(&file) {
            return;
        }
        self.studies.reading = Some(file.clone());
        cx.notify();
        let rate = self.player.rate();
        let job = {
            let file = file.clone();
            cx.background_spawn(async move { load_audio(&file, rate).map(Arc::new) })
        };
        cx.spawn(async move |ws, cx| {
            let read: Result<Arc<AudioData>, String> = job.await;
            ws.update(cx, |ws, cx| {
                ws.studies.reading = None;
                match read {
                    Ok(data) => {
                        ws.studies.audio = Some((file.clone(), data));
                        if ws.studies.open.as_ref() == Some(&file) {
                            ws.study_play(from, cx);
                        }
                    }
                    Err(e) => ws.status = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn study_stop(&mut self, cx: &mut Context<Self>) {
        self.studies.at = self.study_now();
        self.studies.playing = None;
        self.player.stop_audition();
        cx.notify();
    }

    fn study_toggle(&mut self, cx: &mut Context<Self>) {
        match self.studies.playing.is_some() {
            true => self.study_stop(cx),
            false => self.study_play(self.studies.at, cx),
        }
    }

    /// The playhead moved: what's playing carries on from there.
    pub(crate) fn study_seek(&mut self, t: f32, cx: &mut Context<Self>) {
        match self.studies.playing.is_some() {
            true => self.study_play(t, cx),
            false => {
                self.studies.at = t;
                cx.notify();
            }
        }
    }

    /// At its end a loop goes round and a song stops.
    pub(crate) fn study_keep_going(&mut self, s: &Study, cx: &mut Context<Self>) {
        if self.studies.playing.is_some() && self.study_now() >= s.identity.seconds {
            match s.nature.song {
                true => {
                    self.study_stop(cx);
                    self.studies.at = 0.0;
                }
                false => self.study_play(0.0, cx),
            }
        }
    }

    /// A study's picture: kept, or painted in the background (shown once it's ready).
    fn study_spectrogram(&mut self, file: &Path, cx: &mut Context<Self>) -> Option<PathBuf> {
        if let Some(p) = self.studies.spectrograms.get(file) {
            return Some(p.clone());
        }
        if self.studies.painting.iter().any(|f| f == file) {
            return None;
        }
        self.studies.painting.push(file.to_path_buf());
        let path = file.to_path_buf();
        let job = {
            let path = path.clone();
            cx.background_spawn(async move { cli::study::spectrogram(&path) })
        };
        cx.spawn(async move |ws, cx| {
            let painted = job.await;
            ws.update(cx, |ws, cx| {
                match painted {
                    Ok(p) => {
                        ws.studies.painting.retain(|f| *f != path);
                        ws.studies.spectrograms.insert(path, p);
                    }
                    // Left as being painted: a file that can't be read isn't tried each frame.
                    Err(e) => ws.status = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        None
    }

    pub(crate) fn study_sheet(&mut self, s: &Study, cx: &mut Context<Self>) -> impl IntoElement {
        self.study_keep_going(s, cx);
        let bp = self.study_blueprint(&s.file, cx);
        let picture = self.study_spectrogram(&s.file, cx);
        let head = self.study_head(s, cx);
        let lanes = self.study_lanes(s, bp.as_ref(), picture, cx);
        let dock = self.study_dock(s, bp.as_ref(), cx);
        let side = self.study_side(s, cx);
        div()
            .id("study-sheet")
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .child(head)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(lanes)
                            .child(dock),
                    )
                    .child(side),
            )
    }

    /// Its name, what's said of it, and what can be done with it.
    fn study_head(&mut self, s: &Study, cx: &mut Context<Self>) -> Div {
        let playing = self.studies.playing.is_some();
        let reading = self.studies.reading.is_some();
        let picked = self.studies.pick;
        let file = s.file.clone();
        let byline: Vec<String> = [
            s.nature.name.clone(),
            s.identity.artist.clone(),
            s.identity.album.clone(),
            s.identity.year.clone(),
        ]
        .into_iter()
        .filter(|v| !v.is_empty())
        .collect();
        let facts: Vec<_> = facts_of(s)
            .into_iter()
            .map(|f| {
                let (pick, on) = (f.pick, f.pick == picked);
                div()
                    .id(f.name)
                    .debug_selector(move || format!("study-fact-{}", f.name.to_lowercase()))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .px(px(11.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgb(FIELD)))
                    .when(!on, |d| d.hover(|d| d.bg(rgb(CARD))))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(3.))
                            .font_family(MONO)
                            .whitespace_nowrap()
                            .child(
                                div()
                                    .text_size(px(14.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(f.value),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TEXT_3))
                                    .child(f.unit),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_3))
                            .child(sure_dot(f.sure))
                            .child(f.name),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.studies.pick = pick;
                            cx.notify();
                        }),
                    )
            })
            .collect();
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(14.))
            .py(px(10.))
            .bg(rgb(SURFACE))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .id("study-play")
                    .debug_selector(|| "study-play".into())
                    .size(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(18.))
                    .bg(rgb(TEXT))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(0xffffff)))
                    .child(
                        icon(if playing { Icon::Stop } else { Icon::Play }, 14.)
                            .text_color(rgb(0x101014)),
                    )
                    .when(reading, |d| d.opacity(0.6))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _, _, cx| ws.study_toggle(cx)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(120.))
                    .max_w(px(280.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .debug_selector(|| "study-title".into())
                            .text_size(px(15.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(title_of(s)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(TEXT_3))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(byline.join(" · ")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap(px(2.))
                    .p(px(3.))
                    .rounded(px(9.))
                    .bg(rgb(WELL))
                    .border_1()
                    .border_color(rgb(LINE))
                    .children(facts),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap(px(8.))
                    .child(
                        act("Mix reference", "study-reference", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.study_as_reference(cx)),
                        ),
                    )
                    .child(
                        act("Save as playbook", "study-playbook", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| ws.study_to_playbook(cx)),
                        ),
                    )
                    .child(
                        act("Create a song like this", "study-start", true).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| ws.open_create(file.clone(), cx)),
                        ),
                    ),
            )
    }

    fn study_lanes(
        &mut self,
        s: &Study,
        bp: Option<&Blueprint>,
        picture: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let seconds = s.identity.seconds.max(0.01);
        let now = self.study_now();
        let at = (now / seconds).clamp(0.0, 1.0);
        let picked = self.studies.pick;
        let drawn = self.studies.track.clone();
        // The ruler: every bar of a loop, every so many of a song.
        let (ruler, track) = lane(
            format!("{} / {}", clock(now.min(seconds)), clock(seconds)),
            Some(24.),
            None,
        );
        let bars = &s.tempo.bars;
        let every = match bars.len() {
            0..=32 => 1,
            33..=96 => 4,
            97..=256 => 16,
            _ => 32,
        };
        let ticks = bars
            .iter()
            .enumerate()
            .step_by(every)
            .filter(|(_, t)| **t < seconds)
            .map(|(i, t)| {
                div()
                    .absolute()
                    .left(relative(t / seconds))
                    .top_0()
                    .bottom_0()
                    .pl(px(5.))
                    .border_l_1()
                    .border_color(rgb(LINE))
                    .font_family(MONO)
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .flex()
                    .items_center()
                    .child((i + 1).to_string())
            });
        let ruler = ruler.child(
            track
                .child(
                    canvas(move |b, _, _| drawn.set(Some(b)), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .children(ticks)
                .on_mouse_down(MouseButton::Left, seek(seconds, cx)),
        );
        let mut lanes: Vec<Div> = vec![ruler];
        // The form: a song's, or a long part's. A loop has no intro or outro.
        let long = s.nature.song || s.identity.seconds > 45.0;
        if long && s.sections.len() > 1 {
            let (row, track) = lane("Form", Some(40.), None);
            let blocks = s.sections.iter().enumerate().map(|(i, x)| {
                let width = (x.end - x.start) / seconds;
                let color = letter_color(&x.label);
                let on = now >= x.start && now < x.end;
                let chosen = picked == Pick::Section(i);
                let start = x.start;
                div()
                    .id(("study-section", i))
                    .debug_selector(move || format!("study-section-{i}"))
                    .absolute()
                    .left(relative(x.start / seconds))
                    .w(relative(width))
                    .top(px(4.))
                    .bottom(px(4.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .bg(alpha(color, 0.34))
                    .text_size(px(11.5))
                    .text_color(rgb(TEXT))
                    .cursor_pointer()
                    .hover(|d| d.bg(alpha(color, 0.5)))
                    .when(on, |d| d.border_1().border_color(rgb(TEXT_2)))
                    .when(chosen, |d| d.border_1().border_color(rgb(ACCENT)))
                    .child(if width > 0.07 {
                        x.name.clone()
                    } else if width > 0.022 {
                        x.label.clone()
                    } else {
                        String::new()
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            ws.studies.pick = Pick::Section(i);
                            ws.study_seek(start, cx);
                        }),
                    )
            });
            lanes.push(row.child(track.children(blocks)));
        }
        // Who plays where, in a mix.
        if s.nature.makeup == Makeup::Mix && s.sections.len() > 1 {
            for e in &s.instruments {
                let color = role_color(&e.role);
                let (row, track) = lane(part_name(e), Some(22.), Some(color));
                let bars = e
                    .sections
                    .iter()
                    .filter_map(|&i| s.sections.get(i))
                    .map(|x| {
                        div()
                            .absolute()
                            .left(relative(x.start / seconds))
                            .w(relative((x.end - x.start) / seconds))
                            .top(px(6.))
                            .bottom(px(6.))
                            .rounded(px(2.))
                            .bg(alpha(color, 0.55))
                    });
                lanes.push(row.child(track.children(bars)));
            }
        }
        // Chords; or, for one line, its notes.
        if !s.chords.is_empty() {
            let line = s.nature.makeup == Makeup::Line;
            let (row, track) = lane(
                if line { "Notes" } else { "Chords" },
                Some(if s.nature.song { 34. } else { 58. }),
                None,
            );
            let color = if line { VIOLET } else { SKY };
            let blocks = s
                .chords
                .iter()
                .enumerate()
                .filter(|(_, c)| c.chord != "N")
                .map(|(i, c)| {
                    let width = (c.end - c.start) / seconds;
                    // Its name, if it fits: a block is about 1100 wide on a full window.
                    let fits = width > 0.012 + 0.0075 * c.chord.len() as f32;
                    let on = now >= c.start && now < c.end;
                    let chosen = picked == Pick::Chord(i);
                    let start = c.start;
                    div()
                        .id(("study-chord", i))
                        .debug_selector(move || format!("study-chord-{i}"))
                        .absolute()
                        .left(relative(c.start / seconds))
                        .w(relative(width))
                        .top(px(4.))
                        .bottom(px(4.))
                        .px(px(if fits { 6. } else { 0. }))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .rounded(px(4.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .bg(alpha(color, 0.2))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(tint(color, 0.55)))
                        .cursor_pointer()
                        .hover(|d| d.bg(alpha(color, 0.34)))
                        .when(on, |d| d.border_1().border_color(rgb(TEXT_2)))
                        .when(chosen, |d| d.border_1().border_color(rgb(ACCENT)))
                        .when(fits, |d| d.child(c.chord.clone()))
                        .when(width > 0.05 && !c.numeral.is_empty(), |d| {
                            d.child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(10.))
                                    .font_weight(FontWeight::NORMAL)
                                    .opacity(0.75)
                                    .child(c.numeral.clone()),
                            )
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                ws.studies.pick = Pick::Chord(i);
                                ws.study_seek(start, cx);
                            }),
                        )
                });
            lanes.push(row.child(track.children(blocks)));
        }
        // A drum loop's steps, bar after bar.
        if s.nature.makeup == Makeup::Drums {
            let g = &s.groove;
            for (name, steps, color) in [
                ("Kick", &g.kick, CORAL),
                ("Snare or clap", &g.snare, CORAL),
                ("Hats", &g.hats, MINT),
            ] {
                let (row, track) = lane(name, Some(24.), Some(color));
                let cells: Vec<f32> = steps
                    .chars()
                    .map(|c| match c {
                        'X' => 1.0,
                        'x' => 0.75,
                        'o' => 0.5,
                        'g' => 0.28,
                        _ => 0.0,
                    })
                    .collect();
                let starts: Vec<f32> = bars.iter().map(|t| t / seconds).collect();
                let long = bars.get(1).map_or(1.0, |n| n - bars[0]) / seconds;
                let paint = canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let (x, y, w, h) = origin(b);
                        let step = long * w / 16.0;
                        for bar in &starts {
                            for (i, level) in cells.iter().enumerate() {
                                if *level > 0.0 {
                                    let left = x + (bar * w) + i as f32 * step;
                                    let color = alpha(color, *level);
                                    rect(
                                        window,
                                        left,
                                        y + 4.0,
                                        (step - 1.0).max(1.0),
                                        h - 8.0,
                                        color,
                                    );
                                }
                            }
                        }
                    },
                )
                .absolute()
                .size_full();
                lanes.push(row.child(track.child(paint)));
            }
        }
        // How hard it plays, bar by bar.
        if s.nature.song {
            let (row, track) = lane("Energy", Some(36.), None);
            let energy = s.energy.clone();
            let paint = canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let (x, y, w, h) = origin(b);
                    let n = energy.len().max(1) as f32;
                    for (i, e) in energy.iter().enumerate() {
                        let top = h * (1.0 - e * 0.92);
                        rect(
                            window,
                            x + i as f32 * w / n,
                            y + top,
                            (w / n).max(1.0),
                            h - top,
                            alpha(ACCENT, 0.45),
                        );
                    }
                },
            )
            .absolute()
            .size_full();
            lanes.push(row.child(track.child(paint)));
        }
        // The spectrum, its held lines over it.
        let (row, track) = lane("Spectrogram", None, None);
        let lines: Vec<(f32, f32, f32, f32)> = bp.map_or(Vec::new(), |bp| {
            let n = bp.frames.max(1) as f32;
            bp.contours
                .iter()
                .filter_map(|c| {
                    let up = (c.hz? / LOW_HZ).ln() / (HIGH_HZ / LOW_HZ).ln();
                    Some((c.start as f32 / n, c.end as f32 / n, up, c.strength))
                })
                .collect()
        });
        let held = canvas(
            |_, _, _| {},
            move |b, _, window, _| {
                let (x, y, w, h) = origin(b);
                for (from, to, up, strength) in &lines {
                    let color = alpha(0xffffff, 0.3 + 0.5 * strength.clamp(0.0, 1.0));
                    let left = x + from * w;
                    rect(
                        window,
                        left,
                        y + (1.0 - up) * h,
                        ((to - from) * w).max(1.0),
                        1.0,
                        color,
                    );
                }
            },
        )
        .absolute()
        .size_full();
        let hz = |hz: f32, name: &'static str| {
            let up = (hz / LOW_HZ).ln() / (HIGH_HZ / LOW_HZ).ln();
            div()
                .absolute()
                .left(px(4.))
                .top(relative(1.0 - up))
                .font_family(MONO)
                .text_size(px(9.5))
                .text_color(rgb(TEXT_2))
                .child(name)
        };
        let spectrum = match picture {
            Some(p) => track
                .bg(rgb(0x000000))
                .child(
                    img(p)
                        .debug_selector(|| "study-spectrogram".into())
                        .size_full()
                        .object_fit(ObjectFit::Fill),
                )
                .child(held)
                .child(hz(10_000.0, "10k"))
                .child(hz(1_000.0, "1k"))
                .child(hz(100.0, "100")),
            None => track
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .text_color(rgb(TEXT_3))
                .child("Drawing its spectrum…"),
        };
        lanes.push(row.child(spectrum.on_mouse_down(MouseButton::Left, seek(seconds, cx))));
        // Where things start, and the sound itself. Both from the blueprint's frames.
        if let Some(bp) = bp {
            let n = bp.frames.max(1);
            let columns = n.min(COLUMNS);
            let frames = |c: usize| {
                (
                    c * n / columns,
                    ((c + 1) * n / columns).max(c * n / columns + 1),
                )
            };
            let starts: Vec<[f32; 3]> = (0..columns)
                .map(|c| {
                    let (a, z) = frames(c);
                    let mut top = [0f32; 3];
                    for row in &bp.onsets[a.min(bp.onsets.len())..z.min(bp.onsets.len())] {
                        for k in 0..3 {
                            top[k] = top[k].max(row[k]);
                        }
                    }
                    top
                })
                .collect();
            let (row, track) = lane("Starts", Some(28.), None);
            // A whole song starts something everywhere: only its strongest read.
            let bar = if s.nature.song { 0.8 } else { 0.45 };
            let paint = canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let (x, y, w, h) = origin(b);
                    let (cw, rh) = (w / starts.len().max(1) as f32, h / 3.0);
                    for (c, column) in starts.iter().enumerate() {
                        for (k, v) in column.iter().enumerate() {
                            if *v > bar {
                                let color = alpha([CORAL, SKY, MINT][k], 0.45 + 0.55 * v);
                                let top = y + k as f32 * rh;
                                rect(
                                    window,
                                    x + c as f32 * cw,
                                    top + 1.0,
                                    cw.max(1.0),
                                    rh - 2.0,
                                    color,
                                );
                            }
                        }
                    }
                },
            )
            .absolute()
            .size_full();
            lanes.push(row.child(track.child(paint)));
            let levels: Vec<f32> = (0..columns)
                .map(|c| {
                    let (a, z) = frames(c);
                    ((bp.level(a, z) + 42.0) / 42.0).clamp(0.02, 1.0)
                })
                .collect();
            let (row, track) = lane("Sound", Some(48.), None);
            let paint = canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    let (x, y, w, h) = origin(b);
                    let cw = w / levels.len().max(1) as f32;
                    for (c, level) in levels.iter().enumerate() {
                        let tall = level * (h - 6.0);
                        let played = (c as f32 + 0.5) / levels.len() as f32 <= at;
                        let color = rgb(if played { TEXT_2 } else { 0x4f5462 }).into();
                        let left = x + c as f32 * cw;
                        rect(
                            window,
                            left,
                            y + (h - tall) / 2.0,
                            (cw - 1.0).max(1.0),
                            tall,
                            color,
                        );
                    }
                },
            )
            .absolute()
            .size_full();
            lanes.push(
                row.child(
                    track
                        .child(paint)
                        .on_mouse_down(MouseButton::Left, seek(seconds, cx)),
                ),
            );
        }
        div()
            .id("study-lanes")
            .debug_selector(|| "study-lanes".into())
            .relative()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(BG))
            .children(lanes)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(NAME_W))
                    .right_0()
                    .child(
                        div()
                            .debug_selector(|| "study-playhead".into())
                            .absolute()
                            .left(relative(at))
                            .top_0()
                            .bottom_0()
                            .w(px(1.))
                            .bg(rgb(TEXT)),
                    ),
            )
            .when(!s.nature.absent.is_empty(), |d| {
                d.child(
                    div()
                        .relative()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(12.))
                        .py(px(7.))
                        .bg(rgb(SURFACE))
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .debug_selector(|| "study-absent".into())
                        .child("Not in this file")
                        .children(s.nature.absent.iter().map(|a| {
                            div()
                                .px(px(8.))
                                .py(px(1.))
                                .rounded(px(9.))
                                .border_1()
                                .border_color(rgb(LINE))
                                .text_size(px(11.))
                                .child(a.clone())
                        })),
                )
            })
    }

    /// Everything that doesn't belong on a timeline, one tab at a time; only what the file has.
    fn study_dock(&mut self, s: &Study, bp: Option<&Blueprint>, cx: &mut Context<Self>) -> Div {
        let mut tabs = Vec::new();
        if !s.chords.is_empty() && s.nature.makeup != Makeup::Line {
            tabs.push((Dock::Harmony, "Harmony"));
        }
        if !s.groove.is_none() {
            tabs.push((Dock::Groove, "Groove"));
        }
        tabs.push((Dock::Parts, "What plays"));
        tabs.push((Dock::Mix, "Mix"));
        if s.lyrics.is_some() {
            tabs.push((Dock::Words, "Words"));
        }
        tabs.push((Dock::Known, "What's known"));
        if !s.genres.is_empty() {
            tabs.push((Dock::Books, "Playbooks"));
        }
        tabs.push((Dock::Compare, "Compare"));
        let open = match tabs.iter().any(|t| t.0 == self.studies.dock) {
            true => self.studies.dock,
            false => tabs[0].0,
        };
        let color = tint_of(s);
        let body = match open {
            Dock::Harmony => div().flex().child(harmony(s, color)),
            Dock::Groove => div().flex().child(rhythm(s, color)),
            Dock::Parts => instruments(s),
            Dock::Mix => self.study_mix(s, color),
            Dock::Words => self.study_words(s, color, cx),
            Dock::Known => bp.map_or_else(
                || div().text_color(rgb(TEXT_3)).child("Working it out…"),
                facts_view,
            ),
            Dock::Books => self.study_genres(s, cx),
            Dock::Compare => self.compare_card(&s.file, cx).unwrap_or_else(|| {
                div()
                    .text_size(px(13.))
                    .text_color(rgb(TEXT_3))
                    .child("Make a song like this and it's compared here, section by section.")
            }),
        };
        div()
            .flex_none()
            .h(relative(0.4))
            .min_h(px(260.))
            .flex()
            .flex_col()
            .bg(rgb(SURFACE))
            .border_t_1()
            .border_color(rgb(LINE_STRONG))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(px(2.))
                    .px(px(8.))
                    .pt(px(4.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .children(tabs.into_iter().map(|(tab, name)| {
                        let on = tab == open;
                        div()
                            .id(name)
                            .debug_selector(move || format!("study-tab-{}", name.to_lowercase()))
                            .px(px(10.))
                            .pt(px(4.))
                            .pb(px(6.))
                            .text_size(px(12.))
                            .text_color(rgb(if on { TEXT } else { TEXT_3 }))
                            .cursor_pointer()
                            .hover(|d| d.text_color(rgb(TEXT)))
                            .border_b_2()
                            .border_color(if on { rgb(TEXT).into() } else { alpha(0, 0.0) })
                            .child(name)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _, _, cx| {
                                    ws.studies.dock = tab;
                                    cx.notify();
                                }),
                            )
                    })),
            )
            .child(
                div()
                    .id("study-dock")
                    .debug_selector(|| "study-dock".into())
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p(px(14.))
                    .child(body),
            )
    }

    /// What's picked, and where each thing said of it comes from.
    fn study_side(&mut self, s: &Study, cx: &mut Context<Self>) -> Stateful<Div> {
        let pick = self.studies.pick;
        let (kicker, title, sub, rows, lines, sure, from): Reading = match pick {
            Pick::Tempo => (
                "Tempo".into(),
                format!("{:.0} BPM", s.tempo.bpm),
                String::new(),
                vec![],
                s.tempo.why.clone(),
                Some(s.tempo.sure),
                None,
            ),
            Pick::Key => (
                "Key".into(),
                s.key.name.clone(),
                s.key
                    .other
                    .clone()
                    .map_or(String::new(), |o| format!("or {o}")),
                vec![("Fit", format!("{:.0}%", s.key.fit * 100.0))],
                s.key.why.clone(),
                Some(s.key.sure),
                None,
            ),
            Pick::Length => (
                "Length".into(),
                clock(s.identity.seconds),
                String::new(),
                vec![
                    ("Bars", bars_of(s).to_string()),
                    (
                        "Meter",
                        format!(
                            "{}/{}",
                            s.tempo.time_signature[0], s.tempo.time_signature[1]
                        ),
                    ),
                    ("Sections", s.sections.len().to_string()),
                ],
                vec![],
                Some(Sure::Solid),
                None,
            ),
            Pick::Loudness => (
                "Loudness".into(),
                format!("{:.1} LUFS", s.mix.overall.lufs),
                String::new(),
                vec![
                    ("Loudest", format!("{:.1} LUFS", s.mix.overall.loudest_lufs)),
                    ("Crest", format!("{:.1} dB", s.mix.overall.crest_db)),
                    ("Range", format!("{:.1} dB", s.mix.range_db)),
                ],
                vec![],
                Some(Sure::Solid),
                None,
            ),
            Pick::Swing => (
                "Swing".into(),
                format!("{:.0}", s.groove.swing),
                format!("on the {}", s.groove.swing_on),
                vec![
                    (
                        "Syncopation",
                        format!("{:.0}%", s.groove.syncopation * 100.0),
                    ),
                    ("Nearest groove", capital(&s.groove.nearest)),
                ],
                vec![],
                Some(Sure::Likely),
                None,
            ),
            Pick::Chord(i) if s.chords.get(i).is_some() => {
                let c = &s.chords[i];
                let bar = s
                    .tempo
                    .bars
                    .iter()
                    .filter(|t| **t <= c.start + 0.05)
                    .count()
                    .max(1);
                let per = s.tempo.bars.get(1).map_or(2.0, |n| n - s.tempo.bars[0]);
                (
                    if s.nature.makeup == Makeup::Line {
                        "Note"
                    } else {
                        "Chord"
                    }
                    .into(),
                    c.chord.clone(),
                    c.numeral.clone(),
                    vec![
                        ("Starts", clock(c.start)),
                        ("Bar", bar.to_string()),
                        ("Lasts", format!("{:.1} bars", (c.end - c.start) / per)),
                    ],
                    vec![],
                    None,
                    Some(c.start),
                )
            }
            Pick::Section(i) if s.sections.get(i).is_some() => {
                let x = &s.sections[i];
                let parts: Vec<String> = s
                    .instruments
                    .iter()
                    .filter(|e| e.sections.contains(&i))
                    .map(part_name)
                    .collect();
                (
                    format!("Section {}", x.label),
                    x.name.clone(),
                    x.chords.clone(),
                    vec![
                        ("Bars", format!("{} to {}", x.bar + 1, x.bar + x.bars)),
                        ("Starts", clock(x.start)),
                        ("Loudness", format!("{:.1} LUFS", x.lufs)),
                    ],
                    parts,
                    None,
                    Some(x.start),
                )
            }
            _ => (
                "What it is".into(),
                s.nature.name.clone(),
                String::new(),
                vec![],
                s.nature.why.clone(),
                Some(s.nature.sure),
                None,
            ),
        };
        div()
            .id("study-side")
            .debug_selector(move || match pick {
                Pick::Section(_) => "study-section-detail".into(),
                _ => "study-side".into(),
            })
            .w(px(272.))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(16.))
            .bg(rgb(SURFACE))
            .border_l_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_3))
                    .child(kicker),
            )
            .child(
                div()
                    .text_size(px(24.))
                    .line_height(px(28.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .when(!sub.is_empty(), |d| {
                d.child(
                    div()
                        .font_family(MONO)
                        .text_size(px(12.))
                        .text_color(rgb(ACCENT_TEXT))
                        .child(sub),
                )
            })
            .when_some(sure, |d, sure| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(rgb(TEXT_3))
                        .child(sure_dot(sure))
                        .child(sure_word(sure)),
                )
            })
            .children(rows.into_iter().map(|(k, v)| {
                div()
                    .flex()
                    .justify_between()
                    .gap(px(12.))
                    .text_size(px(12.))
                    .child(div().text_color(rgb(TEXT_3)).child(k))
                    .child(div().font_family(MONO).child(v))
            }))
            .children(lines.into_iter().map(|l| {
                div()
                    .text_size(px(12.5))
                    .line_height(px(17.))
                    .text_color(rgb(TEXT_2))
                    .child(l)
            }))
            .when_some(from, |d, from| {
                d.child(div().pt(px(4.)).flex().child(
                    act("Hear from here", "study-hear", false).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| ws.study_play(from, cx)),
                    ),
                ))
            })
    }
}
