//! Grow: a seed grown to sound like a recording. Choose one, pick the stretch to match (up to two
//! seconds), and four strands grow up the stage, each a population of seeds of its own kind,
//! every new best it finds a dot on it. Click a dot to hear it on the keys; plant the one you
//! like and grow branches from it. It all runs on this machine, a strand to a core.

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use gpui::{PathBuilder, PathPromptOptions, hsla};
use pina_engine::dsp::bloom::grow::{
    BANDS, Ear, Find, GENERATIONS, GROW_RATE, Grower, Heard, MOST_RECORDINGS, MOST_SECONDS,
    Recording, closeness, distance, read_recording, recording, render,
};

use super::*;

/// Where each find's dot was drawn, by (strand, find): its place on the stage, 0 to 1.
pub(crate) type Dots = Rc<RefCell<Vec<((usize, usize), (f32, f32))>>>;

/// The four strands' colours.
const STRAND_HUES: [f32; 4] = [0.36, 0.55, 0.08, 0.84];

/// What the growing has to show, shared with the thread doing it.
#[derive(Default)]
pub(crate) struct Grown {
    pub generation: usize,
    pub done: bool,
    pub strands: Vec<(SeedKind, Vec<Find>)>,
}

/// A recording grown toward: the file, all of it (mono at the growing rate) and its outline, where
/// the stretch heard starts, and that stretch as it's heard.
pub(crate) struct Take {
    pub file: PathBuf,
    pub audio: Arc<Vec<f32>>,
    pub peaks: Vec<f32>,
    pub from: f32,
    pub recording: Recording,
    pub target: Heard,
}

impl Take {
    pub fn new(file: PathBuf, audio: Vec<f32>, long: f32) -> Take {
        let r = recording(&audio, 0.0, long);
        Take {
            file,
            peaks: outline(&audio, 480),
            audio: Arc::new(audio),
            from: 0.0,
            target: Ear::new().hear(&r.audio),
            recording: r,
        }
    }

    /// The stretch heard again, from where it now starts and as long as `long`.
    fn hear(&mut self, long: f32) {
        let secs = self.audio.len() as f32 / GROW_RATE as f32;
        self.from = self.from.min((secs - long).max(0.0));
        self.recording = recording(&self.audio, self.from, long);
        self.target = Ear::new().hear(&self.recording.audio);
    }

    pub fn name(&self) -> String {
        self.file
            .file_stem()
            .map_or("Recording".into(), |n| n.to_string_lossy().into_owned())
    }
}

pub(crate) struct GrowView {
    /// What's grown toward (several at different notes, or soft and hard), and the one shown.
    pub takes: Vec<Take>,
    pub take: usize,
    pub long: f32,
    pub grown: Option<Arc<Mutex<Grown>>>,
    pub stop: Arc<AtomicBool>,
    /// The find heard (strand, find), and how it's heard, for the comparison.
    pub picked: Option<(usize, usize)>,
    pub picked_heard: Option<Heard>,
    /// How alike the find tried is to each recording: its note, and that.
    pub picked_each: Vec<(u8, f32)>,
    /// When each find (strand, generation) first showed, so it opens as it arrives.
    pub born: Rc<RefCell<std::collections::HashMap<(usize, usize), f32>>>,
    /// The plant as it was before a find was first tried, kept among its ancestors once.
    pub tried: bool,
    pub stage: Rc<Cell<Bounds<Pixels>>>,
    pub wave: Rc<Cell<Bounds<Pixels>>>,
    pub dots: Dots,
    pub loading: bool,
}

impl GrowView {
    pub fn new(_cx: &mut Context<Workspace>) -> (GrowView, Vec<Subscription>) {
        (
            GrowView {
                takes: Vec::new(),
                take: 0,
                long: 1.5,
                grown: None,
                stop: Arc::new(AtomicBool::new(false)),
                picked: None,
                picked_heard: None,
                picked_each: Vec::new(),
                born: Default::default(),
                tried: false,
                stage: Default::default(),
                wave: Default::default(),
                dots: Default::default(),
                loading: false,
            },
            Vec::new(),
        )
    }

    pub fn running(&self) -> bool {
        self.grown
            .as_ref()
            .is_some_and(|g| g.lock().map(|g| !g.done).unwrap_or(false))
    }

    /// The take shown.
    pub fn current(&self) -> Option<&Take> {
        self.takes.get(self.take)
    }

    pub fn seconds(&self) -> f32 {
        self.current()
            .map_or(0.0, |t| t.audio.len() as f32 / GROW_RATE as f32)
    }

    fn find(&self, (s, f): (usize, usize)) -> Option<Find> {
        let g = self.grown.as_ref()?.lock().ok()?;
        g.strands.get(s)?.1.get(f).cloned()
    }
}

/// A recording's outline: the loudest of each of `n` stretches, the loudest of all at 1.
fn outline(x: &[f32], n: usize) -> Vec<f32> {
    let per = (x.len() / n.max(1)).max(1);
    let peaks: Vec<f32> = x
        .chunks(per)
        .map(|c| c.iter().fold(0f32, |m, v| m.max(v.abs())))
        .collect();
    let top = peaks.iter().fold(1e-6f32, |m, p| m.max(*p));
    peaks.iter().map(|p| p / top).collect()
}

impl Workspace {
    fn grow_view(&mut self) -> Option<&mut GrowView> {
        self.bloom_view.as_mut().map(|v| &mut v.grow)
    }

    /// Whether `at` is on Bloom's window with its Grow page open: a sound dropped there is
    /// grown toward.
    pub(crate) fn over_bloom_growing(&self, at: gpui::Point<f32>) -> bool {
        let Some(v) = self
            .bloom_view
            .as_ref()
            .filter(|v| v.page == BloomPage::Grow)
        else {
            return false;
        };
        let (w, h) = self.bloom_size();
        (v.at.0..v.at.0 + w).contains(&at.x) && (v.at.1..v.at.1 + h).contains(&at.y)
    }

    pub(crate) fn bloom_pick_recording(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Grow from this".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |ws, cx| ws.bloom_load_recording(path, cx))
                .ok();
        })
        .detach();
    }

    /// A recording read (off the main thread) to grow toward.
    pub(crate) fn bloom_load_recording(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(g) = self.grow_view() {
            g.loading = true;
            g.stop.store(true, Ordering::Relaxed);
        }
        cx.notify();
        let read = cx.background_spawn({
            let path = path.clone();
            async move { read_recording(&path) }
        });
        cx.spawn(async move |this, cx| {
            let audio = read.await;
            this.update(cx, |ws, cx| {
                match audio {
                    Ok(audio) => {
                        if let Some(g) = ws.grow_view() {
                            let take = Take::new(path, audio, g.long);
                            // Another, up to four; past that, in place of the one shown.
                            match g.takes.len() < MOST_RECORDINGS {
                                true => {
                                    g.takes.push(take);
                                    g.take = g.takes.len() - 1;
                                }
                                false => g.takes[g.take] = take,
                            }
                            g.grown = None;
                            g.picked = None;
                            g.picked_heard = None;
                            g.loading = false;
                        }
                    }
                    Err(e) => {
                        if let Some(g) = ws.grow_view() {
                            g.loading = false;
                        }
                        ws.status = Some(format!("Couldn't read that: {e}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Every take's stretch as long as `long`, heard again.
    fn bloom_grow_long(&mut self, long: f32) {
        let Some(g) = self.grow_view() else { return };
        g.long = long.min(MOST_SECONDS);
        for t in &mut g.takes {
            t.hear(g.long);
        }
    }

    /// Where the shown take's stretch starts, from a click on its outline.
    fn bloom_grow_from(&mut self, x: f32) {
        let Some(g) = self.grow_view() else { return };
        let b = g.wave.get();
        let at = ((x - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0);
        let (secs, long, k) = (g.seconds(), g.long, g.take);
        if let Some(t) = g.takes.get_mut(k) {
            t.from = (at * secs - long * 0.5).clamp(0.0, (secs - long).max(0.0));
            t.hear(long);
        }
    }

    /// A take shown, or taken out.
    pub(crate) fn bloom_take(&mut self, k: usize, out: bool, cx: &mut Context<Self>) {
        let Some(g) = self.grow_view() else { return };
        if out && k < g.takes.len() {
            g.takes.remove(k);
            g.grown = None;
            g.picked = None;
            g.picked_heard = None;
        }
        g.take = match out {
            true => g.take.min(g.takes.len().saturating_sub(1)),
            false => k.min(g.takes.len().saturating_sub(1)),
        };
        cx.notify();
    }

    pub(crate) fn bloom_grow_start(&mut self, cx: &mut Context<Self>) {
        let luck = self.bloom_view.as_mut().map_or(1, |v| {
            v.luck = v.luck.wrapping_add(0x7f4a_7c15);
            v.luck
        });
        let Some(g) = self.grow_view() else { return };
        if g.running() {
            g.stop.store(true, Ordering::Relaxed);
            return cx.notify();
        }
        let rs: Vec<Recording> = g.takes.iter().map(|t| t.recording.clone()).collect();
        if rs.is_empty() {
            return;
        }
        let grown = Arc::new(Mutex::new(Grown::default()));
        let stop = Arc::new(AtomicBool::new(false));
        g.grown = Some(grown.clone());
        g.stop = stop.clone();
        g.born.borrow_mut().clear();
        g.picked = None;
        g.picked_heard = None;
        std::thread::Builder::new()
            .name("bloom-grow".into())
            .spawn(move || {
                let mut grower = Grower::new(rs, luck);
                let mut ears: Vec<Ear> = (0..grower.strands.len()).map(|_| Ear::new()).collect();
                while !grower.done() && !stop.load(Ordering::Relaxed) {
                    grower.step(&mut ears);
                    if let Ok(mut g) = grown.lock() {
                        g.generation = grower.generation;
                        g.strands = grower
                            .strands
                            .iter()
                            .map(|s| (s.kind, s.finds.clone()))
                            .collect();
                    }
                }
                if let Ok(mut g) = grown.lock() {
                    g.done = true;
                }
            })
            .ok();
        self.wake_bloom();
        cx.notify();
    }

    /// A find tried: the plant's seed, every branch cut back so every key plays it, the shown
    /// recording's note played. The plant before the first one tried is kept among its ancestors.
    /// How alike it is to each recording is worked out, as it really sounds.
    pub(crate) fn bloom_try_find(&mut self, t: usize, at: (usize, usize), cx: &mut Context<Self>) {
        let Some(g) = self.grow_view() else { return };
        let Some(find) = g.find(at) else { return };
        let Some(r) = g.current().map(|t| t.recording.clone()) else {
            return;
        };
        let first = !std::mem::replace(&mut g.tried, true);
        g.picked = Some(at);
        let mut ear = Ear::new();
        g.picked_each = g
            .takes
            .iter()
            .map(|take| {
                let heard = ear.hear(&render(&find.genes, &take.recording));
                (
                    take.recording.note,
                    closeness(distance(&take.target, &heard)),
                )
            })
            .collect();
        g.picked_heard = Some(ear.hear(&render(&find.genes, &r)));
        self.edit(cx, |ws| {
            let Some(b) = ws.bloom_mut(t) else { return };
            if first {
                b.remember();
            }
            b.set_seed(&find.genes);
            for k in 0..BRANCHES {
                b.params[branch_length(k)] = 0.0;
            }
            b.sound = format!("Grown from {}", note_name(r.note));
        });
        self.bloom_play_held(r.note, r.hold, cx);
    }

    /// The seed played at `note` as long as a recording held it.
    fn bloom_play_held(&mut self, note: u8, hold: usize, cx: &mut Context<Self>) {
        self.play_note(note, 0);
        self.play_note(note, 110);
        let ms = (hold as f32 / GROW_RATE as f32 * 1000.0) as u64;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(ms.clamp(120, 2500)))
                .await;
            this.update(cx, |ws, _| ws.play_note(note, 0)).ok();
        })
        .detach();
    }

    /// Recording `k` heard: the stretch it's grown toward, through the same preview the
    /// Browser's samples play through.
    pub(crate) fn bloom_hear_take(&mut self, k: usize) {
        let Some(audio) = self
            .bloom_view
            .as_ref()
            .and_then(|v| v.grow.takes.get(k))
            .map(|t| t.recording.audio.clone())
        else {
            return;
        };
        let rate = self.player.rate();
        let at_rate = pina_engine::audio::engine::resample(&audio, GROW_RATE, rate);
        let data = Arc::new(pina_engine::audio::engine::AudioData::new(vec![at_rate]));
        if let Err(e) = self.player.audition_audio(data, 0.0, 0.0) {
            self.status = Some(e);
        }
    }

    /// The seed tried heard at the shown recording's note.
    pub(crate) fn bloom_hear_seed(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self
            .bloom_view
            .as_ref()
            .and_then(|v| v.grow.current())
            .map(|t| (t.recording.note, t.recording.hold))
        else {
            return;
        };
        self.bloom_play_held(r.0, r.1, cx);
    }

    /// The find tried, planted: its branches sprout.
    pub(crate) fn bloom_plant_find(&mut self, t: usize, cx: &mut Context<Self>) {
        if let Some(g) = self.grow_view() {
            g.tried = false;
        }
        let luck = self.bloom_view.as_ref().map_or(3, |v| v.luck);
        self.edit(cx, |ws| {
            if let Some(b) = ws.bloom_mut(t) {
                b.replant(luck);
                b.ancestors.pop();
                b.generation = b.generation.saturating_sub(1);
            }
        });
        if let Some(v) = self.bloom_view.as_mut() {
            v.page = BloomPage::Garden;
            for k in 0..BRANCHES {
                v.shown[k] = 0.0;
                v.speed[k] = -0.6 * k as f32;
            }
        }
        self.status = Some("Planted: grow its branches to find what's near it".into());
        cx.notify();
    }

    pub(crate) fn bloom_grow_page(&mut self, t: usize, _b: &Bloom, cx: &mut Context<Self>) -> Div {
        let Some(v) = self.bloom_view.as_ref() else {
            return div();
        };
        let g = &v.grow;
        let running = g.running();
        let time = self.bloom_time();
        let (generation, strands) = g
            .grown
            .as_ref()
            .and_then(|x| x.lock().ok().map(|x| (x.generation, x.strands.clone())))
            .unwrap_or_default();
        let best = strands
            .iter()
            .enumerate()
            .flat_map(|(s, (_, f))| f.iter().enumerate().map(move |(k, f)| ((s, k), f.close)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let picked = g.picked.or(best.map(|b| b.0));
        let picked_find = picked.and_then(|(s, k)| strands.get(s)?.1.get(k).cloned());
        let cur = g.current();
        let empty = g.takes.is_empty();
        let room = g.takes.len() < MOST_RECORDINGS;
        let (shown, long) = (g.take, g.long);
        // The recordings: each heard, shown, taken out; the one shown, its stretch.
        let secs = g.seconds().max(1e-3);
        let takes: Vec<Div> = g
            .takes
            .iter()
            .enumerate()
            .map(|(k, take)| {
                let on = k == shown;
                let peaks = take.peaks.clone();
                let (from, span) = (take.from / secs, (long / secs).min(1.0));
                let wave_bounds = g.wave.clone();
                let wave = crate::ui::scale::scaled_canvas(
                    move |bb, _, _| {
                        if on {
                            wave_bounds.set(bb)
                        }
                    },
                    move |bb, _, window, _| paint_outline(window, bb, &peaks, from, span, on),
                )
                .size_full();
                let play = if on {
                    hsla(0.39, 0.6, 0.62, 1.0)
                } else {
                    hsla(0.0, 0.0, 0.45, 1.0)
                };
                div()
                    .p(px(9.))
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .rounded(px(11.))
                    .border_1()
                    .border_color(if on {
                        hsla(0.39, 0.5, 0.55, 0.75)
                    } else {
                        gpui::rgb(LINE).into()
                    })
                    .bg(if on {
                        hsla(0.39, 0.4, 0.3, 0.12)
                    } else {
                        gpui::rgb(CARD).into()
                    })
                    .cursor_pointer()
                    .debug_selector(move || format!("bloom-take-{k}"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(
                                round_button(play, true)
                                    .debug_selector(move || format!("bloom-take-hear-{k}"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.bloom_hear_take(k);
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family(MONO)
                                    .text_size(px(11.))
                                    .text_color(hsla(0.39, 0.5, 0.72, 1.0))
                                    .child(note_name(take.recording.note)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_size(px(11.5))
                                    .text_color(rgb(TEXT_2))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(take.name()),
                            )
                            .child(
                                div()
                                    .size(px(20.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(5.))
                                    .hover(|d| d.bg(rgb(RAISED)))
                                    .debug_selector(move || format!("bloom-take-out-{k}"))
                                    .child(icon(Icon::Close, 10.).text_color(rgb(TEXT_3)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.bloom_take(k, true, cx);
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h(px(if on { 46. } else { 24. }))
                            .rounded(px(6.))
                            .bg(rgb(WELL))
                            .overflow_hidden()
                            .debug_selector(move || {
                                if on {
                                    "bloom-grow-wave".to_string()
                                } else {
                                    format!("bloom-take-wave-{k}")
                                }
                            })
                            .child(wave)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    match on {
                                        true => ws.bloom_grow_from(pt(e.position).x),
                                        false => ws.bloom_take(k, false, cx),
                                    }
                                    cx.notify();
                                }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            ws.bloom_take(k, false, cx);
                        }),
                    )
            })
            .collect();
        let lengths = [0.5f32, 1.0, 1.5, 2.0].map(|l| {
            seg(format!("{l}"), (long - l).abs() < 0.01, 22.).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.bloom_grow_long(l);
                    cx.notify();
                }),
            )
        });
        let heard = cur.map(|c| {
            let r = &c.recording;
            let hold = r.hold as f32 / GROW_RATE as f32;
            match r.hz {
                Some(hz) => format!(
                    "{} heard at {hz:.0} Hz, let go after {hold:.2} s",
                    note_name(r.note)
                ),
                None => format!(
                    "No clear pitch: grown on {}, let go after {hold:.2} s",
                    note_name(r.note)
                ),
            }
        });
        let left = div()
            .w(px(262.))
            .flex_none()
            .h_full()
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .border_r_1()
            .border_color(rgb(0x1f2124))
            .bg(rgb(0x131416))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(caps("Toward"))
                    .child(small(format!("{} of {MOST_RECORDINGS}", g.takes.len()))),
            )
            .children(takes)
            .when(room, |d| {
                d.child(
                    div()
                        .h(px(38.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(11.))
                        .border_1()
                        .border_dashed()
                        .border_color(rgb(0x3a3a44))
                        .text_size(px(11.5))
                        .text_color(rgb(TEXT_3))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(CARD)))
                        .debug_selector(|| "bloom-grow-choose".into())
                        .child(match (g.loading, empty) {
                            (true, _) => "Reading…",
                            (false, true) => "Drop a recording, or choose one",
                            (false, false) => "+ Another: drop or choose",
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.bloom_pick_recording(cx);
                            }),
                        ),
                )
            })
            .when(!empty, |d| {
                d.child(
                    div()
                        .mt(px(4.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(small("Hear"))
                        .child(tray().p(px(2.)).rounded(px(7.)).children(lengths))
                        .child(small("s")),
                )
            })
            .children(heard.map(|h| div().text_size(px(10.5)).text_color(rgb(TEXT_2)).child(h)))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(TEXT_3))
                    .child(match g.takes.len() {
                        0 | 1 => "Runs here, on this machine: four swarms of seeds, each of its own kind, closing in on the sound. A single note works best.",
                        _ => "Several at once: the seed grown to sound like all of them, each at its own note and as hard as it's loud. Runs here, on this machine.",
                    }),
            );
        // The target: the recording's own spectrum in the middle, the swarms closing in.
        let fingerprint: Vec<f32> = cur.map_or_else(Vec::new, |c| {
            let h = &c.target;
            (0..BANDS)
                .map(|b| {
                    let loud: Vec<f32> = h.frames.iter().map(|f| f[b]).collect();
                    let avg = loud.iter().sum::<f32>() / loud.len().max(1) as f32;
                    ((avg + 70.0) / 70.0).clamp(0.0, 1.0)
                })
                .collect()
        });
        let stage_bounds = g.stage.clone();
        let dots = g.dots.clone();
        let born = g.born.clone();
        let target_strands = strands.clone();
        let progress = generation as f32 / GENERATIONS as f32;
        let canvas = crate::ui::scale::scaled_canvas(
            move |bb, _, _| stage_bounds.set(bb),
            move |bb, _, window, _| {
                paint_target(
                    window,
                    bb,
                    time,
                    progress,
                    &fingerprint,
                    &target_strands,
                    picked,
                    running,
                    empty,
                    &dots,
                    &born,
                )
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let half = TARGET / 2.0;
        let rings = [(0.5f32, "50%"), (0.7, "70%"), (0.85, "85%")].map(|(c, l)| {
            div()
                .absolute()
                .left(px(half + 6.0))
                .top(px(half - ring_of(c) * half * 0.94 - 14.0))
                .font_family(MONO)
                .text_size(px(10.))
                .text_color(rgb(0x5d6b63))
                .child(l)
        });
        let each_mean = (!g.picked_each.is_empty())
            .then(|| g.picked_each.iter().map(|e| e.1).sum::<f32>() / g.picked_each.len() as f32);
        let tried_label = picked.zip(picked_find.as_ref()).map(|((s, k), f)| {
            let _ = k;
            let (x, y) = swarm_at(s, f.generation, f.close, time);
            let kind = strands.get(s).map_or("", |x| x.0.name());
            div()
                .absolute()
                .left(px(half + x * half * 0.94 + 16.0))
                .top(px(half + y * half * 0.94 - 30.0))
                .px(px(8.))
                .py(px(3.))
                .rounded(px(7.))
                .bg(alpha(0x0a0c0b, 0.92))
                .border_1()
                .border_color(hsla(STRAND_HUES[s % 4], 0.65, 0.6, 0.6))
                .text_size(px(11.))
                .text_color(rgb(TEXT_2))
                .whitespace_nowrap()
                .child(format!(
                    "{kind} · gen {} · {:.0}%",
                    f.generation,
                    each_mean
                        .filter(|_| g.picked == Some((s, k)))
                        .unwrap_or(f.close)
                        * 100.0
                ))
        });
        let target = div()
            .relative()
            .flex_none()
            .w(px(TARGET))
            .h(px(TARGET))
            .debug_selector(|| "bloom-grow-stage".into())
            .child(canvas)
            .when(!empty, |d| d.children(rings))
            .children(tried_label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let Some(g) = ws.bloom_view.as_ref().map(|v| &v.grow) else {
                        return;
                    };
                    let b = g.stage.get();
                    let (w, h) = (
                        f32::from(b.size.width).max(1.0),
                        f32::from(b.size.height).max(1.0),
                    );
                    let p = pt(e.position);
                    let (x, y) = (
                        (p.x - f32::from(b.origin.x)) / w,
                        (p.y - f32::from(b.origin.y)) / h,
                    );
                    // The eye: growing starts or stops.
                    if ((x - 0.5) * w).hypot((y - 0.5) * h) < EYE * w * 0.5 {
                        return ws.bloom_grow_start(cx);
                    }
                    let hit = g
                        .dots
                        .borrow()
                        .iter()
                        .map(|&(at, (dx, dy))| (at, ((dx - x) * w).hypot((dy - y) * h)))
                        .filter(|(_, d)| *d < 20.0)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|x| x.0);
                    if let Some(at) = hit {
                        ws.bloom_try_find(t, at, cx);
                    }
                }),
            );
        let legend = (0..4).filter_map(|s| {
            let (kind, finds) = strands.get(s)?;
            let close = finds.last().map_or(0.0, |f| f.close);
            let on = picked.is_some_and(|p| p.0 == s);
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(14.))
                    .bg(rgb(0x15181a))
                    .border_1()
                    .border_color(if on {
                        hsla(STRAND_HUES[s], 0.65, 0.6, 0.6)
                    } else {
                        gpui::rgb(LINE).into()
                    })
                    .text_size(px(11.5))
                    .child(div().size(px(8.)).rounded_full().bg(hsla(
                        STRAND_HUES[s],
                        0.65,
                        0.62,
                        1.0,
                    )))
                    .child(kind.name())
                    .child(
                        div()
                            .font_family(MONO)
                            .text_color(rgb(if on { TEXT_2 } else { TEXT_3 }))
                            .child(format!("{:.0}%", close * 100.0)),
                    ),
            )
        });
        let center = div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(rgb(0x0b0e0c))
            .child(target)
            .child(
                div()
                    .absolute()
                    .top(px(12.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(small(match (empty, running, generation) {
                        (true, _, _) => "Drop a recording, or choose one on the left".to_string(),
                        (false, false, 0) => "Click the seed to grow it toward the sound".to_string(),
                        (false, true, n) => format!("Generation {n} of {GENERATIONS} · the nearer the middle, the more alike · click the seed to stop"),
                        (false, false, n) => format!("Grown: {n} generations · click a seed to hear it"),
                    })),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(12.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .gap(px(8.))
                    .children(legend),
            );
        // The seed tried, against the recordings.
        let target_heard = cur.map(|t| t.target.clone());
        let mine = g.picked_heard.clone();
        let each = g.picked_each.clone();
        // As it really sounds against each, once tried; till then as the growing heard it.
        let close = match each.is_empty() {
            true => picked_find.as_ref().map(|f| f.close),
            false => Some(each.iter().map(|e| e.1).sum::<f32>() / each.len() as f32),
        };
        let hear = |label: &'static str,
                    sel: &'static str,
                    hue: f32,
                    seed: bool,
                    cx: &mut Context<Self>| {
            div()
                .flex()
                .items_center()
                .gap(px(7.))
                .child(
                    round_button(hsla(hue, 0.65, 0.62, 1.0), true)
                        .debug_selector(move || sel.into())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                match seed {
                                    true => ws.bloom_hear_seed(cx),
                                    false => {
                                        let k = ws.bloom_view.as_ref().map_or(0, |v| v.grow.take);
                                        ws.bloom_hear_take(k);
                                    }
                                }
                            }),
                        ),
                )
                .child(small(label))
        };
        let right = div()
            .w(px(292.))
            .flex_none()
            .h_full()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .border_l_1()
            .border_color(rgb(0x1f2124))
            .bg(rgb(0x131416))
            .child(caps("The seed tried"))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(10.))
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(40.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(hsla(0.55, 0.8, 0.72, 1.0))
                            .child(close.map_or("—".to_string(), |c| format!("{:.0}%", c * 100.0))),
                    )
                    .child(small(match g.takes.len() {
                        0 | 1 => "alike".to_string(),
                        2 => "alike to both".to_string(),
                        n => format!("alike to all {n}"),
                    })),
            )
            .when(each.len() > 1, |d| {
                d.child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .children(each.iter().map(|(note, c)| {
                            div()
                                .flex_1()
                                .py(px(6.))
                                .rounded(px(7.))
                                .bg(rgb(0x1a1d1f))
                                .flex()
                                .flex_col()
                                .items_center()
                                .child(small(note_name(*note)))
                                .child(
                                    div()
                                        .font_family(MONO)
                                        .text_size(px(11.))
                                        .text_color(rgb(TEXT_2))
                                        .child(format!("{:.0}%", c * 100.0)),
                                )
                        })),
                )
            })
            .child(hear(
                "The recording",
                "bloom-hear-recording",
                0.39,
                false,
                cx,
            ))
            .child(spectrogram(target_heard, 0.36))
            .child(hear("The seed", "bloom-hear-seed", 0.55, true, cx))
            .child(spectrogram(mine, 0.55))
            .child(div().flex_1())
            .child(
                button("Plant it", g.picked.is_some())
                    .h(px(40.))
                    .bg(alpha(0x3ddc97, if g.picked.is_some() { 0.2 } else { 0.06 }))
                    .border_color(rgb(0x3f8a62))
                    .debug_selector(|| "bloom-grow-plant".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _, _, cx| {
                            cx.stop_propagation();
                            let tried = ws
                                .bloom_view
                                .as_ref()
                                .is_some_and(|v| v.grow.picked.is_some());
                            if tried {
                                ws.bloom_plant_find(t, cx);
                            } else {
                                ws.status =
                                    Some("Click a seed in the swarms to try it first".into());
                                cx.notify();
                            }
                        }),
                    ),
            )
            .child(small("Click any seed in the swarms to hear it"));
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .child(left)
            .child(center)
            .child(right)
    }
}

/// The target's side, and its eye's share of it.
const TARGET: f32 = 660.0;
const EYE: f32 = 0.15;

/// How far out a find sits, a share of the target's radius: nearer the eye the more alike.
fn ring_of(close: f32) -> f32 {
    const AT: [(f32, f32); 5] = [
        (0.3, 0.97),
        (0.5, 0.88),
        (0.7, 0.64),
        (0.85, 0.4),
        (1.0, 0.18),
    ];
    let c = close.clamp(AT[0].0, 1.0);
    for w in AT.windows(2) {
        if c <= w[1].0 {
            let t = (c - w[0].0) / (w[1].0 - w[0].0);
            return w[0].1 + (w[1].1 - w[0].1) * t;
        }
    }
    AT[4].1
}

/// Where find `k` of strand `s` sits, the target's units (-1 to 1 from its middle): round its own
/// spiral, turning slowly.
fn swarm_at(s: usize, generation: usize, close: f32, t: f32) -> (f32, f32) {
    let r = ring_of(close);
    let turn = if s.is_multiple_of(2) { 1.0 } else { -1.0 };
    // Further round the later it came, so a strand's finds trace a spiral in, each keeping its
    // place; the whole drifting so slowly it's only seen over minutes.
    // Most finds come early: the square root spreads them out round the spiral.
    let late = (generation as f32 / GENERATIONS as f32).sqrt();
    let a = s as f32 * std::f32::consts::FRAC_PI_2 + turn * late * 5.2 + t * 0.012 * turn;
    (r * a.cos(), r * a.sin())
}

/// A recording's outline, the stretch heard lit.
fn paint_outline(
    window: &mut gpui::Window,
    bb: Bounds<Pixels>,
    peaks: &[f32],
    from: f32,
    span: f32,
    on: bool,
) {
    let w = f32::from(bb.size.width);
    let h = f32::from(bb.size.height);
    let mid = f32::from(bb.origin.y) + h / 2.0;
    let x0 = f32::from(bb.origin.x);
    let (a, z) = (from, (from + span).min(1.0));
    if on {
        window.paint_quad(gpui::fill(
            Bounds::new(
                point(gpui::px(x0 + a * w), bb.origin.y),
                gpui::size(gpui::px((z - a) * w), bb.size.height),
            ),
            hsla(0.39, 0.5, 0.5, 0.16),
        ));
    }
    let n = peaks.len().max(1);
    for (i, p) in peaks.iter().enumerate() {
        let f = i as f32 / n as f32;
        let tall = (p * h * 0.86).max(1.0);
        let lit = on && f >= a && f <= z;
        window.paint_quad(gpui::fill(
            Bounds::new(
                point(gpui::px(x0 + f * w), gpui::px(mid - tall / 2.0)),
                gpui::size(gpui::px((w / n as f32 - 0.4).max(0.6)), gpui::px(tall)),
            ),
            match (lit, on) {
                (true, _) => hsla(0.39, 0.6, 0.66, 0.95),
                (false, true) => hsla(0.0, 0.0, 0.6, 0.4),
                _ => hsla(0.39, 0.3, 0.55, 0.55),
            },
        ));
    }
}

/// A round play button.
fn round_button(color: gpui::Hsla, play: bool) -> Div {
    let glyph = crate::ui::scale::scaled_canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let c = b.center();
            let s = f32::from(b.size.width) * 0.22;
            let mut p = PathBuilder::fill();
            match play {
                true => {
                    p.move_to(point(c.x - gpui::px(s * 0.7), c.y - gpui::px(s)));
                    p.line_to(point(c.x + gpui::px(s), c.y));
                    p.line_to(point(c.x - gpui::px(s * 0.7), c.y + gpui::px(s)));
                    p.close();
                }
                false => {
                    p.move_to(point(c.x - gpui::px(s), c.y - gpui::px(s)));
                    p.line_to(point(c.x + gpui::px(s), c.y - gpui::px(s)));
                    p.line_to(point(c.x + gpui::px(s), c.y + gpui::px(s)));
                    p.line_to(point(c.x - gpui::px(s), c.y + gpui::px(s)));
                    p.close();
                }
            }
            if let Ok(path) = p.build() {
                window.paint_path(path, rgb(0x0b0d0c));
            }
        },
    )
    .size_full();
    div()
        .size(px(24.))
        .flex_none()
        .rounded_full()
        .bg(color)
        .cursor_pointer()
        .child(glyph)
}

fn caps(s: &'static str) -> Div {
    div()
        .text_size(px(10.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_3))
        .child(s.to_uppercase())
}

/// A sound as it's heard, drawn: time across, bands up, louder brighter.
fn spectrogram(h: Option<Heard>, hue: f32) -> Div {
    let canvas = crate::ui::scale::scaled_canvas(
        |_, _, _| {},
        move |bb, _, window, _| {
            let Some(h) = &h else { return };
            let cols = h.frames.len().clamp(1, 64);
            let per = (h.frames.len() as f32 / cols as f32).max(1.0);
            let (w, ht) = (f32::from(bb.size.width), f32::from(bb.size.height));
            let (cw, rh) = (w / cols as f32, ht / BANDS as f32);
            for c in 0..cols {
                let f = &h.frames[((c as f32 * per) as usize).min(h.frames.len() - 1)];
                for (b, db) in f.iter().enumerate() {
                    let v = ((db + 70.0) / 70.0).clamp(0.0, 1.0);
                    if v < 0.05 {
                        continue;
                    }
                    window.paint_quad(gpui::fill(
                        Bounds::new(
                            point(
                                bb.origin.x + gpui::px(c as f32 * cw),
                                bb.origin.y + gpui::px(ht - (b + 1) as f32 * rh),
                            ),
                            gpui::size(gpui::px(cw + 0.5), gpui::px(rh + 0.5)),
                        ),
                        hsla(hue + 0.08 * v, 0.7, 0.12 + 0.6 * v, 0.2 + 0.8 * v),
                    ));
                }
            }
        },
    )
    .size_full();
    div()
        .h(px(74.))
        .rounded(px(7.))
        .overflow_hidden()
        .bg(rgb(WELL))
        .border_1()
        .border_color(rgb(LINE))
        .child(canvas)
}

/// The target: rings of how alike, the generations round its edge, the recording's spectrum as
/// rays round the eye, each strand's finds a swarm spiralling in (the one tried ringed), and the
/// seed in the eye, its glyph to grow or stop.
#[allow(clippy::too_many_arguments)]
fn paint_target(
    window: &mut gpui::Window,
    b: Bounds<Pixels>,
    t: f32,
    progress: f32,
    fingerprint: &[f32],
    strands: &[(SeedKind, Vec<Find>)],
    picked: Option<(usize, usize)>,
    running: bool,
    empty: bool,
    dots: &Dots,
    born: &Rc<RefCell<std::collections::HashMap<(usize, usize), f32>>>,
) {
    let unit = crate::ui::scale::scale();
    let half = f32::from(b.size.width) / 2.0;
    let r_all = half * 0.94;
    let (cx0, cy0) = (f32::from(b.origin.x) + half, f32::from(b.origin.y) + half);
    let at = |(x, y): (f32, f32)| (cx0 + x * r_all, cy0 + y * r_all);
    let pt = |(x, y): (f32, f32)| gpui::point(gpui::px(x), gpui::px(y));
    let circle = |p: &mut PathBuilder, r: f32, from: f32, to: f32| {
        let n = 96;
        for i in 0..=n {
            let a = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * (from + (to - from) * i as f32 / n as f32);
            let q = pt((cx0 + r * a.cos(), cy0 + r * a.sin()));
            if i == 0 {
                p.move_to(q);
            } else {
                p.line_to(q);
            }
        }
    };
    // The ground's glow.
    window.paint_drop_shadows(
        Bounds::centered_at(
            pt((cx0, cy0)),
            gpui::size(gpui::px(r_all * 0.6), gpui::px(r_all * 0.6)),
        ),
        gpui::Corners::all(gpui::px(r_all * 0.3)),
        &[gpui::BoxShadow {
            color: hsla(0.38, 0.45, 0.3, 0.22),
            offset: gpui::point(gpui::px(0.), gpui::px(0.)),
            blur_radius: gpui::px(r_all * 0.6),
            spread_radius: gpui::px(r_all * 0.15),
            inset: false,
        }],
    );
    // Rings: how alike.
    let mut rings = PathBuilder::stroke(gpui::px(1.0 * unit));
    for c in [0.5f32, 0.7, 0.85] {
        circle(&mut rings, ring_of(c) * r_all, 0.0, 1.0);
    }
    if let Ok(p) = rings.build() {
        window.paint_path(p, hsla(0.4, 0.3, 0.6, if empty { 0.05 } else { 0.1 }));
    }
    // The generations, round the edge.
    let mut edge = PathBuilder::stroke(gpui::px(3.0 * unit));
    circle(&mut edge, half * 0.985, 0.0, 1.0);
    if let Ok(p) = edge.build() {
        window.paint_path(p, rgb(0x1f2326));
    }
    if progress > 0.0 {
        let mut done = PathBuilder::stroke(gpui::px(3.0 * unit));
        circle(&mut done, half * 0.985, 0.0, progress.min(1.0));
        if let Ok(p) = done.build() {
            window.paint_path(p, hsla(0.39, 0.6, 0.55, 1.0));
        }
    }
    // The recording's spectrum as rays round the eye, mirrored.
    let eye = EYE * half;
    if !fingerprint.is_empty() {
        let n = fingerprint.len();
        let mut rays = PathBuilder::stroke(gpui::px(2.6 * unit));
        for (i, v) in fingerprint.iter().enumerate() {
            for side in [1.0f32, -1.0] {
                let a = -std::f32::consts::FRAC_PI_2
                    + side * (i as f32 + 0.5) / n as f32 * std::f32::consts::PI;
                let (r0, r1) = (eye * 1.3, eye * 1.3 + 4.0 * unit + v * half * 0.14);
                rays.move_to(pt((cx0 + r0 * a.cos(), cy0 + r0 * a.sin())));
                rays.line_to(pt((cx0 + r1 * a.cos(), cy0 + r1 * a.sin())));
            }
        }
        if let Ok(p) = rays.build() {
            window.paint_path(p, hsla(0.39, 0.6, 0.62, 0.75));
        }
    }
    // The swarms: each strand's finds on its spiral, faint trails joining them, nearer and
    // bigger as they come closer.
    let mut hits = Vec::new();
    let mut born = born.borrow_mut();
    for (s, (_, finds)) in strands.iter().enumerate() {
        let hue = STRAND_HUES[s % 4];
        let places: Vec<(f32, f32)> = finds
            .iter()
            .map(|f| at(swarm_at(s, f.generation, f.close, t)))
            .collect();
        if places.len() > 1 {
            // Smooth between finds: through the midpoints, each find the bend.
            let mut trail = PathBuilder::stroke(gpui::px(1.0 * unit));
            trail.move_to(pt(places[0]));
            for w in places.windows(2).skip(1) {
                let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
                trail.curve_to(pt(mid), pt(w[0]));
            }
            trail.line_to(pt(places[places.len() - 1]));
            if let Ok(p) = trail.build() {
                window.paint_path(p, hsla(hue, 0.5, 0.55, 0.22));
            }
        }
        let n = finds.len();
        for (k, f) in finds.iter().enumerate() {
            let c = places[k];
            hits.push((
                (s, k),
                (
                    (c.0 - f32::from(b.origin.x)) / (2.0 * half),
                    (c.1 - f32::from(b.origin.y)) / (2.0 * half),
                ),
            ));
            // Opening as it arrives: up past its size and back, over half a second.
            // Pictures in tests are of a moment: everything in them has arrived.
            let since = match cfg!(test) {
                true => 1.0,
                false => t - *born.entry((s, f.generation)).or_insert(t),
            };
            let x = (since / 0.5).clamp(0.0, 1.0);
            let open = 1.0 + 1.7 * (x - 1.0).powi(3) + 0.7 * (x - 1.0).powi(2);
            // The newest few of a strand full, older ones smaller and fainter.
            let recent = k + 3 >= n;
            let (scale, fade) = if recent { (1.0, 1.0) } else { (0.82, 0.55) };
            let r = (10.0 + 13.0 * f.close * f.close) * unit * scale * open;
            if k + 1 == n {
                window.paint_drop_shadows(
                    Bounds::centered_at(pt(c), gpui::size(gpui::px(r), gpui::px(r))),
                    gpui::Corners::all(gpui::px(r * 0.5)),
                    &[gpui::BoxShadow {
                        color: hsla(hue, 0.7, 0.55, 0.45),
                        offset: gpui::point(gpui::px(0.), gpui::px(0.)),
                        blur_radius: gpui::px(r * 1.6),
                        spread_radius: gpui::px(r * 0.2),
                        inset: false,
                    }],
                );
            }
            super::garden::paint_find(window, c, r, &f.genes, hue, fade);
            if picked == Some((s, k)) {
                let mut o = PathBuilder::stroke(gpui::px(1.6 * unit));
                for i in 0..=40 {
                    let a = i as f32 / 40.0 * std::f32::consts::TAU;
                    let q = pt((
                        c.0 + (r + 6.0 * unit) * a.cos(),
                        c.1 + (r + 6.0 * unit) * a.sin(),
                    ));
                    if i == 0 {
                        o.move_to(q);
                    } else {
                        o.line_to(q);
                    }
                }
                if let Ok(p) = o.build() {
                    window.paint_path(p, hsla(0.0, 0.0, 1.0, 0.95));
                }
            }
        }
    }
    *dots.borrow_mut() = hits;
    // The eye: the seed, breathing while it grows; a glyph to grow or stop.
    let breathe = if running {
        1.0 + 0.05 * (t * 2.4).sin()
    } else {
        1.0
    };
    let r = eye * 0.62 * breathe;
    window.paint_drop_shadows(
        Bounds::centered_at(
            pt((cx0, cy0)),
            gpui::size(gpui::px(2.0 * r), gpui::px(2.0 * r)),
        ),
        gpui::Corners::all(gpui::px(r)),
        &[gpui::BoxShadow {
            color: hsla(0.08, 0.8, 0.55, if empty { 0.15 } else { 0.45 }),
            offset: gpui::point(gpui::px(0.), gpui::px(0.)),
            blur_radius: gpui::px(r * 1.4),
            spread_radius: gpui::px(r * 0.2),
            inset: false,
        }],
    );
    window.paint_quad(
        gpui::fill(
            Bounds::centered_at(
                pt((cx0, cy0)),
                gpui::size(gpui::px(2.0 * r), gpui::px(2.0 * r)),
            ),
            gpui::linear_gradient(
                160.0,
                gpui::linear_color_stop(hsla(0.12, 0.9, if empty { 0.4 } else { 0.76 }, 1.0), 0.0),
                gpui::linear_color_stop(hsla(0.06, 0.7, if empty { 0.2 } else { 0.42 }, 1.0), 1.0),
            ),
        )
        .corner_radii(gpui::px(r)),
    );
    if !empty {
        let s = r * 0.38;
        let mut glyph = PathBuilder::fill();
        match running {
            true => {
                glyph.move_to(pt((cx0 - s, cy0 - s)));
                glyph.line_to(pt((cx0 + s, cy0 - s)));
                glyph.line_to(pt((cx0 + s, cy0 + s)));
                glyph.line_to(pt((cx0 - s, cy0 + s)));
            }
            false => {
                glyph.move_to(pt((cx0 - s * 0.6, cy0 - s)));
                glyph.line_to(pt((cx0 + s, cy0)));
                glyph.line_to(pt((cx0 - s * 0.6, cy0 + s)));
            }
        }
        glyph.close();
        if let Ok(p) = glyph.build() {
            window.paint_path(p, hsla(0.06, 0.6, 0.18, 1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closer_finds_sit_nearer_the_eye_and_outlines_are_scaled() {
        assert!(ring_of(0.9) < ring_of(0.7) && ring_of(0.7) < ring_of(0.5));
        assert!(ring_of(1.0) > EYE, "outside the eye");
        let (x, y) = swarm_at(1, 30, 0.8, 2.0);
        // A find keeps its place: where it is doesn't hang on how many came before it.
        assert_eq!(swarm_at(2, 40, 0.7, 1.0), swarm_at(2, 40, 0.7, 1.0));
        assert!((x.hypot(y) - ring_of(0.8)).abs() < 1e-4);
        assert_eq!(outline(&[0.25, -0.5, 0.125, 0.0], 2), vec![1.0, 0.25]);
    }
}
